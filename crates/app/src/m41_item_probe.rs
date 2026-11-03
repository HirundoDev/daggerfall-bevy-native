//! M41/W41-5 — `SYS-ITEM-001`: items and durability in depth, one gate.
//!
//! The density of the W41-5 row (roadmap §3.6) is measured against the rules
//! that already exist — 0 re-derivation, 0 second source of truth:
//!
//! - `daggerfall_inventory::items` — the 288-record `FALL.EXE` template table
//!   (`parse_item_templates`, `ItemsFile.cs:435-452`; layout spec M21 §1.2),
//!   the material tables (`ItemBuilder.cs:45,48,51`) and the two class entry
//!   points (`apply_weapon_material`, `:414-427`; `apply_armor_material`,
//!   `:488-507`), plus `material_armor_value`
//!   (`DaggerfallUnityItem.cs:1007-1058`);
//! - `daggerfall_behavior::durability` — wear per physical hit
//!   (`FormulaHelper.cs:1130-1132`), `apply_wear` break rule
//!   (`DaggerfallUnityItem.cs:1170-1182`), shop cost (`:1884-1899`), repair
//!   cost (`:1901-1922`), repair time (`:1924-1933`) and the repair record
//!   (`ItemRepairData.cs:23-93`);
//! - `daggerfall_inventory::equipment` — `damage_item`, the bridge that puts
//!   the wear rule behind `DamageEquipment` (`FormulaHelper.cs:1090`).
//!
//! Blocks (every row cites its rule):
//!
//! - **A · template census** — 288 `TEMPLATE <id> <name> group=<n>
//!   material_bit=<0xNNNN> weight=<kg> value=<n> src=<file:line>` rows over
//!   the retail blob at the installation root, the aggregate
//!   `CENSUS templates=<n> groups=<n> materials=<n>` and the invariants that
//!   make the census trustworthy (offset, record count, group coverage, the
//!   material mask ⟺ class rule, 10 material applications per weapon/armour
//!   template, name form, quarter-kilogram exactness).
//! - **B · condition and max condition** — one `CONDITION <class> <material>
//!   max=<n> formula=<expr> src=…` per weapon material (base 800) and per
//!   armour code (base 4096), `ConditionPercentage`, the refill-to-maximum
//!   rule (R-21.3) and the D-M21-5 material-condition delta.
//! - **C · repair** — `REPAIR value=… condition=… max=… quality=… cost=…
//!   seconds=… src=…` vectors including the D-M21-1 overflow delta
//!   (`32768*86400/1000 = 2_831_155` s, computed in `i64`, no `int32` wrap)
//!   and the repair-state arithmetic (`ready_at`, `days_remaining`).
//! - **D · wear per hit** — `WEAR damage=<n> wear=<n> src=…` vectors for
//!   damage −3, 0, 1, 2, 5, 10, 15, 25, 30 plus the roll-dependent branch
//!   (`SuccessRoll(20)` turning a zero into one) and the three `apply_wear`
//!   outcomes.
//! - **E · materials** — the weapon table Iron..Daedric and the thirteen
//!   armour codes Leather/Chain/Chain2/Iron..Daedric with their measured
//!   weight/value/condition/armour effects.
//! - **F · adversarial** — 0 condition, one break transition, the broken-item
//!   repair price, `i32`/`i64` overflow of wear and repair time, the cost
//!   clamp, the `damage <= 0` guard of `damage_item` and a condition above the
//!   maximum.
//!
//! Contract (single stdout, byte-identical in two independent processes):
//! first line `PROBE-SYS-ITEM-001 - <summary> (M41/W41-5, <CR>)`; a
//! `BLOCK <A..F> rows=<n>` header before each block where `<n>` is the exact
//! number of lines of that block; evidence rows `[OK] <id> - <detail>
//! (src=<file:line>)` or `FAIL <id> got=<v> want=<v> (src=<file:line>)`;
//! structured data rows of the families above; `ROWCENSUS rows=<n> ok=<n>
//! fail=<n> blocks=6`; `VEREDICTO: PASS`; the LAST line is
//! `DIGEST <sha256>` = sha256 of every byte printed before it. The runner-only
//! `PROBE_RUN` variable is never read nor printed.
//!
//! A structured data row is printed **only when the measured value equals its
//! cited golden**; a mismatch replaces it with a `FAIL` row, so a missing data
//! row is a hole the runner counts. `FALL.EXE` unreadable ⇒ `VEREDICTO: SKIP`
//! and exit 2 (never a PASS without retail). Blocks A and F read the blob; a
//! missing blob therefore skips the whole gate.
//!
//! **Gap closed by this lane:** the crate carried the material multipliers but
//! neither the class rule (`template_material_mask`, items.rs:886) nor the
//! material labels (`MATERIAL_NAMES` items.rs:839, `ARMOR_MATERIALS` items.rs:856,
//! lookups items.rs:895/901), so a template census could only have re-derived
//! the gate in the app. 0 arithmetic moved; the three tables are transcriptions
//! of `ItemEnums.cs:64-77` and `ItemBuilder.cs:488-507`.

use std::collections::BTreeSet;

use sha2::{Digest, Sha256};

use daggerfall_behavior::durability::{
    apply_wear, calculate_cost, days_remaining, repair_cost, repair_time_seconds,
    repair_time_seconds_for_damage, wear_amount, wear_amount_rolled, RepairData, WearOutcome,
    WEAR_REROLL_CHANCE,
};
use daggerfall_behavior::locomotion::Dice100;
use daggerfall_inventory::equipment::{EquipResult, EquipSlot, ItemEquipTable};
use daggerfall_inventory::items::{
    apply_armor_material, apply_weapon_material, armor_material_name, condition_percent,
    find_items_offset, material_armor_value, material_name, parse_item_templates, template_group,
    template_material_mask, weapon_material_modifier, Item, ItemGroup, ItemTemplate,
    ARMOR_MATERIALS, COND_MUL, DAMAGE_MOD, DEFAULT_ITEMS_OFFSET, FLAG_ARTIFACT, GROUP_COUNT,
    ITEM_RECORD_SIZE, MATERIAL_ALL, MATERIAL_COUNT, MATERIAL_NAMES, TO_HIT_MOD, TOTAL_ITEMS,
    VALUE_MUL, WEIGHT_MUL,
};

/// Lane CR (W41-5) — travels in the header line, the wiring comments and the
/// gate manifest.
pub const CR: &str = "CR-75eda300-4cb0-4d9f-bcc0-85cf60ac72c5";

/// Measured offset of the first template in THIS retail (spec M21 §1.1: the
/// `FindString("Ruby")` fallback; `DEFAULT_ITEMS_OFFSET` reads `0x40`).
const EXPECTED_OFFSET: usize = 1_796_138;
/// Byte measured at [`DEFAULT_ITEMS_OFFSET`] of THIS retail (GV-IT-01).
const DEFAULT_OFFSET_BYTE: u8 = 0x40;
/// End of the table = offset + 288·48 (spec §1.4 GV-IT-15).
const EXPECTED_TABLE_END: usize = 1_809_962;
/// Measured size of `FALL.EXE` (spec §0, re-measurement 2026-10-21).
const FALL_EXE_SIZE: usize = 1_864_183;
/// Longsword template index (GV-IT-12: 4.5 kg / 15 / 800) — weapon class base.
const LONGSWORD: usize = 120;
/// Cuirass template index (GV-IT-07: 12.5 kg / 100 / 4096) — armour class base.
const CUIRASS: usize = 102;
/// `32768 * 86400 / 1000`, truncated — D-M21-1 real value (never `2_831_155_200`,
/// which is the product BEFORE the `/1000` of `FormulaHelper.cs:1931`).
const D_M21_1_SECONDS: i64 = 2_831_155;

// ── rule citations (file:line of the rule each row exercises) ───────────────

const SRC_OFFSET: &str = "crates/inventory/src/items.rs:205";
const SRC_PARSE: &str = "crates/inventory/src/items.rs:268";
const SRC_TOTAL: &str = "crates/inventory/src/items.rs:250";
const SRC_GROUPS: &str = "crates/inventory/src/items.rs:396";
const SRC_GROUP: &str = "crates/inventory/src/items.rs:498";
const SRC_MASK: &str = "crates/inventory/src/items.rs:886";
const SRC_MATNAMES: &str = "crates/inventory/src/items.rs:839";
const SRC_ARMCODES: &str = "crates/inventory/src/items.rs:856";
const SRC_WEIGHT_MUL: &str = "crates/inventory/src/items.rs:65";
const SRC_COND_MUL: &str = "crates/inventory/src/items.rs:71";
const SRC_APPLY: &str = "crates/inventory/src/items.rs:685";
const SRC_WEAPON_MAT: &str = "crates/inventory/src/items.rs:702";
const SRC_ARMOR_MAT: &str = "crates/inventory/src/items.rs:712";
const SRC_WEAPON_MOD: &str = "crates/inventory/src/items.rs:727";
const SRC_ARMOR_VALUE: &str = "crates/inventory/src/items.rs:766";
const SRC_PERCENT: &str = "crates/inventory/src/items.rs:601";
const SRC_WEAR: &str = "crates/behavior/src/durability.rs:49";
const SRC_WEAR_ROLLED: &str = "crates/behavior/src/durability.rs:59";
const SRC_APPLY_WEAR: &str = "crates/behavior/src/durability.rs:85";
const SRC_COST: &str = "crates/behavior/src/durability.rs:117";
const SRC_REPAIR_COST: &str = "crates/behavior/src/durability.rs:129";
const SRC_REPAIR_TIME: &str = "crates/behavior/src/durability.rs:146";
const SRC_REPAIR_TIME_PAIR: &str = "crates/behavior/src/durability.rs:152";
const SRC_DAYS: &str = "crates/behavior/src/durability.rs:158";
const SRC_REPAIR_DATA: &str = "crates/behavior/src/durability.rs:176";
const SRC_IS_READY: &str = "crates/behavior/src/durability.rs:199";
const SRC_DAYS_LEFT: &str = "crates/behavior/src/durability.rs:204";
const SRC_DAMAGE_ITEM: &str = "crates/inventory/src/equipment.rs:560";

// ── evidence rows ───────────────────────────────────────────────────────────

/// One printed row: `[OK] id - detail (src=…)` or
/// `FAIL id got=<v> want=<v> (src=…)`.
struct Row {
    id: String,
    ok: bool,
    got: String,
    want: String,
    detail: String,
    src: &'static str,
}

impl Row {
    fn render(&self) -> String {
        if self.ok {
            format!("[OK] {} - {} · {} (src={})", self.id, self.detail, self.got, self.src)
        } else {
            format!("FAIL {} got={} want={} (src={})", self.id, self.got, self.want, self.src)
        }
    }
}

/// Row whose verdict is `got == want`.
fn eq_row(
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

/// Row whose verdict is an explicit predicate over measured values.
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

/// One block of the probe: a label plus the exact lines it prints.
struct Section {
    label: &'static str,
    lines: Vec<String>,
}

impl Section {
    fn new(label: &'static str) -> Self {
        Self {
            label,
            lines: Vec::new(),
        }
    }

    fn row(&mut self, row: Row) {
        self.lines.push(row.render());
    }

    fn data(&mut self, line: String) {
        self.lines.push(line);
    }
}

/// Prints `line` (a structured data row) when `matches`, else a `FAIL` row for
/// `id`: a structured row never survives a golden mismatch.
fn golden(
    section: &mut Section,
    id: impl Into<String>,
    matches: bool,
    line: String,
    got: impl Into<String>,
    want: impl Into<String>,
    src: &'static str,
) {
    if matches {
        section.data(line);
    } else {
        section.row(Row {
            id: id.into(),
            ok: false,
            got: got.into(),
            want: want.into(),
            detail: String::new(),
            src,
        });
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// Seed of a `Dice100` whose FIRST `success(20)` equals `success` — the same
/// deterministic search the M21 probe uses (`Dice100` covers 0..99).
///
/// `None` when no seed exists: the caller emits a red row instead of an
/// `expect` abort (exit 101 with no row at all).
fn dice_seed(success: bool) -> Option<u64> {
    (1u64..10_000).find(|&seed| {
        let mut dice = Dice100::new(seed);
        dice.success(WEAR_REROLL_CHANCE) == success
    })
}

/// Every `ItemGroup` variant (29, `GROUP_COUNT`) — cardinality checked in
/// block A, so a new variant cannot silently escape the group sweep.
const ALL_GROUPS: [ItemGroup; GROUP_COUNT] = [
    ItemGroup::None,
    ItemGroup::Drugs,
    ItemGroup::UselessItems1,
    ItemGroup::Armor,
    ItemGroup::Weapons,
    ItemGroup::MagicItems,
    ItemGroup::Artifacts,
    ItemGroup::MensClothing,
    ItemGroup::Books,
    ItemGroup::Furniture,
    ItemGroup::UselessItems2,
    ItemGroup::ReligiousItems,
    ItemGroup::Maps,
    ItemGroup::WomensClothing,
    ItemGroup::Paintings,
    ItemGroup::Gems,
    ItemGroup::PlantIngredients1,
    ItemGroup::PlantIngredients2,
    ItemGroup::CreatureIngredients1,
    ItemGroup::CreatureIngredients2,
    ItemGroup::CreatureIngredients3,
    ItemGroup::MiscellaneousIngredients1,
    ItemGroup::MetalIngredients,
    ItemGroup::MiscellaneousIngredients2,
    ItemGroup::Transportation,
    ItemGroup::Deeds,
    ItemGroup::Jewellery,
    ItemGroup::QuestItems,
    ItemGroup::MiscItems,
];

/// Expected material mask of a template per its class (`ItemBuilder.cs:414-427`
/// weapons, `:488-507` armour; every other class takes no material).
fn expected_mask(template_index: usize) -> u16 {
    match template_group(template_index) {
        ItemGroup::Weapons | ItemGroup::Armor => MATERIAL_ALL,
        _ => 0,
    }
}

/// Fresh instance of a retail template with `material` applied through the
/// class entry point (`None` when the template is absent).
fn materialized(templates: &[ItemTemplate], index: usize, material: usize) -> Option<Item> {
    let template = templates.get(index)?;
    let mut item = Item::from_template(template);
    if template_group(index) == ItemGroup::Armor {
        apply_armor_material(&mut item, 0x0200 + material as i32);
    } else {
        apply_weapon_material(&mut item, material);
    }
    Some(item)
}

// ── block A · template census ───────────────────────────────────────────────

fn block_a(templates: &[ItemTemplate], data: &[u8], offset: Option<usize>) -> Section {
    let mut section = Section::new("A");

    let offset_value = offset.unwrap_or(0);
    let default_byte = data.get(DEFAULT_ITEMS_OFFSET).copied().unwrap_or(0xFF);
    section.row(eq_row(
        "A-OFFSET",
        format!("offset={offset_value} default@{DEFAULT_ITEMS_OFFSET}={default_byte:#04x}"),
        format!(
            "offset={EXPECTED_OFFSET} default@{DEFAULT_ITEMS_OFFSET}={DEFAULT_OFFSET_BYTE:#04x}"
        ),
        format!(
            "live FindString(\"Ruby\") path, never the default · FALL.EXE={FALL_EXE_SIZE} B (spec §1.1)"
        ),
        SRC_OFFSET,
    ));

    let table_end = offset_value + TOTAL_ITEMS * ITEM_RECORD_SIZE;
    section.row(eq_row(
        "A-COUNT",
        format!(
            "records={} table_end={table_end} fits={}",
            templates.len(),
            data.len() >= table_end
        ),
        format!("records={TOTAL_ITEMS} table_end={EXPECTED_TABLE_END} fits=true"),
        "288 records x 48 B = 13824 B inside the blob (spec §1.4 GV-IT-15)",
        SRC_TOTAL,
    ));

    section.row(eq_row(
        "A-GROUP-CARDINALITY",
        ALL_GROUPS.len().to_string(),
        GROUP_COUNT.to_string(),
        "the sweep covers every ItemGroup variant (ItemEnums.cs:27-58)",
        SRC_GROUPS,
    ));

    // Every index resolves to a group that owns it, or to the declared
    // MiscItems catch-all (246, 250, 251, 266, 272, 273 per template_group).
    let mut owned = 0usize;
    let mut unowned = 0usize;
    let mut multi = 0usize;
    let mut group_mismatch = Vec::new();
    for index in 0..templates.len() {
        let owners = ALL_GROUPS
            .iter()
            .filter(|group| group_indices_contains(**group, index))
            .count();
        let resolved = template_group(index);
        if owners == 0 {
            unowned += 1;
            if resolved != ItemGroup::MiscItems {
                group_mismatch.push(index);
            }
        } else {
            owned += 1;
            if owners > 1 {
                multi += 1;
            }
            if !group_indices_contains(resolved, index) {
                group_mismatch.push(index);
            }
        }
    }
    section.row(predicate(
        "A-GROUP-COVERAGE",
        group_mismatch.is_empty() && owned + unowned == templates.len(),
        format!("indices={} owned={owned} unowned={unowned} multi={multi} mismatches={}", templates.len(), group_mismatch.len()),
        format!("indices={} mismatches=0", templates.len()),
        format!(
            "group_indices + template_group agree on every index; unowned={unowned} are the declared decor catch-all"
        ),
        SRC_GROUP,
    ));

    // The material mask is the class rule, not a second table.
    let mut mask_mismatch = 0usize;
    let mut material_class = 0usize;
    let mut inert = 0usize;
    for index in 0..templates.len() {
        let mask = template_material_mask(index);
        if mask == expected_mask(index) {
            if mask == MATERIAL_ALL {
                material_class += 1;
            } else {
                inert += 1;
            }
        } else {
            mask_mismatch += 1;
        }
    }
    section.row(predicate(
        "A-MASK-RULE",
        mask_mismatch == 0 && material_class + inert == templates.len(),
        format!(
            "material={material_class} inert={inert} mismatches={mask_mismatch} mask={MATERIAL_ALL:#06x}"
        ),
        format!("material={material_class} inert={inert} mismatches=0"),
        "material_bit is set exactly for Weapons/Armor (ItemBuilder.cs:414-427, :488-507)",
        SRC_MASK,
    ));

    // 10 material applications per weapon/armour template follow the table.
    let mut applications = 0usize;
    let mut value_mismatch = 0usize;
    let mut condition_mismatch = 0usize;
    let mut refill_mismatch = 0usize;
    let mut material_templates = 0usize;
    for index in 0..templates.len() {
        if template_material_mask(index) != MATERIAL_ALL {
            continue;
        }
        let Some(template) = templates.get(index) else {
            continue;
        };
        material_templates += 1;
        for material in 0..MATERIAL_COUNT {
            let Some(item) = materialized(templates, index, material) else {
                continue;
            };
            applications += 1;
            let want_value =
                i64::from(template.base_price) * 3 * i64::from(VALUE_MUL[material]);
            let want_condition = i64::from(template.hit_points) * i64::from(COND_MUL[material]) / 4;
            if i64::from(item.value) != want_value {
                value_mismatch += 1;
            }
            if i64::from(item.max_condition) != want_condition {
                condition_mismatch += 1;
            }
            if item.current_condition != item.max_condition {
                refill_mismatch += 1;
            }
        }
    }
    section.row(predicate(
        "A-MATERIAL-APPLY",
        value_mismatch == 0 && condition_mismatch == 0 && refill_mismatch == 0,
        format!(
            "templates={material_templates} applications={applications} value_mismatch={value_mismatch} max_mismatch={condition_mismatch} refill_mismatch={refill_mismatch}"
        ),
        format!("templates={material_templates} applications={applications} mismatches=0"),
        "value = base*3*VALUE_MUL, max = hp*COND_MUL/4, current refilled (ItemBuilder.cs:643-651)",
        SRC_APPLY,
    ));

    let empty = templates.iter().filter(|template| template.name.is_empty()).count();
    let non_ascii = templates
        .iter()
        .filter(|template| !template.name.bytes().all(|byte| (0x20..=0x7e).contains(&byte)))
        .count();
    section.row(predicate(
        "A-NAME-FORM",
        empty == 0 && non_ascii == 0 && templates.len() == TOTAL_ITEMS,
        format!("names={} empty={empty} non_ascii={non_ascii}", templates.len()),
        format!("names={TOTAL_ITEMS} empty=0 non_ascii=0"),
        "every retail record carries a printable ASCII name (24-byte NUL-padded field)",
        SRC_PARSE,
    ));

    let quarters = templates
        .iter()
        .filter(|template| template.base_weight_kg() * 4.0 != template.base_weight_units as f32)
        .count();
    section.row(predicate(
        "A-WEIGHT-EXACT",
        quarters == 0,
        format!("templates={} quarter_mismatch={quarters}", templates.len()),
        format!("templates={TOTAL_ITEMS} quarter_mismatch=0"),
        "kg = baseWeightUnits * 0.25 exactly, no rounding (ItemsFile.cs:330)",
        SRC_PARSE,
    ));

    // ── the 288 census rows ────────────────────────────────────────────────
    let mut groups: BTreeSet<i32> = BTreeSet::new();
    let mut masks: BTreeSet<u16> = BTreeSet::new();
    for template in templates {
        let group = template_group(template.index).dfu_value();
        let mask = template_material_mask(template.index);
        groups.insert(group);
        masks.insert(mask);
        let name_ok = !template.name.is_empty()
            && template.name.bytes().all(|byte| (0x20..=0x7e).contains(&byte));
        let mask_ok = mask == expected_mask(template.index);
        let weight_ok =
            template.base_weight_kg() * 4.0 == template.base_weight_units as f32;
        golden(
            &mut section,
            format!("A-TPL-{:03}", template.index),
            name_ok && mask_ok && weight_ok,
            format!(
                "TEMPLATE {} {} group={} material_bit={:#06x} weight={} value={} src={}",
                template.index,
                template.name,
                group,
                mask,
                template.base_weight_kg(),
                template.base_price,
                SRC_PARSE
            ),
            format!("name_ok={name_ok} mask_ok={mask_ok} weight_ok={weight_ok}"),
            "name_ok=true mask_ok=true weight_ok=true",
            SRC_PARSE,
        );
    }
    section.data(format!(
        "CENSUS templates={} groups={} materials={}",
        templates.len(),
        groups.len(),
        masks.len()
    ));

    section
}

/// `group_indices(group).contains(&index)` — the group sweep primitive.
fn group_indices_contains(group: ItemGroup, index: usize) -> bool {
    daggerfall_inventory::items::group_indices(group).contains(&index)
}

// ── block B · condition and max condition ───────────────────────────────────

fn block_b(templates: &[ItemTemplate]) -> Section {
    let mut section = Section::new("B");

    // Weapon class: Longsword (800 base) per material, `hp*COND_MUL/4`.
    for (material, &cond_mul) in COND_MUL.iter().enumerate() {
        let golden_max = 800 * cond_mul / 4;
        let measured = materialized(templates, LONGSWORD, material)
            .map(|item| item.max_condition)
            .unwrap_or(-1);
        let name = material_name(material).unwrap_or("<none>");
        golden(
            &mut section,
            format!("B-WEAPON-{material}"),
            measured == golden_max,
            format!(
                "CONDITION weapon {material} {name} max={measured} formula=800*{cond_mul}/4 src={}",
                SRC_COND_MUL
            ),
            measured.to_string(),
            golden_max.to_string(),
            SRC_COND_MUL,
        );
    }

    // Armour class: Cuirass (4096 base). The three non-plate branches first.
    for (code, name, formula) in [
        (0x0000_i32, "Leather", "4096*1"),
        (0x0100_i32, "Chain", "4096*1"),
        (0x0103_i32, "Chain2", "4096*1"),
    ] {
        let measured = templates
            .get(CUIRASS)
            .map(|template| {
                let mut item = Item::from_template(template);
                apply_armor_material(&mut item, code);
                item.max_condition
            })
            .unwrap_or(-1);
        golden(
            &mut section,
            format!("B-ARMOUR-{code:04x}"),
            measured == 4096,
            format!(
                "CONDITION armour 0x{code:04x} {name} max={measured} formula={formula} src={SRC_ARMOR_MAT}"
            ),
            measured.to_string(),
            "4096".to_string(),
            SRC_ARMOR_MAT,
        );
    }
    // The ten plates: `hp*COND_MUL/4` through `0x0200 + material`.
    for (material, &cond_mul) in COND_MUL.iter().enumerate() {
        let code = 0x0200 + material as i32;
        let golden_max = 4096 * cond_mul / 4;
        let measured = templates
            .get(CUIRASS)
            .map(|template| {
                let mut item = Item::from_template(template);
                apply_armor_material(&mut item, code);
                item.max_condition
            })
            .unwrap_or(-1);
        let name = armor_material_name(code).unwrap_or("<none>");
        golden(
            &mut section,
            format!("B-ARMOUR-{code:04x}"),
            measured == golden_max,
            format!(
                "CONDITION armour 0x{code:04x} {name} max={measured} formula=4096*{cond_mul}/4 src={SRC_COND_MUL}"
            ),
            measured.to_string(),
            golden_max.to_string(),
            SRC_COND_MUL,
        );
    }

    // ConditionPercentage (DaggerfallUnityItem.cs:460-463).
    let percent_cases = [(50, 50), (25, 50), (1, 3), (0, 50), (7, 0)];
    section.row(eq_row(
        "B-PERCENT",
        percent_cases
            .iter()
            .map(|(current, max)| format!("{current}/{max}={}", condition_percent(*current, *max)))
            .collect::<Vec<_>>()
            .join(" "),
        "50/50=100 25/50=50 1/3=33 0/50=0 7/0=100".to_string(),
        "integer percent truncated; max<=0 reports 100 (DaggerfallUnityItem.cs:460-463)",
        SRC_PERCENT,
    ));

    // D-M21-5: Longsword steel ⇒ max 1200; 10 hits x wear_amount(15)=2.
    let steel = materialized(templates, LONGSWORD, 1);
    let (max_condition, condition, percent) = match steel {
        Some(template_item) => {
            let mut item = template_item;
            for _ in 0..10 {
                item.current_condition -= wear_amount(15);
            }
            (
                item.max_condition,
                item.current_condition,
                condition_percent(item.current_condition, item.max_condition),
            )
        }
        None => (-1, -1, -1),
    };
    section.row(eq_row(
        "B-DM21-5",
        format!(
            "max={max_condition} hits=10 wear={} delta={} condition={condition} percent={percent}",
            wear_amount(15),
            -10 * wear_amount(15)
        ),
        "max=1200 hits=10 wear=2 delta=-20 condition=1180 percent=98".to_string(),
        "steel multiplies the 800 base by 6/4 (D-M21-5: 98 %, not 97 %)",
        SRC_COND_MUL,
    ));

    // Condition base per item class = the template hit points (GV-IT-07..12).
    let bases: Vec<String> = [CUIRASS, 107, 108, 113, 115, LONGSWORD]
        .iter()
        .map(|index| {
            templates
                .get(*index)
                .map(|template| format!("{}={}", template.name.replace(' ', "_"), template.hit_points))
                .unwrap_or_else(|| format!("{index}=<missing>"))
        })
        .collect();
    section.row(eq_row(
        "B-ITEM-CLASS",
        bases.join(" "),
        "Cuirass=4096 Helm=2560 Boots=2560 Dagger=50 Staff=300 Longsword=800".to_string(),
        "max_condition starts at the record hitPoints (DaggerfallUnityItem.cs:566-567)",
        SRC_PARSE,
    ));

    section
}

// ── block C · repair ────────────────────────────────────────────────────────

fn block_c() -> Section {
    let mut section = Section::new("C");

    // (value, condition, max, quality) -> cited goldens (GV-DUR-09..17).
    let vectors: [(i32, i32, i32, i32, i32, i64); 9] = [
        (45, 25, 50, 10, 8, 86_400),
        (45, 25, 50, 15, 8, 86_400),
        (5_000, 25, 50, 20, 1_100, 86_400),
        (45, 50, 50, 10, 0, 86_400),
        (0, 1, 10, 10, 2, 86_400),
        (100, 1_000, 2_000, 10, 20, 86_400),
        (100, 999, 2_000, 10, 20, 86_486),
        (500, 25, 50, 20, 110, 86_400),
        (45, 0, 32_768, 10, 8, D_M21_1_SECONDS),
    ];
    for (position, (value, condition, max, quality, cost, seconds)) in
        vectors.into_iter().enumerate()
    {
        let got_cost = repair_cost(value, condition, max, quality);
        let got_seconds = repair_time_seconds(condition, max);
        golden(
            &mut section,
            format!("C-REPAIR-{position}"),
            got_cost == cost && got_seconds == seconds,
            format!(
                "REPAIR value={value} condition={condition} max={max} quality={quality} cost={got_cost} seconds={got_seconds} src={SRC_REPAIR_COST}"
            ),
            format!("cost={got_cost} seconds={got_seconds}"),
            format!("cost={cost} seconds={seconds}"),
            SRC_REPAIR_COST,
        );
    }

    // condition > max still prices like an unrepaired item (`:1908-1909` is ==).
    section.row(eq_row(
        "C-OVER-MAX",
        repair_cost(45, 51, 50, 10).to_string(),
        "8".to_string(),
        "condition 51/50 != max, so the item is priced (FormulaHelper.cs:1908-1909)",
        SRC_REPAIR_COST,
    ));

    // DaysUntilRepaired: ceil(seconds/86400), never negative.
    section.row(eq_row(
        "C-DAYS-CEIL",
        [216_000_i64, 86_400, 86_401, 0, -1]
            .iter()
            .map(|seconds| days_remaining(*seconds).to_string())
            .collect::<Vec<_>>()
            .join("/"),
        "3/1/2/0/0".to_string(),
        "ceil(2.5)=3, 1, ceil(1.00001)=2, 0, 0 (ItemRepairData.cs:88-93)",
        SRC_DAYS,
    ));

    let repair = RepairData::new(1_000, 86_400);
    section.row(eq_row(
        "C-READY-AT",
        format!(
            "ready_at={} ready_87399={} ready_87400={}",
            repair.ready_at(),
            repair.is_ready(87_399),
            repair.is_ready(87_400)
        ),
        "ready_at=87400 ready_87399=false ready_87400=true".to_string(),
        "timeStarted + RepairTime, ready once now >= done (ItemRepairData.cs:33-36, :54-57)",
        SRC_IS_READY,
    ));

    section.row(eq_row(
        "C-DAYS-LEFT",
        format!(
            "at_1000={} at_ready={}",
            repair.days_remaining(1_000),
            repair.days_remaining(87_400)
        ),
        "at_1000=1 at_ready=0".to_string(),
        "DaysUntilRepaired on the record: ceil((done - now)/86400) (ItemRepairData.cs:88-93)",
        SRC_DAYS_LEFT,
    ));

    let not_left = RepairData::new(0, 86_400);
    section.row(eq_row(
        "C-NOT-STARTED",
        format!("ready={}", not_left.is_ready(1_000_000)),
        "ready=false".to_string(),
        "timeStarted == 0 means not left for repair (ItemRepairData.cs:64-68)",
        SRC_REPAIR_DATA,
    ));

    section.row(eq_row(
        "C-FLOOR",
        format!(
            "pair={} negative={}",
            repair_time_seconds(60, 50),
            repair_time_seconds_for_damage(-5)
        ),
        "pair=86400 negative=86400".to_string(),
        "the one-day floor holds for damage 0 and for negative damage (:1932)",
        SRC_REPAIR_TIME_PAIR,
    ));

    section
}

// ── block D · wear per hit ──────────────────────────────────────────────────

fn block_d() -> Section {
    let mut section = Section::new("D");

    let goldens: [(i32, i32); 9] = [
        (-3, 0),
        (0, 0),
        (1, 0),
        (2, 0),
        (5, 1),
        (10, 1),
        (15, 2),
        (25, 3),
        (30, 3),
    ];
    for (damage, want) in goldens {
        let got = wear_amount(damage);
        golden(
            &mut section,
            format!("D-WEAR-{damage}"),
            got == want,
            format!("WEAR damage={damage} wear={got} src={SRC_WEAR}"),
            got.to_string(),
            want.to_string(),
            SRC_WEAR,
        );
    }

    // The 20 % re-roll turns a zero into one point (FormulaHelper.cs:1131-1132).
    let (Some(hit_seed), Some(miss_seed)) = (dice_seed(true), dice_seed(false)) else {
        section.row(eq_row(
            "D-SEEDS",
            "no-seed",
            "seed",
            "Dice100 must yield seeds with and without a success in 20",
            SRC_WEAR_ROLLED,
        ));
        return section;
    };
    for damage in [0, 4] {
        let mut hit_dice = Dice100::new(hit_seed);
        let mut miss_dice = Dice100::new(miss_seed);
        let with_roll = wear_amount_rolled(damage, &mut hit_dice);
        let without_roll = wear_amount_rolled(damage, &mut miss_dice);
        section.row(eq_row(
            format!("D-REROLL-{damage}"),
            format!(
                "base={} roll_lt_20={with_roll} roll_ge_20={without_roll}",
                wear_amount(damage)
            ),
            "base=0 roll_lt_20=1 roll_ge_20=0".to_string(),
            format!("damage {damage}: (10*d+50)/100=0, SuccessRoll(20) re-rolls to 1 · seeds {hit_seed}/{miss_seed}"),
            SRC_WEAR_ROLLED,
        ));
    }

    // apply_wear: Worn, Broke, Unchanged and the re-roll branch.
    let mut condition = 2;
    let mut dice = Dice100::new(1);
    let worn = apply_wear(&mut condition, 5, &mut dice);
    section.row(eq_row(
        "D-APPLY-WORN",
        format!("{worn:?} condition={condition}"),
        "Worn { amount: 1, condition: 1 } condition=1".to_string(),
        "2 - 1 = 1, still alive (DaggerfallUnityItem.cs:1170-1182)",
        SRC_APPLY_WEAR,
    ));
    let mut condition = 1;
    let mut dice = Dice100::new(1);
    let broke = apply_wear(&mut condition, 5, &mut dice);
    section.row(eq_row(
        "D-APPLY-BROKE",
        format!("{broke:?} condition={condition}"),
        "Broke { amount: 1 } condition=0".to_string(),
        "1 - 1 = 0 ⇒ ItemBreaks and the counter clamps at 0",
        SRC_APPLY_WEAR,
    ));
    let mut condition = 10;
    let mut dice = Dice100::new(miss_seed);
    let unchanged = apply_wear(&mut condition, 4, &mut dice);
    section.row(eq_row(
        "D-APPLY-UNCHANGED",
        format!("{unchanged:?} condition={condition}"),
        "Unchanged condition=10".to_string(),
        "zero wear with a failed re-roll changes nothing",
        SRC_APPLY_WEAR,
    ));
    let mut condition = 10;
    let mut dice = Dice100::new(hit_seed);
    let rerolled = apply_wear(&mut condition, 4, &mut dice);
    section.row(eq_row(
        "D-APPLY-REROLL",
        format!("{rerolled:?} condition={condition}"),
        "Worn { amount: 1, condition: 9 } condition=9".to_string(),
        "the re-roll makes the same input wear one point",
        SRC_APPLY_WEAR,
    ));
    section.row(predicate(
        "D-DICE-IGNORED",
        (1u64..=64).all(|seed| {
            wear_amount_rolled(5, &mut Dice100::new(seed)) == 1
                && wear_amount_rolled(25, &mut Dice100::new(seed)) == 3
        }),
        "seeds=64 damage5=1 damage25=3".to_string(),
        "seeds=64 damage5=1 damage25=3".to_string(),
        "non-zero wear never consults the dice",
        SRC_WEAR_ROLLED,
    ));
    let sequence = |seed: u64| {
        let mut dice = Dice100::new(seed);
        (0..5).map(|_| dice.roll()).collect::<Vec<_>>()
    };
    let first = sequence(7);
    let replayed = sequence(7);
    let other = sequence(8);
    section.row(eq_row(
        "D-DETERMINISM",
        format!(
            "replay={} distinct_seed={} stream={:?}",
            first == replayed,
            first != other,
            first
        ),
        format!(
            "replay=true distinct_seed=true stream={:?}",
            sequence(7)
        ),
        "the same seed replays the same stream, another seed does not: 0 global RNG state",
        SRC_WEAR_ROLLED,
    ));

    section
}

// ── block E · materials ─────────────────────────────────────────────────────

fn block_e(templates: &[ItemTemplate]) -> Section {
    let mut section = Section::new("E");

    // Weapon table, measured on the Longsword (spec §2.4 GV-MAT-01..10).
    for material in 0..MATERIAL_COUNT {
        let name = material_name(material).unwrap_or("<none>");
        let item = materialized(templates, LONGSWORD, material);
        let (kg, value, max) = item
            .as_ref()
            .map(|item| (format!("{}", item.weight_kg), item.value, item.max_condition))
            .unwrap_or_else(|| ("<missing>".to_string(), -1, -1));
        let golden_value = 15 * 3 * VALUE_MUL[material];
        let golden_max = 800 * COND_MUL[material] / 4;
        golden(
            &mut section,
            format!("E-WEAPON-{material}"),
            value == golden_value && max == golden_max,
            format!(
                "MATERIAL weapon {material} {name} value_mul={} weight_mul={} cond_mul={} to_hit={} damage={} kg={kg} value={value} max={max} src={SRC_WEAPON_MAT}",
                VALUE_MUL[material], WEIGHT_MUL[material], COND_MUL[material], TO_HIT_MOD[material], DAMAGE_MOD[material]
            ),
            format!("value={value} max={max}"),
            format!("value={golden_value} max={golden_max}"),
            SRC_WEAPON_MAT,
        );
    }

    // Armour branches, measured on the Cuirass (spec §2.4 GV-MAT-11..15).
    let armour_goldens: [i32; 13] = [3, 6, 6, 7, 9, 9, 11, 13, 15, 15, 17, 19, 21];
    for (position, (code, name)) in ARMOR_MATERIALS.iter().copied().enumerate() {
        let measured = templates.get(CUIRASS).map(|template| {
            let mut item = Item::from_template(template);
            apply_armor_material(&mut item, code);
            (format!("{}", item.weight_kg), item.value, item.max_condition, material_armor_value(&item))
        });
        let (kg, value, max, armour) = measured.unwrap_or(("<missing>".to_string(), -1, -1, -1));
        golden(
            &mut section,
            format!("E-ARMOUR-{code:04x}"),
            armour == armour_goldens[position],
            format!(
                "MATERIAL armour 0x{code:04x} {name} armour={armour} kg={kg} value={value} max={max} src={SRC_ARMOR_VALUE}"
            ),
            armour.to_string(),
            armour_goldens[position].to_string(),
            SRC_ARMOR_VALUE,
        );
    }

    section.row(eq_row(
        "E-ARMOUR-TABLE",
        ARMOR_MATERIALS
            .iter()
            .map(|(code, name)| format!("0x{code:04x}={name}"))
            .collect::<Vec<_>>()
            .join(","),
        "0x0000=Leather,0x0100=Chain,0x0103=Chain2,0x0200=Iron,0x0201=Steel,0x0202=Silver,0x0203=Elven,0x0204=Dwarven,0x0205=Mithril,0x0206=Adamantium,0x0207=Ebony,0x0208=Orcish,0x0209=Daedric".to_string(),
        "the three non-plate branches then the ten plates (ItemBuilder.cs:488-507)",
        SRC_ARMCODES,
    ));

    section.row(eq_row(
        "E-WEAPON-TABLE",
        format!(
            "weight={WEIGHT_MUL:?} value={VALUE_MUL:?} cond={COND_MUL:?} to_hit={TO_HIT_MOD:?} damage={DAMAGE_MOD:?}"
        ),
        "weight=[4, 5, 4, 4, 3, 4, 4, 2, 4, 5] value=[1, 2, 4, 8, 16, 32, 64, 128, 256, 512] cond=[4, 6, 6, 8, 12, 16, 20, 24, 28, 32] to_hit=[-10, 0, 0, 10, 20, 30, 30, 40, 50, 60] damage=[-1, 0, 0, 1, 2, 3, 3, 4, 5, 6]".to_string(),
        "the five material tables, transcribed from ItemBuilder.cs:45,48,51 and spec §2.2",
        SRC_WEIGHT_MUL,
    ));

    section.row(eq_row(
        "E-SHIELD",
        [109_usize, 110, 111, 112]
            .iter()
            .map(|index| {
                templates
                    .get(*index)
                    .map(|template| {
                        let mut item = Item::from_template(template);
                        apply_armor_material(&mut item, 0x0200);
                        material_armor_value(&item).to_string()
                    })
                    .unwrap_or_else(|| "<missing>".to_string())
            })
            .collect::<Vec<_>>()
            .join("/"),
        "1/2/3/4".to_string(),
        "Buckler 1, Round 2, Kite 3, Tower 4 (DaggerfallUnityItem.cs:1007-1058)",
        SRC_ARMOR_VALUE,
    ));

    let artifact = templates.get(CUIRASS).map(|template| {
        let mut item = Item::from_template(template);
        apply_armor_material(&mut item, 0x0209);
        item.flags |= FLAG_ARTIFACT;
        material_armor_value(&item)
    });
    section.row(eq_row(
        "E-ARTIFACT",
        artifact.map_or_else(|| "<missing>".to_string(), |value| value.to_string()),
        "10".to_string(),
        "artifact armour halves 21 ⇒ 10 (integer, DaggerfallUnityItem.cs:1052-1057)",
        SRC_ARMOR_VALUE,
    ));

    section.row(eq_row(
        "E-TABLES",
        format!(
            "materials={} names={} armour_codes={} first={} last={} code_0209={} code_020a={}",
            MATERIAL_COUNT,
            MATERIAL_NAMES.len(),
            ARMOR_MATERIALS.len(),
            material_name(0).unwrap_or("<none>"),
            material_name(MATERIAL_COUNT - 1).unwrap_or("<none>"),
            armor_material_name(0x0209).unwrap_or("<none>"),
            armor_material_name(0x020a).unwrap_or("<none>"),
        ),
        "materials=10 names=10 armour_codes=13 first=Iron last=Daedric code_0209=Daedric code_020a=<none>"
            .to_string(),
        "WeaponMaterialTypes (ItemEnums.cs:64-77) as labels, armoured by the 13 codes below",
        SRC_MATNAMES,
    ));

    section.row(eq_row(
        "E-MOD-OUT-OF-RANGE",
        format!(
            "ten={} minus_one={}",
            weapon_material_modifier(10),
            weapon_material_modifier(-1)
        ),
        "ten=0 minus_one=0".to_string(),
        "no material index ⇒ no damage bonus (DaggerfallUnityItem.cs:979-1005)",
        SRC_WEAPON_MOD,
    ));

    section
}

// ── block F · adversarial ───────────────────────────────────────────────────

fn block_f() -> Section {
    let mut section = Section::new("F");

    // (1) A broken item never goes negative and keeps reporting the break.
    let mut condition = 0;
    let mut dice = Dice100::new(1);
    let mut brokes = 0usize;
    let mut minimum = 0;
    for _ in 0..8 {
        if let WearOutcome::Broke { .. } = apply_wear(&mut condition, 15, &mut dice) {
            brokes += 1;
        }
        minimum = minimum.min(condition);
    }
    section.row(eq_row(
        "F-ZERO-FLOOR",
        format!("min={minimum} hits=8 broke={brokes}"),
        "min=0 hits=8 broke=8".to_string(),
        "at 0 condition every further hit re-reports ItemBreaks, the counter never goes negative",
        SRC_APPLY_WEAR,
    ));

    // (2) Exactly one alive->broken transition in a 3-hit sequence.
    let mut condition = 3;
    let mut dice = Dice100::new(1);
    let mut transitions = 0usize;
    let mut alive = true;
    for _ in 0..3 {
        if let WearOutcome::Broke { .. } = apply_wear(&mut condition, 5, &mut dice) {
            if alive {
                transitions += 1;
                alive = false;
            }
        }
    }
    let after = apply_wear(&mut condition, 5, &mut Dice100::new(1));
    section.row(eq_row(
        "F-BREAK-ONCE",
        format!(
            "transitions={transitions} condition={condition} fourth_reports_broke={}",
            matches!(after, WearOutcome::Broke { .. })
        ),
        "transitions=1 condition=0 fourth_reports_broke=true".to_string(),
        "one transition per break; the source re-triggers ItemBreaks on a later hit",
        SRC_APPLY_WEAR,
    ));

    // (3) Repairing a broken item follows the measured rule (condition 0).
    section.row(eq_row(
        "F-REPAIR-BROKEN",
        format!(
            "cost={} seconds={} price_at_zero={}",
            repair_cost(45, 0, 50, 10),
            repair_time_seconds(0, 50),
            repair_cost(45, 0, 50, 10) > 0
        ),
        "cost=8 seconds=86400 price_at_zero=true".to_string(),
        "10*45/100=4, CalculateCost(4,10)=8; damage 50 ⇒ 4320 s floored to one day",
        SRC_REPAIR_COST,
    ));

    // (4) i64 arithmetic: the D-M21-1 product never wraps.
    let huge = repair_time_seconds_for_damage(i64::from(i32::MAX));
    section.row(predicate(
        "F-OVERFLOW-TIME",
        huge == 185_542_587_100 && huge > i64::from(i32::MAX) / 1000,
        format!(
            "seconds={huge} gt_i32_ms={} wrapped_would_be_lt_0={}",
            huge > i64::from(i32::MAX) / 1000,
            huge < 0
        ),
        "seconds=185542587100 gt_i32_ms=true wrapped_would_be_lt_0=false".to_string(),
        "damage i32::MAX * 86400 / 1000 in i64 (D-M21-1, no unchecked wrap)",
        SRC_REPAIR_TIME,
    ));

    // (5) wear_amount at the int32 edge: the 10*d+50 product is i64.
    section.row(eq_row(
        "F-OVERFLOW-WEAR",
        format!(
            "max={} min={}",
            wear_amount(i32::MAX),
            wear_amount(i32::MIN)
        ),
        "max=214748365 min=0".to_string(),
        "(10*i32::MAX+50)/100 computed in i64; a negative damage deals no wear",
        SRC_WEAR,
    ));

    // (6) The shelf-cost product clamps instead of wrapping.
    section.row(eq_row(
        "F-COST-CLAMP",
        calculate_cost(i32::MAX, 20).to_string(),
        i32::MAX.to_string(),
        "2*(c*(q-10)/100+c) evaluated in i64 and clamped to i32 (defensive, no retail input reaches it)",
        SRC_COST,
    ));

    // (7) damage <= 0 never consumes a roll (DamageEquipment guard `:1090`).
    let mut template = ItemTemplate::empty(113);
    template.name = "adversarial-dagger".to_string();
    template.hit_points = 50;
    template.base_price = 1;
    let mut table = ItemEquipTable::new();
    let equipped = matches!(
        table.equip(Item::from_template(&template), true),
        EquipResult::Equipped(_)
    );
    let mut dice = Dice100::new(11);
    let refused = table.damage_item(EquipSlot::RightHand, 0, &mut dice).is_none();
    let after_zero = dice.roll();
    let fresh = Dice100::new(11).roll();
    section.row(eq_row(
        "F-GATE-DAMAGE0",
        format!(
            "equipped={equipped} refused={refused} rolls_consumed={}",
            i32::from(after_zero != fresh)
        ),
        "equipped=true refused=true rolls_consumed=0".to_string(),
        "damage 0 returns None before any roll (DamageEquipment guard FormulaHelper.cs:1090)",
        SRC_DAMAGE_ITEM,
    ));

    // (8) A condition above the maximum is still priced.
    section.row(eq_row(
        "F-REPAIR-OVER-MAX",
        format!("cost={}", repair_cost(45, 1200, 800, 10)),
        "cost=8".to_string(),
        "condition != max ⇒ priced, even above the maximum (FormulaHelper.cs:1908-1909)",
        SRC_REPAIR_COST,
    ));

    // (9) A negative damage value deals no wear at all.
    let Some(miss_seed) = dice_seed(false) else {
        section.row(eq_row(
            "F-NEGATIVE-DAMAGE",
            "no-seed",
            "seed",
            "Dice100 must yield a seed without a success in 20",
            SRC_WEAR,
        ));
        return section;
    };
    section.row(eq_row(
        "F-NEGATIVE-DAMAGE",
        format!(
            "amount={} rolled={}",
            wear_amount(-1_000),
            wear_amount_rolled(-1_000, &mut Dice100::new(miss_seed))
        ),
        "amount=0 rolled=0".to_string(),
        "negative damage is guarded to 0 before the re-roll",
        SRC_WEAR,
    ));

    section
}

// ── probe entry ─────────────────────────────────────────────────────────────

/// `--probe SYS-ITEM-001` — blocks A..F. `EXIT=2` when `FALL.EXE` is not
/// readable at the installation root (`VEREDICTO: SKIP`), never a PASS.
pub fn run_sys_item_001() -> i32 {
    let header = format!(
        "PROBE-SYS-ITEM-001 - items and durability: template census, condition, repair, wear, materials (M41/W41-5, {CR})"
    );

    let data = match daggerfall_app::mode_a::read_install_file(daggerfall_inventory::retail_manifest::FALL_EXE) {
        Ok(data) => data,
        Err(reason) => {
            let mut lines = vec![header, reason];
            lines.push(
                "VEREDICTO: SKIP - FALL.EXE unreadable at the installation root (blocks A-F not executed)"
                    .to_string(),
            );
            let body = format!("{}\n", lines.join("\n"));
            let digest = sha256_hex(body.as_bytes());
            print!("{body}");
            println!("DIGEST {digest}");
            return 2;
        }
    };
    let offset = find_items_offset(&data);
    let templates = match parse_item_templates(&data) {
        Ok(templates) => templates,
        Err(err) => {
            let lines = [
                header,
                format!("FALL.EXE without a readable template table: {err}"),
                "VEREDICTO: SKIP - retail blob unreadable for parsing (blocks A-F not executed)"
                    .to_string(),
            ];
            let body = format!("{}\n", lines.join("\n"));
            let digest = sha256_hex(body.as_bytes());
            print!("{body}");
            println!("DIGEST {digest}");
            return 2;
        }
    };

    let sections = vec![
        block_a(&templates, &data, offset),
        block_b(&templates),
        block_c(),
        block_d(),
        block_e(&templates),
        block_f(),
    ];

    let mut lines: Vec<String> = Vec::with_capacity(420);
    lines.push(header);
    let mut rows = 0usize;
    let mut failures = 0usize;
    for section in &sections {
        lines.push(format!("BLOCK {} rows={}", section.label, section.lines.len()));
        for line in &section.lines {
            if line.starts_with("[OK] ") {
                rows += 1;
            } else if line.starts_with("FAIL ") {
                rows += 1;
                failures += 1;
            }
            lines.push(line.clone());
        }
    }
    lines.push(format!(
        "ROWCENSUS rows={rows} ok={} fail={failures} blocks={}",
        rows - failures,
        sections.len()
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
