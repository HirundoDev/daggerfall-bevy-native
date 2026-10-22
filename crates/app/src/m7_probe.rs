//! PROBE-READMODEL-001 — session read models (M7).
//!
//! Anti-screen sequence of the C3 pack: the **3 PD4 T7 read models**
//! (HUD ← RPGStats · Automap ← Chunks · inventory/quests Panel ←
//! AssetSet/Quest — REQ-L99-038/039/040) are projected from the ECS at
//! runtime with asserts: **change in ECS ⇒ change in read model**, vault
//! locked to the fresh projection (0 hidden state) and byte-by-byte
//! determinism between runs. 0 screenshot-only.
//!
//! Runs in the **default headless** config and — delta documented in the M7
//! note — **does not read ARCH3D.BSA** (the projections do not need the retail
//! corpus: 0 I/O, the session is declared synthetic content); with
//! `--features window` and PASS, the M7 window (`m7_window`) opens the visual
//! evidence (automap with tiles in the client area + the 3 read models in the
//! title bar).
//!
//! Composition in `app` ([`crate::m7_readmodels`]: session resources +
//! projection system — organs do NOT call each other, bus-only). The panel
//! reuses the M5 session ([`crate::m5_probe`]: same starting inventory, same
//! DFU purchase formula — 0 probe↔window divergence).

use bevy::prelude::*;

use daggerfall_behavior::quest::QuestLog;
use daggerfall_inventory::commerce::{InventoryLite, TradeSession};
use daggerfall_memory::chunks::{ChunkCoord, ChunkMap};
use daggerfall_parser_bevy_bridge::ProbeCheck;
use daggerfall_perception::read_models::SessionReadModels;
use daggerfall_skills::stats::RpgStats;

use crate::m5_probe::{dfu_buy_price, merchant_stock, starting_inventory};
use crate::m7_readmodels::{M7Assets, M7Chunks, M7Quests, M7ReadModels, M7Stats};

/// Initial quest of the M7 session (accepted on startup — declared synthetic
/// content, M7 note §N).
pub const M7_INITIAL_QUEST_ID: &str = "q-vendel-shop";
/// Title of [`M7_INITIAL_QUEST_ID`] (visible in the panel/title bar).
pub const M7_INITIAL_QUEST_TITLE: &str = "Find Vendel's shop";
/// Quest accepted by the session script (step 4) — id.
pub const M7_EXTRA_QUEST_ID: &str = "q-cavern";
/// Quest accepted by the session script (step 4) — title.
pub const M7_EXTRA_QUEST_TITLE: &str = "Explore the cave to the east";
/// Initial chunks of the session (2×2 — path already travelled, declared
/// synthetic content).
pub const M7_INITIAL_CHUNKS: [(i32, i32); 4] = [(0, 0), (1, 0), (0, 1), (1, 1)];
/// Chunk loaded by the session script (step 2).
pub const M7_EXTRA_CHUNK: (i32, i32) = (2, 0);
/// Damage applied by the script (step 1) — HP 100 → 70.
pub const M7_SCRIPT_DAMAGE: i32 = 30;
/// Skill-sum progress applied by the script (step 5) — Lv 1 → 2
/// with the DFU formula `floor((progress + 28) / 15)` (M20, spec §7.8: the
/// synthetic `100 × level` threshold no longer exists).
pub const M7_SCRIPT_LEVEL_PROGRESS: i32 = 2;

/// Initial session stats (HP/STA 100/100, Lv 1).
pub fn initial_stats() -> RpgStats {
    RpgStats::new(1, 100, 100)
}

/// Initial session chunks ([`M7_INITIAL_CHUNKS`]).
pub fn initial_chunks() -> ChunkMap {
    let mut map = ChunkMap::new();
    for (x, z) in M7_INITIAL_CHUNKS {
        map.load(ChunkCoord::new(x, z));
    }
    map
}

/// Initial session quests ([`M7_INITIAL_QUEST_ID`]).
pub fn initial_quests() -> QuestLog {
    let mut log = QuestLog::new();
    let accepted = log.accept(M7_INITIAL_QUEST_ID, M7_INITIAL_QUEST_TITLE);
    debug_assert!(accepted, "single initial quest");
    log
}

/// Initial inventory: the SAME as the M5 session (gold 100 + furs×1 —
/// M5 probe ↔ M7 probe ↔ M7 window parity).
pub fn initial_inventory() -> InventoryLite {
    starting_inventory()
}

/// Full report of PROBE-READMODEL-001.
#[derive(Debug, Clone)]
pub struct ReadModelProbeReport {
    /// Checks executed, in order.
    pub checks: Vec<ProbeCheck>,
    /// Global PASS = all checks ok.
    pub pass: bool,
}

/// Headless ECS app with the M7 session + the live projection system
/// (the SAME wiring as the M7 window — [`crate::m7_readmodels`]).
pub fn session_app() -> App {
    let mut app = App::new();
    app.insert_resource(M7Stats(initial_stats()))
        .insert_resource(M7Chunks(initial_chunks()))
        .insert_resource(M7Assets(initial_inventory()))
        .insert_resource(M7Quests(initial_quests()))
        .init_resource::<M7ReadModels>()
        .add_systems(Update, crate::m7_readmodels::m7_project_read_models);
    app
}

/// M7 session script (the SAME in the probe and the window, step by step):
/// damage → loaded chunk → DFU purchase → accepted quest → XP. Returns the
/// price paid (DFU formula).
///
/// M8: `pub(crate)` — the session loop (`m8_session`) reuses THIS script as
/// part of its own (0 duplicated logic).
pub(crate) fn apply_session_script(app: &mut App) -> i32 {
    let world = app.world_mut();

    // Step 1 — damage + experience in the HUD.
    {
        let mut stats = world.resource_mut::<M7Stats>();
        stats.0.damage(M7_SCRIPT_DAMAGE);
        stats.0.gain_level_progress(M7_SCRIPT_LEVEL_PROGRESS);
    }

    // Step 2 — new chunk loaded into the automap.
    world
        .resource_mut::<M7Chunks>()
        .0
        .load(ChunkCoord::new(M7_EXTRA_CHUNK.0, M7_EXTRA_CHUNK.1));

    // Step 3 — real purchase (M5 session: first stock item, DFU price).
    let mut trade = TradeSession::new(merchant_stock(), world.resource::<M7Assets>().0.clone());
    trade.open_dialogue();
    let (item_id, base_cost) = trade
        .first_stock()
        .map(|item| (item.id.clone(), item.base_cost))
        .expect("non-empty M5 catalog");
    let price = dfu_buy_price(base_cost);
    let paid = trade
        .buy(&item_id, price)
        .expect("initial gold 100 ≥ DFU price");
    world.resource_mut::<M7Assets>().0 = trade.inventory;

    // Step 4 — quest accepted into the panel.
    let accepted = world
        .resource_mut::<M7Quests>()
        .0
        .accept(M7_EXTRA_QUEST_ID, M7_EXTRA_QUEST_TITLE);
    debug_assert!(accepted, "single extra quest");
    paid
}

/// Anti-drift (anti-screen core): recomputes the 3 read models from the ECS
/// and requires the vault to match — the only way a read model changes is by
/// changing its ECS source.
fn verify_vault(
    vault: &SessionReadModels,
    stats: &RpgStats,
    chunks: &ChunkMap,
    assets: &InventoryLite,
    quests: &QuestLog,
) -> Vec<ProbeCheck> {
    let expected = crate::m7_readmodels::expected_models(stats, chunks, assets, quests);
    vec![
        ProbeCheck {
            id: "vault_hud_locked_to_ecs",
            ok: vault.hud == expected.hud,
            detail: format!(
                "vault HUD [{}] == fresh ECS projection [{}]",
                vault.hud.summary(),
                expected.hud.summary()
            ),
        },
        ProbeCheck {
            id: "vault_automap_locked_to_ecs",
            ok: vault.automap == expected.automap,
            detail: format!(
                "vault automap ({} chunks) == fresh projection ({} chunks)",
                vault.automap.chunk_count(),
                expected.automap.chunk_count()
            ),
        },
        ProbeCheck {
            id: "vault_panel_locked_to_ecs",
            ok: vault.panel == expected.panel,
            detail: format!(
                "vault panel [{}] == fresh projection [{}]",
                vault.panel.summary(),
                expected.panel.summary()
            ),
        },
    ]
}

/// Executable core of **PROBE-READMODEL-001** — the SAME code is run by the
/// CLI and the tests (anti-screen).
pub fn run_readmodel_probe() -> ReadModelProbeReport {
    let mut checks: Vec<ProbeCheck> = Vec::with_capacity(11);
    let mut app = session_app();
    app.update();

    // ── Initial state: the 3 read models project the session ────────────
    {
        let vault = &app.world().resource::<M7ReadModels>().0;
        let chunks = app.world().resource::<M7Chunks>().0.clone();
        let assets = app.world().resource::<M7Assets>().0.clone();
        let quests = app.world().resource::<M7Quests>().0.clone();
        let stats = app.world().resource::<M7Stats>().0.clone();

        let hud = &vault.hud;
        checks.push(ProbeCheck {
            id: "initial_hud_projects_stats",
            ok: (hud.health, hud.max_health, hud.health_pct) == (100, 100, 100)
                && (hud.stamina, hud.max_stamina, hud.stamina_pct) == (100, 100, 100)
                && hud.level == 1
                && hud.conscious,
            detail: format!("RpgStats(1,100,100) → HUD [{}]", hud.summary()),
        });

        let automap = &vault.automap;
        checks.push(ProbeCheck {
            id: "initial_automap_projects_chunks",
            ok: automap.chunk_count() == M7_INITIAL_CHUNKS.len()
                && M7_INITIAL_CHUNKS
                    .iter()
                    .all(|(x, z)| automap.contains(*x, *z))
                && automap.render_ascii() == "##\n##",
            detail: format!(
                "ChunkMap(2×2) → automap {} chunks · grid:\n{}",
                automap.chunk_count(),
                automap.render_ascii()
            ),
        });

        let panel = &vault.panel;
        checks.push(ProbeCheck {
            id: "initial_panel_projects_inventory_quests",
            ok: panel.gold == 100
                && panel.items == vec![("furs".to_string(), 1)]
                && panel.quests == vec![M7_INITIAL_QUEST_TITLE.to_string()],
            detail: format!("InventoryLite+QuestLog → panel [{}]", panel.summary()),
        });

        checks.extend(verify_vault(vault, &stats, &chunks, &assets, &quests));
    }

    // ── Change in ECS ⇒ change in the 3 read models ─────────────────────
    let paid = apply_session_script(&mut app);
    app.update();
    {
        let vault = &app.world().resource::<M7ReadModels>().0;
        let hud = &vault.hud;
        checks.push(ProbeCheck {
            id: "hud_follows_ecs_change",
            ok: (hud.health, hud.health_pct) == (70, 70) && hud.level == 2 && hud.conscious,
            detail: format!(
                "damage({M7_SCRIPT_DAMAGE}) + level_progress({M7_SCRIPT_LEVEL_PROGRESS}) in ECS → HUD [{}]",
                hud.summary()
            ),
        });

        let automap = &vault.automap;
        checks.push(ProbeCheck {
            id: "automap_follows_ecs_change",
            ok: automap.chunk_count() == M7_INITIAL_CHUNKS.len() + 1
                && automap.contains(M7_EXTRA_CHUNK.0, M7_EXTRA_CHUNK.1),
            detail: format!(
                "load({},{}) in ECS → automap {} chunks (grid:\n{})",
                M7_EXTRA_CHUNK.0,
                M7_EXTRA_CHUNK.1,
                automap.chunk_count(),
                automap.render_ascii()
            ),
        });

        let panel = &vault.panel;
        checks.push(ProbeCheck {
            id: "panel_follows_ecs_change",
            ok: panel.gold == 100 - paid
                && panel.items.contains(&("potion".to_string(), 1))
                && panel.quests.len() == 2
                && panel
                    .quests
                    .iter()
                    .any(|title| title == M7_EXTRA_QUEST_TITLE),
            detail: format!(
                "buy(potion, DFU price {paid}) + accept({M7_EXTRA_QUEST_ID}) in ECS → panel [{}]",
                panel.summary()
            ),
        });

        let stats = app.world().resource::<M7Stats>().0.clone();
        let chunks = app.world().resource::<M7Chunks>().0.clone();
        let assets = app.world().resource::<M7Assets>().0.clone();
        let quests = app.world().resource::<M7Quests>().0.clone();
        checks.extend(verify_vault(vault, &stats, &chunks, &assets, &quests));
    }

    // ── Determinism: 2nd app, SAME script → SAME vault ─────────────────
    let final_vault = app.world().resource::<M7ReadModels>().0.clone();
    let mut twin = session_app();
    twin.update();
    let twin_paid = apply_session_script(&mut twin);
    twin.update();
    let twin_vault = twin.world().resource::<M7ReadModels>().0.clone();
    checks.push(ProbeCheck {
        id: "projection_is_deterministic",
        ok: twin_paid == paid && twin_vault == final_vault,
        detail: format!(
            "2nd app with the same script → payment {twin_paid} == {paid} and identical vault \
             ({} chunks, HP {} Lv {}) — pure projections, 0 hidden state",
            final_vault.automap.chunk_count(),
            final_vault.hud.health,
            final_vault.hud.level
        ),
    });

    let pass = checks.iter().all(|c| c.ok);
    ReadModelProbeReport { checks, pass }
}

/// CLI dispatch of PROBE-READMODEL-001 (retail Mode A, M4 scheme):
/// core shared with the tests (anti-screen) + verdict. **0 .BSA reads**
/// (declared synthetic session — delta documented in the M7 note).
pub fn run_readmodel_001() -> i32 {
    println!("PROBE-READMODEL-001 - M7 session read models (PD4 T7, CR-bbd5db82)");
    let report = run_readmodel_probe();
    for check in &report.checks {
        let status = if check.ok { "OK" } else { "FAIL" };
        println!("[{status}] {} - {}", check.id, check.detail);
    }

    if report.pass {
        let vault = session_app_read_models();
        println!("-- Headless evidence (final read models) --");
        println!("M7 VE: HUD {}", vault.hud.summary());
        println!(
            "M7 VE: automap {} chunks\n{}",
            vault.automap.chunk_count(),
            vault.automap.render_ascii()
        );
        println!("M7 VE: panel {}", vault.panel.summary());
        println!(
            "M7 VE: 3 read models projected from ECS at runtime — HUD (health/stamina/level) · \
             automap (chunks) · panel (inventory + quests) · 0 .BSA reads (synthetic session)"
        );
        println!(
            "VERDICT: PROBE-READMODEL-001 PASS - live ECS→read models (ECS change ⇒ read \
             model change) + determinism"
        );
        0
    } else {
        let failed: Vec<&str> = report
            .checks
            .iter()
            .filter(|c| !c.ok)
            .map(|c| c.id)
            .collect();
        println!(
            "VERDICT: PROBE-READMODEL-001 FAIL - failed checks: {}",
            failed.join(", ")
        );
        1
    }
}

/// Full executed session (script + projection) for the dispatch's final
/// evidence — the SAME path as the probe.
fn session_app_read_models() -> SessionReadModels {
    let mut app = session_app();
    app.update();
    apply_session_script(&mut app);
    app.update();
    app.world().resource::<M7ReadModels>().0.clone()
}

#[cfg(test)]
mod tests {
    use super::*;
    use daggerfall_perception::read_models::{AutomapReadModel, MapCell};

    #[test]
    fn readmodel_probe_passes() {
        let report = run_readmodel_probe();
        for check in &report.checks {
            assert!(check.ok, "failed check: {} — {}", check.id, check.detail);
        }
        assert!(report.pass);
    }

    #[test]
    fn tampered_vault_is_detected() {
        let mut app = session_app();
        app.update();
        let stats = app.world().resource::<M7Stats>().0.clone();
        let chunks = app.world().resource::<M7Chunks>().0.clone();
        let assets = app.world().resource::<M7Assets>().0.clone();
        let quests = app.world().resource::<M7Quests>().0.clone();

        // Healthy vault: the 3 checks are ok.
        let vault = app.world().resource::<M7ReadModels>().0.clone();
        let healthy = verify_vault(&vault, &stats, &chunks, &assets, &quests);
        assert!(healthy.iter().all(|c| c.ok));

        // Anti-false-positive: a frozen/tampered vault (read model that does
        // NOT follow the ECS) MUST be detected.
        let mut frozen = vault.clone();
        frozen.hud.health -= 1;
        frozen.automap.cells.pop();
        frozen.panel.gold += 1;
        let tampered = verify_vault(&frozen, &stats, &chunks, &assets, &quests);
        assert!(tampered.iter().all(|c| !c.ok), "tamper not detected: {tampered:?}");
    }

    #[test]
    fn stale_cells_change_the_projection() {
        // A read model with a cell the ECS does not have (ghost chunk)
        // cannot come from this pure projection.
        let chunks = initial_chunks();
        let from_ecs = crate::m7_readmodels::automap_source_of(&chunks);
        let mut stale = from_ecs.clone();
        stale.cells.push(MapCell { x: 9, z: 9 });
        assert_ne!(
            AutomapReadModel::project(&from_ecs),
            AutomapReadModel::project(&stale)
        );
    }
}
