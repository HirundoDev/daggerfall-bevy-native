//! `weather_light` — the M35 §5 weather → light/fog table
//! (`PROBE-BIOME-001` block E: 7 states × 10 climates = 70/70).
//!
//! **Normative source:** `docs/sintergic/game/programming/m35-vegetation-night-sky-spec-CR-f716455c.md`
//! §3.1 (climate → `sky_base`) and §5 (`SunlightScale` / `ShadowStrength` /
//! fog density per state), measured by §6.1 block E.
//!
//! **Consumption, not reimplementation.** M31 owns the weather table and the
//! outdoor state machine ([`crate::weather`]); this module only reads the
//! resulting [`WeatherType`] and states the light the renderer applies. §5 has
//! **no climate column** by design, so [`light_for`] takes the climate and
//! returns exactly [`weather_light`]: the 70-row grid the probe walks declares
//! ten identical columns per state instead of inventing a per-climate
//! variation (spec §5; §9 R6 forbids coupling to anything but the pinned
//! tables).
//!
//! ## Declared provenance gap (H3) — `Cloudy` and `Fog`
//!
//! §5 attributes its three rows to `m31-travel-weather-calendar-spec-CR-5243af63.md:148-162`
//! (§2.4, light table at `:150-155`). That range — and its source,
//! `reference/dfu-spec/Assets/Scripts/Game/WeatherManager.cs` — declares only
//! **five** scalars:
//!
//! | Corpus line | What it declares |
//! |---|---|
//! | `WeatherManager.cs:38-46` | `Overcast 0.65`, `Rain 0.45`, `Storm 0.25`, `Snow 0.45`, `Winter 0.65` sunlight |
//! | `WeatherManager.cs:49-57` | `0.6 / 0.4 / 0.4 / 0.25 / 0.8` shadow strengths |
//! | `WeatherManager.cs:70-74` | fog densities: Sunny/Overcast `0.0`, Rain `0.003`, Snow `0.005`, Heavy `0.05` |
//!
//! The two rows §5 adds are not declared anywhere in the corpus:
//!
//! - **`Cloudy 0.8 / 0.8`** — `SetSunlightScale()` (`WeatherManager.cs:309-341`)
//!   starts from `SunlightManager.defaultScaleFactor` / `defaultShadowStrength`
//!   (`:311-312`) and branches on Winter, Rain, Storm, Snow and Overcast only:
//!   there is **no `Cloudy` branch**, so the original leaves `Cloudy` at the
//!   default (`1.0 / 1.0`). `SetWeather()` even declares the hole as work not
//!   done: `case WeatherType.Cloudy:` only sets `SunnyFogSettings` under a
//!   literal `// TODO make skybox cloudy` (`:448-451`). §5 states `0.8 / 0.8`
//!   where the original states nothing.
//! - **`Fog 0.65 / 0.6`** — there is no `Fog` scalar either; the state is
//!   composed at `:456-459` (`SetRainOvercast()` + `SetFog(HeavyFogSettings)`),
//!   i.e. it inherits the Overcast pair and takes `0.05` from
//!   `HeavyFogSettings` (`:74`). §5's row is the composed result, not a
//!   declared row.
//!
//! The **normative** table for M35 is §5 (§2/§5 precedence), so it is
//! implemented verbatim and the provenance deviation is printed rather than
//! hidden — [`declares_spec_only_rows`] names both rows for the probe. The
//! deviation is real, not cosmetic: M31's consumers in [`crate::weather`] give
//! `sunlight_scale(Cloudy, false) == 1.0` and `shadow_strength(Cloudy, false)
//! == 1.0` (the upstream default), while §5 states `0.8 / 0.8`. `Fog` *does*
//! agree with M31 (`0.65 / 0.6`, reached through `is_overcast()`), because M31
//! reproduces the composition of `:456-459` instead of a row. Both facts are
//! asserted by the unit tests.

use crate::weather::WeatherType;

/// The four `sky_base` values of the corpus (`{0, 8, 16, 24}`), declared here
/// once because ❤️ heart has **no** `daggerfall_parser_rs` edge and
/// [`MapClimate`] lives below: `reference/dfu-spec/Assets/Scripts/API/MapsFile.cs:449-511`
/// (Mountain `0`, Desert/Desert2 `8`, temperate climates `16`, humid/ocean
/// `24`), spec §3.1 and §2 row 6.
pub const SKY_BASES: [u32; 4] = [0, 8, 16, 24];

/// The ten `MapsFile.Climates` values (`reference/dfu-spec/Assets/Scripts/API/MapsFile.cs:289-301`),
/// mirroring `daggerfall_parser_rs::climate::WorldClimate` by index.
///
/// **Why a local mirror.** The spec's §5/§3.1 tables are keyed by the ten
/// `CLIMATE.PAK` values, but ❤️ heart deliberately has no parser edge
/// (`crate::clock` documents the crate's dependency surface as memory +
/// nervous), so adding `daggerfall_parser_rs` here would drag the whole parser
/// into the time organ for one table. The ten indices are restated with their
/// raw `u8` value, and a caller feeds them with
/// `WorldClimate::index() → MapClimate::from_index(..)`; the duplication is
/// declared, not silent (see the lane report).
///
/// This is **not** [`crate::weather::Climate`]: that enum is the six
/// `WeatherTable.json` groups of M31, while these ten are the map-pixel
/// climates of M13.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum MapClimate {
    /// `223` — ocean.
    Ocean = 223,
    /// `224` — desert.
    Desert = 224,
    /// `225` — Dak'fron desert.
    Desert2 = 225,
    /// `226` — mountain.
    Mountain = 226,
    /// `227` — rainforest.
    Rainforest = 227,
    /// `228` — swamp.
    Swamp = 228,
    /// `229` — subtropical.
    Subtropical = 229,
    /// `230` — mountain woods.
    MountainWoods = 230,
    /// `231` — woodlands (the default climate).
    Woodlands = 231,
    /// `232` — haunted woodlands.
    HauntedWoodlands = 232,
}

impl MapClimate {
    /// Converts the raw `CLIMATE.PAK` value, `None` outside `223..=232`.
    pub const fn from_index(value: u8) -> Option<Self> {
        Some(match value {
            223 => Self::Ocean,
            224 => Self::Desert,
            225 => Self::Desert2,
            226 => Self::Mountain,
            227 => Self::Rainforest,
            228 => Self::Swamp,
            229 => Self::Subtropical,
            230 => Self::MountainWoods,
            231 => Self::Woodlands,
            232 => Self::HauntedWoodlands,
            _ => return None,
        })
    }

    /// Raw value in `CLIMATE.PAK` (`223..=232`).
    pub const fn index(self) -> u8 {
        self as u8
    }

    /// Stable name for probe lines (`MapsFile.Climates` spelling).
    pub const fn name(self) -> &'static str {
        match self {
            Self::Ocean => "Ocean",
            Self::Desert => "Desert",
            Self::Desert2 => "Desert2",
            Self::Mountain => "Mountain",
            Self::Rainforest => "Rainforest",
            Self::Swamp => "Swamp",
            Self::Subtropical => "Subtropical",
            Self::MountainWoods => "MountainWoods",
            Self::Woodlands => "Woodlands",
            Self::HauntedWoodlands => "HauntedWoodlands",
        }
    }

    /// `sky_base` of this climate (spec §3.1, `MapsFile.cs:449-511`).
    pub const fn sky_base(self) -> u32 {
        match self {
            Self::Desert | Self::Desert2 => 8,
            Self::Mountain => 0,
            Self::MountainWoods | Self::Woodlands | Self::HauntedWoodlands => 16,
            Self::Ocean | Self::Rainforest | Self::Swamp | Self::Subtropical => 24,
        }
    }
}

/// The ten map climates in `CLIMATE.PAK` order — the 7 × 10 = 70 grid of
/// block E iterates this.
pub const MAP_CLIMATES: [MapClimate; 10] = [
    MapClimate::Ocean,
    MapClimate::Desert,
    MapClimate::Desert2,
    MapClimate::Mountain,
    MapClimate::Rainforest,
    MapClimate::Swamp,
    MapClimate::Subtropical,
    MapClimate::MountainWoods,
    MapClimate::Woodlands,
    MapClimate::HauntedWoodlands,
];

/// Light and fog of one outdoor weather state (spec §5 row).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WeatherLight {
    /// `SunlightScale` in `[0, 1]`.
    pub sunlight_scale: f64,
    /// `ShadowStrength` in `[0, 1]`.
    pub shadow_strength: f64,
    /// Outdoor fog density (`0` = linear fog without density).
    pub fog_density: f64,
}

/// The §5 table, row by row (`m31-…spec-CR-5243af63.md:148-162`, spec §5).
///
/// `Snow` shares the `Rain` sunlight scale (`0.45`) but not the shadow
/// strength; `Fog` shares the `Overcast` pair and adds the density of
/// `HeavyFogSettings` (`0.05`, `WeatherManager.cs:74`). `Cloudy` is the row
/// that exists **only** in §5 (gap H3 above).
pub fn weather_light(weather: WeatherType) -> WeatherLight {
    match weather {
        WeatherType::Sunny => WeatherLight {
            sunlight_scale: 1.0,
            shadow_strength: 1.0,
            fog_density: 0.0,
        },
        WeatherType::Cloudy => WeatherLight {
            sunlight_scale: 0.8,
            shadow_strength: 0.8,
            fog_density: 0.0,
        },
        WeatherType::Overcast => WeatherLight {
            sunlight_scale: 0.65,
            shadow_strength: 0.6,
            fog_density: 0.0,
        },
        WeatherType::Fog => WeatherLight {
            sunlight_scale: 0.65,
            shadow_strength: 0.6,
            fog_density: 0.05,
        },
        WeatherType::Rain => WeatherLight {
            sunlight_scale: 0.45,
            shadow_strength: 0.4,
            fog_density: 0.003,
        },
        WeatherType::Thunder => WeatherLight {
            sunlight_scale: 0.25,
            shadow_strength: 0.4,
            fog_density: 0.003,
        },
        WeatherType::Snow => WeatherLight {
            sunlight_scale: 0.45,
            shadow_strength: 0.25,
            fog_density: 0.005,
        },
    }
}

/// The §5 row of `weather`, for the whole 7 × 10 grid.
///
/// The `climate` argument is accepted **and deliberately unused**: §5 has no
/// climate column, so the ten columns of block E are identical per state by
/// construction and the probe measures that identity instead of a fabricated
/// variation. The unused binding is the measurement, not an oversight.
pub fn light_for(weather: WeatherType, _climate: MapClimate) -> WeatherLight {
    weather_light(weather)
}

/// `sky_base` of a climate (spec §3.1) — in `SKY_BASES` for all ten.
pub fn sky_base_for(climate: MapClimate) -> u32 {
    climate.sky_base()
}

/// The two §5 rows that the pinned corpus does **not** declare (gap H3):
/// `Cloudy` (no branch in `SetSunlightScale()`,
/// `WeatherManager.cs:309-341`, and a literal `// TODO make skybox cloudy` at
/// `:448-451`) and `Fog` (composed at `:456-459`, never named as a row).
///
/// The probe prints `[DECLARED-DEVIATION H3]` for exactly these two rows.
pub fn declares_spec_only_rows() -> [WeatherType; 2] {
    [WeatherType::Cloudy, WeatherType::Fog]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::weather::{self, WEATHER_TYPES};

    /// The §5 literals, written here from the spec table — never imported from
    /// the implementation (a constant compared to itself is not a measurement).
    const SPEC_5: [(WeatherType, f64, f64, f64); 7] = [
        (WeatherType::Sunny, 1.0, 1.0, 0.0),
        (WeatherType::Cloudy, 0.8, 0.8, 0.0),
        (WeatherType::Overcast, 0.65, 0.6, 0.0),
        (WeatherType::Fog, 0.65, 0.6, 0.05),
        (WeatherType::Rain, 0.45, 0.4, 0.003),
        (WeatherType::Thunder, 0.25, 0.4, 0.003),
        (WeatherType::Snow, 0.45, 0.25, 0.005),
    ];

    #[test]
    fn the_seven_rows_match_the_spec_table_value_by_value() {
        assert_eq!(SPEC_5.len(), WEATHER_TYPES.len());
        for (weather, sunlight, shadow, fog) in SPEC_5 {
            let light = weather_light(weather);
            assert_eq!(light.sunlight_scale, sunlight, "{} sunlight", weather.name());
            assert_eq!(light.shadow_strength, shadow, "{} shadow", weather.name());
            assert_eq!(light.fog_density, fog, "{} fog", weather.name());
        }
    }

    #[test]
    fn the_spec_only_rows_are_cloudy_and_fog() {
        assert_eq!(
            declares_spec_only_rows(),
            [WeatherType::Cloudy, WeatherType::Fog]
        );
    }

    #[test]
    fn the_declared_monotony_holds() {
        let scale = |weather: WeatherType| weather_light(weather).sunlight_scale;
        assert!(scale(WeatherType::Sunny) > scale(WeatherType::Cloudy));
        assert!(scale(WeatherType::Cloudy) > scale(WeatherType::Overcast));
        assert!(scale(WeatherType::Overcast) >= scale(WeatherType::Fog));
        assert!(scale(WeatherType::Fog) > scale(WeatherType::Rain));
        assert!(scale(WeatherType::Rain) > scale(WeatherType::Thunder));
        assert_eq!(scale(WeatherType::Snow), scale(WeatherType::Rain));
        assert_eq!(scale(WeatherType::Snow), 0.45);
        // Shadow strength follows the same order (Snow is the one inversion:
        // snow shadows are the weakest of the table).
        let shadow = |weather: WeatherType| weather_light(weather).shadow_strength;
        assert!(shadow(WeatherType::Sunny) > shadow(WeatherType::Cloudy));
        assert!(shadow(WeatherType::Cloudy) > shadow(WeatherType::Overcast));
        assert!(shadow(WeatherType::Rain) > shadow(WeatherType::Snow));
    }

    #[test]
    fn fog_density_agrees_with_m31() {
        for weather in WEATHER_TYPES {
            assert_eq!(
                weather_light(weather).fog_density,
                weather::fog_density(weather),
                "{} fog density disagrees with M31",
                weather.name()
            );
        }
    }

    #[test]
    fn m31_parity_is_exact_except_the_declared_cloudy_row() {
        for weather in WEATHER_TYPES {
            let light = weather_light(weather);
            let m31_sunlight = weather::sunlight_scale(weather, false);
            let m31_shadow = weather::shadow_strength(weather, false);
            if weather == WeatherType::Cloudy {
                // Gap H3: no Cloudy field exists in `WeatherManager.cs`, so M31
                // falls back to the documented defaults while §5 states 0.8.
                assert_eq!(m31_sunlight, 1.0);
                assert_eq!(m31_shadow, 1.0);
                assert_ne!(light.sunlight_scale, m31_sunlight);
            } else {
                assert_eq!(
                    light.sunlight_scale,
                    m31_sunlight,
                    "{} sunlight deviates from M31",
                    weather.name()
                );
                assert_eq!(
                    light.shadow_strength, m31_shadow,
                    "{} shadow deviates from M31",
                    weather.name()
                );
            }
        }
    }

    #[test]
    fn the_seventy_row_grid_is_constant_across_climates() {
        let mut rows = 0;
        for weather in WEATHER_TYPES {
            let reference = weather_light(weather);
            for climate in MAP_CLIMATES {
                assert_eq!(light_for(weather, climate), reference);
                assert!(SKY_BASES.contains(&sky_base_for(climate)));
                rows += 1;
            }
        }
        assert_eq!(rows, 70);
    }

    #[test]
    fn sky_base_table_matches_spec_3_1() {
        let expected: [(MapClimate, u32); 10] = [
            (MapClimate::Ocean, 24),
            (MapClimate::Desert, 8),
            (MapClimate::Desert2, 8),
            (MapClimate::Mountain, 0),
            (MapClimate::Rainforest, 24),
            (MapClimate::Swamp, 24),
            (MapClimate::Subtropical, 24),
            (MapClimate::MountainWoods, 16),
            (MapClimate::Woodlands, 16),
            (MapClimate::HauntedWoodlands, 16),
        ];
        for (climate, sky_base) in expected {
            assert_eq!(sky_base_for(climate), sky_base, "{}", climate.name());
        }
        // The index round-trip the parser bridge relies on: `WorldClimate as u8`
        // (223) is the first discriminant, the rest follow in `CLIMATE.PAK` order.
        for (offset, climate) in MAP_CLIMATES.into_iter().enumerate() {
            let index = 223 + u8::try_from(offset).expect("ten climates fit in u8");
            assert_eq!(climate.index(), index);
            assert_eq!(MapClimate::from_index(index), Some(climate));
        }
        assert_eq!(MapClimate::Ocean.index(), 223);
        assert_eq!(MapClimate::from_index(222), None);
        assert_eq!(MapClimate::from_index(233), None);
    }
}
