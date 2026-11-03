//! `m42_quest_probe` — `PROBE-SYS-QUEST-001` (M42 / W42-5): quest dispatch in
//! depth, measured **on the engine** (roadmap
//! `docs/acknowledgments/roadmap-m18-m48.md:320`).
//!
//! The audit `DESIGN-AUDIT-276c7d94` measured two holes in this probe and this
//! file repairs both:
//!
//! * **GAP-M42-05** — block A never asked the engine. It enumerated the
//!   author's own array and asserted `QuestRequest::from_effect(x).is_some()`,
//!   i.e. it asserted over the table it was reading. Block A now materialises
//!   every variant as one quest action ([`quest_for`]), runs one real
//!   [`Quest::tick`], and compares the observed requests/failures against a
//!   hand-written oracle ([`ORACLE`]) derived from the real execution path
//!   (`Quest::run_action` → `QuestRequest::from_effect`), never from the
//!   table. A regression in `from_effect` or in `tick` turns the block red.
//! * **GAP-M42-03** — the variant list was a hand-maintained
//!   `[OpcodeEffect; 89]`, so a 90th variant silently escaped the measurement.
//!   The list is now produced by [`next_effect`], an exhaustive `match` with
//!   **no wildcard arm**: adding a variant fails the build, and the walk asserts
//!   that it visits every variant exactly once, from [`FIRST_EFFECT`] to `None`.
//!
//! Two further measurements keep the gate honest:
//!
//! * [`BUILT_SOURCES`] bakes the measured sources into the binary and prints
//!   their `sha256` fingerprint (`SOURCES`). `--skip-build` can no longer bless
//!   a pristine binary over a regressed tree (GAP-M42-06).
//! * The delta of the quest gap is declared `INSTRUMENT_ONLY` (GAP-M42-08) and
//!   the reachability of its 16 classes is measured live, not asserted
//!   (GAP-M42-02): every covered `type_id` is mapped through
//!   [`opcode_coverage`] and the classes that no type reaches are listed with
//!   their measured zero.
//!
//! **0 retail bytes**: every fixture is synthetic, so the probe is
//! retail-independent.
//!
//! **Determinism:** no floats are printed, every iteration is over a fixed
//! walk, and the last line is `DIGEST <sha256>` over every byte printed before
//! it.

use daggerfall_behavior::quests::{
    opcode_coverage, ClockRecipe, DfRandom, OpcodeEffect, Quest, QuestAction, QuestClock,
    QuestFailure, QuestRequest, QuestTask, TaskType, OPCODE_COVERAGE,
};
use sha2::{Digest, Sha256};

/// Lane correlation id (W42-5 quest lane).
const CR: &str = "CR-765daa10-5084-4197-883d-dd626301e59e";

/// Citations (every one resolves on disk).
const SRC_QUESTS: &str = "crates/behavior/src/quests.rs";
const SRC_M29: &str = "crates/app/src/m29_probe.rs:1722";
const SRC_PROBE: &str = "crates/app/src/m42_quest_probe.rs";
const SPEC_ROADMAP: &str = "docs/acknowledgments/roadmap-m18-m48.md:320";
/// UESP coverage table: the catalogue is closed and `0` sits outside it
/// (`§2.2 Cobertura UESP`, measured 2026-10-20 over 247 `.QBN` files).
const SPEC_UESP: &str = "docs/sintergic/game/programming/m29-playable-quests-spec-CR-5243af63.md:211";

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

// ── sources: the binary is bound to what it was built from ───────────────

/// Sources this binary was **built** from: `include_bytes!` bakes the exact
/// bytes into the binary, so the printed fingerprint is the build's, not the
/// tree's. The gate recomputes it from disk and rejects a pristine binary over
/// a regressed source (`--skip-build`, GAP-M42-06).
const BUILT_SOURCES: [(&str, &[u8]); 2] = [
    (SRC_QUESTS, include_bytes!("../../behavior/src/quests.rs")),
    (SRC_PROBE, include_bytes!("m42_quest_probe.rs")),
];

/// `sha256` over the sorted `<path>\0<sha256(bytes)>` lines.
fn source_fingerprint() -> String {
    let mut parts: Vec<String> = BUILT_SOURCES
        .iter()
        .map(|(path, bytes)| format!("{path}\u{0}{}", sha256_hex(bytes)))
        .collect();
    parts.sort();
    sha256_hex(parts.join("\n").as_bytes())
}

/// Comma-joined measured paths, printed so the gate fingerprints the same set.
fn source_paths() -> String {
    BUILT_SOURCES
        .iter()
        .map(|(path, _)| *path)
        .collect::<Vec<&str>>()
        .join(",")
}

// ── variant census: exhaustive walk (GAP-M42-03) ─────────────────────────

/// First variant of `OpcodeEffect` in declaration order (`quests.rs:93`).
const FIRST_EFFECT: OpcodeEffect = OpcodeEffect::LogMessage;

/// Successor of every `OpcodeEffect` variant — an exhaustive `match` with **no
/// wildcard arm**. Adding a 90th variant does not compile here, which is the
/// only mechanism that can keep the census complete (GAP-M42-03).
fn next_effect(effect: OpcodeEffect) -> Option<OpcodeEffect> {
    Some(match effect {
        OpcodeEffect::LogMessage => OpcodeEffect::JournalNote,
        OpcodeEffect::JournalNote => OpcodeEffect::DisplayMessage,
        OpcodeEffect::DisplayMessage => OpcodeEffect::RemoveLogMessage,
        OpcodeEffect::RemoveLogMessage => OpcodeEffect::EndQuest,
        OpcodeEffect::EndQuest => OpcodeEffect::ClearTask,
        OpcodeEffect::ClearTask => OpcodeEffect::StartTask,
        OpcodeEffect::StartTask => OpcodeEffect::UnsetTask,
        OpcodeEffect::UnsetTask => OpcodeEffect::StartTimer,
        OpcodeEffect::StartTimer => OpcodeEffect::StopTimer,
        OpcodeEffect::StopTimer => OpcodeEffect::GivePc,
        OpcodeEffect::GivePc => OpcodeEffect::GiveItem,
        OpcodeEffect::GiveItem => OpcodeEffect::CreateFoe,
        OpcodeEffect::CreateFoe => OpcodeEffect::PlaceFoe,
        OpcodeEffect::PlaceFoe => OpcodeEffect::CreateNpc,
        OpcodeEffect::CreateNpc => OpcodeEffect::CreateNpcAt,
        OpcodeEffect::CreateNpcAt => OpcodeEffect::PlaceNpc,
        OpcodeEffect::PlaceNpc => OpcodeEffect::PlaceItem,
        OpcodeEffect::PlaceItem => OpcodeEffect::GiveItemToNpc,
        OpcodeEffect::GiveItemToNpc => OpcodeEffect::TakeItem,
        OpcodeEffect::TakeItem => OpcodeEffect::TakeGold,
        OpcodeEffect::TakeGold => OpcodeEffect::GiveGold,
        OpcodeEffect::GiveGold => OpcodeEffect::TeleportPc,
        OpcodeEffect::TeleportPc => OpcodeEffect::RevealLocation,
        OpcodeEffect::RevealLocation => OpcodeEffect::RevealMap,
        OpcodeEffect::RevealMap => OpcodeEffect::PlaySound,
        OpcodeEffect::PlaySound => OpcodeEffect::PlaySong,
        OpcodeEffect::PlaySong => OpcodeEffect::PlayVideo,
        OpcodeEffect::PlayVideo => OpcodeEffect::StartQuest,
        OpcodeEffect::StartQuest => OpcodeEffect::RunQuest,
        OpcodeEffect::RunQuest => OpcodeEffect::ChangeReputation,
        OpcodeEffect::ChangeReputation => OpcodeEffect::SetPlayerCrime,
        OpcodeEffect::SetPlayerCrime => OpcodeEffect::SpawnCityGuards,
        OpcodeEffect::SpawnCityGuards => OpcodeEffect::LegalRepute,
        OpcodeEffect::LegalRepute => OpcodeEffect::MakeDiseased,
        OpcodeEffect::MakeDiseased => OpcodeEffect::CureDisease,
        OpcodeEffect::CureDisease => OpcodeEffect::CastSpell,
        OpcodeEffect::CastSpell => OpcodeEffect::AddTopic,
        OpcodeEffect::AddTopic => OpcodeEffect::RemoveTopic,
        OpcodeEffect::RemoveTopic => OpcodeEffect::Rumor,
        OpcodeEffect::Rumor => OpcodeEffect::TravelTime,
        OpcodeEffect::TravelTime => OpcodeEffect::AddFace,
        OpcodeEffect::AddFace => OpcodeEffect::DropFace,
        OpcodeEffect::DropFace => OpcodeEffect::ShowPrompt,
        OpcodeEffect::ShowPrompt => OpcodeEffect::AddQuestor,
        OpcodeEffect::AddQuestor => OpcodeEffect::DropQuestor,
        OpcodeEffect::DropQuestor => OpcodeEffect::MakePermanent,
        OpcodeEffect::MakePermanent => OpcodeEffect::WeatherOverride,
        OpcodeEffect::WeatherOverride => OpcodeEffect::HaveItem,
        OpcodeEffect::HaveItem => OpcodeEffect::PcAt,
        OpcodeEffect::PcAt => OpcodeEffect::KillCount,
        OpcodeEffect::KillCount => OpcodeEffect::InjuredFoe,
        OpcodeEffect::InjuredFoe => OpcodeEffect::ItemFound,
        OpcodeEffect::ItemFound => OpcodeEffect::ItemUsed,
        OpcodeEffect::ItemUsed => OpcodeEffect::ReputeExceeds,
        OpcodeEffect::ReputeExceeds => OpcodeEffect::StateAnd,
        OpcodeEffect::StateAnd => OpcodeEffect::StateOr,
        OpcodeEffect::StateOr => OpcodeEffect::PickOneOf,
        OpcodeEffect::PickOneOf => OpcodeEffect::DestroyNpc,
        OpcodeEffect::DestroyNpc => OpcodeEffect::HideNpc,
        OpcodeEffect::HideNpc => OpcodeEffect::RestoreNpc,
        OpcodeEffect::RestoreNpc => OpcodeEffect::MuteNpc,
        OpcodeEffect::MuteNpc => OpcodeEffect::ChangeFoeTeam,
        OpcodeEffect::ChangeFoeTeam => OpcodeEffect::ChangeFoeInfighting,
        OpcodeEffect::ChangeFoeInfighting => OpcodeEffect::RestrainFoe,
        OpcodeEffect::RestrainFoe => OpcodeEffect::UnrestrainFoe,
        OpcodeEffect::UnrestrainFoe => OpcodeEffect::Enemies,
        OpcodeEffect::Enemies => OpcodeEffect::RemoveFoe,
        OpcodeEffect::RemoveFoe => OpcodeEffect::ClickedNpc,
        OpcodeEffect::ClickedNpc => OpcodeEffect::DailyFrom,
        OpcodeEffect::DailyFrom => OpcodeEffect::AttributeLevel,
        OpcodeEffect::AttributeLevel => OpcodeEffect::LevelCompleted,
        OpcodeEffect::LevelCompleted => OpcodeEffect::FactionReputation,
        OpcodeEffect::FactionReputation => OpcodeEffect::WhenPcEntersExits,
        OpcodeEffect::WhenPcEntersExits => OpcodeEffect::WhenNpcIsAvailable,
        OpcodeEffect::WhenNpcIsAvailable => OpcodeEffect::WhenSkillLevel,
        OpcodeEffect::WhenSkillLevel => OpcodeEffect::ClickedItem,
        OpcodeEffect::ClickedItem => OpcodeEffect::ClickedFoe,
        OpcodeEffect::ClickedFoe => OpcodeEffect::TotingItemAndClickedNpc,
        OpcodeEffect::TotingItemAndClickedNpc => OpcodeEffect::DroppedItemAtPlace,
        OpcodeEffect::DroppedItemAtPlace => OpcodeEffect::Season,
        OpcodeEffect::Season => OpcodeEffect::Climate,
        OpcodeEffect::Climate => OpcodeEffect::CastEffectDo,
        OpcodeEffect::CastEffectDo => OpcodeEffect::CastSpellOnFoe,
        OpcodeEffect::CastSpellOnFoe => OpcodeEffect::DialogLink,
        OpcodeEffect::DialogLink => OpcodeEffect::KillFoe,
        OpcodeEffect::KillFoe => OpcodeEffect::PromptMulti,
        OpcodeEffect::PromptMulti => OpcodeEffect::TrainPc,
        OpcodeEffect::TrainPc => OpcodeEffect::WorldUpdate,
        OpcodeEffect::WorldUpdate => return None,
    })
}

/// Every variant, walked from [`FIRST_EFFECT`]; `cycle` is true when the walk
/// revisits one instead of reaching `None` (never happens: no wildcard arm,
/// every variant appears exactly once in [`next_effect`]).
fn walk_effects() -> (Vec<OpcodeEffect>, bool) {
    let mut walked: Vec<OpcodeEffect> = Vec::new();
    let mut cursor = Some(FIRST_EFFECT);
    while let Some(effect) = cursor {
        if walked.contains(&effect) {
            return (walked, true);
        }
        walked.push(effect);
        cursor = next_effect(effect);
    }
    (walked, false)
}

/// Number of distinct variants in the walk (must equal its length).
fn unique_count(walked: &[OpcodeEffect]) -> usize {
    walked
        .iter()
        .enumerate()
        .filter(|(index, effect)| !walked[..*index].contains(effect))
        .count()
}

// ── the oracle: what one engine tick must do, written from run_action ────

/// Expected engine outcome of one tick, hand-written from
/// `Quest::run_action` (log/journal/display/remove/end/task/timer arms) and
/// from the forwarding arm `QuestRequest::from_effect`. Never derived from the
/// coverage table and never from the probe's own arrays.
#[derive(Clone, Copy)]
enum Expect {
    /// `run_action` handles it internally: no organ request, no failure.
    Internal,
    /// `give pc`: sets the success flag and requests `GiveItemToPc`.
    Success,
    /// Forwarded to its owning organ as exactly this `QuestRequest` variant.
    Request(&'static str),
}

/// One expectation per variant, in declaration order ([`next_effect`] order).
const ORACLE: [(OpcodeEffect, Expect); 89] = [
    (OpcodeEffect::LogMessage, Expect::Internal),
    (OpcodeEffect::JournalNote, Expect::Internal),
    (OpcodeEffect::DisplayMessage, Expect::Internal),
    (OpcodeEffect::RemoveLogMessage, Expect::Internal),
    (OpcodeEffect::EndQuest, Expect::Internal),
    (OpcodeEffect::ClearTask, Expect::Internal),
    (OpcodeEffect::StartTask, Expect::Internal),
    (OpcodeEffect::UnsetTask, Expect::Internal),
    (OpcodeEffect::StartTimer, Expect::Internal),
    (OpcodeEffect::StopTimer, Expect::Internal),
    (OpcodeEffect::GivePc, Expect::Success),
    (OpcodeEffect::GiveItem, Expect::Request("GiveItemToPc")),
    (OpcodeEffect::CreateFoe, Expect::Request("SpawnFoe")),
    (OpcodeEffect::PlaceFoe, Expect::Request("PlaceFoe")),
    (OpcodeEffect::CreateNpc, Expect::Request("CreateNpc")),
    (OpcodeEffect::CreateNpcAt, Expect::Request("CreateNpcAt")),
    (OpcodeEffect::PlaceNpc, Expect::Request("PlaceNpc")),
    (OpcodeEffect::PlaceItem, Expect::Request("PlaceItem")),
    (OpcodeEffect::GiveItemToNpc, Expect::Request("GiveItemToNpc")),
    (OpcodeEffect::TakeItem, Expect::Request("TakeItem")),
    (OpcodeEffect::TakeGold, Expect::Request("TakeGold")),
    (OpcodeEffect::GiveGold, Expect::Request("GiveGold")),
    (OpcodeEffect::TeleportPc, Expect::Request("TeleportPc")),
    (OpcodeEffect::RevealLocation, Expect::Request("RevealLocation")),
    (OpcodeEffect::RevealMap, Expect::Request("RevealLocation")),
    (OpcodeEffect::PlaySound, Expect::Request("PlaySound")),
    (OpcodeEffect::PlaySong, Expect::Request("PlaySong")),
    (OpcodeEffect::PlayVideo, Expect::Request("PlayVideo")),
    (OpcodeEffect::StartQuest, Expect::Request("StartQuest")),
    (OpcodeEffect::RunQuest, Expect::Request("StartQuest")),
    (OpcodeEffect::ChangeReputation, Expect::Request("ChangeReputation")),
    (OpcodeEffect::SetPlayerCrime, Expect::Request("SetPlayerCrime")),
    (OpcodeEffect::SpawnCityGuards, Expect::Request("SpawnCityGuards")),
    (OpcodeEffect::LegalRepute, Expect::Request("LegalRepute")),
    (OpcodeEffect::MakeDiseased, Expect::Request("MakeDiseased")),
    (OpcodeEffect::CureDisease, Expect::Request("CureDisease")),
    (OpcodeEffect::CastSpell, Expect::Request("Condition")),
    (OpcodeEffect::AddTopic, Expect::Request("AddTopic")),
    (OpcodeEffect::RemoveTopic, Expect::Request("RemoveTopic")),
    (OpcodeEffect::Rumor, Expect::Request("Rumor")),
    (OpcodeEffect::TravelTime, Expect::Request("TravelTime")),
    (OpcodeEffect::AddFace, Expect::Request("ShowFace")),
    (OpcodeEffect::DropFace, Expect::Request("HideFace")),
    (OpcodeEffect::ShowPrompt, Expect::Request("Prompt")),
    (OpcodeEffect::AddQuestor, Expect::Request("AddQuestor")),
    (OpcodeEffect::DropQuestor, Expect::Request("DropQuestor")),
    (OpcodeEffect::MakePermanent, Expect::Request("MakePermanent")),
    (OpcodeEffect::WeatherOverride, Expect::Request("WeatherOverride")),
    (OpcodeEffect::HaveItem, Expect::Request("Condition")),
    (OpcodeEffect::PcAt, Expect::Request("Condition")),
    (OpcodeEffect::KillCount, Expect::Request("Condition")),
    (OpcodeEffect::InjuredFoe, Expect::Request("Condition")),
    (OpcodeEffect::ItemFound, Expect::Request("Condition")),
    (OpcodeEffect::ItemUsed, Expect::Request("Condition")),
    (OpcodeEffect::ReputeExceeds, Expect::Request("Condition")),
    (OpcodeEffect::StateAnd, Expect::Request("Condition")),
    (OpcodeEffect::StateOr, Expect::Request("Condition")),
    (OpcodeEffect::PickOneOf, Expect::Request("Condition")),
    (OpcodeEffect::DestroyNpc, Expect::Request("DestroyNpc")),
    (OpcodeEffect::HideNpc, Expect::Request("HideNpc")),
    (OpcodeEffect::RestoreNpc, Expect::Request("HideNpc")),
    (OpcodeEffect::MuteNpc, Expect::Request("MuteNpc")),
    (OpcodeEffect::ChangeFoeTeam, Expect::Request("FoeCommand")),
    (OpcodeEffect::ChangeFoeInfighting, Expect::Request("FoeCommand")),
    (OpcodeEffect::RestrainFoe, Expect::Request("FoeCommand")),
    (OpcodeEffect::UnrestrainFoe, Expect::Request("FoeCommand")),
    (OpcodeEffect::Enemies, Expect::Request("FoeCommand")),
    (OpcodeEffect::RemoveFoe, Expect::Request("FoeCommand")),
    (OpcodeEffect::ClickedNpc, Expect::Request("Condition")),
    (OpcodeEffect::DailyFrom, Expect::Request("Condition")),
    (OpcodeEffect::AttributeLevel, Expect::Request("Condition")),
    (OpcodeEffect::LevelCompleted, Expect::Request("Condition")),
    (OpcodeEffect::FactionReputation, Expect::Request("Condition")),
    (OpcodeEffect::WhenPcEntersExits, Expect::Request("Condition")),
    (OpcodeEffect::WhenNpcIsAvailable, Expect::Request("Condition")),
    (OpcodeEffect::WhenSkillLevel, Expect::Request("Condition")),
    (OpcodeEffect::ClickedItem, Expect::Request("Condition")),
    (OpcodeEffect::ClickedFoe, Expect::Request("Condition")),
    (OpcodeEffect::TotingItemAndClickedNpc, Expect::Request("Condition")),
    (OpcodeEffect::DroppedItemAtPlace, Expect::Request("Condition")),
    (OpcodeEffect::Season, Expect::Request("Condition")),
    (OpcodeEffect::Climate, Expect::Request("Condition")),
    (OpcodeEffect::CastEffectDo, Expect::Request("CastEffectDo")),
    (OpcodeEffect::CastSpellOnFoe, Expect::Request("CastSpellOnFoe")),
    (OpcodeEffect::DialogLink, Expect::Request("DialogLink")),
    (OpcodeEffect::KillFoe, Expect::Request("KillFoe")),
    (OpcodeEffect::PromptMulti, Expect::Request("PromptMulti")),
    (OpcodeEffect::TrainPc, Expect::Request("TrainPc")),
    (OpcodeEffect::WorldUpdate, Expect::Request("WorldUpdate")),
];

/// Observation label of a request (the oracle's vocabulary).
fn request_tag(request: &QuestRequest) -> &'static str {
    match request {
        QuestRequest::SpawnFoe { .. } => "SpawnFoe",
        QuestRequest::PlaceFoe { .. } => "PlaceFoe",
        QuestRequest::CreateNpc { .. } => "CreateNpc",
        QuestRequest::CreateNpcAt { .. } => "CreateNpcAt",
        QuestRequest::PlaceNpc { .. } => "PlaceNpc",
        QuestRequest::PlaceItem { .. } => "PlaceItem",
        QuestRequest::GiveItemToNpc { .. } => "GiveItemToNpc",
        QuestRequest::GiveItemToPc { .. } => "GiveItemToPc",
        QuestRequest::TakeItem { .. } => "TakeItem",
        QuestRequest::TakeGold { .. } => "TakeGold",
        QuestRequest::GiveGold { .. } => "GiveGold",
        QuestRequest::TeleportPc { .. } => "TeleportPc",
        QuestRequest::RevealLocation { .. } => "RevealLocation",
        QuestRequest::RevealMap { .. } => "RevealMap",
        QuestRequest::PlaySound { .. } => "PlaySound",
        QuestRequest::PlaySong { .. } => "PlaySong",
        QuestRequest::PlayVideo { .. } => "PlayVideo",
        QuestRequest::StartQuest { .. } => "StartQuest",
        QuestRequest::ChangeReputation { .. } => "ChangeReputation",
        QuestRequest::SetPlayerCrime { .. } => "SetPlayerCrime",
        QuestRequest::SpawnCityGuards { .. } => "SpawnCityGuards",
        QuestRequest::LegalRepute { .. } => "LegalRepute",
        QuestRequest::MakeDiseased { .. } => "MakeDiseased",
        QuestRequest::CureDisease => "CureDisease",
        QuestRequest::CastSpell { .. } => "CastSpell",
        QuestRequest::AddTopic { .. } => "AddTopic",
        QuestRequest::RemoveTopic { .. } => "RemoveTopic",
        QuestRequest::Rumor { .. } => "Rumor",
        QuestRequest::TravelTime { .. } => "TravelTime",
        QuestRequest::ShowFace { .. } => "ShowFace",
        QuestRequest::HideFace { .. } => "HideFace",
        QuestRequest::Prompt { .. } => "Prompt",
        QuestRequest::AddQuestor { .. } => "AddQuestor",
        QuestRequest::DropQuestor { .. } => "DropQuestor",
        QuestRequest::MakePermanent { .. } => "MakePermanent",
        QuestRequest::WeatherOverride { .. } => "WeatherOverride",
        QuestRequest::DestroyNpc { .. } => "DestroyNpc",
        QuestRequest::HideNpc { .. } => "HideNpc",
        QuestRequest::MuteNpc { .. } => "MuteNpc",
        QuestRequest::FoeCommand { .. } => "FoeCommand",
        QuestRequest::CastEffectDo { .. } => "CastEffectDo",
        QuestRequest::CastSpellOnFoe { .. } => "CastSpellOnFoe",
        QuestRequest::DialogLink { .. } => "DialogLink",
        QuestRequest::KillFoe { .. } => "KillFoe",
        QuestRequest::PromptMulti { .. } => "PromptMulti",
        QuestRequest::TrainPc { .. } => "TrainPc",
        QuestRequest::WorldUpdate { .. } => "WorldUpdate",
        QuestRequest::Condition { .. } => "Condition",
    }
}

/// Failure label, so a red row says which engine rule broke.
fn failure_tag(failure: &QuestFailure) -> String {
    match failure {
        QuestFailure::UnsupportedOpcode(type_id) => format!("UnsupportedOpcode({type_id:#04x})"),
        QuestFailure::MissingMessage(message_id) => format!("MissingMessage({message_id})"),
        QuestFailure::MissingTarget(symbol) => format!("MissingTarget({symbol:?})"),
        QuestFailure::UnhandledEffect(effect) => format!("UnhandledEffect({effect:?})"),
        QuestFailure::NoSuccess => "NoSuccess".to_string(),
    }
}

// ── fixture: one action, one triggered task, one tick ────────────────────

/// Quest carrying exactly one action of `effect` in one already-triggered
/// task, with a message and a task/clock symbol `t` so every internal arm
/// (`clear`/`set`/`unset` task, `start`/`stop` timer) resolves its target and
/// the tick measures the effect and nothing else.
fn quest_for(effect: OpcodeEffect) -> Quest {
    let mut quest = Quest::new("engine", 0);
    quest.add_message(1, "m1");
    let mut task = QuestTask::new("t", TaskType::Standard);
    task.add_action(QuestAction::new(effect).with_message(1).with_target("t"));
    task.is_triggered = true;
    quest.add_task(task);
    let mut rng = DfRandom::srand(0x5EED);
    quest.add_clock(QuestClock::new("t", ClockRecipe::Fixed(60), &mut rng));
    quest
}

/// What the engine actually did during one tick.
struct Observed {
    requests: usize,
    tags: Vec<&'static str>,
    failures: Vec<String>,
    success: bool,
}

fn observe(effect: OpcodeEffect) -> Observed {
    let mut quest = quest_for(effect);
    quest.tick();
    Observed {
        requests: quest.requests().len(),
        tags: quest.requests().iter().map(request_tag).collect(),
        failures: quest.failures().iter().map(failure_tag).collect(),
        success: quest.succeeded(),
    }
}

fn tags_text(tags: &[&'static str]) -> String {
    if tags.is_empty() {
        "none".to_string()
    } else {
        tags.join("+")
    }
}

fn failures_text(failures: &[String]) -> String {
    if failures.is_empty() {
        "none".to_string()
    } else {
        failures.join("+")
    }
}

// ── A · the 89 variants, measured on the engine (GAP-M42-05) ─────────────

struct BlockA {
    rows: Vec<Row>,
    variants: usize,
    executed: usize,
    negative: usize,
}

fn block_a(walked: &[OpcodeEffect]) -> BlockA {
    let mut rows: Vec<Row> = Vec::new();
    let mut executed = 0usize;
    let mut order_ok = walked.len() == ORACLE.len();

    for (index, (effect, expect)) in ORACLE.iter().enumerate() {
        let observed = observe(*effect);
        if observed.requests > 0 {
            executed += 1;
        }
        let position_ok = walked.get(index) == Some(effect);
        order_ok &= position_ok;
        let detail = format!(
            "engine tick=1 requests={} tags={} failures={} success={} pos={index} src={SRC_QUESTS}",
            observed.requests,
            tags_text(&observed.tags),
            failures_text(&observed.failures),
            observed.success
        );
        let (id, ok) = match *expect {
            Expect::Internal => (
                format!("Q-A-{effect:?}-NEGATIVE"),
                observed.requests == 0
                    && observed.failures.is_empty()
                    && !observed.success
                    && position_ok,
            ),
            Expect::Success => (
                format!("Q-A-{effect:?}"),
                observed.success
                    && observed.tags.as_slice() == ["GiveItemToPc"]
                    && observed.failures.is_empty()
                    && position_ok,
            ),
            Expect::Request(tag) => (
                format!("Q-A-{effect:?}"),
                observed.tags.as_slice() == [tag] && observed.failures.is_empty() && position_ok,
            ),
        };
        rows.push(check(id, ok, detail));
    }

    let variants = walked.len();
    let negative = variants.saturating_sub(executed);
    let internal = ORACLE
        .iter()
        .filter(|entry| matches!(entry.1, Expect::Internal))
        .count();
    let success = ORACLE
        .iter()
        .filter(|entry| matches!(entry.1, Expect::Success))
        .count();
    let request = ORACLE.len() - internal - success;
    // The oracle and the walk must describe the same set, and the engine's
    // observed split must be the oracle's split: no count is a literal.
    let identity = order_ok
        && ORACLE.len() == variants
        && executed == request + success
        && negative == internal
        && executed + negative == variants;
    rows.push(check(
        "Q-A-TOTAL",
        identity,
        format!(
            "want executed={} negative={} oracle=internal {internal}/request {request}/success {success} pos={order_ok} src={SRC_QUESTS}",
            request + success,
            internal
        ),
    ));

    BlockA {
        rows,
        variants,
        executed,
        negative,
    }
}

// ── B · 9 gap triggers via Condition ─────────────────────────────────────

const GAP_TRIGGERS: [OpcodeEffect; 9] = [
    OpcodeEffect::WhenPcEntersExits,
    OpcodeEffect::WhenNpcIsAvailable,
    OpcodeEffect::WhenSkillLevel,
    OpcodeEffect::ClickedItem,
    OpcodeEffect::ClickedFoe,
    OpcodeEffect::TotingItemAndClickedNpc,
    OpcodeEffect::DroppedItemAtPlace,
    OpcodeEffect::Season,
    OpcodeEffect::Climate,
];

fn block_b() -> Vec<Row> {
    let mut rows: Vec<Row> = Vec::new();
    for effect in GAP_TRIGGERS {
        let observed = observe(effect);
        rows.push(check(
            format!("Q-B-{effect:?}"),
            observed.tags.as_slice() == ["Condition"] && observed.failures.is_empty(),
            format!(
                "engine tick=1 tag={} failures={} src={SRC_QUESTS}",
                tags_text(&observed.tags),
                failures_text(&observed.failures)
            ),
        ));
    }
    // FactionReputation was already a Condition before the gap-16 delta.
    let observed = observe(OpcodeEffect::FactionReputation);
    rows.push(check(
        "Q-B-FactionReputation",
        observed.tags.as_slice() == ["Condition"] && observed.failures.is_empty(),
        format!(
            "pre-gap engine Condition tag={} src={SRC_QUESTS}",
            tags_text(&observed.tags)
        ),
    ));
    rows
}

// ── C · 7 gap verbs via owned requests ───────────────────────────────────

const GAP_VERBS: [(OpcodeEffect, &str); 7] = [
    (OpcodeEffect::CastEffectDo, "CastEffectDo"),
    (OpcodeEffect::CastSpellOnFoe, "CastSpellOnFoe"),
    (OpcodeEffect::DialogLink, "DialogLink"),
    (OpcodeEffect::KillFoe, "KillFoe"),
    (OpcodeEffect::PromptMulti, "PromptMulti"),
    (OpcodeEffect::TrainPc, "TrainPc"),
    (OpcodeEffect::WorldUpdate, "WorldUpdate"),
];

fn block_c() -> Vec<Row> {
    let mut rows: Vec<Row> = Vec::new();
    for (effect, name) in GAP_VERBS {
        let observed = observe(effect);
        // Gap verbs are single-tick actions: they complete after one update
        // (`Task.cs:207-209`, `quests.rs` `OpcodeEffect::completes_after_update`).
        let single_tick = effect.completes_after_update();
        rows.push(check(
            format!("Q-C-{name}"),
            observed.tags.as_slice() == [name] && observed.failures.is_empty() && single_tick,
            format!(
                "engine tick=1 own-request tag={} SetComplete={single_tick} src={SRC_QUESTS}",
                tags_text(&observed.tags)
            ),
        ));
    }
    rows
}

// ── D · single_tick execution ────────────────────────────────────────────

fn block_d() -> Vec<Row> {
    let mut rows: Vec<Row> = Vec::new();

    // GivePc: success flag + GiveItemToPc request after one tick.
    let mut quest = quest_for(OpcodeEffect::GivePc);
    quest.tick();
    let give_request = quest
        .requests()
        .iter()
        .any(|r| matches!(r, QuestRequest::GiveItemToPc { .. }));
    rows.push(check(
        "Q-D-GivePc",
        quest.succeeded() && give_request,
        format!("one tick: success + GiveItemToPc src={SRC_QUESTS}"),
    ));

    // SpawnCityGuards: request emitted after one tick, no failure.
    let mut quest = quest_for(OpcodeEffect::SpawnCityGuards);
    quest.tick();
    let guard_request = quest
        .requests()
        .iter()
        .any(|r| matches!(r, QuestRequest::SpawnCityGuards { .. }));
    rows.push(check(
        "Q-D-SpawnCityGuards",
        guard_request && quest.failures().is_empty(),
        format!("one tick: SpawnCityGuards request src={SRC_QUESTS}"),
    ));

    // LogMessage: journal entry after one tick, no organ request.
    let mut quest = quest_for(OpcodeEffect::LogMessage);
    quest.tick();
    rows.push(check(
        "Q-D-LogMessage",
        quest.journal().len() == 1 && quest.requests().is_empty(),
        format!("one tick: journal=1, no organ request src={SRC_QUESTS}"),
    ));

    // DestroyNpc: owned DestroyNpc request after one tick.
    let mut quest = quest_for(OpcodeEffect::DestroyNpc);
    quest.tick();
    let destroy_request = quest
        .requests()
        .iter()
        .any(|r| matches!(r, QuestRequest::DestroyNpc { .. }));
    rows.push(check(
        "Q-D-DestroyNpc",
        destroy_request,
        format!("one tick: DestroyNpc request src={SRC_QUESTS}"),
    ));

    rows
}

// ── E · adversarial (non-goals, negative evidence) ───────────────────────

fn block_e() -> Vec<Row> {
    let mut rows: Vec<Row> = Vec::new();

    // PlaySound never completes on its own (repeats until the task clears).
    rows.push(check(
        "Q-E-PlaySound-NEGATIVE",
        !OpcodeEffect::PlaySound.completes_after_update(),
        format!("NEGATIVE: performs until cleared src={SRC_QUESTS}"),
    ));
    // PcAt never completes on its own (re-evaluates every tick).
    rows.push(check(
        "Q-E-PcAt-NEGATIVE",
        !OpcodeEffect::PcAt.completes_after_update(),
        format!("NEGATIVE: re-evaluates every tick src={SRC_QUESTS}"),
    ));
    // CastSpell is a watch condition, not a cast verb.
    let cast = observe(OpcodeEffect::CastSpell);
    rows.push(check(
        "Q-E-CastSpell-Condition-NEGATIVE",
        cast.tags.as_slice() == ["Condition"],
        format!(
            "NEGATIVE: watch condition, never a verb tag={} src={SRC_QUESTS}",
            tags_text(&cast.tags)
        ),
    ));
    // Internal effects run inside run_action: a LogMessage tick records no
    // UnhandledEffect and emits no request.
    let internal = observe(OpcodeEffect::LogMessage);
    rows.push(check(
        "Q-E-Internal-NoUnhandled-NEGATIVE",
        internal.requests == 0 && internal.failures.is_empty(),
        format!(
            "NEGATIVE: internal path, requests={} failures={} src={SRC_QUESTS}",
            internal.requests,
            failures_text(&internal.failures)
        ),
    ));
    // Owned verbs tick without UnhandledEffect.
    let owned = observe(OpcodeEffect::KillFoe);
    rows.push(check(
        "Q-E-KillFoe-NoUnhandled-NEGATIVE",
        owned.failures.is_empty() && !owned.tags.is_empty(),
        format!(
            "NEGATIVE: owned verb never unhandled tag={} src={SRC_QUESTS}",
            tags_text(&owned.tags)
        ),
    ));
    rows
}

// ── F · the delta is INSTRUMENT_ONLY, its reachability is measured ───────

/// Functions the quest gap added; the only one that is not a request variant.
const DELTA_FUNCTIONS: [&str; 1] = ["QuestRequest::from_effect"];

/// Productive consumers of the delta — measured zero (CR-92028c0c): outside
/// `quests.rs` and the `*_probe.rs` files nothing calls `from_effect`,
/// `execute_opcode` or `QuestMachine`.
const DELTA_CONSUMERS: [&str; 0] = [];

/// The 16 classes of the delta: the 9 trigger conditions + the 7 owned verbs.
fn delta_classes() -> Vec<(OpcodeEffect, &'static str)> {
    let mut classes: Vec<(OpcodeEffect, &'static str)> = GAP_TRIGGERS
        .iter()
        .map(|effect| (*effect, "trigger"))
        .collect();
    classes.extend(GAP_VERBS.iter().map(|(effect, _)| (*effect, "verb")));
    classes
}

/// Covered `type_id`s whose effect is `effect` (`0..=255`, measured).
fn reachable_type_ids(effect: OpcodeEffect) -> Vec<u8> {
    (0..=u8::MAX)
        .filter(|type_id| opcode_coverage(*type_id).and_then(|row| row.effect) == Some(effect))
        .collect()
}

fn type_ids_text(ids: &[u8]) -> String {
    if ids.is_empty() {
        "none".to_string()
    } else {
        ids.iter()
            .map(|type_id| format!("{type_id:#04x}"))
            .collect::<Vec<String>>()
            .join("+")
    }
}

/// `reached=0/16`: no covered `type_id` reaches any class of the delta. The
/// number is measured against `OPCODE_COVERAGE`, never declared.
fn block_f(out: &mut String) {
    let classes = delta_classes();
    let scanned = usize::from(u8::MAX) + 1;
    let reached = classes
        .iter()
        .filter(|(effect, _)| !reachable_type_ids(*effect).is_empty())
        .count();

    out.push_str(&format!(
        "INSTRUMENT_ONLY delta_variants={} delta_requests={} delta_functions={} productive_consumers={} functions={} consumers_measured=CR-92028c0c src={SRC_QUESTS}\n",
        classes.len(),
        GAP_VERBS.len(),
        DELTA_FUNCTIONS.len(),
        DELTA_CONSUMERS.len(),
        DELTA_FUNCTIONS.join(",")
    ));
    out.push_str(&format!(
        "NEW_CLASSES reached={reached}/{} scanned={scanned} rows={} measured=CR-92028c0c src={SPEC_UESP}\n",
        classes.len(),
        OPCODE_COVERAGE.len()
    ));
    for (effect, role) in &classes {
        let ids = reachable_type_ids(*effect);
        out.push_str(&format!(
            "NOTE NEWCLASS {effect:?} reachable={} type_ids={} role={role} src={SPEC_UESP}\n",
            ids.len(),
            type_ids_text(&ids)
        ));
    }
}

fn emit(out: &mut String, rows: &[Row]) -> bool {
    let mut ok = true;
    for row in rows {
        if row.ok {
            out.push_str(&format!("[OK] {} - {}\n", row.id, row.detail));
            ok &= true;
        } else {
            out.push_str(&format!("[FAIL] {} got=fail want=ok - {}\n", row.id, row.detail));
            ok = false;
        }
    }
    ok
}

fn write_block(out: &mut String, title: &str, rows: &[Row]) -> (usize, bool) {
    out.push_str(&format!("\n-- {title} --\n"));
    let ok = emit(out, rows);
    (rows.len(), ok)
}

/// `--probe SYS-QUEST-001`: blocks A–F, `EXIT=0` ⟺ every row OK.
#[allow(dead_code)]
pub fn run_sys_quest_001() -> i32 {
    let (walked, cycle) = walk_effects();
    let unique = unique_count(&walked);
    let variants = walked.len();

    let mut out = String::new();
    out.push_str(&format!(
        "PROBE-SYS-QUEST-001 - {variants} quest variants via from_effect, triggers, verbs, single_tick (M42/W42-5, {CR})\n"
    ));
    out.push_str(&format!("roadmap={SPEC_ROADMAP} src={SRC_QUESTS}\n"));
    out.push_str(&format!(
        "SOURCES sha256={} files={} build=baked paths={} src={SRC_PROBE}\n",
        source_fingerprint(),
        BUILT_SOURCES.len(),
        source_paths()
    ));
    out.push_str(&format!(
        "ENUM variants={variants} walked={variants} unique={unique} cycle={} src={SRC_QUESTS}\n",
        u8::from(cycle)
    ));
    out.push_str(&format!(
        "ORACLE variants={} internal={} request={} success={} src={SRC_QUESTS}\n",
        ORACLE.len(),
        ORACLE
            .iter()
            .filter(|entry| matches!(entry.1, Expect::Internal))
            .count(),
        ORACLE
            .iter()
            .filter(|entry| matches!(entry.1, Expect::Request(_)))
            .count(),
        ORACLE
            .iter()
            .filter(|entry| matches!(entry.1, Expect::Success))
            .count()
    ));
    out.push_str(&format!(
        "GAP triggers={} verbs={} src={SRC_QUESTS}\n",
        GAP_TRIGGERS.len(),
        GAP_VERBS.len()
    ));
    out.push_str(&format!(
        "single_tick={SRC_M29} semantics=one-action-per-update src={SRC_QUESTS}\n"
    ));

    let a = block_a(&walked);
    out.push_str(&format!(
        "COVERAGE variants={} executed={} negative={} src={SRC_QUESTS}\n",
        a.variants, a.executed, a.negative
    ));

    let (count_a, ok_a) = write_block(
        &mut out,
        "A · engine oracle (every variant materialised and ticked once)",
        &a.rows,
    );
    let (count_b, ok_b) = write_block(&mut out, "B · 9 gap triggers via Condition", &block_b());
    let (count_c, ok_c) = write_block(&mut out, "C · 7 gap verbs via owned requests", &block_c());
    let (count_d, ok_d) = write_block(&mut out, "D · single_tick execution", &block_d());
    let (count_e, ok_e) = write_block(
        &mut out,
        "E · adversarial (non-goals, negative evidence)",
        &block_e(),
    );
    out.push_str(&format!(
        "\nBLOCK a={count_a} b={count_b} c={count_c} d={count_d} e={count_e}\n"
    ));

    out.push_str("\n-- F · delta retirement: INSTRUMENT_ONLY, reachability measured --\n");
    block_f(&mut out);

    let ok = ok_a
        && ok_b
        && ok_c
        && ok_d
        && ok_e
        && !cycle
        && a.variants == ORACLE.len()
        && unique == variants;
    out.push_str(if ok {
        "\nVEREDICTO: PASS\n"
    } else {
        "\nVEREDICTO: FAIL\n"
    });
    print!("{out}");
    println!("DIGEST {}", sha256_hex(out.as_bytes()));
    if ok { 0 } else { 1 }
}
