//! M8 — window of the **restored session**: the closed-loop story (INV-L99-4,
//! REQ-L99-041) actually executed before the VE — (1) played session mutated ⇒
//! **saved** to `saves/session.dfs` (project root, tangible location) and (2)
//! **new app** that **reloads** it and shows it: the 5 VE states (position ·
//! stats · inventory · quests · chunks) + the ❤️ clock in the title bar and
//! the automap of the restored chunks as geometry in the client area.
//!
//! It is opened from the `--probe SESSION-001` dispatch after a probe PASS
//! (see `main`): the anti-screen invariant (byte-by-byte roundtrip +
//! anti-false-positives) is guaranteed by the **headless** probe with asserts;
//! this window is the visual evidence (criteria (a)-(c) of the M8 pack,
//! m7_window pattern). The same session code (script, save, load) lives in
//! [`crate::m8_session`] — 0 duplicated logic between probe and window.
//!
//! **Documented delta (M8 note):** like the probe and the M7 window, this
//! window **does not read ARCH3D.BSA** (declared synthetic session): the
//! restored read models do not depend on the retail corpus. Source custody
//! intact (PRE-2 §8 rule 3): Mode A only (PathChecker), ModeState=B out of
//! scope. Composition in `app` (bus-only).

use bevy::prelude::*;

use crate::m7_readmodels::{
    expected_models, M7Assets, M7Chunks, M7Quests, M7ReadModels, M7Stats,
};
use crate::m8_session::{
    default_save_path, load_session, m8_session_app, save_session, M8Clock, M8Position,
};

/// M8 window: saves the played session to `saves/session.dfs`, reloads it in
/// a new app and shows it restored (title bar + automap).
/// After the headless probe PASS, this window provides the visual evidence
/// for criteria (a)-(c) of the M8 pack.
///
/// Source custody (PRE-2 §8 rule 3): Mode A only (PathChecker).
pub fn run_windowed_m8(
    path_checker: &daggerfall_inventory::path_checker::PathChecker,
) -> Result<(), String> {
    // Mode A only (milestone boundary), same as the M2-M7 windows.
    if path_checker.use_custom_universe() {
        return Err("M8 supports Mode A only (ModeState=B out of scope)".into());
    }

    // ── (1) SAVE: played session → saves/session.dfs (real location) ──
    let save_path = default_save_path();
    let mut playing = m8_session_app();
    playing.update();
    crate::m8_session::apply_m8_script(&mut playing);
    playing.update();
    let saved = save_session(playing.world_mut(), &save_path)
        .map_err(|err| format!("M8: could not save the session: {err}"))?;
    let bytes = std::fs::read(&save_path)
        .map_err(|err| format!("M8: the save is not on disk: {err}"))?;
    println!(
        "M8 VE: step 1 — played session (position, stats, inventory, quests, chunks + ❤️ \
         clock) SAVED to {} ({} bytes, DFL-SESSION {})",
        save_path.display(),
        bytes.len(),
        daggerfall_memory::session_store::FORMAT_VERSION,
    );

    // ── (2) RELOAD: new app → load ⇒ restored session visible ───────
    let mut restored = m8_session_app();
    restored.update();
    let loaded = load_session(restored.world_mut(), &save_path)
        .map_err(|err| format!("M8: could not reload the session: {err}"))?;
    let roundtrip_equal = loaded == saved;
    let world = restored.world();
    let vault = {
        let stats = &world.resource::<M7Stats>().0;
        let chunks = &world.resource::<M7Chunks>().0;
        let assets = &world.resource::<M7Assets>().0;
        let quests = &world.resource::<M7Quests>().0;
        expected_models(stats, chunks, assets, quests)
    };
    let clock = world.resource::<M8Clock>().0;
    println!(
        "M8 VE: step 2 — NEW app reloaded the save: RESTORED SESSION visible (identical \
         roundtrip: {roundtrip_equal}) — pos ({}, {}) mm · yaw {} mrad · {}-{:02}-{:02} {} · {}",
        loaded.pos_x_mm,
        loaded.pos_z_mm,
        loaded.yaw_mrad,
        clock.year,
        clock.month,
        clock.day,
        clock.time_string(),
        vault.title_line(),
    );

    let title = format!(
        "daggerfall-bevy-native M8 — RESTORED SESSION from {} · pos ({}, {}) mm · \
         {}-{:02}-{:02} {} · {} (CR-bbd5db82)",
        crate::m8_session::SAVE_FILE_REL,
        loaded.pos_x_mm,
        loaded.pos_z_mm,
        clock.year,
        clock.month,
        clock.day,
        clock.time_string(),
        vault.title_line(),
    );

    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title,
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
        .insert_resource(M8Position {
            x_mm: loaded.pos_x_mm,
            z_mm: loaded.pos_z_mm,
            yaw_mrad: loaded.yaw_mrad,
        })
        .insert_resource(M8Clock(clock))
        .insert_resource(M7Stats(world.resource::<M7Stats>().0.clone()))
        .insert_resource(M7Chunks(world.resource::<M7Chunks>().0.clone()))
        .insert_resource(M7Assets(world.resource::<M7Assets>().0.clone()))
        .insert_resource(M7Quests(world.resource::<M7Quests>().0.clone()))
        .insert_resource(M7ReadModels(vault))
        .add_systems(Startup, m8_spawn_restored_scene)
        .run();
    Ok(())
}

/// M8 scene: top-down camera over the automap + ground plane + the **tiles of
/// the restored chunks** (the geometry IS the reloaded save state — one tile
/// per chunk that the load returned to the ECS).
fn m8_spawn_restored_scene(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    vault: Res<M7ReadModels>,
) {
    // Dark ground plane (map canvas).
    let ground = meshes.add(Plane3d::default().mesh().size(40.0, 40.0));
    let ground_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(0.16, 0.17, 0.20),
        perceptual_roughness: 0.95,
        ..default()
    });
    commands.spawn((Mesh3d(ground), MeshMaterial3d(ground_mat)));

    // Luminous bluish tile (chunks restored from the save).
    let tile_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(0.22, 0.55, 0.90),
        perceptual_roughness: 0.6,
        ..default()
    });
    let tile_mesh = meshes.add(Cuboid::new(0.9, 0.08, 0.9));

    for cell in &vault.0.automap.cells {
        commands.spawn((
            Mesh3d(tile_mesh.clone()),
            MeshMaterial3d(tile_mat.clone()),
            Transform::from_xyz(cell.x as f32, 0.04, cell.z as f32),
        ));
    }

    // Readable top-down camera over the restored chunks (same framing as the
    // M7 window — the restored state occupies x 0..=2, z 0..=1).
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
