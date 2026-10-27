//! `rest` — the classic rest/loiter window: hourly recovery, interruption and
//! rent expiry (M32, REQ-L99-065 · spec §3).
//!
//! **Source (Mode A, clean-room transcription):**
//! `DaggerfallRestWindow.cs:60-68` (constants), `:378-400` (10-minute tick and
//! hourly close), `:402-448` (enemy check → vitals → rent), `:509-527`
//! (`TickVitals` / `IsPlayerFullyHealed`),
//! `FormulaHelper.cs:113-129,178-229` (recovery rates).
//!
//! Order of one hour is **load-bearing** and replicated exactly: world time
//! advances 6 × 10 minutes → `totalHours++` → `AreEnemiesNearby` → recovery
//! (Timed/Full only) → rent countdown. An enemy on hour *n* interrupts **before**
//! that hour's recovery and leaves `totalHours = n` (golden vector R10).
//!
//! 0 Bevy, 0 persistence, 0 retail bytes. The real-time pacing constants are
//! data, not behaviour: the engine advances game hours, not wall-clock time.

/// Game minutes per tick (`minutesPerTick`, `:66`).
pub const MINUTES_PER_TICK: u32 = 10;
/// Ticks per game hour (`:395-400`).
pub const TICKS_PER_HOUR: u32 = 60 / MINUTES_PER_TICK;
/// Real seconds per rested hour (`restWaitTimePerHour`, `:67`).
pub const REST_WAIT_TIME_PER_HOUR: f64 = 0.75;
/// Real seconds per loitered hour (`loiterWaitTimePerHour`, `:68`).
pub const LOITER_WAIT_TIME_PER_HOUR: f64 = 1.25;
/// Hours above which closing the window raises `OnSleepEnd`
/// (`sleepEventMinimumHours`, `:60`; compared with `totalHours > 6`, `:287-289`).
pub const SLEEP_EVENT_MINIMUM_HOURS: u32 = 6;
/// `RestModes` branch selected by the window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RestMode {
    /// Rest for a fixed number of hours (`hoursRemaining`).
    TimedRest,
    /// Rest until health, fatigue and spell points are at their maximum.
    FullRest,
    /// Pass time without recovering vitals.
    Loiter,
}

/// `DFCareer.RapidHealingFlags`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RapidHealing {
    /// `Always`.
    pub always: bool,
    /// `InLight`.
    pub in_light: bool,
    /// `InDarkness`.
    pub in_darkness: bool,
}

/// Player vitals touched by the rest window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RestVitals {
    /// Current health.
    pub health: i32,
    /// Maximum health.
    pub max_health: i32,
    /// Current fatigue.
    pub fatigue: i32,
    /// Maximum fatigue.
    pub max_fatigue: i32,
    /// Current spell points.
    pub magicka: i32,
    /// Maximum spell points.
    pub max_magicka: i32,
    /// `Career.NoRegenSpellPoints`.
    pub no_regen_spell_points: bool,
}

impl RestVitals {
    /// Full vitals with no spell-point regeneration flag.
    pub const fn new(
        health: i32,
        max_health: i32,
        fatigue: i32,
        max_fatigue: i32,
        magicka: i32,
        max_magicka: i32,
    ) -> Self {
        Self {
            health,
            max_health,
            fatigue,
            max_fatigue,
            magicka,
            max_magicka,
            no_regen_spell_points: false,
        }
    }

    /// `IsPlayerFullyHealed` (`:524-535`).
    pub fn is_full(&self) -> bool {
        self.health == self.max_health
            && self.fatigue == self.max_fatigue
            && (self.magicka == self.max_magicka || self.no_regen_spell_points)
    }
}

/// `HealingRateModifier(endurance)` (`FormulaHelper.cs:120-129`):
/// `floor(END / 10) - 5`. DFU deliberately does **not** reproduce the classic
/// `modifier + 1` bug for negative modifiers (`:126-127`).
pub fn healing_rate_modifier(endurance: i32) -> i32 {
    endurance.div_euclid(10) - 5
}

/// `addToMedical` (`FormulaHelper.cs:189-203`) before `medical +=`:
/// `60` by default, `100` for `Always`, in `InLight` while day **and**
/// outdoors, or in `InDarkness` otherwise.
pub fn medical_bonus(rapid: RapidHealing, is_day: bool, inside: bool) -> i32 {
    if rapid.always {
        100
    } else if is_day && !inside {
        if rapid.in_light {
            100
        } else {
            60
        }
    } else if rapid.in_darkness {
        100
    } else {
        60
    }
}

/// `CalculateHealthRecoveryRate` (`FormulaHelper.cs:178-205`):
/// `max(floor(HealingRateModifier(END) + medical * MaxHealth / 1000), 1)`
/// with **integer** division on the product (C# `int * int / 1000`).
pub fn health_recovery_per_hour(endurance: i32, medical: i32, max_health: i32) -> i32 {
    let medical = i32::max(medical, 0);
    let from_medical = medical.saturating_mul(max_health.max(0)) / 1000;
    (healing_rate_modifier(endurance) + from_medical).max(1)
}

/// `CalculateFatigueRecoveryRate` (`:209-216`): `max(MaxFatigue / 8, 1)`.
pub fn fatigue_recovery_per_hour(max_fatigue: i32) -> i32 {
    i32::max(max_fatigue.max(0) / 8, 1)
}

/// `CalculateSpellPointRecoveryRate` (`:219-229`):
/// `0` when `NoRegenSpellPoints`, otherwise `max(MaxMagicka / 8, 1)`.
pub fn spell_point_recovery_per_hour(max_magicka: i32, no_regen_spell_points: bool) -> i32 {
    if no_regen_spell_points {
        0
    } else {
        i32::max(max_magicka.max(0) / 8, 1)
    }
}

/// Why the rest window ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RestEnd {
    /// `TimedRest` consumed its hours.
    TimedComplete,
    /// `Loiter` consumed its hours.
    LoiterComplete,
    /// `FullRest` reached full vitals.
    FullyHealed,
    /// An enemy broke the rest (`enemyBrokeRest`, message 354).
    Interrupted,
    /// The rented room ran out of hours.
    RentExpired,
    /// The player died while resting (`youNeverAwaken`).
    YouNeverAwaken,
    /// Engine guard: `FullRest` reached its hour cap without healing.
    CapReached,
}

impl RestEnd {
    /// GUI message id when the source has a numeric one (only 354 is
    /// measured: `enemiesNearby`, `:450-456`).
    pub fn message_id(self) -> Option<u16> {
        match self {
            RestEnd::Interrupted => Some(354),
            _ => None,
        }
    }

    /// Text key of the source message, when it has one.
    pub fn text_key(self) -> Option<&'static str> {
        match self {
            RestEnd::Interrupted => Some("enemyBrokeRest"),
            RestEnd::RentExpired => Some("expiredRentedRoom"),
            RestEnd::YouNeverAwaken => Some("youNeverAwaken"),
            _ => None,
        }
    }
}

/// Measured result of one rest window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RestOutcome {
    /// `totalHours` reached.
    pub hours: u32,
    /// Game minutes advanced (`hours × 60`, from 10-minute ticks).
    pub minutes: u32,
    /// `enemyBrokeRest`.
    pub interrupted: bool,
    /// Reason the window closed.
    pub end: RestEnd,
    /// `OnSleepEnd` (`totalHours > 6`).
    pub sleep_event: bool,
    /// `true` when vitals ended at their maximum.
    pub fully_healed: bool,
    /// `TallySkill(Medical, 1)` calls (one per recovered hour).
    pub medical_ticks: u32,
}

impl RestOutcome {
    /// Message id of the closing popup, if any.
    pub fn message_id(&self) -> Option<u16> {
        self.end.message_id()
    }
}

/// One rest window (`DaggerfallRestWindow`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RestWindow {
    /// Selected mode.
    pub mode: RestMode,
    /// Vitals being recovered.
    pub vitals: RestVitals,
    /// Live endurance (`Stats.LiveEndurance`).
    pub endurance: i32,
    /// Live `Medical` skill.
    pub medical_live: i32,
    /// `RapidHealing` career flag.
    pub rapid_healing: RapidHealing,
    /// `DaggerfallDateTime.IsDay`.
    pub is_day: bool,
    /// `PlayerEnterExit.IsPlayerInside`.
    pub inside: bool,
    /// `hoursRemaining` (`TimedRest` / `Loiter`).
    pub hours_remaining: u32,
    /// `totalHours` (`:395`).
    pub total_hours: u32,
    /// Game minutes advanced by the 10-minute ticks.
    pub total_minutes: u32,
    /// `remainingHoursRented` (`-1` in the source = no rented room).
    pub remaining_hours_rented: Option<u32>,
    /// `PlayerDeath.DeathInProgress`.
    pub death_in_progress: bool,
    /// Engine guard for `FullRest` (the source has none; it closes on death
    /// or leave-UI). One week of game hours.
    pub full_rest_cap_hours: u32,
    medical_ticks: u32,
}

impl RestWindow {
    /// A window with declared defaults: no Medical skill, no rapid healing,
    /// daytime outdoors, no rented room, no death in progress.
    pub fn new(mode: RestMode, vitals: RestVitals, hours: u32) -> Self {
        Self {
            mode,
            vitals,
            endurance: 50,
            medical_live: 0,
            rapid_healing: RapidHealing::default(),
            is_day: true,
            inside: false,
            hours_remaining: hours,
            total_hours: 0,
            total_minutes: 0,
            remaining_hours_rented: None,
            death_in_progress: false,
            full_rest_cap_hours: 24 * 7,
            medical_ticks: 0,
        }
    }

    /// Sets `END` (live endurance) used by the healing-rate modifier.
    pub fn with_endurance(mut self, endurance: i32) -> Self {
        self.endurance = endurance;
        self
    }

    /// Sets the live `Medical` skill used by the recovery formula.
    pub fn with_medical(mut self, medical: i32) -> Self {
        self.medical_live = medical;
        self
    }

    /// Sets the `RapidHealing` flag.
    pub fn with_rapid_healing(mut self, rapid: RapidHealing) -> Self {
        self.rapid_healing = rapid;
        self
    }

    /// Sets the day/inside context used by `addToMedical`.
    pub fn with_environment(mut self, is_day: bool, inside: bool) -> Self {
        self.is_day = is_day;
        self.inside = inside;
        self
    }

    /// Attaches a rented room with `hours` left (`M30` expiry input).
    pub fn with_rented_room(mut self, hours: u32) -> Self {
        self.remaining_hours_rented = Some(hours);
        self
    }

    /// Marks `DeathInProgress` before the window runs.
    pub fn with_death_in_progress(mut self) -> Self {
        self.death_in_progress = true;
        self
    }

    /// `CalculateHealthRecoveryRate` for the current vitals.
    pub fn health_per_hour(&self) -> i32 {
        let bonus = medical_bonus(self.rapid_healing, self.is_day, self.inside);
        health_recovery_per_hour(self.endurance, self.medical_live + bonus, self.vitals.max_health)
    }

    /// `CalculateFatigueRecoveryRate` for the current vitals.
    pub fn fatigue_per_hour(&self) -> i32 {
        fatigue_recovery_per_hour(self.vitals.max_fatigue)
    }

    /// `CalculateSpellPointRecoveryRate` for the current vitals.
    pub fn spell_points_per_hour(&self) -> i32 {
        spell_point_recovery_per_hour(
            self.vitals.max_magicka,
            self.vitals.no_regen_spell_points,
        )
    }

    /// Applies one hour of recovery and returns `IsPlayerFullyHealed`.
    ///
    /// Also runs `TallySkill(Medical, 1)` (`:519`).
    pub fn tick_vitals(&mut self) -> bool {
        let health = self.health_per_hour();
        let fatigue = self.fatigue_per_hour();
        let magicka = self.spell_points_per_hour();
        let vitals = &mut self.vitals;
        vitals.health = (vitals.health + health).min(vitals.max_health);
        vitals.fatigue = (vitals.fatigue + fatigue).min(vitals.max_fatigue);
        vitals.magicka = (vitals.magicka + magicka).min(vitals.max_magicka);
        self.medical_ticks += 1;
        vitals.is_full()
    }

    /// One hourly close (`TickRestHour`, `:395-448`).
    ///
    /// Returns the reason the window ended, or `None` to keep resting.
    pub fn tick_hour(&mut self, enemies_nearby: bool) -> Option<RestEnd> {
        self.total_minutes += MINUTES_PER_TICK * TICKS_PER_HOUR;

        // `:217-220`: a death already in progress closes the window.
        if self.death_in_progress {
            return Some(RestEnd::YouNeverAwaken);
        }

        self.total_hours += 1;

        // `:402-407`: the enemy check precedes the recovery of this hour.
        if enemies_nearby {
            return Some(RestEnd::Interrupted);
        }

        let mut finished = None;
        match self.mode {
            RestMode::TimedRest => {
                self.tick_vitals();
                self.hours_remaining = self.hours_remaining.saturating_sub(1);
                if self.hours_remaining < 1 {
                    finished = Some(RestEnd::TimedComplete);
                }
            }
            RestMode::FullRest => {
                if self.tick_vitals() {
                    finished = Some(RestEnd::FullyHealed);
                }
            }
            RestMode::Loiter => {
                self.hours_remaining = self.hours_remaining.saturating_sub(1);
                if self.hours_remaining < 1 {
                    finished = Some(RestEnd::LoiterComplete);
                }
            }
        }

        // `:441-448`: the rented-room countdown runs after the recovery.
        if self.check_rent() && finished.is_none() {
            finished = Some(RestEnd::RentExpired);
        }
        finished
    }

    /// `CheckRent` (`:441-448`): `true` when the room just expired.
    pub fn check_rent(&mut self) -> bool {
        match self.remaining_hours_rented {
            None => false,
            Some(0) => true,
            Some(hours) => {
                self.remaining_hours_rented = Some(hours - 1);
                hours - 1 == 0
            }
        }
    }

    /// Runs the window. `enemies_nearby(hour)` is asked once per closed hour
    /// (`AreEnemiesNearby(true)`).
    pub fn run<F: FnMut(u32) -> bool>(&mut self, mut enemies_nearby: F) -> RestOutcome {
        // `:221-225`: a FullRest window opened on full vitals closes at once.
        let mut end = if self.mode == RestMode::FullRest && self.vitals.is_full() {
            Some(RestEnd::FullyHealed)
        } else {
            None
        };

        while end.is_none() {
            let hour = self.total_hours + 1;
            let nearby = enemies_nearby(hour);
            end = self.tick_hour(nearby);
            if end.is_none() && self.mode == RestMode::FullRest && self.total_hours >= self.full_rest_cap_hours
            {
                end = Some(RestEnd::CapReached);
            }
        }

        let end = end.unwrap_or(RestEnd::CapReached);
        RestOutcome {
            hours: self.total_hours,
            minutes: self.total_minutes,
            interrupted: end == RestEnd::Interrupted,
            end,
            sleep_event: self.total_hours > SLEEP_EVENT_MINIMUM_HOURS,
            fully_healed: self.vitals.is_full(),
            medical_ticks: self.medical_ticks,
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    fn vitals(health: i32, max_health: i32) -> RestVitals {
        RestVitals::new(health, max_health, max_health, max_health, max_health, max_health)
    }

    #[test]
    fn recovery_golden_rates() {
        // R1
        assert_eq!(medical_bonus(RapidHealing::default(), true, false), 60);
        assert_eq!(health_recovery_per_hour(50, 90, 100), 9);
        // R2
        assert_eq!(health_recovery_per_hour(100, 60, 200), 17);
        // R3: the rounding floor must land on 1, not 0.
        assert_eq!(health_recovery_per_hour(10, 60, 50), 1);
        // R4
        assert_eq!(health_recovery_per_hour(30, 90, 100), 7);
        // R5
        let rapid = RapidHealing {
            always: true,
            in_light: false,
            in_darkness: false,
        };
        assert_eq!(medical_bonus(rapid, false, true), 100);
        assert_eq!(health_recovery_per_hour(50, 100, 100), 10);
        // R6 / R7
        assert_eq!(fatigue_recovery_per_hour(100), 12);
        assert_eq!(fatigue_recovery_per_hour(7), 1);
        assert_eq!(spell_point_recovery_per_hour(100, false), 12);
        assert_eq!(spell_point_recovery_per_hour(100, true), 0);
        // Rapid healing in light only applies outdoors during the day.
        let in_light = RapidHealing {
            always: false,
            in_light: true,
            in_darkness: false,
        };
        assert_eq!(medical_bonus(in_light, true, true), 60);
        assert_eq!(medical_bonus(in_light, true, false), 100);
    }

    #[test]
    fn full_rest_stops_on_the_exact_hour() {
        // R8: 1 HP, R1 rates, MaxHP 100 ⇒ 11 hours (1 + 11 × 9 = 100).
        let mut window = RestWindow::new(RestMode::FullRest, vitals(1, 100), 0)
            .with_endurance(50)
            .with_medical(30);
        let outcome = window.run(|_| false);
        assert_eq!(outcome.hours, 11);
        assert_eq!(outcome.end, RestEnd::FullyHealed);
        assert_eq!(window.vitals.health, 100);
        assert!(outcome.sleep_event);
    }

    #[test]
    fn timed_rest_counts_six_ticks_per_hour() {
        // R9
        let mut window = RestWindow::new(RestMode::TimedRest, vitals(50, 100), 8).with_endurance(50);
        let outcome = window.run(|_| false);
        assert_eq!(outcome.hours, 8);
        assert_eq!(outcome.minutes, 480);
        assert_eq!(outcome.end, RestEnd::TimedComplete);
        assert!(outcome.sleep_event);
        assert_eq!(outcome.medical_ticks, 8);
    }

    #[test]
    fn enemy_after_third_hour_interrupts_before_recovery() {
        // R10
        let mut window = RestWindow::new(RestMode::TimedRest, vitals(50, 100), 12).with_endurance(50);
        let outcome = window.run(|hour| hour >= 3);
        assert_eq!(outcome.hours, 3);
        assert!(outcome.interrupted);
        assert_eq!(outcome.message_id(), Some(354));
        assert_eq!(outcome.medical_ticks, 2, "third hour did not recover");
    }

    #[test]
    fn rent_expires_after_its_hours() {
        let mut window = RestWindow::new(RestMode::TimedRest, vitals(50, 100), 12)
            .with_endurance(50)
            .with_rented_room(24);
        let outcome = window.run(|_| false);
        assert_eq!(outcome.hours, 12);
        assert_eq!(window.remaining_hours_rented, Some(12));
        let mut short = RestWindow::new(RestMode::TimedRest, vitals(50, 100), 12)
            .with_endurance(50)
            .with_rented_room(3);
        let outcome = short.run(|_| false);
        assert_eq!(outcome.hours, 3);
        assert_eq!(outcome.end, RestEnd::RentExpired);
    }

    #[test]
    fn death_in_progress_never_awakens() {
        let mut window =
            RestWindow::new(RestMode::TimedRest, vitals(50, 100), 8).with_death_in_progress();
        let outcome = window.run(|_| false);
        assert_eq!(outcome.end, RestEnd::YouNeverAwaken);
        assert_eq!(outcome.hours, 0);
        assert_eq!(outcome.minutes, 60, "time advanced before the death check");
    }

    #[test]
    fn loiter_does_not_recover_vitals() {
        let mut window = RestWindow::new(RestMode::Loiter, vitals(10, 100), 2).with_endurance(50);
        let outcome = window.run(|_| false);
        assert_eq!(outcome.hours, 2);
        assert_eq!(outcome.end, RestEnd::LoiterComplete);
        assert_eq!(window.vitals.health, 10);
        assert_eq!(outcome.medical_ticks, 0);
    }
}
