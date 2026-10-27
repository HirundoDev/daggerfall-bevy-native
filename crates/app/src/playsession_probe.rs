//! `PLAYSESSION-001` — the twelve-scenario deterministic playthrough (W34-7).
//!
//! ## What is measured
//!
//! **216 000 frames** = 12 scenarios × 18 000 frames at a fixed `1/60 s`
//! (3 600 s of simulated time), each frame folding the **live engine state**
//! into an FNV-1a 64 digest. No wall clock is read, no unseeded RNG is used and
//! no `HashMap` is iterated: the same inputs produce the same digest byte for
//! byte, in this process and in a second one.
//!
//! Every scenario asserts **fields of real engine state**, never a call
//! counter:
//!
//! | Probe | Asserted |
//! |---|---|
//! | S01 título | `compose` of the retail main-menu panel measures a real `320×200` framebuffer (opaque pixels > 0, palettes > 0), byte-identical across two compositions; the 18 000 title frames advance the clock **0** minutes (`SessionMode::Menu`, §2 iv) |
//! | S02 partida nueva | `GameSession::start` → `mode=Playing`, `scene=Exterior`, `snapshot()` carries heart's section (`Some`, `SESSION_BYTES`) and `restore` round-trips it; the 18 000 exterior frames advance **exactly 300** game minutes |
//! | S03 movimiento | `step_pose` over a `CollisionGrid::from_mesh_spec` room: the position changes, the travel is coherent with the **moved** frames (`WALK_SPEED_MPS/60`) within one step, the wall blocks and the position is stable afterwards |
//! | S04 entrar a pueblo | the location markup (session section bytes + the retail name of the destination) **changes** and the decoded envelope matches the real retail destination (region/location/block and its `MAPNAMES` name) |
//! | S05 compra | `TradeSession::buy` debits **exactly** the `calculate_cost` price per purchase, the good appears in the inventory (id + quantity) and the merchant stock decrements |
//! | S06 mazmorra | the first retail dungeon's grid is populated (`sides > 0`), one `.RDB` cell measures **51.2 m**, the `Transit` enters at the `Start` marker and exits **at the door position** (loads 1→2) |
//! | S07 hechizo | the spell points decrement **exactly** `SpellCost::spell_points` and `Levitate` is active with `rounds_left > 0` after a real `cast_effects` |
//! | S08 combate | 300 real `resolve_attack` resolutions: every hit reduces `hp` by exactly `min(damage, remaining)` via `set_health`, and the total reduction accounts for the final health |
//! | S09 guardar | `SaveGameV2` validates, `to_state` yields a **v2** envelope with the 12 canonical sections in `M33_SECTIONS` order, non-empty, and both `from_state` and `from_text` round-trip the document |
//! | S10 salir y recargar | the document written to disk and re-read has the **same digest** |
//! | S11 cargar restaura | position in millimeters and the inventory sets (goods + items) are restored: **0 items lost** |
//! | S12 segundo guardar/cargar | the second write/reload cycle keeps the S09 digest and the file bytes |
//!
//! ## Exit semantics
//!
//! - `EXIT=0` ⟺ **12/12** and 216 000 frames digested. Two separate processes
//!   must print the same `digest`.
//! - `EXIT=2` when a **prerequisite is absent**, detected before any row is
//!   emitted (a missing prerequisite never becomes a synthetic PASS):
//!   - **M33 state**: `attach_m33_codecs` + `registry.coverage(&M33_SECTIONS)`
//!     must reach `12/12`, the envelope version must be `SAVE_VERSION = 2` and
//!     a `sample → to_state → encode → decode → from_state` cycle must return
//!     the same canonical text. `EXIT=2` is printed with the literal reason.
//!   - **Retail custody (Mode A)**: `ARENA2` (override `DF_APP_ROOT`) must
//!     provide the UI assets of S01 (`PAL.PAL`/`PANEL`/`FONT`/`TITL00I0.IMG`)
//!     and `MAPS.BSA` + `BLOCKS.BSA` for S04/S06. Pointing `DF_APP_ROOT` at a
//!     root without them is the documented way to observe `EXIT=2`.
//! - `EXIT=1` for any other discrepancy (a FAIL row, a digest/scenario count
//!   mismatch).
//!
//! Retail bytes are read at run time from the user's installation and
//! **never** packaged (INV-L99-6); the scenario assertions are never adapted
//! to a measured value — a real engine bug is reported, not patched here.

use std::path::{Path, PathBuf};

use daggerfall_behavior::combat::set_health;
use daggerfall_behavior::economy::calculate_cost;
use daggerfall_behavior::magic::{
    calculate_total_effect_costs, effect_by_key, EffectEntry, EffectSettings, EffectTarget,
    MagicDice, MagicState, TargetType,
};
use daggerfall_combat::actor::{Combatant, EntityKind};
use daggerfall_combat::engine::{resolve_attack, AttackInput};
use daggerfall_combat::rng::Dice100;
use daggerfall_heart::clock::SECONDS_PER_MINUTE;
use daggerfall_heart::session::{
    CareerId, GameSession, SessionMode, SECTION_SESSION, SESSION_BYTES,
};
use daggerfall_inventory::commerce::{InventoryLite, MerchantStock, StockItem, TradeSession};
use daggerfall_memory::save_v2::{SaveStateV2, SectionRegistry, M33_SECTIONS, SAVE_VERSION};
use daggerfall_memory::savegame_v2::{attach_m33_codecs, GoodV2, SaveGameV2};
use daggerfall_nervous::events::{LocationRef, SceneId, SceneTarget};
use daggerfall_parser_bevy_bridge::ui_spec::{
    build_main_menu_panel, compose, Framebuffer, UiAssets,
};
use daggerfall_parser_bevy_bridge::{
    CollisionGrid, MeshSpec, DEFAULT_CELL_M, EYE_HEIGHT_M, PLAYER_RADIUS_M,
};
use daggerfall_parser_rs::{bsa, maps};

use crate::interior_retail::RetailSource;
use daggerfall_app::interior_transit::{
    DoorAction, DoorRegistry, DoorSite, SceneError, SceneHandle, SceneLoader, Transit,
};
use crate::m10_probe::named_entry;
use crate::m18_probe::region_suffixes;
use daggerfall_app::m2_kinematics::{step_pose, WALK_SPEED_MPS};
use daggerfall_app::mode_a;
use crate::ui_assets::load_ui_assets;

// ── fixed inputs of the playthrough (one seed, one career: reproducible) ────

/// World seed of both processes.
const SEED: u64 = 0x5EED_0D26;
/// Career id of the session (`CareerId`, M20 index).
const CAREER: u32 = 7;
/// Scenarios of the playthrough.
const SCENARIOS: usize = 12;
/// Frames per scenario (300 s at 1/60 s).
const STEPS_PER_SCENARIO: usize = 18_000;
/// Frames per simulated second (fixed: no wall clock anywhere).
const FPS: u32 = 60;
/// Frame period in seconds (`1/60 s`).
const FRAME_SECONDS: f32 = 1.0 / FPS as f32;

/// FNV-1a 64 offset basis.
const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
/// FNV-1a 64 prime.
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

/// FNV-1a 64 continued from `state`.
fn fnv1a64(bytes: &[u8], state: u64) -> u64 {
    let mut hash = state;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    hash
}

// ── pure arithmetic of the playthrough (unit-tested) ───────────────────────

/// Frames of the whole playthrough.
fn total_steps() -> usize {
    SCENARIOS * STEPS_PER_SCENARIO
}

/// Simulated seconds of the whole playthrough (`216 000 / 60 = 3 600 s`).
fn simulation_seconds() -> f64 {
    total_steps() as f64 / f64::from(FPS)
}

/// Game seconds one frame advances (`1/60 s · 60 game s/s = 1 s`).
const fn game_seconds_per_frame() -> u64 {
    SECONDS_PER_MINUTE / FPS as u64
}

/// Game seconds the whole playthrough advances.
fn game_seconds_total() -> u64 {
    total_steps() as u64 * game_seconds_per_frame()
}

/// Walk distance one frame commands (`WALK_SPEED_MPS / 60`).
const fn step_metres() -> f32 {
    WALK_SPEED_MPS / FPS as f32
}

/// Canonical per-frame state the digest folds (0 text, little endian).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FrameSignature {
    /// Absolute frame index (1-based).
    frame: u64,
    /// Session clock in game minutes.
    game_minutes: u64,
    /// Scene on screen (`SceneId` as its session code).
    scene: u8,
    /// Player position in integer millimeters.
    position_mm: [i32; 3],
    /// Current health (combat).
    health: i32,
    /// Current spell points (magic).
    spell_points: i32,
    /// Gold held (trade).
    gold: i32,
    /// Quantity of the bought good.
    goods: i32,
    /// Active spell effects.
    active_effects: i32,
}

impl FrameSignature {
    /// Fixed-width encoding: 8 + 8 + 1 + 12 + 5·4 = 49 bytes.
    fn to_bytes(self) -> Vec<u8> {
        let mut out = Vec::with_capacity(49);
        out.extend_from_slice(&self.frame.to_le_bytes());
        out.extend_from_slice(&self.game_minutes.to_le_bytes());
        out.push(self.scene);
        for part in self.position_mm {
            out.extend_from_slice(&part.to_le_bytes());
        }
        for value in [
            self.health,
            self.spell_points,
            self.gold,
            self.goods,
            self.active_effects,
        ] {
            out.extend_from_slice(&value.to_le_bytes());
        }
        out
    }
}

/// Folds one frame into the playthrough digest.
fn fold_frame(digest: u64, signature: &FrameSignature) -> u64 {
    fnv1a64(&signature.to_bytes(), digest)
}

/// `SceneId` as the session's own code (1 menu, 2 exterior, 3 interior).
const fn scene_code(scene: SceneId) -> u8 {
    match scene {
        SceneId::Menu => 1,
        SceneId::Exterior => 2,
        SceneId::Interior => 3,
    }
}

/// Meters → integer millimeters (the save's unit).
fn to_mm(metres: f32) -> i32 {
    (metres * 1000.0).round() as i32
}

/// Location markup: the session section's fixed-width digest plus the real
/// retail name of the location — what the probe compares across S02/S04.
fn location_markup(session: &GameSession, name: &str) -> String {
    format!(
        "{}.{}.{}:{name}#{:016x}",
        session.location.region,
        session.location.location,
        session.location.block,
        fnv1a64(&session.to_bytes(), FNV_OFFSET)
    )
}

/// FNV-1a 64 of a composed framebuffer (pixels then palette slots).
fn framebuffer_digest(framebuffer: &Framebuffer) -> u64 {
    let mut hash = fnv1a64(&framebuffer.pixels, FNV_OFFSET);
    hash = fnv1a64(&framebuffer.slots, hash);
    for palette in &framebuffer.palettes {
        hash = fnv1a64(palette.asset.as_bytes(), hash);
        hash = fnv1a64(&palette.hash.to_le_bytes(), hash);
    }
    hash
}

// ── playthrough state ──────────────────────────────────────────────────────

/// Live state the twelve scenarios mutate and every frame folds.
struct World {
    session: GameSession,
    position: [f32; 3],
    health: i32,
    max_health: i32,
    spell_points: i32,
    gold: i32,
    goods: i32,
    active_effects: i32,
    frames: u64,
    digest: u64,
}

impl World {
    /// Fresh playthrough: the session is in `Menu` (title), no frame folded.
    fn new() -> Self {
        Self {
            session: GameSession::new(SEED, CareerId(CAREER)),
            position: [0.0; 3],
            health: 100,
            max_health: 100,
            spell_points: 100,
            gold: 0,
            goods: 0,
            active_effects: 0,
            frames: 0,
            digest: FNV_OFFSET,
        }
    }

    /// One frame: the clock advances only while `Playing` (§2 iv).
    fn advance(&mut self) {
        if self.session.mode == SessionMode::Playing {
            self.session
                .clock
                .raise_seconds(game_seconds_per_frame());
        }
        self.frames += 1;
    }

    /// Canonical state of the current frame.
    fn signature(&self) -> FrameSignature {
        FrameSignature {
            frame: self.frames,
            game_minutes: self.session.clock.total_minutes(),
            scene: scene_code(self.session.scene),
            position_mm: [
                to_mm(self.position[0]),
                to_mm(self.position[1]),
                to_mm(self.position[2]),
            ],
            health: self.health,
            spell_points: self.spell_points,
            gold: self.gold,
            goods: self.goods,
            active_effects: self.active_effects,
        }
    }

    /// Folds the current frame into the digest.
    fn fold(&mut self) {
        self.digest = fold_frame(self.digest, &self.signature());
    }
}

/// Runs `frames` frames: advance → scenario body → fold.
fn run_frames<F>(world: &mut World, frames: usize, mut body: F)
where
    F: FnMut(&mut World, usize),
{
    for index in 0..frames {
        world.advance();
        body(world, index);
        world.fold();
    }
}

/// One scenario row.
type Row = (&'static str, bool, String);

/// Compact `[PASS]`/`[FAIL]` detail helper.
fn verdict(conditions: &[(bool, &str)]) -> (bool, String) {
    let mut failed: Vec<&str> = Vec::new();
    for (ok, label) in conditions {
        if !ok {
            failed.push(label);
        }
    }
    if failed.is_empty() {
        (true, String::new())
    } else {
        (false, format!("failed: {}", failed.join(", ")))
    }
}

// ── retail custody (Mode A) ────────────────────────────────────────────────

/// One location without a `MAPDITEM` entry (a town, M18 index order).
struct Town {
    name: String,
    region: u16,
    location: usize,
}

impl Town {
    fn location_ref(&self) -> LocationRef {
        LocationRef::new(u32::from(self.region), self.location as u32, 0)
    }
}

/// Retail inputs of the playthrough.
struct Retail {
    assets: UiAssets,
    maps_bytes: usize,
    blocks_bytes: usize,
    towns: Vec<Town>,
    source: RetailSource,
    dungeons: Vec<crate::interior_retail::RetailDungeon>,
}

/// Reads one ARENA2 file (case-insensitive resolution, Mode A).
fn read_arena2(name: &str) -> Result<Vec<u8>, String> {
    let path: PathBuf = mode_a::arena2_path(name)?;
    std::fs::read(&path).map_err(|error| format!("{} ({error})", path.display()))
}

/// Enumerates the locations **without** `MAPDITEM` (towns) in index order.
fn towns(archive: &bsa::BsaArchive, data: &[u8]) -> Result<Vec<Town>, String> {
    let mut out: Vec<Town> = Vec::new();
    for (region_index, suffix) in region_suffixes(archive) {
        let names_record = named_entry(archive, data, &format!("MAPNAMES.{suffix}"))?;
        let pitem_record = named_entry(archive, data, &format!("MAPPITEM.{suffix}"))?;
        let ditem_record =
            named_entry(archive, data, &format!("MAPDITEM.{suffix}")).unwrap_or(&[]);
        if names_record.is_empty() || pitem_record.is_empty() {
            continue;
        }
        let names =
            maps::parse_map_names(names_record).map_err(|e| format!("MAPNAMES.{suffix}: {e}"))?;
        if names.is_empty() {
            continue;
        }
        let locations = maps::parse_pitem(pitem_record, names.len())
            .map_err(|e| format!("MAPPITEM.{suffix}: {e}"))?;
        if locations.len() != names.len() {
            return Err(format!(
                "region {region_index}: names={} pitem={}",
                names.len(),
                locations.len()
            ));
        }
        for (location_index, location) in locations.iter().enumerate() {
            let layout = maps::parse_ditem(ditem_record, location.location_id)
                .map_err(|e| format!("MAPDITEM.{suffix}[{location_index}]: {e}"))?;
            if layout.is_none() && !location.name.trim().is_empty() {
                out.push(Town {
                    name: location.name.clone(),
                    region: region_index,
                    location: location_index,
                });
            }
        }
    }
    Ok(out)
}

/// Loads every retail input; any absence is a prerequisite (`EXIT=2`).
fn load_retail() -> Result<Retail, String> {
    let arena2: PathBuf = mode_a::arena2_dir()?;
    let assets = load_ui_assets(&arena2)?;
    let maps_data = read_arena2("MAPS.BSA")?;
    let blocks_data = read_arena2("BLOCKS.BSA")?;
    let archive = bsa::parse(&maps_data).map_err(|e| format!("MAPS.BSA: {e}"))?;
    let towns = towns(&archive, &maps_data)?;
    if towns.len() < 2 {
        return Err(format!(
            "ARENA2 at {} exposed {} town locations, 2 required (S02 start + S04 destination)",
            arena2.display(),
            towns.len()
        ));
    }
    let source = RetailSource::load()?;
    let (_census, dungeons) = source.dungeons()?;
    if dungeons.is_empty() {
        return Err(format!(
            "MAPS.BSA at {} exposed 0 dungeon locations (S06)",
            arena2.display()
        ));
    }
    Ok(Retail {
        assets,
        maps_bytes: maps_data.len(),
        blocks_bytes: blocks_data.len(),
        towns,
        source,
        dungeons,
    })
}

/// M33 prerequisite: the 12 canonical v2 codecs plus a real envelope cycle.
fn m33_gate() -> Result<String, String> {
    let mut registry = SectionRegistry::new();
    registry
        .declare_all(&M33_SECTIONS)
        .map_err(|e| format!("M33 declaration table: {e}"))?;
    attach_m33_codecs(&mut registry).map_err(|e| format!("attach_m33_codecs: {e:?}"))?;
    let coverage = registry.coverage(&M33_SECTIONS);
    if !coverage.complete() {
        return Err(format!(
            "M33 coverage {}/{} missing: {}",
            coverage.count(),
            coverage.total(),
            coverage.missing_list()
        ));
    }
    let sample = SaveGameV2::sample();
    sample
        .validate()
        .map_err(|e| format!("sample.validate: {e:?}"))?;
    let state = sample
        .to_state()
        .map_err(|e| format!("sample.to_state: {e:?}"))?;
    if state.version != SAVE_VERSION {
        return Err(format!(
            "envelope version {} != SAVE_VERSION {SAVE_VERSION}",
            state.version
        ));
    }
    let bytes = state.encode();
    let decoded =
        SaveStateV2::decode(&bytes, &registry).map_err(|e| format!("envelope decode: {e:?}"))?;
    let document = SaveGameV2::from_state(&decoded)
        .map_err(|e| format!("envelope from_state: {e:?}"))?;
    if document.to_text() != sample.to_text() {
        return Err("v2 envelope roundtrip changed the document".to_string());
    }
    Ok(format!(
        "coverage={}/{} version={} roundtrip=ok",
        coverage.count(),
        coverage.total(),
        state.version
    ))
}

// ── S01 title ──────────────────────────────────────────────────────────────

fn s01_title(assets: &UiAssets, world: &mut World) -> Row {
    let first = build_main_menu_panel(assets).and_then(|panel| compose(&[panel], assets));
    let second = build_main_menu_panel(assets).and_then(|panel| compose(&[panel], assets));
    let (Ok(first), Ok(second)) = (first, second) else {
        return ("S01", false, "main-menu composition failed".to_string());
    };
    run_frames(world, STEPS_PER_SCENARIO, |_, _| {});
    let menu_minutes = world.session.elapsed_minutes();
    let (passed, failures) = verdict(&[
        (first.width == 320, "width"),
        (first.height == 200, "height"),
        (first.opaque_pixels() > 0, "opaque"),
        (!first.palettes.is_empty(), "palettes"),
        (first.pixels == second.pixels, "determinism"),
        (first.slots == second.slots, "slots"),
        (menu_minutes == 0, "menu clock"),
    ]);
    let detail = format!(
        "fb={}x{} opaque={} palettes={} digest={:016x} menu_minutes={menu_minutes}{}",
        first.width,
        first.height,
        first.opaque_pixels(),
        first.palettes.len(),
        framebuffer_digest(&first),
        if failures.is_empty() {
            String::new()
        } else {
            format!(" {failures}")
        }
    );
    ("S01", passed, detail)
}

// ── S02 new game ───────────────────────────────────────────────────────────

fn s02_new_game(start: &Town, world: &mut World) -> Row {
    let menu_before = world.session.mode == SessionMode::Menu
        && world.session.scene == SceneId::Menu
        && world.session.elapsed_minutes() == 0;
    if let Err(error) = world.session.start(start.location_ref()) {
        return ("S02", false, format!("start: {error:?}"));
    }
    let snapshot = world.session.snapshot();
    let section = snapshot.section(SECTION_SESSION);
    let section_ok = section.is_some_and(|s| s.bytes.len() == SESSION_BYTES);
    let restore_ok = GameSession::restore(&snapshot)
        .map(|session| session.to_bytes() == world.session.to_bytes())
        .unwrap_or(false);
    run_frames(world, STEPS_PER_SCENARIO, |_, _| {});
    let minutes = world.session.elapsed_minutes();
    let (passed, failures) = verdict(&[
        (menu_before, "menu start"),
        (world.session.mode == SessionMode::Playing, "mode"),
        (world.session.scene == SceneId::Exterior, "scene"),
        (world.session.location == start.location_ref(), "location"),
        (section_ok, "snapshot section"),
        (restore_ok, "restore"),
        (minutes == 300, "clock"),
    ]);
    let detail = format!(
        "scene=Exterior loc=({},{},0) snapshot={}B minutes={minutes}{}",
        start.region,
        start.location,
        if section_ok { SESSION_BYTES } else { 0 },
        if failures.is_empty() {
            String::new()
        } else {
            format!(" {failures}")
        }
    );
    ("S02", passed, detail)
}

// ── S03 movement ───────────────────────────────────────────────────────────

/// 8×8×3 m closed room (floor, ceiling, 4 walls) — the reference room of the
/// kinematics tests, so the grid measures the same geometry.
fn room_spec() -> MeshSpec {
    fn push_quad(spec: &mut MeshSpec, a: [f32; 3], b: [f32; 3], c: [f32; 3], d: [f32; 3]) {
        let base = spec.positions.len() as u32;
        spec.positions.extend([a, b, c, d]);
        spec.normals.extend([[0.0, 1.0, 0.0]; 4]);
        spec.indices
            .extend([base, base + 1, base + 2, base, base + 2, base + 3]);
    }
    let mut spec = MeshSpec::new(Vec::new(), Vec::new(), Vec::new());
    push_quad(&mut spec, [0.0, 0.0, 0.0], [0.0, 0.0, 8.0], [8.0, 0.0, 8.0], [8.0, 0.0, 0.0]);
    push_quad(&mut spec, [0.0, 3.0, 0.0], [8.0, 3.0, 0.0], [8.0, 3.0, 8.0], [0.0, 3.0, 8.0]);
    push_quad(&mut spec, [0.0, 0.0, 0.0], [0.0, 3.0, 0.0], [0.0, 3.0, 8.0], [0.0, 0.0, 8.0]);
    push_quad(&mut spec, [8.0, 0.0, 0.0], [8.0, 0.0, 8.0], [8.0, 3.0, 8.0], [8.0, 3.0, 0.0]);
    push_quad(&mut spec, [0.0, 0.0, 0.0], [8.0, 0.0, 0.0], [8.0, 3.0, 0.0], [0.0, 3.0, 0.0]);
    push_quad(&mut spec, [0.0, 0.0, 8.0], [0.0, 3.0, 8.0], [8.0, 3.0, 8.0], [8.0, 0.0, 8.0]);
    spec
}

fn s03_movement(world: &mut World) -> Row {
    let grid = CollisionGrid::from_mesh_spec(&room_spec(), DEFAULT_CELL_M);
    let spawn = [4.0, EYE_HEIGHT_M, 6.0];
    let mut position = spawn;
    let mut moved = 0usize;
    let mut blocked = 0usize;
    let mut last_moved = 0usize;
    world.position = position;
    run_frames(world, STEPS_PER_SCENARIO, |world, index| {
        let outcome = step_pose(
            position,
            0.0,
            [0.0, 1.0],
            &grid,
            PLAYER_RADIUS_M,
            FRAME_SECONDS,
        );
        if outcome.moved {
            moved += 1;
            last_moved = index;
        } else {
            blocked += 1;
        }
        position = outcome.pos;
        world.position = position;
    });
    world.position = position;
    let travelled = spawn[2] - position[2];
    let expected = moved as f32 * step_metres();
    let stable_tail = STEPS_PER_SCENARIO.saturating_sub(last_moved + 1);
    let (passed, failures) = verdict(&[
        (position != spawn, "position changed"),
        (travelled > 0.0, "distance"),
        ((travelled - expected).abs() <= step_metres() + 1e-3, "coherence"),
        (blocked > 0, "wall"),
        (stable_tail >= 1_000, "stability"),
        ((position[0] - spawn[0]).abs() < 1e-6, "straight line"),
        (position[2] > 0.0, "no traversal"),
    ]);
    let detail = format!(
        "z {:.6}->{:.6} travelled={travelled:.6} moved={moved} blocked={blocked} \
         step={:.6} stable_tail={stable_tail}{}",
        spawn[2],
        position[2],
        step_metres(),
        if failures.is_empty() {
            String::new()
        } else {
            format!(" {failures}")
        }
    );
    ("S03", passed, detail)
}

// ── S04 enter a town ───────────────────────────────────────────────────────

fn s04_enter_town(origin: &Town, destination: &Town, world: &mut World) -> Row {
    let before = location_markup(&world.session, &origin.name);
    let before_bytes = world.session.to_bytes();
    if let Err(error) = world.session.start(destination.location_ref()) {
        return ("S04", false, format!("start: {error:?}"));
    }
    let after = location_markup(&world.session, &destination.name);
    let after_bytes = world.session.to_bytes();
    let decoded = GameSession::from_bytes(&after_bytes);
    run_frames(world, STEPS_PER_SCENARIO, |_, _| {});
    let (passed, failures) = verdict(&[
        (before != after, "markup changed"),
        (before_bytes != after_bytes, "session section"),
        (
            decoded.map(|session| session.location) == Ok(destination.location_ref()),
            "destination",
        ),
        (world.session.scene == SceneId::Exterior, "scene"),
        (origin.name != destination.name, "distinct towns"),
    ]);
    let detail = format!(
        "{before} -> {after}{}",
        if failures.is_empty() {
            String::new()
        } else {
            format!(" {failures}")
        }
    );
    ("S04", passed, detail)
}

// ── S05 buy ────────────────────────────────────────────────────────────────

fn s05_buy(world: &mut World) -> Row {
    const ITEM_ID: &str = "iron_cuirass";
    let base_cost = 300;
    let shop_quality: u8 = 8;
    let price = calculate_cost(i64::from(base_cost), shop_quality, Some(1_000)) as i32;
    let merchant = MerchantStock {
        merchant_name: "M5 shopkeeper".to_string(),
        shop_quality: i32::from(shop_quality),
        items: vec![StockItem {
            id: ITEM_ID.to_string(),
            name: "Iron Cuirass".to_string(),
            base_cost,
            stock: 3,
        }],
    };
    let mut trade = TradeSession::new(merchant, InventoryLite::new(2_000));
    trade.open_dialogue();
    let gold_before = trade.inventory.gold;
    let mut charged: Vec<i32> = Vec::new();
    for _ in 0..2 {
        match trade.buy(ITEM_ID, price) {
            Ok(paid) => charged.push(paid),
            Err(error) => return ("S05", false, format!("buy: {error:?}")),
        }
    }
    world.gold = trade.inventory.gold;
    world.goods = trade.inventory.count(ITEM_ID);
    run_frames(world, STEPS_PER_SCENARIO, |_, _| {});
    let id_rows = trade
        .inventory
        .goods
        .iter()
        .filter(|(good_id, _)| good_id == ITEM_ID)
        .count();
    let stock = trade.merchant.item(ITEM_ID).map(|item| item.stock);
    let (passed, failures) = verdict(&[
        (price > 0, "price"),
        (charged == vec![price, price], "charged"),
        (gold_before - trade.inventory.gold == price * 2, "gold"),
        (trade.inventory.count(ITEM_ID) == 2, "quantity"),
        (id_rows == 1, "id rows"),
        (stock == Some(1), "stock"),
    ]);
    let detail = format!(
        "price={price} gold {gold_before}->{} count={} id_rows={id_rows} stock={:?}{}",
        trade.inventory.gold,
        trade.inventory.count(ITEM_ID),
        stock,
        if failures.is_empty() {
            String::new()
        } else {
            format!(" {failures}")
        }
    );
    ("S05", passed, detail)
}

// ── S06 dungeon ────────────────────────────────────────────────────────────

/// Loader over ONE assembled retail dungeon grid (geometry only).
struct DungeonLoader {
    start: Option<[f32; 3]>,
    grid_digest: u64,
    exterior_digest: u64,
}

impl SceneLoader for DungeonLoader {
    fn enter_interior(&mut self, _target: SceneTarget) -> Result<SceneHandle, SceneError> {
        let Some(position) = self.start else {
            return Err(SceneError::LoadRejected {
                block: 0,
                reason: "the dungeon grid carries no Start marker".to_string(),
            });
        };
        Ok(SceneHandle {
            scene: SceneId::Interior,
            position,
            digest: self.grid_digest,
        })
    }

    fn leave_interior(&mut self, _target: SceneTarget) -> Result<SceneHandle, SceneError> {
        Ok(SceneHandle {
            scene: SceneId::Exterior,
            position: [0.0, 0.0, 0.0],
            digest: self.exterior_digest,
        })
    }
}

fn s06_dungeon(retail: &Retail, world: &mut World) -> Row {
    let Some(dungeon) = retail.dungeons.first() else {
        return ("S06", false, "no dungeon location".to_string());
    };
    let grid = retail.source.assemble(&dungeon.layout);
    let sides = grid.blocks.len();
    let cells_x = f32::from(i16::from(grid.max_x) - i16::from(grid.min_x) + 1);
    let measured_side = if cells_x > 0.0 {
        grid.extent_m().0 / cells_x
    } else {
        0.0
    };
    let digest = grid.digest();
    let digest_stable = grid.digest() == digest;
    let Some(start) = grid.start_marker else {
        return ("S06", false, format!("sides={sides} grid without Start marker"));
    };
    let door_position = [start[0], 0.0, start[2]];
    let location = LocationRef::new(
        u32::from(dungeon.region_index),
        dungeon.location_index as u32,
        0,
    );
    let doors = DoorRegistry::new(vec![DoorSite {
        id: 1,
        building: 0,
        block: 0,
        location,
        world_position: door_position,
        reach_m: 2.5,
    }]);
    let mut loader = DungeonLoader {
        start: Some(start),
        grid_digest: digest,
        exterior_digest: fnv1a64(b"{dungeon}", FNV_OFFSET),
    };
    let exterior = SceneTarget::Exterior {
        map_pixel: (0, 0),
        location,
    };
    let mut transit = Transit::new(exterior, door_position);
    let entered = transit.activate_nearest(DoorAction::Interact, &doors, &mut loader);
    let enter_ok = entered.is_ok();
    world.session.enter_scene(transit.scene);
    world.position = transit.position;
    run_frames(world, STEPS_PER_SCENARIO / 2, |_, _| {});
    let interior_scene = world.session.scene;
    let exited = transit.exit(&mut loader);
    let exit_ok = exited.is_ok();
    world.session.enter_scene(transit.scene);
    world.position = transit.position;
    run_frames(world, STEPS_PER_SCENARIO - STEPS_PER_SCENARIO / 2, |_, _| {});
    let (passed, failures) = verdict(&[
        (sides > 0, "populated grid"),
        ((measured_side - 51.2).abs() <= 1e-3, "rdb side"),
        (digest_stable, "grid digest"),
        (enter_ok, "enter"),
        (transit.loads == 2, "loads"),
        (interior_scene == SceneId::Interior, "interior scene"),
        (exit_ok, "exit"),
        (transit.scene == SceneId::Exterior, "exterior scene"),
        (transit.position == door_position, "exit position"),
        (transit.steps.len() == 2, "steps"),
    ]);
    let detail = format!(
        "grid={} \"{}\" sides={sides} rdb_side={measured_side:.6}m digest={digest:016x} \
         start=({:.3},{:.3}) exit=({:.3},{:.3}) loads={}{}",
        location.region,
        dungeon.location_name,
        start[0],
        start[2],
        transit.position[0],
        transit.position[2],
        transit.loads,
        if failures.is_empty() {
            String::new()
        } else {
            format!(" {failures}")
        }
    );
    ("S06", passed, detail)
}

// ── S07 spell ──────────────────────────────────────────────────────────────

fn s07_spell(world: &mut World) -> Row {
    const KEY: &str = "Levitate";
    let Some(props) = effect_by_key(KEY) else {
        return ("S07", false, format!("template {KEY} not registered"));
    };
    let settings = EffectSettings {
        duration_base: 1,
        duration_plus: 1,
        duration_per_level: 1,
        ..EffectSettings::default()
    };
    let entries = [EffectEntry { props, settings }];
    let cost = calculate_total_effect_costs(&entries, TargetType::CasterOnly, false, &|_| 50);
    let mut target = EffectTarget::new(100, 100, [50; 8]);
    let spell_points_before = target.spell_points;
    let mut magic = MagicState::default();
    let mut dice = MagicDice::with_seed(SEED ^ 0x00D1_CE00);
    magic.cast_effects(&entries, 1, &mut target, &mut dice);
    target.spell_points -= cost.spell_points;
    let spell_points_after = target.spell_points;
    let rounds = magic.rounds_left(KEY);
    let active = magic.active().len();
    world.spell_points = spell_points_after;
    world.active_effects = active as i32;
    run_frames(world, STEPS_PER_SCENARIO, |_, _| {});
    let (passed, failures) = verdict(&[
        (cost.spell_points > 0, "cost"),
        (
            spell_points_before - spell_points_after == cost.spell_points,
            "spell points",
        ),
        (spell_points_after >= 0, "pool"),
        (rounds > 0, "rounds"),
        (active >= 1, "active"),
    ]);
    let detail = format!(
        "{KEY} cost={}sp/{}g points {spell_points_before}->{spell_points_after} \
         rounds={rounds} active={active}{}",
        cost.spell_points,
        cost.gold,
        if failures.is_empty() {
            String::new()
        } else {
            format!(" {failures}")
        }
    );
    ("S07", passed, detail)
}

// ── S08 combat ─────────────────────────────────────────────────────────────

fn s08_combat(world: &mut World) -> Row {
    let attacker = Combatant::new(1, EntityKind::Player);
    let target = Combatant::new(2, EntityKind::EnemyClass);
    let max_health = target.max_health;
    let mut dice = Dice100::with_seed(SEED ^ 0x00C0_BA71);
    let mut health = target.health;
    let mut attacks = 0usize;
    let mut hits = 0usize;
    let mut reductions = 0i32;
    let mut invariant = true;
    world.health = health;
    world.max_health = max_health;
    run_frames(world, STEPS_PER_SCENARIO, |world, index| {
        if index % FPS as usize == 0 && health > 0 {
            attacks += 1;
            let input = AttackInput {
                attacker: &attacker,
                target: &target,
                weapon: None,
                chance_mod: 0,
                damage_mods: 0,
                backstab_level: 0,
                enemy_bonus: 0,
            };
            let outcome = resolve_attack(&input, &mut dice);
            let expected = if outcome.hit {
                outcome.damage.min(health)
            } else {
                0
            };
            let (after, _dead) = set_health(max_health, health - outcome.damage, false);
            if health - after != expected {
                invariant = false;
            }
            if outcome.hit && outcome.damage > 0 {
                hits += 1;
            }
            reductions += (health - after).max(0);
            health = after;
            world.health = health;
        }
    });
    world.health = health;
    let expected_attacks = STEPS_PER_SCENARIO / FPS as usize;
    let (passed, failures) = verdict(&[
        (attacks == expected_attacks, "attack count"),
        (hits > 0, "hits"),
        (invariant, "damage applied"),
        (health < 100, "hp changed"),
        (100 - health == reductions, "hp accounting"),
        (health >= 0 && health <= max_health, "hp range"),
    ]);
    let detail = format!(
        "attacks={attacks} hits={hits} hp=100->{health} reductions={reductions} max={max_health}{}",
        if failures.is_empty() {
            String::new()
        } else {
            format!(" {failures}")
        }
    );
    ("S08", passed, detail)
}

// ── S09 save ───────────────────────────────────────────────────────────────

/// Builds the v2 document from the playthrough's measured state.
fn save_document(world: &World) -> SaveGameV2 {
    let mut document = SaveGameV2::sample();
    document.world.pos_x_mm = i64::from(to_mm(world.position[0]));
    document.world.pos_y_mm = i64::from(to_mm(world.position[1]));
    document.world.pos_z_mm = i64::from(to_mm(world.position[2]));
    document.world.region_index = i64::from(world.session.location.region);
    document.world.location_index = i64::from(world.session.location.location);
    document.world.environment = match world.session.scene {
        SceneId::Menu => 1,
        SceneId::Exterior => 1,
        SceneId::Interior => 2,
    };
    document.character.health = i64::from(world.health);
    document.character.spell_points = i64::from(world.spell_points);
    document.inventory.gold = i64::from(world.gold);
    document.inventory.goods = vec![GoodV2 {
        id: "iron_cuirass".to_string(),
        quantity: i64::from(world.goods),
    }];
    document
}

fn s09_save(world: &mut World, path: &Path) -> Row {
    let document = save_document(world);
    let valid = document.validate();
    if let Err(error) = &valid {
        return ("S09", false, format!("validate: {error:?}"));
    }
    let state = match document.to_state() {
        Ok(state) => state,
        Err(error) => return ("S09", false, format!("to_state: {error:?}")),
    };
    let keys: Vec<&'static str> = state.sections.iter().map(|section| section.id.key()).collect();
    let mut expected: Vec<&'static str> = M33_SECTIONS.iter().map(|(key, _)| *key).collect();
    // `SaveStateV2::sections` is kept in `SectionId` order — the key's
    // lexicographic order (save_v2 §"Wire format"), not the table's order.
    expected.sort_unstable();
    let text = document.to_text();
    let from_state = SaveGameV2::from_state(&state)
        .map(|doc| doc.to_text() == text)
        .unwrap_or(false);
    let from_text = SaveGameV2::from_text(&text)
        .map(|doc| doc.to_text() == text)
        .unwrap_or(false);
    let written = document.save_to_path(path);
    run_frames(world, STEPS_PER_SCENARIO, |_, _| {});
    let (passed, failures) = verdict(&[
        (state.version == SAVE_VERSION, "version"),
        (state.sections.len() == M33_SECTIONS.len(), "sections"),
        (keys == expected, "section order"),
        (state.sections.iter().all(|section| !section.bytes.is_empty()), "payloads"),
        (from_state, "from_state"),
        (from_text, "from_text"),
        (written.is_ok(), "write"),
    ]);
    let detail = format!(
        "version={} sections={}/{} entries={} digest={:016x} bytes={}{}",
        state.version,
        state.sections.len(),
        M33_SECTIONS.len(),
        document.entry_count(),
        document.digest(),
        text.len(),
        if failures.is_empty() {
            String::new()
        } else {
            format!(" {failures}")
        }
    );
    ("S09", passed, detail)
}

// ── S10 reload ─────────────────────────────────────────────────────────────

fn s10_reload(saved: &SaveGameV2, path: &Path, world: &mut World) -> Row {
    let loaded = SaveGameV2::load_from_path(path);
    run_frames(world, STEPS_PER_SCENARIO, |_, _| {});
    let Ok(loaded) = loaded else {
        return ("S10", false, "reload failed".to_string());
    };
    let saved_digest = saved.digest();
    let loaded_digest = loaded.digest();
    let (passed, failures) = verdict(&[
        (saved_digest == loaded_digest, "digest"),
        (saved.to_text() == loaded.to_text(), "text"),
        (loaded.entry_count() == saved.entry_count(), "entries"),
    ]);
    let detail = format!(
        "digest={saved_digest:016x}=={loaded_digest:016x} entries={}{}",
        loaded.entry_count(),
        if failures.is_empty() {
            String::new()
        } else {
            format!(" {failures}")
        }
    );
    ("S10", passed, detail)
}

// ── S11 restore ────────────────────────────────────────────────────────────

fn s11_restore(saved: &SaveGameV2, path: &Path, world: &mut World) -> Row {
    let loaded = SaveGameV2::load_from_path(path);
    run_frames(world, STEPS_PER_SCENARIO, |_, _| {});
    let Ok(loaded) = loaded else {
        return ("S11", false, "reload failed".to_string());
    };
    let position_ok = loaded.world.pos_x_mm == saved.world.pos_x_mm
        && loaded.world.pos_y_mm == saved.world.pos_y_mm
        && loaded.world.pos_z_mm == saved.world.pos_z_mm;
    let goods_lost = saved
        .inventory
        .goods
        .len()
        .saturating_sub(loaded.inventory.goods.len());
    let items_lost = saved
        .inventory
        .items
        .len()
        .saturating_sub(loaded.inventory.items.len());
    let goods_equal = saved.inventory.goods == loaded.inventory.goods;
    let items_equal = saved.inventory.items == loaded.inventory.items;
    let (passed, failures) = verdict(&[
        (position_ok, "position"),
        (goods_lost == 0, "goods"),
        (items_lost == 0, "items"),
        (goods_equal, "goods set"),
        (items_equal, "item set"),
        (
            loaded.inventory.gold == saved.inventory.gold,
            "gold",
        ),
        (
            loaded.character.health == saved.character.health
                && loaded.character.spell_points == saved.character.spell_points,
            "character",
        ),
    ]);
    let detail = format!(
        "pos=({},{},{})mm goods={} items={} lost={} gold={}{}",
        loaded.world.pos_x_mm,
        loaded.world.pos_y_mm,
        loaded.world.pos_z_mm,
        loaded.inventory.goods.len(),
        loaded.inventory.items.len(),
        goods_lost + items_lost,
        loaded.inventory.gold,
        if failures.is_empty() {
            String::new()
        } else {
            format!(" {failures}")
        }
    );
    ("S11", passed, detail)
}

// ── S12 second save/load ───────────────────────────────────────────────────

fn s12_second_cycle(saved: &SaveGameV2, path: &Path, world: &mut World) -> Row {
    let first = SaveGameV2::load_from_path(path);
    let second_path = path.with_extension("second.dfl");
    run_frames(world, STEPS_PER_SCENARIO, |_, _| {});
    let Ok(first) = first else {
        return ("S12", false, "first reload failed".to_string());
    };
    if let Err(error) = first.save_to_path(&second_path) {
        return ("S12", false, format!("second write: {error:?}"));
    }
    let second = SaveGameV2::load_from_path(&second_path);
    let first_bytes = std::fs::read(path);
    let second_bytes = std::fs::read(&second_path);
    let _ = std::fs::remove_file(&second_path);
    let Ok(second) = second else {
        return ("S12", false, "second reload failed".to_string());
    };
    let saved_digest = saved.digest();
    let (passed, failures) = verdict(&[
        (second.digest() == saved_digest, "digest"),
        (
            first_bytes.as_ref().ok() == second_bytes.as_ref().ok(),
            "file bytes",
        ),
        (second.to_text() == first.to_text(), "text"),
    ]);
    let detail = format!(
        "digest={saved_digest:016x}=={:016x} bytes={}{}",
        second.digest(),
        second_bytes.as_ref().map(|bytes| bytes.len()).unwrap_or(0),
        if failures.is_empty() {
            String::new()
        } else {
            format!(" {failures}")
        }
    );
    ("S12", passed, detail)
}

// ── entry point ────────────────────────────────────────────────────────────

/// `--probe PLAYSESSION-001`: the twelve-scenario deterministic playthrough.
pub fn run_playsession_001() -> i32 {
    println!(
        "[PLAYSESSION-001] 12 escenarios x {STEPS_PER_SCENARIO} pasos @ 1/{FPS} s = \
         {} pasos ({:.0} s); juego={} s ({} min); seed={SEED:#x}",
        total_steps(),
        simulation_seconds(),
        game_seconds_total(),
        game_seconds_total() / SECONDS_PER_MINUTE
    );

    // Prerequisites FIRST: a missing one is EXIT=2, never a synthetic row.
    let m33 = match m33_gate() {
        Ok(line) => line,
        Err(reason) => {
            println!("[EXIT=2] prerequisite M33 (savegame v2/migración): {reason}");
            return 2;
        }
    };
    println!("[gate] M33 {m33}");
    let retail = match load_retail() {
        Ok(retail) => retail,
        Err(reason) => {
            println!("[EXIT=2] prerequisite retail (ARENA2 custody): {reason}");
            return 2;
        }
    };
    println!(
        "[gate] retail MAPS.BSA={}B BLOCKS.BSA={}B towns={} dungeons={}",
        retail.maps_bytes,
        retail.blocks_bytes,
        retail.towns.len(),
        retail.dungeons.len()
    );

    let origin = &retail.towns[0];
    let destination = &retail.towns[1];
    let save_path = std::env::temp_dir().join(format!(
        "playsession-001-{}-v2.dfl",
        std::process::id()
    ));

    let mut world = World::new();
    let mut rows: Vec<Row> = vec![
        s01_title(&retail.assets, &mut world),
        s02_new_game(origin, &mut world),
        s03_movement(&mut world),
        s04_enter_town(origin, destination, &mut world),
        s05_buy(&mut world),
        s06_dungeon(&retail, &mut world),
        s07_spell(&mut world),
        s08_combat(&mut world),
    ];

    let document = save_document(&world);
    rows.push(s09_save(&mut world, &save_path));
    rows.push(s10_reload(&document, &save_path, &mut world));
    rows.push(s11_restore(&document, &save_path, &mut world));
    rows.push(s12_second_cycle(&document, &save_path, &mut world));
    let _ = std::fs::remove_file(&save_path);

    let mut passed = 0usize;
    for (id, ok, detail) in &rows {
        if *ok {
            passed += 1;
        }
        println!("{id}: {} {detail}", if *ok { "PASS" } else { "FAIL" });
    }

    println!();
    println!("escenarios: {passed}/{SCENARIOS}");
    println!("pasos: {}", world.frames);
    println!("digest={:016x}", world.digest);

    if passed == SCENARIOS && world.frames as usize == total_steps() {
        0
    } else {
        1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn signature(gold: i32) -> FrameSignature {
        FrameSignature {
            frame: 1,
            game_minutes: 300,
            scene: scene_code(SceneId::Exterior),
            position_mm: [4_000, 1_600, 300],
            health: 100,
            spell_points: 76,
            gold,
            goods: 2,
            active_effects: 1,
        }
    }

    #[test]
    fn step_budget_is_twelve_by_eighteen_thousand() {
        assert_eq!(SCENARIOS, 12);
        assert_eq!(STEPS_PER_SCENARIO, 18_000);
        assert_eq!(total_steps(), 216_000);
    }

    #[test]
    fn simulation_seconds_match_the_declared_hour() {
        assert_eq!(simulation_seconds(), 3_600.0);
        assert_eq!(game_seconds_per_frame(), 1);
        assert_eq!(game_seconds_total(), 216_000);
        assert_eq!(game_seconds_total() / SECONDS_PER_MINUTE, 3_600);
    }

    #[test]
    fn step_metres_is_the_walk_speed_over_the_frame_rate() {
        assert_eq!(step_metres(), WALK_SPEED_MPS / 60.0);
    }

    #[test]
    fn frame_digest_is_a_function_of_the_signature() {
        let first = fold_frame(FNV_OFFSET, &signature(0));
        let second = fold_frame(FNV_OFFSET, &signature(0));
        let changed = fold_frame(FNV_OFFSET, &signature(1));
        assert_eq!(first, second);
        assert_ne!(first, changed);
    }

    #[test]
    fn frame_signature_encoding_is_fixed_width_and_field_separated() {
        let base = signature(0);
        assert_eq!(base.to_bytes().len(), 49);
        let mut other = base;
        other.gold += 1;
        assert_ne!(base.to_bytes(), other.to_bytes());
        let mut later = base;
        later.frame += 1;
        assert_ne!(base.to_bytes(), later.to_bytes());
    }

    #[test]
    fn location_markup_follows_the_location_triple() {
        let mut session = GameSession::new(SEED, CareerId(CAREER));
        let home = location_markup(&session, "home");
        assert!(session.start(LocationRef::new(19, 3, 0)).is_ok());
        let away = location_markup(&session, "away");
        assert_ne!(home, away);
        assert!(away.starts_with("19.3.0:away#"));
    }

    #[test]
    fn millimetre_conversion_rounds_half_away_from_zero() {
        assert_eq!(to_mm(1.5), 1_500);
        assert_eq!(to_mm(-1.5), -1_500);
        assert_eq!(to_mm(0.0004), 0);
    }

    #[test]
    fn scene_codes_follow_the_session_codes() {
        assert_eq!(scene_code(SceneId::Menu), 1);
        assert_eq!(scene_code(SceneId::Exterior), 2);
        assert_eq!(scene_code(SceneId::Interior), 3);
    }
}
