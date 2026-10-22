//! Combat engine (spec M22 · ORDER: `crates/combat {rng, actor, engine}`).
//!
//! [`resolve_attack`] runs the verified DFU pipeline with the frozen roll
//! order: **material gate (0 rolls) → part → critical → success → damage
//! range** (spec §1.1, §2.1, §6). [`EnemyAttack`] replicates
//! `ResetMeleeTimer` (`EnemyAttack.cs:114-132`) and [`DeathSequence`] the
//! `deathInProgress` latch with fall/fade/reset (`PlayerDeath.cs:29-33`, `:160`).
//!
//! Pure: the PRNG arrives as the actor's [`Dice100`] (delta D-M22-1); 0
//! global RNG, 0 wall clock, 0 I/O.

use daggerfall_behavior::combat::{
    adjustments, adrenaline_to_hit, backstab_damage, chance_to_hit, critical_bonus,
    enemy_melee_timer_seconds, fall_duration_seconds, h2h_range, material_gate, set_health,
    skeletal_weapon_adjust, stats_to_hit, struck_body_part, success_roll, weapon_floor, HitMods,
    FADE_SECONDS, RESET_SECONDS,
};

use crate::actor::{Combatant, Weapon};
use crate::rng::Dice100;

/// `WeaponMaterialTypes::Silver = 2` (M21 §2.2 `WeaponMaterialTypes`,
/// `ItemEnums.cs:64-77`) — scale of [`Weapon::native_material_value`] and of
/// `MinMetalToHit` (spec §2.1 material gate).
pub const MATERIAL_SILVER: i32 = 2;

/// Input of [`resolve_attack`]: everything DFU composes outside the
/// formula (live skills M20, materials M21, swing/proficiency/racial on the
/// input side §1.1) arrives already summed by the app — ORDER M22: 0 crate
/// cycle.
#[derive(Debug, Clone, Copy)]
pub struct AttackInput<'a> {
    /// Attacker (stats and `strength_modifier` composed by the app).
    pub attacker: &'a Combatant,
    /// Target (armor, dodging, material gate, biography).
    pub target: &'a Combatant,
    /// `Some` ⇒ armed attack; `None` ⇒ H2H (`FormulaHelper.cs:585-589`).
    pub weapon: Option<&'a Weapon>,
    /// Skill + swing + proficiency + racial + backstab (`:592-612`) and
    /// `weapon_to_hit`, which the engine adds (`:679`).
    pub chance_mod: i32,
    /// Swing + proficiency + racial damageModifiers (`:597-609`).
    pub damage_mods: i32,
    /// Backstab: the engine only rolls if `> 1` (`:984-986`); `≤ 1` = no
    /// backstab.
    pub backstab_level: i32,
    /// `GetBonusOrPenaltyByEnemyType` (`:763`) — composed by the app/M25; `0`
    /// until M25.
    pub enemy_bonus: i32,
}

/// Resolution result — the M22→M21 boundary "report" (spec §5.6): the
/// caller receives `damage` + `struckBodyPart` for `DamageEquipment`, and
/// `material_ineffective`/`rolls_consumed` for the demo log (§7.5).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AttackOutcome {
    /// Did it hit? (`false` also at the material gate).
    pub hit: bool,
    /// Clamped `[3,97]` chance to hit actually used (`0` if the gate cut in
    /// earlier).
    pub chance: i32,
    /// Struck body part (`BodyParts`, `:616`/`:863-871`).
    pub part: usize,
    /// Final damage `≥ 0` (`clamp :699`); `0` on a miss or at the material
    /// gate.
    pub damage: i32,
    /// `MinMetalToHit > NativeMaterialValue` ⇒ 0 damage **with no roll and
    /// no wear** (`:576-583`, GV-DMG-12, demo §7.5).
    pub material_ineffective: bool,
    /// `DFRandom` rolls consumed (material gate = 0; miss = 3: part +
    /// critical + success).
    pub rolls_consumed: u32,
}

/// Attack pipeline (`FormulaHelper.CalculateAttackDamage :538-723`):
///
/// 1. Material gate — **0 rolls** (`:576-583`).
/// 2. Body part — roll 0..19 (`:616`).
/// 3. Critical — `SuccessRoll(crit)` (`:1216-1218`).
/// 4. Composition + success — `SuccessRoll(clamp(…,3,97))` (`:805-827`).
/// 5. Damage range — `Range(min..max)` (`:736` weapon / `:771` H2H).
/// 6. Type bonus (`:763`) → backstab (`:688`) → clamp `≥ 0` (`:699`).
pub fn resolve_attack(input: &AttackInput<'_>, dice: &mut Dice100) -> AttackOutcome {
    // 1 · Material gate — early return, no rolls (`:576-583`).
    if let Some(weapon) = input.weapon {
        if material_gate(input.target.min_metal_to_hit, weapon.native_material_value) {
            return AttackOutcome {
                hit: false,
                chance: 0,
                part: 0,
                damage: 0,
                material_ineffective: true,
                rolls_consumed: 0,
            };
        }
    }

    // 2 · Struck body part (`Range(0,20)`, `:616`).
    let part = struck_body_part(dice.roll_range(0, 19));
    // 3 · Critical — its SuccessRoll BEFORE the success one (frozen order).
    let crit_ok = dice.success(input.attacker.critical_strike);
    // 4 · `CalculateSuccessfulHit` composition (`weapon_to_hit` at `:679`).
    let armor_value = input.target.armor_values.get(part).copied().unwrap_or(0);
    let adrenaline = adrenaline_to_hit(
        input.attacker.adrenaline_bonus(),
        input.target.adrenaline_bonus(),
    );
    let mods = HitMods {
        chance_mod: input.chance_mod
            + input.weapon.map_or(0, |weapon| weapon.to_hit_mod),
        armor_value,
        inc_armor_mod: input.target.inc_armor_mod,
        dec_armor_mod: input.target.dec_armor_mod,
        adrenaline,
        enchant_mod: input.attacker.chance_to_hit_modifier,
        stats_diff: stats_to_hit(
            input.attacker.luck,
            input.target.luck,
            input.attacker.agility,
            input.target.agility,
        ),
        dodging: input.target.dodging,
        critical_bonus: critical_bonus(input.attacker.critical_strike, crit_ok),
        adjustments: adjustments(
            input.target.kind.is_player(),
            input.target.biography_avoid_hit_mod,
            input.target.kind.is_monster(),
        ),
    };
    let chance = chance_to_hit(mods);
    let hit = success_roll(dice.roll(), chance);
    let mut rolls_consumed = 3; // part + critical + success
    if !hit {
        return AttackOutcome {
            hit: false,
            chance,
            part,
            damage: 0,
            material_ineffective: false,
            rolls_consumed,
        };
    }

    // 5 · Damage range — weapon (`:736-761`) or H2H (`:771-791`).
    let mut damage = match input.weapon {
        Some(weapon) => {
            let roll = dice.roll_range(weapon.min_damage, weapon.max_damage);
            rolls_consumed += 1;
            let mut composed = roll + input.damage_mods + input.attacker.strength_modifier;
            if input.target.skeletal_warrior {
                // Bone halving (`:743`) + silver ×2 (`:748-749`), in that order.
                composed = skeletal_weapon_adjust(
                    composed,
                    weapon.flags,
                    weapon.native_material_value == MATERIAL_SILVER,
                );
            }
            weapon_floor(composed + weapon.material_damage_mod)
        }
        None => {
            // The strength bonus already lives in the range ONLY for the
            // player (GV-DMG-09..11, spec §2.1).
            let (min, max) = h2h_range(
                input.attacker.hand_to_hand_skill,
                input.attacker.strength_modifier,
                input.attacker.kind.is_player(),
            );
            let roll = dice.roll_range(min, max);
            rolls_consumed += 1;
            roll + input.damage_mods
        }
    };

    // 6 · Enemy type bonus (`:763`) → backstab (`:688`) → `≥ 0` (`:699`).
    damage += input.enemy_bonus;
    if input.backstab_level > 1 {
        let backstab_ok = dice.success(input.backstab_level);
        rolls_consumed += 1;
        damage = backstab_damage(damage, input.backstab_level, backstab_ok);
    }
    damage = damage.max(0);

    AttackOutcome {
        hit: true,
        chance,
        part,
        damage,
        material_ineffective: false,
        rolls_consumed,
    }
}

/// `EnemyAttack` — enemy melee timer (`EnemyAttack.cs:114-132`). The
/// `ResetMeleeTimer` uses the **player's level and reflexes** (R-22.4,
/// GV-CAD-14/15) and returns seconds (`Range(1500,3001)` already rolled by
/// the caller / the engine with the actor's `Dice100`).
#[derive(Debug, Clone, Copy)]
pub struct EnemyAttack {
    /// Seconds remaining until the next enemy strike.
    pub melee_timer_seconds: f32,
}

impl Default for EnemyAttack {
    fn default() -> Self {
        Self::new()
    }
}

impl EnemyAttack {
    /// Exhausted timer (the first tick fires and the caller refills it with
    /// [`Self::reset_melee_timer`]).
    pub fn new() -> Self {
        Self {
            melee_timer_seconds: 0.0,
        }
    }

    /// `ResetMeleeTimer` (`EnemyAttack.cs:114-132`): the
    /// `Range(1500,3001)` roll (roll in ms, already consumed) adjusted with
    /// the **player's** level and reflexes, `max(0)` and `/980` s — formula in
    /// `behavior::combat::enemy_melee_timer_seconds`.
    pub fn reset_melee_timer(
        &mut self,
        roll_ms: i32,
        player_level: i32,
        player_reflexes: i32,
    ) {
        self.melee_timer_seconds =
            enemy_melee_timer_seconds(roll_ms, player_level, player_reflexes);
    }

    /// Subtracts `dt` seconds; returns `true` exactly upon reaching 0
    /// (without refilling: the caller calls [`Self::reset_melee_timer`] again).
    pub fn tick(&mut self, dt_seconds: f32) -> bool {
        self.melee_timer_seconds = (self.melee_timer_seconds - dt_seconds).max(0.0);
        self.melee_timer_seconds == 0.0
    }
}

/// Death sequence with the `deathInProgress` latch — the first
/// `RaiseOnDeath` emits and subsequent ones do not (`DaggerfallEntity.cs:877-880` +
/// `PlayerDeath.cs` latch; spec §4 · demo §7.4). For the player it carries
/// the camera `fall` and the constants `fade = 2 s` / `reset = 3 s`
/// (`PlayerDeath.cs:29-33`, `:160`).
#[derive(Debug, Clone, PartialEq)]
pub struct DeathSequence {
    death_in_progress: bool,
    events: u32,
    /// Camera fall duration (s).
    pub fall_seconds: f32,
    /// `FadeDuration` (s) — 2.0 for the player, 0 with no camera.
    pub fade_seconds: f32,
    /// `TimeBeforeReset` (s) — 3.0 for the player, 0 with no camera.
    pub reset_seconds: f32,
}

impl DeathSequence {
    /// **Player** death: fall derived from the controller height
    /// (`fall_duration_seconds`), fade 2 s and reset 3 s (`PlayerDeath.cs:29-33`).
    pub fn for_player_height(controller_height: f32) -> Self {
        Self {
            death_in_progress: false,
            events: 0,
            fall_seconds: fall_duration_seconds(controller_height),
            fade_seconds: FADE_SECONDS,
            reset_seconds: RESET_SECONDS,
        }
    }

    /// **Enemy** death: only the `OnDeath` latch — no camera (spec
    /// §7.4: the enemy emits once; the fall/fade/reset sequence belongs to
    /// the player).
    pub fn for_enemy() -> Self {
        Self {
            death_in_progress: false,
            events: 0,
            fall_seconds: 0.0,
            fade_seconds: 0.0,
            reset_seconds: 0.0,
        }
    }

    /// `RaiseOnDeathEvent` with latch: `true` only on the first real death
    /// (while `deathInProgress`, no repetition emits again).
    pub fn raise_on_death(&mut self) -> bool {
        if self.death_in_progress {
            return false;
        }
        self.death_in_progress = true;
        self.events += 1;
        true
    }

    /// `set_health` from `behavior::combat` + latch: applies the clamp/restore
    /// (`DaggerfallEntity.cs:331-338`) and emits `OnDeath` the first time
    /// health becomes `≤ 0`. Returns the resulting health.
    pub fn apply_set_health(&mut self, max_health: i32, amount: i32, restore: bool) -> i32 {
        let (health, died) = set_health(max_health, amount, restore);
        if died {
            self.raise_on_death();
        }
        health
    }

    /// `OnDeath` events emitted (latch: the sequence only runs once).
    pub fn events(&self) -> u32 {
        self.events
    }

    /// Is a death in progress (`deathInProgress` latch)?
    pub fn death_in_progress(&self) -> bool {
        self.death_in_progress
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rng::{actor_seed, Dice100};

    /// Fixed attacker/target for the loop block (chance 35 = GV-HIT-01).
    fn fighters() -> (Combatant, Combatant, Weapon) {
        let attacker = Combatant::new(1, crate::actor::EntityKind::Player);
        let mut target = Combatant::new(2, crate::actor::EntityKind::EnemyMonster);
        target.max_health = 10_000;
        target.health = 10_000;
        let weapon = Weapon {
            native_material_value: 1, // Steel (M21 §2.2)
            min_damage: 4,
            max_damage: 11,
            flags: 0,
            to_hit_mod: 0,
            material_damage_mod: 0,
        };
        (attacker, target, weapon)
    }

    #[test]
    fn material_gate_consumes_zero_rolls() {
        let (attacker, mut target, mut weapon) = fighters();
        target.min_metal_to_hit = 1; // Steel
        weapon.native_material_value = 0; // Iron
        let input = AttackInput {
            attacker: &attacker,
            target: &target,
            weapon: Some(&weapon),
            chance_mod: 45,
            damage_mods: 0,
            backstab_level: 0,
            enemy_bonus: 0,
        };
        let mut dice = Dice100::with_seed(actor_seed(20_261_022, 1));
        let outcome = resolve_attack(&input, &mut dice);
        assert!(outcome.material_ineffective, "GV-DMG-12 / demo §7.5");
        assert_eq!(outcome.damage, 0);
        assert_eq!(outcome.rolls_consumed, 0, "material gate: 0 rolls");
        assert!(!outcome.hit);
    }

    #[test]
    fn attack_is_deterministic_per_seed_and_sensitive_to_it() {
        let (attacker, target, weapon) = fighters();
        let input = AttackInput {
            attacker: &attacker,
            target: &target,
            weapon: Some(&weapon),
            chance_mod: 45,
            damage_mods: 0,
            backstab_level: 0,
            enemy_bonus: 0,
        };
        let run = |seed: u64| {
            let mut dice = Dice100::with_seed(actor_seed(seed, 7));
            (0..32)
                .map(|_| resolve_attack(&input, &mut dice))
                .collect::<Vec<_>>()
        };
        let first = run(20_261_022);
        assert_eq!(first, run(20_261_022), "same seed ⇒ same run");
        assert_ne!(first, run(20_261_023), "different seed ⇒ different run");
        // Internal roll cadence: part + critical + success + damage per
        // hit; on a miss only 3.
        for outcome in &first {
            assert!(outcome.rolls_consumed == 3 || outcome.rolls_consumed == 4);
        }
    }

    #[test]
    fn enemy_timer_and_death_latch() {
        let mut attack = EnemyAttack::new();
        attack.reset_melee_timer(1500, 10, 3);
        assert_eq!(format!("{:.6}", attack.melee_timer_seconds), "1.989796", "GV-CAD-14");
        assert!(!attack.tick(1.0));
        assert!(attack.tick(0.99), "fires once upon reaching 0");

        let mut player = DeathSequence::for_player_height(1.7);
        assert_eq!(format!("{:.6}", player.fall_seconds), "0.850000", "GV-DTH-05");
        assert_eq!(player.fade_seconds, 2.0);
        assert_eq!(player.reset_seconds, 3.0);
        assert_eq!(player.apply_set_health(37, -5, false), 0);
        assert_eq!(player.events(), 1, "first OnDeath emits");
        let _ = player.apply_set_health(37, -5, false);
        assert_eq!(player.events(), 1, "deathInProgress latch: no 2nd event");
        assert!(player.death_in_progress());

        let mut enemy = DeathSequence::for_enemy();
        let _ = enemy.apply_set_health(100, -5, false);
        let _ = enemy.apply_set_health(100, -5, false);
        assert_eq!(enemy.events(), 1, "enemy: OnDeath once (§7.4)");
        assert_eq!(enemy.fall_seconds, 0.0, "no camera");
    }
}
