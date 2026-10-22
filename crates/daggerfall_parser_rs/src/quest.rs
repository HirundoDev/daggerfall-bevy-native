//! `quest` — retail quest container (M16, REQ-L99-049).
//!
//! **Scope (hard boundary):** parser for the `.QBN` **container** (60 B
//! header + 10 sections) and `.QRC` (Text Record DB with the
//! `TEXT.RSC` layout), plus the table of the **82** verbs of the DFU DSL.
//! The semantics of the 87 B of an opcode (section 8) is gap H4 of the M16
//! spec: here opcodes are counted as opaque records, they are never
//! interpreted.
//!
//! **`.QBN` format (measured 247/247, spec M16 §5.2):**
//!
//! ```text
//! 0   u16  QuestId (reserved 0)
//! 2   u16  FactionId (reserved 0)
//! 4   u16  ResourceId (reserved 0)
//! 6   u8[9] ResourceFilename (reserved)
//! 15  u8   HasDebugInfo
//! 16  u16  SectionRecordCount[0..9]
//! 36  u16  SectionOffset[0..10]  (absolute, NOT contiguous by index)
//! 58  u16  Null2
//! 60  ...  sections (19/–/–/20/24/–/33/14/87/8/27 B per record)
//! ```
//!
//! Section 10 (`TextVariables`, 27 B) has no count in the header: its
//! records occupy the rest of the file. It is required that
//! `60 + Σ count[i]·size[i] + n10·27 == filesize` and
//! `offset[10] ≠ 0 ⇔ n10 > 0` (measured 7/7).
//!
//! **`.QRC`:** `[u16 H][(u16 id, u32 offset) × (H/6 - 1)][u16 0xFFFF][u32
//! filesize]` + tokenized text record; identical to `TEXT.RSC`, so it
//! reuses [`crate::text_rsc::TextRsc`]. The `TextRecordId`s are **non
//! decreasing** (17 of the 244 files repeat ids 1008/1009; measured, 0
//! decrements) and the first offset is `≥ H + 2` (1 file, `B0C00Y06`,
//! starts at `H + 3`: 1 measured padding byte).
//!
//! **Custody:** 0 retail bytes in git; the files are read at runtime from the
//! user's ARENA2 (Mode A, INV-L99-6).

use crate::text_rsc::{TextRsc, TextRscError, Token};

/// `.QBN` header.
pub const QBN_HEADER_LEN: usize = 60;
/// Reserved `ResourceFilename` (`u8[9]`).
pub const QBN_RESOURCE_FILENAME_LEN: usize = 9;
/// Sections with a count in the header (`0..=9`).
pub const QBN_SECTION_COUNT: usize = 10;
/// Declared offsets (`0..=10`; 10 is optional).
pub const QBN_SECTION_OFFSETS: usize = 11;
/// Index of the opcode section.
pub const QBN_OPCODE_SECTION: usize = 8;
/// Size of an opcode (opaque record; semantics = H4).
pub const QBN_OPCODE_LEN: usize = 87;
/// Index of the optional `TextVariables` section.
pub const QBN_TEXT_VARIABLE_SECTION: usize = 10;
/// Bytes per record of each section (0 = not implemented by the retail data).
pub const QBN_SECTION_SIZES: [usize; QBN_SECTION_OFFSETS] = [19, 0, 0, 20, 24, 0, 33, 14, 87, 8, 27];
/// Bytes per record of the `TextVariables` section.
pub const QBN_TEXT_VARIABLE_LEN: usize = 27;

/// Quest parse error (0 panic).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QuestError {
    /// File shorter than the minimum header.
    TooShort {
        /// Received bytes.
        len: usize,
        /// Minimum bytes.
        need: usize,
    },
    /// `60 + Σ count·size` does not match the real size of the `.QBN`.
    SizeMismatch {
        /// Size required by the header.
        expected: usize,
        /// Real size.
        actual: usize,
    },
    /// Section 10 is not a multiple of 27 B.
    TextVariablesNotAligned {
        /// Leftover bytes.
        remaining: usize,
        /// Record size.
        record: usize,
    },
    /// `offset[10]` and the records of section 10 contradict each other.
    TextVariablesOffsetMismatch {
        /// Declared offset.
        offset: u16,
        /// Records derived from the rest of the file.
        records: u16,
    },
    /// A section with declared records brings no offset.
    MissingSectionOffset {
        /// Section index.
        index: usize,
    },
    /// A section runs past the end of the file.
    SectionOutOfBounds {
        /// Section index.
        index: usize,
        /// Declared offset.
        offset: usize,
        /// Computed end.
        end: usize,
        /// File bytes.
        len: usize,
    },
    /// Two sections overlap.
    SectionOverlap {
        /// Previous section.
        left: usize,
        /// Following section.
        right: usize,
    },
    /// The first section does not start at byte 60.
    FirstSectionNotAtHeader {
        /// Declared offset.
        offset: usize,
    },
    /// `.QRC` with `H % 6 != 0`.
    QrcHeaderNotMultiple {
        /// Declared `H`.
        header_length: u16,
    },
    /// `.QRC` without records (`H/6 - 1 == 0`).
    QrcNoRecords {
        /// Declared `H`.
        header_length: u16,
    },
    /// The first `.QRC` record starts before `H + 2`.
    QrcFirstRecordBeforeHeader {
        /// Offset of the first record.
        offset: u32,
        /// End of the header (`H + 2`).
        header_end: usize,
    },
    /// Error from the underlying text reader.
    Text(TextRscError),
    /// `RUMOR.DAT`: record that runs past the end of the file.
    RumorTruncated {
        /// Record offset.
        offset: usize,
        /// Required bytes.
        need: usize,
        /// Remaining bytes.
        remaining: usize,
    },
    /// `RUMOR.DAT`: text without a final NUL.
    RumorTextNotTerminated {
        /// Record offset.
        offset: usize,
        /// Declared `TextLength`.
        text_length: u16,
    },
}

impl From<TextRscError> for QuestError {
    fn from(error: TextRscError) -> Self {
        Self::Text(error)
    }
}

impl std::fmt::Display for QuestError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TooShort { len, need } => {
                write!(f, "file too short ({len} < {need} bytes)")
            }
            Self::SizeMismatch { expected, actual } => write!(
                f,
                "size inconsistent with the header: expected {expected} B, actual {actual} B"
            ),
            Self::TextVariablesNotAligned { remaining, record } => write!(
                f,
                "TextVariables section: {remaining} B left over, not a multiple of {record}"
            ),
            Self::TextVariablesOffsetMismatch { offset, records } => write!(
                f,
                "offset[10]={offset} contradicts {records} TextVariables records"
            ),
            Self::MissingSectionOffset { index } => {
                write!(f, "section {index} declares records but offset 0")
            }
            Self::SectionOutOfBounds {
                index,
                offset,
                end,
                len,
            } => write!(
                f,
                "section {index}: [{offset}, {end}) outside the file of {len} B"
            ),
            Self::SectionOverlap { left, right } => {
                write!(f, "sections {left} and {right} overlap")
            }
            Self::FirstSectionNotAtHeader { offset } => {
                write!(f, "the first section starts at {offset}, not at 60")
            }
            Self::QrcHeaderNotMultiple { header_length } => write!(
                f,
                "QRC: H={header_length} is not a multiple of 6 (count = H/6 - 1)"
            ),
            Self::QrcNoRecords { header_length } => {
                write!(f, "QRC: H={header_length} declares no records")
            }
            Self::QrcFirstRecordBeforeHeader {
                offset,
                header_end,
            } => write!(
                f,
                "QRC: first record at {offset}, before the end of the header {header_end}"
            ),
            Self::Text(error) => write!(f, "text: {error}"),
            Self::RumorTruncated {
                offset,
                need,
                remaining,
            } => write!(
                f,
                "RUMOR.DAT: record at {offset} needs {need} B and {remaining} remain"
            ),
            Self::RumorTextNotTerminated {
                offset,
                text_length,
            } => write!(
                f,
                "RUMOR.DAT: record at {offset} with TextLength={text_length} without a final NUL"
            ),
        }
    }
}

impl std::error::Error for QuestError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Text(error) => Some(error),
            _ => None,
        }
    }
}

/// Header + structural validation of a `.QBN` (container).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Qbn {
    /// `QuestId` (reserved 0 in all 247).
    pub quest_id: u16,
    /// `FactionId` (reserved 0).
    pub faction_id: u16,
    /// `ResourceId` (reserved 0).
    pub resource_id: u16,
    /// Reserved `ResourceFilename` (`u8[9]`).
    pub resource_filename: [u8; QBN_RESOURCE_FILENAME_LEN],
    /// `HasDebugInfo` (≠0 in 7 of 247).
    pub has_debug_info: u8,
    /// `SectionRecordCount[0..9]`.
    pub section_counts: [u16; QBN_SECTION_COUNT],
    /// `SectionOffset[0..10]` (absolute).
    pub section_offsets: [u16; QBN_SECTION_OFFSETS],
    /// `Null2` (0 in all 247).
    pub null2: u16,
    /// Records of the `TextVariables` section (derived from the rest).
    pub text_variable_records: u16,
    /// Real size of the file.
    pub file_size: usize,
}

impl Qbn {
    /// Bytes per record of a section (`0` = not implemented).
    pub fn section_record_size(index: usize) -> usize {
        QBN_SECTION_SIZES.get(index).copied().unwrap_or(0)
    }

    /// Parses and validates the container (layout + offsets + file tiling).
    pub fn parse(file: &[u8]) -> Result<Self, QuestError> {
        if file.len() < QBN_HEADER_LEN {
            return Err(QuestError::TooShort {
                len: file.len(),
                need: QBN_HEADER_LEN,
            });
        }
        let read_u16 = |at: usize| u16::from_le_bytes([file[at], file[at + 1]]);
        let quest_id = read_u16(0);
        let faction_id = read_u16(2);
        let resource_id = read_u16(4);
        let mut resource_filename = [0u8; QBN_RESOURCE_FILENAME_LEN];
        resource_filename.copy_from_slice(&file[6..6 + QBN_RESOURCE_FILENAME_LEN]);
        let has_debug_info = file[15];
        let mut section_counts = [0u16; QBN_SECTION_COUNT];
        for (index, count) in section_counts.iter_mut().enumerate() {
            *count = read_u16(16 + index * 2);
        }
        let mut section_offsets = [0u16; QBN_SECTION_OFFSETS];
        for (index, offset) in section_offsets.iter_mut().enumerate() {
            *offset = read_u16(36 + index * 2);
        }
        let null2 = read_u16(58);

        let body: usize = (0..QBN_SECTION_COUNT)
            .map(|index| section_counts[index] as usize * QBN_SECTION_SIZES[index])
            .sum();
        let base = QBN_HEADER_LEN + body;
        if file.len() < base {
            return Err(QuestError::SizeMismatch {
                expected: base,
                actual: file.len(),
            });
        }
        let remaining = file.len() - base;
        if !remaining.is_multiple_of(QBN_TEXT_VARIABLE_LEN) {
            return Err(QuestError::TextVariablesNotAligned {
                remaining,
                record: QBN_TEXT_VARIABLE_LEN,
            });
        }
        let text_variable_records = (remaining / QBN_TEXT_VARIABLE_LEN) as u16;
        if (section_offsets[QBN_TEXT_VARIABLE_SECTION] != 0) != (text_variable_records > 0) {
            return Err(QuestError::TextVariablesOffsetMismatch {
                offset: section_offsets[QBN_TEXT_VARIABLE_SECTION],
                records: text_variable_records,
            });
        }

        let mut ranges: Vec<(usize, usize, usize)> = Vec::new();
        for index in 0..=QBN_TEXT_VARIABLE_SECTION {
            let count = if index == QBN_TEXT_VARIABLE_SECTION {
                text_variable_records as usize
            } else {
                section_counts[index] as usize
            };
            if count == 0 {
                continue;
            }
            let offset = section_offsets[index] as usize;
            if offset == 0 {
                return Err(QuestError::MissingSectionOffset { index });
            }
            let size = QBN_SECTION_SIZES[index];
            let end = offset + count * size;
            if end > file.len() {
                return Err(QuestError::SectionOutOfBounds {
                    index,
                    offset,
                    end,
                    len: file.len(),
                });
            }
            ranges.push((offset, end, index));
        }
        ranges.sort_unstable();
        if let Some((offset, _, _)) = ranges.first() {
            if *offset != QBN_HEADER_LEN {
                return Err(QuestError::FirstSectionNotAtHeader { offset: *offset });
            }
        }
        for pair in ranges.windows(2) {
            if pair[0].1 > pair[1].0 {
                return Err(QuestError::SectionOverlap {
                    left: pair[0].2,
                    right: pair[1].2,
                });
            }
        }

        Ok(Self {
            quest_id,
            faction_id,
            resource_id,
            resource_filename,
            has_debug_info,
            section_counts,
            section_offsets,
            null2,
            text_variable_records,
            file_size: file.len(),
        })
    }

    /// Length in bytes of a section (0 for indexes without records).
    pub fn section_len(&self, index: usize) -> usize {
        let count = if index == QBN_TEXT_VARIABLE_SECTION {
            self.text_variable_records as usize
        } else {
            self.section_counts.get(index).copied().unwrap_or(0) as usize
        };
        count * Qbn::section_record_size(index)
    }

    /// Bytes of a section, if it has records and the file contains them.
    pub fn section<'a>(&self, file: &'a [u8], index: usize) -> Option<&'a [u8]> {
        let len = self.section_len(index);
        if len == 0 {
            return None;
        }
        let start = *self.section_offsets.get(index)? as usize;
        file.get(start..start + len)
    }

    /// Opcode records as 87 B opaque containers (no semantics).
    pub fn opcode_containers<'a>(&self, file: &'a [u8]) -> Vec<&'a [u8]> {
        self.section(file, QBN_OPCODE_SECTION)
            .map(|section| section.chunks_exact(QBN_OPCODE_LEN).collect())
            .unwrap_or_default()
    }

    /// Number of opcodes declared by the header.
    pub fn opcode_count(&self) -> u16 {
        self.section_counts[QBN_OPCODE_SECTION]
    }
}

/// `.QRC`: Text Record DB with the `TEXT.RSC` layout (ids ≥ 1000).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Qrc {
    /// Record table (reuses the `TEXT.RSC` parser).
    pub text: TextRsc,
    /// Real size of the file.
    pub file_size: usize,
    /// Records whose id decreases relative to the previous one (measured: 12
    /// of 244 retail files do not sort their table by id — table order is
    /// iteration order, not id order).
    pub decreasing_ids: usize,
}

impl Qrc {
    /// Parses and validates `H`, offsets, first record and non-decreasing ids.
    pub fn parse(file: &[u8]) -> Result<Self, QuestError> {
        if file.len() < 8 {
            return Err(QuestError::TooShort {
                len: file.len(),
                need: 8,
            });
        }
        let header_length = u16::from_le_bytes([file[0], file[1]]);
        if !(header_length as usize).is_multiple_of(6) {
            return Err(QuestError::QrcHeaderNotMultiple { header_length });
        }
        if header_length / 6 <= 1 {
            return Err(QuestError::QrcNoRecords { header_length });
        }
        let text = TextRsc::parse(file)?;
        let header_end = header_length as usize + 2;
        let mut previous: Option<u16> = None;
        let mut decreasing_ids = 0usize;
        for (index, record) in text.records.iter().enumerate() {
            if index == 0 && (record.offset as usize) < header_end {
                return Err(QuestError::QrcFirstRecordBeforeHeader {
                    offset: record.offset,
                    header_end,
                });
            }
            if let Some(previous) = previous {
                if record.id < previous {
                    decreasing_ids += 1;
                }
            }
            previous = Some(record.id);
        }
        Ok(Self {
            text,
            file_size: file.len(),
            decreasing_ids,
        })
    }

    /// Number of text records.
    pub fn record_count(&self) -> usize {
        self.text.record_count()
    }

    /// Tokens of a message (`id`), route A.
    pub fn record_tokens(&self, file: &[u8], id: u16) -> Result<Vec<Token>, QuestError> {
        Ok(self.text.record_tokens(file, id)?)
    }

    /// Rendered plain text of a message (`id`), route A.
    pub fn record_text(&self, file: &[u8], id: u16) -> Result<String, QuestError> {
        Ok(self.text.record_text(file, id)?)
    }
}

/// DSL template kind: trigger (condition) or action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VerbKind {
    /// Trigger condition (`RegisterAction` block 1).
    Trigger,
    /// Executable action (`RegisterAction` block 2).
    Action,
}

/// Row of the verb table (`Pattern` of `Game/Questing/Actions/*.cs`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QuestVerb {
    /// First semantic literal of the `Pattern`.
    pub verb: &'static str,
    /// Template class (`Actions/<class>.cs`).
    pub class: &'static str,
    /// Trigger or action.
    pub kind: VerbKind,
    /// `RegisterAction` line in `QuestMachine.cs`.
    pub register_line: u16,
}

/// The **82** active verbs of `QuestMachine.RegisterActionTemplates`
/// (`DFU:Game/Questing/QuestMachine.cs:345-428`; 17 triggers + 65 actions).
pub const QUEST_VERBS: [QuestVerb; 82] = [
    // ── Triggers (17) — QuestMachine.cs:345-361 ─────────────────────────
    QuestVerb { verb: "when pc enters/exits", class: "WhenPcEntersExits", kind: VerbKind::Trigger, register_line: 345 },
    QuestVerb { verb: "when <npc> is available", class: "WhenNpcIsAvailable", kind: VerbKind::Trigger, register_line: 346 },
    QuestVerb { verb: "when repute with", class: "WhenReputeWith", kind: VerbKind::Trigger, register_line: 347 },
    QuestVerb { verb: "when skill", class: "WhenSkillLevel", kind: VerbKind::Trigger, register_line: 348 },
    QuestVerb { verb: "when attribute", class: "WhenAttributeLevel", kind: VerbKind::Trigger, register_line: 349 },
    QuestVerb { verb: "when [not] <task>", class: "WhenTask", kind: VerbKind::Trigger, register_line: 350 },
    QuestVerb { verb: "clicked <npc>", class: "ClickedNpc", kind: VerbKind::Trigger, register_line: 351 },
    QuestVerb { verb: "clicked item", class: "ClickedItem", kind: VerbKind::Trigger, register_line: 352 },
    QuestVerb { verb: "level ... completed", class: "LevelCompleted", kind: VerbKind::Trigger, register_line: 353 },
    QuestVerb { verb: "injured", class: "InjuredFoe", kind: VerbKind::Trigger, register_line: 354 },
    QuestVerb { verb: "killed <n>", class: "KilledFoe", kind: VerbKind::Trigger, register_line: 355 },
    QuestVerb { verb: "toting ... clicked", class: "TotingItemAndClickedNpc", kind: VerbKind::Trigger, register_line: 356 },
    QuestVerb { verb: "daily from", class: "DailyFrom", kind: VerbKind::Trigger, register_line: 357 },
    QuestVerb { verb: "dropped ... at", class: "DroppedItemAtPlace", kind: VerbKind::Trigger, register_line: 358 },
    QuestVerb { verb: "season", class: "Season", kind: VerbKind::Trigger, register_line: 359 },
    QuestVerb { verb: "weather", class: "Weather", kind: VerbKind::Trigger, register_line: 360 },
    QuestVerb { verb: "climate", class: "Climate", kind: VerbKind::Trigger, register_line: 361 },
    // ── Actions (65) — QuestMachine.cs:364-428 ──────────────────────────
    QuestVerb { verb: "end quest", class: "EndQuest", kind: VerbKind::Action, register_line: 364 },
    QuestVerb { verb: "prompt", class: "Prompt", kind: VerbKind::Action, register_line: 365 },
    QuestVerb { verb: "say", class: "Say", kind: VerbKind::Action, register_line: 366 },
    QuestVerb { verb: "play sound", class: "PlaySound", kind: VerbKind::Action, register_line: 367 },
    QuestVerb { verb: "start task / setvar", class: "StartTask", kind: VerbKind::Action, register_line: 368 },
    QuestVerb { verb: "clear", class: "ClearTask", kind: VerbKind::Action, register_line: 369 },
    QuestVerb { verb: "log", class: "LogMessage", kind: VerbKind::Action, register_line: 370 },
    QuestVerb { verb: "pick one of", class: "PickOneOf", kind: VerbKind::Action, register_line: 371 },
    QuestVerb { verb: "remove log step", class: "RemoveLogMessage", kind: VerbKind::Action, register_line: 372 },
    QuestVerb { verb: "play video", class: "PlayVideo", kind: VerbKind::Action, register_line: 373 },
    QuestVerb { verb: "pc at ... set/do", class: "PcAt", kind: VerbKind::Action, register_line: 374 },
    QuestVerb { verb: "create npc at", class: "CreateNpcAt", kind: VerbKind::Action, register_line: 375 },
    QuestVerb { verb: "create npc", class: "CreateNpc", kind: VerbKind::Action, register_line: 376 },
    QuestVerb { verb: "place npc", class: "PlaceNpc", kind: VerbKind::Action, register_line: 377 },
    QuestVerb { verb: "place item", class: "PlaceItem", kind: VerbKind::Action, register_line: 378 },
    QuestVerb { verb: "give pc", class: "GivePc", kind: VerbKind::Action, register_line: 379 },
    QuestVerb { verb: "give item ... to", class: "GiveItem", kind: VerbKind::Action, register_line: 380 },
    QuestVerb { verb: "start/stop timer", class: "StartStopTimer", kind: VerbKind::Action, register_line: 381 },
    QuestVerb { verb: "create foe", class: "CreateFoe", kind: VerbKind::Action, register_line: 382 },
    QuestVerb { verb: "place foe", class: "PlaceFoe", kind: VerbKind::Action, register_line: 383 },
    QuestVerb { verb: "hide [npc]", class: "HideNpc", kind: VerbKind::Action, register_line: 384 },
    QuestVerb { verb: "restore [npc]", class: "RestoreNpc", kind: VerbKind::Action, register_line: 385 },
    QuestVerb { verb: "add ... face", class: "AddFace", kind: VerbKind::Action, register_line: 386 },
    QuestVerb { verb: "drop ... face", class: "DropFace", kind: VerbKind::Action, register_line: 387 },
    QuestVerb { verb: "get item", class: "GetItem", kind: VerbKind::Action, register_line: 388 },
    QuestVerb { verb: "start quest", class: "StartQuest", kind: VerbKind::Action, register_line: 389 },
    QuestVerb { verb: "run quest", class: "RunQuest", kind: VerbKind::Action, register_line: 390 },
    QuestVerb { verb: "unset", class: "UnsetTask", kind: VerbKind::Action, register_line: 391 },
    QuestVerb { verb: "change repute with", class: "ChangeReputeWith", kind: VerbKind::Action, register_line: 392 },
    QuestVerb { verb: "repute with ... exceeds", class: "ReputeExceedsDo", kind: VerbKind::Action, register_line: 393 },
    QuestVerb { verb: "reveal", class: "RevealLocation", kind: VerbKind::Action, register_line: 394 },
    QuestVerb { verb: "restrain foe", class: "RestrainFoe", kind: VerbKind::Action, register_line: 395 },
    QuestVerb { verb: "make ... permanent", class: "MakePermanent", kind: VerbKind::Action, register_line: 396 },
    QuestVerb { verb: "have <item> set <task>", class: "HaveItem", kind: VerbKind::Action, register_line: 397 },
    QuestVerb { verb: "add ... as questor", class: "AddAsQuestor", kind: VerbKind::Action, register_line: 398 },
    QuestVerb { verb: "drop ... as questor", class: "DropAsQuestor", kind: VerbKind::Action, register_line: 399 },
    QuestVerb { verb: "<item> used do", class: "ItemUsedDo", kind: VerbKind::Action, register_line: 400 },
    QuestVerb { verb: "take ... from pc", class: "TakeItem", kind: VerbKind::Action, register_line: 401 },
    QuestVerb { verb: "teleport pc to / transfer pc inside", class: "TeleportPc", kind: VerbKind::Action, register_line: 402 },
    QuestVerb { verb: "dialog link", class: "DialogLink", kind: VerbKind::Action, register_line: 403 },
    QuestVerb { verb: "add dialog", class: "AddDialog", kind: VerbKind::Action, register_line: 404 },
    QuestVerb { verb: "rumor mill", class: "RumorMill", kind: VerbKind::Action, register_line: 405 },
    QuestVerb { verb: "make pc ill with", class: "MakePcDiseased", kind: VerbKind::Action, register_line: 406 },
    QuestVerb { verb: "cure ...", class: "CurePcDisease", kind: VerbKind::Action, register_line: 407 },
    QuestVerb { verb: "cast ... spell do", class: "CastSpellDo", kind: VerbKind::Action, register_line: 408 },
    QuestVerb { verb: "cast ... effect do", class: "CastEffectDo", kind: VerbKind::Action, register_line: 409 },
    QuestVerb { verb: "cast ... spell on", class: "CastSpellOnFoe", kind: VerbKind::Action, register_line: 410 },
    QuestVerb { verb: "remove foe", class: "RemoveFoe", kind: VerbKind::Action, register_line: 411 },
    QuestVerb { verb: "legal repute", class: "LegalRepute", kind: VerbKind::Action, register_line: 412 },
    QuestVerb { verb: "mute npc", class: "MuteNpc", kind: VerbKind::Action, register_line: 413 },
    QuestVerb { verb: "destroy npc", class: "DestroyNpc", kind: VerbKind::Action, register_line: 414 },
    QuestVerb { verb: "worldupdate", class: "WorldUpdate", kind: VerbKind::Action, register_line: 415 },
    QuestVerb { verb: "enemies", class: "Enemies", kind: VerbKind::Action, register_line: 416 },
    QuestVerb { verb: "clicked foe", class: "ClickedFoe", kind: VerbKind::Action, register_line: 417 },
    QuestVerb { verb: "kill foe", class: "KillFoe", kind: VerbKind::Action, register_line: 418 },
    QuestVerb { verb: "pay ... do", class: "PayMoney", kind: VerbKind::Action, register_line: 419 },
    QuestVerb { verb: "journal note", class: "JournalNote", kind: VerbKind::Action, register_line: 420 },
    QuestVerb { verb: "change foe ... infighting", class: "ChangeFoeInfighting", kind: VerbKind::Action, register_line: 421 },
    QuestVerb { verb: "change foe ... team", class: "ChangeFoeTeam", kind: VerbKind::Action, register_line: 422 },
    QuestVerb { verb: "play song", class: "PlaySong", kind: VerbKind::Action, register_line: 423 },
    QuestVerb { verb: "setplayercrime", class: "SetPlayerCrime", kind: VerbKind::Action, register_line: 424 },
    QuestVerb { verb: "spawncityguards", class: "SpawnCityGuards", kind: VerbKind::Action, register_line: 425 },
    QuestVerb { verb: "unrestrain foe", class: "UnrestrainFoe", kind: VerbKind::Action, register_line: 426 },
    QuestVerb { verb: "train pc", class: "TrainPc", kind: VerbKind::Action, register_line: 427 },
    QuestVerb { verb: "promptmulti", class: "PromptMulti", kind: VerbKind::Action, register_line: 428 },
];

/// Number of triggers in the table (17).
pub const QUEST_TRIGGER_COUNT: usize = 17;
/// Number of actions in the table (65).
pub const QUEST_ACTION_COUNT: usize = 65;

/// Header of a `RUMOR.DAT` rumor (spec M16 §6, measured 31/31).
pub const RUMOR_HEADER_LEN: usize = 34;
/// Reserved `QuestName` (`char[16]`, empty in all 31).
pub const RUMOR_QUEST_NAME_LEN: usize = 16;

/// Rumor of `RUMOR.DAT`: 34 B header + NUL-terminated text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rumor {
    /// `Faction1` (u16 @0; medido 199…591).
    pub faction1: u16,
    /// `Faction2` (u16 @2; medido 0…847).
    pub faction2: u16,
    /// `Type` (u8 @4; 27×12, 26×10, 12×5, 7×3, 4×1).
    pub kind: u8,
    /// Reserved (`00 00 00` in 31/31).
    pub reserved: [u8; 3],
    /// `RegionID` (0…61).
    pub region: u8,
    /// `Flags` (1 ×26, 8 ×5; the 8 are exactly `Type=12`).
    pub flags: u8,
    /// Reserved `QuestName`.
    pub quest_name: [u8; RUMOR_QUEST_NAME_LEN],
    /// `TextLength` (includes the final NUL).
    pub text_length: u16,
    /// `Unknown` (0 in 31/31).
    pub unknown: u16,
    /// `TimeLimit` (566670 in 31/31).
    pub time_limit: u32,
}

impl Rumor {
    /// Daggerfall Unity's `NPCID` (`RumorFile.cs:137-160`): u32 @22, which
    /// overlaps bytes 22..26 of `QuestName` (10..26). Measured empty in 31/31,
    /// so it is 0 across the whole retail data; it is read to honour the
    /// import rule (`NPCID != 0` ⇒ not imported into the rumor mill).
    pub fn npc_id(&self) -> u32 {
        u32::from_le_bytes([
            self.quest_name[12],
            self.quest_name[13],
            self.quest_name[14],
            self.quest_name[15],
        ])
    }

    /// Text lines (`0xFD` = line break), without the final NUL.
    pub fn lines(text: &[u8]) -> Vec<String> {
        let body = text.strip_suffix(&[0]).unwrap_or(text);
        body.split(|&byte| byte == 0xFD)
            .map(|line| String::from_utf8_lossy(line).into_owned())
            .collect()
    }
}

/// Complete `RUMOR.DAT` (31 records in the retail data; 3506 B).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RumorFile {
    /// Records in file order.
    pub rumors: Vec<Rumor>,
    /// Raw texts (including the final NUL), parallel to `rumors`.
    pub texts: Vec<Vec<u8>>,
}

impl RumorFile {
    /// Parses and validates the measured layout (`34 + TextLength` per record).
    pub fn parse(file: &[u8]) -> Result<Self, QuestError> {
        let mut rumors = Vec::new();
        let mut texts = Vec::new();
        let mut offset = 0;
        while offset < file.len() {
            let remaining = file.len() - offset;
            if remaining < RUMOR_HEADER_LEN {
                return Err(QuestError::RumorTruncated {
                    offset,
                    need: RUMOR_HEADER_LEN,
                    remaining,
                });
            }
            let read_u16 = |at: usize| u16::from_le_bytes([file[offset + at], file[offset + at + 1]]);
            let faction1 = read_u16(0);
            let faction2 = read_u16(2);
            let kind = file[offset + 4];
            let mut reserved = [0u8; 3];
            reserved.copy_from_slice(&file[offset + 5..offset + 8]);
            let region = file[offset + 8];
            let flags = file[offset + 9];
            let mut quest_name = [0u8; RUMOR_QUEST_NAME_LEN];
            quest_name.copy_from_slice(&file[offset + 10..offset + 26]);
            let text_length = read_u16(26);
            let unknown = read_u16(28);
            let time_limit = u32::from_le_bytes([
                file[offset + 30],
                file[offset + 31],
                file[offset + 32],
                file[offset + 33],
            ]);
            let end = offset + RUMOR_HEADER_LEN + text_length as usize;
            if end > file.len() {
                return Err(QuestError::RumorTruncated {
                    offset,
                    need: RUMOR_HEADER_LEN + text_length as usize,
                    remaining,
                });
            }
            let text = &file[offset + RUMOR_HEADER_LEN..end];
            if text_length == 0 || text.last() != Some(&0) {
                return Err(QuestError::RumorTextNotTerminated {
                    offset,
                    text_length,
                });
            }
            rumors.push(Rumor {
                faction1,
                faction2,
                kind,
                reserved,
                region,
                flags,
                quest_name,
                text_length,
                unknown,
                time_limit,
            });
            texts.push(text.to_vec());
            offset = end;
        }
        Ok(Self { rumors, texts })
    }

    /// Resolved text of record `index` (lines joined by `\n`).
    pub fn text(&self, index: usize) -> Option<String> {
        self.texts.get(index).map(|raw| Rumor::lines(raw).join("\n"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SIZES: [usize; QBN_SECTION_OFFSETS] = QBN_SECTION_SIZES;

    /// Builds a synthetic `.QBN` with sections in index order.
    fn synthetic_qbn(counts: [u16; QBN_SECTION_COUNT], text_variables: u16) -> Vec<u8> {
        let mut file = vec![0u8; QBN_HEADER_LEN];
        for (index, count) in counts.iter().enumerate() {
            file[16 + index * 2..18 + index * 2].copy_from_slice(&count.to_le_bytes());
        }
        let mut cursor = QBN_HEADER_LEN as u16;
        for (index, count) in counts.iter().enumerate() {
            if *count == 0 {
                continue;
            }
            file[36 + index * 2..38 + index * 2].copy_from_slice(&cursor.to_le_bytes());
            cursor += u16::try_from(*count as usize * SIZES[index]).unwrap();
        }
        if text_variables > 0 {
            file[36 + 10 * 2..38 + 10 * 2].copy_from_slice(&cursor.to_le_bytes());
        }
        file.resize(
            QBN_HEADER_LEN
                + counts
                    .iter()
                    .enumerate()
                    .map(|(index, count)| *count as usize * SIZES[index])
                    .sum::<usize>()
                + text_variables as usize * QBN_TEXT_VARIABLE_LEN,
            0,
        );
        file
    }

    #[test]
    fn parses_valid_qbn_container() {
        let mut counts = [0u16; QBN_SECTION_COUNT];
        counts[0] = 2;
        counts[3] = 3;
        counts[8] = 5;
        counts[9] = 4;
        let file = synthetic_qbn(counts, 0);
        let qbn = Qbn::parse(&file).unwrap();
        assert_eq!(qbn.section_counts[8], 5);
        assert_eq!(qbn.opcode_count(), 5);
        assert_eq!(qbn.opcode_containers(&file).len(), 5);
        assert_eq!(qbn.opcode_containers(&file)[0].len(), QBN_OPCODE_LEN);
        assert_eq!(qbn.section_len(3), 3 * 20);
        assert_eq!(qbn.file_size, file.len());
        assert_eq!(qbn.text_variable_records, 0);
        assert!(qbn.section(&file, 1).is_none());
    }

    #[test]
    fn parses_optional_text_variables_section() {
        let mut counts = [0u16; QBN_SECTION_COUNT];
        counts[8] = 1;
        let file = synthetic_qbn(counts, 7);
        let qbn = Qbn::parse(&file).unwrap();
        assert_eq!(qbn.text_variable_records, 7);
        assert_eq!(qbn.section_len(QBN_TEXT_VARIABLE_SECTION), 7 * 27);
        assert!(qbn.section(&file, QBN_TEXT_VARIABLE_SECTION).is_some());
    }

    #[test]
    fn rejects_truncated_and_inconsistent_qbn() {
        assert!(matches!(
            Qbn::parse(&[0u8; 59]),
            Err(QuestError::TooShort { .. })
        ));
        let mut counts = [0u16; QBN_SECTION_COUNT];
        counts[0] = 1;
        let mut file = synthetic_qbn(counts, 0);
        file.pop();
        assert!(matches!(
            Qbn::parse(&file),
            Err(QuestError::SizeMismatch { .. })
        ));
        let mut file = synthetic_qbn(counts, 0);
        file.push(1);
        assert!(matches!(
            Qbn::parse(&file),
            Err(QuestError::TextVariablesNotAligned { .. })
        ));
    }

    #[test]
    fn rejects_section_without_offset_and_out_of_bounds() {
        let mut counts = [0u16; QBN_SECTION_COUNT];
        counts[3] = 1;
        let mut file = synthetic_qbn(counts, 0);
        file[36 + 3 * 2..38 + 3 * 2].copy_from_slice(&0u16.to_le_bytes());
        assert_eq!(
            Qbn::parse(&file),
            Err(QuestError::MissingSectionOffset { index: 3 })
        );

        let mut file = synthetic_qbn(counts, 0);
        let file_len = file.len() as u16;
        file[36 + 3 * 2..38 + 3 * 2].copy_from_slice(&file_len.to_le_bytes());
        assert!(matches!(
            Qbn::parse(&file),
            Err(QuestError::SectionOutOfBounds { index: 3, .. })
        ));
    }

    #[test]
    fn rejects_overlapping_sections() {
        let mut counts = [0u16; QBN_SECTION_COUNT];
        counts[0] = 1;
        counts[3] = 1;
        let mut file = synthetic_qbn(counts, 0);
        // Moves Npcs on top of Items.
        file[36 + 3 * 2..38 + 3 * 2].copy_from_slice(&60u16.to_le_bytes());
        assert!(matches!(
            Qbn::parse(&file),
            Err(QuestError::SectionOverlap {
                left: 0,
                right: 3
            })
        ));
    }

    /// Synthetic `.QRC` with the `TEXT.RSC` layout (1 B per record).
    fn synthetic_qrc(ids: &[u16]) -> Vec<u8> {
        let count = ids.len();
        let header_length = ((count + 1) * 6) as u16;
        let table_end = 2 + count * 6;
        let data_start = table_end + 6;
        let file_size = data_start + count;
        let mut file = Vec::new();
        file.extend_from_slice(&header_length.to_le_bytes());
        for (index, id) in ids.iter().enumerate() {
            file.extend_from_slice(&id.to_le_bytes());
            file.extend_from_slice(&((data_start + index) as u32).to_le_bytes());
        }
        file.extend_from_slice(&0xFFFFu16.to_le_bytes());
        file.extend_from_slice(&(file_size as u32).to_le_bytes());
        file.resize(file_size, 0xFE);
        file
    }

    #[test]
    fn parses_qrc_with_monotonic_ids_and_duplicates() {
        let file = synthetic_qrc(&[1000, 1001, 1001]);
        let qrc = Qrc::parse(&file).unwrap();
        assert_eq!(qrc.record_count(), 3);
        assert_eq!(qrc.file_size, file.len());
        assert_eq!(qrc.record_tokens(&file, 1000).unwrap().len(), 0);
    }

    #[test]
    fn counts_decreasing_qrc_ids_and_rejects_bad_header() {
        let file = synthetic_qrc(&[1001, 1000]);
        let qrc = Qrc::parse(&file).unwrap();
        assert_eq!(qrc.record_count(), 2);
        assert_eq!(qrc.decreasing_ids, 1);
        let mut bad = synthetic_qrc(&[1000]);
        bad[0] = 7;
        assert_eq!(
            Qrc::parse(&bad),
            Err(QuestError::QrcHeaderNotMultiple { header_length: 7 })
        );
        let mut bad = synthetic_qrc(&[1000]);
        bad[0..2].copy_from_slice(&6u16.to_le_bytes());
        assert_eq!(
            Qrc::parse(&bad),
            Err(QuestError::QrcNoRecords { header_length: 6 })
        );
    }

    #[test]
    fn qrc_text_uses_the_shared_token_parser() {
        // Record with a justifier and a subrecord separator.
        let mut file = Vec::new();
        let header_length = 12u16;
        let data_start = 14usize;
        file.extend_from_slice(&header_length.to_le_bytes());
        file.extend_from_slice(&1000u16.to_le_bytes());
        file.extend_from_slice(&(data_start as u32).to_le_bytes());
        file.extend_from_slice(&0xFFFFu16.to_le_bytes());
        file.extend_from_slice(&((data_start + 12) as u32).to_le_bytes());
        file.extend_from_slice(b"text\xfd\xffvalue\xfe");
        let qrc = Qrc::parse(&file).unwrap();
        assert_eq!(qrc.record_text(&file, 1000).unwrap(), "text\nvalue");
        assert_eq!(qrc.text.record_variants(&file, 1000).unwrap().len(), 2);
    }

    #[test]
    fn verb_table_has_82_entries() {
        assert_eq!(QUEST_VERBS.len(), 82);
        let triggers = QUEST_VERBS
            .iter()
            .filter(|verb| verb.kind == VerbKind::Trigger)
            .count();
        let actions = QUEST_VERBS
            .iter()
            .filter(|verb| verb.kind == VerbKind::Action)
            .count();
        assert_eq!(triggers, QUEST_TRIGGER_COUNT);
        assert_eq!(actions, QUEST_ACTION_COUNT);
        assert_eq!(triggers + actions, 82);
        // Register lines: 345-361 (triggers) and 364-428 (actions).
        assert_eq!(QUEST_VERBS[0].register_line, 345);
        assert_eq!(QUEST_VERBS[16].register_line, 361);
        assert_eq!(QUEST_VERBS[17].register_line, 364);
        assert_eq!(QUEST_VERBS[81].register_line, 428);
    }

    /// The table is derived from the pinned corpus: same order and same
    /// classes as `QuestMachine.cs:345-428`, and every verb appears in its `Pattern`.
    #[test]
    fn verb_table_matches_pinned_corpus() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../reference/dfu-spec");
        let machine = root.join("Assets/Scripts/Game/Questing/QuestMachine.cs");
        let source = std::fs::read_to_string(&machine).expect("pinned DFU corpus");
        let mut registered: Vec<String> = Vec::new();
        for line in source.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("//") {
                continue;
            }
            if let Some(rest) = trimmed.strip_prefix("RegisterAction(new ") {
                let class = rest
                    .split("(null)")
                    .next()
                    .unwrap_or_default()
                    .trim_start_matches("Actions.");
                registered.push(class.to_string());
            }
        }
        assert_eq!(registered.len(), 82, "active RegisterAction() entries");
        let table: Vec<&str> = QUEST_VERBS.iter().map(|verb| verb.class).collect();
        assert_eq!(registered, table);

        for verb in QUEST_VERBS.iter() {
            let path = root.join(format!("Assets/Scripts/Game/Questing/Actions/{}.cs", verb.class));
            let action = std::fs::read_to_string(&path)
                .unwrap_or_else(|_| panic!("missing {}", path.display()));
            assert!(
                action.contains("override string Pattern"),
                "{} without Pattern",
                verb.class
            );
            let lower = action.to_lowercase();
            for word in verb
                .verb
                .split(|ch: char| !ch.is_ascii_alphanumeric())
                .filter(|word| word.len() > 1)
            {
                assert!(
                    lower.contains(&word.to_lowercase()),
                    "verb `{}` does not appear in {}",
                    word,
                    verb.class
                );
            }
        }
    }

    fn synthetic_rumor(kind: u8, flags: u8, text: &[u8]) -> Vec<u8> {
        let mut file = Vec::new();
        file.extend_from_slice(&508u16.to_le_bytes());
        file.extend_from_slice(&810u16.to_le_bytes());
        file.push(kind);
        file.extend_from_slice(&[0, 0, 0]);
        file.push(0);
        file.push(flags);
        file.extend_from_slice(&[0u8; RUMOR_QUEST_NAME_LEN]);
        let mut body = text.to_vec();
        body.push(0);
        file.extend_from_slice(&(body.len() as u16).to_le_bytes());
        file.extend_from_slice(&0u16.to_le_bytes());
        file.extend_from_slice(&566_670u32.to_le_bytes());
        file.extend_from_slice(&body);
        file
    }

    #[test]
    fn parses_rumor_file_and_splits_lines() {
        let mut file = synthetic_rumor(27, 1, b"opening\xfdclosing");
        file.extend_from_slice(&synthetic_rumor(12, 8, b"content"));
        let rumors = RumorFile::parse(&file).unwrap();
        assert_eq!(rumors.rumors.len(), 2);
        assert_eq!(rumors.rumors[0].kind, 27);
        assert_eq!(rumors.rumors[1].flags, 8);
        assert_eq!(rumors.rumors[0].time_limit, 566_670);
        assert_eq!(rumors.rumors[0].text_length, 16);
        assert_eq!(rumors.text(0).unwrap(), "opening\nclosing");
        assert_eq!(rumors.text(1).unwrap(), "content");
        assert!(rumors.text(2).is_none());
    }

    #[test]
    fn rejects_broken_rumor_records() {
        let file = synthetic_rumor(27, 1, b"words");
        assert!(matches!(
            RumorFile::parse(&file[..RUMOR_HEADER_LEN - 1]),
            Err(QuestError::RumorTruncated { .. })
        ));
        let mut truncated = file.clone();
        truncated.truncate(truncated.len() - 1);
        assert!(matches!(
            RumorFile::parse(&truncated),
            Err(QuestError::RumorTruncated { .. })
        ));
        let mut unterminated = file;
        let last = unterminated.len() - 1;
        unterminated[last] = b'x';
        assert_eq!(
            RumorFile::parse(&unterminated),
            Err(QuestError::RumorTextNotTerminated {
                offset: 0,
                text_length: 6
            })
        );
    }
}
