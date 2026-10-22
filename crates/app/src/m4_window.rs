//! M4 — window of the **real retail interior** (`.RDB` from BLOCKS.BSA +
//! `ARCH3D.BSA`, M11) + **clock in the title** (feature `window`, Mode A —
//! PRE-2 §8 rule 3 custody).
//!
//! It is opened from the `--probe INTERIOR-001` dispatch after a probe PASS
//! (see `main`): the anti-screen invariant (INV-L99-5 extended to the interior
//! door) is guaranteed by the **headless** probe with asserts; this window is
//! the visual evidence for criteria (a) navigable interior and (c) time
//! system. The ❤️ [`GameClock`] (owned by `heart` per PRE-3 §3) advances in the
//! window **title** at the scale
//! [`TIME_SCALE_GAME_SECONDS_PER_REAL_SECOND`] (60 → 1 real s = 1 game minute)
//! and is refreshed every [`TITLE_REFRESH_SECS`] (winit `set_title` does not
//! run per frame).
//!
//! **World source (2026-10-20 correction):** the premise "DFON1.BSA absent
//! from disk" was **FALSE** (WU-A · DoD WI-3): that file does not exist in any
//! Daggerfall distribution — interiors are assembled from `.RDB` (`BLOCKS.BSA`)
//! plus `ARCH3D.BSA` models, both on disk. This window stops using
//! `build_shop_mesh` (synthetic M4 interior) and renders the **same
//! deterministic subject as `PROBE-RDB-001`**:
//! [`crate::m11_probe::evaluate_real_interior`] = first record by directory
//! order whose assembled interior is navigable and PASSes the real 12-check
//! probe (SINGLE core shared with `--probe RDB-001` and with block C of
//! PROBE-INTERIOR-001 — 0 duplicated logic). The chosen subject is declared in
//! the VE (record, ranking and digest), so the choice is reproducible.
//!
//! World = [`bridge::retail_world`] of the subject (walls/floor/props **from
//! the `.RDB` record** + surrounding street strip, delta documented in
//! [`bridge::retail_world`]) → [`bridge::CollisionGrid`] (same
//! AB-06/INV-GAME004-2 authority exercised by the probe). Movement reuses the
//! M2 systems (`m2_gather_action` + `m2_apply_player_action` →
//! `resolve_step`): the player walks the interior with the probe's own
//! collision. The window eye is the **probe's verified spawn**
//! (`grid.find_spawn` = check `rdb_spawn_inside_block`: free, grounded and
//! within bounds) and the yaw looks toward the axis with the longest free run
//! among the 8 canonical ones (deterministic); with no spawn available, it
//! falls back to the real door threshold (also verified free by
//! `retail_street_to_door`).
//!
//! Materials: 1 double-sided PBR for the RDB world (record walls/props/floor
//! and street strip) + unlit sky sphere (M3 additive, visible through the
//! door; M1/M2 PBR materials intact). `ClearColor` = sky blue (M3).

use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use daggerfall_parser_bevy_bridge as bridge;
use daggerfall_heart::clock::{GameClock, TIME_SCALE_GAME_SECONDS_PER_REAL_SECOND};

use crate::m2_player;

/// Title refresh (s): the clock advances 60 game seconds per real second;
/// without this throttle the text would change every frame (winit/frame cost).
const TITLE_REFRESH_SECS: f32 = 0.25;

/// M4 clock (❤️ heart): the SAME `GameClock` as block B of the probe.
#[derive(Resource, Debug)]
pub struct M4Clock(pub GameClock);

/// World + spawn of the M4 scene (VE line and title).
#[derive(Resource)]
pub struct M4Scene {
    /// `.RDB` record of the deterministic subject (and its directory slot).
    pub block_name: String,
    /// Real models / lights / flats of the record (subject census).
    pub counts: (usize, usize, usize),
    /// Triangles of the placed `.RDB` mesh (real ARCH3D models).
    pub triangles: usize,
    /// Solid cells of the world (interior + street strip).
    pub solid_cells: usize,
    /// Radius of the sky sphere (envelope).
    pub sky_radius: f32,
    /// Real door: side and lateral range `(from, to)` of the probe sweep.
    pub door: (bridge::Side, f32, f32),
    /// Eye position (inside the real door, verified free).
    pub eye: Vec3,
    /// Initial yaw (rad, toward the interior).
    pub yaw: f32,
    /// Collision/render world: street strip + real `.RDB` mesh.
    pub world: bridge::MeshSpec,
}

/// M4 window: retail `.RDB` interior navigable in first person + clock in the
/// title. After the headless probe PASS (synthetic A + B time + C real), this
/// window provides the visual evidence for (a) interior and (c) hourly cycle
/// over the **real interior** of the deterministic RDB-001 subject.
///
/// Source custody (PRE-2 §8 rule 3): Mode A only (PathChecker) —
/// BLOCKS.BSA/ARCH3D.BSA are read from the user's ARENA2 at runtime (0 retail
/// bytes in git, INV-L99-6); resolution shared with the probe in
/// [`crate::mode_a_arena2_path`].
pub fn run_windowed_m4(
    path_checker: &daggerfall_inventory::path_checker::PathChecker,
) -> Result<(), String> {
    // Mode A only (milestone boundary), same as the M2/M3 windows.
    if path_checker.use_custom_universe() {
        return Err("M4 supports Mode A only (ModeState=B out of scope)".into());
    }

    // Deterministic subject of PROBE-RDB-001 (M11): directory census +
    // selection by directory order + real 12-check probe. Same core as
    // `--probe RDB-001` (0 duplicated logic, 0 build_shop_mesh).
    let eval = crate::m11_probe::evaluate_real_interior()
        .map_err(|reason| format!("RDB-001 subject: {reason}"))?;
    if !eval.report.pass {
        let failed = eval
            .report
            .checks
            .iter()
            .filter(|check| !check.ok)
            .map(|check| check.id)
            .collect::<Vec<_>>()
            .join(", ");
        return Err(format!(
            "subject {} did not PASS the real probe (failed checks: {failed})",
            eval.subject.block_name
        ));
    }
    let (side, gap_from, gap_to) = eval
        .report
        .door
        .ok_or("RDB-001 subject without a real door flanked by walls")?;
    let interior = eval.subject;

    let world = bridge::retail_world(&interior);
    let grid = bridge::CollisionGrid::from_mesh_spec(&world, bridge::DEFAULT_CELL_M);

    // Eye inside the interior: the same point the `retail_street_to_door`
    // check walks and verifies free (0 crossings) — real door +
    // ENTER_DEPTH_M inward, at the probe's eye height.
    let (bounds_min, bounds_max) = interior
        .mesh_bounds()
        .ok_or("RDB interior without geometry (undefined footprint)")?;
    let axis = side.axis();
    let sign = side.inward_sign();
    let lateral = side.lateral_axis();
    let plane = if sign < 0.0 {
        bounds_max[axis]
    } else {
        bounds_min[axis]
    };
    let mut door_eye = [0.0f32; 3];
    door_eye[axis] = plane + sign * bridge::ENTER_DEPTH_M;
    door_eye[lateral] = (gap_from + gap_to) * 0.5;
    door_eye[1] = bridge::EYE_HEIGHT_M;
    if !grid.is_free(door_eye, bridge::PLAYER_RADIUS_M) {
        return Err(format!(
            "door eye ({:.2},{:.2},{:.2}) not free in the interior grid {}",
            door_eye[0], door_eye[1], door_eye[2], interior.block_name
        ));
    }

    // Window eye: the SAME spawn as the probe's generic layer
    // (`grid.find_spawn`, check `rdb_spawn_inside_block`: free + grounded +
    // within bounds) — look from inside the block, not from the door
    // threshold. Fallback: the already verified door eye.
    let (eye, spawn_kind) = match grid.find_spawn(&world, bridge::EYE_HEIGHT_M, bridge::PLAYER_RADIUS_M)
    {
        Some(spawn) => (spawn.eye, "probe spawn (find_spawn)"),
        None => (door_eye, "door threshold (fallback)"),
    };

    // Yaw: direction with the longest free run among the 8 canonical axes of
    // the legibility probe (deterministic; avoids staring at a wall 0.5 m
    // away). Bevy forward = (−sin yaw, 0, −cos yaw).
    let mut best = (f32::MIN, [0.0f32, 0.0, -1.0]);
    for dir in bridge::view_probe::LOOK_DIRS {
        let hit = bridge::first_geometry_hit(&world, eye, [dir[0], 0.0, dir[1]], 1_000.0)
            .unwrap_or(1_000.0);
        if hit > best.0 {
            best = (hit, [dir[0], 0.0, dir[1]]);
        }
    }
    let look = best.1;
    let yaw = (-look[0]).atan2(-look[2]);

    let span = (bounds_max[0] - bounds_min[0])
        .max(bounds_max[2] - bounds_min[2])
        .max(bridge::RDB_SIDE_M);
    let counts = interior.counts();
    let clock = GameClock::classic_default();

    println!(
        "M4 VE: retail interior {} (record #{} of {} by directory order · {} candidates · \
         digest {}) · {} models/{} lights/{} flats · {} RDB triangles · real door side {} \
         lateral [{:.1},{:.1}] m · {} solid cells · eye={} =({:.2},{:.2},{:.2}) · free \
         view {:.1} m · yaw={:.3} rad · clock {}-{:02}-{:02} {} {} · title with clock ({} game \
         s/real s) · WASD+mouse-look (Esc releases cursor)",
        interior.block_name,
        eval.subject_rank + 1,
        eval.census.total,
        eval.candidates,
        interior.digest(),
        counts.0,
        counts.1,
        counts.2,
        interior.mesh.triangle_count(),
        side.label(),
        gap_from,
        gap_to,
        grid.solid_cell_count(),
        spawn_kind,
        eye[0],
        eye[1],
        eye[2],
        best.0,
        yaw,
        clock.year,
        clock.month,
        clock.day,
        clock.time_string(),
        clock.phase_label(),
        TIME_SCALE_GAME_SECONDS_PER_REAL_SECOND,
    );

    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: m4_title(&clock, &interior.block_name),
                resolution: (1280u32, 720u32).into(),
                ..default()
            }),
            ..default()
        }))
        .insert_resource(ClearColor(Color::srgba(0.40, 0.62, 0.94, 1.0)))
        // Readable interior: same fill as M3 (Bevy 0.20: the global resource
        // is GlobalAmbientLight — AmbientLight is a camera component). This
        // window only; M1/M2/M3 intact.
        .insert_resource(GlobalAmbientLight {
            brightness: 400.0,
            ..default()
        })
        .insert_resource(M4Scene {
            block_name: interior.block_name.clone(),
            counts,
            triangles: interior.mesh.triangle_count(),
            solid_cells: grid.solid_cell_count(),
            sky_radius: span * 2.0 + 100.0,
            door: (side, gap_from, gap_to),
            eye: Vec3::from_array(eye),
            yaw,
            world,
        })
        .insert_resource(M4Clock(clock))
        .insert_resource(m2_player::M2Grid(grid))
        .insert_resource(m2_player::ActionState::default())
        .add_systems(Startup, (m4_spawn_scene, m2_player::m2_setup_cursor))
        .add_systems(
            Update,
            (
                m2_player::m2_gather_action,
                m2_player::m2_escape_cursor,
                m2_player::m2_apply_player_action,
                m4_tick_clock,
            ),
        )
        .run();
    Ok(())
}

/// M4 title: real `.RDB` record + clock (raw DFU date `YYYY-MM-DD`,
/// `HH:MM:SS`, day/night phase) — updated with the clock tick.
///
/// **Without** "synthetic"/"DFON1" labels: this window's world is the `.RDB`
/// subject of the probe (correction).
fn m4_title(clock: &GameClock, block_name: &str) -> String {
    format!(
        "daggerfall-bevy-native M4 — retail interior {block_name} (BLOCKS.BSA · .RDB) · \
         {}-{:02}-{:02} {} {} (CR-bbd5db82 · CR-265af36b)",
        clock.year,
        clock.month,
        clock.day,
        clock.time_string(),
        clock.phase_label(),
    )
}

/// M4 clock tick: [`GameClock::raise_seconds`] with integer carry (DFU
/// `RaiseTime` semantics) over the documented scale + title refresh.
fn m4_tick_clock(
    time: Res<Time>,
    scene: Res<M4Scene>,
    mut clock: ResMut<M4Clock>,
    mut windows: Query<&mut Window, With<PrimaryWindow>>,
    mut carry: Local<f64>,
    mut refresh: Local<f32>,
) {
    *carry += f64::from(time.delta_secs()) * (TIME_SCALE_GAME_SECONDS_PER_REAL_SECOND as f64);
    let whole = carry.floor();
    if whole >= 1.0 {
        clock.0.raise_seconds(whole as u64);
        *carry -= whole;
    }

    *refresh += time.delta_secs();
    if *refresh >= TITLE_REFRESH_SECS {
        *refresh = 0.0;
        let title = m4_title(&clock.0, &scene.block_name);
        for mut window in &mut windows {
            window.title = title.clone();
        }
    }
}

/// M4 scene: real `.RDB` world (record walls/floor/props + street strip) +
/// unlit sky sphere + first-person camera inside the interior + intact
/// directional light (M1/M2/M3). 0 synthetic shop geometry.
fn m4_spawn_scene(
    mut commands: Commands,
    scene: Res<M4Scene>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    use bridge::{material_from_spec, mesh_from_spec, MaterialSpec};

    // ── Real RDB world: record walls/props/floor + street strip ────
    let world = mesh_from_spec(&scene.world, &mut meshes);
    let world_mat = material_from_spec(
        &MaterialSpec {
            base_color: [0.78, 0.72, 0.60, 1.0],
            perceptual_roughness: 0.85,
            ..Default::default()
        },
        &mut materials,
    );
    commands.spawn((Mesh3d(world), MeshMaterial3d(world_mat)));

    // ── Sky: unlit enclosing sphere (M3 additive), cull None — visible
    //    through the door; inside it caps the record enclosure.
    let sky_mat = material_from_spec(
        &MaterialSpec {
            base_color: [0.40, 0.62, 0.94, 1.0],
            perceptual_roughness: 1.0,
            metallic: 0.0,
            unlit: true,
        },
        &mut materials,
    );
    commands.spawn((
        Mesh3d(meshes.add(Sphere::new(scene.sky_radius))),
        MeshMaterial3d(sky_mat),
    ));

    // ── First-person camera inside the interior (M2 movement systems) ─
    commands.spawn((
        Camera3d::default(),
        Transform::from_translation(scene.eye)
            .with_rotation(Quat::from_euler(EulerRot::YXZ, scene.yaw, 0.0, 0.0)),
        m2_player::M2Player {
            yaw: scene.yaw,
            pitch: 0.0,
            radius: bridge::PLAYER_RADIUS_M,
        },
    ));

    // ── Directional light (illuminance default = AMBIENT_DAYLIGHT 10,000 lux;
    //    no shadows by default ⇒ the ceiling does not black out the interior)
    commands.spawn((
        DirectionalLight::default(),
        Transform::from_rotation(Quat::from_euler(EulerRot::XYZ, -0.9, 0.7, 0.0)),
    ));

    println!(
        "M4 VE: retail RDB interior window — {} · {} models/{} lights/{} flats · {} triangles \
         · {} solid cells · real door {} lateral [{:.1},{:.1}] m · eye=({:.2},{:.2},{:.2}) \
         yaw={:.3} rad · ❤️ clock in title (continuous tick) · WASD+mouse-look",
        scene.block_name,
        scene.counts.0,
        scene.counts.1,
        scene.counts.2,
        scene.triangles,
        scene.solid_cells,
        scene.door.0.label(),
        scene.door.1,
        scene.door.2,
        scene.eye.x,
        scene.eye.y,
        scene.eye.z,
        scene.yaw,
    );
}
