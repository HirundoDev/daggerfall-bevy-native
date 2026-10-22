//! `clock` — minimal playable time system (M4).
//!
//! **Organic owner:** ❤️ Heart — PRE-3 §3 assigns "time orchestration
//! (`DaggerfallDateTime`/day-night)" to Heart (cadence); this module is pure
//! Rust with **0 dependencies** (the heart crate keeps `cargo tree` without
//! edges), so no Bevy primitive decides the domain: the M4 window only feeds
//! `raise_seconds` with the frame `dt` (AB-06 / INV-GAME-004-2).
//!
//! **Mode A parity (clean-room):** constants and semantics taken from the MIT
//! corpus at `reference/dfu-spec/Assets/Scripts/Utility/DaggerfallDateTime.cs`
//! (`MinutesPerDay=1440`, `DawnHour=6`, `DuskHour=18`, `LightsOnHour=17`,
//! `LightsOffHour=8`, classic start `405-05-00 12:00:00`, carry
//! second→minute→hour→day(30)→month(12)→year from `RaiseTime`).
//!
//! **Documented minimal playable scope (criterion (c) M4):** integer clock
//! for a full day with day/night transitions and city lights; it does NOT
//! include weather, lunar months or seasons (PRE-3 §9 = antibody map, it does
//! not define a time system; the DFU spec for seasons/moons is outside M4 and
//! documented in the note).
//!
//! The same core runs in (i) `cargo run -p daggerfall-app -- --probe
//! INTERIOR-001` (verdict block B) and (ii) `cargo test` — 0 disconnected
//! parallel test.

/// Seconds per minute (DFU `SecondsPerMinute`).
pub const SECONDS_PER_MINUTE: u64 = 60;
/// Minutes per hour (DFU `MinutesPerHour`).
pub const MINUTES_PER_HOUR: u64 = 60;
/// Hours per day (DFU `HoursPerDay`).
pub const HOURS_PER_DAY: u64 = 24;
/// Minutes per day (DFU `MinutesPerDay` = 1440).
pub const MINUTES_PER_DAY: u64 = MINUTES_PER_HOUR * HOURS_PER_DAY;
/// Days per month (DFU `DaysPerMonth`).
pub const DAYS_PER_MONTH: u64 = 30;
/// Months per year (DFU `MonthsPerYear`).
pub const MONTHS_PER_YEAR: u64 = 12;
/// Dawn hour (DFU `DawnHour`): `is_day` starts here.
pub const DAWN_HOUR: u32 = 6;
/// Dusk hour (DFU `DuskHour`): `is_night` starts here.
pub const DUSK_HOUR: u32 = 18;
/// City-lights-on hour (DFU `LightsOnHour`).
pub const LIGHTS_ON_HOUR: u32 = 17;
/// City-lights-off hour (DFU `LightsOffHour`).
pub const LIGHTS_OFF_HOUR: u32 = 8;
/// Real-time→game-time scale of the M4 window (outside the DFU spec: documented
/// M4 decision — 1 real s = 1 game min ⇒ full day = 24 real min).
pub const TIME_SCALE_GAME_SECONDS_PER_REAL_SECOND: u64 = 60;

/// Integer game clock (deterministic: 0 `f32` in the arithmetic).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GameClock {
    /// Year (DFU starts at 405).
    pub year: u32,
    /// Month 0..11 (DFU `Month`, 0-based).
    pub month: u32,
    /// Day of month 0..29 (DFU `Day`, 0-based).
    pub day: u32,
    /// Hour 0..23.
    pub hour: u32,
    /// Minute 0..59.
    pub minute: u32,
    /// Second 0..59.
    pub second: u32,
}

impl GameClock {
    /// DFU classic start: `405-05-00 12:00:00`.
    pub fn classic_default() -> Self {
        Self {
            year: 405,
            month: 5,
            day: 0,
            hour: 12,
            minute: 0,
            second: 0,
        }
    }

    /// Advances the clock by `seconds` (≥ 0) with integer carry (DFU
    /// `RaiseTime` semantics, no floating point → byte-for-byte determinism).
    pub fn raise_seconds(&mut self, seconds: u64) {
        let mut total = u64::from(self.second) + seconds;
        let carry_minutes = total / SECONDS_PER_MINUTE;
        self.second = (total % SECONDS_PER_MINUTE) as u32;

        total = u64::from(self.minute) + carry_minutes;
        let carry_hours = total / MINUTES_PER_HOUR;
        self.minute = (total % MINUTES_PER_HOUR) as u32;

        total = u64::from(self.hour) + carry_hours;
        let carry_days = total / HOURS_PER_DAY;
        self.hour = (total % HOURS_PER_DAY) as u32;

        total = u64::from(self.day) + carry_days;
        let carry_months = total / DAYS_PER_MONTH;
        self.day = (total % DAYS_PER_MONTH) as u32;

        total = u64::from(self.month) + carry_months;
        let carry_years = total / MONTHS_PER_YEAR;
        self.month = (total % MONTHS_PER_YEAR) as u32;

        self.year += carry_years as u32;
    }

    /// Minute of the day (0..1439).
    pub fn minute_of_day(&self) -> u64 {
        u64::from(self.hour) * MINUTES_PER_HOUR + u64::from(self.minute)
    }

    /// Absolute minute since the clock epoch (to compare progress).
    pub fn total_minutes(&self) -> u64 {
        let days = u64::from(self.year) * MONTHS_PER_YEAR * DAYS_PER_MONTH
            + u64::from(self.month) * DAYS_PER_MONTH
            + u64::from(self.day);
        days * MINUTES_PER_DAY + self.minute_of_day()
    }

    /// Is it day? (DFU `IsDay`: `DawnHour ≤ hour < DuskHour`).
    pub fn is_day(&self) -> bool {
        self.hour >= DAWN_HOUR && self.hour < DUSK_HOUR
    }

    /// Is it night? (DFU `IsNight`: exact complement of `IsDay`).
    pub fn is_night(&self) -> bool {
        !self.is_day()
    }

    /// Are the city lights on? (DFU `IsCityLightsOn`:
    /// `hour ≥ LightsOnHour || hour < LightsOffHour`).
    pub fn is_city_lights_on(&self) -> bool {
        self.hour >= LIGHTS_ON_HOUR || self.hour < LIGHTS_OFF_HOUR
    }

    /// Phase label for evidence/window.
    pub fn phase_label(&self) -> &'static str {
        if self.is_day() {
            "day"
        } else {
            "night"
        }
    }

    /// `HH:MM:SS` (evidence and window title).
    pub fn time_string(&self) -> String {
        format!("{:02}:{:02}:{:02}", self.hour, self.minute, self.second)
    }
}

/// One executable check of the time block.
#[derive(Debug, Clone)]
pub struct TimeCheck {
    /// Stable identifier of the check.
    pub id: &'static str,
    /// PASS/FAIL.
    pub ok: bool,
    /// Detail with numbers (evidence for the verdict).
    pub detail: String,
}

/// PROBE-INTERIOR-001 time-block report.
#[derive(Debug, Clone)]
pub struct TimeCycleReport {
    /// Checks executed, in order.
    pub checks: Vec<TimeCheck>,
    /// Global PASS = every check ok.
    pub pass: bool,
}

/// Runs the **time system** block of PROBE-INTERIOR-001: 24 h cycle
/// with day/night transitions, DFU boundary parity, exact carries and
/// determinism (same input → same state).
pub fn run_time_cycle_probe() -> TimeCycleReport {
    let mut checks: Vec<TimeCheck> = Vec::with_capacity(5);

    // 1. DFU parity: constants + classic start.
    let start = GameClock::classic_default();
    let parity_ok = MINUTES_PER_DAY == 1440
        && HOURS_PER_DAY == 24
        && DAWN_HOUR == 6
        && DUSK_HOUR == 18
        && LIGHTS_ON_HOUR == 17
        && LIGHTS_OFF_HOUR == 8
        && DAYS_PER_MONTH == 30
        && MONTHS_PER_YEAR == 12
        && start
            == (GameClock {
                year: 405,
                month: 5,
                day: 0,
                hour: 12,
                minute: 0,
                second: 0,
            });
    checks.push(TimeCheck {
        id: "time_dfu_parity",
        ok: parity_ok,
        detail: format!(
            "DaggerfallDateTime.cs: MinPerDay={} · Dawn={DAWN_HOUR} · Dusk={DUSK_HOUR} · \
             LightsOn={LIGHTS_ON_HOUR} · LightsOff={LIGHTS_OFF_HOUR} · Days/month={DAYS_PER_MONTH} · \
             Months/year={MONTHS_PER_YEAR} · start {} {}",
            MINUTES_PER_DAY,
            start.time_string(),
            start.phase_label(),
        ),
    });

    // 2. Exact carries (59 s, 60 s, 1 h, 1 day).
    let mut c = GameClock::classic_default();
    c.raise_seconds(59);
    let after_59 = (c.minute, c.second);
    c.raise_seconds(1);
    let after_60 = (c.minute, c.second);
    let mut c_hour = GameClock::classic_default();
    c_hour.raise_seconds(3600);
    let after_hour = (c_hour.hour, c_hour.minute, c_hour.second);
    let mut c_day = GameClock::classic_default();
    c_day.raise_seconds(MINUTES_PER_DAY * SECONDS_PER_MINUTE);
    let after_day = (c_day.day, c_day.hour, c_day.minute, c_day.second);
    let exact_ok = after_59 == (0, 59)
        && after_60 == (1, 0)
        && after_hour == (13, 0, 0)
        && after_day == (1, 12, 0, 0);
    checks.push(TimeCheck {
        id: "time_raise_exact",
        ok: exact_ok,
        detail: format!(
            "+59s → {m59}:{s59:02} · +60s → {m60}:{s60:02} · +3600s → {h:02}:{mi:02}:{se:02} · \
             +86400s → day {d} {h2:02}:{mi2:02} (integer carry, 0 floating point)",
            m59 = after_59.0,
            s59 = after_59.1,
            m60 = after_60.0,
            s60 = after_60.1,
            h = after_hour.0,
            mi = after_hour.1,
            se = after_hour.2,
            d = after_day.0,
            h2 = after_day.1,
            mi2 = after_day.2,
        ),
    });

    // 3. 24 h cycle with day→night and night→day transitions.
    let mut cycle = GameClock::classic_default();
    let first = cycle;
    let mut samples = Vec::new();
    let mut day_to_night = 0u32;
    let mut night_to_day = 0u32;
    let mut was_day = cycle.is_day();
    for _ in 0..24 {
        cycle.raise_seconds(MINUTES_PER_DAY * SECONDS_PER_MINUTE / 24);
        if cycle.is_day() != was_day {
            if cycle.is_night() {
                day_to_night += 1;
            } else {
                night_to_day += 1;
            }
            was_day = cycle.is_day();
        }
        if matches!(cycle.minute_of_day(), 0 | 360 | 720 | 1080) {
            samples.push(format!(
                "{}h {}",
                cycle.hour,
                cycle.phase_label()
            ));
        }
    }
    let wrapped = cycle.day == first.day + 1 && cycle.minute_of_day() == first.minute_of_day();
    let cycle_ok = wrapped && day_to_night == 1 && night_to_day == 1 && cycle.is_day();
    checks.push(TimeCheck {
        id: "time_day_night_cycle_24h",
        ok: cycle_ok,
        detail: format!(
            "24 × 1 h from {} → arrives {} (day {}, wraps at same minute_of_day={}) · \
             day→night transitions={d2n} · night→night→day={n2d} (1 and 1 required) · samples: {}",
            first.time_string(),
            cycle.time_string(),
            cycle.day,
            cycle.minute_of_day(),
            samples.join(" → "),
            d2n = day_to_night,
            n2d = night_to_day,
        ),
    });

    // 4. DFU boundaries (dawn/dusk and city lights).
    let at = |hour: u32| GameClock {
        hour,
        ..GameClock::classic_default()
    };
    let boundaries_ok = at(11).is_day()
        && at(17).is_day()
        && at(18).is_night()
        && at(5).is_night()
        && at(6).is_day()
        && at(23).is_night()
        && at(17).is_city_lights_on()
        && at(7).is_city_lights_on()
        && !at(8).is_city_lights_on()
        && !at(16).is_city_lights_on();
    checks.push(TimeCheck {
        id: "time_boundary_dfu",
        ok: boundaries_ok,
        detail: format!(
            "IsDay: 11h=D · 17h=D · 18h=N · 5h=N · 6h=D · 23h=N · \
             Lights: 17h=ON · 7h=ON · 8h=OFF · 16h=OFF (DawnHour={DAWN_HOUR}, DuskHour={DUSK_HOUR}, \
             LightsOn={LIGHTS_ON_HOUR}, LightsOff={LIGHTS_OFF_HOUR})"
        ),
    });

    // 5. Determinism: two clocks with the same sequence → same state.
    let ticks: [u64; 7] = [1, 59, 60, 61, 3600, 86400, 7];
    let mut a = GameClock::classic_default();
    let mut b = GameClock::classic_default();
    for tick in ticks {
        a.raise_seconds(tick);
        b.raise_seconds(tick);
    }
    let det_ok = a == b;
    checks.push(TimeCheck {
        id: "time_determinism",
        ok: det_ok,
        detail: format!(
            "same sequence {:?} s over 2 clocks → {:?} · identical state={det_ok}",
            ticks,
            (a.year, a.month, a.day, a.hour, a.minute, a.second),
        ),
    });

    let pass = checks.iter().all(|c| c.ok);
    TimeCycleReport { checks, pass }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn time_cycle_probe_passes() {
        let report = run_time_cycle_probe();
        for check in &report.checks {
            assert!(check.ok, "check {} FAIL: {}", check.id, check.detail);
        }
        assert!(report.pass, "time block must PASS: {report:?}");
        assert_eq!(report.checks.len(), 5, "5 time checks");
    }

    #[test]
    fn raises_carry_across_day_month_year() {
        let mut clock = GameClock::classic_default();
        // +29 exact days → day 29, same 12:00 hour (month 5 intact).
        clock.raise_seconds(29 * 1440 * 60);
        assert_eq!((clock.day, clock.hour, clock.minute), (29, 12, 0));
        assert_eq!((clock.year, clock.month), (405, 5));
        // +12 h → 00:00 and day 30 → carry to month 6, day 0.
        clock.raise_seconds(12 * 3600);
        assert_eq!((clock.month, clock.day, clock.hour), (6, 0, 0));
        // +6 months → month 12 → year 406 month 0.
        clock.raise_seconds(6 * 30 * 1440 * 60);
        assert_eq!((clock.year, clock.month, clock.day), (406, 0, 0));
    }

    #[test]
    fn classic_default_matches_dfu() {
        let clock = GameClock::classic_default();
        assert_eq!(
            (clock.year, clock.month, clock.day, clock.hour, clock.minute, clock.second),
            (405, 5, 0, 12, 0, 0)
        );
        assert!(clock.is_day());
        assert!(!clock.is_night());
        assert!(!clock.is_city_lights_on());
        assert_eq!(clock.minute_of_day(), 720);
    }

    #[test]
    fn probe_is_deterministic() {
        let a = run_time_cycle_probe();
        let b = run_time_cycle_probe();
        assert_eq!(a.pass, b.pass);
        assert_eq!(
            a.checks
                .iter()
                .map(|c| (c.id, c.ok, c.detail.as_str()))
                .collect::<Vec<_>>(),
            b.checks
                .iter()
                .map(|c| (c.id, c.ok, c.detail.as_str()))
                .collect::<Vec<_>>()
        );
    }
}
