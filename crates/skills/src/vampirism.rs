//! `vampirism` — vampire and werebeast stages, bonuses, hunger and the sun
//! (M32, REQ-L99-065 · spec §4-§5).
//!
//! **Source (Mode A, clean-room transcription):**
//! `VampirismEffect.cs:203-231,275-295,325-367`,
//! `LycanthropyEffect.cs:40-43,225-240,521-527,566-583,606-636`,
//! `PassiveSpecialsEffect.cs:31-35,149-172` (sun damage),
//! `VampirismInfection.cs:184-186` (time jump, shared with
//! [`crate::conditions::infection_time_jump_minutes`]).
//!
//! Stage one is the *infection* (a disease incumbent, §1.1/§2); stage two is
//! the permanent racial override modelled here. Bonuses, immunities and the
//! sun/hunger rules only exist at stage two — an infected host still walks in
//! daylight.
//!
//! 0 Bevy, 0 persistence, 0 retail bytes.

use crate::conditions::{LycanthropyKind, AGILITY, ATTRIBUTE_COUNT, ENDURANCE, INTELLIGENCE};
use crate::conditions::{SPEED, STRENGTH};

// ═══════════════════════════════════════════════════════════════════════════
// §4 · Vampirism
// ═══════════════════════════════════════════════════════════════════════════

/// Stat bonus applied to every attribute except INT (`VampirismEffect.cs:346-358`).
pub const VAMPIRE_STAT_BONUS: i32 = 20;
/// Skill bonus (`:359-365`).
pub const VAMPIRE_SKILL_BONUS: i32 = 30;
/// The six boosted skills (`:359-365`).
pub const VAMPIRE_SKILLS: [&str; 6] = [
    "Jumping",
    "Running",
    "Stealth",
    "CriticalStrike",
    "Climbing",
    "HandToHand",
];
/// Health lost per sun tick (`PassiveSpecialsEffect.cs:31-35`).
pub const SUN_DAMAGE_AMOUNT: i32 = 12;
/// Magic rounds (game minutes) per sun tick (`:31-35`).
pub const SUN_DAMAGE_ROUNDS: u64 = 4;
/// Minutes of satiation after feeding (`VampirismEffect.cs:284-295`):
/// `now - lastTimeFed <= MinutesPerDay`.
pub const VAMPIRE_SATIATION_MINUTES: u64 = 1440;
/// Out of 100 chance that the `$CUREVAM` quest cures vampirism
/// (`VampirismEffect.cs:225-231`).
pub const VAMPIRE_CURE_CHANCE: i32 = 30;

/// Infection stage of the vampirism disease.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VampireStage {
    /// Stage one: `VampirismInfection` (a disease, no bonuses yet).
    Infected,
    /// Stage two: the permanent racial override.
    Full,
}

/// Vampire state of one host.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VampireState {
    /// Current stage.
    pub stage: VampireStage,
    /// `VampireClans.Anthotis` — adds INT +20 (`:366-367`).
    pub anthotis: bool,
    /// Classic minute of the last feed (`lastTimeFed`).
    pub last_fed_minute: u64,
}

impl VampireState {
    /// A stage-one infection contracted at `minute`.
    pub fn infected(minute: u64) -> Self {
        Self {
            stage: VampireStage::Infected,
            anthotis: false,
            last_fed_minute: minute,
        }
    }

    /// A full vampire (`DeployFullBlownVampirism` → `CreateVampirismCurse`).
    pub fn full(minute: u64, anthotis: bool) -> Self {
        Self {
            stage: VampireStage::Full,
            anthotis,
            last_fed_minute: minute,
        }
    }

    /// `ApplyVampireAdvantages` (`:346-367`): +20 to all but INT, +20 INT for
    /// Anthotis. Zero while still stage one.
    pub fn stat_bonus(&self) -> [i32; ATTRIBUTE_COUNT] {
        if self.stage != VampireStage::Full {
            return [0; ATTRIBUTE_COUNT];
        }
        let mut bonus = [VAMPIRE_STAT_BONUS; ATTRIBUTE_COUNT];
        bonus[INTELLIGENCE] = if self.anthotis {
            VAMPIRE_STAT_BONUS
        } else {
            0
        };
        bonus
    }

    /// Skill bonus per boosted skill (`:359-365`); zero at stage one.
    pub fn skill_bonus(&self) -> i32 {
        if self.stage == VampireStage::Full {
            VAMPIRE_SKILL_BONUS
        } else {
            0
        }
    }

    /// `ImmunityFlags |= Disease | Paralysis` (`:118-123`).
    pub fn is_immune_to_disease(&self) -> bool {
        self.stage == VampireStage::Full
    }

    /// `MinMetalToHit = Silver` (`:118-123`).
    pub fn needs_silver(&self) -> bool {
        self.stage == VampireStage::Full
    }

    /// `IsSatiated` (`:290-295`): `now - lastTimeFed <= 1440`.
    pub fn is_satiated(&self, now_minute: u64) -> bool {
        now_minute.saturating_sub(self.last_fed_minute) <= VAMPIRE_SATIATION_MINUTES
    }

    /// Feeds now (`UpdateSatiation`, `:284-288`).
    pub fn feed(&mut self, now_minute: u64) {
        self.last_fed_minute = now_minute;
    }

    /// A full vampire cannot rest unless satiated (text 36, `:203-219`).
    pub fn can_rest(&self, now_minute: u64) -> bool {
        self.stage != VampireStage::Full || self.is_satiated(now_minute)
    }

    /// Fast travel is blocked during the day (`:192-200`).
    pub fn can_fast_travel(&self, is_day: bool) -> bool {
        self.stage != VampireStage::Full || !is_day
    }

    /// Sun damage over `minutes_in_sun` game minutes: 12 HP every 4 magic
    /// rounds (`PassiveSpecialsEffect.cs:149-172`). Only at stage two.
    pub fn sun_damage(&self, minutes_in_sun: u64) -> i32 {
        if self.stage != VampireStage::Full {
            return 0;
        }
        let ticks = minutes_in_sun / SUN_DAMAGE_ROUNDS;
        i32::try_from(ticks).unwrap_or(i32::MAX) * SUN_DAMAGE_AMOUNT
    }
}

/// `$CUREVAM` roll (`VampirismEffect.cs:225-231`): `roll < 30` out of 100.
pub fn vampire_cure_succeeds(roll: i32) -> bool {
    roll < VAMPIRE_CURE_CHANCE
}

// ═══════════════════════════════════════════════════════════════════════════
// §5 · Lycanthropy
// ═══════════════════════════════════════════════════════════════════════════

/// Stat bonus applied to STR, AGI, END and SPD (`LycanthropyEffect.cs:566-574`).
pub const LYCAN_STAT_BONUS: i32 = 40;
/// Skill bonus (`:576-583`).
pub const LYCAN_SKILL_BONUS: i32 = 30;
/// The seven boosted skills (`:576-583`).
pub const LYCAN_SKILLS: [&str; 7] = [
    "Swimming",
    "Running",
    "Stealth",
    "CriticalStrike",
    "Climbing",
    "HandToHand",
    "Jumping",
];
/// `needToKillPeriod = MinutesPerDay × DaysPerMonth` (`:40-43`).
pub const NEED_TO_KILL_MINUTES: u64 = 1440 * 30;
/// `needToKillHealthLimitMinimum` (`:40`).
pub const HEALTH_LIMIT_MINIMUM: i32 = 4;
/// `lastCastMorphSelf` cooldown (`:633-636`): one day.
pub const MORPH_COOLDOWN_MINUTES: u64 = 1440;
/// `$CUREWER` chance out of 100 (`:440-451`).
pub const LYCAN_CURE_CHANCE: i32 = 30;

/// Stage of the lycanthropy disease.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LycanthropyStage {
    /// Stage one: `LycanthropyInfection` (a disease).
    Infected,
    /// Stage two: the permanent racial override.
    Full,
}

/// Lycanthrope state of one host.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LycanthropyState {
    /// Werewolf or wereboar.
    pub kind: LycanthropyKind,
    /// Current stage.
    pub stage: LycanthropyStage,
    /// `isTransformed`.
    pub transformed: bool,
    /// Classic minute of the last `MorphSelf` (`lastCastMorphSelf`).
    pub last_morph_minute: u64,
    /// Classic minute of the last innocent killed (`lastKillMinute`).
    pub last_kill_minute: u64,
    /// `wearingHircineRing`.
    pub hircine_ring: bool,
}

impl LycanthropyState {
    /// A stage-one infection contracted at `minute`.
    pub fn infected(kind: LycanthropyKind, minute: u64) -> Self {
        Self {
            kind,
            stage: LycanthropyStage::Infected,
            transformed: false,
            last_morph_minute: minute,
            last_kill_minute: minute,
            hircine_ring: false,
        }
    }

    /// A full lycanthrope (`CreateLycanthropyCurse`).
    pub fn full(kind: LycanthropyKind, minute: u64) -> Self {
        Self {
            stage: LycanthropyStage::Full,
            ..Self::infected(kind, minute)
        }
    }

    /// `ApplyLycanthropeAdvantages` (`:566-574`): +40 STR, AGI, END, SPD.
    pub fn stat_bonus(&self) -> [i32; ATTRIBUTE_COUNT] {
        if self.stage != LycanthropyStage::Full {
            return [0; ATTRIBUTE_COUNT];
        }
        let mut bonus = [0; ATTRIBUTE_COUNT];
        bonus[STRENGTH] = LYCAN_STAT_BONUS;
        bonus[AGILITY] = LYCAN_STAT_BONUS;
        bonus[ENDURANCE] = LYCAN_STAT_BONUS;
        bonus[SPEED] = LYCAN_STAT_BONUS;
        bonus
    }

    /// Skill bonus per boosted skill (`:576-583`); zero at stage one.
    pub fn skill_bonus(&self) -> i32 {
        if self.stage == LycanthropyStage::Full {
            LYCAN_SKILL_BONUS
        } else {
            0
        }
    }

    /// Disease immunity (`:190-196`).
    pub fn is_immune_to_disease(&self) -> bool {
        self.stage == LycanthropyStage::Full
    }

    /// `Silver` while transformed, `Iron` otherwise (`:190-196`).
    pub fn needs_silver(&self) -> bool {
        self.stage == LycanthropyStage::Full && self.transformed
    }

    /// Minutes elapsed since the last innocent kill.
    pub fn minutes_since_kill(&self, now_minute: u64) -> u64 {
        now_minute.saturating_sub(self.last_kill_minute)
    }

    /// `GetNeedToKill` (`:625-627`): `> needToKillPeriod`, no Hircine ring.
    pub fn needs_to_kill(&self, now_minute: u64) -> bool {
        !self.hircine_ring && self.minutes_since_kill(now_minute) > NEED_TO_KILL_MINUTES
    }

    /// Health reduction once the urge is overdue (`:232-240`):
    /// `round(lateMinutes × 24 / 1440)`.
    ///
    /// The source uses `Mathf.RoundToInt` on a float; this integer form rounds
    /// half up. Every measured vector (Y2/Y3/Y4) is an exact minute multiple,
    /// where both agree.
    pub fn max_health_reduction(late_minutes: u64) -> i32 {
        let scaled = late_minutes.saturating_mul(24);
        i32::try_from((scaled + 720) / 1440).unwrap_or(i32::MAX)
    }

    /// `SetMaxHealthLimiter(RawMaxHealth - reduction)` with the 4 HP floor
    /// (`:234-237`).
    pub fn limited_max_health(raw_max_health: i32, late_minutes: u64) -> i32 {
        (raw_max_health - Self::max_health_reduction(late_minutes)).max(HEALTH_LIMIT_MINIMUM)
    }

    /// `CanCastMorphSelf` (`:631-636`): ring always allows, otherwise
    /// `now - lastCast > 1440`.
    pub fn can_morph(&self, now_minute: u64) -> bool {
        self.hircine_ring
            || now_minute.saturating_sub(self.last_morph_minute) > MORPH_COOLDOWN_MINUTES
    }

    /// Full moon forces a morph every magic round unless the ring is worn
    /// (`ForceTransformDuringFullMoon`, `:606-621`).
    pub fn full_moon_forces_morph(&self, is_full_moon: bool) -> bool {
        is_full_moon && !self.hircine_ring && !self.transformed
    }

    /// `MorphSelf` (`:521-527`): heals to maximum on transform (classic
    /// behaviour, replicated on purpose — spec §11 risk 6). Returns the
    /// health the transform restores: the host's maximum.
    pub fn morph(&mut self, now_minute: u64, max_health: i32) -> i32 {
        self.transformed = true;
        self.last_morph_minute = now_minute;
        max_health
    }

    /// Returns to the humanoid form (classic lets the player shift back for
    /// the remainder of the magic round).
    pub fn revert(&mut self) {
        self.transformed = false;
    }
}

/// `$CUREWER` roll (`:440-451`): `roll < 30` out of 100.
pub fn lycanthropy_cure_succeeds(roll: i32) -> bool {
    roll < LYCAN_CURE_CHANCE
}

// ═══════════════════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vampire_bonuses_and_anthotis() {
        let mut stats = [50; ATTRIBUTE_COUNT];
        let vampire = VampireState::full(0, false);
        for (value, bonus) in stats.iter_mut().zip(vampire.stat_bonus()) {
            *value += bonus;
        }
        assert_eq!(stats[STRENGTH], 70);
        assert_eq!(stats[INTELLIGENCE], 50);
        assert_eq!(vampire.skill_bonus(), VAMPIRE_SKILL_BONUS);
        assert_eq!(VAMPIRE_SKILLS.len(), 6);

        let anthotis = VampireState::full(0, true);
        assert_eq!(anthotis.stat_bonus()[INTELLIGENCE], 20);
        // Stage one grants nothing.
        let infected = VampireState::infected(0);
        assert_eq!(infected.stat_bonus(), [0; ATTRIBUTE_COUNT]);
        assert_eq!(infected.skill_bonus(), 0);
        assert!(!infected.is_immune_to_disease());
    }

    #[test]
    fn satiation_uses_less_or_equal() {
        let mut vampire = VampireState::full(1000, false);
        assert!(vampire.is_satiated(2440));
        assert!(!vampire.is_satiated(2441));
        assert!(!vampire.can_rest(2441));
        vampire.feed(2441);
        assert!(vampire.can_rest(2441));
        assert!(!vampire.can_fast_travel(true));
        assert!(vampire.can_fast_travel(false));
    }

    #[test]
    fn sun_damage_is_twelve_every_four_minutes() {
        let vampire = VampireState::full(0, false);
        assert_eq!(vampire.sun_damage(20), 60);
        assert_eq!(vampire.sun_damage(3), 0);
        assert_eq!(vampire.sun_damage(4), 12);
        assert_eq!(VampireState::infected(0).sun_damage(20), 0);
    }

    #[test]
    fn cure_chances_are_thirty_percent() {
        assert!(vampire_cure_succeeds(29));
        assert!(!vampire_cure_succeeds(30));
        assert!(lycanthropy_cure_succeeds(29));
        assert!(!lycanthropy_cure_succeeds(30));
    }

    #[test]
    fn lycanthrope_bonuses_and_health_limit() {
        let lycan = LycanthropyState::full(LycanthropyKind::Werewolf, 0);
        let bonus = lycan.stat_bonus();
        assert_eq!(bonus[STRENGTH], 40);
        assert_eq!(bonus[AGILITY], 40);
        assert_eq!(bonus[ENDURANCE], 40);
        assert_eq!(bonus[SPEED], 40);
        assert_eq!(bonus[INTELLIGENCE], 0);
        assert_eq!(lycan.skill_bonus(), 30);
        assert_eq!(LYCAN_SKILLS.len(), 7);

        // Y2 / Y3 / Y4
        assert_eq!(LycanthropyState::max_health_reduction(14400), 240);
        assert_eq!(LycanthropyState::max_health_reduction(1440), 24);
        assert_eq!(LycanthropyState::limited_max_health(100, 14400), 4);
        assert_eq!(LycanthropyState::limited_max_health(1000, 14400), 760);

        assert!(lycan.is_immune_to_disease());
        assert!(!lycan.needs_silver());
        let mut transformed = lycan;
        transformed.morph(0, 100);
        assert!(transformed.transformed);
        assert!(transformed.needs_silver());
    }

    #[test]
    fn urge_and_cooldown_rules() {
        let mut lycan = LycanthropyState::full(LycanthropyKind::Wereboar, 0);
        assert!(!lycan.needs_to_kill(NEED_TO_KILL_MINUTES));
        assert!(lycan.needs_to_kill(NEED_TO_KILL_MINUTES + 1));
        lycan.hircine_ring = true;
        assert!(!lycan.needs_to_kill(NEED_TO_KILL_MINUTES + 1));

        lycan.hircine_ring = false;
        assert!(lycan.can_morph(MORPH_COOLDOWN_MINUTES + 1));
        assert!(!lycan.can_morph(MORPH_COOLDOWN_MINUTES));
        assert!(lycan.full_moon_forces_morph(true));
        lycan.transformed = true;
        assert!(!lycan.full_moon_forces_morph(true));
        lycan.transformed = false;
        lycan.hircine_ring = true;
        assert!(!lycan.full_moon_forces_morph(true));
    }
}
