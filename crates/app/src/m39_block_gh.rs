//! M39/UI-3 — blocks **G + H** (automap · travel · transport · teleport ·
//! conversation · guilds · quest · court · daedra · coven), 6 + 9 windows of
//! plan §1/§2.
//!
//! **Two blocks in one module.** [`evidence`] reports the **first** letter
//! (`G`) and [`evidence_g`] the second (`H`); see [`crate::m39_block_cd`] for
//! why the frozen `evidence` signature needs the companion.
//!
//! ## Data provenance (plan §3.1 — zero invention)
//!
//! | family | source |
//! |---|---|
//! | automap, travel, teleport | retail `MAPS.BSA` (`MAPNAMES`/`MAPTABLE`/`MAPPITEM`/`MAPDITEM`), read through [`crate::m10_probe::named_entry`] |
//! | travel cost/time | retail `CLIMATE.PAK` + `daggerfall_heart::travel` |
//! | `time.day` | `daggerfall_heart::clock` + `daggerfall_heart::calendar` |
//! | `transport.*` | `daggerfall_behavior::economy::TransportMode` + `daggerfall_heart::travel` |
//! | guilds | `daggerfall_behavior::guild` (`guild`, `cure_price`, `donate`, `training_advance_seeded`) |
//! | court | `daggerfall_behavior::law` (`penalty_amount`, `cap_fine_to_gold`) |
//! | conversation, coven | retail `FACTION.TXT` via `daggerfall_behavior::factions` |
//! | `quest.offers` | retail `.QBN` offer records of `ARENA2` |
//!
//! A family whose retail/domain source is missing never invents a value: the
//! intent is `Rejected` with a citable [`RULE_*`] reason (plan §3.1) and the
//! read model stays absent. `DaggerfallDaedraSummonedWindow` (031) is
//! read-only by plan §3.4 and reports its reason.
//!
//! ## Determinism (plan §3.2)
//!
//! No RNG, no clock, no hash-order: every number comes from retail bytes or a
//! domain formula, and the interaction digest is `fnv1a64` of the interaction
//! signature ([`Interaction::sealed`]). Two runs over the same state hand back
//! the same `Interaction` and the same digest.

use std::path::Path;

use daggerfall_behavior::economy::TransportMode;
use daggerfall_behavior::factions::{FactionGraph, FACTION_FILENAME};
use daggerfall_behavior::guild::{
    cure_price, donate, guild, training_advance_seeded, GuildKind, Holiday, ALL_SERVICES,
};
use daggerfall_behavior::law::{cap_fine_to_gold, penalty_amount, Crimes};
use daggerfall_heart::calendar;
use daggerfall_heart::clock::GameClock;
use daggerfall_heart::travel::{
    calculate_travel_time, calculate_trip_cost, Transport, TravelOptions,
};
use daggerfall_nervous::events::WindowId;
use daggerfall_parser_bevy_bridge::ui_compat::UiCompatError;
use daggerfall_parser_bevy_bridge::ui_spec::UiAssets;
use daggerfall_parser_rs::climate::ClimateMap;
use daggerfall_parser_rs::{bsa, maps};
use daggerfall_perception::read_models::{M39SocialModels, M39TravelModels};
use daggerfall_perception::ui_census;
use daggerfall_perception::ui_shell::canonical_id_for;

use crate::m10_probe::named_entry;
use crate::m18_demo::M19_SEED;
use crate::m18_probe::{map_pixel, region_suffixes};
use crate::m39_state::{BlockEvidence, Interaction, M39State, ModalOutcome};

/// Windows of block G (plan §2 partition: 51 = 16+1+2+7+7+3+6+9).
pub const BLOCK_G_WINDOWS: usize = 6;
/// Windows of block H (plan §2 partition: 51 = 16+1+2+7+7+3+6+9).
pub const BLOCK_H_WINDOWS: usize = 9;

/// Census names of block G, in plan §1 ordinal order.
pub const G_WINDOWS: [&str; BLOCK_G_WINDOWS] = [
    "DaggerfallAutomapWindow",
    "DaggerfallExteriorAutomapWindow",
    "DaggerfallTeleportPopUp",
    "DaggerfallTransportWindow",
    "DaggerfallTravelMapWindow",
    "DaggerfallTravelPopUp",
];

/// Census names of block H, in plan §1 ordinal order.
pub const H_WINDOWS: [&str; BLOCK_H_WINDOWS] = [
    "DaggerfallCourtWindow",
    "DaggerfallDaedraSummonedWindow",
    "DaggerfallGuildServiceCureDisease",
    "DaggerfallGuildServiceDonation",
    "DaggerfallGuildServicePopupWindow",
    "DaggerfallGuildServiceTraining",
    "DaggerfallQuestOfferWindow",
    "DaggerfallTalkWindow",
    "DaggerfallWitchesCovenPopupWindow",
];

/// Reason of a window that is not this module's (plan §3.3: never a default).
const RULE_WINDOW: &str = "m39: blocks G+H deliver only their own 15 windows";
/// Reason of a name outside the frozen census.
const RULE_CENSUS: &str = "m39: window is not in the frozen census";
/// Reason of an intent whose retail source is missing (plan §3.1).
const RULE_NO_MAPS: &str = "m39: retail MAPS.BSA/CLIMATE.PAK unavailable in ARENA2";
/// Reason of an intent whose faction source is missing.
const RULE_NO_FACTIONS: &str = "m39: retail FACTION.TXT unavailable or empty";
/// Reason: the flow reached its declared end (never a silent no-op).
const RULE_EXHAUSTED: &str = "m39: the window has no further flow step";
/// Reason of a flow with no pending retail offer.
const RULE_NO_OFFER: &str = "m39: no retail .QBN offer pending in ARENA2";
/// Read-only reason of `DaggerfallDaedraSummonedWindow` (plan §3.4).
const RULE_READ_ONLY_031: &str =
    "m39: DaedraSummoned paints a summoned line only (no asset, no delta; plan §3.4)";
/// Reason of a trip the domain formula refuses.
const RULE_TRIP: &str = "m39: heart::travel refuses the retail route";
/// Reason of a court penalty outside the crime table.
const RULE_NO_PENALTY: &str = "m39: law::penalty_amount has no penalty for the crime";

/// One window of a block G/H roster, with its delivered bookkeeping.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Slot {
    /// Census name.
    name: &'static str,
    /// Handle resolved through the census (plan §3.3).
    window: WindowId,
    /// Census ordinal (`index + 1`).
    ordinal: u16,
    /// Intents delivered so far.
    intents: usize,
    /// Intents delivered so far: one per interaction with ≥1 outcome (`<= intents`).
    delivered: usize,
    /// Read models that moved so far.
    changed: usize,
    /// Flow steps consumed (drives the next step of the flow).
    progress: usize,
    /// Selected region of the travel flows (index into [`Retail::regions`]).
    region: usize,
    /// Read-only reason when the window declares one (plan §3.4).
    read_only: Option<&'static str>,
    /// Delta of the last interaction.
    last_delta: Vec<(&'static str, i64)>,
}

impl Slot {
    fn new(name: &'static str) -> Result<Self, UiCompatError> {
        let window = canonical_id_for(name).ok_or(UiCompatError::Navigation(RULE_CENSUS))?;
        let index = ui_census::census()
            .iter()
            .position(|row| row.name == name)
            .ok_or(UiCompatError::Navigation(RULE_CENSUS))?;
        let ordinal = u16::try_from(index + 1).map_err(|_| UiCompatError::Navigation(RULE_CENSUS))?;
        Ok(Self {
            name,
            window,
            ordinal,
            intents: 0,
            delivered: 0,
            changed: 0,
            progress: 0,
            region: 0,
            read_only: None,
            last_delta: Vec::new(),
        })
    }
}

/// One retail region of `MAPS.BSA`, projected from its four records.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
struct Region {
    /// `MAPNAMES.<nnn>` suffix (the archive's own region id).
    suffix: String,
    /// World map-pixel of the region's first location (`MAPTABLE`).
    pixel: (i32, i32),
    /// Location names of the region (`MAPNAMES`).
    names: Vec<String>,
    /// Exterior block cells of every location of the region (`MAPPITEM`).
    cells: u16,
}

/// One faction of retail `FACTION.TXT`, projected to the five fields the flows
/// read (the graph itself is not kept: the blocks only project values).
#[derive(Debug, Clone, PartialEq, Eq)]
struct FactionRow {
    /// `#id` of the block.
    id: i32,
    /// Literal `name`.
    name: String,
    /// `rep`.
    rep: i32,
    /// `power`.
    power: i32,
    /// `summon` (Daedra/witches summoning slot).
    summon: i32,
}

/// Retail + domain sources the two blocks project, loaded once per session.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
struct Retail {
    /// Regions of `MAPS.BSA`, in `MAPNAMES` suffix order.
    regions: Vec<Region>,
    /// Blocks of the first retail dungeon layout (`MAPDITEM`).
    dungeon_blocks: u16,
    /// Starting blocks of that layout (the automap's entrance marker).
    dungeon_start: u16,
    /// Factions of `FACTION.TXT`.
    factions: Vec<FactionRow>,
    /// Faction ruling the first region (`region_faction`), when declared.
    region_faction: Option<usize>,
    /// `.QBN` offer records present in `ARENA2`.
    quest_offers: u16,
}

/// Blocks G and H own this state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GhState {
    /// Block G roster (6 windows, plan §1 ordinals 023/033/062/064/065/066).
    g: [Slot; BLOCK_G_WINDOWS],
    /// Block H roster (9 windows, plan §1 ordinals 030/031/034/035/036/037/053/060/072).
    h: [Slot; BLOCK_H_WINDOWS],
    /// Retail + domain sources.
    retail: Retail,
    /// Terrain of the travel flows (`CLIMATE.PAK`); `None` without retail.
    terrain: Option<ClimateMap>,
    /// Session clock of `time.day`.
    clock: GameClock,
}

impl GhState {
    /// Fills the block state from the retail bank and the `ARENA2` directory.
    ///
    /// The pair is resolved by Mode A; a missing retail family never fails the
    /// session (other blocks share it): it leaves the family empty and the
    /// flows that need it `Rejected` with their citable rule.
    pub fn from_retail(assets: &UiAssets, arena2: &Path) -> Result<Self, UiCompatError> {
        let _ = assets;
        let mut g = Vec::with_capacity(BLOCK_G_WINDOWS);
        for name in G_WINDOWS {
            g.push(Slot::new(name)?);
        }
        let mut h = Vec::with_capacity(BLOCK_H_WINDOWS);
        for name in H_WINDOWS {
            h.push(Slot::new(name)?);
        }
        let g: [Slot; BLOCK_G_WINDOWS] = g.try_into().map_err(|_| UiCompatError::Navigation(RULE_WINDOW))?;
        let h: [Slot; BLOCK_H_WINDOWS] = h.try_into().map_err(|_| UiCompatError::Navigation(RULE_WINDOW))?;

        Ok(Self {
            g,
            h,
            retail: load_retail(arena2),
            terrain: load_terrain(arena2),
            clock: GameClock::classic_default(),
        })
    }
}

// ─────────────────────────────────────────────────────────────────────────
// Retail loaders (plan §3.1: file of ARENA2, never a literal)
// ─────────────────────────────────────────────────────────────────────────

/// Loads every retail family the two blocks project.
///
/// Each record is optional on its own: a family whose file/record is missing
/// stays empty and the flows that need it reject with [`RULE_NO_MAPS`],
/// [`RULE_NO_FACTIONS`] or [`RULE_NO_OFFER`].
fn load_retail(arena2: &Path) -> Retail {
    let (factions, region_faction) = load_factions(arena2);
    let mut retail = Retail {
        factions,
        region_faction,
        quest_offers: count_retail(arena2, "QBN"),
        ..Retail::default()
    };
    if let Some((regions, blocks, start)) = load_maps(arena2) {
        retail.regions = regions;
        retail.dungeon_blocks = blocks;
        retail.dungeon_start = start;
    }
    retail
}

/// Parses `MAPS.BSA` into its regions plus the first dungeon layout.
fn load_maps(arena2: &Path) -> Option<(Vec<Region>, u16, u16)> {
    let data = std::fs::read(arena2.join("MAPS.BSA")).ok()?;
    let archive = bsa::parse(&data).ok()?;
    let mut regions = Vec::new();
    let mut dungeon: Option<(u16, u16)> = None;
    for (_, suffix) in region_suffixes(&archive) {
        // Empty region slots are not index regions: `MAPS.BSA` declares more
        // `MAPNAMES` slots than regions with locations (M18 measured 62 slots,
        // 61 with locations), so an empty record is skipped, never an error.
        let Ok(names_record) = named_entry(&archive, &data, &format!("MAPNAMES.{suffix}")) else {
            continue;
        };
        let Ok(pitem_record) = named_entry(&archive, &data, &format!("MAPPITEM.{suffix}")) else {
            continue;
        };
        if names_record.is_empty() || pitem_record.is_empty() {
            continue;
        }
        let Ok(names) = maps::parse_map_names(names_record) else {
            continue;
        };
        if names.is_empty() {
            continue;
        }
        let Ok(locations) = maps::parse_pitem(pitem_record, names.len()) else {
            continue;
        };
        let pixel = named_entry(&archive, &data, &format!("MAPTABLE.{suffix}"))
            .ok()
            .and_then(|record| maps::parse_map_table(record, names.len()).ok())
            .and_then(|table| table.first().map(map_pixel))
            .unwrap_or((0, 0));
        let cells = locations
            .iter()
            .try_fold(0usize, |total, location| total.checked_add(location.cell_count()))
            .unwrap_or(usize::MAX);
        if dungeon.is_none() {
            if let Ok(ditem) = named_entry(&archive, &data, &format!("MAPDITEM.{suffix}")) {
                for location in &locations {
                    let Ok(Some(layout)) = maps::parse_ditem(ditem, location.location_id) else {
                        continue;
                    };
                    let blocks = u16::try_from(layout.blocks.len()).unwrap_or(u16::MAX);
                    let start = u16::try_from(
                        layout
                            .blocks
                            .iter()
                            .filter(|block| block.is_starting_block)
                            .count(),
                    )
                    .unwrap_or(u16::MAX);
                    dungeon = Some((blocks, start));
                    break;
                }
            }
        }
        regions.push(Region {
            suffix,
            pixel,
            names,
            cells: u16::try_from(cells).unwrap_or(u16::MAX),
        });
    }
    let (blocks, start) = dungeon.unwrap_or((0, 0));
    Some((regions, blocks, start))
}

/// Parses `FACTION.TXT` into the projected rows the flows read, plus the
/// faction ruling region 0 (`factions::region_faction`).
fn load_factions(arena2: &Path) -> (Vec<FactionRow>, Option<usize>) {
    let Ok(text) = std::fs::read(arena2.join(FACTION_FILENAME)) else {
        return (Vec::new(), None);
    };
    let Ok(text) = String::from_utf8(text) else {
        return (Vec::new(), None);
    };
    let Ok(graph) = FactionGraph::parse(&text, 0) else {
        return (Vec::new(), None);
    };
    let rows: Vec<FactionRow> = graph
        .ids()
        .iter()
        .filter_map(|id| graph.get(*id))
        .map(|faction| FactionRow {
            id: faction.id,
            name: faction.name.clone(),
            rep: faction.rep,
            power: faction.power,
            summon: faction.summon,
        })
        .collect();
    let ruler = graph
        .region_faction(0)
        .ok()
        .and_then(|id| rows.iter().position(|row| row.id == id));
    (rows, ruler)
}

/// Counts the retail records of `extension` in `ARENA2` (the `.QBN` offers).
fn count_retail(arena2: &Path, extension: &str) -> u16 {
    let Ok(entries) = std::fs::read_dir(arena2) else {
        return 0;
    };
    let count = entries
        .filter_map(|entry| entry.ok())
        .filter(|entry| {
            entry
                .path()
                .extension()
                .is_some_and(|found| found.eq_ignore_ascii_case(extension))
        })
        .count();
    u16::try_from(count).unwrap_or(u16::MAX)
}

/// Parses `CLIMATE.PAK` (the travel flows' terrain).
fn load_terrain(arena2: &Path) -> Option<ClimateMap> {
    let bytes = std::fs::read(arena2.join(CLIMATE_FILE)).ok()?;
    ClimateMap::parse(&bytes).ok()
}

/// Retail file of the travel terrain.
const CLIMATE_FILE: &str = "CLIMATE.PAK";

// ─────────────────────────────────────────────────────────────────────────
// Commit helpers (plan §3.6: nothing clamped, nothing unchanged)
// ─────────────────────────────────────────────────────────────────────────

/// Commits one `i64`-shaped route only when the value really moves.
fn commit_int(
    state: &mut M39State,
    path: &'static str,
    value: i64,
    outcomes: &mut Vec<ModalOutcome>,
    delta: &mut Vec<(&'static str, i64)>,
) -> Result<(), UiCompatError> {
    let before = state.value(path)?;
    if before == value.to_string() {
        return Ok(());
    }
    let outcome = state.commit_i64(path, value)?;
    M39State::validate(&outcome)?;
    outcomes.push(outcome);
    delta.push((path, 1));
    Ok(())
}

/// Commits one text-shaped route only when the text really changes.
///
/// A text route has no numeric magnitude: the reported magnitude is `1` — one
/// step of the flow (plan §4.1 prints the same `+N` shape for every route).
fn commit_str(
    state: &mut M39State,
    path: &'static str,
    value: &str,
    outcomes: &mut Vec<ModalOutcome>,
    delta: &mut Vec<(&'static str, i64)>,
) -> Result<(), UiCompatError> {
    let before = state.value(path)?;
    if before == value {
        return Ok(());
    }
    let outcome = state.commit_text(path, value)?;
    M39State::validate(&outcome)?;
    outcomes.push(outcome);
    delta.push((path, 1));
    Ok(())
}

/// Commits one flag-shaped route only when the flag really moves.
///
/// `M39State` freezes `commit_i64`/`commit_text` (plan §4); the flag is written
/// through the family setter with the very same falsifier: the `before` comes
/// from the read model, and an unchanged flag is never reported.
fn commit_flag(
    state: &mut M39State,
    path: &'static str,
    value: bool,
    outcomes: &mut Vec<ModalOutcome>,
    delta: &mut Vec<(&'static str, i64)>,
) -> Result<(), UiCompatError> {
    let before = state.value(path)?;
    let after = value.to_string();
    if before == after {
        return Ok(());
    }
    if !state.models.set_bool(path, value) {
        return Err(UiCompatError::Input(RULE_TYPE));
    }
    let outcome = ModalOutcome::Committed {
        read_model: path,
        before,
        after,
    };
    M39State::validate(&outcome)?;
    outcomes.push(outcome);
    delta.push((path, 1));
    Ok(())
}

/// Route refused by its own type/range (plan §3.6 — never clamped).
const RULE_TYPE: &str = "m39: route refuses this value (type or range)";

// ─────────────────────────────────────────────────────────────────────────
// Travel projection (retail terrain + heart::travel)
// ─────────────────────────────────────────────────────────────────────────

/// One quoted trip of the travel/transport flows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Trip {
    /// Days the popup shows (`mins/1440`, plus the remainder day).
    days: u16,
    /// Gold the popup charges (`TripCost::total`).
    gold: i64,
    /// Hours the transport window shows.
    hours: u16,
    /// Total minutes (drives the clock of `time.day`).
    minutes: u64,
}

/// Quoted trip between two retail map-pixels for one transport mode.
fn quote(
    terrain: &ClimateMap,
    from: (i32, i32),
    to: (i32, i32),
    mode: TransportMode,
) -> Option<Trip> {
    let options = TravelOptions {
        travel_ship: mode == TransportMode::Ship,
        ..TravelOptions::default()
    };
    let transport = match mode {
        TransportMode::Horse => Transport::Horse,
        TransportMode::Cart => Transport::Cart,
        TransportMode::Foot | TransportMode::Ship => Transport::Foot,
    };
    let lookup = |x: i32, y: i32| terrain.climate_index(x, y);
    let report = calculate_travel_time(&lookup, from, to, transport, &options).ok()?;
    let cost = calculate_trip_cost(
        report.minutes,
        report.ocean_pixels,
        options.sleep_inn,
        false,
        mode == TransportMode::Ship,
        options.travel_ship,
    );
    let days = u16::try_from(report.days()).ok()?;
    let hours = u16::try_from(report.minutes.div_ceil(60)).ok()?;
    Some(Trip {
        days,
        gold: cost.total,
        hours,
        minutes: report.minutes,
    })
}

// ─────────────────────────────────────────────────────────────────────────
// Window dispatch
// ─────────────────────────────────────────────────────────────────────────

/// Which block a name belongs to, and its roster index.
fn position(name: &str) -> Result<(char, usize), UiCompatError> {
    if let Some(index) = G_WINDOWS.iter().position(|candidate| *candidate == name) {
        return Ok(('G', index));
    }
    if let Some(index) = H_WINDOWS.iter().position(|candidate| *candidate == name) {
        return Ok(('H', index));
    }
    Err(UiCompatError::Navigation(RULE_WINDOW))
}

/// Delivers the interactions of blocks G and H for one window name.
///
/// One call is one intent of that window's flow (plan §2): the flow reads its
/// retail/domain source, commits every read model that really moves and hands
/// back the sealed [`Interaction`]. A name outside the two rosters is a
/// navigation error (plan §3.3 — never a default).
pub fn interact(
    state: &mut M39State,
    name: &'static str,
) -> Result<Vec<Interaction>, UiCompatError> {
    let (block, index) = position(name)?;
    // Plan §3.3: the name resolves through the census and must be the handle
    // this state was built for.
    let declared = match block {
        'G' => state.gh.g[index].window,
        _ => state.gh.h[index].window,
    };
    if declared != canonical_id_for(name).ok_or(UiCompatError::Navigation(RULE_CENSUS))? {
        return Err(UiCompatError::Navigation(RULE_WINDOW));
    }

    let mut slot = match block {
        'G' => state.gh.g[index].clone(),
        _ => state.gh.h[index].clone(),
    };
    let mut outcomes = Vec::new();
    let mut delta = Vec::new();

    let mut rejected: Option<&'static str> = None;
    match name {
        "DaggerfallAutomapWindow" => g_automap_dungeon(state, &mut slot, &mut outcomes, &mut delta, &mut rejected)?,
        "DaggerfallExteriorAutomapWindow" => {
            g_automap_exterior(state, &mut slot, &mut outcomes, &mut delta, &mut rejected)?
        }
        "DaggerfallTeleportPopUp" => g_teleport(state, &mut slot, &mut outcomes, &mut delta, &mut rejected)?,
        "DaggerfallTransportWindow" => {
            g_transport(state, &mut slot, &mut outcomes, &mut delta, &mut rejected)?
        }
        "DaggerfallTravelMapWindow" => {
            g_travel_map(state, &mut slot, &mut outcomes, &mut delta, &mut rejected)?
        }
        "DaggerfallTravelPopUp" => {
            g_travel_popup(state, &mut slot, &mut outcomes, &mut delta, &mut rejected)?
        }
        "DaggerfallCourtWindow" => h_court(state, &mut slot, &mut outcomes, &mut delta, &mut rejected)?,
        "DaggerfallDaedraSummonedWindow" => {
            let outcome = M39State::read_only(RULE_READ_ONLY_031);
            M39State::validate(&outcome)?;
            slot.read_only = Some(RULE_READ_ONLY_031);
            outcomes.push(outcome);
        }
        "DaggerfallGuildServiceCureDisease" => {
            h_cure(state, &mut slot, &mut outcomes, &mut delta, &mut rejected)?
        }
        "DaggerfallGuildServiceDonation" => {
            h_donation(state, &mut slot, &mut outcomes, &mut delta, &mut rejected)?
        }
        "DaggerfallGuildServicePopupWindow" => {
            h_service(state, &mut slot, &mut outcomes, &mut delta, &mut rejected)?
        }
        "DaggerfallGuildServiceTraining" => {
            h_training(state, &mut slot, &mut outcomes, &mut delta, &mut rejected)?
        }
        "DaggerfallQuestOfferWindow" => h_quest(state, &mut slot, &mut outcomes, &mut delta, &mut rejected)?,
        "DaggerfallTalkWindow" => h_talk(state, &mut slot, &mut outcomes, &mut delta, &mut rejected)?,
        "DaggerfallWitchesCovenPopupWindow" => {
            h_coven(state, &mut slot, &mut outcomes, &mut delta, &mut rejected)?
        }
        _ => return Err(UiCompatError::Navigation(RULE_WINDOW)),
    }

    if let Some(reason) = rejected {
        let outcome = M39State::rejected(reason);
        M39State::validate(&outcome)?;
        outcomes.push(outcome);
    }

    // One interaction is one intent: it counts once when it commits at least
    // one outcome, whatever the number of routes it moves.
    let delivered = usize::from(!outcomes.is_empty());
    slot.intents += 1;
    slot.delivered += delivered;
    slot.changed += delta.len();
    slot.last_delta = delta.clone();
    if rejected.is_none() {
        slot.progress += 1;
    }

    match block {
        'G' => state.gh.g[index] = slot.clone(),
        _ => state.gh.h[index] = slot.clone(),
    }

    let interaction = Interaction {
        id: slot.ordinal,
        name,
        block,
        intents: 1,
        delivered,
        outcomes,
        delta,
        read_only: slot.read_only,
        digest: String::new(),
    }
    .sealed();
    Ok(vec![interaction])
}

// ─────────────────────────────────────────────────────────────────────────
// Block G flows
// ─────────────────────────────────────────────────────────────────────────

/// 023 `DaggerfallAutomapWindow` — dungeon automap: reveal the next block of
/// the first retail dungeon layout and paint its census.
fn g_automap_dungeon(
    state: &mut M39State,
    slot: &mut Slot,
    outcomes: &mut Vec<ModalOutcome>,
    delta: &mut Vec<(&'static str, i64)>,
    rejected: &mut Option<&'static str>,
) -> Result<(), UiCompatError> {
    let total = state.gh.retail.dungeon_blocks;
    if total == 0 {
        *rejected = Some(RULE_NO_MAPS);
        return Ok(());
    }
    let discovered = slot.progress + 1;
    if discovered > usize::from(total) {
        *rejected = Some(RULE_EXHAUSTED);
        return Ok(());
    }
    commit_int(state, "m39.automap.chunks", i64::from(total), outcomes, delta)?;
    commit_int(
        state,
        "m39.automap.markers",
        i64::from(state.gh.retail.dungeon_start),
        outcomes,
        delta,
    )?;
    commit_int(
        state,
        "m39.automap.dungeon_blocks",
        discovered as i64,
        outcomes,
        delta,
    )?;
    Ok(())
}

/// 033 `DaggerfallExteriorAutomapWindow` — exterior automap of the first
/// retail region: reveal one location nameplate per intent over the region's
/// real exterior cell census.
fn g_automap_exterior(
    state: &mut M39State,
    slot: &mut Slot,
    outcomes: &mut Vec<ModalOutcome>,
    delta: &mut Vec<(&'static str, i64)>,
    rejected: &mut Option<&'static str>,
) -> Result<(), UiCompatError> {
    let Some(region) = state.gh.retail.regions.first().cloned() else {
        *rejected = Some(RULE_NO_MAPS);
        return Ok(());
    };
    let revealed = slot.progress + 1;
    if revealed > region.names.len() {
        *rejected = Some(RULE_EXHAUSTED);
        return Ok(());
    }
    commit_int(state, "m39.automap.chunks", i64::from(region.cells), outcomes, delta)?;
    commit_int(state, "m39.automap.markers", revealed as i64, outcomes, delta)?;
    Ok(())
}

/// 062 `DaggerfallTeleportPopUp` — cycle the retail location names of the
/// window's region as teleport destinations.
fn g_teleport(
    state: &mut M39State,
    slot: &mut Slot,
    outcomes: &mut Vec<ModalOutcome>,
    delta: &mut Vec<(&'static str, i64)>,
    rejected: &mut Option<&'static str>,
) -> Result<(), UiCompatError> {
    let Some(region) = state.gh.retail.regions.get(slot.region).cloned() else {
        *rejected = Some(RULE_NO_MAPS);
        return Ok(());
    };
    if region.names.is_empty() {
        *rejected = Some(RULE_NO_MAPS);
        return Ok(());
    }
    let destination = region.names[slot.progress % region.names.len()].clone();
    commit_str(state, "m39.teleport.destination", &destination, outcomes, delta)?;
    Ok(())
}

/// 064 `DaggerfallTransportWindow` — one retail transport mode per intent.
fn g_transport(
    state: &mut M39State,
    slot: &mut Slot,
    outcomes: &mut Vec<ModalOutcome>,
    delta: &mut Vec<(&'static str, i64)>,
    rejected: &mut Option<&'static str>,
) -> Result<(), UiCompatError> {
    let (Some(terrain), Some(from), regions) = (
        state.gh.terrain.as_ref(),
        state.gh.retail.regions.first().map(|region| region.pixel),
        state.gh.retail.regions.len(),
    ) else {
        *rejected = Some(RULE_NO_MAPS);
        return Ok(());
    };
    if regions < 2 {
        *rejected = Some(RULE_NO_MAPS);
        return Ok(());
    }
    // One mode and one destination per intent: both step with the flow, so a
    // repeated intent always moves the quote (never a silent no-op).
    let mode = TransportMode::ALL[slot.progress % TransportMode::ALL.len()];
    let target = (slot.progress + 1) % regions;
    let to = state.gh.retail.regions[target].pixel;
    let Some(trip) = quote(terrain, from, to, mode) else {
        *rejected = Some(RULE_TRIP);
        return Ok(());
    };
    commit_int(state, "m39.transport.time_hours", i64::from(trip.hours), outcomes, delta)?;
    commit_int(state, "m39.transport.fare", trip.gold, outcomes, delta)?;
    Ok(())
}

/// 065 `DaggerfallTravelMapWindow` — the retail region roster plus the quote
/// of the newly selected region.
fn g_travel_map(
    state: &mut M39State,
    slot: &mut Slot,
    outcomes: &mut Vec<ModalOutcome>,
    delta: &mut Vec<(&'static str, i64)>,
    rejected: &mut Option<&'static str>,
) -> Result<(), UiCompatError> {
    let (Some(terrain), Some(from), regions) = (
        state.gh.terrain.as_ref(),
        state.gh.retail.regions.first().map(|region| region.pixel),
        state.gh.retail.regions.len(),
    ) else {
        *rejected = Some(RULE_NO_MAPS);
        return Ok(());
    };
    if regions == 0 {
        *rejected = Some(RULE_NO_MAPS);
        return Ok(());
    }
    slot.region = (slot.region + 1) % regions;
    let selected = state.gh.retail.regions[slot.region].clone();
    let Some(trip) = quote(terrain, from, selected.pixel, TransportMode::Foot) else {
        *rejected = Some(RULE_TRIP);
        return Ok(());
    };
    commit_int(state, "m39.travel.regions", regions as i64, outcomes, delta)?;
    commit_str(state, "m39.travel.selected_region", &selected.suffix, outcomes, delta)?;
    commit_int(state, "m39.travel.cost_days", i64::from(trip.days), outcomes, delta)?;
    commit_int(state, "m39.travel.cost_gold", trip.gold, outcomes, delta)?;
    Ok(())
}

/// 066 `DaggerfallTravelPopUp` — the same retail quote, plus the arrival day
/// the session clock reaches (`heart::clock` + `heart::calendar`).
fn g_travel_popup(
    state: &mut M39State,
    slot: &mut Slot,
    outcomes: &mut Vec<ModalOutcome>,
    delta: &mut Vec<(&'static str, i64)>,
    rejected: &mut Option<&'static str>,
) -> Result<(), UiCompatError> {
    let (Some(terrain), Some(from), regions) = (
        state.gh.terrain.as_ref(),
        state.gh.retail.regions.first().map(|region| region.pixel),
        state.gh.retail.regions.len(),
    ) else {
        *rejected = Some(RULE_NO_MAPS);
        return Ok(());
    };
    if regions < 2 {
        *rejected = Some(RULE_NO_MAPS);
        return Ok(());
    }
    // The popup confirms the trip to its own next destination: the destination
    // steps with the flow, so a repeated intent always moves a route.
    let target = (slot.progress + 1) % regions;
    let destination = state.gh.retail.regions[target].clone();
    let Some(trip) = quote(terrain, from, destination.pixel, TransportMode::Foot) else {
        *rejected = Some(RULE_TRIP);
        return Ok(());
    };
    commit_str(state, "m39.travel.selected_region", &destination.suffix, outcomes, delta)?;
    commit_int(state, "m39.travel.cost_days", i64::from(trip.days), outcomes, delta)?;
    state.gh.clock.raise_seconds(trip.minutes.saturating_mul(60));
    let day = calendar::day_of_month(&state.gh.clock);
    let day = u16::try_from(day).map_err(|_| UiCompatError::Input(RULE_TRIP))?;
    commit_int(state, "m39.time.day", i64::from(day), outcomes, delta)?;
    Ok(())
}

// ─────────────────────────────────────────────────────────────────────────
// Block H flows
// ─────────────────────────────────────────────────────────────────────────

/// Crimes the court flow cycles (retail `Crimes` table, `law.rs`).
const COURT_CRIMES: [Crimes; 3] = [Crimes::Trespassing, Crimes::Assault, Crimes::Murder];

/// Guild kinds the service flows cycle (retail `GuildKind` table, `guild.rs`).
const SERVICE_GUILDS: [GuildKind; 2] = [GuildKind::Mages, GuildKind::Fighters];

/// Label of a guild kind (the variant's own name, never a retail caption the
/// `guild` organ does not declare).
fn guild_label(kind: GuildKind) -> &'static str {
    match kind {
        GuildKind::Fighters => "Fighters",
        GuildKind::Mages => "Mages",
        GuildKind::Thieves => "Thieves",
        GuildKind::DarkBrotherhood => "DarkBrotherhood",
        GuildKind::Temple(_) => "Temple",
        GuildKind::KnightlyOrder => "KnightlyOrder",
    }
}

/// 030 `DaggerfallCourtWindow` — the retail penalty of one crime, one crime per
/// intent (`law::penalty_amount` + `law::cap_fine_to_gold`, no RNG).
fn h_court(
    state: &mut M39State,
    slot: &mut Slot,
    outcomes: &mut Vec<ModalOutcome>,
    delta: &mut Vec<(&'static str, i64)>,
    rejected: &mut Option<&'static str>,
) -> Result<(), UiCompatError> {
    let crime = COURT_CRIMES[slot.progress % COURT_CRIMES.len()];
    let Some(penalty) = penalty_amount(0, crime) else {
        *rejected = Some(RULE_NO_PENALTY);
        return Ok(());
    };
    let (fine, days) = cap_fine_to_gold(penalty, 0, i32::MAX);
    let ruling = format!("{}: {fine} gold, {days} days", crime.name());
    commit_str(state, "m39.court.ruling", &ruling, outcomes, delta)?;
    Ok(())
}

/// 034 `DaggerfallGuildServiceCureDisease` — the retail cure price of one
/// guild per intent (`guild::cure_price`).
fn h_cure(
    state: &mut M39State,
    slot: &mut Slot,
    outcomes: &mut Vec<ModalOutcome>,
    delta: &mut Vec<(&'static str, i64)>,
    rejected: &mut Option<&'static str>,
) -> Result<(), UiCompatError> {
    let _ = rejected;
    let kind = SERVICE_GUILDS[slot.progress % SERVICE_GUILDS.len()];
    let def = guild(kind);
    let price = cure_price(1, &def, 0, 0, Holiday::None);
    commit_str(state, "m39.service.kind", guild_label(kind), outcomes, delta)?;
    commit_int(state, "m39.service.charged", i64::from(price), outcomes, delta)?;
    commit_flag(state, "m39.service.executed", price > 0, outcomes, delta)?;
    Ok(())
}

/// 035 `DaggerfallGuildServiceDonation` — `guild::donate` over the faction
/// reputation of the window's region.
fn h_donation(
    state: &mut M39State,
    slot: &mut Slot,
    outcomes: &mut Vec<ModalOutcome>,
    delta: &mut Vec<(&'static str, i64)>,
    rejected: &mut Option<&'static str>,
) -> Result<(), UiCompatError> {
    let Some(faction) = state.gh.retail.region_faction.and_then(|index| {
        state.gh.retail.factions.get(index).cloned()
    }) else {
        *rejected = Some(RULE_NO_FACTIONS);
        return Ok(());
    };
    let amount = i32::try_from(slot.progress + 1).unwrap_or(i32::MAX).saturating_mul(10);
    let outcome = donate(faction.rep, amount, u32::try_from(slot.progress % 100).unwrap_or(0));
    commit_int(state, "m39.service.charged", i64::from(amount), outcomes, delta)?;
    commit_flag(state, "m39.service.executed", outcome.applied, outcomes, delta)?;
    Ok(())
}

/// 036 `DaggerfallGuildServicePopupWindow` — one declared guild service per
/// intent (the `GuildService` roster of `guild.rs`).
fn h_service(
    state: &mut M39State,
    slot: &mut Slot,
    outcomes: &mut Vec<ModalOutcome>,
    delta: &mut Vec<(&'static str, i64)>,
    rejected: &mut Option<&'static str>,
) -> Result<(), UiCompatError> {
    let _ = rejected;
    let service = ALL_SERVICES[slot.progress % ALL_SERVICES.len()];
    commit_str(state, "m39.service.kind", service.name(), outcomes, delta)?;
    Ok(())
}

/// 037 `DaggerfallGuildServiceTraining` — the trainer's seeded advance
/// (`guild::training_advance_seeded`) quoted as the gold charged at level 1.
fn h_training(
    state: &mut M39State,
    slot: &mut Slot,
    outcomes: &mut Vec<ModalOutcome>,
    delta: &mut Vec<(&'static str, i64)>,
    rejected: &mut Option<&'static str>,
) -> Result<(), UiCompatError> {
    let _ = rejected;
    let kind = SERVICE_GUILDS[slot.progress % SERVICE_GUILDS.len()];
    let def = guild(kind);
    let multiplier = i16::try_from(def.training_skills.len()).unwrap_or(i16::MAX);
    let seed = M19_SEED.wrapping_add(slot.progress as u64);
    let advance = training_advance_seeded(multiplier, seed);
    commit_str(state, "m39.service.kind", guild_label(kind), outcomes, delta)?;
    commit_int(state, "m39.service.charged", i64::from(advance), outcomes, delta)?;
    commit_flag(state, "m39.service.executed", advance > 0, outcomes, delta)?;
    Ok(())
}

/// 053 `DaggerfallQuestOfferWindow` — accept one retail `.QBN` offer per
/// intent; the remaining offers are the retail census minus the accepted ones.
fn h_quest(
    state: &mut M39State,
    slot: &mut Slot,
    outcomes: &mut Vec<ModalOutcome>,
    delta: &mut Vec<(&'static str, i64)>,
    rejected: &mut Option<&'static str>,
) -> Result<(), UiCompatError> {
    let offers = state.gh.retail.quest_offers;
    if offers == 0 {
        *rejected = Some(RULE_NO_OFFER);
        return Ok(());
    }
    if slot.progress >= usize::from(offers) {
        *rejected = Some(RULE_OFFER_EXHAUSTED);
        return Ok(());
    }
    let remaining = u16::try_from(usize::from(offers) - slot.progress).unwrap_or(0);
    commit_int(state, "m39.quest.offers", i64::from(remaining), outcomes, delta)?;
    commit_str(state, "m39.quest.decision", "accepted", outcomes, delta)?;
    Ok(())
}

/// 060 `DaggerfallTalkWindow` — retail faction of the region as the selected
/// topic, one faction per intent (`factions::FactionGraph`).
fn h_talk(
    state: &mut M39State,
    slot: &mut Slot,
    outcomes: &mut Vec<ModalOutcome>,
    delta: &mut Vec<(&'static str, i64)>,
    rejected: &mut Option<&'static str>,
) -> Result<(), UiCompatError> {
    let factions = state.gh.retail.factions.len();
    if factions == 0 {
        *rejected = Some(RULE_NO_FACTIONS);
        return Ok(());
    }
    let selected = state.gh.retail.factions[slot.progress % factions].clone();
    let answer = format!("{} rep {} power {}", selected.name, selected.rep, selected.power);
    commit_int(state, "m39.talk.topics", factions as i64, outcomes, delta)?;
    commit_str(state, "m39.talk.faction", &selected.name, outcomes, delta)?;
    commit_str(state, "m39.talk.selected.topic", &selected.name, outcomes, delta)?;
    commit_str(state, "m39.talk.answer", &answer, outcomes, delta)?;
    Ok(())
}

/// 072 `DaggerfallWitchesCovenPopupWindow` — one summonable faction of the
/// retail `FACTION.TXT` per intent (`summon` field).
fn h_coven(
    state: &mut M39State,
    slot: &mut Slot,
    outcomes: &mut Vec<ModalOutcome>,
    delta: &mut Vec<(&'static str, i64)>,
    rejected: &mut Option<&'static str>,
) -> Result<(), UiCompatError> {
    let summonable: Vec<FactionRow> = state
        .gh
        .retail
        .factions
        .iter()
        .filter(|faction| faction.summon > 0)
        .cloned()
        .collect();
    if summonable.is_empty() {
        *rejected = Some(RULE_NO_FACTIONS);
        return Ok(());
    }
    let selected = summonable[slot.progress % summonable.len()].clone();
    let service = format!("{} summons {}", selected.name, selected.summon);
    commit_str(state, "m39.coven.service", &service, outcomes, delta)?;
    Ok(())
}

/// Reason of a quest flow whose retail offers are all accepted.
const RULE_OFFER_EXHAUSTED: &str = "m39: every retail .QBN offer of the session was accepted";

// ─────────────────────────────────────────────────────────────────────────
// Evidence
// ─────────────────────────────────────────────────────────────────────────

/// Evidence of block **G** (automap, travel, transport, teleport, time).
///
/// `ok` requires all six windows delivered, every reported delta to name a
/// declared route of [`M39TravelModels`] with a non-zero magnitude, and the
/// flows to have run — otherwise the citable rule that failed.
pub fn evidence(state: &M39State) -> BlockEvidence {
    block_evidence('G', &state.gh.g, BLOCK_G_WINDOWS, false)
}

/// Evidence of block **H** ([`evidence`] carries the `G` half of this module).
///
/// The one read-only window of the block (`031`) is counted, never treated as
/// a delta (plan §3.4).
pub fn evidence_g(state: &M39State) -> BlockEvidence {
    block_evidence('H', &state.gh.h, BLOCK_H_WINDOWS, true)
}

/// Composes one [`BlockEvidence`] from a roster of delivered windows.
fn block_evidence(
    block: char,
    slots: &[Slot],
    declared: usize,
    has_read_only: bool,
) -> BlockEvidence {
    let intents: usize = slots.iter().map(|slot| slot.intents).sum();
    let delivered: usize = slots.iter().map(|slot| slot.delivered).sum();
    let changed: usize = slots.iter().map(|slot| slot.changed).sum();
    let read_only = slots.iter().filter(|slot| slot.read_only.is_some()).count();

    let paths: &[&str] = if block == 'G' {
        M39TravelModels::PATHS
    } else {
        M39SocialModels::PATHS
    };

    let every_window = slots.iter().all(|slot| slot.delivered > 0);
    let deltas_citable = slots.iter().all(|slot| {
        slot.last_delta
            .iter()
            .all(|(path, moved)| paths.contains(path) && *moved != 0)
    });
    let read_only_declared = !has_read_only || read_only == 1;
    let flowed = changed > 0;

    let reason = if slots.len() != declared {
        Some(RULE_ROSTER)
    } else if !every_window {
        Some(RULE_UNDELIVERED)
    } else if !deltas_citable {
        Some(RULE_DELTA)
    } else if !read_only_declared {
        Some(RULE_READ_ONLY)
    } else if !flowed {
        Some(RULE_FLOWED)
    } else {
        None
    };

    BlockEvidence {
        block,
        windows: declared,
        flows: slots.len(),
        intents,
        delivered,
        state_delta: changed,
        read_only,
        ok: reason.is_none(),
        reason,
    }
}

/// Reason: the roster does not match the plan §2 partition.
const RULE_ROSTER: &str = "m39: block roster does not match the plan §2 partition";
/// Reason: a window of the block never delivered an interaction.
const RULE_UNDELIVERED: &str = "m39: a window of the block never delivered";
/// Reason: a reported delta is not a route of the block.
const RULE_DELTA: &str = "m39: block delta is not a declared route or is zero";
/// Reason: the read-only windows do not match plan §3.4.
const RULE_READ_ONLY: &str = "m39: block read-only roster does not match plan §3.4";
/// Reason: no flow of the block moved a read model.
const RULE_FLOWED: &str = "m39: no flow of the block moved a read model";

#[cfg(test)]
mod m39_block_gh_tests {
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU32, Ordering};

    use daggerfall_parser_rs::texture::Palette;
    use daggerfall_skills::career::CFG_BYTES;

    use super::*;
    use crate::m18_demo::M20_CLASS_FILE;

    /// Scratch `ARENA2` directory carrying one synthetic `CLASS01.CFG`,
    /// removed on drop.
    struct Scratch(PathBuf);

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    static NEXT_SCRATCH: AtomicU32 = AtomicU32::new(0);

    /// 74-byte synthetic career (`CLASS01.CFG` shape) — the same vector the
    /// state and the sibling blocks build their scratch bank from.
    fn synthetic_career_bytes() -> Vec<u8> {
        let mut bytes = vec![0u8; CFG_BYTES];
        bytes[4..6].copy_from_slice(&0x0C00u16.to_le_bytes()); // SP× 1.5
        bytes[16..19].copy_from_slice(&[31, 29, 32]);
        bytes[19..22].copy_from_slice(&[22, 24, 25]);
        bytes[22..28].copy_from_slice(&[23, 26, 27, 28, 30, 33]);
        bytes[28..38].copy_from_slice(b"Spellsword");
        bytes[52..54].copy_from_slice(&12u16.to_le_bytes());
        bytes[54..58].copy_from_slice(&0x0001_0000u32.to_le_bytes());
        let attributes: [u16; 8] = [60, 50, 55, 50, 50, 40, 45, 50];
        for (index, value) in attributes.iter().enumerate() {
            bytes[58 + index * 2..60 + index * 2].copy_from_slice(&value.to_le_bytes());
        }
        bytes
    }

    fn scratch_dir() -> Scratch {
        let serial = NEXT_SCRATCH.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "daggerfall-m39-block-gh-{}-{serial}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).expect("scratch arena2");
        std::fs::write(dir.join(M20_CLASS_FILE), synthetic_career_bytes()).expect("career write");
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

    fn scratch_state() -> (Scratch, M39State) {
        let dir = scratch_dir();
        let state = M39State::new(&empty_assets(), &dir.0).expect("state construction");
        (dir, state)
    }

    /// The retail bank of this workstation, when it is installed.
    fn retail_arena2() -> Option<PathBuf> {
        if let Some(path) = std::env::var_os("DF_ARENA2") {
            let path = PathBuf::from(path);
            if path.is_dir() {
                return Some(path);
            }
        }
        let home = std::env::var_os("HOME")?;
        let path = PathBuf::from(home)
            .join("Games")
            .join("Daggerfall")
            .join("app")
            .join("ARENA2");
        path.is_dir().then_some(path)
    }

    fn retail_state(arena2: &Path) -> M39State {
        M39State::new(&empty_assets(), arena2).expect("state over real retail")
    }

    fn all_windows() -> Vec<&'static str> {
        G_WINDOWS.iter().chain(H_WINDOWS.iter()).copied().collect()
    }

    fn block_of(name: &str) -> char {
        if G_WINDOWS.contains(&name) {
            'G'
        } else {
            'H'
        }
    }

    fn declared_paths(name: &str) -> &'static [&'static str] {
        if block_of(name) == 'G' {
            M39TravelModels::PATHS
        } else {
            M39SocialModels::PATHS
        }
    }

    fn one(state: &mut M39State, name: &'static str) -> Interaction {
        let mut delivered =
            interact(state, name).unwrap_or_else(|error| panic!("{name}: {error:?}"));
        assert_eq!(delivered.len(), 1, "{name} delivers exactly one interaction");
        delivered.remove(0)
    }

    fn digests(arena2: &Path) -> Vec<(u16, String)> {
        let mut state = retail_state(arena2);
        all_windows()
            .into_iter()
            .map(|name| {
                let row = one(&mut state, name);
                (row.id, row.digest)
            })
            .collect()
    }

    #[test]
    fn roster_follows_the_census_and_the_plan_partition() {
        let state = GhState::from_retail(&empty_assets(), Path::new("/nonexistent/arena2"))
            .expect("retail is optional for construction");

        assert_eq!(state.g.len(), BLOCK_G_WINDOWS);
        assert_eq!(state.h.len(), BLOCK_H_WINDOWS);
        assert!(state.retail.regions.is_empty());
        assert!(state.retail.factions.is_empty());
        assert!(state.terrain.is_none());
        for slot in state.g.iter().chain(state.h.iter()) {
            assert_eq!(
                slot.window,
                canonical_id_for(slot.name).expect("census name"),
                "{} must resolve through the census",
                slot.name
            );
            assert_eq!(slot.intents, 0);
            assert_eq!(slot.delivered, 0);
            assert!(slot.last_delta.is_empty());
            assert_eq!(slot.read_only, None);
        }
        // Both rosters are in plan §1 ordinal order (each block separately).
        for roster in [&state.g[..], &state.h[..]] {
            let ordinals: Vec<u16> = roster.iter().map(|slot| slot.ordinal).collect();
            let mut sorted = ordinals.clone();
            sorted.sort_unstable();
            assert_eq!(ordinals, sorted, "plan §1 ordinal order");
            let unique: std::collections::BTreeSet<u16> = sorted.into_iter().collect();
            assert_eq!(unique.len(), roster.len(), "one ordinal per window");
        }
    }

    #[test]
    fn foreign_windows_are_a_navigation_error() {
        let (_dir, mut state) = scratch_state();
        for name in [
            "DaggerfallInventoryWindow",
            "DaggerfallMainMenuWindow",
            "DaggerfallNopeWindow",
        ] {
            assert!(
                matches!(interact(&mut state, name), Err(UiCompatError::Navigation(rule)) if rule == RULE_WINDOW),
                "{name} is not this module's"
            );
        }
    }

    #[test]
    fn every_window_delivers_one_sealed_interaction() {
        let (_dir, mut state) = scratch_state();
        for name in all_windows() {
            let row = one(&mut state, name);
            assert_eq!(row.name, name);
            assert_eq!(row.block, block_of(name), "{name}");
            assert_eq!(row.intents, 1);
            assert_eq!(
                row.delivered,
                usize::from(!row.outcomes.is_empty()),
                "{name}: one intent counts once, whatever the routes it moves"
            );
            assert!(row.delivered <= row.intents, "{name}");
            assert!(row.delivered >= 1, "{name} delivered nothing");
            assert_eq!(row.digest.len(), 16, "{name}");
            assert!(
                row.digest.chars().all(|digit| digit.is_ascii_hexdigit()),
                "{name}"
            );
            for outcome in &row.outcomes {
                M39State::validate(outcome).unwrap_or_else(|error| panic!("{name}: {error:?}"));
            }
            for (path, moved) in &row.delta {
                assert_ne!(*moved, 0, "{name}: zero delta on {path}");
                assert!(
                    declared_paths(name).contains(path),
                    "{name}: {path} is not a declared route of its block"
                );
            }
        }
    }

    #[test]
    fn daedra_summoned_is_the_read_only_surface() {
        let (_dir, mut state) = scratch_state();
        let row = one(&mut state, "DaggerfallDaedraSummonedWindow");

        assert_eq!(row.read_only, Some(RULE_READ_ONLY_031));
        assert!(row.delta.is_empty(), "read-only never reports a delta");
        assert_eq!(row.delivered, 1);
        assert!(matches!(
            row.outcomes.as_slice(),
            [ModalOutcome::ReadOnly { reason }] if *reason == RULE_READ_ONLY_031
        ));
    }

    #[test]
    fn missing_retail_rejects_with_its_rule() {
        let (_dir, mut state) = scratch_state();
        for (name, rule) in [
            ("DaggerfallAutomapWindow", RULE_NO_MAPS),
            ("DaggerfallExteriorAutomapWindow", RULE_NO_MAPS),
            ("DaggerfallTeleportPopUp", RULE_NO_MAPS),
            ("DaggerfallTravelMapWindow", RULE_NO_MAPS),
            ("DaggerfallTalkWindow", RULE_NO_FACTIONS),
            ("DaggerfallWitchesCovenPopupWindow", RULE_NO_FACTIONS),
            ("DaggerfallQuestOfferWindow", RULE_NO_OFFER),
        ] {
            let row = one(&mut state, name);
            assert!(row.delta.is_empty(), "{name}: a rejection moves nothing");
            assert!(
                matches!(
                    row.outcomes.as_slice(),
                    [ModalOutcome::Rejected { reason }] if *reason == rule
                ),
                "{name}: expected {rule}, got {:?}",
                row.outcomes
            );
        }
    }

    #[test]
    fn retail_run_delivers_citable_deltas_and_evidence() {
        let Some(arena2) = retail_arena2() else {
            eprintln!("skip: no ARENA2 (set DF_ARENA2 to run the retail flows)");
            return;
        };
        let has_factions = arena2.join(FACTION_FILENAME).is_file();
        let has_offers = {
            let mut found = false;
            if let Ok(entries) = std::fs::read_dir(&arena2) {
                found = entries.filter_map(|entry| entry.ok()).any(|entry| {
                    entry
                        .path()
                        .extension()
                        .is_some_and(|extension| extension.eq_ignore_ascii_case("QBN"))
                });
            }
            found
        };
        let mut state = retail_state(&arena2);

        for name in G_WINDOWS {
            let row = one(&mut state, name);
            assert!(!row.delta.is_empty(), "{name} moved no read model");
        }
        for name in H_WINDOWS {
            let row = one(&mut state, name);
            if name == "DaggerfallDaedraSummonedWindow" {
                assert!(row.delta.is_empty());
                continue;
            }
            let sourced = match name {
                "DaggerfallGuildServiceDonation"
                | "DaggerfallTalkWindow"
                | "DaggerfallWitchesCovenPopupWindow" => has_factions,
                "DaggerfallQuestOfferWindow" => has_offers,
                _ => true,
            };
            if sourced {
                assert!(!row.delta.is_empty(), "{name} moved no read model");
            }
        }

        let g = evidence(&state);
        assert_eq!(g.block, 'G');
        assert_eq!(g.windows, BLOCK_G_WINDOWS);
        assert_eq!(g.flows, BLOCK_G_WINDOWS);
        assert_eq!(g.intents, BLOCK_G_WINDOWS);
        assert_eq!(g.read_only, 0);
        assert!(g.ok, "block G FAIL reason={:?}", g.reason);
        assert!(g.state_delta > 0);

        let h = evidence_g(&state);
        assert_eq!(h.block, 'H');
        assert_eq!(h.windows, BLOCK_H_WINDOWS);
        assert_eq!(h.flows, BLOCK_H_WINDOWS);
        assert_eq!(h.intents, BLOCK_H_WINDOWS);
        assert_eq!(h.read_only, 1, "plan §3.4: only 031 is read-only");
        assert_eq!(h.ok, h.reason.is_none());
        if has_factions && has_offers {
            assert!(h.ok, "block H FAIL reason={:?}", h.reason);
        }
    }

    #[test]
    fn evidence_is_pending_before_the_flows_run() {
        let (_dir, state) = scratch_state();
        let g = evidence(&state);
        let h = evidence_g(&state);

        assert_eq!((g.block, g.windows), ('G', BLOCK_G_WINDOWS));
        assert_eq!((h.block, h.windows), ('H', BLOCK_H_WINDOWS));
        assert!(!g.ok);
        assert!(!h.ok);
        assert_eq!(g.reason, Some(RULE_UNDELIVERED));
        assert_eq!(h.reason, Some(RULE_UNDELIVERED));
    }

    #[test]
    fn two_runs_agree_on_every_digest() {
        let scratch = scratch_dir();
        assert_eq!(digests(&scratch.0), digests(&scratch.0));
        if let Some(arena2) = retail_arena2() {
            assert_eq!(digests(&arena2), digests(&arena2));
        }
    }

    #[test]
    fn travel_map_cycles_the_retail_regions() {
        let Some(arena2) = retail_arena2() else {
            eprintln!("skip: no ARENA2");
            return;
        };
        let mut state = retail_state(&arena2);
        let regions = state.gh.retail.regions.len();
        assert!(regions >= 3, "retail MAPS.BSA carries {regions} regions");

        let first = one(&mut state, "DaggerfallTravelMapWindow");
        assert!(first
            .delta
            .iter()
            .any(|(path, _)| *path == "m39.travel.selected_region"));
        assert_eq!(
            state.value("m39.travel.selected_region").expect("route"),
            state.gh.retail.regions[1].suffix
        );
        let second = one(&mut state, "DaggerfallTravelMapWindow");
        assert!(second
            .delta
            .iter()
            .any(|(path, _)| *path == "m39.travel.selected_region"));
        assert_eq!(
            state.value("m39.travel.selected_region").expect("route"),
            state.gh.retail.regions[2].suffix
        );
        assert_eq!(
            state.value("m39.travel.regions").expect("route"),
            regions.to_string()
        );
    }

    #[test]
    fn automap_reveals_until_its_layout_is_exhausted() {
        let Some(arena2) = retail_arena2() else {
            eprintln!("skip: no ARENA2");
            return;
        };
        let mut state = retail_state(&arena2);
        let total = state.gh.retail.dungeon_blocks;
        assert!(total > 0, "retail MAPDITEM carries a layout");

        // Walking the whole layout would be thousands of intents: the guard is
        // exercised at the edge the flow itself computes.
        for _ in 0..2 {
            let row = one(&mut state, "DaggerfallAutomapWindow");
            assert!(row
                .delta
                .iter()
                .any(|(path, _)| *path == "m39.automap.dungeon_blocks"));
        }
        state.gh.g[0].progress = usize::from(total);
        let exhausted = one(&mut state, "DaggerfallAutomapWindow");
        assert!(matches!(
            exhausted.outcomes.as_slice(),
            [ModalOutcome::Rejected { reason }] if *reason == RULE_EXHAUSTED
        ));
        assert!(exhausted.delta.is_empty());
    }

    #[test]
    fn a_repeated_intent_moves_a_route_or_rejects() {
        let Some(arena2) = retail_arena2() else {
            eprintln!("skip: no ARENA2");
            return;
        };
        let mut state = retail_state(&arena2);
        for name in all_windows() {
            let _ = one(&mut state, name);
            let second = one(&mut state, name);
            if second.read_only.is_some() {
                assert!(second.delta.is_empty(), "{name}");
                continue;
            }
            for outcome in &second.outcomes {
                M39State::validate(outcome).unwrap_or_else(|error| panic!("{name}: {error:?}"));
            }
            let rejected = second
                .outcomes
                .iter()
                .any(|outcome| matches!(outcome, ModalOutcome::Rejected { .. }));
            if rejected {
                assert!(
                    second.outcomes.len() == 1,
                    "{name}: a rejection is the whole interaction"
                );
            } else {
                assert!(
                    !second.delta.is_empty(),
                    "{name}: the second intent neither moved a route nor rejected"
                );
            }
        }
    }
}
