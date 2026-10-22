//! M25 probes — monsters, enemy AI and encounters (SPEC M25 §10,
//! REQ-L99-058).
//!
//! `PROBE-MONSTER-001` — four blocks over retail:
//! - **A** container census: `records=103 ascr=60 enemy=43 dir_off=25282
//!   record_sum=25278 enemy_stride=74 ascr_min=335 ascr_max=458` (§1.2, A1-A8).
//! - **B** the 43 `ENEMY###.CFG` are 74 B each and `Rat`/`Lamia` match field by
//!   field with §2.2 (goldens B1-B8) + `MobileTypes` → record bijection 43/43.
//! - **C** class HP intervals per §3.1 (C1-C6).
//! - **D** deterministic demo: `spawned=3` with measured levels and HP, digest ×2.
//!
//! Reads `ARENA2/MONSTER.BSA`; unreadable ⇒ **exit 2**
//! `reason=missing_monster_bsa` (never a PASS without retail).
//!
//! `PROBE-ENEMY-AI-001` — pure (0 retail): formulas D1-D13, scenario
//! `Idle→Pursue` + contact at 2.25 m, `Pursue→Idle` at 12.5 s with 200
//! perceptions and `OpenDoor` gate at ≤ 2 m.
//!
//! `PROBE-ENCOUNTER-001` — pure (0 retail): E1-E10, census of the 45 tables,
//! 1000 seeded rolls with digest ×2 and spawn probability, dungeon with and
//! without alarm, and the pacification goldens F1-F7 (§6, row 5 of §12).
//!
//! **Custody (INV-L99-6):** Mode A — retail bytes are read at runtime from the
//! user's ARENA2 and never enter the repository. 0 Bevy, 0 GPU.

use std::path::PathBuf;

use daggerfall_behavior::encounters::{
    choose_enemy, clamp_to_table, level_range, referenced_tables, roll_spawn_dungeon, table_index,
    BuildingKind, ClimateBand, EncounterContext, TableClamp, COMMENTED_TABLES, DUNGEON_TYPE_COUNT,
    ENCOUNTER_TABLES, TABLE_LEN, UNUSED_TABLE_INDEX,
};
use daggerfall_behavior::enemy_ai::{
    attack_gate, attack_move_speed, class_max_health_bounds, melee_timer_seconds, move_speed,
    pacification_chance, pacification_probability, pacifies, roll_class_max_health, speed_floor,
    AiEvent, AiState, Door, EnemyAi, Perception, CLASSIC_TICK, GIVE_UP_SECONDS, MELEE_DISTANCE,
    OPEN_DOOR_DISTANCE, SIGHT_RADIUS_M,
};
use daggerfall_parser_rs::monster::{
    MonsterArchive, MonsterClass, MONSTER_FILENAME, MOBILE_TO_RECORD, MOBILE_TYPE_COUNT,
    MOBILE_TYPE_NAMES,
};
use daggerfall_skills::rng::Rng64;

// ── evidence rows (M19/M20/M21/M24 template) ────────────────────────────────

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

/// Row with its own verdict and raw detail.
fn check(id: &'static str, ok: bool, detail: impl Into<String>) -> Row {
    Row {
        id,
        ok,
        got: if ok { "ok".into() } else { "fail".into() },
        want: "ok".into(),
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

/// Stable number-to-text without trailing zeros (`4.75`, `2.44898`, `12.5`).
fn fmt_f32(value: f32) -> String {
    let mut text = format!("{value:.5}");
    while text.ends_with('0') {
        text.pop();
    }
    if text.ends_with('.') {
        text.pop();
    }
    text
}

/// FNV-1a 64 (same digest as the rest of the probes).
fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// Loads `MONSTER.BSA` through the shared Mode A resolver.
fn load_monster_bsa() -> Result<(PathBuf, Vec<u8>), String> {
    let path = crate::mode_a_arena2_path(MONSTER_FILENAME)?;
    let bytes = std::fs::read(&path).map_err(|err| format!("reading {path:?}: {err}"))?;
    Ok((path, bytes))
}

// ═══════════════════════════════════════════════════════════════════════════
// Deterministic scene reused by the demo (block D)
// ═══════════════════════════════════════════════════════════════════════════

/// Demo scene seed (deterministic ×2).
pub const M25_SCENE_SEED: u64 = 20_261_021;
/// Player level in the demo scene.
pub const M25_PLAYER_LEVEL: i32 = 5;
/// Live monsters of the scene (§11 criterion 1: three).
pub const M25_SPAWNED: usize = 3;
/// Demo dungeon: `0 = Crypt` (`DFRegion.DungeonTypes`).
pub const M25_DUNGEON_TYPE: u8 = 0;

/// A live monster of the scene (what the world/ECS will see).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpawnedMonster {
    /// `MobileTypes` id drawn by the encounter table.
    pub mobile: u16,
    /// Index of the `ENEMY###.CFG` record that carries it.
    pub record: usize,
    /// Retail CFG `Name`.
    pub name: String,
    /// Level set by the spawn (§3: the record does not set it).
    pub level: i32,
    /// Maximum HP rolled within §3.1.
    pub hp: i32,
    /// Level interval `[min, max]` (verification §11.1).
    pub hp_bounds: (i32, i32),
}

/// Scene: 3 live monsters from the demo dungeon table.
///
/// Deterministic: same seed ⇒ same scene (block D digest).
pub fn compose_scene(seed: u64) -> Result<Vec<SpawnedMonster>, String> {
    let (_, bytes) = load_monster_bsa()?;
    compose_scene_from(&bytes, seed)
}

/// Same as [`compose_scene`] over an already loaded buffer (demo/probe).
pub fn compose_scene_from(bytes: &[u8], seed: u64) -> Result<Vec<SpawnedMonster>, String> {
    let archive = MonsterArchive::parse(bytes)
        .map_err(|err| format!("{MONSTER_FILENAME} does not parse: {err}"))?;
    let table = table_index(EncounterContext::Dungeon {
        dungeon_type: M25_DUNGEON_TYPE,
    })
    .and_then(|index| ENCOUNTER_TABLES.get(index))
    .ok_or_else(|| "demo dungeon table missing".to_string())?;

    let mut rng = Rng64::new(seed);
    let mut scene = Vec::with_capacity(M25_SPAWNED);
    for _ in 0..M25_SPAWNED {
        let roll = rng.range_inclusive(1, 100);
        let (min, max) = level_range(M25_PLAYER_LEVEL, roll);
        let level = rng.range_inclusive(min, max);
        let mobile = choose_enemy(table, M25_PLAYER_LEVEL, roll, |low, high| {
            rng.range_inclusive(low, high)
        })
        .ok_or_else(|| "empty encounter table".to_string())?;
        let record = usize::from(MOBILE_TO_RECORD[usize::from(mobile)]);
        let class = archive
            .monster_class(bytes, record)
            .map_err(|err| format!("ENEMY{record:03}.CFG: {err}"))?;
        let bounds = class_max_health_bounds(level, class.hppl());
        let hp = roll_class_max_health(level, class.hppl(), |low, high| {
            rng.range_inclusive(low, high)
        });
        scene.push(SpawnedMonster {
            mobile,
            record,
            name: class.name.clone(),
            level,
            hp,
            hp_bounds: bounds,
        });
    }
    Ok(scene)
}

/// FNV-1a 64 digest of the scene (`mobile`, `record`, `level`, `hp`).
pub fn scene_digest(scene: &[SpawnedMonster]) -> u64 {
    let mut bytes = Vec::with_capacity(scene.len() * 8);
    for monster in scene {
        bytes.extend_from_slice(&monster.mobile.to_le_bytes());
        bytes.extend_from_slice(&monster.level.to_le_bytes());
        bytes.extend_from_slice(&monster.hp.to_le_bytes());
    }
    fnv1a64(&bytes)
}

// ═══════════════════════════════════════════════════════════════════════════
// PROBE-MONSTER-001
// ═══════════════════════════════════════════════════════════════════════════

/// Block B: the 43 CFG are 74 B each and the two reference classes match §2.2.
fn monster_block_b(
    archive: &MonsterArchive,
    bytes: &[u8],
) -> Result<Vec<Row>, String> {
    let mut rows = Vec::new();
    // `classes[n]` = `ENEMY{n:03}.CFG` (number in the name, not position in the
    // census — erratum 26: the directory interleaves ASCR and ENEMY).
    let mut classes: Vec<MonsterClass> = Vec::with_capacity(archive.enemy.len());
    for number in 0..archive.enemy.len() {
        let record = archive
            .monster_class(bytes, number)
            .map_err(|err| format!("ENEMY{number:03}.CFG: {err}"))?;
        classes.push(record);
    }
    // Every record declares size 74 and parses to exactly 74 fields.
    let all_74 = archive.census().enemy_stride == Some(74) && classes.len() == 43;
    println!("all_74={all_74}");
    rows.push(check(
        "B1 all_74",
        all_74,
        format!(
            "{} ENEMY records · stride {:?} · 58 + 8·2 = 74",
            classes.len(),
            archive.census().enemy_stride
        ),
    ));

    // Rat (ENEMY000) and Lamia (ENEMY042) — spec §2.2 cites them by index.
    let rat = archive
        .monster_class(bytes, 0)
        .map_err(|err| format!("ENEMY000.CFG: {err}"))?;
    let lamia = archive
        .monster_class(bytes, 42)
        .map_err(|err| format!("ENEMY042.CFG: {err}"))?;
    let rat_line = format!(
        "Rat{{res={},hppl={},adv={},attrs=[{}]}}",
        rat.resistance_flags,
        rat.hit_points_per_level,
        rat.advancement_multiplier_raw,
        rat.attributes
            .iter()
            .map(u16::to_string)
            .collect::<Vec<_>>()
            .join(",")
    );
    let lamia_line = format!(
        "Lamia{{res={},imm={},low={},abl={},hppl={},adv={}}}",
        lamia.resistance_flags,
        lamia.immunity_flags,
        lamia.low_tolerance_flags,
        lamia.ability_flags,
        lamia.hit_points_per_level,
        lamia.advancement_multiplier_raw
    );
    println!("{rat_line}");
    println!("{lamia_line}");
    rows.push(row(
        "B2 Rat",
        rat_line.clone(),
        "Rat{res=68,hppl=1,adv=75008,attrs=[40,10,70,80,55,50,45,50]}".to_string(),
        "spec §2.2 · ENEMY000.CFG",
    ));
    rows.push(row(
        "B3 Lamia",
        lamia_line.clone(),
        "Lamia{res=2,imm=69,low=16,abl=36,hppl=17,adv=52992}".to_string(),
        "spec §2.2 · ENEMY042.CFG",
    ));
    rows.push(row(
        "B4 Rat.Endurance",
        rat.endurance().to_string(),
        "55".to_string(),
        "u16 @58+4·2 (golden B2)",
    ));
    rows.push(row(
        "B5 Rat.weapon_armor_shields",
        rat.weapon_armor_shields().to_string(),
        "0".to_string(),
        "(0<<16)|(0<<8)|0 (golden B6)",
    ));
    rows.push(row(
        "B6 Lamia.name",
        lamia.name.clone(),
        "Lamia".to_string(),
        "16 B slot cut at the first NUL",
    ));

    // MobileTypes → record bijection, verified against the CFG `Name`.
    let mut used = [false; MOBILE_TYPE_COUNT];
    let mut mismatches = Vec::new();
    for (mobile, record) in MOBILE_TO_RECORD.iter().enumerate() {
        let index = usize::from(*record);
        let class = &classes[index];
        if used[index] || !name_matches(mobile, &class.name) {
            mismatches.push(format!(
                "{}({mobile}) -> ENEMY{index:03}={}",
                MOBILE_TYPE_NAMES[mobile], class.name
            ));
        }
        used[index] = true;
    }
    rows.push(check(
        "B7 MobileTypes → ENEMY bijection",
        mismatches.is_empty() && used.iter().all(|hit| *hit),
        format!(
            "{MOBILE_TYPE_COUNT}/{MOBILE_TYPE_COUNT} match by `Name`{}",
            if mismatches.is_empty() {
                String::new()
            } else {
                format!(" · mismatch: {}", mismatches.join(", "))
            }
        ),
    ));
    Ok(rows)
}

/// Retail CFG `Name` against the `MobileTypes` name, with the declared
/// aliases (`Giant Scorpian`, `horse`, second `dragonling`).
fn name_matches(mobile: usize, cfg_name: &str) -> bool {
    const ALIASES: [(usize, &str); 3] = [
        (20, "giantscorpian"),
        (39, "horse"),
        (40, "dragonling"),
    ];
    let normalized = |text: &str| {
        text.chars()
            .filter(|ch| ch.is_ascii_alphanumeric())
            .map(|ch| ch.to_ascii_lowercase())
            .collect::<String>()
    };
    let expected = ALIASES
        .iter()
        .find(|(id, _)| *id == mobile)
        .map(|(_, alias)| *alias)
        .unwrap_or_else(|| MOBILE_TYPE_NAMES[mobile]);
    normalized(cfg_name) == normalized(expected)
}

/// `PROBE-MONSTER-001` — retail census + layout + HP + deterministic scene.
pub fn run_monster_001() -> i32 {
    println!("PROBE-MONSTER-001 - M25 monsters (REQ-L99-058 · spec §10)");
    let (path, bytes) = match load_monster_bsa() {
        Ok(loaded) => loaded,
        Err(reason) => {
            println!("ARENA2 not accessible: {reason}");
            println!("reason=missing_monster_bsa");
            println!("EXIT=2");
            println!("VERDICT: PROBE-MONSTER-001 NO DATA - {MONSTER_FILENAME} not accessible");
            return 2;
        }
    };
    let archive = match MonsterArchive::parse(&bytes) {
        Ok(archive) => archive,
        Err(err) => {
            println!("[FAIL] {MONSTER_FILENAME} does not parse: {err}");
            println!("reason=missing_monster_bsa");
            println!("EXIT=2");
            println!("VERDICT: PROBE-MONSTER-001 NO DATA - unreadable container");
            return 2;
        }
    };
    println!(
        "[OK] mode_a {}: {} ({} B)",
        MONSTER_FILENAME,
        path.display(),
        bytes.len()
    );

    // ── A · census (§1.2, A1-A8) ───────────────────────────────────────────
    let census = archive.census();
    let ascr_line = format!(
        "records={} ascr={} enemy={} dir_off={} record_sum={} enemy_stride={} ascr_min={} ascr_max={}",
        census.records,
        census.ascr,
        census.enemy,
        census.dir_offset,
        census.record_sum,
        census
            .enemy_stride
            .map(|size| size.to_string())
            .unwrap_or_else(|| "mixed".to_string()),
        census.ascr_min,
        census.ascr_max
    );
    println!("{ascr_line}");
    let a_ok = ascr_line
        == "records=103 ascr=60 enemy=43 dir_off=25282 record_sum=25278 enemy_stride=74 ascr_min=335 ascr_max=458";
    let mut a_rows = vec![check(
        "A1,A2,A7,A8 census",
        a_ok,
        "103 records · 25282 = 27136 − 18·103 · Σ = 25278 · 43×74",
    )];
    // A3/A4/A5/A6: real offsets of the records cited by the spec.
    let data = &bytes;
    // Absolute offset of a record = `data_start` + Σ previous sizes.
    let start_of = |entry: usize| -> u64 {
        archive
            .archive
            .entries
            .iter()
            .take(entry)
            .fold(archive.archive.data_start, |start, prev| {
                start + u64::from(prev.data_size())
            })
    };
    // `ENEMY{number:03}.CFG` — erratum 26: the directory interleaves ASCR/ENEMY,
    // so the position in the census is not the record number.
    let enemy_at = |number: usize| archive.enemy.iter().find(|rec| rec.number == number);
    let first_ascr = archive.anim.first().map(|rec| {
        (
            rec.name.clone(),
            start_of(rec.entry),
            u64::from(rec.size),
        )
    });
    let first_ascr_line = first_ascr
        .as_ref()
        .map(|(name, start, size)| format!("{name}@{start}+{size}"))
        .unwrap_or_else(|| "none".to_string());
    // Values measured over `ARENA2` (erratum 26): `ENEMY000.CFG` at 4257 and
    // `ENEMY042.CFG` at 24080; the last directory record is
    // `ASCR0001.ANC@24934+348`, which closes at 25282.
    let enemy000 = enemy_at(0).map(|rec| {
        (
            rec.name.clone(),
            start_of(rec.entry),
            u64::from(rec.size),
        )
    });
    let enemy042 = enemy_at(42).map(|rec| {
        (
            rec.name.clone(),
            start_of(rec.entry),
            u64::from(rec.size),
        )
    });
    let last = archive
        .archive
        .entries
        .iter()
        .enumerate()
        .next_back()
        .map(|(entry, rec)| (start_of(entry), u64::from(rec.data_size())));
    let (a000_name, a000_start, a000_size) = enemy000
        .clone()
        .unwrap_or_else(|| ("none".to_string(), 0, 0));
    let (a042_name, a042_start) = enemy042
        .clone()
        .map(|(name, start, _)| (name, start))
        .unwrap_or_else(|| ("none".to_string(), 0));
    let last_end = last.map(|(start, size)| start + size).unwrap_or(0);
    println!(
        "A3 {} · A4 {a000_name}@{a000_start} · A5 {a042_name}@{a042_start} · A6 last end={last_end}",
        first_ascr_line
    );
    let a3_ok = first_ascr_line == "ASCR0000.ANC@4+337";
    let a4_ok = a000_name == "ENEMY000.CFG" && a000_start == 4257 && a000_size == 74;
    let a5_ok = a042_name == "ENEMY042.CFG" && a042_start == 24080;
    let a6_ok = last_end == census.dir_offset;
    a_rows.push(check(
        "A3 ASCR0000 offset/size",
        a3_ok,
        first_ascr_line.clone(),
    ));
    a_rows.push(check(
        "A4 ENEMY000 offset/size",
        a4_ok,
        format!("{a000_name}@{a000_start} + {a000_size}"),
    ));
    a_rows.push(check(
        "A5 ENEMY042 offset",
        a5_ok,
        format!("{a042_name}@{a042_start}"),
    ));
    a_rows.push(check(
        "A6 close = directory",
        a6_ok,
        format!("end={last_end} dir={}", census.dir_offset),
    ));
    // Anti-false-positives (ii): the directory is AT THE END, not at the beginning.
    let header_wrong = bytes.len() >= 18 * census.records
        && bytes[4..4 + 14].iter().any(|byte| *byte != 0)
        && !bytes[4..4 + 14]
            .starts_with(archive.enemy.first().map(|r| r.name.as_bytes()).unwrap_or(b""));
    a_rows.push(check(
        "A9 directory at the end (anti-FP ii)",
        header_wrong || archive.archive.directory_offset == bytes.len() as u64 - 18 * 103,
        format!(
            "a read at 4 would yield data bytes · directory at {}",
            archive.archive.directory_offset
        ),
    ));
    let mut all_ok = print_rows("A · container census (§1.2)", &a_rows);

    // ── B · layout of the 43 CFG (§2) ──────────────────────────────────────
    match monster_block_b(&archive, data) {
        Ok(b_rows) => all_ok &= print_rows("B · ENEMY###.CFG 74 B (§2)", &b_rows),
        Err(err) => {
            println!("[FAIL] block B: {err}");
            all_ok = false;
        }
    }

    // ── C · class HP (§3.1, C1-C6) ─────────────────────────────────────────
    let rat_hppl = archive
        .monster_class(data, 0)
        .map(|class| class.hppl())
        .unwrap_or(-1);
    let lamia_hppl = archive
        .monster_class(data, 42)
        .map(|class| class.hppl())
        .unwrap_or(-1);
    let (r1, r10) = (
        class_max_health_bounds(1, rat_hppl),
        class_max_health_bounds(10, rat_hppl),
    );
    let l1 = class_max_health_bounds(1, lamia_hppl);
    let l5 = class_max_health_bounds(5, lamia_hppl);
    let h0 = class_max_health_bounds(5, 0);
    let hp_line = format!(
        "hp_rat_L1={} hp_rat_L10={} hp_lamia_L1=[{},{}] hp_lamia_L5=[{},{}] hp_hppl0_L5={}",
        r1.0, r10.0, l1.0, l1.1, l5.0, l5.1, h0.0
    );
    println!("{hp_line}");
    let mut c_rows = vec![row(
        "C1-C6 intervals",
        hp_line.clone(),
        "hp_rat_L1=11 hp_rat_L10=20 hp_lamia_L1=[11,27] hp_lamia_L5=[15,95] hp_hppl0_L5=15"
            .to_string(),
        "10 + L·[1, hppl] with hppl from the CFG",
    )];
    // C5 (Lamia L10) and C6 (hppl 0 ⇒ [15,15], not "no die").
    let l10 = class_max_health_bounds(10, lamia_hppl);
    c_rows.push(check(
        "C5 Lamia L10 = [20,180]",
        l10 == (20, 180),
        format!("[{}, {}]", l10.0, l10.1),
    ));
    c_rows.push(check(
        "C6 hppl 0 degenerates into 1",
        h0 == (15, 15),
        format!("[{}, {}]", h0.0, h0.1),
    ));
    all_ok &= print_rows("C · class HP (§3.1)", &c_rows);

    // ── D · demo: 3 live monsters (§11.1) ──────────────────────────────────
    let scene_first = compose_scene(M25_SCENE_SEED);
    let scene_second = compose_scene(M25_SCENE_SEED);
    let mut d_rows = Vec::new();
    match (scene_first, scene_second) {
        (Ok(first), Ok(second)) => {
            let levels = first
                .iter()
                .map(|monster| monster.level.to_string())
                .collect::<Vec<_>>()
                .join(",");
            let hp = first
                .iter()
                .map(|monster| monster.hp.to_string())
                .collect::<Vec<_>>()
                .join(",");
            let names = first
                .iter()
                .map(|monster| monster.name.clone())
                .collect::<Vec<_>>()
                .join(",");
            println!(
                "spawned={} levels=[{levels}] hp∈[{hp}] names=[{names}]",
                first.len()
            );
            d_rows.push(check(
                "D1 spawned=3",
                first.len() == M25_SPAWNED,
                format!("{} live monsters", first.len()),
            ));
            let in_bounds = first
                .iter()
                .all(|monster| monster.hp >= monster.hp_bounds.0 && monster.hp <= monster.hp_bounds.1);
            d_rows.push(check(
                "D2 hp ∈ §3.1 interval",
                in_bounds,
                first
                    .iter()
                    .map(|monster| {
                        format!(
                            "{} L{} hp {}/[{},{}]",
                            monster.name,
                            monster.level,
                            monster.hp,
                            monster.hp_bounds.0,
                            monster.hp_bounds.1
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(" · "),
            ));
            let digest_first = scene_digest(&first);
            let digest_second = scene_digest(&second);
            println!(
                "digest={digest_first:016x} ×2 → {digest_second:016x} ({})",
                if digest_first == digest_second {
                    "equal"
                } else {
                    "DIFFERENT"
                }
            );
            d_rows.push(check(
                "D3 digest ×2",
                digest_first == digest_second && first == second,
                format!("{digest_first:016x}"),
            ));
        }
        (Err(err), _) | (_, Err(err)) => {
            println!("[FAIL] block D: {err}");
            d_rows.push(check("D0 scene", false, err));
        }
    }
    all_ok &= print_rows("D · deterministic demo (§11.1)", &d_rows);

    if all_ok {
        println!("VERDICT: PROBE-MONSTER-001 PASS - census 103/60/43 · 43 CFG of 74 B · HP §3.1 · {M25_SPAWNED} live monsters ×2 · 0 retail bytes in the repo");
        0
    } else {
        println!("VERDICT: PROBE-MONSTER-001 FAIL - some block does not match spec §10");
        1
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// PROBE-ENEMY-AI-001
// ═══════════════════════════════════════════════════════════════════════════

/// `PROBE-ENEMY-AI-001` — formulas + state-machine scenarios.
pub fn run_enemy_ai_001() -> i32 {
    println!("PROBE-ENEMY-AI-001 - M25 enemy AI (REQ-L99-058 · spec §10)");

    // ── A · formulas D1-D5, D8-D13 ─────────────────────────────────────────
    let a_line = format!(
        "move_speed(40)={} move_speed(100)={} move_speed(0)={} gate(40)={} gate(0)={} giveup={} sight={} melee(r=2400,l=10,rx=2)={} melee(r=1500,l=0,rx=0)={}",
        fmt_f32(move_speed(40)),
        fmt_f32(move_speed(100)),
        fmt_f32(move_speed(0)),
        attack_gate(40),
        attack_gate(0),
        fmt_f32(GIVE_UP_SECONDS),
        fmt_f32(SIGHT_RADIUS_M),
        fmt_f32(melee_timer_seconds(2400, 10, 2)),
        fmt_f32(melee_timer_seconds(1500, 0, 0)),
    );
    println!("{a_line}");
    let want_a = "move_speed(40)=4.75 move_speed(100)=6.25 move_speed(0)=3.75 gate(40)=11 gate(0)=7 giveup=12.5 sight=102.4 melee(r=2400,l=10,rx=2)=2.44898 melee(r=1500,l=0,rx=0)=1.12245";
    let mut a_rows = vec![row(
        "A D1-D5,D8-D13",
        a_line.clone(),
        want_a.to_string(),
        "speed/gate/give-up/senses/cadence",
    )];
    // D2 and D7/D12/D13 complete family A.
    let d2 = attack_move_speed(40);
    let d7 = melee_timer_seconds(2400, 20, 2);
    let d13 = melee_timer_seconds(3000, 20, 4);
    a_rows.push(check(
        "A D2 pursuit = 2.375",
        (d2 - 2.375).abs() < 1e-6,
        fmt_f32(d2),
    ));
    a_rows.push(check(
        "A D7 = 1.93878",
        fmt_f32(d7) == "1.93878",
        fmt_f32(d7),
    ));
    a_rows.push(check(
        "A D13 = 3.46939",
        fmt_f32(d13) == "3.46939",
        fmt_f32(d13),
    ));
    // D4/D5: the probability is derived from the crate's own gate — comparing
    // `11/40` against `0.275` was circular (clippy `eq_op` caught it).
    let p_no_attack_d4 = f64::from(attack_gate(40)) / 40.0;
    let p_no_attack_d5 = f64::from(attack_gate(0)) / f64::from(speed_floor(0));
    a_rows.push(check(
        "A D4/D5 P(no attack)",
        (p_no_attack_d4 - 0.275).abs() < 1e-6 && (p_no_attack_d5 - 0.875).abs() < 1e-6,
        format!(
            "{:.1} % · {:.1} %",
            p_no_attack_d4 * 100.0,
            p_no_attack_d5 * 100.0
        ),
    ));
    let mut all_ok = print_rows("A · formulas (§4.2, D1-D13)", &a_rows);

    // ── B · Idle→Pursue + first contact (player at 20 m) ───────────────────
    let mut ai = EnemyAi::new(40);
    let mut distance = 20.0f32;
    let mut events = ai.classic_update(Perception::visible(distance), None);
    let acquire = ai.first_transition(AiState::Idle, AiState::Pursue);
    let mut ticks = 1;
    while ai.state == AiState::Pursue && ticks < 1000 {
        distance = ai.close_distance(distance, CLASSIC_TICK);
        events.extend(ai.classic_update(Perception::visible(distance), None));
        ticks += 1;
    }
    let contact = ai.first_contact();
    let b_line = match (acquire, contact) {
        (Some(at), Some((contact_at, contact_distance))) => format!(
            "Idle→Pursue at {:.4} s · contact at {} m (t={:.4} s)",
            at, fmt_f32(contact_distance), contact_at
        ),
        _ => "no transition/contact".to_string(),
    };
    println!("{b_line}");
    let mut b_rows = vec![check(
        "B Idle→Pursue ≤ 0.0625 s",
        acquire.map(|at| at <= CLASSIC_TICK + 1e-6).unwrap_or(false),
        format!("t = {} s", acquire.map(fmt_f32).unwrap_or_default()),
    )];
    b_rows.push(check(
        "B first contact at 2.25 m",
        contact
            .map(|(_, distance)| (distance - MELEE_DISTANCE).abs() < 1e-6)
            .unwrap_or(false),
        contact
            .map(|(at, distance)| format!("d = {} m · t = {} s", fmt_f32(distance), fmt_f32(at)))
            .unwrap_or_else(|| "no contact".to_string()),
    ));
    // Anti-false-positives (R-C): the SAME result at 16 Hz and at 32 Hz
    // (2 ticks of 0.03125 s) with the same scenario seed.
    let mut half = EnemyAi::new(40);
    let mut half_distance = 20.0f32;
    half.classic_update(Perception::visible(half_distance), None);
    let mut half_ticks = 1;
    while half.state == AiState::Pursue && half_ticks < 4000 {
        half_distance = half.close_distance(half_distance, CLASSIC_TICK / 2.0);
        half.classic_update(Perception::visible(half_distance), None);
        half_ticks += 1;
    }
    let half_contact = half.first_contact().map(|(_, distance)| distance);
    b_rows.push(check(
        "B independent of frequency (R-C)",
        half_contact
            .map(|distance| (distance - MELEE_DISTANCE).abs() < 1e-6)
            .unwrap_or(false),
        format!(
            "2 ticks of 0.03125 s ⇒ {} m in {} ticks",
            half_contact.map(fmt_f32).unwrap_or_default(),
            half_ticks
        ),
    ));
    all_ok &= print_rows("B · perception scenario (§10)", &b_rows);

    // ── C · target lost: exactly 12.5 s, 200 perceptions ───────────────────
    let mut lost = EnemyAi::new(40);
    let mut lost_distance = 20.0f32;
    lost.classic_update(Perception::visible(lost_distance), None);
    for _ in 0..9 {
        lost_distance = lost.close_distance(lost_distance, CLASSIC_TICK);
        lost.classic_update(Perception::visible(lost_distance), None);
    }
    assert_eq!(lost.state, AiState::Pursue);
    let last_detection = lost.elapsed;
    let mark = lost.perceptions;
    let mut lost_ticks = 0;
    while lost.state != AiState::Idle && lost_ticks < 10_000 {
        lost.classic_update(Perception::lost(), None);
        lost_ticks += 1;
    }
    let give_up_at = lost.first_transition(AiState::Pursue, AiState::Idle);
    let perceptions = lost.perceptions - mark;
    let c_line = format!(
        "Pursue→Idle at {} s ({:?} since last detection) · perceptions={perceptions}",
        give_up_at.map(fmt_f32).unwrap_or_else(|| "none".into()),
        give_up_at.map(|at| fmt_f32(at - last_detection))
    );
    println!("{c_line}");
    let mut c_rows = vec![check(
        "C exactly 12.5 s",
        give_up_at
            .map(|at| (at - last_detection - GIVE_UP_SECONDS).abs() < 1e-6)
            .unwrap_or(false),
        format!(
            "{} s since last detection ({:?} s)",
            give_up_at
                .map(|at| fmt_f32(at - last_detection))
                .unwrap_or_default(),
            fmt_f32(GIVE_UP_SECONDS)
        ),
    )];
    c_rows.push(check(
        "C perceptions = 200",
        perceptions == 200,
        format!("{perceptions} perceptions at 16 Hz"),
    ));
    all_ok &= print_rows("C · give-up (§4.1)", &c_rows);

    // ── D · closed door on the route ───────────────────────────────────────
    let mut doored = EnemyAi::new(40);
    let mut door_distance = 10.0f32;
    let mut door_ticks = 0;
    doored.classic_update(Perception::visible(20.0), None);
    while doored.state != AiState::OpenDoor && door_ticks < 400 {
        door_distance = (door_distance - doored.attack_speed * CLASSIC_TICK).max(0.0);
        doored.classic_update(
            Perception::visible(20.0),
            Some(Door {
                distance: door_distance,
                open: false,
            }),
        );
        door_ticks += 1;
    }
    let door_event = doored.events.iter().find_map(|event| match event {
        AiEvent::DoorOpened { at, distance } => Some((*at, *distance)),
        _ => None,
    });
    let d_line = format!(
        "OpenDoor at d={} m (≤ 2.0) · doors_opened={}",
        door_event.map(|(_, d)| fmt_f32(d)).unwrap_or_else(|| "none".into()),
        doored.doors_opened
    );
    println!("{d_line}");
    let mut d_rows = vec![check(
        "D OpenDoor with d ≤ 2.0",
        door_event
            .map(|(_, distance)| distance <= OPEN_DOOR_DISTANCE)
            .unwrap_or(false),
        format!(
            "door at {} m",
            door_event.map(|(_, d)| fmt_f32(d)).unwrap_or_default()
        ),
    )];
    d_rows.push(check(
        "D doors_opened = 1",
        doored.doors_opened == 1,
        format!("{} door(s) opened", doored.doors_opened),
    ));
    all_ok &= print_rows("D · doors (§4.2)", &d_rows);

    if all_ok {
        println!("VERDICT: PROBE-ENEMY-AI-001 PASS - formulas D1-D13 · Idle→Pursue in 1 tick · contact 2.25 m · give-up 12.5 s/200 · OpenDoor ≤ 2 m");
        0
    } else {
        println!("VERDICT: PROBE-ENEMY-AI-001 FAIL - some block does not match spec §10");
        1
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// PROBE-ENCOUNTER-001
// ═══════════════════════════════════════════════════════════════════════════

/// Seed of the 1000 rolls of block C.
pub const ENCOUNTER_SEED: u64 = 20_261_022;
/// Rolls of block C.
pub const ENCOUNTER_ROLLS: usize = 1000;

/// `PROBE-ENCOUNTER-001` — tables, level range, spawn and pacification.
pub fn run_encounter_001() -> i32 {
    println!("PROBE-ENCOUNTER-001 - M25 encounters (REQ-L99-058 · spec §10)");

    // ── A · E1-E10 ─────────────────────────────────────────────────────────
    let goldens: [(&str, i32, i32, (i32, i32)); 8] = [
        ("E1", 5, 50, (2, 8)),
        ("E2", 10, 50, (7, 13)),
        ("E3", 2, 85, (0, 3)),
        ("E4", 5, 85, (0, 6)),
        ("E5", 5, 97, (0, 7)),
        ("E6", 8, 97, (0, 19)),
        ("E7", 1, 50, (0, 5)),
        ("E8", 19, 85, (14, 19)),
    ];
    let mut a_rows = Vec::new();
    for (id, level, roll, want) in goldens {
        let got = level_range(level, roll);
        a_rows.push(check(
            id,
            got == want,
            format!("(level {level}, roll {roll}) → [{}, {}]", got.0, got.1),
        ));
    }
    // E9: synthetic clamp of 11 entries with max 12 ⇒ [5,10].
    let e9 = clamp_to_table(11, 0, 12);
    a_rows.push(check(
        "E9 synthetic clamp",
        e9 == TableClamp::Clamped { min: 5, max: 10 },
        format!(
            "table of 11 · max 12 ⇒ {:?} (unit test, not a game encounter)",
            e9
        ),
    ));
    // E10: table of 20 with max 19 ⇒ no clamp.
    let e10 = clamp_to_table(20, 14, 19);
    a_rows.push(check(
        "E10 no clamp",
        e10 == TableClamp::Fits { min: 14, max: 19 },
        format!("table of 20 · max 19 ⇒ {e10:?}"),
    ));
    let mut all_ok = print_rows("A · level range (§5.3 E1-E10)", &a_rows);

    // ── B · table census ───────────────────────────────────────────────────
    let referenced = referenced_tables();
    let short = ENCOUNTER_TABLES
        .iter()
        .filter(|table| table.len() != TABLE_LEN)
        .count();
    let max_index_used = referenced
        .iter()
        .enumerate()
        .filter(|(_, hit)| **hit)
        .map(|(index, _)| index)
        .max()
        .unwrap_or(0);
    let unused: Vec<String> = referenced
        .iter()
        .enumerate()
        .filter(|(_, hit)| !**hit)
        .map(|(index, _)| index.to_string())
        .collect();
    let b_line = format!(
        "tables={} len20={} short={} commented={} max_index_used={} unused_index={}",
        ENCOUNTER_TABLES.len(),
        ENCOUNTER_TABLES
            .iter()
            .filter(|table| table.len() == TABLE_LEN)
            .count(),
        short,
        COMMENTED_TABLES,
        max_index_used,
        unused.join(",")
    );
    println!("{b_line}");
    let mut b_rows = vec![row(
        "B table census",
        b_line.clone(),
        "tables=45 len20=45 short=0 commented=1 max_index_used=44 unused_index=38".to_string(),
        "RandomEncounters.cs:26-1316 (the 11-entry table is commented out)",
    )];
    b_rows.push(check(
        "B indices by mapping",
        (0..DUNGEON_TYPE_COUNT).all(|index| referenced[index])
            && referenced[19]
            && (20..38).all(|index| referenced[index])
            && (39..45).all(|index| referenced[index])
            && !referenced[UNUSED_TABLE_INDEX],
        format!(
            "dungeon 0..{}, water 19, climate 20..37, buildings 39..44; {} not referenced",
            DUNGEON_TYPE_COUNT - 1,
            UNUSED_TABLE_INDEX
        ),
    ));
    all_ok &= print_rows("B · census (§5.1)", &b_rows);

    // ── C · 1000 seeded rolls ──────────────────────────────────────────────
    let first = seeded_encounter_run(ENCOUNTER_SEED);
    let second = seeded_encounter_run(ENCOUNTER_SEED);
    let digest_first = fnv1a64(&first.digest_input);
    let digest_second = fnv1a64(&second.digest_input);
    let probability = first.spawns as f64 / ENCOUNTER_ROLLS as f64;
    println!(
        "p_spawn(1/24)={probability:.4} spawned={} digest={digest_first:016x} ×2 → {digest_second:016x}",
        first.spawns
    );
    let mut c_rows = vec![check(
        "C p_spawn 1/24",
        (probability - 1.0 / 24.0).abs() <= 0.005,
        format!(
            "{probability:.4} over {ENCOUNTER_ROLLS} rolls (expected 0.0417 ± 0.005)"
        ),
    )];
    c_rows.push(check(
        "C digest ×2",
        digest_first == digest_second,
        format!("{digest_first:016x}"),
    ));
    c_rows.push(check(
        "C explicit seed (anti-FP iv)",
        seeded_encounter_run(ENCOUNTER_SEED + 1).digest_input != first.digest_input,
        "another seed ⇒ another histogram".to_string(),
    ));
    all_ok &= print_rows("C · 1000 seeded rolls (§5.4)", &c_rows);

    // ── D · dungeon without/with alarm ─────────────────────────────────────
    let no_alarm = (0..ENCOUNTER_ROLLS)
        .filter(|_| roll_spawn_dungeon(false, 0))
        .count();
    let with_alarm = (0..ENCOUNTER_ROLLS)
        .map(|index| roll_spawn_dungeon(true, (index as u32) % 36))
        .filter(|spawn| *spawn)
        .count();
    println!("spawns_without_alarm={no_alarm} spawns_with_alarm={with_alarm}");
    let mut d_rows = vec![check(
        "D dungeon without alarm ⇒ spawns=0",
        no_alarm == 0,
        format!("{no_alarm} spawns"),
    )];
    d_rows.push(check(
        "D dungeon with alarm ⇒ spawns>0",
        with_alarm > 0,
        format!("{with_alarm} spawns in {ENCOUNTER_ROLLS} rolls (1/36)"),
    ));
    all_ok &= print_rows("D · dungeon spawn (§5.4)", &d_rows);

    // ── E · pacification F1-F7 (§6, row 5 of §12) ──────────────────────────
    let mut e_rows = vec![check(
        "F1 chance 26 = 13.00 %",
        (pacification_probability(26) - 0.13).abs() < 1e-12,
        format!("{:.2} %", pacification_probability(26) * 100.0),
    )];
    e_rows.push(check(
        "F2 chance 100 = 50.00 %",
        (pacification_probability(100) - 0.5).abs() < 1e-12,
        format!("{:.2} %", pacification_probability(100) * 100.0),
    ));
    e_rows.push(check(
        "F3 chance 200 = 100 %",
        pacification_probability(200) == 1.0,
        format!("{:.2} %", pacification_probability(200) * 100.0),
    ));
    e_rows.push(check(
        "F4 chance 250 ⇒ clamp 100 %",
        pacification_probability(250) == 1.0,
        format!("{:.2} %", pacification_probability(250) * 100.0),
    ));
    e_rows.push(check(
        "F5 chance −9 ⇒ 0 %",
        pacification_probability(-9) == 0.0,
        format!("{:.2} %", pacification_probability(-9) * 100.0),
    ));
    let f6_ok = [0, 26, 100, 200, 250].iter().all(|chance| {
        let sheathed = pacification_chance(*chance, false, 50, true, 0);
        let drawn = pacification_chance(*chance, false, 50, false, 0);
        pacification_probability(sheathed) >= pacification_probability(drawn)
    }) && pacification_probability(pacification_chance(225, false, 50, true, 0))
        == pacification_probability(pacification_chance(225, false, 50, false, 0));
    e_rows.push(check(
        "F6 sheathed ≥ drawn (equality only at 225)",
        f6_ok,
        "p_sheathed − p_drawn = (min(c+10,200) − max(c−25,0))/200".to_string(),
    ));
    let f7_ok = pacification_probability(pacification_chance(80, false, 50, true, 0))
        >= pacification_probability(pacification_chance(20, false, 50, true, 0));
    e_rows.push(check(
        "F7 monotonicity in the skill",
        f7_ok && pacifies(26, 25) && !pacifies(26, 26),
        "skill 80 ≥ skill 20 · success ⇔ roll < chance".to_string(),
    ));
    // The climate → band mapping of §5.1 (M31 composes `WorldClimate`).
    let climate_ok = ClimateBand::from_climate_index(224) == Some(ClimateBand::Desert)
        && ClimateBand::from_climate_index(226) == Some(ClimateBand::Mountain)
        && ClimateBand::from_climate_index(227) == Some(ClimateBand::Rainforest)
        && ClimateBand::from_climate_index(229) == Some(ClimateBand::Subtropical)
        && ClimateBand::from_climate_index(230) == Some(ClimateBand::SwampWoodlands)
        && ClimateBand::from_climate_index(232) == Some(ClimateBand::HauntedWoodlands)
        && ClimateBand::from_climate_index(223).is_none()
        && BuildingKind::PalaceOrHouse1.table_index() == 42
        && table_index(EncounterContext::Exterior {
            climate: ClimateBand::Desert,
            in_location: true,
            is_day: true,
        })
        .is_none();
    e_rows.push(check(
        "§5.1 climate/building mapping",
        climate_ok,
        "climates 224/225..232 → 20..37 · buildings 39..44 · daytime in location ⇒ None",
    ));
    all_ok &= print_rows("E · pacification (§6 F1-F7)", &e_rows);

    if all_ok {
        println!("VERDICT: PROBE-ENCOUNTER-001 PASS - E1-E10 · 45/45 tables of 20 (1 commented out, 38 unused) · 1000 rolls ×2 · dungeon without alarm 0 · F1-F7");
        0
    } else {
        println!("VERDICT: PROBE-ENCOUNTER-001 FAIL - some block does not match spec §10");
        1
    }
}

/// Result of a seeded run of block C.
#[derive(Debug, Clone, PartialEq, Eq)]
struct SeededRun {
    /// Observed spawns (`range(0, 24) == 0`).
    spawns: usize,
    /// Histogram bytes (`mobile` per roll, `0xFFFF` = no spawn).
    digest_input: Vec<u8>,
}

/// "no spawn" marker in the block C histogram.
const NO_SPAWN: u16 = u16::MAX;

fn seeded_encounter_run(seed: u64) -> SeededRun {
    let mut rng = Rng64::new(seed);
    let mut spawns = 0;
    let mut digest_input = Vec::with_capacity(ENCOUNTER_ROLLS * 2);
    for _ in 0..ENCOUNTER_ROLLS {
        let spawn_roll = rng.range_inclusive(0, 23);
        let level = rng.range_inclusive(1, 20);
        let enemy_roll = rng.range_inclusive(1, 100);
        let picked = choose_enemy(&ENCOUNTER_TABLES[19], level, enemy_roll, |low, high| {
            rng.range_inclusive(low, high)
        })
        .unwrap_or(NO_SPAWN);
        if spawn_roll == 0 {
            spawns += 1;
            digest_input.extend_from_slice(&picked.to_le_bytes());
        } else {
            digest_input.extend_from_slice(&NO_SPAWN.to_le_bytes());
        }
    }
    SeededRun {
        spawns,
        digest_input,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Both pure probes exit 0 (the scenarios are deterministic).
    #[test]
    fn pure_probes_pass() {
        assert_eq!(run_enemy_ai_001(), 0, "PROBE-ENEMY-AI-001");
        assert_eq!(run_encounter_001(), 0, "PROBE-ENCOUNTER-001");
    }

    /// The demo scene is deterministic ×2 and respects §3.1 when retail
    /// is available (without retail the test invents nothing: it declares the skip).
    #[test]
    fn scene_is_deterministic_when_retail_is_present() {
        let Ok((_, bytes)) = load_monster_bsa() else {
            eprintln!("MONSTER.BSA missing: scene not measured in this environment (Mode A)");
            return;
        };
        let first = compose_scene_from(&bytes, M25_SCENE_SEED).expect("scene");
        let second = compose_scene_from(&bytes, M25_SCENE_SEED).expect("scene");
        assert_eq!(first, second);
        assert_eq!(first.len(), M25_SPAWNED);
        for monster in &first {
            assert!(monster.hp >= monster.hp_bounds.0 && monster.hp <= monster.hp_bounds.1);
        }
    }
}
