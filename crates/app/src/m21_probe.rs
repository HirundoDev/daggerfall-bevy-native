//! M21 — Anti-Screen probes of **items, equipment, durability and loot**
//! (REQ-L99-054): `PROBE-ITEM-001` (blocks A + C), `PROBE-EQUIP-001`
//! (block B) and `PROBE-LOOT-001` (block D).
//!
//! Contract: `docs/sintergic/game/programming/m21-items-equipment-loot-spec.md`
//! §0 (units), §1.4 (GV-IT), §2.4-2.6 (GV-MAT/CND), §4.2 (GV-DUR),
//! §3.1 (GV-EQP), §5.1-5.3 (GV-LOT) and **§7 probe contract**. The goldens
//! are transcribed here; the GV-IT and the census are measured at runtime
//! over the retail blob `FALL.EXE` at the installation root (Mode A,
//! read-only, 0 retail bytes in git — INV-L99-6).
//!
//! `PROBE-ITEM-001` — blocks:
//!
//! - **A · formulas (55 checks)**: `GV-IT-01..15` (blob parsing —
//!   **always** via `find_items_offset`, never `DEFAULT_ITEMS_OFFSET`:
//!   byte 1776954 reads `0x40` and the first `Ruby` is at 1796138) +
//!   `GV-MAT-01..15` (material over real retail templates) +
//!   `GV-CND-01..08` (condition/weight) + `GV-DUR-01..17` (wear,
//!   repair, time).
//! - **C · disk census**: `census: records=288 first=Ruby last=Map
//!   offset=1796138 sha256=<64 hex>` over the 13824 B block.
//!   `FALL.EXE` missing/unreadable ⇒ **exit 2** (never 0, never skip; the
//!   GV-IT depend on the blob, so the whole block exits with 2).
//!
//! `PROBE-EQUIP-001` — 10 checks (`GV-EQP-01..10`) over a
//! synthetic `ItemEquipTable` + dump of the 27 slots with `serialize_uids()`.
//!
//! `PROBE-LOOT-001` — 500 containers `key='K'` level 5 with an explicit
//! `Rng64` per container (`derive_rng`, 0 global RNG), SHA-256 digest and
//! `--seed <u64>` (documented default). Three internal checks: digest ×2
//! identical with the same seed, different digest with an alternate seed and
//! observed limits/chain cap.
//!
//! **Measured errata (declared, never "fixed" silently):**
//!
//! 1. **GV-DUR-17 (ORDER):** spec §4.2 line 285 cites
//!    `2831155200` = `32768·86400` **before** the `/1000` of
//!    `FormulaHelper.cs:1915`; the real value of the formula is **2_831_155 s**
//!    and the engine already returns it (`crates/behavior/src/durability.rs:326-331`).
//!    The probe compares against `2_831_155`.
//! 2. **GV-EQP-07 (D-M21-4, ORDER):** a potion with `stack=5` **does not**
//!    equip — DFU resolves `GetEquipSlot(Drugs)=None` (`ItemEquipTable.cs:100-106`)
//!    and `equip` returns `None` **before** the split. The check verifies that
//!    faithful behavior; the split branch is verified with a synthetic
//!    stackable+equippable item (equips 1, 4 remain).
//!
//! **Outputs (§7):** `EXIT=0` ⟺ all checks OK · `EXIT=1` on any
//! FAIL (line `FAIL <id> got=<v> want=<v>`) · `EXIT=2` with no readable disk.

use std::path::PathBuf;

use daggerfall_behavior::durability::{
    apply_wear, days_remaining, repair_cost, repair_time_seconds, repair_time_seconds_for_damage,
    wear_amount, wear_amount_rolled, WEAR_REROLL_CHANCE,
};
use daggerfall_behavior::locomotion::{max_encumbrance, Dice100};
use daggerfall_inventory::equipment::{
    item_hands, EquipResult, EquipSlot, ItemEquipTable, BOW_LEFT_HAND_WITH_SWITCHING,
};
use daggerfall_inventory::items::{
    apply_armor_material, apply_weapon_material, condition_percent, effective_unit_weight_kg,
    find_items_offset, parse_item_templates, total_weight_kg, Item, ItemTemplate,
    DEFAULT_ITEMS_OFFSET, ITEM_RECORD_SIZE, TOTAL_ITEMS,
};
use daggerfall_inventory::loot::{container_digest_bytes, generate_container_with_counts, matrix};
use sha2::{Digest, Sha256};

/// Measured offset of the first template in THIS retail (spec §1.1: the
/// fallback path of `FindString("Ruby")`; `DEFAULT_ITEMS_OFFSET` reads `0x40`).
const EXPECTED_OFFSET: usize = 1_796_138;
/// Byte measured at `DEFAULT_ITEMS_OFFSET` of THIS retail (spec §1.1, GV-IT-01).
const DEFAULT_OFFSET_BYTE: u8 = 0x40;
/// End of the table = offset + 288·48 (spec §1.4 GV-IT-14).
const EXPECTED_TABLE_END: usize = 1_809_962;
/// Measured size of `FALL.EXE` (spec §0, re-measurement 2026-10-21).
const FALL_EXE_SIZE: usize = 1_864_183;
/// Erratum GV-DUR-17 (ORDER): `32768·86400/1000` truncated, not the
/// product before the division that the spec cites (`durability.rs:326-331`).
const GV_DUR_17_SECONDS: i64 = 2_831_155;
/// Default seed of PROBE-LOOT-001 — the M21 spec date, same pattern
/// as `M20_SEED = 20_261_020`; `--seed <u64>` replaces it.
const LOOT_SEED_DEFAULT: u64 = 20_261_021;
/// Containers of block D (spec §7: `n=500`).
const LOOT_CONTAINERS: u32 = 500;
/// Level of the containers (spec §7: the one of GV-LOT-01).
const LOOT_LEVEL: i32 = 5;
/// Matrix key (spec §7 block D).
const LOOT_KEY: char = 'K';

// ── evidence rows (M19/M20 template) ────────────────────────────────────

/// Printed row of the probe: `[OK] <id> - <detail>` or
/// `FAIL <id> got=<v> want=<v>` (spec §7 literal).
#[derive(Debug, Clone)]
struct Row {
    id: &'static str,
    ok: bool,
    got: String,
    want: String,
    detail: String,
}

/// Row whose verdict is the equality `got == want` (all the relevant
/// values travel INSIDE both strings).
fn row(id: &'static str, got: String, want: String, detail: impl Into<String>) -> Row {
    Row {
        id,
        ok: got == want,
        got,
        want,
        detail: detail.into(),
    }
}

fn print_rows(title: &str, rows: &[Row]) -> bool {
    println!("\n-- {title} --");
    let mut ok = true;
    for row in rows {
        if row.ok {
            println!("[OK] {} - {}", row.id, row.detail);
        } else {
            ok = false;
            println!("FAIL {} got={} want={}", row.id, row.got, row.want);
        }
    }
    ok
}

// ── source custody (Mode A, installation root) ──────────────────────────

/// Mode A installation root: `$DF_APP_ROOT` or by default
/// `$HOME/Games/Daggerfall/app` (same resolution as `mode_a_arena2_dir`
/// in `main.rs`, without the `ARENA2/` step: `FALL.EXE` lives at the ROOT —
/// spec §0, never inside `ARENA2/`).
fn install_root() -> Result<PathBuf, String> {
    match std::env::var_os("DF_APP_ROOT") {
        Some(value) => Ok(PathBuf::from(value)),
        None => {
            let home =
                std::env::var_os("HOME").ok_or("HOME not defined (set $DF_APP_ROOT)")?;
            Ok(PathBuf::from(home).join("Games/Daggerfall/app"))
        }
    }
}

/// Reads `FALL.EXE` from the installation root. `Err` ⇒ the caller closes
/// with `EXIT=2` (never a PASS without retail data).
fn load_fall_exe() -> Result<Vec<u8>, String> {
    let root = install_root()?;
    let path = root.join("FALL.EXE");
    std::fs::read(&path).map_err(|err| format!("FALL.EXE unreadable: {err} ({path:?})"))
}

/// SHA-256 in lowercase hex (spec §7 digests: census and loot).
fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hasher.finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

// ── block A · GV-IT (templates, transcribed from §1.4) ──────────────────

/// Template golden (spec §1.4) — measured byte by byte from the retail blob.
#[derive(Debug, Clone, Copy)]
struct ItGolden {
    id: &'static str,
    index: usize,
    name: &'static str,
    kg: f32,
    hp: u16,
    price: i32,
    ench: i16,
    rar: u8,
    var: u8,
    draw: u8,
    props: u8,
    world: (i16, i16),
    player: (i16, i16),
}

/// GV-IT-02..14 (GV-IT-01 and GV-IT-15 are computed over the whole blob).
const IT_GOLDENS: [ItGolden; 13] = [
    ItGolden { id: "GV-IT-02", index: 0, name: "Ruby", kg: 0.25, hp: 1000, price: 250, ench: 1750, rar: 10, var: 12, draw: 20, props: 0x01, world: (254, 0), player: (0, 0) },
    ItGolden { id: "GV-IT-03", index: 1, name: "Emerald", kg: 0.25, hp: 1000, price: 425, ench: 2750, rar: 14, var: 12, draw: 23, props: 0x01, world: (254, 1), player: (0, 0) },
    ItGolden { id: "GV-IT-04", index: 2, name: "Sapphire", kg: 0.25, hp: 1000, price: 375, ench: 2250, rar: 15, var: 18, draw: 21, props: 0x01, world: (254, 2), player: (0, 0) },
    ItGolden { id: "GV-IT-05", index: 3, name: "Diamond", kg: 0.25, hp: 1000, price: 500, ench: 3000, rar: 15, var: 18, draw: 22, props: 0x01, world: (254, 3), player: (0, 0) },
    ItGolden { id: "GV-IT-06", index: 4, name: "Jade", kg: 0.25, hp: 1000, price: 10, ench: 200, rar: 5, var: 8, draw: 30, props: 0x01, world: (254, 4), player: (0, 0) },
    ItGolden { id: "GV-IT-07", index: 102, name: "Cuirass", kg: 12.5, hp: 4096, price: 100, ench: 400, rar: 3, var: 5, draw: 50, props: 0x00, world: (207, 11), player: (245, 3) },
    ItGolden { id: "GV-IT-08", index: 107, name: "Helm", kg: 2.5, hp: 2560, price: 80, ench: 900, rar: 3, var: 6, draw: 0, props: 0x00, world: (207, 12), player: (245, 27) },
    ItGolden { id: "GV-IT-09", index: 108, name: "Boots", kg: 2.0, hp: 2560, price: 50, ench: 175, rar: 3, var: 3, draw: 20, props: 0x00, world: (204, 1), player: (245, 0) },
    ItGolden { id: "GV-IT-10", index: 113, name: "Dagger", kg: 0.5, hp: 50, price: 1, ench: 200, rar: 1, var: 0, draw: 100, props: 0x04, world: (207, 5), player: (234, 5) },
    ItGolden { id: "GV-IT-11", index: 115, name: "Staff", kg: 2.0, hp: 300, price: 5, ench: 700, rar: 1, var: 0, draw: 100, props: 0x10, world: (207, 7), player: (234, 21) },
    ItGolden { id: "GV-IT-12", index: 120, name: "Longsword", kg: 4.5, hp: 800, price: 15, ench: 600, rar: 1, var: 0, draw: 100, props: 0x04, world: (207, 2), player: (234, 12) },
    ItGolden { id: "GV-IT-13", index: 286, name: "Ship deed", kg: 0.25, hp: 1, price: 0, ench: 0, rar: 0, var: 0, draw: 0, props: 0x00, world: (209, 8), player: (0, 0) },
    ItGolden { id: "GV-IT-14", index: 287, name: "Map", kg: 0.25, hp: 1, price: 0, ench: 0, rar: 0, var: 0, draw: 0, props: 0x00, world: (209, 8), player: (0, 0) },
];

fn it_got(template: &ItemTemplate) -> String {
    format!(
        "name={} kg={} hp={} price={} ench={} rar={} var={} draw={} props={:#04x} world=({},{}) player=({},{})",
        template.name,
        template.base_weight_kg(),
        template.hit_points,
        template.base_price,
        template.enchantment_points,
        template.rarity,
        template.variants,
        template.draw_order_or_effect,
        template.properties,
        template.world_texture_file(),
        template.world_texture_record(),
        template.player_texture_file(),
        template.player_texture_record(),
    )
}

fn it_want(golden: &ItGolden) -> String {
    format!(
        "name={} kg={} hp={} price={} ench={} rar={} var={} draw={} props={:#04x} world=({},{}) player=({},{})",
        golden.name,
        golden.kg,
        golden.hp,
        golden.price,
        golden.ench,
        golden.rar,
        golden.var,
        golden.draw,
        golden.props,
        golden.world.0,
        golden.world.1,
        golden.player.0,
        golden.player.1,
    )
}

fn block_it_rows(
    data: &[u8],
    offset: Option<usize>,
    templates: &[ItemTemplate],
) -> Vec<Row> {
    let mut rows = Vec::with_capacity(15);

    // GV-IT-01: the live path is the `FindString("Ruby")` fallback, not the
    // default (anti-false-positive (b) of spec §7).
    let offset = offset.unwrap_or(0);
    let default_byte = data.get(DEFAULT_ITEMS_OFFSET).copied().unwrap_or(0xFF);
    rows.push(row(
        "GV-IT-01",
        format!("offset={offset} default@{DEFAULT_ITEMS_OFFSET}={default_byte:#04x}"),
        format!(
            "offset={EXPECTED_OFFSET} default@{DEFAULT_ITEMS_OFFSET}={DEFAULT_OFFSET_BYTE:#04x}"
        ),
        "FindString(\"Ruby\") live fallback · default=1776954 reads 0x40 (ItemsFile.cs:425-433)",
    ));

    for golden in &IT_GOLDENS {
        let got = templates
            .get(golden.index)
            .map_or_else(|| "<template missing>".to_string(), it_got);
        rows.push(row(
            golden.id,
            got,
            it_want(golden),
            format!(
                "{} idx={} · ItemsFile.cs:435-452 (spec §1.4)",
                golden.name, golden.index
            ),
        ));
    }

    // GV-IT-15: census 288·48 = 13824, end of table and fit in the file.
    let table_end = offset + TOTAL_ITEMS * ITEM_RECORD_SIZE;
    rows.push(row(
        "GV-IT-15",
        format!(
            "records={} table_end={table_end} fits={}",
            templates.len(),
            data.len() >= table_end
        ),
        format!("records={TOTAL_ITEMS} table_end={EXPECTED_TABLE_END} fits=true"),
        format!(
            "288·48=13824 B · FALL.EXE={FALL_EXE_SIZE} B measured (ItemsFile.cs:180)"
        ),
    ));

    rows
}

// ── block A · GV-MAT (material §2.4) ────────────────────────────────────

/// Material golden (spec §2.4): resulting weight/value/max condition.
#[derive(Debug, Clone, Copy)]
struct MatGolden {
    id: &'static str,
    /// `Some(plate)` for armor (`native_material = 0x0200 + plate`),
    /// `None` for weapon (`WeaponMaterialTypes` index).
    armor: Option<i32>,
    material: usize,
    label: &'static str,
    kg: f32,
    value: i32,
    cond: i32,
}

/// GV-MAT-01..10 — retail Longsword (4.5 kg / 15 / 800) with each material.
const MAT_WEAPON_GOLDENS: [MatGolden; 10] = [
    MatGolden { id: "GV-MAT-01", armor: None, material: 0, label: "Iron", kg: 4.5, value: 45, cond: 800 },
    MatGolden { id: "GV-MAT-02", armor: None, material: 1, label: "Steel", kg: 5.5, value: 90, cond: 1200 },
    MatGolden { id: "GV-MAT-03", armor: None, material: 2, label: "Silver", kg: 4.5, value: 180, cond: 1200 },
    MatGolden { id: "GV-MAT-04", armor: None, material: 3, label: "Elven", kg: 4.5, value: 360, cond: 1600 },
    MatGolden { id: "GV-MAT-05", armor: None, material: 4, label: "Dwarven", kg: 3.5, value: 720, cond: 2400 },
    MatGolden { id: "GV-MAT-06", armor: None, material: 5, label: "Mithril", kg: 4.5, value: 1440, cond: 3200 },
    MatGolden { id: "GV-MAT-07", armor: None, material: 6, label: "Adamantium", kg: 4.5, value: 2880, cond: 4000 },
    MatGolden { id: "GV-MAT-08", armor: None, material: 7, label: "Ebony", kg: 2.25, value: 5760, cond: 4800 },
    MatGolden { id: "GV-MAT-09", armor: None, material: 8, label: "Orcish", kg: 4.5, value: 11520, cond: 5600 },
    MatGolden { id: "GV-MAT-10", armor: None, material: 9, label: "Daedric", kg: 5.5, value: 23040, cond: 6400 },
];

/// GV-MAT-11..15 — retail Cuirass (12.5 kg / 100 / 4096) per armor branch.
const MAT_ARMOR_GOLDENS: [MatGolden; 5] = [
    MatGolden { id: "GV-MAT-11", armor: Some(0x0000), material: 0, label: "Leather", kg: 6.25, value: 100, cond: 4096 },
    MatGolden { id: "GV-MAT-12", armor: Some(0x0200), material: 0, label: "Iron", kg: 12.5, value: 300, cond: 4096 },
    MatGolden { id: "GV-MAT-13", armor: Some(0x0201), material: 1, label: "Steel", kg: 15.5, value: 600, cond: 6144 },
    MatGolden { id: "GV-MAT-14", armor: Some(0x0204), material: 4, label: "Dwarven", kg: 9.5, value: 4800, cond: 12288 },
    MatGolden { id: "GV-MAT-15", armor: Some(0x0209), material: 9, label: "Daedric", kg: 15.5, value: 153600, cond: 32768 },
];

/// Applies the material to a fresh instance of the retail template `base`
/// and serializes the result with the SAME format as the golden (weight with
/// `{}`, condition `current/max` — R-21.3: the engine restores to the maximum).
fn mat_got(base: &ItemTemplate, golden: &MatGolden) -> String {
    let mut item = Item::from_template(base);
    match golden.armor {
        Some(native) => apply_armor_material(&mut item, native),
        None => apply_weapon_material(&mut item, golden.material),
    }
    format!(
        "kg={} value={} cond={}/{}",
        item.weight_kg, item.value, item.current_condition, item.max_condition
    )
}

fn mat_want(golden: &MatGolden) -> String {
    format!(
        "kg={} value={} cond={}/{}",
        golden.kg, golden.value, golden.cond, golden.cond
    )
}

fn block_mat_rows(templates: &[ItemTemplate]) -> Vec<Row> {
    let mut rows = Vec::with_capacity(15);
    let longsword = templates.get(120);
    let cuirass = templates.get(102);
    for golden in MAT_WEAPON_GOLDENS.iter().chain(MAT_ARMOR_GOLDENS.iter()) {
        let base = if golden.armor.is_some() {
            cuirass
        } else {
            longsword
        };
        let got = base.map_or_else(
            || "<base template missing>".to_string(),
            |base| mat_got(base, golden),
        );
        rows.push(row(
            golden.id,
            got,
            mat_want(golden),
            format!(
                "{} {} · ItemBuilder.cs:643-651 / :488-507 (spec §2.4)",
                if golden.armor.is_some() { "Cuirass" } else { "Longsword" },
                golden.label
            ),
        ));
    }
    rows
}

// ── block A · GV-CND (condition and weight §2.6) ────────────────────────

/// Seed of a `Dice100` whose first `success(20)` equals `success` —
/// deterministic, covers 0..99 in fewer than 10000 seeds (measured at runtime).
fn dice_seed(success: bool) -> u64 {
    (1u64..10_000)
        .find(|&seed| {
            let mut dice = Dice100::new(seed);
            dice.success(WEAR_REROLL_CHANCE) == success
        })
        .expect("Dice100 covers 0..99: seeds with and without success in 20 exist")
}

fn block_cnd_rows(templates: &[ItemTemplate]) -> Vec<Row> {
    let mut rows = Vec::with_capacity(8);
    let cases: [( &str, i32, i32, i32, &str); 5] = [
        ("GV-CND-01", 50, 50, 100, "dagger 50/50"),
        ("GV-CND-02", 25, 50, 50, "dagger 25/50"),
        ("GV-CND-03", 49, 50, 98, "dagger 49/50"),
        ("GV-CND-04", 1, 3, 33, "1/3 (truncated)"),
        ("GV-CND-05", 7, 0, 100, "max<=0 ⇒ 100% (DaggerfallUnityItem.cs:460-463)"),
    ];
    for (id, current, max, want, label) in cases {
        rows.push(row(
            id,
            condition_percent(current, max).to_string(),
            want.to_string(),
            format!("{label} · ConditionPercentage"),
        ));
    }

    // GV-CND-06: encumbrance limit = floor(Strength · 1.5) (spec §2.5).
    rows.push(row(
        "GV-CND-06",
        format!(
            "{}/{}/{}",
            max_encumbrance(50),
            max_encumbrance(100),
            max_encumbrance(33)
        ),
        "75/150/49".to_string(),
        "floor(1.5·STR) for STR 50/100/33 · FormulaHelper.cs:77-83",
    ));

    // GV-CND-07/08: effective weight of arrows = 0 (retail template 131).
    let arrow = templates.get(131);
    let (got07, detail07) = match arrow {
        Some(template) => {
            let item = Item::from_template(template);
            (
                effective_unit_weight_kg(&item, template).to_string(),
                format!(
                    "Arrow idx=131 base={} kg ⇒ hasNoEncumbrance ⇒ 0 (DaggerfallUnityItem.cs:667-672)",
                    template.base_weight_kg()
                ),
            )
        }
        None => ("<template 131 missing>".to_string(), "FALL.EXE without Arrow".to_string()),
    };
    rows.push(row("GV-CND-07", got07, "0".to_string(), detail07));

    let (got08, detail08) = match arrow {
        Some(template) => {
            let mut item = Item::from_template(template);
            item.stack_count = 20;
            (
                total_weight_kg(&item, template).to_string(),
                "0 effective · stack 20 ⇒ 0·20=0 (IsStackable :681-695)".to_string(),
            )
        }
        None => ("<template 131 missing>".to_string(), "FALL.EXE without Arrow".to_string()),
    };
    rows.push(row("GV-CND-08", got08, "0".to_string(), detail08));

    rows
}

// ── block A · GV-DUR (durability §4.2) ──────────────────────────────────

fn block_dur_rows() -> Vec<Row> {
    let mut rows = Vec::with_capacity(17);

    // GV-DUR-01: amount 0, with the 20 re-roll it yields 1 and without it 0.
    let hit = dice_seed(true);
    let miss = dice_seed(false);
    let mut dice_hit = Dice100::new(hit);
    let mut dice_miss = Dice100::new(miss);
    rows.push(row(
        "GV-DUR-01",
        format!(
            "amount={} roll<20={} roll>=20={}",
            wear_amount(4),
            wear_amount_rolled(4, &mut dice_hit),
            wear_amount_rolled(4, &mut dice_miss)
        ),
        "amount=0 roll<20=1 roll>=20=0".to_string(),
        format!(
            "(10·4+50)/100=0 · SuccessRoll(20) ⇒ 1 (FormulaHelper.cs:1130-1132) · seeds {hit}/{miss}"
        ),
    ));

    for (id, damage, want) in [
        ("GV-DUR-02", 5, 1),
        ("GV-DUR-03", 9, 1),
        ("GV-DUR-04", 10, 1),
        ("GV-DUR-05", 15, 2),
        ("GV-DUR-06", 25, 3),
    ] {
        rows.push(row(
            id,
            wear_amount(damage).to_string(),
            want.to_string(),
            format!("(10·{damage}+50)/100 truncated · FormulaHelper.cs:1130"),
        ));
    }

    // GV-DUR-07/08: application over the dagger condition (2/50 and 1/50).
    let mut current = 2;
    let mut dice = Dice100::new(1);
    let outcome = apply_wear(&mut current, 5, &mut dice);
    rows.push(row(
        "GV-DUR-07",
        format!("{outcome:?} current={current}"),
        "Worn { amount: 1, condition: 1 } current=1".to_string(),
        "dagger 2/50 takes 5 ⇒ 2-1=1 · DaggerfallUnityItem.cs:1170-1182",
    ));
    let mut current = 1;
    let mut dice = Dice100::new(1);
    let outcome = apply_wear(&mut current, 5, &mut dice);
    rows.push(row(
        "GV-DUR-08",
        format!("{outcome:?} current={current}"),
        "Broke { amount: 1 } current=0".to_string(),
        "dagger 1/50 takes 5 ⇒ 1-1=0 ⇒ ItemBreaks",
    ));

    for (id, value, condition, max, quality, want) in [
        ("GV-DUR-09", 45, 25, 50, 10, 8),
        ("GV-DUR-10", 45, 25, 50, 15, 8),
        ("GV-DUR-11", 5000, 25, 50, 20, 1100),
        ("GV-DUR-15", 45, 50, 50, 10, 0),
    ] {
        rows.push(row(
            id,
            repair_cost(value, condition, max, quality).to_string(),
            want.to_string(),
            format!(
                "value={value} cond={condition}/{max} q={quality} · FormulaHelper.cs:1885-1906"
            ),
        ));
    }

    for (id, condition, max, want) in [
        ("GV-DUR-12", 25, 50, 86_400),
        ("GV-DUR-13", 1000, 2000, 86_400),
        ("GV-DUR-14", 999, 2000, 86_486),
    ] {
        rows.push(row(
            id,
            repair_time_seconds(condition, max).to_string(),
            want.to_string(),
            format!(
                "damage={max}-{condition} · max(damage·86400/1000, 86400) · FormulaHelper.cs:1908-1917"
            ),
        ));
    }

    rows.push(row(
        "GV-DUR-16",
        days_remaining(216_000).to_string(),
        "3".to_string(),
        "ceil(216000/86400)=ceil(2.5)=3 · ItemRepairData.cs:88-93",
    ));

    // GV-DUR-17: spec erratum ORDER — the spec cites
    // 2_831_155_200 (= 32768·86400 BEFORE the /1000 of FormulaHelper.cs:1915);
    // the value of the formula (and of the engine) is 2_831_155 s.
    rows.push(row(
        "GV-DUR-17",
        repair_time_seconds_for_damage(32_768).to_string(),
        GV_DUR_17_SECONDS.to_string(),
        "damage=32768 ⇒ 32768·86400/1000 truncated · spec erratum ORDER (durability.rs:326-331)",
    ));

    rows
}

// ── PROBE-ITEM-001 (blocks A + C) ───────────────────────────────────────

/// `--probe ITEM-001` — block A (55 golden vectors) + block C (disk
/// census). `EXIT=2` if `FALL.EXE` is not readable at the installation root.
pub fn run_item_001() -> i32 {
    println!(
        "PROBE-ITEM-001 - items/material/condition/durability M21 (REQ-L99-054 · CR-776ad8a5 · spec §1/§2/§4)"
    );
    let data = match load_fall_exe() {
        Ok(data) => data,
        Err(reason) => {
            println!("{reason}");
            println!("EXIT=2");
            println!(
                "VERDICT: PROBE-ITEM-001 NO DATA - FALL.EXE unreadable at the installation root (blocks A+C not executed; never a PASS without retail)"
            );
            return 2;
        }
    };
    let offset = find_items_offset(&data);
    let templates = match parse_item_templates(&data) {
        Ok(templates) => templates,
        Err(err) => {
            println!("FALL.EXE without a readable template table: {err}");
            println!("EXIT=2");
            println!(
                "VERDICT: PROBE-ITEM-001 NO DATA - retail blob unreadable for parsing (blocks A+C not executed)"
            );
            return 2;
        }
    };

    let it_rows = block_it_rows(&data, offset, &templates);
    let mat_rows = block_mat_rows(&templates);
    let cnd_rows = block_cnd_rows(&templates);
    let dur_rows = block_dur_rows();

    let it_ok = print_rows("A1 · FALL.EXE templates §1.4 (GV-IT-01..15)", &it_rows);
    let mat_ok = print_rows("A2 · material §2.4 (GV-MAT-01..15)", &mat_rows);
    let cnd_ok = print_rows("A3 · condition and weight §2.6 (GV-CND-01..08)", &cnd_rows);
    let dur_ok = print_rows("A4 · durability §4.2 (GV-DUR-01..17)", &dur_rows);

    let checks = it_rows.len() + mat_rows.len() + cnd_rows.len() + dur_rows.len();
    let passed = it_rows.iter().filter(|row| row.ok).count()
        + mat_rows.iter().filter(|row| row.ok).count()
        + cnd_rows.iter().filter(|row| row.ok).count()
        + dur_rows.iter().filter(|row| row.ok).count();
    let failed = checks - passed;
    println!("checks={checks} passed={passed} failed={failed}");
    if checks != 55 {
        println!("FAIL probe_coverage got={checks} want=55 (golden vectors of block A)");
    }

    // ── Block C · disk census (spec §7) ─────────────────────────────────
    let block_len = TOTAL_ITEMS * ITEM_RECORD_SIZE;
    let census = offset
        .filter(|offset| *offset + block_len <= data.len())
        .and_then(|offset| data.get(offset..offset + block_len))
        .map(sha256_hex);
    let Some(sha256) = census else {
        println!("EXIT=2");
        println!(
            "VERDICT: PROBE-ITEM-001 NO DATA - 13824 B block unreadable (census not executed)"
        );
        return 2;
    };
    println!("\n-- C · disk census (FALL.EXE, spec §7) --");
    println!(
        "census: records={} first={} last={} offset={} sha256={sha256}",
        templates.len(),
        templates.first().map_or("<empty>", |t| t.name.as_str()),
        templates.last().map_or("<empty>", |t| t.name.as_str()),
        offset.unwrap_or(0),
    );

    let all_ok = it_ok && mat_ok && cnd_ok && dur_ok && checks == 55;
    let code = i32::from(!all_ok);
    println!("EXIT={code}");
    if all_ok {
        println!(
            "VERDICT: PROBE-ITEM-001 PASS - checks=55 passed=55 failed=0 (A 15+15+8+17) + census offset={EXPECTED_OFFSET}"
        );
    } else {
        println!("VERDICT: PROBE-ITEM-001 FAIL - see [FAIL] rows");
    }
    code
}

// ── block B · GV-EQP (equipment §3.1) ───────────────────────────────────

/// Synthetic item for block B (same template as the `equipment.rs` tests:
/// empty template with test data).
fn synthetic(index: usize, stack: i32) -> Item {
    let mut template = ItemTemplate::empty(index);
    template.name = format!("synthetic-{index}");
    template.hit_points = 100;
    template.base_weight_units = 4;
    template.base_price = 10;
    let mut item = Item::from_template(&template);
    item.stack_count = stack;
    item
}

/// Serializes the 27 uids of the table (`ItemEquipTable::serialize_uids`,
/// `ItemEquipTable.cs:331-345`; 0 = empty slot).
fn slots_line(table: &ItemEquipTable) -> String {
    let uids: Vec<String> = table.serialize_uids().iter().map(|uid| uid.to_string()).collect();
    format!("[{}]", uids.join(", "))
}

fn equipped_slot(attempt_result: EquipResult) -> (EquipSlot, Vec<usize>, Option<i32>) {
    match attempt_result {
        EquipResult::Equipped(attempt) => (
            attempt.slot,
            attempt
                .unequipped
                .iter()
                .map(|item| item.template_index)
                .collect(),
            attempt.remainder.as_ref().map(|item| item.stack_count),
        ),
        EquipResult::Rejected(item) => (EquipSlot::None, vec![item.template_index], None),
    }
}

fn block_equip_rows() -> (Vec<Row>, String) {
    let mut rows = Vec::with_capacity(10);

    // Sequence 01..05 over ONE table (pattern of spec §3.1 and of the golden
    // tests of `equipment.rs`).
    let mut table = ItemEquipTable::new();

    // GV-EQP-01: Longsword (Either) with free hands ⇒ RightHand (19).
    let (slot, displaced, remainder) = equipped_slot(table.equip(synthetic(120, 1), true));
    rows.push(row(
        "GV-EQP-01",
        format!("{slot:?}({}) displaced={displaced:?} remainder={remainder:?}", slot.index().map_or(-1, |i| i as i32)),
        "RightHand(19) displaced=[] remainder=None".to_string(),
        "GetWeaponSlot Either → GetFirstSlot(Right,Left) with Right free (ItemEquipTable.cs:435-456)",
    ));

    // GV-EQP-02: Dagger with Right occupied, Left free ⇒ LeftHand (21).
    let (slot, displaced, remainder) = equipped_slot(table.equip(synthetic(113, 1), true));
    rows.push(row(
        "GV-EQP-02",
        format!("{slot:?}({}) displaced={displaced:?} remainder={remainder:?}", slot.index().map_or(-1, |i| i as i32)),
        "LeftHand(21) displaced=[] remainder=None".to_string(),
        "Right occupied ⇒ GetFirstSlot falls back to Left (ItemEquipTable.cs:452)",
    ));

    // GV-EQP-03: Claymore (Both) unequips Left and Right BEFORE landing
    // (`:120-125`) — with Longsword+Dagger equipped, both come off (left
    // first) and the Claymore ends up in RightHand.
    let (slot, displaced, remainder) = equipped_slot(table.equip(synthetic(122, 1), true));
    rows.push(row(
        "GV-EQP-03",
        format!("{slot:?}({}) displaced={displaced:?} remainder={remainder:?}", slot.index().map_or(-1, |i| i as i32)),
        "RightHand(19) displaced=[113, 120] remainder=None".to_string(),
        "Both weapon ⇒ unequip Left and Right in that order (ItemEquipTable.cs:120-125)",
    ));

    // GV-EQP-04: Buckler (LeftOnly) with a Claymore equipped ⇒ it unequips it
    // and lands in LeftHand.
    let (slot, displaced, remainder) = equipped_slot(table.equip(synthetic(109, 1), true));
    rows.push(row(
        "GV-EQP-04",
        format!("{slot:?}({}) displaced={displaced:?} remainder={remainder:?}", slot.index().map_or(-1, |i| i as i32)),
        "LeftHand(21) displaced=[122] remainder=None".to_string(),
        "LeftOnly ⇒ unequips the Both weapon from the right hand (ItemEquipTable.cs:127-133)",
    ));

    // GV-EQP-05: Staff (Both) with a Buckler in Left ⇒ it unequips it.
    let (slot, displaced, remainder) = equipped_slot(table.equip(synthetic(115, 1), true));
    rows.push(row(
        "GV-EQP-05",
        format!("{slot:?}({}) displaced={displaced:?} remainder={remainder:?}", slot.index().map_or(-1, |i| i as i32)),
        "RightHand(19) displaced=[109] remainder=None".to_string(),
        "Both weapon ⇒ unequip from both hands (ItemEquipTable.cs:120-125)",
    ));

    // GV-EQP-06: arrows do not equip (slot None ⇒ rejection with the item
    // returned, 0 uids in the table).
    let mut table = ItemEquipTable::new();
    let (slot, rejected, _) = equipped_slot(table.equip(synthetic(131, 1), true));
    let all_empty = table.serialize_uids().iter().all(|&uid| uid == 0);
    rows.push(row(
        "GV-EQP-06",
        format!("{slot:?} rejected_idx={rejected:?} table_empty={all_empty}"),
        "None rejected_idx=[131] table_empty=true".to_string(),
        "GetItemHands(Arrow)=None ⇒ never reaches a slot (ItemEquipTable.cs:113-115)",
    ));

    // GV-EQP-07 (ERRATUM D-M21-4, ORDER): DFU resolves
    // `GetEquipSlot(Drugs)=None` (`ItemEquipTable.cs:100-106`) ⇒ `equip`
    // returns `None` BEFORE the split: a potion stack=5 does NOT equip and its
    // stack stays intact (5). The split branch is verified separately with a
    // synthetic stackable+equippable item (gem): equips 1, 4 remain.
    let mut table = ItemEquipTable::new();
    let potion = synthetic(78, 5);
    let potion_slot = table.get_equip_slot(&potion);
    let potion_state = match table.equip(potion, true) {
        EquipResult::Rejected(item) => {
            format!("Rejected(idx={},stack={})", item.template_index, item.stack_count)
        }
        EquipResult::Equipped(attempt) => format!("Equipped({:?}) without rejection", attempt.slot),
    };
    let gem_state = match table.equip(synthetic(0, 5), true) {
        EquipResult::Equipped(attempt) => format!(
            "{:?} equipped_stack={} remainder={}",
            attempt.slot,
            table.get(attempt.slot).map_or(-1, |item| item.stack_count),
            attempt.remainder.as_ref().map_or(-1, |item| item.stack_count)
        ),
        EquipResult::Rejected(item) => format!("Rejected(idx={}) without split", item.template_index),
    };
    rows.push(row(
        "GV-EQP-07",
        format!("potion_Drugs slot={potion_slot:?} {potion_state} · gem {gem_state}"),
        "potion_Drugs slot=None Rejected(idx=78,stack=5) · gem Crystal0 equipped_stack=1 remainder=4".to_string(),
        "erratum D-M21-4: Drugs ⇒ None before the split (ItemEquipTable.cs:100-106) · split with stackable+equippable (:104-106)",
    ));

    // GV-EQP-08/09: Cuirass ⇒ ChestArmor (18); Tower shield ⇒ LeftHand (21).
    let mut table = ItemEquipTable::new();
    let (slot, displaced, remainder) = equipped_slot(table.equip(synthetic(102, 1), true));
    rows.push(row(
        "GV-EQP-08",
        format!("{slot:?}({}) displaced={displaced:?} remainder={remainder:?}", slot.index().map_or(-1, |i| i as i32)),
        "ChestArmor(18) displaced=[] remainder=None".to_string(),
        "GetEquipSlot(armor 102)=ChestArmor (ItemEquipTable.cs:406-432)",
    ));
    let (slot, displaced, remainder) = equipped_slot(table.equip(synthetic(112, 1), true));
    rows.push(row(
        "GV-EQP-09",
        format!("{slot:?}({}) displaced={displaced:?} remainder={remainder:?}", slot.index().map_or(-1, |i| i as i32)),
        "LeftHand(21) displaced=[] remainder=None".to_string(),
        "shields 109..=112 ⇒ LeftHand (ItemEquipTable.cs:425-428)",
    ));

    // GV-EQP-10: bow with BowLeftHandWithSwitching=false ⇒ Both (two hands).
    let mut table = ItemEquipTable::new();
    let bow = synthetic(129, 1);
    let hands = item_hands(&bow);
    let (slot, displaced, remainder) = equipped_slot(table.equip(bow, true));
    rows.push(row(
        "GV-EQP-10",
        format!("{hands:?}({BOW_LEFT_HAND_WITH_SWITCHING}) {slot:?}({}) displaced={displaced:?} remainder={remainder:?}", slot.index().map_or(-1, |i| i as i32)),
        "Both(false) RightHand(19) displaced=[] remainder=None".to_string(),
        "BowLeftHandWithSwitching=false ⇒ Both (ItemEquipTable.cs:633-635)",
    ));
    let final_line = slots_line(&table);

    (rows, final_line)
}

/// `--probe EQUIP-001` — 10 equipment golden vectors over a synthetic
/// `ItemEquipTable` + dump of the 27 slots.
pub fn run_equip_001() -> i32 {
    println!(
        "PROBE-EQUIP-001 - 27-slot equipment table M21 (REQ-L99-054 · CR-776ad8a5 · spec §3)"
    );
    let (rows, slots) = block_equip_rows();
    let ok = print_rows("B · equipment §3.1 (GV-EQP-01..10)", &rows);
    let checks = rows.len();
    let passed = rows.iter().filter(|row| row.ok).count();
    let failed = checks - passed;
    println!("checks={checks} passed={passed}");
    if failed > 0 {
        println!("failed={failed}");
    }
    if checks != 10 {
        println!("FAIL probe_coverage got={checks} want=10 (golden vectors of block B)");
    }
    println!("slots(27)={slots}");

    let all_ok = ok && checks == 10;
    let code = i32::from(!all_ok);
    println!("EXIT={code}");
    if all_ok {
        println!(
            "VERDICT: PROBE-EQUIP-001 PASS - checks=10 passed=10 (erratum D-M21-4: potion does NOT equip, split with a stackable item)"
        );
    } else {
        println!("VERDICT: PROBE-EQUIP-001 FAIL - see [FAIL] rows");
    }
    code
}

// ── block D · GV-LOT (loot §5) ──────────────────────────────────────────

/// Block D seed: `--seed <u64>` if present; if the value does not parse a
/// warning is printed and the default is used (never a silent failure).
fn parse_loot_seed() -> u64 {
    let args: Vec<String> = std::env::args().collect();
    match args.iter().position(|arg| arg == "--seed") {
        Some(position) => match args.get(position + 1) {
            Some(raw) => match raw.parse::<u64>() {
                Ok(seed) => seed,
                Err(_) => {
                    println!("--seed invalid {raw:?}; using the default {LOOT_SEED_DEFAULT}");
                    LOOT_SEED_DEFAULT
                }
            },
            None => {
                println!("--seed without a value; using the default {LOOT_SEED_DEFAULT}");
                LOOT_SEED_DEFAULT
            }
        },
        None => LOOT_SEED_DEFAULT,
    }
}

/// Cap of a `while SuccessRoll((int)chance) { ...; chance *= 0.5 }` chain
/// (`LootTables.cs:201-255`): number of MAXIMUM successes — if the truncation
/// or the `·0.5` break, the cap blows up and the block D check fails
/// (anti-false-positive (c) of spec §7). For WP=5 it yields 3 (5,2,1,0) and
/// for RL=100 it yields 7 (100,50,25,12,6,3,1,0).
fn chain_cap(initial: i32) -> i32 {
    let mut chance = initial as f32;
    let mut cap = 0;
    while chance as i32 > 0 {
        cap += 1;
        chance *= 0.5;
    }
    cap
}

/// A complete block D run: SHA-256 digest of the 500 containers (canonical
/// concatenation `container_digest_bytes` in `container_id` order, spec §7) +
/// gold/item/category observations.
struct LootRun {
    digest: String,
    gold_min: i32,
    gold_max: i32,
    items_total: usize,
    weapons_max: i32,
    religious_max: i32,
}

fn run_loot(seed: u64) -> LootRun {
    let mut bytes = Vec::new();
    let mut gold_min = i32::MAX;
    let mut gold_max = i32::MIN;
    let mut items_total = 0usize;
    let mut weapons_max = 0;
    let mut religious_max = 0;
    for container_id in 0..LOOT_CONTAINERS {
        let (container, counts) =
            generate_container_with_counts(seed, container_id, LOOT_KEY, LOOT_LEVEL);
        bytes.extend_from_slice(&container_digest_bytes(&container));
        gold_min = gold_min.min(container.gold);
        gold_max = gold_max.max(container.gold);
        items_total += container.items.len();
        weapons_max = weapons_max.max(counts.weapons);
        religious_max = religious_max.max(counts.religious);
    }
    LootRun {
        digest: sha256_hex(&bytes),
        gold_min,
        gold_max,
        items_total,
        weapons_max,
        religious_max,
    }
}

/// `--probe LOOT-001` — 500 containers `K` level 5 with an explicit RNG per
/// container (spec §5/§7). `EXIT=0` ⟺ reproducible digest, digest sensitive
/// to the seed and correct limits.
pub fn run_loot_001() -> i32 {
    let seed = parse_loot_seed();
    let alt_seed = seed.wrapping_add(1);
    println!(
        "PROBE-LOOT-001 - deterministic loot M21 (REQ-L99-054 · CR-776ad8a5 · spec §5)"
    );
    println!("seed={seed} seed_alt={alt_seed} (default {LOOT_SEED_DEFAULT}; --seed <u64> changes it)");

    let primary = run_loot(seed);
    let repeat = run_loot(seed);
    let alternate = run_loot(alt_seed);

    let table = matrix(LOOT_KEY);
    let weapons_cap = chain_cap(table.wp);
    let religious_cap = chain_cap(table.rl);
    let items_mean = primary.items_total as f64 / f64::from(LOOT_CONTAINERS);

    println!("\n-- D · 500 containers key={LOOT_KEY} level {LOOT_LEVEL} (explicit Rng64 per container, 0 global RNG) --");
    println!(
        "key={LOOT_KEY} n={LOOT_CONTAINERS} gold_min={} gold_max={} items_mean={items_mean:.2} weapons_max={weapons_cap} religious_max={religious_cap}",
        primary.gold_min, primary.gold_max,
    );
    println!("digest={}", primary.digest);
    println!(
        "(WP/RL chain cap of matrix {LOOT_KEY}: wp={} rl={} ⇒ {weapons_cap}/{religious_cap}; observed over 500: weapons_max={} religious_max={})",
        table.wp, table.rl, primary.weapons_max, primary.religious_max,
    );

    let obs_ok = primary.weapons_max <= weapons_cap && primary.religious_max <= religious_cap;
    let rows = vec![
        row(
            "GV-LOT-digest-x2",
            format!("digest_a={} digest_b={}", primary.digest, repeat.digest),
            format!("digest_a={} digest_b={}", primary.digest, primary.digest),
            format!(
                "two runs with seed={seed} ⇒ SHA-256 identical byte by byte (spec §7 runtime ×2)"
            ),
        ),
        row(
            "GV-LOT-digest-seed",
            if alternate.digest == primary.digest {
                format!("digest_S={} digest_S'={}", primary.digest, alternate.digest)
            } else {
                format!(
                    "digest_S={} digest_S'≠digest_S",
                    primary.digest
                )
            },
            format!("digest_S={} digest_S'≠digest_S", primary.digest),
            format!("alternate seed seed_alt={alt_seed} ⇒ different digest (anti-false-positive (d))"),
        ),
        row(
            "GV-LOT-limites",
            format!(
                "gold_min={} gold_max={} weapons_cap={weapons_cap} religious_cap={religious_cap} obs_ok={obs_ok}",
                primary.gold_min, primary.gold_max,
            ),
            "gold_min=5 gold_max=50 weapons_cap=3 religious_cap=7 obs_ok=true".to_string(),
            format!(
                "gold Range(1,11)·5 ⇒ 5..50 (GV-LOT-01) · chain caps WP=5/RL=100 (GV-LOT-03/05) · observed weapons={} religious={}",
                primary.weapons_max, primary.religious_max,
            ),
        ),
    ];

    let ok = print_rows("D · loot §5.3 (GV-LOT block D)", &rows);
    let checks = rows.len();
    let passed = rows.iter().filter(|row| row.ok).count();
    let failed = checks - passed;
    println!("checks={checks} passed={passed} failed={failed}");

    let all_ok = ok && checks == 3;
    let code = i32::from(!all_ok);
    println!("EXIT={code}");
    if all_ok {
        println!(
            "VERDICT: PROBE-LOOT-001 PASS - checks=3 passed=3 failed=0 · digest ×2 identical with seed={seed} · 0 global RNG"
        );
    } else {
        println!("VERDICT: PROBE-LOOT-001 FAIL - see [FAIL] rows");
    }
    code
}
