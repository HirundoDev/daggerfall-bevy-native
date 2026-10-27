//! M33 probes — total persistence and the classic SAVEGAME
//! (SPEC-M33-TOTAL-PERSISTENCE, REQ-L99-066, spec
//! `m33-total-persistence-savegame-spec-CR-5243af63.md` §6 and §9).
//!
//! - **`PROBE-SAVE-ALL-001`** (`--probe SAVE-ALL-001`) — blocks A-E:
//!   - **A** 24 synthetic states (baseline + one per module M19-M32 + three
//!     combined + six boundary): `save→load→save` byte-identical **and**
//!     `load(save(s)) == s` → `[OK] roundtrip: 24/24 idénticos`.
//!     Composition note: the spec §6.1 parenthetical (baseline + 14 modules +
//!     3 combined) adds up to 18, not 24; the six boundary states (empty,
//!     UTF-8, negatives, max census, clock boundary, enchanted item) close the
//!     declared total without inventing module ownership.
//!   - **B** the S0 census: `85` lines, `1705` B, `f793b743974d7f39`.
//!   - **C** five corruption mutations, each one **must fail** (anti-false-
//!     positive rule: a reader that accepts a mutation breaks the check).
//!   - **D** two complete passes ⇒ the same digest, printed `×2`.
//!   - **E** escaping of `\ | \n \r` + UTF-8 in `name`/`quest` (GV-06).
//!
//!   `EXIT=0 ⟺ A∧B∧C∧D∧E`; `EXIT=1` on any FAIL. 0 ARENA2 dependence.
//! - **`PROBE-SAVE-MIGRATION-001`** — three v1 fixtures embedded as text
//!   (0 retail bytes): each migrates to a valid v2 and
//!   `migrate(migrate(x)) == migrate(x)`; GV-05/GV-09 census printed;
//!   `EXIT=1` when a valid v1 is rejected or a migrated v2 does not re-parse
//!   (§6.2 — GV-09 gated against the corrected literal, corrección #23).
//! - **`PROBE-SAVEGAME-READ-001`** (Mode A, read-only) — retail `MAPSAVE.SAV`
//!   layout + `SAVE00I0.IMG` header + the classic census. Each classic file
//!   (`SAVETREE.DAT`, `SAVEVARS.DAT`, `IMAGE.RAW`) has **three** declared
//!   states (CR-28fb1784, F-AUD-03): **absent** ⇒ the spec §2.4 N2/N3 NEGATIVE
//!   declaration as written; **present and comparable** ⇒ compared against the
//!   source that corresponds to it and its verdict is real; **present with no
//!   source of truth** ⇒ `NOT-COMPARABLE`, a declared `SKIP` with its literal
//!   reason — never a FAIL for the missing authority and never a silent PASS.
//!   `EXIT=2` with the literal reason when ARENA2 (or a classic file that
//!   exists) cannot be read.
//! - **`PROBE-SAVEGAME-WRITE-001`** — synthetic `SAVETREE.DAT` round-trip
//!   through **our** writer/reader + a `MAPSAVE` copy round-trip; acceptance
//!   by the original 1.07 binary is declared NEGATIVE (§6.4); `EXIT=2` when no
//!   writable destination exists.
//! - **`M33-DEMO-001`** (DEMO-033, §9, headless) — create, advance 10 game
//!   minutes, save, mutate gold/HP/quest/guild, load ⇒ the snapshot comes back
//!   exactly and `save→load→save` is byte-identical. 0 UI lines.
//!
//! Mode A custody: retail bytes are read at runtime only; the probes write
//! only their own temp workspace (0 retail bytes written back, 0 in git).

use std::fs;
use std::path::{Path, PathBuf};

use daggerfall_memory::savegame::{
    BuildingRecord, MapSave, RecordRoot, SaveTree, SaveTreeHeader, SaveTreeRecord,
    BUILDING_BYTES, MAPSAVE_DIR_ENTRY_BYTES, MAPSAVE_DIR_TYPE, RECORD_ROOT_BYTES,
    SAVETREE_HEADER_BYTES, SAVETREE_VERSION,
};
use daggerfall_memory::savegame_v2::{
    migrate_text, ActiveEffectV2, AutomapV2, ClockSection, ContainerV2,
    DiseaseV2, GoodV2, GuildMembershipV2, ItemV2, LoanV2, SaveGameV2, S0_TEXT,
    ATTRIBUTE_COUNT, DISEASE_ACCUMULATORS, EQUIPPED_SLOTS, SKILL_COUNT, WEATHER_COUNT,
};
use daggerfall_memory::session_store::SaveError;
use daggerfall_parser_rs::image::ImgHeader;

// ═══════════════════════════════════════════════════════════════════════════
// Evidence rows (M19..M32 template)
// ═══════════════════════════════════════════════════════════════════════════

/// `PASS`/`FAIL` word for the summary lines.
fn verdict(ok: bool) -> &'static str {
    if ok {
        "PASS"
    } else {
        "FAIL"
    }
}

/// FNV-1a 64 (offset basis `0xcbf2_9ce4_8422_2325`, prime `0x1000_0000_01b3`).
///
/// Same function as `session_store::fnv1a64` (which is crate-private); the
/// probe needs it to re-seal the block-C mutations so they reach the closed-
/// key parser instead of dying at the checksum.
fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// Per-probe writable workspace (spec §6.1 EXIT=2 / §6.4 destination rule).
fn workspace(tag: &str) -> Result<PathBuf, String> {
    let dir = std::env::temp_dir().join(format!("daggerfall-m33-{tag}-{}", std::process::id()));
    fs::create_dir_all(&dir).map_err(|error| format!("create {}: {error}", dir.display()))?;
    Ok(dir)
}

/// The `S0` body without its checksum line (block C mutates the payload).
fn s0_body_lines() -> Vec<String> {
    let mut lines: Vec<String> = S0_TEXT.lines().map(str::to_string).collect();
    lines.pop();
    lines
}

/// Re-seals a mutated payload with a fresh FNV checksum (block C: mutations
/// 2-4 must pass the checksum barrier and fail at the closed keys instead).
fn seal(lines: Vec<String>) -> String {
    let mut body = lines.join("\n");
    body.push('\n');
    let checksum = fnv1a64(body.as_bytes());
    body.push_str(&format!("checksum={checksum:016x}\n"));
    body
}

// ═══════════════════════════════════════════════════════════════════════════
// PROBE-SAVE-ALL-001
// ═══════════════════════════════════════════════════════════════════════════

/// The 24 synthetic states of block A: baseline + one per module M19-M32 +
/// three combined + six boundary states (the spec parenthetical adds to 18;
/// the six boundary states close the declared 24 — see the module doc).
fn synthetic_states() -> Vec<(&'static str, SaveGameV2)> {
    let mut states: Vec<(&'static str, SaveGameV2)> = Vec::with_capacity(24);

    // 1 · baseline — the canonical §4 golden sample S0.
    states.push(("baseline", SaveGameV2::sample()));

    // 2-15 · one state per module M19-M32 (risk R-M33-1: a module without
    // persisted state fails its own state).
    let mut s = SaveGameV2::new();
    s.world.pos_x_mm = 12_345;
    s.world.pos_y_mm = -700;
    s.world.pos_z_mm = 9_876;
    s.world.yaw_mrad = 1_571;
    s.world.pitch_mrad = -100;
    s.world.region_index = 5;
    s.world.location_index = 3;
    s.world.environment = 1;
    s.world.map_id = 65_535;
    states.push(("M19-locomotion", s));

    let mut s = SaveGameV2::new();
    s.character.level = 7;
    s.character.experience = 1_234;
    s.character.health = 80;
    s.character.max_health = 100;
    s.character.attributes = [45, 40, 38, 50, 42, 35, 48, 41];
    s.character.skills = {
        let mut skills = [0i64; SKILL_COUNT];
        skills[0] = 40;
        skills[1] = 33;
        skills[SKILL_COUNT - 1] = 12;
        skills
    };
    s.character.levelup_skill_sum = 150;
    states.push(("M20-character", s));

    let mut s = SaveGameV2::new();
    s.inventory.gold = 2_500;
    s.inventory.goods = vec![GoodV2 {
        id: "hierro".to_string(),
        quantity: 12,
    }];
    s.inventory.items = vec![ItemV2 {
        group: 1,
        index: 3,
        value: 250,
        flags: 1,
        current_condition: 40,
        max_condition: 50,
        type_dependent: 6,
        material: 2,
        color: 1,
        weight_cu: 30,
        enchantment_points: 0,
        variants: 0,
        draw_order: 2,
        enchantment_slots: 0,
        enchants: Vec::new(),
    }];
    s.inventory.equipped = {
        let mut slots = [-1i64; EQUIPPED_SLOTS];
        slots[0] = 0;
        slots
    };
    s.inventory.containers = vec![ContainerV2 {
        kind: 4,
        items: vec![0],
    }];
    states.push(("M21-inventory", s));

    let mut s = SaveGameV2::new();
    s.character.health = 31;
    s.character.max_health = 100;
    s.character.stamina = 45;
    s.character.fatigue = 62;
    s.rng.states = vec![("combat".to_string(), 0x00C0_FFEE)];
    states.push(("M22-combat", s));

    let mut s = SaveGameV2::new();
    s.character.spell_points = 12;
    s.character.max_spell_points = 30;
    s.effects.effects = vec![ActiveEffectV2 {
        key: "FIREBALL".to_string(),
        school: 2,
        variants: 1,
        rounds: 3,
        magnitude: 25,
    }];
    states.push(("M23-magic", s));

    let mut s = SaveGameV2::new();
    s.inventory.goods = vec![GoodV2 {
        id: "potion-heal".to_string(),
        quantity: 3,
    }];
    s.inventory.items = vec![ItemV2 {
        group: 3,
        index: 9,
        value: 800,
        flags: 4,
        current_condition: 50,
        max_condition: 50,
        type_dependent: 2,
        material: 4,
        color: 2,
        weight_cu: 5,
        enchantment_points: 60,
        variants: 1,
        draw_order: 0,
        enchantment_slots: 2,
        enchants: vec![(1, 40), (3, 7)],
    }];
    states.push(("M24-enchant", s));

    let mut s = SaveGameV2::new();
    s.rng.states = vec![("encounter".to_string(), 999_983)];
    s.world.region_index = 9;
    s.world.environment = 3;
    s.map.chunks = vec![(2, 2), (2, 3)];
    states.push(("M25-monsters", s));

    let mut s = SaveGameV2::new();
    s.rng.states = vec![("npc".to_string(), 4_242)];
    s.world.location_index = 17;
    s.world.environment = 2;
    states.push(("M26-npcs", s));

    let mut s = SaveGameV2::new();
    s.guilds.guilds = vec![
        GuildMembershipV2 {
            faction: 150,
            rank: 3,
            noted: 1,
            min_last_change: 500,
            blessing: 5,
        },
        GuildMembershipV2 {
            faction: 160,
            rank: 1,
            noted: 0,
            min_last_change: 90,
            blessing: 0,
        },
    ];
    states.push(("M27-guilds", s));

    let mut s = SaveGameV2::new();
    s.crime.crime_committed = 1;
    s.crime.legal_rep = vec![(19, 80), (20, 65)];
    s.crime.prison_days = 3;
    states.push(("M28-crime", s));

    let mut s = SaveGameV2::new();
    s.quests.quests = vec![
        (
            "q-a".to_string(),
            "Encuentra la posada".to_string(),
        ),
        (
            "q-b".to_string(),
            "Habla con el guardia".to_string(),
        ),
    ];
    s.quests.states = vec![
        ("q-a".to_string(), "stage=2".to_string()),
        ("q-b".to_string(), "stage=1".to_string()),
    ];
    states.push(("M29-quests", s));

    let mut s = SaveGameV2::new();
    s.inventory.gold = 9_999;
    s.economy.bank = vec![(19, 5_000), (21, 250)];
    s.economy.loans = vec![LoanV2 {
        region: 19,
        principal: 300,
        due_min: 144_000,
        unpaid: 1,
    }];
    s.economy.property_house = 3;
    states.push(("M30-economy", s));

    let mut s = SaveGameV2::new();
    s.calendar.game_minutes = 600_000;
    s.calendar.weather = [1, 2, 0, 3, 4, 5];
    s.calendar.is_day = 1;
    s.clock = ClockSection {
        year: 406,
        month: 0,
        day: 1,
        hour: 6,
        minute: 30,
        second: 15,
    };
    states.push(("M31-calendar", s));

    let mut s = SaveGameV2::new();
    s.character.fatigue = 55;
    s.effects.diseases = vec![
        DiseaseV2 {
            id: 3,
            days: 5,
            incubation: 1,
            accumulated: [0; DISEASE_ACCUMULATORS],
        },
        DiseaseV2 {
            id: 7,
            days: 1,
            incubation: 0,
            accumulated: [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11],
        },
    ];
    s.effects.vampire_clan = 0;
    s.effects.lycanthropy = 1;
    states.push(("M32-conditions", s));

    // 16-18 · three combined states (multi-module sessions).
    let mut s = SaveGameV2::new();
    s.world.pos_x_mm = 4_000;
    s.world.pos_z_mm = -1_500;
    s.world.region_index = 7;
    s.calendar.game_minutes = 12_345;
    s.calendar.weather = [0, 0, 1, 1, 2, 2];
    s.calendar.is_day = 0;
    s.map.chunks = vec![(0, 0), (0, 1), (1, 0)];
    s.map.automaps = vec![AutomapV2 {
        location: 19,
        blocks: vec![1, 0, 1],
    }];
    states.push(("M19+M31+M18-travel", s));

    let mut s = SaveGameV2::new();
    s.inventory.gold = 1_234;
    s.inventory.goods = vec![GoodV2 {
        id: "trigo".to_string(),
        quantity: 7,
    }];
    s.economy.bank = vec![(19, 900)];
    s.economy.loans = vec![LoanV2 {
        region: 19,
        principal: 100,
        due_min: 2_000,
        unpaid: 0,
    }];
    s.quests.quests = vec![(
        "q-mercado".to_string(),
    "Vende el trigo en el mercado".to_string(),
    )];
    states.push(("M21+M30+M29-trade", s));

    let mut s = SaveGameV2::new();
    s.effects.effects = vec![ActiveEffectV2 {
        key: "LEVITATE".to_string(),
        school: 0,
        variants: 0,
        rounds: 12,
        magnitude: 4,
    }];
    s.inventory.items = vec![ItemV2 {
        group: 2,
        index: 5,
        value: 1_500,
        flags: 2,
        current_condition: 45,
        max_condition: 50,
        type_dependent: 3,
        material: 5,
        color: 4,
        weight_cu: 12,
        enchantment_points: 45,
        variants: 0,
        draw_order: 1,
        enchantment_slots: 1,
        enchants: vec![(2, 25)],
    }];
    s.effects.diseases = vec![DiseaseV2 {
        id: 11,
        days: 2,
        incubation: 3,
        accumulated: [0; DISEASE_ACCUMULATORS],
    }];
    s.effects.vampire_clan = 2;
    states.push(("M23+M24+M32-magic-health", s));

    // 19-24 · six boundary states (census/extremes the modules alone do not
    // exercise: empty document, UTF-8, negatives, max values, clock edge and
    // the `item.<i>.ench` path).
    states.push(("empty", SaveGameV2::new()));

    let mut s = SaveGameV2::sample();
    s.character.name = "Zoë — la Errante (Ñ/ç/中文)".to_string();
    s.quests.quests.push((
        "q-utf8".to_string(),
        "Encuentra el☃ y la llave 中文".to_string(),
    ));
    states.push(("utf8-text", s));

    let mut s = SaveGameV2::new();
    s.world.pos_x_mm = -9_999_999;
    s.world.pos_z_mm = i64::MIN;
    s.world.yaw_mrad = -180_000;
    s.character.health = -5;
    s.crime.legal_rep = vec![(19, -100)];
    states.push(("negative-values", s));

    let mut s = SaveGameV2::new();
    s.world.pos_x_mm = i64::MAX;
    s.clock = ClockSection {
        year: i64::MAX,
        month: i64::MAX,
        day: i64::MAX,
        hour: i64::MAX,
        minute: i64::MAX,
        second: i64::MAX,
    };
    s.character.level = i64::MAX;
    s.character.name = "Máximo".to_string();
    s.character.attributes = [i64::MAX; ATTRIBUTE_COUNT];
    s.character.skills = [i64::MAX; SKILL_COUNT];
    s.inventory.gold = i64::MAX;
    s.inventory.goods = vec![GoodV2 {
        id: "rareza".to_string(),
        quantity: i64::MAX,
    }];
    s.inventory.items = vec![ItemV2 {
        group: i64::MAX,
        index: i64::MAX,
        value: i64::MAX,
        flags: i64::MAX,
        current_condition: i64::MAX,
        max_condition: i64::MAX,
        type_dependent: i64::MAX,
        material: i64::MAX,
        color: i64::MAX,
        weight_cu: i64::MAX,
        enchantment_points: i64::MAX,
        variants: i64::MAX,
        draw_order: i64::MAX,
        enchantment_slots: 1,
        enchants: vec![(i64::MAX, 0)],
    }];
    s.inventory.equipped = {
        let mut slots = [-1i64; EQUIPPED_SLOTS];
        slots[0] = 0;
        slots
    };
    s.inventory.containers = vec![ContainerV2 {
        kind: 8,
        items: vec![0],
    }];
    s.effects.effects = vec![ActiveEffectV2 {
        key: "MAX".to_string(),
        school: i64::MAX,
        variants: i64::MAX,
        rounds: i64::MAX,
        magnitude: i64::MAX,
    }];
    s.effects.diseases = vec![DiseaseV2 {
        id: i64::MAX,
        days: i64::MAX,
        incubation: i64::MAX,
        accumulated: [i64::MAX; DISEASE_ACCUMULATORS],
    }];
    s.effects.vampire_clan = i64::MAX;
    s.effects.lycanthropy = i64::MAX;
    s.guilds.guilds = vec![GuildMembershipV2 {
        faction: i64::MAX,
        rank: i64::MAX,
        noted: i64::MAX,
        min_last_change: i64::MAX,
        blessing: i64::MAX,
    }];
    s.crime.crime_committed = i64::MAX;
    s.crime.legal_rep = vec![(1, i64::MAX)];
    s.crime.prison_days = i64::MAX;
    s.quests.quests = vec![("q-max".to_string(), "Misión máxima".to_string())];
    s.quests.states = vec![("q-max".to_string(), "stage=99".to_string())];
    s.economy.bank = vec![(1, i64::MAX)];
    s.economy.loans = vec![LoanV2 {
        region: 1,
        principal: i64::MAX,
        due_min: i64::MAX,
        unpaid: i64::MAX,
    }];
    s.economy.property_house = i64::MAX;
    s.economy.property_ship = i64::MAX;
    s.calendar.game_minutes = u64::MAX;
    s.calendar.weather = [i64::MAX; WEATHER_COUNT];
    s.calendar.is_day = i64::MAX;
    s.map.chunks = vec![(i64::MIN, i64::MIN)];
    s.map.automaps = vec![AutomapV2 {
        location: i64::MAX,
        blocks: vec![i64::MAX, i64::MIN],
    }];
    s.rng.states = vec![("max".to_string(), u64::MAX)];
    states.push(("max-census", s));

    let mut s = SaveGameV2::new();
    s.clock = ClockSection {
        year: 9_999,
        month: 11,
        day: 29,
        hour: 23,
        minute: 59,
        second: 59,
    };
    s.calendar.game_minutes = u64::MAX - 1;
    s.calendar.is_day = 0;
    states.push(("clock-boundary", s));

    let mut s = SaveGameV2::sample();
    s.inventory.items[0].enchantment_slots = 3;
    s.inventory.items[0].enchants = vec![(1, 100), (2, 200), (3, 300)];
    s.inventory.items[0].enchantment_points = 60;
    states.push(("enchanted-item", s));

    states
}

/// `save → load → save` through the file API: `(equal, identical, lines,
/// bytes)` where `equal` is `load(save(s)) == s` and `identical` is the
/// byte identity of the two writes.
fn roundtrip_state(
    state: &SaveGameV2,
    first: &Path,
    second: &Path,
) -> Result<(bool, bool, usize, usize), String> {
    state
        .save_to_path(first)
        .map_err(|error| format!("save: {error}"))?;
    let bytes_one = fs::read(first).map_err(|error| format!("read: {error}"))?;
    let loaded = SaveGameV2::load_from_path(first).map_err(|error| format!("load: {error}"))?;
    let equal = &loaded == state;
    loaded
        .save_to_path(second)
        .map_err(|error| format!("re-save: {error}"))?;
    let bytes_two = fs::read(second).map_err(|error| format!("re-read: {error}"))?;
    Ok((
        equal,
        bytes_one == bytes_two,
        loaded.line_count(),
        bytes_one.len(),
    ))
}

/// Block A — 24 synthetic states, each with `save→load→save` byte-identical
/// and `load(save(s)) == s`.
fn block_a(workspace: &Path) -> bool {
    println!("\n-- A · roundtrip (24 synthetic states) --");
    let states = synthetic_states();
    println!(
        "composition: baseline=1 modules=M19..M32=14 combined=3 boundary=6 total={}",
        states.len()
    );
    let first = workspace.join("save-all-1.dfl");
    let second = workspace.join("save-all-2.dfl");
    let mut identical = 0usize;
    let mut ok = states.len() == 24;
    for (name, state) in &states {
        match roundtrip_state(state, &first, &second) {
            Ok((equal, same_bytes, lines, bytes)) => {
                if equal && same_bytes {
                    identical += 1;
                    println!("[OK] {name} - lines={lines} bytes={bytes} equal={equal} identical={same_bytes}");
                } else {
                    ok = false;
                    println!("FAIL {name} got equal={equal} identical={same_bytes} want=true/true");
                }
            }
            Err(reason) => {
                ok = false;
                println!("FAIL {name} - {reason}");
            }
        }
    }
    if ok && identical == 24 {
        println!("[OK] roundtrip: 24/24 idénticos");
    } else {
        ok = false;
        println!("[FAIL] roundtrip: {identical}/24 idénticos");
    }
    ok
}

/// Block B — the S0 census against the spec §4 literals.
fn block_b() -> bool {
    println!("\n-- B · S0 census --");
    let doc = SaveGameV2::sample();
    let text = doc.to_text();
    let lines = text.lines().count();
    let bytes = text.len();
    let checksum = text
        .lines()
        .next_back()
        .and_then(|line| line.strip_prefix("checksum="))
        .unwrap_or("");
    let golden = text == S0_TEXT;
    let ok = lines == 85 && bytes == 1705 && checksum == "f793b743974d7f39" && golden;
    if ok {
        println!("[OK] S0 lines={lines} bytes={bytes} checksum={checksum}");
    } else {
        println!(
            "[FAIL] S0 lines={lines} bytes={bytes} checksum={checksum} golden={golden} want lines=85 bytes=1705 checksum=f793b743974d7f39"
        );
    }
    ok
}

/// Block C — five mutations; each one must be rejected (anti-false-positive:
/// the probe never passes by not comparing).
fn block_c() -> bool {
    println!("\n-- C · corruption (5 mutations, each must fail) --");
    // C1 — one byte in `gold`, checksum untouched.
    let mutated_byte = S0_TEXT.replace("gold=55", "gold=65");
    // C2 — unknown key, re-sealed (reaches the closed-key rule).
    let unknown = seal({
        let mut lines = s0_body_lines();
        lines.push("gold_extra=1".to_string());
        lines
    });
    // C3 — duplicated key, re-sealed.
    let duplicated = seal({
        let mut lines = s0_body_lines();
        let at = lines
            .iter()
            .position(|line| line == "gold=55")
            .map(|index| index + 1)
            .unwrap_or(0);
        lines.insert(at, "gold=55".to_string());
        lines
    });
    // C4 — inflated `count`, re-sealed (the third goods line does not exist).
    let inflated = seal({
        let mut lines = s0_body_lines();
        if let Some(line) = lines.iter_mut().find(|line| *line == "goods_count=2") {
            *line = "goods_count=3".to_string();
        }
        lines
    });
    // C5 — final line is not a checksum.
    let no_checksum = {
        let mut lines: Vec<String> = S0_TEXT.lines().map(str::to_string).collect();
        if let Some(last) = lines.last_mut() {
            *last = "not-a-checksum".to_string();
        }
        let mut body = lines.join("\n");
        body.push('\n');
        body
    };

    let mutations = [
        ("C1 gold-byte", mutated_byte),
        ("C2 unknown-key", unknown),
        ("C3 duplicated-key", duplicated),
        ("C4 inflated-count", inflated),
        ("C5 no-checksum-line", no_checksum),
    ];
    let mut detected = 0usize;
    for (name, text) in mutations {
        match SaveGameV2::from_text(&text) {
            Err(error @ (SaveError::Corrupt(_) | SaveError::BadVersion(_) | SaveError::BadMagic)) => {
                detected += 1;
                println!("[OK] {name} - rejected: {error}");
            }
            Err(error) => println!("FAIL {name} - wrong error kind: {error}"),
            Ok(_) => println!("FAIL {name} - the mutation was ACCEPTED (false positive)"),
        }
    }
    if detected == 5 {
        println!("[OK] corrupt: 5/5 (Corrupt|BadVersion|BadMagic)");
        true
    } else {
        println!("[FAIL] corrupt: {detected}/5 (Corrupt|BadVersion|BadMagic)");
        false
    }
}

/// One complete pass over the 24 states (save → load per state), returning
/// the FNV-1a 64 digest of everything the pass saw.
fn full_pass(workspace: &Path, tag: &str) -> Result<u64, String> {
    let states = synthetic_states();
    let path = workspace.join(format!("pass-{tag}.dfl"));
    let mut seen = String::new();
    for (name, state) in &states {
        state
            .save_to_path(&path)
            .map_err(|error| format!("{name} save: {error}"))?;
        let loaded =
            SaveGameV2::load_from_path(&path).map_err(|error| format!("{name} load: {error}"))?;
        if &loaded != state {
            return Err(format!("{name}: load(save(s)) != s"));
        }
        seen.push_str(name);
        seen.push('\n');
        seen.push_str(&loaded.to_text());
    }
    Ok(fnv1a64(seen.as_bytes()))
}

/// Block D — two complete passes must print the same digest.
fn block_d(workspace: &Path) -> bool {
    println!("\n-- D · determinism (2 complete passes) --");
    let (first, second) = match (full_pass(workspace, "1"), full_pass(workspace, "2")) {
        (Ok(first), Ok(second)) => (first, second),
        (first, second) => {
            let reason = first
                .err()
                .or_else(|| second.err())
                .unwrap_or_else(|| "unknown pass error".to_string());
            println!("[FAIL] pass error: {reason}");
            return false;
        }
    };
    if first == second {
        println!("digest={first:016x} ×2 idéntico");
        true
    } else {
        println!("FAIL digest1={first:016x} digest2={second:016x} want identical");
        false
    }
}

/// Block E — escaping of `\ | \n \r` + UTF-8 in `name`/`quest`, plus the
/// GV-06 literal (39 B, `f72794d18c2336d8`).
fn block_e() -> bool {
    println!("\n-- E · escaping (GV-06) --");
    let mut cases: Vec<(&str, SaveGameV2)> = Vec::with_capacity(4);

    let mut s = SaveGameV2::sample();
    s.character.name = "a\\b|c".to_string();
    cases.push(("name-backslash-pipe", s));

    // The GV-06 string: `Título con \ barra y | pipe\ny salto`.
    let mut s = SaveGameV2::sample();
    s.character.name = "Título con \\ barra y | pipe\ny salto".to_string();
    cases.push(("name-utf8-newline", s));

    let mut s = SaveGameV2::sample();
    s.quests.quests.push((
        "q-\\|odd".to_string(),
        "título con | pipe".to_string(),
    ));
    cases.push(("quest-id-odd", s));

    let mut s = SaveGameV2::sample();
    s.quests.quests.push((
        "q-e4".to_string(),
        "Explora\nla caverna — áéíóú ñ".to_string(),
    ));
    cases.push(("quest-title-utf8", s));

    let mut passed = 0usize;
    for (name, state) in &cases {
        let text = state.to_text();
        match SaveGameV2::from_text(&text) {
            Ok(parsed) if &parsed == state && parsed.to_text() == text => {
                passed += 1;
                println!("[OK] {name} - roundtrip identical ({} B)", text.len());
            }
            Ok(parsed) => println!(
                "FAIL {name} - reparse differs equal={}",
                &parsed == state
            ),
            Err(error) => println!("FAIL {name} - {error}"),
        }
    }

    // GV-06: the escaped `name=` value of the second case.
    let text = cases[1].1.to_text();
    let escaped = text
        .lines()
        .find(|line| line.starts_with("name="))
        .map(|line| &line[5..])
        .unwrap_or("");
    let escaped_bytes = escaped.as_bytes();
    let gv06 = escaped_bytes.len() == 39 && fnv1a64(escaped_bytes) == 0xf727_94d1_8c23_36d8;
    println!(
        "[E] GV-06 escaped={} B fnv={:016x} ok={gv06}",
        escaped_bytes.len(),
        fnv1a64(escaped_bytes)
    );
    if passed == 4 && gv06 {
        println!("[OK] escaping: 4/4");
        true
    } else {
        println!("[FAIL] escaping: {passed}/4 gv06={gv06}");
        false
    }
}

/// `PROBE-SAVE-ALL-001` — blocks A-E (spec §6.1).
pub fn run_save_all_001() -> i32 {
    println!(
        "PROBE-SAVE-ALL-001 - M33 total persistence: roundtrip, S0 census, corruption, determinism, escaping (M33, CR-5243af63, REQ-L99-066 · spec §6.1)"
    );
    let workspace = match workspace("save-all") {
        Ok(workspace) => workspace,
        Err(reason) => {
            println!("[EXIT=2] probe workspace: {reason}");
            return 2;
        }
    };
    let a_ok = block_a(&workspace);
    let b_ok = block_b();
    let c_ok = block_c();
    let d_ok = block_d(&workspace);
    let e_ok = block_e();
    let pass = a_ok && b_ok && c_ok && d_ok && e_ok;
    println!(
        "\nSUMMARY PROBE-SAVE-ALL-001: A={} B={} C={} D={} E={} ⇒ {}",
        verdict(a_ok),
        verdict(b_ok),
        verdict(c_ok),
        verdict(d_ok),
        verdict(e_ok),
        verdict(pass)
    );
    if pass {
        0
    } else {
        1
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// PROBE-SAVE-MIGRATION-001
// ═══════════════════════════════════════════════════════════════════════════

/// Fixture (i) — the v1 sample of the spec GV-05 (442 B payload,
/// `checksum=1e619ee6ebcc7685`): the `session_store` test sample as the spec
/// measured it (Spanish ids/titles, pre-i18n test data).
const V1_SAMPLE: &str = r"DFL-SESSION 1
pos_x_mm=1500
pos_z_mm=-2000
yaw_mrad=314
clock_year=405
clock_month=5
clock_day=0
clock_hour=13
clock_minute=1
clock_second=1
level=2
experience=50
health=70
max_health=100
stamina=100
max_stamina=100
gold=55
goods_count=2
goods=pieles|1
goods=pocion|1
quests_count=2
quest=q-tienda-vendel|Localiza la tienda de Vendel
quest=q-caverna|Explora la caverna al este
chunks_count=5
chunk=0|0
chunk=0|1
chunk=1|0
chunk=1|1
chunk=2|0
checksum=1e619ee6ebcc7685
";

/// Fixture (ii) — the same v1 sample with `goods_count=0` (§5(d): empty list).
const V1_NO_GOODS: &str = r"DFL-SESSION 1
pos_x_mm=1500
pos_z_mm=-2000
yaw_mrad=314
clock_year=405
clock_month=5
clock_day=0
clock_hour=13
clock_minute=1
clock_second=1
level=2
experience=50
health=70
max_health=100
stamina=100
max_stamina=100
gold=55
goods_count=0
quests_count=2
quest=q-tienda-vendel|Localiza la tienda de Vendel
quest=q-caverna|Explora la caverna al este
chunks_count=5
chunk=0|0
chunk=0|1
chunk=1|0
chunk=1|1
chunk=2|0
checksum=17d39163d0aec995
";

/// Fixture (iii) — the same v1 sample with escaped text in a quest
/// (`q-|odd` id and a title with `\` and a newline, escaped in the file).
const V1_ESCAPED: &str = r"DFL-SESSION 1
pos_x_mm=1500
pos_z_mm=-2000
yaw_mrad=314
clock_year=405
clock_month=5
clock_day=0
clock_hour=13
clock_minute=1
clock_second=1
level=2
experience=50
health=70
max_health=100
stamina=100
max_stamina=100
gold=55
goods_count=2
goods=pieles|1
goods=pocion|1
quests_count=2
quest=q-tienda-vendel|Localiza la tienda de Vendel
quest=q-\podd|Explora la caverna\\al este\ny sigue
chunks_count=5
chunk=0|0
chunk=0|1
chunk=1|0
chunk=1|1
chunk=2|0
checksum=a82a7e5764370bd4
";

/// `PROBE-SAVE-MIGRATION-001` — 3 embedded fixtures (spec §6.2).
pub fn run_save_migration_001() -> i32 {
    println!(
        "PROBE-SAVE-MIGRATION-001 - M33 v1→v2 migration: 3 embedded fixtures, idempotence, digest ×2 (M33, CR-5243af63, REQ-L99-066 · spec §5, §6.2)"
    );
    let fixtures = [
        ("fixture-i-sample", V1_SAMPLE),
        ("fixture-ii-no-goods", V1_NO_GOODS),
        ("fixture-iii-escaped", V1_ESCAPED),
    ];

    // GV-05 census over fixture (i): payload before the checksum line.
    let payload = V1_SAMPLE
        .lines()
        .take(V1_SAMPLE.lines().count().saturating_sub(1))
        .fold(String::new(), |mut body, line| {
            body.push_str(line);
            body.push('\n');
            body
        });
    let v1_checksum = V1_SAMPLE
        .lines()
        .next_back()
        .and_then(|line| line.strip_prefix("checksum="))
        .unwrap_or("");
    let gv05 = payload.len() == 442 && v1_checksum == "1e619ee6ebcc7685";
    if gv05 {
        println!("[OK] GV-05 v1: payload={} B checksum={v1_checksum}", payload.len());
    } else {
        println!(
            "[FAIL] GV-05 v1: payload={} B checksum={v1_checksum} want payload=442 B checksum=1e619ee6ebcc7685",
            payload.len()
        );
    }

    let mut migrated: Vec<String> = Vec::with_capacity(3);
    let mut migrated_ok = 0usize;
    let mut idempotent_ok = 0usize;
    let mut clock_only_ok = true;
    println!("\n-- migration (v1 → v2 → v2) --");
    for (name, fixture) in fixtures {
        let v2 = match migrate_text(fixture) {
            Ok(v2) => v2,
            Err(error) => {
                println!("FAIL {name} - valid v1 rejected: {error}");
                continue;
            }
        };
        let parsed = match SaveGameV2::from_text(&v2) {
            Ok(parsed) => parsed,
            Err(error) => {
                println!("FAIL {name} - migrated v2 does not re-parse: {error}");
                continue;
            }
        };
        if parsed.calendar.game_minutes != 0 {
            clock_only_ok = false;
        }
        let again = match migrate_text(&v2) {
            Ok(again) => again,
            Err(error) => {
                println!("FAIL {name} - v2→v2 migration failed: {error}");
                continue;
            }
        };
        let idempotent = again == v2;
        migrated_ok += 1;
        if idempotent {
            idempotent_ok += 1;
        }
        let lines = v2.lines().count();
        let checksum = v2
            .lines()
            .next_back()
            .and_then(|line| line.strip_prefix("checksum="))
            .unwrap_or("");
        println!(
            "[OK] {name} - v1→v2 ok v2 parses idempotent={idempotent} lines={lines} bytes={} checksum={checksum}",
            v2.len()
        );
        migrated.push(v2);
    }

    // GV-09 — census of fixture (i) migrated: HARD GATE against the
    // corrected literal (spec §6.2 corrección #23 — the pre-(18) literal
    // lacked `vampire_clan=0` + `lycanthropy=0` = +2 lines / +29 B).
    let mut gv09 = false;
    if let Some(v2) = migrated.first() {
        let lines = v2.lines().count();
        let bytes = v2.len();
        let checksum = v2
            .lines()
            .next_back()
            .and_then(|line| line.strip_prefix("checksum="))
            .unwrap_or("");
        gv09 = lines == 73 && bytes == 1324 && checksum == "1cc138db8a7e7c95";
        println!(
            "[{}] GV-09: lines={lines} bytes={bytes} checksum={checksum} (want 73/1324/1cc138db8a7e7c95 · corrección #23)",
            verdict(gv09)
        );
    }

    println!(
        "[DECLARED] migrated_clock_only=1 game_minutes=0 (§5(g): only clock_* migrates; the absolute date is never invented)"
    );

    let digest_one = migrated.iter().fold(String::new(), |mut acc, text| {
        acc.push_str(text);
        acc
    });
    let digest_one = fnv1a64(digest_one.as_bytes());
    let digest_two = migrated.iter().fold(String::new(), |mut acc, text| {
        acc.push_str(text);
        acc
    });
    let digest_two = fnv1a64(digest_two.as_bytes());
    let digest_ok = migrated.len() == 3 && digest_one == digest_two;
    let pass = migrated_ok == 3 && idempotent_ok == 3 && clock_only_ok && gv05 && gv09 && digest_ok;
    if migrated_ok == 3 && idempotent_ok == 3 && gv09 && digest_ok {
        println!(
            "[OK] migration: 3/3 v1→v2 · v2→v2 idempotente 3/3 · GV-09 3/3 · digest={digest_one:016x} ×2"
        );
    } else {
        println!(
            "[FAIL] migration: {migrated_ok}/3 v1→v2 · v2→v2 idempotente {idempotent_ok}/3 · GV-09 {} · digest1={digest_one:016x} digest2={digest_two:016x}",
            verdict(gv09)
        );
    }
    println!(
        "\nSUMMARY PROBE-SAVE-MIGRATION-001: migrated={} idempotent={} GV-05={} GV-09={} clock_only={} digest×2={} ⇒ {}",
        verdict(migrated_ok == 3),
        verdict(idempotent_ok == 3),
        verdict(gv05),
        verdict(gv09),
        verdict(clock_only_ok),
        verdict(digest_ok),
        verdict(pass)
    );
    if pass {
        0
    } else {
        1
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// PROBE-SAVEGAME-READ-001 (Mode A, read-only)
// ═══════════════════════════════════════════════════════════════════════════

/// The classic save files the spec §2.4 N2/N3 census names, in report order.
const CLASSIC_SAVE_FILES: [&str; 3] = ["SAVETREE.DAT", "SAVEVARS.DAT", "IMAGE.RAW"];

/// Declared state of **one** classic save file.
///
/// The three states are the point of CR-28fb1784 / F-AUD-03: a file that
/// exists is no longer a FAIL by itself, and the absence of a source of truth
/// is not allowed to look like a PASS or like a FAIL.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClassicSaveState {
    /// (a) Not in ARENA2 — the spec §2.4 N2/N3 declaration holds as written.
    Absent,
    /// (b) Present **and** compared against the source that corresponds to it:
    /// the comparison, not the mere presence, decides the verdict.
    Compared {
        /// Whether the documented layout matched.
        ok: bool,
        /// Raw comparison facts (`version`, buildings, records, framing).
        detail: String,
    },
    /// (c) Present with no source of truth to compare against: declared
    /// `NOT-COMPARABLE` — a `SKIP` with its literal reason, never a FAIL for
    /// the missing authority and never a silent PASS.
    NotComparable {
        /// Why the comparison cannot be established.
        reason: String,
    },
}

impl ClassicSaveState {
    /// Whether the state blocks the probe (exit 1).
    ///
    /// Only a **comparable** file that does not match its source blocks:
    /// `Absent` is the declared NEGATIVE and `NotComparable` is a declared
    /// `SKIP`, so neither one is a FAIL.
    fn blocking(&self) -> bool {
        matches!(self, Self::Compared { ok: false, .. })
    }

    /// One line of the deterministic digest input (no path, no clock).
    fn row(&self, name: &str) -> String {
        match self {
            Self::Absent => format!("{name}:absent"),
            Self::Compared { ok, detail } => format!("{name}:compared:{ok}:{detail}"),
            Self::NotComparable { reason } => format!("{name}:not-comparable:{reason}"),
        }
    }
}

/// `(a)`/`(b)`/`(c)` — classifies one classic save file from what is on disk.
///
/// `present = None` means the file is not in ARENA2 (state `(a)`); `Some`
/// carries the bytes read from it, which are classified as `(b)` when a source
/// of truth corresponds to the name and as `(c)` when it does not.
fn classic_save_state(name: &str, present: Option<&[u8]>) -> ClassicSaveState {
    let Some(bytes) = present else {
        return ClassicSaveState::Absent;
    };
    match classic_save_comparator(name) {
        Some(compare) => compare(bytes),
        None => ClassicSaveState::NotComparable {
            reason: "no source of truth in the project for this file (M39 is blocked by the retail \
                     files and spec §2.4 only declares its absence)"
                .to_string(),
        },
    }
}

/// The source of truth that corresponds to `name`, if the project has one.
///
/// `SAVETREE.DAT` does: the documented classic framing the `savegame` module
/// derives from (`SaveTreeHeader.cs:24-25,59-81`, `SaveTreeBaseRecord.cs:26,
/// 220-283`, `SaveTree.cs:212-270`), so its comparison is **structural** and
/// claims no semantic compatibility (spec §2.4 N1). `SAVEVARS.DAT` and
/// `IMAGE.RAW` do not: nothing in the project derives their layout, so they
/// declare `NOT-COMPARABLE` instead of inventing a verdict.
fn classic_save_comparator(name: &str) -> Option<fn(&[u8]) -> ClassicSaveState> {
    match name {
        "SAVETREE.DAT" => Some(compare_save_tree_layout),
        _ => None,
    }
}

/// Raw structural facts of a `SAVETREE.DAT` under the documented framing.
struct SaveTreeFraming {
    /// Header version word (`SaveTreeHeader.cs:24`).
    version: i32,
    /// `recordLength / 26` building records.
    buildings: usize,
    /// Framed records (`[i32 length][71 B RecordRoot][data]`).
    records: usize,
    /// Padding fields (`length <= 0`) the source skips (`SaveTree.cs:219-225`).
    padding: usize,
    /// Bytes the walk accounted for.
    consumed: usize,
    /// Bytes in the file.
    bytes: usize,
    /// A record length that runs past the end of the file.
    truncated: bool,
    /// A record shorter than the 71 B root.
    short_root: bool,
}

/// Walks the documented framing (`SaveTree.cs:212-270`): 19 B header,
/// `i32 recordLength` buildings block of `26·N`, then self-delimiting
/// `[i32 length][71 B RecordRoot][data]` records.
///
/// `None` when the file is too short for the header and the buildings length,
/// or when that length is negative / not a multiple of `26` — cases the
/// project reader already rejects, so the caller has declared them
/// `NOT-COMPARABLE` before reaching this walk.
fn save_tree_framing(bytes: &[u8]) -> Option<SaveTreeFraming> {
    let word = |at: usize| {
        bytes
            .get(at..at + 4)
            .map(|slice| i32::from_le_bytes([slice[0], slice[1], slice[2], slice[3]]))
    };
    let version = word(0)?;
    let buildings_start = SAVETREE_HEADER_BYTES;
    let record_length = word(buildings_start)?;
    if record_length < 0 || !(record_length as usize).is_multiple_of(BUILDING_BYTES) {
        return None;
    }
    let record_length = record_length as usize;
    let mut framing = SaveTreeFraming {
        version,
        buildings: record_length / BUILDING_BYTES,
        records: 0,
        padding: 0,
        consumed: 0,
        bytes: bytes.len(),
        truncated: false,
        short_root: false,
    };
    let mut at = buildings_start + 4 + record_length;
    if at > bytes.len() {
        framing.truncated = true;
        framing.consumed = bytes.len();
        return Some(framing);
    }
    while at + 4 <= bytes.len() {
        let Some(length) = word(at) else { break };
        if length <= 0 {
            framing.padding += 1;
            at += 4;
            continue;
        }
        let length = length as usize;
        if at + 4 + length > bytes.len() {
            framing.truncated = true;
            break;
        }
        if length < RECORD_ROOT_BYTES {
            framing.short_root = true;
            break;
        }
        framing.records += 1;
        at += 4 + length;
    }
    framing.consumed = at;
    Some(framing)
}

/// `(b)` — compares a present `SAVETREE.DAT` against the documented framing.
///
/// The layout is the source that corresponds to this file; the comparison is
/// structural (header version `0x126`, buildings block, record framing that
/// covers the file exactly) and makes **no** semantic claim (§2.4 N1). Our own
/// reader is a cross-check, not the authority: when it rejects the file the
/// state degrades to `NOT-COMPARABLE` with its reason, because the module
/// declares no compatibility and a rejection would be a FAIL for the wrong
/// reason.
fn compare_save_tree_layout(bytes: &[u8]) -> ClassicSaveState {
    if let Err(error) = SaveTree::decode(bytes) {
        return ClassicSaveState::NotComparable {
            reason: format!(
                "the project reader rejects it and claims no compatibility (spec §2.4 N1): {error}"
            ),
        };
    }
    let Some(framing) = save_tree_framing(bytes) else {
        return ClassicSaveState::NotComparable {
            reason: "the documented framing cannot be walked (header or buildings length)"
                .to_string(),
        };
    };
    let version_ok = framing.version == SAVETREE_VERSION;
    let cover_ok = framing.consumed == framing.bytes;
    ClassicSaveState::Compared {
        ok: version_ok && cover_ok && !framing.truncated && !framing.short_root,
        detail: format!(
            "version={:#x} version_ok={version_ok} buildings={} records={} padding={} \
             framing={}B bytes={} covered={cover_ok} truncated={} short_root={}",
            framing.version,
            framing.buildings,
            framing.records,
            framing.padding,
            framing.consumed,
            framing.bytes,
            framing.truncated,
            framing.short_root
        ),
    }
}

/// `PROBE-SAVEGAME-READ-001` — retail `MAPSAVE.SAV` layout, `SAVE00I0.IMG`
/// header and the classic census: absent (`(a)`), present and compared (`(b)`)
/// or present and `NOT-COMPARABLE` (`(c)`) per file (spec §6.3).
pub fn run_savegame_read_001() -> i32 {
    println!(
        "PROBE-SAVEGAME-READ-001 - classic SAVEGAME read (Mode A, read-only): MAPSAVE.SAV layout + IMG header + classic census absent/compared/not-comparable (M33, CR-5243af63 · CR-28fb1784, REQ-L99-066 · spec §6.3)"
    );
    // ARENA2 inaccessible ⇒ EXIT=2 with the literal reason, never a false PASS.
    if let Err(reason) = crate::mode_a_arena2_dir() {
        println!("[EXIT=2] ARENA2 not accessible: {reason}");
        return 2;
    }
    let mapsave_path = match crate::mode_a_arena2_path("MAPSAVE.SAV") {
        Ok(path) => path,
        Err(reason) => {
            println!("[EXIT=2] ARENA2 not accessible: {reason}");
            return 2;
        }
    };
    let bytes = match fs::read(&mapsave_path) {
        Ok(bytes) => bytes,
        Err(error) => {
            println!("[EXIT=2] ARENA2 not accessible: read {}: {error}", mapsave_path.display());
            return 2;
        }
    };
    println!("\n-- DISK · MAPSAVE.SAV --");
    println!("[READ] path={} bytes={}", mapsave_path.display(), bytes.len());
    let map = match MapSave::decode(&bytes) {
        Ok(map) => map,
        Err(error) => {
            println!("[FAIL] MAPSAVE decode: {error}");
            println!("\nSUMMARY PROBE-SAVEGAME-READ-001: mapsave=FAIL ⇒ FAIL");
            return 1;
        }
    };
    let count = map.entries.len();
    let data = map.data_bytes();
    let (found, total) = map.discovered();
    let formula = 4 + data + count * MAPSAVE_DIR_ENTRY_BYTES;
    let formula_ok = formula == bytes.len();
    let dir_ok = map.dir_type == MAPSAVE_DIR_TYPE;
    let names_ok = map
        .entries
        .iter()
        .enumerate()
        .all(|(index, entry)| entry.name_str() == format!("MAPSAVE.{index:03}"));
    println!("[READ] formula 4+{data}+{count}*{MAPSAVE_DIR_ENTRY_BYTES}={formula} bytes={} ok={formula_ok}", bytes.len());
    println!(
        "[READ] dir_type={:#06x} count={count} names=MAPSAVE.000..{:03} ok={dir_ok} names_ok={names_ok}",
        map.dir_type,
        count.saturating_sub(1)
    );
    let per_region = map
        .entries
        .iter()
        .map(|entry| {
            format!(
                "{}={}/{}",
                entry.name_str(),
                entry.discovered(),
                entry.data.len()
            )
        })
        .collect::<Vec<_>>()
        .join(" ");
    println!("[READ] discovered_by_region {per_region}");

    println!("\n-- DISK · SAVE00I0.IMG --");
    let img_ok = match crate::mode_a_arena2_path("SAVE00I0.IMG") {
        Err(reason) => {
            println!("[EXIT=2] ARENA2 not accessible: {reason}");
            return 2;
        }
        Ok(path) => match fs::read(&path) {
            Err(error) => {
                println!("[EXIT=2] ARENA2 not accessible: read {}: {error}", path.display());
                return 2;
            }
            Ok(img) => match ImgHeader::parse(&img) {
                Ok(header) => {
                    let ok = img.len() == 760
                        && header.x_offset == 126
                        && header.y_offset == 5
                        && header.width == 68
                        && header.height == 11;
                    println!(
                        "[READ] SAVE00I0.IMG bytes={} header=x,{},y,{},w,{},h,{} compression={:#06x} pixel_data={} ok={ok}",
                        img.len(),
                        header.x_offset,
                        header.y_offset,
                        header.width,
                        header.height,
                        header.compression,
                        header.pixel_data_len_u16()
                    );
                    ok
                }
                Err(error) => {
                    println!("[FAIL] SAVE00I0.IMG header: {error:?}");
                    false
                }
            },
        },
    };

    // Classic census (spec §2.4 N2/N3) — three declared states per file
    // (CR-28fb1784, F-AUD-03): absent ⇒ NEGATIVE as written, present and
    // comparable ⇒ compared against its source, present with no source of
    // truth ⇒ NOT-COMPARABLE (declared SKIP). Presence alone is never a FAIL.
    println!("\n-- CLASSIC (spec §2.4 N2/N3) --");
    let mut classic_blocking = false;
    let mut absent = 0usize;
    let mut compared = 0usize;
    let mut not_comparable = 0usize;
    let mut rows: Vec<String> = Vec::with_capacity(CLASSIC_SAVE_FILES.len());
    for name in CLASSIC_SAVE_FILES {
        let present: Option<Vec<u8>> = match crate::mode_a_arena2_path(name) {
            Err(_) => None,
            Ok(path) => match fs::read(&path) {
                Ok(bytes) => Some(bytes),
                Err(error) => {
                    println!("[EXIT=2] ARENA2 not accessible: read {}: {error}", path.display());
                    return 2;
                }
            },
        };
        let state = classic_save_state(name, present.as_deref());
        match &state {
            ClassicSaveState::Absent => {
                absent += 1;
                println!("[NEGATIVE] {name}: ausente");
            }
            ClassicSaveState::Compared { ok: true, detail } => {
                compared += 1;
                println!(
                    "[COMPARED] {name}: presente — el layout documentado cumple ({detail}) ⇒ PASS estructural (0 claim de compatibilidad semántica, §2.4 N1)"
                );
            }
            ClassicSaveState::Compared { ok: false, detail } => {
                compared += 1;
                println!(
                    "[FAIL] {name}: presente y comparable — el layout documentado NO cumple ({detail})"
                );
            }
            ClassicSaveState::NotComparable { reason } => {
                not_comparable += 1;
                let bytes = present.as_ref().map_or(0, Vec::len);
                println!(
                    "[NOT-COMPARABLE] {name}: presente ({bytes} B) — {reason} ⇒ SKIP declarado (suma 0 al veredicto: ni PASS ni FAIL)"
                );
            }
        }
        classic_blocking |= state.blocking();
        rows.push(state.row(name));
    }
    let classic_digest = fnv1a64(rows.join("\n").as_bytes());
    println!(
        "[DIGEST] classic_list={classic_digest:016x} absent={absent} compared={compared} not_comparable={not_comparable}"
    );
    let classic_clause = if absent == CLASSIC_SAVE_FILES.len() {
        "[SKIP] savetree/savevars/image: ausentes (NEGATIVE N2/N3)".to_string()
    } else {
        format!(
            "classic: absent={absent} compared={compared} not_comparable={not_comparable} digest={classic_digest:016x}"
        )
    };

    let ok = count == 62
        && data == 15_286
        && formula_ok
        && dir_ok
        && names_ok
        && img_ok
        && !classic_blocking;
    if ok {
        println!(
            "[OK] mapsave: {count} records, {data} data bytes, discovered={found} de {total} · {classic_clause}"
        );
    } else {
        println!(
            "[FAIL] mapsave: count={count} data={data} formula={formula_ok} dir={dir_ok} names={names_ok} img={img_ok} classic_blocking={classic_blocking}"
        );
    }
    let classic_verdict = if classic_blocking {
        "FAIL"
    } else if not_comparable > 0 {
        "declared(SKIP)"
    } else {
        "PASS"
    };
    // A declared SKIP is never reported as a plain PASS: the overall word
    // carries the degradation, so no reader can mistake it for a comparison
    // that happened (CR-28fb1784, F-AUD-03).
    let overall = match (ok, not_comparable) {
        (false, _) => "FAIL",
        (true, 0) => "PASS",
        (true, _) => "PASS(declared SKIP)",
    };
    println!(
        "\nSUMMARY PROBE-SAVEGAME-READ-001: mapsave={} img={} classic={classic_verdict} ⇒ {overall}",
        verdict(count == 62 && data == 15_286 && formula_ok && dir_ok && names_ok),
        verdict(img_ok)
    );
    if ok {
        0
    } else {
        1
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// PROBE-SAVEGAME-WRITE-001
// ═══════════════════════════════════════════════════════════════════════════

/// Synthetic `SAVETREE.DAT`: 19 B header (version `0x126`), a buildings block
/// of `recordLength = 26·10` and three framed records (`[i32 length][71 B
/// RecordRoot][data]`), including the GV-12 shape `71+107`.
fn synthetic_save_tree() -> SaveTree {
    let buildings: Vec<BuildingRecord> = (0..10u16)
        .map(|index| BuildingRecord {
            name_seed: 0x1000 + index,
            service_time_limit: 600,
            reserved_1: 0,
            reserved_2: 0,
            reserved_3: 0,
            reserved_4: 0,
            faction_id: 150,
            sector: -1,
            location_id: 7,
            building_type: 3,
            quality: 2,
        })
        .collect();
    let records = vec![
        SaveTreeRecord {
            root: RecordRoot {
                record_type: 0x02,
                record_id: 1,
                time: 42,
                x: 1_000,
                ..RecordRoot::default()
            },
            data: vec![0xA5; 107],
        },
        SaveTreeRecord {
            root: RecordRoot {
                record_type: 0x03,
                record_id: 2,
                ..RecordRoot::default()
            },
            data: Vec::new(),
        },
        SaveTreeRecord {
            root: RecordRoot {
                record_type: 0x0a,
                record_id: 3,
                ..RecordRoot::default()
            },
            data: vec![1, 2, 3, 4],
        },
    ];
    SaveTree {
        header: SaveTreeHeader {
            version: SAVETREE_VERSION,
            x: 1_000,
            y: -20,
            z: 2_500,
            map_id: 19,
            environment: 1,
        },
        buildings,
        records,
    }
}

/// `PROBE-SAVEGAME-WRITE-001` — synthetic round-trips through our own
/// reader/writer, with the original's acceptance declared NEGATIVE (§6.4).
pub fn run_savegame_write_001() -> i32 {
    println!(
        "PROBE-SAVEGAME-WRITE-001 - classic SAVEGAME write: synthetic SAVETREE + MAPSAVE copy round-trip through our reader (M33, CR-5243af63, REQ-L99-066 · spec §6.4)"
    );
    let workspace = match workspace("save-write") {
        Ok(workspace) => workspace,
        Err(reason) => {
            println!("[EXIT=2] writable destination: {reason}");
            return 2;
        }
    };

    // ── synthetic SAVETREE.DAT: write → read with OUR reader ──────────────
    let tree = synthetic_save_tree();
    let bytes = tree.encode();
    let path = workspace.join("SAVETREE.DAT");
    if let Err(error) = fs::write(&path, &bytes) {
        println!("[EXIT=2] writable destination: write {}: {error}", path.display());
        return 2;
    }
    let read_back = match fs::read(&path) {
        Ok(read_back) => read_back,
        Err(error) => {
            println!("[EXIT=2] writable destination: read {}: {error}", path.display());
            return 2;
        }
    };
    let decoded = match SaveTree::decode(&read_back) {
        Ok(decoded) => decoded,
        Err(error) => {
            println!("[FAIL] SAVETREE decode: {error}");
            println!("\nSUMMARY PROBE-SAVEGAME-WRITE-001: savetree=FAIL ⇒ FAIL");
            return 1;
        }
    };
    let original = decoded == tree;
    let reencoded = decoded.encode() == bytes;
    let record_length = tree.buildings.len() * BUILDING_BYTES;
    println!("\n-- synthetic SAVETREE.DAT --");
    println!(
        "[WRITE] bytes={} header={SAVETREE_HEADER_BYTES}B version={:#x} buildings={} recordLength={record_length} records={}",
        bytes.len(),
        decoded.header.version,
        tree.buildings.len(),
        tree.records.len()
    );
    for (index, record) in tree.records.iter().enumerate() {
        println!(
            "[WRITE] record[{index}] framed={} root={RECORD_ROOT_BYTES} data={}",
            record.framed_len(),
            record.data.len()
        );
    }
    println!(
        "[WRITE] roundtrip decoded==original={original} re-encode identical={reencoded}"
    );
    let savetree_ok = original && reencoded && record_length == 26 * 10;

    // ── MAPSAVE copy: retail read (Mode A) → our copy → our reader ────────
    println!("\n-- MAPSAVE copy --");
    let mut mapsave_ok = true;
    match crate::mode_a_arena2_path("MAPSAVE.SAV") {
        Ok(source) => match fs::read(&source) {
            Err(error) => {
                mapsave_ok = false;
                println!("[FAIL] mapsave copy read: {error}");
            }
            Ok(retail) => {
                let source_map = match MapSave::decode(&retail) {
                    Ok(map) => map,
                    Err(error) => {
                        println!("[FAIL] mapsave source decode: {error}");
                        MapSave::default()
                    }
                };
                let copy_path = workspace.join("MAPSAVE.SAV");
                if let Err(error) = fs::write(&copy_path, &retail) {
                    mapsave_ok = false;
                    println!("[FAIL] mapsave copy write: {error}");
                } else {
                    let copy_bytes = fs::read(&copy_path).unwrap_or_default();
                    match MapSave::decode(&copy_bytes) {
                        Ok(copy) => {
                            let count = copy.entries.len();
                            let names_ok = copy
                                .entries
                                .iter()
                                .enumerate()
                                .all(|(index, entry)| {
                                    entry.name_str() == format!("MAPSAVE.{index:03}")
                                });
                            let identical = copy.encode() == copy_bytes && copy == source_map;
                            println!(
                                "[WRITE] MAPSAVE copy: count={count} dir_type={:#06x} names_ok={names_ok} identical={identical} (retail {} B read-only, copy in temp)",
                                copy.dir_type,
                                retail.len()
                            );
                            mapsave_ok = count == 62 && names_ok && identical;
                        }
                        Err(error) => {
                            mapsave_ok = false;
                            println!("[FAIL] mapsave copy decode: {error}");
                        }
                    }
                }
            }
        },
        Err(reason) => println!(
            "[SKIP] mapsave copy: ARENA2 not accessible: {reason} (the synthetic SAVETREE roundtrip still gates EXIT)"
        ),
    }

    println!(
        "\n[DECLARED] aceptación por el original: NEGATIVE (sin binario 1.07 en el host)"
    );
    println!(
        "[DECLARED] light-record rule (type 0x07, streamLength×39): not implemented — our reader decodes the stored length (savegame.rs declared limit)"
    );

    let pass = savetree_ok && mapsave_ok;
    println!(
        "\nSUMMARY PROBE-SAVEGAME-WRITE-001: savetree={} mapsave_copy={} ⇒ {}",
        verdict(savetree_ok),
        verdict(mapsave_ok),
        verdict(pass)
    );
    if pass {
        0
    } else {
        1
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// M33-DEMO-001 (DEMO-033, spec §9)
// ═══════════════════════════════════════════════════════════════════════════

/// Advances a six-field clock by `minutes` (mod 60 / 24 / 30-day month kept
/// simple: only the fields the demo needs can roll over).
fn advance_clock(clock: &mut ClockSection, minutes: i64) {
    let total = clock.minute + minutes;
    clock.minute = total % 60;
    clock.hour += total / 60;
    if clock.hour >= 24 {
        clock.hour %= 24;
        clock.day += 1;
    }
}

/// `M33-DEMO-001` — DEMO-033 headless (spec §9): create, advance 10 game
/// minutes, save, mutate gold/HP/quest/guild, load ⇒ the snapshot returns
/// exactly and `save→load→save` is byte-identical. 0 UI lines.
pub fn run_m33_demo_001() -> i32 {
    println!(
        "PROBE-M33-DEMO-001 - DEMO-033 headless: save → mutate → load restores the snapshot exactly (M33, CR-5243af63, REQ-L99-066 · spec §9)"
    );
    let workspace = match workspace("demo") {
        Ok(workspace) => workspace,
        Err(reason) => {
            println!("[EXIT=2] probe workspace: {reason}");
            return 2;
        }
    };
    let path = workspace.join("demo-033.dfl");
    let path_again = workspace.join("demo-033-again.dfl");

    // ── create + advance 10 game minutes ──────────────────────────────────
    let mut doc = SaveGameV2::sample();
    let minutes_before = doc.calendar.game_minutes;
    doc.calendar.game_minutes = minutes_before + 10;
    let clock_before = doc.clock;
    advance_clock(&mut doc.clock, 10);
    println!(
        "[DEMO-033] advance: game_minutes={minutes_before}→{} clock={:02}:{:02}→{:02}:{:02} (10 min)",
        doc.calendar.game_minutes,
        clock_before.hour,
        clock_before.minute,
        doc.clock.hour,
        doc.clock.minute
    );

    // ── save (the snapshot everything must come back to) ──────────────────
    let snapshot = doc.clone();
    if let Err(error) = doc.save_to_path(&path) {
        println!("[FAIL] save: {error}");
        println!("\nSUMMARY PROBE-M33-DEMO-001: save=FAIL ⇒ FAIL");
        return 1;
    }
    let saved_bytes = fs::read(&path).unwrap_or_default();
    println!(
        "[DEMO-033] saved: {} bytes={} checksum={}",
        path.display(),
        saved_bytes.len(),
        snapshot
            .to_text()
            .lines()
            .next_back()
            .unwrap_or("")
    );

    // ── mutate the world (gold, HP, quest, guild) ─────────────────────────
    doc.inventory.gold = 9_999;
    doc.character.health = 1;
    doc.quests.quests.pop();
    doc.guilds.guilds.push(GuildMembershipV2 {
        faction: 200,
        rank: 1,
        noted: 0,
        min_last_change: 10,
        blessing: 0,
    });
    println!(
        "[DEMO-033] mutated: gold={} health={} quests_count={} guilds_count={}",
        doc.inventory.gold,
        doc.character.health,
        doc.quests.quests.len(),
        doc.guilds.guilds.len()
    );

    // ── load: the state must come back exactly to the snapshot ────────────
    let loaded = match SaveGameV2::load_from_path(&path) {
        Ok(loaded) => loaded,
        Err(error) => {
            println!("[FAIL] load: {error}");
            println!("\nSUMMARY PROBE-M33-DEMO-001: load=FAIL ⇒ FAIL");
            return 1;
        }
    };
    let guild_render = |guild: &GuildMembershipV2| {
        format!(
            "{},{},{},{},{}",
            guild.faction, guild.rank, guild.noted, guild.min_last_change, guild.blessing
        )
    };
    let mut restore_ok = true;
    let checks = [
        (
            "gold",
            snapshot.inventory.gold,
            loaded.inventory.gold,
        ),
        (
            "health",
            snapshot.character.health,
            loaded.character.health,
        ),
        (
            "quests_count",
            snapshot.quests.quests.len() as i64,
            loaded.quests.quests.len() as i64,
        ),
    ];
    for (field, saved, restored) in checks {
        let ok = saved == restored;
        restore_ok &= ok;
        println!("[DEMO-033] {field}: saved={saved} loaded={restored} ok={ok}");
    }
    let saved_guild = snapshot.guilds.guilds.first().map(guild_render);
    let loaded_guild = loaded.guilds.guilds.first().map(guild_render);
    let guild_ok = saved_guild == loaded_guild && saved_guild.is_some();
    restore_ok &= guild_ok;
    println!(
        "[DEMO-033] guild.0: saved={} loaded={} ok={guild_ok}",
        saved_guild.as_deref().unwrap_or("-"),
        loaded_guild.as_deref().unwrap_or("-")
    );
    let doc_ok = loaded == snapshot && loaded.to_text() == snapshot.to_text();
    restore_ok &= doc_ok;
    println!("[DEMO-033] doc: loaded==snapshot={doc_ok}");

    // ── save → load → save byte-identical ─────────────────────────────────
    if let Err(error) = loaded.save_to_path(&path_again) {
        println!("[FAIL] re-save: {error}");
        println!("\nSUMMARY PROBE-M33-DEMO-001: restore={} byte_identical=FAIL ⇒ FAIL", verdict(restore_ok));
        return 1;
    }
    let again_bytes = fs::read(&path_again).unwrap_or_default();
    let digest_one = fnv1a64(&saved_bytes);
    let digest_two = fnv1a64(&again_bytes);
    let identical = saved_bytes == again_bytes && digest_one == digest_two;
    println!(
        "[DEMO-033] save→load→save byte-identical: digest={digest_one:016x} ×2 identical={identical}"
    );

    let pass = restore_ok && identical;
    println!(
        "\nSUMMARY PROBE-M33-DEMO-001: restore={} byte_identical={} ⇒ {}",
        verdict(restore_ok),
        verdict(identical),
        verdict(pass)
    );
    if pass {
        0
    } else {
        1
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Antibodies — PROBE-SAVEGAME-READ-001 classic census (CR-28fb1784, F-AUD-03)
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    /// A `SAVETREE.DAT` in the documented framing: 19 B header, a buildings
    /// block of `26·N` and one framed record per entry of `records` (payload
    /// bytes). Built here so the antibodies need 0 retail bytes.
    fn synthetic_save_tree(version: i32, buildings: usize, records: &[usize]) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(&version.to_le_bytes());
        for field in [1_000i32, 2_000, 3_000] {
            out.extend_from_slice(&field.to_le_bytes());
        }
        out.extend_from_slice(&7u16.to_le_bytes());
        out.push(0);
        assert_eq!(out.len(), SAVETREE_HEADER_BYTES, "documented 19 B header");
        out.extend_from_slice(&((buildings * BUILDING_BYTES) as i32).to_le_bytes());
        out.extend(std::iter::repeat_n(0u8, buildings * BUILDING_BYTES));
        for payload in records {
            out.extend_from_slice(&((RECORD_ROOT_BYTES + payload) as i32).to_le_bytes());
            out.extend(std::iter::repeat_n(0u8, RECORD_ROOT_BYTES + payload));
        }
        out
    }

    /// (a) ANTIBODY — absence keeps the spec §2.4 N2/N3 declaration intact:
    /// all three files are `Absent` and none of them blocks the probe. The fix
    /// must not silently delete the NEGATIVE case it grew out of.
    #[test]
    fn absent_classic_files_stay_negative() {
        for name in CLASSIC_SAVE_FILES {
            let state = classic_save_state(name, None);
            assert_eq!(
                state,
                ClassicSaveState::Absent,
                "{name}: an absent classic file must stay NEGATIVE N2/N3"
            );
            assert!(
                !state.blocking(),
                "{name}: absence must never block PROBE-SAVEGAME-READ-001"
            );
            assert_eq!(state.row(name), format!("{name}:absent"));
        }
    }

    /// (c) ANTIBODY (the regression this ticket fixes) — a **present** classic
    /// file with no source of truth is declared `NOT-COMPARABLE`: it neither
    /// blocks the probe (the old code FAILed it merely for existing) nor is it
    /// reported as a comparison that happened.
    #[test]
    fn present_without_source_of_truth_is_not_comparable() {
        for name in ["SAVEVARS.DAT", "IMAGE.RAW"] {
            let state = classic_save_state(name, Some(b"retail bytes nobody can adjudicate"));
            match &state {
                ClassicSaveState::NotComparable { reason } => {
                    assert!(
                        reason.contains("no source of truth"),
                        "{name}: the declared reason must name the missing source, got {reason:?}"
                    );
                }
                other => panic!(
                    "{name}: a present file without a source of truth must be NOT-COMPARABLE, got {other:?}"
                ),
            }
            assert!(
                !state.blocking(),
                "{name}: NOT-COMPARABLE is a declared SKIP, never a FAIL (exit 1)"
            );
        }
    }

    /// (c) The same rule covers a `SAVETREE.DAT` our non-claiming reader
    /// rejects: the missing authority degrades the state to `NOT-COMPARABLE`
    /// with the reader's own reason instead of a FAIL for the wrong reason.
    #[test]
    fn rejected_save_tree_is_not_comparable_not_a_fail() {
        let state = classic_save_state("SAVETREE.DAT", Some(&[0x26, 0x01, 0x00]));
        match &state {
            ClassicSaveState::NotComparable { reason } => assert!(
                reason.contains("claims no compatibility"),
                "the reason must state the missing authority, got {reason:?}"
            ),
            other => panic!("a 3 B SAVETREE.DAT must be NOT-COMPARABLE, got {other:?}"),
        }
        assert!(!state.blocking());
    }

    /// (b) A present `SAVETREE.DAT` **is** comparable: the documented framing
    /// is compared and its verdict is real — exact framing passes, a truncated
    /// tail and a short record both fail the comparison (never the presence).
    ///
    /// The exact fixture carries the structural census of the F-AUD-03 part
    /// of this lane: **332 B** = 19 header + 4 + 2·26 buildings + (4+71) +
    /// (4+71+107), `version 0x126`, `buildings=2`, `records=2`, `padding=0`
    /// (`CR-M39SAVES-20261027`, spec M33 §2.2 / §6.4). The real M39 (import a
    /// 1996 retail file) stays NO-EJECUTABLE — `BACKLOG-POST-M35…:60`.
    #[test]
    fn present_save_tree_is_compared_against_its_layout() {
        let exact = synthetic_save_tree(SAVETREE_VERSION, 2, &[0, 107]);
        assert_eq!(
            exact.len(),
            332,
            "19 header + 4 + 2·26 buildings + (4+71) + (4+71+107)"
        );
        match classic_save_state("SAVETREE.DAT", Some(&exact)) {
            ClassicSaveState::Compared { ok: true, detail } => {
                assert!(detail.contains("version=0x126"), "detail: {detail}");
                assert!(detail.contains("version_ok=true"), "detail: {detail}");
                assert!(detail.contains("buildings=2"), "detail: {detail}");
                assert!(detail.contains("records=2"), "detail: {detail}");
                assert!(detail.contains("padding=0"), "detail: {detail}");
                assert!(detail.contains("framing=332B bytes=332"), "detail: {detail}");
                assert!(detail.contains("covered=true"), "detail: {detail}");
                assert!(detail.contains("truncated=false"), "detail: {detail}");
                assert!(detail.contains("short_root=false"), "detail: {detail}");
            }
            other => panic!("an exact documented framing must be Compared ok, got {other:?}"),
        }

        let mut truncated = exact.clone();
        truncated.truncate(exact.len() - 20);
        match classic_save_state("SAVETREE.DAT", Some(&truncated)) {
            ClassicSaveState::Compared { ok: false, detail } => assert!(
                detail.contains("truncated=true") || detail.contains("covered=false"),
                "detail: {detail}"
            ),
            other => panic!("a truncated tail must be Compared not-ok, got {other:?}"),
        }

        let mut short_root = Vec::new();
        short_root.extend_from_slice(&SAVETREE_VERSION.to_le_bytes());
        for field in [0i32, 0, 0] {
            short_root.extend_from_slice(&field.to_le_bytes());
        }
        short_root.extend_from_slice(&0u16.to_le_bytes());
        short_root.push(0);
        short_root.extend_from_slice(&0i32.to_le_bytes());
        short_root.extend_from_slice(&40i32.to_le_bytes());
        short_root.extend(std::iter::repeat_n(0u8, 40));
        match classic_save_state("SAVETREE.DAT", Some(&short_root)) {
            ClassicSaveState::Compared { ok: false, detail } => assert!(
                detail.contains("short_root=true"),
                "detail: {detail}"
            ),
            other => panic!("a 40 B record cannot hold the root, got {other:?}"),
        }
    }

    /// The three states are distinguishable in the digest input: same states
    /// give the same digest (deterministic across processes), and each state
    /// produces its own row so a state change necessarily changes the digest.
    #[test]
    fn three_states_are_distinct_and_the_digest_is_deterministic() {
        let absent = classic_save_state("SAVEVARS.DAT", None);
        let not_comparable = classic_save_state("SAVEVARS.DAT", Some(b"whatever"));
        let compared = classic_save_state(
            "SAVETREE.DAT",
            Some(&synthetic_save_tree(SAVETREE_VERSION, 1, &[71])),
        );
        let rows_one = [
            absent.row("SAVEVARS.DAT"),
            not_comparable.row("SAVEVARS.DAT"),
            compared.row("SAVETREE.DAT"),
        ];
        let rows_two = rows_one.clone();
        assert_eq!(rows_one, rows_two, "the digest input must be deterministic");
        for (index, row) in rows_one.iter().enumerate() {
            assert!(
                rows_one.iter().skip(index + 1).all(|other| other != row),
                "each state must produce its own row: {row:?} collides"
            );
        }
        let mut unique = rows_one.to_vec();
        unique.sort();
        unique.dedup();
        assert_eq!(unique.len(), rows_one.len(), "three states, three rows");
        let digest_one = fnv1a64(rows_one.join("\n").as_bytes());
        let digest_two = fnv1a64(rows_two.join("\n").as_bytes());
        assert_eq!(digest_one, digest_two, "same states ⇒ same digest");
        assert_ne!(
            digest_one,
            fnv1a64([compared.row("SAVETREE.DAT")].join("\n").as_bytes()),
            "a different state list must change the digest"
        );
        assert!(!absent.blocking() && !not_comparable.blocking() && !compared.blocking());
    }
}
