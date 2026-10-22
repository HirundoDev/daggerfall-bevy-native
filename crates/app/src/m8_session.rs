//! M8 — closed session loop: **ECS composition of save/load**
//! (PD4 T2 L120 "Session/Saves → heart (loop) + memory",
//! INV-L99-4 / REQ-L99-041).
//!
//! **The format and the I/O** belong to 🧠 Memory
//! (`daggerfall_memory::session_store`); what lives here belongs to the
//! `app` (the composer, M5/M7 precedent "the `app` composes", bus-only —
//! 0 organ→organ dependency):
//!
//! | State | ECS resource | Source organ | Snapshot fields |
//! |---|---|---|---|
//! | Position | [`M8Position`] (mm/mrad integers) | session (app) | `pos_x_mm`/`pos_z_mm`/`yaw_mrad` |
//! | Time | [`M8Clock`] (`GameClock`) | ❤️ heart | `clock_*` (6 integers) |
//! | Stats | `M7Stats` (`RpgStats`) | 🎮 skills | `level`…`max_stamina` |
//! | Inventory | `M7Assets` (`InventoryLite`) | 📦 inventory | `gold` + `goods` |
//! | Quests | `M7Quests` (`QuestLog`) | 🎭 behavior | `quests` |
//! | Chunks | `M7Chunks` (`ChunkMap`) | 🧠 memory | `chunks` |
//!
//! [`save_session`] freezes the ECS state into a [`SessionSnapshot`] and
//! writes it; [`load_session`] reads it and **restores the resources** of a
//! new app — the byte-by-byte roundtrip is verified by `PROBE-SESSION-001`
//! (anti-screen: restoration is measured by snapshot and read-model
//! equality, not by a screenshot).
//!
//! **Save location:** [`default_save_path`] = `saves/session.dfs`
//! under the working directory (visible and tangible for the VE; the probe
//! uses its own temporary directory — 0 artifacts in the repo).

use std::path::{Path, PathBuf};

use bevy::prelude::*;
use daggerfall_behavior::quest::QuestLog;
use daggerfall_heart::clock::GameClock;
use daggerfall_inventory::commerce::InventoryLite;
use daggerfall_memory::chunks::{ChunkCoord, ChunkMap};
use daggerfall_memory::session_store::{SaveError, SessionSnapshot};
use daggerfall_skills::stats::RpgStats;

use crate::m7_readmodels::{M7Assets, M7Chunks, M7Quests, M7Stats};

/// Session position in **fixed integers** (X/Z millimetres + yaw
/// milliradians — NFR NUMERIC-never-FLOAT; in the `window` config it is
/// mapped from/to the first-person camera `Transform`).
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct M8Position {
    /// X column in millimetres.
    pub x_mm: i64,
    /// Z row in millimetres.
    pub z_mm: i64,
    /// Yaw in milliradians.
    pub yaw_mrad: i64,
}

/// Session time (❤️ heart owns the time loop — PRE-3 §3). The save freezes
/// the 6 integer fields of [`GameClock`].
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq)]
pub struct M8Clock(pub GameClock);

impl Default for M8Clock {
    fn default() -> Self {
        Self(GameClock::classic_default())
    }
}

/// Default relative path of the session save (root = process CWD).
pub const SAVE_FILE_REL: &str = "saves/session.dfs";

/// Default save path: `saves/session.dfs` under the CWD.
pub fn default_save_path() -> PathBuf {
    std::env::current_dir()
        .map(|dir| dir.join(SAVE_FILE_REL))
        .unwrap_or_else(|_| PathBuf::from(SAVE_FILE_REL))
}

/// Headless ECS app with the M7 session (stats/chunks/inventory/quests) +
/// the M8 resources (position + ❤️ clock). The SAME wiring is used by the
/// `PROBE-SESSION-001` probe and the M8 window.
pub fn m8_session_app() -> App {
    let mut app = crate::m7_probe::session_app();
    app.insert_resource(M8Position::default())
        .insert_resource(M8Clock::default());
    app
}

/// Freezes the ECS state into a [`SessionSnapshot`] (composer → primitives
/// mapping, bus-only). Pure function of the session state.
pub fn snapshot_of(
    pos: &M8Position,
    clock: &GameClock,
    stats: &RpgStats,
    chunks: &ChunkMap,
    assets: &InventoryLite,
    quests: &QuestLog,
) -> SessionSnapshot {
    SessionSnapshot {
        pos_x_mm: pos.x_mm,
        pos_z_mm: pos.z_mm,
        yaw_mrad: pos.yaw_mrad,
        clock_year: i64::from(clock.year),
        clock_month: i64::from(clock.month),
        clock_day: i64::from(clock.day),
        clock_hour: i64::from(clock.hour),
        clock_minute: i64::from(clock.minute),
        clock_second: i64::from(clock.second),
        level: i64::from(stats.level),
        experience: i64::from(stats.experience),
        health: i64::from(stats.health),
        max_health: i64::from(stats.max_health),
        stamina: i64::from(stats.stamina),
        max_stamina: i64::from(stats.max_stamina),
        gold: i64::from(assets.gold),
        goods: assets
            .goods
            .iter()
            .map(|(id, qty)| (id.clone(), i64::from(*qty)))
            .collect(),
        quests: quests
            .active()
            .iter()
            .map(|quest| (quest.id.clone(), quest.title.clone()))
            .collect(),
        chunks: chunks.iter().map(|c| (i64::from(c.x), i64::from(c.z))).collect(),
    }
}

/// Snapshot of the full session ECS state (resource reads).
pub fn snapshot_from_world(world: &World) -> SessionSnapshot {
    let pos = world.resource::<M8Position>();
    let clock = &world.resource::<M8Clock>().0;
    let stats = &world.resource::<M7Stats>().0;
    let chunks = &world.resource::<M7Chunks>().0;
    let assets = &world.resource::<M7Assets>().0;
    let quests = &world.resource::<M7Quests>().0;
    snapshot_of(pos, clock, stats, chunks, assets, quests)
}

/// **Saves** the ECS session to `path` (memory owns the format). Returns the
/// frozen snapshot (the probe compares it against the restoration).
pub fn save_session(world: &mut World, path: &Path) -> Result<SessionSnapshot, SaveError> {
    let snapshot = snapshot_from_world(world);
    snapshot.save_to_path(path)?;
    Ok(snapshot)
}

/// **Loads** the session from `path` and restores the app resources
/// (roundtrip: what one session saved comes back into a new app). Missing
/// file → honest [`SaveError::Io`] (no fabricated default state).
pub fn load_session(world: &mut World, path: &Path) -> Result<SessionSnapshot, SaveError> {
    let snapshot = SessionSnapshot::load_from_path(path)?;
    restore_world(world, &snapshot)?;
    Ok(snapshot)
}

/// Restores the ECS resources from a snapshot (exact inverse of
/// [`snapshot_from_world`] — any impossible value is corruption).
pub fn restore_world(world: &mut World, snap: &SessionSnapshot) -> Result<(), SaveError> {
    let to_i32 = |value: i64, field: &str| {
        i32::try_from(value)
            .map_err(|_| SaveError::Corrupt(format!("{field} out of i32 range: {value}")))
    };
    let to_u32 = |value: i64, field: &str| {
        u32::try_from(value)
            .map_err(|_| SaveError::Corrupt(format!("{field} out of u32 range: {value}")))
    };

    let stats = RpgStats {
        level: to_i32(snap.level, "level")?,
        experience: to_i32(snap.experience, "experience")?,
        health: to_i32(snap.health, "health")?,
        max_health: to_i32(snap.max_health, "max_health")?,
        stamina: to_i32(snap.stamina, "stamina")?,
        max_stamina: to_i32(snap.max_stamina, "max_stamina")?,
    };
    let mut chunks = ChunkMap::new();
    for (x, z) in &snap.chunks {
        let x = to_i32(*x, "chunk.x")?;
        let z = to_i32(*z, "chunk.z")?;
        chunks.load(ChunkCoord::new(x, z));
    }
    let mut goods = Vec::with_capacity(snap.goods.len());
    for (id, qty) in &snap.goods {
        goods.push((id.clone(), to_i32(*qty, "goods.qty")?));
    }
    let assets = InventoryLite {
        gold: to_i32(snap.gold, "gold")?,
        goods,
    };
    let mut quests = QuestLog::new();
    for (id, title) in &snap.quests {
        let accepted = quests.accept(id, title);
        if !accepted {
            return Err(SaveError::Corrupt(format!(
                "duplicate quest in the save: {id:?}"
            )));
        }
    }
    let clock = GameClock {
        year: to_u32(snap.clock_year, "clock.year")?,
        month: to_u32(snap.clock_month, "clock.month")?,
        day: to_u32(snap.clock_day, "clock.day")?,
        hour: to_u32(snap.clock_hour, "clock.hour")?,
        minute: to_u32(snap.clock_minute, "clock.minute")?,
        second: to_u32(snap.clock_second, "clock.second")?,
    };

    world.insert_resource(M8Position {
        x_mm: snap.pos_x_mm,
        z_mm: snap.pos_z_mm,
        yaw_mrad: snap.yaw_mrad,
    });
    world.insert_resource(M8Clock(clock));
    world.insert_resource(M7Stats(stats));
    world.insert_resource(M7Chunks(chunks));
    world.insert_resource(M7Assets(assets));
    world.insert_resource(M7Quests(quests));
    Ok(())
}

/// M8 script over the M7 session (the SAME in probe and window): the M7
/// script (damage → chunk → DFU purchase → quest → XP) **+ position + ❤️
/// clock** — the 6 snapshot states end up mutated. Returns the price paid.
pub fn apply_m8_script(app: &mut App) -> i32 {
    let paid = crate::m7_probe::apply_session_script(app);
    let world = app.world_mut();
    world.resource_mut::<M8Position>().x_mm = crate::m8_probe::M8_SCRIPT_POS_MM.0;
    world.resource_mut::<M8Position>().z_mm = crate::m8_probe::M8_SCRIPT_POS_MM.1;
    world.resource_mut::<M8Position>().yaw_mrad = crate::m8_probe::M8_SCRIPT_YAW_MRAD;
    world
        .resource_mut::<M8Clock>()
        .0
        .raise_seconds(crate::m8_probe::M8_SCRIPT_CLOCK_SECS);
    paid
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_roundtrip_through_world_is_identity() {
        let mut app = m8_session_app();
        app.update();
        apply_m8_script(&mut app);
        app.update();

        let before = snapshot_from_world(app.world());
        let mut twin = m8_session_app();
        twin.update();
        restore_world(twin.world_mut(), &before).expect("restore");
        twin.update();
        let after = snapshot_from_world(twin.world());
        assert_eq!(before, after, "restore into a new app ≠ saved session");
    }

    #[test]
    fn load_missing_file_is_honest_error() {
        let mut app = m8_session_app();
        app.update();
        let missing = std::env::temp_dir()
            .join(format!("daggerfall-m8-unit-missing-{}.dfs", std::process::id()));
        let _ = std::fs::remove_file(&missing);
        let err = load_session(app.world_mut(), &missing).expect_err("no save ⇒ error");
        assert!(
            SessionSnapshot::is_not_found(&err),
            "expected an honest NotFound: {err}"
        );
    }
}
