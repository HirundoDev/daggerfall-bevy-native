//! M18 **B3-d (O3, CS-12)** — visual precipitation fed by the **M13 climate
//! index**.
//!
//! The weather *machine* (one draw per climate, `SetClimateWeathers` /
//! `SetWeatherFromWeatherClimateArray`) is M31's (`heart/src/weather.rs`,
//! lane M31 — this module is a **consumer**, it never mutates it). What the
//! window and `PROBE-WEATHER-VISUAL-001` need is the **precipitation state** of
//! the place the eye stands on, and the place declares its climate in
//! `CLIMATE.PAK` (M13, read-only here through
//! [`daggerfall_parser_rs::climate::ClimateMap`]).
//!
//! ## Where the numbers come from (0 invented values)
//!
//! Every probability below is a row **published in the binding M31 spec**
//! (`docs/sintergic/game/programming/m31-travel-weather-calendar-spec-CR-5243af63.md`
//! §2.1 `WeatherTable`), in the source's `CompileOdds` order
//! `[Sunny, Cloudy, Overcast, Fog, Rain, Snow, Thunder]` (`Weather.cs:113-127`),
//! **not** the `WeatherType` enum order:
//!
//! | Climate (`CLIMATE.PAK`) | Season | Sun | Clo | Ovc | Fog | Rain | Snow | Thun | Σ |
//! |:---|:---:|---:|---:|---:|---:|---:|---:|---:|---:|
//! | Desert (224) · Desert2 (225) | Winter | 75 | 15 | 0 | 3 | 5 | 0 | 2 | 100 |
//! | Desert · Desert2 | Summer | 85 | 15 | 0 | 0 | 0 | 0 | 0 | 100 |
//! | Mountain (226) · MountainWoods (230) | Winter | 18 | 20 | 25 | 2 | 0 | 35 | 0 | 100 |
//! | Mountain · MountainWoods | Summer | 45 | 25 | 15 | 0 | 10 | 0 | 5 | 100 |
//! | Rainforest (227) | Spring | 20 | 15 | 10 | 3 | 37 | 0 | 15 | 100 |
//! | Swamp (228) | Winter | 15 | 20 | 25 | 25 | 0 | 15 | 0 | 100 |
//! | Subtropical (229) | Fall | 25 | 20 | 15 | 0 | 25 | 0 | 15 | 100 |
//! | Woodlands (231) | Winter | 25 | 15 | 20 | 5 | 0 | 25 | 0 | **90** |
//! | Woodlands · HauntedWoodlands (232) | Summer | 60 | 20 | 5 | 0 | 10 | 0 | 5 | 100 |
//!
//! `Woodlands.Winter` sums **90**: `Validate()` normalises ×`100/90`
//! (`Weather.cs:70-84`) — the only row outside 100.
//!
//! **Declared demo simplification (not a spec claim):** a `CLIMATE.PAK` value
//! carries no season of its own (the season is M31's calendar), so each climate
//! family is demonstrated in the season the spec publishes for it
//! ([`row_for`]). The window prints the row it used, so the reader can see
//! which season the numbers belong to. When the M31 calendar lands, the row is
//! selected by the real season (`Season::of_month`, kept here for that day) and
//! the machine's own `WeatherType` replaces [`draw`].
//!
//! **Drop density** is a declared visual mapping (`drops = 3 × percent`),
//! [`DROPS_PER_PERCENT`]; 0 % ⇒ **0 drops** (the CS-12 negative control).

use daggerfall_parser_rs::climate::{ClimateError, WorldClimate};

/// Pixels (streaks) drawn per percent point of the precipitation chance.
///
/// Declared visual density: `Rain 10 %` ⇒ 30 streaks, `Rain 37 %` ⇒ 111.
/// It scales with the spec's own probability, so a dry row is **0** by
/// construction.
pub const DROPS_PER_PERCENT: usize = 3;

/// Weather states of `WeatherType` (`Weather.cs:15-33`).
///
/// The `CompileOdds` **order** is [`Self::ORDER`]; the enum order of the source
/// (`Sunny, Cloudy, Overcast, Fog, Rain, Thunder, Snow`) is deliberately not
/// used — W5/W9 of the spec fail with it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WeatherKind {
    /// `Sunny`.
    Sunny,
    /// `Cloudy`.
    Cloudy,
    /// `Overcast`.
    Overcast,
    /// `Fog` (= overcast + density 0.05; **no** drops).
    Fog,
    /// `Rain`.
    Rain,
    /// `Snow`.
    Snow,
    /// `Thunder` (implies rain + storm).
    Thunder,
}

impl WeatherKind {
    /// `CompileOdds` order (`Weather.cs:113-127`).
    pub const ORDER: [Self; 7] = [
        Self::Sunny,
        Self::Cloudy,
        Self::Overcast,
        Self::Fog,
        Self::Rain,
        Self::Snow,
        Self::Thunder,
    ];

    /// Stable name (evidence rows).
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Sunny => "Sunny",
            Self::Cloudy => "Cloudy",
            Self::Overcast => "Overcast",
            Self::Fog => "Fog",
            Self::Rain => "Rain",
            Self::Snow => "Snow",
            Self::Thunder => "Thunder",
        }
    }

    /// Does the state put something in the air (`IsRaining || IsSnowing`)?
    pub fn is_precipitation(self) -> bool {
        matches!(self, Self::Rain | Self::Snow | Self::Thunder)
    }
}

/// Season of `DaggerfallDateTime.Seasons` (spec §0: same values as the source).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Season {
    /// `Fall = 0`.
    Fall,
    /// `Spring = 1`.
    Spring,
    /// `Summer = 2`.
    Summer,
    /// `Winter = 3`.
    Winter,
}

impl Season {
    /// Season of a `DaggerfallDateTime.Month` (`0..=11`, spec §3.2):
    /// `{11,0,1}→Winter`, `{2,3,4}→Spring`, `{5,6,7}→Summer`, `{8,9,10}→Fall`.
    pub fn of_month(month: u8) -> Self {
        match month % 12 {
            11 | 0 | 1 => Self::Winter,
            2..=4 => Self::Spring,
            5..=7 => Self::Summer,
            _ => Self::Fall,
        }
    }

    /// Stable name (evidence rows).
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Fall => "Fall",
            Self::Spring => "Spring",
            Self::Summer => "Summer",
            Self::Winter => "Winter",
        }
    }
}

/// One published `WeatherTable` row (`WeatherTable.json`, spec §2.1).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WeatherRow {
    /// Climate family name as the table spells it (`Desert`, `Woodlands`…).
    pub family: &'static str,
    /// Season the row belongs to.
    pub season: Season,
    /// Percent per state in [`WeatherKind::ORDER`].
    pub chance: [u8; 7],
}

impl WeatherRow {
    /// Σ of the row as published (`Woodlands/Winter` = 90).
    pub fn sum(&self) -> u32 {
        self.chance.iter().map(|value| u32::from(*value)).sum()
    }

    /// Normalisation factor: ×`100/Σ` only when |Σ − 100| ≥ 0.1
    /// (`Weather.cs:70-84`; every other published row is exactly 100).
    pub fn scale(&self) -> f32 {
        let sum = self.sum() as f32;
        if (sum - 100.0).abs() >= 0.1 {
            100.0 / sum
        } else {
            1.0
        }
    }

    /// Normalised percent of one state (`chance × scale`).
    pub fn percent(&self, kind: WeatherKind) -> f32 {
        let index = WeatherKind::ORDER
            .iter()
            .position(|candidate| *candidate == kind)
            .unwrap_or(0);
        f32::from(self.chance[index]) * self.scale()
    }

    /// Label of the row for the evidence lines (`Woodlands/Summer`).
    pub fn label(&self) -> String {
        format!("{}/{}", self.family, self.season.as_str())
    }
}

/// The rows the binding spec publishes (spec §2.1), in table order.
pub const PUBLISHED_ROWS: [WeatherRow; 9] = [
    WeatherRow { family: "Desert", season: Season::Winter, chance: [75, 15, 0, 3, 5, 0, 2] },
    WeatherRow { family: "Desert", season: Season::Summer, chance: [85, 15, 0, 0, 0, 0, 0] },
    WeatherRow { family: "Mountains", season: Season::Winter, chance: [18, 20, 25, 2, 0, 35, 0] },
    WeatherRow { family: "Mountains", season: Season::Summer, chance: [45, 25, 15, 0, 10, 0, 5] },
    WeatherRow { family: "Jungle", season: Season::Spring, chance: [20, 15, 10, 3, 37, 0, 15] },
    WeatherRow { family: "Swamp", season: Season::Winter, chance: [15, 20, 25, 25, 0, 15, 0] },
    WeatherRow { family: "Subtropical", season: Season::Fall, chance: [25, 20, 15, 0, 25, 0, 15] },
    WeatherRow { family: "Woodlands", season: Season::Winter, chance: [25, 15, 20, 5, 0, 25, 0] },
    WeatherRow { family: "Woodlands", season: Season::Summer, chance: [60, 20, 5, 0, 10, 0, 5] },
];

/// `WeatherTable` row of a climate **as the spec publishes it**.
///
/// The climate → row mapping is the source's own (`Weather.cs:205-227`):
/// `Desert|Desert2→Desert`, `Mountain|MountainWoods→Mountains`,
/// `Rainforest→Jungle`, `Ocean|Swamp→Swamp`, `Subtropical→Subtropical`,
/// `Woodlands|HauntedWoodlands→Woodlands`. The season is the one the spec
/// publishes for that row (declared demo simplification, module docs).
///
/// `Ocean` has no published row (`Ocean→Swamp` in the source, but the window
/// never stands on `Ocean` — the water is not walkable, CS-08): it resolves to
/// the `Swamp` row, and the caller prints which row it used.
pub fn row_for(climate: WorldClimate) -> &'static WeatherRow {
    match climate {
        // `Desert/Summer` is the dry row the spec publishes with 0 % rain and
        // 0 % snow: the CS-12 negative control.
        WorldClimate::Desert | WorldClimate::Desert2 => &PUBLISHED_ROWS[1],
        WorldClimate::Mountain | WorldClimate::MountainWoods => &PUBLISHED_ROWS[2],
        WorldClimate::Rainforest => &PUBLISHED_ROWS[4],
        WorldClimate::Ocean | WorldClimate::Swamp => &PUBLISHED_ROWS[5],
        WorldClimate::Subtropical => &PUBLISHED_ROWS[6],
        WorldClimate::Woodlands | WorldClimate::HauntedWoodlands => &PUBLISHED_ROWS[8],
    }
}

/// `WeatherClimateSeason.GetWeather` (spec §2.2): subtract each normalised
/// chance from `rand` in [`WeatherKind::ORDER`] and return the state whose
/// subtraction reaches `≤ 0`; `Sunny` if the row is exhausted.
///
/// `rand ∈ [0,100)` — `Random.Range(0f, 100f)` never yields 100.
pub fn draw(row: &WeatherRow, rand: f32) -> WeatherKind {
    let mut left = rand;
    for kind in WeatherKind::ORDER {
        left -= row.percent(kind);
        if left <= 0.0 {
            return kind;
        }
    }
    WeatherKind::Sunny
}

/// Streaks the visual layer draws for the resolved state.
///
/// `Rain`/`Snow` scale with their own normalised percent; `Thunder` implies
/// rain (`IsRaining && IsStorming`) so it adds the rain share; `Fog`,
/// `Sunny`, `Cloudy` and `Overcast` are **0** (fog is not precipitation).
pub fn drops(row: &WeatherRow, kind: WeatherKind) -> usize {
    let percent = match kind {
        WeatherKind::Rain | WeatherKind::Snow => row.percent(kind),
        WeatherKind::Thunder => row.percent(WeatherKind::Rain) + row.percent(WeatherKind::Thunder),
        WeatherKind::Sunny | WeatherKind::Cloudy | WeatherKind::Overcast | WeatherKind::Fog => 0.0,
    };
    (percent * DROPS_PER_PERCENT as f32).round() as usize
}

/// Resolved precipitation of one place: the M13 climate → row → state → drops.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Precipitation {
    /// `CLIMATE.PAK` value (`223..=232`).
    pub climate: u8,
    /// Row used (published percentages).
    pub row: &'static WeatherRow,
    /// `rand` handed to [`draw`] (`[0,100)`).
    pub roll: f32,
    /// State the draw resolved.
    pub kind: WeatherKind,
    /// Streaks to render.
    pub drops: usize,
}

impl Precipitation {
    /// Resolves the precipitation of a place from its `CLIMATE.PAK` value.
    pub fn resolve(climate: u8, roll: f32) -> Result<Self, ClimateError> {
        let world = WorldClimate::from_index(climate)?;
        let row = row_for(world);
        let kind = draw(row, roll);
        Ok(Self {
            climate,
            row,
            roll,
            kind,
            drops: drops(row, kind),
        })
    }

    /// Evidence line (`M18 VE: precipitation …` / probe rows).
    pub fn evidence(&self) -> String {
        format!(
            "climate={} ({}) row={} roll={:.2} kind={} drops={}",
            self.climate,
            WorldClimate::from_index(self.climate)
                .map(|climate| climate.name())
                .unwrap_or("?"),
            self.row.label(),
            self.roll,
            self.kind.as_str(),
            self.drops
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn published_rows_only_normalise_the_woodlands_winter_row() {
        for row in &PUBLISHED_ROWS {
            if row.family == "Woodlands" && row.season == Season::Winter {
                assert_eq!(row.sum(), 90);
                assert!((row.scale() - 100.0 / 90.0).abs() < 1e-5);
                assert!((row.percent(WeatherKind::Sunny) - 27.777_778).abs() < 1e-3);
            } else {
                assert_eq!(row.sum(), 100, "{}", row.label());
                assert_eq!(row.scale(), 1.0);
            }
        }
    }

    #[test]
    fn spec_vectors_w1_to_w11() {
        let woodlands_summer = &PUBLISHED_ROWS[8];
        assert_eq!(draw(woodlands_summer, 0.0), WeatherKind::Sunny);
        assert_eq!(draw(woodlands_summer, 60.1), WeatherKind::Cloudy);
        assert_eq!(draw(woodlands_summer, 80.1), WeatherKind::Overcast);
        assert_eq!(draw(woodlands_summer, 85.1), WeatherKind::Rain);
        assert_eq!(draw(woodlands_summer, 95.1), WeatherKind::Thunder);
        let mountains_winter = &PUBLISHED_ROWS[2];
        assert_eq!(draw(mountains_winter, 18.0), WeatherKind::Sunny);
        assert_eq!(draw(mountains_winter, 18.01), WeatherKind::Cloudy);
        assert_eq!(draw(mountains_winter, 65.0), WeatherKind::Fog);
        assert_eq!(draw(mountains_winter, 65.01), WeatherKind::Snow);
        let woodlands_winter = &PUBLISHED_ROWS[7];
        assert_eq!(draw(woodlands_winter, 27.0), WeatherKind::Sunny);
        assert_eq!(draw(woodlands_winter, 27.9), WeatherKind::Cloudy);
    }

    #[test]
    fn dry_desert_summer_has_no_precipitation_and_zero_drops() {
        let dry = Precipitation::resolve(224, 50.0).expect("Desert");
        assert_eq!(dry.kind, WeatherKind::Sunny);
        assert_eq!(dry.drops, 0);
        // The same roll on the humid row rains.
        let wet = Precipitation::resolve(227, 50.0).expect("Rainforest");
        assert_eq!(wet.kind, WeatherKind::Rain);
        assert!(wet.drops > 0);
    }

    #[test]
    fn fog_and_overcast_are_not_precipitation() {
        let row = &PUBLISHED_ROWS[2];
        assert_eq!(drops(row, WeatherKind::Fog), 0);
        assert_eq!(drops(row, WeatherKind::Overcast), 0);
        assert!(drops(row, WeatherKind::Snow) > 0);
        assert!(!WeatherKind::Fog.is_precipitation());
        assert!(WeatherKind::Thunder.is_precipitation());
    }

    #[test]
    fn season_of_month_matches_the_source_table() {
        assert_eq!(Season::of_month(11), Season::Winter);
        assert_eq!(Season::of_month(0), Season::Winter);
        assert_eq!(Season::of_month(2), Season::Spring);
        assert_eq!(Season::of_month(5), Season::Summer);
        assert_eq!(Season::of_month(8), Season::Fall);
    }

    #[test]
    fn climate_out_of_range_is_a_typed_error() {
        assert!(Precipitation::resolve(222, 0.0).is_err());
        assert!(Precipitation::resolve(233, 0.0).is_err());
    }
}
