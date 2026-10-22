//! M7 — window of the **session read models**: the automap as geometry in
//! the client area (tiles = the chunks of the read model, painted ONLY when
//! the ECS loads them) + the 3 PD4 T7 read models in the title bar (HUD ·
//! automap · panel — REQ-L99-038/039/040).
//!
//! It is opened from the `--probe READMODEL-001` dispatch after a probe PASS
//! (see `main`): the anti-screen invariant (change in ECS ⇒ change in read
//! model + determinism) is guaranteed by the **headless** probe with asserts;
//! this window is the visual evidence (criteria (a)-(c) of the M7 pack,
//! m5_window pattern). The session script is the SAME as the probe's
//! ([`crate::m7_probe::apply_session_script`] step by step): damage → new
//! chunk → DFU purchase → accepted quest → XP — the tiles grow and the title
//! updates live.
//!
//! **Documented delta (M7 note):** like the probe, this window **does not read
//! ARCH3D.BSA** (0 I/O, declared synthetic session): the read models do not
//! depend on the retail corpus. Source custody intact (PRE-2 §8 rule 3):
//! Mode A only (PathChecker), ModeState=B out of scope. Composition in `app`
//! (bus-only — `crate::m7_readmodels`).

use bevy::prelude::*;
use bevy::window::PrimaryWindow;

use crate::m5_probe::{dfu_buy_price, merchant_stock};
use crate::m7_probe::{
    initial_chunks, initial_inventory, initial_quests, initial_stats, M7_EXTRA_CHUNK,
    M7_EXTRA_QUEST_ID, M7_EXTRA_QUEST_TITLE, M7_SCRIPT_DAMAGE, M7_SCRIPT_LEVEL_PROGRESS,
};
use crate::m7_readmodels::{
    m7_project_read_models, M7Assets, M7Chunks, M7Quests, M7ReadModels, M7Stats,
};

/// Title bar refresh (s) — M4/M5 pattern (winit `set_title` does not run
/// per frame).
const TITLE_REFRESH_SECS: f32 = 0.25;
/// Session script cadence (s) — 5 steps in ~5 s of live window.
const STEP_SECS: f32 = 1.0;

/// Automap tile material (resource — created in Startup).
#[derive(Resource)]
struct M7TileMaterial(Handle<StandardMaterial>);

/// Single-tile mesh (resource — Cuboid 0.9×0.08×0.9 m, one cell = one tile
/// with a readable gap between cells).
#[derive(Resource)]
struct M7TileMesh(Handle<Mesh>);

/// Window session script state (step + real accumulator).
#[derive(Resource)]
struct M7Timeline {
    /// Steps already applied (0..=5).
    step: u8,
    /// Real seconds accumulated since the last step.
    clock: f32,
}

/// M7 window: automap with tiles in the client area + the 3 read models in
/// the title bar. After the headless probe PASS, this window provides the
/// visual evidence for criteria (a)-(c) of the M7 pack.
///
/// Source custody (PRE-2 §8 rule 3): Mode A only (PathChecker).
pub fn run_windowed_m7(
    path_checker: &daggerfall_inventory::path_checker::PathChecker,
) -> Result<(), String> {
    // Mode A only (milestone boundary), same as the M2-M5 windows.
    if path_checker.use_custom_universe() {
        return Err("M7 supports Mode A only (ModeState=B out of scope)".into());
    }

    println!(
        "M7 VE: read models window — 0 .BSA reads (declared synthetic session) · initial \
         chunks {:?} + script (damage → chunk {M7_EXTRA_CHUNK:?} → DFU purchase → quest \
         {M7_EXTRA_QUEST_ID} → XP Lv2) · title bar with the 3 read models (PD4 T7)",
        crate::m7_probe::M7_INITIAL_CHUNKS,
    );

    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "daggerfall-bevy-native M7 — session read models (CR-bbd5db82)"
                    .to_string(),
                resolution: (1280u32, 720u32).into(),
                ..default()
            }),
            ..default()
        }))
        .insert_resource(ClearColor(Color::srgba(0.10, 0.11, 0.14, 1.0)))
        .insert_resource(GlobalAmbientLight {
            brightness: 500.0,
            ..default()
        })
        .insert_resource(M7Stats(initial_stats()))
        .insert_resource(M7Chunks(initial_chunks()))
        .insert_resource(M7Assets(initial_inventory()))
        .insert_resource(M7Quests(initial_quests()))
        .init_resource::<M7ReadModels>()
        .insert_resource(M7Timeline {
            step: 0,
            clock: 0.0,
        })
        .add_systems(Startup, m7_spawn_scene)
        .add_systems(
            Update,
            (
                m7_project_read_models,
                m7_sync_automap_tiles,
                m7_session_tick,
                m7_tick_title,
            )
                .chain(),
        )
        .run();
    Ok(())
}

/// M7 scene: top-down camera over the automap + ground plane + tile material
/// (the tiles are spawned by [`m7_sync_automap_tiles`] from the read model —
/// the geometry IS the projection).
fn m7_spawn_scene(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    // Dark ground plane (map canvas).
    let ground = meshes.add(Plane3d::default().mesh().size(40.0, 40.0));
    let ground_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(0.16, 0.17, 0.20),
        perceptual_roughness: 0.95,
        ..default()
    });
    commands.spawn((Mesh3d(ground), MeshMaterial3d(ground_mat)));

    // Luminous bluish tile (loaded chunks).
    let tile_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(0.22, 0.55, 0.90),
        perceptual_roughness: 0.6,
        ..default()
    });
    commands.insert_resource(M7TileMaterial(tile_mat));
    commands.insert_resource(M7TileMesh(meshes.add(Cuboid::new(0.9, 0.08, 0.9))));

    // Readable top-down camera over the initial 2×2 map (frames the growth up
    // to x=2 during the script).
    commands.spawn((
        Camera3d::default(),
        Transform::from_xyz(1.0, 9.0, 7.0).looking_at(Vec3::new(1.0, 0.0, 0.5), Vec3::Y),
    ));

    // Directional light (readable tiles over the dark ground).
    commands.spawn((
        DirectionalLight {
            illuminance: 9_000.0,
            ..default()
        },
        Transform::from_xyz(-4.0, 10.0, 4.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
}

/// Syncs the automap tiles with the **read model** (not with the ECS
/// directly): a tile exists on screen only if the AutomapReadModel says the
/// chunk is loaded — the geometry is the live projection (load a chunk in the
/// ECS ⇒ its tile appears).
fn m7_sync_automap_tiles(
    mut commands: Commands,
    vault: Res<M7ReadModels>,
    tile_mesh: Option<Res<M7TileMesh>>,
    tile_material: Option<Res<M7TileMaterial>>,
    mut spawned: Local<std::collections::HashSet<(i32, i32)>>,
) {
    let (Some(mesh), Some(material)) = (tile_mesh, tile_material) else {
        return;
    };
    for cell in &vault.0.automap.cells {
        if spawned.insert((cell.x, cell.z)) {
            commands.spawn((
                Mesh3d(mesh.0.clone()),
                MeshMaterial3d(material.0.clone()),
                Transform::from_xyz(cell.x as f32, 0.04, cell.z as f32),
            ));
        }
    }
}

/// Window session script (the SAME as the probe, step by step): every
/// STEP_SECS mutates the ECS and the title bar / tiles reflect it live.
fn m7_session_tick(
    time: Res<Time>,
    mut timeline: ResMut<M7Timeline>,
    mut stats: ResMut<M7Stats>,
    mut chunks: ResMut<M7Chunks>,
    mut assets: ResMut<M7Assets>,
    mut quests: ResMut<M7Quests>,
) {
    if timeline.step >= 5 {
        return;
    }
    timeline.clock += time.delta_secs();
    if timeline.clock < STEP_SECS {
        return;
    }
    timeline.clock = 0.0;
    timeline.step += 1;

    match timeline.step {
        1 => {
            stats.0.damage(M7_SCRIPT_DAMAGE);
            println!(
                "M7 VE: step 1 — damage({M7_SCRIPT_DAMAGE}) in ECS → HUD [{}]",
                daggerfall_perception::read_models::HudReadModel::project(
                    &crate::m7_readmodels::hud_source_of(&stats.0)
                )
                .summary()
            );
        }
        2 => {
            chunks.0.load(daggerfall_memory::chunks::ChunkCoord::new(
                M7_EXTRA_CHUNK.0,
                M7_EXTRA_CHUNK.1,
            ));
            println!(
                "M7 VE: step 2 — load({},{}) in ECS → automap {} chunks (tile painted)",
                M7_EXTRA_CHUNK.0,
                M7_EXTRA_CHUNK.1,
                chunks.0.len()
            );
        }
        3 => {
            let mut trade = daggerfall_inventory::commerce::TradeSession::new(
                merchant_stock(),
                assets.0.clone(),
            );
            trade.open_dialogue();
            if let Some((item_id, base_cost)) = trade
                .first_stock()
                .map(|item| (item.id.clone(), item.base_cost))
            {
                let price = dfu_buy_price(base_cost);
                match trade.buy(&item_id, price) {
                    Ok(paid) => {
                        assets.0 = trade.inventory;
                        println!(
                            "M7 VE: step 3 — buy({item_id}, DFU price {paid}) in ECS → panel \
                             [{}]",
                            daggerfall_perception::read_models::SessionPanelReadModel::project(
                                &crate::m7_readmodels::panel_source_of(&assets.0, &quests.0)
                            )
                            .summary()
                        );
                    }
                    Err(err) => println!("M7 VE: step 3 — purchase rejected ({err:?})"),
                }
            }
        }
        4 => {
            quests.0.accept(M7_EXTRA_QUEST_ID, M7_EXTRA_QUEST_TITLE);
            println!(
                "M7 VE: step 4 — accept({M7_EXTRA_QUEST_ID}) in ECS → panel with {} active quests",
                quests.0.len()
            );
        }
        5 => {
            stats.0.gain_level_progress(M7_SCRIPT_LEVEL_PROGRESS);
            println!(
                "M7 VE: step 5 — level_progress({M7_SCRIPT_LEVEL_PROGRESS}) in ECS → HUD level {} (live title bar)",
                stats.0.level
            );
        }
        _ => {}
    }
}

/// Title bar tick (M4/M5 pattern): the 3 vault read models on one line,
/// refreshed every [`TITLE_REFRESH_SECS`].
fn m7_tick_title(
    time: Res<Time>,
    vault: Res<M7ReadModels>,
    mut windows: Query<&mut Window, With<PrimaryWindow>>,
    mut refresh: Local<f32>,
) {
    *refresh += time.delta_secs();
    if *refresh < TITLE_REFRESH_SECS {
        return;
    }
    *refresh = 0.0;
    let title = format!(
        "daggerfall-bevy-native M7 — {} (CR-bbd5db82)",
        vault.0.title_line()
    );
    for mut window in &mut windows {
        window.title = title.clone();
    }
}
