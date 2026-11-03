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

/// Inverse of [`day_of_year`] (`:629-633`): `(Month, Day)` **0-based**, the
/// exact pair `DayOfYear = Month*30 + Day + 1` was built from. `None` outside
/// `1..=360`, so an out-of-range holiday never silently lands on a date.
pub fn date_of_day_of_year(day_of_year: u64) -> Option<(u32, u32)> {
    if !(1..=DAYS_PER_YEAR).contains(&day_of_year) {
        return None;
    }
    let zero_based = day_of_year - 1;
    Some((
        (zero_based / DAYS_PER_MONTH) as u32,
        (zero_based % DAYS_PER_MONTH) as u32,
    ))
}

/// Holidays of the classic year (`W43-5`, SPEC §3.8): the names of
/// `DFLocation.Holidays` ids `1..=53` (`DFLocation.cs:286-343`) in enum order.
///
/// `None` (id 0) and `Old_Life_Festival` (id 54, marked *Not used* in the
/// source, `:342`) are **not** in the table: they are exactly the two ids the
/// `dayOfYear` population (`FormulaHelper.cs:1815-1822`, mirrored in
/// `behavior::economy::holidays::DAY`) never enumerates.
pub mod holiday {
    use super::date_of_day_of_year;

    /// What the engine declares a holiday **does**. Every one of the 53
    /// holidays carries a declared effect, including [`HolidayEffect::None`]:
    /// "this festival changes no price and no service" is a declaration, not a
    /// gap, so no holiday can be left undeclared.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum HolidayEffect {
        /// Declared: no price and no service changes (`Behavior` default).
        None,
        /// Inn room free for the night (`CalculateRoomCost`, Heart's Day).
        FreeRoom,
        /// Half price in normal shops (`DaggerfallTradeWindow:444-449`).
        HalfPriceNormalShop,
        /// Half price in the Mages Guild (`:444-449`).
        HalfPriceMagesGuild,
        /// Half price for weapons in normal shops (`:444-449`).
        HalfPriceWeapons,
        /// Free item identification (`FormulaHelper.CalculateIdentifyCost`).
        FreeIdentify,
        /// Free temple cure (`DaggerfallGuildServiceCureDisease:79`).
        FreeCure,
        /// Half-price temple cure (`:79`).
        HalfCure,
    }

    impl HolidayEffect {
        /// Stable name for probes and logs.
        pub const fn name(self) -> &'static str {
            match self {
                Self::None => "None",
                Self::FreeRoom => "FreeRoom",
                Self::HalfPriceNormalShop => "HalfPriceNormalShop",
                Self::HalfPriceMagesGuild => "HalfPriceMagesGuild",
                Self::HalfPriceWeapons => "HalfPriceWeapons",
                Self::FreeIdentify => "FreeIdentify",
                Self::FreeCure => "FreeCure",
                Self::HalfCure => "HalfCure",
            }
        }
    }

    /// `DFLocation.Holidays` ids `1..=53`, in enum order (`:286-341`).
    pub const HOLIDAY_NAMES: [&str; 53] = [
        "New_Life",             // 1
        "Scour_Day",            // 2
        "Ovanka",               // 3
        "South_Winds_Prayer",   // 4
        "Day_of_Lights",        // 5
        "Waking_Day",           // 6
        "Mad_Pelagius",         // 7
        "Othroktide",           // 8
        "Day_of_Release",       // 9
        "Hearts_Day",           // 10
        "Perserverance_Day",    // 11
        "Aduros_Nau",           // 12
        "First_Harvest",        // 13
        "Day_of_Waiting",       // 14
        "Flower_Day",           // 15
        "Festival_of_Blades",   // 16
        "Gardtide",             // 17
        "Day_of_the_Dead",      // 18
        "Day_of_Shame",         // 19
        "Jesters_Day",          // 20
        "Second_Harvest",       // 21
        "Marukhs_Day",          // 22
        "Fire_Festival",        // 23
        "Fishing_Day",          // 24
        "Drigh_RZimb",          // 25
        "Mid_Year",             // 26
        "Dancing_Day",          // 27
        "Tibedetha",            // 28
        "Merchants_Festival",   // 29
        "Divad_Etept",          // 30
        "Suns_Rest",            // 31
        "Fiery_Night",          // 32
        "Maiden_Katrica",       // 33
        "Koomu_Alazeri",        // 34
        "Feast_of_the_Tiger",   // 35
        "Appreciation_Day",     // 36
        "Harvest_End",          // 37
        "Tales_and_Tallow",     // 38
        "Khurat",               // 39
        "Riglametha",           // 40
        "Childrens_Day",        // 41
        "Dirij_Tereur",         // 42
        "Witches_Festival",     // 43
        "Broken_Diamonds",      // 44
        "Emperors_Birthday",    // 45
        "Serpents_Dance",       // 46
        "Moon_Festival",        // 47
        "Hel_Anseilak",         // 48
        "Warriors_Festival",    // 49
        "North_Winds_Festival", // 50
        "Baranth_Do",           // 51
        "Chila",                // 52
        "Saturalia",            // 53
    ];

    /// The effect each holiday declares, index-aligned with
    /// [`HOLIDAY_NAMES`] (id = index + 1). The nine mechanical festivals are
    /// the ones the retail UI/price code branches on
    /// (`DaggerfallTradeWindow:444-449`, `FormulaHelper.CalculateRoomCost`,
    /// `CalculateIdentifyCost`, `DaggerfallGuildServiceCureDisease:79`); the
    /// other 44 declare `None`.
    pub const HOLIDAY_EFFECTS: [HolidayEffect; 53] = [
        HolidayEffect::None,               // New_Life
        HolidayEffect::None,               // Scour_Day
        HolidayEffect::None,               // Ovanka
        HolidayEffect::FreeCure,           // South_Winds_Prayer
        HolidayEffect::None,               // Day_of_Lights
        HolidayEffect::None,               // Waking_Day
        HolidayEffect::None,               // Mad_Pelagius
        HolidayEffect::None,               // Othroktide
        HolidayEffect::None,               // Day_of_Release
        HolidayEffect::FreeRoom,           // Hearts_Day
        HolidayEffect::None,               // Perserverance_Day
        HolidayEffect::None,               // Aduros_Nau
        HolidayEffect::FreeCure,           // First_Harvest
        HolidayEffect::None,               // Day_of_Waiting
        HolidayEffect::None,               // Flower_Day
        HolidayEffect::None,               // Festival_of_Blades
        HolidayEffect::None,               // Gardtide
        HolidayEffect::None,               // Day_of_the_Dead
        HolidayEffect::None,               // Day_of_Shame
        HolidayEffect::None,               // Jesters_Day
        HolidayEffect::FreeCure,           // Second_Harvest
        HolidayEffect::None,               // Marukhs_Day
        HolidayEffect::None,               // Fire_Festival
        HolidayEffect::None,               // Fishing_Day
        HolidayEffect::None,               // Drigh_RZimb
        HolidayEffect::None,               // Mid_Year
        HolidayEffect::None,               // Dancing_Day
        HolidayEffect::None,               // Tibedetha
        HolidayEffect::HalfPriceNormalShop, // Merchants_Festival
        HolidayEffect::None,               // Divad_Etept
        HolidayEffect::None,               // Suns_Rest
        HolidayEffect::None,               // Fiery_Night
        HolidayEffect::None,               // Maiden_Katrica
        HolidayEffect::None,               // Koomu_Alazeri
        HolidayEffect::None,               // Feast_of_the_Tiger
        HolidayEffect::None,               // Appreciation_Day
        HolidayEffect::None,               // Harvest_End
        HolidayEffect::HalfPriceMagesGuild, // Tales_and_Tallow
        HolidayEffect::None,               // Khurat
        HolidayEffect::None,               // Riglametha
        HolidayEffect::None,               // Childrens_Day
        HolidayEffect::None,               // Dirij_Tereur
        HolidayEffect::FreeIdentify,       // Witches_Festival
        HolidayEffect::None,               // Broken_Diamonds
        HolidayEffect::None,               // Emperors_Birthday
        HolidayEffect::None,               // Serpents_Dance
        HolidayEffect::None,               // Moon_Festival
        HolidayEffect::None,               // Hel_Anseilak
        HolidayEffect::HalfPriceWeapons,   // Warriors_Festival
        HolidayEffect::HalfCure,           // North_Winds_Festival
        HolidayEffect::None,               // Baranth_Do
        HolidayEffect::None,               // Chila
        HolidayEffect::None,               // Saturalia
    ];

    /// Name of holiday `id` (`1..=53`), `None` outside the table.
    pub fn name(id: i32) -> Option<&'static str> {
        usize::try_from(id)
            .ok()
            .and_then(|index| index.checked_sub(1))
            .and_then(|index| HOLIDAY_NAMES.get(index))
            .copied()
    }

    /// Declared effect of holiday `id` (`1..=53`), `None` outside the table.
    pub fn effect(id: i32) -> Option<HolidayEffect> {
        usize::try_from(id)
            .ok()
            .and_then(|index| index.checked_sub(1))
            .and_then(|index| HOLIDAY_EFFECTS.get(index))
            .copied()
    }

    /// Number of holidays the table declares (== the `dayOfYear` population).
    pub const COUNT: usize = HOLIDAY_NAMES.len();

    /// `(month_of_year, day_of_month)` 1-based of a holiday, from its
    /// `dayOfYear` (`:629-639`): the pair the calendar prints.
    pub fn date_of_holiday(day_of_year: u64) -> Option<(u64, u64)> {
        let (month, day) = date_of_day_of_year(day_of_year)?;
        Some((u64::from(month) + 1, u64::from(day) + 1))
    }
}

/// Seasonal value of the date (`GetSeasonValue`, `:577-607`).
pub fn season(clock: &GameClock) -> Season {
    Season::from_month(clock.month)
}

/// Birth-sign name of the date: 1:1 with the month (`:104-114`).
///
/// **Out-of-range month is normalized, not defaulted.** The source does not
/// index the table directly: `GetBirthSignName()` (`:569-575`) first runs
/// `RaiseTime(0)`, whose month carry is `Month -= (Month / 12) * 12;
/// Year += …` (`:357-368`), and only then indexes the localized list with the
/// carried month. So `Month = 12` is month 0 of the next year ⇒
/// `TheRitual`; `Month = 23` ⇒ `TheThief`. `Month` is `int` in the source, so
/// a negative value would index out of the list (an exception, not a sign);
/// [`GameClock::month`] is `u32`, so that branch is unreachable here.
pub fn birth_sign(clock: &GameClock) -> &'static str {
    BIRTH_SIGNS[(clock.month as usize) % BIRTH_SIGNS.len()]
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

    /// `GetBirthSignName` normalizes an out-of-range `Month` through
    /// `RaiseTime(0)` (`:569-575`, `:357-368`) before indexing the table: the
    /// month carries into the year, so 12 is 0 and 23 is 11. A fallback
    /// constant would return month 5 (`TheSteed`) for every invalid month.
    #[test]
    fn birth_sign_normalizes_an_out_of_range_month() {
        let mut clock = GameClock::classic_default();
        clock.month = 12;
        assert_eq!(birth_sign(&clock), "TheRitual");
        clock.month = 23;
        assert_eq!(birth_sign(&clock), "TheThief");
        clock.month = 24;
        assert_eq!(birth_sign(&clock), "TheRitual");
        clock.month = 0;
        assert_eq!(birth_sign(&clock), "TheRitual");
        clock.month = 11;
        assert_eq!(birth_sign(&clock), "TheThief");
    }

    /// The holiday table is total and index-aligned: every id in `1..=53`
    /// has a name and a declared effect, and ids `0`/`54` have neither
    /// (`DFLocation.Holidays` `None` and the unused `Old_Life_Festival`).
    #[test]
    fn holiday_table_is_total_and_index_aligned() {
        assert_eq!(holiday::COUNT, holiday::HOLIDAY_EFFECTS.len());
        for id in 1..=53 {
            assert!(holiday::name(id).is_some(), "holiday {id} has no name");
            assert!(holiday::effect(id).is_some(), "holiday {id} declares nothing");
        }
        assert_eq!(holiday::name(0), None);
        assert_eq!(holiday::name(54), None);
        assert_eq!(holiday::effect(54), None);
        assert_eq!(holiday::name(10), Some("Hearts_Day"));
        assert_eq!(holiday::effect(10), Some(holiday::HolidayEffect::FreeRoom));
        assert_eq!(
            holiday::effect(43),
            Some(holiday::HolidayEffect::FreeIdentify)
        );
        assert_eq!(holiday::effect(50), Some(holiday::HolidayEffect::HalfCure));
    }

    /// `date_of_day_of_year` inverts `day_of_year` exactly (`:629-633`).
    #[test]
    fn day_of_year_round_trips_through_its_inverse() {
        assert_eq!(date_of_day_of_year(0), None);
        assert_eq!(date_of_day_of_year(361), None);
        for day_of_year_value in 1..=DAYS_PER_YEAR {
            let (month, day) = date_of_day_of_year(day_of_year_value).expect("in range");
            let mut clock = GameClock::classic_default();
            clock.month = month;
            clock.day = day;
            assert_eq!(day_of_year(&clock), day_of_year_value);
            assert!(month < 12 && day < 30);
        }
        assert_eq!(holiday::date_of_holiday(1), Some((1, 1)));
        assert_eq!(holiday::date_of_holiday(360), Some((12, 30)));
    }
}
