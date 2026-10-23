//! M29 · Playable quests — pure quest machine (SPEC-M29-QUESTS; REQ-L99-062).
//!
//! This module executes the retail `QBN` opcodes parsed in M16. The machine
//! fixes the execution semantics measured in `daggerfall-unity` at
//! `0038a1e6` and leaves the 87-byte record opaque except for the two fields
//! the census measured: the type byte at `+0` and the message id at `+81`.
//! The remaining bytes describe sub-records that the spec does not measure;
//! they are never reinterpreted here (documented NEGATIVE, spec R-29-3).
//!
//! Anchors are `file:line` of the pinned upstream Daggerfall Unity checkout
//! @ `0038a1e6`, vendored here as `reference/dfu-spec/Assets/Scripts/Game/Questing/`.
//!
//! Purity: 0 Bevy, 0 I/O, 0 global RNG, 0 parser dependency. Every roll enters
//! through an injected [`DfRandom`]; effects owned by another organ leave as a
//! typed [`QuestRequest`] (the M28 request pattern) and are never materialised
//! here.
//!
//! Resource tick (`Quest.cs:307-311`) materialises as the tick of [`Quest::clocks`]:
//! a `Clock` **is** a `QuestResource` in DFU (`Clock.cs:30`), so that loop is
//! exactly the clock step. Every
//! other resource (foes, NPCs, items, places) is owned by M22/M26/M27/M30 and
//! is represented here only as a [`QuestRequest`]; this organ never ticks it.
//! The 87-byte record stays opaque outside `+0` (type) and `+81` (message id).
//!
//! Task birth state follows `Task.cs:181-185`: a headerless task is
//! [`TaskType::Headless`] and starts `IsTriggered = true`, while the other
//! four kinds start untriggered. Completion follows `Task.cs:207-209` ("the
//! action itself should decide"): [`OpcodeEffect::completes_after_update`]
//! marks the classes that never call `SetComplete` (`PlaySound`, `PcAt`) and
//! therefore re-run every tick. [`Quest`] also carries the local/global
//! boolean variables (`Task.cs:107-114`), the UID and tombstone fields
//! (`Quest.cs:119-122,230-242`) and a drainable display mailbox
//! (`Quest.cs:847-854`).

use std::collections::BTreeMap;

pub use crate::determinism::DfRandom;

/// Quest-machine tick rate: `ticksPerSecond = 10`
/// (`QuestMachine.cs:45`, throttle `:318-321`).
pub const TICKS_PER_SECOND: u32 = 10;

/// Opcode record length in section 8: `QBN_OPCODE_LEN = 87`
/// (`crates/daggerfall_parser_rs/src/quest.rs:52`; header sizes `:56`).
pub const QBN_OPCODE_LEN: usize = 87;

/// Type byte inside an opcode record (spec 2.1 step 3).
pub const QBN_OPCODE_TYPE_OFFSET: usize = 0;

/// Message id (`u16` LE) inside an opcode record (spec 2.5).
pub const QBN_OPCODE_MESSAGE_OFFSET: usize = 81;

/// Sentinel message id: 5547 of 6407 retail opcodes carry it (spec 2.5).
pub const NO_MESSAGE_ID: u16 = 0xFFFF;

/// `ticksToEnd = 2` when `EndQuest()` is called (`Quest.cs:373`).
pub const END_TICKS_TO_END: u32 = 2;

/// `QuestSuccessRep = 5` (`Quest.cs:36`, applied `:382-386`).
pub const SUCCESS_REPUTATION: i16 = 5;

/// `QuestFailureRep = -2` (`Quest.cs:37`, applied `:382-386`).
pub const FAILURE_REPUTATION: i16 = -2;

/// No time value: `FromRange(GetTimeInSeconds(0,0,1), GetTimeInSeconds(7,0,0))`
/// (`Clock.cs:189-198`).
pub const CLOCK_MIN_SECONDS: i64 = 60;

/// Upper bound of the default clock range (7 days, `Clock.cs:189-198`).
pub const CLOCK_MAX_SECONDS: i64 = 604_800;

/// `returnTripMultiplier = 2.5f` for clock flag `&16` (`Clock.cs:32`, applied
/// to the summed one-way time at `:422-436`). M29 only multiplies the one-way
/// seconds a caller supplies in [`ClockRecipe::ReturnTrip`]; resolving the
/// route is M31.
pub const RETURN_TRIP_MULTIPLIER: f32 = 2.5;

/// Seconds in a day: `GetDaysString` divides by `86400` (`Clock.cs:349-355`).
pub const SECONDS_PER_DAY: i64 = 86_400;

/// First step id reserved for `JournalNote` entries. DFU stores notes in the
/// Notebook without a `stepID` (`Actions/JournalNote.cs:50-52`) while log
/// entries carry one (`Quest.cs:52`, `Quest.AddLogStep`). The target keeps
/// both in [`Quest::journal`] and separates them at close: note ids start at
/// this base so they never collide with log steps.
pub const NOTE_STEP_BASE: u32 = 0x8000_0000;

/// Semantic class of one opcode type. Each variant names a real
/// `ActionTemplate` under `Assets/Scripts/Game/Questing/Actions/`; the anchor
/// is repeated in the coverage table so a type can be audited without reading
/// this enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpcodeEffect {
    /// `log <id> step <step>` adds a journal entry
    /// (`Actions/LogMessage.cs:23`, `SetComplete` `:69`).
    LogMessage,
    /// `journal note <id>` adds a free note (`Actions/JournalNote.cs:20`).
    JournalNote,
    /// Message popup; the quest breaks its current tick
    /// (`Actions/Say.cs:21`, `Quest.cs:220-224`).
    DisplayMessage,
    /// Removes a previously logged entry (`Actions/RemoveLogMessage.cs:23`).
    RemoveLogMessage,
    /// Terminates the quest after two ticks (`Actions/EndQuest.cs:20`,
    /// `Quest.cs:367-400`).
    EndQuest,
    /// `clear task` unsets a task (`Actions/ClearTask.cs:23`).
    ClearTask,
    /// `start task` triggers a task (`Actions/StartTask.cs:22`).
    StartTask,
    /// `unset task` clears a task (`Actions/UnsetTask.cs:21`).
    UnsetTask,
    /// Starts a named clock (`Actions/StartStopTimer.cs:21`,
    /// `Clock.cs:309-341`).
    StartTimer,
    /// Stops a named clock (`Actions/StartStopTimer.cs:21`).
    StopTimer,
    /// `give pc` marks success and offers the reward
    /// (`Actions/GivePc.cs:27`, success at `:152`).
    GivePc,
    /// Gives an item to the player without closing the quest: the
    /// direct-to-player paths of `give pc` (`Actions/GivePc.cs:175-186`)
    /// never touch `QuestSuccess` (only `:152` does).
    GiveItem,
    /// Creates a foe (`Actions/CreateFoe.cs:24`).
    CreateFoe,
    /// Places a foe at a location (`Actions/PlaceFoe.cs:20`).
    PlaceFoe,
    /// Creates an NPC (`Actions/CreateNpc.cs:24`).
    CreateNpc,
    /// Creates an NPC at a place (`Actions/CreateNpcAt.cs:28`).
    CreateNpcAt,
    /// Places an NPC (`Actions/PlaceNpc.cs:26`).
    PlaceNpc,
    /// Places an item (`Actions/PlaceItem.cs:21`).
    PlaceItem,
    /// Gives an item to an NPC (`Actions/GiveItem.cs:19`).
    GiveItemToNpc,
    /// Takes an item from the player (`Actions/TakeItem.cs:21`).
    TakeItem,
    /// Takes gold from the player (`Actions/PayMoney.cs:19`).
    TakeGold,
    /// Pays gold to the player (`Actions/GivePc.cs:27`; DFU models `give pc`
    /// as item-or-nothing, so this row is the gold variant of that verb).
    GiveGold,
    /// Teleports the player (`Actions/TeleportPc.cs:28`).
    TeleportPc,
    /// Reveals a location on the map (`Actions/RevealLocation.cs:20`).
    RevealLocation,
    /// Adds a location to the global map (`Actions/RevealLocation.cs:20`).
    RevealMap,
    /// Plays a sound once (`Actions/PlaySound.cs:20`).
    PlaySound,
    /// Plays a song (`Actions/PlaySong.cs:22`).
    PlaySong,
    /// Plays a movie (`Actions/PlayVideo.cs:23`).
    PlayVideo,
    /// Starts another quest (`Actions/StartQuest.cs:25`).
    StartQuest,
    /// Runs another quest immediately (`Actions/RunQuest.cs:22`).
    RunQuest,
    /// Changes reputation with a faction (`Actions/ChangeReputeWith.cs:20`).
    ChangeReputation,
    /// Sets the player crime (`Actions/SetPlayerCrime.cs:22`, M28).
    SetPlayerCrime,
    /// Spawns city guards (`Actions/SpawnCityGuards.cs:20`, M26).
    SpawnCityGuards,
    /// Changes the regional legal reputation (`Actions/LegalRepute.cs:20`,
    /// M28).
    LegalRepute,
    /// Infects the player (`Actions/MakePcDiseased.cs:24`, M32).
    MakeDiseased,
    /// Cures all diseases (`Actions/CurePcDisease.cs:24`, M32).
    CureDisease,
    /// Watches the player readying/casting a spell: the class is a condition
    /// ("executes target task when player readies a spell containing specific
    /// effects", `Actions/CastSpellDo.cs:23-27`), not a cast verb.
    CastSpell,
    /// Enables a dialogue topic (`Actions/AddDialog.cs:20`, M26).
    AddTopic,
    /// Disables a dialogue topic; DFU has no inverse class (0 hits), topics
    /// close with the quest (`Actions/AddDialog.cs:20` is the topic verb).
    RemoveTopic,
    /// Adds a message to the rumor mill (`Actions/RumorMill.cs:20`, M26).
    Rumor,
    /// Requests a travel time; clock flag `&16` (`Clock.cs:215-226`, M31).
    TravelTime,
    /// Shows an NPC face (`Actions/AddFace.cs:20`).
    AddFace,
    /// Removes an NPC face (`Actions/DropFace.cs:20`).
    DropFace,
    /// Shows a yes/no prompt (`Actions/Prompt.cs:22`).
    ShowPrompt,
    /// Makes an NPC the questor (`Actions/AddAsQuestor.cs:20`).
    AddQuestor,
    /// Stops an NPC being the questor (`Actions/DropAsQuestor.cs:20`).
    DropQuestor,
    /// Makes a quest item ordinary (`Actions/MakePermanent.cs:21`).
    MakePermanent,
    /// Overrides regional weather (`Actions/Weather.cs:23`, M31).
    WeatherOverride,
    /// Checks inventory (`Actions/HaveItem.cs:22`).
    HaveItem,
    /// Checks the player is at a location (`Actions/PcAt.cs:26`).
    PcAt,
    /// Counts kills (`Actions/KilledFoe.cs:20`).
    KillCount,
    /// Checks a foe was hurt (`Actions/InjuredFoe.cs:24`).
    InjuredFoe,
    /// Checks an item was found (`Actions/GetItem.cs:24`).
    ItemFound,
    /// Checks an item was used (`Actions/ItemUsedDo.cs:21`).
    ItemUsed,
    /// Checks reputation (`Actions/ReputeExceedsDo.cs:22`).
    ReputeExceeds,
    /// AND of states (`Actions/WhenTask.cs:24`).
    StateAnd,
    /// OR of states (`Actions/WhenTask.cs:24`).
    StateOr,
    /// Chooses one state at random (`Actions/PickOneOf.cs:22`).
    PickOneOf,
    /// Deletes an NPC (`Actions/DestroyNpc.cs:22`).
    DestroyNpc,
    /// Hides an NPC (`Actions/HideNpc.cs:23`).
    HideNpc,
    /// Restores a hidden NPC (`Actions/RestoreNpc.cs:23`).
    RestoreNpc,
    /// Stops an NPC from talking (`Actions/MuteNpc.cs:20`).
    MuteNpc,
    /// Changes foe team (`Actions/ChangeFoeTeam.cs:22`).
    ChangeFoeTeam,
    /// Changes foe infighting (`Actions/ChangeFoeInfighting.cs:22`).
    ChangeFoeInfighting,
    /// Restrains a foe (`Actions/RestrainFoe.cs:21`).
    RestrainFoe,
    /// Releases a restrained foe (`Actions/UnrestrainFoe.cs:20`).
    UnrestrainFoe,
    /// Spawns enemies (`Actions/Enemies.cs:20`).
    Enemies,
    /// Hides a foe resource, ending its part in the quest
    /// (`Actions/RemoveFoe.cs:18-24`, `foe.IsHidden = true` at `:61`).
    RemoveFoe,
    /// Checks an NPC was clicked (`Actions/ClickedNpc.cs:24`).
    ClickedNpc,
    /// Daily clock window (`Actions/DailyFrom.cs:26`).
    DailyFrom,
    /// Attribute check (`Actions/WhenAttributeLevel.cs:22`).
    AttributeLevel,
    /// Player level gate: `level <n> completed` (`Actions/LevelCompleted.cs:20`,
    /// `CheckTrigger` at `:49-52` compares `PlayerEntity.Level`).
    LevelCompleted,
    /// Faction reputation check (`Actions/WhenReputeWith.cs:24`).
    FactionReputation,
}

impl OpcodeEffect {
    /// `Task.cs:207-209`: "the action itself should decide if/when to be
    /// complete". Every class that calls `SetComplete()` at the end of its
    /// `Update` completes after one run; the two measured exceptions keep
    /// running until the task is cleared:
    ///
    /// - [`OpcodeEffect::PlaySound`] — `Actions/PlaySound.cs:93-115` repeats
    ///   by `interval`/`count` and never calls `SetComplete` ("the play sound
    ///   command performs until task is cleared").
    /// - [`OpcodeEffect::PcAt`] — `Actions/PcAt.cs:106-144` re-evaluates the
    ///   place every tick (start/clear the target task) and never completes.
    ///
    /// The condition-only classes (`KilledFoe`, `InjuredFoe`, `ClickedNpc`,
    /// `DailyFrom`, `WhenReputeWith`, `WhenTask`, `Weather`,
    /// `LevelCompleted`) do not override `Update` at all — they only answer
    /// `CheckTrigger` — so they are irrelevant here; the default `true` is
    /// what an action that reached the end of its `Update` observes.
    pub fn completes_after_update(self) -> bool {
        !matches!(self, Self::PlaySound | Self::PcAt)
    }
}

/// One row of the type coverage table (spec 2.4).
#[derive(Debug, Clone, Copy)]
pub struct OpcodeCoverage {
    /// Type byte at `+0`.
    pub type_id: u8,
    /// Label of the UESP pseudo-code catalogue (target language).
    pub uesp_label: &'static str,
    /// Semantic class, or `None` for a NEGATIVE row (type 0 and the six
    /// UESP-labelled unknowns).
    pub effect: Option<OpcodeEffect>,
    /// `Actions/<Class>.cs:<line>` for a covered type; for NEGATIVE rows the
    /// search terms that establish the absence.
    pub anchor: &'static str,
}

const NEGATIVE_TYPE_ZERO: &str = "UESP:Quest_pseudo-codes: no §3 entry for type 0; git grep 'type 0' Actions/ = 0";
const NEGATIVE_UNKNOWN: &str = "UESP:Quest_pseudo-codes Unknown Types; git grep 'Unknown' Actions/ = 0";

/// 70 rows: every UESP type in the pack table (69 used in retail plus
/// `0x3E`, documented and unused). `covered_opcode_count()` counts the 62
/// documented types **used** in retail; `negative_opcode_count()` counts the
/// seven NEGATIVE rows (type 0 plus six labelled unknowns).
pub const OPCODE_COVERAGE: [OpcodeCoverage; 70] = [
    OpcodeCoverage { type_id: 0x00, uesp_label: "Item & Location", effect: None, anchor: NEGATIVE_TYPE_ZERO },
    OpcodeCoverage { type_id: 0x01, uesp_label: "Item & NPC", effect: Some(OpcodeEffect::GiveItemToNpc), anchor: "Actions/GiveItem.cs:19" },
    OpcodeCoverage { type_id: 0x02, uesp_label: "Check Kill Count", effect: Some(OpcodeEffect::KillCount), anchor: "Actions/KilledFoe.cs:20" },
    OpcodeCoverage { type_id: 0x03, uesp_label: "PC finds Item", effect: Some(OpcodeEffect::ItemFound), anchor: "Actions/GetItem.cs:24" },
    OpcodeCoverage { type_id: 0x04, uesp_label: "Items", effect: Some(OpcodeEffect::GiveItem), anchor: "Actions/GivePc.cs:175 (item to player; success only at `:152`, so this row never closes the quest)" },
    OpcodeCoverage { type_id: 0x05, uesp_label: "Unknown", effect: None, anchor: NEGATIVE_UNKNOWN },
    OpcodeCoverage { type_id: 0x06, uesp_label: "States", effect: Some(OpcodeEffect::EndQuest), anchor: "Actions/EndQuest.cs:20" },
    OpcodeCoverage { type_id: 0x07, uesp_label: "States", effect: Some(OpcodeEffect::ClearTask), anchor: "Actions/ClearTask.cs:23" },
    OpcodeCoverage { type_id: 0x08, uesp_label: "Quest", effect: Some(OpcodeEffect::StartQuest), anchor: "Actions/StartQuest.cs:25" },
    OpcodeCoverage { type_id: 0x09, uesp_label: "Repeating Spawn", effect: Some(OpcodeEffect::CreateFoe), anchor: "Actions/CreateFoe.cs:24" },
    OpcodeCoverage { type_id: 0x0A, uesp_label: "Add Topics", effect: Some(OpcodeEffect::AddTopic), anchor: "Actions/AddDialog.cs:20" },
    OpcodeCoverage { type_id: 0x0B, uesp_label: "Remove Topics", effect: Some(OpcodeEffect::RemoveTopic), anchor: "Actions/AddDialog.cs:20" },
    OpcodeCoverage { type_id: 0x0C, uesp_label: "Start timer", effect: Some(OpcodeEffect::StartTimer), anchor: "Actions/StartStopTimer.cs:21" },
    OpcodeCoverage { type_id: 0x0D, uesp_label: "Stop timer", effect: Some(OpcodeEffect::StopTimer), anchor: "Actions/StartStopTimer.cs:21" },
    OpcodeCoverage { type_id: 0x11, uesp_label: "Locations", effect: Some(OpcodeEffect::RevealMap), anchor: "UESP 0x11 'Locations' Role Unknown (no class backs it); minimal observable = map petition `Actions/RevealLocation.cs:20`, no state invented" },
    OpcodeCoverage { type_id: 0x13, uesp_label: "Add Location to Map", effect: Some(OpcodeEffect::RevealMap), anchor: "Actions/RevealLocation.cs:20" },
    OpcodeCoverage { type_id: 0x15, uesp_label: "Mob hurt by PC", effect: Some(OpcodeEffect::InjuredFoe), anchor: "Actions/InjuredFoe.cs:24" },
    OpcodeCoverage { type_id: 0x16, uesp_label: "Place Mob at Location", effect: Some(OpcodeEffect::PlaceFoe), anchor: "Actions/PlaceFoe.cs:20" },
    OpcodeCoverage { type_id: 0x17, uesp_label: "Create Log Entry", effect: Some(OpcodeEffect::LogMessage), anchor: "Actions/LogMessage.cs:23" },
    OpcodeCoverage { type_id: 0x18, uesp_label: "Remove Log", effect: Some(OpcodeEffect::RemoveLogMessage), anchor: "Actions/RemoveLogMessage.cs:23" },
    OpcodeCoverage { type_id: 0x1A, uesp_label: "Give Item to NPC", effect: Some(OpcodeEffect::GiveItemToNpc), anchor: "Actions/GiveItem.cs:19" },
    OpcodeCoverage { type_id: 0x1B, uesp_label: "Add Global Map", effect: Some(OpcodeEffect::RevealMap), anchor: "Actions/RevealLocation.cs:20" },
    OpcodeCoverage { type_id: 0x1C, uesp_label: "PC meets NPC", effect: Some(OpcodeEffect::ClickedNpc), anchor: "Actions/ClickedNpc.cs:24" },
    OpcodeCoverage { type_id: 0x1D, uesp_label: "Yes/No Question", effect: Some(OpcodeEffect::ShowPrompt), anchor: "Actions/Prompt.cs:22" },
    OpcodeCoverage { type_id: 0x1E, uesp_label: "NPC, Location", effect: Some(OpcodeEffect::PlaceNpc), anchor: "Actions/PlaceNpc.cs:26" },
    OpcodeCoverage { type_id: 0x1F, uesp_label: "Daily Clock", effect: Some(OpcodeEffect::DailyFrom), anchor: "Actions/DailyFrom.cs:26" },
    OpcodeCoverage { type_id: 0x22, uesp_label: "Random State", effect: Some(OpcodeEffect::PickOneOf), anchor: "Actions/PickOneOf.cs:22" },
    OpcodeCoverage { type_id: 0x23, uesp_label: "Cycle state", effect: Some(OpcodeEffect::PickOneOf), anchor: "Actions/PickOneOf.cs:19-22 declares the gap: DFU has no cycle verb (UESP 0x23 'cycle' would clear the current state and activate the next; grep -i 'class .*Cycle' Actions/ = 0), so the nearest class `PickOneOf` (random pick) is used and no wrap-around state is invented" },
    OpcodeCoverage { type_id: 0x24, uesp_label: "Give Item to PC", effect: Some(OpcodeEffect::GiveItem), anchor: "Actions/GivePc.cs:175 (item to player; success only at `:152`, so this row never closes the quest)" },
    OpcodeCoverage { type_id: 0x25, uesp_label: "Item", effect: Some(OpcodeEffect::ReputeExceeds), anchor: "Actions/WhenReputeWith.cs:24 (UESP: set State2 when NPC reputation >= Value; DFU trigger `when repute with … is at least`)" },
    OpcodeCoverage { type_id: 0x26, uesp_label: "Rumors", effect: Some(OpcodeEffect::Rumor), anchor: "Actions/RumorMill.cs:20" },
    OpcodeCoverage { type_id: 0x27, uesp_label: "Item and Mob", effect: Some(OpcodeEffect::GiveItemToNpc), anchor: "Actions/GiveItem.cs:19" },
    OpcodeCoverage { type_id: 0x2B, uesp_label: "PC at Location", effect: Some(OpcodeEffect::PcAt), anchor: "Actions/PcAt.cs:26" },
    OpcodeCoverage { type_id: 0x2C, uesp_label: "Delete NPC", effect: Some(OpcodeEffect::DestroyNpc), anchor: "Actions/DestroyNpc.cs:22" },
    OpcodeCoverage { type_id: 0x2E, uesp_label: "Hide NPC", effect: Some(OpcodeEffect::HideNpc), anchor: "Actions/HideNpc.cs:23" },
    OpcodeCoverage { type_id: 0x30, uesp_label: "Show NPC", effect: Some(OpcodeEffect::RestoreNpc), anchor: "Actions/RestoreNpc.cs:23" },
    OpcodeCoverage { type_id: 0x31, uesp_label: "Cure disease", effect: Some(OpcodeEffect::CureDisease), anchor: "Actions/CurePcDisease.cs:24" },
    OpcodeCoverage { type_id: 0x32, uesp_label: "Play movie", effect: Some(OpcodeEffect::PlayVideo), anchor: "Actions/PlayVideo.cs:23" },
    OpcodeCoverage { type_id: 0x33, uesp_label: "Display Message", effect: Some(OpcodeEffect::DisplayMessage), anchor: "Actions/Say.cs:21" },
    OpcodeCoverage { type_id: 0x34, uesp_label: "AND States", effect: Some(OpcodeEffect::StateAnd), anchor: "Actions/WhenTask.cs:24" },
    OpcodeCoverage { type_id: 0x35, uesp_label: "OR States", effect: Some(OpcodeEffect::StateOr), anchor: "Actions/WhenTask.cs:24" },
    OpcodeCoverage { type_id: 0x36, uesp_label: "Make Item ordinary", effect: Some(OpcodeEffect::MakePermanent), anchor: "Actions/MakePermanent.cs:21" },
    OpcodeCoverage { type_id: 0x37, uesp_label: "Escort NPC", effect: Some(OpcodeEffect::AddFace), anchor: "Actions/AddFace.cs:20" },
    OpcodeCoverage { type_id: 0x38, uesp_label: "End Escort NPC", effect: Some(OpcodeEffect::DropFace), anchor: "Actions/DropFace.cs:20" },
    OpcodeCoverage { type_id: 0x39, uesp_label: "Use Item", effect: Some(OpcodeEffect::ItemUsed), anchor: "Actions/ItemUsedDo.cs:21" },
    OpcodeCoverage { type_id: 0x3A, uesp_label: "Cure Vampirism", effect: Some(OpcodeEffect::CureDisease), anchor: "Actions/CurePcDisease.cs:24" },
    OpcodeCoverage { type_id: 0x3B, uesp_label: "Cure Lycanthropy", effect: Some(OpcodeEffect::CureDisease), anchor: "Actions/CurePcDisease.cs:24" },
    OpcodeCoverage { type_id: 0x3C, uesp_label: "Play Sound", effect: Some(OpcodeEffect::PlaySound), anchor: "Actions/PlaySound.cs:20" },
    OpcodeCoverage { type_id: 0x3D, uesp_label: "Reputation", effect: Some(OpcodeEffect::ChangeReputation), anchor: "Actions/ChangeReputeWith.cs:20" },
    OpcodeCoverage { type_id: 0x3E, uesp_label: "Weather Override", effect: Some(OpcodeEffect::WeatherOverride), anchor: "Actions/Weather.cs:23" },
    OpcodeCoverage { type_id: 0x3F, uesp_label: "Unknown", effect: None, anchor: NEGATIVE_UNKNOWN },
    OpcodeCoverage { type_id: 0x40, uesp_label: "Unknown", effect: None, anchor: NEGATIVE_UNKNOWN },
    OpcodeCoverage { type_id: 0x41, uesp_label: "Legal Reputation", effect: Some(OpcodeEffect::LegalRepute), anchor: "Actions/LegalRepute.cs:20" },
    OpcodeCoverage { type_id: 0x44, uesp_label: "Mob", effect: Some(OpcodeEffect::RemoveFoe), anchor: "Actions/RemoveFoe.cs:18 (UESP: done with Mob; hides the foe resource at `:61`, distinct from the kill count of 0x02)" },
    OpcodeCoverage { type_id: 0x45, uesp_label: "Mob", effect: Some(OpcodeEffect::Enemies), anchor: "Actions/Enemies.cs:20 declares the gap: UESP sets a foe power level, DFU has no power-level class (grep PowerLevel Questing/ = 0); nearest foe command only" },
    OpcodeCoverage { type_id: 0x46, uesp_label: "PC has Items", effect: Some(OpcodeEffect::HaveItem), anchor: "Actions/HaveItem.cs:22" },
    OpcodeCoverage { type_id: 0x47, uesp_label: "Take PC Gold", effect: Some(OpcodeEffect::TakeGold), anchor: "Actions/PayMoney.cs:19" },
    OpcodeCoverage { type_id: 0x48, uesp_label: "Unknown", effect: None, anchor: NEGATIVE_UNKNOWN },
    OpcodeCoverage { type_id: 0x49, uesp_label: "PC casts Spell", effect: Some(OpcodeEffect::CastSpell), anchor: "Actions/CastSpellDo.cs:25 (watch condition on the player readying a spell; no cast verb in the class)" },
    OpcodeCoverage { type_id: 0x4C, uesp_label: "Give Item to PC", effect: Some(OpcodeEffect::GiveItem), anchor: "Actions/GivePc.cs:182 (`give pc <item> notify <id>`: item to player, then the message; success only at `:152`)" },
    OpcodeCoverage { type_id: 0x4D, uesp_label: "Check Player Level", effect: Some(OpcodeEffect::LevelCompleted), anchor: "Actions/LevelCompleted.cs:20 (`level <n> completed`, `PlayerEntity.Level` at `:51`): DFU has no generic player-level trigger, so the gap is declared here" },
    OpcodeCoverage { type_id: 0x4E, uesp_label: "Unknown", effect: None, anchor: NEGATIVE_UNKNOWN },
    OpcodeCoverage { type_id: 0x4F, uesp_label: "Check Faction Reputation", effect: Some(OpcodeEffect::FactionReputation), anchor: "Actions/WhenReputeWith.cs:24 (trigger `when repute with … is at least`; the faction id is resolved by the owning organ, DFU has no faction-only trigger)" },
    OpcodeCoverage { type_id: 0x51, uesp_label: "Locations, NPC", effect: Some(OpcodeEffect::CreateNpcAt), anchor: "Actions/CreateNpcAt.cs:28 (UESP GW: Unknown, 'maybe create them'; the row claims only the create-at verb)" },
    OpcodeCoverage { type_id: 0x52, uesp_label: "Stop NPC talk", effect: Some(OpcodeEffect::MuteNpc), anchor: "Actions/MuteNpc.cs:20" },
    OpcodeCoverage { type_id: 0x53, uesp_label: "Location", effect: Some(OpcodeEffect::RevealLocation), anchor: "UESP 0x53 'Location' Role Unknown (no class backs it); minimal observable = reveal petition `Actions/RevealLocation.cs:20`, no state invented" },
    OpcodeCoverage { type_id: 0x54, uesp_label: "Play Sound", effect: Some(OpcodeEffect::PlaySound), anchor: "Actions/PlaySound.cs:20" },
    OpcodeCoverage { type_id: 0x55, uesp_label: "Choose Questor", effect: Some(OpcodeEffect::AddQuestor), anchor: "Actions/AddAsQuestor.cs:20" },
    OpcodeCoverage { type_id: 0x56, uesp_label: "End Questor", effect: Some(OpcodeEffect::DropQuestor), anchor: "Actions/DropAsQuestor.cs:20" },
    OpcodeCoverage { type_id: 0x57, uesp_label: "Unknown", effect: None, anchor: NEGATIVE_UNKNOWN },
];

/// Coverage row for a type byte (first match; the table has unique types).
pub fn opcode_coverage(type_id: u8) -> Option<&'static OpcodeCoverage> {
    OPCODE_COVERAGE.iter().find(|row| row.type_id == type_id)
}

/// Documented types used in retail, excluding `0x3E` (documented, 0 uses:
/// spec 2.3, table row 62) and the seven NEGATIVE rows. Equals 62.
pub fn covered_opcode_count() -> usize {
    let mut seen = [false; 256];
    let mut count = 0;
    for row in &OPCODE_COVERAGE {
        if row.type_id == 0x3E {
            continue;
        }
        let index = usize::from(row.type_id);
        if row.effect.is_some() && !seen[index] {
            seen[index] = true;
            count += 1;
        }
    }
    count
}

/// Unique rows without semantics: type 0 plus the six UESP-labelled unknowns.
/// Equals 7.
pub fn negative_opcode_count() -> usize {
    let mut seen = [false; 256];
    let mut count = 0;
    for row in &OPCODE_COVERAGE {
        let index = usize::from(row.type_id);
        if row.effect.is_none() && !seen[index] {
            seen[index] = true;
            count += 1;
        }
    }
    count
}

/// Decoded fields of one 87-byte opcode record. Everything outside `+0`
/// (type) and `+81` (message id) stays opaque: the spec measures exactly
/// those two fields (spec 2.1 step 3, 2.5) and the sub-records are not
/// reinterpreted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DecodedOpcode {
    /// Type byte at `+0`.
    pub type_id: u8,
    /// Message id (`u16` LE) at `+81`; [`NO_MESSAGE_ID`] means none.
    pub message_id: u16,
    /// `true` when the type has a semantic row (spec 2.4).
    pub supported: bool,
}

/// Decodes type and message id from an opcode record. Returns `None` when the
/// record is not exactly [`QBN_OPCODE_LEN`] bytes.
pub fn decode_opcode(record: &[u8]) -> Option<DecodedOpcode> {
    if record.len() != QBN_OPCODE_LEN {
        return None;
    }
    let type_id = record[QBN_OPCODE_TYPE_OFFSET];
    let message_id = u16::from_le_bytes([
        record[QBN_OPCODE_MESSAGE_OFFSET],
        record[QBN_OPCODE_MESSAGE_OFFSET + 1],
    ]);
    let supported = opcode_coverage(type_id).is_some_and(|row| row.effect.is_some());
    Some(DecodedOpcode {
        type_id,
        message_id,
        supported,
    })
}

/// Typed request to the organ that owns an effect crossing the quest
/// boundary. Payload fields are `Option` because the opcode record keeps its
/// sub-records opaque: the machine knows the semantic class and the message
/// id, never a resource index it did not measure. The owning organ resolves
/// the referent from the quest context (M22/M23/M26/M27/M28/M30/M31/M32).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QuestRequest {
    /// Spawn a foe (`Actions/CreateFoe.cs:24`, M22).
    SpawnFoe { foe: Option<String> },
    /// Place a foe (`Actions/PlaceFoe.cs:20`, M22).
    PlaceFoe { foe: Option<String> },
    /// Create an NPC (`Actions/CreateNpc.cs:24`, M26).
    CreateNpc { npc: Option<String> },
    /// Create an NPC at a place (`Actions/CreateNpcAt.cs:28`, M26).
    CreateNpcAt { npc: Option<String> },
    /// Place an NPC (`Actions/PlaceNpc.cs:26`, M26).
    PlaceNpc { npc: Option<String> },
    /// Place an item (`Actions/PlaceItem.cs:21`, M30).
    PlaceItem { item: Option<String> },
    /// Give an item to an NPC (`Actions/GiveItem.cs:19`, M26).
    GiveItemToNpc { item: Option<String> },
    /// Give an item to the player (`Actions/GivePc.cs:27`, M30).
    GiveItemToPc { item: Option<String> },
    /// Take an item (`Actions/TakeItem.cs:21`, M30).
    TakeItem { item: Option<String> },
    /// Debit gold (`Actions/PayMoney.cs:19`, M30).
    TakeGold { amount: Option<u32> },
    /// Credit gold (`Actions/GivePc.cs:27`, M30).
    GiveGold { amount: Option<u32> },
    /// Teleport the player (`Actions/TeleportPc.cs:28`, M31).
    TeleportPc { destination: Option<String> },
    /// Reveal a location (`Actions/RevealLocation.cs:20`, M31).
    RevealLocation { location: Option<String> },
    /// Add a location to the map (`Actions/RevealLocation.cs:20`, M31).
    RevealMap { location: Option<String> },
    /// Play a sound (`Actions/PlaySound.cs:20`, perception).
    PlaySound { sound_id: Option<u16> },
    /// Play a song (`Actions/PlaySong.cs:22`, perception).
    PlaySong { song_id: Option<u16> },
    /// Play a movie (`Actions/PlayVideo.cs:23`, perception).
    PlayVideo { movie_id: Option<u16> },
    /// Start another quest (`Actions/StartQuest.cs:25`, M29).
    StartQuest { quest_name: Option<String> },
    /// Change reputation; `propagate` is true only for the close rule
    /// (`Quest.cs:382-386`, M27).
    ChangeReputation {
        faction_id: Option<u16>,
        delta: Option<i16>,
        propagate: bool,
    },
    /// Set the player crime (`Actions/SetPlayerCrime.cs:22`, M28).
    SetPlayerCrime { crime: Option<u8> },
    /// Spawn city guards (`Actions/SpawnCityGuards.cs:20`, M26).
    SpawnCityGuards { count: Option<u8> },
    /// Change the regional legal reputation (`Actions/LegalRepute.cs:20`, M28).
    LegalRepute { delta: Option<i16> },
    /// Infect the player (`Actions/MakePcDiseased.cs:24`, M32).
    MakeDiseased { disease: Option<String> },
    /// Cure diseases (`Actions/CurePcDisease.cs:24`, M32).
    CureDisease,
    /// Casts a spell (`Actions/CastSpellDo.cs:25`, M23). The opcode table
    /// never emits it: `CastSpellDo` is a watch condition, so type `0x49`
    /// leaves as [`QuestRequest::Condition`]; the variant stays for direct
    /// callers that own the cast verb.
    CastSpell { spell_id: Option<u16> },
    /// Add a dialogue topic (`Actions/AddDialog.cs:20`, M26).
    AddTopic { topic: Option<String> },
    /// Remove a dialogue topic (`Actions/AddDialog.cs:20`, M26).
    RemoveTopic { topic: Option<String> },
    /// Add a rumor (`Actions/RumorMill.cs:20`, M26).
    Rumor { message_id: u16 },
    /// Travel time for clock flag `&16` (`Clock.cs:215-226`, M31).
    TravelTime {
        seconds: Option<i64>,
        flag: Option<u8>,
    },
    /// Show an NPC face (`Actions/AddFace.cs:20`, perception).
    ShowFace { npc: Option<String> },
    /// Remove an NPC face (`Actions/DropFace.cs:20`, perception).
    HideFace { npc: Option<String> },
    /// Show a prompt (`Actions/Prompt.cs:22`, perception).
    Prompt { message_id: u16 },
    /// Make an NPC the questor (`Actions/AddAsQuestor.cs:20`, M26).
    AddQuestor { npc: Option<String> },
    /// Stop an NPC being the questor (`Actions/DropAsQuestor.cs:20`, M26).
    DropQuestor { npc: Option<String> },
    /// Make a quest item ordinary (`Actions/MakePermanent.cs:21`, M30).
    MakePermanent { item: Option<String> },
    /// Override regional weather (`Actions/Weather.cs:23`, M31).
    WeatherOverride {
        region_id: Option<u16>,
        weather: Option<u8>,
    },
    /// Delete an NPC (`Actions/DestroyNpc.cs:22`, M26).
    DestroyNpc { npc: Option<String> },
    /// Hide or restore an NPC (`Actions/HideNpc.cs:23`,
    /// `Actions/RestoreNpc.cs:23`, M26).
    HideNpc { npc: Option<String>, hidden: bool },
    /// Stop an NPC from talking (`Actions/MuteNpc.cs:20`, M26).
    MuteNpc { npc: Option<String> },
    /// Foe command owned by combat (`Actions/ChangeFoeTeam.cs:22`,
    /// `Actions/ChangeFoeInfighting.cs:22`, `Actions/RestrainFoe.cs:21`,
    /// `Actions/UnrestrainFoe.cs:20`, `Actions/Enemies.cs:20`,
    /// `Actions/RemoveFoe.cs:18`; M22).
    FoeCommand { effect: OpcodeEffect },
    /// Condition evaluated by the owning organ (inventory, place, combat,
    /// reputation, spell watch, level, clock window, faction); the target
    /// never guesses it.
    Condition { effect: OpcodeEffect },
}

impl QuestRequest {
    /// Builds the request for an effect whose sub-record payload is opaque.
    /// `target` is the optional symbol a test assigned to the action; when it
    /// is absent the payload stays `None` (never invented).
    pub fn from_effect(effect: OpcodeEffect, message_id: u16, target: Option<&str>) -> Option<Self> {
        let named = || target.map(str::to_string);
        let message = (message_id != NO_MESSAGE_ID).then_some(message_id);
        let request = match effect {
            OpcodeEffect::CreateFoe => Self::SpawnFoe { foe: named() },
            OpcodeEffect::PlaceFoe => Self::PlaceFoe { foe: named() },
            OpcodeEffect::CreateNpc => Self::CreateNpc { npc: named() },
            OpcodeEffect::CreateNpcAt => Self::CreateNpcAt { npc: named() },
            OpcodeEffect::PlaceNpc => Self::PlaceNpc { npc: named() },
            OpcodeEffect::PlaceItem => Self::PlaceItem { item: named() },
            OpcodeEffect::GiveItemToNpc => Self::GiveItemToNpc { item: named() },
            OpcodeEffect::GivePc | OpcodeEffect::GiveItem => {
                Self::GiveItemToPc { item: named() }
            }
            OpcodeEffect::GiveGold => Self::GiveGold { amount: None },
            OpcodeEffect::TakeItem => Self::TakeItem { item: named() },
            OpcodeEffect::TakeGold => Self::TakeGold { amount: None },
            OpcodeEffect::TeleportPc => Self::TeleportPc {
                destination: named(),
            },
            OpcodeEffect::RevealLocation | OpcodeEffect::RevealMap => Self::RevealLocation {
                location: named(),
            },
            OpcodeEffect::PlaySound => Self::PlaySound { sound_id: None },
            OpcodeEffect::PlaySong => Self::PlaySong { song_id: None },
            OpcodeEffect::PlayVideo => Self::PlayVideo { movie_id: None },
            OpcodeEffect::StartQuest | OpcodeEffect::RunQuest => Self::StartQuest {
                quest_name: named(),
            },
            OpcodeEffect::ChangeReputation => Self::ChangeReputation {
                faction_id: None,
                delta: None,
                propagate: false,
            },
            OpcodeEffect::SetPlayerCrime => Self::SetPlayerCrime { crime: None },
            OpcodeEffect::SpawnCityGuards => Self::SpawnCityGuards { count: None },
            OpcodeEffect::LegalRepute => Self::LegalRepute { delta: None },
            OpcodeEffect::MakeDiseased => Self::MakeDiseased { disease: named() },
            OpcodeEffect::CureDisease => Self::CureDisease,
            OpcodeEffect::AddTopic => Self::AddTopic { topic: named() },
            OpcodeEffect::RemoveTopic => Self::RemoveTopic { topic: named() },
            OpcodeEffect::Rumor => Self::Rumor {
                message_id: message.unwrap_or(NO_MESSAGE_ID),
            },
            OpcodeEffect::TravelTime => Self::TravelTime {
                seconds: None,
                flag: None,
            },
            OpcodeEffect::AddFace => Self::ShowFace { npc: named() },
            OpcodeEffect::DropFace => Self::HideFace { npc: named() },
            OpcodeEffect::ShowPrompt => Self::Prompt {
                message_id: message.unwrap_or(NO_MESSAGE_ID),
            },
            OpcodeEffect::AddQuestor => Self::AddQuestor { npc: named() },
            OpcodeEffect::DropQuestor => Self::DropQuestor { npc: named() },
            OpcodeEffect::MakePermanent => Self::MakePermanent { item: named() },
            OpcodeEffect::WeatherOverride => Self::WeatherOverride {
                region_id: None,
                weather: None,
            },
            OpcodeEffect::DestroyNpc => Self::DestroyNpc { npc: named() },
            OpcodeEffect::HideNpc => Self::HideNpc {
                npc: named(),
                hidden: true,
            },
            OpcodeEffect::RestoreNpc => Self::HideNpc {
                npc: named(),
                hidden: false,
            },
            OpcodeEffect::MuteNpc => Self::MuteNpc { npc: named() },
            OpcodeEffect::ChangeFoeTeam
            | OpcodeEffect::ChangeFoeInfighting
            | OpcodeEffect::RestrainFoe
            | OpcodeEffect::UnrestrainFoe
            | OpcodeEffect::Enemies
            | OpcodeEffect::RemoveFoe => Self::FoeCommand { effect },
            OpcodeEffect::HaveItem
            | OpcodeEffect::PcAt
            | OpcodeEffect::KillCount
            | OpcodeEffect::InjuredFoe
            | OpcodeEffect::ItemFound
            | OpcodeEffect::ItemUsed
            | OpcodeEffect::ReputeExceeds
            | OpcodeEffect::StateAnd
            | OpcodeEffect::StateOr
            | OpcodeEffect::PickOneOf
            | OpcodeEffect::ClickedNpc
            | OpcodeEffect::DailyFrom
            | OpcodeEffect::AttributeLevel
            | OpcodeEffect::LevelCompleted
            | OpcodeEffect::CastSpell
            | OpcodeEffect::FactionReputation => Self::Condition { effect },
            _ => return None,
        };
        Some(request)
    }
}

/// Parses a Daggerfall time value: `dd.hh:mm`, `hh:mm` or `mm` in minutes
/// (`Clock.cs:368-388`). Returns `None` for an empty, signed, non-numeric or
/// overflowing value.
pub fn parse_time_seconds(text: &str) -> Option<i64> {
    fn component(text: &str) -> Option<i64> {
        if text.is_empty() || !text.bytes().all(|byte| byte.is_ascii_digit()) {
            return None;
        }
        text.parse::<i64>().ok()
    }

    let text = text.trim();
    if text.is_empty() {
        return None;
    }
    let parts: Vec<&str> = text.split(':').collect();
    let (days, hours, minutes) = match parts.as_slice() {
        [hour_minute, minutes] => match hour_minute.split_once('.') {
            Some((days, hours)) => (component(days)?, component(hours)?, component(minutes)?),
            None => (0, component(hour_minute)?, component(minutes)?),
        },
        [minutes] => (0, 0, component(minutes)?),
        _ => return None,
    };
    days.checked_mul(86_400)?
        .checked_add(hours.checked_mul(3_600)?)?
        .checked_add(minutes.checked_mul(60)?)
}

/// Clock construction recipe (`Clock.cs:189-226`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClockRecipe {
    /// No value: `60..=604800` seconds (`Clock.cs:189-198`).
    Default,
    /// One value: fixed duration as written.
    Fixed(i64),
    /// Two values: inclusive range when `max > min`, else `min`
    /// (`Clock.cs:205-213`).
    Range(i64, i64),
    /// Text form parsed by [`parse_time_seconds`].
    Text(&'static str),
    /// Clock flag `&16`: the duration is the cautious **round trip** of the
    /// one-way seconds M31 resolves (`Clock.cs:215-220` sums every place and
    /// multiplies once by `returnTripMultiplier`, `:431-433`).
    ReturnTrip {
        /// Cautious one-way travel time in seconds, resolved by M31.
        one_way_seconds: i64,
    },
}

/// Inclusive draw through the injected LCG. Negative or inverted bounds are
/// clamped to `0`; retail values are below `i32::MAX` (`Clock.cs:385-388`).
fn draw_inclusive(rng: &mut DfRandom, min: i64, max: i64) -> i64 {
    let low = min.clamp(0, i64::from(i32::MAX)) as i32;
    let high = max.clamp(i64::from(low), i64::from(i32::MAX)) as i32;
    i64::from(rng.random_range_inclusive(low, high))
}

/// Duration in seconds for a clock recipe. `Default` draws
/// `60..=604800`; `Fixed` clamps negatives to 0; `Range` draws inclusive when
/// `max > min` and returns `min` otherwise; `Text` returns the parsed value
/// or 0 for an invalid text (the caller keeps the text as declared data).
pub fn clock_duration(recipe: ClockRecipe, rng: &mut DfRandom) -> i64 {
    match recipe {
        ClockRecipe::Default => draw_inclusive(rng, CLOCK_MIN_SECONDS, CLOCK_MAX_SECONDS),
        ClockRecipe::Fixed(seconds) => seconds.max(0),
        ClockRecipe::Range(min, max) => {
            let min = min.max(0);
            if max > min {
                draw_inclusive(rng, min, max)
            } else {
                min
            }
        }
        ClockRecipe::Text(text) => parse_time_seconds(text).unwrap_or(0),
        // `(int)(minutes * returnTripMultiplier)` (`Clock.cs:431-433`): both
        // DFU and this `as` cast truncate toward zero. The multiplication is
        // in `f32` exactly like the source field, and `as i64` saturates at
        // the bounds instead of wrapping; the one-way value is clamped at 0
        // first so a negative input never flips the round trip.
        ClockRecipe::ReturnTrip { one_way_seconds } => {
            (one_way_seconds.max(0) as f32 * RETURN_TRIP_MULTIPLIER) as i64
        }
    }
}

/// Named quest timer. The symbol is the task it triggers (`Clock.cs:401-411`);
/// `flag` carries the documented clock bits uninterpreted (`Clock.cs:244-252`)
/// except `&16`, which the caller turns into a [`QuestRequest::TravelTime`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuestClock {
    /// Task symbol fired on expiry.
    pub symbol: String,
    /// Seconds left.
    pub remaining: i64,
    /// Duration assigned at construction.
    pub starting_seconds: i64,
    /// Clock flags (`Clock.cs:244-252`).
    pub flag: u8,
    /// `clockEnabled`.
    pub enabled: bool,
    /// `clockFinished`.
    pub finished: bool,
    /// Lower bound of the recipe.
    pub min_range: i64,
    /// Upper bound of the recipe.
    pub max_range: i64,
    /// `lastWorldTimeSample` (`Clock.cs:490-499`): world seconds observed at
    /// the last tick, persisted so a restored clock resumes from the same
    /// sample.
    pub last_world_time_sample: i64,
}

impl QuestClock {
    /// Builds a clock from a recipe and draws its duration with `rng`.
    pub fn new(symbol: impl Into<String>, recipe: ClockRecipe, rng: &mut DfRandom) -> Self {
        let seconds = clock_duration(recipe, rng);
        let (min_range, max_range) = match recipe {
            ClockRecipe::Default => (CLOCK_MIN_SECONDS, CLOCK_MAX_SECONDS),
            ClockRecipe::Fixed(value) => (value.max(0), value.max(0)),
            ClockRecipe::Range(min, max) => (min.max(0), max.max(0)),
            ClockRecipe::Text(_) => (seconds, seconds),
            ClockRecipe::ReturnTrip { .. } => (seconds, seconds),
        };
        Self {
            symbol: symbol.into(),
            remaining: seconds,
            starting_seconds: seconds,
            flag: 0,
            enabled: true,
            finished: false,
            min_range,
            max_range,
            last_world_time_sample: 0,
        }
    }

    /// Sets the flag byte (`Clock.cs:244-252`).
    pub fn with_flag(mut self, flag: u8) -> Self {
        self.flag = flag;
        self
    }

    /// Clock flag `&16` means "duration is the round trip to the quest
    /// places" (`Clock.cs:215-220`). Such a clock asks M31 for that time when
    /// it is added/started; it never computes the route itself.
    pub fn wants_travel_time(&self) -> bool {
        self.flag & 0x10 == 0x10
    }

    /// `TravelTime` petition for a `&16` clock; `None` for every other clock.
    pub fn travel_request(&self) -> Option<QuestRequest> {
        self.wants_travel_time().then_some(QuestRequest::TravelTime {
            seconds: Some(self.remaining),
            flag: Some(self.flag),
        })
    }

    /// Replaces the remaining time (used by `start timer` and by M31 travel
    /// time requests).
    pub fn set_remaining(&mut self, seconds: i64) {
        self.remaining = seconds.max(0);
        self.starting_seconds = self.remaining;
        self.finished = false;
        self.enabled = true;
    }

    /// Subtracts whole world seconds. Returns `true` exactly on the tick that
    /// reaches zero; afterwards the clock is disabled and finished
    /// (`Clock.cs:276-300`).
    pub fn tick(&mut self, delta_seconds: i64) -> bool {
        if !self.enabled || self.finished || delta_seconds <= 0 {
            return false;
        }
        self.remaining = self.remaining.saturating_sub(delta_seconds).max(0);
        if self.remaining == 0 {
            self.enabled = false;
            self.finished = true;
            return true;
        }
        false
    }

    /// Seconds left (never negative).
    pub fn remaining(&self) -> i64 {
        self.remaining
    }

    /// Finished?
    pub fn is_finished(&self) -> bool {
        self.finished
    }

    /// `GetDaysString`: `ceil(seconds / 86400)` (`Clock.cs:349-355`).
    pub fn days(&self) -> i64 {
        if self.remaining <= 0 {
            0
        } else {
            self.remaining.saturating_add(SECONDS_PER_DAY - 1) / SECONDS_PER_DAY
        }
    }

    /// Re-enables a non-finished clock (`StartTimer`, `Clock.cs:309-341`).
    pub fn start_timer(&mut self) {
        if self.enabled {
            return;
        }
        if !self.finished {
            self.enabled = true;
        }
    }

    /// Stops a non-finished clock (`StopTimer`, `Clock.cs:343-347`).
    pub fn stop_timer(&mut self) {
        if !self.finished {
            self.enabled = false;
        }
    }
}

/// Task kinds (`Task.cs:107-114`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskType {
    /// Starts on its own.
    Headless,
    /// Started by `set`/a trigger.
    Standard,
    /// Monitoring task that clears when its target is triggered
    /// (`Task.cs:263-276`).
    PersistUntil,
    /// Boolean variable without actions: its state mirrors the local
    /// variable named after the task (`Quest::var`/`Quest::set_var`).
    Variable,
    /// Link to a global variable: its state mirrors
    /// `Quest::global_var(task.global_var)`.
    GlobalVarLink,
}

/// One action or trigger of a task. The DFU contract is
/// `QuestAction.cs:23-90,120-205`; `trigger_met` is the world's answer to the
/// trigger test, so this module stays free of world queries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuestAction {
    /// Semantic class (see [`OpcodeEffect`]).
    pub effect: OpcodeEffect,
    /// `IsTriggerCondition`.
    pub is_trigger: bool,
    /// `IsAlwaysOnTriggerCondition`.
    pub is_always_on_trigger: bool,
    /// `IsComplete`: completed actions never run again (`Task.cs:211-212`).
    pub is_complete: bool,
    /// Message id carried by the opcode (`+81`); [`NO_MESSAGE_ID`] means none.
    pub message_id: u16,
    /// Optional symbol for clock/task-targeted effects. Not measured in the
    /// opcode record; a caller may set it when the sub-record is resolved.
    pub target_symbol: Option<String>,
    /// Result of the trigger test for this tick.
    pub trigger_met: bool,
    /// Explicit journal step (`log <id> step <step>`); `None` allocates one.
    pub step_id: Option<u32>,
    /// `InitialiseOnSet` calls observed (only on the rising edge).
    pub initialisations: u32,
    /// `Update` calls observed.
    pub updates: u32,
}

impl QuestAction {
    /// Non-trigger action with no message.
    pub fn new(effect: OpcodeEffect) -> Self {
        Self {
            effect,
            is_trigger: false,
            is_always_on_trigger: false,
            is_complete: false,
            message_id: NO_MESSAGE_ID,
            target_symbol: None,
            trigger_met: false,
            step_id: None,
            initialisations: 0,
            updates: 0,
        }
    }

    /// Regular trigger condition.
    pub fn trigger(effect: OpcodeEffect) -> Self {
        Self {
            is_trigger: true,
            ..Self::new(effect)
        }
    }

    /// Always-on trigger condition.
    pub fn always_on(effect: OpcodeEffect) -> Self {
        Self {
            is_trigger: true,
            is_always_on_trigger: true,
            ..Self::new(effect)
        }
    }

    /// Sets the message id.
    pub fn with_message(mut self, message_id: u16) -> Self {
        self.message_id = message_id;
        self
    }

    /// Sets the target symbol.
    pub fn with_target(mut self, symbol: impl Into<String>) -> Self {
        self.target_symbol = Some(symbol.into());
        self
    }

    /// Sets the trigger answer.
    pub fn with_trigger_met(mut self, met: bool) -> Self {
        self.trigger_met = met;
        self
    }

    /// Fixes the journal step id.
    pub fn with_step_id(mut self, step_id: u32) -> Self {
        self.step_id = Some(step_id);
        self
    }

    /// Marks the action as already complete.
    pub fn completed(mut self) -> Self {
        self.is_complete = true;
        self
    }

    /// Persisted form (`ActionSaveData_v1`, `QuestAction.cs:181-190`).
    pub fn snapshot(&self) -> ActionSnapshot {
        ActionSnapshot {
            effect: self.effect,
            is_complete: self.is_complete,
            is_trigger: self.is_trigger,
            is_always_on_trigger: self.is_always_on_trigger,
            trigger_met: self.trigger_met,
            step_id: self.step_id,
            message_id: self.message_id,
            target: self.target_symbol.clone(),
            initialisations: self.initialisations,
            updates: self.updates,
        }
    }
}

/// Persisted action shape (`ActionSaveData_v1`, `QuestAction.cs:181-190`).
/// `type` is the semantic [`OpcodeEffect`]; the per-class `actionSpecific`
/// payload is not reinterpreted (spec R-29-7). `initialisations`/`updates`
/// are the target's own observability counters, kept so a round trip is
/// byte-identical.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionSnapshot {
    /// Semantic class (`ActionSaveData_v1.type`).
    pub effect: OpcodeEffect,
    /// `isComplete`.
    pub is_complete: bool,
    /// `isTriggerCondition`.
    pub is_trigger: bool,
    /// `isAlwaysOnTriggerCondition`.
    pub is_always_on_trigger: bool,
    /// World answer to the trigger test.
    pub trigger_met: bool,
    /// Explicit journal step.
    pub step_id: Option<u32>,
    /// Message id from `+81`.
    pub message_id: u16,
    /// Target symbol of the action (`actionSpecific` anchor).
    pub target: Option<String>,
    /// `InitialiseOnSet` calls observed.
    pub initialisations: u32,
    /// `Update` calls observed.
    pub updates: u32,
}

impl ActionSnapshot {
    /// Rebuilds the action a restored quest needs to keep running.
    pub fn to_action(&self) -> QuestAction {
        QuestAction {
            effect: self.effect,
            is_trigger: self.is_trigger,
            is_always_on_trigger: self.is_always_on_trigger,
            is_complete: self.is_complete,
            message_id: self.message_id,
            target_symbol: self.target.clone(),
            trigger_met: self.trigger_met,
            step_id: self.step_id,
            initialisations: self.initialisations,
            updates: self.updates,
        }
    }
}

/// Task with its trigger/action list (`Task.cs:107-308`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuestTask {
    /// Unique task symbol.
    pub symbol: String,
    /// Task kind.
    pub task_type: TaskType,
    /// Target task for [`TaskType::PersistUntil`].
    pub target_symbol: Option<String>,
    /// Triggers and actions in declaration order.
    pub actions: Vec<QuestAction>,
    /// `IsTriggered`.
    pub is_triggered: bool,
    /// `prevTriggered` of the last update.
    pub prev_triggered: bool,
    /// One-way drop (`Task.cs:295-308`).
    pub dropped: bool,
    /// `globalVarName`/`globalVarLink` (`Task.cs:493-494`): the global
    /// variable a [`TaskType::GlobalVarLink`] task is bound to.
    pub global_var: Option<String>,
    /// `hasTriggerConditions` (`Task.cs:497`): derived when a trigger action
    /// is added, persisted verbatim.
    pub has_trigger_conditions: bool,
}

impl QuestTask {
    /// Empty task. A headerless task is headless and **begins as triggered**
    /// (`Task.cs:181-185`: "No match on task header treat as a headless task
    /// (e.g. startup task) / Startup task begins as triggered"); the other
    /// four kinds start untriggered and wait for `set`/a trigger.
    pub fn new(symbol: impl Into<String>, task_type: TaskType) -> Self {
        let is_triggered = matches!(task_type, TaskType::Headless);
        Self {
            symbol: symbol.into(),
            task_type,
            target_symbol: None,
            actions: Vec::new(),
            is_triggered,
            prev_triggered: false,
            dropped: false,
            global_var: None,
            has_trigger_conditions: false,
        }
    }

    /// Sets the target of a `PersistUntil` task.
    pub fn with_target(mut self, symbol: impl Into<String>) -> Self {
        self.target_symbol = Some(symbol.into());
        self
    }

    /// Sets `globalVarName`/`globalVarLink` (`Task.cs:493-494`).
    pub fn with_global_var(mut self, name: impl Into<String>) -> Self {
        self.global_var = Some(name.into());
        self
    }

    /// Adds a trigger or action; `hasTriggerConditions` follows the
    /// `IsTriggerCondition` flag of the added action (`Task.cs:497`).
    pub fn add_action(&mut self, action: QuestAction) {
        self.has_trigger_conditions |= action.is_trigger;
        self.actions.push(action);
    }

    /// `Task.Clear`: unsets the task, keeping the actions (`Task.cs:295-300`).
    pub fn clear(&mut self) {
        self.is_triggered = false;
    }

    /// `Task.Drop`: one-way removal (`Task.cs:303-308`).
    pub fn drop_task(&mut self) {
        self.dropped = true;
        self.is_triggered = false;
    }

    /// `RearmActions`: allows one-shot actions to run again
    /// (`Task.cs:271-276`).
    pub fn rearm_actions(&mut self) {
        for action in &mut self.actions {
            action.is_complete = false;
        }
    }
}

/// Why a quest failed. NEGATIVE opcodes and missing messages never pass in
/// silence (spec 2.4 rule 3; `Quest.cs:666-685`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QuestFailure {
    /// A type without semantics was executed (`spec 2.4`).
    UnsupportedOpcode(u8),
    /// A message id is absent or the sentinel `0xFFFF`.
    MissingMessage(u16),
    /// A targeted task or clock does not exist.
    MissingTarget(String),
    /// A non-trigger action class has no execution path.
    UnhandledEffect(OpcodeEffect),
    /// The quest closed without `give pc` and without another failure.
    NoSuccess,
}

/// Terminal outcome (`Quest.cs:127-140`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QuestOutcome {
    /// `give pc` ran and no failure was recorded.
    Success,
    /// First recorded failure, or [`QuestFailure::NoSuccess`].
    Failure(QuestFailure),
}

/// One journal entry: log steps carry their `stepID`
/// (`Quest.cs:52,87-92`); notes use [`NOTE_STEP_BASE`] plus a counter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JournalEntry {
    /// Log step id, or a note id at [`NOTE_STEP_BASE`] and above.
    pub step_id: u32,
    /// Message id.
    pub message_id: u16,
    /// World time surrogate (seconds since the quest started).
    pub date_time: i64,
}

/// Running quest: tasks, clocks, journal, close clock and reputation
/// (`Quest.cs:36-400`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Quest {
    /// Quest name (`.QBN` file stem).
    pub name: String,
    /// Faction used by the close reputation rule (`Quest.cs:382-386`).
    pub faction_id: u16,
    /// Tasks in declaration order.
    pub tasks: Vec<QuestTask>,
    /// Timers.
    pub clocks: Vec<QuestClock>,
    /// Active journal entries in insertion order.
    pub journal: Vec<JournalEntry>,
    /// Entries copied to the Notebook at `EndQuest` (`Quest.cs:388-398`).
    pub finished_journal: Vec<JournalEntry>,
    /// Display-only messages (`say`/0x33) waiting to be shown. They are never
    /// journal entries: `AddFinishedQuest` receives only the `log` steps
    /// (spec 6.3 B, `Quest.cs:388-398`).
    pub pending_messages: Vec<u16>,
    /// Ticks left before completion (`Quest.cs:300-305`).
    pub ticks_to_end: u32,
    /// One-tick break flag (`Quest.cs:220-224`, `Task.cs:251-253`).
    pub quest_break: bool,
    /// Set only by `give pc` (`Actions/GivePc.cs:152`).
    pub success: bool,
    /// `questComplete`.
    pub complete: bool,
    /// Machine time at acceptance (`Quest.cs:281-285`).
    pub start_time: i64,
    /// `UID` assigned at create time (`Quest.cs:119-122`).
    pub uid: u64,
    /// `QuestTombstoned` (`Quest.cs:230-237`): the quest stays a while after
    /// completion for post-quest rumours and failure text.
    pub tombstoned: bool,
    /// `QuestTombstoneTime` (`Quest.cs:239-242`); not valid until
    /// `tombstoned` is set.
    pub tombstone_time: i64,
    /// Local boolean variables (`TaskType::Variable`, `Task.cs:107-114`);
    /// read/written through [`Quest::set_var`]/[`Quest::var`].
    variables: BTreeMap<String, bool>,
    /// Global boolean variables (`globalVarName`, `Task.cs:493-494`);
    /// read/written through [`Quest::set_global_var`]/[`Quest::global_var`].
    global_vars: BTreeMap<String, bool>,
    /// Display messages already drained by the presentation cycle
    /// (`ShowPendingTaskMessages`, `Quest.cs:847-854`): the mailbox never
    /// grows without bound, and the drained sequence stays observable.
    delivered_messages: Vec<u16>,
    messages: Vec<(u16, String)>,
    failures: Vec<QuestFailure>,
    requests: Vec<QuestRequest>,
    journal_skipped: u32,
    next_step_id: u32,
    next_note_id: u32,
    date_time: i64,
    tick_in_second: u32,
    end_applied: bool,
}

impl Quest {
    /// Empty quest, not successful, not complete.
    pub fn new(name: impl Into<String>, faction_id: u16) -> Self {
        Self {
            name: name.into(),
            faction_id,
            tasks: Vec::new(),
            clocks: Vec::new(),
            journal: Vec::new(),
            finished_journal: Vec::new(),
            pending_messages: Vec::new(),
            ticks_to_end: 0,
            quest_break: false,
            success: false,
            complete: false,
            start_time: 0,
            uid: 0,
            tombstoned: false,
            tombstone_time: 0,
            variables: BTreeMap::new(),
            global_vars: BTreeMap::new(),
            delivered_messages: Vec::new(),
            messages: Vec::new(),
            failures: Vec::new(),
            requests: Vec::new(),
            journal_skipped: 0,
            next_step_id: 1,
            next_note_id: 0,
            date_time: 0,
            tick_in_second: 0,
            end_applied: false,
        }
    }

    /// Adds a task (symbols are unique in retail data; a repeated symbol is
    /// appended and [`Quest::task`] returns the first).
    pub fn add_task(&mut self, task: QuestTask) {
        self.tasks.push(task);
    }

    /// Adds a clock. A clock carrying flag `&16` asks M31 for the round-trip
    /// travel time as soon as it is added (`Clock.cs:215-220`); the request
    /// travels as [`QuestRequest::TravelTime`] and no route is computed here.
    pub fn add_clock(&mut self, clock: QuestClock) {
        if let Some(request) = clock.travel_request() {
            self.requests.push(request);
        }
        self.clocks.push(clock);
    }

    /// Sets the quest UID assigned at create time (`Quest.cs:119-122`).
    pub fn with_uid(mut self, uid: u64) -> Self {
        self.uid = uid;
        self
    }

    /// Tombstones the quest at `world_time` (`Quest.cs:230-242`): the record
    /// stays available for post-quest text after completion.
    pub fn tombstone(&mut self, world_time: i64) {
        self.tombstoned = true;
        self.tombstone_time = world_time;
    }

    /// Local boolean variable (`TaskType::Variable`); unset variables read
    /// `false`.
    pub fn var(&self, name: &str) -> bool {
        self.variables.get(name).copied().unwrap_or(false)
    }

    /// Sets a local boolean variable; a `Variable` task bound to `name`
    /// observes it on the next [`Quest::tick`].
    pub fn set_var(&mut self, name: impl Into<String>, value: bool) {
        self.variables.insert(name.into(), value);
    }

    /// Global boolean variable (`globalVarName`, `Task.cs:493-494`).
    pub fn global_var(&self, name: &str) -> bool {
        self.global_vars.get(name).copied().unwrap_or(false)
    }

    /// Sets a global boolean variable; a `GlobalVarLink` task bound to `name`
    /// observes it on the next [`Quest::tick`].
    pub fn set_global_var(&mut self, name: impl Into<String>, value: bool) {
        self.global_vars.insert(name.into(), value);
    }

    /// Local variables, in name order.
    pub fn variables(&self) -> &BTreeMap<String, bool> {
        &self.variables
    }

    /// Global variables, in name order.
    pub fn global_vars(&self) -> &BTreeMap<String, bool> {
        &self.global_vars
    }

    /// Registers resolved message text (M16 supplies it; this module never
    /// parses `.QRC`).
    pub fn add_message(&mut self, id: u16, text: impl Into<String>) {
        self.messages.push((id, text.into()));
    }

    /// Resolved message by id. Missing ids return `None`, never an error
    /// (`Quest.cs:666-685`).
    pub fn message(&self, id: u16) -> Option<&str> {
        self.messages
            .iter()
            .find(|(message_id, _)| *message_id == id)
            .map(|(_, text)| text.as_str())
    }

    /// Task by symbol.
    pub fn task(&self, symbol: &str) -> Option<&QuestTask> {
        self.tasks.iter().find(|task| task.symbol == symbol)
    }

    /// Mutable task by symbol.
    pub fn task_mut(&mut self, symbol: &str) -> Option<&mut QuestTask> {
        self.tasks.iter_mut().find(|task| task.symbol == symbol)
    }

    /// Clock by symbol.
    pub fn clock(&self, symbol: &str) -> Option<&QuestClock> {
        self.clocks.iter().find(|clock| clock.symbol == symbol)
    }

    /// Mutable clock by symbol.
    pub fn clock_mut(&mut self, symbol: &str) -> Option<&mut QuestClock> {
        self.clocks.iter_mut().find(|clock| clock.symbol == symbol)
    }

    /// Requests emitted for other organs.
    pub fn requests(&self) -> &[QuestRequest] {
        &self.requests
    }

    /// Recorded failures in discovery order.
    pub fn failures(&self) -> &[QuestFailure] {
        &self.failures
    }

    /// Journal entries skipped because their message id is absent.
    pub fn journal_skipped(&self) -> u32 {
        self.journal_skipped
    }

    /// World-time surrogate of the last whole second.
    pub fn date_time(&self) -> i64 {
        self.date_time
    }

    fn record_failure(&mut self, failure: QuestFailure) {
        self.failures.push(failure);
    }

    /// Executes one decoded opcode. A NEGATIVE type records
    /// [`QuestFailure::UnsupportedOpcode`] and returns it (spec 2.4 rule 3);
    /// a covered type applies its effect and returns the failure it produced,
    /// if any.
    pub fn execute_opcode(&mut self, opcode: DecodedOpcode) -> Result<(), QuestFailure> {
        let effect = opcode_coverage(opcode.type_id).and_then(|row| row.effect);
        let Some(effect) = effect else {
            let failure = QuestFailure::UnsupportedOpcode(opcode.type_id);
            self.record_failure(failure.clone());
            return Err(failure);
        };
        let before = self.failures.len();
        self.run_action(effect, opcode.message_id, None, None);
        match self.failures.get(before).cloned() {
            Some(failure) => Err(failure),
            None => Ok(()),
        }
    }

    /// One quest tick (0.1 s at 10 Hz). Order follows `Quest.cs:290-346`:
    /// early return when complete, `ticksToEnd` countdown, resources, then
    /// the task loop with its skip and break rules.
    pub fn tick(&mut self) {
        if self.complete {
            return;
        }
        // The presentation cycle pops the pending popups until the stack is
        // empty (`ShowPendingTaskMessages`, `Quest.cs:847-854`); draining at
        // the top of the tick bounds the mailbox and keeps the drained order
        // observable.
        let shown = self.take_pending_messages();
        self.delivered_messages.extend(shown);
        if self.ticks_to_end > 0 {
            self.ticks_to_end -= 1;
            if self.ticks_to_end == 0 {
                self.complete = true;
            }
        }
        self.tick_in_second += 1;
        let whole_second = self.tick_in_second >= TICKS_PER_SECOND;
        if whole_second {
            self.tick_in_second = 0;
            self.date_time = self.date_time.saturating_add(1);
        }
        let sample = self.date_time;
        let mut expired: Vec<String> = Vec::new();
        for clock in &mut self.clocks {
            if whole_second {
                clock.last_world_time_sample = sample;
                if clock.tick(1) {
                    expired.push(clock.symbol.clone());
                }
            }
        }
        for symbol in expired {
            if let Some(task) = self.tasks.iter_mut().find(|task| task.symbol == symbol) {
                task.is_triggered = true;
            }
        }
        if self.complete {
            self.quest_break = false;
            return;
        }
        let mut index = 0;
        while index < self.tasks.len() {
            if self.quest_break || self.complete {
                self.quest_break = false;
                return;
            }
            if !self.tasks[index].dropped {
                self.update_task(index);
            }
            index += 1;
            if self.quest_break || self.complete {
                self.quest_break = false;
                return;
            }
        }
    }

    /// `TaskType::Variable` and `TaskType::GlobalVarLink` are the boolean
    /// variables of `Task.cs:107-114`: the task state mirrors the variable,
    /// so a "when var set" trigger fires as soon as the variable is set and
    /// stops when it is cleared. Every other kind keeps its own state.
    fn bind_variable_task(&mut self, index: usize) {
        let task = &self.tasks[index];
        let state = match task.task_type {
            TaskType::Variable => self.var(&task.symbol),
            TaskType::GlobalVarLink => task
                .global_var
                .as_deref()
                .is_some_and(|name| self.global_var(name)),
            TaskType::Headless | TaskType::Standard | TaskType::PersistUntil => return,
        };
        self.tasks[index].is_triggered = state;
    }

    /// `Task.Update` (`Task.cs:194-282`) for the task at `index`.
    fn update_task(&mut self, index: usize) {
        self.bind_variable_task(index);
        let mut ran_primary_always_on = false;
        let action_count = self.tasks[index].actions.len();
        for action_index in 0..action_count {
            let (is_complete, is_trigger, is_always_on, trigger_met) = {
                let action = &self.tasks[index].actions[action_index];
                (
                    action.is_complete,
                    action.is_trigger,
                    action.is_always_on_trigger,
                    action.trigger_met,
                )
            };
            if is_complete {
                continue;
            }
            if (is_trigger && !self.tasks[index].is_triggered) || is_always_on {
                if is_always_on && !ran_primary_always_on {
                    self.tasks[index].is_triggered = trigger_met;
                    ran_primary_always_on = true;
                } else if is_always_on {
                    if trigger_met {
                        self.tasks[index].is_triggered = true;
                    }
                } else {
                    self.tasks[index].is_triggered = trigger_met;
                }
            }
            if self.tasks[index].is_triggered && !is_trigger {
                let was_triggered = self.tasks[index].prev_triggered;
                if !was_triggered {
                    self.tasks[index].actions[action_index].initialisations += 1;
                }
                self.tasks[index].actions[action_index].updates += 1;
                let effect = self.tasks[index].actions[action_index].effect;
                let message_id = self.tasks[index].actions[action_index].message_id;
                let target = self.tasks[index].actions[action_index]
                    .target_symbol
                    .clone();
                let step_id = self.tasks[index].actions[action_index].step_id;
                self.run_action(effect, message_id, target.as_deref(), step_id);
                // `Task.cs:207-209`: the action itself decides. The classes
                // that call `SetComplete()` finish after one run; the
                // repeating ones (`PlaySound`, `PcAt`) stay runnable.
                if effect.completes_after_update() {
                    self.tasks[index].actions[action_index].is_complete = true;
                }
                if self.quest_break {
                    return;
                }
            }
        }
        if self.tasks[index].task_type == TaskType::PersistUntil {
            let target_symbol = self.tasks[index].target_symbol.clone();
            let target_triggered = target_symbol
                .as_deref()
                .and_then(|symbol| self.tasks.iter().find(|task| task.symbol == symbol))
                .is_some_and(|task| task.is_triggered);
            if target_triggered {
                self.tasks[index].clear();
            } else {
                self.tasks[index].rearm_actions();
            }
        }
        self.tasks[index].prev_triggered = self.tasks[index].is_triggered;
    }

    /// Applies one action effect. Internal effects mutate the quest; effects
    /// owned by another organ become [`QuestRequest`] values.
    fn run_action(
        &mut self,
        effect: OpcodeEffect,
        message_id: u16,
        target: Option<&str>,
        step_id: Option<u32>,
    ) {
        match effect {
            OpcodeEffect::LogMessage => self.log_message(message_id, step_id),
            OpcodeEffect::JournalNote => self.log_note(message_id),
            OpcodeEffect::DisplayMessage => {
                // `ShowMessagePopup` returns before `QuestBreak` when the
                // message does not exist (`Quest.cs:780-782`); only a popup
                // that was actually queued breaks the tick (`:838-841`).
                if self.display_message(message_id) {
                    self.quest_break = true;
                }
            }
            OpcodeEffect::RemoveLogMessage => {
                // DFU removes the log entry by `stepID`
                // (`Actions/RemoveLogMessage.cs:61`, `Quest.cs:569-576`). The
                // message id is a declared fallback for an action whose step
                // was not resolved; with two entries sharing a message id and
                // a step given, only that step is removed.
                match step_id {
                    Some(step_id) => self.journal.retain(|entry| entry.step_id != step_id),
                    None => self.journal.retain(|entry| entry.message_id != message_id),
                }
            }
            OpcodeEffect::EndQuest => {
                // `Actions/EndQuest.cs:60-61`: break the quest tick *before*
                // `EndQuest()`, so the remaining tasks of this tick do not run
                // (`Quest.cs:321-325`).
                self.quest_break = true;
                self.end_quest();
            }
            OpcodeEffect::ClearTask => match target {
                Some(symbol) => match self.task_mut(symbol) {
                    Some(task) => task.clear(),
                    None => self.record_failure(QuestFailure::MissingTarget(symbol.to_string())),
                },
                None => self.record_failure(QuestFailure::MissingTarget(String::new())),
            },
            OpcodeEffect::StartTask => match target {
                Some(symbol) => match self.task_mut(symbol) {
                    Some(task) => task.is_triggered = true,
                    None => self.record_failure(QuestFailure::MissingTarget(symbol.to_string())),
                },
                None => self.record_failure(QuestFailure::MissingTarget(String::new())),
            },
            OpcodeEffect::UnsetTask => match target {
                Some(symbol) => match self.task_mut(symbol) {
                    Some(task) => task.clear(),
                    None => self.record_failure(QuestFailure::MissingTarget(symbol.to_string())),
                },
                None => self.record_failure(QuestFailure::MissingTarget(String::new())),
            },
            OpcodeEffect::StartTimer => match target {
                Some(symbol) => {
                    let travel = match self.clock_mut(symbol) {
                        Some(clock) => {
                            clock.start_timer();
                            clock.travel_request()
                        }
                        None => {
                            self.record_failure(QuestFailure::MissingTarget(symbol.to_string()));
                            None
                        }
                    };
                    if let Some(request) = travel {
                        self.requests.push(request);
                    }
                }
                None => self.record_failure(QuestFailure::MissingTarget(String::new())),
            },
            OpcodeEffect::StopTimer => match target {
                Some(symbol) => match self.clock_mut(symbol) {
                    Some(clock) => clock.stop_timer(),
                    None => self.record_failure(QuestFailure::MissingTarget(symbol.to_string())),
                },
                None => self.record_failure(QuestFailure::MissingTarget(String::new())),
            },
            OpcodeEffect::GivePc => {
                self.success = true;
                if let Some(request) = QuestRequest::from_effect(effect, message_id, target) {
                    self.requests.push(request);
                }
            }
            _ => match QuestRequest::from_effect(effect, message_id, target) {
                Some(request) => self.requests.push(request),
                None => self.record_failure(QuestFailure::UnhandledEffect(effect)),
            },
        }
    }

    fn log_message(&mut self, message_id: u16, step_id: Option<u32>) {
        if message_id == NO_MESSAGE_ID || self.message(message_id).is_none() {
            self.journal_skipped += 1;
            self.record_failure(QuestFailure::MissingMessage(message_id));
            return;
        }
        let step_id = step_id.unwrap_or_else(|| {
            let step = self.next_step_id;
            self.next_step_id = self.next_step_id.saturating_add(1);
            step
        });
        if let Some(entry) = self.journal.iter_mut().find(|entry| entry.step_id == step_id) {
            entry.message_id = message_id;
            entry.date_time = self.date_time;
        } else {
            self.journal.push(JournalEntry {
                step_id,
                message_id,
                date_time: self.date_time,
            });
        }
    }

    /// `say`/0x33: the message becomes a pending popup, never a journal entry
    /// (`Actions/Say.cs:58-62` never calls `AddLogStep`; `EndQuest` only
    /// copies log steps, `Quest.cs:388-398`). A sentinel or absent id is
    /// recorded as a failure and nothing is queued; returns `true` only when
    /// the message was really queued (`Quest.cs:780-782`: `GetMessage`
    /// returns `null` and the caller returns before `QuestBreak`).
    fn display_message(&mut self, message_id: u16) -> bool {
        if message_id == NO_MESSAGE_ID || self.message(message_id).is_none() {
            self.record_failure(QuestFailure::MissingMessage(message_id));
            return false;
        }
        self.pending_messages.push(message_id);
        true
    }

    fn log_note(&mut self, message_id: u16) {
        if message_id == NO_MESSAGE_ID || self.message(message_id).is_none() {
            self.journal_skipped += 1;
            self.record_failure(QuestFailure::MissingMessage(message_id));
            return;
        }
        let step_id = NOTE_STEP_BASE.saturating_add(self.next_note_id);
        self.next_note_id = self.next_note_id.saturating_add(1);
        self.journal.push(JournalEntry {
            step_id,
            message_id,
            date_time: self.date_time,
        });
    }

    /// `EndQuest()` (`Quest.cs:367-400`): sets `ticksToEnd = 2`, copies the
    /// journal entries whose message exists (log steps only; notes live in
    /// the Notebook), records skipped ids as [`QuestFailure::MissingMessage`]
    /// and requests the faction reputation change through M27.
    pub fn end_quest(&mut self) {
        if self.end_applied {
            return;
        }
        self.end_applied = true;
        self.ticks_to_end = END_TICKS_TO_END;
        let journal = self.journal.clone();
        for entry in journal {
            if entry.step_id >= NOTE_STEP_BASE {
                continue;
            }
            if entry.message_id == NO_MESSAGE_ID || self.message(entry.message_id).is_none() {
                self.journal_skipped += 1;
                self.record_failure(QuestFailure::MissingMessage(entry.message_id));
            } else {
                self.finished_journal.push(entry);
            }
        }
        if self.faction_id > 0 {
            let delta = if self.success {
                SUCCESS_REPUTATION
            } else {
                FAILURE_REPUTATION
            };
            self.requests.push(QuestRequest::ChangeReputation {
                faction_id: Some(self.faction_id),
                delta: Some(delta),
                propagate: true,
            });
        }
    }

    /// Quest complete?
    pub fn is_complete(&self) -> bool {
        self.complete
    }

    /// `give pc` ran?
    pub fn succeeded(&self) -> bool {
        self.success
    }

    /// Ticks left before completion.
    pub fn ticks_to_end(&self) -> u32 {
        self.ticks_to_end
    }

    /// Active journal.
    pub fn journal(&self) -> &[JournalEntry] {
        &self.journal
    }

    /// Notebook entries built by [`Quest::end_quest`].
    pub fn finished_journal(&self) -> &[JournalEntry] {
        &self.finished_journal
    }

    /// Display-only messages queued for the presentation layer, in order.
    /// Inspection only: a consumer that presents them must drain through
    /// [`Quest::take_pending_messages`], or the mailbox grows without bound.
    pub fn pending_messages(&self) -> &[u16] {
        &self.pending_messages
    }

    /// Drains the display mailbox in queue order and returns the messages
    /// (`ShowPendingTaskMessages` pops until empty, `Quest.cs:847-854`). A
    /// second call returns an empty vector.
    pub fn take_pending_messages(&mut self) -> Vec<u16> {
        std::mem::take(&mut self.pending_messages)
    }

    /// Messages already drained by the tick cycle, in presentation order.
    pub fn delivered_messages(&self) -> &[u16] {
        &self.delivered_messages
    }

    /// Terminal outcome; `None` while the quest is still running. A recorded
    /// failure overrides success (a missing message is never a success).
    pub fn outcome(&self) -> Option<QuestOutcome> {
        if !self.complete {
            return None;
        }
        if let Some(failure) = self.failures.first() {
            return Some(QuestOutcome::Failure(failure.clone()));
        }
        if self.success {
            Some(QuestOutcome::Success)
        } else {
            Some(QuestOutcome::Failure(QuestFailure::NoSuccess))
        }
    }

    /// Serializable shape (spec 5). Tasks carry their `TaskSaveData_v1`
    /// fields, including `actions` in `ActionSaveData_v1` form
    /// (`QuestAction.cs:181-190`); the per-class `actionSpecific` payload
    /// stays opaque (spec R-29-7).
    pub fn snapshot(&self) -> QuestSnapshot {
        QuestSnapshot {
            name: self.name.clone(),
            faction_id: self.faction_id,
            tasks: self
                .tasks
                .iter()
                .map(|task| TaskSnapshot {
                    symbol: task.symbol.clone(),
                    target_symbol: task.target_symbol.clone(),
                    is_triggered: task.is_triggered,
                    prev_triggered: task.prev_triggered,
                    dropped: task.dropped,
                    task_type: task.task_type,
                    global_var: task.global_var.clone(),
                    has_trigger_conditions: task.has_trigger_conditions,
                    actions: task.actions.iter().map(QuestAction::snapshot).collect(),
                })
                .collect(),
            clocks: self
                .clocks
                .iter()
                .map(|clock| ClockSnapshot {
                    symbol: clock.symbol.clone(),
                    remaining: clock.remaining,
                    starting_seconds: clock.starting_seconds,
                    flag: clock.flag,
                    enabled: clock.enabled,
                    finished: clock.finished,
                    min_range: clock.min_range,
                    max_range: clock.max_range,
                    last_world_time_sample: clock.last_world_time_sample,
                })
                .collect(),
            journal: self.journal.clone(),
            finished_journal: self.finished_journal.clone(),
            pending_messages: self.pending_messages.clone(),
            delivered_messages: self.delivered_messages.clone(),
            ticks_to_end: self.ticks_to_end,
            quest_break: self.quest_break,
            success: self.success,
            complete: self.complete,
            start_time: self.start_time,
            uid: self.uid,
            tombstoned: self.tombstoned,
            tombstone_time: self.tombstone_time,
            variables: self.variables.clone(),
            global_vars: self.global_vars.clone(),
            messages: self.messages.clone(),
            failures: self.failures.clone(),
            requests: self.requests.clone(),
            journal_skipped: self.journal_skipped,
            next_step_id: self.next_step_id,
            next_note_id: self.next_note_id,
            date_time: self.date_time,
            tick_in_second: self.tick_in_second,
            end_applied: self.end_applied,
        }
    }

    /// Rebuilds a quest from [`QuestSnapshot`], actions included, so a
    /// restored quest keeps executing its tasks (`TaskSaveData_v1.actions`).
    pub fn from_snapshot(snapshot: QuestSnapshot) -> Self {
        let tasks = snapshot
            .tasks
            .into_iter()
            .map(|task| QuestTask {
                symbol: task.symbol,
                task_type: task.task_type,
                target_symbol: task.target_symbol,
                actions: task.actions.iter().map(ActionSnapshot::to_action).collect(),
                is_triggered: task.is_triggered,
                prev_triggered: task.prev_triggered,
                dropped: task.dropped,
                global_var: task.global_var,
                has_trigger_conditions: task.has_trigger_conditions,
            })
            .collect();
        let clocks = snapshot
            .clocks
            .into_iter()
            .map(|clock| QuestClock {
                symbol: clock.symbol,
                remaining: clock.remaining,
                starting_seconds: clock.starting_seconds,
                flag: clock.flag,
                enabled: clock.enabled,
                finished: clock.finished,
                min_range: clock.min_range,
                max_range: clock.max_range,
                last_world_time_sample: clock.last_world_time_sample,
            })
            .collect();
        Self {
            name: snapshot.name,
            faction_id: snapshot.faction_id,
            tasks,
            clocks,
            journal: snapshot.journal,
            finished_journal: snapshot.finished_journal,
            pending_messages: snapshot.pending_messages,
            delivered_messages: snapshot.delivered_messages,
            ticks_to_end: snapshot.ticks_to_end,
            quest_break: snapshot.quest_break,
            success: snapshot.success,
            complete: snapshot.complete,
            start_time: snapshot.start_time,
            uid: snapshot.uid,
            tombstoned: snapshot.tombstoned,
            tombstone_time: snapshot.tombstone_time,
            variables: snapshot.variables,
            global_vars: snapshot.global_vars,
            messages: snapshot.messages,
            failures: snapshot.failures,
            requests: snapshot.requests,
            journal_skipped: snapshot.journal_skipped,
            next_step_id: snapshot.next_step_id,
            next_note_id: snapshot.next_note_id,
            date_time: snapshot.date_time,
            tick_in_second: snapshot.tick_in_second,
            end_applied: snapshot.end_applied,
        }
    }
}

/// Persisted task shape (spec 5; `Task.cs:485-497`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskSnapshot {
    /// Task symbol.
    pub symbol: String,
    /// `PersistUntil` target.
    pub target_symbol: Option<String>,
    /// `triggered`.
    pub is_triggered: bool,
    /// `prevTriggered`.
    pub prev_triggered: bool,
    /// `dropped`.
    pub dropped: bool,
    /// Task kind.
    pub task_type: TaskType,
    /// `globalVarName`/`globalVarLink` (`Task.cs:493-494`).
    pub global_var: Option<String>,
    /// `hasTriggerConditions` (`Task.cs:497`).
    pub has_trigger_conditions: bool,
    /// `actions` (`Task.cs:496`, `ActionSaveData_v1`).
    pub actions: Vec<ActionSnapshot>,
}

/// Persisted clock shape (spec 5; `Clock.cs:490-499`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClockSnapshot {
    /// Task symbol.
    pub symbol: String,
    /// Seconds left.
    pub remaining: i64,
    /// Assigned duration.
    pub starting_seconds: i64,
    /// Clock flags.
    pub flag: u8,
    /// `clockEnabled`.
    pub enabled: bool,
    /// `clockFinished`.
    pub finished: bool,
    /// Lower range.
    pub min_range: i64,
    /// Upper range.
    pub max_range: i64,
    /// `lastWorldTimeSample` (`Clock.cs:492`): world seconds observed at the
    /// last tick.
    pub last_world_time_sample: i64,
}

/// Persisted quest shape (spec 5). Round-trips identically.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuestSnapshot {
    /// Quest name.
    pub name: String,
    /// Faction id.
    pub faction_id: u16,
    /// Tasks.
    pub tasks: Vec<TaskSnapshot>,
    /// Clocks.
    pub clocks: Vec<ClockSnapshot>,
    /// Active journal.
    pub journal: Vec<JournalEntry>,
    /// Notebook entries.
    pub finished_journal: Vec<JournalEntry>,
    /// Display-only messages queued for presentation.
    pub pending_messages: Vec<u16>,
    /// Display messages already drained by the tick cycle.
    pub delivered_messages: Vec<u16>,
    /// Ticks left.
    pub ticks_to_end: u32,
    /// Break flag.
    pub quest_break: bool,
    /// Success flag.
    pub success: bool,
    /// Complete flag.
    pub complete: bool,
    /// Acceptance time.
    pub start_time: i64,
    /// `UID` (`Quest.cs:119-122`).
    pub uid: u64,
    /// `QuestTombstoned` (`Quest.cs:230-237`).
    pub tombstoned: bool,
    /// `QuestTombstoneTime` (`Quest.cs:239-242`).
    pub tombstone_time: i64,
    /// Local boolean variables (`TaskType::Variable`).
    pub variables: BTreeMap<String, bool>,
    /// Global boolean variables (`TaskType::GlobalVarLink`).
    pub global_vars: BTreeMap<String, bool>,
    /// Resolved messages.
    pub messages: Vec<(u16, String)>,
    /// Recorded failures.
    pub failures: Vec<QuestFailure>,
    /// Pending requests.
    pub requests: Vec<QuestRequest>,
    /// Skipped entries.
    pub journal_skipped: u32,
    /// Next log step.
    pub next_step_id: u32,
    /// Next note id.
    pub next_note_id: u32,
    /// World-time surrogate.
    pub date_time: i64,
    /// Ticks inside the current second.
    pub tick_in_second: u32,
    /// `EndQuest` already applied.
    pub end_applied: bool,
}

/// Quest offer eligibility (`QuestListsManager.cs:331-355`).
///
/// Measured rule for a guild quest:
/// `(status == Nonmember && minReq == 0) || (minReq < 10 && minReq <= rank) ||
/// (minReq >= 10 && minReq <= rep)`. This function covers the member branch;
/// [`offer_eligible_for_status`] adds the non-member gate.
pub fn offer_eligible(min_req: u16, rank: u16, reputation: i32) -> bool {
    if min_req == 0 {
        return true;
    }
    if min_req < 10 {
        i32::from(rank) >= i32::from(min_req)
    } else {
        reputation >= i32::from(min_req)
    }
}

/// Membership-aware offer rule (`QuestListsManager.cs:351`): a non-member is
/// eligible only for `minReq == 0`; a member uses [`offer_eligible`].
pub fn offer_eligible_for_status(
    min_req: u16,
    rank: u16,
    reputation: i32,
    member: bool,
) -> bool {
    if !member {
        min_req == 0
    } else {
        offer_eligible(min_req, rank, reputation)
    }
}

/// Quest driver: 10 Hz accumulator (`QuestMachine.cs:45,299-327`) and the
/// quest list (`Tick` at `:441-516`).
#[derive(Debug, Clone, PartialEq)]
pub struct QuestMachine {
    rng: DfRandom,
    quests: Vec<Quest>,
    accumulator: f64,
    ticks_elapsed: u64,
}

impl QuestMachine {
    /// Empty machine with a seeded LCG.
    pub fn new(seed: u32) -> Self {
        Self {
            rng: DfRandom::srand(seed),
            quests: Vec::new(),
            accumulator: 0.0,
            ticks_elapsed: 0,
        }
    }

    /// Accumulates `delta_seconds` and runs whole 10 Hz ticks
    /// (`QuestMachine.cs:318-321`).
    pub fn update(&mut self, delta_seconds: f64) {
        if !delta_seconds.is_finite() || delta_seconds <= 0.0 {
            return;
        }
        let tick = 1.0 / f64::from(TICKS_PER_SECOND);
        self.accumulator += delta_seconds;
        // The epsilon absorbs the binary representation of 0.1 so that
        // update(1.0) runs exactly ten ticks.
        while self.accumulator + 1e-9 >= tick {
            self.accumulator -= tick;
            if self.accumulator < 0.0 {
                self.accumulator = 0.0;
            }
            self.tick();
        }
    }

    /// One exact quest tick.
    pub fn tick(&mut self) {
        self.ticks_elapsed = self.ticks_elapsed.saturating_add(1);
        for quest in &mut self.quests {
            quest.tick();
        }
    }

    /// Accepts a quest and fixes its start time (`Quest.cs:281-285`).
    pub fn start_quest(&mut self, mut quest: Quest) {
        quest.start_time = self.elapsed_seconds();
        self.quests.push(quest);
    }

    /// Quest by name (first match).
    pub fn quest(&self, name: &str) -> Option<&Quest> {
        self.quests.iter().find(|quest| quest.name == name)
    }

    /// Active (non-complete) quests in acceptance order.
    pub fn active_quests(&self) -> impl Iterator<Item = &Quest> {
        self.quests.iter().filter(|quest| !quest.is_complete())
    }

    /// Every quest, including completed ones.
    pub fn quests(&self) -> impl Iterator<Item = &Quest> {
        self.quests.iter()
    }

    /// Seeded LCG used by clocks, parse seeding and quest rolls.
    pub fn rng(&mut self) -> &mut DfRandom {
        &mut self.rng
    }

    /// Whole seconds elapsed at 10 Hz.
    pub fn elapsed_seconds(&self) -> i64 {
        (self.ticks_elapsed / u64::from(TICKS_PER_SECOND)) as i64
    }

    /// Exact ticks run so far.
    pub fn ticks_elapsed(&self) -> u64 {
        self.ticks_elapsed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn action(effect: OpcodeEffect, message_id: u16) -> QuestAction {
        QuestAction::new(effect).with_message(message_id)
    }

    fn triggered_task(symbol: &str, actions: Vec<QuestAction>) -> QuestTask {
        let mut task = QuestTask::new(symbol, TaskType::Standard);
        for item in actions {
            task.add_action(item);
        }
        task.is_triggered = true;
        task
    }

    fn opcode(type_id: u8, message_id: u16) -> DecodedOpcode {
        DecodedOpcode {
            type_id,
            message_id,
            supported: opcode_coverage(type_id).is_some_and(|row| row.effect.is_some()),
        }
    }

    #[test]
    fn time_values_follow_clock_formulas() {
        assert_eq!(parse_time_seconds("1:30"), Some(5_400));
        assert_eq!(parse_time_seconds("2.3:00"), Some(183_600));
        assert_eq!(parse_time_seconds("45"), Some(2_700));
        assert_eq!(parse_time_seconds("0.0:0"), Some(0));
        assert_eq!(parse_time_seconds(""), None);
        assert_eq!(parse_time_seconds("-1:00"), None);
    }

    #[test]
    fn clock_recipes_draw_expected_ranges() {
        let mut rng = DfRandom::srand(0x5EED);
        for _ in 0..200 {
            let drawn = clock_duration(ClockRecipe::Default, &mut rng);
            assert!((CLOCK_MIN_SECONDS..=CLOCK_MAX_SECONDS).contains(&drawn));
        }
        assert_eq!(clock_duration(ClockRecipe::Fixed(42), &mut rng), 42);
        assert_eq!(clock_duration(ClockRecipe::Range(1_200, 600), &mut rng), 1_200);
        for _ in 0..200 {
            let drawn = clock_duration(ClockRecipe::Range(600, 1_200), &mut rng);
            assert!((600..=1_200).contains(&drawn));
        }
        assert_eq!(clock_duration(ClockRecipe::Text("1:30"), &mut rng), 5_400);
        assert_eq!(clock_duration(ClockRecipe::Fixed(-5), &mut rng), 0);
    }

    #[test]
    fn clock_ticks_to_zero_exactly_once() {
        let mut rng = DfRandom::srand(7);
        let mut clock = QuestClock::new("timer", ClockRecipe::Fixed(5), &mut rng);
        assert!(!clock.tick(3));
        assert_eq!(clock.remaining(), 2);
        assert!(clock.tick(7));
        assert_eq!(clock.remaining(), 0);
        assert!(clock.is_finished());
        assert!(!clock.tick(1));
        assert_eq!(clock.days(), 0);
        assert_eq!(QuestClock::new("d", ClockRecipe::Fixed(86_400), &mut rng).days(), 1);
        assert_eq!(QuestClock::new("d", ClockRecipe::Fixed(86_401), &mut rng).days(), 2);
    }

    #[test]
    fn clock_expiry_triggers_its_task() {
        let mut rng = DfRandom::srand(11);
        let mut quest = Quest::new("timer", 0);
        quest.add_message(1, "tick");
        quest.add_clock(QuestClock::new("timer", ClockRecipe::Fixed(5), &mut rng));
        quest.add_task(triggered_task("timer", vec![action(OpcodeEffect::LogMessage, 1)]));
        quest.task_mut("timer").unwrap().is_triggered = false;
        for _ in 0..49 {
            quest.tick();
        }
        assert!(!quest.task("timer").unwrap().is_triggered);
        quest.tick();
        assert!(quest.task("timer").unwrap().is_triggered);
        assert_eq!(quest.journal().len(), 1);
    }

    #[test]
    fn give_pc_is_the_only_success_and_close_reputation_is_a_request() {
        let mut quest = Quest::new("giver", 41);
        quest.add_message(1, "done");
        quest.add_task(triggered_task("or", vec![action(OpcodeEffect::GivePc, 1)]));
        quest.tick();
        assert!(quest.succeeded());
        quest.end_quest();
        let changes: Vec<_> = quest
            .requests()
            .iter()
            .filter(|request| matches!(request, QuestRequest::ChangeReputation { .. }))
            .collect();
        assert_eq!(
            changes,
            vec![&QuestRequest::ChangeReputation {
                faction_id: Some(41),
                delta: Some(SUCCESS_REPUTATION),
                propagate: true,
            }]
        );

        let mut failed = Quest::new("fail", 41);
        failed.tick();
        failed.end_quest();
        assert!(matches!(
            failed.requests().last(),
            Some(QuestRequest::ChangeReputation {
                delta: Some(FAILURE_REPUTATION),
                propagate: true,
                ..
            })
        ));

        let mut neutral = Quest::new("neutral", 0);
        neutral.end_quest();
        assert!(!neutral
            .requests()
            .iter()
            .any(|request| matches!(request, QuestRequest::ChangeReputation { .. })));
    }

    #[test]
    fn end_quest_completes_after_two_ticks() {
        let mut quest = Quest::new("end", 0);
        quest.end_quest();
        assert_eq!(quest.ticks_to_end(), END_TICKS_TO_END);
        quest.tick();
        assert!(!quest.is_complete());
        quest.tick();
        assert!(quest.is_complete());
        assert_eq!(quest.outcome(), Some(QuestOutcome::Failure(QuestFailure::NoSuccess)));
    }

    #[test]
    fn journal_has_four_entries_and_three_finished_logs() {
        let mut quest = Quest::new("journal", 0);
        for id in 1..=4 {
            quest.add_message(id, format!("m{id}"));
        }
        quest.add_task(triggered_task(
            "log",
            vec![
                action(OpcodeEffect::LogMessage, 1),
                action(OpcodeEffect::LogMessage, 2),
                action(OpcodeEffect::LogMessage, 3),
                action(OpcodeEffect::JournalNote, 4),
            ],
        ));
        quest.tick();
        assert_eq!(quest.journal().len(), 4);
        quest.end_quest();
        assert_eq!(quest.finished_journal().len(), 3);
        assert!(quest.finished_journal().iter().all(|entry| entry.step_id < NOTE_STEP_BASE));
        let snapshot = quest.snapshot();
        let reopened = Quest::from_snapshot(snapshot.clone());
        assert_eq!(reopened.snapshot(), snapshot);
        assert_eq!(reopened.journal().len(), 4);
    }

    #[test]
    fn missing_message_never_succeeds_and_never_panics() {
        let mut quest = Quest::new("missing", 0);
        quest.add_task(triggered_task(
            "bad",
            vec![
                action(OpcodeEffect::GivePc, 1),
                action(OpcodeEffect::LogMessage, NO_MESSAGE_ID),
                action(OpcodeEffect::JournalNote, 999),
            ],
        ));
        quest.tick();
        assert!(quest.succeeded());
        assert_eq!(quest.journal_skipped(), 2);
        quest.end_quest();
        quest.tick();
        quest.tick();
        assert_eq!(
            quest.outcome(),
            Some(QuestOutcome::Failure(QuestFailure::MissingMessage(NO_MESSAGE_ID)))
        );
        assert!(quest.finished_journal().is_empty());
    }

    #[test]
    fn journal_dedupes_by_explicit_step_id() {
        let mut quest = Quest::new("dedupe", 0);
        quest.add_message(1, "a");
        quest.add_message(2, "b");
        quest.add_task(triggered_task(
            "log",
            vec![
                action(OpcodeEffect::LogMessage, 1).with_step_id(7),
                action(OpcodeEffect::LogMessage, 2).with_step_id(7),
            ],
        ));
        quest.tick();
        assert_eq!(quest.journal().len(), 1);
        assert_eq!(quest.journal()[0].message_id, 2);
    }

    #[test]
    fn persist_until_clears_or_rearms() {
        let mut active = Quest::new("active", 0);
        active.add_message(1, "m");
        let mut target = QuestTask::new("target", TaskType::Standard);
        target.is_triggered = true;
        active.add_task(target);
        let mut persist = QuestTask::new("persist", TaskType::PersistUntil)
            .with_target("target");
        persist.add_action(action(OpcodeEffect::LogMessage, 1));
        persist.is_triggered = true;
        active.add_task(persist);
        active.tick();
        assert!(!active.task("persist").unwrap().is_triggered);
        assert!(active.task("persist").unwrap().actions[0].is_complete);

        let mut inactive = Quest::new("inactive", 0);
        inactive.add_message(1, "m");
        inactive.add_task(QuestTask::new("target", TaskType::Standard));
        let mut persist = QuestTask::new("persist", TaskType::PersistUntil)
            .with_target("target");
        persist.add_action(action(OpcodeEffect::LogMessage, 1).with_step_id(3));
        persist.is_triggered = true;
        inactive.add_task(persist);
        inactive.tick();
        assert!(!inactive.task("persist").unwrap().actions[0].is_complete);
        inactive.tick();
        assert_eq!(inactive.task("persist").unwrap().actions[0].initialisations, 1);
        assert_eq!(inactive.journal().len(), 1);
    }

    #[test]
    fn always_on_primary_stops_secondary_only_starts() {
        let mut quest = Quest::new("always", 0);
        let mut task = QuestTask::new("t", TaskType::Standard);
        task.add_action(QuestAction::always_on(OpcodeEffect::PcAt).with_trigger_met(false));
        task.add_action(QuestAction::always_on(OpcodeEffect::PcAt).with_trigger_met(true));
        task.is_triggered = true;
        quest.add_task(task);
        quest.tick();
        assert!(quest.task("t").unwrap().is_triggered);
        quest.task_mut("t").unwrap().actions[0].trigger_met = false;
        quest.task_mut("t").unwrap().actions[1].trigger_met = false;
        quest.tick();
        assert!(!quest.task("t").unwrap().is_triggered);
    }

    #[test]
    fn completed_action_never_runs_again() {
        let mut quest = Quest::new("complete", 0);
        quest.add_message(1, "m");
        quest.add_task(triggered_task(
            "t",
            vec![action(OpcodeEffect::LogMessage, 1).completed()],
        ));
        quest.tick();
        assert!(quest.journal().is_empty());
    }

    #[test]
    fn quest_break_stops_remaining_tasks() {
        let mut quest = Quest::new("break", 0);
        for id in 1..=2 {
            quest.add_message(id, format!("m{id}"));
        }
        quest.add_task(triggered_task("a", vec![action(OpcodeEffect::LogMessage, 1)]));
        quest.add_task(triggered_task("b", vec![action(OpcodeEffect::DisplayMessage, 2)]));
        quest.add_task(triggered_task("c", vec![action(OpcodeEffect::GivePc, 3)]));
        quest.add_task(triggered_task("d", vec![action(OpcodeEffect::GivePc, 4)]));
        quest.tick();
        assert_eq!(quest.journal().len(), 1);
        assert_eq!(quest.pending_messages(), &[2]);
        assert!(!quest.succeeded());
        assert!(!quest.task("c").unwrap().actions[0].is_complete);
        assert!(!quest.task("d").unwrap().actions[0].is_complete);
        assert!(!quest.quest_break);
    }

    #[test]
    fn offer_eligibility_matches_quest_lists_manager() {
        assert!(offer_eligible(0, 0, 0));
        assert!(!offer_eligible(5, 4, 0));
        assert!(offer_eligible(5, 5, 0));
        assert!(!offer_eligible(20, 0, 19));
        assert!(offer_eligible(20, 0, 20));
        assert!(!offer_eligible_for_status(5, 9, 9, false));
        assert!(offer_eligible_for_status(0, 0, 0, false));
    }

    #[test]
    fn snapshot_roundtrip_is_identical() {
        let mut rng = DfRandom::srand(99);
        let mut quest = Quest::new("snap", 3);
        quest.add_message(1, "m");
        quest.add_clock(QuestClock::new("clock", ClockRecipe::Fixed(10), &mut rng));
        quest.add_task(triggered_task("t", vec![action(OpcodeEffect::LogMessage, 1)]));
        quest.tick();
        quest.end_quest();
        quest.tick();
        let snapshot = quest.snapshot();
        assert_eq!(Quest::from_snapshot(snapshot.clone()).snapshot(), snapshot);
    }

    #[test]
    fn snapshot_carries_task_actions_and_keeps_executing() {
        let mut rng = DfRandom::srand(0xBEEF);
        let mut quest = Quest::new("shape", 7);
        quest.add_message(1, "logged");
        quest.add_message(2, "later");
        quest.add_clock(QuestClock::new("short", ClockRecipe::Fixed(10), &mut rng));
        quest.add_clock(QuestClock::new("long", ClockRecipe::Range(20, 30), &mut rng));

        let mut first = QuestTask::new("first", TaskType::Standard);
        first.add_action(action(OpcodeEffect::LogMessage, 1).with_step_id(4));
        first.is_triggered = true;
        quest.add_task(first);

        let mut waiting = QuestTask::new("waiting", TaskType::GlobalVarLink)
            .with_global_var("global_var");
        waiting.add_action(QuestAction::trigger(OpcodeEffect::PcAt).with_trigger_met(false));
        waiting.add_action(action(OpcodeEffect::LogMessage, 2));
        quest.add_task(waiting);

        quest.tick();
        assert_eq!(quest.journal().len(), 1);
        assert_eq!(quest.snapshot().tasks[0].actions.len(), 1);
        assert_eq!(quest.snapshot().tasks[1].actions.len(), 2);
        assert!(quest.snapshot().tasks[1].has_trigger_conditions);
        assert_eq!(
            quest.snapshot().tasks[1].global_var.as_deref(),
            Some("global_var")
        );

        let snapshot = quest.snapshot();
        let mut restored = Quest::from_snapshot(snapshot.clone());
        assert_eq!(restored.snapshot(), snapshot);

        // The restored task still runs: flip its trigger and tick.
        let trigger = &mut restored
            .task_mut("waiting")
            .expect("restored task")
            .actions[0];
        trigger.trigger_met = true;
        restored.tick();
        assert_eq!(restored.journal().len(), 2);
        assert_eq!(restored.journal()[1].message_id, 2);
    }

    #[test]
    fn display_message_is_pending_and_never_journals() {
        let mut quest = Quest::new("display", 0);
        quest.add_message(9, "hello");
        quest.add_task(triggered_task(
            "say",
            vec![action(OpcodeEffect::DisplayMessage, 9)],
        ));
        quest.tick();
        assert_eq!(quest.pending_messages(), &[9]);
        assert!(quest.journal().is_empty());
        quest.end_quest();
        quest.tick();
        quest.tick();
        assert!(quest.finished_journal().is_empty());
    }

    #[test]
    fn decode_opcode_measures_exactly_two_fields() {
        let mut record = [0u8; QBN_OPCODE_LEN];
        record[QBN_OPCODE_TYPE_OFFSET] = 0x17;
        record[QBN_OPCODE_MESSAGE_OFFSET] = 0x34;
        record[QBN_OPCODE_MESSAGE_OFFSET + 1] = 0x12;
        assert_eq!(
            decode_opcode(&record),
            Some(DecodedOpcode {
                type_id: 0x17,
                message_id: 0x1234,
                supported: true,
            })
        );
        assert_eq!(decode_opcode(&record[..86]), None);
        let mut sentinel = record;
        sentinel[QBN_OPCODE_MESSAGE_OFFSET] = 0xFF;
        sentinel[QBN_OPCODE_MESSAGE_OFFSET + 1] = 0xFF;
        assert_eq!(decode_opcode(&sentinel).unwrap().message_id, NO_MESSAGE_ID);
    }

    #[test]
    fn negative_opcode_records_a_failure() {
        let mut quest = Quest::new("negative", 0);
        let failure = quest.execute_opcode(opcode(0x48, NO_MESSAGE_ID));
        assert_eq!(failure, Err(QuestFailure::UnsupportedOpcode(0x48)));
        assert_eq!(quest.failures(), &[QuestFailure::UnsupportedOpcode(0x48)]);
        let mut covered = Quest::new("covered", 0);
        covered.add_message(1, "m");
        assert_eq!(covered.execute_opcode(opcode(0x17, 1)), Ok(()));
        assert_eq!(covered.journal().len(), 1);
    }

    #[test]
    fn coverage_table_is_complete() {
        assert_eq!(OPCODE_COVERAGE.len(), 70);
        assert_eq!(covered_opcode_count(), 62);
        assert_eq!(negative_opcode_count(), 7);
        assert!(OPCODE_COVERAGE
            .iter()
            .any(|row| row.type_id == 0x3E && row.effect.is_some()));
        for row in &OPCODE_COVERAGE {
            assert!(!row.anchor.is_empty());
            if row.effect.is_none() {
                assert!(matches!(row.type_id, 0 | 5 | 0x3F | 0x40 | 0x48 | 0x4E | 0x57));
            }
        }
    }

    #[test]
    fn coverage_table_pins_semantics() {
        let mut negative: Vec<u8> = OPCODE_COVERAGE
            .iter()
            .filter(|row| row.effect.is_none())
            .map(|row| row.type_id)
            .collect();
        negative.sort_unstable();
        assert_eq!(negative, vec![0x00, 0x05, 0x3F, 0x40, 0x48, 0x4E, 0x57]);

        let effect = |type_id: u8| opcode_coverage(type_id).and_then(|row| row.effect);
        for type_id in [0x04, 0x24, 0x4C] {
            assert_eq!(effect(type_id), Some(OpcodeEffect::GiveItem));
        }
        // Only the `give pc` verb sets success; no documented type uses it.
        assert!(!OPCODE_COVERAGE
            .iter()
            .any(|row| row.effect == Some(OpcodeEffect::GivePc)));
        assert_eq!(effect(0x17), Some(OpcodeEffect::LogMessage));
        assert_eq!(effect(0x06), Some(OpcodeEffect::EndQuest));
        assert_eq!(effect(0x33), Some(OpcodeEffect::DisplayMessage));
        assert_eq!(effect(0x4D), Some(OpcodeEffect::LevelCompleted));

        // Every documented effect has an execution path: either an internal
        // branch of `run_action` or a `QuestRequest` the owner can resolve.
        for row in OPCODE_COVERAGE.iter().filter(|row| row.effect.is_some()) {
            let effect = row.effect.expect("filtered");
            let internal = matches!(
                effect,
                OpcodeEffect::LogMessage
                    | OpcodeEffect::JournalNote
                    | OpcodeEffect::DisplayMessage
                    | OpcodeEffect::RemoveLogMessage
                    | OpcodeEffect::EndQuest
                    | OpcodeEffect::ClearTask
                    | OpcodeEffect::StartTask
                    | OpcodeEffect::UnsetTask
                    | OpcodeEffect::StartTimer
                    | OpcodeEffect::StopTimer
                    | OpcodeEffect::GivePc
            );
            assert!(
                internal || QuestRequest::from_effect(effect, NO_MESSAGE_ID, None).is_some(),
                "type {:#04X} ({:?}) has no execution path",
                row.type_id,
                effect
            );
        }
    }

    #[test]
    fn headless_task_starts_triggered() {
        assert!(QuestTask::new("startup", TaskType::Headless).is_triggered);
        for kind in [
            TaskType::Standard,
            TaskType::PersistUntil,
            TaskType::Variable,
            TaskType::GlobalVarLink,
        ] {
            assert!(!QuestTask::new("t", kind).is_triggered, "{kind:?} must wait");
        }

        let mut quest = Quest::new("startup", 0);
        quest.add_message(1, "boot");
        let mut task = QuestTask::new("startup", TaskType::Headless);
        task.add_action(action(OpcodeEffect::LogMessage, 1));
        quest.add_task(task);
        quest.tick();
        assert_eq!(quest.journal().len(), 1);
        assert!(quest.task("startup").unwrap().prev_triggered);
    }

    #[test]
    fn end_quest_breaks_the_tick_before_ending() {
        let mut quest = Quest::new("end-break", 0);
        let mut first = QuestTask::new("a", TaskType::Standard);
        first.add_action(action(OpcodeEffect::EndQuest, NO_MESSAGE_ID));
        first.is_triggered = true;
        quest.add_task(first);
        let mut second = QuestTask::new("b", TaskType::Standard);
        second.add_action(action(OpcodeEffect::GivePc, NO_MESSAGE_ID));
        second.is_triggered = true;
        quest.add_task(second);
        quest.tick();
        assert_eq!(quest.ticks_to_end(), END_TICKS_TO_END);
        assert!(!quest.succeeded());
        assert!(!quest.task("b").unwrap().actions[0].is_complete);
        assert_eq!(quest.task("b").unwrap().actions[0].updates, 0);
        assert!(!quest.quest_break);
    }

    #[test]
    fn remove_log_removes_the_step_not_every_message() {
        let mut quest = Quest::new("remove", 0);
        quest.add_message(7, "shared");
        let mut task = QuestTask::new("t", TaskType::Standard);
        task.add_action(action(OpcodeEffect::LogMessage, 7).with_step_id(1));
        task.add_action(action(OpcodeEffect::LogMessage, 7).with_step_id(2));
        task.add_action(action(OpcodeEffect::RemoveLogMessage, 7).with_step_id(1));
        task.is_triggered = true;
        quest.add_task(task);
        quest.tick();
        assert_eq!(
            quest
                .journal()
                .iter()
                .map(|entry| entry.step_id)
                .collect::<Vec<_>>(),
            vec![2]
        );

        // Without a resolved step the message id stays the declared fallback.
        let mut fallback = Quest::new("fallback", 0);
        fallback.add_message(7, "shared");
        let mut task = QuestTask::new("t", TaskType::Standard);
        task.add_action(action(OpcodeEffect::LogMessage, 7).with_step_id(1));
        task.add_action(action(OpcodeEffect::LogMessage, 7).with_step_id(2));
        task.add_action(action(OpcodeEffect::RemoveLogMessage, 7));
        task.is_triggered = true;
        fallback.add_task(task);
        fallback.tick();
        assert!(fallback.journal().is_empty());
    }

    #[test]
    fn repeating_actions_follow_the_effect_policy() {
        for (effect, repeats) in [
            (OpcodeEffect::PlaySound, true),
            (OpcodeEffect::PcAt, true),
            (OpcodeEffect::LogMessage, false),
        ] {
            let mut quest = Quest::new("repeat", 0);
            quest.add_message(1, "m");
            let mut task = QuestTask::new("t", TaskType::Standard);
            task.add_action(action(effect, 1).with_target("sound"));
            task.is_triggered = true;
            quest.add_task(task);
            quest.tick();
            quest.tick();
            let action = &quest.task("t").unwrap().actions[0];
            assert_eq!(action.updates, if repeats { 2 } else { 1 }, "{effect:?}");
            assert_eq!(action.is_complete, !repeats, "{effect:?}");
            assert_eq!(effect.completes_after_update(), !repeats);
        }
        // The sound request is re-emitted on both ticks.
        let mut quest = Quest::new("sound", 0);
        let mut task = QuestTask::new("t", TaskType::Standard);
        task.add_action(action(OpcodeEffect::PlaySound, NO_MESSAGE_ID));
        task.is_triggered = true;
        quest.add_task(task);
        quest.tick();
        quest.tick();
        assert_eq!(
            quest
                .requests()
                .iter()
                .filter(|request| matches!(request, QuestRequest::PlaySound { .. }))
                .count(),
            2
        );
    }

    #[test]
    fn travel_clock_multiplies_the_one_way_time() {
        let mut rng = DfRandom::srand(3);
        assert_eq!(
            clock_duration(ClockRecipe::ReturnTrip { one_way_seconds: 100 }, &mut rng),
            250
        );
        // 2.5 · 3 = 7.5 truncates toward zero, exactly like `(int)` in DFU.
        assert_eq!(
            clock_duration(ClockRecipe::ReturnTrip { one_way_seconds: 3 }, &mut rng),
            7
        );
        assert_eq!(
            clock_duration(
                ClockRecipe::ReturnTrip {
                    one_way_seconds: -10
                },
                &mut rng
            ),
            0
        );

        let clock = QuestClock::new("trip", ClockRecipe::Fixed(0), &mut rng).with_flag(0x10);
        assert!(clock.wants_travel_time());
        assert!(clock.flag & 0x10 == 0x10);

        let mut quest = Quest::new("travel", 0);
        quest.add_clock(QuestClock::new(
            "trip",
            ClockRecipe::ReturnTrip { one_way_seconds: 40 },
            &mut rng,
        )
        .with_flag(0x10));
        assert_eq!(
            quest.requests(),
            &[QuestRequest::TravelTime {
                seconds: Some(100),
                flag: Some(0x10),
            }]
        );
        let mut plain = Quest::new("plain", 0);
        plain.add_clock(QuestClock::new("timer", ClockRecipe::Fixed(5), &mut rng));
        assert!(plain.requests().is_empty());
        assert!(plain.clocks[0].travel_request().is_none());
    }

    #[test]
    fn missing_display_message_does_not_break_the_tick() {
        let mut quest = Quest::new("missing-display", 0);
        let mut first = QuestTask::new("say", TaskType::Standard);
        first.add_action(action(OpcodeEffect::DisplayMessage, 999));
        first.is_triggered = true;
        quest.add_task(first);
        let mut second = QuestTask::new("reward", TaskType::Standard);
        second.add_action(action(OpcodeEffect::GivePc, NO_MESSAGE_ID));
        second.is_triggered = true;
        quest.add_task(second);
        quest.tick();
        assert_eq!(quest.failures(), &[QuestFailure::MissingMessage(999)]);
        assert!(quest.pending_messages().is_empty());
        assert!(quest.succeeded(), "the tick continued: no break was set");
        assert!(!quest.quest_break);
    }

    #[test]
    fn pending_messages_drain_once() {
        let mut quest = Quest::new("mailbox", 0);
        quest.add_message(7, "seven");
        assert_eq!(quest.execute_opcode(opcode(0x33, 7)), Ok(()));
        assert_eq!(quest.execute_opcode(opcode(0x33, 7)), Ok(()));
        assert_eq!(quest.pending_messages(), &[7, 7]);
        assert_eq!(quest.take_pending_messages(), vec![7, 7]);
        assert!(quest.take_pending_messages().is_empty());
        assert!(quest.pending_messages().is_empty());

        // The tick cycle drains into the delivered sequence.
        let mut quest = Quest::new("cycle", 0);
        quest.add_message(9, "nine");
        let mut task = QuestTask::new("say", TaskType::Standard);
        task.add_action(action(OpcodeEffect::DisplayMessage, 9));
        task.is_triggered = true;
        quest.add_task(task);
        quest.tick();
        assert_eq!(quest.pending_messages(), &[9]);
        quest.tick();
        assert!(quest.pending_messages().is_empty());
        assert_eq!(quest.delivered_messages(), &[9]);
    }

    #[test]
    fn snapshot_carries_uid_tombstone_and_world_time_sample() {
        let mut rng = DfRandom::srand(21);
        let mut quest = Quest::new("identity", 0).with_uid(0xDEAD_BEEF);
        quest.add_clock(QuestClock::new("timer", ClockRecipe::Fixed(60), &mut rng));
        for _ in 0..10 {
            quest.tick();
        }
        quest.tombstone(42);
        let snapshot = quest.snapshot();
        assert_eq!(snapshot.uid, 0xDEAD_BEEF);
        assert!(snapshot.tombstoned);
        assert_eq!(snapshot.tombstone_time, 42);
        assert_eq!(snapshot.clocks[0].last_world_time_sample, 1);
        assert_eq!(Quest::from_snapshot(snapshot.clone()).snapshot(), snapshot);
    }

    #[test]
    fn variables_are_real_and_drive_variable_tasks() {
        let mut quest = Quest::new("vars", 0);
        quest.add_message(1, "local");
        quest.add_message(2, "global");
        assert!(!quest.var("met"));
        assert!(!quest.global_var("g_met"));

        let mut local = QuestTask::new("met", TaskType::Variable);
        local.add_action(action(OpcodeEffect::LogMessage, 1));
        quest.add_task(local);
        let mut global = QuestTask::new("linked", TaskType::GlobalVarLink).with_global_var("g_met");
        global.add_action(action(OpcodeEffect::LogMessage, 2));
        quest.add_task(global);

        quest.tick();
        assert!(quest.journal().is_empty());

        quest.set_var("met", true);
        quest.set_global_var("g_met", true);
        assert!(quest.var("met"));
        quest.tick();
        assert!(quest.task("met").unwrap().is_triggered);
        assert!(quest.task("linked").unwrap().is_triggered);
        assert_eq!(quest.journal().len(), 2);

        let snapshot = quest.snapshot();
        assert_eq!(snapshot.variables.get("met"), Some(&true));
        assert_eq!(snapshot.global_vars.get("g_met"), Some(&true));
        let restored = Quest::from_snapshot(snapshot.clone());
        assert_eq!(restored.snapshot(), snapshot);
        assert!(restored.var("met"));
        assert!(restored.global_var("g_met"));

        quest.set_global_var("g_met", false);
        quest.tick();
        assert!(!quest.task("linked").unwrap().is_triggered);
    }

    #[test]
    fn machine_runs_exactly_ten_ticks_per_second() {
        let mut machine = QuestMachine::new(0x5EED);
        machine.update(1.0);
        assert_eq!(machine.ticks_elapsed(), 10);
        machine.update(0.05);
        assert_eq!(machine.ticks_elapsed(), 10);
        machine.update(0.05);
        assert_eq!(machine.ticks_elapsed(), 11);
        machine.tick();
        let mut quest = Quest::new("accepted", 0);
        quest.add_message(1, "m");
        machine.start_quest(quest);
        assert_eq!(machine.quest("accepted").unwrap().start_time, 1);
    }
}
