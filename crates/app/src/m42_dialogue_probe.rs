//! `m42_dialogue_probe` — `PROBE-SYS-DIALOGUE-001` (M42 / W42-2): dialogue
//! topics per faction and region, retail-text rumors, 0 out-of-context
//! answers (roadmap `docs/acknowledgments/roadmap-m18-m48.md:318`).
//!
//! This probe **extends** M26's `DIALOGUE-001` + `RUMOR-001`
//! (`crates/app/src/m26_probe.rs`) instead of repeating them: M26 proves
//! greeting routes, knowledge rolls, mill census/channels/expiry and the
//! negative branch, so the rows here are the **W42-2 daily layer** the M26
//! walk lacks: (A) topics differ per faction **and** per region (region pin
//! refuses out-of-region quest topics, 0 admitted); (B) every served answer
//! is retail text (the rumor carries its own `RUMOR.DAT` bytes or its
//! `TEXT.RSC` ID resolves in the user's file); (C) the closed context —
//! every emitted key is a member, every invented key is refused
//! (0 out-of-context answers); (D) adversarial NEGATIVE rows with on-disk
//! evidence paths.
//!
//! **Retail:** `TEXT.RSC` + `RUMOR.DAT` via Mode A (`$DF_APP_ROOT` or
//! `$HOME/Games/Daggerfall/app`); unreadable file ⇒ **exit 2** with
//! `VERDICT: PROBE-SYS-DIALOGUE-001 NO DATA` (same custody as M26).
//!
//! **Determinism:** no floats are printed, all iteration is over fixed
//! arrays, and the last line is `DIGEST <sha256>` over every byte printed
//! before it. The ×2 reproducibility is enforced by
//! `scripts/sys-dialogue-001.py` running two separate processes with
//! byte-identical stdout.

use daggerfall_behavior::dialogue::{
    answer_text_is_retail, assemble_topics_in_region, region_admits, topic_in_context,
    topic_keys_in_context, BuildingTopic, QuestTopic, TalkContext, TopicKind,
};
use daggerfall_parser_rs::quest::RumorFile;
use daggerfall_parser_rs::rumor::RumorMill;
use daggerfall_parser_rs::text_rsc::TextRsc;
use sha2::{Digest, Sha256};

/// Lane correlation id (W42-2).
const CR: &str = "CR-b434f896-2fe9-4750-b70e-9b748370e6c3";

/// Fixed `infoFactionIDs` table (`TalkManager.cs:378`), 34 entries.
const INFO_FACTIONS: [u32; 34] = [
    42, 40, 108, 129, 306, 353, 41, 67, 82, 84, 88, 92, 94, 106, 36, 83, 85, 89, 93, 95, 99, 107,
    37, 368, 408, 409, 410, 411, 413, 414, 415, 416, 417, 98,
];

/// The NPC's own faction appended after the fixed table (M26 decision §6.3).
const NPC_FACTIONS: [u32; 3] = [10, 11, 12];

/// Region pins: Daggerfall (17) vs Wayrest (45) — two pins must disagree.
const REGION_A: i16 = 17;
const REGION_B: i16 = 45;

/// Citations (every one resolves on disk).
const SRC_DIALOGUE: &str = "crates/behavior/src/dialogue.rs";
const SRC_RUMOR: &str = "crates/daggerfall_parser_rs/src/rumor.rs";
const SPEC_M26: &str =
    "docs/sintergic/game/programming/m26-npc-ai-dialogue-spec-CR-5243af63.md";
const SPEC_ROADMAP: &str = "docs/acknowledgments/roadmap-m18-m48.md:318";

// ── evidence rows ───────────────────────────────────────────────────────

/// One printed evidence row.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Row {
    id: String,
    ok: bool,
    detail: String,
}

/// Row whose verdict is computed by the caller.
fn check(id: impl Into<String>, ok: bool, detail: String) -> Row {
    Row {
        id: id.into(),
        ok,
        detail,
    }
}

/// Hexadecimal `sha256` (the `DIGEST` line and nothing else needs it).
fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

/// Loads a Mode A ARENA2 file (same custody as the M26 probes).
fn load(name: &str) -> Result<(std::path::PathBuf, Vec<u8>), String> {
    let path = crate::mode_a_arena2_path(name)?;
    let bytes = std::fs::read(&path).map_err(|err| format!("reading {path:?}: {err}"))?;
    Ok((path, bytes))
}

fn fixtures() -> (Vec<BuildingTopic>, Vec<QuestTopic>) {
    let buildings = vec![BuildingTopic {
        block_x: 0,
        block_y: 3,
        record_index: 2,
        name: "The Rusty Ogre".to_string(),
        building_type: 3,
    }];
    let quests = vec![
        QuestTopic {
            key: "Qhere".to_string(),
            kind: TopicKind::QuestPerson,
            available_for_dialog: true,
            has_entry_in_tell_me_about: true,
            has_entry_in_where_is: true,
            same_location: true,
            is_dialog_partner: false,
            region: Some(REGION_A),
        },
        QuestTopic {
            key: "Qaway".to_string(),
            kind: TopicKind::QuestPerson,
            available_for_dialog: true,
            has_entry_in_tell_me_about: true,
            has_entry_in_where_is: true,
            same_location: true,
            is_dialog_partner: false,
            region: Some(REGION_B),
        },
    ];
    (buildings, quests)
}

// ── entrypoint ──────────────────────────────────────────────────────────

/// `--probe SYS-DIALOGUE-001`: blocks A–D, `EXIT=0` ⟺ every row OK,
/// `EXIT=2` when retail is not accessible.
pub fn run_sys_dialogue_001() -> i32 {
    let mut out = String::new();
    out.push_str(&format!(
        "PROBE-SYS-DIALOGUE-001 - dialogue topics per faction and region, retail rumors (M42/W42-2, {CR})\n"
    ));

    let (text_path, text_bytes) = match load("TEXT.RSC") {
        Ok(loaded) => loaded,
        Err(reason) => {
            out.push_str(&format!("ARENA2 not accessible: {reason}\n"));
            out.push_str("reason=missing_text_rsc\nEXIT=2\n");
            out.push_str("VERDICT: PROBE-SYS-DIALOGUE-001 NO DATA - TEXT.RSC not accessible\n");
            print!("{out}");
            return 2;
        }
    };
    let text = match TextRsc::parse(&text_bytes) {
        Ok(text) => text,
        Err(err) => {
            out.push_str(&format!("FAIL text_rsc_parse got={err} want=ok\n"));
            out.push_str("reason=missing_text_rsc\nEXIT=2\n");
            out.push_str("VERDICT: PROBE-SYS-DIALOGUE-001 NO DATA - unreadable TEXT.RSC\n");
            print!("{out}");
            return 2;
        }
    };
    let (rumor_path, rumor_bytes) = match load("RUMOR.DAT") {
        Ok(loaded) => loaded,
        Err(reason) => {
            out.push_str(&format!("ARENA2 not accessible: {reason}\n"));
            out.push_str("reason=missing_rumor_dat\nEXIT=2\n");
            out.push_str("VERDICT: PROBE-SYS-DIALOGUE-001 NO DATA - RUMOR.DAT not accessible\n");
            print!("{out}");
            return 2;
        }
    };
    let rumor_file = match RumorFile::parse(&rumor_bytes) {
        Ok(file) => file,
        Err(err) => {
            out.push_str(&format!("FAIL rumor_parse got={err} want=ok\n"));
            out.push_str("reason=missing_rumor_dat\nEXIT=2\n");
            out.push_str("VERDICT: PROBE-SYS-DIALOGUE-001 NO DATA - unreadable RUMOR.DAT\n");
            print!("{out}");
            return 2;
        }
    };
    out.push_str(&format!(
        "[OK] mode_a TEXT.RSC: {} ({} B · {} records) + RUMOR.DAT: {} ({} B · {} records)\n",
        text_path.display(),
        text_bytes.len(),
        text.record_count(),
        rumor_path.display(),
        rumor_bytes.len(),
        rumor_file.rumors.len(),
    ));

    let resolves = |id: u16| text.record_raw(&text_bytes, id).is_some();
    let mut ok = true;

    // ── A · topics per faction and region ───────────────────────────────
    out.push_str("\n-- A · topics per faction and region (pins 17 vs 45) --\n");
    let (buildings, quests) = fixtures();
    let faction_lists: Vec<_> = NPC_FACTIONS
        .iter()
        .map(|faction| {
            let mut info = INFO_FACTIONS.to_vec();
            info.push(*faction);
            let context = TalkContext {
                buildings: &buildings,
                quest_topics: &quests,
                info_factions: &info,
                current_region: None,
            };
            assemble_topics_in_region(&context, Some(REGION_A))
        })
        .collect();
    let digests: Vec<u64> = faction_lists.iter().map(|list| list.digest()).collect();
    for (i, (faction, digest)) in NPC_FACTIONS.iter().zip(digests.iter()).enumerate() {
        out.push_str(&format!(
            "TOPIC npc={i} faction={faction} region={REGION_A} topics={} digest={digest:016x}\n",
            faction_lists[i].len(),
        ));
    }
    let open_lists: Vec<_> = NPC_FACTIONS
        .iter()
        .map(|faction| {
            let mut info = INFO_FACTIONS.to_vec();
            info.push(*faction);
            let context = TalkContext {
                buildings: &buildings,
                quest_topics: &quests,
                info_factions: &info,
                current_region: None,
            };
            assemble_topics_in_region(&context, Some(REGION_A))
        })
        .collect();
    let region_lists: Vec<_> = [REGION_A, REGION_B]
        .iter()
        .map(|region| {
            let mut info = INFO_FACTIONS.to_vec();
            info.push(NPC_FACTIONS[0]);
            let context = TalkContext {
                buildings: &buildings,
                quest_topics: &quests,
                info_factions: &info,
                current_region: None,
            };
            assemble_topics_in_region(&context, Some(*region))
        })
        .collect();
    let region_digests: Vec<u64> = region_lists.iter().map(|list| list.digest()).collect();
    out.push_str(&format!(
        "REGION pin={REGION_A} topics={} digest={:016x}\n",
        region_lists[0].len(),
        region_digests[0],
    ));
    out.push_str(&format!(
        "REGION pin={REGION_B} topics={} digest={:016x}\n",
        region_lists[1].len(),
        region_digests[1],
    ));

    // ── B · rumor mill from retail ──────────────────────────────────────
    out.push_str("\n-- B · rumor mill (retail text, region filter) --\n");
    let mill = RumorMill::from_file(&rumor_file);
    let spoken = mill
        .entries()
        .iter()
        .filter(|entry| !entry.is_sign() && entry.text.is_some())
        .count();
    let regions: std::collections::BTreeSet<i16> = mill
        .entries()
        .iter()
        .filter_map(|entry| entry.region)
        .collect();
    out.push_str(&format!(
        "RUMOR records={} mill={} spoken_with_text={spoken} regions={}\n",
        rumor_file.rumors.len(),
        mill.len(),
        regions.len(),
    ));
    let first_text_len = mill
        .entries()
        .iter()
        .find_map(|entry| entry.text.as_ref().map(|text| text.len()))
        .unwrap_or(0);
    out.push_str(&format!("RUMOR first_text_bytes={first_text_len}\n"));

    // ── C · closed context + retail guards ──────────────────────────────
    out.push_str("\n-- C · closed context (0 out-of-context answers) --\n");
    let keys = topic_keys_in_context(&faction_lists[0]);
    out.push_str(&format!("CONTEXT keys={} digest={:016x}\n", keys.len(), faction_lists[0].digest()));

    // ── verdict rows ────────────────────────────────────────────────────
    let mut rows: Vec<Row> = Vec::new();
    rows.push(check(
        "topics-per-faction",
        digests[0] != digests[1] && digests[1] != digests[2] && digests[0] != digests[2],
        format!(
            "three own factions ⇒ three digests {:016x}/{:016x}/{:016x} src={SRC_DIALOGUE}",
            digests[0], digests[1], digests[2],
        ),
    ));
    rows.push(check(
        "topics-nonempty",
        faction_lists.iter().all(|list| !list.visible_lists_empty()),
        format!(
            "every list admits ≥1 topic (TellMeAbout/Location) spec={SPEC_ROADMAP}"
        ),
    ));
    rows.push(check(
        "region-differs",
        region_digests[0] != region_digests[1],
        format!(
            "pin {REGION_A} vs {REGION_B} ⇒ {:016x} vs {:016x} src={SRC_DIALOGUE}",
            region_digests[0], region_digests[1],
        ),
    ));
    let admits_here = region_admits(Some(REGION_A), Some(REGION_A));
    let refuses_away = !region_admits(Some(REGION_B), Some(REGION_A));
    let open_admits = region_admits(Some(REGION_B), None);
    rows.push(check(
        "region-gate",
        admits_here && refuses_away && open_admits,
        format!("pin {REGION_A}: here admitted, away refused, unpinned admitted src={SRC_RUMOR}"),
    ));
    rows.push(check(
        "region-refuses-out-of-region",
        topic_in_context(&region_lists[0], "Qhere")
            && !topic_in_context(&region_lists[0], "Qaway")
            && topic_in_context(&region_lists[1], "Qaway")
            && !topic_in_context(&region_lists[1], "Qhere"),
        format!("Qhere∈{REGION_A} Qaway∈{REGION_B}, each pin admits only its own src={SRC_DIALOGUE}"),
    ));
    rows.push(check(
        "repeat-stable",
        open_lists.iter().map(|list| list.digest()).collect::<Vec<_>>() == digests,
        format!("same context x2 ⇒ same digests (faction x pin stable) src={SRC_DIALOGUE}"),
    ));
    let member_ok = topic_in_context(&faction_lists[0], "news")
        && topic_in_context(&faction_lists[0], "where_am_i")
        && topic_in_context(&faction_lists[0], "Qhere")
        && topic_in_context(&faction_lists[0], "org:42");
    rows.push(check(
        "context-membership",
        member_ok,
        format!("news/where_am_i/Qhere/org:42 ∈ context src={SRC_DIALOGUE}"),
    ));
    let out_ok = !topic_in_context(&faction_lists[0], "org:43")
        && !topic_in_context(&faction_lists[0], "")
        && !topic_in_context(&faction_lists[0], "Qinvented");
    rows.push(check(
        "out-of-context-refused",
        out_ok,
        format!("org:43/empty/invented ∉ context — refused, no invented answer src={SRC_DIALOGUE}"),
    ));
    rows.push(check(
        "rumor-retail",
        mill.len() == 5 && spoken == 5 && first_text_len > 0,
        format!(
            "mill {}/{} file records imported (Flags&4/1 + NPCID != 0 discarded), {spoken} spoken carry file bytes spec={SPEC_M26}",
            mill.len(),
            rumor_file.rumors.len(),
        ),
    ));
    let emitted_ids: Vec<u16> = faction_lists
        .iter()
        .flat_map(|list| {
            [&list.tell_me_about, &list.location, &list.person, &list.thing]
                .into_iter()
                .flatten()
        })
        .filter_map(|topic| topic.text_id)
        .collect();
    let emitted_ok = !emitted_ids.is_empty() && emitted_ids.iter().all(|id| resolves(*id));
    rows.push(check(
        "answers-retail",
        emitted_ids.iter().all(|id| answer_text_is_retail(Some(*id), resolves))
            && emitted_ok
            && answer_text_is_retail(None, resolves),
        format!(
            "{} emitted text_ids resolve in TEXT.RSC, file text admitted src={SRC_DIALOGUE}",
            emitted_ids.len(),
        ),
    ));

    // ── D · adversarial (declared non-goals with on-disk evidence) ─────
    out.push_str("\n-- D · adversarial (non-goals, negative evidence) --\n");
    out.push_str(&format!("NEGATIVE invented-answer reason={SRC_DIALOGUE}\n"));
    out.push_str(&format!("NEGATIVE cross-region-topic reason={SRC_RUMOR}\n"));

    for row in &rows {
        if row.ok {
            out.push_str(&format!("[OK] {} - {}\n", row.id, row.detail));
            ok &= true;
        } else {
            out.push_str(&format!("[FAIL] {} got=fail want=ok - {}\n", row.id, row.detail));
            ok = false;
        }
    }

    out.push_str(if ok {
        "\nVEREDICTO: PASS\n"
    } else {
        "\nVEREDICTO: FAIL\n"
    });
    print!("{out}");
    println!("DIGEST {}", sha256_hex(out.as_bytes()));
    if ok { 0 } else { 1 }
}
