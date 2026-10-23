//! `bio` — character biography: `BIO.DAT` and `BIOGnnT0.TXT` (M20;
//! spec `m20-character-spec.md` §6.1-§6.3).
//!
//! MIT source (`daggerfall-unity` @ 0038a1e6):
//! - `DFU:BioFile.cs:20-53` — `BIO.DAT`: `StreamReader.ReadToEnd()` +
//!   `Split('\0')`, keeping **all** pieces (`Lines`, unfiltered).
//!   Measured retail: 1727 B, 34 pieces, 30 non-empty (§6.1).
//! - `DFU:BiogFile.cs:47-127` (questions at `:104-119`, answers at
//!   `:120-127`), `:570-604` (`Question.lines` = [`QUESTION_TEXT_LINES`]) —
//!   `BIOGnnT0.TXT`: questions (`^\d+\.`) with 2 text lines, answers
//!   (`^[a-h]\.`) and their effect lines. The `#<id>` header on the first
//!   line is the file's backstory id (`:69-89`); retail does not use it
//!   (all 18 files start with question 1), it is tolerated and exposed in
//!   [`BiogQuestion::text_ref`] of the first question.
//! - `DFU:BiogFile.cs:248-445` — grammar of [`parse_effect`]
//!   (`ApplyPlayerEffect`; skills also in `GetSkillEffects`, `:458-488`).
//!
//! **Measured census grammar (spec §6.2).** Question `^\d+\.\t`, answer
//! `^[a-h]\.\t`, effect `^\t…`. With it the census is reproducible:
//! `BIOG00T0` = 12 questions / 69 answers / 35 skill effects; all 18
//! files = 153 / 879 / 691 (`#` 502, `!` 77, `GP` 55, `IT` 420, `r`+`rf`
//! 247). [`BiogFile::question_count`] and [`BiogFile::answer_count`] return
//! that census; [`BiogFile::census`] classifies the effect lines.
//!
//! **Non-homogeneous retail (measured).** Several files (`BIOG01T0`,
//! `BIOG02T0`, …, `BIOG10T0`) use spaces instead of TAB on some of their
//! lines, and `BIOG00T0` carries an `i.` answer outside `a..=h`;
//! `BiogFile.cs:104-127` accepts any separator (`Split('.')`) and any
//! letter. That is why `questions` keeps **all** blocks (216 across the 18
//! files) while the §6.2 census counts only TAB-anchored lines
//! (153 / 879). Non-TAB lines are never classified as effects.
//!
//! **Boundaries (spec §6.3, §7.3-§7.4).** `GP` → M30, `IT` → M21, `r`/`rf` →
//! M27, `#`/`!`/`?` → biography UI (M34), `AE`/`AF`/`AO` not implemented
//! in DFU. `RP`/`RD`/`MR`/`TH`/`FT`/`RR` are stored; their consumption is
//! M22/M23/M27/M32. Here they are only **classified**.
//!
//! Pure: 0 Bevy, 0 I/O (no file reading), it takes `&[u8]`/`&str`;
//! whoever reads ARENA2 is the probe/app.

use crate::character::Skill;

/// Records of the classic `BIO.DAT` layout (M20 lane contract).
///
/// `DFU:BioFile.cs:27-50` does not fix a record size (it reads the whole file
/// and splits on `'\0'`); the constant is exposed for consumers indexing the
/// fixed 100-record layout.
pub const BIO_SEGMENTS: usize = 100;
/// Bytes per record of the classic `BIO.DAT` layout (M20 lane contract).
pub const BIO_SEGMENT_BYTES: usize = 2000;
/// Text lines per question (`BiogFile.Question.lines`, `BiogFile.cs:572`).
pub const QUESTION_TEXT_LINES: usize = 2;

// ── BIO.DAT ─────────────────────────────────────────────────────────────

/// Biography text (`BIO.DAT`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Biography {
    segments: Vec<String>,
    lines: Vec<String>,
}

impl Biography {
    /// Splits the bytes on `'\0'` keeping **all** pieces
    /// (`BioFile.cs:34-43`) and derives the non-empty lines.
    ///
    /// Decoding is lossy UTF-8 (`from_utf8_lossy`), the same `StreamReader
    /// .ReadToEnd()` does in the source for this ASCII file.
    /// Lines are trimmed with [`str::trim`]: retail leaves two `' '` filler
    /// pieces (indices 7 and 21), hence 34 pieces → 30 lines (§6.1).
    pub fn parse(bytes: &[u8]) -> Biography {
        let text = String::from_utf8_lossy(bytes);
        let segments: Vec<String> = text.split('\0').map(str::to_string).collect();
        let lines: Vec<String> = segments
            .iter()
            .map(|segment| segment.trim())
            .filter(|segment| !segment.is_empty())
            .map(str::to_string)
            .collect();
        Biography { segments, lines }
    }

    /// All pieces from the NUL split, empty ones included (retail: 34).
    pub fn raw_segments(&self) -> &[String] {
        &self.segments
    }

    /// Non-empty pieces (retail: 30).
    pub fn lines(&self) -> &[String] {
        &self.lines
    }

    /// First non-empty line — **indicative only**: the first retail line is a
    /// truncated clause (the content is not reproduced in the repo,
    /// INV-L99-6; it is verified by `sha256`).
    pub fn first_line(&self) -> Option<&str> {
        self.lines.first().map(String::as_str)
    }
}

// ── BIOGnnT0.TXT ────────────────────────────────────────────────────────

/// Biography question (`BiogFile.Question`, `BiogFile.cs:570-595`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BiogQuestion {
    /// Number written in the file (`^\d+\.`).
    pub number: u32,
    /// Question text (up to [`QUESTION_TEXT_LINES`] joined lines).
    pub text: String,
    /// Header backstory id (`#<id>`, `BiogFile.cs:69-89`); retail: `None`.
    pub text_ref: Option<u32>,
    /// Answers in file order.
    pub answers: Vec<BiogAnswer>,
}

/// Biography answer (`BiogFile.Answer`, `BiogFile.cs:597-610`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BiogAnswer {
    /// Answer letter (`a`..=`h`, spec §6.2).
    pub letter: char,
    /// Answer text.
    pub text: String,
    /// First text command (`#`/`!`/`?`) with a parseable id in the answer.
    pub text_ref: Option<u32>,
    /// Raw effect lines, without the TAB and without edge whitespace.
    pub effects: Vec<String>,
}

/// Parsed `BIOGnnT0.TXT` file.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BiogFile {
    /// Question blocks in file order (includes those using spaces as
    /// separator: `BiogFile.cs:104-127` accepts any separator).
    pub questions: Vec<BiogQuestion>,
    /// Question lines of the §6.2 grammar (`^\d+\.\t`).
    grammar_questions: usize,
    /// Answer lines of the §6.2 grammar (`^[a-h]\.\t`).
    grammar_answers: usize,
    /// `#`/`!` commands counted for [`EffectCensus::text_ref`] (§6.2).
    text_refs: usize,
}

impl BiogFile {
    /// Parses a `BIOGnnT0.TXT`.
    ///
    /// Tolerant to CRLF, to a `#<id>` header and to the separator after the
    /// number or letter: `BiogFile.cs:104-127` uses `Split('.')`, which accepts
    /// TAB and spaces (retail mixes both: `BIOG01T0`, `BIOG02T0`, …, `BIOG10T0`
    /// use spaces on part of their lines, and `BIOG00T0` carries an `i.`
    /// answer outside `a..=h`). Effect lines are those carrying a TAB
    /// (`^\t…`, §6.2); the rest of the non-TAB lines is discarded as text.
    pub fn parse(source: &str) -> BiogFile {
        let source = source.strip_prefix('\u{feff}').unwrap_or(source);
        let lines: Vec<&str> = source.lines().collect();
        let mut questions: Vec<BiogQuestion> = Vec::new();
        let mut grammar_questions = 0usize;
        let mut grammar_answers = 0usize;
        let mut text_refs = 0usize;
        let mut backstory: Option<u32> = None;

        let mut index = 0;
        while index < lines.len() {
            let line = lines[index];
            let trimmed = line.trim();
            if trimmed.is_empty() {
                index += 1;
                continue;
            }

            if let Some((number, text)) = question_head(line) {
                if is_grammar_question(line) {
                    grammar_questions += 1;
                }
                let mut question = BiogQuestion {
                    number,
                    text: text.to_string(),
                    text_ref: None,
                    answers: Vec::new(),
                };
                index += 1;
                // Text continuation (`Question.lines = 2`).
                let mut text_lines = 1;
                while index < lines.len() && text_lines < QUESTION_TEXT_LINES {
                    let next = lines[index];
                    if !next.starts_with('\t')
                        || next.trim().is_empty()
                        || question_head(next).is_some()
                        || answer_head(next).is_some()
                    {
                        break;
                    }
                    question.text.push(' ');
                    question.text.push_str(next.trim());
                    text_lines += 1;
                    index += 1;
                }
                questions.push(question);
                continue;
            }

            if let Some((letter, text)) = answer_head(line) {
                if is_grammar_answer(line) {
                    grammar_answers += 1;
                }
                if let Some(question) = questions.last_mut() {
                    question.answers.push(BiogAnswer {
                        letter,
                        text: text.to_string(),
                        text_ref: None,
                        effects: Vec::new(),
                    });
                }
                index += 1;
                continue;
            }

            if let Some(rest) = line.strip_prefix('\t') {
                let effect = rest.trim();
                index += 1;
                if effect.is_empty() {
                    continue;
                }
                let Some(question) = questions.last_mut() else {
                    continue;
                };
                if is_text_command(effect) {
                    // `#`/`!` are the declared census; `?` (10 retail lines,
                    // not listed by §6.2) is also a text command.
                    if is_census_text_command(effect) {
                        text_refs += 1;
                    }
                    if let (Some(id), Some(answer)) =
                        (text_command_id(effect), question.answers.last_mut())
                    {
                        if answer.text_ref.is_none() {
                            answer.text_ref = Some(id);
                        }
                    }
                } else if let Some(answer) = question.answers.last_mut() {
                    answer.effects.push(effect.to_string());
                }
                continue;
            }

            // `#<id>` backstory header, only before question 1.
            if questions.is_empty() && backstory.is_none() && is_text_command(trimmed) {
                backstory = text_command_id(trimmed);
            }
            // Any other line (text without TAB, `&`/`0x1A` noise) is
            // discarded: it is never classified as an effect.
            index += 1;
        }

        if let Some(id) = backstory {
            if let Some(first) = questions.first_mut() {
                first.text_ref = Some(id);
            }
        }

        BiogFile {
            questions,
            grammar_questions,
            grammar_answers,
            text_refs,
        }
    }

    /// Census questions of the §6.2 grammar (lines `^\d+\.\t`).
    ///
    /// This is **not** `questions.len()`: `questions` also includes the blocks
    /// separated by spaces (accepted by `BiogFile.cs:104-127`), whereas this
    /// count is what spec §6.2 measures (`BIOG00T0` = 12, all 18
    /// files = 153).
    pub fn question_count(&self) -> usize {
        self.grammar_questions
    }

    /// Census answers of the §6.2 grammar (lines `^[a-h]\.\t`;
    /// `BIOG00T0` = 69, all 18 files = 879).
    pub fn answer_count(&self) -> usize {
        self.grammar_answers
    }

    /// Effect census of the §6.2 grammar.
    pub fn census(&self) -> EffectCensus {
        let mut census = EffectCensus {
            text_ref: self.text_refs,
            ..EffectCensus::default()
        };
        for answer in self.questions.iter().flat_map(|q| q.answers.iter()) {
            for effect in &answer.effects {
                match parse_effect(effect) {
                    BiographyEffect::Skill { .. } => census.skill += 1,
                    BiographyEffect::Gold(_) => census.gold += 1,
                    BiographyEffect::Item { .. } => census.item += 1,
                    BiographyEffect::Reputation { .. } => census.reputation += 1,
                    BiographyEffect::PlayerMod { .. } => census.player_mod += 1,
                    BiographyEffect::TextRef => census.text_ref += 1,
                    BiographyEffect::Unsupported(_) => census.unsupported += 1,
                    BiographyEffect::Unknown(_) => census.unknown += 1,
                }
            }
        }
        census
    }
}

/// Effect census of a `BIOGnnT0.TXT` (spec §6.2).
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct EffectCensus {
    /// `^\t<0..=34> ±N` — skill effects.
    pub skill: usize,
    /// `^\tGP ±N` — gold (M30 boundary).
    pub gold: usize,
    /// `^\tIT <group> <index> <material>` — items (M21 boundary).
    pub item: usize,
    /// `^\tr<N>` / `^\trf<N>` — reputation (M27 boundary).
    pub reputation: usize,
    /// `^\t(RP|RD|MR|TH|FT|RR) ±N` — stored player mods.
    pub player_mod: usize,
    /// `^\t#` + `^\t!` (text deck; `?` is not part of the §6.2 census).
    pub text_ref: usize,
    /// `^\t(AE|AF|AO)` — commands declared unimplemented by DFU.
    pub unsupported: usize,
    /// Unrecognized lines.
    pub unknown: usize,
}

/// Classified biography effect (`ApplyPlayerEffect`, `BiogFile.cs:248-445`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BiographyEffect {
    /// `NN ±d`: skill `NN` (0..=34) with a delta.
    Skill {
        /// Affected skill.
        skill: Skill,
        /// Permanent skill delta.
        delta: i32,
    },
    /// `RP/RD/MR/TH/FT/RR ±N`: stored mod (§6.3).
    PlayerMod {
        /// Player mod.
        kind: PlayerMod,
        /// Assigned value.
        amount: i32,
    },
    /// `GP ±N`: gold (M30 boundary).
    Gold(i32),
    /// `IT g i m`: item (M21 boundary).
    Item {
        /// Item group.
        group: u8,
        /// Index within the group.
        index: u8,
        /// Material.
        material: u8,
    },
    /// `r<N>` (social group) / `rf<N>` (faction) (M27 boundary).
    Reputation {
        /// Social group or faction id.
        faction: u16,
        /// Reputation delta.
        delta: i32,
    },
    /// `#<id>` / `!<id>` / `?<id>`: text command (M34).
    TextRef,
    /// `AE`/`AF`/`AO`: not implemented in DFU (`BiogFile.cs:429-440`).
    Unsupported(String),
    /// Line outside the grammar.
    Unknown(String),
}

/// Stored biography modifiers (`PlayerEntity.cs:172-177`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlayerMod {
    /// `RP` — poison resistance.
    ResistPoison,
    /// `RD` — disease resistance.
    ResistDisease,
    /// `MR` — magic resistance.
    ResistMagic,
    /// `TH` — avoid hit.
    AvoidHit,
    /// `FT` — fatigue.
    Fatigue,
    /// `RR` — reaction.
    Reaction,
}

/// Classifies an effect line, in the same branch order as
/// `ApplyPlayerEffect` (`BiogFile.cs:248-445`): skill → `GP` → `IT` → `r`/`rf`
/// → `RP/RD/MR/TH/FT/RR` → `#`/`!`/`?` → `AE`/`AF`/`AO` → unknown.
///
/// `Gold` carries the **signed value as written** (`GP -250` →
/// `Gold(-250)`). The source applies `-=` to the already-signed value when the
/// token starts with `-` (`BiogFile.cs:295-300`, double negation: a defect of
/// `ApplyPlayerEffect`) and accepts the split form `GP + N`; here it is
/// normalized to the written value, which is all M30 needs (M20 does not
/// apply gold).
///
/// `r<N>` (social group, `SGroupReputations`) and `rf<N>` (faction,
/// `ChangeReputation`) share [`BiographyEffect::Reputation::faction`]
/// because the M20 contract in `character.rs` has a single id; the raw line
/// is preserved in [`BiogAnswer::effects`], so the distinction is not lost.
pub fn parse_effect(line: &str) -> BiographyEffect {
    let tokens: Vec<&str> = line.split_whitespace().collect();
    let Some(&first) = tokens.first() else {
        return BiographyEffect::Unknown(line.to_string());
    };

    // Skill — `int.TryParse(tokens[0])` (`BiogFile.cs:253-267`).
    if let Ok(index) = first.parse::<i32>() {
        return match (
            Skill::from_index(index),
            tokens.get(1).and_then(|t| t.parse::<i32>().ok()),
        ) {
            (Some(skill), Some(delta)) => BiographyEffect::Skill { skill, delta },
            _ => BiographyEffect::Unknown(line.to_string()),
        };
    }

    // Gold — `GP`, with the split form `GP + N` joined (`:269-300`).
    if first.starts_with("GP") {
        let mut value = tokens.get(1).and_then(|t| t.parse::<i32>().ok());
        if value.is_none() {
            if let (Some(sign), Some(digits)) = (tokens.get(1), tokens.get(2)) {
                let mut merged = String::with_capacity(sign.len() + digits.len());
                merged.push_str(sign);
                merged.push_str(digits);
                value = merged.parse().ok();
            }
        }
        return match value {
            Some(value) => BiographyEffect::Gold(value),
            None => BiographyEffect::Unknown(line.to_string()),
        };
    }

    // Item — `IT <group> <index> <material>` (`:302-340`).
    if first.starts_with("IT") {
        let field = |position: usize| tokens.get(position).and_then(|t| t.parse::<u8>().ok());
        return match (field(1), field(2), field(3)) {
            (Some(group), Some(index), Some(material)) => BiographyEffect::Item {
                group,
                index,
                material,
            },
            _ => BiographyEffect::Unknown(line.to_string()),
        };
    }

    // Reputation — `r<N>` (social group) / `rf<N>` (faction) (`:342-372`).
    if let Some(rest) = first.strip_prefix('r') {
        let id = rest.strip_prefix('f').unwrap_or(rest);
        return match (
            id.parse::<u16>(),
            tokens.get(1).and_then(|t| t.parse::<i32>().ok()),
        ) {
            (Ok(faction), Some(delta)) => BiographyEffect::Reputation { faction, delta },
            _ => BiographyEffect::Unknown(line.to_string()),
        };
    }

    // Player mods — `RP/RD/MR/TH/FT/RR` (`:373-428`).
    if let Some(kind) = player_mod(first) {
        return match tokens.get(1).and_then(|t| t.parse::<i32>().ok()) {
            Some(amount) => BiographyEffect::PlayerMod { kind, amount },
            None => BiographyEffect::Unknown(line.to_string()),
        };
    }

    // Text commands (`:418-422`).
    if is_text_command(first) {
        return BiographyEffect::TextRef;
    }

    // Not implemented in DFU (`:424-440`).
    if matches!(first.get(..2), Some("AE" | "AF" | "AO")) {
        return BiographyEffect::Unsupported(line.to_string());
    }

    BiographyEffect::Unknown(line.to_string())
}

/// `RP/RD/MR/TH/FT/RR` → player mod (§6.3).
fn player_mod(command: &str) -> Option<PlayerMod> {
    match command.get(..2)? {
        "RP" => Some(PlayerMod::ResistPoison),
        "RD" => Some(PlayerMod::ResistDisease),
        "MR" => Some(PlayerMod::ResistMagic),
        "TH" => Some(PlayerMod::AvoidHit),
        "FT" => Some(PlayerMod::Fatigue),
        "RR" => Some(PlayerMod::Reaction),
        _ => None,
    }
}

/// `^\d+\.` with any separator (`BiogFile.cs:110-119`) → number and text.
fn question_head(line: &str) -> Option<(u32, &str)> {
    let digits = line.bytes().take_while(|b| b.is_ascii_digit()).count();
    if digits == 0 {
        return None;
    }
    let rest = line[digits..].strip_prefix('.')?;
    Some((line[..digits].parse().ok()?, rest.trim()))
}

/// `^<letter>\.` with any separator (`BiogFile.cs:120-127`;
/// `char.IsLetter` in the source) → answer letter and text.
fn answer_head(line: &str) -> Option<(char, &str)> {
    let mut chars = line.chars();
    let letter = chars.next()?;
    if !letter.is_ascii_alphabetic() {
        return None;
    }
    let rest = chars.as_str().strip_prefix('.')?;
    Some((letter, rest.trim()))
}

/// `^\d+\.\t` — §6.2 census question.
fn is_grammar_question(line: &str) -> bool {
    let digits = line.bytes().take_while(|b| b.is_ascii_digit()).count();
    digits > 0 && line[digits..].starts_with(".\t")
}

/// `^[a-h]\.\t` — §6.2 census answer.
fn is_grammar_answer(line: &str) -> bool {
    matches!(line.chars().next(), Some('a'..='h')) && line[1..].starts_with(".\t")
}

/// Biography text command (`#`/`!`/`?`, `BiogFile.cs:418-422`).
fn is_text_command(line: &str) -> bool {
    matches!(line.as_bytes().first(), Some(b'#' | b'!' | b'?'))
}

/// Declared-census text command (`#`/`!`, §6.2; `?` is excluded).
fn is_census_text_command(line: &str) -> bool {
    matches!(line.as_bytes().first(), Some(b'#' | b'!'))
}

/// Id of the text command, when numeric (`#4150` → 4150).
fn text_command_id(line: &str) -> Option<u32> {
    if !is_text_command(line) {
        return None;
    }
    line[1..].trim().parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_effect_classifies_every_family() {
        assert_eq!(
            parse_effect("22 +6"),
            BiographyEffect::Skill {
                skill: Skill::Destruction,
                delta: 6
            }
        );
        assert_eq!(
            parse_effect("0 -3"),
            BiographyEffect::Skill {
                skill: Skill::Medical,
                delta: -3
            }
        );
        assert_eq!(
            parse_effect("35 +1"),
            BiographyEffect::Unknown("35 +1".to_string())
        );
        assert_eq!(
            parse_effect("-1 +1"),
            BiographyEffect::Unknown("-1 +1".to_string())
        );
        assert_eq!(parse_effect("5"), BiographyEffect::Unknown("5".to_string()));

        assert_eq!(parse_effect("GP +500"), BiographyEffect::Gold(500));
        assert_eq!(parse_effect("GP -250"), BiographyEffect::Gold(-250));
        assert_eq!(parse_effect("GP + 300"), BiographyEffect::Gold(300));
        assert_eq!(
            parse_effect("GP"),
            BiographyEffect::Unknown("GP".to_string())
        );

        assert_eq!(
            parse_effect("IT 3 2 1"),
            BiographyEffect::Item {
                group: 3,
                index: 2,
                material: 1
            }
        );
        assert_eq!(
            parse_effect("IT 3 2"),
            BiographyEffect::Unknown("IT 3 2".to_string())
        );

        assert_eq!(
            parse_effect("r4 +10"),
            BiographyEffect::Reputation {
                faction: 4,
                delta: 10
            }
        );
        assert_eq!(
            parse_effect("rf42 -5"),
            BiographyEffect::Reputation {
                faction: 42,
                delta: -5
            }
        );
        assert_eq!(
            parse_effect("r +10"),
            BiographyEffect::Unknown("r +10".to_string())
        );

        assert_eq!(
            parse_effect("RP +5"),
            BiographyEffect::PlayerMod {
                kind: PlayerMod::ResistPoison,
                amount: 5
            }
        );
        assert_eq!(
            parse_effect("RD -2"),
            BiographyEffect::PlayerMod {
                kind: PlayerMod::ResistDisease,
                amount: -2
            }
        );
        assert_eq!(
            parse_effect("MR +3"),
            BiographyEffect::PlayerMod {
                kind: PlayerMod::ResistMagic,
                amount: 3
            }
        );
        assert_eq!(
            parse_effect("TH -1"),
            BiographyEffect::PlayerMod {
                kind: PlayerMod::AvoidHit,
                amount: -1
            }
        );
        assert_eq!(
            parse_effect("FT -5"),
            BiographyEffect::PlayerMod {
                kind: PlayerMod::Fatigue,
                amount: -5
            }
        );
        assert_eq!(
            parse_effect("RR +4"),
            BiographyEffect::PlayerMod {
                kind: PlayerMod::Reaction,
                amount: 4
            }
        );

        assert_eq!(parse_effect("#4150"), BiographyEffect::TextRef);
        assert_eq!(parse_effect("!4156"), BiographyEffect::TextRef);
        assert_eq!(parse_effect("?4512"), BiographyEffect::TextRef);

        assert_eq!(
            parse_effect("AE M 8 12 +3"),
            BiographyEffect::Unsupported("AE M 8 12 +3".to_string())
        );
        assert_eq!(
            parse_effect("AF"),
            BiographyEffect::Unsupported("AF".to_string())
        );
        assert_eq!(
            parse_effect("AO"),
            BiographyEffect::Unsupported("AO".to_string())
        );

        assert_eq!(
            parse_effect("c.      A book"),
            BiographyEffect::Unknown("c.      A book".to_string())
        );
        assert_eq!(parse_effect(""), BiographyEffect::Unknown(String::new()));
    }

    #[test]
    fn biog_file_parses_crlf_tabs_and_effects() {
        let source = concat!(
            "1.\tWhat school of magic have you been\r\n",
            "\tstudying the longest?\r\n",
            "a.\tDestruction\r\n",
            "\t#4150\r\n",
            "\t!4156\r\n",
            "\t22 +6\r\n",
            "b.\tRestoration\r\n",
            "\tRP +5\r\n",
        );
        let biog = BiogFile::parse(source);
        assert_eq!(biog.question_count(), 1);
        assert_eq!(biog.answer_count(), 2);

        let question = &biog.questions[0];
        assert_eq!(question.number, 1);
        assert_eq!(
            question.text,
            "What school of magic have you been studying the longest?"
        );
        assert_eq!(question.text_ref, None);

        let first = &question.answers[0];
        assert_eq!(first.letter, 'a');
        assert_eq!(first.text, "Destruction");
        assert_eq!(first.text_ref, Some(4150));
        assert_eq!(first.effects, vec!["22 +6".to_string()]);

        let second = &question.answers[1];
        assert_eq!(second.letter, 'b');
        assert_eq!(second.text, "Restoration");
        assert_eq!(second.text_ref, None);
        assert_eq!(second.effects, vec!["RP +5".to_string()]);

        let census = biog.census();
        assert_eq!(census.skill, 1);
        assert_eq!(census.player_mod, 1);
        // `#4150` + `!4156` (both are §6.2 text census).
        assert_eq!(census.text_ref, 2);
        assert_eq!(census.unknown, 0);
        assert_eq!(census.gold + census.item + census.reputation, 0);
        assert_eq!(census.unsupported, 0);
    }

    #[test]
    fn biog_file_tolerates_spaced_blocks_and_header() {
        let source =
            "#4116\r\n1.\tPick one\r\na.      spaced\r\n\tGP +100\r\n\tIT 2 1 0\r\n\tc.  junk\r\n";
        let biog = BiogFile::parse(source);
        // Complete structural block (the source's `Split('.')`)…
        assert_eq!(biog.questions.len(), 1);
        assert_eq!(biog.questions[0].answers.len(), 1);
        assert_eq!(biog.questions[0].answers[0].letter, 'a');
        assert_eq!(biog.questions[0].answers[0].text, "spaced");
        // …but the space-separated answer is not §6.2 census (`^[a-h]\.\t`).
        assert_eq!(biog.question_count(), 1);
        assert_eq!(biog.answer_count(), 0);
        // TAB effects are classified (and junk, as unknown).
        let census = biog.census();
        assert_eq!((census.gold, census.item, census.unknown), (1, 1, 1));
        assert_eq!(biog.questions[0].text_ref, Some(4116));
    }

    #[test]
    fn biography_parse_splits_on_nul() {
        let bio = Biography::parse(b"alpha\0\0beta");
        assert_eq!(bio.raw_segments().len(), 3);
        assert_eq!(bio.raw_segments()[1], "");
        assert_eq!(bio.lines().len(), 2);
        assert_eq!(bio.first_line(), Some("alpha"));
        assert_eq!(Biography::parse(b"").lines().len(), 0);
        assert_eq!(Biography::parse(b"").first_line(), None);
    }

    #[test]
    fn biography_trims_filler_segments() {
        let bio = Biography::parse(b"one\r\n\0 \0two\r\n");
        assert_eq!(bio.raw_segments().len(), 3);
        assert_eq!(bio.lines(), ["one", "two"]);
    }
}
