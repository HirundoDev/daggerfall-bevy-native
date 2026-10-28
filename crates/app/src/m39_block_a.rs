//! M39 block A — the 16 `Playable` windows of plan
//! `m39-ui3-plan-CR-7bf61244-e4e4-4b93-930f-149c60181da0.md` §1/§2 that belong
//! to the menu / settings / saves / messages family.
//!
//! ## Measured provenance (never invented)
//!
//! | Read model | Retail datum | Source |
//! |:---|:---|:---|
//! | `m39.settings.scale` | `maxSpeed` of `Z.CFG` (install root) | `Z.CFG`, retail install |
//! | `m39.settings.language` | name of the shipped text bank (`TEXT.RSC`) | `ARENA2/TEXT.RSC` |
//! | `m39.settings.applied` | the wizard's apply step | plan §5 (`m39.settings.applied`) |
//! | `m39.controls.binding` | `controls` of `Z.CFG` (029) and the second shipped control record (067) | `Z.CFG`, `ARENA2/VIEWPLYR.DAT` |
//! | `m39.controls.conflicts` | duplicate key codes inside the zero-free tail of the shipped control records | `ARENA2/BETAPLYR.DAT`, `ARENA2/VIEWPLYR.DAT` |
//! | `m39.input.prompt` | `TEXT.RSC` record 255 (`"From where dost thou hail?"`) | `ARENA2/TEXT.RSC` |
//! | `m39.input.max_len` | `MAPSAVE_NAME_BYTES` (12) | `daggerfall_memory::savegame` |
//! | `m39.input.value` | label of a decodable classic slot | `ARENA2/SAVE<n>/SAVETREE.DAT` |
//! | `m39.menu.items` | declared wiring of the panel's button rows (`MENU_TARGETS`) | plan §1 census windows |
//! | `m39.menu.selected(.name)` | cursor of the list picker over that wiring | `MENU_TARGETS` |
//! | `m39.menu.depth` | depth of the window stack while the start window is open | `AState::stack` |
//! | `m39.saves.*` | `SAVE0..SAVE5` decoded with `SaveTree::decode` | `daggerfall_memory::savegame` |
//! | `m39.messages.*` | `TEXT.RSC` record 1069 (message box) and `BIOG00T0.TXT` questions (demo class questionnaire) | `ARENA2/TEXT.RSC`, `ARENA2/BIOG00T0.TXT` |
//! | `m39.video.frames` | frame count of `ANIM0001.VID` (058) / `AZURA.FLC` (074) | `daggerfall_parser_rs::video` |
//!
//! ## Read-only surfaces (plan §3.4)
//!
//! 071 `DaggerfallVidPlayerWindow` and 074 `DemoFLCWindow` have no IMG/CIF/RCI/FNT
//! raster of their own (m34-window-assets TSV): their flows deliver
//! `ModalOutcome::ReadOnly` with the citation and never commit a read model.
//!
//! ## Navigation
//!
//! Every flow that opens a child window cancels it again and every flow that
//! pushes itself closes again, so `AState::stack` returns to its entry depth
//! (plan §4.1 test (c)).

use std::path::{Path, PathBuf};

use daggerfall_memory::savegame::{SaveTree, MAPSAVE_NAME_BYTES};
use daggerfall_nervous::events::WindowId;
use daggerfall_parser_bevy_bridge::ui_compat::UiCompatError;
use daggerfall_parser_bevy_bridge::ui_spec::{self, UiAssets};
use daggerfall_parser_rs::text_rsc::TextRsc;
use daggerfall_parser_rs::video;
use daggerfall_perception::read_models::M39_ABSENT;
use daggerfall_perception::ui_shell::canonical_id_for;
use daggerfall_skills::bio::BiogFile;

use crate::m39_state::{fnv1a64_hex, BlockEvidence, Interaction, M39State, ModalOutcome};

/// Block letter of plan §2.
pub const BLOCK: char = 'A';

/// The 16 windows of this block as `(census ordinal, census name)` (plan §1).
pub const WINDOWS: [(u16, &str); 16] = [
    (22, "DaggerfallAdvancedSettingsWindow"),
    (29, "DaggerfallControlsWindow"),
    (39, "DaggerfallInputMessageBox"),
    (42, "DaggerfallJoystickControlsWindow"),
    (43, "DaggerfallListPickerWindow"),
    (44, "DaggerfallLoadClassicGameWindow"),
    (47, "DaggerfallMessageBox"),
    (48, "DaggerfallPauseOptionsWindow"),
    (58, "DaggerfallStartNewGameWizard"),
    (59, "DaggerfallStartWindow"),
    (67, "DaggerfallUnityMouseControlsWindow"),
    (68, "DaggerfallUnitySaveGameWindow"),
    (69, "DaggerfallUnitySetupGameWizard"),
    (71, "DaggerfallVidPlayerWindow"),
    (73, "DemoClassQuestionsWindow"),
    (74, "DemoFLCWindow"),
];

/// Declared wiring of the pause/start menu: the rows the surface lists, each
/// one a real census window of this block.
pub const MENU_TARGETS: [&str; 5] = [
    "DaggerfallStartWindow",
    "DaggerfallLoadClassicGameWindow",
    "DaggerfallUnitySaveGameWindow",
    "DaggerfallAdvancedSettingsWindow",
    "DaggerfallPauseOptionsWindow",
];

const RULE_WINDOW: &str = "m39 block A: window is not assigned to block A";
const RULE_UNCHANGED: &str = "m39: intent leaves the read model unchanged";
const RULE_MAX_LEN: &str = "m39: input exceeds m39.input.max_len";
const RULE_NO_SAVE: &str = "m39: no classic save slot carries a decodable SAVETREE.DAT";
const RULE_NO_CFG: &str = "m39: Z.CFG does not declare the setting";
const RULE_NO_TEXT: &str = "m39: TEXT.RSC is absent or the record is not decodable";
const RULE_NO_BIOG: &str = "m39: BIOG00T0.TXT is absent or carries no question";
const RULE_NO_CONTROLS: &str = "m39: the install ships no control record";
const RULE_NO_VIDEO: &str = "m39: the install ships no decodable video";
const RULE_EVIDENCE: &str = "m39 block A: not every window delivered an outcome";

/// `m34-window-assets` TSV, `DaggerfallVidPlayerWindow.cs:101`: the surface is
/// a `DaggerfallVideo` `.VID`, outside the IMG/CIF/RCI/FNT enum and the window
/// carries no text of its own.
const READ_ONLY_071: &str = "m34-window-assets TSV, DaggerfallVidPlayerWindow.cs:101: superficie = DaggerfallVideo .VID, fuera del enum IMG/CIF/RCI/FNT y la ventana no tiene texto propio";
/// `m34-window-assets` TSV, `DemoFLCWindow.cs:37`: an `FLCPlayer` of 320x200
/// over an arbitrary `.FLC`, outside the IMG/CIF/RCI/FNT enum.
const READ_ONLY_074: &str = "m34-window-assets TSV, DemoFLCWindow.cs:37: FLCPlayer de 320x200 sobre un .FLC arbitrario, fuera del enum IMG/CIF/RCI/FNT";

// ── The 21 read models of block A (plan §5) ────────────────────────────────
const R_SETTINGS_SCALE: &str = "m39.settings.scale";
const R_SETTINGS_LANGUAGE: &str = "m39.settings.language";
const R_SETTINGS_APPLIED: &str = "m39.settings.applied";
const R_CONTROLS_BINDING: &str = "m39.controls.binding";
const R_CONTROLS_CONFLICTS: &str = "m39.controls.conflicts";
const R_INPUT_PROMPT: &str = "m39.input.prompt";
const R_INPUT_VALUE: &str = "m39.input.value";
const R_INPUT_MAX_LEN: &str = "m39.input.max_len";
const R_MENU_ITEMS: &str = "m39.menu.items";
const R_MENU_SELECTED: &str = "m39.menu.selected";
const R_MENU_SELECTED_NAME: &str = "m39.menu.selected.name";
const R_MENU_DEPTH: &str = "m39.menu.depth";
const R_SAVES_COUNT: &str = "m39.saves.count";
const R_SAVES_SELECTED_SLOT: &str = "m39.saves.selected.slot";
const R_SAVES_SELECTED_NAME: &str = "m39.saves.selected.name";
const R_SAVES_LOADED_NAME: &str = "m39.saves.loaded.name";
const R_MESSAGES_COUNT: &str = "m39.messages.count";
const R_MESSAGES_PREFIX: &str = "m39.messages.selected.prefix";
const R_MESSAGES_TEXT: &str = "m39.messages.selected.text";
const R_MESSAGES_DECISION: &str = "m39.messages.decision";
const R_VIDEO_FRAMES: &str = "m39.video.frames";

// ═══════════════════════════════════════════════════════════════════════════
// Retail bank — everything the block reads, derived once from the install
// ═══════════════════════════════════════════════════════════════════════════

/// Datum the block derives from the retail install. Every field is a measured
/// value or an empty bank; [`RetailBank::from_retail`] never fails on a missing
/// file (the flows then deliver `Rejected` with a citable reason).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RetailBank {
    /// Install root (`ARENA2`'s parent), where `Z.CFG` lives.
    pub install: PathBuf,
    /// `Z.CFG` `key value` pairs, in file order.
    pub zcfg: Vec<(String, String)>,
    /// Decodable classic slots: `(slot, label)`.
    pub saves: Vec<(u16, String)>,
    /// Decoded `TEXT.RSC` records the block needs: `(id, text)`.
    pub text: Vec<(u16, String)>,
    /// `BIOG00T0.TXT` questions: `(number, text, first answer letter)`.
    pub questions: Vec<(u32, String, char)>,
    /// Shipped control records: `(file, key codes, duplicate key codes)`.
    pub controls: Vec<(String, u16, u16)>,
    /// Decodable videos: `(asset, frame count)`.
    pub videos: Vec<(String, u16)>,
    /// Rows the menu panel composes (declared wiring).
    pub menu_entries: usize,
}

impl RetailBank {
    /// Derives the bank from the frozen assets and an `ARENA2` directory.
    /// Missing or undecodable files simply leave their slice empty — the flows
    /// decide what to report.
    pub fn from_retail(assets: &UiAssets, arena2: &Path) -> Self {
        let install = arena2
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| arena2.to_path_buf());

        let zcfg = std::fs::read_to_string(install.join("Z.CFG"))
            .map(|text| parse_cfg(&text))
            .unwrap_or_default();

        let mut saves = Vec::new();
        for slot in 0..6u16 {
            let path = arena2.join(format!("SAVE{slot}")).join("SAVETREE.DAT");
            let Ok(bytes) = std::fs::read(&path) else {
                continue;
            };
            let Ok(tree) = SaveTree::decode(&bytes) else {
                continue;
            };
            saves.push((slot, format!("SAVE{slot} ^{}", tree.records.len())));
        }

        let mut text = Vec::new();
        if let Ok(bytes) = std::fs::read(arena2.join("TEXT.RSC")) {
            if let Ok(rsc) = TextRsc::parse(&bytes) {
                for id in [255u16, 1069] {
                    if let Some(record) = rsc.record_ascii(&bytes, id) {
                        text.push((id, record.trim().to_string()));
                    }
                }
            }
        }

        let mut questions = Vec::new();
        if let Ok(bytes) = std::fs::read(arena2.join("BIOG00T0.TXT")) {
            let source = String::from_utf8_lossy(&bytes);
            let bio = BiogFile::parse(&source);
            for question in &bio.questions {
                if let Some(answer) = question.answers.first() {
                    questions.push((question.number, question.text.clone(), answer.letter));
                }
            }
        }

        let mut control_files: Vec<String> = Vec::new();
        if let Some(name) = cfg(&zcfg, "controls") {
            control_files.push(name.to_string());
        }
        control_files.push("viewplyr.dat".to_string());
        let mut controls = Vec::new();
        for file in control_files {
            if controls.iter().any(|(seen, _, _)| seen == &file) {
                continue;
            }
            let Ok(bytes) = std::fs::read(arena2.join(file.to_ascii_uppercase())) else {
                continue;
            };
            let tail = zero_free_tail(&bytes);
            controls.push((
                file,
                tail.len() as u16,
                duplicate_codes(tail),
            ));
        }

        let mut videos = Vec::new();
        for asset in ["ANIM0001.VID", "AZURA.FLC"] {
            let Ok(bytes) = std::fs::read(arena2.join(asset)) else {
                continue;
            };
            if let Ok(decoded) = video::decode(&bytes) {
                videos.push((asset.to_string(), decoded.frames.len() as u16));
            }
        }

        Self {
            install,
            zcfg,
            saves,
            text,
            questions,
            controls,
            videos,
            // Measured: the nodes the retail main-menu panel composes.
            menu_entries: ui_spec::build_main_menu_panel(assets)
                .map(|panel| panel.nodes.len())
                .unwrap_or(0),
        }
    }
}

/// `key value` pairs of `Z.CFG` (blank lines and `;` comments skipped).
fn parse_cfg(source: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for line in source.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with(';') {
            continue;
        }
        let mut parts = line.split_whitespace();
        let Some(key) = parts.next() else {
            continue;
        };
        out.push((key.to_string(), parts.collect::<Vec<_>>().join(" ")));
    }
    out
}

/// Value of `key` in a parsed `Z.CFG`.
fn cfg<'a>(zcfg: &'a [(String, String)], key: &str) -> Option<&'a str> {
    zcfg.iter()
        .find(|(seen, _)| seen.eq_ignore_ascii_case(key))
        .map(|(_, value)| value.as_str())
}

/// Longest zero-free suffix of a retail control record. The install ships a
/// zero-padded header followed by the key-code tail; the tail is the longest
/// suffix that carries no zero byte.
fn zero_free_tail(bytes: &[u8]) -> &[u8] {
    let start = bytes
        .iter()
        .rposition(|byte| *byte == 0)
        .map(|index| index + 1)
        .unwrap_or(0);
    bytes.get(start..).unwrap_or(&[])
}

/// Number of duplicated key codes (each repeat counts once).
fn duplicate_codes(codes: &[u8]) -> u16 {
    let mut sorted = codes.to_vec();
    sorted.sort_unstable();
    let mut duplicates = 0u16;
    let mut index = 1usize;
    while index < sorted.len() {
        if sorted[index] == sorted[index - 1] {
            duplicates = duplicates.saturating_add(1);
            let value = sorted[index];
            while index < sorted.len() && sorted[index] == value {
                index += 1;
            }
        } else {
            index += 1;
        }
    }
    duplicates
}

// ═══════════════════════════════════════════════════════════════════════════
// State
// ═══════════════════════════════════════════════════════════════════════════

/// One block-A flow as it was delivered.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Delivery {
    /// Census window name.
    pub name: &'static str,
    /// Intents the surface attempted.
    pub intents: usize,
    /// Intents that produced at least one outcome (`<= intents`); one intent
    /// counts once no matter how many outcomes it commits (§4.1).
    pub delivered: usize,
    /// Outcomes, in delivery order.
    pub outcomes: Vec<ModalOutcome>,
    /// Read models that changed, with their signed delta.
    pub delta: Vec<(&'static str, i64)>,
    /// `Some(reason)` when the surface is read-only.
    pub read_only: Option<&'static str>,
    /// `true` between a `declare()` and the first outcome that delivers it.
    pending: bool,
}

impl Delivery {
    fn new(name: &'static str) -> Self {
        Self {
            name,
            ..Self::default()
        }
    }

    /// Opens one intent: it counts as delivered only when it commits an
    /// outcome (plan §4.1, `Interaction::delivered`).
    fn declare(&mut self) {
        self.intents = self.intents.saturating_add(1);
        self.pending = true;
    }

    fn push(&mut self, outcome: ModalOutcome) {
        if self.pending {
            self.delivered = self.delivered.saturating_add(1);
            self.pending = false;
        }
        self.outcomes.push(outcome);
    }

    fn note(&mut self, path: &'static str, delta: i64) {
        if delta == 0 {
            return;
        }
        if let Some(slot) = self.delta.iter_mut().find(|(seen, _)| *seen == path) {
            slot.1 = slot.1.saturating_add(delta);
        } else {
            self.delta.push((path, delta));
        }
    }
}

/// Block-A state: the retail bank, the navigation stack and the deliveries.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AState {
    /// Retail datum of the block.
    pub bank: RetailBank,
    /// Window stack (`Opened` pushes, `Cancelled`/`Closed` pop).
    pub stack: Vec<WindowId>,
    /// Delivered flows, in delivery order.
    pub deliveries: Vec<Delivery>,
}

impl AState {
    /// Derives the state from the frozen assets and an `ARENA2` directory.
    /// Absent retail files leave the affected banks empty and the flows report
    /// `Rejected` with a citable reason; the construction itself never fails.
    pub fn from_retail(
        assets: &UiAssets,
        arena2: &Path,
    ) -> Result<Self, UiCompatError> {
        Ok(Self {
            bank: RetailBank::from_retail(assets, arena2),
            stack: Vec::new(),
            deliveries: Vec::new(),
        })
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Commit helpers
// ═══════════════════════════════════════════════════════════════════════════

/// Signed delta between two read-model values: numeric when both parse,
/// otherwise the change in character count.
fn delta_of(before: &str, after: &str) -> i64 {
    if let (Ok(before), Ok(after)) = (before.parse::<i64>(), after.parse::<i64>()) {
        return after.saturating_sub(before);
    }
    after.chars().count() as i64 - before.chars().count() as i64
}

fn set_i64(state: &mut M39State, del: &mut Delivery, path: &'static str, value: i64) {
    let before = state
        .value(path)
        .unwrap_or_else(|_| M39_ABSENT.to_string());
    if before == value.to_string() {
        del.push(M39State::rejected(RULE_UNCHANGED));
        return;
    }
    match state.commit_i64(path, value) {
        Ok(outcome) => {
            if let ModalOutcome::Committed { before, after, .. } = &outcome {
                del.note(path, delta_of(before, after));
            }
            del.push(outcome);
        }
        Err(_) => del.push(M39State::rejected(RULE_UNCHANGED)),
    }
}

fn set_text(state: &mut M39State, del: &mut Delivery, path: &'static str, value: &str) {
    let before = state
        .value(path)
        .unwrap_or_else(|_| M39_ABSENT.to_string());
    if before == value {
        del.push(M39State::rejected(RULE_UNCHANGED));
        return;
    }
    match state.commit_text(path, value) {
        Ok(outcome) => {
            if let ModalOutcome::Committed { before, after, .. } = &outcome {
                del.note(path, delta_of(before, after));
            }
            del.push(outcome);
        }
        Err(_) => del.push(M39State::rejected(RULE_UNCHANGED)),
    }
}

fn text(del: &mut Delivery, value: String) {
    del.push(ModalOutcome::Text { value });
}

fn window_id(name: &str) -> WindowId {
    canonical_id_for(name).expect(RULE_WINDOW)
}

fn open(state: &mut M39State, del: &mut Delivery, name: &str) -> WindowId {
    let window = window_id(name);
    state.a.stack.push(window);
    del.push(ModalOutcome::Opened { window });
    window
}

fn cancel(state: &mut M39State, del: &mut Delivery, window: &WindowId) {
    if state.a.stack.last() == Some(window) {
        state.a.stack.pop();
    }
    del.push(ModalOutcome::Cancelled);
}

fn close(state: &mut M39State, del: &mut Delivery) {
    state.a.stack.pop();
    del.push(ModalOutcome::Closed);
}

// ═══════════════════════════════════════════════════════════════════════════
// Flows (plan §5)
// ═══════════════════════════════════════════════════════════════════════════

/// 022 `DaggerfallAdvancedSettingsWindow` — `maxSpeed` of `Z.CFG`.
fn flow_022(state: &mut M39State, bank: &RetailBank, del: &mut Delivery) {
    del.declare();
    let Some(raw) = cfg(&bank.zcfg, "maxSpeed") else {
        del.push(M39State::rejected(RULE_NO_CFG));
        return;
    };
    let Ok(scale) = raw.parse::<i64>() else {
        del.push(M39State::rejected(RULE_NO_CFG));
        return;
    };
    text(del, format!("maxSpeed={scale}"));
    del.declare();
    set_i64(state, del, R_SETTINGS_SCALE, scale);
    del.declare();
    text(
        del,
        state
            .value(R_SETTINGS_SCALE)
            .unwrap_or_else(|_| M39_ABSENT.to_string()),
    );
}

/// 029 `DaggerfallControlsWindow` — `controls` of `Z.CFG` + measured records.
fn flow_029(state: &mut M39State, bank: &RetailBank, del: &mut Delivery) {
    del.declare();
    let Some(file) = cfg(&bank.zcfg, "controls") else {
        del.push(M39State::rejected(RULE_NO_CONTROLS));
        return;
    };
    let file = file.to_string();
    text(del, format!("controls={file}"));
    del.declare();
    set_text(state, del, R_CONTROLS_BINDING, &file);
    del.declare();
    match bank.controls.first() {
        Some((file, keys, conflicts)) => {
            text(del, format!("{file}: {keys} key(s), {conflicts} conflict(s)"))
        }
        None => del.push(M39State::rejected(RULE_NO_CONTROLS)),
    }
}

/// 039 `DaggerfallInputMessageBox` — prompt 255, `MAPSAVE_NAME_BYTES` bound.
fn flow_039(state: &mut M39State, bank: &RetailBank, del: &mut Delivery) {
    del.declare();
    match bank.text.iter().find(|(id, _)| *id == 255) {
        Some((id, prompt)) => {
            text(del, format!("#{id} {prompt}"));
            del.declare();
            set_text(state, del, R_INPUT_PROMPT, prompt);
        }
        None => del.push(M39State::rejected(RULE_NO_TEXT)),
    }
    del.declare();
    set_i64(state, del, R_INPUT_MAX_LEN, MAPSAVE_NAME_BYTES as i64);
    del.declare();
    match bank.saves.first() {
        Some((_, label)) => set_text(state, del, R_INPUT_VALUE, label),
        None => del.push(M39State::rejected(RULE_NO_SAVE)),
    }
    del.declare();
    let probe = bank
        .text
        .iter()
        .find(|(id, _)| *id == 255)
        .map(|(_, prompt)| prompt.clone())
        .or_else(|| bank.saves.first().map(|(_, label)| label.clone()));
    match probe {
        Some(value) if value.chars().count() > MAPSAVE_NAME_BYTES => {
            del.push(M39State::rejected(RULE_MAX_LEN));
        }
        Some(value) => text(del, value),
        None => del.push(M39State::rejected(RULE_NO_TEXT)),
    }
}

/// 042 `DaggerfallJoystickControlsWindow` — conflicts of the shipped records.
fn flow_042(state: &mut M39State, bank: &RetailBank, del: &mut Delivery) {
    del.declare();
    let conflicts: u16 = bank.controls.iter().map(|(_, _, count)| *count).sum();
    set_i64(state, del, R_CONTROLS_CONFLICTS, conflicts as i64);
    del.declare();
    text(
        del,
        format!("{} control record(s) measured", bank.controls.len()),
    );
}

/// 043 `DaggerfallListPickerWindow` — cursor over the declared wiring.
fn flow_043(state: &mut M39State, bank: &RetailBank, del: &mut Delivery) {
    let cursor = if MENU_TARGETS.is_empty() {
        0usize
    } else {
        1usize.min(MENU_TARGETS.len() - 1)
    };
    del.declare();
    set_i64(state, del, R_MENU_SELECTED, cursor as i64);
    del.declare();
    if let Some(name) = MENU_TARGETS.get(cursor) {
        set_text(state, del, R_MENU_SELECTED_NAME, name);
    } else {
        del.push(M39State::rejected(RULE_WINDOW));
    }
    del.declare();
    text(del, format!("{} entr(y/ies) wired", bank.menu_entries));
}

/// 044 `DaggerfallLoadClassicGameWindow` — classic slots decoded from disk.
fn flow_044(state: &mut M39State, bank: &RetailBank, del: &mut Delivery) {
    del.declare();
    set_i64(state, del, R_SAVES_COUNT, bank.saves.len() as i64);
    del.declare();
    match bank.saves.first() {
        Some((slot, label)) => {
            set_i64(state, del, R_SAVES_SELECTED_SLOT, i64::from(*slot));
            del.declare();
            set_text(state, del, R_SAVES_SELECTED_NAME, label);
            del.declare();
            set_text(state, del, R_SAVES_LOADED_NAME, label);
        }
        None => del.push(M39State::rejected(RULE_NO_SAVE)),
    }
}

/// 047 `DaggerfallMessageBox` — `TEXT.RSC` 1069 as the pending message.
fn flow_047(state: &mut M39State, bank: &RetailBank, del: &mut Delivery) {
    del.declare();
    set_i64(state, del, R_MESSAGES_COUNT, bank.text.len() as i64);
    del.declare();
    match bank.text.iter().find(|(id, _)| *id == 1069) {
        Some((id, message)) => {
            set_text(state, del, R_MESSAGES_TEXT, message);
            del.declare();
            let prefix = format!("#{id}");
            set_text(state, del, R_MESSAGES_PREFIX, &prefix);
            del.declare();
            set_text(state, del, R_MESSAGES_DECISION, &prefix);
        }
        None => del.push(M39State::rejected(RULE_NO_TEXT)),
    }
}

/// 048 `DaggerfallPauseOptionsWindow` — menu rows + depth + child round trip.
fn flow_048(state: &mut M39State, bank: &RetailBank, del: &mut Delivery) {
    del.declare();
    set_i64(state, del, R_MENU_ITEMS, bank.menu_entries as i64);
    del.declare();
    text(
        del,
        state
            .value(R_MENU_DEPTH)
            .unwrap_or_else(|_| M39_ABSENT.to_string()),
    );
    del.declare();
    let child = open(state, del, "DaggerfallLoadClassicGameWindow");
    del.declare();
    cancel(state, del, &child);
}

/// 058 `DaggerfallStartNewGameWizard` — intro video frame count + steps.
fn flow_058(state: &mut M39State, bank: &RetailBank, del: &mut Delivery) {
    del.declare();
    match bank.videos.iter().find(|(asset, _)| asset == "ANIM0001.VID") {
        Some((asset, frames)) => {
            text(del, format!("{asset}: {frames} frame(s)"));
            del.declare();
            set_i64(state, del, R_VIDEO_FRAMES, i64::from(*frames));
        }
        None => del.push(M39State::rejected(RULE_NO_VIDEO)),
    }
    del.declare();
    text(del, format!("{} wizard step(s) wired", MENU_TARGETS.len()));
}

/// 059 `DaggerfallStartWindow` — depth, entries and child round trip.
fn flow_059(state: &mut M39State, bank: &RetailBank, del: &mut Delivery) {
    del.declare();
    let depth = state.a.stack.len() as i64 + 1;
    set_i64(state, del, R_MENU_DEPTH, depth);
    del.declare();
    text(del, MENU_TARGETS.join(", "));
    del.declare();
    let me = open(state, del, "DaggerfallStartWindow");
    del.declare();
    let child = open(state, del, "DaggerfallStartNewGameWizard");
    del.declare();
    cancel(state, del, &child);
    del.declare();
    if state.a.stack.last() == Some(&me) {
        close(state, del);
    } else {
        del.push(M39State::rejected(RULE_UNCHANGED));
    }
    let _ = bank;
}

/// 067 `DaggerfallUnityMouseControlsWindow` — the second shipped record.
fn flow_067(state: &mut M39State, bank: &RetailBank, del: &mut Delivery) {
    del.declare();
    match bank
        .controls
        .iter()
        .find(|(file, _, _)| file.eq_ignore_ascii_case("viewplyr.dat"))
    {
        Some((file, keys, conflicts)) => {
            text(del, format!("{file}: {keys} key(s), {conflicts} conflict(s)"));
            del.declare();
            set_text(state, del, R_CONTROLS_BINDING, file);
        }
        None => del.push(M39State::rejected(RULE_NO_CONTROLS)),
    }
}

/// 068 `DaggerfallUnitySaveGameWindow` — overwrite target of the save list.
fn flow_068(state: &mut M39State, bank: &RetailBank, del: &mut Delivery) {
    del.declare();
    match bank.saves.first() {
        Some((_, label)) => set_text(state, del, R_SAVES_SELECTED_NAME, label),
        None => del.push(M39State::rejected(RULE_NO_SAVE)),
    }
    del.declare();
    text(del, format!("name bound {MAPSAVE_NAME_BYTES} byte(s)"));
}

/// 069 `DaggerfallUnitySetupGameWizard` — language bank + apply step.
fn flow_069(state: &mut M39State, bank: &RetailBank, del: &mut Delivery) {
    del.declare();
    if bank.text.is_empty() {
        del.push(M39State::rejected(RULE_NO_TEXT));
    } else {
        set_text(state, del, R_SETTINGS_LANGUAGE, "TEXT.RSC");
    }
    del.declare();
    set_i64(state, del, R_SETTINGS_APPLIED, 1);
    del.declare();
    text(
        del,
        state
            .value(R_SETTINGS_LANGUAGE)
            .unwrap_or_else(|_| M39_ABSENT.to_string()),
    );
}

/// 071 `DaggerfallVidPlayerWindow` — read-only `.VID` surface.
fn flow_071(state: &mut M39State, bank: &RetailBank, del: &mut Delivery) {
    del.read_only = Some(READ_ONLY_071);
    del.declare();
    del.push(M39State::read_only(READ_ONLY_071));
    del.declare();
    match bank.videos.iter().find(|(asset, _)| asset == "ANIM0001.VID") {
        Some((asset, frames)) => text(del, format!("{asset}: {frames} frame(s)")),
        None => del.push(M39State::rejected(RULE_NO_VIDEO)),
    }
    del.declare();
    close(state, del);
}

/// 073 `DemoClassQuestionsWindow` — `BIOG00T0.TXT` questions.
fn flow_073(state: &mut M39State, bank: &RetailBank, del: &mut Delivery) {
    del.declare();
    set_i64(state, del, R_MESSAGES_COUNT, bank.questions.len() as i64);
    let first = bank.questions.first().cloned();
    del.declare();
    match &first {
        Some((number, question, _)) => {
            text(del, question.clone());
            del.declare();
            let prefix = format!("{number}.");
            set_text(state, del, R_MESSAGES_PREFIX, &prefix);
        }
        None => del.push(M39State::rejected(RULE_NO_BIOG)),
    }
    del.declare();
    match &first {
        Some((_, question, _)) => set_text(state, del, R_MESSAGES_TEXT, question),
        None => del.push(M39State::rejected(RULE_NO_BIOG)),
    }
    del.declare();
    match &first {
        Some((_, _, letter)) => set_text(state, del, R_MESSAGES_DECISION, &letter.to_string()),
        None => del.push(M39State::rejected(RULE_NO_BIOG)),
    }
    del.declare();
    let child = open(state, del, "DaggerfallStartNewGameWizard");
    del.declare();
    cancel(state, del, &child);
}

/// 074 `DemoFLCWindow` — read-only `.FLC` surface.
fn flow_074(state: &mut M39State, bank: &RetailBank, del: &mut Delivery) {
    del.read_only = Some(READ_ONLY_074);
    del.declare();
    del.push(M39State::read_only(READ_ONLY_074));
    del.declare();
    match bank.videos.iter().find(|(asset, _)| asset == "AZURA.FLC") {
        Some((asset, frames)) => text(del, format!("{asset}: {frames} frame(s)")),
        None => del.push(M39State::rejected(RULE_NO_VIDEO)),
    }
    del.declare();
    close(state, del);
}

// ═══════════════════════════════════════════════════════════════════════════
// Entry points
// ═══════════════════════════════════════════════════════════════════════════

/// Runs the flow of one block-A window and returns every delivery of the block
/// recorded so far, in census order (deterministic, plan §4.1).
pub fn interact(
    state: &mut M39State,
    name: &'static str,
) -> Result<Vec<Interaction>, UiCompatError> {
    if !WINDOWS.iter().any(|(_, known)| *known == name) {
        return Err(UiCompatError::Text(RULE_WINDOW));
    }
    let bank = state.a.bank.clone();
    let mut delivery = Delivery::new(name);
    match name {
        "DaggerfallAdvancedSettingsWindow" => flow_022(state, &bank, &mut delivery),
        "DaggerfallControlsWindow" => flow_029(state, &bank, &mut delivery),
        "DaggerfallInputMessageBox" => flow_039(state, &bank, &mut delivery),
        "DaggerfallJoystickControlsWindow" => flow_042(state, &bank, &mut delivery),
        "DaggerfallListPickerWindow" => flow_043(state, &bank, &mut delivery),
        "DaggerfallLoadClassicGameWindow" => flow_044(state, &bank, &mut delivery),
        "DaggerfallMessageBox" => flow_047(state, &bank, &mut delivery),
        "DaggerfallPauseOptionsWindow" => flow_048(state, &bank, &mut delivery),
        "DaggerfallStartNewGameWizard" => flow_058(state, &bank, &mut delivery),
        "DaggerfallStartWindow" => flow_059(state, &bank, &mut delivery),
        "DaggerfallUnityMouseControlsWindow" => flow_067(state, &bank, &mut delivery),
        "DaggerfallUnitySaveGameWindow" => flow_068(state, &bank, &mut delivery),
        "DaggerfallUnitySetupGameWizard" => flow_069(state, &bank, &mut delivery),
        "DaggerfallVidPlayerWindow" => flow_071(state, &bank, &mut delivery),
        "DemoClassQuestionsWindow" => flow_073(state, &bank, &mut delivery),
        "DemoFLCWindow" => flow_074(state, &bank, &mut delivery),
        _ => {}
    }

    match state
        .a
        .deliveries
        .iter_mut()
        .find(|seen| seen.name == name)
    {
        Some(slot) => *slot = delivery,
        None => state.a.deliveries.push(delivery),
    }

    let mut out = Vec::with_capacity(WINDOWS.len());
    for (ordinal, window) in WINDOWS.iter() {
        if let Some(delivery) = state
            .a
            .deliveries
            .iter()
            .find(|seen| seen.name == *window)
        {
            out.push(build(*ordinal, delivery));
        }
    }
    Ok(out)
}

fn build(id: u16, delivery: &Delivery) -> Interaction {
    let mut interaction = Interaction {
        id,
        name: delivery.name,
        block: BLOCK,
        intents: delivery.intents,
        delivered: delivery.delivered,
        outcomes: delivery.outcomes.clone(),
        delta: delivery.delta.clone(),
        read_only: delivery.read_only,
        digest: String::new(),
    };
    interaction.digest = fnv1a64_hex(&interaction.signature());
    interaction
}

/// Block-A evidence, derived from the recorded deliveries.
pub fn evidence(state: &M39State) -> BlockEvidence {
    let deliveries = &state.a.deliveries;
    if deliveries.is_empty() {
        return BlockEvidence::pending(BLOCK, WINDOWS.len());
    }
    let flows = deliveries.len();
    let intents: usize = deliveries.iter().map(|delivery| delivery.intents).sum();
    let delivered: usize = deliveries.iter().map(|delivery| delivery.delivered).sum();
    let read_only = deliveries
        .iter()
        .filter(|delivery| delivery.read_only.is_some())
        .count();
    let mut changed: Vec<&'static str> = deliveries
        .iter()
        .flat_map(|delivery| delivery.delta.iter().map(|(path, _)| *path))
        .collect();
    changed.sort_unstable();
    changed.dedup();

    let ok = flows == WINDOWS.len()
        && deliveries
            .iter()
            .all(|delivery| !delivery.outcomes.is_empty());
    BlockEvidence {
        block: BLOCK,
        windows: flows,
        flows,
        intents,
        delivered,
        state_delta: changed.len(),
        read_only,
        ok,
        reason: if ok { None } else { Some(RULE_EVIDENCE) },
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Tests (plan §4.1 a-e)
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod m39_block_a_tests {
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU32, Ordering};

    use daggerfall_parser_bevy_bridge::ui_spec::UiAssets;
    use daggerfall_parser_rs::texture::Palette;
    use daggerfall_skills::career::CFG_BYTES;

    use super::*;

    const CLASS_FILE: &str = "CLASS01.CFG";

    struct Scratch(PathBuf);

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    static NEXT_SCRATCH: AtomicU32 = AtomicU32::new(0);

    /// 74-byte synthetic `CLASS01.CFG` (the shape the `daggerfall-skills`
    /// fixtures parse).
    fn career_bytes() -> Vec<u8> {
        let mut bytes = vec![0u8; CFG_BYTES];
        bytes[4..6].copy_from_slice(&0x0C00u16.to_le_bytes());
        bytes[16..19].copy_from_slice(&[31, 29, 32]);
        bytes[19..22].copy_from_slice(&[22, 24, 25]);
        bytes[22..28].copy_from_slice(&[23, 26, 27, 28, 30, 33]);
        bytes[28..38].copy_from_slice(b"Spellsword");
        bytes[52..54].copy_from_slice(&12u16.to_le_bytes());
        bytes[54..58].copy_from_slice(&0x0001_0000u32.to_le_bytes());
        for (index, value) in [60u16, 50, 55, 50, 50, 40, 45, 50].iter().enumerate() {
            bytes[58 + index * 2..60 + index * 2].copy_from_slice(&value.to_le_bytes());
        }
        bytes
    }

    fn scratch() -> Scratch {
        let serial = NEXT_SCRATCH.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "daggerfall-m39-block-a-{}-{serial}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).expect("scratch arena2");
        std::fs::write(dir.join(CLASS_FILE), career_bytes()).expect("career write");
        Scratch(dir)
    }

    fn zero_palette() -> Palette {
        Palette {
            colors: [[0u8; 3]; 256],
        }
    }

    fn empty_assets() -> UiAssets {
        UiAssets::new(zero_palette(), zero_palette(), zero_palette())
    }

    /// Bank with every datum the 16 flows read, so the happy paths run without
    /// the retail install.
    fn fixture_bank() -> RetailBank {
        RetailBank {
            install: PathBuf::from("/nonexistent-install"),
            zcfg: vec![
                ("maxSpeed".to_string(), "200".to_string()),
                ("controls".to_string(), "betaplyr.dat".to_string()),
                ("region".to_string(), "17".to_string()),
            ],
            saves: vec![(0u16, "SAVE0 ^12".to_string())],
            text: vec![
                (255u16, "From where dost thou hail?".to_string()),
                (1069u16, "Are you sure you want to exit the game?".to_string()),
            ],
            questions: vec![
                (1u32, "You are born in...".to_string(), 'a'),
                (2u32, "Your childhood was...".to_string(), 'b'),
            ],
            controls: vec![
                ("betaplyr.dat".to_string(), 38u16, 0u16),
                ("viewplyr.dat".to_string(), 38u16, 0u16),
            ],
            videos: vec![
                ("ANIM0001.VID".to_string(), 30u16),
                ("AZURA.FLC".to_string(), 40u16),
            ],
            menu_entries: MENU_TARGETS.len(),
        }
    }

    fn fixture_state(dir: &Scratch) -> M39State {
        let assets = empty_assets();
        let mut state = M39State::new(&assets, &dir.0).expect("state construction");
        state.a.bank = fixture_bank();
        state
    }

    fn run_all(state: &mut M39State) -> Vec<Interaction> {
        let mut interactions = Vec::new();
        for (_, name) in WINDOWS.iter() {
            interactions = interact(state, name).expect("interact");
        }
        interactions
    }

    #[test]
    fn m39_block_a_every_window_delivers() {
        let dir = scratch();
        let mut state = fixture_state(&dir);
        let interactions = run_all(&mut state);
        assert_eq!(interactions.len(), WINDOWS.len());
        for interaction in &interactions {
            assert_eq!(interaction.block, BLOCK, "{}", interaction.name);
            assert!(
                interaction.delivered >= 1,
                "{} delivered {}",
                interaction.name,
                interaction.delivered
            );
            assert!(
                interaction.intents >= interaction.delivered,
                "{} attempted {} but delivered {}",
                interaction.name,
                interaction.intents,
                interaction.delivered
            );
            assert!(
                interaction.delivered <= interaction.outcomes.len(),
                "{} counts intents, not outcomes",
                interaction.name
            );
            assert_eq!(interaction.digest.len(), 16, "{}", interaction.name);
            for outcome in &interaction.outcomes {
                M39State::validate(outcome).expect("outcome validates");
            }
        }
        let evidence = evidence(&state);
        assert!(evidence.ok, "evidence: {:?}", evidence.reason);
        assert_eq!(evidence.windows, WINDOWS.len());
        assert_eq!(evidence.read_only, 2);
        assert!(evidence.state_delta > 0);
    }

    /// The counter is per intent: a second outcome of the same intent, or a
    /// declared intent with no outcome yet, must not move `delivered`.
    #[test]
    fn m39_block_a_delivery_counts_one_intent_once() {
        let mut delivery = Delivery::new("DaggerfallMessageBox");
        delivery.declare();
        delivery.push(M39State::rejected(RULE_UNCHANGED));
        delivery.push(M39State::rejected(RULE_UNCHANGED));
        assert_eq!(delivery.intents, 1);
        assert_eq!(delivery.delivered, 1, "two outcomes of one intent count once");
        delivery.declare();
        assert_eq!(delivery.intents, 2);
        assert_eq!(delivery.delivered, 1, "a declared intent is not delivered yet");
        delivery.push(M39State::rejected(RULE_UNCHANGED));
        assert_eq!(delivery.delivered, 2);
        assert!(delivery.delivered <= delivery.intents);
    }

    #[test]
    fn m39_block_a_max_len_rejects() {
        let dir = scratch();
        let mut state = fixture_state(&dir);
        let interactions = run_all(&mut state);
        let input = interactions
            .iter()
            .find(|interaction| interaction.name == "DaggerfallInputMessageBox")
            .expect("input message box delivered");
        assert!(
            input.outcomes.iter().any(|outcome| matches!(
                outcome,
                ModalOutcome::Rejected { reason } if *reason == RULE_MAX_LEN
            )),
            "outcomes: {:?}",
            input.outcomes
        );
        assert_eq!(
            state.value(R_INPUT_MAX_LEN).expect("max len"),
            MAPSAVE_NAME_BYTES.to_string()
        );
    }

    #[test]
    fn m39_block_a_navigation_stack_is_restored() {
        let dir = scratch();
        let mut state = fixture_state(&dir);
        for (_, name) in WINDOWS.iter() {
            let entry_depth = state.a.stack.len();
            let _ = interact(&mut state, name).expect("interact");
            assert_eq!(
                state.a.stack.len(),
                entry_depth,
                "{name} left the stack at {:?}",
                state.a.stack
            );
        }
    }

    #[test]
    fn m39_block_a_settings_apply_changes_one_route() {
        let dir = scratch();
        let mut state = fixture_state(&dir);
        let routes = [
            R_SETTINGS_SCALE,
            R_SETTINGS_LANGUAGE,
            R_SETTINGS_APPLIED,
            R_CONTROLS_BINDING,
            R_CONTROLS_CONFLICTS,
            R_INPUT_PROMPT,
            R_INPUT_VALUE,
            R_INPUT_MAX_LEN,
            R_MENU_ITEMS,
            R_MENU_SELECTED,
            R_MENU_SELECTED_NAME,
            R_MENU_DEPTH,
            R_SAVES_COUNT,
            R_SAVES_SELECTED_SLOT,
            R_SAVES_SELECTED_NAME,
            R_SAVES_LOADED_NAME,
            R_MESSAGES_COUNT,
            R_MESSAGES_PREFIX,
            R_MESSAGES_TEXT,
            R_MESSAGES_DECISION,
            R_VIDEO_FRAMES,
        ];
        let before: Vec<String> = routes
            .iter()
            .map(|route| state.value(route).unwrap_or_default())
            .collect();
        let _ = interact(&mut state, "DaggerfallAdvancedSettingsWindow").expect("interact");
        let after: Vec<String> = routes
            .iter()
            .map(|route| state.value(route).unwrap_or_default())
            .collect();
        let changed: Vec<&str> = routes
            .iter()
            .zip(before.iter().zip(after.iter()))
            .filter(|(_, (before, after))| before != after)
            .map(|(route, _)| *route)
            .collect();
        assert_eq!(changed, vec![R_SETTINGS_SCALE], "changed: {changed:?}");
        assert_eq!(after[0], "200");
    }

    #[test]
    fn m39_block_a_is_deterministic() {
        let first_dir = scratch();
        let second_dir = scratch();
        let mut first = fixture_state(&first_dir);
        let mut second = fixture_state(&second_dir);
        let first_runs = run_all(&mut first);
        let second_runs = run_all(&mut second);
        let first_digests: Vec<(&str, String)> = first_runs
            .iter()
            .map(|interaction| (interaction.name, interaction.digest.clone()))
            .collect();
        let second_digests: Vec<(&str, String)> = second_runs
            .iter()
            .map(|interaction| (interaction.name, interaction.digest.clone()))
            .collect();
        assert_eq!(first_digests, second_digests);
        assert_eq!(first.a.deliveries, second.a.deliveries);
    }
}
