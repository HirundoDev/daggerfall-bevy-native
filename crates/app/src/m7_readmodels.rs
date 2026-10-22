//! M7 — ECS composition of the PD4 T7 session read models: **the `app`
//! composes** (bus-only, bevy-bridge-matrix — organs do not call each other;
//! M5 precedent "the `app` composes").
//!
//! Each resource wraps its source organ type —
//! `M7Stats` 🎮 skills (`RpgStats`) · `M7Chunks` 🧠 memory (`ChunkMap`) ·
//! `M7Assets` 📦 inventory (`InventoryLite` = AssetSet) · `M7Quests` 🎭
//! behavior (`QuestLog`) — and [`m7_project_read_models`] projects them via
//! the 👁️ perception DTOs into the [`M7ReadModels`] vault (the 3 PD4 T7 read
//! models, REQ-L99-038/039/040).
//!
//! The SAME system runs in the headless `PROBE-READMODEL-001` probe
//! (change in ECS ⇒ change in read model — anti-screen) and in the M7 window
//! (visual evidence, m5_window pattern). 0 duplicated logic.

use bevy::prelude::*;

use daggerfall_behavior::quest::QuestLog;
use daggerfall_inventory::commerce::InventoryLite;
use daggerfall_memory::chunks::ChunkMap;
use daggerfall_perception::read_models::{
    AutomapSource, HudSource, MapCell, PanelSource, SessionReadModels,
};
use daggerfall_skills::stats::RpgStats;

/// Session RPG stats (🎮 skills → 👁️ perception).
#[derive(Resource, Debug)]
pub struct M7Stats(pub RpgStats);

/// Session loaded chunks (🧠 memory → 👁️ perception).
#[derive(Resource, Debug)]
pub struct M7Chunks(pub ChunkMap);

/// Session inventory/AssetSet (📦 inventory → 👁️ perception).
#[derive(Resource, Debug)]
pub struct M7Assets(pub InventoryLite);

/// Session active quests (🎭 behavior → 👁️ perception).
#[derive(Resource, Debug)]
pub struct M7Quests(pub QuestLog);

/// Vault of the 3 session read models (PD4 T7) — consumed by the M7 window
/// title bar and the PROBE-READMODEL-001 probe.
#[derive(Resource, Debug, Default)]
pub struct M7ReadModels(pub SessionReadModels);

/// Composer → HUD DTO mapping (source: `RpgStats` from 🎮 skills).
pub fn hud_source_of(stats: &RpgStats) -> HudSource {
    HudSource {
        health: stats.health,
        max_health: stats.max_health,
        stamina: stats.stamina,
        max_stamina: stats.max_stamina,
        level: stats.level,
    }
}

/// Composer → Automap DTO mapping (source: `ChunkMap` from 🧠 memory).
pub fn automap_source_of(chunks: &ChunkMap) -> AutomapSource {
    AutomapSource {
        cells: chunks
            .iter()
            .map(|coord| MapCell {
                x: coord.x,
                z: coord.z,
            })
            .collect(),
    }
}

/// Composer → Panel DTO mapping (sources: `InventoryLite` from 📦 inventory
/// = AssetSet + `QuestLog` from 🎭 behavior).
pub fn panel_source_of(assets: &InventoryLite, quests: &QuestLog) -> PanelSource {
    PanelSource {
        gold: assets.gold,
        items: assets.goods.clone(),
        quests: quests.active().iter().map(|quest| quest.title.clone()).collect(),
    }
}

/// Joint projection of the 3 read models from session state (used by the live
/// system and by the probe anti-drift check).
pub fn expected_models(
    stats: &RpgStats,
    chunks: &ChunkMap,
    assets: &InventoryLite,
    quests: &QuestLog,
) -> SessionReadModels {
    SessionReadModels::project(
        &hud_source_of(stats),
        &automap_source_of(chunks),
        &panel_source_of(assets, quests),
    )
}

/// Live ECS → read models projection system: pure, deterministic and
/// re-runnable on every tick — the ONLY path that changes the vault is a
/// change in the session resources (anti-screen).
pub fn m7_project_read_models(
    stats: Res<M7Stats>,
    chunks: Res<M7Chunks>,
    assets: Res<M7Assets>,
    quests: Res<M7Quests>,
    mut vault: ResMut<M7ReadModels>,
) {
    vault.0 = expected_models(&stats.0, &chunks.0, &assets.0, &quests.0);
}
