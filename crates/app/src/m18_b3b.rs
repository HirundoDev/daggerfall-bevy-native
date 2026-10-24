//! **B3-b** — retail dialogue (`TEXT.RSC`) and retail journal (`QBN`/`QRC`) of M18.
//!
//! - **dialogue (CS-15):** the greeting is the retail route
//!   [`simple_greeting`] with a very-high reaction (`≥ 30` ⇒ record `7209`) and
//!   two topics are the retail answers of one direction question and one
//!   "tell me about" question (`answersToDirections` /
//!   `answersToNonDirections`); every line is resolved from the user's
//!   `TEXT.RSC` — **0 invented prose**, text macros (`%pcf`, `_qgiver_`…) are
//!   shown verbatim and counted, never substituted;
//! - **journal (CS-16):** the main quests of the retail `.QBN`/`.QRC` set are
//!   projected into [`QuestRecord`] (title, section counts, messages with id),
//!   ingested in a [`QuestLog`] and rendered as the journal overlay.
//!
//! Overlays are multi-line RGBA textures built by stacking
//! [`FntFile::render_text`] masks with a fixed gap, painted only with the HUD
//! ink (`m20_ink`): 0 antialiasing, 0 off-ink pixels.
//!
//! Probe blocks:
//! - **A** greeting record 7209 resolved with non-empty retail text + 2 topics,
//!   digest ×2 (same two compositions ⇒ same digest);
//! - **B** journal: ≥1 main quest with messages, a sample message printed,
//!   digest ×2;
//! - **C** negative control: an absent quest or an absent text id is an
//!   explicit `Err` (never a fabricated fallback);
//! - **D** overlays: >0 opaque pixels and 0 colours outside the ink.
//!
//! Custody (INV-L99-6): 0 retail bytes in the repository — everything is read
//! from the user's ARENA2 at runtime and lives in memory.

use std::collections::BTreeMap;
use std::path::Path;

use daggerfall_behavior::dialogue::{
    direction_answer, non_direction_answer, simple_greeting, Greeting, SocialGroup,
    MAX_ANSWERS_TELL_ME_ABOUT, MIN_VERY_LIKE_REACTION,
};
use daggerfall_behavior::quest::{
    macro_counts, main_quest_names, QuestCounts, QuestLog, QuestMessage, QuestRecord,
};
use daggerfall_parser_rs::image::FntFile;
use daggerfall_parser_rs::quest::{Qbn, Qrc};
use daggerfall_parser_rs::text_rsc::TextRsc;

use crate::m18_demo::DemoTexture;

/// Greeting record of the retail route with a very-high reaction
/// (`TalkManager.cs:91-93,955-971`).
const DIALOGUE_REACTION: i16 = MIN_VERY_LIKE_REACTION;
/// Reaction band used by both topic answers (`0..2`, very good).
const TOPIC_REACTION_BAND: u8 = 2;
/// Social group of the street population (`Commoners`, `sgroup = 0`).
const TOPIC_GROUP: SocialGroup = SocialGroup::Commoners;
/// Vertical gap between stacked overlay lines (pixels).
const OVERLAY_LINE_GAP: u16 = 2;
/// Maximum characters rendered per overlay line (display clip, declared).
const OVERLAY_MAX_CHARS: usize = 64;
/// Maximum lines of one overlay (bounds the texture).
const OVERLAY_MAX_LINES: usize = 20;
/// Main quests previewed in the journal overlay (the probe reads them all).
pub const JOURNAL_PREVIEW_QUESTS: usize = 4;
/// Messages previewed per quest in the journal overlay.
pub const JOURNAL_PREVIEW_MESSAGES: usize = 3;

/// One retail dialogue line (record id + verbatim retail text).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DialogueLine {
    /// `TEXT.RSC` record id (retail source of the text).
    pub record: u16,
    /// Text resolved from that record (macros kept verbatim).
    pub text: String,
}

/// Retail dialogue of one street NPC: greeting + the two topic answers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dialogue {
    /// Greeting (`simple_greeting`, record 7209 with a very-high reaction).
    pub greeting: DialogueLine,
    /// Direction answer (`answersToDirections`).
    pub direction: DialogueLine,
    /// "Tell me about" answer (`answersToNonDirections`).
    pub topic: DialogueLine,
}

impl Dialogue {
    /// Every line in render order.
    pub fn lines(&self) -> [&DialogueLine; 3] {
        [&self.greeting, &self.direction, &self.topic]
    }

    /// Deterministic digest of the dialogue (ids + text).
    pub fn digest(&self) -> u64 {
        let mut bytes = Vec::with_capacity(96);
        for line in self.lines() {
            bytes.extend_from_slice(&line.record.to_le_bytes());
            bytes.extend_from_slice(line.text.as_bytes());
        }
        fnv1a64(&bytes)
    }

    /// Text macros present in the retail lines (kept verbatim, §7.4).
    pub fn macros(&self) -> BTreeMap<String, usize> {
        let mut out: BTreeMap<String, usize> = BTreeMap::new();
        for line in self.lines() {
            for (key, count) in macro_counts(&line.text) {
                *out.entry(key).or_insert(0) += count;
            }
        }
        out
    }
}

/// Retail journal: main quests ingested in a [`QuestLog`] (CS-16).
#[derive(Debug, Clone, Default)]
pub struct Journal {
    /// Projected main quests (name, counts, messages).
    pub log: QuestLog,
    /// Main quest names found in the retail `.QBN`/`.QRC` set.
    pub main_total: usize,
    /// Messages resolved across the ingested quests.
    pub messages: usize,
}

impl Journal {
    /// Ingested records in ingestion order.
    pub fn records(&self) -> &[QuestRecord] {
        self.log.records()
    }

    /// Deterministic digest (name + counts + messages).
    pub fn digest(&self) -> u64 {
        let mut bytes = Vec::new();
        for record in self.records() {
            bytes.extend_from_slice(record.name.as_bytes());
            let counts = record.counts;
            for value in [
                counts.items,
                counts.npcs,
                counts.locations,
                counts.timers,
                counts.mobs,
                counts.opcodes,
                counts.states,
                counts.text_variables,
            ] {
                bytes.extend_from_slice(&value.to_le_bytes());
            }
            for message in &record.messages {
                bytes.extend_from_slice(&message.id.to_le_bytes());
                bytes.extend_from_slice(message.text.as_bytes());
            }
        }
        fnv1a64(&bytes)
    }
}

/// FNV-1a 64 over bytes (deterministic digest, no dependencies).
fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// Reads a retail file from ARENA2 Mode A.
fn load(name: &str) -> Result<Vec<u8>, String> {
    let path = crate::mode_a_arena2_path(name)?;
    std::fs::read(&path).map_err(|err| format!("reading {path:?}: {err}"))
}

/// Sorted `(stem, bytes)` of every file with `extension` in `dir`.
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

/// Resolves one `TEXT.RSC` record id to its retail text.
///
/// An absent id is an explicit error — the dialogue never falls back to an
/// invented string.
pub fn text_line(text: &TextRsc, bytes: &[u8], record: u16) -> Result<DialogueLine, String> {
    let resolved = text
        .record_text(bytes, record)
        .map_err(|err| format!("TEXT.RSC record {record}: {err:?}"))?;
    if resolved.trim().is_empty() {
        return Err(format!("TEXT.RSC record {record} is empty"));
    }
    Ok(DialogueLine {
        record,
        text: resolved,
    })
}

/// Builds the retail dialogue from an already parsed `TEXT.RSC`.
pub fn dialogue_from(text: &TextRsc, bytes: &[u8]) -> Result<Dialogue, String> {
    let greeting = match simple_greeting(DIALOGUE_REACTION, false) {
        Greeting::Record(record) => text_line(text, bytes, record)?,
        Greeting::NoAnswer => {
            return Err(format!(
                "simple_greeting({DIALOGUE_REACTION}) returned no answer"
            ))
        }
    };
    // Two topics, each one honoring the retail cap of answers per question
    // (`MAX_ANSWERS_TELL_ME_ABOUT`, `TalkManager.cs:125`): one direction answer
    // and one "tell me about" answer.
    let direction_record = direction_answer(TOPIC_GROUP, TOPIC_REACTION_BAND);
    let topic_record = non_direction_answer(true, TOPIC_GROUP, TOPIC_REACTION_BAND);
    if direction_record == topic_record {
        return Err(format!(
            "retail answer tables collapsed to one record ({direction_record})"
        ));
    }
    if MAX_ANSWERS_TELL_ME_ABOUT == 0 {
        return Err("retail answer cap is 0".to_string());
    }
    Ok(Dialogue {
        greeting,
        direction: text_line(text, bytes, direction_record)?,
        topic: text_line(text, bytes, topic_record)?,
    })
}

/// Loads `TEXT.RSC` from ARENA2 and builds the retail dialogue.
pub fn load_dialogue() -> Result<Dialogue, String> {
    let bytes = load("TEXT.RSC")?;
    let text = TextRsc::parse(&bytes).map_err(|err| format!("TEXT.RSC: {err:?}"))?;
    dialogue_from(&text, &bytes)
}

/// Projects `QBN` + `QRC` into a [`QuestRecord`] (0 invented string).
fn build_record(
    name: &str,
    qbn: Option<&Qbn>,
    qrc: Option<(&Qrc, &[u8])>,
) -> Result<QuestRecord, String> {
    let counts = match qbn {
        Some(container) => QuestCounts {
            items: container.section_counts[0],
            npcs: container.section_counts[3],
            locations: container.section_counts[4],
            timers: container.section_counts[6],
            mobs: container.section_counts[7],
            opcodes: container.section_counts[8],
            states: container.section_counts[9],
            text_variables: container.text_variable_records,
        },
        None => QuestCounts::default(),
    };
    let mut messages = Vec::new();
    if let Some((qrc, bytes)) = qrc {
        for index in 0..qrc.record_count() {
            let id = qrc.text.records[index].id;
            let text = qrc
                .text
                .record_text_by_index(bytes, index)
                .map_err(|err| format!("{name} id {id}: {err:?}"))?;
            messages.push(QuestMessage { id, text });
        }
    }
    Ok(QuestRecord::new(name, counts, messages))
}

/// Loads the main quests of the retail `.QBN`/`.QRC` set from `dir`.
pub fn load_journal(dir: &Path) -> Result<Journal, String> {
    let qbn_files = collect(dir, "QBN")?;
    let qrc_files = collect(dir, "QRC")?;
    let mut names: Vec<String> = Vec::with_capacity(qbn_files.len() + qrc_files.len());
    names.extend(qbn_files.iter().map(|(stem, _)| stem.clone()));
    for (stem, _) in &qrc_files {
        if !names.iter().any(|name| name == stem) {
            names.push(stem.clone());
        }
    }
    let main = main_quest_names(&names);
    let mut containers: BTreeMap<&str, Qbn> = BTreeMap::new();
    for (name, bytes) in &qbn_files {
        let container = Qbn::parse(bytes).map_err(|err| format!("QBN {name}: {err:?}"))?;
        containers.insert(name.as_str(), container);
    }
    let mut texts: BTreeMap<&str, (Qrc, &[u8])> = BTreeMap::new();
    for (name, bytes) in &qrc_files {
        let qrc = Qrc::parse(bytes).map_err(|err| format!("QRC {name}: {err:?}"))?;
        texts.insert(name.as_str(), (qrc, bytes.as_slice()));
    }
    let mut log = QuestLog::new();
    let mut messages = 0usize;
    for name in &main {
        let qrc = texts.get(name.as_str()).map(|(qrc, bytes)| (qrc, *bytes));
        if qrc.is_none() {
            // Declared: a main quest without QRC carries no retail text ⇒ it is
            // not ingested (0 invented messages).
            println!("M18 VE: journal — main quest {name} has no QRC (not ingested)");
            continue;
        }
        let record = build_record(name, containers.get(name.as_str()), qrc)?;
        messages += record.messages.len();
        if !log.ingest(record) {
            return Err(format!("duplicate quest {name} in the journal"));
        }
    }
    Ok(Journal {
        log,
        main_total: main.len(),
        messages,
    })
}

/// Resolves a message of an ingested quest; an absent quest/id is `Err`.
pub fn journal_message<'a>(journal: &'a Journal, name: &str, id: u16) -> Result<&'a str, String> {
    if journal.log.record(name).is_none() {
        return Err(format!("quest {name} absent from the retail journal"));
    }
    journal
        .log
        .message(name, id)
        .ok_or_else(|| format!("quest {name} has no message {id}"))
}

/// Resolves a dialogue record; an absent id is `Err`.
pub fn dialogue_line(dialogue: &Dialogue, record: u16) -> Result<&str, String> {
    dialogue
        .lines()
        .into_iter()
        .find(|line| line.record == record)
        .map(|line| line.text.as_str())
        .ok_or_else(|| format!("dialogue line {record} absent"))
}

/// One overlay line source: the text, already split on newlines and clipped.
fn overlay_lines(text: &str) -> impl Iterator<Item = String> + '_ {
    text.split('\n').map(|line| {
        let line = line.trim_end();
        if line.chars().count() > OVERLAY_MAX_CHARS {
            line.chars().take(OVERLAY_MAX_CHARS).collect()
        } else {
            line.to_string()
        }
    })
}

/// Renders `lines` with the retail `.FNT` and the HUD ink, stacked with
/// [`OVERLAY_LINE_GAP`] (0 off-ink pixel: transparency outside the glyphs).
pub fn text_overlay(font: &FntFile, ink: [u8; 3], lines: &[String]) -> DemoTexture {
    let clipped: Vec<String> = lines
        .iter()
        .flat_map(|line| overlay_lines(line))
        .take(OVERLAY_MAX_LINES)
        .collect();
    let masks: Vec<_> = clipped.iter().map(|line| font.render_text(line)).collect();
    let width = masks
        .iter()
        .map(|mask| mask.width)
        .max()
        .unwrap_or(1)
        .max(1);
    let mut height = 0u32;
    for (index, mask) in masks.iter().enumerate() {
        height += u32::from(mask.height);
        if index + 1 < masks.len() {
            height += u32::from(OVERLAY_LINE_GAP);
        }
    }
    let height = height.max(1) as u16;
    let mut rgba = vec![0u8; usize::from(width) * usize::from(height) * 4];
    let mut y_offset = 0u16;
    for mask in &masks {
        for y in 0..mask.height {
            for x in 0..mask.width {
                if !mask.pixel(x, y) {
                    continue;
                }
                let at = ((y_offset + y) as usize * usize::from(width) + x as usize) * 4;
                rgba[at] = ink[0];
                rgba[at + 1] = ink[1];
                rgba[at + 2] = ink[2];
                rgba[at + 3] = 255;
            }
        }
        y_offset += mask.height + OVERLAY_LINE_GAP;
    }
    DemoTexture {
        key: daggerfall_parser_bevy_bridge::mesh_spec::NO_TEXTURE,
        width,
        height,
        rgba,
    }
}

/// Dialogue overlay: `[record] text` per retail line (0 invented words).
pub fn dialogue_overlay(font: &FntFile, ink: [u8; 3], dialogue: &Dialogue) -> DemoTexture {
    let lines: Vec<String> = dialogue
        .lines()
        .iter()
        .map(|line| format!("[{}] {}", line.record, line.text))
        .collect();
    text_overlay(font, ink, &lines)
}

/// Journal overlay: bounded preview of the main quests (title + messages).
pub fn journal_overlay(font: &FntFile, ink: [u8; 3], journal: &Journal) -> DemoTexture {
    let mut lines: Vec<String> = Vec::new();
    for record in journal.records().iter().take(JOURNAL_PREVIEW_QUESTS) {
        lines.push(format!("{} — {}", record.name, record.title));
        for message in record.messages.iter().take(JOURNAL_PREVIEW_MESSAGES) {
            lines.push(format!("  [{}] {}", message.id, message.text));
        }
    }
    if lines.is_empty() {
        // Empty journal (no quest ingested): the overlay carries no invented
        // text, only its own declared emptiness marker.
        lines.push(format!("journal: 0/{} main quests", journal.main_total));
    }
    text_overlay(font, ink, &lines)
}

/// Both retail overlays of the M18 window (B3-b): dialogue + journal.
#[cfg_attr(not(feature = "window"), allow(dead_code))]
pub struct Overlays {
    /// Dialogue overlay texture (`[record] text` per retail line).
    pub dialogue: DemoTexture,
    /// Journal overlay texture (bounded preview of the main quests).
    pub journal: DemoTexture,
    /// One-line evidence of the dialogue (ids + digest).
    pub dialogue_summary: String,
    /// One-line evidence of the journal (quests / messages / digest).
    pub journal_summary: String,
}

/// Composes both overlays from the user's ARENA2 (0 retail bytes in the repo).
///
/// The window calls this once at startup; a missing retail file is an explicit
/// `Err` and the window degrades without inventing text.
#[cfg_attr(not(feature = "window"), allow(dead_code))]
pub fn compose_overlays(font: &FntFile, ink: [u8; 3]) -> Result<Overlays, String> {
    let dir = crate::mode_a_arena2_dir()?;
    let dialogue = load_dialogue()?;
    let journal = load_journal(&dir)?;
    let dialogue_tex = dialogue_overlay(font, ink, &dialogue);
    let journal_tex = journal_overlay(font, ink, &journal);
    Ok(Overlays {
        dialogue_summary: format!(
            "records {} / {} / {} · {}×{} px · {} opaque px · digest {:016x}",
            dialogue.greeting.record,
            dialogue.direction.record,
            dialogue.topic.record,
            dialogue_tex.width,
            dialogue_tex.height,
            opaque_pixels(&dialogue_tex),
            dialogue.digest()
        ),
        journal_summary: format!(
            "{} main quests · {} messages · {}×{} px · {} opaque px · digest {:016x}",
            journal.records().len(),
            journal.messages,
            journal_tex.width,
            journal_tex.height,
            opaque_pixels(&journal_tex),
            journal.digest()
        ),
        dialogue: dialogue_tex,
        journal: journal_tex,
    })
}

/// Marks a check and prints it (`[OK]`/`[FAIL]`).
fn check(passed: &mut bool, condition: bool, label: &str, detail: String) {
    if !condition {
        *passed = false;
    }
    println!(
        "[{}] {label} - {detail}",
        if condition { "OK" } else { "FAIL" }
    );
}

/// Opaque pixels of an overlay texture.
fn opaque_pixels(texture: &DemoTexture) -> usize {
    texture.rgba.chunks_exact(4).filter(|px| px[3] != 0).count()
}

/// Pixels whose colour is not the ink (0 expected) and partially transparent
/// pixels (0 expected).
fn off_ink_pixels(texture: &DemoTexture, ink: [u8; 3]) -> (usize, usize) {
    let mut off_ink = 0usize;
    let mut partial = 0usize;
    for px in texture.rgba.chunks_exact(4) {
        match px[3] {
            0 => {}
            255 => {
                if px[0..3] != ink {
                    off_ink += 1;
                }
            }
            _ => partial += 1,
        }
    }
    (off_ink, partial)
}

/// Runs `PROBE-RETAIL-TEXT-001` — CS-15 dialogue + CS-16 journal.
pub fn run_retail_text_001() -> i32 {
    println!("PROBE-RETAIL-TEXT-001 - M18 B3-b retail dialogue (TEXT.RSC) + journal (QBN/QRC)");
    let dir = match crate::mode_a_arena2_dir() {
        Ok(dir) => dir,
        Err(reason) => {
            println!("ARENA2 not accessible: {reason}");
            println!("VERDICT: PROBE-RETAIL-TEXT-001 NO DATA - ARENA2 not accessible");
            return 2;
        }
    };
    let mut passed = true;

    // ── A · retail greeting + 2 topics, determinism ×2 ──────────────────
    let dialogue = match load_dialogue() {
        Ok(dialogue) => dialogue,
        Err(reason) => {
            println!("[FAIL] A dialogue: {reason}");
            println!("VERDICT: PROBE-RETAIL-TEXT-001 FAIL - dialogue");
            return 1;
        }
    };
    let dialogue_2 = match load_dialogue() {
        Ok(dialogue) => dialogue,
        Err(reason) => {
            println!("[FAIL] A dialogue 2: {reason}");
            println!("VERDICT: PROBE-RETAIL-TEXT-001 FAIL - dialogue 2");
            return 1;
        }
    };
    let greeting_ok = dialogue.greeting.record == 7209
        && !dialogue.greeting.text.trim().is_empty()
        && dialogue.direction.record != dialogue.topic.record
        && !dialogue.direction.text.trim().is_empty()
        && !dialogue.topic.text.trim().is_empty();
    check(
        &mut passed,
        greeting_ok,
        "A greeting",
        format!(
            "record {} (retail 7209) · {} chars · topics {} / {} · sources TEXT.RSC",
            dialogue.greeting.record,
            dialogue.greeting.text.chars().count(),
            dialogue.direction.record,
            dialogue.topic.record
        ),
    );
    let macros = dialogue.macros();
    check(
        &mut passed,
        dialogue.digest() == dialogue_2.digest(),
        "A determinism",
        format!(
            "digest {:016x} ×2 · macros kept verbatim: {:?}",
            dialogue.digest(),
            macros
        ),
    );

    // ── B · journal of the main quests ──────────────────────────────────
    let journal = match load_journal(&dir) {
        Ok(journal) => journal,
        Err(reason) => {
            println!("[FAIL] B journal: {reason}");
            println!("VERDICT: PROBE-RETAIL-TEXT-001 FAIL - journal");
            return 1;
        }
    };
    let journal_2 = match load_journal(&dir) {
        Ok(journal) => journal,
        Err(reason) => {
            println!("[FAIL] B journal 2: {reason}");
            println!("VERDICT: PROBE-RETAIL-TEXT-001 FAIL - journal 2");
            return 1;
        }
    };
    let with_messages = journal
        .records()
        .iter()
        .filter(|record| !record.messages.is_empty())
        .count();
    let sample = journal
        .records()
        .iter()
        .find(|record| !record.messages.is_empty())
        .and_then(|record| {
            record
                .messages
                .iter()
                .find(|message| !message.text.trim().is_empty())
                .map(|message| format!("{} [{}] {}", record.name, message.id, message.text))
        });
    check(
        &mut passed,
        with_messages >= 1 && journal.messages > 0 && sample.is_some(),
        "B journal",
        format!(
            "{}/{} main quests ingested · {} messages · {} with messages · sample: {}",
            journal.records().len(),
            journal.main_total,
            journal.messages,
            with_messages,
            sample.as_deref().unwrap_or("none")
        ),
    );
    check(
        &mut passed,
        journal.digest() == journal_2.digest(),
        "B determinism",
        format!("digest {:016x} ×2", journal.digest()),
    );

    // ── C · negative control (absent quest/id ⇒ Err, 0 invented text) ───
    let absent_quest = journal_message(&journal, "ZZZZZZZZ", 1000).is_err();
    let absent_message = journal
        .records()
        .first()
        .map(|record| record.name.as_str())
        .is_some_and(|name| journal_message(&journal, name, u16::MAX).is_err());
    let absent_dialogue = dialogue_line(&dialogue, u16::MAX).is_err();
    let text_bytes = match load("TEXT.RSC") {
        Ok(bytes) => bytes,
        Err(reason) => {
            println!("ARENA2 not accessible: {reason}");
            println!("VERDICT: PROBE-RETAIL-TEXT-001 NO DATA - TEXT.RSC unreadable");
            return 2;
        }
    };
    let text = match TextRsc::parse(&text_bytes) {
        Ok(text) => text,
        Err(err) => {
            println!("[FAIL] C TEXT.RSC: {err:?}");
            println!("VERDICT: PROBE-RETAIL-TEXT-001 FAIL - TEXT.RSC");
            return 1;
        }
    };
    let absent_record = text_line(&text, &text_bytes, u16::MAX).is_err();
    check(
        &mut passed,
        absent_quest && absent_message && absent_dialogue && absent_record,
        "C negative_control",
        format!(
            "absent quest err={absent_quest} · absent message err={absent_message} · absent dialogue err={absent_dialogue} · absent TEXT.RSC id err={absent_record} (0 invented fallback)"
        ),
    );

    // ── D · overlays (opaque pixels + ink-only colours) ─────────────────
    let font = match crate::ui_assets::load_ui_assets(&dir)
        .map_err(|err| err.to_string())
        .and_then(|assets| {
            assets
                .font(daggerfall_parser_bevy_bridge::ui_spec::DEFAULT_FONT)
                .cloned()
                .ok_or_else(|| "FONT0000.FNT missing".to_string())
        }) {
        Ok(font) => font,
        Err(reason) => {
            println!("ARENA2 UI assets not loadable: {reason}");
            println!("VERDICT: PROBE-RETAIL-TEXT-001 NO DATA - UI font not loadable");
            return 2;
        }
    };
    let ink = [255u8, 255, 255];
    let dialogue_tex = dialogue_overlay(&font, ink, &dialogue);
    let journal_tex = journal_overlay(&font, ink, &journal);
    let dialogue_opaque = opaque_pixels(&dialogue_tex);
    let journal_opaque = opaque_pixels(&journal_tex);
    let (dialogue_off, dialogue_partial) = off_ink_pixels(&dialogue_tex, ink);
    let (journal_off, journal_partial) = off_ink_pixels(&journal_tex, ink);
    check(
        &mut passed,
        dialogue_opaque > 0
            && journal_opaque > 0
            && dialogue_off == 0
            && journal_off == 0
            && dialogue_partial == 0
            && journal_partial == 0,
        "D overlays",
        format!(
            "dialogue {}×{} {} opaque px · journal {}×{} {} opaque px · off-ink {} / {} · partial alpha {} / {}",
            dialogue_tex.width,
            dialogue_tex.height,
            dialogue_opaque,
            journal_tex.width,
            journal_tex.height,
            journal_opaque,
            dialogue_off,
            journal_off,
            dialogue_partial,
            journal_partial
        ),
    );

    if passed {
        println!(
            "VERDICT: PROBE-RETAIL-TEXT-001 PASS - greeting 7209 + 2 topics resolved from TEXT.RSC · {} main quests / {} QRC messages · dialogue digest {:016x} · journal digest {:016x} · overlays ink-only",
            journal.records().len(),
            journal.messages,
            dialogue.digest(),
            journal.digest()
        );
        0
    } else {
        println!("VERDICT: PROBE-RETAIL-TEXT-001 FAIL");
        1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fnv_is_stable() {
        assert_eq!(fnv1a64(b""), 0xcbf2_9ce4_8422_2325);
        assert_eq!(fnv1a64(b"a"), 0xaf63_dc4c_8601_ec8c);
    }

    #[test]
    fn absent_dialogue_line_is_error() {
        let dialogue = Dialogue {
            greeting: DialogueLine {
                record: 7209,
                text: "x".into(),
            },
            direction: DialogueLine {
                record: 1,
                text: "y".into(),
            },
            topic: DialogueLine {
                record: 2,
                text: "z".into(),
            },
        };
        assert!(dialogue_line(&dialogue, 7209).is_ok());
        assert!(dialogue_line(&dialogue, 999).is_err());
        assert_eq!(dialogue.digest(), dialogue.digest());
    }

    #[test]
    fn empty_journal_message_is_error() {
        let journal = Journal {
            log: QuestLog::new(),
            main_total: 0,
            messages: 0,
        };
        assert!(journal_message(&journal, "ZZZZZZZZ", 1000).is_err());
        assert_eq!(journal.digest(), 0xcbf2_9ce4_8422_2325);
    }
}
