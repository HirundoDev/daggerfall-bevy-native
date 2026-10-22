//! `quest` — session active-quest log (M7) and retail quest ingestion
//! (M16, REQ-L99-049).
//!
//! **Declared minimal contract** (C3 pack M7): the source of the read model
//! Inventory/Quests of PD4 T7 ("Inventory/active quests → projection
//! from **AssetSet/Quest**", anchor REQ-L99-040 — behavior→perception;
//! the AssetSet is supplied by 📦 inventory reusing `InventoryLite`, the
//! existing inventory source — mapping documented in the M7 note,
//! 0 duplicated type).
//!
//! 🎭 Behavior organ rules (bevy-bridge-matrix): gameplay rules/domain ·
//! **0 Bevy** · **0 persistence**. `QuestLog` is the in-session memory log:
//! accept/complete with unique ids, deterministic insertion order.
//!
//! **M16 (additive, M7/M8 API intact):** [`QuestRecord`] is the projection of
//! an already parsed retail quest (`.QBN` = container, `.QRC` = messages
//! resolved by text route A); [`QuestLog::ingest`] accepts it into the
//! active log and keeps its messages. Parsing lives in
//! `daggerfall_parser_rs::quest`; this organ does not depend on the parser (DAG:
//! `app` → `parser-bevy-bridge` → `daggerfall_parser_rs`).
//!
//! The two ingestion rules of spec M16 §7 also live here:
//! [`main_quest_names`] (`name[0] == 'S'` ⇒ main quest) and
//! [`sample_side_quests`] (one specimen per `byte0 × byte2` cell, chosen by
//! lexicographic order), plus the text macro counter
//! ([`macro_counts`], §7.4) — 0 invented string: everything comes from the `.QRC`.

use std::collections::{BTreeMap, BTreeSet};

/// Session quest (unique identifier + visible title).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Quest {
    /// Stable id (uniqueness in the log).
    pub id: String,
    /// Visible title (quest panel).
    pub title: String,
}

/// Family of a retail quest, derived from the file name (spec §4.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuestFamily {
    /// `S…` — main quests (34 in retail).
    Main,
    /// `_…` — starting quests (Tutor/Initial).
    Starting,
    /// `$…` — cure (vampire/lycanthropy).
    Cure,
    /// Standard family (guild/temple/royalty/daedra/commoner…).
    Standard,
}

impl QuestFamily {
    /// Family of `name` by its first byte (`UESP` Appendix B).
    pub fn of(name: &str) -> Self {
        match name.as_bytes().first() {
            Some(b'S') => Self::Main,
            Some(b'_') => Self::Starting,
            Some(b'$') => Self::Cure,
            _ => Self::Standard,
        }
    }

    /// Stable label (for reports).
    pub fn label(self) -> &'static str {
        match self {
            Self::Main => "main",
            Self::Starting => "starting",
            Self::Cure => "cure",
            Self::Standard => "standard",
        }
    }
}

/// Section counters of a `.QBN` (container; 0 opcode semantics).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct QuestCounts {
    /// Section 0 — Items (19 B/record).
    pub items: u16,
    /// Section 3 — Npcs (20 B/record).
    pub npcs: u16,
    /// Section 4 — Locations (24 B/record).
    pub locations: u16,
    /// Section 6 — Timers (33 B/record).
    pub timers: u16,
    /// Section 7 — Mobs (14 B/record).
    pub mobs: u16,
    /// Section 8 — OpCodes (87 B/record, opaque).
    pub opcodes: u16,
    /// Section 9 — States (8 B/record).
    pub states: u16,
    /// Section 10 — TextVariables (27 B/record; optional).
    pub text_variables: u16,
}

/// Quest message resolved from its `.QRC` (rendered text, route A).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuestMessage {
    /// `TextRecordId` of the `.QRC` (1000…2011 in retail).
    pub id: u16,
    /// Plain text rendered from the retail tokens.
    pub text: String,
}

/// Retail quest projected into the session log.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuestRecord {
    /// Retail name (stem of the `.QBN`, e.g. `A0C00Y00`).
    pub name: String,
    /// Visible title: first line of message 1000, or the name if missing.
    pub title: String,
    /// Family derived from the name.
    pub family: QuestFamily,
    /// Section counters of the `.QBN`.
    pub counts: QuestCounts,
    /// Messages resolved from the `.QRC` in table order.
    pub messages: Vec<QuestMessage>,
}

impl QuestRecord {
    /// Builds the record; the title comes from message 1000 (0 invented).
    pub fn new(name: &str, counts: QuestCounts, messages: Vec<QuestMessage>) -> Self {
        let title = messages
            .iter()
            .find(|message| message.id == 1000)
            .map(|message| first_line(&message.text).to_string())
            .filter(|title| !title.is_empty())
            .unwrap_or_else(|| name.to_string());
        Self {
            name: name.to_string(),
            title,
            family: QuestFamily::of(name),
            counts,
            messages,
        }
    }

    /// Message by id (first record with that id).
    pub fn message(&self, id: u16) -> Option<&str> {
        self.messages
            .iter()
            .find(|message| message.id == id)
            .map(|message| message.text.as_str())
    }

    /// Text macros counted over all messages (§7.4).
    pub fn macros(&self) -> BTreeMap<String, usize> {
        let text = self
            .messages
            .iter()
            .map(|message| message.text.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        macro_counts(&text)
    }
}

/// Session active quests (source of the Panel read model).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct QuestLog {
    active: Vec<Quest>,
    records: Vec<QuestRecord>,
}

impl QuestLog {
    /// Empty log (0 active quests).
    pub fn new() -> Self {
        Self::default()
    }

    /// Accepts a quest. Returns `true` if it is fresh (`false` = id already
    /// active — no duplicates, no mutation).
    pub fn accept(&mut self, id: &str, title: &str) -> bool {
        if self.active.iter().any(|quest| quest.id == id) {
            return false;
        }
        self.active.push(Quest {
            id: id.to_string(),
            title: title.to_string(),
        });
        true
    }

    /// Ingests a retail quest: accepts it and keeps its messages.
    ///
    /// Returns `false` without mutation if the id is already active.
    pub fn ingest(&mut self, record: QuestRecord) -> bool {
        if !self.accept(&record.name, &record.title) {
            return false;
        }
        self.records.push(record);
        true
    }

    /// Completes (and removes) an active quest by id.
    pub fn complete(&mut self, id: &str) -> Option<Quest> {
        let position = self.active.iter().position(|quest| quest.id == id)?;
        Some(self.active.remove(position))
    }

    /// Is quest `id` active?
    pub fn is_active(&self, id: &str) -> bool {
        self.active.iter().any(|quest| quest.id == id)
    }

    /// Active quests in acceptance order (deterministic).
    pub fn active(&self) -> &[Quest] {
        &self.active
    }

    /// Ingested retail records (in ingestion order; the history survives
    /// [`QuestLog::complete`]).
    pub fn records(&self) -> &[QuestRecord] {
        &self.records
    }

    /// Retail record by quest name.
    pub fn record(&self, name: &str) -> Option<&QuestRecord> {
        self.records.iter().find(|record| record.name == name)
    }

    /// Resolved message of an ingested quest.
    pub fn message(&self, name: &str, id: u16) -> Option<&str> {
        self.record(name).and_then(|record| record.message(id))
    }

    /// Number of active quests.
    pub fn len(&self) -> usize {
        self.active.len()
    }

    /// No active quests?
    pub fn is_empty(&self) -> bool {
        self.active.is_empty()
    }
}

/// Main quests: names with `name[0] == 'S'` in lexicographic order
/// (verifiable criterion of spec M16 §7.1; 34 in retail).
pub fn main_quest_names(names: &[String]) -> Vec<String> {
    let mut main: Vec<String> = names
        .iter()
        .filter(|name| name.as_bytes().first() == Some(&b'S'))
        .cloned()
        .collect();
    main.sort();
    main
}

/// Deterministic side-quest sampling: one specimen per `byte0 × byte2` cell,
/// the first in lexicographic order (spec §7.2).
pub fn sample_side_quests(names: &[String]) -> Vec<String> {
    let mut side: Vec<&String> = names
        .iter()
        .filter(|name| name.as_bytes().first() != Some(&b'S'))
        .collect();
    side.sort();
    let mut cells: BTreeSet<(u8, u8)> = BTreeSet::new();
    let mut sampled = Vec::new();
    for name in side {
        let bytes = name.as_bytes();
        let cell = (
            bytes.first().copied().unwrap_or(0),
            bytes.get(2).copied().unwrap_or(0),
        );
        if cells.insert(cell) {
            sampled.push(name.clone());
        }
    }
    sampled
}

/// Counts text macros of a `.QRC` per spec M16 §7.4.
///
/// Recognizes `%name`, `=name_` and `_+name_` (the forms `_x_`, `__x__`,
/// `___x___` normalize to `_x_`; two adjacent macros such as
/// `_npc2__qgiver_` are counted separately). Returns keys with their sigil
/// (`%pcf`, `_qgiver_`, `=traveltime_`) and occurrence count.
pub fn macro_counts(text: &str) -> BTreeMap<String, usize> {
    fn scan_alnum(bytes: &[u8], mut index: usize) -> usize {
        while index < bytes.len() && bytes[index].is_ascii_alphanumeric() {
            index += 1;
        }
        index
    }

    let bytes = text.as_bytes();
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'%' => {
                let end = scan_alnum(bytes, index + 1);
                if end > index + 1 {
                    *counts.entry(format!("%{}", &text[index + 1..end])).or_insert(0) += 1;
                    index = end;
                } else {
                    index += 1;
                }
            }
            b'=' => {
                let end = scan_alnum(bytes, index + 1);
                if end > index + 1 && bytes.get(end) == Some(&b'_') {
                    *counts
                        .entry(format!("={}_", &text[index + 1..end]))
                        .or_insert(0) += 1;
                    index = end + 1;
                } else {
                    index += 1;
                }
            }
            b'_' => {
                let mut start = index;
                while start < bytes.len() && bytes[start] == b'_' {
                    start += 1;
                }
                let end = scan_alnum(bytes, start);
                if end > start && bytes.get(end) == Some(&b'_') {
                    *counts
                        .entry(format!("_{}_", &text[start..end]))
                        .or_insert(0) += 1;
                    index = end + 1;
                } else {
                    index += 1;
                }
            }
            _ => index += 1,
        }
    }
    counts
}

/// First non-empty line of a resolved text.
fn first_line(text: &str) -> &str {
    text.lines().next().unwrap_or("").trim()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accept_rejects_duplicates_and_keeps_order() {
        let mut log = QuestLog::new();
        assert!(log.accept("q1", "First"));
        assert!(log.accept("q2", "Second"));
        assert!(!log.accept("q1", "Duplicate"));
        assert_eq!(log.len(), 2);
        let ids: Vec<&str> = log.active().iter().map(|quest| quest.id.as_str()).collect();
        assert_eq!(ids, vec!["q1", "q2"]);
        assert!(log.is_active("q1"));
        assert!(!log.is_active("q9"));
    }

    #[test]
    fn complete_removes_only_the_target() {
        let mut log = QuestLog::new();
        log.accept("q1", "First");
        log.accept("q2", "Second");
        let done = log.complete("q1").expect("q1 active");
        assert_eq!(done.id, "q1");
        assert!(!log.is_active("q1"));
        assert!(log.is_active("q2"));
        assert!(log.complete("q9").is_none());
    }

    fn message(id: u16, text: &str) -> QuestMessage {
        QuestMessage {
            id,
            text: text.to_string(),
        }
    }

    #[test]
    fn ingest_keeps_messages_and_rejects_duplicates() {
        let mut log = QuestLog::new();
        let record = QuestRecord::new(
            "A0C00Y00",
            QuestCounts {
                items: 10,
                opcodes: 44,
                ..QuestCounts::default()
            },
            vec![
                message(1000, "My name is _qgiver_.\nSecond line"),
                message(1001, "No thanks."),
            ],
        );
        // §4.1: main = `name[0] == 'S'` (34 in retail); `A0C00Y00` is
        // standard by that rule even though it is the opening quest in DFU.
        assert_eq!(record.family, QuestFamily::Standard);
        assert_eq!(record.title, "My name is _qgiver_.");
        assert!(log.ingest(record.clone()));
        assert!(!log.ingest(record));
        assert_eq!(log.len(), 1);
        assert_eq!(log.records().len(), 1);
        assert_eq!(log.message("A0C00Y00", 1001), Some("No thanks."));
        assert_eq!(log.message("A0C00Y00", 9999), None);
        assert_eq!(log.record("A0C00Y00").unwrap().counts.opcodes, 44);
        assert!(log.complete("A0C00Y00").is_some());
        assert_eq!(log.records().len(), 1, "the history survives complete");
    }

    #[test]
    fn title_falls_back_to_name_without_message_1000() {
        let record = QuestRecord::new(
            "M0B40Y04",
            QuestCounts::default(),
            vec![message(1001, "  ")],
        );
        assert_eq!(record.title, "M0B40Y04");
        assert_eq!(record.family, QuestFamily::Standard);
        assert_eq!(QuestFamily::of("_BRISIEN"), QuestFamily::Starting);
        assert_eq!(QuestFamily::of("$CUREVAM"), QuestFamily::Cure);
    }

    #[test]
    fn main_and_side_sampling_is_deterministic() {
        let names: Vec<String> = [
            "S0000010",
            "S0000001",
            "A0C00Y00",
            "A0C00Y04",
            "A0B00Y00",
            "K0C00Y06",
            "_TUTOR__",
            "_BRISIEN",
        ]
        .iter()
        .map(|name| name.to_string())
        .collect();
        assert_eq!(main_quest_names(&names), vec!["S0000001", "S0000010"]);
        let sample = sample_side_quests(&names);
        assert_eq!(
            sample,
            vec![
                "A0B00Y00".to_string(),
                "A0C00Y00".to_string(),
                "K0C00Y06".to_string(),
                "_BRISIEN".to_string(),
                "_TUTOR__".to_string(),
            ]
        );
        assert_eq!(sample, sample_side_quests(&names), "same input ⇒ same set");
    }

    #[test]
    fn macro_counts_matches_retail_forms() {
        let counts = macro_counts(
            "My name is _qgiver_. %qdt: met _qgiver_ in ___qgiver_ with _npc2__qgiver_ \
             and =traveltime_ plus %g/%g2 %pct",
        );
        assert_eq!(counts.get("_qgiver_"), Some(&4));
        assert_eq!(counts.get("%qdt"), Some(&1));
        assert_eq!(counts.get("_npc2_"), Some(&1));
        assert_eq!(counts.get("=traveltime_"), Some(&1));
        assert_eq!(counts.get("%g"), Some(&1));
        assert_eq!(counts.get("%g2"), Some(&1));
        assert_eq!(counts.get("%pct"), Some(&1));
        assert_eq!(macro_counts("no macros"), BTreeMap::new());
        let record = QuestRecord::new(
            "X0C00Y00",
            QuestCounts::default(),
            vec![message(1000, "pay %pcf to the guild %pcf")],
        );
        assert_eq!(record.macros().get("%pcf"), Some(&2));
    }
}
