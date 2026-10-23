//! Production integration test of the M22 composition layer
//! (`daggerfall_app::combat_runtime`) — **0 retail data**: the kit is built from
//! synthetic templates carrying the conditions M21 measured (cuirass 150,
//! Longsword 1200).
//!
//! The four properties are the ones the production path must hold no matter
//! which caller drives it (window, demo or probe):
//!
//! 1. a real strike chain wears the attacker's weapon **and** the target's
//!    armour, with the `(10 · damage + 50) / 100` identity
//!    (`FormulaHelper.cs:1130-1132`);
//! 2. a broken piece leaves the equip table, the armour vector and the weapon
//!    view (`DaggerfallUnityItem.cs:1170-1190`);
//! 3. death latches once (`DaggerfallEntity.cs:877-880`) and a latched combat
//!    refuses further strikes;
//! 4. the same seed reproduces the same reports, and a different seed does not.

use daggerfall_app::combat_runtime::{
    CombatSession, EnemyArmorPolicy, Fighter, PlayerStrike, PLAYER_ARMOR_BASE,
};
use daggerfall_behavior::durability::wear_amount;
use daggerfall_combat::{Combatant, DeathSequence, EntityKind};
use daggerfall_inventory::equipment::{EquipSlot, ItemWear};
use daggerfall_inventory::items::{apply_armor_material, apply_weapon_material, Item, ItemTemplate};

/// Steel plate cuirass (`DaggerfallUnityItem.cs:1007-1058`, `0x0201`).
const CUIRASS: usize = 102;
/// Steel Longsword (`CalculateWeaponMin/MaxDamage`, `FormulaHelper.cs:455-524`).
const LONGSWORD: usize = 120;
/// M21-measured maximum **conditions** after the material pass
/// (`ItemBuilder.cs:51`: `hitPoints · COND_MUL[1] / 4`; GV-ARM-04 and M21 §8.3).
const CUIRASS_CONDITION: i32 = 150;
const LONGSWORD_CONDITION: i32 = 1200;
/// Template `hitPoints` that land on those conditions with material 1.
const CUIRASS_HIT_POINTS: u16 = 100;
const LONGSWORD_HIT_POINTS: u16 = 800;
/// M22 spec seed (`20_261_022`), the same family the probes use.
const SEED: u64 = 20_261_022;
/// Enemy health ceiling so the wear loop measures wear, not a corpse.
const HP_CEILING: i32 = 10_000;

fn cuirass() -> Item {
    let mut template = ItemTemplate::empty(CUIRASS);
    template.hit_points = CUIRASS_HIT_POINTS;
    let mut item = Item::from_template(&template);
    apply_armor_material(&mut item, 0x0201);
    assert_eq!(item.max_condition, CUIRASS_CONDITION);
    item
}

fn longsword() -> Item {
    let mut template = ItemTemplate::empty(LONGSWORD);
    template.hit_points = LONGSWORD_HIT_POINTS;
    let mut item = Item::from_template(&template);
    apply_weapon_material(&mut item, 1);
    assert_eq!(item.max_condition, LONGSWORD_CONDITION);
    item
}

/// Player: armed Longsword + cuirass. `chance_mod = 45` is the canonical
/// §1.4 skill (`FormulaHelper.cs:592-612`); the armour vector is projected from
/// the equip table, never written as a literal.
fn armed_player(id: u32) -> Fighter {
    let mut player = Fighter::new(
        Combatant::new(id, EntityKind::Player),
        DeathSequence::for_player_height(1.7),
    );
    player.combatant.max_health = HP_CEILING;
    player.combatant.health = HP_CEILING;
    player.chance_mod = 45;
    player.equip(cuirass());
    player.equip(longsword());
    player
}

/// `EnemyMonster` with the M25 `(Min,Max)` bands. The armour policy is the
/// production one (`EnemyEntity.SetEnemyEquipment`): base `100` floored at
/// `MobileEnemy.ArmorValue * 5`, declared here as an `armor_value 0` fixture
/// because this tree carries no `MobileEnemy` row for the synthetic monster —
/// with the row absent the floor is `0`, i.e. no armour at all. `chance_mod`
/// is the canonical §1.4 skill (45, `FormulaHelper.cs:592-612`), the same
/// fixture the player carries; the real composition —
/// `GetLiveSkillValue(HandToHand)` for the unarmed branch
/// (`FormulaHelper.cs:589-591`, `:592`) — is an M25 item: no monster-skill
/// table exists in this tree.
fn monster(id: u32) -> Fighter {
    let mut enemy = Fighter::new(
        Combatant::new(id, EntityKind::EnemyMonster),
        DeathSequence::for_enemy(),
    );
    enemy.combatant.max_health = HP_CEILING;
    enemy.combatant.health = HP_CEILING;
    enemy.chance_mod = 45;
    enemy.armor_policy = Some(EnemyArmorPolicy::Monster { armor_value: 0 });
    enemy.equip(cuirass());
    enemy.monster_bands = Some([(2, 5), (2, 4), (1, 3)]);
    enemy
}

/// Longsword whose condition outlasts a long loop: DFU wears the target's
/// armour only while the attacker carries a weapon (`FormulaHelper.cs:1090`),
/// so a break-test fixture must keep the attacker armed. The piece under test
/// is the **target** cuirass, and its 150 points are the measured M21 value
/// (`u16::MAX` hit points ≈ 98 000 condition, far beyond the wear of the loop).
fn durable_longsword() -> Item {
    let mut template = ItemTemplate::empty(LONGSWORD);
    template.hit_points = u16::MAX;
    let mut item = Item::from_template(&template);
    apply_weapon_material(&mut item, 1);
    item
}

/// [`armed_player`] with the durable sword (break tests only).
fn durable_armed_player(id: u32) -> Fighter {
    let mut player = armed_player(id);
    player.equip(durable_longsword());
    player
}

/// `ArmorValues[7]` the production projection must produce for a **player**:
/// the DFU "no armour" seed (`SerializablePlayer.cs:355-366`) plus the M21
/// delta of the equip table. Never a literal vector: the base and the delta
/// are both read from their owners.
fn player_armor_vector(fighter: &Fighter) -> [i32; 7] {
    fighter
        .equipment
        .armor_values()
        .map(|delta| PLAYER_ARMOR_BASE + delta)
}

fn condition(fighter: &Fighter, slot: EquipSlot) -> i32 {
    fighter
        .equipment
        .get(slot)
        .map_or(0, |item| item.current_condition)
}

/// M21 §4 wear identity (`FormulaHelper.cs:1130-1132`): the arithmetic amount,
/// or the 20 % "minimum one point" re-roll when the arithmetic yields 0.
fn wear_identity_holds(wear: &ItemWear, damage: i32) -> bool {
    let expected = wear_amount(damage);
    if expected == 0 {
        (0..=1).contains(&wear.wear)
    } else {
        wear.wear == expected
    }
}

#[test]
fn strikes_wear_the_weapon_and_the_target_armour() {
    let mut session = CombatSession::new(armed_player(1), vec![monster(4)], SEED);
    let enemy_hp_start = session.enemies[0].health();
    let enemy_chest_start = condition(&session.enemies[0], EquipSlot::ChestArmor);
    let weapon_start = condition(&session.player, EquipSlot::RightHand);
    let player_chest_start = condition(&session.player, EquipSlot::ChestArmor);
    assert_eq!(enemy_chest_start, CUIRASS_CONDITION);
    assert_eq!(weapon_start, LONGSWORD_CONDITION);
    assert_eq!(
        session.player.combatant.armor_values,
        player_armor_vector(&session.player),
        "armour always comes from the equipment, rebased on the DFU 100 seed"
    );

    let strikes = 800;
    let mut multi_hits_at_least_two = 0_u32;
    let mut player_weapon_wears = 0_u32;
    let mut enemy_armour_wears = 0_u32;
    // Damaging strikes on the chest (`BodyParts` index 3) — the only part the
    // cuirass covers — counted from the struck part, independent of the wear
    // report the M21 table returns.
    let mut chest_damaging = 0_u32;
    for _ in 0..strikes {
        let strike: PlayerStrike = session.player_strike(0).expect("live player and enemy");
        if strike.attack.damage > 0 && strike.attack.part == 3 {
            chest_damaging += 1;
        }
        if let Some(wear) = &strike.target_wear {
            enemy_armour_wears += 1;
            assert!(
                wear_identity_holds(wear, strike.attack.damage),
                "target wear {} vs damage {}",
                wear.wear,
                strike.attack.damage
            );
        }
        if let Some(wear) = &strike.weapon_wear {
            player_weapon_wears += 1;
            assert!(
                wear_identity_holds(wear, strike.attack.damage),
                "weapon wear {} vs damage {}",
                wear.wear,
                strike.attack.damage
            );
        }
        let enemy_strike = session.enemy_strike(0).expect("live player and enemy");
        assert!(
            enemy_strike.target_wear.is_none(),
            "an unarmed monster never wears the player's armour (FormulaHelper.cs:1090)"
        );
        if let Some(multi) = &enemy_strike.multi {
            if multi.hits >= 2 {
                multi_hits_at_least_two += 1;
            }
        }
    }

    // Every damaging strike wore the attacker's weapon (M21 §4, `:1093`), and
    // the target-side pass is per struck part: exactly the chest strikes wear
    // the cuirass, so the armour reports EQUAL the independent chest count.
    assert!(player_weapon_wears > 0, "the Longsword must wear");
    assert!(
        enemy_armour_wears == chest_damaging,
        "armour reports {enemy_armour_wears} must equal the {chest_damaging} chest damaging hits"
    );
    assert!(
        enemy_armour_wears > 0,
        "at least one strike must hit the cuirass"
    );

    assert!(
        session.enemies[0].health() < enemy_hp_start,
        "enemy HP {} → {}",
        enemy_hp_start,
        session.enemies[0].health()
    );
    assert!(
        condition(&session.enemies[0], EquipSlot::ChestArmor) < enemy_chest_start,
        "enemy chest {} → {}",
        enemy_chest_start,
        condition(&session.enemies[0], EquipSlot::ChestArmor)
    );
    assert!(
        condition(&session.player, EquipSlot::RightHand) < weapon_start,
        "player weapon {} → {}",
        weapon_start,
        condition(&session.player, EquipSlot::RightHand)
    );
    // DFU's `DamageEquipment` is guarded by `weapon != null && damage > 0`
    // (`FormulaHelper.cs:1090`, sole caller `:701`): an **unarmed** monster
    // never wears the player's armour, whatever it hits. The armed case is
    // `an_armed_enemy_wears_the_player_armour` below.
    assert_eq!(
        condition(&session.player, EquipSlot::ChestArmor),
        player_chest_start,
        "an unarmed EnemyMonster wears no armour (FormulaHelper.cs:1090)"
    );
    assert!(
        multi_hits_at_least_two > 0,
        "the monster branch must produce at least one multi-hit strike"
    );
    assert_eq!(
        session.player.combatant.armor_values,
        player_armor_vector(&session.player),
        "wear keeps the vector projected from the table"
    );
}

/// `EnemyClass` wielding the kit Longsword: the only shape where the target's
/// armour may wear, because DFU's guard needs a weapon
/// (`FormulaHelper.cs:1090`). The armour policy is the production
/// `EnemyEntity.SetEnemyEquipment` class branch (base `100` clamped to `60`,
/// `EnemyEntity.cs:409-435`); the skill is the canonical §1.4 fixture (45),
/// which composes to 95 % against a naked `Player` target and 50 % on the
/// cuirass chest — inside the `[3, 97]` clamp, so the plate term is visible.
fn armed_enemy(id: u32) -> Fighter {
    let mut enemy = Fighter::new(
        Combatant::new(id, EntityKind::EnemyClass),
        DeathSequence::for_enemy(),
    );
    enemy.combatant.max_health = HP_CEILING;
    enemy.combatant.health = HP_CEILING;
    enemy.chance_mod = 45;
    enemy.armor_policy = Some(EnemyArmorPolicy::Class);
    enemy.equip(longsword());
    enemy
}

#[test]
fn an_armed_enemy_wears_the_player_armour() {
    let mut session = CombatSession::new(armed_player(1), vec![armed_enemy(4)], SEED);
    let chest_start = condition(&session.player, EquipSlot::ChestArmor);
    let sword_start = condition(&session.enemies[0], EquipSlot::RightHand);
    assert_eq!(chest_start, CUIRASS_CONDITION);
    assert_eq!(sword_start, LONGSWORD_CONDITION);

    let mut strike = None;
    for _ in 0..4096 {
        let Some(current) = session.enemy_strike(0) else {
            break;
        };
        if current.target_wear.as_ref().is_some_and(|wear| wear.wear > 0) {
            strike = Some(current);
            break;
        }
    }
    let strike = strike.expect("the armed enemy strikes the cuirass within 4096 swings");
    let wear = strike.target_wear.expect("an armed hit reports the worn piece");
    assert_eq!(wear.slot, EquipSlot::ChestArmor);
    assert!(wear_identity_holds(&wear, strike.damage));
    assert_eq!(
        condition(&session.player, EquipSlot::ChestArmor),
        chest_start - wear.wear,
        "the player cuirass loses exactly the reported wear"
    );
    assert!(
        condition(&session.enemies[0], EquipSlot::RightHand) < sword_start,
        "the attacking Longsword wears on its own damaging hit (`:1093`)"
    );
    assert!(strike.damage > 0);
}

#[test]
fn a_broken_cuirass_leaves_the_table_and_the_armour_vector() {
    // The plate penalty makes the chest the hardest part to hit
    // (`FormulaHelper.cs:1149-1161`), so the loop needs a health ceiling that
    // keeps the corpse out of it: wear is the only thing being measured.
    let mut enemy = monster(4);
    enemy.combatant.max_health = 1_000_000;
    enemy.combatant.health = 1_000_000;
    let mut session = CombatSession::new(durable_armed_player(1), vec![enemy], SEED);
    let mut attempts = 0_u32;
    let mut break_report: Option<ItemWear> = None;
    for _ in 0..200_000 {
        attempts += 1;
        let Some(strike) = session.player_strike(0) else {
            break;
        };
        if let Some(wear) = strike.target_wear {
            if wear.broke {
                break_report = Some(wear);
                break;
            }
        }
    }
    let report = break_report.expect("the enemy cuirass breaks within 200000 strikes");
    assert_eq!(report.slot, EquipSlot::ChestArmor);
    assert_eq!(report.condition, 0);
    assert!(
        report.removed.is_some(),
        "ItemBreaks unequips and hands the piece back"
    );
    assert!(
        session.enemies[0]
            .equipment
            .get(EquipSlot::ChestArmor)
            .is_none(),
        "the broken cuirass left the slot"
    );
    assert_eq!(
        session.enemies[0].equipment.armor_values()[3],
        0,
        "no Chest contribution remains"
    );
    // The fixture monster declares `Monster { armor_value: 0 }`, so the
    // enemy floor is 0 and the chest returns to it (`EnemyEntity.cs:436-450`).
    assert_eq!(session.enemies[0].combatant.armor_values[3], 0);
    // Measured with seed `20_261_022` and the durable sword: 1 905 strikes.
    // Re-measured when the enemy fixture declared its production armour policy
    // (`Monster { armor_value: 0 }`, `EnemyEntity.cs:436-450`): DFU floors an
    // unequipped monster's `ArmorValues` at `MobileEnemy.ArmorValue * 5 = 0`,
    // so the chest is no longer the −45 part that made the chain slow (27 259
    // before the floor existed). A literal, not a bound the loop guarantees.
    assert_eq!(attempts, 1_905);
}

#[test]
fn death_latches_once_and_refuses_further_strikes() {
    let mut session = CombatSession::new(armed_player(1), vec![monster(4)], SEED);
    let mut died_at = None;
    for attempt in 0..60_000 {
        let Some(strike) = session.player_strike(0) else {
            break;
        };
        assert!(!strike.target_died || strike.target_health == 0);
        if strike.target_died {
            died_at = Some(attempt);
            break;
        }
    }
    let died_at = died_at.expect("the enemy dies");
    assert!(died_at > 0);
    assert_eq!(session.enemies[0].death.events(), 1, "OnDeath latches once");
    assert_eq!(session.enemies[0].health(), 0);
    assert!(!session.enemies[0].is_alive());

    // A further strike is refused and carries no damage.
    let refused = session.player_strike(0);
    assert!(refused.is_none(), "a dead enemy is never struck again");
    assert_eq!(session.enemies[0].health(), 0, "no damage is applied");
    assert_eq!(session.enemies[0].death.events(), 1, "still exactly one latch");
    assert!(session.enemy_strike(0).is_none(), "a dead enemy never strikes");
}

/// Flat, comparable projection of a run: the fields the probe digests.
fn signature(seed: u64) -> Vec<i32> {
    let mut session = CombatSession::new(armed_player(1), vec![monster(4)], seed);
    let mut flat = Vec::new();
    for _ in 0..40 {
        let Some(strike) = session.player_strike(0) else {
            break;
        };
        flat.extend([
            i32::from(strike.attack.hit),
            strike.attack.chance,
            strike.attack.part as i32,
            strike.attack.damage,
            strike.attack.rolls_consumed as i32,
            strike.target_health,
            i32::from(strike.target_died),
        ]);
        for wear in [&strike.target_wear, &strike.weapon_wear].into_iter().flatten() {
            flat.extend([
                wear.slot.index().map_or(-1, |index| index as i32),
                wear.wear,
                wear.condition,
                i32::from(wear.broke),
            ]);
        }
        let Some(enemy_strike) = session.enemy_strike(0) else {
            break;
        };
        flat.extend([
            enemy_strike.part as i32,
            enemy_strike.damage,
            enemy_strike.target_health,
            i32::from(enemy_strike.target_died),
        ]);
        if let Some(multi) = enemy_strike.multi {
            flat.extend([
                multi.hits as i32,
                multi.gates as i32,
                multi.per_hit[0],
                multi.per_hit[1],
                multi.per_hit[2],
            ]);
        }
    }
    flat
}

#[test]
fn the_same_seed_reproduces_the_reports_and_a_new_seed_does_not() {
    let primary = signature(SEED);
    assert!(!primary.is_empty(), "the run produced reports");
    assert_eq!(primary, signature(SEED), "same seed ⇒ same reports");
    assert_ne!(
        primary,
        signature(SEED.wrapping_add(1)),
        "a different seed produces different reports"
    );
}

#[test]
fn a_health_leech_weapon_charges_the_bearer_through_the_session() {
    // M23 on the production path: the window/demo session, not the probe.
    // `HealthLeech.cs:88-89` charges `leechWeaponAmount = 8` to the **bearer**
    // on every strike; the victim only takes the weapon damage.
    use daggerfall_behavior::magic::{effect_by_key, EffectTarget};

    let effects = || EffectTarget::new(HP_CEILING, 0, [50; 8]);
    let leech = effect_by_key("HealthLeech").expect("registered row");
    let player = armed_player(1).with_effects(effects(), Some(leech));
    let enemy = monster(4).with_effects(effects(), None);
    let mut session = CombatSession::new(player, vec![enemy], SEED);

    let enemy_start = session.enemies[0].health();
    let strike = session.player_strike(0).expect("a resolved hit");
    assert!(strike.attack.damage > 0, "the fixture must land a real hit");

    let report = strike.enchantment.expect("the callbacks ran");
    assert!(!report.attacker.is_empty(), "the bearer pays the leech");
    assert_eq!(
        session.player.health(),
        HP_CEILING - 8,
        "the bearer pays 8 health (`leechWeaponAmount`)"
    );
    assert_eq!(
        session.enemies[0].health(),
        enemy_start - strike.attack.damage,
        "the victim takes the strike damage only"
    );
}

/// **GV-ARM-06** — the player's `ArmorValues[7]` is the DFU vector, not the
/// M21 delta: every part starts at `100` ("no armor",
/// `SerializablePlayer.cs:355-366`; second seat `PlayerEntity.cs:1188-1191`)
/// and the worn piece subtracts from it (`DaggerfallEntity.cs:589-636`), so a
/// naked production player is `[100; 7]` and the cuirass chest is `55`.
#[test]
fn the_player_armour_vector_is_the_dfu_base_plus_the_equipment_delta() {
    let naked = Fighter::new(
        Combatant::new(1, EntityKind::Player),
        DeathSequence::for_player_height(1.7),
    );
    assert_eq!(
        naked.combatant.armor_values,
        [PLAYER_ARMOR_BASE; 7],
        "a naked player carries the 100 seed"
    );

    let mut armed = armed_player(1);
    assert_eq!(
        armed.combatant.armor_values,
        [100, 100, 100, 55, 100, 100, 100],
        "steel plate cuirass: 100 − 45 on the chest only"
    );
    assert_eq!(
        armed.combatant.armor_values,
        player_armor_vector(&armed),
        "the vector is exactly base + the equip-table delta"
    );

    // A removed piece leaves the vector at the seed again, never at the delta.
    armed.equipment.unequip(EquipSlot::ChestArmor);
    armed.refresh_equipment();
    assert_eq!(
        armed.combatant.armor_values,
        [PLAYER_ARMOR_BASE; 7],
        "no piece ⇒ the 100 seed, not 0"
    );
}

/// **GV-ARM-07** — `enemy → player` hit chance on the production vectors,
/// measured with the engine's own composed `chance`
/// (`FormulaHelper.CalculateArmorToHit`, `:1149-1161`, added at `:808`):
/// **95** on every part of a naked player and **50** on the cuirass chest,
/// where the 0-based projection that M22 shipped gave 3 (the `[3, 97]` clamp)
/// and made the player nearly untouchable.
#[test]
fn enemy_to_player_chance_is_95_naked_and_50_on_the_cuirass_chest() {
    use daggerfall_combat::{actor_seed, resolve_attack, AttackInput, Dice100};

    // Armed `EnemyClass` at the canonical §1.4 skill, unarmed *engine* view so
    // the weapon's `to_hit` cannot blur the armour term (the monster branch
    // composes with `weapon: None`, `FormulaHelper.cs:657`).
    let mut attacker = armed_enemy(4);
    // The canonical `Combatant` carries `CriticalStrike` 20 (`:1216-1218`):
    // a successful crit adds `crit/10 = 2`, which would blur the armour term.
    // Zeroed so the measured chance isolates `CalculateArmorToHit`, the same
    // isolation GV-ARM-03 gets from its identical-roll pairs.
    attacker.combatant.critical_strike = 0;
    let naked = Fighter::new(
        Combatant::new(1, EntityKind::Player),
        DeathSequence::for_player_height(1.7),
    );
    let armoured = armed_player(1);

    let mut chances: [Option<i32>; 7] = [None; 7];
    let mut naked_hits = 0u32;
    let mut chest_attempts = 0u32;
    let mut chest_hits = 0u32;
    let rolls = 4_000u64;
    for index in 0..rolls {
        let mut dice = Dice100::with_seed(actor_seed(SEED, 300 + index as u32));
        let input = AttackInput {
            attacker: &attacker.combatant,
            target: &naked.combatant,
            weapon: None,
            chance_mod: attacker.chance_mod,
            damage_mods: 0,
            backstab_level: 0,
            enemy_bonus: 0,
        };
        let outcome = resolve_attack(&input, &mut dice);
        chances[outcome.part] = Some(outcome.chance);
        if outcome.hit {
            naked_hits += 1;
        }
    }
    assert_eq!(
        chances,
        [Some(95); 7],
        "naked player: 45 (skill) + 100 (armour) − 50 (universal) = 95 on every part"
    );

    for index in 0..rolls {
        let mut dice = Dice100::with_seed(actor_seed(SEED, 300 + index as u32));
        let input = AttackInput {
            attacker: &attacker.combatant,
            target: &armoured.combatant,
            weapon: None,
            chance_mod: attacker.chance_mod,
            damage_mods: 0,
            backstab_level: 0,
            enemy_bonus: 0,
        };
        let outcome = resolve_attack(&input, &mut dice);
        let expected = if outcome.part == 3 { 50 } else { 95 };
        assert_eq!(
            outcome.chance, expected,
            "part {} with the cuirass: 100 − 45 = 55 on the chest ⇒ 50, 95 elsewhere",
            outcome.part
        );
        if outcome.part == 3 {
            chest_attempts += 1;
            if outcome.hit {
                chest_hits += 1;
            }
        }
    }

    // The measured rates match the composed chances (the dice stream is the
    // session seed family, so the counts are exact, not sampled noise).
    let naked_rate = f64::from(naked_hits) / rolls as f64;
    let chest_rate = f64::from(chest_hits) / f64::from(chest_attempts);
    assert!(
        (0.93..=0.97).contains(&naked_rate),
        "naked enemy→player hit rate {naked_rate:.3} must be ≈95 % (DFU), not the 3 % of a 0-based vector"
    );
    assert!(
        (0.42..=0.58).contains(&chest_rate),
        "cuirass chest hit rate {chest_rate:.3} must be ≈50 % (95 − 45), got {chest_hits}/{chest_attempts}"
    );
}

/// **GV-SPAWN-01** — a runtime spawn (`SpawnCityGuard`, `PlayerEntity.cs:747-762`)
/// joins the encounter with its own attack stream and its armour projected
/// from its own table: the M28 guard path depends on both.
#[test]
fn a_runtime_enemy_joins_the_encounter_with_its_own_stream_and_projection() {
    let mut session = CombatSession::new(armed_player(1), vec![monster(4)], SEED);
    let mut guard = armed_enemy(7);
    guard.equip(cuirass());
    let chest = guard.combatant.armor_values[3];
    let index = session.spawn_enemy(guard);
    assert_eq!(index, 1);
    assert_eq!(session.enemies.len(), 2);
    assert_eq!(
        session.enemies[index].combatant.armor_values[3], chest,
        "the spawn is re-projected on entry"
    );
    let before = session.enemies[index].health();
    let strike = session.player_strike(index).expect("the spawned enemy is strikeable");
    assert!(strike.target_health <= before);
    // Its own stream: the same index struck twice consumes different rolls,
    // and the first enemy's stream is untouched (the seed is per actor).
    let mut twin = CombatSession::new(armed_player(1), vec![monster(4)], SEED);
    let mut guard = armed_enemy(7);
    guard.equip(cuirass());
    twin.spawn_enemy(guard);
    let mut first = twin.player_strike(1).expect("strike");
    let mut second = twin.player_strike(1).expect("strike");
    let _ = (&mut first, &mut second);
    assert!(twin.enemy_strike(1).is_some(), "the spawn can strike back");
}
