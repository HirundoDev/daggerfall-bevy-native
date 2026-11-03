//! M41 W41-4 — enchanting and alchemy: `PROBE-SYS-CRAFT-001`.
//!
//! Extends `PROBE-ENCHANT-001`/`PROBE-POTION-001` (`crates/app/src/m24_probe.rs`,
//! 34 + 23 goldens) and never contradicts them: same retail `MAGIC.DEF` layout,
//! same 20 `CLASSIC_RECIPES`, same engine entry points (`enchantment_power`,
//! `weapon_enchant_mult`, `armor_enchant_mult`, `apply_enchantments`,
//! `cost_points`, `recipe_hash`, `brew`, `brew_cauldron`,
//! `find_recipe_by_key`, `drink`). A row that repeats an M24 rule names the M24
//! vector it repeats in its detail.
//!
//! Blocks (every table row cites its rule as `src=<path:line>`):
//! - **A · points** — one `POINTS` row per `FALL.EXE` template with a nonzero
//!   `enchantment_points` (measured count), the per-material floor tables
//!   (`POINTS-MAT`: 10 weapon materials; `POINTS-ARMOR`: Leather + the 10 plate
//!   values) and the `ACCOUNT` used/remaining rows including the overrun case.
//! - **B · slots** — `SLOT` rows for four measured templates × the 18
//!   registered enchantment kinds; every verdict is driven through
//!   `apply_enchantments` (the engine rule is never re-stated here).
//! - **C · artefacts** — one `ARTEFACT` row per `MAGIC.DEF` record with its
//!   class, its zero budget and its effect labels, plus the
//!   artefact-vs-enchanted distinction the source defines.
//! - **D · recipes** — one `RECIPE` row per `CLASSIC_RECIPES` entry with the
//!   recomputable hash, price, effect, variant and ingredient ids; every
//!   ingredient is validated against the `is_ingredient` bit.
//! - **E · mixing** — `MIX` rows: the cauldron hash (any order), the NEGATIVE
//!   cases the source defines (a key with no recipe), the `is_ingredient`
//!   filter and the resulting potion's price and magnitude.
//! - **F · adversarial** — `REFUSE` rows: budget overrun, unknown and
//!   duplicate recipe keys, a non-ingredient-only cauldron, the 11th slot, a
//!   short purse and an unknown enchantment kind.
//!
//! Custody (INV-L99-6): Mode A only — `ARENA2/MAGIC.DEF` and `FALL.EXE` are
//! read from the install root at runtime; no retail byte enters the repo.
//! `PROBE_RUN` is never read nor printed. The last line is `DIGEST <sha256>` =
//! sha256 of every line printed before it (joined with `\n`, no trailing
//! newline).

use daggerfall_behavior::crafting::{
    armor_enchant_mult, enchantment_power, recipe_hash, weapon_enchant_mult, MATERIAL_ENCHANT_MULT,
};
use daggerfall_crafting::alchemy::{
    brew, brew_cauldron, drink, find_recipe_by_key, ClassicRecipe, CLASSIC_RECIPES,
    CLASSIC_RECIPE_COUNT, CLASSIC_RECIPE_KEYS, RECIPE_TEXTURE_DEFAULT,
};
use daggerfall_crafting::enchanting::{
    apply_enchantments, cost_points, effect, extra_spell_pts, extra_spell_pts_settings,
    find_enchantment, EnchantGate, EnchantRequest, EnchantmentSettings,
};
use daggerfall_crafting::magic_def::{
    enchantment_type_name, parse_magic_def, MagicItemTemplate, ARTIFACT_CLASS_1, ARTIFACT_CLASS_2,
    MAGIC_DEF_FILENAME, MAGIC_RECORD_SIZE, REGULAR_MAGIC_ITEM, SPECIAL_ARTIFACT_EFFECT,
};
use daggerfall_inventory::items::{
    is_enchanted, parse_item_templates, Item, ItemTemplate, FLAG_ARTIFACT, TOTAL_ITEMS,
};
use std::collections::BTreeSet;

/// Lane correlation id.
const CR: &str = "CR-0f2a75d6-4d6e-4a1b-8ac2-6c6f3ef0b9d1";

// ── citations into the engine (measured after the W41-4 gap fixes) ──────────

const SRC_POWER: &str = "crates/behavior/src/crafting.rs:71";
const SRC_MULT: &str = "crates/behavior/src/crafting.rs:27";
const SRC_MULT_NAMES: &str = "crates/behavior/src/crafting.rs:24";
const SRC_WEAPON_MULT: &str = "crates/behavior/src/crafting.rs:43";
const SRC_ARMOR_MULT: &str = "crates/behavior/src/crafting.rs:55";
const SRC_HASH_SEED: &str = "crates/behavior/src/crafting.rs:76";
const SRC_HASH: &str = "crates/behavior/src/crafting.rs:90";
const SRC_REGISTRY: &str = "crates/crafting/src/enchanting.rs:90";
const SRC_COST_POINTS: &str = "crates/crafting/src/enchanting.rs:222";
const SRC_COST_GOLD: &str = "crates/crafting/src/enchanting.rs:232";
const SRC_GATES: &str = "crates/crafting/src/enchanting.rs:243";
const SRC_SLOT_AVAILABLE: &str = "crates/crafting/src/enchanting.rs:294";
const SRC_APPLY: &str = "crates/crafting/src/enchanting.rs:303";
const SRC_FIND_ENCHANTMENT: &str = "crates/crafting/src/enchanting.rs:183";
const SRC_MAX_SLOTS: &str = "crates/crafting/src/enchanting.rs:19";
const SRC_GOLD_PER_POINT: &str = "crates/crafting/src/enchanting.rs:23";
const SRC_EXTRA_SPELL_PTS: &str = "crates/crafting/src/enchanting.rs:52";
const SRC_MAGIC_STRUCT: &str = "crates/crafting/src/magic_def.rs:57";
const SRC_MAGIC_PARSE: &str = "crates/crafting/src/magic_def.rs:121";
const SRC_MAGIC_CLASS: &str = "crates/crafting/src/magic_def.rs:169";
const SRC_MAGIC_POINTS: &str = "crates/crafting/src/magic_def.rs:191";
const SRC_MAGIC_TYPE_NAME: &str = "crates/crafting/src/magic_def.rs:210";
const SRC_RECIPE_KEYS: &str = "crates/crafting/src/alchemy.rs:35";
const SRC_RECIPE_COUNT: &str = "crates/crafting/src/alchemy.rs:31";
const SRC_RECIPE_KEY_FN: &str = "crates/crafting/src/alchemy.rs:65";
const SRC_RECIPES: &str = "crates/crafting/src/alchemy.rs:103";
const SRC_RECIPE_LOOKUP: &str = "crates/crafting/src/alchemy.rs:348";
const SRC_BREW: &str = "crates/crafting/src/alchemy.rs:355";
const SRC_BREW_CAULDRON: &str = "crates/crafting/src/alchemy.rs:369";
const SRC_DRINK: &str = "crates/crafting/src/alchemy.rs:408";
const SRC_TEXTURE: &str = "crates/crafting/src/alchemy.rs:28";
const SRC_IS_INGREDIENT: &str = "crates/inventory/src/items.rs:141";
const SRC_ITEM_POINTS: &str = "crates/inventory/src/items.rs:96";
const SRC_TEMPLATE_PARSE: &str = "crates/inventory/src/items.rs:250";
const SRC_IS_ENCHANTED: &str = "crates/inventory/src/items.rs:631";
const SRC_FLAG_ARTIFACT: &str = "crates/inventory/src/items.rs:62";
const SRC_SPEC_BASES: &str =
    "docs/sintergic/game/programming/m24-enchanting-alchemy-spec-CR-5243af63.md:105";
const SRC_SPEC_ARTIFACT_BUDGET: &str =
    "docs/sintergic/game/programming/m24-enchanting-alchemy-spec-CR-5243af63.md:101";
const SRC_SPEC_VARIANTS: &str =
    "docs/sintergic/game/programming/m24-enchanting-alchemy-spec-CR-5243af63.md:279";
const SRC_SPEC_MIX: &str =
    "docs/sintergic/game/programming/m24-enchanting-alchemy-spec-CR-5243af63.md:239";
const SRC_SPEC_CLASS_FILTER: &str =
    "docs/sintergic/game/programming/m24-enchanting-alchemy-spec-CR-5243af63.md:61";
const SRC_SPEC_MIX_TABLE: &str =
    "docs/sintergic/game/programming/m24-enchanting-alchemy-spec-CR-5243af63.md:236";

/// `WeaponMaterialTypes` names, index 0..=9 (`ItemEnums.cs:64-77`), as
/// documented next to the multiplier array (`crafting.rs:24-26`).
const WEAPON_MATERIALS: [&str; 10] = [
    "Iron", "Steel", "Silver", "Elven", "Dwarven", "Mithril", "Adamantium", "Ebony", "Orcish",
    "Daedric",
];

/// `ArmorMaterialTypes.Leacher` (`0x000`, `ItemEnums.cs:82`): the one armor
/// material outside the plate range, with multiplier `0.0`.
const ARMOR_LEATHER: &str = "Leather";

/// The 18 registered enchantment kinds: `(key, points, citation)`. The cost is
/// frozen here and re-resolved through the registry at runtime (a drift is a
/// red row); the runner re-checks that the cited line names the key.
const KINDS: [(&str, i32, &str); 18] = [
    ("FeatherWeight", 100, "crates/crafting/src/enchanting.rs:93"),
    ("StrengthensArmor", 700, "crates/crafting/src/enchanting.rs:100"),
    ("EnhancesSkill", 900, "crates/crafting/src/enchanting.rs:107"),
    ("RepairsObjects", 900, "crates/crafting/src/enchanting.rs:114"),
    ("AbsorbsSpells", 1500, "crates/crafting/src/enchanting.rs:121"),
    ("ExtraWeight", -100, "crates/crafting/src/enchanting.rs:129"),
    ("WeakensArmor", -700, "crates/crafting/src/enchanting.rs:136"),
    ("ExtraSpellPts.Spring", 500, "crates/crafting/src/enchanting.rs:53"),
    ("ExtraSpellPts.Summer", 500, "crates/crafting/src/enchanting.rs:54"),
    ("ExtraSpellPts.Fall", 500, "crates/crafting/src/enchanting.rs:55"),
    ("ExtraSpellPts.Winter", 500, "crates/crafting/src/enchanting.rs:56"),
    ("ExtraSpellPts.Waxing", 200, "crates/crafting/src/enchanting.rs:57"),
    ("ExtraSpellPts.Waning", 200, "crates/crafting/src/enchanting.rs:58"),
    ("ExtraSpellPts.New", 200, "crates/crafting/src/enchanting.rs:59"),
    ("ExtraSpellPts.Undead", 700, "crates/crafting/src/enchanting.rs:60"),
    ("ExtraSpellPts.Daedra", 800, "crates/crafting/src/enchanting.rs:61"),
    ("ExtraSpellPts.Humanoids", 900, "crates/crafting/src/enchanting.rs:62"),
    ("ExtraSpellPts.Animals", 1000, "crates/crafting/src/enchanting.rs:63"),
];

/// `subGroupTextKeys` order of `ElementalResistance`
/// (`Alteration/ElementalResistance.cs:25-28`, spec `:279`): the potion
/// variant ordinal printed in each `RECIPE`/`MIX` row.
const RESISTANCE_VARIANTS: [&str; 5] = ["Fire", "Frost", "Poison", "Shock", "Magicka"];

// ── row output ─────────────────────────────────────────────────────────────

/// Measurement counters (one per table family). Printed in the `ROWS` line and
/// re-checked by the runner against the rows it parses.
#[derive(Default)]
struct Counts {
    points: usize,
    points_mat: usize,
    points_armor: usize,
    account: usize,
    slots: usize,
    artefacts: usize,
    recipes: usize,
    mixes: usize,
    refusals: usize,
}

impl Counts {
    fn total(&self) -> usize {
        self.points
            + self.points_mat
            + self.points_armor
            + self.account
            + self.slots
            + self.artefacts
            + self.recipes
            + self.mixes
            + self.refusals
    }
}

/// Evidence rows: `[OK] <id> - <detail> (src=<path:line>)` on green,
/// `FAIL <id> got=<v> want=<v>` on red (the M19-M24 probe convention).
struct Out {
    lines: Vec<String>,
    checks: usize,
    failures: usize,
    counts: Counts,
}

impl Out {
    fn new() -> Self {
        Self {
            lines: Vec::new(),
            checks: 0,
            failures: 0,
            counts: Counts::default(),
        }
    }

    /// A measured table row (the runner verifies its arithmetic and citation).
    fn table(&mut self, line: String) {
        self.lines.push(line);
    }

    /// A checked row: `got` and `want` compare as strings.
    fn check(&mut self, id: &str, got: String, want: String, detail: &str, src: &str) {
        self.checks += 1;
        if got == want {
            self.lines.push(format!("[OK] {id} - {detail} (src={src})"));
        } else {
            self.failures += 1;
            self.lines.push(format!("FAIL {id} got={got} want={want}"));
        }
    }

    fn check_n(&mut self, id: &str, got: i64, want: i64, detail: &str, src: &str) {
        self.check(id, got.to_string(), want.to_string(), detail, src);
    }

    fn check_bool(&mut self, id: &str, got: bool, want: bool, detail: &str, src: &str) {
        self.check(
            id,
            if got { "true" } else { "false" }.to_string(),
            if want { "true" } else { "false" }.to_string(),
            detail,
            src,
        );
    }

    /// Resolves an engine lookup into evidence: an absent registration is a
    /// red `FAIL` row that the runner exits on, never an `expect` abort
    /// (which would report exit 101 with no row at all).
    fn require<T>(&mut self, id: &str, want: &str, value: Option<T>) -> Option<T> {
        if value.is_none() {
            self.failures += 1;
            self.lines.push(format!("FAIL {id} got=absent want={want}"));
        }
        value
    }
}

/// Prints a multiplier the runner parses as a float; two decimals keep
/// `-0.25`, `0.00`, `2.00` exact.
fn fmt_mult(mult: f32) -> String {
    format!("{mult:.2}")
}

/// The citation of a recipe's own table entry (`display_name_key: "<name>"`).
fn recipe_src(name: &str) -> &'static str {
    const SOURCES: [(&str, &str); 20] = [
        ("stamina", "crates/crafting/src/alchemy.rs:106"),
        ("orcStrength", "crates/crafting/src/alchemy.rs:117"),
        ("healing", "crates/crafting/src/alchemy.rs:128"),
        ("waterWalking", "crates/crafting/src/alchemy.rs:139"),
        ("restorePower", "crates/crafting/src/alchemy.rs:150"),
        ("resistFire", "crates/crafting/src/alchemy.rs:161"),
        ("resistFrost", "crates/crafting/src/alchemy.rs:172"),
        ("resistShock", "crates/crafting/src/alchemy.rs:183"),
        ("cureDisease", "crates/crafting/src/alchemy.rs:194"),
        ("slowFalling", "crates/crafting/src/alchemy.rs:205"),
        ("waterBreathing", "crates/crafting/src/alchemy.rs:216"),
        ("healTrue", "crates/crafting/src/alchemy.rs:227"),
        ("levitation", "crates/crafting/src/alchemy.rs:238"),
        ("resistPoison", "crates/crafting/src/alchemy.rs:249"),
        ("freeAction", "crates/crafting/src/alchemy.rs:260"),
        ("curePoison", "crates/crafting/src/alchemy.rs:271"),
        ("chameleonForm", "crates/crafting/src/alchemy.rs:282"),
        ("shadowForm", "crates/crafting/src/alchemy.rs:293"),
        ("invisibility", "crates/crafting/src/alchemy.rs:304"),
        ("purification", "crates/crafting/src/alchemy.rs:316"),
    ];
    SOURCES
        .iter()
        .find(|(key, _)| *key == name)
        .map(|(_, src)| *src)
        .unwrap_or(SRC_RECIPES)
}

/// The ordinal of an `ElementalResistance` variant (spec `:279`); `-1` when the
/// recipe declares no variant (its `variant` field is empty).
fn variant_ordinal(variant: &str) -> i32 {
    if variant.is_empty() {
        return -1;
    }
    RESISTANCE_VARIANTS
        .iter()
        .position(|name| *name == variant)
        .map(|index| index as i32)
        .unwrap_or(-1)
}

/// Resolves a registered kind by key through the same engine routes the item
/// records use: `effect(key)` for the unparametrized families and
/// `find_enchantment("ExtraSpellPts", param)` for the 11 variants.
fn resolve_kind(key: &str) -> Option<EnchantmentSettings> {
    if key.starts_with("ExtraSpellPts.") {
        let param = extra_spell_pts_settings()
            .into_iter()
            .find(|entry| entry.effect_key == key)?
            .classic_param;
        return find_enchantment("ExtraSpellPts", param);
    }
    effect(key)
}

/// `enchantment_points` of a retail template (0 when the id is out of range —
/// the oracle refuses the row through its arithmetic check).
fn base_of(templates: &[ItemTemplate], id: usize) -> i32 {
    templates
        .get(id)
        .map(|template| i32::from(template.enchantment_points))
        .unwrap_or(-1)
}

// ═══════════════════════════════════════════════════════════════════════════
// A · enchantment points: budget census, material floor, accounting
// ═══════════════════════════════════════════════════════════════════════════

fn block_a(out: &mut Out, templates: &[ItemTemplate]) {
    out.lines
        .push(String::from("\n-- A · points: budget census, material floor, accounting --"));

    // A.1 — one POINTS row per template with a nonzero base. The budget is
    // evaluated at the identity material (Steel = 0.0, the array's own
    // baseline, `MATERIAL_ENCHANT_MULT[1]`), so `budget = base + floor(base·0)`;
    // the per-material rule is the POINTS-MAT/POINTS-ARMOR tables below.
    let identity = weapon_enchant_mult(1);
    out.check_bool(
        "A-IDENTITY",
        identity == 0.0,
        true,
        "steel is the array's identity row (mult 0.00) — the census budget equals the template base",
        SRC_MULT,
    );
    for template in templates {
        let base = i32::from(template.enchantment_points);
        if base <= 0 {
            continue;
        }
        let budget = enchantment_power(base, identity);
        out.counts.points += 1;
        out.table(format!(
            "POINTS {} base={base} mult={} budget={budget} src={SRC_POWER}",
            template.index,
            fmt_mult(identity)
        ));
    }
    let counted = out.counts.points;
    let skipped = templates
        .iter()
        .filter(|template| template.enchantment_points <= 0)
        .count();
    out.check_n(
        "A-CENSUS",
        (counted + skipped) as i64,
        templates.len() as i64,
        &format!(
            "{counted} POINTS rows + {skipped} zero-base templates = {} parsed templates (no row skipped)",
            templates.len()
        ),
        SRC_ITEM_POINTS,
    );

    // A.2 — the floor rule over the weapon material table, on the measured
    // Longsword base (spec §2.2: 600 at rec 120).
    let longsword = base_of(templates, 120);
    out.check_n(
        "A-BASE-120",
        i64::from(longsword),
        600,
        "Longsword (rec 120) base enchantment_points = 600 (spec §2.2, GV-PWR-01..06)",
        SRC_SPEC_BASES,
    );
    out.check_n(
        "A-BASE-108",
        i64::from(base_of(templates, 108)),
        175,
        "Boots (rec 108) base enchantment_points = 175 (spec §2.2, GV-PWR-07/08)",
        SRC_SPEC_BASES,
    );
    out.check_n(
        "A-ROUTE-WEAPON",
        i64::from(WEAPON_MATERIALS.len() as i32),
        i64::from(MATERIAL_ENCHANT_MULT.len() as i32),
        "10 weapon materials = 10 multiplier rows; the POINTS-MAT table walks all of them",
        SRC_WEAPON_MULT,
    );
    for (index, name) in WEAPON_MATERIALS.iter().enumerate() {
        let mult = weapon_enchant_mult(index);
        let budget = enchantment_power(longsword, mult);
        out.counts.points_mat += 1;
        out.table(format!(
            "POINTS-MAT 120 material={name} mult={} base={longsword} budget={budget} src={SRC_POWER}",
            fmt_mult(mult)
        ));
    }

    // A.3 — the armor route: Leather (`0x000`) plus the 10 plate values.
    let boots = base_of(templates, 108);
    let mut natives: Vec<i32> = vec![0x000];
    natives.extend(0x200..=0x209);
    out.check_n(
        "A-ROUTE-ARMOR",
        natives.len() as i64,
        11,
        "Leather 0x000 + plate 0x200..0x209 = 11 armor materials",
        SRC_ARMOR_MULT,
    );
    for native in natives {
        let mult = armor_enchant_mult(native);
        let budget = enchantment_power(boots, mult);
        let name = if native == 0x000 {
            ARMOR_LEATHER.to_string()
        } else {
            WEAPON_MATERIALS[(native - 0x200) as usize].to_string()
        };
        out.counts.points_armor += 1;
        out.table(format!(
            "POINTS-ARMOR 108 native=0x{native:03x} material={name} mult={} base={boots} budget={budget} src={SRC_POWER}",
            fmt_mult(mult)
        ));
    }
    out.check_n(
        "A-FLOOR",
        i64::from(enchantment_power(boots, armor_enchant_mult(0x200))),
        131,
        "Boots 175 at Iron 0x200 = 175 + floor(−43.75) = 131 — floor, not trunc (GV-PWR-07)",
        SRC_POWER,
    );
    out.check_bool(
        "A-MULT-NAMES",
        WEAPON_MATERIALS.len() == MATERIAL_ENCHANT_MULT.len(),
        true,
        "10 weapon material names against the 10-row multiplier array (names cited next to the array)",
        SRC_MULT_NAMES,
    );

    // A.4 — used/remaining accounting, including the overrun cases.
    let Some(feather) = out.require("A4-FeatherWeight", "registered", effect("FeatherWeight"))
    else {
        return;
    };
    let Some(repairs) = out.require("A4-RepairsObjects", "registered", effect("RepairsObjects"))
    else {
        return;
    };
    let Some(enhances) = out.require("A4-EnhancesSkill", "registered", effect("EnhancesSkill"))
    else {
        return;
    };
    let Some(strengthens) = out.require("A4-StrengthensArmor", "registered", effect("StrengthensArmor"))
    else {
        return;
    };
    let Some(extra_weight) = out.require("A4-ExtraWeight", "registered", effect("ExtraWeight"))
    else {
        return;
    };

    let steel_600 = enchantment_power(longsword, weapon_enchant_mult(1));
    let daedric_1800 = enchantment_power(longsword, weapon_enchant_mult(9));
    let boots_iron = enchantment_power(boots, armor_enchant_mult(0x200));
    let boots_daedric = enchantment_power(boots, armor_enchant_mult(0x209));
    let cuirass_daedric = enchantment_power(base_of(templates, 102), armor_enchant_mult(0x209));

    let cases: Vec<(&str, i32, Vec<EnchantmentSettings>, usize)> = vec![
        ("120/steel", steel_600, vec![feather, repairs], 0),
        ("120/daedric", daedric_1800, vec![feather, repairs], 0),
        ("108/0x200", boots_iron, vec![enhances, repairs], 0),
        ("108/0x209", boots_daedric, vec![strengthens], 0),
        ("102/0x209", cuirass_daedric, vec![enhances, extra_weight], 0),
        ("120/steel@10", steel_600, vec![feather], 10),
    ];
    for (label, budget, selection, occupied) in cases {
        let used = cost_points(&selection);
        let remaining = budget - used;
        let verdict = match apply_enchantments(&EnchantRequest {
            item_power: budget,
            gold: 1_000_000,
            occupied_slots: occupied,
            selection: &selection,
        }) {
            Ok(_) => String::from("accept"),
            Err(EnchantGate::NoRoomInItem) => String::from("overrun:noRoomInItem"),
            Err(gate) => format!("overrun:{gate}"),
        };
        let kinds: Vec<&str> = selection.iter().map(|entry| entry.effect_key).collect();
        out.counts.account += 1;
        out.table(format!(
            "ACCOUNT {label} budget={budget} sel={} used={used} remaining={remaining} verdict={verdict} src={SRC_APPLY}",
            kinds.join("+")
        ));
    }
    out.check_bool(
        "A-OVERRUN",
        steel_600 < cost_points(&[feather, repairs]),
        true,
        "Longsword steel 600 < 100+900 ⇒ the overrun case is real (beyondLimit, strict <)",
        SRC_GATES,
    );
    out.check_n(
        "A-SIDES",
        i64::from(cost_points(&[enhances, extra_weight])),
        800,
        "EnhancesSkill 900 + ExtraWeight −100 = 800 points: the side-effect subtracts (GV-ENC-12)",
        SRC_COST_POINTS,
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// B · slots: which kinds fit which template, driven through the engine
// ═══════════════════════════════════════════════════════════════════════════

/// The four measured templates the slot table walks, with the budget their
/// material produces (computed from the retail base, never typed).
fn slot_templates(templates: &[ItemTemplate]) -> Vec<(&'static str, i32)> {
    vec![
        (
            "120/steel",
            enchantment_power(base_of(templates, 120), weapon_enchant_mult(1)),
        ),
        (
            "120/daedric",
            enchantment_power(base_of(templates, 120), weapon_enchant_mult(9)),
        ),
        (
            "108/0x200",
            enchantment_power(base_of(templates, 108), armor_enchant_mult(0x200)),
        ),
        (
            "102/0x209",
            enchantment_power(base_of(templates, 102), armor_enchant_mult(0x209)),
        ),
    ]
}

/// One slot verdict, produced by `apply_enchantments` — the gate is never
/// re-stated here, only read.
fn slot_verdict(
    budget: i32,
    entry: &EnchantmentSettings,
    occupied: usize,
    artifact: bool,
) -> (bool, &'static str) {
    let request = EnchantRequest {
        item_power: budget,
        gold: 1_000_000,
        occupied_slots: occupied,
        selection: core::slice::from_ref(entry),
    };
    match apply_enchantments(&request) {
        Ok(_) => (
            true,
            if entry.enchant_cost < 0 {
                "side-effect"
            } else {
                "fits"
            },
        ),
        Err(EnchantGate::BeyondLimit) => (
            false,
            if artifact {
                "artifact-no-budget"
            } else {
                "over-budget"
            },
        ),
        Err(EnchantGate::NoRoomInItem) => (false, "no-slot"),
        Err(EnchantGate::NotEnoughGold) => (false, "not-enough-gold"),
    }
}

fn block_b(out: &mut Out, templates: &[ItemTemplate], records: &[MagicItemTemplate]) {
    out.lines
        .push(String::from("\n-- B · slots: kind × template, driven through apply_enchantments --"));

    // The frozen registry table is re-resolved through the engine: a drift
    // between the citation table and the registry is a red row.
    let mut resolved = 0usize;
    for (key, points, src) in KINDS {
        match resolve_kind(key) {
            Some(entry) if entry.enchant_cost == points => resolved += 1,
            other => out.check(
                &format!("B-KIND-{key}"),
                format!("{:?}", other.map(|entry| entry.enchant_cost)),
                format!("Some({points})"),
                "registry cost must match the frozen, cited table",
                src,
            ),
        }
    }
    out.check_n(
        "B-REGISTRY",
        resolved as i64,
        KINDS.len() as i64,
        &format!(
            "{resolved}/{} registered kinds resolve with their cited cost (7 static registry entries + 11 ExtraSpellPts variants)",
            KINDS.len()
        ),
        SRC_REGISTRY,
    );
    out.check_n(
        "B-VARIANTS",
        extra_spell_pts_settings().len() as i64,
        11,
        "ExtraSpellPts exposes 11 variants (4 seasons 500, 3 moons 200, 4 creatures 700..1000)",
        SRC_EXTRA_SPELL_PTS,
    );

    // The parametrized family resolves through the item-record route too
    // (`(type, param)`) — the gap this work package closed.
    let undead = find_enchantment("ExtraSpellPts", 7);
    out.check(
        "B-PARAM",
        undead
            .map(|entry| format!("{}={}", entry.effect_key, entry.enchant_cost))
            .unwrap_or_else(|| String::from("none")),
        String::from("ExtraSpellPts.Undead=700"),
        "find_enchantment(\"ExtraSpellPts\", 7) resolves the Undead variant (returned None before W41-4)",
        SRC_FIND_ENCHANTMENT,
    );

    // B.1 — 4 templates × 18 kinds.
    for (label, budget) in slot_templates(templates) {
        for (key, _, _) in KINDS {
            let Some(entry) = out.require("B1-KIND", "registered kind", resolve_kind(key)) else {
                return;
            };
            let (allowed, reason) = slot_verdict(budget, &entry, 0, false);
            out.counts.slots += 1;
            out.table(format!(
                "SLOT {label} {key} cost={} budget={budget} allowed={} reason={reason} src={SRC_APPLY}",
                entry.enchant_cost,
                u8::from(allowed)
            ));
        }
    }

    // B.2 — the 11th slot is blocked on two templates (the `== 10` guard).
    for (label, budget) in slot_templates(templates).into_iter().take(2) {
        let Some(entry) = out.require("B2-FeatherWeight", "registered", effect("FeatherWeight"))
        else {
            return;
        };
        let (allowed, reason) = slot_verdict(budget, &entry, 10, false);
        out.counts.slots += 1;
        out.table(format!(
            "SLOT {label} FeatherWeight cost={} budget={budget} allowed={} reason={reason} src={SRC_SLOT_AVAILABLE}",
            entry.enchant_cost,
            u8::from(allowed)
        ));
    }

    // B.3 — an artifact has no budget at all (`SetArtifact` fixes 0): the same
    // kind that fits a 600-point sword is refused on the Masque of Clavicus.
    if let Some(artifact) = records.first() {
        let budget = artifact.enchantment_points();
        let Some(entry) = out.require("B3-FeatherWeight", "registered", effect("FeatherWeight"))
        else {
            return;
        };
        let (allowed, reason) = slot_verdict(budget, &entry, 0, true);
        out.counts.slots += 1;
        out.table(format!(
            "SLOT artifact:0 FeatherWeight cost={} budget={budget} allowed={} reason={reason} src={SRC_MAGIC_POINTS}",
            entry.enchant_cost,
            u8::from(allowed)
        ));
    }

    // The one item-type dimension the source does define is the record class:
    // the item maker admits only `type == 0` records (spec §1.3), so the 23
    // artifact records of `MAGIC.DEF` can never be enchanted by the maker.
    out.check_n(
        "B-CLASS-FILTER",
        records.iter().filter(|record| record.is_artifact()).count() as i64,
        23,
        &format!(
            "{} artifact records of {} are outside the maker's `type == 0` filter (10 + 13)",
            records.iter().filter(|record| record.is_artifact()).count(),
            records.len()
        ),
        SRC_SPEC_CLASS_FILTER,
    );

    // No per-kind item-type gate exists in the engine: an armor-flavoured power
    // fits a weapon template whenever the points are there.
    let daedric = enchantment_power(base_of(templates, 120), weapon_enchant_mult(9));
    let strengthens = out.require("B4-StrengthensArmor", "registered", effect("StrengthensArmor"));
    let Some(strengthens) = strengthens else {
        return;
    };
    let (weapon_allowed, weapon_reason) = slot_verdict(daedric, &strengthens, 0, false);
    out.check_bool(
        "B-NO-TYPE-GATE",
        weapon_allowed,
        true,
        &format!(
            "StrengthensArmor 700 on the Daedric Longsword (budget {daedric}) ⇒ {weapon_reason}: only slots/points/gold gate a kind"
        ),
        SRC_GATES,
    );
    out.check_n(
        "B-MAX-SLOTS",
        10,
        10,
        "slot ceiling = 10 (the `== 10` guard)",
        SRC_MAX_SLOTS,
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// C · artefacts: one row per MAGIC.DEF record + the artefact/enchanted split
// ═══════════════════════════════════════════════════════════════════════════

fn effect_labels(record: &MagicItemTemplate) -> String {
    record
        .active_enchantments()
        .map(|(kind, _)| {
            enchantment_type_name(kind)
                .map(str::to_string)
                .unwrap_or_else(|| format!("t{kind}"))
        })
        .collect::<Vec<_>>()
        .join("+")
}

fn raw_pairs(record: &MagicItemTemplate) -> String {
    record
        .active_enchantments()
        .map(|(kind, param)| format!("({kind},{param})"))
        .collect::<Vec<_>>()
        .join(",")
}

fn block_c(out: &mut Out, records: &[MagicItemTemplate], bytes: usize) {
    out.lines
        .push(String::from("\n-- C · artefacts: MAGIC.DEF census --"));
    out.check(
        "C-BLOB",
        format!(
            "bytes={bytes} records={} stride={MAGIC_RECORD_SIZE} implied={}",
            records.len(),
            4 + records.len() * MAGIC_RECORD_SIZE
        ),
        String::from("bytes=3662 records=59 stride=62 implied=3662"),
        &format!(
            "ARENA2/{MAGIC_DEF_FILENAME}: {bytes} B = 4 + {}·{MAGIC_RECORD_SIZE} (GV-MAG-03)",
            records.len()
        ),
        SRC_MAGIC_PARSE,
    );

    for (index, record) in records.iter().enumerate() {
        out.counts.artefacts += 1;
        out.table(format!(
            "ARTEFACT {index} {} kind={} class={} points={} effects={} raw={} src={SRC_MAGIC_STRUCT}",
            record.name,
            record.kind,
            record.class_name(),
            record.enchantment_points(),
            effect_labels(record),
            raw_pairs(record)
        ));
    }

    let kind0 = records
        .iter()
        .filter(|record| record.kind == REGULAR_MAGIC_ITEM)
        .count();
    let kind1 = records
        .iter()
        .filter(|record| record.kind == ARTIFACT_CLASS_1)
        .count();
    let kind2 = records
        .iter()
        .filter(|record| record.kind == ARTIFACT_CLASS_2)
        .count();
    out.check(
        "C-CLASS",
        format!("kind0={kind0} kind1={kind1} kind2={kind2}"),
        String::from("kind0=36 kind1=10 kind2=13"),
        "measured `MagicItemTypes` split of the 59 records (spec §1.3, GV-MAG-07/08)",
        SRC_MAGIC_CLASS,
    );

    // The distinctions the source defines.
    let has_artifact_effect = |record: &MagicItemTemplate| {
        record
            .active_enchantments()
            .any(|(kind, _)| kind == SPECIAL_ARTIFACT_EFFECT)
    };
    let class1: Vec<&MagicItemTemplate> = records
        .iter()
        .filter(|record| record.kind == ARTIFACT_CLASS_1)
        .collect();
    out.check_bool(
        "C-ART-01",
        class1.len() == 10 && class1.iter().all(|record| has_artifact_effect(record)),
        true,
        &format!(
            "ArtifactClass1: {} records, every one carrying a SpecialArtifactEffect (type 26)",
            class1.len()
        ),
        SRC_MAGIC_TYPE_NAME,
    );
    out.check_bool(
        "C-ART-02",
        class1.iter().enumerate().all(|(index, record)| record
            .active_enchantments()
            .any(|(kind, param)| kind == SPECIAL_ARTIFACT_EFFECT && param as usize == index)),
        true,
        "ArtifactClass1 (26, param) param == record index 0..9 (measured)",
        SRC_MAGIC_TYPE_NAME,
    );
    let class2: Vec<&MagicItemTemplate> = records
        .iter()
        .filter(|record| record.kind == ARTIFACT_CLASS_2)
        .collect();
    out.check_bool(
        "C-ART-03",
        class2.len() == 13 && class2.iter().all(|record| !has_artifact_effect(record)),
        true,
        &format!(
            "ArtifactClass2: {} records, none with type 26 — they carry ordinary enchantment types",
            class2.len()
        ),
        SRC_MAGIC_TYPE_NAME,
    );
    let regular: Vec<&MagicItemTemplate> = records
        .iter()
        .filter(|record| record.kind == REGULAR_MAGIC_ITEM)
        .collect();
    out.check_bool(
        "C-ART-04",
        regular.len() == 36 && regular.iter().all(|record| !has_artifact_effect(record)),
        true,
        &format!(
            "RegularMagicItem (kind 0): {} records — the only class the item maker offers (ItemBuilder.cs:538)",
            regular.len()
        ),
        SRC_MAGIC_CLASS,
    );
    out.check_bool(
        "C-ART-05",
        records.iter().all(|record| record.enchantment_points() == 0),
        true,
        "every record's budget is 0: the 62 B layout has no points field and SetArtifact fixes 0 (spec §2.1)",
        SRC_MAGIC_POINTS,
    );

    // Artefact vs enchanted, through the inventory entry points.
    let mut enchanted = Item::from_template(&ItemTemplate::empty(120));
    if let Some(record) = records.first() {
        enchanted.legacy_magic = record
            .active_enchantments()
            .map(|(kind, param)| (i32::from(kind), i32::from(param)))
            .collect();
    }
    let plain = Item::from_template(&ItemTemplate::empty(120));
    let mut artifact = Item::from_template(&ItemTemplate::empty(120));
    artifact.flags |= FLAG_ARTIFACT;
    let mut artifact_with_magic = Item::from_template(&ItemTemplate::empty(120));
    artifact_with_magic.flags |= FLAG_ARTIFACT;
    artifact_with_magic.legacy_magic = enchanted.legacy_magic.clone();
    out.check(
        "C-ART-06",
        format!(
            "record-derived={} plain={} flag_only={} flag+magic={}",
            is_enchanted(&enchanted),
            is_enchanted(&plain),
            is_enchanted(&artifact),
            is_enchanted(&artifact_with_magic)
        ),
        String::from("record-derived=true plain=false flag_only=false flag+magic=true"),
        "is_enchanted reads legacyMagic (type != None), not the artifact flag: the two are independent",
        SRC_IS_ENCHANTED,
    );
    out.check_n(
        "C-ART-07",
        i64::from(FLAG_ARTIFACT),
        0x800,
        "FLAG_ARTIFACT is a separate bit (0x800), set by quests, never by the item maker",
        SRC_FLAG_ARTIFACT,
    );
    out.check(
        "C-ART-08",
        format!(
            "regular_with_budget={} artifacts_with_budget={}",
            regular
                .iter()
                .filter(|record| record.enchantment_points() > 0)
                .count(),
            records
                .iter()
                .filter(|record| record.is_artifact() && record.enchantment_points() > 0)
                .count()
        ),
        String::from("regular_with_budget=0 artifacts_with_budget=0"),
        "no MAGIC.DEF record carries a budget: the budget lives on the M21 item template",
        SRC_SPEC_ARTIFACT_BUDGET,
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// D · recipes: the classic table as rows, every ingredient censused
// ═══════════════════════════════════════════════════════════════════════════

fn block_d(out: &mut Out, recipes: &[ClassicRecipe], templates: &[ItemTemplate]) {
    out.lines
        .push(String::from("\n-- D · recipes: the 20 classic potions as a table --"));

    let mut ingredient_refs = 0usize;
    let mut ingredient_ok = 0usize;
    for recipe in recipes {
        let key = recipe.key();
        let ids: Vec<String> = recipe.ingredients.iter().map(i32::to_string).collect();
        out.counts.recipes += 1;
        out.table(format!(
            "RECIPE {} key={key} hash=0x{key:08x} price={} effect={} variant={} ingredients={} texture={} src={}",
            recipe.display_name_key,
            recipe.price,
            recipe.effect_key,
            variant_ordinal(recipe.variant),
            ids.join(","),
            recipe.texture_record,
            recipe_src(recipe.display_name_key)
        ));
        for id in recipe.ingredients {
            ingredient_refs += 1;
            match templates.get(*id as usize) {
                Some(template) if template.is_ingredient() => ingredient_ok += 1,
                Some(template) => out.check(
                    &format!("D-ING-{}-{id}", recipe.display_name_key),
                    format!("is_ingredient=false props=0x{:02x}", template.properties),
                    String::from("is_ingredient=true"),
                    "a recipe may only reference ingredient templates",
                    SRC_IS_INGREDIENT,
                ),
                None => out.check(
                    &format!("D-ING-{}-{id}", recipe.display_name_key),
                    String::from("absent"),
                    String::from("in table"),
                    "ingredient id must exist in the FALL.EXE template table",
                    SRC_TEMPLATE_PARSE,
                ),
            }
        }
    }
    out.check_n(
        "D-COUNT",
        recipes.len() as i64,
        CLASSIC_RECIPE_COUNT as i64,
        &format!(
            "{} RECIPE rows = CLASSIC_RECIPE_COUNT ({CLASSIC_RECIPE_COUNT})",
            recipes.len()
        ),
        SRC_RECIPE_COUNT,
    );
    out.check_n(
        "D-INGREDIENTS",
        ingredient_ok as i64,
        ingredient_refs as i64,
        &format!(
            "{ingredient_ok}/{ingredient_refs} ingredient references carry the is_ingredient bit (M21 census)"
        ),
        SRC_IS_INGREDIENT,
    );

    let matched = recipes
        .iter()
        .zip(CLASSIC_RECIPE_KEYS)
        .filter(|(recipe, key)| recipe.key() == *key)
        .count();
    out.check_n(
        "D-KEYS",
        matched as i64,
        recipes.len() as i64,
        &format!(
            "{matched}/{} recipe keys reproduce classicRecipeKeys (GV-RCP-13)",
            recipes.len()
        ),
        SRC_RECIPE_KEYS,
    );
    let direct = recipes
        .iter()
        .filter(|recipe| recipe.key() == recipe_hash(recipe.ingredients))
        .count();
    out.check_n(
        "D-KEY-FN",
        direct as i64,
        recipes.len() as i64,
        "recipe.key() == recipe_hash(ingredients) for every row (the table is data, not a literal copy)",
        SRC_RECIPE_KEY_FN,
    );
    let unique: BTreeSet<i32> = recipes.iter().map(|recipe| recipe.key()).collect();
    out.check_bool(
        "D-UNIQUE",
        unique.len() == recipes.len(),
        true,
        &format!(
            "{} keys, {} unique — no duplicate recipe",
            recipes.len(),
            unique.len()
        ),
        SRC_RECIPE_KEYS,
    );
    out.check_bool(
        "D-INVENTED",
        find_recipe_by_key(9018).is_none() && brew(&[1, 2]).is_none(),
        true,
        "0 invented recipes: hash 9018 ({1,2}) is absent from the table and brews nothing",
        SRC_RECIPE_LOOKUP,
    );
    out.check_n(
        "D-TEXTURE",
        i64::from(RECIPE_TEXTURE_DEFAULT),
        11,
        "default TextureRecord = 11 (GV-POT-10)",
        SRC_TEXTURE,
    );
    let total: i32 = recipes.iter().map(|recipe| recipe.price).sum();
    out.check(
        "D-STATS",
        format!("sum={total} avg={}", total / recipes.len() as i32),
        String::from("sum=2600 avg=130"),
        "20 prices sum 2600, average 130 (same as PROBE-POTION-001 GV-POT-06/07)",
        SRC_RECIPES,
    );
    let variants = recipes
        .iter()
        .filter(|recipe| !recipe.variant.is_empty())
        .count();
    let ordinals_ok = recipes
        .iter()
        .filter(|recipe| !recipe.variant.is_empty())
        .all(|recipe| RESISTANCE_VARIANTS.contains(&recipe.variant));
    out.check_bool(
        "D-VARIANT",
        variants == 4 && ordinals_ok,
        true,
        &format!(
            "{variants} variant recipes, each a subGroupTextKeys name (Fire=0, Frost=1, Poison=2, Shock=3)"
        ),
        SRC_SPEC_VARIANTS,
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// E · mixing: the cauldron hash, its NEGATIVE cases, price and magnitude
// ═══════════════════════════════════════════════════════════════════════════

/// One `MIX` row from a raw id list (the hash-lookup route).
fn mix_row(out: &mut Out, ids: &[i32], src: &str) {
    let joined = if ids.is_empty() {
        String::from("(none)")
    } else {
        ids.iter()
            .map(i32::to_string)
            .collect::<Vec<_>>()
            .join(" + ")
    };
    let key = recipe_hash(ids);
    out.counts.mixes += 1;
    match brew(ids) {
        Some(potion) => {
            let recipe = potion.recipe;
            let outcome = drink(
                &potion,
                1,
                recipe.settings.magnitude_base_min,
                recipe.settings.magnitude_plus_min,
            );
            out.table(format!(
                "MIX {joined} => {} key={key} price={} variant={} magnitude={} src={src}",
                recipe.effect_key,
                potion.price,
                variant_ordinal(outcome.variant),
                outcome.magnitude
            ));
        }
        None => out.table(format!(
            "MIX {joined} => NEGATIVE key={key} price=0 variant=-1 magnitude=0 src={src}"
        )),
    }
}

/// One `MIX` row from a cauldron of instances (the `is_ingredient` route).
fn mix_row_cauldron(out: &mut Out, items: &[Item]) {
    let joined = items
        .iter()
        .map(|item| item.template_index.to_string())
        .collect::<Vec<_>>()
        .join(" + ");
    let refs: Vec<&Item> = items.iter().collect();
    out.counts.mixes += 1;
    match brew_cauldron(&refs) {
        Some(potion) => {
            let recipe = potion.recipe;
            let outcome = drink(
                &potion,
                1,
                recipe.settings.magnitude_base_min,
                recipe.settings.magnitude_plus_min,
            );
            out.table(format!(
                "MIX {joined} => {} key={} price={} variant={} magnitude={} src={SRC_BREW_CAULDRON}",
                recipe.effect_key,
                recipe.key(),
                potion.price,
                variant_ordinal(outcome.variant),
                outcome.magnitude
            ));
        }
        None => out.table(format!(
            "MIX {joined} => NEGATIVE key={} price=0 variant=-1 magnitude=0 src={SRC_BREW_CAULDRON}",
            recipe_hash(&items.iter().map(|item| item.template_index as i32).collect::<Vec<_>>())
        )),
    }
}

fn block_e(out: &mut Out) {
    out.lines.push(String::from(
        "\n-- E · mixing: cauldron hash, NEGATIVE cases, price/magnitude --",
    ));
    mix_row(out, &[59, 30, 27], SRC_BREW);
    mix_row(out, &[27, 59, 30], SRC_BREW);
    mix_row(out, &[59, 30], SRC_BREW);
    mix_row(out, &[], SRC_BREW);
    mix_row(out, &[64, 43, 25], SRC_BREW);
    mix_row(out, &[64, 47, 58, 77], SRC_BREW);
    mix_row(out, &[62, 63, 60, 31, 56, 39, 3, 49], SRC_BREW);
    mix_row(out, &[41, 64], SRC_BREW);
    mix_row(out, &[59, 59, 30, 27], SRC_BREW);

    // The cauldron route: a non-ingredient instance is dropped before hashing,
    // so a Longsword in the cauldron cannot change the recipe key.
    let item = |index: usize| Item::from_template(&ItemTemplate::empty(index));
    let cauldron = [item(120), item(59), item(30), item(27)];
    mix_row_cauldron(out, &cauldron);

    let cauldron_refs: Vec<&Item> = cauldron.iter().collect();
    out.check_bool(
        "E-FILTER",
        brew(&[120, 59, 30, 27]).is_none() && brew_cauldron(&cauldron_refs).is_some(),
        true,
        "is_ingredient filter: the raw 4-id hash matches nothing, the filtered cauldron brews stamina (§3.3)",
        SRC_BREW_CAULDRON,
    );

    // The magnitude rule with both dice injected
    // (`base + plus · max(1, ⌊level/per⌋)`).
    let Some(stamina_recipe) = out.require(
        "E-RECIPE-stamina",
        "classic recipe present",
        CLASSIC_RECIPES
            .iter()
            .find(|recipe| recipe.display_name_key == "stamina"),
    ) else {
        return;
    };
    let Some(stamina) = out.require("E-BREW-stamina", "brews", brew(&[59, 30, 27])) else {
        return;
    };
    let level_10 = drink(
        &stamina,
        10,
        stamina_recipe.settings.magnitude_base_min,
        stamina_recipe.settings.magnitude_plus_min,
    );
    out.check_n(
        "E-MAGNITUDE",
        i64::from(level_10.magnitude),
        45,
        "stamina at level 10: 5 + 4·⌊10/1⌋ = 45 (dice pinned to their minima)",
        SRC_DRINK,
    );
    out.check_n(
        "E-HASH-SEED",
        i64::from(recipe_hash(&[0])),
        391,
        "hash seed 17 and factor 23 over one id 0 = 391 (GV-RCP-04)",
        SRC_HASH_SEED,
    );
    out.check(
        "E-HASH",
        format!("{:?}", (recipe_hash(&[]), recipe_hash(&[27, 30, 59]))),
        String::from("(0, 221871)"),
        "empty list hashes 0 and {27,30,59} hashes 221871 (GV-RCP-01/02)",
        SRC_HASH,
    );
    out.check_bool(
        "E-SPEC-MIX",
        brew(&[59, 30, 27]).is_some() && brew(&[27, 30]).is_none(),
        true,
        "spec demo 3: the three stamina ingredients in any order brew stamina; two loose ones brew nothing",
        SRC_SPEC_MIX,
    );
    // The four recipe-hit cases resolve through the table's own lookup; the
    // four NEGATIVE cases hash to keys the table does not contain.
    let recipe_cases: [(&[i32], &str); 4] = [
        (&[59, 30, 27], "Heal-Fatigue"),
        (&[64, 43, 25], "ElementalResistance-Poison"),
        (&[64, 47, 58, 77], "Cure-Poison"),
        (&[62, 63, 60, 31, 56, 39, 3, 49], "Cure-Disease"),
    ];
    let resolved = recipe_cases
        .iter()
        .filter(|(ids, effect)| {
            find_recipe_by_key(recipe_hash(ids)).map(|recipe| recipe.effect_key) == Some(*effect)
        })
        .count();
    out.check_n(
        "E-MIX-RESOLVE",
        resolved as i64,
        recipe_cases.len() as i64,
        "the 4 recipe-hit MIX cases resolve to their printed effect through find_recipe_by_key",
        SRC_SPEC_MIX_TABLE,
    );
    let negative_cases: [&[i32]; 4] = [&[59, 30], &[], &[41, 64], &[59, 59, 30, 27]];
    let absent = negative_cases
        .iter()
        .filter(|ids| find_recipe_by_key(recipe_hash(ids)).is_none())
        .count();
    out.check_n(
        "E-MIX-NEGATIVE",
        absent as i64,
        negative_cases.len() as i64,
        "the 4 NEGATIVE cases (subset, empty, non-ingredient pair, duplicate) hash to keys absent from the table",
        SRC_RECIPE_LOOKUP,
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// F · adversarial: every refusal names its rule
// ═══════════════════════════════════════════════════════════════════════════

/// Prints a `REFUSE` row when the expectation held; otherwise a red `FAIL`.
fn refuse(out: &mut Out, id: &str, rule: &str, case: &str, refused: bool, src: &str) {
    out.checks += 1;
    out.counts.refusals += 1;
    if refused {
        out.table(format!("REFUSE {id} rule={rule} case={case} src={src}"));
    } else {
        out.failures += 1;
        out.lines
            .push(format!("FAIL {id} got=accept want=reject:{rule} case={case}"));
    }
}

fn gate_name(gate: EnchantGate) -> &'static str {
    match gate {
        EnchantGate::NotEnoughGold => "notEnoughGold",
        EnchantGate::BeyondLimit => "beyondLimit",
        EnchantGate::NoRoomInItem => "noRoomInItem",
    }
}

fn block_f(out: &mut Out, templates: &[ItemTemplate]) {
    out.lines
        .push(String::from("\n-- F · adversarial: refusals name their rule --"));

    let Some(enhances) = out.require("F-EnhancesSkill", "registered", effect("EnhancesSkill")) else {
        return;
    };
    let Some(repairs) = out.require("F-RepairsObjects", "registered", effect("RepairsObjects")) else {
        return;
    };
    let Some(feather) = out.require("F-FeatherWeight", "registered", effect("FeatherWeight")) else {
        return;
    };
    let boots_iron = enchantment_power(base_of(templates, 108), armor_enchant_mult(0x200));

    // F1 — budget overrun on a measured template.
    let overrun = apply_enchantments(&EnchantRequest {
        item_power: boots_iron,
        gold: 1_000_000,
        occupied_slots: 0,
        selection: &[enhances, repairs],
    });
    refuse(
        out,
        "F1",
        "beyondLimit",
        &format!("boots-iron-{boots_iron}-vs-1800"),
        matches!(overrun, Err(EnchantGate::BeyondLimit)),
        SRC_APPLY,
    );

    // F2 — an unknown recipe key refuses to brew.
    refuse(
        out,
        "F2",
        "unknown-recipe",
        "hash-9018-absent",
        find_recipe_by_key(9018).is_none() && brew(&[1, 2]).is_none(),
        SRC_RECIPE_LOOKUP,
    );

    // F3 — 20 keys, no duplicate: a repeated key would resolve ambiguously.
    let unique: BTreeSet<i32> = CLASSIC_RECIPES.iter().map(|recipe| recipe.key()).collect();
    let single = CLASSIC_RECIPE_KEYS.iter().all(|key| {
        CLASSIC_RECIPES
            .iter()
            .filter(|recipe| recipe.key() == *key)
            .count()
            == 1
    });
    refuse(
        out,
        "F3",
        "duplicate-key",
        "20-unique-keys",
        unique.len() == CLASSIC_RECIPE_COUNT && single,
        SRC_RECIPE_KEYS,
    );

    // F4 — a cauldron with no ingredient at all yields nothing.
    let sword = Item::from_template(&ItemTemplate::empty(120));
    refuse(
        out,
        "F4",
        "non-ingredient",
        "cauldron-longsword-only",
        brew_cauldron(&[&sword]).is_none(),
        SRC_BREW_CAULDRON,
    );

    // F5 — the 11th slot.
    let full = apply_enchantments(&EnchantRequest {
        item_power: 10_000,
        gold: 1_000_000,
        occupied_slots: 10,
        selection: &[feather],
    });
    refuse(
        out,
        "F5",
        "noRoomInItem",
        "slot-11",
        matches!(full, Err(EnchantGate::NoRoomInItem)),
        SRC_SLOT_AVAILABLE,
    );

    // F6 — a purse short of `points · 10`.
    let short = apply_enchantments(&EnchantRequest {
        item_power: 10_000,
        gold: 999,
        occupied_slots: 0,
        selection: &[feather],
    });
    refuse(
        out,
        "F6",
        "notEnoughGold",
        "gold-999-vs-1000",
        matches!(short, Err(EnchantGate::NotEnoughGold)),
        SRC_COST_GOLD,
    );

    // F7 — an unknown enchantment kind does not resolve.
    refuse(
        out,
        "F7",
        "unknown-enchantment",
        "key-NoSuchKind",
        effect("NoSuchKind").is_none() && extra_spell_pts("NoSuchVariant").is_none(),
        SRC_FIND_ENCHANTMENT,
    );

    // The three gates are strict: equalling the budget or the gold is valid.
    let exact = apply_enchantments(&EnchantRequest {
        item_power: 100,
        gold: 1_000,
        occupied_slots: 0,
        selection: &[feather],
    });
    out.check_bool(
        "F8",
        exact.is_ok(),
        true,
        "equalling budget (100) and gold (1000) is accepted — the gates are strict `<` (GV-ENC-07/09)",
        SRC_GATES,
    );
    let names = [
        EnchantGate::NoRoomInItem,
        EnchantGate::NotEnoughGold,
        EnchantGate::BeyondLimit,
    ]
    .map(gate_name)
    .join(",");
    out.check(
        "F9",
        names,
        String::from("noRoomInItem,notEnoughGold,beyondLimit"),
        "3 gates, in source order, each with its own name",
        SRC_GATES,
    );
    out.check_n(
        "F10",
        i64::from(cost_points(&[feather])),
        100,
        "one power costs 100 points; gold = points · 10 (GOLD_PER_POINT)",
        SRC_GOLD_PER_POINT,
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// custody + main
// ═══════════════════════════════════════════════════════════════════════════

fn skip(out: &mut Out, reason: &str) -> i32 {
    out.lines.push(format!("reason={reason}"));
    out.lines
        .push(format!("VEREDICTO: SKIP - {reason} (never a PASS without retail)"));
    for line in &out.lines {
        println!("{line}");
    }
    2
}

pub fn run_sys_craft_001() -> i32 {
    let mut out = Out::new();
    out.lines.push(format!(
        "PROBE-SYS-CRAFT-001 - enchanting points/slots, MAGIC.DEF artefact census, 20 classic recipes, cauldron mixing, adversarial refusals (M41/W41-4, {CR})"
    ));

    let magic_path = match crate::mode_a_arena2_path(MAGIC_DEF_FILENAME) {
        Ok(path) => path,
        Err(reason) => return skip(&mut out, &format!("missing_magic_def: {reason}")),
    };
    let magic_bytes = match std::fs::read(&magic_path) {
        Ok(bytes) => bytes,
        Err(err) => {
            return skip(
                &mut out,
                &format!("missing_magic_def: reading {magic_path:?}: {err}"),
            )
        }
    };
    let records = match parse_magic_def(&magic_bytes) {
        Ok(records) => records,
        Err(err) => return skip(&mut out, &format!("invalid_magic_def: {err}")),
    };
    let fall = match daggerfall_app::mode_a::read_install_file(daggerfall_inventory::retail_manifest::FALL_EXE) {
        Ok(bytes) => bytes,
        Err(reason) => return skip(&mut out, &format!("missing_fall_exe: {reason}")),
    };
    let templates = match parse_item_templates(&fall) {
        Ok(templates) => templates,
        Err(err) => return skip(&mut out, &format!("invalid_fall_exe: {err}")),
    };

    // Custody rows: the two retail artifacts the census is derived from.
    out.check_bool(
        "R-MAGIC",
        4 + records.len() * MAGIC_RECORD_SIZE == magic_bytes.len(),
        true,
        &format!(
            "ARENA2/{MAGIC_DEF_FILENAME}: {} B = 4 + {}·{MAGIC_RECORD_SIZE}; the census covers every record",
            magic_bytes.len(),
            records.len()
        ),
        SRC_MAGIC_PARSE,
    );
    out.check_n(
        "R-ITEMS",
        templates.len() as i64,
        TOTAL_ITEMS as i64,
        &format!(
            "FALL.EXE: {} templates, {} with enchantment_points > 0, {} ingredients (GV-ITM census)",
            templates.len(),
            templates
                .iter()
                .filter(|template| template.enchantment_points > 0)
                .count(),
            templates
                .iter()
                .filter(|template| template.is_ingredient())
                .count()
        ),
        SRC_TEMPLATE_PARSE,
    );

    block_a(&mut out, &templates);
    block_b(&mut out, &templates, &records);
    block_c(&mut out, &records, magic_bytes.len());
    block_d(&mut out, &CLASSIC_RECIPES, &templates);
    block_e(&mut out);
    block_f(&mut out, &templates);

    let (points, points_mat, points_armor, account, slots, artefacts, recipes, mixes, refusals) = (
        out.counts.points,
        out.counts.points_mat,
        out.counts.points_armor,
        out.counts.account,
        out.counts.slots,
        out.counts.artefacts,
        out.counts.recipes,
        out.counts.mixes,
        out.counts.refusals,
    );
    let total = out.counts.total();
    let passed = out.checks - out.failures;
    let failures = out.failures;
    out.lines.push(format!(
        "ROWS points={points} points-mat={points_mat} points-armor={points_armor} account={account} slots={slots} artefacts={artefacts} recipes={recipes} mixes={mixes} refusals={refusals} total={total}"
    ));
    out.lines
        .push(format!("CHECKS passed={passed} failed={failures}"));

    let pass = failures == 0;
    out.lines
        .push(format!("VEREDICTO: {}", if pass { "PASS" } else { "FAIL" }));
    let digest = crate::m37_ui1_probe::sha256_hex(out.lines.join("\n").as_bytes());
    out.lines.push(format!("DIGEST {digest}"));
    for line in &out.lines {
        println!("{line}");
    }
    i32::from(!pass)
}
