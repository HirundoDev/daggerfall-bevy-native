//! `progression` — gameplay signals → skill uses → advancement → level-up
//! (M20; the piece audit P1-3 declares missing).
//!
//! Faithful to the MIT source, signal by signal:
//! - **Running**: 1 use every 4 classic updates while running
//!   (`runningTallyCounter == 3 ⇒ TallySkill`, `PlayerEntity.cs:311-321`);
//! - **Swimming**: 1 use per game minute while swimming, inside the
//!   `lastGameMinutes != gameMinutes` block (`PlayerEntity.cs:408-417`);
//! - **Jumping**: 1 use on the jump edge (`!CheckedCurrentJump && IsJumping`),
//!   latched until the body leaves the jump (`PlayerEntity.cs:425-438`);
//! - **Climbing**: 1 use per climbing skill check, failures included
//!   (`ClimbingMotor.ClimbingSkillCheck`, `ClimbingMotor.cs:823-828`);
//! - **Dodging**: 1 use per resolved enemy attack, hit or miss
//!   (`EnemyAttack.cs:257-260`);
//! - **Weapon / hand-to-hand**: 1 use of the weapon skill (`HandToHand` when
//!   melee/unarmed) **only on hit**, plus 1 of `CriticalStrike`
//!   (`WeaponManager.cs:419-432`).
//!
//! Every tally goes through [`Character::tally_skill`] (the single clamp
//! `[0, 20_000]`) and every advancement through [`Character::raise_skills`]
//! (the single 100 cap + mastery rule), so the driver adds **signals**, never
//! a second progression rule.
//!
//! **Declared deltas (spec M20 §7):**
//! - **D-M20-1** — the driver owns its SplitMix64 stream ([`Rng64`]) with an
//!   explicit seed: the level-up roll is reproducible ×2 instead of following
//!   `UnityEngine.Random`.
//! - **D-M20-4** — DFU evaluates `RaiseSkills()` on rest and travel
//!   (`DaggerfallRestWindow.cs:731`, `DaggerfallTravelPopUp.cs:380`); rest and
//!   travel are M32/M31, so the driver evaluates on every advanced game minute.
//!   The result is identical because the 360-minute gate lives **inside**
//!   `RaiseSkills` (`PlayerEntity.cs:1366-1370`): a closed gate returns
//!   `checked = false` and touches no state, so over-evaluating is idempotent.
//! - **D-M20-5** — DFU applies the level-up when the character sheet is opened
//!   (`DaggerfallCharacterSheetWindow.cs:369-391`); the sheet is M34, so the
//!   driver applies it at the tick that makes the character ready and reports
//!   it. Point distribution stays with M34.

use crate::character::{Character, LevelUpReport, RaiseReport, Skill};
use crate::rng::Rng64;

/// Duration of one engine classic update in seconds (`PlayerEntity.FixedUpdate`
/// at the classic 15.5 Hz step; the same step used by
/// `daggerfall_behavior::locomotion::breath_drain_interval_s`).
pub const CLASSIC_UPDATE_SECONDS: f32 = 0.0625;

/// Classic updates between two Running uses: DFU tallies when the counter
/// reaches 3, i.e. on the 4th update (`PlayerEntity.cs:311-321`).
pub const RUNNING_UPDATES_PER_USE: u8 = 4;

/// Movement signals of one M19 frame.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MotionSignals {
    /// `playerMotor.IsRunning` (grounded, running, not riding).
    pub running: bool,
    /// `MotionMode::Swimming` this frame.
    pub swimming: bool,
    /// Airborne because of a jump (`MotionMode::Airborne`, `PlayerEntity.cs:429`).
    pub jumping: bool,
    /// A climbing die was consumed this frame
    /// (`Frame::climb_check`, `ClimbingMotor.cs:823`).
    pub climbing_check: bool,
}

/// Observable effect of one driver step.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DriverStep {
    /// Uses accumulated in this step, aggregated per skill (p. e. 250 of
    /// Running for 1000 classic updates).
    pub tallies: Vec<(Skill, i32)>,
    /// Skills advanced by `RaiseSkills` (skill, new permanent value).
    pub raised: Vec<(Skill, i16)>,
    /// The character was ready to level up at evaluation time. With D-M20-5
    /// the level-up is applied in the same step, so this stays `true` even
    /// though `Character::ready_to_level_up` is already back to `false`.
    pub ready_to_level_up: bool,
    /// Level-up applied in this step (D-M20-5).
    pub level_up: Option<LevelUpReport>,
    /// The 360-minute advancement gate opened (`RaiseReport::checked`).
    pub checked: bool,
}

/// Converts gameplay signals into skill uses and applies advancement.
///
/// Pure state: an [`Rng64`] stream, the Running counter, the jump latch and
/// the sub-update time carry. No wall clock, no I/O.
#[derive(Debug, Clone, PartialEq)]
pub struct SkillDriver {
    rng: Rng64,
    running_counter: u8,
    jump_latched: bool,
    update_carry: f32,
}

impl SkillDriver {
    /// Creates a driver with an explicit seed for its own RNG stream (D-M20-1).
    pub fn new(seed: u64) -> Self {
        Self {
            rng: Rng64::new(seed),
            running_counter: 0,
            jump_latched: false,
            update_carry: 0.0,
        }
    }

    /// Own RNG stream (level-up rolls only).
    pub fn rng(&self) -> &Rng64 {
        &self.rng
    }

    /// Accumulates whole classic updates for the elapsed `seconds` and tallies
    /// Running (every 4th update), Jumping (per jump edge) and Climbing (per
    /// climbing check).
    ///
    /// Frame-rate independent: `seconds` is accumulated in a carry and only
    /// whole `CLASSIC_UPDATE_SECONDS` are processed; the remainder waits for
    /// the next frame, so two frames of 0.03125 s tally exactly like one frame
    /// of 0.0625 s. The jump edge and the climbing flag are frame-level
    /// signals (`PlayerEntity.Update`, not `ClassicUpdate`) and are processed
    /// once per call.
    pub fn advance(
        &mut self,
        character: &mut Character,
        seconds: f32,
        signals: &MotionSignals,
    ) -> DriverStep {
        let mut step = DriverStep::default();
        if seconds > 0.0 && seconds.is_finite() {
            self.update_carry += seconds;
        }
        let updates = (self.update_carry / CLASSIC_UPDATE_SECONDS).floor();
        if updates >= 1.0 {
            let whole = updates as u32;
            self.update_carry = (self.update_carry - updates * CLASSIC_UPDATE_SECONDS).max(0.0);
            self.tally_running(character, &mut step, whole, signals.running);
        }
        // `PlayerEntity.cs:425-438`: tally on the edge, reset when the jump ends.
        if signals.jumping {
            if !self.jump_latched {
                tally(character, &mut step, Skill::Jumping, 1);
                self.jump_latched = true;
            }
        } else {
            self.jump_latched = false;
        }
        // `ClimbingMotor.cs:823-828`: the check tallies even when it fails.
        if signals.climbing_check {
            tally(character, &mut step, Skill::Climbing, 1);
        }
        step
    }

    /// `runningTallyCounter` (`PlayerEntity.cs:311-321`): the counter only
    /// advances while running and is **not** reset when the run stops — the
    /// source leaves it untouched too.
    fn tally_running(
        &mut self,
        character: &mut Character,
        step: &mut DriverStep,
        updates: u32,
        running: bool,
    ) {
        if !running {
            return;
        }
        let mut uses = 0i32;
        for _ in 0..updates {
            if self.running_counter == RUNNING_UPDATES_PER_USE - 1 {
                uses += 1;
                self.running_counter = 0;
            } else {
                self.running_counter += 1;
            }
        }
        tally(character, step, Skill::Running, uses);
    }

    /// One game minute: Swimming +1 when swimming (`PlayerEntity.cs:408-417`,
    /// tallied **before** the gate, as the source does), then `RaiseSkills` at
    /// `now_minutes` and, when the character is ready, the level-up (D-M20-5).
    pub fn minute(
        &mut self,
        character: &mut Character,
        now_minutes: i64,
        signals: &MotionSignals,
    ) -> DriverStep {
        let mut step = DriverStep::default();
        if signals.swimming {
            tally(character, &mut step, Skill::Swimming, 1);
        }
        let report: RaiseReport = character.raise_skills(now_minutes);
        step.checked = report.checked;
        step.raised = report.raised;
        step.ready_to_level_up = report.ready_to_level_up;
        if report.ready_to_level_up {
            step.level_up = character.level_up(&mut self.rng);
        }
        step
    }

    /// Player attack resolved: weapon skill (or `HandToHand` when unarmed)
    /// **only on hit**, plus `CriticalStrike` (`WeaponManager.cs:419-432`).
    pub fn player_strike(
        &mut self,
        character: &mut Character,
        weapon_skill: Option<Skill>,
        hit: bool,
    ) -> DriverStep {
        let mut step = DriverStep::default();
        if !hit {
            return step;
        }
        let skill = weapon_skill.unwrap_or(Skill::HandToHand);
        tally(character, &mut step, skill, 1);
        tally(character, &mut step, Skill::CriticalStrike, 1);
        step
    }

    /// Enemy attack resolved against the player: Dodging +1 always, damage or
    /// not (`EnemyAttack.cs:257-260`).
    pub fn enemy_strike(&mut self, character: &mut Character) -> DriverStep {
        let mut step = DriverStep::default();
        tally(character, &mut step, Skill::Dodging, 1);
        step
    }

    /// Explicit use (training, locks, stealth…): goes through
    /// [`Character::tally_skill`] like every other path.
    pub fn use_skill(
        &mut self,
        character: &mut Character,
        skill: Skill,
        amount: i32,
    ) -> DriverStep {
        let mut step = DriverStep::default();
        tally(character, &mut step, skill, amount);
        step
    }
}

/// Single tally point: clamps through [`Character::tally_skill`] and aggregates
/// in the step **what the character actually accepted** — `after - before`, not
/// the requested amount (first-occurrence order, so the sequence is
/// deterministic).
///
/// At the `MAX_SKILL_USES` clamp (20 000) the applied amount is 0, so the skill is
/// **omitted**: the step reports effects, and a raise-shaped row for a tally
/// the clamp swallowed would make `DriverStep::tallies` (the `observable` of
/// `m20_report_step`) untruthful.
fn tally(character: &mut Character, step: &mut DriverStep, skill: Skill, amount: i32) {
    if amount == 0 {
        return;
    }
    let before = character.uses[skill.index()];
    character.tally_skill(skill, amount);
    let applied = character.uses[skill.index()].saturating_sub(before);
    if applied == 0 {
        return;
    }
    match step.tallies.iter_mut().find(|(entry, _)| *entry == skill) {
        Some((_, total)) => *total += applied,
        None => step.tallies.push((skill, applied)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::career::Career;
    use crate::character::MAX_SKILL_USES;

    /// 74-byte synthetic career: all attributes 50, HPL 10, advancement ×1.00,
    /// primary `[Mysticism, Alteration, Thaumaturgy]`.
    fn test_career() -> Career {
        let mut bytes = vec![0u8; crate::career::CFG_BYTES];
        bytes[4..6].copy_from_slice(&0x0800u16.to_le_bytes()); // SP× 1.0
        bytes[16..19].copy_from_slice(&[27, 25, 26]);
        bytes[19..22].copy_from_slice(&[24, 22, 23]);
        bytes[22..28].copy_from_slice(&[0, 28, 32, 7, 9, 20]);
        bytes[28..32].copy_from_slice(b"Test");
        bytes[52..54].copy_from_slice(&10u16.to_le_bytes());
        bytes[54..58].copy_from_slice(&0x0001_0000u32.to_le_bytes());
        let attributes: [u16; 8] = [50; 8];
        for (index, value) in attributes.iter().enumerate() {
            bytes[58 + index * 2..60 + index * 2].copy_from_slice(&value.to_le_bytes());
        }
        Career::parse(&bytes).expect("synthetic career")
    }

    fn fresh_character() -> Character {
        let mut rng = Rng64::new(20_261_020);
        Character::starting(test_career(), &mut rng, 1)
    }

    fn uses(character: &Character, skill: Skill) -> i32 {
        character.uses[skill.index()]
    }

    fn running_signals() -> MotionSignals {
        MotionSignals {
            running: true,
            ..MotionSignals::default()
        }
    }

    #[test]
    fn running_tallies_one_use_per_four_classic_updates() {
        let mut driver = SkillDriver::new(1);
        let mut character = fresh_character();
        driver.advance(&mut character, 0.25, &running_signals());
        assert_eq!(uses(&character, Skill::Running), 1, "4 × 0.0625 s");

        let mut driver = SkillDriver::new(1);
        let mut character = fresh_character();
        driver.advance(&mut character, 0.1875, &running_signals());
        assert_eq!(uses(&character, Skill::Running), 0, "3 × 0.0625 s");

        let mut driver = SkillDriver::new(1);
        let mut character = fresh_character();
        let step = driver.advance(&mut character, 62.5, &running_signals());
        assert_eq!(uses(&character, Skill::Running), 250, "1000 / 4");
        assert_eq!(step.tallies, vec![(Skill::Running, 250)], "aggregated per skill");

        // Not running: the counter does not move.
        let mut driver = SkillDriver::new(1);
        let mut character = fresh_character();
        driver.advance(&mut character, 62.5, &MotionSignals::default());
        assert_eq!(uses(&character, Skill::Running), 0);
    }

    #[test]
    fn running_is_frame_rate_independent() {
        // 8 × 0.03125 s (two halves per classic update) ≡ 4 × 0.0625 s: the
        // same 4 updates, hence the same single Running use.
        let mut split = SkillDriver::new(7);
        let mut split_character = fresh_character();
        for _ in 0..8 {
            split.advance(&mut split_character, 0.03125, &running_signals());
        }

        let mut whole = SkillDriver::new(7);
        let mut whole_character = fresh_character();
        for _ in 0..4 {
            whole.advance(&mut whole_character, 0.0625, &running_signals());
        }

        assert_eq!(uses(&split_character, Skill::Running), 1);
        assert_eq!(
            uses(&split_character, Skill::Running),
            uses(&whole_character, Skill::Running)
        );
        // A single 0.03125 s frame does not complete an update on its own.
        let mut half = SkillDriver::new(7);
        let mut half_character = fresh_character();
        half.advance(&mut half_character, 0.03125, &running_signals());
        assert_eq!(half_character.uses, fresh_character().uses, "carry held");

        // 1000 one-update frames ≡ one 62.5 s frame.
        let mut tiny = SkillDriver::new(7);
        let mut tiny_character = fresh_character();
        for _ in 0..1000 {
            tiny.advance(&mut tiny_character, 0.0625, &running_signals());
        }
        assert_eq!(uses(&tiny_character, Skill::Running), 250);
    }

    #[test]
    fn jumping_tallies_once_per_edge() {
        let mut driver = SkillDriver::new(1);
        let mut character = fresh_character();
        let airborne = MotionSignals {
            jumping: true,
            ..MotionSignals::default()
        };
        driver.advance(&mut character, 0.0625, &airborne);
        assert_eq!(uses(&character, Skill::Jumping), 1);
        for _ in 0..10 {
            let step = driver.advance(&mut character, 0.0625, &airborne);
            assert!(step.tallies.is_empty(), "no re-tally while latched");
        }
        assert_eq!(uses(&character, Skill::Jumping), 1);
        driver.advance(&mut character, 0.0625, &MotionSignals::default());
        driver.advance(&mut character, 0.0625, &airborne);
        assert_eq!(uses(&character, Skill::Jumping), 2, "second jump");
    }

    #[test]
    fn climbing_tallies_once_per_check_and_not_without_the_flag() {
        let mut driver = SkillDriver::new(1);
        let mut character = fresh_character();
        let check = MotionSignals {
            climbing_check: true,
            ..MotionSignals::default()
        };
        for _ in 0..3 {
            driver.advance(&mut character, 0.0625, &check);
        }
        assert_eq!(uses(&character, Skill::Climbing), 3);
        for _ in 0..5 {
            driver.advance(&mut character, 0.0625, &MotionSignals::default());
        }
        assert_eq!(uses(&character, Skill::Climbing), 3);
    }

    #[test]
    fn swimming_tallies_per_minute_even_with_the_gate_closed() {
        let mut driver = SkillDriver::new(1);
        let mut character = fresh_character();
        let swimming = MotionSignals {
            swimming: true,
            ..MotionSignals::default()
        };
        let step = driver.minute(&mut character, 0, &swimming);
        assert!(!step.checked, "gate closed at minute 0");
        assert_eq!(uses(&character, Skill::Swimming), 1);
        assert_eq!(step.tallies, vec![(Skill::Swimming, 1)]);
        let step = driver.minute(&mut character, 120, &MotionSignals::default());
        assert_eq!(uses(&character, Skill::Swimming), 1, "not swimming");
        assert!(step.tallies.is_empty());
        driver.minute(&mut character, 240, &swimming);
        assert_eq!(uses(&character, Skill::Swimming), 2);
    }

    #[test]
    fn dodging_tallies_per_enemy_attack_regardless_of_damage() {
        let mut driver = SkillDriver::new(1);
        let mut character = fresh_character();
        for _ in 0..5 {
            driver.enemy_strike(&mut character);
        }
        assert_eq!(uses(&character, Skill::Dodging), 5);
    }

    #[test]
    fn player_strike_tallies_only_on_hit() {
        let mut driver = SkillDriver::new(1);
        let mut character = fresh_character();
        driver.player_strike(&mut character, Some(Skill::LongBlade), false);
        assert_eq!(uses(&character, Skill::LongBlade), 0);
        assert_eq!(uses(&character, Skill::CriticalStrike), 0);

        let step = driver.player_strike(&mut character, Some(Skill::LongBlade), true);
        assert_eq!(uses(&character, Skill::LongBlade), 1);
        assert_eq!(uses(&character, Skill::CriticalStrike), 1);
        assert_eq!(
            step.tallies,
            vec![(Skill::LongBlade, 1), (Skill::CriticalStrike, 1)]
        );

        // Unarmed ⇒ HandToHand (`WeaponManager.cs:424-425`).
        driver.player_strike(&mut character, None, true);
        assert_eq!(uses(&character, Skill::HandToHand), 1);
        assert_eq!(uses(&character, Skill::CriticalStrike), 2);
    }

    #[test]
    fn uses_clamp_at_cap_and_skills_stop_at_one_hundred() {
        let mut driver = SkillDriver::new(1);
        let mut character = fresh_character();
        driver.use_skill(&mut character, Skill::Running, 25_000);
        assert_eq!(uses(&character, Skill::Running), 20_000);

        character.skills.set_permanent(Skill::Running, 100);
        let step = driver.minute(&mut character, 361, &MotionSignals::default());
        assert!(step.checked);
        assert!(step.raised.is_empty(), "100 cannot advance");
        assert_eq!(character.permanent_skill(Skill::Running), 100);
        assert_eq!(uses(&character, Skill::Running), 0, "counter always resets");
    }

    #[test]
    fn mastery_blocks_the_advance_but_resets_uses() {
        let mut driver = SkillDriver::new(1);
        let mut character = fresh_character();
        let primary = character.career.primary[0];
        character.skills.set_permanent(primary, 100);
        character.skills.set_permanent(Skill::Running, 95);
        driver.use_skill(&mut character, Skill::Running, 10_000);
        let step = driver.minute(&mut character, 361, &MotionSignals::default());
        assert!(step.checked);
        assert_eq!(character.permanent_skill(Skill::Running), 95);
        assert_eq!(uses(&character, Skill::Running), 0);
    }

    #[test]
    fn tallies_report_the_applied_amount_not_the_requested_one() {
        // At the `MAX_SKILL_USES` clamp the character accepts nothing, so the
        // step must not report a raise-shaped row: `DriverStep::tallies` is
        // the observable the demo prints for the step.
        let mut driver = SkillDriver::new(1);
        let mut character = fresh_character();
        driver.use_skill(&mut character, Skill::Running, MAX_SKILL_USES);
        let clamped = driver.use_skill(&mut character, Skill::Running, 5);
        assert_eq!(uses(&character, Skill::Running), MAX_SKILL_USES);
        assert!(clamped.tallies.is_empty(), "nothing applied: {:?}", clamped.tallies);

        // A normal tally still reports exactly the amount applied.
        let mut character = fresh_character();
        let normal = driver.use_skill(&mut character, Skill::Running, 5);
        assert_eq!(normal.tallies, vec![(Skill::Running, 5)]);

        // A partial clamp reports only the accepted part.
        let mut character = fresh_character();
        driver.use_skill(&mut character, Skill::Running, MAX_SKILL_USES - 2);
        let partial = driver.use_skill(&mut character, Skill::Running, 5);
        assert_eq!(uses(&character, Skill::Running), MAX_SKILL_USES);
        assert_eq!(partial.tallies, vec![(Skill::Running, 2)]);

        // The `advance` path aggregates through the same helper.
        let mut character = fresh_character();
        driver.use_skill(&mut character, Skill::Running, MAX_SKILL_USES);
        let advanced = driver.advance(&mut character, 62.5, &running_signals());
        assert!(
            advanced.tallies.is_empty(),
            "clamped Running reports nothing: {:?}",
            advanced.tallies
        );
    }

    #[test]
    fn minute_applies_the_level_up() {
        let mut driver = SkillDriver::new(20_261_020);
        let mut character = fresh_character();
        assert_eq!(character.level, 1);
        let health_before = character.max_health;
        // Three primary skills advance +1 each ⇒ level sum +3 ⇒ level 2
        // (`(3 + 28) / 15`, `FormulaHelper.cs:330-337`).
        for skill in character.career.primary {
            driver.use_skill(&mut character, skill, MAX_SKILL_USES);
        }
        let step = driver.minute(&mut character, 361, &MotionSignals::default());
        assert!(step.checked);
        assert!(step.ready_to_level_up);
        let report = step.level_up.expect("level-up applied (D-M20-5)");
        assert_eq!(report.level, 2);
        assert_eq!(character.level, 2);
        assert_eq!(character.max_health, health_before + report.hit_points);
        assert!(
            (4..=6).contains(&report.bonus_pool),
            "pool range 4..=6, got {}",
            report.bonus_pool
        );
        assert!(
            (4..=6).contains(&character.pending_attribute_points),
            "pending {}",
            character.pending_attribute_points
        );
        assert!(!character.ready_to_level_up, "consumed by the driver");
        // A second evaluation at the same readiness does nothing.
        let again = driver.minute(&mut character, 722, &MotionSignals::default());
        assert!(!again.ready_to_level_up);
        assert!(again.level_up.is_none());
        assert_eq!(character.level, 2);
    }

    #[test]
    fn two_drivers_with_the_same_seed_replay_identically() {
        let script = |driver: &mut SkillDriver, character: &mut Character| -> Vec<DriverStep> {
            let mut steps = Vec::new();
            let running = running_signals();
            steps.push(driver.advance(character, 0.1875, &running));
            steps.push(driver.advance(character, 0.0625, &running));
            steps.push(driver.advance(
                character,
                0.0625,
                &MotionSignals {
                    jumping: true,
                    ..MotionSignals::default()
                },
            ));
            steps.push(driver.advance(
                character,
                0.0625,
                &MotionSignals {
                    climbing_check: true,
                    ..MotionSignals::default()
                },
            ));
            for _ in 0..5 {
                steps.push(driver.enemy_strike(character));
            }
            steps.push(driver.player_strike(character, None, true));
            steps.push(driver.player_strike(character, Some(Skill::Axe), false));
            for skill in character.career.primary {
                steps.push(driver.use_skill(character, skill, MAX_SKILL_USES));
            }
            steps.push(driver.minute(
                character,
                361,
                &MotionSignals {
                    swimming: true,
                    ..MotionSignals::default()
                },
            ));
            steps.push(driver.minute(character, 722, &running));
            steps
        };

        let mut first_driver = SkillDriver::new(4_242);
        let mut first_character = fresh_character();
        let first = script(&mut first_driver, &mut first_character);

        let mut second_driver = SkillDriver::new(4_242);
        let mut second_character = fresh_character();
        let second = script(&mut second_driver, &mut second_character);

        assert_eq!(first, second);
        assert_eq!(first_driver, second_driver);
        assert_eq!(first_character, second_character);
        assert_eq!(first_character.level, 2);
        // The level-up lands on the `minute(361)` step, the second to last.
        assert_eq!(
            first
                .iter()
                .filter_map(|step| step.level_up)
                .map(|report| report.level)
                .collect::<Vec<_>>(),
            vec![2]
        );

        // The seed only feeds the level-up roll: a different seed changes the
        // RNG stream, never the tally sequence.
        let mut other = SkillDriver::new(4_243);
        let mut other_character = fresh_character();
        let third = script(&mut other, &mut other_character);
        assert_ne!(first_driver.rng(), other.rng());
        let tallies = |steps: &[DriverStep]| -> Vec<Vec<(Skill, i32)>> {
            steps.iter().map(|step| step.tallies.clone()).collect()
        };
        assert_eq!(tallies(&first), tallies(&third));
    }
}
