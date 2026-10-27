//! `weather` — per climate/season weather odds, the `GetWeather` extraction
//! and the outdoor weather state machine (M31, SPEC-M31 §2 · REQ-L99-064).
//!
//! **Data custody (MIT, 0 retail bytes):** [`WEATHER_TABLE`] is a transcription
//! of `WeatherTable.json` (DFU v1.1.1, upstream `0038a1e6…`, read through
//! `git show HEAD:Assets/Resources/WeatherTable.json`). Rows whose JSON object
//! omits a weather type are 0 % (`JsonUtility` leaves the float field at 0),
//! and `Woodlands/Winter` sums 90 %, which is the single row the source
//! normalizes (`Weather.cs:70-84`).
//!
//! **Extraction (clean-room):** `WeatherClimateSeason.GetWeather`
//! (`Weather.cs:113-127`) draws `rand ∈ [0,100)`, subtracts each chance in
//! [`COMPILE_ORDER`] and returns on `rand <= 0`; the fallback is Sunny. The
//! order is the insertion order of `CompileOdds` (`Weather.cs:94-108`), **not**
//! the [`WeatherType`] enum order — SPEC-M31 §4.5 W1-W9 depend on it.
//!
//! **Determinism (SPEC-M31 §8 seed policy):** the source uses
//! `UnityEngine.Random`; the engine uses [`WeatherRng`], an explicitly seeded
//! splitmix64 stream with the same uniform distribution over `[0,100)` at
//! 0.01 % resolution. 0 global state, 0 clock reads.

use crate::calendar::Season;

/// Weather states (`WeatherType`, `Weather.cs:15-33`): descending
/// pleasant-ness, hence the transitions are a simple arithmetic comparison.
/// Save aliases (`None`/`Rain_Normal`/`Snow_Normal`) are not duplicated.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(u8)]
pub enum WeatherType {
    /// `Sunny` = 0.
    #[default]
    Sunny = 0,
    /// `Cloudy` = 1.
    Cloudy = 1,
    /// `Overcast` = 2.
    Overcast = 2,
    /// `Fog` = 3.
    Fog = 3,
    /// `Rain` = 4.
    Rain = 4,
    /// `Thunder` = 5.
    Thunder = 5,
    /// `Snow` = 6.
    Snow = 6,
}

/// All weather states in enum order.
pub const WEATHER_TYPES: [WeatherType; 7] = [
    WeatherType::Sunny,
    WeatherType::Cloudy,
    WeatherType::Overcast,
    WeatherType::Fog,
    WeatherType::Rain,
    WeatherType::Thunder,
    WeatherType::Snow,
];

/// `CompileOdds` insertion order (`Weather.cs:94-108`): Snow precedes Thunder.
pub const COMPILE_ORDER: [WeatherType; 7] = [
    WeatherType::Sunny,
    WeatherType::Cloudy,
    WeatherType::Overcast,
    WeatherType::Fog,
    WeatherType::Rain,
    WeatherType::Snow,
    WeatherType::Thunder,
];

impl WeatherType {
    /// Index used by [`WeatherOdds`] rows (enum order, 0..6).
    pub const fn index(self) -> usize {
        self as usize
    }

    /// Stable name for probes.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Sunny => "Sunny",
            Self::Cloudy => "Cloudy",
            Self::Overcast => "Overcast",
            Self::Fog => "Fog",
            Self::Rain => "Rain",
            Self::Thunder => "Thunder",
            Self::Snow => "Snow",
        }
    }

    /// `IsRaining` after `SetWeather` (`WeatherManager.cs:419-470`).
    pub const fn is_raining(self) -> bool {
        matches!(self, Self::Rain | Self::Thunder)
    }

    /// `IsStorming` (`Thunder` implies raining and storming).
    pub const fn is_storming(self) -> bool {
        matches!(self, Self::Thunder)
    }

    /// `IsSnowing`.
    pub const fn is_snowing(self) -> bool {
        matches!(self, Self::Snow)
    }

    /// `IsOvercast` after `SetWeather`: `Fog` implies overcast, `Snow` goes
    /// through `SetSnowOvercast`, rain/storm through `SetRainOvercast`.
    pub const fn is_overcast(self) -> bool {
        matches!(
            self,
            Self::Overcast | Self::Fog | Self::Rain | Self::Thunder | Self::Snow
        )
    }
}

/// Weather-odds row in [`WeatherType`] order:
/// `[Sunny, Cloudy, Overcast, Fog, Rain, Thunder, Snow]` (percent).
pub type WeatherOdds = [f64; 7];

/// Weather climates of `WeatherTable.json` / `MapsFile.Climates` groups.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Climate {
    /// `Desert` (raw 224) and `Desert2` (225).
    Desert = 0,
    /// `Mountain` (226) and `MountainWoods` (230), `Weather.cs:205-227`.
    Mountains = 1,
    /// `Rainforest` (227).
    Jungle = 2,
    /// `Ocean` (223) and `Swamp` (228).
    Swamp = 3,
    /// `Subtropical` (229).
    Subtropical = 4,
    /// `Woodlands` (231) and `HauntedWoodlands` (232).
    Woodlands = 5,
}

/// Climates in table order (`WeatherManager.SetClimateWeathers`, `:424-432`).
pub const CLIMATES: [Climate; 6] = [
    Climate::Desert,
    Climate::Mountains,
    Climate::Jungle,
    Climate::Swamp,
    Climate::Subtropical,
    Climate::Woodlands,
];

impl Climate {
    /// Table row from the raw `CLIMATE.PAK` value (`MapsFile.Climates`).
    pub const fn from_climate_value(value: u8) -> Option<Self> {
        match value {
            224 | 225 => Some(Self::Desert),
            226 | 230 => Some(Self::Mountains),
            227 => Some(Self::Jungle),
            223 | 228 => Some(Self::Swamp),
            229 => Some(Self::Subtropical),
            231 | 232 => Some(Self::Woodlands),
            _ => None,
        }
    }

    /// Stable name for probes (the `WeatherTable.json` object name).
    pub const fn name(self) -> &'static str {
        match self {
            Self::Desert => "Desert",
            Self::Mountains => "Mountains",
            Self::Jungle => "Jungle",
            Self::Swamp => "Swamp",
            Self::Subtropical => "Subtropical",
            Self::Woodlands => "Woodlands",
        }
    }

    /// Row index in [`WEATHER_TABLE`].
    pub const fn index(self) -> usize {
        self as usize
    }
}

/// `WeatherTable.json` transcribed to `[climate][season][weather]`, in
/// percent, with `season` indexed by [`Season`] discriminant
/// (Fall = 0, Spring = 1, Summer = 2, Winter = 3) and `weather` in
/// [`WeatherType`] order. Missing JSON fields are 0.
///
/// Sums are 100 for every cell except `Woodlands/Winter` (90), which
/// [`WeatherClimateSeason::odds`] normalizes (`Weather.cs:70-84`).
pub const WEATHER_TABLE: [[WeatherOdds; 4]; 6] = [
    // Desert
    [
        [80.0, 15.0, 0.0, 0.0, 3.0, 2.0, 0.0],  // Fall
        [75.0, 15.0, 0.0, 0.0, 5.0, 5.0, 0.0],  // Spring
        [85.0, 15.0, 0.0, 0.0, 0.0, 0.0, 0.0],  // Summer
        [75.0, 15.0, 0.0, 3.0, 5.0, 2.0, 0.0],  // Winter
    ],
    // Mountains
    [
        [30.0, 18.0, 20.0, 2.0, 20.0, 10.0, 0.0], // Fall
        [30.0, 23.0, 15.0, 2.0, 20.0, 10.0, 0.0], // Spring
        [45.0, 25.0, 15.0, 0.0, 10.0, 5.0, 0.0],  // Summer
        [18.0, 20.0, 25.0, 2.0, 0.0, 0.0, 35.0],  // Winter
    ],
    // Jungle
    [
        [20.0, 20.0, 20.0, 0.0, 25.0, 15.0, 0.0], // Fall
        [20.0, 15.0, 10.0, 3.0, 37.0, 15.0, 0.0], // Spring
        [35.0, 20.0, 10.0, 0.0, 25.0, 10.0, 0.0], // Summer
        [15.0, 20.0, 25.0, 3.0, 25.0, 12.0, 0.0], // Winter
    ],
    // Swamp
    [
        [15.0, 15.0, 15.0, 20.0, 20.0, 15.0, 0.0], // Fall
        [10.0, 10.0, 20.0, 20.0, 25.0, 15.0, 0.0], // Spring
        [25.0, 15.0, 15.0, 15.0, 20.0, 10.0, 0.0], // Summer
        [15.0, 20.0, 25.0, 25.0, 0.0, 0.0, 15.0],  // Winter
    ],
    // Subtropical
    [
        [25.0, 20.0, 15.0, 0.0, 25.0, 15.0, 0.0], // Fall
        [30.0, 15.0, 10.0, 3.0, 27.0, 15.0, 0.0], // Spring
        [40.0, 15.0, 10.0, 0.0, 20.0, 15.0, 0.0], // Summer
        [20.0, 20.0, 20.0, 5.0, 25.0, 10.0, 0.0], // Winter
    ],
    // Woodlands
    [
        [25.0, 15.0, 20.0, 10.0, 20.0, 10.0, 0.0], // Fall
        [35.0, 15.0, 10.0, 5.0, 25.0, 10.0, 0.0],  // Spring
        [60.0, 20.0, 5.0, 0.0, 10.0, 5.0, 0.0],    // Summer
        [25.0, 15.0, 20.0, 5.0, 0.0, 0.0, 25.0],   // Winter (sums 90)
    ],
];

/// `WeatherClimateSeason` for one climate and season, with `CompileOdds`
/// normalization cached by construction.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WeatherClimateSeason {
    /// Raw percent row (`WeatherTable.json` values).
    raw: WeatherOdds,
    /// Normalized percent row (`Validate`, `Weather.cs:70-84`).
    odds: WeatherOdds,
}

impl WeatherClimateSeason {
    /// Builds the row for `climate`/`season` and normalizes it like
    /// `Validate()`: untouched when `|Σ-100| < 0.1`, scaled by `100/Σ`
    /// otherwise.
    pub fn new(climate: Climate, season: Season) -> Self {
        let raw = WEATHER_TABLE[climate.index()][season as usize];
        let total: f64 = raw.iter().sum();
        let odds = if (total - 100.0).abs() < 0.1 || total <= 0.0 {
            raw
        } else {
            let scale = 100.0 / total;
            let mut odds = raw;
            for chance in &mut odds {
                *chance *= scale;
            }
            odds
        };
        Self { raw, odds }
    }

    /// Raw row (percent, before normalization).
    pub fn raw(&self) -> WeatherOdds {
        self.raw
    }

    /// Normalized row (percent, after normalization).
    pub fn odds(&self) -> WeatherOdds {
        self.odds
    }

    /// `GetWeather()` (`Weather.cs:113-127`): cumulative subtraction in
    /// [`COMPILE_ORDER`] with an inclusive `<= 0` test; Sunny fallback.
    pub fn get_weather(&self, rand: f64) -> WeatherType {
        let mut remaining = rand;
        for weather in COMPILE_ORDER {
            remaining -= self.odds[weather.index()];
            if remaining <= 0.0 {
                return weather;
            }
        }
        WeatherType::Sunny
    }
}

/// Probability of `weather` for `climate`/`season` after normalization, in
/// `0.0..=1.0` (basis for the SPEC-M31 §5-B frequency check).
pub fn probability(climate: Climate, season: Season, weather: WeatherType) -> f64 {
    WeatherClimateSeason::new(climate, season).odds()[weather.index()] / 100.0
}

/// Engine PRNG for weather draws (SPEC-M31 §8 seed policy): splitmix64 with an
/// explicit seed, uniform over `[0,100)` at 0.01 % resolution. The source's
/// `Random.Range(0f, 100f)` is [0,100); the fallback in `GetWeather` is
/// therefore unreachable for a full table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WeatherRng {
    state: u64,
}

impl Default for WeatherRng {
    fn default() -> Self {
        Self::new(0)
    }
}

impl WeatherRng {
    /// Seeded stream (`0` is a valid seed: splitmix64 has no zero fixed point).
    pub const fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    /// Re-seeds the stream (`DFRandom.srand` analogue).
    pub fn srand(&mut self, seed: u64) {
        self.state = seed;
    }

    /// Current internal state (reproducibility evidence).
    pub fn state(&self) -> u64 {
        self.state
    }

    /// Uniform draw in `[0,100)` with 0.01 % resolution.
    pub fn next_percent(&mut self) -> f64 {
        loop {
            let value = (self.next_u64() >> 32) as u32;
            // Rejection over the largest multiple of 10 000 that fits in u32:
            // every accepted value maps to exactly one basis point.
            let limit = (u32::MAX / 10_000) * 10_000;
            if value < limit {
                return f64::from(value % 10_000) / 100.0;
            }
        }
    }

    /// `GetWeather()` for `climate`/`season` using the next random draw.
    pub fn draw(&mut self, climate: Climate, season: Season) -> WeatherType {
        let rand = self.next_percent();
        WeatherClimateSeason::new(climate, season).get_weather(rand)
    }

    /// splitmix64 step (explicit state → 0 global state).
    fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
}

/// Default seed of the weather stream (SPEC-M31 §5 seed policy).
pub const WEATHER_SEED: u64 = 0x0038_A1E6;

/// `SetClimateWeathers()` (`WeatherManager.cs:424-432`): one draw per climate
/// in [`CLIMATES`] order, applied later by
/// [`weather_for_climate_value`].
pub fn draw_climate_weathers(rng: &mut WeatherRng, season: Season) -> [WeatherType; 6] {
    let mut array = [WeatherType::Sunny; 6];
    for (slot, climate) in array.iter_mut().zip(CLIMATES) {
        *slot = rng.draw(climate, season);
    }
    array
}

/// `SetWeatherFromWeatherClimateArray()` (`:434-446`): the array is indexed by
/// the shared `TravelTimeCalculator.climateIndices` table
/// (raw `CLIMATE.PAK` value − 223), **not** by [`Climate`] — MountainWoods and
/// Woodlands share slot 5, exactly like the source.
pub fn weather_for_climate_value(
    array: &[WeatherType; 6],
    climate_value: u8,
) -> Option<WeatherType> {
    let offset = usize::from(climate_value.checked_sub(crate::travel::OCEAN_CLIMATE)?);
    let index = crate::travel::CLIMATE_INDICES.get(offset).copied()?;
    array.get(usize::from(index)).copied()
}

/// Outdoor weather state after `SetWeather` (`WeatherManager.cs:419-470`):
/// `ClearAllWeather()` (all flags false) and then the target's flags.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct WeatherState {
    /// `PlayerWeather.WeatherType`.
    pub current: WeatherType,
    /// `IsRaining`.
    pub is_raining: bool,
    /// `IsStorming`.
    pub is_storming: bool,
    /// `IsOvercast`.
    pub is_overcast: bool,
    /// `IsSnowing`.
    pub is_snowing: bool,
}

impl WeatherState {
    /// Applies a weather transition: all flags are cleared first, then set
    /// from the target state (`ClearAllWeather` + `SetWeather`).
    pub fn apply(&mut self, next: WeatherType) {
        self.current = next;
        self.is_raining = next.is_raining();
        self.is_storming = next.is_storming();
        self.is_snowing = next.is_snowing();
        self.is_overcast = next.is_overcast();
    }

    /// Density of the current outdoor fog (`WeatherManager.FogSettings`).
    pub fn fog_density(&self) -> f64 {
        fog_density(self.current)
    }
}

/// `DuskHour` sunlight scale for a winter day (`:38-46`).
pub const WINTER_SUNLIGHT_SCALE: f64 = 0.65;
/// `OvercastSunlightScale`.
pub const OVERCAST_SUNLIGHT_SCALE: f64 = 0.65;
/// `RainSunlightScale`.
pub const RAIN_SUNLIGHT_SCALE: f64 = 0.45;
/// `StormSunlightScale`.
pub const STORM_SUNLIGHT_SCALE: f64 = 0.25;
/// `SnowSunlightScale`.
pub const SNOW_SUNLIGHT_SCALE: f64 = 0.45;

/// `WinterShadowStrength` (`:48-58`).
pub const WINTER_SHADOW_STRENGTH: f64 = 0.8;
/// `OvercastShadowStrength`.
pub const OVERCAST_SHADOW_STRENGTH: f64 = 0.6;
/// `RainShadowStrength`.
pub const RAIN_SHADOW_STRENGTH: f64 = 0.4;
/// `StormShadowStrength`.
pub const STORM_SHADOW_STRENGTH: f64 = 0.4;
/// `SnowShadowStrength`.
pub const SNOW_SHADOW_STRENGTH: f64 = 0.25;
/// `SunlightManager.defaultShadowStrength` is outside the pinned subcorpus:
/// the engine's documented default (identity) is 1.0.
pub const DEFAULT_SHADOW_STRENGTH: f64 = 1.0;
/// `SunlightManager.defaultScaleFactor`, same custody note as above.
pub const DEFAULT_SUNLIGHT_SCALE: f64 = 1.0;

/// Outdoor fog density per state (`FogSettings`, `WeatherManager.cs:60-71`).
/// `Fog` uses `HeavyFogSettings` (0.05); the rest of the states declare their
/// own settings (Sunny/Overcast are `Linear` with density 0).
pub fn fog_density(weather: WeatherType) -> f64 {
    match weather {
        WeatherType::Sunny | WeatherType::Cloudy | WeatherType::Overcast => 0.0,
        WeatherType::Fog => 0.05,
        WeatherType::Rain | WeatherType::Thunder => 0.003,
        WeatherType::Snow => 0.005,
    }
}

/// `SetSunlightScale` (`WeatherManager.cs:309-345`): winter first, then
/// rain/storm/snow/overcast override.
pub fn sunlight_scale(weather: WeatherType, winter: bool) -> f64 {
    let mut scale = DEFAULT_SUNLIGHT_SCALE;
    if winter {
        scale = WINTER_SUNLIGHT_SCALE;
    }
    if weather.is_raining() && !weather.is_storming() {
        scale = RAIN_SUNLIGHT_SCALE;
    } else if weather.is_raining() && weather.is_storming() {
        scale = STORM_SUNLIGHT_SCALE;
    } else if weather.is_snowing() {
        scale = SNOW_SUNLIGHT_SCALE;
    } else if weather.is_overcast() {
        scale = OVERCAST_SUNLIGHT_SCALE;
    }
    scale
}

/// Shadow strength before fog attenuation (`:309-345`).
pub fn shadow_strength(weather: WeatherType, winter: bool) -> f64 {
    let mut strength = DEFAULT_SHADOW_STRENGTH;
    if winter {
        strength = WINTER_SHADOW_STRENGTH;
    }
    if weather.is_raining() && !weather.is_storming() {
        strength = RAIN_SHADOW_STRENGTH;
    } else if weather.is_raining() && weather.is_storming() {
        strength = STORM_SHADOW_STRENGTH;
    } else if weather.is_snowing() {
        strength = SNOW_SHADOW_STRENGTH;
    } else if weather.is_overcast() {
        strength = OVERCAST_SHADOW_STRENGTH;
    }
    strength
}

/// `shadowStrength *= exp(-50 * density)` (`:343`): the fog attenuation the
/// engine applies to the shadow strength of the current outdoor state.
pub fn attenuated_shadow(weather: WeatherType, winter: bool) -> f64 {
    shadow_strength(weather, winter) * (-50.0 * fog_density(weather)).exp()
}

/// `IsSnowFreeClimate` (`:261-276`): Desert, Desert2, Rainforest and
/// Subtropical never receive snow.
pub fn is_snow_free(climate: Climate) -> bool {
    matches!(
        climate,
        Climate::Desert | Climate::Jungle | Climate::Subtropical
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 5e-5
    }

    #[test]
    fn table_sums_are_100_except_woodlands_winter() {
        let mut off_target = Vec::new();
        for climate in CLIMATES {
            for season in [
                Season::Fall,
                Season::Spring,
                Season::Summer,
                Season::Winter,
            ] {
                let row = WEATHER_TABLE[climate.index()][season as usize];
                let total: f64 = row.iter().sum();
                if (total - 100.0).abs() >= 0.1 {
                    off_target.push((climate.name(), season.name(), total));
                }
            }
        }
        assert_eq!(off_target, vec![("Woodlands", "Winter", 90.0)]);
    }

    #[test]
    fn woodlands_winter_is_normalized_and_sums_100() {
        let row = WeatherClimateSeason::new(Climate::Woodlands, Season::Winter);
        let odds = row.odds();
        let total: f64 = odds.iter().sum();
        assert!((total - 100.0).abs() < 0.1, "sum {total}");
        assert!(close(odds[WeatherType::Sunny.index()], 27.777_777));
        assert!(close(odds[WeatherType::Cloudy.index()], 16.666_666));
        assert!(close(odds[WeatherType::Overcast.index()], 22.222_222));
        assert!(close(odds[WeatherType::Fog.index()], 5.555_555));
        assert!(close(odds[WeatherType::Snow.index()], 27.777_777));
        assert_eq!(odds[WeatherType::Rain.index()], 0.0);
    }

    /// SPEC-M31 §4.5 W1-W11 (`rand` → weather).
    #[test]
    fn weather_golden_vectors_w1_w11() {
        let wood_summer = WeatherClimateSeason::new(Climate::Woodlands, Season::Summer);
        let mountain_winter = WeatherClimateSeason::new(Climate::Mountains, Season::Winter);
        let wood_winter = WeatherClimateSeason::new(Climate::Woodlands, Season::Winter);
        let cases = [
            (&wood_summer, 0.0, WeatherType::Sunny),
            (&wood_summer, 60.1, WeatherType::Cloudy),
            (&wood_summer, 80.1, WeatherType::Overcast),
            (&wood_summer, 85.1, WeatherType::Rain),
            (&wood_summer, 95.1, WeatherType::Thunder),
            (&mountain_winter, 18.0, WeatherType::Sunny),
            (&mountain_winter, 18.01, WeatherType::Cloudy),
            (&mountain_winter, 65.0, WeatherType::Fog),
            (&mountain_winter, 65.01, WeatherType::Snow),
            (&wood_winter, 27.0, WeatherType::Sunny),
            (&wood_winter, 27.9, WeatherType::Cloudy),
        ];
        for (index, (row, rand, expected)) in cases.iter().enumerate() {
            assert_eq!(
                row.get_weather(*rand),
                *expected,
                "W{} rand={rand}",
                index + 1
            );
        }
    }

    #[test]
    fn enum_order_would_break_w5_and_w9() {
        // Falsifier (b) of SPEC-M31 §5: selecting with the enum order would
        // hit Snow (index 6) before Thunder (index 5) for W5/W9.
        assert_eq!(COMPILE_ORDER[5], WeatherType::Snow);
        assert_eq!(COMPILE_ORDER[6], WeatherType::Thunder);
    }

    /// SPEC-M31 §4.5 W12-W13: fog-attenuated shadow strength.
    #[test]
    fn shadow_golden_vectors_w12_w13() {
        // The vector is stated at 4 decimals (0.3443 / 0.1947).
        let rain = attenuated_shadow(WeatherType::Rain, false);
        assert_eq!((rain * 10_000.0).round() / 10_000.0, 0.3443);
        let snow = attenuated_shadow(WeatherType::Snow, false);
        assert_eq!((snow * 10_000.0).round() / 10_000.0, 0.1947);
    }

    #[test]
    fn seeded_stream_is_reproducible_and_in_range() {
        let mut a = WeatherRng::new(WEATHER_SEED);
        let mut b = WeatherRng::new(WEATHER_SEED);
        for _ in 0..10_000 {
            let value = a.next_percent();
            assert!((0.0..100.0).contains(&value));
            assert_eq!(value, b.next_percent());
        }
        assert_eq!(a.state(), b.state());
    }

    /// SPEC-M31 §5-B: 10 000 draws per (climate, season), `max_abs_delta`
    /// below 0.02 against the declared (normalized) distribution.
    #[test]
    fn ten_thousand_draws_match_the_table() {
        let mut rng = WeatherRng::new(WEATHER_SEED);
        let mut worst: f64 = 0.0;
        for climate in CLIMATES {
            for season in [
                Season::Fall,
                Season::Spring,
                Season::Summer,
                Season::Winter,
            ] {
                let row = WeatherClimateSeason::new(climate, season);
                let mut counts = [0u32; 7];
                for _ in 0..10_000 {
                    let drawn = rng.draw(climate, season);
                    counts[drawn.index()] += 1;
                }
                for weather in WEATHER_TYPES {
                    let observed = f64::from(counts[weather.index()]) / 10_000.0;
                    let expected = row.odds()[weather.index()] / 100.0;
                    worst = worst.max((observed - expected).abs());
                }
            }
        }
        assert!(worst < 0.02, "max_abs_delta={worst}");
    }

    #[test]
    fn set_weather_clears_all_flags_before_the_target() {
        let mut state = WeatherState::default();
        state.apply(WeatherType::Thunder);
        assert!(state.is_raining && state.is_storming && state.is_overcast);
        assert!(!state.is_snowing);
        state.apply(WeatherType::Fog);
        assert!(!state.is_raining && !state.is_storming && !state.is_snowing);
        assert!(state.is_overcast);
        assert!((state.fog_density() - 0.05).abs() < f64::EPSILON);
        state.apply(WeatherType::Sunny);
        assert!(!state.is_overcast && state.fog_density() == 0.0);
    }

    #[test]
    fn climate_values_map_to_the_table_rows() {
        assert_eq!(Climate::from_climate_value(223), Some(Climate::Swamp));
        assert_eq!(Climate::from_climate_value(225), Some(Climate::Desert));
        assert_eq!(Climate::from_climate_value(230), Some(Climate::Mountains));
        assert_eq!(
            Climate::from_climate_value(232),
            Some(Climate::Woodlands)
        );
        assert_eq!(Climate::from_climate_value(0), None);
        assert!(is_snow_free(Climate::Desert));
        assert!(is_snow_free(Climate::Jungle));
        assert!(!is_snow_free(Climate::Mountains));
    }

    #[test]
    fn climate_array_route_uses_the_shared_indices() {
        let array = [
            WeatherType::Sunny,
            WeatherType::Rain,
            WeatherType::Fog,
            WeatherType::Snow,
            WeatherType::Cloudy,
            WeatherType::Overcast,
        ];
        // 226 Mountain → slot 1; 230 MountainWoods → slot 5 (source quirk).
        assert_eq!(
            weather_for_climate_value(&array, 226),
            Some(WeatherType::Rain)
        );
        assert_eq!(
            weather_for_climate_value(&array, 230),
            Some(WeatherType::Overcast)
        );
        assert_eq!(weather_for_climate_value(&array, 222), None);
    }

    #[test]
    fn sunlight_scales_follow_precedence() {
        assert_eq!(sunlight_scale(WeatherType::Rain, false), 0.45);
        assert_eq!(sunlight_scale(WeatherType::Thunder, false), 0.25);
        assert_eq!(sunlight_scale(WeatherType::Snow, false), 0.45);
        assert_eq!(sunlight_scale(WeatherType::Overcast, false), 0.65);
        assert_eq!(sunlight_scale(WeatherType::Cloudy, false), 1.0);
        assert_eq!(sunlight_scale(WeatherType::Cloudy, true), 0.65);
        assert_eq!(shadow_strength(WeatherType::Snow, false), 0.25);
        assert_eq!(shadow_strength(WeatherType::Sunny, true), 0.8);
    }
}
