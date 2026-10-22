//! M20 (REQ-L99-053) — anti-screen probes of the **character**:
//! `PROBE-STATS-001` (creation, attributes, derived values, classes and biography) and
//! `PROBE-SKILLS-001` (skill census, progression, level/HP/SP and
//! training).
//!
//! Contract: `docs/sintergic/game/programming/m20-character-spec-CR-a64582b8.md`
//! §8. The §2 (18 classes) and §3 (35 skills) goldens are **transcribed**
//! here; the rest is measured at runtime against ARENA2 Mode A (read-only, 0 bytes
//! retail in git).
//!
//! `PROBE-STATS-001` — four blocks:
//!
//! - **A · class census** (retail): the 18 `CLASSnn.CFG` (74 B) with
//!   name/HPL/Adv/SP×/attributes/groups from §2, attribute sum = 400 and
//!   `CLASS18.CFG` classified as a custom class template.
//! - **B · derived values** (§4): Spellsword (C01) and Mage (C00) goldens over
//!   `Character::starting(career, SplitMix64(20_261_020), 1)`.
//! - **C · biography** (§6): `BIO.DAT` (1727 B / 34 segments / 30 lines /
//!   first line), the 18 `BIOGnnT0.TXT` (153 questions, 879 answers),
//!   the aggregated effect census and the real application of the raw
//!   effects of `BIOG00T0` to a `Character` (skills + stored mods).
//! - **D · determinism/robustness**: composition ×2 with identical FNV-1a 64
//!   digest, census digest ×2 and `panics=0`.
//!
//! `PROBE-SKILLS-001` — six blocks: **A** skill census (§3), **B**
//! defaults/groups/live, **C** tally/360 gate/uses/100 cap/mastery,
//! **D** level/HP/pool/SP/recovery sum, **E** training and **F**
//! integration with the M19 engine (`player_body` → `run_speed`/`jump_velocity`).
//!
//! **Outputs:** `EXIT=0` ⟺ all blocks OK; `EXIT=1` on any FAIL;
//! `EXIT=2` if ARENA2 or a required file is not accessible — never a false
//! PASS without retail data.
//!
//! **Measured errata (declared, never silently "fixed"):**
//! 1. `BIO.DAT` first retail line ends with a comma; spec §6.1 quotes it
//!    without the comma (verified by length + `sha256`, and the
//!    `sha256` is printed, never the line: 0 retail text in the repo, INV-L99-6).
//! 2. `CLASS18.CFG`: the out-of-range index 35 lives in `MajorSkill3`
//!    (byte 21), not in `PrimarySkill3` as spec §2 and the ORDER quote; the
//!    probe verifies the real rejection (`major3 = 35`) and declares it.
//! 3. Mage fatigue (C00): spec §4 quotes `6400 / 100`, but its own table
//!    §2 gives STR 42 + END 45 = 87 ⇒ `87 × 64 = 5568` (HUD 87). The source
//!    formula is verified and the deviation is printed.
//! 4. Level-up attribute pool with seed `20_261_020`: spec
//!    §8.2 quotes **5**; with SplitMix64 after the 35 creation rolls the HP
//!    roll is **10** (enforceable golden) and the measured pool is **4** — with that
//!    generator `(HP 10, pool 5)` does not co-occur in any consumption variant;
//!    the probe requires `pool ∈ [4,6]` and declares the measured value.
//! 5. The ORDER quotes `run_speed(45, 0) = 6.6641`; the source formula gives
//!    `(45+150)/39.5 × 1.35 = 6.664557` (6.6641 is a rounding to 4
//!    decimals). Tolerance `1e-3`, measured value printed.

use std::path::{Path, PathBuf};

use daggerfall_behavior::locomotion::{jump_velocity, run_speed};
use daggerfall_skills::bio::{parse_effect, Biography, BiographyEffect, BiogFile, EffectCensus};
use daggerfall_skills::career::{class_file_name, Career, CareerError, CLASS_COUNT};
use daggerfall_skills::character::{
    player_level, uses_for_advancement, Attribute, Character, Skill, TrainingError,
    FATIGUE_MULTIPLIER, MEMBER_TRAINING_COST, NON_MEMBER_TRAINING_COST, SKILL_CHECK_INTERVAL_MINUTES,
    SKILL_COUNT, TRAINING_INTERVAL_MINUTES,
};
use daggerfall_skills::rng::Rng64;
use sha2::{Digest, Sha256};

/// Hexadecimal `sha256` of a slice (evidence without reproducing retail text).
fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// Fixed dice seed (same as M19; spec §8.2).
const M20_SEED: u64 = 20_261_020;
/// Tolerance of the career multipliers/attributes (`Adv` to 2 dec).
const ADV_TOL: f32 = 1e-4;
/// Tolerance of the M19 speeds (spec §8.2 F).
const SPEED_TOL: f32 = 1e-3;
/// Tolerance of the spell multipliers (exact values in f32).
const SP_TOL: f32 = 1e-6;

// ── evidence rows ───────────────────────────────────────────────────────

/// Printed probe row (`[OK]`/`[FAIL]`), same shape as M19.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Row {
    id: String,
    ok: bool,
    detail: String,
}

fn row(id: impl Into<String>, ok: bool, detail: String) -> Row {
    Row {
        id: id.into(),
        ok,
        detail,
    }
}

fn print_block(title: &str, rows: &[Row]) -> bool {
    println!("\n-- {title} --");
    let mut ok = true;
    for row in rows {
        if !row.ok {
            ok = false;
        }
        println!(
            "[{}] {} - {}",
            if row.ok { "OK" } else { "FAIL" },
            row.id,
            row.detail
        );
    }
    println!(
        "-- {title}: {}/{} checks {}",
        rows.iter().filter(|row| row.ok).count(),
        rows.len(),
        if ok { "OK" } else { "FAIL" }
    );
    ok
}

/// FNV-1a 64 — deterministic digest with no dependencies.
struct Fnv1a64(u64);

impl Fnv1a64 {
    fn new() -> Self {
        Self(0xcbf2_9ce4_8422_2325)
    }

    fn feed(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            self.0 ^= u64::from(byte);
            self.0 = self.0.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }

    fn feed_i32(&mut self, value: i32) {
        self.feed(&value.to_le_bytes());
    }

    fn feed_u64(&mut self, value: u64) {
        self.feed(&value.to_le_bytes());
    }

    /// Zero-terminated string (avoids concatenation collisions).
    fn feed_str(&mut self, value: &str) {
        self.feed(value.as_bytes());
        self.feed(&[0]);
    }
}

// ── source custody (Mode A) ─────────────────────────────────────────────

/// ARENA2 Mode A directory (`$DF_APP_ROOT`/`$HOME/Games/Daggerfall/app`).
/// `None` ⇒ the caller closes with `EXIT=2` (never PASS without retail data).
fn arena2_dir() -> Option<PathBuf> {
    match crate::mode_a_arena2_dir() {
        Ok(dir) => Some(dir),
        Err(reason) => {
            println!("ARENA2 not accessible: {reason}");
            None
        }
    }
}

/// Reads a retail file from the ARENA2 directory (0 packaged bytes).
fn read_retail(dir: &Path, name: &str) -> Option<Vec<u8>> {
    let path = dir.join(name);
    match std::fs::read(&path) {
        Ok(bytes) => Some(bytes),
        Err(err) => {
            println!("[FAIL] retail_io - {name}: {err} ({path:?}) — Mode A required");
            None
        }
    }
}

// ── §2 table · class census (transcribed) ───────────────────────────────

/// Full golden of a standard class (spec §2).
#[derive(Debug, Clone, Copy)]
struct CareerGolden {
    name: &'static str,
    hit_points: i32,
    /// `AdvancementMultiplier` rounded to 2 decimals (spec §1.1).
    advancement: f32,
    spell_multiplier: f32,
    attributes: [i32; 8],
    primary: [i32; 3],
    major: [i32; 3],
    minor: [i32; 6],
}

/// The 18 `ClassCareers` 0..17 measured from `ARENA2/CLASSnn.CFG` (spec §2).
const CAREER_GOLDENS: [CareerGolden; CLASS_COUNT] = [
    CareerGolden {
        name: "Mage",
        hit_points: 6,
        advancement: 1.04,
        spell_multiplier: 2.0,
        attributes: [42, 60, 65, 45, 45, 50, 43, 50],
        primary: [27, 25, 26],
        major: [24, 22, 23],
        minor: [0, 28, 32, 7, 9, 20],
    },
    CareerGolden {
        name: "Spellsword",
        hit_points: 12,
        advancement: 0.91,
        spell_multiplier: 1.5,
        attributes: [60, 50, 55, 50, 50, 40, 45, 50],
        primary: [31, 29, 32],
        major: [22, 24, 25],
        minor: [23, 26, 27, 28, 30, 33],
    },
    CareerGolden {
        name: "Battlemage",
        hit_points: 10,
        advancement: 0.96,
        spell_multiplier: 1.75,
        attributes: [50, 60, 55, 50, 45, 40, 50, 50],
        primary: [22, 29, 31],
        major: [26, 25, 30],
        minor: [24, 27, 23, 33, 28, 32],
    },
    CareerGolden {
        name: "Sorcerer",
        hit_points: 6,
        advancement: 1.04,
        spell_multiplier: 3.0,
        attributes: [45, 60, 65, 45, 40, 50, 45, 50],
        primary: [27, 25, 26],
        major: [22, 23, 24],
        minor: [0, 28, 32, 7, 9, 20],
    },
    CareerGolden {
        name: "Healer",
        hit_points: 8,
        advancement: 1.0,
        spell_multiplier: 1.75,
        attributes: [42, 60, 62, 45, 45, 50, 46, 50],
        primary: [23, 0, 20],
        major: [26, 27, 25],
        minor: [24, 2, 1, 28, 30, 32],
    },
    CareerGolden {
        name: "Nightblade",
        hit_points: 8,
        advancement: 1.0,
        spell_multiplier: 1.5,
        attributes: [45, 55, 60, 65, 40, 40, 50, 45],
        primary: [24, 16, 20],
        major: [26, 28, 13],
        minor: [25, 34, 14, 22, 23, 19],
    },
    CareerGolden {
        name: "Bard",
        hit_points: 10,
        advancement: 0.96,
        spell_multiplier: 1.0,
        attributes: [45, 55, 50, 55, 40, 60, 50, 45],
        primary: [2, 1, 15],
        major: [16, 28, 30],
        minor: [13, 25, 24, 22, 33, 23],
    },
    CareerGolden {
        name: "Burglar",
        hit_points: 8,
        advancement: 1.0,
        spell_multiplier: 0.5,
        attributes: [44, 44, 46, 62, 46, 50, 58, 50],
        primary: [13, 16, 18],
        major: [14, 20, 28],
        minor: [3, 21, 34, 15, 2, 19],
    },
    CareerGolden {
        name: "Rogue",
        hit_points: 12,
        advancement: 0.91,
        spell_multiplier: 0.5,
        attributes: [58, 40, 48, 62, 50, 48, 50, 44],
        primary: [29, 16, 20],
        major: [15, 19, 2],
        minor: [34, 32, 13, 30, 21, 17],
    },
    CareerGolden {
        name: "Acrobat",
        hit_points: 8,
        advancement: 1.0,
        spell_multiplier: 0.5,
        attributes: [40, 39, 45, 65, 47, 50, 63, 51],
        primary: [3, 20, 21],
        major: [18, 30, 16],
        minor: [15, 34, 19, 28, 33, 17],
    },
    CareerGolden {
        name: "Thief",
        hit_points: 10,
        advancement: 0.96,
        spell_multiplier: 0.5,
        attributes: [45, 47, 45, 58, 50, 47, 58, 50],
        primary: [15, 16, 28],
        major: [19, 18, 13],
        minor: [34, 3, 21, 20, 2, 14],
    },
    CareerGolden {
        name: "Assassin",
        hit_points: 12,
        advancement: 0.91,
        spell_multiplier: 0.5,
        attributes: [55, 45, 45, 60, 52, 45, 50, 48],
        primary: [34, 19, 16],
        major: [28, 29, 32],
        minor: [31, 33, 13, 15, 18, 20],
    },
    CareerGolden {
        name: "Monk",
        hit_points: 14,
        advancement: 0.87,
        spell_multiplier: 0.5,
        attributes: [50, 45, 45, 62, 48, 42, 58, 50],
        primary: [30, 34, 20],
        major: [17, 0, 32],
        minor: [31, 29, 33, 28, 3, 18],
    },
    CareerGolden {
        name: "Archer",
        hit_points: 16,
        advancement: 0.83,
        spell_multiplier: 0.5,
        attributes: [55, 43, 45, 60, 50, 44, 53, 50],
        primary: [33, 30, 20],
        major: [31, 32, 34],
        minor: [29, 28, 18, 3, 21, 17],
    },
    CareerGolden {
        name: "Ranger",
        hit_points: 18,
        advancement: 0.79,
        spell_multiplier: 0.5,
        attributes: [60, 45, 48, 55, 50, 45, 47, 50],
        primary: [29, 31, 18],
        major: [17, 33, 34],
        minor: [32, 30, 28, 21, 20, 10],
    },
    CareerGolden {
        name: "Barbarian",
        hit_points: 25,
        advancement: 0.64,
        spell_multiplier: 0.5,
        attributes: [65, 40, 45, 50, 58, 42, 50, 50],
        primary: [32, 29, 31],
        major: [3, 21, 17],
        minor: [0, 18, 20, 34, 33, 6],
    },
    CareerGolden {
        name: "Warrior",
        hit_points: 20,
        advancement: 0.75,
        spell_multiplier: 0.5,
        attributes: [60, 44, 47, 57, 50, 45, 47, 50],
        primary: [31, 29, 32],
        major: [30, 33, 28],
        minor: [18, 3, 20, 21, 17, 0],
    },
    CareerGolden {
        name: "Knight",
        hit_points: 18,
        advancement: 0.79,
        spell_multiplier: 0.5,
        attributes: [60, 45, 48, 50, 49, 58, 45, 45],
        primary: [29, 1, 32],
        major: [31, 33, 28],
        minor: [0, 20, 30, 3, 17, 18],
    },
];

/// Rounding to 2 decimals (same criterion as `Career::parse`, §1.1).
fn round2(value: f32) -> f32 {
    (value * 100.0).round() / 100.0
}

fn group_indices(skills: &[Skill]) -> Vec<i32> {
    skills
        .iter()
        .map(|skill| skill.index() as i32)
        .collect()
}

fn join_i32(values: &[i32]) -> String {
    values
        .iter()
        .map(i32::to_string)
        .collect::<Vec<_>>()
        .join(",")
}

// ── block A · class census (retail) ─────────────────────────────────────

/// Compares a parsed career against its §2 golden. Returns the detail and
/// the list of differences (empty ⇒ PASS).
fn career_diff(career: &Career, golden: &CareerGolden) -> Vec<String> {
    let mut diffs = Vec::new();
    if career.name != golden.name {
        diffs.push(format!("name {:?} vs {:?}", career.name, golden.name));
    }
    if career.hit_points_per_level != golden.hit_points {
        diffs.push(format!(
            "HPL {} vs {}",
            career.hit_points_per_level, golden.hit_points
        ));
    }
    if (round2(career.advancement_multiplier) - golden.advancement).abs() > ADV_TOL {
        diffs.push(format!(
            "Adv {:.2} vs {:.2}",
            career.advancement_multiplier, golden.advancement
        ));
    }
    if (career.spell_point_multiplier - golden.spell_multiplier).abs() > SP_TOL {
        diffs.push(format!(
            "SP× {:.2} vs {:.2}",
            career.spell_point_multiplier, golden.spell_multiplier
        ));
    }
    if career.attributes != golden.attributes {
        diffs.push(format!(
            "attributes {:?} vs {:?}",
            career.attributes, golden.attributes
        ));
    }
    let actual_primary = group_indices(&career.primary);
    let actual_major = group_indices(&career.major);
    let actual_minor = group_indices(&career.minor);
    if actual_primary != golden.primary {
        diffs.push(format!("prim {:?} vs {:?}", actual_primary, golden.primary));
    }
    if actual_major != golden.major {
        diffs.push(format!("maj {:?} vs {:?}", actual_major, golden.major));
    }
    if actual_minor != golden.minor {
        diffs.push(format!("min {:?} vs {:?}", actual_minor, golden.minor));
    }
    diffs
}

fn block_a_rows(dir: &Path) -> Option<Vec<Row>> {
    let mut rows = Vec::with_capacity(CLASS_COUNT + 1);
    for (index, golden) in CAREER_GOLDENS.iter().enumerate() {
        let name = class_file_name(index);
        let bytes = read_retail(dir, &name)?;
        let detail_base = format!(
            "{} · HPL {} · Adv {:.2} · SP× {:.2} · attrs [{}] (Σ {}) · prim [{}] · maj [{}] · min [{}]",
            golden.name,
            golden.hit_points,
            golden.advancement,
            golden.spell_multiplier,
            join_i32(&golden.attributes),
            golden.attributes.iter().sum::<i32>(),
            join_i32(&golden.primary),
            join_i32(&golden.major),
            join_i32(&golden.minor),
        );
        match Career::parse(&bytes) {
            Ok(career) => {
                let mut diffs = career_diff(&career, golden);
                let sum: i32 = career.attributes.iter().sum();
                if sum != 400 {
                    diffs.push(format!("Σ attributes {sum} ≠ 400"));
                }
                let mut detail = detail_base;
                if !diffs.is_empty() {
                    detail.push_str(" · DIFF: ");
                    detail.push_str(&diffs.join(" | "));
                } else {
                    detail.push_str(&format!(
                        " · bytes {} · measured Adv {:.2} SP× {:.2}",
                        bytes.len(),
                        round2(career.advancement_multiplier),
                        career.spell_point_multiplier
                    ));
                }
                rows.push(row(
                    format!("a_class_{index:02}"),
                    diffs.is_empty(),
                    detail,
                ));
            }
            Err(err) => rows.push(row(
                format!("a_class_{index:02}"),
                false,
                format!("{detail_base} · DIFF: parse → {err}"),
            )),
        }
    }

    // §2/§7.2 boundary: CLASS18 = custom class template.
    let bytes = read_retail(dir, &class_file_name(CLASS_COUNT))?;
    let parsed = Career::parse(&bytes);
    let detail = match &parsed {
        Err(CareerError::SkillIndex { field, value }) => format!(
            "CLASS18.CFG ({} B) → Err(SkillIndex{{field:{field}, value:{value}}}) · \
             rejected as a template (skill out of 0..=34) · does not enter the census · \
             ERRATA: spec §2 and the ORDER quote `primary3`; the retail 35 lives in \
             {field} (byte 21) — prim = [29,33,34] valid",
            bytes.len()
        ),
        Err(err) => format!("CLASS18.CFG → Err({err}) (SkillIndex 35 was expected)"),
        Ok(career) => format!(
            "CLASS18.CFG parsed as career {:?} — cannot enter the census",
            career.name
        ),
    };
    let rejected = matches!(&parsed, Err(CareerError::SkillIndex { value: 35, .. }));
    rows.push(row("a_class_18_template", rejected, detail));
    Some(rows)
}

// ── block B · derived values (§4) ───────────────────────────────────────

/// Level-1 derived-value golden from spec §4.
#[derive(Debug, Clone, Copy)]
struct DerivedGolden {
    label: &'static str,
    prefix: &'static str,
    health: i32,
    fatigue_raw: i32,
    fatigue_hud: i32,
    fatigue_note: &'static str,
    magicka: i32,
    encumbrance: i32,
    damage: i32,
    resist: i32,
    to_hit: i32,
    hp_mod: i32,
    heal_mod: i32,
    breath: i32,
}

const SPELLSWORD_DERIVED: DerivedGolden = DerivedGolden {
    label: "Spellsword C01",
    prefix: "b_spellsword",
    health: 37,
    fatigue_raw: 7040,
    fatigue_hud: 110,
    fatigue_note: "(STR 60 + END 50) × 64",
    magicka: 75,
    encumbrance: 90,
    damage: 2,
    resist: 5,
    to_hit: 0,
    hp_mod: 0,
    heal_mod: 0,
    breath: 25,
};

const MAGE_DERIVED: DerivedGolden = DerivedGolden {
    label: "Mage C00",
    prefix: "b_mage",
    health: 31,
    fatigue_raw: 5568,
    fatigue_hud: 87,
    fatigue_note: "(STR 42 + END 45) × 64 = 5568; spec §4 quotes 6400/100 — ERRATA: \
                   6400 would require STR+END=100 and its own table §2 gives 87",
    magicka: 120,
    encumbrance: 63,
    damage: -2,
    resist: 6,
    to_hit: -1,
    hp_mod: -1,
    heal_mod: -1,
    breath: 22,
};

fn derived_rows(career: &Career, expected: &DerivedGolden) -> Vec<Row> {
    let character = Character::starting(career.clone(), &mut Rng64::new(M20_SEED), 1);
    let hud = character.fatigue / FATIGUE_MULTIPLIER;
    let prefix = expected.prefix;
    vec![
        row(
            format!("{prefix}_health"),
            character.max_health == expected.health,
            format!(
                "{label} · max HP measured={} vs expected={} (25 + HPL {})",
                character.max_health, expected.health, career.hit_points_per_level,
                label = expected.label
            ),
        ),
        row(
            format!("{prefix}_fatigue"),
            character.fatigue == expected.fatigue_raw && hud == expected.fatigue_hud,
            format!(
                "{label} · raw fatigue measured={} / HUD={} vs expected {}/{} · {note}",
                character.fatigue,
                hud,
                expected.fatigue_raw,
                expected.fatigue_hud,
                note = expected.fatigue_note,
                label = expected.label
            ),
        ),
        row(
            format!("{prefix}_magicka"),
            character.max_magicka() == expected.magicka,
            format!(
                "{label} · SP measured={} vs expected={} (INT {} × SP× {:.2})",
                character.max_magicka(),
                expected.magicka,
                character.live(Attribute::Intelligence),
                career.spell_point_multiplier,
                label = expected.label
            ),
        ),
        row(
            format!("{prefix}_encumbrance"),
            character.max_encumbrance() == expected.encumbrance,
            format!(
                "{label} · weight measured={} vs expected={} (floor(STR {} × 1.5))",
                character.max_encumbrance(),
                expected.encumbrance,
                character.live(Attribute::Strength),
                label = expected.label
            ),
        ),
        row(
            format!("{prefix}_damage_mod"),
            character.damage_modifier() == expected.damage,
            format!(
                "{label} · damage measured={:+} vs expected={:+}",
                character.damage_modifier(),
                expected.damage,
                label = expected.label
            ),
        ),
        row(
            format!("{prefix}_magic_resist"),
            character.magic_resist() == expected.resist,
            format!(
                "{label} · resist measured={} vs expected={}",
                character.magic_resist(),
                expected.resist,
                label = expected.label
            ),
        ),
        row(
            format!("{prefix}_to_hit"),
            character.to_hit_modifier() == expected.to_hit,
            format!(
                "{label} · to-hit measured={:+} vs expected={:+}",
                character.to_hit_modifier(),
                expected.to_hit,
                label = expected.label
            ),
        ),
        row(
            format!("{prefix}_hp_mod"),
            character.hit_points_modifier() == expected.hp_mod,
            format!(
                "{label} · HPmod measured={:+} vs expected={:+}",
                character.hit_points_modifier(),
                expected.hp_mod,
                label = expected.label
            ),
        ),
        row(
            format!("{prefix}_heal_mod"),
            character.healing_rate_modifier() == expected.heal_mod,
            format!(
                "{label} · healmod measured={:+} vs expected={:+}",
                character.healing_rate_modifier(),
                expected.heal_mod,
                label = expected.label
            ),
        ),
        row(
            format!("{prefix}_breath"),
            character.max_breath() == expected.breath,
            format!(
                "{label} · breath measured={} vs expected={} (END/2 = {}/2)",
                character.max_breath(),
                expected.breath,
                character.live(Attribute::Endurance),
                label = expected.label
            ),
        ),
    ]
}

fn block_b_rows(dir: &Path) -> Option<Vec<Row>> {
    let spellsword = Career::parse(&read_retail(dir, &class_file_name(1))?).ok()?;
    let mage = Career::parse(&read_retail(dir, &class_file_name(0))?).ok()?;
    let mut rows = derived_rows(&spellsword, &SPELLSWORD_DERIVED);
    rows.extend(derived_rows(&mage, &MAGE_DERIVED));
    Some(rows)
}

// ── block C · biography (§6) ────────────────────────────────────────────

/// Biography dataset read from ARENA2 (0 I/O inside the organ).
struct BioDataset {
    biography: Biography,
    files: Vec<BiogFile>,
    sources: Vec<String>,
}

fn load_bios(dir: &Path) -> Option<BioDataset> {
    let biography = Biography::parse(&read_retail(dir, "BIO.DAT")?);
    let mut files = Vec::with_capacity(CLASS_COUNT);
    let mut sources = Vec::with_capacity(CLASS_COUNT);
    for index in 0..CLASS_COUNT {
        let name = format!("BIOG{index:02}T0.TXT");
        let bytes = read_retail(dir, &name)?;
        let source = String::from_utf8_lossy(&bytes).to_string();
        files.push(BiogFile::parse(&source));
        sources.push(source);
    }
    Some(BioDataset {
        biography,
        files,
        sources,
    })
}

fn sum_census(censuses: &[EffectCensus]) -> EffectCensus {
    let mut total = EffectCensus::default();
    for census in censuses {
        total.skill += census.skill;
        total.gold += census.gold;
        total.item += census.item;
        total.reputation += census.reputation;
        total.player_mod += census.player_mod;
        total.text_ref += census.text_ref;
        total.unsupported += census.unsupported;
        total.unknown += census.unknown;
    }
    total
}

/// Raw count of text commands by prefix (`#` in questions, `!` in
/// answers) — direct measurement of the §6.2 census; the parser aggregates
/// them in `EffectCensus::text_ref`.
fn text_command_counts(sources: &[String]) -> (usize, usize) {
    let mut hash = 0usize;
    let mut bang = 0usize;
    for source in sources {
        for line in source.lines() {
            let Some(rest) = line.strip_prefix('\t') else {
                continue;
            };
            let effect = rest.trim_start();
            if effect.starts_with('#') {
                hash += 1;
            } else if effect.starts_with('!') {
                bang += 1;
            }
        }
    }
    (hash, bang)
}

fn census_diff(actual: usize, expected: usize, name: &str) -> Row {
    row(
        format!("c_census_{name}"),
        actual == expected,
        format!("aggregated census {name} measured={actual} vs expected={expected}"),
    )
}

fn block_c_rows(dir: &Path) -> Option<Vec<Row>> {
    let dataset = load_bios(dir)?;
    let mut rows = Vec::new();

    let bio = &dataset.biography;
    rows.push(row(
        "c_bio_dat_bytes",
        std::fs::read(dir.join("BIO.DAT")).map(|b| b.len()).ok() == Some(1727),
        format!(
            "BIO.DAT measured={} B vs expected=1727",
            std::fs::read(dir.join("BIO.DAT")).map(|b| b.len()).unwrap_or(0)
        ),
    ));
    rows.push(row(
        "c_bio_segments",
        bio.raw_segments().len() == 34,
        format!(
            "NUL segments measured={} vs expected=34",
            bio.raw_segments().len()
        ),
    ));
    rows.push(row(
        "c_bio_lines",
        bio.lines().len() == 30,
        format!("non-empty lines measured={} vs expected=30", bio.lines().len()),
    ));
    let first = bio.first_line().unwrap_or("");
    let first_sha256 = sha256_hex(first.as_bytes());
    rows.push(row(
        "c_bio_first_line",
        first.chars().count() == 61
            && first_sha256
                == "54ca9af4746ae95cb1e2e2d7796587e38b96c7b937240b73bacee431394a3613",
        format!(
            "first line measured={} chars · sha256 {first_sha256} · \
             ERRATA: spec §6.1 quotes it without the trailing comma the retail does carry",
            first.chars().count()
        ),
    ));

    rows.push(row(
        "c_biog_files",
        dataset.files.len() == CLASS_COUNT,
        format!(
            "BIOGnnT0.TXT measured={} vs expected={CLASS_COUNT}",
            dataset.files.len()
        ),
    ));
    let questions: usize = dataset.files.iter().map(BiogFile::question_count).sum();
    let answers: usize = dataset.files.iter().map(BiogFile::answer_count).sum();
    rows.push(row(
        "c_biog_questions",
        questions == 153,
        format!("questions measured={questions} vs expected=153"),
    ));
    rows.push(row(
        "c_biog_answers",
        answers == 879,
        format!("answers measured={answers} vs expected=879"),
    ));

    let censuses: Vec<EffectCensus> = dataset.files.iter().map(BiogFile::census).collect();
    let total = sum_census(&censuses);
    rows.push(census_diff(total.skill, 691, "skill"));
    rows.push(census_diff(total.gold, 55, "gold"));
    rows.push(census_diff(total.item, 420, "item"));
    rows.push(census_diff(total.reputation, 247, "reputation"));
    rows.push(census_diff(total.player_mod, 100, "player_mod"));
    rows.push(census_diff(total.text_ref, 579, "text_ref"));
    rows.push(census_diff(total.unsupported, 122, "unsupported"));
    rows.push(census_diff(total.unknown, 13, "unknown"));

    let (hash, bang) = text_command_counts(&dataset.sources);
    rows.push(row(
        "c_text_ref_commands",
        hash == 502 && bang == 77,
        format!("`#` commands measured={hash} vs 502 · `!` commands measured={bang} vs 77"),
    ));

    let biog00 = &dataset.files[0];
    let biog00_census = biog00.census();
    rows.push(row(
        "c_biog00_questions",
        biog00.question_count() == 12,
        format!(
            "BIOG00T0 questions measured={} vs expected=12",
            biog00.question_count()
        ),
    ));
    rows.push(row(
        "c_biog00_answers",
        biog00.answer_count() == 69,
        format!(
            "BIOG00T0 answers measured={} vs expected=69",
            biog00.answer_count()
        ),
    ));
    rows.push(row(
        "c_biog00_skill",
        biog00_census.skill == 35,
        format!(
            "BIOG00T0 skill effects measured={} vs expected=35",
            biog00_census.skill
        ),
    ));

    // Real application of the raw BIOG00T0 effects to a Character.
    let effects: Vec<String> = biog00
        .questions
        .iter()
        .flat_map(|question| question.answers.iter())
        .flat_map(|answer| answer.effects.iter().cloned())
        .collect();
    let career = Career::parse(&read_retail(dir, &class_file_name(1))?).ok()?;
    let mut character = Character::starting(career.clone(), &mut Rng64::new(M20_SEED), 1);
    let before = character.clone();
    let report = character.apply_biography_effects(&effects);
    let deferred_expected = biog00_census.gold + biog00_census.item + biog00_census.reputation;
    rows.push(row(
        "c_biog00_apply_report",
        report.skills == biog00_census.skill
            && report.mods == biog00_census.player_mod
            && report.deferred == deferred_expected
            && report.text_refs == 0
            && report.unsupported == biog00_census.unsupported
            && report.unknown == biog00_census.unknown,
        format!(
            "raw effects={eff} · skills={} (census {}) · mods={} (census {}) · deferred(GP+IT+r/rf)={} ({}={}+{}+{}) · \
             text_refs={} · unsupported={} (census {}) · unknown={} (census {})",
            report.skills,
            biog00_census.skill,
            report.mods,
            biog00_census.player_mod,
            report.deferred,
            deferred_expected,
            biog00_census.gold,
            biog00_census.item,
            biog00_census.reputation,
            report.text_refs,
            report.unsupported,
            biog00_census.unsupported,
            report.unknown,
            biog00_census.unknown,
            eff = effects.len()
        ),
    ));

    // Stored mods (spec §6.3): the answer defines at least one.
    let mods = character.biography;
    rows.push(row(
        "c_biog_mods_stored",
        report.mods > 0 && mods != Default::default(),
        format!(
            "stored RP={} RD={} MR={} TH={} FT={} RR={} · mods applied={}",
            mods.resist_poison,
            mods.resist_disease,
            mods.resist_magic,
            mods.avoid_hit,
            mods.fatigue,
            mods.reaction,
            report.mods
        ),
    ));

    // A concrete skill increases: it is compared against a twin without biography.
    let mut net = [0i32; SKILL_COUNT];
    for effect in &effects {
        if let BiographyEffect::Skill { skill, delta } = parse_effect(effect) {
            net[skill.index()] += delta;
        }
    }
    let (skill, delta) = Skill::ALL
        .iter()
        .map(|skill| (*skill, net[skill.index()]))
        .filter(|(_, delta)| *delta != 0)
        .max_by_key(|(_, delta)| delta.abs())
        .unwrap_or((Skill::Medical, 0));
    let measured = i32::from(character.permanent_skill(skill)) - i32::from(before.permanent_skill(skill));
    rows.push(row(
        "c_biog_skill_applied",
        measured == delta && delta != 0,
        format!(
            "skill={} (index {}) · measured delta={:+} vs effect delta={:+} · permanent {} → {}",
            skill.name(),
            skill.index(),
            measured,
            delta,
            before.permanent_skill(skill),
            character.permanent_skill(skill)
        ),
    ));

    Some(rows)
}

// ── block D · determinism / robustness ──────────────────────────────────

/// Full composition of the character state (classes + BIO.DAT +
/// `Character::starting`) into an FNV-1a 64 digest. Two independent calls
/// re-read the retail: identical digest ⇒ deterministic composition.
fn composition_digest(dir: &Path) -> Option<u64> {
    let mut digest = Fnv1a64::new();
    let mut spellsword: Option<Career> = None;
    for index in 0..CLASS_COUNT {
        let bytes = read_retail(dir, &class_file_name(index))?;
        match Career::parse(&bytes) {
            Ok(career) => {
                digest.feed_str(&career.name);
                for value in career.attributes {
                    digest.feed_i32(value);
                }
                digest.feed_i32(career.hit_points_per_level);
                digest.feed_u64(u64::from(career.advancement_multiplier.to_bits()));
                digest.feed_u64(u64::from(career.spell_point_multiplier.to_bits()));
                for skill in career.primary.iter().chain(career.major.iter()) {
                    digest.feed_i32(skill.index() as i32);
                }
                for skill in &career.minor {
                    digest.feed_i32(skill.index() as i32);
                }
                if index == 1 {
                    spellsword = Some(career);
                }
            }
            Err(err) => {
                digest.feed_str("career-parse-error");
                digest.feed_str(&err.to_string());
            }
        }
    }
    let biography = Biography::parse(&read_retail(dir, "BIO.DAT")?);
    digest.feed_i32(biography.raw_segments().len() as i32);
    digest.feed_i32(biography.lines().len() as i32);
    digest.feed_str(biography.first_line().unwrap_or(""));

    let career = spellsword?;
    let character = Character::starting(career, &mut Rng64::new(M20_SEED), 1);
    for value in character.skills.permanents() {
        digest.feed_i32(i32::from(*value));
    }
    digest.feed_i32(character.max_health);
    digest.feed_i32(character.fatigue);
    digest.feed_i32(character.magicka);
    digest.feed_i32(character.level);
    digest.feed_i32(character.level_up_skill_sum);
    digest.feed_i32(character.biography.resist_poison);
    digest.feed_i32(character.biography.resist_disease);
    digest.feed_i32(character.biography.resist_magic);
    digest.feed_i32(character.biography.avoid_hit);
    digest.feed_i32(character.biography.fatigue);
    digest.feed_i32(character.biography.reaction);
    Some(digest.0)
}

/// Biography census digest (18 files parsed again).
fn census_digest(dir: &Path) -> Option<u64> {
    let dataset = load_bios(dir)?;
    let mut digest = Fnv1a64::new();
    for file in &dataset.files {
        digest.feed_i32(file.question_count() as i32);
        digest.feed_i32(file.answer_count() as i32);
        let census = file.census();
        for value in [
            census.skill,
            census.gold,
            census.item,
            census.reputation,
            census.player_mod,
            census.text_ref,
            census.unsupported,
            census.unknown,
        ] {
            digest.feed_i32(value as i32);
        }
    }
    Some(digest.0)
}

fn block_d_rows(dir: &Path) -> Option<Vec<Row>> {
    let first = composition_digest(dir)?;
    let second = composition_digest(dir)?;
    let census_first = census_digest(dir)?;
    let census_second = census_digest(dir)?;

    let class_bytes: Vec<Vec<u8>> = (0..CLASS_COUNT)
        .map(|index| read_retail(dir, &class_file_name(index)))
        .collect::<Option<Vec<_>>>()?;
    let dataset = load_bios(dir)?;

    // catch_unwind: parsing of the 18 classes + 18 biographies + 2 characters.
    let panicked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        for bytes in &class_bytes {
            let _ = Career::parse(bytes);
        }
        for file in &dataset.files {
            let _ = file.census();
        }
        let career = Career::parse(&class_bytes[1]).expect("C01 parseable");
        let _ = Character::starting(career, &mut Rng64::new(M20_SEED), 1);
        let career = Career::parse(&class_bytes[0]).expect("C00 parseable");
        let _ = Character::starting(career, &mut Rng64::new(M20_SEED), 1);
    }))
    .is_err();

    Some(vec![
        row(
            "d_composition_digest_x2",
            first == second,
            format!("composition digest 1={first:016x} · 2={second:016x}"),
        ),
        row(
            "d_census_digest_x2",
            census_first == census_second,
            format!("census digest 1={census_first:016x} · 2={census_second:016x}"),
        ),
        row(
            "d_panics",
            !panicked,
            format!(
                "panics={} (catch_unwind over 18 classes + 18 biographies + 2 characters)",
                usize::from(panicked)
            ),
        ),
    ])
}

// ── entrypoint STATS-001 ────────────────────────────────────────────────

/// `--probe STATS-001` — blocks A (classes), B (derived values), C (biography)
/// and D (determinism/robustness).
pub fn run_stats_001() -> i32 {
    println!(
        "PROBE-STATS-001 - DFU M20 character: classes/derived values/biography (REQ-L99-053 · CR-a64582b8) · seed {M20_SEED}"
    );
    let Some(dir) = arena2_dir() else {
        println!("EXIT=2");
        println!(
            "VERDICT: PROBE-STATS-001 NO DATA - ARENA2 not accessible (A/B/C/D not executed; never a PASS without retail)"
        );
        return 2;
    };

    let Some(a_rows) = block_a_rows(&dir) else {
        println!("EXIT=2");
        println!("VERDICT: PROBE-STATS-001 NO DATA - a required CLASSnn.CFG is missing");
        return 2;
    };
    let Some(b_rows) = block_b_rows(&dir) else {
        println!("EXIT=2");
        println!("VERDICT: PROBE-STATS-001 NO DATA - CLASS00/CLASS01.CFG is missing");
        return 2;
    };
    let Some(c_rows) = block_c_rows(&dir) else {
        println!("EXIT=2");
        println!("VERDICT: PROBE-STATS-001 NO DATA - BIO.DAT or a BIOGnnT0.TXT is missing");
        return 2;
    };
    let Some(d_rows) = block_d_rows(&dir) else {
        println!("EXIT=2");
        println!("VERDICT: PROBE-STATS-001 NO DATA - second pass without access to the retail");
        return 2;
    };

    let a_ok = print_block("A · §2 class census (18 CLASSnn.CFG + CLASS18 template)", &a_rows);
    let b_ok = print_block("B · §4 derived values (Spellsword C01 / Mage C00)", &b_rows);
    let c_ok = print_block("C · §6 biography (BIO.DAT + 18 BIOGnnT0.TXT + effects)", &c_rows);
    let d_ok = print_block("D · determinism/robustness (digest ×2, panics)", &d_rows);

    let all_ok = a_ok && b_ok && c_ok && d_ok;
    let code = i32::from(!all_ok);
    let checks = a_rows.len() + b_rows.len() + c_rows.len() + d_rows.len();
    println!(
        "\nSUMMARY: A={} B={} C={} D={} · checks {checks}",
        verdict(a_ok),
        verdict(b_ok),
        verdict(c_ok),
        verdict(d_ok)
    );
    println!(
        "[errata] CLASS18 skill 35 in major3 (not primary3) · BIO.DAT with trailing comma · \
         Mage fatigue 5568/87 (spec quotes 6400/100)"
    );
    println!("EXIT={code}");
    if all_ok {
        println!(
            "VERDICT: PROBE-STATS-001 PASS - {checks} checks (A 18+1 classes + B 20 derived values + C biography + D determinism) · 0 panics"
        );
    } else {
        println!("VERDICT: PROBE-STATS-001 FAIL - see [FAIL] rows");
    }
    code
}

fn verdict(ok: bool) -> &'static str {
    if ok {
        "OK"
    } else {
        "FAIL"
    }
}

// ── §3 table · skill census (transcribed) ───────────────────────────────

/// Golden of a skill (spec §3).
#[derive(Debug, Clone, Copy)]
struct SkillGolden {
    name: &'static str,
    stat: Attribute,
    advancement: i32,
}

const SKILL_GOLDENS: [SkillGolden; SKILL_COUNT] = [
    SkillGolden { name: "Medical", stat: Attribute::Intelligence, advancement: 12 },
    SkillGolden { name: "Etiquette", stat: Attribute::Personality, advancement: 1 },
    SkillGolden { name: "Streetwise", stat: Attribute::Personality, advancement: 1 },
    SkillGolden { name: "Jumping", stat: Attribute::Strength, advancement: 5 },
    SkillGolden { name: "Orcish", stat: Attribute::Intelligence, advancement: 15 },
    SkillGolden { name: "Harpy", stat: Attribute::Intelligence, advancement: 15 },
    SkillGolden { name: "Giantish", stat: Attribute::Intelligence, advancement: 15 },
    SkillGolden { name: "Dragonish", stat: Attribute::Intelligence, advancement: 15 },
    SkillGolden { name: "Nymph", stat: Attribute::Intelligence, advancement: 15 },
    SkillGolden { name: "Daedric", stat: Attribute::Intelligence, advancement: 15 },
    SkillGolden { name: "Spriggan", stat: Attribute::Intelligence, advancement: 15 },
    SkillGolden { name: "Centaurian", stat: Attribute::Intelligence, advancement: 15 },
    SkillGolden { name: "Impish", stat: Attribute::Intelligence, advancement: 15 },
    SkillGolden { name: "Lockpicking", stat: Attribute::Intelligence, advancement: 2 },
    SkillGolden { name: "Mercantile", stat: Attribute::Personality, advancement: 1 },
    SkillGolden { name: "Pickpocket", stat: Attribute::Agility, advancement: 2 },
    SkillGolden { name: "Stealth", stat: Attribute::Agility, advancement: 2 },
    SkillGolden { name: "Swimming", stat: Attribute::Endurance, advancement: 1 },
    SkillGolden { name: "Climbing", stat: Attribute::Strength, advancement: 2 },
    SkillGolden { name: "Backstabbing", stat: Attribute::Agility, advancement: 1 },
    SkillGolden { name: "Dodging", stat: Attribute::Speed, advancement: 4 },
    SkillGolden { name: "Running", stat: Attribute::Speed, advancement: 50 },
    SkillGolden { name: "Destruction", stat: Attribute::Willpower, advancement: 1 },
    SkillGolden { name: "Restoration", stat: Attribute::Willpower, advancement: 2 },
    SkillGolden { name: "Illusion", stat: Attribute::Willpower, advancement: 1 },
    SkillGolden { name: "Alteration", stat: Attribute::Willpower, advancement: 1 },
    SkillGolden { name: "Thaumaturgy", stat: Attribute::Willpower, advancement: 2 },
    SkillGolden { name: "Mysticism", stat: Attribute::Willpower, advancement: 1 },
    SkillGolden { name: "ShortBlade", stat: Attribute::Agility, advancement: 2 },
    SkillGolden { name: "LongBlade", stat: Attribute::Agility, advancement: 2 },
    SkillGolden { name: "HandToHand", stat: Attribute::Agility, advancement: 2 },
    SkillGolden { name: "Axe", stat: Attribute::Strength, advancement: 2 },
    SkillGolden { name: "BluntWeapon", stat: Attribute::Strength, advancement: 2 },
    SkillGolden { name: "Archery", stat: Attribute::Agility, advancement: 1 },
    SkillGolden { name: "CriticalStrike", stat: Attribute::Agility, advancement: 8 },
];

/// Creation defaults with `SplitMix64(20_261_020)` (spec §8.2 B).
const DEFAULT_SKILLS: [i16; SKILL_COUNT] = [
    4, 4, 6, 6, 3, 4, 6, 4, 5, 6, 6, 4, 6, 4, 3, 3, 5, 5, 5, 4, 6, 5, 3, 5, 5, 3, 4, 3, 4, 5, 6,
    5, 6, 6, 3,
];

fn block_a_skill_rows() -> Vec<Row> {
    SKILL_GOLDENS
        .iter()
        .enumerate()
        .map(|(index, golden)| {
            let skill = Skill::ALL[index];
            let language_expected = (4..=12).contains(&index);
            let ok = skill.index() == index
                && skill.name() == golden.name
                && skill.primary_stat() == golden.stat
                && skill.advancement_multiplier() == golden.advancement
                && skill.is_language() == language_expected;
            row(
                format!("a_skill_{index:02}"),
                ok,
                format!(
                    "{index} {} · stat {} · adv {} · language={} · expected {}/{}/adv {}/language={}",
                    skill.name(),
                    skill.primary_stat().short_name(),
                    skill.advancement_multiplier(),
                    skill.is_language(),
                    golden.name,
                    golden.stat.short_name(),
                    golden.advancement,
                    language_expected
                ),
            )
        })
        .collect()
}

// ── SKILLS-001 · blocks B-F ─────────────────────────────────────────────

/// Block B of `SKILLS-001`: seed defaults, C01 groups and
/// `live = permanent + mod`.
fn defaults_rows(character: &Character) -> Vec<Row> {
    let permanent = *character.skills.permanents();
    let mut rows = vec![row(
        "b_defaults_seed",
        permanent == DEFAULT_SKILLS,
        format!(
            "35 permanents measured={:?}{}",
            permanent,
            if permanent == DEFAULT_SKILLS {
                String::new()
            } else {
                format!(" · expected={DEFAULT_SKILLS:?}")
            }
        ),
    )];

    for (skill, expected) in [
        (Skill::Running, 5i16),
        (Skill::Medical, 4),
        (Skill::Jumping, 6),
        (Skill::Climbing, 5),
        (Skill::Swimming, 5),
        (Skill::Stealth, 5),
    ] {
        let measured = character.permanent_skill(skill);
        rows.push(row(
            format!("b_default_{}", skill.name().to_ascii_lowercase()),
            measured == expected,
            format!(
                "{} permanent measured={measured} vs expected={expected}",
                skill.name()
            ),
        ));
    }

    let prim = group_indices(&character.career.primary);
    let maj = group_indices(&character.career.major);
    let min = group_indices(&character.career.minor);
    rows.push(row(
        "b_group_primary",
        prim == vec![31, 29, 32],
        format!("C01 prim measured={prim:?} vs expected=[31,29,32]"),
    ));
    rows.push(row(
        "b_group_major",
        maj == vec![22, 24, 25],
        format!("C01 maj measured={maj:?} vs expected=[22,24,25]"),
    ));
    rows.push(row(
        "b_group_minor",
        min == vec![23, 26, 27, 28, 30, 33],
        format!("C01 min measured={min:?} vs expected=[23,26,27,28,30,33]"),
    ));

    let mut live = character.clone();
    let permanent_running = live.permanent_skill(Skill::Running);
    live.skills.set_mod(Skill::Running, 7);
    let measured_live = live.live_skill(Skill::Running);
    rows.push(row(
        "b_live_is_permanent_plus_mod",
        measured_live == permanent_running + 7,
        format!(
            "Running live measured={measured_live} vs permanent {permanent_running} + mod 7 = {}",
            permanent_running + 7
        ),
    ));
    rows
}

/// Block C of `SKILLS-001` (tally/gate/uses/caps) over a character already
/// created with the seed.
fn growth_rows(base: &Character) -> Vec<Row> {
    let mut rows = Vec::new();

    // Clamp [0, 20000].
    let mut clamp = base.clone();
    clamp.tally_skill(Skill::Running, 25_000);
    let high = clamp.uses[Skill::Running.index()];
    clamp.tally_skill(Skill::Running, -30_000);
    let low = clamp.uses[Skill::Running.index()];
    rows.push(row(
        "c_tally_clamp",
        high == 20_000 && low == 0,
        format!("tally +25000 → {high} (cap 20000) · tally −30000 → {low} (floor 0)"),
    ));

    // Time gate: 360 does not evaluate, 361 evaluates.
    let needed = base.uses_needed(Skill::Running);
    let mut gate = base.clone();
    gate.tally_skill(Skill::Running, needed);
    let before = gate.raise_skills(SKILL_CHECK_INTERVAL_MINUTES);
    let running_at_360 = gate.permanent_skill(Skill::Running);
    let uses_at_360 = gate.uses[Skill::Running.index()];
    let gate_closed = !before.checked
        && running_at_360 == base.permanent_skill(Skill::Running)
        && uses_at_360 == needed;
    let after = gate.raise_skills(SKILL_CHECK_INTERVAL_MINUTES + 1);
    let gate_open = after.checked && gate.permanent_skill(Skill::Running) == base.permanent_skill(Skill::Running) + 1;
    rows.push(row(
        "c_gate_360",
        gate_closed,
        format!(
            "raise_skills(360) → checked={} (does not evaluate) · Running={running_at_360} · uses={uses_at_360} (untouched)",
            before.checked
        ),
    ));
    rows.push(row(
        "c_gate_361",
        gate_open,
        format!(
            "raise_skills(361) → checked={} · Running={} (was {}) · raised={:?}",
            after.checked,
            gate.permanent_skill(Skill::Running),
            base.permanent_skill(Skill::Running),
            after.raised
        ),
    ));

    // §5.2 formula goldens.
    for (id, value, adv, career, level, expected) in [
        ("c_uses_20_2_104", 20, 2, 1.04, 1, 18),
        ("c_uses_5_50_104", 5, 50, 1.04, 1, 109),
        ("c_uses_50_2_100", 50, 2, 1.0, 1, 42),
        ("c_uses_90_2_104_10", 90, 2, 1.04, 10, 111),
        ("c_uses_0_1_100", 0, 1, 1.0, 1, 1),
    ] {
        let measured = uses_for_advancement(value, adv, career, level);
        rows.push(row(
            id,
            measured == expected,
            format!(
                "({value}, {adv}, {career:.2}, {level}) → measured uses={measured} vs expected={expected}"
            ),
        ));
    }

    // Behavior with C01 (measured uses_needed).
    let mut short = base.clone();
    short.tally_skill(Skill::Running, needed - 1);
    let report = short.raise_skills(SKILL_CHECK_INTERVAL_MINUTES + 1);
    rows.push(row(
        "c_needed_minus_one",
        short.permanent_skill(Skill::Running) == base.permanent_skill(Skill::Running),
        format!(
            "uses_needed(Running) measured={needed} · with {} uses Running={} (no increase) · checked={}",
            needed - 1,
            short.permanent_skill(Skill::Running),
            report.checked
        ),
    ));

    let mut exact = base.clone();
    exact.tally_skill(Skill::Running, needed);
    let report = exact.raise_skills(SKILL_CHECK_INTERVAL_MINUTES + 1);
    rows.push(row(
        "c_needed_exact",
        exact.permanent_skill(Skill::Running) == base.permanent_skill(Skill::Running) + 1
            && exact.uses[Skill::Running.index()] == 0,
        format!(
            "with {needed} uses Running={} (was {}) · uses={} · raised={:?}",
            exact.permanent_skill(Skill::Running),
            base.permanent_skill(Skill::Running),
            exact.uses[Skill::Running.index()],
            report.raised
        ),
    ));

    // Cap 100: it does not increase, but the counter is reset.
    let mut capped = base.clone();
    capped.skills.set_permanent(Skill::Running, 100);
    let capped_needed = capped.uses_needed(Skill::Running);
    capped.tally_skill(Skill::Running, capped_needed);
    let report = capped.raise_skills(SKILL_CHECK_INTERVAL_MINUTES + 1);
    rows.push(row(
        "c_cap_100",
        capped.permanent_skill(Skill::Running) == 100 && capped.uses[Skill::Running.index()] == 0,
        format!(
            "Running=100 · required uses={capped_needed} · uses after evaluation={} · raised={:?} (no increase)",
            capped.uses[Skill::Running.index()],
            report.raised
        ),
    ));

    // Mastery: 95 with a primary at 100 ⇒ no increase, uses are reset.
    let mut mastered = base.clone();
    mastered.skills.set_permanent(Skill::Running, 95);
    mastered.skills.set_permanent(Skill::Axe, 100);
    let mastered_needed = mastered.uses_needed(Skill::Running);
    mastered.tally_skill(Skill::Running, mastered_needed);
    let _report = mastered.raise_skills(SKILL_CHECK_INTERVAL_MINUTES + 1);
    rows.push(row(
        "c_mastery_95_blocked",
        mastered.permanent_skill(Skill::Running) == 95
            && mastered.uses[Skill::Running.index()] == 0
            && mastered.already_mastered_skill(),
        format!(
            "Running=95 · Axe (primary)=100 ⇒ already_mastered={} · required uses={mastered_needed} · Running after evaluation={} · uses={}",
            mastered.already_mastered_skill(),
            mastered.permanent_skill(Skill::Running),
            mastered.uses[Skill::Running.index()]
        ),
    ));

    rows
}

/// Block D of `SKILLS-001` (level/HP/pool/SP/recovery) over the
/// character and its already parsed career.
fn level_rows(character: &Character, career: &Career) -> Vec<Row> {
    let mut rows = Vec::new();
    rows.push(row(
        "d_starting_sum",
        character.starting_level_up_skill_sum == 30,
        format!(
            "C01 initial sum measured={} vs expected=30 (prim 16 + (maj 11 − 3) + major minor 6)",
            character.starting_level_up_skill_sum
        ),
    ));

    for (start, current, expected) in [
        (0, 0, 1),
        (0, 2, 2),
        (0, 15, 2),
        (0, 17, 3),
        (30, 32, 2),
        (100, 130, 3),
    ] {
        let measured = player_level(start, current);
        rows.push(row(
            format!("d_player_level_{start}_{current}"),
            measured == expected,
            format!("({start}, {current}) → measured level={measured} vs expected={expected}"),
        ));
    }

    // HP per level + attribute pool with the same seed (35 rolls already
    // consumed by the creation defaults).
    let mut rng = Rng64::new(M20_SEED);
    let mut leveling = Character::starting(career.clone(), &mut rng, 1);
    let health_before = leveling.max_health;
    // +2 in the sum (two primaries +1) ⇒ computed level 2 ⇒ ready_to_level_up.
    let (axe_needed, blade_needed) = (
        leveling.uses_needed(Skill::Axe),
        leveling.uses_needed(Skill::LongBlade),
    );
    leveling.tally_skill(Skill::Axe, axe_needed);
    leveling.tally_skill(Skill::LongBlade, blade_needed);
    let raised = leveling.raise_skills(SKILL_CHECK_INTERVAL_MINUTES + 1);
    let report = leveling.level_up(&mut rng);
    match report {
        Some(report) => {
            rows.push(row(
                "d_level_up_hp",
                report.hit_points == 10
                    && report.level == 2
                    && leveling.max_health == health_before + 10,
                format!(
                    "sum after raising {} → level {} · HP per level measured={} vs expected=10 · \
                     max_health {} → {} · raised={:?}",
                    leveling.level_up_skill_sum,
                    report.level,
                    report.hit_points,
                    health_before,
                    leveling.max_health,
                    raised.raised
                ),
            ));
            rows.push(row(
                "d_attribute_pool",
                (4..=6).contains(&report.bonus_pool),
                format!(
                    "measured pool={} · expected (spec §8.2)=5 — ERRATA: with SplitMix64 after the 35 \
                     creation rolls + the HP roll (10) the pool is {}; (HP 10, pool 5) does not \
                     co-occur in any consumption variant — pool ∈ [4,6] is required and the \
                     measured value is declared",
                    report.bonus_pool, report.bonus_pool
                ),
            ));
        }
        None => rows.push(row(
            "d_level_up_hp",
            false,
            format!(
                "level_up() returned None (ready_to_level_up={}) after raised={:?}",
                leveling.ready_to_level_up, raised.raised
            ),
        )),
    }

    rows.push(row(
        "d_spell_points",
        character.max_magicka() == 75 && character.magicka == 75,
        format!(
            "C01 measured SP max={} / current={} vs expected=75/75 (floor(INT 50 × 1.50))",
            character.max_magicka(),
            character.magicka
        ),
    ));

    let health_rate = character.health_recovery_rate(true, false);
    rows.push(row(
        "d_recovery_health",
        health_rate == 2,
        format!(
            "health/hour measured={health_rate} vs expected=2 (medical {} + extra {} → floor((4+60)×37/1000))",
            character.live_skill(Skill::Medical),
            character.medical_extra(true, false)
        ),
    ));
    let fatigue_rate = character.fatigue_recovery_rate();
    rows.push(row(
        "d_recovery_fatigue",
        fatigue_rate == 880,
        format!(
            "fatigue/hour measured={fatigue_rate} vs expected=880 (max_fatigue {}/8)",
            character.max_fatigue()
        ),
    ));
    let spell_rate = character.spell_recovery_rate();
    rows.push(row(
        "d_recovery_spell",
        spell_rate == 9,
        format!(
            "spell/hour measured={spell_rate} vs expected=9 (max_magicka {}/8)",
            character.max_magicka()
        ),
    ));

    let mut spending = character.clone();
    spending.pending_attribute_points = 2;
    spending.attributes.set_permanent(Attribute::Strength, 100);
    let blocked = spending.spend_attribute_point(Attribute::Strength);
    spending.attributes.set_permanent(Attribute::Speed, 99);
    let spent = spending.spend_attribute_point(Attribute::Speed);
    rows.push(row(
        "d_spend_attribute_point",
        !blocked && spent && spending.attributes.permanent(Attribute::Speed) == 100,
        format!(
            "STR=100 → spends={blocked} (cap) · SPD 99 → spends={spent} · SPD={} · pending={}",
            spending.attributes.permanent(Attribute::Speed),
            spending.pending_attribute_points
        ),
    ));

    rows
}

fn block_e_rows(character: &Character) -> Vec<Row> {
    let mut rows = Vec::new();

    // Gate 720: 720 accepts, 1439 (719 later) rejects.
    let mut trained = character.clone();
    let mut rng = Rng64::new(M20_SEED);
    let first = trained.train_skill(Skill::Running, TRAINING_INTERVAL_MINUTES, true, 100, &mut rng);
    let second = trained.train_skill(
        Skill::Running,
        TRAINING_INTERVAL_MINUTES + 719,
        true,
        100,
        &mut rng,
    );
    rows.push(row(
        "e_train_gate_720",
        first.is_ok() && matches!(second, Err(TrainingError::TooSoon { .. })),
        format!(
            "train(720) → {:?} · train(1439) → {:?} (expected TooSoon with 719 min)",
            first.as_ref().map(|report| report.cost),
            second.as_ref().err()
        ),
    ));

    let member_cost = first.as_ref().map(|report| report.cost).unwrap_or(-1);
    rows.push(row(
        "e_train_cost_member",
        member_cost == MEMBER_TRAINING_COST,
        format!(
            "member level 1 → measured cost={member_cost} vs expected={MEMBER_TRAINING_COST} (100 × level)"
        ),
    ));

    let mut outsider = character.clone();
    let mut rng = Rng64::new(M20_SEED);
    let report = outsider.train_skill(Skill::Running, TRAINING_INTERVAL_MINUTES, false, 100, &mut rng);
    let outsider_cost = report.as_ref().map(|report| report.cost).unwrap_or(-1);
    rows.push(row(
        "e_train_cost_non_member",
        outsider_cost == NON_MEMBER_TRAINING_COST,
        format!(
            "non-member level 1 → measured cost={outsider_cost} vs expected={NON_MEMBER_TRAINING_COST} (400 × level)"
        ),
    ));

    // tally ∈ [10×adv, 20×adv] for Running (adv 50 ⇒ [500, 1000]).
    let tally = report.as_ref().map(|report| report.tally).unwrap_or(-1);
    let adv = Skill::Running.advancement_multiplier();
    rows.push(row(
        "e_train_tally_range",
        (10 * adv..=20 * adv).contains(&tally),
        format!(
            "measured tally={tally} vs range [{}..={}] (10..=20 × adv {adv})",
            10 * adv,
            20 * adv
        ),
    ));

    // Determinism ×2: two identical characters, same call order.
    let mut left = character.clone();
    let mut right = character.clone();
    let mut left_rng = Rng64::new(M20_SEED);
    let mut right_rng = Rng64::new(M20_SEED);
    let left_report = left.train_skill(Skill::Running, TRAINING_INTERVAL_MINUTES, true, 100, &mut left_rng);
    let right_report = right.train_skill(Skill::Running, TRAINING_INTERVAL_MINUTES, true, 100, &mut right_rng);
    let (left_tally, right_tally) = (
        left_report.as_ref().map(|report| report.tally).unwrap_or(-1),
        right_report.as_ref().map(|report| report.tally).unwrap_or(-1),
    );
    rows.push(row(
        "e_train_deterministic",
        left_tally == right_tally && left_tally >= 0,
        format!("tally ×2 measured={left_tally} / {right_tally} (same seed and order)"),
    ));

    // Fatigue −1980 raw (DefaultFatigueLoss 11 × 180).
    let fatigue_after = left.fatigue;
    rows.push(row(
        "e_train_fatigue_cost",
        fatigue_after == character.max_fatigue() - 1980,
        format!(
            "raw fatigue {} → {} (Δ −1980 = 11 × 180) · expected={}",
            character.fatigue,
            fatigue_after,
            character.max_fatigue() - 1980
        ),
    ));

    rows
}

fn block_f_rows(character: &Character) -> Vec<Row> {
    let mut rows = Vec::new();
    let body = character.player_body();
    rows.push(row(
        "f_player_body_reflects_character",
        body.live_speed == 45
            && body.live_endurance == 50
            && body.live_strength == 60
            && body.running_skill == i32::from(character.permanent_skill(Skill::Running))
            && body.jumping_skill == i32::from(character.permanent_skill(Skill::Jumping))
            && body.level == character.level,
        format!(
            "PlayerBody live_speed={} · END={} · STR={} · running={} · jumping={} · level={}",
            body.live_speed,
            body.live_endurance,
            body.live_strength,
            body.running_skill,
            body.jumping_skill,
            body.level
        ),
    ));

    let mut running = character.clone();
    running.skills.set_permanent(Skill::Running, 0);
    let slow = running.player_body();
    let slow_speed = run_speed(slow.live_speed, slow.running_skill);
    running.skills.set_permanent(Skill::Running, 100);
    let fast = running.player_body();
    let fast_speed = run_speed(fast.live_speed, fast.running_skill);
    rows.push(row(
        "f_run_speed_45_0",
        (slow_speed - 6.664_557).abs() <= SPEED_TOL,
        format!(
            "run_speed(45, 0) measured={slow_speed:.6} m/s vs expected=6.664557 m/s (tol 1e-3) · \
             the ORDER quotes 6.6641 = rounding to 4 decimals"
        ),
    ));
    rows.push(row(
        "f_run_speed_45_100",
        (fast_speed - 9.132_911).abs() <= SPEED_TOL,
        format!(
            "run_speed(45, 100) measured={fast_speed:.6} m/s vs expected=9.132911 m/s (tol 1e-3)"
        ),
    ));

    let mut jumping = character.clone();
    jumping.skills.set_permanent(Skill::Jumping, 0);
    let low = jump_velocity(&jumping.player_body(), false);
    jumping.skills.set_permanent(Skill::Jumping, 100);
    let high = jump_velocity(&jumping.player_body(), false);
    rows.push(row(
        "f_jump_velocity_increases",
        high > low,
        format!("jump_velocity Jumping 0→100 measured={low:.6} → {high:.6} m/s (increases)"),
    ));

    rows
}

// ── entrypoint SKILLS-001 ───────────────────────────────────────────────

/// `--probe SKILLS-001` — blocks A (skill census §3), B (defaults/groups), C
/// (tally/gate/uses/caps), D (level/HP/SP), E (training) and F
/// (M19 integration).
pub fn run_skills_001() -> i32 {
    println!(
        "PROBE-SKILLS-001 - DFU M20 skills and progression (REQ-L99-053 · CR-a64582b8) · seed {M20_SEED}"
    );
    let Some(dir) = arena2_dir() else {
        println!("EXIT=2");
        println!("VERDICT: PROBE-SKILLS-001 NO DATA - ARENA2 not accessible");
        return 2;
    };
    let Some(bytes) = read_retail(&dir, &class_file_name(1)) else {
        println!("EXIT=2");
        println!("VERDICT: PROBE-SKILLS-001 NO DATA - missing CLASS01.CFG");
        return 2;
    };
    let career = match Career::parse(&bytes) {
        Ok(career) => career,
        Err(err) => {
            println!("EXIT=1");
            println!("VERDICT: PROBE-SKILLS-001 FAIL - CLASS01.CFG → {err}");
            return 1;
        }
    };
    let character = Character::starting(career.clone(), &mut Rng64::new(M20_SEED), 1);

    let a_rows = block_a_skill_rows();
    let b_rows = defaults_rows(&character);
    let c_rows = growth_rows(&character);
    let d_rows = level_rows(&character, &career);
    let e_rows = block_e_rows(&character);
    let f_rows = block_f_rows(&character);

    let a_ok = print_block("A · skill census §3 (35 indices)", &a_rows);
    let b_ok = print_block("B · seed defaults, C01 groups and live = perm + mod", &b_rows);
    let c_ok = print_block("C · tally/gate 360/uses §5.2/cap 100/mastery", &c_rows);
    let d_ok = print_block("D · level/HP/pool/SP/recovery §5", &d_rows);
    let e_ok = print_block("E · training §5.7", &e_rows);
    let f_ok = print_block("F · M19 integration (PlayerBody → run_speed/jump_velocity)", &f_rows);

    let all_ok = a_ok && b_ok && c_ok && d_ok && e_ok && f_ok;
    let code = i32::from(!all_ok);
    let checks = a_rows.len() + b_rows.len() + c_rows.len() + d_rows.len() + e_rows.len() + f_rows.len();
    println!(
        "\nSUMMARY: A={} B={} C={} D={} E={} F={} · checks {checks}",
        verdict(a_ok),
        verdict(b_ok),
        verdict(c_ok),
        verdict(d_ok),
        verdict(e_ok),
        verdict(f_ok)
    );
    println!(
        "[errata] level-up pool measured=4 (spec quotes 5; HP=10 exact) · run_speed(45,0)=6.664557 (ORDER quotes 6.6641)"
    );
    println!("EXIT={code}");
    if all_ok {
        println!(
            "VERDICT: PROBE-SKILLS-001 PASS - {checks} checks (A 35 skills + B defaults/groups + C progression + D level/HP/SP + E training + F M19)"
        );
    } else {
        println!("VERDICT: PROBE-SKILLS-001 FAIL - see [FAIL] rows");
    }
    code
}

#[cfg(test)]
mod tests {
    use super::*;
    use daggerfall_skills::character::SkillSet;

    /// The transcribed §2 table is internally consistent: 18 unique
    /// names, attribute sum 400 and groups within `0..=34`.
    #[test]
    fn career_goldens_are_internally_consistent() {
        let mut names: Vec<&str> = CAREER_GOLDENS.iter().map(|golden| golden.name).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), CLASS_COUNT, "18 unique class names");
        for (index, golden) in CAREER_GOLDENS.iter().enumerate() {
            assert_eq!(
                golden.attributes.iter().sum::<i32>(),
                400,
                "class {index} ({}) must sum to 400 attributes",
                golden.name
            );
            for value in golden
                .primary
                .iter()
                .chain(golden.major.iter())
                .chain(golden.minor.iter())
            {
                assert!(
                    (0..SKILL_COUNT as i32).contains(value),
                    "class {index} ({}) skill index {value} out of range",
                    golden.name
                );
            }
        }
    }

    /// The transcribed §3 table matches the `Skill` enum (name, stat and
    /// multiplier), and languages are exactly 4..=12.
    #[test]
    fn skill_goldens_match_the_enum() {
        for (index, golden) in SKILL_GOLDENS.iter().enumerate() {
            let skill = Skill::ALL[index];
            assert_eq!(skill.index(), index);
            assert_eq!(skill.name(), golden.name);
            assert_eq!(skill.primary_stat(), golden.stat);
            assert_eq!(skill.advancement_multiplier(), golden.advancement);
            assert_eq!(skill.is_language(), (4..=12).contains(&index));
        }
    }

    /// Creation defaults with the lane seed (spec §8.2 B).
    #[test]
    fn default_skills_follow_splitmix64_seed() {
        let skills = SkillSet::defaults(&mut Rng64::new(M20_SEED));
        assert_eq!(*skills.permanents(), DEFAULT_SKILLS);
        assert_eq!(skills.permanent(Skill::Running), 5);
        assert_eq!(skills.permanent(Skill::Medical), 4);
        assert_eq!(skills.permanent(Skill::Jumping), 6);
    }

    /// Uses (§5.2) and level (§5.4) goldens.
    #[test]
    fn progression_goldens_pass() {
        for (value, adv, career, level, expected) in [
            (20, 2, 1.04, 1, 18),
            (5, 50, 1.04, 1, 109),
            (50, 2, 1.0, 1, 42),
            (90, 2, 1.04, 10, 111),
            (0, 1, 1.0, 1, 1),
        ] {
            assert_eq!(
                uses_for_advancement(value, adv, career, level),
                expected,
                "uses ({value}, {adv}, {career}, {level})"
            );
        }
        for (start, current, expected) in
            [(0, 0, 1), (0, 2, 2), (0, 15, 2), (0, 17, 3), (30, 32, 2), (100, 130, 3)]
        {
            assert_eq!(player_level(start, current), expected);
        }
    }

    /// Anti-false-positive: a tampered golden no longer matches.
    #[test]
    fn tampered_uses_golden_fails() {
        assert_ne!(uses_for_advancement(20, 2, 1.04, 1), 19);
        assert_ne!(player_level(30, 32), 3);
    }
}
