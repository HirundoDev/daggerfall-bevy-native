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
    enemy_melee_timer_seconds, fall_duration_seconds, h2h_range, material_gate, multi_hit_gate,
    set_health, skeletal_weapon_adjust, stats_to_hit, struck_body_part, success_roll, weapon_floor,
    HitMods, FADE_SECONDS, RESET_SECONDS,
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
    let chance = hit_chance(input, part, crit_ok);
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

/// `CalculateSuccessfulHit` (`FormulaHelper.cs:796-827`): `chanceToHitMod`
/// plus the armour of the struck part (`:1149-1161`), adrenaline
/// (`:1163-1183`), enchantment (`:814`), stats (`:1185-1200`), crit + dodge
/// (`:1213-1218`) and adjustments (`:1224-1251`), clamped `[3,97]`
/// (`behavior::combat::chance_to_hit`). The composition lives here only: the
/// weapon/H2H pipeline and the monster branch both consume it.
fn hit_chance(input: &AttackInput<'_>, part: usize, crit_ok: bool) -> i32 {
    let armor_value = input.target.armor_values.get(part).copied().unwrap_or(0);
    let adrenaline = adrenaline_to_hit(
        input.attacker.adrenaline_bonus(),
        input.target.adrenaline_bonus(),
    );
    let mods = HitMods {
        chance_mod: input.chance_mod + input.weapon.map_or(0, |weapon| weapon.to_hit_mod),
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
    chance_to_hit(mods)
}

/// `MonsterAttackInput` — DFU's monster branch of `CalculateAttackDamage`
/// (`FormulaHelper.cs:636-672`): the struck part is chosen ONCE (`:616`) and
/// up to three bands are gated by the player's reflexes.
#[derive(Debug, Clone, Copy)]
pub struct MonsterAttackInput<'a> {
    /// Attacking monster (`AIAttacker`).
    pub attacker: &'a Combatant,
    /// Struck entity — the **player's** reflexes feed the gate (`:654-657`).
    pub target: &'a Combatant,
    /// `(MinDamage,MaxDamage)`, `(MinDamage2,MaxDamage2)`, `(MinDamage3,MaxDamage3)`
    /// of `MobileEnemy` (M25 composes; a band with `min <= 0` never fires, `:657`).
    pub damage_bands: [(i32, i32); 3],
    /// Skill + swing/proficiency/racial + weapon_to_hit, composed by the app.
    pub chance_mod: i32,
    /// `GetBonusOrPenaltyByEnemyType` added PER HIT that deals > 0 (`:667-669`).
    pub enemy_bonus: i32,
    /// The **target player's** `Reflexes` (`:654-657`).
    pub player_reflexes: i32,
}

/// Per-band report of [`resolve_monster_attack`] — the monster half of the
/// M22→M21 wear boundary (`DamageEquipment` runs once per landed band).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MonsterAttackOutcome {
    /// Struck body part: rolled once for all bands (`:616`).
    pub part: usize,
    /// Damage of each band (0 = gate failed, band ≤ 0, miss, or no roll).
    pub per_hit: [i32; 3],
    /// Number of bands that passed `CalculateSuccessfulHit`.
    pub hits: u32,
    /// Gate rolls that passed (instrumentation for the probe).
    pub gates: u32,
    /// Total damage, clamped `>= 0` (`:699`).
    pub damage: i32,
    /// `DFRandom` rolls consumed (part + every gate + each fired crit/success/damage).
    pub rolls_consumed: u32,
}

/// Monster multi-hit branch (`FormulaHelper.cs:636-672`), frozen roll order:
/// **part (1) → per band [gate (1, always) → crit (1) → success (1) → damage
/// (1, on hit)]**.
///
/// Monsters are unarmed: no material gate, no backstab and no strength bonus
/// (`:659` rolls the raw band). The enemy-type bonus is added per hit that
/// deals `> 0` (`:669`), and the total is clamped `>= 0` (`:699`).
pub fn resolve_monster_attack(
    input: &MonsterAttackInput<'_>,
    dice: &mut Dice100,
) -> MonsterAttackOutcome {
    // The struck part is chosen ONCE for the whole strike (`:615-616`).
    let part = struck_body_part(dice.roll_range(0, 19));
    let mut rolls_consumed = 1;
    // `CalculateSuccessfulHit` reads the same composition as the weapon
    // branch; monsters carry no weapon (`:657`), hence `weapon: None`.
    let view = AttackInput {
        attacker: input.attacker,
        target: input.target,
        weapon: None,
        chance_mod: input.chance_mod,
        damage_mods: 0,
        backstab_level: 0,
        enemy_bonus: input.enemy_bonus,
    };
    let mut per_hit = [0i32; 3];
    let mut hits = 0u32;
    let mut gates = 0u32;

    for (band, &(min, max)) in input.damage_bands.iter().enumerate() {
        // The gate roll is consumed for every band, dead `(0, 0)` ones included
        // (`DFRandom.rand() % 100 < reflexesChance`, `:657`).
        let gate_roll = dice.roll();
        rolls_consumed += 1;
        if !multi_hit_gate(gate_roll, input.player_reflexes) {
            continue;
        }
        gates += 1;
        if min <= 0 {
            continue;
        }
        // `CalculateSuccessfulHit` rolls crit before the hit (`:1216-1218`,
        // `:827`).
        let crit_ok = dice.success(input.attacker.critical_strike);
        let chance = hit_chance(&view, part, crit_ok);
        let hit = success_roll(dice.roll(), chance);
        rolls_consumed += 2;
        if !hit {
            continue;
        }
        let mut hit_damage = dice.roll_range(min, max);
        rolls_consumed += 1;
        if hit_damage > 0 {
            hit_damage += input.enemy_bonus;
        }
        per_hit[band] = hit_damage;
        hits += 1;
    }

    let damage = per_hit.iter().sum::<i32>().max(0);
    MonsterAttackOutcome {
        part,
        per_hit,
        hits,
        gates,
        damage,
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

    #[test]
    fn armor_values_builder_reaches_calculate_armor_to_hit() {
        // `CalculateArmorToHit` (`FormulaHelper.cs:1149-1161`) adds
        // `ArmorValues[struckBodyPart]` to the chance. The builder is the only
        // way the app hands the M21 projection in (ORDER M22).
        let attacker = Combatant::new(1, crate::actor::EntityKind::Player);
        let bare = Combatant::new(2, crate::actor::EntityKind::EnemyClass);
        let armored = Combatant::new(2, crate::actor::EntityKind::EnemyClass)
            .with_armor_values([0, 0, 0, -45, 0, 0, 0]);
        let bare_input = AttackInput {
            attacker: &attacker,
            target: &bare,
            weapon: None,
            // 100 − 50 (universal adjustment) = 50 bare; the chest −45 gives
            // 5, so neither side clamps and the delta is the raw term.
            chance_mod: 100,
            damage_mods: 0,
            backstab_level: 0,
            enemy_bonus: 0,
        };
        let armored_input = AttackInput {
            target: &armored,
            ..bare_input
        };
        let mut saw_chest = false;
        for seed in 0..256u64 {
            let mut bare_dice = Dice100::with_seed(seed);
            let mut armored_dice = Dice100::with_seed(seed);
            let bare_outcome = resolve_attack(&bare_input, &mut bare_dice);
            let armored_outcome = resolve_attack(&armored_input, &mut armored_dice);
            assert_eq!(bare_outcome.part, armored_outcome.part, "same rolls");
            assert_eq!(
                bare_outcome.chance - armored_outcome.chance,
                -armored.armor_values[armored_outcome.part],
                "Δchance is exactly the armour term (seed {seed})"
            );
            if armored_outcome.part == 3 {
                saw_chest = true;
                assert_eq!(bare_outcome.chance - armored_outcome.chance, 45);
            }
        }
        assert!(saw_chest, "256 seeds strike the chest at least once");
    }

    /// Monster attacker and player target for the multi-hit block.
    /// `chance_mod = 200` clamps the chance at 97 (`:805-827`), so the
    /// outcomes turn on the gate and damage rolls only.
    fn monster_pair() -> (Combatant, Combatant) {
        (
            Combatant::new(1, crate::actor::EntityKind::EnemyMonster),
            Combatant::new(2, crate::actor::EntityKind::Player),
        )
    }

    fn monster_input<'a>(
        attacker: &'a Combatant,
        target: &'a Combatant,
        damage_bands: [(i32, i32); 3],
        enemy_bonus: i32,
        player_reflexes: i32,
    ) -> MonsterAttackInput<'a> {
        MonsterAttackInput {
            attacker,
            target,
            damage_bands,
            chance_mod: 200,
            enemy_bonus,
            player_reflexes,
        }
    }

    #[test]
    fn monster_bands_land_up_to_three_hits_on_one_body_part() {
        let (attacker, target) = monster_pair();
        let input = monster_input(&attacker, &target, [(2, 5); 3], 0, 2);
        let mut multi_hit_runs = 0;
        for seed in 0..64u64 {
            let outcome = resolve_monster_attack(&input, &mut Dice100::with_seed(seed));
            // The part is rolled ONCE (`:616`): it is the first roll of the
            // seed's stream and never re-rolled per band.
            assert_eq!(
                outcome.part,
                struck_body_part(Dice100::with_seed(seed).roll_range(0, 19)),
                "seed {seed} part is the first roll"
            );
            if outcome.hits >= 2 {
                multi_hit_runs += 1;
            }
        }
        // Measured over seeds 0..64: 31 strikes land two or three bands.
        assert_eq!(multi_hit_runs, 31);

        // Measured per-seed vectors.
        let seed_1 = resolve_monster_attack(&input, &mut Dice100::with_seed(1));
        assert_eq!(
            seed_1,
            MonsterAttackOutcome {
                part: 5,
                per_hit: [0, 5, 4],
                hits: 2,
                gates: 2,
                damage: 9,
                rolls_consumed: 10,
            }
        );
        let seed_5 = resolve_monster_attack(&input, &mut Dice100::with_seed(5));
        assert_eq!(
            seed_5,
            MonsterAttackOutcome {
                part: 4,
                per_hit: [0, 2, 2],
                hits: 2,
                gates: 2,
                damage: 4,
                rolls_consumed: 10,
            }
        );
        let seed_7 = resolve_monster_attack(&input, &mut Dice100::with_seed(7));
        assert_eq!(
            seed_7,
            MonsterAttackOutcome {
                part: 1,
                per_hit: [5, 0, 3],
                hits: 2,
                gates: 2,
                damage: 8,
                rolls_consumed: 10,
            }
        );
        let seed_10 = resolve_monster_attack(&input, &mut Dice100::with_seed(10));
        assert_eq!(
            seed_10,
            MonsterAttackOutcome {
                part: 1,
                per_hit: [5, 5, 2],
                hits: 3,
                gates: 3,
                damage: 12,
                rolls_consumed: 13,
            }
        );
        let seed_0 = resolve_monster_attack(&input, &mut Dice100::with_seed(0));
        assert_eq!(
            seed_0,
            MonsterAttackOutcome {
                part: 0,
                per_hit: [0, 0, 3],
                hits: 1,
                gates: 1,
                damage: 3,
                rolls_consumed: 7,
            }
        );
    }

    #[test]
    fn monster_gate_never_passing_consumes_only_the_gate_rolls() {
        // `reflexes = 20` ⇒ `50 − 10·18 = −130` (`:654`): no band fires.
        let (attacker, target) = monster_pair();
        let input = monster_input(&attacker, &target, [(2, 5); 3], 0, 20);
        let outcome = resolve_monster_attack(&input, &mut Dice100::with_seed(0));
        assert_eq!(outcome.rolls_consumed, 4, "1 part + 3 gates");
        assert_eq!(outcome.gates, 0);
        assert_eq!(outcome.hits, 0);
        assert_eq!(outcome.per_hit, [0, 0, 0]);
        assert_eq!(outcome.damage, 0);
    }

    #[test]
    fn monster_zero_min_band_never_rolls_crit_success_or_damage() {
        // All bands dead: the gates still pass (measured 1/2/3) yet every
        // strike consumes exactly 1 part + 3 gate rolls (`:657` short-circuit).
        let (attacker, target) = monster_pair();
        let input = monster_input(&attacker, &target, [(0, 0); 3], 0, 2);
        for (seed, gates) in [(0u64, 1u32), (1, 2), (2, 3)] {
            let outcome = resolve_monster_attack(&input, &mut Dice100::with_seed(seed));
            assert_eq!(outcome.gates, gates, "seed {seed} passing gates");
            assert_eq!(outcome.rolls_consumed, 4, "seed {seed}: 1 + 3, no band rolls");
            assert_eq!(outcome.hits, 0);
            assert_eq!(outcome.per_hit, [0, 0, 0]);
            assert_eq!(outcome.damage, 0);
        }
        // Part 0 is `Head`; the zero-min bands never add a body-part roll.
        assert_eq!(
            resolve_monster_attack(&input, &mut Dice100::with_seed(0)).part,
            0
        );
    }

    #[test]
    fn monster_enemy_bonus_is_added_per_hit() {
        let (attacker, target) = monster_pair();
        let plain = monster_input(&attacker, &target, [(2, 5); 3], 0, 2);
        let bonus = monster_input(&attacker, &target, [(2, 5); 3], 10, 2);
        // Seed 1 measured: bands 1 and 2 land (5 and 4 raw).
        let raw = resolve_monster_attack(&plain, &mut Dice100::with_seed(1));
        let boosted = resolve_monster_attack(&bonus, &mut Dice100::with_seed(1));
        assert_eq!(raw.hits, 2);
        assert_eq!(boosted.per_hit, [0, 15, 14]);
        assert_eq!(boosted.damage - raw.damage, 20, "10 per landed hit");
        assert_eq!(raw.rolls_consumed, boosted.rolls_consumed, "bonus rolls nothing");
    }

    #[test]
    fn monster_attack_is_deterministic_per_seed() {
        let (attacker, target) = monster_pair();
        let input = monster_input(&attacker, &target, [(2, 5); 3], 0, 2);
        let first = resolve_monster_attack(&input, &mut Dice100::with_seed(10));
        let again = resolve_monster_attack(&input, &mut Dice100::with_seed(10));
        assert_eq!(first, again, "same seed ⇒ identical outcome");
        let other = resolve_monster_attack(&input, &mut Dice100::with_seed(11));
        assert_ne!(first, other, "different seed ⇒ different outcome");
    }
}
