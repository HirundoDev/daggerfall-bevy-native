//! `calendar` — Daggerfall calendar view: derived dates, seasons, birth signs
//! and the bit-exact lunar phases (M31, SPEC-M31 §3 · REQ-L99-064).
//!
//! **Source custody (clean-room, MIT):** values and formulas transcribed from
//! `reference/dfu-spec/Assets/Scripts/Utility/DaggerfallDateTime.cs` (DFU
//! v1.1.1, upstream `0038a1e6…`): constants `:33-55`, defaults `:58-62`,
//! `IsDay`/`IsNight` `:163-174`, `GetSeasonValue` `:577-607`, `GetDayOfYear`
//! `:629-633`, `GetMonthOfYear` `:635-639`, `GetLunarPhase` `:657-690`.
//!
//! **Custody of state:** the carrying lives in [`crate::clock::GameClock`]
//! (M4); this module only *derives* a calendar view from its public fields,
//! so `clock.rs` keeps its frozen signature (SPEC-M31 §6 delta) and 0 file or
//! floating-point state enters the calendar. All arithmetic is integer:
//! `Year*360` is 145 800 at the classic start, which needs `i64` (SPEC §8.7).

use crate::clock::{DAYS_PER_MONTH, MONTHS_PER_YEAR, GameClock};

/// Days per week (`DaggerfallDateTime.DaysPerWeek`, `:37`).
pub const DAYS_PER_WEEK: u32 = 7;
/// Days per year (`DaysPerMonth * MonthsPerYear`, `:42`) = 360.
pub const DAYS_PER_YEAR: u64 = DAYS_PER_MONTH * MONTHS_PER_YEAR;
/// Seconds per hour (`:44`).
pub const SECONDS_PER_HOUR: u64 = 3600;
/// Seconds per day (`:45`) = 86 400.
pub const SECONDS_PER_DAY: u64 = SECONDS_PER_HOUR * 24;
/// Seconds per year (`:47`) = 31 104 000.
pub const SECONDS_PER_YEAR: u64 = SECONDS_PER_DAY * DAYS_PER_YEAR;
/// Midday hour (`MiddayHour`, `:52`).
pub const MIDDAY_HOUR: u32 = 12;
/// Midnight hour (`MidnightHour`, `:53`).
pub const MIDNIGHT_HOUR: u32 = 0;
/// Classic game start in minutes from the classic epoch (`:30`).
pub const CLASSIC_GAME_START_MINUTES: u64 = 523_530;
/// Lunar cycle length in days (`GetLunarPhase`, `:676`).
pub const LUNAR_CYCLE_DAYS: i64 = 32;

/// Season, ordered as in game (`DaggerfallDateTime.Seasons`, `:275-281`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u8)]
pub enum Season {
    /// `Fall` = 0.
    Fall = 0,
    /// `Spring` = 1.
    Spring = 1,
    /// `Summer` = 2.
    Summer = 2,
    /// `Winter` = 3.
    Winter = 3,
}

/// Season names in enum order (`seasonNames`, index = discriminant).
pub const SEASON_NAMES: [&str; 4] = ["Fall", "Spring", "Summer", "Winter"];

impl Season {
    /// `GetSeasonValue()` (`:577-607`): months `{11,0,1}` → Winter,
    /// `{2,3,4}` → Spring, `{5,6,7}` → Summer, `{8,9,10}` → Fall; the
    /// source's `default` (month outside `0..12`) is Summer.
    pub const fn from_month(month: u32) -> Self {
        match month {
            11 | 0 | 1 => Self::Winter,
            2..=4 => Self::Spring,
            5..=7 => Self::Summer,
            8..=10 => Self::Fall,
            _ => Self::Summer,
        }
    }

    /// Stable name for probes (`seasonNames`).
    pub const fn name(self) -> &'static str {
        SEASON_NAMES[self as usize]
    }
}

/// Lunar phase (`DaggerfallUnityEnums.cs:644-655`), same discriminants.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i8)]
pub enum LunarPhase {
    /// `None` = -1 (only returned by the source on `Year < 0`).
    None = -1,
    /// `New` = 0.
    New = 0,
    /// `OneWax` = 1.
    OneWax = 1,
    /// `HalfWax` = 2.
    HalfWax = 2,
    /// `ThreeWax` = 3.
    ThreeWax = 3,
    /// `Full` = 4.
    Full = 4,
    /// `ThreeWane` = 5.
    ThreeWane = 5,
    /// `HalfWane` = 6.
    HalfWane = 6,
    /// `OneWane` = 7.
    OneWane = 7,
}

/// All lunar phases in enum order (`None` excluded: it is the error value).
pub const LUNAR_PHASES: [LunarPhase; 8] = [
    LunarPhase::New,
    LunarPhase::OneWax,
    LunarPhase::HalfWax,
    LunarPhase::ThreeWax,
    LunarPhase::Full,
    LunarPhase::ThreeWane,
    LunarPhase::HalfWane,
    LunarPhase::OneWane,
];

impl LunarPhase {
    /// Stable name for probes.
    pub const fn name(self) -> &'static str {
        match self {
            Self::None => "None",
            Self::New => "New",
            Self::OneWax => "OneWax",
            Self::HalfWax => "HalfWax",
            Self::ThreeWax => "ThreeWax",
            Self::Full => "Full",
            Self::ThreeWane => "ThreeWane",
            Self::HalfWane => "HalfWane",
            Self::OneWane => "OneWane",
        }
    }

    /// `GetLunarPhase` mapping from the 0..31 ratio (`:676-689`).
    pub const fn from_ratio(ratio: i64) -> Self {
        match ratio {
            0 => Self::Full,
            16 => Self::New,
            1..=5 => Self::ThreeWane,
            6..=10 => Self::HalfWane,
            11..=15 => Self::OneWane,
            17..=22 => Self::OneWax,
            23..=28 => Self::HalfWax,
            29..=31 => Self::ThreeWax,
            _ => Self::None,
        }
    }
}

/// Birth signs by month (`DaggerfallDateTime.BirthSigns`, `:256-274`):
/// `BirthSignValue = (BirthSigns)Month` (`:104-114`).
pub const BIRTH_SIGNS: [&str; 12] = [
    "TheRitual",
    "TheLover",
    "TheLord",
    "TheMage",
    "TheShadow",
    "TheSteed",
    "TheApprentice",
    "TheWarrior",
    "TheLady",
    "TheTower",
    "TheAtronach",
    "TheThief",
];

/// `DayOfYear` = `Month*30 + Day + 1` in `1..=360` (`:629-633`).
pub fn day_of_year(clock: &GameClock) -> u64 {
    u64::from(clock.month) * DAYS_PER_MONTH + u64::from(clock.day) + 1
}

/// `MonthOfYear` = `Month + 1` in `1..=12` (`:635-639`).
pub fn month_of_year(clock: &GameClock) -> u64 {
    u64::from(clock.month) + 1
}

/// `DayOfMonth` = `Day + 1` (`:623-627`).
pub fn day_of_month(clock: &GameClock) -> u64 {
    u64::from(clock.day) + 1
}

/// `MinuteOfDay` = `Hour*60 + Minute` (`:641-646`).
pub fn minute_of_day(clock: &GameClock) -> u64 {
    u64::from(clock.hour) * 60 + u64::from(clock.minute)
}

/// Seasonal value of the date (`GetSeasonValue`, `:577-607`).
pub fn season(clock: &GameClock) -> Season {
    Season::from_month(clock.month)
}

/// Birth-sign name of the date: 1:1 with the month (`:104-114`).
pub fn birth_sign(clock: &GameClock) -> &'static str {
    BIRTH_SIGNS
        .get(clock.month as usize)
        .copied()
        .unwrap_or("TheSteed")
}

/// `GetLunarPhase(isMasser)` (`:657-690`):
/// `(DayOfYear + Year*360 + offset) % 32`, `offset = 3` for Masser (aligns
/// the vanilla full moon) and `-1` for Secunda.
pub fn lunar_ratio(clock: &GameClock, is_masser: bool) -> i64 {
    let offset: i64 = if is_masser { 3 } else { -1 };
    let absolute = day_of_year(clock) as i64 + i64::from(clock.year) * DAYS_PER_YEAR as i64;
    (absolute + offset).rem_euclid(LUNAR_CYCLE_DAYS)
}

/// Lunar phase of one moon (`:676-690`).
pub fn lunar_phase(clock: &GameClock, is_masser: bool) -> LunarPhase {
    LunarPhase::from_ratio(lunar_ratio(clock, is_masser))
}

/// Phases of both moons, in the source order `(Masser, Secunda)`.
pub fn lunar_phases(clock: &GameClock) -> (LunarPhase, LunarPhase) {
    (lunar_phase(clock, true), lunar_phase(clock, false))
}

/// Is it day? (`IsDay`, `:163-166`: `6 ≤ Hour < 18`).
pub fn is_day(clock: &GameClock) -> bool {
    clock.is_day()
}

/// Is it night? (`IsNight`, `:171-174`: exact complement of `IsDay`).
pub fn is_night(clock: &GameClock) -> bool {
    clock.is_night()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn classic_with_day(day: u32) -> GameClock {
        let mut clock = GameClock::classic_default();
        clock.day = day;
        clock
    }

    #[test]
    fn constants_match_daggerfall_date_time() {
        assert_eq!(DAYS_PER_YEAR, 360);
        assert_eq!(SECONDS_PER_YEAR, 31_104_000);
        assert_eq!(DAYS_PER_WEEK, 7);
        assert_eq!(CLASSIC_GAME_START_MINUTES, 523_530);
        assert_eq!(MIDDAY_HOUR, 12);
    }

    #[test]
    fn derived_values_of_the_classic_start() {
        let clock = GameClock::classic_default();
        // SPEC-M31 §4.4 base: Year 405, Month 5, Day 0 => DayOfYear 151.
        assert_eq!(day_of_year(&clock), 151);
        assert_eq!(month_of_year(&clock), 6);
        assert_eq!(day_of_month(&clock), 1);
        assert_eq!(minute_of_day(&clock), 720);
        assert_eq!(season(&clock), Season::Summer);
        assert_eq!(season(&clock).name(), "Summer");
        assert_eq!(birth_sign(&clock), "TheSteed");
        assert!(is_day(&clock));
        assert!(!is_night(&clock));
    }

    #[test]
    fn season_boundaries_follow_the_source_switch() {
        // Month 11/0/1 Winter · 2/3/4 Spring · 5/6/7 Summer · 8/9/10 Fall.
        assert_eq!(Season::from_month(11), Season::Winter);
        assert_eq!(Season::from_month(0), Season::Winter);
        assert_eq!(Season::from_month(1), Season::Winter);
        assert_eq!(Season::from_month(2), Season::Spring);
        assert_eq!(Season::from_month(4), Season::Spring);
        assert_eq!(Season::from_month(5), Season::Summer);
        assert_eq!(Season::from_month(7), Season::Summer);
        assert_eq!(Season::from_month(8), Season::Fall);
        assert_eq!(Season::from_month(10), Season::Fall);
        // Source `default` for an out-of-range month is Summer.
        assert_eq!(Season::from_month(12), Season::Summer);
    }

    /// SPEC-M31 §4.4 L1-L12: the golden vector of both moons.
    #[test]
    fn lunar_golden_vectors_l1_l12() {
        const TABLE: [(u32, i64, LunarPhase, i64, LunarPhase); 12] = [
            (0, 2, LunarPhase::ThreeWane, 30, LunarPhase::ThreeWax),
            (1, 3, LunarPhase::ThreeWane, 31, LunarPhase::ThreeWax),
            (2, 4, LunarPhase::ThreeWane, 0, LunarPhase::Full),
            (5, 7, LunarPhase::HalfWane, 3, LunarPhase::ThreeWane),
            (10, 12, LunarPhase::OneWane, 8, LunarPhase::HalfWane),
            (14, 16, LunarPhase::New, 12, LunarPhase::OneWane),
            (16, 18, LunarPhase::OneWax, 14, LunarPhase::OneWane),
            (20, 22, LunarPhase::OneWax, 18, LunarPhase::OneWax),
            (23, 25, LunarPhase::HalfWax, 21, LunarPhase::OneWax),
            (29, 31, LunarPhase::ThreeWax, 27, LunarPhase::HalfWax),
            (30, 0, LunarPhase::Full, 28, LunarPhase::HalfWax),
            (31, 1, LunarPhase::ThreeWane, 29, LunarPhase::ThreeWax),
        ];
        for (day, masser_ratio, masser, secunda_ratio, secunda) in TABLE {
            let clock = classic_with_day(day);
            assert_eq!(lunar_ratio(&clock, true), masser_ratio, "day +{day}");
            assert_eq!(lunar_phase(&clock, true), masser, "day +{day}");
            assert_eq!(lunar_ratio(&clock, false), secunda_ratio, "day +{day}");
            assert_eq!(lunar_phase(&clock, false), secunda, "day +{day}");
        }
    }

    /// Falsifier of SPEC-M31 §5 (a): inverting `offset` must break L3/L11.
    #[test]
    fn lunar_offset_sign_is_load_bearing() {
        let l3 = classic_with_day(2);
        assert_eq!(lunar_phase(&l3, false), LunarPhase::Full);
        // With the offset inverted, Secunda's ratio moves off 0 here.
        let inverted = (day_of_year(&l3) as i64 + i64::from(l3.year) * 360 + 1) % 32;
        assert_ne!(inverted, 0);
    }

    #[test]
    fn fifteen_hostile_days_break_the_ratio() {
        assert_eq!(LunarPhase::from_ratio(15), LunarPhase::OneWane);
        assert_eq!(LunarPhase::from_ratio(16), LunarPhase::New);
        assert_eq!(LunarPhase::from_ratio(17), LunarPhase::OneWax);
        assert_eq!(LunarPhase::from_ratio(32), LunarPhase::None);
    }
}
