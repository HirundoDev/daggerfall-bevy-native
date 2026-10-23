//! M23 — the demo window's magic layer, pure (0 Bevy): the cast of the demo
//! spell and the per-game-minute magic round, testable without a window.
//!
//! DFU runs one magic round per game minute passed — "Every game minute
//! passing is another magic round … Execute magic rounds for each minute
//! passed" (`EntityEffectBroker.cs:205-245`, round counter
//! `MagicRoundsSinceStartup` `:70-75`) — and charges a cast against the
//! caster's spell-point pool with the retail bundle formula
//! (`CalculateTotalEffectCosts`, `FormulaHelper.cs:2189-2227`), refusing a
//! cast the pool cannot pay (`EntityEffectManager.cs:337-343`) and decreasing
//! the pool on release (`:387-388`).
//!
//! The window systems in `m18_window` are thin shells over [`MagicLayer`]: the
//! cast key, the round hook at the M20 minute clock and the projection of the
//! movement flags into the M19 engine (`MoveInput.levitating`,
//! `PlayerBody.slow_falling`/`water_breathing`). The item triggers
//! (Held/Equipped/Used/Breaks/MagicRound/Reroll) are M24's — this tree has no
//! item organ, and this layer does not invent one.

use daggerfall_behavior::magic::{
    calculate_total_effect_costs, effect_by_key, CastReport, EffectEntry, EffectSettings,
    EffectTarget, MagicDice, MagicState, RoundReport, SpellCost, TargetType,
};
use daggerfall_combat::actor_seed;

/// Actor id reserved for the window's cast/round dice: sibling of
/// `combat_runtime::WEAR_STREAM_ACTOR` (`u32::MAX`) and `EFFECT_STREAM_ACTOR`
/// (`u32::MAX - 1`) — a dedicated stream so a cast or a magic round can never
/// shift an attack, wear or enchantment roll.
pub const MAGIC_STREAM_ACTOR: u32 = u32::MAX - 2;

/// Duration settings of the demo's Levitate (duration 1 + 1 per level step ⇒
/// 2 rounds at level 1): the same reference spell the M18 demo block and
/// `PROBE-EFFECT-001` block E cast (`m19_probe::cast_movement_effects`). The
/// rest of the settings stay 0 — `Levitate` declares `DurationCosts` only and
/// supports neither chance nor magnitude (`Levitate.cs:23-37`).
const LEVITATE_SETTINGS: EffectSettings = EffectSettings {
    duration_base: 1,
    duration_plus: 1,
    duration_per_level: 1,
    chance_base: 0,
    chance_plus: 0,
    chance_per_level: 0,
    magnitude_base_min: 0,
    magnitude_base_max: 0,
    magnitude_plus_min: 0,
    magnitude_plus_max: 0,
    magnitude_per_level: 0,
};

/// Result of a demo cast attempt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CastOutcome {
    /// Retail spell-point cost of the bundle (`CalculateTotalEffectCosts`).
    pub cost: SpellCost,
    /// `false` when the pool could not pay the cost: nothing was cast and
    /// nothing moved (`EntityEffectManager.cs:337-343`).
    pub applied: bool,
    /// The engine report of the cast (`None` when the pool refused it).
    pub report: Option<CastReport>,
}

/// The window's magic layer: the active-effect state plus the dedicated dice
/// stream of casts and rounds ([`MAGIC_STREAM_ACTOR`]).
#[derive(Debug, Clone)]
pub struct MagicLayer {
    state: MagicState,
    dice: MagicDice,
}

impl MagicLayer {
    /// Layer over the given session seed; the dice is the dedicated magic
    /// stream, so casts/rounds never shift an attack or wear roll.
    pub fn new(seed: u64) -> Self {
        Self {
            state: MagicState::default(),
            dice: MagicDice::with_seed(actor_seed(seed, MAGIC_STREAM_ACTOR)),
        }
    }

    /// Casts the demo's Levitate (caster-only target) on `target`.
    ///
    /// - **Cost**: the retail bundle formula
    ///   (`calculate_total_effect_costs`, `FormulaHelper.cs:2189-2227`).
    ///   `live_school_skill` is the caster's live skill for the effect's
    ///   school — Levitate is Thaumaturgy (`Levitate.cs:23-37`) — resolved by
    ///   the caller from the character sheet, which is DFU's
    ///   `GetLiveSkillValue(Properties.MagicSkill)` for this single-entry
    ///   bundle.
    /// - **Pool**: a cast the spell points cannot pay is refused untouched
    ///   (`EntityEffectManager.cs:337-343`); a cast that runs decreases the
    ///   pool by the cost (`:387-388`).
    /// - **Effects**: [`MagicState::cast_effects`] runs the DFU cast lifecycle
    ///   (chance, `SetDuration`, initial `MagicRound`).
    pub fn cast_levitate(
        &mut self,
        level: i32,
        live_school_skill: i32,
        target: &mut EffectTarget,
    ) -> CastOutcome {
        let entries = [EffectEntry {
            props: effect_by_key("Levitate").expect("reference template registered"),
            settings: LEVITATE_SETTINGS,
        }];
        let cost = calculate_total_effect_costs(
            &entries,
            TargetType::CasterOnly,
            false,
            &|_| live_school_skill,
        );
        if target.spell_points < cost.spell_points {
            return CastOutcome {
                cost,
                applied: false,
                report: None,
            };
        }
        target.spell_points -= cost.spell_points;
        let report = self
            .state
            .cast_effects(&entries, level, target, &mut self.dice);
        CastOutcome {
            cost,
            applied: true,
            report: Some(report),
        }
    }

    /// One game-minute magic round ([`MagicState::magic_round_with`], DFU
    /// `DoMagicRound`, `EntityEffectManager.cs:1730-1735`) over the caster's
    /// level; `expired` names the effects whose `End()` ran this round.
    pub fn magic_round(&mut self, level: i32, target: &mut EffectTarget) -> RoundReport {
        self.state.magic_round_with(level, target, &mut self.dice)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use daggerfall_behavior::magic::CAST_COST_FLOOR;

    fn target(pool: i32) -> EffectTarget {
        EffectTarget::new(100, pool, [50; 8])
    }

    #[test]
    fn levitate_cast_charges_the_pool_and_raises_the_movement_flag() {
        let mut layer = MagicLayer::new(7);
        let mut target = target(100);
        let outcome = layer.cast_levitate(1, 50, &mut target);
        assert!(outcome.applied, "a pool of 100 pays the level-1 cost");
        let report = outcome.report.expect("an applied cast reports");
        assert_eq!(
            report.scheduled,
            vec![2],
            "SetDuration 1 + 1 per level step @ 1 ⇒ 2 rounds"
        );
        assert!(
            outcome.cost.spell_points >= CAST_COST_FLOOR,
            "the retail floor of 5 spell points holds"
        );
        assert_eq!(
            target.spell_points,
            100 - outcome.cost.spell_points,
            "the pool paid exactly the retail cost (EntityEffectManager.cs:387-388)"
        );
        let movement = target.movement_effects();
        assert!(
            movement.levitating,
            "the initial MagicRound raised the flag M19 consumes"
        );
        assert!(!movement.slow_falling, "no flag invented");
        assert!(!movement.water_breathing, "no flag invented");
    }

    #[test]
    fn the_magic_round_expires_levitate_after_its_two_applications() {
        let mut layer = MagicLayer::new(7);
        let mut target = target(100);
        let cast = layer.cast_levitate(1, 50, &mut target);
        assert!(cast.applied && target.movement_effects().levitating);
        let first = layer.magic_round(1, &mut target);
        assert!(first.expired.is_empty(), "round 1 of 2 still runs");
        assert!(target.movement_effects().levitating);
        let second = layer.magic_round(1, &mut target);
        assert_eq!(
            second.expired,
            vec!["Levitate"],
            "End() lands on round N + 1 (EntityEffectManager.cs:1730-1735)"
        );
        assert!(
            !target.movement_effects().levitating,
            "the flag left with End()"
        );
    }

    #[test]
    fn a_pool_below_the_cost_refuses_the_cast_untouched() {
        let mut layer = MagicLayer::new(7);
        let mut poor = target(CAST_COST_FLOOR - 1);
        let before = poor.observe();
        let outcome = layer.cast_levitate(1, 50, &mut poor);
        assert!(!outcome.applied);
        assert!(outcome.report.is_none());
        assert_eq!(poor.spell_points, CAST_COST_FLOOR - 1, "not charged");
        assert_eq!(poor.observe(), before, "no effect ran");
        assert!(!poor.movement_effects().levitating);
    }
}
