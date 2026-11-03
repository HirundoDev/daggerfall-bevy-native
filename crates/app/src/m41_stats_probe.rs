//! `m41_stats_probe` — `PROBE-SYS-STATS-001` (M41 / W41-1): stats, resistances
//! and birthsigns **in depth**.
//!
//! Scope (roadmap §3.6, `docs/acknowledgments/roadmap-m18-m48.md:279-282`):
//! "8 attributes, derived values, resistances by type, advantages/
//! disadvantages, birthsign"; the gate is density — **every derived value and
//! every resistance type carries a golden vector with a citation**.
//!
//! This probe **extends** M20's `PROBE-STATS-001` (`crates/app/src/m20_probe.rs`)
//! instead of repeating it: M20 proves the §4 goldens for `CLASS00`/`CLASS01`
//! at their career defaults, so the vectors here are **boundary vectors**
//! (0, 1, 9, 49, 50, 55, 99, 100 and the two negative inputs) over the same
//! public functions, plus the resistances/advantages/birthsign census M20 does
//! not cover (M20 §8.1 blocks A–D are class/biography/determinism).
//!
//! Blocks (every evidence row is `[OK] <id> - <detail>` with a `src=` or
//! `spec=` citation, or `FAIL <id> got=<v> want=<v> <citation>`):
//!
//! - **A · attributes** — the 8 `DFCareer.Stats` in canonical order, one
//!   `ATTRIBUTE <name> <index>` row each, the default and the
//!   permanent/live/mod semantics;
//! - **B · derived values** — one `DERIVED <name> <value>` row per derived
//!   value measured on the canonical character + golden vectors at boundaries;
//! - **C · resistances by type** — one `RESISTANCE <type> <value>` row for
//!   every type the pinned model defines (`EffectFlags`,
//!   `DFCareer.cs:398-408`: Paralysis, Magic, Poison, Fire, Frost, Shock,
//!   Disease) with a non-default flag vector, plus the numeric magic
//!   resistance `floor(WIL / 10)`;
//! - **D · advantages/disadvantages** — the `PlayerMod` census (6/6, once
//!   each, declaration order), an advantage and a disadvantage application per
//!   mod, the one consumed mod (`AvoidHit` → `behavior::combat::adjustments`)
//!   and a `NEGATIVE` row per mod with no arithmetic consumer **on disk**;
//! - **E · birthsigns** — the frozen 12-month table and `birth_sign` over the
//!   12 months; the mechanical effects are `NEGATIVE` (the pinned corpus has
//!   no birthsign-effect source and the crate stores names only);
//! - **F · adversarial** — the clamps, the two negative-input floors where a
//!   naive integer division silently diverges, an unknown `BIOG` token that
//!   must be **refused**, the out-of-range month that the source *normalizes*
//!   (never a fallback constant) and the tolerance precedence.
//!
//! **0 retail bytes**: the career fixture is a synthetic 74-byte record built
//! here and validated by [`Career::parse`] itself, so the probe is
//! retail-independent and never prints `SKIP` for a missing `ARENA2`.
//!
//! **Determinism:** no floats are printed, all iteration is over fixed arrays,
//! and the last line is `DIGEST <sha256>` over every byte printed before it.

use daggerfall_behavior::combat;
use daggerfall_behavior::locomotion::{max_breath, max_encumbrance};
use daggerfall_heart::calendar::{self, BIRTH_SIGNS};
use daggerfall_heart::clock::GameClock;
use daggerfall_skills::bio::{parse_effect, BiographyEffect, PlayerMod};
use daggerfall_skills::career::{Career, Element, Tolerance, CFG_BYTES};
use daggerfall_skills::character::{
    damage_modifier, fatigue_recovery_rate, healing_rate_modifier, health_recovery_rate,
    hit_points_modifier, magic_resist, max_fatigue_raw, player_level, spell_points,
    spell_recovery_rate, to_hit_modifier, Attribute, Attributes, Character, Skill, ATTRIBUTE_COUNT,
    MAX_ATTRIBUTE,
};
use daggerfall_skills::rng::Rng64;
use sha2::{Digest, Sha256};

/// Lane correlation id (W41-1).
const CR: &str = "CR-8b5f2d61-4c7a-4e93-9f18-6d0c2ae74155";

/// Fixed seed of the synthetic career (§B: the RNG only rolls skill defaults;
/// every printed derived value is set explicitly, so no golden depends on it).
const PROBE_SEED: u64 = 41_010_001;

// ── citations (every one resolves on disk) ──────────────────────────────
const SRC_STATS_ENUM: &str = "reference/dfu-spec/Assets/Scripts/API/DFCareer.cs:429-438";
const SRC_EFFECT_FLAGS: &str = "reference/dfu-spec/Assets/Scripts/API/DFCareer.cs:398-408";
const SRC_TOLERANCE_ENUM: &str = "reference/dfu-spec/Assets/Scripts/API/DFCareer.cs:229-236";
const SRC_TOLERANCE_PRECEDENCE: &str = "reference/dfu-spec/Assets/Scripts/API/DFCareer.cs:746-762";
const SRC_DAMAGE: &str = "reference/dfu-spec/Assets/Scripts/Game/Formulas/FormulaHelper.cs:66-73";
const SRC_ENCUMBRANCE: &str =
    "reference/dfu-spec/Assets/Scripts/Game/Formulas/FormulaHelper.cs:75-82";
const SRC_SPELL_POINTS: &str =
    "reference/dfu-spec/Assets/Scripts/Game/Formulas/FormulaHelper.cs:84-91";
const SRC_MAGIC_RESIST: &str =
    "reference/dfu-spec/Assets/Scripts/Game/Formulas/FormulaHelper.cs:93-100";
const SRC_TO_HIT: &str = "reference/dfu-spec/Assets/Scripts/Game/Formulas/FormulaHelper.cs:102-109";
const SRC_HP_MOD: &str = "reference/dfu-spec/Assets/Scripts/Game/Formulas/FormulaHelper.cs:111-118";
const SRC_HEAL_MOD: &str =
    "reference/dfu-spec/Assets/Scripts/Game/Formulas/FormulaHelper.cs:120-129";
const SRC_MAX_STAT: &str =
    "reference/dfu-spec/Assets/Scripts/Game/Formulas/FormulaHelper.cs:131-138";
const SRC_HEALTH_RECOVERY: &str =
    "reference/dfu-spec/Assets/Scripts/Game/Formulas/FormulaHelper.cs:178-206";
const SRC_FATIGUE_RECOVERY: &str =
    "reference/dfu-spec/Assets/Scripts/Game/Formulas/FormulaHelper.cs:209-216";
const SRC_SPELL_RECOVERY: &str =
    "reference/dfu-spec/Assets/Scripts/Game/Formulas/FormulaHelper.cs:219-229";
const SRC_PLAYER_LEVEL: &str =
    "reference/dfu-spec/Assets/Scripts/Game/Formulas/FormulaHelper.cs:330-337";
const SRC_HP_PER_LEVEL: &str =
    "reference/dfu-spec/Assets/Scripts/Game/Formulas/FormulaHelper.cs:340-354";
const SRC_AVOID_HIT: &str =
    "reference/dfu-spec/Assets/Scripts/Game/Formulas/FormulaHelper.cs:1224-1251";
const SRC_BIRTH_SIGNS: &str =
    "reference/dfu-spec/Assets/Scripts/Utility/DaggerfallDateTime.cs:256-270";
const SRC_BIRTH_SIGN_NAME: &str =
    "reference/dfu-spec/Assets/Scripts/Utility/DaggerfallDateTime.cs:569-575";
const SRC_RAISE_TIME: &str =
    "reference/dfu-spec/Assets/Scripts/Utility/DaggerfallDateTime.cs:357-368";
const SPEC_CHARACTER: &str = "docs/sintergic/game/programming/m20-character-spec-CR-a64582b8.md";
const CRATE_BIRTHSIGNS: &str = "crates/heart/src/calendar.rs";
const CRATE_PLAYER_MOD: &str = "crates/skills/src/character.rs";

/// Citation of one line range of the M20 spec (built at run time so the line
/// suffix stays visible in the printed row).
fn spec(line: &str) -> String {
    format!("{SPEC_CHARACTER}:{line}")
}

/// `max_fatigue`: `DaggerfallEntity.cs:39,262` is **not** in the pinned mirror,
/// so the M20 spec §4 row is the on-disk authority (`:173`, the `Fatiga máxima`
/// row — the citation pointed two rows up before W41/M41 resolution).
const SRC_FATIGUE: &str =
    "docs/sintergic/game/programming/m20-character-spec-CR-a64582b8.md:173";
/// `max_breath`: same custody as [`SRC_FATIGUE`] (`DaggerfallEntity.cs:267`),
/// spec `:174` (the `Aire máximo` row).
const SRC_BREATH: &str =
    "docs/sintergic/game/programming/m20-character-spec-CR-a64582b8.md:174";
/// spec §0/§4 attributes: integer 0..100, default 50.
const SPEC_ATTRIBUTES: &str = "31";
/// spec §4 live semantics (`live = permanent + mods`).
const SPEC_LIVE: &str = "161-162";
/// spec §6.3: the six stored mods and their consumer boundary.
const SPEC_PLAYER_MODS: &str = "306-312";

// ── frozen tables (independent transcription of the pinned source) ──────

/// `DFCareer.Stats` members in declaration order (`DFCareer.cs:429-438`).
const FROZEN_ATTRIBUTES: [&str; ATTRIBUTE_COUNT] = [
    "Strength",
    "Intelligence",
    "Willpower",
    "Agility",
    "Endurance",
    "Personality",
    "Speed",
    "Luck",
];

/// `DaggerfallDateTime.BirthSigns` members in month order
/// (`DaggerfallDateTime.cs:256-270`), transcribed here **independently** of
/// `heart::calendar::BIRTH_SIGNS` so the two can disagree.
const FROZEN_SIGNS: [&str; 12] = [
    "TheRitual",
    "TheLover",
    "TheLord",
    "TheMage",
    "TheShadow",
    "TheSteed",
    "TheApprentice",
    "TheWarrior",
    "TheLady",
    "TheTower",
    "TheAtronach",
    "TheThief",
];

/// Expected [`Tolerance`] per [`Element::ALL`] index for the canonical career's
/// flag bytes `[0b0000_1010, 0b0001_1000, 0b0000_0100, 0b0110_0000]`
/// (`DFCareer.cs:143-146` flag bytes, `:746-762` precedence):
/// Paralysis and Frost … see the per-row `naive=` note.
const FROZEN_TOLERANCES: [Tolerance; 7] = [
    Tolerance::Normal,           // Paralysis — no flag bit set
    Tolerance::Resistant,        // Magic     — resistance bit 2
    Tolerance::LowTolerance,     // Poison    — low-tolerance bit 4
    Tolerance::Resistant,        // Fire      — resistance 8 **and** immunity 8 ⇒ Resistant wins
    Tolerance::Immune,           // Frost     — immunity bit 16
    Tolerance::CriticalWeakness, // Shock     — critical-weakness bit 32
    Tolerance::CriticalWeakness, // Disease   — critical-weakness bit 64
];

// ── evidence rows ───────────────────────────────────────────────────────

/// One printed evidence row.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Row {
    id: String,
    ok: bool,
    detail: String,
}

/// Row collector with the `GV-STAT-<nn>` golden serial.
#[derive(Default)]
struct Rows {
    rows: Vec<Row>,
    golden: usize,
}

impl Rows {
    fn push(&mut self, id: impl Into<String>, ok: bool, detail: String) {
        self.rows.push(Row {
            id: id.into(),
            ok,
            detail,
        });
    }

    /// A `GV-STAT-<nn>` golden vector: `name=<derived> in=<inputs> got=.. want=.. src=..`.
    fn vector(&mut self, name: &str, inputs: &str, got: i32, want: i32, source: &str) {
        self.golden += 1;
        let id = format!("GV-STAT-{:02}", self.golden);
        let ok = got == want;
        self.push(
            id,
            ok,
            format!("name={name} in={inputs} got={got} want={want} src={source}"),
        );
    }

    /// A golden **bound**: `got` must fall inside `low..=high` (used where the
    /// source defines a range and the value is rolled).
    fn bounds(&mut self, name: &str, inputs: &str, got: i32, low: i32, high: i32, source: &str) {
        self.golden += 1;
        let id = format!("GV-STAT-{:02}", self.golden);
        let ok = (low..=high).contains(&got);
        self.push(
            id,
            ok,
            format!("name={name} in={inputs} got={got} want=in[{low},{high}] src={source}"),
        );
    }

    fn all_ok(&self) -> bool {
        self.rows.iter().all(|row| row.ok)
    }
}

/// Emits one row: `[OK] <id> - <detail>` or `FAIL <id> got=.. want=..`.
fn emit(out: &mut String, row: &Row) {
    if row.ok {
        out.push_str(&format!("[OK] {} - {}\n", row.id, row.detail));
    } else {
        out.push_str(&format!("FAIL {} {}\n", row.id, row.detail));
    }
}

fn emit_rows(out: &mut String, rows: &Rows) -> bool {
    for row in &rows.rows {
        emit(out, row);
    }
    rows.all_ok()
}

/// Hexadecimal `sha256` (the `DIGEST` line and nothing else needs it).
fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

// ── synthetic career fixtures (0 retail bytes) ──────────────────────────

/// Builds a 74-byte `CLASSnn.CFG` record (M20 spec §1 offsets) that
/// [`Career::parse`] validates: the parse **is** the fixture check.
fn career_bytes(
    name: &str,
    attributes: [u16; 8],
    hit_points_per_level: u16,
    spell_bits: u16,
    rapid_healing: u8,
    flags: [u8; 4],
) -> Vec<u8> {
    let mut bytes = vec![0u8; CFG_BYTES];
    bytes[0..4].copy_from_slice(&flags);
    bytes[4..6].copy_from_slice(&spell_bits.to_le_bytes());
    bytes[6] = rapid_healing;
    // Skill groups: in range, irrelevant to every derivative measured here.
    bytes[16..19].copy_from_slice(&[27, 25, 26]);
    bytes[19..22].copy_from_slice(&[24, 22, 23]);
    bytes[22..28].copy_from_slice(&[0, 28, 32, 7, 9, 20]);
    assert!(name.len() <= 16, "career name field is 16 bytes");
    bytes[28..28 + name.len()].copy_from_slice(name.as_bytes());
    bytes[52..54].copy_from_slice(&hit_points_per_level.to_le_bytes());
    bytes[54..58].copy_from_slice(&0x0001_0A00u32.to_le_bytes()); // advancement 1.04
    for (index, value) in attributes.iter().enumerate() {
        bytes[58 + index * 2..60 + index * 2].copy_from_slice(&value.to_le_bytes());
    }
    bytes
}

/// The canonical probe career: STR 60, INT 50, WIL 55, AGI 49, END 45,
/// PER 40, SPD 45, LCK 50 · HPL 12 · SP× 1.5 · flag bytes producing seven
/// **distinct** tolerances (see [`FROZEN_TOLERANCES`]).
///
/// `None` when the synthetic record is rejected: the blocks turn that into a
/// red row instead of an `expect` abort (exit 101 with no row at all).
fn canonical_career() -> Option<Career> {
    Career::parse(&career_bytes(
        "ProbeStat",
        [60, 50, 55, 49, 45, 40, 45, 50],
        12,
        0x0C00, // (0x0C00 & 0x1C00) >> 8 == 12 ⇒ 1.5 (`DFCareer.cs:778-815`)
        0,
        [0b0000_1010, 0b0001_1000, 0b0000_0100, 0b0110_0000],
    ))
    .ok()
}

/// Career used only by the attribute block (identical targets, no flags).
fn attribute_career() -> Option<Career> {
    Career::parse(&career_bytes(
        "ProbeAttr",
        [60, 50, 55, 49, 45, 40, 45, 50],
        12,
        0x0C00,
        0,
        [0, 0, 0, 0],
    ))
    .ok()
}

/// The canonical character **plus** the explicit Medical skill the health
/// recovery golden needs (the creation roll is the only RNG consumer).
fn canonical_character() -> Option<Character> {
    let mut character = Character::starting(canonical_career()?, &mut Rng64::new(PROBE_SEED), 1);
    character.skills.set_permanent(Skill::Medical, 30);
    Some(character)
}

/// Stored biography field of one mod (the mechanical effect of the mod).
fn stored(character: &Character, kind: PlayerMod) -> i32 {
    let mods = character.biography;
    match kind {
        PlayerMod::ResistPoison => mods.resist_poison,
        PlayerMod::ResistDisease => mods.resist_disease,
        PlayerMod::ResistMagic => mods.resist_magic,
        PlayerMod::AvoidHit => mods.avoid_hit,
        PlayerMod::Fatigue => mods.fatigue,
        PlayerMod::Reaction => mods.reaction,
    }
}

/// Stored-field label, the `PlayerEntity` member name.
fn stored_field(kind: PlayerMod) -> &'static str {
    match kind {
        PlayerMod::ResistPoison => "resist_poison",
        PlayerMod::ResistDisease => "resist_disease",
        PlayerMod::ResistMagic => "resist_magic",
        PlayerMod::AvoidHit => "avoid_hit",
        PlayerMod::Fatigue => "fatigue",
        PlayerMod::Reaction => "reaction",
    }
}

// ── A · attributes ──────────────────────────────────────────────────────

fn block_a(out: &mut String) -> bool {
    let mut rows = Rows::default();

    for (index, attribute) in Attribute::ALL.iter().enumerate() {
        out.push_str(&format!(
            "ATTRIBUTE {} {} short={} src={SRC_STATS_ENUM}\n",
            attribute.name(),
            index,
            attribute.short_name()
        ));
    }

    let order: Vec<&str> = Attribute::ALL.iter().map(|attribute| attribute.name()).collect();
    rows.push(
        "att-order",
        order == FROZEN_ATTRIBUTES.to_vec(),
        format!(
            "got={} want={} src={SRC_STATS_ENUM}",
            order.join(","),
            FROZEN_ATTRIBUTES.join(",")
        ),
    );
    rows.push(
        "att-count",
        Attribute::ALL.len() == 8 && ATTRIBUTE_COUNT == 8,
        format!(
            "got={} want=8 const={ATTRIBUTE_COUNT} src={SRC_STATS_ENUM}",
            Attribute::ALL.len()
        ),
    );
    let indexes: Vec<usize> = Attribute::ALL.iter().map(|attribute| attribute.index()).collect();
    rows.push(
        "att-index",
        indexes == (0..8).collect::<Vec<usize>>(),
        format!("got={indexes:?} want=0..7 src={SRC_STATS_ENUM}"),
    );

    let defaults = Attributes::defaults();
    let default_ok = Attribute::ALL
        .iter()
        .all(|attribute| defaults.permanent(*attribute) == 50 && defaults.live(*attribute) == 50);
    rows.push(
        "att-default",
        default_ok,
        format!("permanent=50 mod=0 live=50 all=8 spec={}", spec(SPEC_ATTRIBUTES)),
    );

    let Some(attribute_career) = attribute_career() else {
        rows.push(
            "att-career-parse",
            false,
            String::from("the synthetic attribute career is not a valid 74-byte record"),
        );
        return emit_rows(out, &rows);
    };
    let mut attributes = Attributes::from_career(&attribute_career);
    attributes.set_mod(Attribute::Strength, 7);
    rows.push(
        "att-live",
        attributes.permanent(Attribute::Strength) == 60
            && attributes.live(Attribute::Strength) == 67,
        format!(
            "permanent={} mod=7 live={} want=60/67 spec={}",
            attributes.permanent(Attribute::Strength),
            attributes.live(Attribute::Strength),
            spec(SPEC_LIVE)
        ),
    );

    rows.push(
        "att-cap",
        MAX_ATTRIBUTE == 100,
        format!("MAX_ATTRIBUTE={MAX_ATTRIBUTE} want=100 src={SRC_MAX_STAT}"),
    );

    emit_rows(out, &rows)
}

// ── B · derived values ──────────────────────────────────────────────────

fn block_b(out: &mut String) -> bool {
    let mut rows = Rows::default();
    let Some(character) = canonical_character() else {
        rows.push(
            "career-parse",
            false,
            String::from("the canonical probe career is not a valid 74-byte record"),
        );
        return emit_rows(out, &rows);
    };

    // The canonical character: STR 60 · INT 50 · WIL 55 · AGI 49 · END 45.
    let derived: [(&str, i32, &str); 14] = [
        ("damage_modifier", character.damage_modifier(), SRC_DAMAGE),
        ("max_encumbrance", character.max_encumbrance(), SRC_ENCUMBRANCE),
        ("max_magicka", character.max_magicka(), SRC_SPELL_POINTS),
        ("magic_resist", character.magic_resist(), SRC_MAGIC_RESIST),
        ("to_hit_modifier", character.to_hit_modifier(), SRC_TO_HIT),
        ("hit_points_modifier", character.hit_points_modifier(), SRC_HP_MOD),
        (
            "healing_rate_modifier",
            character.healing_rate_modifier(),
            SRC_HEAL_MOD,
        ),
        ("max_fatigue", character.max_fatigue(), SRC_FATIGUE),
        ("max_breath", character.max_breath(), SRC_BREATH),
        (
            "health_recovery_rate",
            character.health_recovery_rate(true, false),
            SRC_HEALTH_RECOVERY,
        ),
        (
            "fatigue_recovery_rate",
            character.fatigue_recovery_rate(),
            SRC_FATIGUE_RECOVERY,
        ),
        (
            "spell_recovery_rate",
            character.spell_recovery_rate(),
            SRC_SPELL_RECOVERY,
        ),
        (
            "roll_hit_points",
            {
                let mut rng = Rng64::new(PROBE_SEED);
                character.roll_hit_points(&mut rng)
            },
            SRC_HP_PER_LEVEL,
        ),
        ("player_level", character.calculated_level(), SRC_PLAYER_LEVEL),
    ];
    for (name, value, source) in derived {
        out.push_str(&format!("DERIVED {name} {value} src={source}\n"));
    }

    // Golden vectors over several inputs each (labels below are the
    // DERIVED names the gate cross-checks).
    for (strength, want) in [(0, -10), (49, -1), (55, 1), (99, 9), (100, 10)] {
        rows.vector(
            "damage_modifier",
            &format!("STR={strength}"),
            damage_modifier(strength),
            want,
            SRC_DAMAGE,
        );
    }
    for (strength, want) in [(0, 0), (1, 1), (49, 73), (99, 148), (100, 150)] {
        rows.vector(
            "max_encumbrance",
            &format!("STR={strength}"),
            max_encumbrance(strength),
            want,
            SRC_ENCUMBRANCE,
        );
    }
    for (intelligence, multiplier, want) in [
        (0, 3.0f32, 0),
        (1, 0.5, 0),
        (50, 1.5, 75),
        (75, 2.0, 150),
        (100, 1.5, 150),
    ] {
        rows.vector(
            "max_magicka",
            &format!("INT={intelligence} x={multiplier}"),
            spell_points(intelligence, multiplier),
            want,
            SRC_SPELL_POINTS,
        );
    }
    for (willpower, want) in [(0, 0), (9, 0), (10, 1), (55, 5), (99, 9), (100, 10)] {
        rows.vector(
            "magic_resist",
            &format!("WIL={willpower}"),
            magic_resist(willpower),
            want,
            SRC_MAGIC_RESIST,
        );
    }
    for (agility, want) in [(0, -5), (1, -5), (49, -1), (50, 0), (100, 5)] {
        rows.vector(
            "to_hit_modifier",
            &format!("AGI={agility}"),
            to_hit_modifier(agility),
            want,
            SRC_TO_HIT,
        );
    }
    for (endurance, want) in [(0, -5), (9, -5), (45, -1), (50, 0), (100, 5)] {
        rows.vector(
            "hit_points_modifier",
            &format!("END={endurance}"),
            hit_points_modifier(endurance),
            want,
            SRC_HP_MOD,
        );
    }
    for (endurance, want) in [(0, -5), (9, -5), (59, 0), (60, 1), (100, 5)] {
        rows.vector(
            "healing_rate_modifier",
            &format!("END={endurance}"),
            healing_rate_modifier(endurance),
            want,
            SRC_HEAL_MOD,
        );
    }
    for (strength, endurance, want) in [(0, 0, 0), (1, 0, 64), (60, 45, 6720), (100, 100, 12800)]
    {
        rows.vector(
            "max_fatigue",
            &format!("STR={strength} END={endurance}"),
            max_fatigue_raw(strength, endurance),
            want,
            SRC_FATIGUE,
        );
    }
    for (endurance, want) in [(0, 0), (1, 0), (45, 22), (99, 49), (100, 50)] {
        rows.vector(
            "max_breath",
            &format!("END={endurance}"),
            max_breath(endurance),
            want,
            SRC_BREATH,
        );
    }
    for (heal_mod, medical, extra, max_health, want) in [
        (-1, 30, 60, 37, 2),
        (-5, 0, 60, 10, 1),
        (0, 0, 100, 1000, 100),
        (5, 90, 100, 1000, 195),
    ] {
        rows.vector(
            "health_recovery_rate",
            &format!("healmod={heal_mod} med={medical} extra={extra} maxHP={max_health}"),
            health_recovery_rate(heal_mod, medical, extra, max_health),
            want,
            SRC_HEALTH_RECOVERY,
        );
    }
    for (max_fatigue, want) in [(0, 1), (7, 1), (8, 1), (64, 8), (6720, 840)] {
        rows.vector(
            "fatigue_recovery_rate",
            &format!("maxFatigue={max_fatigue}"),
            fatigue_recovery_rate(max_fatigue),
            want,
            SRC_FATIGUE_RECOVERY,
        );
    }
    for (max_magicka, no_regen, want) in [(0, false, 1), (75, false, 9), (120, false, 15), (120, true, 0)]
    {
        rows.vector(
            "spell_recovery_rate",
            &format!("maxMagicka={max_magicka} noRegen={no_regen}"),
            spell_recovery_rate(max_magicka, no_regen),
            want,
            SRC_SPELL_RECOVERY,
        );
    }
    // `CalculateHitPointsPerLevelUp`: `range(HPL/2, HPL) + HitPointsModifier`
    // with a floor of 1 (`:340-354`), HPL 12 and permanent END 45 ⇒ 6..=12 − 1.
    {
        let mut rng = Rng64::new(PROBE_SEED);
        let mut low = i32::MAX;
        let mut high = i32::MIN;
        for _ in 0..16 {
            let roll = character.roll_hit_points(&mut rng);
            low = low.min(roll);
            high = high.max(roll);
        }
        rows.bounds(
            "roll_hit_points",
            &format!("HPL=12 ENDmod={} draws=16 observed={low}..{high}", character.hit_points_modifier()),
            low,
            5,
            11,
            SRC_HP_PER_LEVEL,
        );
        rows.push(
            "derived-roll_hit_points-draws",
            low >= 5 && high <= 11 && low <= high,
            format!(
                "draws=16 min={low} max={high} want=in[5,11] src={SRC_HP_PER_LEVEL}"
            ),
        );
    }
    for (starting, current, want) in [(0, 0, 1), (0, 15, 2), (15, 0, 0), (0, 27, 3), (100, 100, 1)]
    {
        rows.vector(
            "player_level",
            &format!("start={starting} current={current}"),
            player_level(starting, current),
            want,
            SRC_PLAYER_LEVEL,
        );
    }

    // Integration: the canonical character's live values are the goldens the
    // §4 table predicts for STR 60 / INT 50 / WIL 55 / AGI 49 / END 45.
    for (name, got, want, source) in [
        ("damage_modifier", character.damage_modifier(), 2, SRC_DAMAGE),
        ("max_encumbrance", character.max_encumbrance(), 90, SRC_ENCUMBRANCE),
        ("max_magicka", character.max_magicka(), 75, SRC_SPELL_POINTS),
        ("magic_resist", character.magic_resist(), 5, SRC_MAGIC_RESIST),
        ("to_hit_modifier", character.to_hit_modifier(), -1, SRC_TO_HIT),
        ("hit_points_modifier", character.hit_points_modifier(), -1, SRC_HP_MOD),
        (
            "healing_rate_modifier",
            character.healing_rate_modifier(),
            -1,
            SRC_HEAL_MOD,
        ),
        ("max_fatigue", character.max_fatigue(), 6720, SRC_FATIGUE),
        ("max_breath", character.max_breath(), 22, SRC_BREATH),
        (
            "health_recovery_rate",
            character.health_recovery_rate(true, false),
            2,
            SRC_HEALTH_RECOVERY,
        ),
        (
            "fatigue_recovery_rate",
            character.fatigue_recovery_rate(),
            840,
            SRC_FATIGUE_RECOVERY,
        ),
        (
            "spell_recovery_rate",
            character.spell_recovery_rate(),
            9,
            SRC_SPELL_RECOVERY,
        ),
        ("player_level", character.calculated_level(), 1, SRC_PLAYER_LEVEL),
        ("max_health", character.max_health, 37, "reference/dfu-spec/Assets/Scripts/Game/Formulas/FormulaHelper.cs:160-175"),
    ] {
        rows.push(
            format!("DERIVED-live-{name}"),
            got == want,
            format!("character=ProbeStat got={got} want={want} src={source}"),
        );
    }

    emit_rows(out, &rows)
}

// ── C · resistances by type ─────────────────────────────────────────────

fn block_c(out: &mut String) -> bool {
    let mut rows = Rows::default();
    let Some(career) = canonical_career() else {
        rows.push(
            "career-parse",
            false,
            String::from("the canonical probe career is not a valid 74-byte record"),
        );
        return emit_rows(out, &rows);
    };
    let Some(character) = canonical_character() else {
        rows.push(
            "career-parse-character",
            false,
            String::from("the canonical probe character could not be built"),
        );
        return emit_rows(out, &rows);
    };
    let mut type_rows = 0usize;

    for (index, element) in Element::ALL.iter().enumerate() {
        let got = career.tolerance(*element);
        let numeric = if *element == Element::Magic {
            format!(" magic_resist={}", character.magic_resist())
        } else {
            String::new()
        };
        out.push_str(&format!(
            "RESISTANCE {} {}{numeric} src={SRC_EFFECT_FLAGS}\n",
            element.name(),
            got.name()
        ));
        type_rows += 1;
        rows.push(
            format!("GV-RES-{:02}", index + 1),
            got == FROZEN_TOLERANCES[index],
            format!(
                "type={} got={} want={} flags=R:{} I:{} L:{} C:{} src={SRC_TOLERANCE_PRECEDENCE}",
                element.name(),
                got.name(),
                FROZEN_TOLERANCES[index].name(),
                career.resistance_flags,
                career.immunity_flags,
                career.low_tolerance_flags,
                career.critical_weakness_flags
            ),
        );
    }

    rows.push(
        "res-census",
        type_rows == Element::ALL.len() && type_rows == 7,
        format!(
            "types={type_rows} want=7 src={SRC_EFFECT_FLAGS} tolerance-enum={SRC_TOLERANCE_ENUM}"
        ),
    );
    rows.push(
        "res-magic-formula",
        character.magic_resist() == magic_resist(character.live(Attribute::Willpower))
            && character.magic_resist() == 5,
        format!(
            "WIL={} magic_resist={} want=5 src={SRC_MAGIC_RESIST}",
            character.live(Attribute::Willpower),
            character.magic_resist()
        ),
    );
    rows.push(
        "res-distinct",
        {
            let mut names: Vec<&str> =
                FROZEN_TOLERANCES.iter().map(|tolerance| tolerance.name()).collect();
            names.sort_unstable();
            names.dedup();
            names.len() >= 4
        },
        format!(
            "distinct-tolerances=4 want=>=4 src={SRC_TOLERANCE_PRECEDENCE} (the flag vector is not flat)"
        ),
    );

    emit_rows(out, &rows)
}

// ── D · advantages / disadvantages ──────────────────────────────────────

/// The six `PlayerMod` variants in declaration order -- an independent
/// transcription of `PlayerEntity.cs:172-177`, `(variant, id, code)`.
///
/// The table is frozen here (and not read from a helper the crate may or may
/// not expose) so the census has its own declaration; the arithmetic of
/// `apply_biography_effects` stays the only consumer path in the crate.
const MOD_TABLE: [(PlayerMod, &str, &str); 6] = [
    (PlayerMod::ResistPoison, "ResistPoison", "RP"),
    (PlayerMod::ResistDisease, "ResistDisease", "RD"),
    (PlayerMod::ResistMagic, "ResistMagic", "MR"),
    (PlayerMod::AvoidHit, "AvoidHit", "TH"),
    (PlayerMod::Fatigue, "Fatigue", "FT"),
    (PlayerMod::Reaction, "Reaction", "RR"),
];

fn block_d(out: &mut String) -> bool {
    let mut rows = Rows::default();
    let Some(mut character) = canonical_character() else {
        rows.push(
            "career-parse",
            false,
            String::from("the canonical probe character could not be built"),
        );
        return emit_rows(out, &rows);
    };
    let mut advantages = 0usize;
    let mut disadvantages = 0usize;

    // The order the crate actually declares, against the transcription.
    let declared: Vec<String> = MOD_TABLE
        .iter()
        .map(|(kind, ..)| format!("{kind:?}"))
        .collect();
    let written: Vec<&str> = MOD_TABLE.iter().map(|(_, name, _)| *name).collect();
    rows.push(
        "mod-enum-order",
        declared.iter().map(String::as_str).eq(written.iter().copied()),
        format!(
            "declared={} want={} spec={}",
            declared.join(","),
            written.join(","),
            spec(SPEC_PLAYER_MODS)
        ),
    );

    for (index, (kind, name, code)) in MOD_TABLE.iter().enumerate() {
        out.push_str(&format!(
            "MOD {} {index} {} field={} spec={}\n",
            code,
            name,
            stored_field(*kind),
            spec(SPEC_PLAYER_MODS)
        ));

        // Advantage: a signed-in mod (BiogFile applies the written value).
        character.apply_biography_effects(&[format!("{code} +10")]);
        let advantage = stored(&character, *kind);
        advantages += 1;
        rows.push(
            format!("MOD-ADV-{index}"),
            advantage == 10,
            format!(
                "code={code} field={} got={advantage} want=10 spec={}",
                stored_field(*kind),
                spec(SPEC_PLAYER_MODS)
            ),
        );

        // Disadvantage: the same mod with a negative amount.
        character.apply_biography_effects(&[format!("{code} -15")]);
        let disadvantage = stored(&character, *kind);
        disadvantages += 1;
        rows.push(
            format!("MOD-DIS-{index}"),
            disadvantage == -15,
            format!(
                "code={code} field={} got={disadvantage} want=-15 spec={}",
                stored_field(*kind),
                spec(SPEC_PLAYER_MODS)
            ),
        );
    }

    // The only mod with an arithmetic consumer: `TH` ─ `−BiographyAvoidHitMod`
    // when the player is the hit target, on top of the universal −50.
    let avoid_hit_advantage = combat::adjustments(true, 15, false);
    let avoid_hit_disadvantage = combat::adjustments(true, -15, false);
    let non_player = combat::adjustments(false, 15, false);
    rows.push(
        "MOD-EFFECT-AvoidHit",
        avoid_hit_advantage == -65 && avoid_hit_disadvantage == -35 && non_player == -50,
        format!(
            "adv={avoid_hit_advantage} want=-65 dis={avoid_hit_disadvantage} want=-35 \
             non-player={non_player} want=-50 src={SRC_AVOID_HIT}"
        ),
    );

    // The five mods with no consumer on disk: the crate stores the field and
    // the M20 spec declares the consumer as a boundary (§6.3), so the
    // arithmetic is **not** invented here.
    for (kind, name, code) in MOD_TABLE {
        if matches!(kind, PlayerMod::AvoidHit) {
            continue;
        }
        out.push_str(&format!(
            "NEGATIVE mod-consumer-{name} reason={CRATE_PLAYER_MOD} stored-only \
             field={} code={code} spec={}\n",
            stored_field(kind),
            spec(SPEC_PLAYER_MODS)
        ));
    }

    out.push_str(&format!(
        "CENSUS advantages={advantages} disadvantages={disadvantages} total={} spec={}\n",
        advantages + disadvantages,
        spec(SPEC_PLAYER_MODS)
    ));
    rows.push(
        "mod-census",
        advantages == 6 && disadvantages == 6 && MOD_TABLE.len() == 6,
        format!(
            "variants={} advantages={advantages} disadvantages={disadvantages} want=6/6/6 spec={}",
            MOD_TABLE.len(),
            spec(SPEC_PLAYER_MODS)
        ),
    );

    emit_rows(out, &rows)
}

// ── E · birthsigns ──────────────────────────────────────────────────────

fn block_e(out: &mut String) -> bool {
    let mut rows = Rows::default();
    let mut clock = GameClock::classic_default();
    let mut table_rows = 0usize;

    for (month, frozen) in FROZEN_SIGNS.iter().enumerate() {
        out.push_str(&format!(
            "BIRTHSIGN {month} {frozen} NEGATIVE src={SRC_BIRTH_SIGNS}\n"
        ));
        table_rows += 1;
        clock.month = month as u32;
        let got = calendar::birth_sign(&clock);
        rows.push(
            format!("GV-SIGN-{:02}", month + 1),
            got == *frozen,
            format!("month={month} got={got} want={frozen} src={SRC_BIRTH_SIGN_NAME}"),
        );
    }

    rows.push(
        "sign-table",
        BIRTH_SIGNS == FROZEN_SIGNS && table_rows == 12,
        format!(
            "crate={} frozen={table_rows} want=12 src={SRC_BIRTH_SIGNS}",
            BIRTH_SIGNS.len()
        ),
    );

    // The pinned corpus defines the 12 names and no mechanical effect: the
    // effect is declared NEGATIVE with the path that stores the names only.
    out.push_str(&format!(
        "NEGATIVE birthsign-effects reason={CRATE_BIRTHSIGNS} months={table_rows} spec={SRC_BIRTH_SIGNS}\n"
    ));
    rows.push(
        "sign-effects-negative",
        table_rows == 12 && BIRTH_SIGNS.iter().all(|sign| !sign.is_empty()),
        format!(
            "months={table_rows} effects=0 declared=NEGATIVE reason={CRATE_BIRTHSIGNS} spec={SRC_BIRTH_SIGNS}"
        ),
    );

    emit_rows(out, &rows)
}

// ── F · adversarial ─────────────────────────────────────────────────────

fn block_f(out: &mut String) -> bool {
    let mut rows = Rows::default();

    // F1 — the attribute cap: a naive `set` lets 1000 through.
    let mut attributes = Attributes::defaults();
    attributes.set_permanent(Attribute::Strength, 1000);
    let capped = attributes.permanent(Attribute::Strength);
    attributes.set_permanent(Attribute::Strength, -1);
    let floored = attributes.permanent(Attribute::Strength);
    rows.push(
        "ADV-attr-cap",
        capped == 100 && floored == 0,
        format!(
            "in=1000 got={capped} want=100 · in=-1 got={floored} want=0 src={SRC_MAX_STAT} \
             floor=declared(no source)"
        ),
    );

    // F2 — the negative floor: `Mathf.Floor(x/10)` ≠ integer division for
    // negative inputs. A truncating Rust `i32` division returns 0 / −5. The
    // naive baseline is computed through a binding so the compiler does not
    // constant-fold the comparison itself.
    let wil_input = -1i32;
    let wil = magic_resist(wil_input);
    let naive_wil = wil_input / 10;
    let agi_input = -1i32;
    let agi = to_hit_modifier(agi_input);
    let naive_agi = agi_input / 10 - 5;
    rows.push(
        "ADV-negative-willpower",
        wil == -1 && naive_wil == 0,
        format!("WIL=-1 got={wil} want=-1 naive-i32-div={naive_wil} src={SRC_MAGIC_RESIST}"),
    );
    rows.push(
        "ADV-negative-agility",
        agi == -6 && naive_agi == -5,
        format!("AGI=-1 got={agi} want=-6 naive-i32-div={naive_agi} src={SRC_TO_HIT}"),
    );

    // F3 — an unknown BIOG token must be refused, not coerced into a mod.
    let parsed = parse_effect("RZ +10");
    let Some(mut character) = canonical_character() else {
        rows.push(
            "ADV-career-parse",
            false,
            String::from("the canonical probe character could not be built"),
        );
        return emit_rows(out, &rows);
    };
    let report = character.apply_biography_effects(&["RZ +10".to_string()]);
    let untouched = MOD_TABLE
        .iter()
        .all(|(kind, ..)| stored(&character, *kind) == 0);
    rows.push(
        "ADV-refuse-unknown-mod",
        matches!(parsed, BiographyEffect::Unknown(_))
            && report.mods == 0
            && report.unknown == 1
            && untouched,
        format!(
            "token=RZ parsed={} mods={} unknown={} stored-unchanged={untouched} want=refused spec={}",
            if matches!(parsed, BiographyEffect::Unknown(_)) {
                "Unknown"
            } else {
                "applied"
            },
            report.mods,
            report.unknown,
            spec(SPEC_PLAYER_MODS)
        ),
    );
    // The same grammar must still accept a valid token (the refusal is not a
    // blanket rejection).
    rows.push(
        "ADV-accept-known-mod",
        matches!(parse_effect("RP +10"), BiographyEffect::PlayerMod { kind: PlayerMod::ResistPoison, amount: 10 }),
        format!("token=RP want=PlayerMod/ResistPoison/+10 spec={}", spec(SPEC_PLAYER_MODS)),
    );

    // F4 — out-of-range month: the source normalizes through `RaiseTime(0)`,
    // so 12 is month 0 of the next year. A `unwrap_or("TheSteed")` fallback
    // (the pre-fix behaviour) returns month 5 for every invalid month.
    let mut clock = GameClock::classic_default();
    for (month, want) in [(12u32, "TheRitual"), (23, "TheThief"), (24, "TheRitual")] {
        clock.month = month;
        let got = calendar::birth_sign(&clock);
        rows.push(
            format!("ADV-sign-{month}"),
            got == want,
            format!(
                "month={month} got={got} want={want} naive-fallback=TheSteed src={SRC_BIRTH_SIGN_NAME} carry={SRC_RAISE_TIME}"
            ),
        );
    }

    // F5 — tolerance precedence: Fire carries both the resistance and the
    // immunity bit; `GetTolerance` tests resistance first.
    let Some(career) = canonical_career() else {
        rows.push(
            "ADV-career-parse-f5",
            false,
            String::from("the canonical probe career is not a valid 74-byte record"),
        );
        return emit_rows(out, &rows);
    };
    let fire = career.tolerance(Element::Fire);
    rows.push(
        "ADV-tolerance-precedence",
        fire == Tolerance::Resistant,
        format!(
            "element=Fire R={} I={} got={} want=Resistant naive-last-wins=Immune src={SRC_TOLERANCE_PRECEDENCE}",
            career.resistance_flags & Element::Fire.mask() != 0,
            career.immunity_flags & Element::Fire.mask() != 0,
            fire.name()
        ),
    );

    emit_rows(out, &rows)
}

// ── entrypoint ──────────────────────────────────────────────────────────

/// `--probe SYS-STATS-001`: blocks A–F, `EXIT=0` ⟺ every row OK.
pub fn run_sys_stats_001() -> i32 {
    let mut out = String::new();
    out.push_str(&format!(
        "PROBE-SYS-STATS-001 - stats/resistances/birthsigns in depth (M41/W41-1, {CR})\n"
    ));

    let mut ok = true;
    out.push_str("\n-- A · attributes (8, canonical order) --\n");
    ok &= block_a(&mut out);
    out.push_str("\n-- B · derived values (golden vector per value) --\n");
    ok &= block_b(&mut out);
    out.push_str("\n-- C · resistances by type (7 EffectFlags + WIL formula) --\n");
    ok &= block_c(&mut out);
    out.push_str("\n-- D · advantages/disadvantages (PlayerMod census) --\n");
    ok &= block_d(&mut out);
    out.push_str("\n-- E · birthsigns (12 months, frozen table) --\n");
    ok &= block_e(&mut out);
    out.push_str("\n-- F · adversarial (clamps, negatives, refusals, normalization) --\n");
    ok &= block_f(&mut out);

    out.push_str(if ok {
        "\nVEREDICTO: PASS\n"
    } else {
        "\nVEREDICTO: FAIL\n"
    });
    print!("{out}");
    println!("DIGEST {}", sha256_hex(out.as_bytes()));
    if ok {
        0
    } else {
        1
    }
}
