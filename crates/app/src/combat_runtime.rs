//! M22 **production composition layer** — the single runtime the interactive
//! window (`m18_window`), the integrated demo (`m18_demo`) and the probes
//! (`m22_probe`) consume. The probe consumes this runtime, never the reverse:
//! every rule below is the DFU rule, transcribed once.
//!
//! The pure engine (`daggerfall_combat`) resolves a strike from an already
//! composed [`AttackInput`]; the M21 core (`daggerfall_inventory`) owns the
//! equipment table and the wear arithmetic. This module is the join DFU keeps
//! inside `DaggerfallEntity`/`WeaponManager`:
//!
//! - [`Fighter`] — entity + equipment + death latch + the composition its
//!   strike needs (skill/swing/proficiency/racial on `chance_mod`, strength on
//!   `damage_mods`, `GetBonusOrPenaltyByEnemyType` on `enemy_bonus`,
//!   `Reflexes`/level for the enemy timer and the monster gate).
//! - [`CombatSession`] — the per-actor PRNG streams (delta D-M22-1: each actor
//!   carries its own `DFRandom`) plus the wear stream (delta **D-M22-3**).
//! - [`weapon_from_item`] — `DaggerfallUnityItem.GetBaseDamageMin/Max` +
//!   `GetWeaponMaterialModifier` projected onto the engine's [`Weapon`].
//!
//! - [`Fighter::effects`]/[`Fighter::weapon_effects`] — the M23 enchantment
//!   half of a strike: when the app supplies the effect entity and the weapon's
//!   enchantment row, [`CombatSession::player_strike`] runs
//!   [`strike_callbacks`] (`WeaponManager.cs:618`, `FormulaHelper.cs:704-722`)
//!   and writes the resulting health back through `SetHealth`.
//!
//! Boundaries declared here, not implemented (M24/M25 own them): poison
//! (`DaggerfallUnityItem.poisonType`, M23), backstab ×3 for monsters
//! (`FormulaHelper.cs:684-697` only scales player attacks, M23), the item → effect
//! row resolution (M24) and `OnMonsterHit` (M25 AI reaction).

use daggerfall_behavior::locomotion::Dice100 as WearDice;
use daggerfall_behavior::magic::{EffectDice, EffectProperties, EffectTarget, MagicDice};
use daggerfall_combat::enchantments::{strike_callbacks, StrikeCallbacks};
use daggerfall_combat::{
    actor_seed, resolve_attack, resolve_monster_attack, AttackInput, AttackOutcome, Combatant,
    DeathSequence, Dice100, EntityKind, MonsterAttackInput, MonsterAttackOutcome, Weapon,
};
use daggerfall_inventory::equipment::{
    EquipResult, EquipSlot, ItemEquipTable, ItemWear, BODY_PART_COUNT,
};
use daggerfall_inventory::items::{
    weapon_max_damage, weapon_min_damage, weapon_material_modifier, Item, ItemGroup, TO_HIT_MOD,
};

/// Actor id reserved for the **wear** stream: `actor_seed(seed, WEAR_STREAM_ACTOR)`
/// can never collide with a combatant id in the scenarios (1, 4, 9_0xx).
///
/// Delta **D-M22-3**: DFU's armour wear re-roll (`Dice100.SuccessRoll(20)`,
/// `FormulaHelper.cs:1130-1132`) draws from the global `Dice100` stream, so a
/// wear roll shifts every later attack roll. Here the wear pass owns its own
/// per-session stream, so armour wear never perturbs attack resolution; the
/// wear roll is consumed **only** when `(10 * damage + 50) / 100 == 0`.
pub const WEAR_STREAM_ACTOR: u32 = u32::MAX;

/// Actor id reserved for the **enchantment** stream (M23): the item payloads
/// roll their magnitudes and chances off their own stream, so they can never
/// shift an attack or a wear roll (`Dice100.SuccessRoll`, `FormulaHelper.cs`).
pub const EFFECT_STREAM_ACTOR: u32 = u32::MAX - 1;

/// Engine view of a weapon, composed from the M21 item
/// (`DaggerfallUnityItem.GetBaseDamageMin/Max`, `DaggerfallUnityItem.cs:969-977`,
/// `GetWeaponMaterialModifier`, `:979-1005`, through the M21 tables
/// `FormulaHelper.CalculateWeaponMin/MaxDamage` and `TO_HIT_MOD`).
///
/// `None` for a non-weapon (`ItemGroup::Weapons` is the DFU group that owns
/// 113..=131) or for a template whose maximum is `0` — an Arrow (131) reaches
/// `CalculateWeaponMaxDamage`'s default `0`, so it never becomes a `Weapon`.
pub fn weapon_from_item(item: &Item) -> Option<Weapon> {
    if item.item_group != ItemGroup::Weapons {
        return None;
    }
    let max_damage = weapon_max_damage(item.template_index);
    if max_damage <= 0 {
        return None;
    }
    let material = usize::try_from(item.native_material).ok();
    let to_hit_mod = material
        .and_then(|index| TO_HIT_MOD.get(index).copied())
        .unwrap_or(0);
    Some(Weapon {
        native_material_value: item.native_material,
        min_damage: weapon_min_damage(item.template_index),
        max_damage,
        flags: item.flags,
        to_hit_mod,
        material_damage_mod: weapon_material_modifier(item.native_material),
    })
}

/// `EnemyEntity.SetEnemyEquipment` "no armour" base
/// (`EnemyEntity.cs:409-413`: `ArmorValues[i] = 100`).
pub const ENEMY_ARMOR_BASE: i32 = 100;

/// Player "no armour" base (`SerializablePlayer.cs:355-366` "Initialize body
/// part armor values to 100 (no armor)"; second seat `PlayerEntity.cs:1188-1191`
/// in `SetEntityDefaults`). Every body part starts at **100** and each worn
/// piece *subtracts* its delta (`DaggerfallEntity.cs:589-636`), so the entity
/// vector is `base + delta` — exactly like the enemy policy. The `0` of
/// [`ItemEquipTable::armor_values`] is that delta, not an `ArmorValues` value.
pub const PLAYER_ARMOR_BASE: i32 = 100;

/// `EnemyEntity.SetEnemyEquipment` class clamp (`EnemyEntity.cs:424-435`);
/// DFU's own comment calls classic's `> 50` variant an oversight.
pub const ENEMY_CLASS_ARMOR_CAP: i32 = 60;

/// `EnemyEntity.SetEnemyEquipment` armour policy (`EnemyEntity.cs:404-451`) —
/// how a spawned enemy projects its equipment onto its own `ArmorValues[7]`.
///
/// Both entities carry the **same** base: every body part starts at `100`
/// ("no armour") and each worn piece subtracts its delta
/// (`DaggerfallEntity.cs:589-636`). The player is seeded at `100` by
/// `SerializablePlayer.cs:355-366` (and again by `PlayerEntity.cs:1188-1191`
/// in `SetEntityDefaults`); an enemy is seeded at `100` by
/// `EnemyEntity.cs:409-413` (or floored at `MobileEnemy.ArmorValue * 5` when
/// unequipped, `:436-450`). `FormulaHelper.CalculateArmorToHit` returns
/// `ArmorValues[part]` verbatim and `CalculateSuccessfulHit` adds it
/// (`FormulaHelper.cs:808`, `:1149-1161`).
///
/// [`ItemEquipTable::armor_values`] projects the **delta** (`0` = no armour,
/// negative with armour) so that the equip table stays free of an entity
/// convention; neither [`Fighter::refresh_equipment`] nor this policy may hand
/// that delta to [`Combatant::armor_values`]. A player naked at `0` would hit
/// the `[3, 97]` clamp at 3 % where DFU gives 95 % (a steel cuirass, delta
/// `-45`, gives 50 %), and an enemy would lose 100 points of its own vector.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnemyArmorPolicy {
    /// `EntityTypes.EnemyClass`: every part is clamped to `<= 60`
    /// (`EnemyEntity.cs:424-435`).
    Class,
    /// `EntityTypes.EnemyMonster`: equipment only counts while it beats the
    /// definition, `MobileEnemy.ArmorValue * 5` (`EnemyEntity.cs:436-450`).
    Monster {
        /// `MobileEnemy.ArmorValue` of the row (M25).
        armor_value: i32,
    },
}

impl EnemyArmorPolicy {
    /// `EnemyEntity.SetEnemyEquipment` (`EnemyEntity.cs:409-450`) over
    /// `table`: `100`, minus the worn armour of the `Head..Feet` slots
    /// (`:415-422`, the `Feet` slot is excluded by the source loop), then the
    /// class clamp or the monster floor.
    pub fn project(self, table: &ItemEquipTable) -> [i32; BODY_PART_COUNT] {
        let mut values = table.armor_values_over_slots(EquipSlot::Head, EquipSlot::Feet);
        for value in &mut values {
            *value += ENEMY_ARMOR_BASE;
        }
        match self {
            Self::Class => {
                for value in &mut values {
                    if *value > ENEMY_CLASS_ARMOR_CAP {
                        *value = ENEMY_CLASS_ARMOR_CAP;
                    }
                }
            }
            Self::Monster { armor_value } => {
                // `(sbyte)(mobileEnemy.ArmorValue * 5)` (`:445-447`).
                let floor = armor_value * 5;
                for value in &mut values {
                    if *value > floor {
                        *value = floor;
                    }
                }
            }
        }
        values
    }
}

/// One combatant plus everything the M22 pipeline reads outside the pure
/// formulas: the 27-slot equip table (M21), the death latch
/// (`DaggerfallEntity.deathInProgress`), the composed [`Weapon`] view and the
/// per-strike modifiers the app owns (`FormulaHelper.cs:592-612`).
#[derive(Debug, Clone)]
pub struct Fighter {
    /// Identity + live stats the engine reads (`Combatant`).
    pub combatant: Combatant,
    /// Equipped items (M21); `armor_values` is always re-projected from here.
    pub equipment: ItemEquipTable,
    /// `EnemyEntity.SetEnemyEquipment` armour policy of a spawned enemy
    /// (`EnemyEntity.cs:404-451`); `None` = the player projection: the M21
    /// delta rebased on [`PLAYER_ARMOR_BASE`] (`SerializablePlayer.cs:355-366`).
    pub armor_policy: Option<EnemyArmorPolicy>,
    /// `OnDeath` latch + camera sequence (`DaggerfallEntity.cs:877-880`).
    pub death: DeathSequence,
    /// Composed view of the equipped weapon (refreshed from the table).
    pub weapon: Option<Weapon>,
    /// Slot the composed weapon occupies (`ItemEquipTable.GetWeaponSlot`).
    pub weapon_slot: Option<EquipSlot>,
    /// Skill + swing/proficiency/racial (+ backstab chance), composed by the
    /// app (`FormulaHelper.cs:592-612`).
    pub chance_mod: i32,
    /// Swing damage modifier (`FormulaHelper.cs:597-599`).
    pub damage_mods: i32,
    /// Backstab level — `> 1` rolls `SuccessRoll(level)` for the ×3
    /// (`FormulaHelper.cs:684-697`).
    pub backstab_level: i32,
    /// `GetBonusOrPenaltyByEnemyType` (`FormulaHelper.cs:763`, M25 composes).
    pub enemy_bonus: i32,
    /// M20 `Reflexes` — read by the monster gate when this fighter is the
    /// TARGET (`FormulaHelper.cs:654-657`).
    pub reflexes: i32,
    /// M20 level — feeds `EnemyAttack::reset_melee_timer`
    /// (`EnemyAttack.cs:114-132`).
    pub level: i32,
    /// `MobileEnemy` `(Min,Max)` bands (M25); `Some` only for `EnemyMonster`.
    pub monster_bands: Option<[(i32, i32); 3]>,
    /// M23 effect entity of this fighter: the enchantment payloads mutate this
    /// view, and `health`/`max_health` are re-projected from `combatant`
    /// before every strike. `None` until the app supplies one — `Combatant`
    /// carries two attributes, the eight `DFCareer.Stats` live in the character
    /// sheet.
    pub effects: Option<EffectTarget>,
    /// Enchantment row the equipped weapon carries (`EnchantmentPayloadFlags`),
    /// resolved by the app from the item (M24); `None` = unenchanted weapon.
    pub weapon_effects: Option<&'static EffectProperties>,
}

impl Fighter {
    /// Fighter with an empty equip table and no weapon. The armour vector is
    /// projected from that empty table, never from a literal.
    pub fn new(combatant: Combatant, death: DeathSequence) -> Self {
        let mut fighter = Self {
            combatant,
            equipment: ItemEquipTable::new(),
            armor_policy: None,
            death,
            weapon: None,
            weapon_slot: None,
            chance_mod: 0,
            damage_mods: 0,
            backstab_level: 0,
            enemy_bonus: 0,
            reflexes: 0,
            level: 1,
            monster_bands: None,
            effects: None,
            weapon_effects: None,
        };
        fighter.refresh_equipment();
        fighter
    }

    /// Attaches the M23 half of the strike: the effect entity and the weapon's
    /// enchantment row ([`strike_callbacks`] runs on the first `player_strike`
    /// once both fighters carry an entity).
    pub fn with_effects(
        mut self,
        effects: EffectTarget,
        weapon_effects: Option<&'static EffectProperties>,
    ) -> Self {
        self.effects = Some(effects);
        self.weapon_effects = weapon_effects;
        self
    }

    /// Delegates to [`ItemEquipTable::equip`] (`EquipItem`,
    /// `ItemEquipTable.cs:94-149`) with `alwaysEquip = true` — a kit is laid
    /// down over a known slot, and every displaced piece travels back inside
    /// the [`EquipResult`] — then re-projects the table.
    pub fn equip(&mut self, item: Item) -> EquipResult {
        let result = self.equipment.equip(item, true);
        self.refresh_equipment();
        result
    }

    /// Re-projects the table: `combatant.armor_values` from
    /// [`ItemEquipTable::armor_values`] (`DaggerfallEntity.UpdateEquippedArmorValues`,
    /// `DaggerfallEntity.cs:589-636`) **plus** [`PLAYER_ARMOR_BASE`] — the
    /// `100` "no armour" seed of `SerializablePlayer.cs:355-366` — or from the
    /// spawned enemy's [`EnemyArmorPolicy`] (`EnemyEntity.SetEnemyEquipment`,
    /// `EnemyEntity.cs:404-451`) — never a literal — plus
    /// `weapon_slot`/`weapon` from the table. Must be called after every
    /// wear: a broken piece leaves the slot, and the armour bonus with it.
    pub fn refresh_equipment(&mut self) {
        self.combatant.armor_values = match self.armor_policy {
            None => self
                .equipment
                .armor_values()
                .map(|delta| PLAYER_ARMOR_BASE + delta),
            Some(policy) => policy.project(&self.equipment),
        };
        self.weapon_slot = self.equipment.equipped_weapon_slot();
        self.weapon = self
            .weapon_slot
            .and_then(|slot| self.equipment.get(slot))
            .and_then(weapon_from_item);
    }

    /// Sets the spawned enemy's armour policy (`EnemyEntity.SetEnemyEquipment`)
    /// and re-projects the table.
    pub fn with_enemy_armor(mut self, policy: EnemyArmorPolicy) -> Self {
        self.armor_policy = Some(policy);
        self.refresh_equipment();
        self
    }

    /// Current health.
    pub fn health(&self) -> i32 {
        self.combatant.health
    }

    /// Alive (`health > 0`; the latch itself lives in [`Fighter::death`]).
    pub fn is_alive(&self) -> bool {
        self.combatant.health > 0
    }
}

/// Result of the player's strike: the engine report plus everything the
/// M22→M21 wear boundary produced (`FormulaHelper.cs:1088-1122`).
#[derive(Debug, Clone)]
pub struct PlayerStrike {
    /// Engine report of `resolve_attack`.
    pub attack: AttackOutcome,
    /// Target health after `SetHealth` (`DaggerfallEntity.cs:331-338`).
    pub target_health: i32,
    /// The strike brought the target to `<= 0` (already-latched deaths are
    /// unreachable: a dead target is refused by the guards).
    pub target_died: bool,
    /// `DamageEquipment` report for the target's struck part (`None` when the
    /// damage was 0 or no armour piece occupies that part).
    pub target_wear: Option<ItemWear>,
    /// M23 enchantment callbacks of this strike (`WeaponManager.cs:618`):
    /// `None` when no effect entity or no enchanted weapon was supplied.
    pub enchantment: Option<StrikeCallbacks>,
    /// Attacker weapon wear (`ApplyConditionDamageThroughPhysicalHit`,
    /// `FormulaHelper.cs:1093`); `None` when unarmed or damage was 0.
    pub weapon_wear: Option<ItemWear>,
}

/// Runs the M23 `Strikes`/`Struck` callbacks of one resolved hit and writes the
/// health an effect moved back into the combatant through `SetHealth`
/// (`DaggerfallEntity.cs:331-338`). `None` when either fighter lacks its effect
/// entity or neither side carries an enchantment row — then nothing runs.
fn run_strike_enchantment<D: EffectDice>(
    attacker: &mut Fighter,
    target: &mut Fighter,
    damage: i32,
    dice: &mut D,
) -> Option<StrikeCallbacks> {
    if attacker.effects.is_none() || target.effects.is_none() {
        return None;
    }
    // `WeaponManager.cs:618` fires the item callbacks on a **hit** only; a
    // zero-damage swing never reaches them.
    if damage <= 0 {
        return None;
    }
    if attacker.weapon_effects.is_none() && target.weapon_effects.is_none() {
        return None;
    }
    let report = {
        let attacker_effects = attacker.effects.as_mut()?;
        let target_effects = target.effects.as_mut()?;
        // Live resources first: the effect entity is the projection of the
        // combatant, never a second source of truth.
        attacker_effects.health = attacker.combatant.health;
        attacker_effects.max_health = attacker.combatant.max_health;
        target_effects.health = target.combatant.health;
        target_effects.max_health = target.combatant.max_health;
        strike_callbacks(
            attacker.weapon_effects,
            target.weapon_effects,
            attacker_effects,
            target_effects,
            damage,
            dice,
        )
    };
    // Write back only what the payloads moved (`SetHealth`, `:331-338`).
    for fighter in [attacker, target] {
        let Some(effects) = fighter.effects.as_mut() else {
            continue;
        };
        if effects.health != fighter.combatant.health {
            let is_player = fighter.combatant.kind == EntityKind::Player;
            let clamped = fighter.death.apply_set_health(
                fighter.combatant.max_health,
                effects.health,
                is_player,
            );
            fighter.combatant.health = clamped;
            effects.health = clamped;
        }
    }
    Some(report)
}

/// Result of the enemy's strike — the monster branch (`multi`) or the single
/// attack (`single`), plus the same wear boundary and the same M23 item
/// callbacks as [`PlayerStrike`].
#[derive(Debug, Clone)]
pub struct EnemyStrike {
    /// Monster multi-hit report (`FormulaHelper.cs:636-672`); `Some` for an
    /// unarmed `EnemyMonster` with bands.
    pub multi: Option<MonsterAttackOutcome>,
    /// Weapon/H2H report; `Some` for every other attacker.
    pub single: Option<AttackOutcome>,
    /// Struck body part (`:616`, rolled once for all bands).
    pub part: usize,
    /// Damage the strike carried (sum of the bands, or the single outcome).
    pub damage: i32,
    /// Player health after `SetHealth`.
    pub target_health: i32,
    /// The strike brought the player to `<= 0`.
    pub target_died: bool,
    /// `DamageEquipment` report for the player's struck part.
    pub target_wear: Option<ItemWear>,
    /// M23 enchantment callbacks of this strike (`EnemyAttack.cs:262-268` for
    /// the player target and `:377-383` for a non-player one): the enemy's
    /// weapon `Strikes` payload plus the defender's (the player's) worn
    /// artifact `Struck` payload, whose request lands on the AI **attacker**
    /// (`FormulaHelper.cs:703-717`, the Ring of Namira path). `None` when no
    /// effect entity or no enchantment row was supplied.
    pub enchantment: Option<StrikeCallbacks>,
    /// Enemy weapon wear (`None` for the unarmed monster branch).
    pub weapon_wear: Option<ItemWear>,
}

/// One combat encounter: the player, the enemies and the PRNG streams.
///
/// Streams (delta D-M22-1): `player_dice = actor_seed(seed, player.id)`, one
/// `enemy_dice[i] = actor_seed(seed, enemies[i].id)`, and the wear stream
/// `actor_seed(seed, WEAR_STREAM_ACTOR)` (delta **D-M22-3**).
#[derive(Debug, Clone)]
pub struct CombatSession {
    /// The player.
    pub player: Fighter,
    /// Enemies, indexed by the `*_strike(index)` methods.
    pub enemies: Vec<Fighter>,
    /// Player attack stream.
    player_dice: Dice100,
    /// One attack stream per enemy (`EnemyAttack` timer rolls included).
    enemy_dice: Vec<Dice100>,
    /// Wear stream (delta D-M22-3).
    wear_dice: WearDice,
    /// M23 enchantment stream ([`EFFECT_STREAM_ACTOR`]) — `MagicDice` is the
    /// `EffectDice` generator (`DFRandom` LCG, `DFRandom.cs:52-56`).
    effect_dice: MagicDice,
    /// Session seed the per-actor streams derive from (`actor_seed(seed, id)`),
    /// kept so a runtime spawn ([`CombatSession::spawn_enemy`]) opens its dice
    /// from the same family as the actors composed at construction.
    seed: u64,
}

impl CombatSession {
    /// Session over the given fighters; every fighter is re-projected once so
    /// armour can never start stale.
    pub fn new(player: Fighter, mut enemies: Vec<Fighter>, seed: u64) -> Self {
        let mut player = player;
        player.refresh_equipment();
        for enemy in &mut enemies {
            enemy.refresh_equipment();
        }
        let player_dice = Dice100::with_seed(actor_seed(seed, player.combatant.id));
        let enemy_dice = enemies
            .iter()
            .map(|enemy| Dice100::with_seed(actor_seed(seed, enemy.combatant.id)))
            .collect();
        let wear_dice = WearDice::new(actor_seed(seed, WEAR_STREAM_ACTOR));
        let effect_dice = MagicDice::with_seed(actor_seed(seed, EFFECT_STREAM_ACTOR));
        Self {
            player,
            enemies,
            player_dice,
            enemy_dice,
            wear_dice,
            effect_dice,
            seed,
        }
    }

    /// Adds an actor to the encounter at runtime (`SpawnCityGuard`,
    /// `PlayerEntity.cs:747-762`): the fighter is re-projected once (armour can
    /// never start stale) and gets its own attack stream
    /// `actor_seed(seed, id)`, the same rule [`CombatSession::new`] applies.
    /// Returns the index the `*_strike(index)` methods address.
    pub fn spawn_enemy(&mut self, mut fighter: Fighter) -> usize {
        fighter.refresh_equipment();
        let dice = Dice100::with_seed(actor_seed(self.seed, fighter.combatant.id));
        self.enemies.push(fighter);
        self.enemy_dice.push(dice);
        self.enemies.len() - 1
    }

    /// The player attacks enemy `enemy_index` (`FormulaHelper.CalculateAttackDamage`
    /// weapon/H2H branch, `FormulaHelper.cs:538-828`).
    ///
    /// `None` when the index does not exist, the enemy is dead, or the player
    /// is dead — and then **0 rolls** are consumed.
    pub fn player_strike(&mut self, enemy_index: usize) -> Option<PlayerStrike> {
        let enemy = self.enemies.get(enemy_index)?;
        if !enemy.is_alive() || !self.player.is_alive() {
            return None;
        }
        // Armour always comes from the equipment (never a stale vector).
        self.player.refresh_equipment();
        self.enemies[enemy_index].refresh_equipment();

        let Self {
            player,
            enemies,
            player_dice,
            wear_dice,
            effect_dice,
            ..
        } = self;
        let attack = {
            let target = &enemies[enemy_index];
            let input = AttackInput {
                attacker: &player.combatant,
                target: &target.combatant,
                weapon: player.weapon.as_ref(),
                chance_mod: player.chance_mod,
                damage_mods: player.damage_mods,
                backstab_level: player.backstab_level,
                enemy_bonus: player.enemy_bonus,
            };
            resolve_attack(&input, player_dice)
        };

        // M21 §4 boundary: DFU's only call site is guarded by
        // `if (weapon != null && damage > 0)` (`FormulaHelper.cs:1090`, sole
        // caller `:701`) — an **unarmed** strike never wears the attacker's
        // weapon (there is none) nor the target's armour. Weapon FIRST
        // (`:1093`), then the target's worn piece (`:1094-1117`).
        let (target_wear, weapon_wear) = match (attack.damage > 0, player.weapon_slot) {
            (true, Some(slot)) => {
                let weapon_wear = player.equipment.damage_item(slot, attack.damage, wear_dice);
                let target = &mut enemies[enemy_index];
                let target_wear =
                    target
                        .equipment
                        .damage_equipment(attack.part, attack.damage, wear_dice);
                (target_wear, weapon_wear)
            }
            _ => (None, None),
        };

        // `SetHealth(before - damage)` with the clamp and the `OnDeath` latch
        // (`DaggerfallEntity.cs:331-338`, `:877-880`).
        let target = &mut enemies[enemy_index];
        if attack.damage > 0 {
            let before = target.combatant.health;
            let after = target.death.apply_set_health(
                target.combatant.max_health,
                before - attack.damage,
                false,
            );
            target.combatant.health = after;
        }
        // A broken piece must leave `armor_values` / the weapon view.
        target.refresh_equipment();
        player.refresh_equipment();

        // M23: the item callbacks of the strike (`HealthLeech`, `Vampiric`,
        // `MaceOfMolagBal`, a struck artifact), on the effect entities.
        let enchantment = run_strike_enchantment(player, target, attack.damage, effect_dice);

        Some(PlayerStrike {
            attack,
            enchantment,
            target_health: target.combatant.health,
            target_died: !target.is_alive(),
            target_wear,
            weapon_wear,
        })
    }

    /// Enemy `enemy_index` attacks the player.
    ///
    /// The monster branch (an **unarmed** `EnemyMonster` with `monster_bands`)
    /// runs [`resolve_monster_attack`] — up to three reflex-gated bands
    /// (`FormulaHelper.cs:636-672`); every other attacker resolves one
    /// [`resolve_attack`]. `None` on the same guards as [`Self::player_strike`],
    /// consuming 0 rolls. The M23 item callbacks run with the same composition
    /// as [`Self::player_strike`] (see [`run_strike_enchantment`]): DFU emits
    /// the enemy-side `Strikes` from `EnemyAttack.cs:262-268` (player target)
    /// and `:377-383` (non-player target), and the defender's worn artifact
    /// runs out of band from the damage formula (`FormulaHelper.cs:703-717`).
    pub fn enemy_strike(&mut self, enemy_index: usize) -> Option<EnemyStrike> {
        let enemy = self.enemies.get(enemy_index)?;
        if !enemy.is_alive() || !self.player.is_alive() {
            return None;
        }
        self.player.refresh_equipment();
        self.enemies[enemy_index].refresh_equipment();

        let Self {
            player,
            enemies,
            enemy_dice,
            wear_dice,
            effect_dice,
            ..
        } = self;
        let (multi, single) = {
            let attacker = &enemies[enemy_index];
            // DFU `:554-571`: an equipped `EnemyEntity` keeps its weapon only
            // while the weapon's average damage is at least the first band's;
            // when the weaponless attack is more damaging, `weapon` is nulled
            // and the band branch below runs. Unarmed monsters always take it.
            let monster = attacker.combatant.kind.is_monster()
                && match (attacker.weapon.as_ref(), attacker.monster_bands) {
                    (None, _) => true,
                    (Some(weapon), Some(bands)) => {
                        let (min, max) = bands[0];
                        (min + max) / 2 > (weapon.min_damage + weapon.max_damage) / 2
                    }
                    (Some(_), None) => false,
                };
            match attacker.monster_bands {
                Some(bands) if monster => {
                    let input = MonsterAttackInput {
                        attacker: &attacker.combatant,
                        target: &player.combatant,
                        damage_bands: bands,
                        chance_mod: attacker.chance_mod,
                        enemy_bonus: attacker.enemy_bonus,
                        // The TARGET's reflexes feed the multi-hit gate (`:654-657`).
                        player_reflexes: player.reflexes,
                    };
                    let outcome = resolve_monster_attack(&input, &mut enemy_dice[enemy_index]);
                    (Some(outcome), None)
                }
                _ => {
                    let input = AttackInput {
                        attacker: &attacker.combatant,
                        target: &player.combatant,
                        weapon: attacker.weapon.as_ref(),
                        chance_mod: attacker.chance_mod,
                        damage_mods: attacker.damage_mods,
                        backstab_level: attacker.backstab_level,
                        enemy_bonus: attacker.enemy_bonus,
                    };
                    let outcome = resolve_attack(&input, &mut enemy_dice[enemy_index]);
                    (None, Some(outcome))
                }
            }
        };
        let (part, damage) = match (&multi, &single) {
            (Some(outcome), _) => (outcome.part, outcome.damage),
            (None, Some(outcome)) => (outcome.part, outcome.damage),
            // Unreachable: the match above always fills exactly one side.
            (None, None) => (0, 0),
        };

        // Same DFU guard and order as the player side
        // (`if (weapon != null && damage > 0)`, `FormulaHelper.cs:1090`): a
        // monster is unarmed by construction (`:657`), so its multi-hit branch
        // wears neither its weapon (none) nor the player's armour.
        let (target_wear, weapon_wear) = match (damage > 0, enemies[enemy_index].weapon_slot) {
            (true, Some(slot)) => {
                let attacker = &mut enemies[enemy_index];
                let weapon_wear = attacker.equipment.damage_item(slot, damage, wear_dice);
                let target_wear = player.equipment.damage_equipment(part, damage, wear_dice);
                (target_wear, weapon_wear)
            }
            _ => (None, None),
        };

        if damage > 0 {
            let before = player.combatant.health;
            let after = player.death.apply_set_health(
                player.combatant.max_health,
                before - damage,
                false,
            );
            player.combatant.health = after;
        }
        enemies[enemy_index].refresh_equipment();
        player.refresh_equipment();

        // M23: the item callbacks of the strike, the same composition as
        // `player_strike` with the enemy as bearer — its weapon's `Strikes`
        // payload and the (player) defender's worn artifact `Struck` payload,
        // whose request lands on the AI attacker
        // (`FormulaHelper.cs:703-717`). The write-back is `SetHealth` on both
        // effect entities (`DaggerfallEntity.cs:331-338`), inside
        // `run_strike_enchantment`.
        let enchantment =
            run_strike_enchantment(&mut enemies[enemy_index], player, damage, effect_dice);

        Some(EnemyStrike {
            multi,
            single,
            part,
            damage,
            target_health: player.combatant.health,
            target_died: !player.is_alive(),
            target_wear,
            enchantment,
            weapon_wear,
        })
    }

    /// `Range(1500, 3001)` on the ENEMY actor PRNG
    /// (`EnemyAttack.cs:114-132`), for the caller's
    /// [`daggerfall_combat::EnemyAttack::reset_melee_timer`]. `None` when the
    /// index does not exist.
    pub fn enemy_timer_roll(&mut self, enemy_index: usize) -> Option<i32> {
        if enemy_index >= self.enemies.len() {
            return None;
        }
        self.enemy_dice
            .get_mut(enemy_index)
            .map(|dice| dice.roll_range(1500, 3000))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use daggerfall_behavior::magic::{effect_by_key, Request};
    use daggerfall_combat::EntityKind;
    use daggerfall_inventory::items::{apply_armor_material, apply_weapon_material, ItemTemplate};

    /// Steel Longsword (template 120, material 1). `hitPoints` is the base the
    /// material table scales (`ItemBuilder.cs:51`): `800 · COND_MUL[1] / 4`
    /// = the M21-measured 1200.
    fn longsword() -> Item {
        let mut template = ItemTemplate::empty(120);
        template.hit_points = 800;
        let mut item = Item::from_template(&template);
        apply_weapon_material(&mut item, 1);
        item
    }

    /// Steel plate cuirass (template 102, `ArmorMaterialTypes` `0x0201`):
    /// `100 · COND_MUL[1] / 4` = the 150 of GV-ARM-04.
    fn cuirass() -> Item {
        let mut template = ItemTemplate::empty(102);
        template.hit_points = 100;
        let mut item = Item::from_template(&template);
        apply_armor_material(&mut item, 0x0201);
        item
    }

    fn fighter(id: u32, kind: EntityKind) -> Fighter {
        Fighter::new(Combatant::new(id, kind), DeathSequence::for_enemy())
    }

    #[test]
    fn weapon_view_comes_from_the_retail_tables() {
        let weapon = weapon_from_item(&longsword()).expect("template 120 is a Longsword");
        assert_eq!(weapon.min_damage, 2, "CalculateWeaponMinDamage(Longsword)");
        assert_eq!(weapon.max_damage, 16, "CalculateWeaponMaxDamage(Longsword)");
        assert_eq!(weapon.native_material_value, 1, "steel");
        assert_eq!(weapon.material_damage_mod, 0, "steel modifier");
        assert_eq!(
            weapon.to_hit_mod,
            TO_HIT_MOD[1],
            "GetWeaponMaterialModifier * 10"
        );
        // Iron Longsword: modifier −1 to damage, −10 to hit.
        let mut iron = ItemTemplate::empty(120);
        iron.hit_points = 1200;
        let mut iron_item = Item::from_template(&iron);
        apply_weapon_material(&mut iron_item, 0);
        let iron = weapon_from_item(&iron_item).expect("iron Longsword");
        assert_eq!(iron.material_damage_mod, -1);
        assert_eq!(iron.to_hit_mod, -10);
    }

    #[test]
    fn non_weapons_and_zero_damage_templates_have_no_view() {
        assert!(
            weapon_from_item(&cuirass()).is_none(),
            "an armour piece is not a weapon"
        );
        // Arrow = template 131: group Weapons, CalculateWeaponMaxDamage default 0.
        let arrow = ItemTemplate::empty(131);
        assert!(weapon_from_item(&Item::from_template(&arrow)).is_none());
    }

    #[test]
    fn refresh_after_a_break_drops_the_bonus_and_the_weapon() {
        let mut fighter = fighter(1, EntityKind::Player);
        fighter.equip(cuirass());
        fighter.equip(longsword());
        assert_eq!(
            fighter.combatant.armor_values[3],
            PLAYER_ARMOR_BASE - 45,
            "plate 0x0201 · 9 · 5 subtracted from the 100 seed (SerializablePlayer.cs:355-366)"
        );
        assert_eq!(fighter.weapon_slot, Some(EquipSlot::RightHand));
        assert!(fighter.weapon.is_some());

        let mut dice = WearDice::new(7);
        let armour = fighter
            .equipment
            .damage_item(EquipSlot::ChestArmor, 2000, &mut dice)
            .expect("the cuirass is worn");
        assert!(armour.broke, "200 > 150 condition ⇒ ItemBreaks");
        fighter.refresh_equipment();
        assert_eq!(
            fighter.combatant.armor_values[3], PLAYER_ARMOR_BASE,
            "bonus left with the piece ⇒ the 100 seed again"
        );
        assert!(fighter.equipment.get(EquipSlot::ChestArmor).is_none());

        let weapon = fighter
            .equipment
            .damage_item(EquipSlot::RightHand, 20_000, &mut dice)
            .expect("the Longsword is worn");
        assert!(weapon.broke);
        fighter.refresh_equipment();
        assert!(fighter.weapon.is_none(), "a broken weapon leaves the view");
        assert_eq!(fighter.weapon_slot, None);
    }

    #[test]
    fn guards_consume_no_rolls() {
        let mut dead_enemy = fighter(2, EntityKind::EnemyClass);
        dead_enemy.combatant.health = 0;
        let mut session = CombatSession::new(fighter(1, EntityKind::Player), vec![dead_enemy], 11);
        assert!(session.player_strike(0).is_none(), "dead enemy ⇒ None");
        assert!(session.player_strike(9).is_none(), "no such enemy ⇒ None");
        assert!(session.enemy_strike(0).is_none(), "dead enemy ⇒ None");
        let mut reference = CombatSession::new(
            fighter(1, EntityKind::Player),
            vec![fighter(2, EntityKind::EnemyClass)],
            11,
        );
        assert_eq!(
            session.player_dice.roll(),
            reference.player_dice.roll(),
            "the guards consumed no player roll"
        );

        // A dead player refuses both directions as well.
        let mut player = fighter(1, EntityKind::Player);
        player.combatant.health = 0;
        let mut session = CombatSession::new(player, vec![fighter(2, EntityKind::EnemyClass)], 11);
        assert!(session.player_strike(0).is_none(), "dead player ⇒ None");
        assert!(session.enemy_strike(0).is_none(), "dead player ⇒ None");
        let mut reference = CombatSession::new(
            fighter(1, EntityKind::Player),
            vec![fighter(2, EntityKind::EnemyClass)],
            11,
        );
        assert_eq!(
            session.enemy_dice[0].roll(),
            reference.enemy_dice[0].roll(),
            "the guards consumed no enemy roll"
        );
    }

    /// A player wearing `Ring_of_Namira` — its `Struck` row armed as the
    /// defender artifact (M24 would resolve it from the item) — is hit by an
    /// enemy: the reflect request lands on the **attacker** (DFU
    /// `FormulaHelper.cs:703-717` fires the defender artifact out of band when
    /// the struck entity is the player, with `targetEntity: AIAttacker`; the
    /// enemy strike itself is emitted from `EnemyAttack.cs:262-268`).
    #[test]
    fn enemy_strike_runs_the_defenders_struck_artifact_at_the_attacker() {
        let mut player = fighter(1, EntityKind::Player);
        // A hardy player survives the bounded retaliation loop.
        player.combatant.max_health = 10_000;
        player.combatant.health = 10_000;
        let enemy = fighter(2, EntityKind::EnemyClass)
            .with_effects(EffectTarget::new(100, 0, [0; 8]), None);
        // The ring is the defender's worn artifact row (M24 would resolve it
        // from the item; here the row is attached directly to prove the
        // wiring) — `with_effects` arms both halves of the fighter.
        let player = player.with_effects(
            EffectTarget::new(10_000, 0, [0; 8]),
            Some(effect_by_key("Ring_of_Namira").expect("registered row")),
        );
        let mut session = CombatSession::new(player, vec![enemy], 11);
        // Bounded loop until the enemy lands a hit (the DEMO-001 fight block
        // uses the same pattern, `m18_demo.rs:2433`).
        let mut landed = None;
        for _ in 0..256 {
            if let Some(strike) = session.enemy_strike(0) {
                if strike.damage > 0 {
                    landed = Some(strike);
                    break;
                }
            }
        }
        let strike = landed.expect("the enemy lands a strike within the loop");
        let report = strike.enchantment.expect("the defender artifact composed");
        assert_eq!(
            session.enemies[0]
                .effects
                .as_ref()
                .expect("armed")
                .request_count(Request::ReflectDamage),
            1,
            "the reflect request lands on the attacker (the enemy)"
        );
        assert_eq!(
            session.player
                .effects
                .as_ref()
                .expect("armed")
                .request_count(Request::ReflectDamage),
            0,
            "the wearer carries no request of its own artifact"
        );
        assert_eq!(
            report.attacker.len(),
            1,
            "the report exposes exactly the attacker-side request"
        );
    }

    /// The enemy's enchanted weapon runs its `Strikes` payload on the enemy
    /// side and the moved health is written back through `SetHealth` — the
    /// same `run_strike_enchantment` write-back as `player_strike`
    /// (`VampiricEffect.cs:112-128`: the bearer heals the strike damage).
    #[test]
    fn enemy_strike_runs_the_attackers_weapon_payload_with_write_back() {
        let mut player = fighter(1, EntityKind::Player);
        player.combatant.max_health = 10_000;
        player.combatant.health = 10_000;
        let mut enemy = fighter(2, EntityKind::EnemyClass)
            .with_effects(EffectTarget::new(100, 0, [0; 8]), None);
        enemy.weapon_effects = Some(effect_by_key("VampiricEffect").expect("registered row"));
        // Headroom so the 100→100+damage heal is visible, not clamped.
        enemy.combatant.max_health = 10_000;
        let mut session = CombatSession::new(
            player.with_effects(EffectTarget::new(10_000, 0, [0; 8]), None),
            vec![enemy],
            11,
        );
        let mut landed = None;
        for _ in 0..256 {
            if let Some(strike) = session.enemy_strike(0) {
                if strike.damage > 0 {
                    landed = Some(strike);
                    break;
                }
            }
        }
        let strike = landed.expect("the enemy lands a strike within the loop");
        let report = strike.enchantment.expect("the weapon payload composed");
        assert_eq!(
            session.enemies[0].combatant.health,
            100 + strike.damage,
            "the bearer heals the resolved damage (write-back SetHealth)"
        );
        assert!(!report.attacker.is_empty(), "the heal is in the report");
        assert!(
            report.defender.is_empty(),
            "an unarmed player carries no target payload"
        );
    }
}
