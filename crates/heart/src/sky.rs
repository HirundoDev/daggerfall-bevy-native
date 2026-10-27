//! `sky` — the M35 layered 2D sky (spec §4, `PROBE-BIOME-001` block G, CS-11).
//!
//! **Normative source:** `docs/sintergic/game/programming/m35-vegetation-night-sky-spec-CR-f716455c.md`
//! §4 (layers L0-L4, hour bands, palettes, stars, moons), §6.1 block G and §7.2
//! (what a `PASS` needs). **Custody:** pure arithmetic over
//! [`crate::clock::GameClock`] — 0 file I/O, 0 retail bytes, 0 new crate
//! dependencies, 0 global state. Every output is a pure function of
//! `(clock, sky_base, weather, seed)`, so two processes print the same digest
//! (§0 determinism clause).
//!
//! | Layer | Content | Variation key (§4) | Cost (§4) |
//! |---|---|---|---|
//! | L0 | vertical 2-colour gradient | `(sky_base, hour_band)` | 1 fullscreen quad |
//! | L1 | stars + 2 moons with phase | `(hour, moon_phase)` | 1 static quad |
//! | L2 | cloud planes | `(weather, wind)` | ≤ 8 quads |
//! | L3 | rain/snow instances | `(weather, intensity)` | ≤ 4 096 particles |
//! | L4 | distance fog | `(weather, biome)` | 1 volume |
//!
//! **What this module does NOT claim.** The `sky_base` → colour mapping is not
//! in the pinned corpus (gap H5: `Assets/Scripts/Game/DaggerfallSky.cs` is
//! absent from `reference/dfu-spec/`, a 120-file `.cs` corpus) and the star
//! field has no identified retail texture (gap H6). Both are declared
//! `[INFERENCE]` in their items, so neither can sustain a `PASS`: §7.2 demands
//! four side-by-side captures and §4 signs the limitation instead of hiding it.
//!
//! **Relationship to the bus band.** [`HourBand`] here is the *sky* refinement
//! (four bands) of the two-value day/night band the bus carries
//! (`daggerfall_nervous::events::HourBand`); the thresholds are not duplicated
//! in either place — both read [`crate::clock`].

use crate::calendar::{self, LunarPhase};
use crate::clock::{self, GameClock};
use crate::weather::{WeatherRng, WeatherType};
use crate::weather_light::weather_light;

/// Stars drawn in the L1 quad (declared `[INFERENCE]`: the corpus pins no
/// star count, spec §4 calls the field a seeded invention).
pub const STAR_COUNT: usize = 512;

/// A generated star of the L1 quad.
///
/// `x`/`y` are normalised viewport coordinates in `[0, 1)` and `magnitude` is
/// the brightness weight in `[0.15, 1.0]`. The **positions** are not a
/// declared parity with the original (gap H6: no retail star texture was
/// identified); only the *presence* of the field and the L0 palette are
/// measured by block G. This limitation is signed, not hidden (spec §4).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Star {
    /// Normalised horizontal position, `[0, 1)`.
    pub x: f32,
    /// Normalised vertical position, `[0, 1)`.
    pub y: f32,
    /// Brightness weight, `[0.15, 1.0]`.
    pub magnitude: f32,
}

/// The four hour bands of the sky (spec §4: `night` / `dawn` / `day` / `dusk`).
///
/// The boundaries are the pinned `DaggerfallDateTime` constants read from
/// [`crate::clock`] — the spec forbids restating them ("`DawnHour` is read
/// from the clock in the implementation; 6 is not hardcoded", §4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum HourBand {
    /// Night: `[0, DawnHour)` ∪ `[DuskHour, 1440)` (DFU `IsNight`,
    /// `DaggerfallDateTime.cs:173`).
    Night,
    /// Dawn: `[DawnHour, LightsOffHour)`.
    Dawn,
    /// Day: `[LightsOffHour, LightsOnHour)`.
    Day,
    /// Dusk: `[LightsOnHour, DuskHour)`.
    Dusk,
}

impl HourBand {
    /// Stable name for probe lines (lower case, as the spec names the bands).
    pub const fn name(self) -> &'static str {
        match self {
            Self::Night => "night",
            Self::Dawn => "dawn",
            Self::Day => "day",
            Self::Dusk => "dusk",
        }
    }
}

/// The four bands in spec order — the 4×4 = 16 grid of block G iterates this.
pub const HOUR_BANDS: [HourBand; 4] = [
    HourBand::Night,
    HourBand::Dawn,
    HourBand::Day,
    HourBand::Dusk,
];

/// Band of one minute of the day.
///
/// The four windows are a **declared** refinement (`[DECLARED]`), not a corpus
/// literal: the corpus pins the *day/night* threshold (`DawnHour = 6`,
/// `DuskHour = 18`) and the two city-light thresholds
/// (`LightsOffHour = 8`, `LightsOnHour = 17`, `DaggerfallDateTime.cs:48-51`),
/// but it pins no twilight width. Rather than invent a twilight duration, the
/// two already-pinned light thresholds bracket the twilight — dawn ends when
/// the city lights go off and dusk starts when they come on — so the four
/// windows use **0 new magic numbers** and tile `[0, 1440)` exactly once:
///
/// ```text
/// [0, 360) ∪ [1080, 1440)  night    (720 min)
/// [360, 480)               dawn     (120 min)
/// [480, 1020)              day      (540 min)
/// [1020, 1080)             dusk     ( 60 min)
/// ```
///
/// `minute_of_day` outside a valid day is folded with `% 1440` so the function
/// is total and panic-free for any `u64` input.
pub fn hour_band(minute_of_day: u64) -> HourBand {
    let hour = (minute_of_day % clock::MINUTES_PER_DAY) / clock::MINUTES_PER_HOUR;
    if hour < u64::from(clock::DAWN_HOUR) || hour >= u64::from(clock::DUSK_HOUR) {
        HourBand::Night
    } else if hour < u64::from(clock::LIGHTS_OFF_HOUR) {
        HourBand::Dawn
    } else if hour < u64::from(clock::LIGHTS_ON_HOUR) {
        HourBand::Day
    } else {
        HourBand::Dusk
    }
}

/// Two-colour vertical gradient of the L0 layer (sRGB8, spec §0).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SkyPalette {
    /// Zenith colour (top of the gradient).
    pub top: [u8; 3],
    /// Horizon colour (bottom of the gradient).
    pub horizon: [u8; 3],
}

/// L0 palette of one `(sky_base, band)` pair, or `None` for a `sky_base`
/// outside [`SKY_BASES`](crate::weather_light::SKY_BASES) (`{0, 8, 16, 24}` —
/// `MapsFile.cs:449-511`).
///
/// **`[INFERENCE]` (gap H5).** The corpus pins the four `sky_base` values per
/// climate (`reference/dfu-spec/Assets/Scripts/API/MapsFile.cs:449-511`) but
/// **not** the colour of a sky base: `Assets/Scripts/Game/DaggerfallSky.cs`
/// is absent from the pinned 120-file corpus. The ramp below is therefore a
/// declared choice with two falsifiable properties, both asserted by the unit
/// tests: (i) it is **monotone** across bands for every sky base
/// (`night < dusk < dawn < day` in mean sRGB luminance — the night palette can
/// never be the day palette), and (ii) the per-family tint is monotone along
/// one declared axis, so `8` (desert) is the warmest and `24` (humid swamp) the
/// greenest. **No capture, no `PASS`:** per spec §7.2 these 16 combinations are
/// `NC` until the four side-by-side captures exist.
pub fn sky_palette(sky_base: u32, band: HourBand) -> Option<SkyPalette> {
    let tint = family_tint(sky_base)?;
    let (top, horizon) = band_gradient(band);
    Some(SkyPalette {
        top: shift(top, tint),
        horizon: shift(horizon, tint),
    })
}

/// Base gradient of one band, before the per-family tint (`[INFERENCE]`, H5).
const fn band_gradient(band: HourBand) -> ([i16; 3], [i16; 3]) {
    match band {
        HourBand::Night => ([6, 10, 26], [16, 24, 52]),
        HourBand::Dawn => ([64, 82, 138], [236, 178, 132]),
        HourBand::Day => ([58, 118, 190], [170, 205, 232]),
        HourBand::Dusk => ([56, 62, 120], [226, 138, 104]),
    }
}

/// Per-`sky_base` tint along one declared axis (`[INFERENCE]`, H5):
/// `0` mountain (reference, coldest), `8` desert (warm), `16` temperate
/// (slightly bluer), `24` swamp (humid green). `None` outside
/// [`SKY_BASES`](crate::weather_light::SKY_BASES).
const fn family_tint(sky_base: u32) -> Option<[i16; 3]> {
    match sky_base {
        0 => Some([0, 0, 0]),
        8 => Some([18, 6, -10]),
        16 => Some([4, 10, 14]),
        24 => Some([0, 8, -6]),
        _ => None,
    }
}

/// Adds a tint and saturates to `0..=255`.
const fn shift(rgb: [i16; 3], tint: [i16; 3]) -> [u8; 3] {
    [
        saturate(rgb[0] + tint[0]),
        saturate(rgb[1] + tint[1]),
        saturate(rgb[2] + tint[2]),
    ]
}

/// Saturating `i16` → `u8`.
const fn saturate(value: i16) -> u8 {
    if value < 0 {
        0
    } else if value > 255 {
        255
    } else {
        value as u8
    }
}

/// Seeded star field of the L1 quad (gap H6, spec §4).
///
/// The stream is M31's [`WeatherRng`] (splitmix64 with an explicit seed, 0
/// global state) rather than a second PRNG: `next_percent` yields 10 000
/// levels per coordinate, finer than the 1280 px of the parity capture (§0),
/// so the quantisation is invisible. Same `seed` ⇒ same field; the field does
/// not depend on the clock (a star does not move during a night).
pub fn star_field(seed: u64) -> Vec<Star> {
    let mut rng = WeatherRng::new(seed);
    (0..STAR_COUNT)
        .map(|_| Star {
            x: draw_unit(&mut rng),
            y: draw_unit(&mut rng),
            magnitude: 0.15 + 0.85 * draw_unit(&mut rng),
        })
        .collect()
}

/// One deterministic draw in `[0, 1)`.
fn draw_unit(rng: &mut WeatherRng) -> f32 {
    (rng.next_percent() / 100.0) as f32
}

/// FNV-1a 64 of [`star_field`] — the ×2 determinism digest of block G.
///
/// The transcript is the canonical little-endian `f32` bits of
/// `x, y, magnitude` for every star, so a digest match is bit identity and not
/// a printing coincidence (same construction as
/// `crates/app/src/m34_light_probe.rs`).
pub fn star_digest(seed: u64) -> u64 {
    let mut bytes = Vec::with_capacity(STAR_COUNT * 12);
    for star in star_field(seed) {
        bytes.extend_from_slice(&star.x.to_bits().to_le_bytes());
        bytes.extend_from_slice(&star.y.to_bits().to_le_bytes());
        bytes.extend_from_slice(&star.magnitude.to_bits().to_le_bytes());
    }
    fnv1a64(&bytes)
}

/// FNV-1a 64 over `bytes` (offset basis `0xcbf29ce484222325`, prime
/// `0x100000001b3` — the workspace digest, spec §0).
fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// Names of the two moons in [`calendar::lunar_phases`] order (`Masser`,
/// `Secunda` — `DaggerfallDateTime.cs:146,657-690`).
pub const MOON_NAMES: [&str; 2] = ["Masser", "Secunda"];

/// One moon disc of the L1 quad.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MoonDisc {
    /// `Masser` or `Secunda`.
    pub name: &'static str,
    /// Phase **of M31** — M35 never recomputes it (spec §4 last row).
    pub phase: LunarPhase,
    /// Lit fraction in `[0.0, 1.0]` derived from [`Self::phase`].
    pub illuminated_fraction: f32,
}

/// Lit fraction of one lunar phase, in `[0.0, 1.0]`.
///
/// `[DECLARED]`: the corpus pins the eight *phases* bit-exactly
/// (`DaggerfallDateTime.cs:657-690`, M31 `calendar::LUNAR_PHASES`) but no
/// illumination percentage — the original draws the disc from the moon
/// texture. The mapping is the symmetric eight-step eighths grid
/// (`New 0 · One* ¼ · Half* ½ · Three* ¾ · Full 1`), monotone from new to full
/// and mirrored back; `None` (the source's error value) is treated as new.
pub const fn illuminated_fraction(phase: LunarPhase) -> f32 {
    match phase {
        LunarPhase::New | LunarPhase::None => 0.0,
        LunarPhase::OneWax | LunarPhase::OneWane => 0.25,
        LunarPhase::HalfWax | LunarPhase::HalfWane => 0.5,
        LunarPhase::ThreeWax | LunarPhase::ThreeWane => 0.75,
        LunarPhase::Full => 1.0,
    }
}

/// The two moons for the clock's tick, in `(Masser, Secunda)` order.
///
/// The phase is **consumed** from [`calendar::lunar_phases`] (M31), never
/// recomputed: M35 must agree with `CALENDAR-001` for the same tick (§7.2
/// criterion 2).
pub fn moons(clock: &GameClock) -> [MoonDisc; 2] {
    let (masser, secunda) = calendar::lunar_phases(clock);
    [
        MoonDisc {
            name: MOON_NAMES[0],
            phase: masser,
            illuminated_fraction: illuminated_fraction(masser),
        },
        MoonDisc {
            name: MOON_NAMES[1],
            phase: secunda,
            illuminated_fraction: illuminated_fraction(secunda),
        },
    ]
}

/// Cloud planes of L2: at most [`CLOUD_MAX_QUADS`] quads (spec §4).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CloudCover {
    /// Quads drawn, `0..=8`.
    pub quads: u8,
    /// Cover density in `[0, 1]`.
    pub density: f32,
    /// Seeded drift phase in `[0, 1)` — the "wind" key of §4.
    pub drift: f32,
}

/// Instanced meteors of L3: at most [`PRECIPITATION_MAX_PARTICLES`] (spec §4).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PrecipitationSpread {
    /// Instances drawn, `0..=4 096`.
    pub particles: u16,
    /// Intensity in `[0, 1]` (`0` when the state has no precipitation).
    pub intensity: f32,
}

/// Distance fog of L4: exactly [`FOG_VOLUMES`] volume (spec §4).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FogVolume {
    /// Volumes drawn (always [`FOG_VOLUMES`]).
    pub volumes: u8,
    /// Density of the §5 table (M35 [`crate::weather_light`]).
    pub density: f64,
}

/// Cloud quads of L2 (spec §4 cost column).
pub const CLOUD_MAX_QUADS: u8 = 8;
/// Meteor particles of L3 (spec §4 cost column).
pub const PRECIPITATION_MAX_PARTICLES: u16 = 4096;
/// Fog volumes of L4 (spec §4 cost column).
pub const FOG_VOLUMES: u8 = 1;

/// The five layers of one frame (spec §4).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SkyLayers {
    /// L0 — background gradient.
    pub l0: SkyPalette,
    /// L1 — the two moons (the star field is [`star_field`], drawn in the same
    /// static quad).
    pub l1: [MoonDisc; 2],
    /// L2 — cloud planes.
    pub l2: CloudCover,
    /// L3 — precipitation instances.
    pub l3: PrecipitationSpread,
    /// L4 — distance fog.
    pub l4: FogVolume,
}

/// Composes the five layers for one frame, or `None` when `sky_base` is not
/// one of [`SKY_BASES`](crate::weather_light::SKY_BASES).
///
/// **Declared derivations** (no invented per-weather constants):
/// - L2 density is the sunlight attenuation of the §5 row,
///   `1 − SunlightScale` (`[DECLARED]`: §4 says "density by `WeatherType`" but
///   pins no number, so the normative §5 column is the only datum used);
///   quads = `round(density · 8)`.
/// - L3 is present only for a state with precipitation (`Rain`, `Thunder`,
///   `Snow` — M31 [`WeatherType::is_raining`]/[`WeatherType::is_snowing`]),
///   particles = `round(density · 4 096)`; dry states carry `0`.
/// - L4 density is the §5 fog density verbatim, always one volume.
/// - The L2 `drift` phase comes from `seed` (the "wind" key): the engine has
///   no wind simulator, so the plane offset is the seeded phase.
pub fn compose(
    clock: &GameClock,
    sky_base: u32,
    weather: WeatherType,
    seed: u64,
) -> Option<SkyLayers> {
    let band = hour_band(clock.minute_of_day());
    let l0 = sky_palette(sky_base, band)?;
    let light = weather_light(weather);
    let density = (1.0 - light.sunlight_scale) as f32;
    let intensity = if weather.is_raining() || weather.is_snowing() {
        density
    } else {
        0.0
    };
    let mut rng = WeatherRng::new(seed);
    Some(SkyLayers {
        l0,
        l1: moons(clock),
        l2: CloudCover {
            quads: (density * f32::from(CLOUD_MAX_QUADS)).round() as u8,
            density,
            drift: draw_unit(&mut rng),
        },
        l3: PrecipitationSpread {
            particles: (intensity * f32::from(PRECIPITATION_MAX_PARTICLES)).round() as u16,
            intensity,
        },
        l4: FogVolume {
            volumes: FOG_VOLUMES,
            density: light.fog_density,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::weather_light::SKY_BASES;

    /// Mean sRGB luminance of a palette, `0.2126 R + 0.7152 G + 0.0722 B`.
    fn mean_luminance(palette: SkyPalette) -> f64 {
        let channel = |rgb: [u8; 3]| {
            0.2126 * f64::from(rgb[0]) + 0.7152 * f64::from(rgb[1]) + 0.0722 * f64::from(rgb[2])
        };
        (channel(palette.top) + channel(palette.horizon)) / 2.0
    }

    #[test]
    fn the_four_bands_tile_the_whole_day() {
        let mut night = 0_u64;
        let mut dawn = 0_u64;
        let mut day = 0_u64;
        let mut dusk = 0_u64;
        for minute in 0..clock::MINUTES_PER_DAY {
            match hour_band(minute) {
                HourBand::Night => night += 1,
                HourBand::Dawn => dawn += 1,
                HourBand::Day => day += 1,
                HourBand::Dusk => dusk += 1,
            }
        }
        // No gap, no overlap: the four windows sum the whole day.
        assert_eq!(night + dawn + day + dusk, clock::MINUTES_PER_DAY);
        // Widths derived from the pinned hours, not from literals.
        assert_eq!(
            night,
            (u64::from(clock::DAWN_HOUR) + clock::MINUTES_PER_DAY / clock::MINUTES_PER_HOUR
                - u64::from(clock::DUSK_HOUR))
                * clock::MINUTES_PER_HOUR
        );
        assert_eq!(
            dawn,
            (u64::from(clock::LIGHTS_OFF_HOUR) - u64::from(clock::DAWN_HOUR))
                * clock::MINUTES_PER_HOUR
        );
        assert_eq!(
            day,
            (u64::from(clock::LIGHTS_ON_HOUR) - u64::from(clock::LIGHTS_OFF_HOUR))
                * clock::MINUTES_PER_HOUR
        );
        assert_eq!(
            dusk,
            (u64::from(clock::DUSK_HOUR) - u64::from(clock::LIGHTS_ON_HOUR))
                * clock::MINUTES_PER_HOUR
        );
        // Boundaries and totality.
        assert_eq!(hour_band(0), HourBand::Night);
        assert_eq!(hour_band(u64::from(clock::DAWN_HOUR) * 60), HourBand::Dawn);
        assert_eq!(
            hour_band(u64::from(clock::LIGHTS_OFF_HOUR) * 60),
            HourBand::Day
        );
        assert_eq!(
            hour_band(u64::from(clock::LIGHTS_ON_HOUR) * 60),
            HourBand::Dusk
        );
        assert_eq!(hour_band(u64::from(clock::DUSK_HOUR) * 60), HourBand::Night);
        assert_eq!(hour_band(clock::MINUTES_PER_DAY), HourBand::Night);
    }

    #[test]
    fn sixteen_palettes_and_none_outside_the_sky_bases() {
        let mut seen = 0;
        for sky_base in SKY_BASES {
            for band in HOUR_BANDS {
                assert!(
                    sky_palette(sky_base, band).is_some(),
                    "sky_base {sky_base} band {}",
                    band.name()
                );
                seen += 1;
            }
        }
        assert_eq!(seen, 16);
        for rejected in [1, 4, 7, 12, 20, 28, 32, u32::MAX] {
            assert_eq!(sky_palette(rejected, HourBand::Day), None);
        }
    }

    #[test]
    fn star_field_is_deterministic_and_seed_dependent() {
        assert_eq!(star_digest(7), star_digest(7));
        assert_ne!(star_digest(7), star_digest(8));
        let field = star_field(7);
        assert_eq!(field.len(), STAR_COUNT);
        for star in &field {
            assert!((0.0..1.0).contains(&star.x), "x out of range: {}", star.x);
            assert!((0.0..1.0).contains(&star.y), "y out of range: {}", star.y);
            assert!(
                (0.15..=1.0).contains(&star.magnitude),
                "magnitude out of range: {}",
                star.magnitude
            );
            assert!(star.magnitude.is_finite());
        }
        // A second call is the same bytes, not merely the same length.
        assert_eq!(star_field(7), field);
    }

    #[test]
    fn moons_take_their_phase_from_m31() {
        let mut clock = GameClock::classic_default();
        for advance in [0_u64, 3 * clock::MINUTES_PER_DAY * 60, 71 * 24 * 3600] {
            clock.raise_seconds(advance);
            let discs = moons(&clock);
            let (masser, secunda) = calendar::lunar_phases(&clock);
            assert_eq!(discs[0].name, "Masser");
            assert_eq!(discs[1].name, "Secunda");
            assert_eq!(discs[0].phase, masser);
            assert_eq!(discs[1].phase, secunda);
            for disc in &discs {
                assert_eq!(disc.illuminated_fraction, illuminated_fraction(disc.phase));
                assert!((0.0..=1.0).contains(&disc.illuminated_fraction));
            }
        }
    }

    #[test]
    fn the_night_palette_is_strictly_darker_than_the_day_palette() {
        for sky_base in SKY_BASES {
            let night = mean_luminance(sky_palette(sky_base, HourBand::Night).expect("night"));
            let dusk = mean_luminance(sky_palette(sky_base, HourBand::Dusk).expect("dusk"));
            let dawn = mean_luminance(sky_palette(sky_base, HourBand::Dawn).expect("dawn"));
            let day = mean_luminance(sky_palette(sky_base, HourBand::Day).expect("day"));
            assert!(night < day, "sky_base {sky_base}: night is not darker");
            // The declared monotone ramp, band by band.
            assert!(night < dusk, "sky_base {sky_base}: night >= dusk");
            assert!(dusk < dawn, "sky_base {sky_base}: dusk >= dawn");
            assert!(dawn < day, "sky_base {sky_base}: dawn >= day");
            // CS-11 criterion 1: the night palette is NOT the fixed clear colour
            // `0.40, 0.62, 0.94` of the current engine (spec §7.2).
            let clear_color = mean_luminance(SkyPalette {
                top: [102, 158, 240],
                horizon: [102, 158, 240],
            });
            assert!(night < clear_color, "sky_base {sky_base}: night == clear color");
        }
    }

    #[test]
    fn illumination_is_monotone_from_new_to_full() {
        assert_eq!(illuminated_fraction(LunarPhase::New), 0.0);
        assert_eq!(illuminated_fraction(LunarPhase::Full), 1.0);
        let waxing: Vec<f32> = [
            LunarPhase::New,
            LunarPhase::OneWax,
            LunarPhase::HalfWax,
            LunarPhase::ThreeWax,
            LunarPhase::Full,
        ]
        .iter()
        .map(|phase| illuminated_fraction(*phase))
        .collect();
        assert!(waxing.windows(2).all(|pair| pair[0] < pair[1]));
        // Waning mirrors waxing.
        assert_eq!(
            illuminated_fraction(LunarPhase::ThreeWane),
            illuminated_fraction(LunarPhase::ThreeWax)
        );
        assert_eq!(
            illuminated_fraction(LunarPhase::OneWane),
            illuminated_fraction(LunarPhase::OneWax)
        );
    }

    #[test]
    fn compose_respects_the_layer_cost_envelope() {
        let clock = GameClock::classic_default();
        assert_eq!(compose(&clock, 7, WeatherType::Sunny, 1), None);
        for weather in crate::weather::WEATHER_TYPES {
            let layers = compose(&clock, 16, weather, 42).expect("sky_base 16 is valid");
            assert!(layers.l2.quads <= CLOUD_MAX_QUADS);
            assert!(layers.l3.particles <= PRECIPITATION_MAX_PARTICLES);
            assert_eq!(layers.l4.volumes, FOG_VOLUMES);
            assert_eq!(layers.l4.density, crate::weather::fog_density(weather));
            assert!((0.0..1.0).contains(&layers.l2.drift));
            if weather.is_raining() || weather.is_snowing() {
                assert!(layers.l3.particles > 0, "{} has no instances", weather.name());
            } else {
                assert_eq!(layers.l3.particles, 0, "{} invents instances", weather.name());
            }
        }
        // Determinism of the seed-derived drift phase.
        let first = compose(&clock, 0, WeatherType::Rain, 9).expect("valid");
        let second = compose(&clock, 0, WeatherType::Rain, 9).expect("valid");
        assert_eq!(first, second);
    }
}
