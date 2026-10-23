//! M29 probes — playable quests (SPEC-M29-QUESTS §6; REQ-L99-062).
//!
//! `PROBE-QUEST-EXEC-001` — execution, determinism, census and coverage:
//! - **A** golden vectors GV-29-01..24: header/layout of the `QBN` container
//!   (60 B, section sizes `[19,0,0,20,24,0,33,14,87,8,27]`, `offsets[8]` **read**
//!   from the header), the clock/time formulas (`dd.hh:mm`/`hh:mm`/`mm`, the
//!   `60..=604800` default draw, inverted range, expiry at `<= 0`) and the
//!   close/task/journal invariants (reputation `0/+5/-2`, `ticksToEnd = 2`,
//!   `AddFinishedQuest` with the log steps, `PersistUntil` clear/rearm,
//!   always-on primary/secondary, `IsComplete`, `QuestBreak`).
//! - **B** determinism: 20 synthetic quests (triggers, clocks with the four
//!   recipes, actions `log`/`say`/`journal note`/`give item`/`give pc`) run
//!   twice with `DfRandom::srand(0x5EED)`; the canonical trace digest and its
//!   length must be identical. The seed falsifier contrasts
//!   `srand(0x5EED)` against `srand(0x1234)` and requires at least one of the
//!   20 clock durations to change.
//! - **C** census §2.1 over `DISK:ARENA2/*.QBN` (247 files) and `*.QRC`
//!   (244 files): per-file tiling identity
//!   `60 + Σ count·size + 27·tv == len`, opcode records read at
//!   `offsets[8] + 87·i`, histogram of the type byte, `0xFFFF` message ids.
//! - **D** coverage table: every used type has a row (`uncovered=0`), the
//!   seven NEGATIVE rows are exercised, `0x48` stays NEGATIVE.
//!
//! `PROBE-QUEST-LIFECYCLE-001` — §6.2 steps 1-7: offer filter
//! (`QuestListsManager.cs:351`), acceptance (`questStartTime`), `end quest` ⇒
//! two ticks to `questComplete`, closing reputation `+5`/`-2`/`0` emitted as
//! an observable `ChangeReputation` request (M27 owns the graph; this organ
//! creates no entity), and a mid-flight `save → load` whose task
//! `is_triggered` flags, clock `remaining` values and snapshot digest are
//! identical twice.
//!
//! `PROBE-QUEST-JOURNAL-001` — §6.3 blocks A-D: 3 `LogMessage` + 1
//! `JournalNote` ⇒ 4 entries in insertion order; `EndQuest` copies the 3 log
//! steps into the Notebook; the sentinel `0xFFFF` and an absent id resolve to
//! `None` and produce `skipped=1` + an explicit `QuestFailure` (DFU raises an
//! NRE in `JournalNote.cs:50-52`; the target never panics); reopening the
//! journal through the snapshot keeps the same 4 entries with no duplicate
//! `stepID`.
//!
//! **Digest substitution (declared):** spec §6.1 asks for `xxh3`; this
//! repository has no `xxh3` implementation, so the trace digest is
//! **SHA-256 hex** (`sha2`, already an `app` dependency). The falsable
//! property is the equality `×2`, not the algorithm.
//!
//! **Census artifact policy (declared):** the per-record rows
//! `{file, offset, type}` are written to `quest-opcode-census.json` under the
//! directory named by `$M29_CENSUS_OUT`; with the variable unset the probe
//! prints the summary and the top-15 histogram to stdout and writes nothing.
//!
//! **Helper reuse:** the ARENA2 custodian (`crate::mode_a_arena2_dir` /
//! `mode_a_arena2_path`) is the M16 one. `m16_probe`'s `collect`/`load` are
//! module-private, so this file carries the same 15-line directory scan over
//! that custodian instead of changing another lane's path.
//!
//! **Custody:** only counts, type ids, file stems and digests leave the probe;
//! no retail bytes are printed or persisted.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use daggerfall_antibodies::quest_guardrails::{
    check_clock_remaining, check_closing_reputation, check_display_message_not_journaled,
    check_finished_journal, check_journal_steps, check_missing_message,
    check_missing_message_break, check_persist_until, check_task_birth_state, check_ticks_to_end,
    check_unsupported_opcode,
};
use daggerfall_behavior::quests::{
    clock_duration, covered_opcode_count, decode_opcode, negative_opcode_count, opcode_coverage,
    offer_eligible_for_status, parse_time_seconds, ClockRecipe, DecodedOpcode, DfRandom,
    OpcodeEffect, Quest, QuestAction, QuestClock, QuestFailure, QuestMachine, QuestRequest,
    QuestSnapshot, QuestTask, TaskType, CLOCK_MAX_SECONDS, CLOCK_MIN_SECONDS, END_TICKS_TO_END,
    FAILURE_REPUTATION, NO_MESSAGE_ID, NOTE_STEP_BASE, OPCODE_COVERAGE, QBN_OPCODE_LEN,
    RETURN_TRIP_MULTIPLIER, SUCCESS_REPUTATION,
};
use daggerfall_parser_rs::quest::{
    Qbn, QBN_HEADER_LEN, QBN_OPCODE_SECTION, QBN_SECTION_COUNT, QBN_SECTION_SIZES,
    QBN_TEXT_VARIABLE_LEN, QBN_TEXT_VARIABLE_SECTION,
};
use sha2::{Digest, Sha256};

// ── evidence rows (M19/M25/M26/M27/M28 template) ────────────────────────────

/// Printed row: `[OK] <id> - <detail>` or `FAIL <id> got=<v> want=<v>`.
struct Row {
    id: &'static str,
    ok: bool,
    got: String,
    want: String,
    detail: String,
}

fn row(id: &'static str, got: String, want: impl Into<String>, detail: impl Into<String>) -> Row {
    let want = want.into();
    Row {
        id,
        ok: got == want,
        got,
        want,
        detail: detail.into(),
    }
}

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

// ── ARENA2 custody (same custodian as M16) ─────────────────────────────────

/// `sha256` hexadecimal of a slice (declared substitution for `xxh3`).
fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// Sorted `(stem, bytes)` of the files with the given extension under ARENA2.
fn collect(dir: &Path, extension: &str) -> Result<Vec<(String, Vec<u8>)>, String> {
    let mut files = Vec::new();
    let entries = std::fs::read_dir(dir).map_err(|err| format!("reading {dir:?}: {err}"))?;
    for entry in entries {
        let path = entry
            .map_err(|err| format!("entry of {dir:?}: {err}"))?
            .path();
        let matches = path
            .extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| ext.eq_ignore_ascii_case(extension));
        if !matches {
            continue;
        }
        let stem = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .ok_or_else(|| format!("non-UTF-8 file name: {path:?}"))?
            .to_string();
        let bytes = std::fs::read(&path).map_err(|err| format!("reading {path:?}: {err}"))?;
        files.push((stem, bytes));
    }
    files.sort_by(|left, right| left.0.cmp(&right.0));
    Ok(files)
}

/// Σ of the section lengths declared by the header (section 10 derived from
/// the file tail, `Qbn::section_len`).
fn section_sum(container: &Qbn) -> usize {
    (0..=QBN_TEXT_VARIABLE_SECTION)
        .map(|index| container.section_len(index))
        .sum()
}

// ═══════════════════════════════════════════════════════════════════════════
// PROBE-QUEST-EXEC-001
// ═══════════════════════════════════════════════════════════════════════════

/// Full `BTreeMap`-based census of §2.1.
#[derive(Default)]
struct Census {
    files: usize,
    counts_sum: usize,
    opcodes: usize,
    types: BTreeMap<u8, usize>,
    unknown_labels: usize,
    type_zero: usize,
    message_sentinel: usize,
    text_variable_records: usize,
    text_variable_files: usize,
    walk_unvalidated: usize,
    offset_divergent: usize,
    /// Records whose type byte is out of the catalogue when the walk uses the
    /// **computed** base instead of `offsets[8]` (must be > 0: the computed
    /// walk is misaligned).
    computed_unknown_types: usize,
    qrc: usize,
    qrc_sentinel: usize,
    records: Vec<(String, usize, u8)>,
}

impl Census {
    /// UESP-labelled `Unknown` types (`spec 2.2`, `2.3`).
    fn is_unknown_label(type_id: u8) -> bool {
        matches!(type_id, 0x05 | 0x3F | 0x40 | 0x48 | 0x4E | 0x57)
    }
}

/// Runs `PROBE-QUEST-EXEC-001` (§6.1, blocks A-D).
pub fn run_quest_exec_001() -> i32 {
    println!(
        "PROBE-QUEST-EXEC-001 - opcodes, tasks, clocks, journal and census (M29, CR-73d23384, \
         REQ-L99-062 · spec §6.1)"
    );

    let dir = match crate::mode_a_arena2_dir() {
        Ok(dir) => dir,
        Err(reason) => {
            println!("ARENA2 not accessible (quests): {reason}");
            return 2;
        }
    };
    let qbn_files = match collect(&dir, "QBN") {
        Ok(files) => files,
        Err(reason) => {
            println!("ARENA2 not accessible (QBN): {reason}");
            return 2;
        }
    };
    let qrc_files = match collect(&dir, "QRC") {
        Ok(files) => files,
        Err(reason) => {
            println!("ARENA2 not accessible (QRC): {reason}");
            return 2;
        }
    };

    let mut ok = true;
    ok &= block_a(&qbn_files);
    ok &= block_b();
    let census = block_c(&qbn_files, &qrc_files);
    ok &= census.ok;
    ok &= block_d(&census.census);

    if ok {
        println!("\nVERDICT: PROBE-QUEST-EXEC-001 PASS - A∧B∧C∧D");
        0
    } else {
        println!("\nVERDICT: PROBE-QUEST-EXEC-001 FAIL - A∧B∧C∧D");
        1
    }
}

/// Block A — golden vectors GV-29-01..24.
fn block_a(qbn_files: &[(String, Vec<u8>)]) -> bool {
    let mut rows: Vec<Row> = Vec::new();

    // ── A1 GV-29-01: header length ───────────────────────────────────────
    rows.push(row(
        "A1 GV-29-01 header_len",
        QBN_HEADER_LEN.to_string(),
        "60",
        "QBN_HEADER_LEN (`quest.rs:42`); opcode record = 87 B (GV-29-04)",
    ));

    // ── A2/A3 GV-29-02/03: $CUREVAM header arithmetic ────────────────────
    let curevam = qbn_files
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case("$CUREVAM"));
    match curevam.and_then(|(_, bytes)| Qbn::parse(bytes).ok().map(|header| (bytes, header))) {
        Some((bytes, header)) => {
            let sum = section_sum(&header);
            rows.push(row(
                "A2 GV-29-02 $CUREVAM sections_sum",
                format!("sum={sum} len={} header+sum={}", bytes.len(), QBN_HEADER_LEN + sum),
                "sum=5159 len=5219 header+sum=5219",
                "Σ count·size over sections 0..=10 (spec §3.1)",
            ));
            let base = header.section_offsets[QBN_OPCODE_SECTION] as usize;
            let count = header.section_counts[QBN_OPCODE_SECTION] as usize;
            rows.push(row(
                "A3 GV-29-03 $CUREVAM offsets[8]",
                format!("offsets8={base} count8={count} end={}", base + QBN_OPCODE_LEN * count),
                "offsets8=521 count8=54 end=5219",
                "offsets[8] read from the header (@36+2·8), never computed",
            ));
        }
        None => rows.push(Row {
            id: "A2 GV-29-02 $CUREVAM sections_sum",
            ok: false,
            got: "$CUREVAM.QBN missing or unparsable".into(),
            want: "sum=5159 len=5219 header+sum=5219".into(),
            detail: "spec §3.1 golden vector fixture".into(),
        }),
    }

    // ── A4 GV-29-04: every opcode record at offsets[8] + 87·i ────────────
    let mut records = 0usize;
    let mut out_of_bounds = 0usize;
    let mut decode_failures = 0usize;
    for (_name, bytes) in qbn_files {
        let Ok(header) = Qbn::parse(bytes) else {
            continue;
        };
        let base = header.section_offsets[QBN_OPCODE_SECTION] as usize;
        let count = header.section_counts[QBN_OPCODE_SECTION] as usize;
        for index in 0..count {
            records += 1;
            let start = base + index * QBN_OPCODE_LEN;
            let end = start + QBN_OPCODE_LEN;
            if end > bytes.len() {
                out_of_bounds += 1;
                continue;
            }
            if decode_opcode(&bytes[start..end]).is_none() {
                decode_failures += 1;
            }
        }
    }
    rows.push(row(
        "A4 GV-29-04 opcode_offset(i)",
        format!("records={records} out_of_bounds={out_of_bounds} decode_failures={decode_failures}"),
        "records=6407 out_of_bounds=0 decode_failures=0",
        "247 files · offset(i) = offsets[8] + 87·i · each record inside the file",
    ));

    // ── A5 GV-29-05: no opcodes ⇒ no executable semantics (synthetic) ────
    let mut empty = Quest::new("no-opcodes", 0);
    empty.tick();
    let empty_ok = empty.tasks.is_empty()
        && empty.clocks.is_empty()
        && empty.journal().is_empty()
        && empty.requests().is_empty()
        && empty.failures().is_empty()
        && empty.ticks_to_end() == 0
        && decode_opcode(&[]).is_none()
        && decode_opcode(&[0u8; QBN_OPCODE_LEN - 1]).is_none();
    rows.push(check(
        "A5 GV-29-05 no opcodes (synthetic)",
        empty_ok,
        "count[8]=0 ⇒ 0 tasks / 0 journal / 0 requests · synthetic fixture declared \
         (`decode_opcode` rejects a truncated record)",
    ));

    // ── A6 GV-29-06: sections 1/2/5 empty in 247/247 ─────────────────────
    let mut files_parsed = 0usize;
    let mut empty_sections = 0usize;
    for (_name, bytes) in qbn_files {
        let Ok(header) = Qbn::parse(bytes) else {
            continue;
        };
        files_parsed += 1;
        if [1usize, 2, 5]
            .iter()
            .all(|index| header.section_counts[*index] == 0)
        {
            empty_sections += 1;
        }
    }
    let sizes_zero = [1usize, 2, 5]
        .iter()
        .all(|index| QBN_SECTION_SIZES[*index] == 0);
    rows.push(row(
        "A6 GV-29-06 empty sections 1/2/5",
        format!(
            "files={files_parsed}/{} sections_empty={empty_sections} sizes_zero={sizes_zero}",
            qbn_files.len()
        ),
        format!(
            "files={}/{} sections_empty={} sizes_zero=true",
            qbn_files.len(),
            qbn_files.len(),
            qbn_files.len()
        ),
        "QBN_SECTION_SIZES[1]=[2]=[5]=0 (`quest.rs:56`)",
    ));

    // ── A7..A14 GV-29-07..14: time values and clock bounds ───────────────
    let time_cases: [(&'static str, &str, i64); 4] = [
        ("A7 GV-29-07 1:30", "1:30", 5_400),
        ("A8 GV-29-08 2.3:00", "2.3:00", 183_600),
        ("A9 GV-29-09 45", "45", 2_700),
        ("A14 GV-29-14 0.0:00", "0.0:00", 0),
    ];
    for (id, text, want) in time_cases {
        rows.push(row(
            id,
            format!("{:?}={:?}", text, parse_time_seconds(text)),
            format!("{text:?}={:?}", Some(want)),
            "Clock.cs:368-388 (dd.hh:mm · hh:mm · mm)",
        ));
    }

    let mut rng = DfRandom::srand(0x5EED);
    let mut default_bounds = (i64::MAX, i64::MIN);
    let mut default_in_range = true;
    for _ in 0..200 {
        let drawn = clock_duration(ClockRecipe::Default, &mut rng);
        default_in_range &= (CLOCK_MIN_SECONDS..=CLOCK_MAX_SECONDS).contains(&drawn);
        default_bounds = (default_bounds.0.min(drawn), default_bounds.1.max(drawn));
    }
    rows.push(check(
        "A10 GV-29-10 default clock range",
        default_in_range
            && CLOCK_MIN_SECONDS == 60
            && CLOCK_MAX_SECONDS == 604_800,
        format!(
            "200 draws in 60..=604800 (min={} max={}) · Clock.cs:189-198",
            default_bounds.0, default_bounds.1
        ),
    ));

    let mut range_in_range = true;
    let mut range_values = BTreeSet::new();
    for _ in 0..200 {
        let drawn = clock_duration(ClockRecipe::Range(600, 1_200), &mut rng);
        range_in_range &= (600..=1_200).contains(&drawn);
        range_values.insert(drawn);
    }
    rows.push(check(
        "A11 GV-29-11 range 0:10..0:20",
        range_in_range && range_values.len() > 1,
        format!(
            "200 draws in 600..=1200 · distinct={} (greater than 1 ⇒ it is a range draw)",
            range_values.len()
        ),
    ));
    rows.push(row(
        "A12 GV-29-12 inverted range",
        clock_duration(ClockRecipe::Range(1_200, 600), &mut rng).to_string(),
        "1200",
        "max > min false ⇒ min (Clock.cs:205-213)",
    ));

    let mut clock = QuestClock::new("expiry", ClockRecipe::Fixed(5), &mut rng);
    let fired = clock.tick(7);
    let mut quest = Quest::new("expiry", 0);
    quest.add_message(1, "expired");
    quest.add_clock(QuestClock::new("timer", ClockRecipe::Fixed(5), &mut rng));
    let mut timer_task = QuestTask::new("timer", TaskType::Standard);
    timer_task.add_action(QuestAction::new(OpcodeEffect::LogMessage).with_message(1));
    quest.add_task(timer_task);
    for _ in 0..49 {
        quest.tick();
    }
    let triggered_before = quest.task("timer").is_some_and(|task| task.is_triggered);
    quest.tick();
    let triggered_after = quest.task("timer").is_some_and(|task| task.is_triggered);
    let expiry_ok = fired
        && clock.remaining() == 0
        && clock.is_finished()
        && !triggered_before
        && triggered_after
        && quest.journal().len() == 1
        && check_clock_remaining(clock.remaining()).is_ok();
    rows.push(check(
        "A13 GV-29-13 remaining=5 delta=7",
        expiry_ok,
        "remaining=0 finished=true fires once · quest clock: not triggered at tick 49, \
         triggered at tick 50 (5 s at 10 Hz) · Clock.cs:287,292-296",
    ));

    // ── A15..A24 GV-29-15..24: lifecycle, task and journal invariants ────
    let neutral = {
        let mut quest = Quest::new("neutral", 0);
        quest.add_message(1, "reward");
        let mut task = QuestTask::new("reward", TaskType::Standard);
        task.add_action(QuestAction::new(OpcodeEffect::GivePc).with_message(1));
        task.is_triggered = true;
        quest.add_task(task);
        quest.tick();
        quest.end_quest();
        quest
    };
    rows.push(check(
        "A15 GV-29-15 factionId=0 close",
        closing_reputation(&neutral).is_none()
            && check_closing_reputation(0, 0).is_ok()
            && neutral.succeeded(),
        "give pc ran but factionId=0 ⇒ no ChangeReputation request (Quest.cs:382)",
    ));

    let success = {
        let mut quest = Quest::new("success", 41);
        quest.add_message(1, "reward");
        let mut task = QuestTask::new("reward", TaskType::Standard);
        task.add_action(QuestAction::new(OpcodeEffect::GivePc).with_message(1));
        task.is_triggered = true;
        quest.add_task(task);
        quest.tick();
        quest.end_quest();
        quest
    };
    let success_delta = closing_reputation(&success);
    rows.push(check(
        "A16 GV-29-16 success +5",
        success_delta == Some(SUCCESS_REPUTATION)
            && success.succeeded()
            && propagates(&success)
            && check_closing_reputation(41, SUCCESS_REPUTATION).is_ok(),
        format!(
            "factionId=41 give pc ⇒ ChangeReputation(delta={success_delta:?}, propagate=true) \
             · Quest.cs:36,382-386"
        ),
    ));

    let failure = {
        let mut quest = Quest::new("failure", 41);
        quest.tick();
        quest.end_quest();
        quest
    };
    let failure_delta = closing_reputation(&failure);
    rows.push(check(
        "A17 GV-29-17 failure -2",
        failure_delta == Some(FAILURE_REPUTATION)
            && !failure.succeeded()
            && check_closing_reputation(41, FAILURE_REPUTATION).is_ok(),
        format!("no give pc ⇒ ChangeReputation(delta={failure_delta:?}) · Quest.cs:37,382-386"),
    ));

    let mut countdown = Quest::new("countdown", 0);
    countdown.end_quest();
    let after_end = countdown.ticks_to_end();
    countdown.tick();
    let alive_mid = !countdown.is_complete();
    countdown.tick();
    rows.push(check(
        "A18 GV-29-18 ticksToEnd=2",
        after_end == END_TICKS_TO_END
            && alive_mid
            && countdown.is_complete()
            && check_ticks_to_end(after_end).is_ok(),
        format!("end_quest ⇒ ticks_to_end={after_end}; alive after 1 tick, complete after 2 · Quest.cs:373"),
    ));

    let logged = {
        let mut quest = Quest::new("logged", 0);
        for id in 1..=3 {
            quest.add_message(id, format!("m{id}"));
        }
        let mut task = QuestTask::new("log", TaskType::Standard);
        for id in 1..=3 {
            task.add_action(QuestAction::new(OpcodeEffect::LogMessage).with_message(id));
        }
        task.is_triggered = true;
        quest.add_task(task);
        quest.tick();
        quest.end_quest();
        quest
    };
    let finished: Vec<(u32, u16)> = logged
        .finished_journal()
        .iter()
        .map(|entry| (entry.step_id, entry.message_id))
        .collect();
    rows.push(row(
        "A19 GV-29-19 AddFinishedQuest(3)",
        format!("finished={} journal={}", finished.len(), logged.journal().len()),
        "finished=3 journal=3",
        "EndQuest copies the log steps whose message exists (Quest.cs:388-398)",
    ));

    let persist_active = {
        let mut quest = Quest::new("persist-active", 0);
        quest.add_message(1, "m");
        let mut target = QuestTask::new("target", TaskType::Standard);
        target.is_triggered = true;
        quest.add_task(target);
        let mut persist = QuestTask::new("persist", TaskType::PersistUntil).with_target("target");
        persist.add_action(QuestAction::new(OpcodeEffect::LogMessage).with_message(1));
        persist.is_triggered = true;
        quest.add_task(persist);
        quest.tick();
        quest
    };
    let active_persist_ok = persist_active
        .task("persist")
        .is_some_and(|task| !task.is_triggered && task.actions[0].is_complete)
        && check_persist_until("persist", "target", false, true).is_ok();
    rows.push(check(
        "A20 GV-29-20 PersistUntil clear",
        active_persist_ok,
        "target already triggered ⇒ task.clear() and no rearm (Task.cs:263-270)",
    ));

    let persist_inactive = {
        let mut quest = Quest::new("persist-inactive", 0);
        quest.add_message(1, "m");
        quest.add_task(QuestTask::new("target", TaskType::Standard));
        let mut persist = QuestTask::new("persist", TaskType::PersistUntil).with_target("target");
        persist.add_action(QuestAction::new(OpcodeEffect::LogMessage).with_message(1));
        persist.is_triggered = true;
        quest.add_task(persist);
        quest.tick();
        quest
    };
    let inactive_persist_ok = persist_inactive.task("persist").is_some_and(|task| {
        !task.actions[0].is_complete && task.actions[0].updates >= 1 && task.actions[0].initialisations == 1
    });
    rows.push(check(
        "A21 GV-29-21 PersistUntil rearm",
        inactive_persist_ok,
        "target inactive ⇒ rearm_actions keeps the action runnable and it ran once (Task.cs:271-276)",
    ));

    let mut always = Quest::new("always", 0);
    let mut task = QuestTask::new("t", TaskType::Standard);
    task.add_action(QuestAction::always_on(OpcodeEffect::PcAt).with_trigger_met(false));
    task.add_action(QuestAction::always_on(OpcodeEffect::PcAt).with_trigger_met(true));
    task.is_triggered = true;
    always.add_task(task);
    always.tick();
    let starts = always.task("t").is_some_and(|task| task.is_triggered);
    if let Some(task) = always.task_mut("t") {
        for action in &mut task.actions {
            action.trigger_met = false;
        }
    }
    always.tick();
    let stopped = always.task("t").is_some_and(|task| !task.is_triggered);
    rows.push(check(
        "A22 GV-29-22 always-on primary/secondary",
        starts && stopped,
        "1st always-on decides (can stop), 2nd only starts · Task.cs:218-238",
    ));

    let mut completed = Quest::new("completed", 0);
    completed.add_message(1, "m");
    let mut task = QuestTask::new("t", TaskType::Standard);
    task.add_action(
        QuestAction::new(OpcodeEffect::LogMessage)
            .with_message(1)
            .completed(),
    );
    task.is_triggered = true;
    completed.add_task(task);
    completed.tick();
    completed.tick();
    rows.push(check(
        "A23 GV-29-23 IsComplete",
        completed.journal().is_empty(),
        "an action already complete never runs again (Task.cs:211-212)",
    ));

    let mut broken = Quest::new("break", 0);
    for id in 1..=2 {
        broken.add_message(id, format!("m{id}"));
    }
    let mut first = QuestTask::new("a", TaskType::Standard);
    first.add_action(QuestAction::new(OpcodeEffect::LogMessage).with_message(1));
    broken.add_task(first);
    let mut second = QuestTask::new("b", TaskType::Standard);
    second.add_action(QuestAction::new(OpcodeEffect::DisplayMessage).with_message(2));
    broken.add_task(second);
    let mut third = QuestTask::new("c", TaskType::Standard);
    third.add_action(QuestAction::new(OpcodeEffect::GivePc));
    broken.add_task(third);
    let mut fourth = QuestTask::new("d", TaskType::Standard);
    fourth.add_action(QuestAction::new(OpcodeEffect::GivePc));
    broken.add_task(fourth);
    for symbol in ["a", "b", "c", "d"] {
        if let Some(task) = broken.task_mut(symbol) {
            task.is_triggered = true;
        }
    }
    broken.tick();
    let break_ok = broken.journal().len() == 1
        && broken.pending_messages() == vec![2u16]
        && !broken.succeeded()
        && !broken.quest_break
        && broken
            .task("c")
            .is_some_and(|task| !task.actions[0].is_complete)
        && broken
            .task("d")
            .is_some_and(|task| !task.actions[0].is_complete);
    rows.push(check(
        "A24 GV-29-24 QuestBreak",
        break_ok,
        "say in task 2 of 4 stops tasks 3-4 for that tick and the flag clears (Quest.cs:321-325)",
    ));

    // ── A25 M-01: Headless starts triggered, the other kinds do not ──────
    let headless_birth = QuestTask::new("startup", TaskType::Headless).is_triggered;
    let other_birth = [
        TaskType::Standard,
        TaskType::PersistUntil,
        TaskType::Variable,
        TaskType::GlobalVarLink,
    ]
    .iter()
    .all(|kind| !QuestTask::new("t", *kind).is_triggered);
    let mut auto_start = Quest::new("auto-start", 0);
    auto_start.add_message(1, "boot");
    let mut startup = QuestTask::new("startup", TaskType::Headless);
    startup.add_action(QuestAction::new(OpcodeEffect::LogMessage).with_message(1));
    auto_start.add_task(startup);
    auto_start.tick();
    rows.push(check(
        "A25 M-01 Headless starts triggered",
        headless_birth
            && other_birth
            && auto_start.journal().len() == 1
            && check_task_birth_state(true, headless_birth).is_ok()
            && check_task_birth_state(false, false).is_ok(),
        "a headerless task is Headless and IsTriggered=true from construction \
         (`Task.cs:181-185`); Standard/PersistUntil/Variable/GlobalVarLink stay false; the \
         headless task logs on its first tick with no manual trigger",
    ));

    // ── A26 m-01: EndQuest breaks the tick before ending ─────────────────
    let mut end_break = Quest::new("end-break", 0);
    let mut closer = QuestTask::new("a", TaskType::Standard);
    closer.add_action(QuestAction::new(OpcodeEffect::EndQuest));
    closer.is_triggered = true;
    end_break.add_task(closer);
    let mut after = QuestTask::new("b", TaskType::Standard);
    after.add_action(QuestAction::new(OpcodeEffect::GivePc));
    after.is_triggered = true;
    end_break.add_task(after);
    end_break.tick();
    let end_break_ok = end_break.ticks_to_end() == END_TICKS_TO_END
        && !end_break.succeeded()
        && end_break.task("b").is_some_and(|task| {
            task.actions[0].updates == 0 && !task.actions[0].is_complete
        })
        && !end_break.quest_break;
    rows.push(check(
        "A26 m-01 EndQuest sets QuestBreak",
        end_break_ok,
        format!(
            "end quest in task 1 of 2 ⇒ ticks_to_end={:?}; task 2 did not run that tick and the \
             flag cleared (`Actions/EndQuest.cs:60-61`, `Quest.cs:321-325`)",
            end_break.ticks_to_end()
        ),
    ));

    // ── A27 m-02: RemoveLog removes the step, not every message ──────────
    let mut remove = Quest::new("remove", 0);
    remove.add_message(7, "shared");
    let mut remover = QuestTask::new("t", TaskType::Standard);
    remover.add_action(QuestAction::new(OpcodeEffect::LogMessage).with_message(7).with_step_id(1));
    remover.add_action(QuestAction::new(OpcodeEffect::LogMessage).with_message(7).with_step_id(2));
    remover.add_action(
        QuestAction::new(OpcodeEffect::RemoveLogMessage)
            .with_message(7)
            .with_step_id(1),
    );
    remover.is_triggered = true;
    remove.add_task(remover);
    remove.tick();
    let remaining_steps: Vec<u32> = remove.journal().iter().map(|entry| entry.step_id).collect();
    rows.push(check(
        "A27 m-02 RemoveLog by stepID",
        remaining_steps == vec![2],
        format!(
            "two log entries sharing message 7 in steps 1/2; removing step 1 leaves {remaining_steps:?} \
             (`Actions/RemoveLogMessage.cs:61` → `Quest.cs:569-576`)"
        ),
    ));

    // ── A28 m-03: the action decides whether it completes ────────────────
    let mut repeater = Quest::new("repeater", 0);
    let mut sound = QuestTask::new("sound", TaskType::Standard);
    sound.add_action(QuestAction::new(OpcodeEffect::PlaySound));
    sound.is_triggered = true;
    repeater.add_task(sound);
    repeater.add_message(1, "once");
    let mut once = QuestTask::new("once", TaskType::Standard);
    once.add_action(QuestAction::new(OpcodeEffect::LogMessage).with_message(1));
    once.is_triggered = true;
    repeater.add_task(once);
    repeater.tick();
    repeater.tick();
    let sound_action = repeater.task("sound").map(|task| &task.actions[0]);
    let log_action = repeater.task("once").map(|task| &task.actions[0]);
    let repeat_ok = sound_action.is_some_and(|action| {
        action.updates == 2 && !action.is_complete && !action.effect.completes_after_update()
    }) && log_action.is_some_and(|action| {
        action.updates == 1 && action.is_complete && action.effect.completes_after_update()
    });
    rows.push(check(
        "A28 m-03 action decides completion",
        repeat_ok,
        "PlaySound re-runs on both ticks (updates=2, is_complete=false · \
         `Actions/PlaySound.cs:93-115`); LogMessage runs once (updates=1, is_complete=true · \
         `SetComplete` at `Actions/LogMessage.cs:69`) — `Task.cs:207-209`",
    ));

    // ── A29 m-07: `&16` clock asks M31 for the 2.5× round trip ───────────
    let mut travel_rng = DfRandom::srand(0x7EA1);
    let one_way = 40i64;
    let round_trip = clock_duration(
        ClockRecipe::ReturnTrip {
            one_way_seconds: one_way,
        },
        &mut travel_rng,
    );
    let mut travel_quest = Quest::new("travel", 0);
    travel_quest.add_clock(
        QuestClock::new(
            "trip",
            ClockRecipe::ReturnTrip {
                one_way_seconds: one_way,
            },
            &mut travel_rng,
        )
        .with_flag(0x10),
    );
    let travel_request = travel_quest.requests().first().cloned();
    let plain_quest = {
        let mut quest = Quest::new("plain", 0);
        quest.add_clock(QuestClock::new(
            "timer",
            ClockRecipe::Fixed(5),
            &mut travel_rng,
        ));
        quest
    };
    rows.push(check(
        "A29 m-07 travel clock &16",
        round_trip == (one_way as f32 * RETURN_TRIP_MULTIPLIER) as i64
            && round_trip == 100
            && travel_request
                == Some(QuestRequest::TravelTime {
                    seconds: Some(100),
                    flag: Some(0x10),
                })
            && plain_quest.requests().is_empty(),
        format!(
            "one_way={one_way} ⇒ round_trip={round_trip} (2.5×, truncated like `(int)` in \
             `Clock.cs:431-433`) and the clock emits {travel_request:?} for M31 when added \
             (`Clock.cs:215-220`); a `&16`-free clock emits nothing"
        ),
    ));

    // ── A30 m-08: a missing message never breaks the tick ────────────────
    let mut missing_display = Quest::new("missing-display", 0);
    let mut say = QuestTask::new("say", TaskType::Standard);
    say.add_action(QuestAction::new(OpcodeEffect::DisplayMessage).with_message(999));
    say.is_triggered = true;
    missing_display.add_task(say);
    let mut reward = QuestTask::new("reward", TaskType::Standard);
    reward.add_action(QuestAction::new(OpcodeEffect::GivePc));
    reward.is_triggered = true;
    missing_display.add_task(reward);
    missing_display.tick();
    let missing_display_ok = missing_display.failures()
        == [QuestFailure::MissingMessage(999)]
        && missing_display.pending_messages().is_empty()
        && missing_display.succeeded()
        && !missing_display.quest_break
        && check_missing_message_break(999, false, missing_display.quest_break).is_ok();
    rows.push(check(
        "A30 m-08 missing display no break",
        missing_display_ok,
        "display 999 does not exist ⇒ MissingMessage(999), nothing queued, no QuestBreak and the \
         next task still runs (`Quest.cs:780-782` returns before `:838-841`)",
    ));

    print_rows("A · golden vectors GV-29-01..24 + QA M-01/m-01..m-03/m-07/m-08", &rows)
}

/// `ChangeReputation` delta requested at close, if any.
fn closing_reputation(quest: &Quest) -> Option<i16> {
    quest
        .requests()
        .iter()
        .rev()
        .find_map(|request| match request {
            QuestRequest::ChangeReputation { delta, .. } => *delta,
            _ => None,
        })
}

/// The close reputation request propagates through M27.
fn propagates(quest: &Quest) -> bool {
    quest.requests().iter().any(|request| {
        matches!(
            request,
            QuestRequest::ChangeReputation {
                propagate: true,
                ..
            }
        )
    })
}

// ── B · determinism ────────────────────────────────────────────────────────

/// Synthetic quests executed by block B.
const DETERMINISM_QUESTS: usize = 20;

/// Ticks per run (20 s of quest time at 10 Hz).
const DETERMINISM_TICKS: usize = 200;

/// Clock recipe of quest `index`; the order of the draws is part of the
/// determinism contract, so both the runner and the seed contrast use it.
fn recipe_for(index: usize) -> ClockRecipe {
    match index % 4 {
        0 => ClockRecipe::Default,
        1 => ClockRecipe::Fixed(30 + index as i64 * 7),
        2 => ClockRecipe::Range(60, 600),
        _ => ClockRecipe::Text("1:30"),
    }
}

fn faction_for(index: usize) -> u16 {
    if index.is_multiple_of(3) {
        0
    } else {
        41 + (index % 3) as u16
    }
}

/// Synthetic quest with triggers, one clock and the five action classes the
/// spec names for block B (`log`, `say`, `journal note`, `give item`,
/// `give pc`).
fn synthetic_quest(machine: &mut QuestMachine, index: usize) -> Quest {
    let mut quest = Quest::new(format!("Q{index:02}"), faction_for(index));
    let message = 1_000 + index as u16;
    quest.add_message(message, format!("message {index}"));
    quest.add_message(message + 1, format!("note {index}"));
    quest.add_message(message + 2, format!("say {index}"));

    let clock = QuestClock::new("timer", recipe_for(index), machine.rng());
    quest.add_clock(clock);

    let mut timer_task = QuestTask::new("timer", TaskType::Standard);
    timer_task.add_action(QuestAction::new(OpcodeEffect::LogMessage).with_message(message));
    quest.add_task(timer_task);

    // The headerless kind is exercised by `startup`: `Headless` is triggered
    // from construction (`Task.cs:181-185`) and the probe never forces that
    // state. `runner` is a `Standard` task, whose trigger state is legitimately
    // set by the world.
    quest.add_message(message + 3, format!("boot {index}"));
    let mut startup = QuestTask::new("startup", TaskType::Headless);
    startup.add_action(QuestAction::new(OpcodeEffect::LogMessage).with_message(message + 3));
    quest.add_task(startup);

    let mut runner = QuestTask::new("runner", TaskType::Standard);
    runner.add_action(
        QuestAction::trigger(OpcodeEffect::PcAt).with_trigger_met(!index.is_multiple_of(3)),
    );
    runner.add_action(QuestAction::new(OpcodeEffect::JournalNote).with_message(message + 1));
    runner.add_action(QuestAction::new(OpcodeEffect::DisplayMessage).with_message(message + 2));
    runner.add_action(
        QuestAction::new(OpcodeEffect::GiveItem)
            .with_message(message)
            .with_target("item"),
    );
    if index.is_multiple_of(5) {
        runner.add_action(QuestAction::new(OpcodeEffect::GivePc).with_message(message));
    }
    runner.is_triggered = !index.is_multiple_of(3);
    quest.add_task(runner);

    let mut monitor = QuestTask::new("monitor", TaskType::PersistUntil).with_target("timer");
    monitor.add_action(QuestAction::new(OpcodeEffect::LogMessage).with_message(message));
    monitor.is_triggered = true;
    quest.add_task(monitor);
    quest
}

/// Canonical trace of a machine: quest state, tasks, clocks, journal,
/// requests and failures, in declaration order.
fn trace_of(machine: &QuestMachine) -> String {
    let mut out = String::new();
    for quest in machine.quests() {
        out.push_str(&format!(
            "q={};start={};complete={};ticks_to_end={};success={};date={};",
            quest.name,
            quest.start_time,
            quest.is_complete(),
            quest.ticks_to_end(),
            quest.succeeded(),
            quest.date_time()
        ));
        for task in &quest.tasks {
            out.push_str(&format!(
                "t={}:{:?}:{}:{}:{};",
                task.symbol, task.task_type, task.is_triggered, task.prev_triggered, task.dropped
            ));
            for action in &task.actions {
                out.push_str(&format!(
                    "a={:?}:{}:{}:{};",
                    action.effect, action.updates, action.initialisations, action.is_complete
                ));
            }
        }
        for clock in &quest.clocks {
            out.push_str(&format!(
                "c={}:{}:{}:{}:{};",
                clock.symbol, clock.remaining, clock.starting_seconds, clock.enabled, clock.finished
            ));
        }
        for entry in quest.journal() {
            out.push_str(&format!(
                "j={}:{}:{};",
                entry.step_id, entry.message_id, entry.date_time
            ));
        }
        for entry in quest.finished_journal() {
            out.push_str(&format!("f={}:{};", entry.step_id, entry.message_id));
        }
        out.push_str(&format!(
            "p={:?};r={:?};x={:?};s={};",
            quest.pending_messages(),
            quest.requests(),
            quest.failures(),
            quest.journal_skipped()
        ));
    }
    out.push_str(&format!("ticks={};", machine.ticks_elapsed()));
    out
}

/// Builds the 20 synthetic quests with `seed` and returns the canonical trace.
fn determinism_run(seed: u32) -> String {
    let mut machine = QuestMachine::new(seed);
    for index in 0..DETERMINISM_QUESTS {
        let quest = synthetic_quest(&mut machine, index);
        machine.start_quest(quest);
    }
    for _ in 0..DETERMINISM_TICKS {
        machine.tick();
    }
    trace_of(&machine)
}

/// The 20 clock durations drawn with `seed`, in construction order.
fn clock_durations(seed: u32) -> Vec<i64> {
    let mut rng = DfRandom::srand(seed);
    (0..DETERMINISM_QUESTS)
        .map(|index| clock_duration(recipe_for(index), &mut rng))
        .collect()
}

/// Block B — determinism `×2` and the seed falsifier.
fn block_b() -> bool {
    let trace_a = determinism_run(0x5EED);
    let trace_b = determinism_run(0x5EED);
    let digest_a = sha256_hex(trace_a.as_bytes());
    let digest_b = sha256_hex(trace_b.as_bytes());
    println!("determinism: digest={digest_a} trace_len={}", trace_a.len());
    println!("determinism: digest={digest_b} trace_len={}", trace_b.len());

    let durations_fixed = clock_durations(0x5EED);
    let durations_other = clock_durations(0x1234);
    let changed = durations_fixed
        .iter()
        .zip(&durations_other)
        .filter(|(left, right)| left != right)
        .count();
    println!(
        "seed contrast: srand(0x5EED) vs srand(0x1234) → {changed}/{DETERMINISM_QUESTS} clock durations differ"
    );

    let rows = vec![
        row(
            "B1 trace digest ×2",
            digest_a.clone(),
            digest_b.clone(),
            "SHA-256 of the canonical trace (declared substitution for xxh3, spec §6.1) · \
             20 quests · srand(0x5EED)",
        ),
        row(
            "B2 trace_len ×2",
            trace_a.len().to_string(),
            trace_b.len().to_string(),
            format!("{DETERMINISM_TICKS} ticks · 20 synthetic quests"),
        ),
        check(
            "B3 seed falsifier",
            changed > 0 && changed <= DETERMINISM_QUESTS,
            format!(
                "{changed}/{DETERMINISM_QUESTS} clock durations change with another seed ⇒ the \
                 clock RNG is seeded, not constant (spec §6.1 c)"
            ),
        ),
    ];
    print_rows("B · determinism (20 quests ×2)", &rows)
}

// ── C · census ─────────────────────────────────────────────────────────────

/// Result of block C: the census plus the wall-clock-free verdict.
struct CensusBlock {
    census: Census,
    ok: bool,
}

/// Block C — census §2.1 over the 247 `.QBN` and 244 `.QRC`.
fn block_c(qbn_files: &[(String, Vec<u8>)], qrc_files: &[(String, Vec<u8>)]) -> CensusBlock {
    let mut census = Census {
        files: qbn_files.len(),
        ..Census::default()
    };

    for (name, bytes) in qbn_files {
        let Ok(header) = Qbn::parse(bytes) else {
            census.walk_unvalidated += 1;
            continue;
        };
        let declared: usize = (0..QBN_SECTION_COUNT)
            .map(|index| header.section_counts[index] as usize * QBN_SECTION_SIZES[index])
            .sum();
        let expected = QBN_HEADER_LEN + declared + header.text_variable_records as usize * QBN_TEXT_VARIABLE_LEN;
        if expected != bytes.len() {
            census.walk_unvalidated += 1;
            continue;
        }
        census.counts_sum += header.section_counts.iter().map(|count| *count as usize).sum::<usize>();
        census.text_variable_records += header.text_variable_records as usize;
        if header.text_variable_records > 0 {
            census.text_variable_files += 1;
        }

        // Anti-false-positive (d): the walk uses the offsets READ from the
        // header. The computed base (`60 + Σ_{i<8} count·size`) is measured
        // only to prove it would misalign the walk.
        let computed_base = QBN_HEADER_LEN
            + (0..QBN_OPCODE_SECTION)
                .map(|index| header.section_counts[index] as usize * QBN_SECTION_SIZES[index])
                .sum::<usize>();
        let base = header.section_offsets[QBN_OPCODE_SECTION] as usize;
        let count = header.section_counts[QBN_OPCODE_SECTION] as usize;
        if base != computed_base {
            census.offset_divergent += 1;
            // A computed-base walk lands on bytes that belong to another
            // section: count the records whose type byte is out of the
            // catalogue. The read walk leaves `uncovered=0` (block D).
            for index in 0..count {
                let start = computed_base + index * QBN_OPCODE_LEN;
                if let Some(record) = bytes.get(start..start + QBN_OPCODE_LEN) {
                    if let Some(decoded) = decode_opcode(record) {
                        if opcode_coverage(decoded.type_id).is_none() {
                            census.computed_unknown_types += 1;
                        }
                    }
                }
            }
        }

        for index in 0..count {
            let start = base + index * QBN_OPCODE_LEN;
            let end = start + QBN_OPCODE_LEN;
            let Some(record) = bytes.get(start..end) else {
                census.walk_unvalidated += 1;
                break;
            };
            let Some(decoded) = decode_opcode(record) else {
                census.walk_unvalidated += 1;
                break;
            };
            census.opcodes += 1;
            *census.types.entry(decoded.type_id).or_default() += 1;
            if Census::is_unknown_label(decoded.type_id) {
                census.unknown_labels += 1;
            }
            if decoded.type_id == 0 {
                census.type_zero += 1;
            }
            if decoded.message_id == NO_MESSAGE_ID {
                census.message_sentinel += 1;
            }
            census.records.push((name.clone(), start, decoded.type_id));
        }
    }

    census.qrc = qrc_files.len();
    for (_name, bytes) in qrc_files {
        if bytes.windows(2).any(|pair| pair == [0xFF, 0xFF]) {
            census.qrc_sentinel += 1;
        }
    }

    println!(
        "census: files={} counts_sum={} opcodes={} distinct_types={} unknown_labels={} \
         type0={} text_variable_records={}/{} qrc={} qrc_ffff={} walk_unvalidated={} \
         msg_ffff={} offsets_read_divergent={} computed_walk_out_of_catalogue={}",
        census.files,
        census.counts_sum,
        census.opcodes,
        census.types.len(),
        census.unknown_labels,
        census.type_zero,
        census.text_variable_records,
        census.text_variable_files,
        census.qrc,
        census.qrc_sentinel,
        census.walk_unvalidated,
        census.message_sentinel,
        census.offset_divergent,
        census.computed_unknown_types
    );

    let mut ranked: Vec<(&u8, &usize)> = census.types.iter().collect();
    ranked.sort_by(|left, right| right.1.cmp(left.1).then(left.0.cmp(right.0)));
    println!("top-15 opcode types (type × occurrences):");
    for (type_id, count) in ranked.iter().take(15) {
        println!("  {type_id:>3} ({type_id:#04x}) × {count}");
    }

    let mut rows = vec![
        row(
            "C1 files",
            census.files.to_string(),
            "247",
            "DISK:ARENA2/*.QBN (spec §2.1 step 1) — removing one file must fail here",
        ),
        row(
            "C2 counts_sum",
            census.counts_sum.to_string(),
            "11462",
            "Σ count[0..9] of the header (spec §2.3)",
        ),
        row(
            "C3 opcodes",
            census.opcodes.to_string(),
            "6407",
            "Σ count[8] · records read at offsets[8] + 87·i",
        ),
        row(
            "C4 distinct_types",
            census.types.len().to_string(),
            "69",
            "distinct type bytes at +0 (spec §2.3)",
        ),
        row(
            "C5 unknown_labels",
            census.unknown_labels.to_string(),
            "29",
            "occurrences of the six UESP `Unknown` labels (5·8 + 63·2 + 64·1 + 72·4 + 78·8 + 87·6)",
        ),
        row(
            "C6 type0",
            census.type_zero.to_string(),
            "92",
            "type 0 is outside the UESP catalogue (NEGATIVE)",
        ),
        row(
            "C7 text_variable_records",
            format!("{}/{}", census.text_variable_records, census.text_variable_files),
            "149/7",
            "section 10 derived from the file tail (27 B per record)",
        ),
        row(
            "C8 qrc sentinel",
            format!("qrc={} qrc_ffff={}", census.qrc, census.qrc_sentinel),
            "qrc=244 qrc_ffff=244",
            "every retail .QRC carries the 0xFFFF pair (spec §2.5)",
        ),
        row(
            "C9 walk_unvalidated",
            census.walk_unvalidated.to_string(),
            "0",
            "per-file identity 60 + Σ count·size + 27·tv == len and every record inside the file",
        ),
        check(
            "C10 offsets read from header",
            census.offset_divergent == 245
                && census.files == 247
                && census.computed_unknown_types > 0,
            format!(
                "offsets[8] != base_calculada in {}/{} files; a computed-base walk yields {} records \
                 whose type byte is out of the catalogue (the read walk yields uncovered=0) ⇒ the \
                 offsets are READ (spec §6.1 d, R-29-2)",
                census.offset_divergent, census.files, census.computed_unknown_types
            ),
        ),
    ];

    rows.push(census_artifact(&census.records));

    let ok = print_rows("C · census §2.1 (DISK:ARENA2, 247 QBN · 244 QRC)", &rows);
    CensusBlock { census, ok }
}

/// Writes `quest-opcode-census.json` when `$M29_CENSUS_OUT` is set (declared
/// policy in the module header).
fn census_artifact(records: &[(String, usize, u8)]) -> Row {
    let Some(dir) = std::env::var_os("M29_CENSUS_OUT") else {
        return check(
            "C11 census artifact",
            true,
            format!(
                "$M29_CENSUS_OUT unset ⇒ {}-record summary printed, no file written (declared policy)",
                records.len()
            ),
        );
    };
    let dir = PathBuf::from(dir);
    if let Err(err) = std::fs::create_dir_all(&dir) {
        return check(
            "C11 census artifact",
            false,
            format!("creating {dir:?}: {err}"),
        );
    }
    let mut json = String::from("{\n  \"source\": \"DISK:ARENA2/*.QBN\",\n  \"records\": [\n");
    for (index, (file, offset, type_id)) in records.iter().enumerate() {
        let comma = if index + 1 == records.len() { "" } else { "," };
        json.push_str(&format!(
            "    {{\"file\": \"{file}.QBN\", \"offset\": {offset}, \"type\": {type_id}}}{comma}\n"
        ));
    }
    json.push_str("  ]\n}\n");
    let path = dir.join("quest-opcode-census.json");
    match std::fs::write(&path, json) {
        Ok(()) => check(
            "C11 census artifact",
            true,
            format!("{} rows → {}", records.len(), path.display()),
        ),
        Err(err) => check(
            "C11 census artifact",
            false,
            format!("writing {}: {err}", path.display()),
        ),
    }
}

// ── D · coverage ───────────────────────────────────────────────────────────

/// Block D — every used type has a row; the NEGATIVE rows stay NEGATIVE.
fn block_d(census: &Census) -> bool {
    let used: Vec<u8> = census.types.keys().copied().collect();
    let mut uncovered = 0usize;
    for type_id in &used {
        if opcode_coverage(*type_id).is_none() {
            uncovered += 1;
        }
    }
    let documented: Vec<u8> = OPCODE_COVERAGE
        .iter()
        .filter(|row| row.effect.is_some())
        .map(|row| row.type_id)
        .collect();
    let unused_documented: Vec<u8> = documented
        .iter()
        .copied()
        .filter(|type_id| !census.types.contains_key(type_id))
        .collect();
    let negative_used = used
        .iter()
        .filter(|type_id| {
            opcode_coverage(**type_id).is_some_and(|row| row.effect.is_none())
        })
        .count();

    let covered = covered_opcode_count();
    let negative = negative_opcode_count();
    println!(
        "coverage: covered={covered} negative={negative} uncovered={uncovered} \
         negative_used={negative_used} unused_documented={}",
        unused_documented.len()
    );

    let rows = vec![
        row(
            "D1 covered types",
            covered.to_string(),
            "62",
            "documented types used in retail (0x3E excluded, spec §2.3)",
        ),
        row(
            "D2 negative rows",
            negative.to_string(),
            "7",
            "type 0 plus the six UESP `Unknown` labels (spec §2.4)",
        ),
        row(
            "D3 uncovered used types",
            uncovered.to_string(),
            "0",
            "every type byte found by the census has a coverage row (spec §2.4 rule 2)",
        ),
        row(
            "D4 unused documented",
            format!("{} {unused_documented:02X?}", unused_documented.len()),
            "1 [3E]",
            "0x3E (62, Weather Override) is documented and absent from the 247 files (R-29-8)",
        ),
        check(
            "D5 0x48 stays NEGATIVE",
            opcode_coverage(0x48).is_some_and(|row| row.effect.is_none())
                && census.types.contains_key(&0x48),
            format!(
                "type 0x48 used {}× · effect=None ⇒ UnsupportedOpcode, never a silent success \
                 (spec §6.1 b)",
                census.types.get(&0x48).copied().unwrap_or(0)
            ),
        ),
        check(
            "D6 negative types execute as failures",
            {
                let mut quest = Quest::new("negative", 0);
                let failure = quest.execute_opcode(DecodedOpcode {
                    type_id: 0x48,
                    message_id: NO_MESSAGE_ID,
                    supported: false,
                });
                failure == Err(QuestFailure::UnsupportedOpcode(0x48))
                    && check_unsupported_opcode(0x48, !quest.failures().is_empty()).is_ok()
            },
            "a NEGATIVE opcode records QuestFailure::UnsupportedOpcode (spec §2.4 rule 3)",
        ),
    ];
    print_rows("D · coverage table (type → semantics or NEGATIVE)", &rows)
}

// ═══════════════════════════════════════════════════════════════════════════
// PROBE-QUEST-LIFECYCLE-001
// ═══════════════════════════════════════════════════════════════════════════

/// Quest accepted by step 4: one task whose single action ends the quest.
fn lifecycle_quest() -> Quest {
    let mut quest = Quest::new("lifecycle", 41);
    let mut closer = QuestTask::new("closer", TaskType::Standard);
    closer.add_action(QuestAction::new(OpcodeEffect::EndQuest));
    closer.is_triggered = true;
    quest.add_task(closer);
    quest
}

/// Quest used by step 7: several task kinds and two clock recipes, so the
/// snapshot carries non-trivial flags and remaining times.
fn mid_flight_quest() -> Quest {
    let mut rng = DfRandom::srand(0xA11CE);
    let mut quest = Quest::new("midflight", 41).with_uid(0x0000_0000_00C0_FFEE);
    for id in 1..=3 {
        quest.add_message(id, format!("m{id}"));
    }
    quest.add_clock(QuestClock::new("short", ClockRecipe::Fixed(3), &mut rng));
    quest.add_clock(QuestClock::new("range", ClockRecipe::Range(20, 40), &mut rng));

    let mut first = QuestTask::new("first", TaskType::Standard);
    first.add_action(
        QuestAction::new(OpcodeEffect::LogMessage)
            .with_message(1)
            .with_step_id(1),
    );
    first.is_triggered = true;
    quest.add_task(first);

    let mut waiting = QuestTask::new("waiting", TaskType::GlobalVarLink).with_global_var("g_var");
    waiting.add_action(QuestAction::trigger(OpcodeEffect::PcAt).with_trigger_met(false));
    waiting.add_action(QuestAction::new(OpcodeEffect::LogMessage).with_message(2));
    quest.add_task(waiting);

    let mut monitor = QuestTask::new("monitor", TaskType::PersistUntil).with_target("first");
    monitor.add_action(QuestAction::new(OpcodeEffect::LogMessage).with_message(3));
    monitor.is_triggered = true;
    quest.add_task(monitor);
    quest
}

/// Canonical digest of a snapshot (SHA-256 of its `Debug` form, whose field
/// order is the declaration order; declared substitution for `xxh3`).
fn snapshot_digest(snapshot: &QuestSnapshot) -> String {
    sha256_hex(format!("{snapshot:?}").as_bytes())
}

/// Runs `PROBE-QUEST-LIFECYCLE-001` (§6.2 steps 1-7).
pub fn run_quest_lifecycle_001() -> i32 {
    println!(
        "PROBE-QUEST-LIFECYCLE-001 - offer, accept, end, reputation and save→load (M29, \
         CR-73d23384, REQ-L99-062 · spec §6.2)"
    );
    let mut rows: Vec<Row> = Vec::new();

    // ── step 1 · minReq=0, non-member ────────────────────────────────────
    rows.push(check(
        "L1 step1 offer minReq=0 non-member",
        offer_eligible_for_status(0, 0, 0, false) && !offer_eligible_for_status(5, 9, 9, false),
        "minReq=0 ⇒ eligible for a non-member; minReq=5 ⇒ not (QuestListsManager.cs:351)",
    ));

    // ── step 2 · minReq=5, rank 4 / 5 ────────────────────────────────────
    rows.push(check(
        "L2 step2 offer minReq=5 rank 4/5",
        !offer_eligible_for_status(5, 4, 0, true) && offer_eligible_for_status(5, 5, 0, true),
        "rank=4 ⇒ not eligible; rank=5 ⇒ eligible (QuestListsManager.cs:351)",
    ));

    // ── step 3 · minReq=20, reputation 19 / 20 ───────────────────────────
    rows.push(check(
        "L3 step3 offer minReq=20 rep 19/20",
        !offer_eligible_for_status(20, 0, 19, true) && offer_eligible_for_status(20, 0, 20, true),
        "rep=19 ⇒ not eligible; rep=20 ⇒ eligible (minReq ≥ 10 uses reputation)",
    ));

    // ── step 4 · accept ──────────────────────────────────────────────────
    let mut machine = QuestMachine::new(0x5EED);
    machine.update(2.5);
    machine.start_quest(lifecycle_quest());
    let active = machine.active_quests().count();
    let start_time = machine.quest("lifecycle").map(|quest| quest.start_time);
    rows.push(row(
        "L4 step4 accept",
        format!("quest_active={active} start_time={start_time:?}"),
        "quest_active=1 start_time=Some(2)",
        "StartQuest fixes questStartTime (Quest.cs:281-285) after 25 ticks = 2 s",
    ));

    // ── step 5 · end quest ⇒ 2 ticks to completion ───────────────────────
    machine.tick();
    let applied = machine.quest("lifecycle").map(|quest| quest.ticks_to_end());
    let mut ticks = 0u32;
    let mut complete = false;
    while ticks < 8 {
        machine.tick();
        ticks += 1;
        if machine.quest("lifecycle").is_some_and(|quest| quest.is_complete()) {
            complete = true;
            break;
        }
    }
    let remaining_active = machine.active_quests().count();
    rows.push(check(
        "L5 step5 end quest ticks=2",
        applied == Some(END_TICKS_TO_END)
            && ticks == END_TICKS_TO_END
            && complete
            && remaining_active == 0
            && check_ticks_to_end(applied.unwrap_or(u32::MAX)).is_ok(),
        format!(
            "end applied (opcode 0x06) ⇒ ticks_to_end={applied:?}; complete after {ticks} ticks; \
             quest_active={remaining_active} · Quest.cs:300-305,373"
        ),
    ));

    // ── step 6 · closing reputation ──────────────────────────────────────
    let success = {
        let mut quest = Quest::new("lifecycle-success", 41);
        quest.add_message(1, "reward");
        let mut reward = QuestTask::new("reward", TaskType::Standard);
        reward.add_action(QuestAction::new(OpcodeEffect::GivePc).with_message(1));
        reward.is_triggered = true;
        quest.add_task(reward);
        quest.tick();
        quest.end_quest();
        quest
    };
    let success_delta = closing_reputation(&success);
    rows.push(check(
        "L6a step6 success rep(+5)",
        success.succeeded()
            && success_delta == Some(SUCCESS_REPUTATION)
            && propagates(&success)
            && check_closing_reputation(41, SUCCESS_REPUTATION).is_ok(),
        format!(
            "give pc ⇒ ChangeReputation(faction=41, delta={success_delta:?}, propagate=true) — a \
             request for M27, no entity created here"
        ),
    ));

    let failure = {
        let mut quest = Quest::new("lifecycle-failure", 41);
        quest.tick();
        quest.end_quest();
        quest
    };
    let failure_delta = closing_reputation(&failure);
    rows.push(check(
        "L6b step6 failure rep(-2)",
        !failure.succeeded()
            && failure_delta == Some(FAILURE_REPUTATION)
            && check_closing_reputation(41, FAILURE_REPUTATION).is_ok(),
        format!("end without give pc ⇒ ChangeReputation(delta={failure_delta:?})"),
    ));

    let neutral = {
        let mut quest = Quest::new("lifecycle-neutral", 0);
        quest.add_message(1, "reward");
        let mut reward = QuestTask::new("reward", TaskType::Standard);
        reward.add_action(QuestAction::new(OpcodeEffect::GivePc).with_message(1));
        reward.is_triggered = true;
        quest.add_task(reward);
        quest.tick();
        quest.end_quest();
        quest
    };
    rows.push(check(
        "L6c step6 factionId=0 rep(0)",
        neutral.succeeded()
            && closing_reputation(&neutral).is_none()
            && check_closing_reputation(0, 0).is_ok(),
        "factionId=0 ⇒ no ChangeReputation request (Quest.cs:382)",
    ));

    // ── step 7 · save → load mid-flight ──────────────────────────────────
    let mut midway = mid_flight_quest();
    for _ in 0..12 {
        midway.tick();
    }
    midway.set_var("seen", true);
    midway.set_global_var("g_var", true);
    midway.tombstone(7);
    let snapshot = midway.snapshot();
    let restored = Quest::from_snapshot(snapshot.clone());
    let tasks_before: Vec<(String, bool)> = midway
        .tasks
        .iter()
        .map(|task| (task.symbol.clone(), task.is_triggered))
        .collect();
    let tasks_after: Vec<(String, bool)> = restored
        .tasks
        .iter()
        .map(|task| (task.symbol.clone(), task.is_triggered))
        .collect();
    let clocks_before: Vec<(String, i64)> = midway
        .clocks
        .iter()
        .map(|clock| (clock.symbol.clone(), clock.remaining))
        .collect();
    let clocks_after: Vec<(String, i64)> = restored
        .clocks
        .iter()
        .map(|clock| (clock.symbol.clone(), clock.remaining))
        .collect();
    let digest_before = snapshot_digest(&snapshot);
    let digest_after = snapshot_digest(&restored.snapshot());
    println!(
        "snapshot: digest={digest_before} ×2 tasks={} clocks={} journal={} ticks={}",
        tasks_before.len(),
        clocks_before.len(),
        midway.journal().len(),
        midway.date_time()
    );
    rows.push(row(
        "L7a step7 task is_triggered ×2",
        format!("{tasks_before:?}"),
        format!("{tasks_after:?}"),
        "every task state survives save→load (spec §5 TaskSaveData_v1)",
    ));
    rows.push(row(
        "L7b step7 clock remaining ×2",
        format!("{clocks_before:?}"),
        format!("{clocks_after:?}"),
        "every clock remaining survives save→load (spec §5 ClockSaveData_v1)",
    ));
    rows.push(row(
        "L7c step7 snapshot digest ×2",
        digest_before,
        digest_after,
        "SHA-256 of the canonical snapshot (declared substitution for xxh3)",
    ));
    rows.push(check(
        "L7d step7 UID and tombstone persist (i-03)",
        snapshot.uid == 0x00C0_FFEE
            && snapshot.tombstoned
            && snapshot.tombstone_time == 7
            && restored.uid == snapshot.uid
            && restored.tombstoned == snapshot.tombstoned
            && restored.tombstone_time == snapshot.tombstone_time
            && snapshot
                .clocks
                .iter()
                .all(|clock| clock.last_world_time_sample > 0)
            && restored.snapshot().clocks == snapshot.clocks,
        format!(
            "uid={:#X} tombstoned={} time={} · last_world_time_sample={:?} survives save→load \
             (`Quest.cs:119-122,230-242`, `Clock.cs:490-499`)",
            snapshot.uid,
            snapshot.tombstoned,
            snapshot.tombstone_time,
            snapshot
                .clocks
                .iter()
                .map(|clock| clock.last_world_time_sample)
                .collect::<Vec<_>>()
        ),
    ));
    let mut restored_for_var = restored.clone();
    restored_for_var.tick();
    rows.push(check(
        "L7e step7 local/global variables persist (i-04)",
        snapshot.variables.get("seen") == Some(&true)
            && snapshot.global_vars.get("g_var") == Some(&true)
            && restored.var("seen")
            && restored.global_var("g_var")
            && restored_for_var
                .task("waiting")
                .is_some_and(|task| task.is_triggered),
        format!(
            "variables={:?} global_vars={:?} survive save→load; a `Variable`/`GlobalVarLink` task \
             reads them on the next tick (Task.cs:107-114,493-494)",
            snapshot.variables, snapshot.global_vars
        ),
    ));

    let ok = print_rows("L · lifecycle steps 1-7 (§6.2)", &rows);
    if ok {
        println!("\nVERDICT: PROBE-QUEST-LIFECYCLE-001 PASS - 7/7 steps");
        0
    } else {
        println!("\nVERDICT: PROBE-QUEST-LIFECYCLE-001 FAIL - see L rows");
        1
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// PROBE-QUEST-JOURNAL-001
// ═══════════════════════════════════════════════════════════════════════════

/// Quest with 3 `LogMessage` + 1 `JournalNote` in one triggered task.
fn journal_quest() -> Quest {
    let mut quest = Quest::new("journal", 0);
    for id in 1..=4 {
        quest.add_message(id, format!("m{id}"));
    }
    let mut log = QuestTask::new("log", TaskType::Standard);
    for id in 1..=3 {
        log.add_action(QuestAction::new(OpcodeEffect::LogMessage).with_message(id));
    }
    log.add_action(QuestAction::new(OpcodeEffect::JournalNote).with_message(4));
    log.is_triggered = true;
    quest.add_task(log);
    quest
}

/// Quest whose single log action carries `message_id`.
fn single_action_quest(effect: OpcodeEffect, message_id: u16) -> Quest {
    let mut quest = Quest::new("single", 0);
    let mut task = QuestTask::new("t", TaskType::Standard);
    task.add_action(QuestAction::new(effect).with_message(message_id));
    task.is_triggered = true;
    quest.add_task(task);
    quest
}

/// Runs `PROBE-QUEST-JOURNAL-001` (§6.3 blocks A-D).
pub fn run_quest_journal_001() -> i32 {
    println!(
        "PROBE-QUEST-JOURNAL-001 - log, note, notebook and missing messages (M29, \
         CR-73d23384, REQ-L99-062 · spec §6.3)"
    );
    let mut rows: Vec<Row> = Vec::new();

    // ── A · 3 log + 1 note ⇒ 4 entries in insertion order ────────────────
    let mut quest = journal_quest();
    quest.tick();
    let messages: Vec<u16> = quest.journal().iter().map(|entry| entry.message_id).collect();
    let steps: Vec<u32> = quest.journal().iter().map(|entry| entry.step_id).collect();
    println!("journal: entries={} messages={messages:?}", quest.journal().len());
    rows.push(row(
        "J-A1 journal_entries=4",
        quest.journal().len().to_string(),
        "4",
        "3 LogMessage + 1 JournalNote in one tick (spec §6.3 A)",
    ));
    rows.push(check(
        "J-A2 insertion order",
        messages == vec![1u16, 2, 3, 4]
            && steps[..3].iter().all(|step| *step < NOTE_STEP_BASE)
            && steps[3] >= NOTE_STEP_BASE
            && check_journal_steps(&steps).is_ok(),
        format!(
            "order {messages:?}; steps {steps:?}: the three log steps stay below NOTE_STEP_BASE \
             and the note sits at it (Quest.cs:52,87-92)"
        ),
    ));

    // ── B · EndQuest ⇒ AddFinishedQuest(3) ───────────────────────────────
    quest.end_quest();
    let finished: Vec<(u32, u16)> = quest
        .finished_journal()
        .iter()
        .map(|entry| (entry.step_id, entry.message_id))
        .collect();
    let finished_messages: Vec<u16> = finished.iter().map(|(_, message)| *message).collect();
    println!(
        "notebook: AddFinishedQuest={} messages={finished_messages:?}",
        finished.len()
    );
    rows.push(check(
        "J-B1 AddFinishedQuest=3",
        finished.len() == 3
            && finished_messages == vec![1u16, 2, 3]
            && check_finished_journal(&finished).is_ok()
            && check_display_message_not_journaled(4, &messages[..3]).is_ok(),
        "only the log steps reach the Notebook; the journal note and any display message do not \
         (Quest.cs:388-398)",
    ));

    // ── C · sentinel and absent ids ──────────────────────────────────────
    let sentinel = single_action_quest(OpcodeEffect::LogMessage, NO_MESSAGE_ID);
    let mut sentinel = sentinel;
    sentinel.tick();
    let sentinel_failure = sentinel.failures().last().cloned();
    rows.push(check(
        "J-C1 sentinel 0xFFFF skipped=1",
        sentinel.message(NO_MESSAGE_ID).is_none()
            && sentinel.journal_skipped() == 1
            && sentinel.journal().is_empty()
            && sentinel_failure == Some(QuestFailure::MissingMessage(NO_MESSAGE_ID))
            && check_missing_message(NO_MESSAGE_ID, sentinel.succeeded()).is_ok(),
        format!(
            "GetMessage(0xFFFF) → None; skipped={} and failure={sentinel_failure:?} — no panic \
             (DFU would NRE in JournalNote.cs:50-52)",
            sentinel.journal_skipped()
        ),
    ));

    let absent = {
        let mut quest = single_action_quest(OpcodeEffect::JournalNote, 999);
        quest.tick();
        quest
    };
    let absent_failure = absent.failures().last().cloned();
    rows.push(check(
        "J-C2 absent id skipped=1",
        absent.message(999).is_none()
            && absent.journal_skipped() == 1
            && absent.journal().is_empty()
            && absent_failure == Some(QuestFailure::MissingMessage(999)),
        format!(
            "journal note with an absent id ⇒ skipped={} + failure={absent_failure:?} (never a \
             silent entry, never an NRE)",
            absent.journal_skipped()
        ),
    ));

    rows.push(check(
        "J-C3 no panic on missing messages",
        sentinel.failures().len() == 1 && absent.failures().len() == 1,
        "both cases recorded exactly one typed failure and returned control (0 panic / 0 NRE)",
    ));

    // ── C4 · JournalNote with the sentinel (the DFU NRE path) ────────────
    let note_sentinel = {
        let mut quest = single_action_quest(OpcodeEffect::JournalNote, NO_MESSAGE_ID);
        quest.tick();
        quest
    };
    let note_failure = note_sentinel.failures().last().cloned();
    rows.push(check(
        "J-C4 JournalNote 0xFFFF skipped=1",
        note_sentinel.journal_skipped() == 1
            && note_sentinel.journal().is_empty()
            && note_failure == Some(QuestFailure::MissingMessage(NO_MESSAGE_ID)),
        format!(
            "journal note with the sentinel id ⇒ skipped={} + failure={note_failure:?}, 0 panic \
             (the exact path that NREs in `JournalNote.cs:50-52`)",
            note_sentinel.journal_skipped()
        ),
    ));

    // ── E · the display mailbox drains once ──────────────────────────────
    let mut mailbox = Quest::new("mailbox", 0);
    mailbox.add_message(7, "seven");
    let display = DecodedOpcode {
        type_id: 0x33,
        message_id: 7,
        supported: true,
    };
    let _ = mailbox.execute_opcode(display);
    let _ = mailbox.execute_opcode(display);
    let queued = mailbox.pending_messages().len();
    let first = mailbox.take_pending_messages();
    let second = mailbox.take_pending_messages();

    let mut drained = Quest::new("drained", 0);
    drained.add_message(7, "seven");
    let mut say = QuestTask::new("say", TaskType::Standard);
    say.add_action(QuestAction::new(OpcodeEffect::DisplayMessage).with_message(7));
    say.is_triggered = true;
    drained.add_task(say);
    drained.tick();
    let after_say = drained.pending_messages().to_vec();
    drained.tick();
    rows.push(check(
        "J-E1 mailbox drains once (m-09)",
        queued == 2
            && first == vec![7, 7]
            && second.is_empty()
            && after_say == vec![7]
            && drained.pending_messages().is_empty()
            && drained.delivered_messages() == [7],
        format!(
            "2 displays ⇒ queued={queued}, take={first:?}, second take={second:?}; the tick cycle \
             drains into delivered={:?} and the mailbox stops growing (`Quest.cs:847-854`)",
            drained.delivered_messages()
        ),
    ));

    // ── D · reopen the journal ───────────────────────────────────────────
    let snapshot = quest.snapshot();
    let reopened = Quest::from_snapshot(snapshot.clone());
    let reopened_messages: Vec<u16> = reopened
        .journal()
        .iter()
        .map(|entry| entry.message_id)
        .collect();
    let reopened_steps: Vec<u32> = reopened.journal().iter().map(|entry| entry.step_id).collect();
    rows.push(check(
        "J-D1 reopen keeps 4 entries",
        reopened.journal().len() == 4 && reopened_messages == messages,
        format!(
            "reopened journal={} messages={reopened_messages:?} (identical to insertion order)",
            reopened.journal().len()
        ),
    ));
    rows.push(check(
        "J-D2 no duplicate stepID",
        check_journal_steps(&reopened_steps).is_ok()
            && reopened_steps.iter().copied().collect::<BTreeSet<u32>>().len() == 4,
        format!(
            "step ids {reopened_steps:?} unique · the stepID is the dedupe key \
             (Quest.cs:52,87-92)"
        ),
    ));

    let ok = print_rows("J · journal blocks A-D (§6.3)", &rows);
    if ok {
        println!("\nVERDICT: PROBE-QUEST-JOURNAL-001 PASS - A∧B∧C∧D");
        0
    } else {
        println!("\nVERDICT: PROBE-QUEST-JOURNAL-001 FAIL - see J rows");
        1
    }
}
