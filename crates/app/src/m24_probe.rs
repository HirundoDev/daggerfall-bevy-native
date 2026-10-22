//! Probes M24 — item enchanting and potion alchemy (SPEC M24 §5,
//! REQ-L99-057).
//!
//! `PROBE-ENCHANT-001` — 34 golden vectors in three blocks:
//!
//! - **A · GV-MAG-01..10** — the retail `MAGIC.DEF` layout: record count 59,
//!   62-byte records, 3662-byte total, and the measured rec0/rec1 fields
//!   (GV-MAG-04..08). Reads `ARENA2/MAGIC.DEF`; absent ⇒ **exit 2**
//!   `reason=missing_magic_def` (never a PASS without retail).
//! - **B · GV-PWR-01..12** — enchantment power by material (weapons and
//!   armor, `base + floor(base · mult)`, spec §2.2).
//! - **C · GV-ENC-01..12** — power/side-effect accounting, gold cost and the
//!   three gates (spec §2.5).
//!
//! Plus the §3.4 **effect-key contract**: all 19 M23 keys the recipes and the
//! registry reference must resolve — a missing key prints `EFFECT_KEY_MISSING
//! <key>` and fails the probe (never a silent pass, R-G4).
//!
//! `PROBE-POTION-001` — 23 golden vectors: **GV-RCP-01..13** (exact recipe
//! hashes over sorted ids, incl. the `purification` `int` wrap) + **GV-POT-01..10**
//! (prices, statistics, textures). `recipes=20 checks=23`. The SHA-256 digest
//! over `(recipe_key, price, effect_key, variant)` sorted by key is computed
//! twice and must be byte-identical (runtime ×2). Reads `FALL.EXE` to validate
//! every recipe's ingredient template ids against the `is_ingredient` bit
//! (M21 census); absent ⇒ **exit 2**. Standalone (`--probe POTION-001`), the
//! same pattern as `EFFECT-001`/`SPELLMAKER-001`.
//!
//! **Custodia (INV-L99-6):** Mode A only — retail bytes are read from the
//! user's ARENA2/install root at runtime and never enter the repository. 0
//! Bevy, 0 GPU.

use sha2::{Digest, Sha256};

use daggerfall_behavior::crafting::{
    armor_enchant_mult, enchantment_power, recipe_hash, weapon_enchant_mult,
};
use daggerfall_behavior::magic::effect_by_key;
use daggerfall_crafting::alchemy::{
    CLASSIC_RECIPES, CLASSIC_RECIPE_KEYS, RECIPE_TEXTURE_DEFAULT,
};
use daggerfall_crafting::enchanting::{
    apply_enchantments, cost_gold, cost_points, effect, extra_spell_pts, EnchantRequest,
    EnchantmentSettings,
};
use daggerfall_crafting::magic_def::{
    parse_magic_def, MagicItemTemplate, MAGIC_ENCHANT_SLOTS, MAGIC_DEF_FILENAME, MAGIC_RECORD_SIZE,
};
use daggerfall_inventory::items::parse_item_templates;
use std::path::PathBuf;

// ── evidence rows (M19/M20/M21/M23 pattern) ──────────────────────────────

/// Printed row: `[OK] <id> - <detail>` or `FAIL <id> got=<v> want=<v>`.
#[derive(Debug, Clone)]
struct Row {
    id: &'static str,
    ok: bool,
    got: String,
    want: String,
    detail: String,
}

/// Row whose verdict is the equality `got == want`.
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

/// SHA-256 in lowercase hex (spec §5 digests).
fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hasher.finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

// ── source custody (Mode A) ──────────────────────────────────────────

/// Mode A install root (same resolution as `m21_probe::install_root`;
/// `FALL.EXE` lives at the ROOT, never in `ARENA2/`, spec §0).
fn install_root() -> Result<PathBuf, String> {
    match std::env::var_os("DF_APP_ROOT") {
        Some(value) => Ok(PathBuf::from(value)),
        None => {
            let home =
                std::env::var_os("HOME").ok_or("HOME not set (set $DF_APP_ROOT)")?;
            Ok(PathBuf::from(home).join("Games/Daggerfall/app"))
        }
    }
}

/// Reads `FALL.EXE` from the install root. `Err` ⇒ the caller closes with
/// `EXIT=2` (never a PASS without retail data).
fn load_fall_exe() -> Result<Vec<u8>, String> {
    let root = install_root()?;
    let path = root.join("FALL.EXE");
    std::fs::read(&path).map_err(|err| format!("FALL.EXE unreadable: {err} ({path:?})"))
}

// ═══════════════════════════════════════════════════════════════════════════
// PROBE-ENCHANT-001
// ═══════════════════════════════════════════════════════════════════════════

/// The 19 M23 effect keys the contract (§3.4) requires: 14 literal keys plus
/// the 5 `ElementalResistance` variants (matched by `(class, variant)`, but
/// exposed to M24 under the composite key).
const CONTRACT_KEYS: [&str; 19] = [
    "Heal-Fatigue",
    "Fortify-Strength",
    "Heal-Health",
    "Heal-SpellPoints",
    "Cure-Disease",
    "FreeAction",
    "Cure-Poison",
    "WaterWalking",
    "Levitate",
    "WaterBreathing",
    "Slowfall",
    "Shadow-Normal",
    "Invisibility-Normal",
    "Chameleon-Normal",
    "ElementalResistance-Fire",
    "ElementalResistance-Frost",
    "ElementalResistance-Poison",
    "ElementalResistance-Shock",
    "ElementalResistance-Magicka",
];

/// Block A — GV-MAG-01..10 over the parsed retail `MAGIC.DEF`.
///
/// `bytes` is the raw retail blob (for the size and stride vectors);
/// `records` is its parse.
fn block_a_rows(records: &[MagicItemTemplate], bytes: &[u8]) -> Vec<Row> {
    let file_len = bytes.len();
    let rec0 = records.first();
    let rec1 = records.get(1);
    let name0 = rec0.map(|r| r.name.as_str()).unwrap_or("");
    let name1 = rec1.map(|r| r.name.as_str()).unwrap_or("");
    let ench0 = rec0.map(|r| r.enchantments[0]).unwrap_or((0, 0));
    let ench1 = rec1.map(|r| r.enchantments[0]).unwrap_or((0, 0));

    // GV-MAG-10: reading ONE record advances exactly 62 bytes. A 1-record
    // header over the first 62 bytes parses to exactly 1 record, but the same
    // header over only 61 bytes is truncated — so the record cannot be shorter
    // than 62 (and GV-MAG-03 pins the total, so it cannot be longer).
    let mut prefix_ok = false;
    if bytes.len() >= 4 + MAGIC_RECORD_SIZE {
        let mut full = (1u32).to_le_bytes().to_vec();
        full.extend_from_slice(&bytes[4..4 + MAGIC_RECORD_SIZE]);
        let mut short = (1u32).to_le_bytes().to_vec();
        short.extend_from_slice(&bytes[4..4 + MAGIC_RECORD_SIZE - 1]);
        prefix_ok = parse_magic_def(&full).map(|r| r.len() == 1).unwrap_or(false)
            && parse_magic_def(&short).is_err();
    }
    let stride_ok = prefix_ok;

    vec![
        row(
            "GV-MAG-01",
            format!("recordCount={}", records.len()),
            "recordCount=59".to_string(),
            "u32 header = 59 records (`MagicItemsFile.cs:76`)",
        ),
        row(
            "GV-MAG-02",
            format!("size={}", MAGIC_RECORD_SIZE),
            "size=62".to_string(),
            "layout 32+1+1+1+20+2+4+1 = 62 B (`:83-105`)",
        ),
        row(
            "GV-MAG-03",
            format!("total={file_len} implied={}", 4 + records.len() * MAGIC_RECORD_SIZE),
            format!("total=3662 implied={}", 4 + 59 * MAGIC_RECORD_SIZE),
            "4 + 59·62 = 3662 B medidos ✔",
        ),
        row(
            "GV-MAG-04",
            format!("name0={name0}"),
            "name0=The Masque of Clavicus".to_string(),
            "record 0 name (measured)",
        ),
        row(
            "GV-MAG-05",
            format!(
                "groupIndex={}/{}",
                rec0.map(|r| r.group).unwrap_or(0),
                rec0.map(|r| r.group_index).unwrap_or(0)
            ),
            "groupIndex=25/2".to_string(),
            "record 0 group/groupIndex (measured)",
        ),
        row(
            "GV-MAG-06",
            format!(
                "uses/value/material={}/{}/{}",
                rec0.map(|r| r.uses).unwrap_or(0),
                rec0.map(|r| r.value).unwrap_or(0),
                rec0.map(|r| r.material).unwrap_or(0)
            ),
            "uses/value/material=1500/50000/0".to_string(),
            "record 0 i16/i32/u8 LE (measured)",
        ),
        row(
            "GV-MAG-07",
            format!("ench0={ench0:?}"),
            "ench0=(26, 0)".to_string(),
            "record 0 enchantments[0] type=26 artifact",
        ),
        row(
            "GV-MAG-08",
            format!(
                "name1={name1} group={}/{} ench={:?}",
                rec1.map(|r| r.group).unwrap_or(0),
                rec1.map(|r| r.group_index).unwrap_or(0),
                ench1
            ),
            "name1=The Mehrunes Razor group=3/0 ench=(26, 1)".to_string(),
            "record 1 Weapons (measured)",
        ),
        row(
            "GV-MAG-09",
            format!("slots_bytes={}", MAGIC_ENCHANT_SLOTS * 2),
            "slots_bytes=20".to_string(),
            "10 × (s8,s8) = 20 B (`:892` guarda == 10)",
        ),
        row(
            "GV-MAG-10",
            format!("stride_ok={stride_ok}"),
            "stride_ok=true".to_string(),
            "1 record advances 62 B (prefix 62 ⇒ ok, 61 ⇒ truncated)",
        ),
    ]
}

/// Block B — GV-PWR-01..12, power by material (§2.2).
fn enchant_block_b_rows() -> Vec<Row> {
    // Longsword base 600 (rec120), Boots 175 (rec108), Cuirass 400 (rec102),
    // Staff 700 (rec115) — measured in FALL.EXE (§2.2).
    let longsword = |material: usize| enchantment_power(600, weapon_enchant_mult(material));
    let boots = |native: i32| enchantment_power(175, armor_enchant_mult(native));
    let cuirass = |native: i32| enchantment_power(400, armor_enchant_mult(native));
    let staff = |material: usize| enchantment_power(700, weapon_enchant_mult(material));

    vec![
        row(
            "GV-PWR-01",
            format!("power={}", longsword(0)),
            "power=450".to_string(),
            "Longsword 600 Iron(0): 600 + floor(−150)",
        ),
        row(
            "GV-PWR-02",
            format!("power={}", longsword(1)),
            "power=600".to_string(),
            "Longsword 600 Steel(1): 600 + 0",
        ),
        row(
            "GV-PWR-03",
            format!("power={}", longsword(3)),
            "power=750".to_string(),
            "Longsword 600 Elven(3): 600 + floor(150)",
        ),
        row(
            "GV-PWR-04",
            format!("power={}", longsword(4)),
            "power=900".to_string(),
            "Longsword 600 Dwarven(4): 600 + floor(300)",
        ),
        row(
            "GV-PWR-05",
            format!("power={}", longsword(7)),
            "power=1200".to_string(),
            "Longsword 600 Ebony(7): 600 + floor(600)",
        ),
        row(
            "GV-PWR-06",
            format!("power={}", longsword(9)),
            "power=1800".to_string(),
            "Longsword 600 Daedric(9): 600 + floor(1200)",
        ),
        row(
            "GV-PWR-07",
            format!("power={}", boots(0x200)),
            "power=131".to_string(),
            "Boots 175 Iron(0x200): 175 + floor(−43.75)=131 (FLOOR)",
        ),
        row(
            "GV-PWR-08",
            format!("power={}", boots(0x209)),
            "power=525".to_string(),
            "Boots 175 Daedric(0x209): 175 + floor(350)",
        ),
        row(
            "GV-PWR-09",
            format!("power={}", cuirass(0x000)),
            "power=400".to_string(),
            "Cuirass 400 Leather(0x000): 400 + 0",
        ),
        row(
            "GV-PWR-10",
            format!("power={}", cuirass(0x209)),
            "power=1200".to_string(),
            "Cuirass 400 Daedric(0x209): 400 + floor(800)",
        ),
        row(
            "GV-PWR-11",
            format!("power={}", staff(5)),
            "power=875".to_string(),
            "Staff 700 Mithril(5): 700 + floor(175)",
        ),
        row(
            "GV-PWR-12",
            format!("power={}", staff(9)),
            "power=2100".to_string(),
            "Staff 700 Daedric(9): 700 + floor(1400)",
        ),
    ]
}

/// Formats the verdict of applying a selection: `accept` or `reject:<gate>`.
fn enc_verdict(power: i32, gold: i32, selection: &[EnchantmentSettings]) -> String {
    match apply_enchantments(&EnchantRequest {
        item_power: power,
        gold,
        occupied_slots: 0,
        selection,
    }) {
        Ok(item) => format!("accept gold_after={}", item.gold_after),
        Err(gate) => format!("reject:{gate}"),
    }
}

/// Block C — GV-ENC-01..12, accounting and gates (§2.5).
fn enchant_block_c_rows() -> Vec<Row> {
    let feather = effect("FeatherWeight").expect("FeatherWeight");
    let strengthens = effect("StrengthensArmor").expect("StrengthensArmor");
    let enhances = effect("EnhancesSkill").expect("EnhancesSkill");
    let repairs = effect("RepairsObjects").expect("RepairsObjects");
    let absorbs = effect("AbsorbsSpells").expect("AbsorbsSpells");
    let animals = extra_spell_pts("Animals").expect("ExtraSpellPts.Animals");
    let extra_weight = effect("ExtraWeight").expect("ExtraWeight");

    // GV-ENC-04 selection: EnhancesSkill 900 + ExtraSpellPts(Animals) 1000 +
    // FeatherWeight 100 = 2000 points / 20000 gold.
    let enc04 = [enhances, animals, feather];

    // `pts/gold/verdict` for a selection at its accepting budget: `got` and
    // `want` share one format so the equality test is meaningful.
    let accept_row = |id: &'static str,
                      sel: &[EnchantmentSettings],
                      want_pts: i32,
                      want_gold: i32,
                      detail: &str| {
        let points = cost_points(sel);
        let gold = cost_gold(sel);
        let verdict = enc_verdict(want_pts, want_gold, sel);
        row(
            id,
            format!("pts={points} gold={gold} {verdict}"),
            format!("pts={want_pts} gold={want_gold} accept gold_after=0"),
            detail.to_string(),
        )
    };

    let mut rows = vec![
        accept_row(
            "GV-ENC-01",
            &[feather],
            100,
            1000,
            "FeatherWeight 100 (poder): 100 pts / 1000 gp (§2.3)",
        ),
        accept_row(
            "GV-ENC-02",
            &[absorbs],
            1500,
            15000,
            "AbsorbsSpells 1500 (poder): 1500 pts / 15000 gp",
        ),
        accept_row(
            "GV-ENC-03",
            &[enhances, repairs],
            1800,
            18000,
            "EnhancesSkill+RepairsObjects (both power): 1800/18000",
        ),
        accept_row(
            "GV-ENC-04",
            &enc04,
            2000,
            20000,
            "EnhancesSkill+ExtraSpellPts(Animals)+FeatherWeight: 2000/20000",
        ),
        accept_row(
            "GV-ENC-05",
            &[strengthens],
            700,
            7000,
            "StrengthensArmor 700 (poder): 700/7000",
        ),
    ];

    // GV-ENC-06..09 — the strict gates on the GV-ENC-04 selection (2000 pts).
    rows.push(row(
        "GV-ENC-06",
        enc_verdict(1800, 20000, &enc04),
        "reject:beyondLimit".to_string(),
        "2000 pts with power 1800 ⇒ rejects (strict <)",
    ));
    rows.push(row(
        "GV-ENC-07",
        enc_verdict(2000, 20000, &enc04).split(' ').next().unwrap_or("").to_string(),
        "accept".to_string(),
        "2000 pts with power 2000 ⇒ accepts (matching the budget is valid)",
    ));
    rows.push(row(
        "GV-ENC-08",
        enc_verdict(2000, 19999, &enc04),
        "reject:notEnoughGold".to_string(),
        "gold 19999 < 20000 ⇒ rejects (strict <)",
    ));
    rows.push(row(
        "GV-ENC-09",
        enc_verdict(2000, 20000, &enc04),
        "accept gold_after=0".to_string(),
        "gold 20000 ≥ 20000 ⇒ accepts, 0 left",
    ));

    // GV-ENC-10 — the 11th slot is blocked (`:629`, `:674`, `:892`).
    let tenth = match apply_enchantments(&EnchantRequest {
        item_power: 9999,
        gold: 99999,
        occupied_slots: 10,
        selection: &[feather],
    }) {
        Ok(_) => "accept".to_string(),
        Err(gate) => format!("reject:{gate}"),
    };
    rows.push(row(
        "GV-ENC-10",
        tenth,
        "reject:noRoomInItem".to_string(),
        "10 enchantments already applied ⇒ slot 11 blocked",
    ));

    // GV-ENC-11 — empty selection is a no-op at zero cost.
    rows.push(row(
        "GV-ENC-11",
        {
            let sel: [EnchantmentSettings; 0] = [];
            let points = cost_points(&sel);
            let gold = cost_gold(&sel);
            let verdict = enc_verdict(0, 0, &sel);
            format!("pts={points} gold={gold} {verdict}")
        },
        "pts=0 gold=0 accept gold_after=0".to_string(),
        "0 enchantments ⇒ 0/0, accepts without changing anything",
    ));

    // GV-ENC-12 — the discriminating case: a negative side-effect subtracts
    // points (900 − 100 = 800) and costs no gold (900 · 10 = 9000).
    rows.push(accept_row(
        "GV-ENC-12",
        &[enhances, extra_weight],
        800,
        9000,
        "EnhancesSkill 900 (power) + ExtraWeight −100 (secondary): 800/9000",
    ));

    rows
}

/// `--probe ENCHANT-001` — 34 GV + §3.4 contract (spec §5).
/// `EXIT=2` if `MAGIC.DEF`/ARENA2 is not accessible.
pub fn run_enchant_001() -> i32 {
    println!(
        "PROBE-ENCHANT-001 - M24 enchanting (REQ-L99-057 · CR-f26e353c · spec §5 A-C)"
    );
    let path = match crate::mode_a_arena2_path(MAGIC_DEF_FILENAME) {
        Ok(path) => path,
        Err(reason) => {
            println!("ARENA2 not accessible: {reason}");
            println!("reason=missing_magic_def");
            println!("EXIT=2");
            println!(
                "VERDICT: PROBE-ENCHANT-001 NO DATA - MAGIC.DEF not accessible (A = census; never a PASS without retail)"
            );
            return 2;
        }
    };
    let bytes = match std::fs::read(&path) {
        Ok(bytes) => bytes,
        Err(err) => {
            println!("ARENA2 not accessible: reading {path:?}: {err}");
            println!("reason=missing_magic_def");
            println!("EXIT=2");
            println!(
                "VERDICT: PROBE-ENCHANT-001 NO DATA - MAGIC.DEF unreadable (never a PASS without retail)"
            );
            return 2;
        }
    };
    let records = match parse_magic_def(&bytes) {
        Ok(records) => records,
        Err(err) => {
            println!("MAGIC.DEF invalid: {err}");
            println!("reason=missing_magic_def");
            println!("EXIT=2");
            println!(
                "VERDICT: PROBE-ENCHANT-001 NO DATA - MAGIC.DEF does not parse ({err})"
            );
            return 2;
        }
    };
    println!(
        "[OK] mode_a {}: {} ({} B · recordCount={} · {} B/record)",
        MAGIC_DEF_FILENAME,
        path.display(),
        bytes.len(),
        records.len(),
        MAGIC_RECORD_SIZE
    );

    let a_rows = block_a_rows(&records, &bytes);
    print_rows("A · MAGIC.DEF census + GV-MAG-01..10", &a_rows);
    let b_rows = enchant_block_b_rows();
    print_rows("B · budget by material (GV-PWR-01..12)", &b_rows);
    let c_rows = enchant_block_c_rows();
    print_rows("C · accounting and gates (GV-ENC-01..12)", &c_rows);

    // §3.4 effect-key contract: every recipe/registry key must exist in M23.
    println!("\n-- §3.4 contract (M23 keys) --");
    let mut contract_ok = true;
    for key in CONTRACT_KEYS {
        if effect_by_key(key).is_none() {
            contract_ok = false;
            println!("EFFECT_KEY_MISSING {key}");
        }
    }
    if contract_ok {
        println!("[OK] contract-keys - 19/19 effect keys resolved in M23 (§3.4)");
    }

    let checks = a_rows.len() + b_rows.len() + c_rows.len();
    let passed = a_rows
        .iter()
        .chain(&b_rows)
        .chain(&c_rows)
        .filter(|r| r.ok)
        .count();
    let failed = checks - passed;
    let code = i32::from(failed > 0 || !contract_ok);
    println!("checks={checks} passed={passed} failed={failed}");
    println!("EXIT={code}");
    if code == 0 {
        println!(
            "VERDICT: PROBE-ENCHANT-001 PASS - A∧B∧C∧contract · MAGIC.DEF 59×62=3662 B · GV-PWR 12 materials (floor) · GV-ENC 12 (sign, gold ×10, 3 strict gates) · 19/19 keys §3.4"
        );
    } else {
        println!(
            "VERDICT: PROBE-ENCHANT-001 FAIL - see [FAIL] rows and EFFECT_KEY_MISSING (checks={checks})"
        );
    }
    code
}

// ═══════════════════════════════════════════════════════════════════════════
// PROBE-POTION-001
// ═══════════════════════════════════════════════════════════════════════════

/// SHA-256 digest over the 20 recipes as `(recipe_key, price, effect_key,
/// variant)` rows sorted by key (spec §5 B). Deterministic — called twice must
/// be byte-identical (runtime ×2).
fn potion_digest() -> String {
    let mut rows: Vec<(i32, i32, &str, &str)> = CLASSIC_RECIPES
        .iter()
        .map(|recipe| (recipe.key(), recipe.price, recipe.effect_key, recipe.variant))
        .collect();
    rows.sort_by_key(|(key, ..)| *key);
    let mut buf = Vec::new();
    for (key, price, effect_key, variant) in rows {
        buf.extend_from_slice(format!("{key},{price},{effect_key},{variant}\n").as_bytes());
    }
    sha256_hex(&buf)
}

/// Block B — GV-RCP-01..13: exact recipe hashes (§3.5).
fn block_b_rows() -> Vec<Row> {
    let hash = |ids: &[i32]| recipe_hash(ids);
    // GV-RCP-13: all 20 recipes fold to their canonical keys, and only
    // `purification` overflows `2^31` needing the `int` wrap.
    let matched = CLASSIC_RECIPES
        .iter()
        .zip(CLASSIC_RECIPE_KEYS)
        .filter(|(recipe, key)| recipe.key() == *key)
        .count();
    let purification = &CLASSIC_RECIPES[19].ingredients;
    let raw_purification: i64 = {
        let mut sorted = (*purification).to_vec();
        sorted.sort_unstable();
        let mut h: i64 = 17;
        for id in sorted {
            h = h * 23 + i64::from(id);
        }
        h
    };
    let wrap_ok = raw_purification == 1_346_355_782_844 && recipe_hash(purification) == 2_031_019_196;

    vec![
        row(
            "GV-RCP-01",
            format!("hash={}", hash(&[27, 30, 59])),
            "hash=221871".to_string(),
            "{27,30,59} = classicRecipeKeys[0]",
        ),
        row(
            "GV-RCP-02",
            format!("hash={}", hash(&[])),
            "hash=0".to_string(),
            "empty list ⇒ 0 (`:297-298`)",
        ),
        row(
            "GV-RCP-03",
            format!("hash={}", hash(&[9])),
            "hash=400".to_string(),
            "391 + 9",
        ),
        row(
            "GV-RCP-04",
            format!("hash={}", hash(&[0])),
            "hash=391".to_string(),
            "17·23 + 0",
        ),
        row(
            "GV-RCP-05",
            format!("hash={}", hash(&[1, 2])),
            "hash=9018".to_string(),
            "392 → 9016 + 2",
        ),
        row(
            "GV-RCP-06",
            format!("hash={}", hash(&[2, 3])),
            "hash=9042".to_string(),
            "393 → 9039 + 3",
        ),
        row(
            "GV-RCP-07",
            format!("hash={}", hash(&[27, 56])),
            "hash=9670".to_string(),
            "418 → 9614 + 56",
        ),
        row(
            "GV-RCP-08",
            format!("hash={}", hash(&[30, 59])),
            "hash=9742".to_string(),
            "421 → 9683 + 59",
        ),
        row(
            "GV-RCP-09",
            format!("hash={}", hash(&[4, 5, 6])),
            "hash=209076".to_string(),
            "395 → 9090 → 209070 + 6",
        ),
        row(
            "GV-RCP-10",
            format!("hash={}", hash(&[1000])),
            "hash=1391".to_string(),
            "391 + 1000",
        ),
        row(
            "GV-RCP-11",
            format!("hash={}", hash(&[112, 227])),
            "hash=11796".to_string(),
            "503 → 11569 + 227",
        ),
        row(
            "GV-RCP-12",
            format!("hash={}", hash(&[59, 27, 30])),
            "hash=221871".to_string(),
            "prior sorting ⇒ identical to GV-RCP-01",
        ),
        row(
            "GV-RCP-13",
            format!("{matched}/20 wrap_ok={wrap_ok}"),
            "20/20 wrap_ok=true".to_string(),
            "the 20 keys resolved by the fold; only purification overflows 2³¹ (1346355782844 → 2031019196)",
        ),
    ]
}

/// Block C — GV-POT-01..10: prices and accounting (§3.6).
fn block_c_rows() -> Vec<Row> {
    let price = |name: &str| {
        CLASSIC_RECIPES
            .iter()
            .find(|r| r.display_name_key == name)
            .map(|r| r.price)
            .unwrap_or(-1)
    };
    let total: i32 = CLASSIC_RECIPES.iter().map(|r| r.price).sum();
    let count = CLASSIC_RECIPES.len() as i32;
    let ge200 = CLASSIC_RECIPES.iter().filter(|r| r.price >= 200).count();
    let le50 = CLASSIC_RECIPES.iter().filter(|r| r.price <= 50).count();

    vec![
        row(
            "GV-POT-01",
            format!("price={}", price("stamina")),
            "price=25".to_string(),
            "stamina 25 gp (HealFatigue.cs:47)",
        ),
        row(
            "GV-POT-02",
            format!("price={}", price("healing")),
            "price=50".to_string(),
            "healing 50 gp (HealHealth.cs:47)",
        ),
        row(
            "GV-POT-03",
            format!("price={}", price("purification")),
            "price=500".to_string(),
            "purification 500 gp (CureDisease.cs:57)",
        ),
        row(
            "GV-POT-04",
            format!("price={}", price("invisibility")),
            "price=250".to_string(),
            "invisibility 250 gp (InvisibilityNormal.cs:49)",
        ),
        row(
            "GV-POT-05",
            format!("price={}", price("resistPoison")),
            "price=125".to_string(),
            "resistPoison 125 gp (ElementalResistance.cs:130)",
        ),
        row(
            "GV-POT-06",
            format!("sum={total}"),
            "sum=2600".to_string(),
            "Σ 20 prices = 2600",
        ),
        row(
            "GV-POT-07",
            format!("avg={}", total / count),
            "avg=130".to_string(),
            "2600 / 20 = 130",
        ),
        row(
            "GV-POT-08",
            format!("ge200={ge200}"),
            "ge200=5".to_string(),
            "5 recipes with price ≥ 200",
        ),
        row(
            "GV-POT-09",
            format!("le50={le50}"),
            "le50=4".to_string(),
            "4 recipes with price ≤ 50",
        ),
        row(
            "GV-POT-10",
            format!("texture={RECIPE_TEXTURE_DEFAULT}"),
            "texture=11".to_string(),
            "default TextureRecord = 11 (PotionRecipe.cs:34)",
        ),
    ]
}

/// `--probe POTION-001` — 20 recipes · 23 GV + digest ×2 (spec §5 B).
/// `EXIT=2` if `FALL.EXE` is not accessible (M21 ingredient census).
pub fn run_potion_001() -> i32 {
    println!(
        "PROBE-POTION-001 - M24 alchemy (REQ-L99-057 · CR-f26e353c · spec §5 B)"
    );
    let bytes = match load_fall_exe() {
        Ok(bytes) => bytes,
        Err(reason) => {
            println!("ARENA2 not accessible: {reason}");
            println!("EXIT=2");
            println!(
                "VERDICT: PROBE-POTION-001 NO DATA - FALL.EXE not accessible (ingredients not censused; never a PASS without retail)"
            );
            return 2;
        }
    };
    let templates = match parse_item_templates(&bytes) {
        Ok(templates) => templates,
        Err(err) => {
            println!("FALL.EXE invalid: {err:?}");
            println!("EXIT=2");
            println!(
                "VERDICT: PROBE-POTION-001 NO DATA - FALL.EXE templates do not parse"
            );
            return 2;
        }
    };
    println!(
        "[OK] mode_a FALL.EXE: {} templates (M21 census)",
        templates.len()
    );

    let digest_a = potion_digest();
    let digest_b = potion_digest();
    let recipes = CLASSIC_RECIPES.len();
    println!("recipes={recipes}");
    println!("digest={digest_a}");
    println!("digest_x2={digest_a} == {digest_b} (byte-identical: {})", digest_a == digest_b);

    let b_rows = block_b_rows();
    print_rows("B · recipe hashes (GV-RCP-01..13)", &b_rows);
    let c_rows = block_c_rows();
    print_rows("C · prices and accounting (GV-POT-01..10)", &c_rows);

    // Ingredient census: every recipe's template ids must be `is_ingredient`
    // in the FALL.EXE catalog (M21), else the recipe references a non-ingredient.
    println!("\n-- ingredient census (M21, is_ingredient bit) --");
    let mut ing_total = 0usize;
    let mut ing_ok = 0usize;
    let mut ing_bad: Vec<String> = Vec::new();
    for recipe in &CLASSIC_RECIPES {
        for &id in recipe.ingredients {
            ing_total += 1;
            match templates.get(id as usize) {
                Some(template) if template.is_ingredient() => ing_ok += 1,
                Some(template) => ing_bad.push(format!(
                    "{}#{} props=0x{:02x} !is_ingredient",
                    recipe.display_name_key, id, template.properties
                )),
                None => ing_bad.push(format!(
                    "{}#{} out of table ({} templates)",
                    recipe.display_name_key,
                    id,
                    templates.len()
                )),
            }
        }
    }
    let ingredients_ok = ing_bad.is_empty();
    if ingredients_ok {
        println!("[OK] ingredients - {ing_ok}/{ing_total} references with the is_ingredient bit (20 recipes)");
    } else {
        for bad in &ing_bad {
            println!("FAIL ingredient {bad}");
        }
    }

    let checks = b_rows.len() + c_rows.len();
    let passed = b_rows
        .iter()
        .chain(&c_rows)
        .filter(|r| r.ok)
        .count();
    let failed = checks - passed;
    let digest_ok = digest_a == digest_b;
    let code = i32::from(failed > 0 || !ingredients_ok || !digest_ok);
    println!("checks={checks} passed={passed} failed={failed}");
    println!("EXIT={code}");
    if code == 0 {
        println!(
            "VERDICT: PROBE-POTION-001 PASS - recipes=20 · GV-RCP 13 (20/20 keys, purification wrap) · GV-POT 10 (Σ2600 avg130) · digest ×2 identical · {ing_total}/{ing_total} ingredients is_ingredient"
        );
    } else {
        println!(
            "VERDICT: PROBE-POTION-001 FAIL - see [FAIL] rows (checks={checks})"
        );
    }
    code
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The ENCHANT probe's three blocks total exactly 34 checks (10 GV-MAG +
    /// 12 GV-PWR + 12 GV-ENC), the contract count in the spec §5.
    #[test]
    fn enchant_probe_has_34_checks() {
        let (records, blob) = synthetic_records();
        let a = block_a_rows(&records, &blob).len();
        let b = enchant_block_b_rows().len();
        let c = enchant_block_c_rows().len();
        assert_eq!(a + b + c, 34, "GV-MAG {a} + GV-PWR {b} + GV-ENC {c}");
    }

    /// The POTION probe's two blocks total exactly 23 checks (13 GV-RCP +
    /// 10 GV-POT).
    #[test]
    fn potion_probe_has_23_checks() {
        let b = block_b_rows().len();
        let c = block_c_rows().len();
        assert_eq!(b + c, 23, "GV-RCP {b} + GV-POT {c}");
    }

    /// The digest is deterministic across runs (runtime ×2) and the block rows
    /// all pass on the fixed data.
    #[test]
    fn potion_digest_is_stable_and_rows_pass() {
        assert_eq!(potion_digest(), potion_digest());
        for probe_rows in [block_b_rows(), block_c_rows()] {
            for row in probe_rows {
                assert!(row.ok, "{} got={} want={}", row.id, row.got, row.want);
            }
        }
    }

    /// Blocks B and C (24 rows) pass fully on the fixed data; in block A only
    /// the data-independent layout rows hold on a synthetic 2-record blob
    /// (count 59 and the 3662-byte total are retail vectors checked live).
    #[test]
    fn enchant_rows_pass() {
        for probe_rows in [enchant_block_b_rows(), enchant_block_c_rows()] {
            for row in probe_rows {
                assert!(row.ok, "{} got={} want={}", row.id, row.got, row.want);
            }
        }
        let (records, blob) = synthetic_records();
        let a_rows = block_a_rows(&records, &blob);
        let layout_ids = ["GV-MAG-02", "GV-MAG-09", "GV-MAG-10"];
        for row in a_rows {
            if layout_ids.contains(&row.id) {
                assert!(row.ok, "{} got={} want={}", row.id, row.got, row.want);
            }
        }
    }

    /// Two synthetic `MAGIC.DEF` records for layout tests (no retail bytes):
    /// returns the parse plus the raw blob for the size/stride vectors.
    fn synthetic_records() -> (Vec<MagicItemTemplate>, Vec<u8>) {
        let mut blob = (2u32).to_le_bytes().to_vec();
        blob.extend_from_slice(&synthetic_record("Rec0", 0, 2, 0, (26, 0)));
        blob.extend_from_slice(&synthetic_record("Rec1", 0, 3, 0, (26, 1)));
        let records = parse_magic_def(&blob).expect("synthetic blob parses");
        (records, blob)
    }

    fn synthetic_record(name: &str, kind: u8, group: u8, group_index: u8, ench: (i8, i8)) -> [u8; MAGIC_RECORD_SIZE] {
        let mut rec = [0u8; MAGIC_RECORD_SIZE];
        let bytes = name.as_bytes();
        let n = bytes.len().min(32);
        rec[0..n].copy_from_slice(&bytes[..n]);
        rec[32] = kind;
        rec[33] = group;
        rec[34] = group_index;
        rec[35] = ench.0 as u8;
        rec[36] = ench.1 as u8;
        rec
    }
}
