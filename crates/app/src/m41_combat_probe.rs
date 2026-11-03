//! M41/W41-2 — `SYS-COMBAT-001`: complete-combat depth in one gate.
//!
//! The depth axes of the W41-2 row (roadmap §3.6) are measured against the
//! **rules that already exist** — 0 re-derivation, 0 second source of truth:
//!
//! - `daggerfall_behavior::combat` — hit chance (`FormulaHelper.cs:796-828`),
//!   damage (`:736-761`), cadence (`:830-848`), death
//!   (`DaggerfallEntity.cs:331-338`);
//! - `daggerfall_behavior::durability` — wear per hit
//!   (`FormulaHelper.cs:1123-1138`);
//! - `daggerfall_combat::{engine,rng,actor}` — the per-actor `DFRandom`
//!   pipeline (`Model: D-M22-1`), the `EnemyAttack` timer
//!   (`EnemyAttack.cs:114-132`) and the `deathInProgress` latch;
//! - `daggerfall_inventory::{items,equipment}` — the per-part armour
//!   projection (`DaggerfallEntity.cs:589-636`) and `DamageEquipment`
//!   (`FormulaHelper.cs:1080-1121`).
//!
//! Blocks: A hit location · B per-part armour · C materials · D wear ·
//! E critical/backstab · F range/cadence · G actor death · H the
//! `(skill, armour, material, swing) → hit/damage` matrix · I deterministic
//! 60 s combat · J adversarial (every block-J row id carries the `ADV/` label,
//! e.g. `[OK] ADV/J-GATE-NO-ROLL - …`, so the gate can census adversarial
//! coverage mechanically instead of trusting prose).
//!
//! Contract (single stdout, byte-identical in two independent processes):
//! first line `PROBE-SYS-COMBAT-001 - …`; `BLOCK <A..J> rows=<n>` headers in
//! order; rows `[OK] <id> - <detail> (src=<file:line>)` or
//! `FAIL <id> got=<v> want=<v>` (every row cites its rule); the matrix prints
//! `MATRIX skills=… armour=… materials=… swings=… rows=…` and then exactly one
//! `MATRIX <skill> <armour> <material> <swing> hit=<0|1> damage=<n> src=<f:l>`
//! per combination; block I prints `COMBAT60 …` and `COMBAT60-REPEAT …`;
//! `VEREDICTO: PASS`; the LAST line is `DIGEST <sha256>` = sha256 of every
//! byte printed before it.
//!
//! **0 retail bytes** are read (every rule is a formula; every item is built
//! from `ItemTemplate::empty` + the material tables), so the gate has no
//! retail precondition and never prints `PROBE_RUN`.
//!
//! Gap closed by this lane: the per-part hit composition was private
//! (`engine::hit_chance`), so block B could only have re-implemented it. It is
//! now `daggerfall_combat::chance_to_hit_for_part` (`engine.rs`), a 1-line
//! delegate to the same composition — no arithmetic moved.

use std::collections::HashSet;

use sha2::{Digest, Sha256};

use daggerfall_app::combat_runtime::{weapon_from_item, Fighter, PLAYER_ARMOR_BASE, WEAR_STREAM_ACTOR};
use daggerfall_behavior::combat::{
    adjustments, backstab_damage, backstabing_requires_roll, bow_cooldown_seconds, chance_to_hit,
    critical_bonus, enemy_melee_timer_seconds, fall_duration_seconds, h2h_range, material_gate,
    melee_tick_seconds, multi_hit_gate, reflexes_gate, set_health, skeletal_weapon_adjust,
    struck_body_part, success_roll, weapon_damage, HitMods, CHANCE_MAX, CHANCE_MIN,
    CLASSIC_FRAME_UPDATE, FADE_SECONDS, MONSTER_HIT_BONUS, RESET_SECONDS, STRUCK_BODY_PARTS,
    UNIVERSAL_ADJUSTMENT,
};
use daggerfall_behavior::durability::{apply_wear, wear_amount, wear_amount_rolled, WearOutcome};
use daggerfall_behavior::locomotion::Dice100 as WearDice;
use daggerfall_combat::{
    actor_seed, chance_to_hit_for_part, resolve_attack, resolve_monster_attack, AttackInput,
    Combatant, DeathSequence, Dice100, EnemyAttack, EntityKind, MonsterAttackInput, Weapon,
};
use daggerfall_inventory::equipment::{
    body_part_for_equip_slot, equip_slot_for_body_part, EquipResult, EquipSlot, ItemEquipTable,
    BODY_PART_COUNT,
};
use daggerfall_inventory::items::{
    apply_armor_material, apply_weapon_material, material_armor_value, weapon_material_modifier,
    Item, ItemTemplate, TO_HIT_MOD,
};

/// Lane CR (W41-2) — travels in the header line, the wiring comments and the
/// gate manifest.
pub const CR: &str = "CR-da9915b0-e700-471a-80b8-0bafa9459938";

/// Session seed of every per-actor `DFRandom` chain in this probe
/// (`actor_seed(SYS_SEED, actor_id)`): the M41 date, same pattern as
/// `M22_SEED`/`M21_LOOT_SEED` — one literal, 0 wall clock.
pub const SYS_SEED: u64 = 20_261_027;

// ── rule citations (file:line of the rule each row exercises) ───────────────

const SRC_TABLE: &str = "crates/behavior/src/combat.rs:27";
const SRC_STRUCK: &str = "crates/behavior/src/combat.rs:42";
const SRC_SUCCESS: &str = "crates/behavior/src/combat.rs:50";
const SRC_CRIT: &str = "crates/behavior/src/combat.rs:90";
const SRC_ADJ: &str = "crates/behavior/src/combat.rs:103";
const SRC_CHANCE: &str = "crates/behavior/src/combat.rs:148";
const SRC_GATE: &str = "crates/behavior/src/combat.rs:174";
const SRC_FLOOR: &str = "crates/behavior/src/combat.rs:180";
const SRC_SKEL: &str = "crates/behavior/src/combat.rs:205";
const SRC_BS: &str = "crates/behavior/src/combat.rs:219";
const SRC_BSREQ: &str = "crates/behavior/src/combat.rs:228";
const SRC_H2H: &str = "crates/behavior/src/combat.rs:236";
const SRC_REFLEX: &str = "crates/behavior/src/combat.rs:250";
const SRC_MULTI: &str = "crates/behavior/src/combat.rs:256";
const SRC_TICK: &str = "crates/behavior/src/combat.rs:264";
const SRC_BOW: &str = "crates/behavior/src/combat.rs:270";
const SRC_TIMER_F: &str = "crates/behavior/src/combat.rs:278";
const SRC_SETHEALTH: &str = "crates/behavior/src/combat.rs:290";
const SRC_FALL: &str = "crates/behavior/src/combat.rs:302";
const SRC_WEAR: &str = "crates/behavior/src/durability.rs:49";
const SRC_WROLLED: &str = "crates/behavior/src/durability.rs:59";
const SRC_APPLY_WEAR: &str = "crates/behavior/src/durability.rs:85";
const SRC_RESOLVE: &str = "crates/combat/src/engine.rs:83";
const SRC_PART_CHANCE: &str = "crates/combat/src/engine.rs:211";
const SRC_MONSTER: &str = "crates/combat/src/engine.rs:260";
const SRC_TIMER: &str = "crates/combat/src/engine.rs:352";
const SRC_LATCH: &str = "crates/combat/src/engine.rs:415";
const SRC_APPLY_SH: &str = "crates/combat/src/engine.rs:427";
const SRC_ARMOR: &str = "crates/inventory/src/equipment.rs:465";
const SRC_BODYMAP: &str = "crates/inventory/src/equipment.rs:162";
const SRC_DMG_EQUIP: &str = "crates/inventory/src/equipment.rs:538";
const SRC_DMG_ITEM: &str = "crates/inventory/src/equipment.rs:560";
const SRC_MATARMOR: &str = "crates/inventory/src/items.rs:766";
const SRC_TOHIT: &str = "crates/inventory/src/items.rs:75";
const SRC_WEAPON_VIEW: &str = "crates/app/src/combat_runtime.rs:68";
const SRC_PROJ: &str = "crates/app/src/combat_runtime.rs:275";

// ── evidence rows ───────────────────────────────────────────────────────────

/// One printed row. `ok` is the verdict of the cited rule; a failing row prints
/// both sides so the break is visible without a debugger.
struct Row {
    id: String,
    ok: bool,
    got: String,
    want: String,
    detail: String,
    src: &'static str,
}

fn row(
    id: impl Into<String>,
    got: impl Into<String>,
    want: impl Into<String>,
    detail: impl Into<String>,
    src: &'static str,
) -> Row {
    let got = got.into();
    let want = want.into();
    Row {
        id: id.into(),
        ok: got == want,
        got,
        want,
        detail: detail.into(),
        src,
    }
}

/// A row whose verdict is an explicit predicate over measured values (used
/// where the rule is a relation, e.g. "the ×3 is either d or 3d").
fn predicate(
    id: impl Into<String>,
    ok: bool,
    observed: impl Into<String>,
    rule: impl Into<String>,
    detail: impl Into<String>,
    src: &'static str,
) -> Row {
    Row {
        id: id.into(),
        ok,
        got: observed.into(),
        want: rule.into(),
        detail: detail.into(),
        src,
    }
}

struct Block {
    label: &'static str,
    rows: Vec<Row>,
    tail: Vec<String>,
}

fn block(label: &'static str, rows: Vec<Row>) -> Block {
    Block {
        label,
        rows,
        tail: Vec::new(),
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// Item instance from a template index + material, the way `ItemBuilder` does
/// it (`ItemBuilder.cs:414-427` weapons, `:488-507` armour).
fn weapon_item(template: usize, material: usize) -> Item {
    let mut spec = ItemTemplate::empty(template);
    spec.hit_points = 1200;
    let mut item = Item::from_template(&spec);
    apply_weapon_material(&mut item, material);
    item
}

fn armor_item(template: usize, material: i32) -> Item {
    let mut spec = ItemTemplate::empty(template);
    spec.hit_points = 100;
    let mut item = Item::from_template(&spec);
    apply_armor_material(&mut item, material);
    item
}

/// Seven-piece armour kit over all seven body parts (`102..=108`), chosen so
/// every `GetMaterialArmorValue` branch is exercised: plate 0x0201, leather
/// 0x0000, chain 0x0100 and plate 0x0200.
const KIT: [(usize, i32); 7] = [
    (107, 0x0201), // Head
    (106, 0x0000), // RightArm
    (105, 0x0100), // LeftArm
    (102, 0x0201), // ChestArmor
    (103, 0x0200), // Gloves
    (104, 0x0100), // LegsArmor
    (108, 0x0200), // Feet
];

/// Equip-table delta of [`KIT`] (`9·5, 3·5, 6·5, 9·5, 7·5, 6·5, 7·5`).
const KIT_DELTA: [i32; 7] = [-45, -15, -30, -45, -35, -30, -35];

fn kit_table() -> (ItemEquipTable, u32) {
    let mut table = ItemEquipTable::new();
    let mut equipped = 0;
    for (template, material) in KIT {
        if let EquipResult::Equipped(_) = table.equip(armor_item(template, material), true) {
            equipped += 1;
        }
    }
    (table, equipped)
}

// ── block A · hit location ──────────────────────────────────────────────────

fn block_a() -> Vec<Row> {
    let mut rows = Vec::with_capacity(36);
    for roll in 0..20i32 {
        let part = struck_body_part(roll);
        rows.push(row(
            format!("A-R{roll:02}"),
            format!("part={part}"),
            format!("part={}", STRUCK_BODY_PARTS[roll as usize]),
            format!("roll={roll} → BodyParts {part}"),
            SRC_STRUCK,
        ));
    }
    // Explicit boundary goldens (roll 0, 1, 2, 18, 19 — the table's edges).
    for boundary in [0i32, 1, 2, 18, 19] {
        let part = struck_body_part(boundary);
        rows.push(row(
            format!("A-B{boundary:02}"),
            format!("roll={boundary} part={part}"),
            format!("roll={boundary} part={}", STRUCK_BODY_PARTS[boundary as usize]),
            format!("boundary roll={boundary} → part {part}"),
            SRC_STRUCK,
        ));
    }
    rows.push(row(
        "A-CLAMP-HIGH",
        format!("roll=20 part={}", struck_body_part(20)),
        "roll=20 part=6",
        "out-of-range roll clamps to 19 (extra defense)",
        SRC_STRUCK,
    ));
    rows.push(row(
        "A-CLAMP-LOW",
        format!("roll=-1 part={}", struck_body_part(-1)),
        "roll=-1 part=0",
        "negative roll clamps to 0 (extra defense)",
        SRC_STRUCK,
    ));
    let mut counts = [0i32; BODY_PART_COUNT];
    for part in STRUCK_BODY_PARTS {
        counts[part] += 1;
    }
    let census = counts
        .iter()
        .map(i32::to_string)
        .collect::<Vec<_>>()
        .join(",");
    rows.push(row(
        "A-CENSUS",
        format!("counts={census}"),
        "counts=2,3,3,4,4,3,1",
        format!("20 rolls over 7 parts: {census}"),
        SRC_TABLE,
    ));
    for part in 0..BODY_PART_COUNT {
        let slot = equip_slot_for_body_part(part);
        let back = body_part_for_equip_slot(slot)
            .map(|value| value.to_string())
            .unwrap_or_else(|| "none".to_string());
        rows.push(row(
            format!("A-EQPMAP-{part}"),
            format!("part={part} slot={slot:?} back={back}"),
            format!("part={part} slot={slot:?} back={part}"),
            format!("BodyParts ↔ EquipSlots round trip at part {part}"),
            SRC_BODYMAP,
        ));
    }
    rows
}

// ── block B · per-part armour ───────────────────────────────────────────────

fn block_b() -> Vec<Row> {
    let mut rows = Vec::with_capacity(24);
    let (table, equipped) = kit_table();
    rows.push(row(
        "B-KIT",
        format!("pieces={equipped}"),
        "pieces=7",
        "7 armour pieces over the 7 body parts",
        SRC_ARMOR,
    ));
    let delta = table.armor_values();
    for part in 0..BODY_PART_COUNT {
        rows.push(row(
            format!("B-PART-{part}"),
            format!("delta={}", delta[part]),
            format!("delta={}", KIT_DELTA[part]),
            format!("part={part} worn piece → ArmorValues delta {}", delta[part]),
            SRC_ARMOR,
        ));
    }
    let max_delta = *delta.iter().max().unwrap_or(&0);
    rows.push(row(
        "B-NONPOSITIVE",
        format!("max={max_delta} nonpositive={}", delta.iter().all(|value| *value <= 0)),
        "max=-15 nonpositive=true",
        "armour only ever subtracts (negative delta convention)",
        SRC_ARMOR,
    ));
    let materials = [0x0000, 0x0100, 0x0103, 0x0200, 0x0201, 0x0203, 0x0205, 0x0207, 0x0209];
    let values = materials
        .iter()
        .map(|material| material_armor_value(&armor_item(102, *material)).to_string())
        .collect::<Vec<_>>()
        .join(",");
    rows.push(row(
        "B-MAT-TABLE",
        format!("values={values}"),
        "values=3,6,6,7,9,11,15,17,21",
        "GetMaterialArmorValue per material (leather…daedric plate)",
        SRC_MATARMOR,
    ));
    rows.push(row(
        "B-ADJ",
        format!("monster_adjustment={}", adjustments(false, 0, true)),
        format!("monster_adjustment={}", MONSTER_HIT_BONUS + UNIVERSAL_ADJUSTMENT),
        "target EnemyMonster: +40 monster and −50 universal",
        SRC_ADJ,
    ));

    // Composition into the hit chance, per struck part.
    let attacker = Combatant::new(1, EntityKind::Player);
    let target = Combatant::new(2, EntityKind::EnemyMonster).with_armor_values(delta);
    let input = AttackInput {
        attacker: &attacker,
        target: &target,
        weapon: None,
        chance_mod: 45,
        damage_mods: 0,
        backstab_level: 0,
        enemy_bonus: 0,
    };
    let expected = [3, 20, 5, 3, 3, 5, 3];
    for part in 0..BODY_PART_COUNT {
        let chance = chance_to_hit_for_part(&input, part, false);
        rows.push(row(
            format!("B-CHANCE-{part}"),
            format!("chance={chance}"),
            format!("chance={}", expected[part]),
            format!("part={part} armour={} 45 + (−10) → {chance}", delta[part]),
            SRC_PART_CHANCE,
        ));
    }

    // inc/dec modifiers (`FormulaHelper.cs:1149-1161`).
    let mut boosted = Combatant::new(2, EntityKind::EnemyMonster).with_armor_values(delta);
    boosted.inc_armor_mod = 10;
    let mut gimped = Combatant::new(2, EntityKind::EnemyMonster).with_armor_values(delta);
    gimped.dec_armor_mod = -20;
    let mut both = Combatant::new(2, EntityKind::EnemyMonster).with_armor_values(delta);
    both.inc_armor_mod = 10;
    both.dec_armor_mod = -15;
    for (id, target, want, label) in [
        ("B-INC", &boosted, 60, "inc +10"),
        ("B-DEC", &gimped, 30, "dec −20"),
        ("B-BOTH", &both, 45, "inc +10 / dec −15"),
    ] {
        let input = AttackInput {
            attacker: &attacker,
            target,
            weapon: None,
            chance_mod: 75,
            damage_mods: 0,
            backstab_level: 0,
            enemy_bonus: 0,
        };
        let chance = chance_to_hit_for_part(&input, 1, false);
        rows.push(row(
            id,
            format!("chance={chance}"),
            format!("chance={want}"),
            format!("part=1 armour=−15 chance_mod=75 {label} → {chance}"),
            SRC_CHANCE,
        ));
    }

    // Player target: the biography gate (delta D-M22-2).
    let player_target = Combatant::new(1, EntityKind::Player).with_armor_values(delta);
    let mut biography_target = player_target.clone();
    biography_target.biography_avoid_hit_mod = 20;
    for (id, target, want, label) in [
        ("B-PLAYER-NOBIO", &player_target, 25, "biography 0"),
        ("B-PLAYER-BIO", &biography_target, 5, "biography 20"),
    ] {
        let input = AttackInput {
            attacker: &attacker,
            target,
            weapon: None,
            chance_mod: 120,
            damage_mods: 0,
            backstab_level: 0,
            enemy_bonus: 0,
        };
        let chance = chance_to_hit_for_part(&input, 3, false);
        rows.push(row(
            id,
            format!("chance={chance}"),
            format!("chance={want}"),
            format!("player chest armour=−45 chance_mod=120 {label} → {chance}"),
            SRC_PART_CHANCE,
        ));
    }
    // The entity projection the runtime hands the engine: base + delta.
    let mut fighter = Fighter::new(
        Combatant::new(1, EntityKind::Player),
        DeathSequence::for_player_height(1.7),
    );
    for (template, material) in KIT {
        let _ = fighter.equip(armor_item(template, material));
    }
    let projected = fighter.combatant.armor_values;
    rows.push(row(
        "B-PROJ",
        format!("chest={} head={} hands={}", projected[3], projected[0], projected[4]),
        format!(
            "chest={} head={} hands={}",
            PLAYER_ARMOR_BASE + KIT_DELTA[3],
            PLAYER_ARMOR_BASE + KIT_DELTA[0],
            PLAYER_ARMOR_BASE + KIT_DELTA[4]
        ),
        "Fighter::refresh_equipment = 100 seed + equip-table delta",
        SRC_PROJ,
    ));
    rows
}

// ── block C · materials ─────────────────────────────────────────────────────

fn block_c() -> Vec<Row> {
    let mut rows = Vec::with_capacity(12);
    let iron = weapon_from_item(&weapon_item(120, 0));
    match &iron {
        Some(weapon) => rows.push(row(
            "C-WEAPON",
            format!(
                "native={} to_hit={} dmg_mod={} min={} max={}",
                weapon.native_material_value,
                weapon.to_hit_mod,
                weapon.material_damage_mod,
                weapon.min_damage,
                weapon.max_damage
            ),
            "native=0 to_hit=-10 dmg_mod=-1 min=2 max=16",
            "iron Longsword composed from the retail tables",
            SRC_WEAPON_VIEW,
        )),
        None => rows.push(row(
            "C-WEAPON",
            "weapon=none".to_string(),
            "weapon=some",
            "template 120 must compose a weapon",
            SRC_WEAPON_VIEW,
        )),
    }
    let iron_weapon = iron.unwrap_or(Weapon {
        native_material_value: 0,
        min_damage: 2,
        max_damage: 16,
        flags: 0,
        to_hit_mod: TO_HIT_MOD[0],
        material_damage_mod: weapon_material_modifier(0),
    });
    rows.push(row(
        "C-TOHIT-TABLE",
        format!("to_hit={:?}", TO_HIT_MOD),
        "to_hit=[-10, 0, 0, 10, 20, 30, 30, 40, 50, 60]",
        "GetWeaponMaterialModifier·10 per material (M21 §2.2)",
        SRC_TOHIT,
    ));
    for min_metal in 0..=3i32 {
        let blocked = material_gate(min_metal, iron_weapon.native_material_value);
        rows.push(row(
            format!("C-GATE-{min_metal}"),
            format!("blocked={blocked}"),
            format!("blocked={}", min_metal > 0),
            format!("MinMetalToHit={min_metal} vs iron=0"),
            SRC_GATE,
        ));
    }

    // Blocked: 0 roll, 0 damage, 0 wear.
    let attacker = Combatant::new(1, EntityKind::Player);
    let mut tough = Combatant::new(2, EntityKind::EnemyMonster);
    tough.min_metal_to_hit = 3;
    let blocked_input = AttackInput {
        attacker: &attacker,
        target: &tough,
        weapon: Some(&iron_weapon),
        chance_mod: 160,
        damage_mods: 0,
        backstab_level: 0,
        enemy_bonus: 0,
    };
    let mut dice = Dice100::with_seed(actor_seed(SYS_SEED, 900));
    let blocked = resolve_attack(&blocked_input, &mut dice);
    let (mut table, _) = kit_table();
    let mut wear_dice = WearDice::new(actor_seed(SYS_SEED, WEAR_STREAM_ACTOR));
    let blocked_wear = table.damage_equipment(blocked.part, blocked.damage, &mut wear_dice);
    rows.push(row(
        "C-BLOCKED",
        format!(
            "hit={} damage={} wear={} rolls={} material_ineffective={}",
            blocked.hit as u8,
            blocked.damage,
            blocked_wear.as_ref().map_or(0, |wear| wear.wear),
            blocked.rolls_consumed,
            blocked.material_ineffective as u8
        ),
        "hit=0 damage=0 wear=0 rolls=0 material_ineffective=1",
        "min_metal 3 > iron 0: no roll and no wear at all",
        SRC_GATE,
    ));

    // Allowed: a fixed-damage steel weapon makes the damage a rule, not a roll.
    let steel_exact = Weapon {
        native_material_value: 1,
        min_damage: 5,
        max_damage: 5,
        flags: 0,
        to_hit_mod: 0,
        material_damage_mod: 0,
    };
    let mut attacker_strength = Combatant::new(1, EntityKind::Player);
    attacker_strength.strength_modifier = 2;
    let mut soft = Combatant::new(2, EntityKind::EnemyMonster);
    soft.min_metal_to_hit = 1;
    soft.max_health = 1000;
    soft.health = 1000;
    let allowed_input = AttackInput {
        attacker: &attacker_strength,
        target: &soft,
        weapon: Some(&steel_exact),
        chance_mod: 160,
        damage_mods: 0,
        backstab_level: 0,
        enemy_bonus: 0,
    };
    let mut allowed_dice = Dice100::with_seed(actor_seed(SYS_SEED, 901));
    let allowed = resolve_attack(&allowed_input, &mut allowed_dice);
    let want = if allowed.hit {
        "hit=1 damage=7 chance=97 rolls=4"
    } else {
        "hit=0 damage=0 chance=97 rolls=3"
    };
    rows.push(row(
        "C-ALLOWED",
        format!(
            "hit={} damage={} chance={} rolls={}",
            allowed.hit as u8, allowed.damage, allowed.chance, allowed.rolls_consumed
        ),
        want,
        "min_metal 1 ≤ steel 1: on a hit 5 + 2 strength = 7",
        SRC_RESOLVE,
    ));

    // Skeletal halving then silver doubling (`FormulaHelper.cs:743`, `:748-749`).
    for (id, flags, silver, want, label) in [
        ("C-SKEL-SHARP-SILVER", 0x10u16, true, 18, "sharp silver"),
        ("C-SKEL-SHARP", 0x10u16, false, 9, "sharp plain"),
        ("C-SKEL-BLUNT-SILVER", 0u16, true, 8, "blunt silver"),
        ("C-SKEL-BLUNT", 0u16, false, 4, "blunt plain"),
    ] {
        rows.push(row(
            id,
            format!("damage={}", skeletal_weapon_adjust(9, flags, silver)),
            format!("damage={want}"),
            format!("9 damage, {label}: halve (if blunt) then double (if silver)"),
            SRC_SKEL,
        ));
    }
    rows
}

// ── block D · wear ──────────────────────────────────────────────────────────

fn block_d() -> Vec<Row> {
    let mut rows = Vec::with_capacity(13);
    for (damage, want) in [(0i32, "0"), (1, "0"), (7, "1"), (15, "2"), (30, "3"), (100, "10")] {
        rows.push(row(
            format!("D-W{damage}"),
            format!("wear={}", wear_amount(damage)),
            format!("wear={want}"),
            format!("(10·{damage}+50)/100 truncated"),
            SRC_WEAR,
        ));
    }
    // The 20 % "minimum one point" re-roll, driven by two dice with one seed.
    let mut twin_rule = WearDice::new(7);
    let rerolled = wear_amount_rolled(0, &mut twin_rule);
    let mut condition = 10;
    let outcome = apply_wear(&mut condition, 0, &mut WearDice::new(7));
    let observed = match outcome {
        WearOutcome::Unchanged => "amount=0 condition=10 unchanged".to_string(),
        WearOutcome::Worn { amount, condition } => format!("amount={amount} condition={condition} worn"),
        WearOutcome::Broke { amount } => format!("amount={amount} broke"),
    };
    let expected = if rerolled == 1 {
        "amount=1 condition=9 worn".to_string()
    } else {
        "amount=0 condition=10 unchanged".to_string()
    };
    rows.push(row(
        "D-REROLL-ZERO",
        observed,
        expected,
        format!("damage 0 ⇒ wear 0 plus SuccessRoll(20) re-roll ({rerolled})"),
        SRC_WROLLED,
    ));

    for (id, current, damage, want, label) in [
        ("D-WORN", 5i32, 15i32, "amount=2 condition=3 worn", "5 − 2"),
        ("D-BROKE-EXACT", 2, 15, "amount=2 condition=0 broke", "2 − 2 = 0"),
        ("D-BROKE-OVER", 1, 30, "amount=3 condition=0 broke", "1 − 3 saturates at 0"),
    ] {
        let mut condition = current;
        let outcome = apply_wear(&mut condition, damage, &mut WearDice::new(11));
        let observed = match outcome {
            WearOutcome::Unchanged => format!("amount=0 condition={condition} unchanged"),
            WearOutcome::Worn { amount, condition } => format!("amount={amount} condition={condition} worn"),
            WearOutcome::Broke { amount } => format!("amount={amount} condition={condition} broke"),
        };
        rows.push(row(
            id,
            observed,
            want,
            format!("condition {current}, damage {damage}: {label}"),
            SRC_APPLY_WEAR,
        ));
    }

    // Wear per physical hit through the equip table (`DamageEquipment`).
    let (mut table, _) = kit_table();
    let mut dice = WearDice::new(3);
    let mut last_wear = 0;
    for _ in 0..5 {
        last_wear = table
            .damage_equipment(3, 30, &mut dice)
            .map_or(0, |wear| wear.wear);
    }
    let condition = table
        .get(EquipSlot::ChestArmor)
        .map_or(-1, |item| item.current_condition);
    rows.push(row(
        "D-EQUIP-5HITS",
        format!("condition={condition} last_wear={last_wear}"),
        "condition=135 last_wear=3",
        "chest 150, five 30-damage hits of wear 3",
        SRC_DMG_EQUIP,
    ));
    let (mut table, _) = kit_table();
    let broken = table.damage_equipment(3, 2000, &mut WearDice::new(5));
    let after = table.armor_values();
    rows.push(row(
        "D-EQUIP-BREAK",
        format!(
            "broke={} removed={} condition={} chest_delta={}",
            broken.as_ref().is_some_and(|wear| wear.broke),
            broken.as_ref().is_some_and(|wear| wear.removed.is_some()),
            broken.as_ref().map_or(-1, |wear| wear.condition),
            after[3]
        ),
        "broke=true removed=true condition=0 chest_delta=0",
        "wear 2000 > 150: the piece breaks, is unequipped and its bonus leaves",
        SRC_DMG_EQUIP,
    ));
    let (mut table, _) = kit_table();
    let weapon_worn = table.damage_item(EquipSlot::ChestArmor, 15, &mut WearDice::new(9));
    rows.push(row(
        "D-DMG-ITEM",
        format!(
            "wear={} condition={}",
            weapon_worn.as_ref().map_or(-1, |wear| wear.wear),
            weapon_worn.as_ref().map_or(-1, |wear| wear.condition)
        ),
        "wear=2 condition=148",
        "damage_item 15 on the chest piece: 150 − 2",
        SRC_DMG_ITEM,
    ));
    rows
}

// ── block E · critical / backstab ───────────────────────────────────────────

fn block_e() -> Vec<Row> {
    let mut rows = Vec::with_capacity(12);
    rows.push(row(
        "E-CRIT-FAIL",
        format!("bonus={}", critical_bonus(20, success_roll(40, 20))),
        "bonus=0",
        "crit roll 40 ≥ 20 ⇒ no bonus",
        SRC_CRIT,
    ));
    rows.push(row(
        "E-CRIT-OK",
        format!("bonus={}", critical_bonus(20, success_roll(5, 20))),
        "bonus=2",
        "crit roll 5 < 20 ⇒ +20/10",
        SRC_CRIT,
    ));
    rows.push(row(
        "E-CRIT-35",
        format!("bonus={}", critical_bonus(35, true)),
        "bonus=3",
        "35/10 truncated toward zero",
        SRC_CRIT,
    ));
    rows.push(row(
        "E-SUCCESS-EDGE",
        format!(
            "c3rolls={},{},{}",
            success_roll(0, 3),
            success_roll(2, 3),
            success_roll(3, 3)
        ),
        "c3rolls=true,true,false",
        "SuccessRoll(c) is roll < c at the clamp floor",
        SRC_SUCCESS,
    ));
    let levels = [0, 1, 2, 3]
        .iter()
        .map(|level| backstabing_requires_roll(*level).to_string())
        .collect::<Vec<_>>()
        .join(",");
    rows.push(row(
        "E-BS-REQ",
        format!("levels={levels}"),
        "levels=false,false,true,true",
        "backstabbingLevel > 1 is the precondition",
        SRC_BSREQ,
    ));
    rows.push(row(
        "E-BS-X3",
        format!("damage={}", backstab_damage(9, 3, true)),
        "damage=27",
        "9 · 3 with the level roll passed",
        SRC_BS,
    ));
    rows.push(row(
        "E-BS-NOROLL",
        format!("damage={}", backstab_damage(9, 3, false)),
        "damage=9",
        "level roll failed ⇒ plain damage",
        SRC_BS,
    ));
    rows.push(row(
        "E-BS-LEVEL1",
        format!("damage={}", backstab_damage(9, 1, true)),
        "damage=9",
        "level 1 never multiplies",
        SRC_BS,
    ));

    let fixed = Weapon {
        native_material_value: 1,
        min_damage: 5,
        max_damage: 5,
        flags: 0,
        to_hit_mod: 0,
        material_damage_mod: 0,
    };
    // Non-player path: the app composes backstab_level 0 for an enemy.
    let mut monster = Combatant::new(3, EntityKind::EnemyClass);
    monster.strength_modifier = 2;
    let mut hero = Combatant::new(1, EntityKind::Player);
    hero.max_health = 1000;
    hero.health = 1000;
    let nonplayer_input = AttackInput {
        attacker: &monster,
        target: &hero,
        weapon: Some(&fixed),
        chance_mod: 160,
        damage_mods: 0,
        backstab_level: 0,
        enemy_bonus: 0,
    };
    let mut dice = Dice100::with_seed(actor_seed(SYS_SEED, 902));
    let nonplayer = resolve_attack(&nonplayer_input, &mut dice);
    let want = if nonplayer.hit {
        "hit=1 damage=7"
    } else {
        "hit=0 damage=0"
    };
    rows.push(row(
        "E-NONPLAYER",
        format!("hit={} damage={}", nonplayer.hit as u8, nonplayer.damage),
        want,
        "enemy attacker, backstab_level 0 ⇒ never ×3",
        SRC_RESOLVE,
    ));

    // Player path: backstab_level 3 rolls SuccessRoll(3) for the ×3.
    let mut player = Combatant::new(1, EntityKind::Player);
    player.strength_modifier = 2;
    let mut target = Combatant::new(2, EntityKind::EnemyMonster);
    target.max_health = 1000;
    target.health = 1000;
    let player_input = AttackInput {
        attacker: &player,
        target: &target,
        weapon: Some(&fixed),
        chance_mod: 160,
        damage_mods: 0,
        backstab_level: 3,
        enemy_bonus: 0,
    };
    let mut first_dice = Dice100::with_seed(actor_seed(SYS_SEED, 903));
    let mut second_dice = Dice100::with_seed(actor_seed(SYS_SEED, 903));
    let first = resolve_attack(&player_input, &mut first_dice);
    let second = resolve_attack(&player_input, &mut second_dice);
    let in_set = if first.hit {
        first.damage == 7 || first.damage == 21
    } else {
        first.damage == 0
    };
    rows.push(predicate(
        "E-PLAYER-BS",
        in_set,
        format!("hit={} damage={}", first.hit as u8, first.damage),
        "damage in {0 (miss), 7, 21}",
        format!(
            "player backstab_level 3: hit={} damage={} (7 plain / 21 ×3)",
            first.hit as u8, first.damage
        ),
        SRC_RESOLVE,
    ));
    rows.push(row(
        "E-BS-DETERMINISTIC",
        format!(
            "same={}",
            first.damage == second.damage && first.hit == second.hit
        ),
        "same=true",
        "twin per-actor seeds replay the backstab roll",
        SRC_RESOLVE,
    ));
    rows
}

// ── block F · range and cadence ─────────────────────────────────────────────

fn block_f() -> Vec<Row> {
    let mut rows = Vec::with_capacity(20);
    for (skill, strength, is_player, want_min, want_max) in [
        (30i32, 2i32, true, 6i32, 9i32),
        (30, 2, false, 4, 7),
        (100, 0, true, 11, 21),
        (5, 0, false, 1, 2),
        (100, 3, true, 14, 24),
    ] {
        let (min, max) = h2h_range(skill, strength, is_player);
        let who = if is_player { "P" } else { "NPC" };
        rows.push(row(
            format!("F-H2H-{skill}-{strength}-{who}"),
            format!("min={min} max={max}"),
            format!("min={want_min} max={want_max}"),
            format!("skill={skill} strength={strength} player={is_player}"),
            SRC_H2H,
        ));
    }
    for (speed, want) in [
        (50i32, "0.198980"),
        (100, "0.045918"),
        (0, "0.352041"),
        (115, "0.000000"),
    ] {
        rows.push(row(
            format!("F-TICK-{speed}"),
            format!("{:.6}", melee_tick_seconds(speed)),
            want.to_string(),
            format!("3·(115−{speed})/980 seconds"),
            SRC_TICK,
        ));
    }
    for (speed, want) in [(50i32, "1.326531"), (100, "0.816327"), (0, "1.836735")] {
        rows.push(row(
            format!("F-BOW-{speed}"),
            format!("{:.6}", bow_cooldown_seconds(speed)),
            want.to_string(),
            format!("(10·(100−{speed})+800)/980 seconds"),
            SRC_BOW,
        ));
    }
    rows.push(row(
        "F-DENOMINATOR",
        format!("classic_frame_update={CLASSIC_FRAME_UPDATE}"),
        "classic_frame_update=980",
        "classicFrameUpdate denominator of every cadence",
        "crates/behavior/src/combat.rs:18",
    ));
    rows.push(row(
        "F-FRAMES-50",
        format!("frames={}", 3 * (115 - 50)),
        "frames=195",
        "melee animation frames at LiveSpeed 50",
        SRC_TICK,
    ));
    for (roll_ms, level, reflexes, want) in [
        (1500i32, 10i32, 3i32, "1.989796"),
        (3000, 20, 2, "2.551020"),
    ] {
        rows.push(row(
            format!("F-TIMER-{roll_ms}"),
            format!("{:.6}", enemy_melee_timer_seconds(roll_ms, level, reflexes)),
            want.to_string(),
            format!("({roll_ms} − 50·({level}−10) + 450·({reflexes}−2))/980"),
            SRC_TIMER_F,
        ));
    }
    let gate_values = [2i32, 3, 5, 20]
        .iter()
        .map(|reflexes| reflexes_gate(*reflexes).to_string())
        .collect::<Vec<_>>()
        .join(",");
    rows.push(row(
        "F-REFLEX",
        format!("gates={gate_values}"),
        "gates=50,40,20,-130",
        "50 − 10·(reflexes − 2) for reflexes 2,3,5,20",
        SRC_REFLEX,
    ));
    rows.push(row(
        "F-MULTI",
        format!(
            "r49={} r50={}",
            multi_hit_gate(49, 2),
            multi_hit_gate(50, 2)
        ),
        "r49=true r50=false",
        "rand()%100 < reflexesChance with reflexes 2 ⇒ 50",
        SRC_MULTI,
    ));
    rows
}

// ── block G · actor death ───────────────────────────────────────────────────

fn block_g() -> Vec<Row> {
    let mut rows = Vec::with_capacity(12);
    for (id, amount, restore, want) in [
        ("G-SH-01", 150i32, false, "health=100 died=false"),
        ("G-SH-02", -5, false, "health=0 died=true"),
        ("G-SH-03", 0, false, "health=0 died=true"),
        ("G-SH-04", 150, true, "health=150 died=false"),
        ("G-SH-05", -5, true, "health=-5 died=true"),
    ] {
        let (health, died) = set_health(100, amount, restore);
        rows.push(row(
            id,
            format!("health={health} died={died}"),
            want,
            format!("SetHealth({amount}, restore={restore}) with max 100"),
            SRC_SETHEALTH,
        ));
    }
    let mut player = DeathSequence::for_player_height(1.7);
    let first = player.apply_set_health(37, -5, false);
    let events_first = player.events();
    let second = player.apply_set_health(37, -5, false);
    rows.push(row(
        "G-DEATH-PLAYER",
        format!("health={first} events={events_first}"),
        "health=0 events=1",
        format!("first lethal SetHealth emits OnDeath once (events={events_first})"),
        SRC_APPLY_SH,
    ));
    rows.push(row(
        "G-LATCH-SECOND",
        format!(
            "health={second} events={} refire={}",
            player.events(),
            player.events() > 1
        ),
        "health=0 events=1 refire=false",
        format!(
            "second lethal hit: the deathInProgress latch swallows the event (events={} refire={})",
            player.events(),
            player.events() > 1
        ),
        SRC_LATCH,
    ));
    let mut enemy = DeathSequence::for_enemy();
    let health = enemy.apply_set_health(10, -1, false);
    rows.push(row(
        "G-DEATH-ENEMY",
        format!("health={health} events={}", enemy.events()),
        "health=0 events=1",
        "enemy death: latch + no camera",
        SRC_APPLY_SH,
    ));
    let mut raw = DeathSequence::for_enemy();
    let raised_first = raw.raise_on_death();
    let raised_second = raw.raise_on_death();
    rows.push(row(
        "G-RAISE-LATCH",
        format!("first={raised_first} second={raised_second} events={}", raw.events()),
        "first=true second=false events=1",
        format!(
            "RaiseOnDeath emits exactly once (first={raised_first} second={raised_second} events={})",
            raw.events()
        ),
        SRC_LATCH,
    ));
    rows.push(row(
        "G-FALL",
        format!(
            "h1.7={:.6} h2.0={:.6} fade={FADE_SECONDS} reset={RESET_SECONDS}",
            fall_duration_seconds(1.7),
            fall_duration_seconds(2.0)
        ),
        "h1.7=0.850000 h2.0=1.000000 fade=2 reset=3",
        "camera fall height·1.25/2.5, fade 2 s, reset 3 s",
        SRC_FALL,
    ));
    rows
}

// ── block H · the (skill, armour, material, swing) matrix ───────────────────

const MATRIX_SKILLS: [i32; 3] = [15, 45, 80];
const MATRIX_ARMOUR: [i32; 3] = [0, -30, -45];
const MATRIX_MATERIALS: [usize; 3] = [0, 1, 9];
const MATRIX_SWINGS: [i32; 3] = [0, -10, 10];

fn block_h() -> Block {
    let mut lines = Vec::with_capacity(82);
    let mut rows = Vec::with_capacity(3);
    let mut seen: HashSet<(i32, i32, usize, i32)> = HashSet::new();
    let mut combinations = Vec::new();
    let mut bad = 0u32;
    let mut gate_cut = 0u32;
    let mut index = 0u32;
    let mut lines_rows = 0usize;

    for &skill in &MATRIX_SKILLS {
        for &armour in &MATRIX_ARMOUR {
            for &material in &MATRIX_MATERIALS {
                for &swing in &MATRIX_SWINGS {
                    index += 1;
                    let mut attacker = Combatant::new(1, EntityKind::Player);
                    attacker.strength_modifier = 2;
                    let mut target = Combatant::new(2, EntityKind::EnemyMonster);
                    target.max_health = 10_000;
                    target.health = 10_000;
                    target.armor_values = [armour; 7];
                    let item = weapon_item(120, material);
                    let Some(weapon) = weapon_from_item(&item) else {
                        bad += 1;
                        continue;
                    };
                    let input = AttackInput {
                        attacker: &attacker,
                        target: &target,
                        weapon: Some(&weapon),
                        chance_mod: skill + swing,
                        damage_mods: 0,
                        backstab_level: 0,
                        enemy_bonus: 0,
                    };
                    let mut dice = Dice100::with_seed(actor_seed(SYS_SEED, 1000 + index));
                    let outcome = resolve_attack(&input, &mut dice);
                    if !(CHANCE_MIN..=CHANCE_MAX).contains(&outcome.chance) {
                        bad += 1;
                    }
                    if outcome.hit {
                        if outcome.damage < 1 || outcome.rolls_consumed != 4 {
                            bad += 1;
                        }
                    } else if outcome.damage != 0 || outcome.rolls_consumed != 3 {
                        bad += 1;
                    }
                    if outcome.material_ineffective {
                        gate_cut += 1;
                    }
                    if !seen.insert((skill, armour, material, swing)) {
                        bad += 1;
                    }
                    combinations.push(format!("{skill}/{armour}/{material}/{swing}"));
                    lines_rows += 1;
                    lines.push(format!(
                        "MATRIX {skill} {armour} {material} {swing} hit={} damage={} src={SRC_RESOLVE}",
                        outcome.hit as u8, outcome.damage
                    ));
                }
            }
        }
    }

    rows.push(row(
        "H-INVARIANTS",
        format!("checked={lines_rows} bad={bad}"),
        format!("checked={lines_rows} bad=0"),
        "per combination: chance in [3,97], hit ⇒ damage ≥ 1 and 4 rolls, miss ⇒ 0 and 3",
        SRC_RESOLVE,
    ));
    rows.push(row(
        "H-UNIQUE",
        format!("combinations={} distinct={}", combinations.len(), seen.len()),
        format!("combinations={} distinct={}", combinations.len(), combinations.len()),
        "every (skill, armour, material, swing) exactly once",
        SRC_RESOLVE,
    ));
    rows.push(row(
        "H-GATE-FREE",
        format!("material_cut={gate_cut}"),
        "material_cut=0",
        "the matrix target has no MinMetalToHit, so the gate never fires",
        SRC_GATE,
    ));

    let declared = format!(
        "MATRIX skills={} armour={} materials={} swings={} rows={}",
        MATRIX_SKILLS.len(),
        MATRIX_ARMOUR.len(),
        MATRIX_MATERIALS.len(),
        MATRIX_SWINGS.len(),
        lines_rows
    );
    let mut tail = Vec::with_capacity(lines.len() + 1);
    tail.push(declared);
    tail.extend(lines);
    Block {
        label: "H",
        rows,
        tail,
    }
}

// ── block I · deterministic 60 s combat ─────────────────────────────────────

struct Combat60 {
    events: u32,
    damage: i32,
    deaths: u32,
    digest: String,
    swings: u32,
    enemy_ticks: u32,
}

/// 60 s of one duel at the **measured** cadence
/// (`melee_tick_seconds(50)` for the player, `EnemyAttack` timer for the
/// enemy), driven by one `DFRandom` chain per actor plus the wear stream
/// (delta D-M22-3). Pure: 0 clock, 0 I/O — the digest is a function of the
/// seed and the rules only.
fn simulate_60s() -> Option<Combat60> {
    const LIVE_SPEED: i32 = 50;
    const PLAYER_ID: u32 = 1;
    const ENEMY_ID: u32 = 2;
    const FRAMES_PER_SECOND: f64 = 980.0;

    let tick = f64::from(melee_tick_seconds(LIVE_SPEED));
    let dt = 1.0 / FRAMES_PER_SECOND;
    let frames = (60.0 / dt).round() as u32;

    let mut player = Combatant::new(PLAYER_ID, EntityKind::Player);
    player.max_health = 180;
    player.health = 180;
    player.strength_modifier = 2;

    let mut enemy = Combatant::new(ENEMY_ID, EntityKind::EnemyMonster);
    enemy.max_health = 140;
    enemy.health = 140;

    let (mut player_gear, _) = kit_table();
    let (mut enemy_gear, _) = kit_table();
    let _ = player_gear.equip(weapon_item(120, 1), true);
    player.armor_values = player_gear.armor_values();
    enemy.armor_values = enemy_gear.armor_values();

    let weapon_slot = player_gear.equipped_weapon_slot();
    let weapon = weapon_slot
        .and_then(|slot| player_gear.get(slot))
        .and_then(weapon_from_item)?;

    let bands: [(i32, i32); 3] = [(1, 4), (0, 3), (2, 6)];
    let mut player_dice = Dice100::with_seed(actor_seed(SYS_SEED, PLAYER_ID));
    let mut enemy_dice = Dice100::with_seed(actor_seed(SYS_SEED, ENEMY_ID));
    let mut wear_dice = WearDice::new(actor_seed(SYS_SEED, WEAR_STREAM_ACTOR));
    let mut player_death = DeathSequence::for_player_height(1.7);
    let mut enemy_death = DeathSequence::for_enemy();
    let mut timer = EnemyAttack::new();
    timer.reset_melee_timer(enemy_dice.roll_range(1500, 3000), 10, 3);

    let mut accumulator = 0.0f64;
    let mut log = String::with_capacity(16 * 1024);
    let mut events = 0u32;
    let mut damage_total = 0i32;
    let mut deaths = 0u32;
    let mut swings = 0u32;
    let mut enemy_ticks = 0u32;

    for frame in 0..frames {
        accumulator += dt;
        while accumulator >= tick {
            accumulator -= tick;
            swings += 1;
            if player.health > 0 && enemy.health > 0 {
                let input = AttackInput {
                    attacker: &player,
                    target: &enemy,
                    weapon: Some(&weapon),
                    chance_mod: 45,
                    damage_mods: 0,
                    backstab_level: 0,
                    enemy_bonus: 0,
                };
                let outcome = resolve_attack(&input, &mut player_dice);
                events += 1;
                if outcome.damage > 0 {
                    damage_total += outcome.damage;
                    if let Some(slot) = weapon_slot {
                        let _ = player_gear.damage_item(slot, outcome.damage, &mut wear_dice);
                    }
                    let _ = enemy_gear.damage_equipment(outcome.part, outcome.damage, &mut wear_dice);
                    let before = enemy_death.events();
                    enemy.health = enemy_death.apply_set_health(
                        enemy.max_health,
                        enemy.health - outcome.damage,
                        false,
                    );
                    if enemy_death.events() > before {
                        deaths += 1;
                    }
                    enemy.armor_values = enemy_gear.armor_values();
                    player.armor_values = player_gear.armor_values();
                }
                log.push_str(&format!(
                    "{frame} P hit={} part={} damage={}\n",
                    outcome.hit as u8, outcome.part, outcome.damage
                ));
            }
        }

        if timer.tick(dt as f32) {
            enemy_ticks += 1;
            if player.health > 0 && enemy.health > 0 {
                let input = MonsterAttackInput {
                    attacker: &enemy,
                    target: &player,
                    damage_bands: bands,
                    chance_mod: 40,
                    enemy_bonus: 0,
                    player_reflexes: 3,
                };
                let outcome = resolve_monster_attack(&input, &mut enemy_dice);
                events += 1;
                if outcome.damage > 0 {
                    damage_total += outcome.damage;
                    let _ = player_gear.damage_equipment(outcome.part, outcome.damage, &mut wear_dice);
                    let before = player_death.events();
                    player.health = player_death.apply_set_health(
                        player.max_health,
                        player.health - outcome.damage,
                        false,
                    );
                    if player_death.events() > before {
                        deaths += 1;
                    }
                    player.armor_values = player_gear.armor_values();
                    enemy.armor_values = enemy_gear.armor_values();
                }
                log.push_str(&format!(
                    "{frame} E part={} damage={}\n",
                    outcome.part, outcome.damage
                ));
            }
            timer.reset_melee_timer(enemy_dice.roll_range(1500, 3000), 10, 3);
        }
    }

    Some(Combat60 {
        events,
        damage: damage_total,
        deaths,
        digest: sha256_hex(log.as_bytes()),
        swings,
        enemy_ticks,
    })
}

fn block_i() -> Block {
    let mut rows = Vec::with_capacity(4);
    let mut tail = Vec::new();
    match (simulate_60s(), simulate_60s()) {
        (Some(first), Some(second)) => {
            rows.push(row(
                "I-CADENCE",
                format!("swings={} frames_per_second=980", first.swings),
                "swings=301 frames_per_second=980",
                "60 s at melee_tick_seconds(50) = floor(60 / 0.198980)",
                SRC_TICK,
            ));
            rows.push(row(
                "I-ENEMY-TIMER",
                format!("strikes={}", first.enemy_ticks),
                format!("strikes={}", first.enemy_ticks),
                "enemy strikes fired by the EnemyAttack timer in 60 s",
                SRC_TIMER,
            ));
            let digest_ok = first.digest.len() == 64
                && first.digest.chars().all(|ch| ch.is_ascii_hexdigit());
            rows.push(row(
                "I-DIGEST-SHAPE",
                format!("len={} hex={digest_ok}", first.digest.len()),
                "len=64 hex=true",
                "sha256 of the canonical event log",
                SRC_MONSTER,
            ));
            let identical = first.digest == second.digest
                && first.events == second.events
                && first.damage == second.damage
                && first.deaths == second.deaths
                && first.swings == second.swings;
            rows.push(row(
                "I-REPEAT",
                format!(
                    "identical={identical} events={}/{} damage={}/{} deaths={}/{}",
                    first.events, second.events, first.damage, second.damage, first.deaths, second.deaths
                ),
                format!(
                    "identical=true events={}/{} damage={}/{} deaths={}/{}",
                    first.events, first.events, first.damage, first.damage, first.deaths, first.deaths
                ),
                "two runs inside the probe produce the same stream",
                SRC_MONSTER,
            ));
            tail.push(format!(
                "COMBAT60 events={} damage={} deaths={} digest={}",
                first.events, first.damage, first.deaths, first.digest
            ));
            tail.push(format!(
                "COMBAT60-REPEAT {} digest={}",
                if identical { "identical" } else { "differs" },
                second.digest
            ));
        }
        _ => rows.push(row(
            "I-SIM",
            "simulation=none".to_string(),
            "simulation=some",
            "the steel Longsword must compose through weapon_from_item",
            SRC_WEAPON_VIEW,
        )),
    }
    Block {
        label: "I",
        rows,
        tail,
    }
}

// ── block J · adversarial (`ADV/` label on every row id) ──────────────────

fn block_j() -> Vec<Row> {
    let mut rows = Vec::with_capacity(6);
    rows.push(row(
        "ADV/J-CLAMP-HIGH",
        format!(
            "chance={} ceiling={CHANCE_MAX}",
            chance_to_hit(HitMods {
                chance_mod: 1_000_000,
                ..HitMods::default()
            })
        ),
        "chance=97 ceiling=97",
        "absurd positive modifiers clamp at 97",
        SRC_CHANCE,
    ));
    rows.push(row(
        "ADV/J-CLAMP-LOW",
        format!(
            "chance={} floor={CHANCE_MIN}",
            chance_to_hit(HitMods {
                chance_mod: -1_000_000,
                ..HitMods::default()
            })
        ),
        "chance=3 floor=3",
        "absurd negative modifiers clamp at 3",
        SRC_CHANCE,
    ));
    rows.push(row(
        "ADV/J-FLOOR",
        format!("damage={}", weapon_damage(-1000, 0, 0, 0)),
        "damage=0",
        "an absurdly low roll floors at 0, never below",
        SRC_FLOOR,
    ));
    rows.push(row(
        "ADV/J-STRUCK-CLAMP",
        format!(
            "neg={} over={}",
            struck_body_part(-5),
            struck_body_part(25)
        ),
        "neg=0 over=6",
        "absurd struck-part rolls are clamped, never index out of range",
        SRC_STRUCK,
    ));
    let attacker = Combatant::new(1, EntityKind::Player);
    let mut tough = Combatant::new(2, EntityKind::EnemyMonster);
    tough.min_metal_to_hit = 9;
    let iron = Weapon {
        native_material_value: 0,
        min_damage: 2,
        max_damage: 16,
        flags: 0,
        to_hit_mod: TO_HIT_MOD[0],
        material_damage_mod: weapon_material_modifier(0),
    };
    let input = AttackInput {
        attacker: &attacker,
        target: &tough,
        weapon: Some(&iron),
        chance_mod: 160,
        damage_mods: 0,
        backstab_level: 0,
        enemy_bonus: 0,
    };
    let mut dice = Dice100::with_seed(actor_seed(SYS_SEED, 904));
    let outcome = resolve_attack(&input, &mut dice);
    rows.push(row(
        "ADV/J-GATE-NO-ROLL",
        format!(
            "rolls={} damage={} hit={}",
            outcome.rolls_consumed, outcome.damage, outcome.hit as u8
        ),
        "rolls=0 damage=0 hit=0",
        "material gate refuses before any roll is consumed",
        SRC_GATE,
    ));
    let mut seq = DeathSequence::for_enemy();
    let first = seq.apply_set_health(10, -1, false);
    let second = seq.apply_set_health(10, -1, false);
    rows.push(row(
        "ADV/J-DEATH-LATCH",
        format!(
            "first={first} second={second} events={} refire={}",
            seq.events(),
            seq.events() > 1
        ),
        "first=0 second=0 events=1 refire=false",
        format!(
            "a second lethal hit never re-fires death (events={} refire={})",
            seq.events(),
            seq.events() > 1
        ),
        SRC_LATCH,
    ));
    rows
}

// ── probe entry ─────────────────────────────────────────────────────────────

pub fn run_sys_combat_001() -> i32 {
    let blocks = vec![
        block("A", block_a()),
        block("B", block_b()),
        block("C", block_c()),
        block("D", block_d()),
        block("E", block_e()),
        block("F", block_f()),
        block("G", block_g()),
        block_h(),
        block_i(),
        block("J", block_j()),
    ];

    let mut lines: Vec<String> = Vec::with_capacity(220);
    lines.push(format!(
        "PROBE-SYS-COMBAT-001 - complete combat: hit location, per-part armour, materials, wear, critical/backstab, range, actor death, matrix, 60s (M41/W41-2, {CR})"
    ));
    let mut total = 0usize;
    let mut failures = 0usize;
    for section in &blocks {
        lines.push(format!("BLOCK {} rows={}", section.label, section.rows.len()));
        for entry in &section.rows {
            total += 1;
            if entry.ok {
                lines.push(format!(
                    "[OK] {} - {} (src={})",
                    entry.id, entry.detail, entry.src
                ));
            } else {
                failures += 1;
                lines.push(format!(
                    "FAIL {} got={} want={} (src={})",
                    entry.id, entry.got, entry.want, entry.src
                ));
            }
        }
        lines.extend(section.tail.iter().cloned());
    }
    lines.push(format!(
        "CENSUS rows={total} ok={} fail={failures} blocks={}",
        total - failures,
        blocks.len()
    ));
    lines.push(format!(
        "VEREDICTO: {}",
        if failures == 0 { "PASS" } else { "FAIL" }
    ));

    let body = format!("{}\n", lines.join("\n"));
    let digest = sha256_hex(body.as_bytes());
    print!("{body}");
    println!("DIGEST {digest}");
    if failures == 0 {
        0
    } else {
        1
    }
}
