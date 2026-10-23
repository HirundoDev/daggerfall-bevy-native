//! M26 probes — living NPCs: AI, dialogue and rumors (SPEC-M26-NPC-AI §8-§9,
//! REQ-L99-059).
//!
//! `PROBE-NPC-AI-001` — pure (0 retail):
//! - **A** population caps: `cap(1)=24 cap(16)=24 cap(32)=48 cap(48)=72
//!   cap(64)=96 cap(1024)=96 tick_ms=100` (§1.1 A1-A8).
//! - **B** units and navigation §2.1 B1-B5.
//! - **C** appearance probabilities §2.2 C1-C6.
//! - **D** 60 s of game (600 ticks of 100 ms): ≥1 cycle
//!   `SeekingTile→MovingForward`, `path_cells > 0`, path digest ×2.
//!
//! `PROBE-DIALOGUE-001` — greeting §3.1 (IDs 7209/7208/7207/7206), topics of 3
//! NPCs from different factions, negative branch of `answersToNonDirections` and
//! stability of the seeded roll. Every emitted ID resolves against
//! `TEXT.RSC`; unreadable file ⇒ **exit 2**.
//!
//! `PROBE-RUMOR-001` — `RUMOR.DAT` census (`records=31 bytes=3506
//! header=34 sum_text=2452`), `GetFlagsForNewRumor` over the 31 records,
//! 1000 seeded selections with weighted reservoir and the F7-F10 boundaries
//! (expiry and region filters). Without a readable `RUMOR.DAT` ⇒ **exit 2**.
//!
//! `NPC-DEMO-001` — demo §9: retail population of `maxPopulation` NPCs, a
//! walk of ≥10 cells with digest ×2, dialogue with 3 NPCs (greeting from the
//! table + an "I don't know" resolved from `TEXT.RSC`), a real rumor from the
//! file and guards ≈ `pool.Count/32`.
//!
//! **Custody (INV-L99-6):** Mode A — retail bytes are read at runtime from the
//! user's ARENA2 and never enter the repository. 0 Bevy, 0 GPU.

use std::path::PathBuf;

use daggerfall_behavior::dialogue::{
    assemble_topics, knowledge_hash, knows_topic, non_direction_answer,
    organization_answer, simple_greeting, BuildingTopic, Greeting, QuestTopic, QuestionType,
    SocialGroup, TalkContext, TopicKind, ANSWERS_TO_DIRECTIONS, ANSWERS_TO_NON_DIRECTIONS,
    GREETINGS,
};
use daggerfall_behavior::npc_ai::{
    cell_meters, expected_guards, margin_cells, max_population, nav_grid_side, roll_appearance,
    seconds_for_cells, tick_millis, Appearance, Gender, NavGrid, Walker, GENDERS, GUARD_ONE_IN,
    OUTFIT_VARIANTS,
};
use daggerfall_parser_bevy_bridge::npc_spec::{
    NpcSpec, NpcWorld, PopulationRequest, CITY_POPULATION,
};
use daggerfall_parser_rs::dfrandom::DfRandom;
use daggerfall_parser_rs::namegen::NameGen;
use daggerfall_parser_rs::quest::RumorFile;
use daggerfall_parser_rs::rumor::{
    flags_for_new_rumor, RumorMill, RumorMillEntry, FLAG_SIGN, FLAG_SPOKEN, QUEST_RUMOR_WEIGHT,
};
use daggerfall_parser_rs::text_rsc::TextRsc;
use daggerfall_parser_rs::texture::{RecordKind, TextureArchive};

// ── evidence rows (M19/M25 template) ────────────────────────────────────────

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

/// Stable number-to-text without trailing zeros (`4.75`, `1.2308`).
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

/// Loads a Mode A ARENA2 file through the shared resolver.
fn load(name: &str) -> Result<(PathBuf, Vec<u8>), String> {
    let path = crate::mode_a_arena2_path(name)?;
    let bytes = std::fs::read(&path).map_err(|err| format!("reading {path:?}: {err}"))?;
    Ok((path, bytes))
}

/// Canonical seed of the M26 scenarios (lane date).
pub const M26_SCENE_SEED: u64 = 20_262_121;
/// Seconds of game in block D (§8).
pub const M26_RUTINE_SECONDS: f32 = 60.0;
/// Demo population seed (same as the lane scenario).
pub const M26_POP_SEED: u32 = 20_262_121;
/// Canonical demo region (17 = Daggerfall).
pub const M26_POP_REGION: u8 = 17;
/// Canonical demo subclimate (231 = Woodlands → Breton).
pub const M26_POP_SUBCLIMATE: u8 = 231;
/// Fixed `infoFactionIDs` table (`TalkManager.cs:378`), 34 entries.
pub const INFO_FACTIONS: [u32; 34] = [
    42, 40, 108, 129, 306, 353, 41, 67, 82, 84, 88, 92, 94, 106, 36, 83, 85, 89, 93, 95, 99, 107,
    37, 368, 408, 409, 410, 411, 413, 414, 415, 416, 417, 98,
];

/// Runs the routine scenario for `seconds` seconds with a 100 ms tick.
fn walk_for(seed: u64, seconds: f32) -> Walker {
    let grid = NavGrid::city(1, seed);
    let mut walker = Walker::new(seed, (3, 32));
    let ticks = (seconds / (tick_millis() as f32 / 1000.0)).round() as usize;
    for _ in 0..ticks {
        walker.step(&grid, tick_millis() as f32 / 1000.0);
    }
    walker
}

// ═══════════════════════════════════════════════════════════════════════════
// PROBE-NPC-AI-001
// ═══════════════════════════════════════════════════════════════════════════

// ── C1-C6 literal anchors (retail/source contract, not sweep identities) ────
//
// Derived from the source constants `GUARD_ONE_IN = 32`, `GENDERS = 2` and
// `OUTFIT_VARIANTS = 4` (`MobilePersonNPC.cs:144-156`), never from the roll
// space the sweep enumerates:
//   combos    = GUARD_ONE_IN · GENDERS · OUTFIT_VARIANTS = 32·2·4 = 256
//   guards    = combos / GUARD_ONE_IN                   = 256 / 32 = 8
//   civilians = combos − guards                         = 248
//   females   = civilians / GENDERS                     = 248 / 2 = 124
//   outfit0   = civilians / OUTFIT_VARIANTS             = 248 / 4 = 62
//   joint     = guards: a guard is forced male + outfit 0, so every guard
//               joins the joint event                     = 8
const C_ANCHOR_COMBOS: usize = 256;
const C_ANCHOR_GUARDS: usize = 8;
const C_ANCHOR_CIVILIANS: usize = 248;
const C_ANCHOR_FEMALES: usize = 124;
const C_ANCHOR_OUTFIT0: usize = 62;
const C_ANCHOR_JOINT: usize = 8;

// Source constants restated so a drift in `behavior` fails this probe loudly
// instead of silently re-deriving the anchors from the sweep loops.
const C_SOURCE_GUARD_ONE_IN: u32 = 32;
const C_SOURCE_GENDERS: u8 = 2;
const C_SOURCE_OUTFIT_VARIANTS: u8 = 4;

/// `PROBE-NPC-AI-001` — population caps, units, appearance and routine.
pub fn run_npc_ai_001() -> i32 {
    println!("PROBE-NPC-AI-001 - living NPCs: population and routine (M26, REQ-L99-059 · spec §8)");

    // ── A · population caps (§1.1 A1-A8) ───────────────────────────────────
    let a_line = format!(
        "cap(1)={} cap(16)={} cap(32)={} cap(48)={} cap(64)={} cap(1024)={} tick_ms={}",
        max_population(1, 1),
        max_population(16, 1),
        max_population(32, 1),
        max_population(48, 1),
        max_population(64, 1),
        max_population(1024, 1),
        tick_millis(),
    );
    println!("{a_line}");
    let mut rows = vec![row(
        "A population caps",
        a_line,
        "cap(1)=24 cap(16)=24 cap(32)=48 cap(48)=72 cap(64)=96 cap(1024)=96 tick_ms=100".to_string(),
        "Clamp((w·h)/16,1,4)·24 with integer division of the quotient · 1/10 s",
    )];

    // ── B · units and navigation (§2.1 B1-B5) ─────────────────────────────
    let b_line = format!(
        "cell={} s_per_cell={:.4} s_10={:.4} nav_cells(8x8)={} margin_cells(2500u)={:.4}",
        fmt_f32(cell_meters()),
        seconds_for_cells(1.0),
        seconds_for_cells(10.0),
        nav_grid_side(8),
        margin_cells(),
    );
    println!("{b_line}");
    rows.push(row(
        "B navigation units",
        b_line,
        "cell=1.6 s_per_cell=1.2308 s_10=12.3077 nav_cells(8x8)=512 margin_cells(2500u)=39.0625"
            .to_string(),
        "64·0.025 m/cell · 1.3 m/s · 8 blocks × 64 cells · 2500 u ⇒ 62.5 m",
    ));

    // ── C · appearance (§2.2 C1-C6) ────────────────────────────────────────
    let p_guard = 1.0 / GUARD_ONE_IN as f32;
    let p_female = 1.0 / GENDERS as f32;
    let p_outfit0 = 1.0 / OUTFIT_VARIANTS as f32;
    let guards = count_guards(M26_SCENE_SEED, max_population(1, 1));
    let c_line = format!(
        "p_guard={} p_female_non_guard={} p_outfit0_non_guard={} joint_guard_male_outfit0={} guards(96)~{}",
        fmt_f32(p_guard),
        fmt_f32(p_female),
        fmt_f32(p_outfit0),
        fmt_f32(p_guard),
        expected_guards(96) as usize,
    );
    println!("{c_line}");
    rows.push(row(
        "C appearance probabilities",
        c_line,
        "p_guard=0.03125 p_female_non_guard=0.5 p_outfit0_non_guard=0.25 joint_guard_male_outfit0=0.03125 guards(96)~3".to_string(),
        "1/32 · 1/2 (non-guard branch) · 1/4 (non-guard branch) · the guard forces male/outfit 0",
    ));
    let sweep = appearance_sweep();
    let civilians = sweep.combos - sweep.guards;
    rows.push(check(
        "C guards drawn",
        GUARD_ONE_IN == C_SOURCE_GUARD_ONE_IN
            && GENDERS == C_SOURCE_GENDERS
            && OUTFIT_VARIANTS == C_SOURCE_OUTFIT_VARIANTS
            && sweep.combos == C_ANCHOR_COMBOS
            && sweep.guards == C_ANCHOR_GUARDS
            && civilians == C_ANCHOR_CIVILIANS
            && sweep.females == C_ANCHOR_FEMALES
            && sweep.outfit0 == C_ANCHOR_OUTFIT0
            && sweep.joint == C_ANCHOR_JOINT,
        format!(
            "sweep of {} rolls ({}×{}×{}): guards={}/{} (anchor {}) · non-guard females={}/{} (anchor {}) · non-guard outfit0={}/{} (anchor {}) · joint={} (anchor {}) · seeded draw guards({})={guards}",
            sweep.combos, GUARD_ONE_IN, GENDERS, OUTFIT_VARIANTS,
            sweep.guards, sweep.combos, C_ANCHOR_GUARDS,
            sweep.females, civilians, C_ANCHOR_FEMALES,
            sweep.outfit0, civilians, C_ANCHOR_OUTFIT0,
            sweep.joint, C_ANCHOR_JOINT,
            max_population(1, 1),
        ),
    ));

    // ── D · observed routine (§2, 60 s = 600 ticks) ────────────────────────
    let a = walk_for(M26_SCENE_SEED, M26_RUTINE_SECONDS);
    let b = walk_for(M26_SCENE_SEED, M26_RUTINE_SECONDS);
    let cells = a.cells_visited();
    println!(
        "routine: seconds={} moves={} path_cells={} cells_visited={} digest_a={:016x} digest_b={:016x}",
        fmt_f32(M26_RUTINE_SECONDS),
        a.moves(),
        a.path().len(),
        cells,
        a.digest(),
        b.digest(),
    );
    rows.push(check(
        "D routine cycles",
        a.moves() >= 1,
        format!("{} SeekingTile→MovingForward transitions", a.moves()),
    ));
    rows.push(check(
        "D path cells",
        !a.path().is_empty() && cells > 1,
        format!("{} path cells · {cells} distinct", a.path().len()),
    ));
    rows.push(row(
        "D digest x2",
        format!("{:016x}", a.digest()),
        format!("{:016x}", b.digest()),
        "same seed ⇒ same path (R-C)",
    ));

    let ok = print_rows("PROBE-NPC-AI-001", &rows);
    println!(
        "VERDICT: PROBE-NPC-AI-001 {}",
        if ok { "PASS" } else { "FAIL" }
    );
    if ok { 0 } else { 1 }
}

/// Deterministic sweep of the appearance roll space (§2.2 C1-C6):
/// `guard_roll ∈ 0..GUARD_ONE_IN` × `gender_roll ∈ 0..GENDERS` × `outfit_roll ∈ 0..OUTFIT_VARIANTS`.
#[derive(Default)]
struct AppearanceSweep {
    combos: usize,
    guards: usize,
    females: usize,
    outfit0: usize,
    joint: usize,
}

fn appearance_sweep() -> AppearanceSweep {
    let mut sweep = AppearanceSweep::default();
    for guard_roll in 0..GUARD_ONE_IN {
        for gender_roll in 0..u32::from(GENDERS) {
            for outfit_roll in 0..u32::from(OUTFIT_VARIANTS) {
                let appearance = roll_appearance(guard_roll, gender_roll, outfit_roll);
                sweep.combos += 1;
                if appearance.guard {
                    sweep.guards += 1;
                    if appearance.gender == Gender::Male && appearance.outfit == 0 {
                        sweep.joint += 1;
                    }
                } else {
                    if appearance.gender == Gender::Female {
                        sweep.females += 1;
                    }
                    if appearance.outfit == 0 {
                        sweep.outfit0 += 1;
                    }
                }
            }
        }
    }
    sweep
}

/// Guards drawn in `count` NPCs (one roll per NPC).
fn count_guards(seed: u64, count: usize) -> usize {
    let mut draws = SplitMixLike::new(seed);
    (0..count)
        .filter(|_| draw_appearance(&mut draws).guard)
        .count()
}

/// Appearance draw with the same sequence as the population.
fn draw_appearance(rng: &mut SplitMixLike) -> Appearance {
    roll_appearance(rng.next() as u32, rng.next() as u32, rng.next() as u32)
}

/// Probe-local SplitMix64 (same sequence as the `behavior` organ).
struct SplitMixLike {
    state: u64,
}

impl SplitMixLike {
    fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    fn next(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// PROBE-DIALOGUE-001
// ═══════════════════════════════════════════════════════════════════════════

/// `PROBE-DIALOGUE-001` — greeting, topics by faction and negative branch.
pub fn run_dialogue_001() -> i32 {
    println!(
        "PROBE-DIALOGUE-001 - NPC dialogue (M26, REQ-L99-059 · spec §8)"
    );
    let (path, bytes) = match load("TEXT.RSC") {
        Ok(loaded) => loaded,
        Err(reason) => {
            println!("ARENA2 not accessible: {reason}");
            println!("reason=missing_text_rsc");
            println!("EXIT=2");
            println!("VERDICT: PROBE-DIALOGUE-001 NO DATA - TEXT.RSC not accessible");
            return 2;
        }
    };
    let text = match TextRsc::parse(&bytes) {
        Ok(text) => text,
        Err(err) => {
            println!("[FAIL] text_rsc_parse - {err}");
            println!("reason=missing_text_rsc");
            println!("EXIT=2");
            println!("VERDICT: PROBE-DIALOGUE-001 NO DATA - unreadable TEXT.RSC");
            return 2;
        }
    };
    println!(
        "[OK] mode_a TEXT.RSC: {} ({} B · {} records)",
        path.display(),
        bytes.len(),
        text.record_count()
    );

    let resolves = |id: u16| text.record_raw(&bytes, id).is_some();
    let mut rows = Vec::new();

    // ── A · simple greeting path (§3.1 D1-D6) ──────────────────────────────
    let reactions = [30i16, 29, 10, 0, -1];
    let records: Vec<u16> = reactions
        .iter()
        .map(|reaction| match simple_greeting(*reaction, false) {
            Greeting::Record(id) => id,
            Greeting::NoAnswer => 0,
        })
        .collect();
    let a_line = format!(
        "path=simple greeting(30)={} greeting(29)={} greeting(10)={} greeting(0)={} greeting(-1)={}",
        records[0], records[1], records[2], records[3], records[4]
    );
    println!("{a_line}");
    rows.push(row(
        "A greeting path",
        a_line,
        "path=simple greeting(30)=7209 greeting(29)=7208 greeting(10)=7208 greeting(0)=7207 greeting(-1)=7206".to_string(),
        "simple path (Province, no quest greeting, ≥ −20, no refusal)",
    ));
    let extra_ok = matches!(simple_greeting(100, false), Greeting::Record(7209))
        && matches!(simple_greeting(-21, false), Greeting::NoAnswer);
    rows.push(check(
        "A greeting edges",
        extra_ok,
        "reaction 100 ⇒ 7209 (no upper branch) · reaction −21 ⇒ no answer",
    ));
    let a_resolvable = records.iter().all(|id| resolves(*id));
    rows.push(check(
        "A greeting resolvable",
        a_resolvable,
        format!("IDs {records:?} present in TEXT.RSC"),
    ));

    // ── B · topics of 3 NPCs from different factions ───────────────────────
    let buildings = [BuildingTopic {
        block_x: 0,
        block_y: 3,
        record_index: 2,
        name: "The Rusty Ogre".to_string(),
        building_type: 3,
    }];
    let quest_topics = [QuestTopic {
        key: "Q0person".to_string(),
        kind: TopicKind::QuestPerson,
        available_for_dialog: true,
        has_entry_in_tell_me_about: true,
        has_entry_in_where_is: true,
        same_location: true,
        is_dialog_partner: false,
    }];
    // DFU adds the fixed table of 34 organizations (`TalkManager.cs:378`); this
    // target also adds the NPC's own faction as a topic — declared M26
    // decision (§6.3: it consumes the faction relation, it does not modify it).
    let npc_factions = [10u32, 11, 12];
    let mut info_lists: Vec<Vec<u32>> = Vec::new();
    for faction in npc_factions {
        let mut list = INFO_FACTIONS.to_vec();
        list.push(faction);
        info_lists.push(list);
    }
    let lists: Vec<_> = info_lists
        .iter()
        .map(|info| {
            assemble_topics(&TalkContext {
                buildings: &buildings,
                quest_topics: &quest_topics,
                info_factions: info,
            })
        })
        .collect();
    let digests: Vec<u64> = lists.iter().map(|list| list.digest()).collect();
    let repeat: Vec<u64> = info_lists
        .iter()
        .map(|info| {
            assemble_topics(&TalkContext {
                buildings: &buildings,
                quest_topics: &quest_topics,
                info_factions: info,
            })
            .digest()
        })
        .collect();
    println!(
        "topics_a={} topics_b={} topics_c={} digest_a={:016x} digest_b={:016x} digest_c={:016x}",
        lists[0].len(),
        lists[1].len(),
        lists[2].len(),
        digests[0],
        digests[1],
        digests[2],
    );
    let non_empty = lists.iter().all(|list| !list.visible_lists_empty());
    let distinct = digests[0] != digests[1] && digests[1] != digests[2] && digests[0] != digests[2];
    rows.push(check(
        "B topics non-empty",
        non_empty,
        "every TellMeAbout/Location list has at least one topic (anti-false-positive i)",
    ));
    rows.push(check(
        "B topics differ by faction",
        distinct,
        "three different own factions ⇒ three different digests",
    ));
    rows.push(row(
        "B topic digest x2",
        format!("{:016x}", fnv1a64(&digests.iter().flat_map(|d| d.to_le_bytes()).collect::<Vec<_>>())),
        format!("{:016x}", fnv1a64(&repeat.iter().flat_map(|d| d.to_le_bytes()).collect::<Vec<_>>())),
        "same context ⇒ same topic list",
    ));

    // ── C · topic without knowledge ⇒ negative branch ──────────────────────
    let npc_group = SocialGroup::Commoners;
    let mut negative: Option<(String, u16, bool)> = None;
    for topic in &lists[0].tell_me_about {
        let question = match topic.kind {
            TopicKind::Organization => QuestionType::OrganizationInfo,
            TopicKind::QuestLocation => QuestionType::QuestLocation,
            TopicKind::QuestPerson => QuestionType::QuestPerson,
            TopicKind::QuestItem => QuestionType::QuestItem,
            TopicKind::AnyNews | TopicKind::WhereAmI | TopicKind::LocalBuilding => continue,
        };
        let hash = knowledge_hash(10, &topic.key);
        if !knows_topic(hash, question, npc_group) {
            let answer = non_direction_answer(false, npc_group, 1);
            let text_ok = resolves(answer);
            negative = Some((topic.key.clone(), answer, text_ok));
            break;
        }
    }
    match &negative {
        Some((key, answer, text_ok)) => {
            println!(
                "negative_branch answersToNonDirections topic={key} answer_id={answer} text_ok={text_ok}"
            );
            let in_table = ANSWERS_TO_NON_DIRECTIONS.contains(answer);
            rows.push(check(
                "C negative branch",
                in_table,
                format!("{answer} ∈ answersToNonDirections (0..15)"),
            ));
            rows.push(check(
                "C answer is retail text",
                *text_ok,
                "the ID resolves in TEXT.RSC: 0 invented text (anti-false-positive ii)",
            ));
        }
        None => {
            println!("negative_branch none");
            rows.push(check(
                "C negative branch",
                false,
                "no topic without knowledge in the list — the roll does not hide the branch",
            ));
        }
    }
    let all_answer_ids_resolvable = ANSWERS_TO_DIRECTIONS
        .iter()
        .chain(ANSWERS_TO_NON_DIRECTIONS.iter())
        .copied()
        .chain(GREETINGS.iter().copied())
        .chain((0..INFO_FACTIONS.len()).map(organization_answer))
        .all(&resolves);
    let emitted_topic_ids_resolvable = lists
        .iter()
        .flat_map(|list| [&list.tell_me_about, &list.location, &list.person, &list.thing])
        .flatten()
        .filter_map(|topic| topic.text_id)
        .all(&resolves);
    rows.push(check(
        "C all emitted ids resolve",
        all_answer_ids_resolvable && emitted_topic_ids_resolvable,
        "all 4 ID tables (directions, non-directions, greetings, organizations 0..33) and every text_id emitted in the 3 lists resolve in TEXT.RSC; the 35th faction (the NPC's own) has no fixed record and travels without text_id",
    ));

    // ── D · stability of the same question ────────────────────────────────
    let hash = knowledge_hash(10, "org:42");
    let knowledge_a = knows_topic(hash, QuestionType::OrganizationInfo, npc_group);
    let knowledge_b = knows_topic(hash, QuestionType::OrganizationInfo, npc_group);
    let answer_a = non_direction_answer(knowledge_a, npc_group, 1);
    let answer_b = non_direction_answer(knowledge_b, npc_group, 1);
    rows.push(row(
        "D stable answer",
        format!("{knowledge_a}/{answer_a}"),
        format!("{knowledge_b}/{answer_b}"),
        "roll seeded by (NPC, topic): repeating the question does not change it",
    ));

    let ok = print_rows("PROBE-DIALOGUE-001", &rows);
    println!(
        "VERDICT: PROBE-DIALOGUE-001 {}",
        if ok { "PASS" } else { "FAIL" }
    );
    if ok { 0 } else { 1 }
}

// ═══════════════════════════════════════════════════════════════════════════
// PROBE-RUMOR-001
// ═══════════════════════════════════════════════════════════════════════════

/// `PROBE-RUMOR-001` — census, channels, weighted reservoir and expiry.
pub fn run_rumor_001() -> i32 {
    println!(
        "PROBE-RUMOR-001 - rumor mill from RUMOR.DAT (M26, REQ-L99-059 · spec §8)"
    );
    let (path, bytes) = match load("RUMOR.DAT") {
        Ok(loaded) => loaded,
        Err(reason) => {
            println!("ARENA2 not accessible: {reason}");
            println!("reason=missing_rumor_dat");
            println!("EXIT=2");
            println!("VERDICT: PROBE-RUMOR-001 NO DATA - RUMOR.DAT not accessible");
            return 2;
        }
    };
    let file = match RumorFile::parse(&bytes) {
        Ok(file) => file,
        Err(err) => {
            println!("[FAIL] rumor_parse - {err}");
            println!("reason=missing_rumor_dat");
            println!("EXIT=2");
            println!("VERDICT: PROBE-RUMOR-001 NO DATA - unreadable RUMOR.DAT");
            return 2;
        }
    };
    println!("[OK] mode_a RUMOR.DAT: {}", path.display());
    let mut rows = Vec::new();

    // ── A · census (F1) ───────────────────────────────────────────────────
    let sum_text: usize = file
        .rumors
        .iter()
        .map(|rumor| usize::from(rumor.text_length))
        .sum();
    let a_line = format!(
        "records={} bytes={} header={} sum_text={}",
        file.rumors.len(),
        bytes.len(),
        daggerfall_parser_rs::quest::RUMOR_HEADER_LEN,
        sum_text
    );
    println!("{a_line}");
    rows.push(row(
        "A census",
        a_line,
        "records=31 bytes=3506 header=34 sum_text=2452".to_string(),
        "34·31 + Σ textLength = 3506",
    ));
    let texts_match = file
        .rumors
        .iter()
        .zip(file.texts.iter())
        .all(|(rumor, text)| text.len() == usize::from(rumor.text_length) && rumor.text_length > 0);
    rows.push(check(
        "A text lengths",
        texts_match,
        "each record carries exactly `textLength` bytes of text (NUL included)",
    ));

    // ── B · channels by type (F2/F3) ──────────────────────────────────────
    let types = [4u8, 7, 12, 26, 27];
    let flags: Vec<u8> = types.iter().map(|kind| flags_for_new_rumor(*kind)).collect();
    let b_line = format!(
        "types {{{},{},{},{},{}}} -> {{{},{},{},{},{}}}",
        types[0], types[1], types[2], types[3], types[4], flags[0], flags[1], flags[2], flags[3], flags[4]
    );
    println!("{b_line}");
    println!("type10={}", flags_for_new_rumor(10));
    rows.push(row(
        "B flags by type",
        format!("{flags:?}"),
        "[1, 1, 8, 1, 1]".to_string(),
        "types {4,7,12,26,27} → {sign, sign, spoken, sign, sign}",
    ));
    rows.push(row(
        "B type10",
        flags_for_new_rumor(10).to_string(),
        "1".to_string(),
        "witch burnings ⇒ sign",
    ));
    let sign_count = file.rumors.iter().filter(|r| r.flags & FLAG_SIGN != 0).count();
    let spoken_count = file
        .rumors
        .iter()
        .filter(|r| r.flags & FLAG_SPOKEN != 0)
        .count();
    rows.push(row(
        "B channel census",
        format!("sign_count={sign_count} spoken_count={spoken_count}"),
        "sign_count=26 spoken_count=5".to_string(),
        "file flags: 1×26 / 8×5",
    ));
    let mill = RumorMill::from_file(&file);
    rows.push(check(
        "B imported rumors",
        mill.len() == spoken_count,
        format!(
            "{} entries imported (`Flags & 4/1` and `NPCID != 0` discarded)",
            mill.len()
        ),
    ));

    // ── C · weighted reservoir (F4) ───────────────────────────────────────
    let mut weighted = RumorMill::new();
    weighted.add_quest_rumor(1, None);
    weighted.add_quest_rumor(2, None);
    weighted.add_non_quest_rumor(0, 0, 0, 12, 0, 0);
    let (p_first, p_second, p_ambient, digest_a) = measure_selection(&weighted);
    let (_, _, _, digest_b) = measure_selection(&weighted);
    println!(
        "p_quest={:.5} p_quest_second={:.5} p_ambient={:.5} digest_a={digest_a:016x} digest_b={digest_b:016x}",
        p_first, p_second, p_ambient
    );
    let c_ok = (p_first - 0.495_05).abs() <= 0.03
        && (p_second - 0.495_05).abs() <= 0.03
        && (p_ambient - 0.009_9).abs() <= 0.01;
    rows.push(check(
        "C weighted probabilities",
        c_ok,
        format!(
            "2 quest (weight {QUEST_RUMOR_WEIGHT}) + 1 ambient (weight 1) ⇒ 50/101 and 1/101 in 1000 seeded rolls"
        ),
    ));
    rows.push(row(
        "C digest x2",
        format!("{digest_a:016x}"),
        format!("{digest_b:016x}"),
        "deterministic histogram (explicit seed, anti-false-positive iii)",
    ));

    // ── D · expiry and filters (F7-F10) ───────────────────────────────────
    let mut expiry = RumorMill::new();
    expiry.add_non_quest_rumor(0, 0, 0, 12, 0, 0);
    let mut eq_survives = false;
    let mut expires = false;
    if let Some(entry) = expiry.entries().first() {
        let limit = entry.time_limit;
        let mut copy = expiry.clone();
        copy.refresh(limit);
        eq_survives = copy.len() == 1;
        copy.refresh(limit + 1);
        expires = copy.is_empty();
    }
    let mut filters = RumorMill::new();
    filters.add_non_quest_rumor(0, 0, 60, 12, 0, 0);
    filters.add_non_quest_rumor(0, 0, -1, 12, 0, 0);
    let mut rng = DfRandom::new();
    rng.srand(7);
    let kept_region0 = filters.valid(0, false, |_| false, &mut rng);
    let region60_dropped = kept_region0.iter().all(|entry| entry.region == Some(-1));
    let region_minus1_kept = kept_region0.iter().any(|entry| entry.region == Some(-1));
    println!(
        "eq_survives={eq_survives} now+1_expires={expires} region60_dropped={region60_dropped} region-1_kept={region_minus1_kept}"
    );
    rows.push(check(
        "D expiry boundaries",
        eq_survives && expires,
        "strict `timeLimit < now`: equal ⇒ lives, +1 ⇒ expires",
    ));
    rows.push(check(
        "D region filter",
        region60_dropped && region_minus1_kept,
        "region 60 dropped with current region 0 · `regionID == −1` kept",
    ));

    let ok = print_rows("PROBE-RUMOR-001", &rows);
    println!(
        "VERDICT: PROBE-RUMOR-001 {}",
        if ok { "PASS" } else { "FAIL" }
    );
    if ok { 0 } else { 1 }
}

/// 1000 seeded selections: per-entry probabilities + histogram
/// digest.
fn measure_selection(mill: &RumorMill) -> (f64, f64, f64, u64) {
    let mut rng = DfRandom::new();
    rng.srand(M26_SCENE_SEED as u32);
    let mut first = 0usize;
    let mut second = 0usize;
    let mut ambient = 0usize;
    const DRAWS: usize = 1_000;
    for _ in 0..DRAWS {
        let valid: Vec<&RumorMillEntry> = mill.entries().iter().collect();
        if let Some(picked) = RumorMill::select_weighted(&valid, QUEST_RUMOR_WEIGHT, &mut rng) {
            match picked.quest_id {
                1 => first += 1,
                2 => second += 1,
                _ => ambient += 1,
            }
        }
    }
    let digest = fnv1a64(
        &[
            first as u32,
            second as u32,
            ambient as u32,
        ]
        .iter()
        .flat_map(|value| value.to_le_bytes())
        .collect::<Vec<_>>(),
    );
    (
        first as f64 / DRAWS as f64,
        second as f64 / DRAWS as f64,
        ambient as f64 / DRAWS as f64,
        digest,
    )
}

// ═══════════════════════════════════════════════════════════════════════════
// NPC-DEMO-001 — demo §9
// ═══════════════════════════════════════════════════════════════════════════

/// `NPC-DEMO-001` — demo §9 with population, dialogue and retail rumors.
pub fn run_demo_001() -> i32 {
    println!("NPC-DEMO-001 - M26 demo: living NPCs, dialogue and rumors (spec §9)");

    let namegen_bytes = match load("NAMEGEN.DAT") {
        Ok((_, bytes)) => bytes,
        Err(reason) => return demo_missing("NAMEGEN.DAT", &reason),
    };
    let texture_bytes = match load("TEXTURE.256") {
        Ok((_, bytes)) => bytes,
        Err(reason) => return demo_missing("TEXTURE.256", &reason),
    };
    let rsc_bytes = match load("TEXT.RSC") {
        Ok((_, bytes)) => bytes,
        Err(reason) => return demo_missing("TEXT.RSC", &reason),
    };
    let rumor_bytes = match load("RUMOR.DAT") {
        Ok((_, bytes)) => bytes,
        Err(reason) => return demo_missing("RUMOR.DAT", &reason),
    };

    let namegen = match NameGen::parse(&namegen_bytes) {
        Ok(namegen) => namegen,
        Err(err) => {
            println!("[FAIL] namegen_parse - {err}");
            return 2;
        }
    };
    let flats = match TextureArchive::parse(&texture_bytes) {
        Ok(flats) => flats,
        Err(err) => {
            println!("[FAIL] flats_parse - {err:?}");
            return 2;
        }
    };
    let text = match TextRsc::parse(&rsc_bytes) {
        Ok(text) => text,
        Err(err) => {
            println!("[FAIL] text_rsc_parse - {err}");
            return 2;
        }
    };
    let rumor_file = match RumorFile::parse(&rumor_bytes) {
        Ok(file) => file,
        Err(err) => {
            println!("[FAIL] rumor_parse - {err}");
            return 2;
        }
    };
    let images = flats
        .records
        .iter()
        .filter(|header| matches!(header.kind, RecordKind::Image(_)))
        .count();
    println!("flats: TEXTURE.256 records={} images={images}", flats.records.len());

    let mut rows = Vec::new();

    // ── 1 · population = maxPopulation ────────────────────────────────────
    let cap = max_population(1, 1);
    let world = NpcWorld {
        namegen: &namegen,
        person_flats: &flats,
        text: &text,
        text_bytes: &rsc_bytes,
    };
    let population = match world.populate(PopulationRequest {
        seed: M26_POP_SEED,
        region: M26_POP_REGION,
        subclimate: M26_POP_SUBCLIMATE,
        count: cap,
    }) {
        Ok(population) => population,
        Err(err) => {
            println!("[FAIL] population - {err}");
            return 1;
        }
    };
    let n = population.npcs.len();
    println!("city: N={n} cap={cap} pool.Count={} (CITY_POPULATION={CITY_POPULATION})", population.npcs.len());
    rows.push(check(
        "1 population",
        n == cap && n == max_population(1, 1),
        format!("{n} NPCs = maxPopulation({cap}); pool.Count = {n}"),
    ));
    rows.push(check(
        "1 population determinism",
        world
            .populate(PopulationRequest {
                seed: M26_POP_SEED,
                region: M26_POP_REGION,
                subclimate: M26_POP_SUBCLIMATE,
                count: cap,
            })
            .map(|again| again.digest() == population.digest())
            .unwrap_or(false),
        "same request ⇒ same population digest (M15)",
    ));

    // ── 2 · walk ≥10 cells with digest ×2 ─────────────────────────────────
    let walker_a = walk_for(M26_SCENE_SEED, M26_RUTINE_SECONDS);
    let walker_b = walk_for(M26_SCENE_SEED, M26_RUTINE_SECONDS);
    let cells = walker_a.cells_visited();
    println!(
        "walk: path_cells={} cells_visited={} moves={} digest_a={:016x} digest_b={:016x}",
        walker_a.path().len(),
        cells,
        walker_a.moves(),
        walker_a.digest(),
        walker_b.digest()
    );
    rows.push(check(
        "2 walk >= 10 cells",
        cells >= 10 && walker_a.path().len() >= 10,
        format!(
            "{cells} distinct cells ({:.1} m of walk over a 1.6 m navgrid)",
            cells as f32 * cell_meters()
        ),
    ));
    rows.push(row(
        "2 walk digest x2",
        format!("{:016x}", walker_a.digest()),
        format!("{:016x}", walker_b.digest()),
        "N-4: the building→street→building routine is a target contract, not DFU fidelity",
    ));

    // ── 3 · dialogue with 3 NPCs ──────────────────────────────────────────
    let buildings = [BuildingTopic {
        block_x: 0,
        block_y: 1,
        record_index: 0,
        name: "The Rusty Ogre".to_string(),
        building_type: 3,
    }];
    let quest_topics: [QuestTopic; 0] = [];
    let mut dialogue_ok = true;
    let mut greetings_ok = true;
    let mut no_answer = 0usize;
    for npc in population.npcs.iter().take(3) {
        let mut info = INFO_FACTIONS.to_vec();
        info.push(100 + npc.index as u32);
        let lists = assemble_topics(&TalkContext {
            buildings: &buildings,
            quest_topics: &quest_topics,
            info_factions: &info,
        });
        let greeting = simple_greeting(30, false);
        let greeting_id = match greeting {
            Greeting::Record(id) => id,
            Greeting::NoAnswer => 0,
        };
        greetings_ok &= [7209u16, 7208, 7207, 7206].contains(&greeting_id);
        dialogue_ok &= !lists.visible_lists_empty();

        // "I don't know": first organization topic the NPC does not know.
        let mut found = None;
        for topic in &lists.tell_me_about {
            let question = match topic.kind {
                TopicKind::Organization => QuestionType::OrganizationInfo,
                _ => continue,
            };
            let hash = knowledge_hash(npc.index as u64, &topic.key);
            if !knows_topic(hash, question, SocialGroup::Commoners) {
                let answer = non_direction_answer(false, SocialGroup::Commoners, 1);
                if text.record_raw(&rsc_bytes, answer).is_some() {
                    found = Some((topic.key.clone(), answer));
                    break;
                }
            }
        }
        match found {
            Some((key, answer)) => {
                no_answer += 1;
                println!(
                    "talk npc={} topics={} greeting_id={greeting_id} unknown={key} answer_id={answer}",
                    npc.index,
                    lists.len()
                );
            }
            None => {
                println!(
                    "talk npc={} topics={} greeting_id={greeting_id} unknown=none",
                    npc.index,
                    lists.len()
                );
            }
        }
    }
    rows.push(check(
        "3 dialogue topics",
        dialogue_ok,
        "three NPCs with visible topics (news, where am I, 34 organizations)",
    ));
    rows.push(check(
        "3 greeting table",
        greetings_ok,
        "greeting with ID from the §3.1 table (7209)",
    ));
    rows.push(check(
        "3 at least one no-answer",
        no_answer >= 1,
        format!("{no_answer}/3 negative answers resolved from TEXT.RSC"),
    ));

    // ── 4 · real rumor from RUMOR.DAT ─────────────────────────────────────
    let mill = RumorMill::from_file(&rumor_file);
    let text_lengths_ok = rumor_file
        .rumors
        .iter()
        .zip(rumor_file.texts.iter())
        .all(|(rumor, text)| text.len() == usize::from(rumor.text_length));
    // Rumor from another region: filtered out when querying the current region.
    let foreign = rumor_file
        .rumors
        .iter()
        .position(|rumor| rumor.region != 0 && rumor.flags & FLAG_SPOKEN != 0);
    let mut filtered_out = true;
    let mut rumor_line = String::from("rumor=none");
    if let Some(index) = foreign {
        let mut probe_mill = RumorMill::new();
        if let Some(entry) = RumorMillEntry::from_record(
            &rumor_file.rumors[index],
            rumor_file.text(index),
        ) {
            probe_mill.add_entry(entry);
        }
        let mut rng = DfRandom::new();
        rng.srand(M26_SCENE_SEED as u32);
        let kept = probe_mill.valid(0, false, |_| false, &mut rng);
        filtered_out = kept.is_empty();
        rumor_line = format!(
            "records={} bytes={} region={} text_len={} filtered_out={filtered_out}",
            rumor_file.rumors.len(),
            rumor_bytes.len(),
            rumor_file.rumors[index].region,
            rumor_file.rumors[index].text_length,
        );
    }
    println!("rumor: {rumor_line}");
    rows.push(check(
        "4 rumor census",
        rumor_file.rumors.len() == 31 && rumor_bytes.len() == 3506 && text_lengths_ok,
        "31 records · 3506 B · text == textLength in 31/31",
    ));
    rows.push(check(
        "4 foreign region filtered",
        filtered_out,
        "a rumor from a different region does not appear when querying region 0 (F9)",
    ));
    rows.push(check(
        "4 mill loaded",
        mill.len() == 5,
        format!("{} spoken rumors imported from the file", mill.len()),
    ));

    // ── 5 · guards ≈ pool.Count/32 ────────────────────────────────────────
    let draw = count_guards_demo(M26_SCENE_SEED, &population.npcs);
    println!(
        "guards: count={} expected={:.2} all_male_outfit0={}",
        draw.0,
        expected_guards(n),
        draw.1
    );
    rows.push(check(
        "5 guards",
        draw.0 <= n / 8 && draw.1,
        format!(
            "{} guards in {n} NPCs (expected {:.2}); all male with outfit 0",
            draw.0,
            expected_guards(n)
        ),
    ));

    let ok = print_rows("NPC-DEMO-001", &rows);
    println!("VERDICT: NPC-DEMO-001 {}", if ok { "PASS" } else { "FAIL" });
    if ok { 0 } else { 1 }
}

/// Demo guards over the population: one roll per NPC; returns
/// `(count, all_male_outfit_0)`.
fn count_guards_demo(seed: u64, npcs: &[NpcSpec]) -> (usize, bool) {
    let mut rng = SplitMixLike::new(seed);
    let mut count = 0usize;
    let mut uniform = true;
    for npc in npcs {
        let appearance = roll_appearance(
            rng.next() as u32,
            u32::from(npc.gender == daggerfall_parser_rs::namegen::Gender::Female),
            rng.next() as u32,
        );
        if appearance.guard {
            count += 1;
            uniform &= appearance.gender == daggerfall_behavior::npc_ai::Gender::Male
                && appearance.outfit == 0;
        }
    }
    (count, uniform)
}

/// Demo `exit 2` output with the missing file.
fn demo_missing(name: &str, reason: &str) -> i32 {
    println!("ARENA2 not accessible ({name}): {reason}");
    println!("reason=missing_retail_data");
    println!("EXIT=2");
    println!("VERDICT: NPC-DEMO-001 NO DATA - {name} not accessible");
    2
}
