//! M3 — city window: 3D streets + sky + clouds
//! (`window` feature, Mode A — PRE-2 §8 rule 3 custody).
//!
//! It is opened from the `--probe CITY-001` dispatch after a probe PASS
//! (see `main`): the anti-screen invariant (INV-L99-5) is guaranteed by the
//! **headless** probe with asserts; this window is the visual evidence for
//! criteria (a) named streets and (c) sky+clouds. Camera movement reuses the
//! M2 systems (`m2_gather_action` + `m2_apply_player_action` →
//! `CollisionGrid::resolve_step` over the **combined** city mesh: same
//! AB-06/INV-GAME004-2 authority, same function exercised by PROBE-CITY-001).
//!
//! Materials: 3 for the city (design [`bridge::CityMesh`]: ground + blocks +
//! landmarks) + unlit sky sphere with cull None + unlit cloud planes — the
//! `unlit` field of `MaterialSpec` is M3-additive (default false = M1/M2 PBR
//! intact). `ClearColor` = sky blue (an artifact visible only if the renderer
//! clears before drawing the sphere).

use bevy::prelude::*;
use daggerfall_parser_bevy_bridge as bridge;

use crate::m2_player;

/// M3 spawn: eye on the inner street gap 1 + yaw toward the street interior +
/// name (for the window title and the VE line).
#[derive(Resource, Debug)]
pub struct M3Spawn {
    pub eye: Vec3,
    pub yaw: f32,
    pub street: String,
}

/// City loaded for the M3 scene (3-spec mesh + metadata).
#[derive(Resource)]
pub struct M3City {
    pub city: bridge::CityMesh,
    pub landmark_count: usize,
    pub total_m: f32,
}

/// M3 window: 4x4 city (deterministic streets + ARCH3D landmarks fit-to-lot +
/// box blocks) + unlit sky/clouds + first-person camera on the street with the
/// same M2 collision systems.
///
/// Source custody (PRE-2 §8 rule 3): Mode A only (PathChecker) — resolution
/// shared with the probe in [`crate::mode_a_arch3d_path`].
pub fn run_windowed_m3(
    path_checker: &daggerfall_inventory::path_checker::PathChecker,
) -> Result<(), String> {
    // Mode A only (milestone boundary), same as the M2 window.
    if path_checker.use_custom_universe() {
        return Err("M3 supports Mode A only (ModeState=B out of scope)".into());
    }

    let bsa_path = crate::mode_a_arch3d_path()?;
    let data = std::fs::read(&bsa_path).map_err(|e| format!("reading {bsa_path:?}: {e}"))?;
    let landmarks = crate::convertible_blocks(&data, 6)?;

    // 4x4 city (HH-1 followup 5) with landmarks from retail ARCH3D.
    let layout = bridge::CityLayout::new(4);
    let city = bridge::build_city_mesh(&layout, &landmarks);
    let grid = bridge::CollisionGrid::from_mesh_spec(&city.combined, bridge::DEFAULT_CELL_M);

    // Spawn on the inner street gap 1 (halfway along a block row, not at the
    // intersection); yaw π ⇒ Forward(π)=(0,0,+1) looking along the street
    // flanked by façades toward the city interior.
    let (street_pos, street_name) = bridge::street_spawn(&layout, 1);
    let eye = Vec3::new(street_pos[0], bridge::EYE_HEIGHT_M, street_pos[2]);
    let yaw = std::f32::consts::PI;

    println!(
        "M3 VE: {bsa_path:?} ({} bytes) -> {} landmarks · 4x4 city ({} combined triangles) · {} solid cells · street {street_name} · spawn eye=({:.2},{:.2},{:.2}) yaw={yaw:.2} rad · unlit sky+clouds · WASD+mouse-look (Esc releases cursor)",
        data.len(),
        landmarks.len(),
        city.triangle_count(),
        grid.solid_cell_count(),
        eye.x,
        eye.y,
        eye.z,
    );

    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: format!(
                    "daggerfall-bevy-native M3 — 4x4 city · {street_name} (CR-bbd5db82)"
                ),
                resolution: (1280u32, 720u32).into(),
                ..default()
            }),
            ..default()
        }))
        .insert_resource(ClearColor(Color::srgba(0.40, 0.62, 0.94, 1.0)))
        // Bright day: the default ambient (80 cd/m²) leaves the faces in the
        // canyon shade almost black — M3 raises the fill so the visual
        // evidence shows readable façades without touching M1/M2 (this window
        // only; Bevy 0.20: the global resource is GlobalAmbientLight —
        // AmbientLight is a camera component).
        .insert_resource(GlobalAmbientLight {
            brightness: 400.0,
            ..default()
        })
        .insert_resource(M3City {
            city,
            landmark_count: landmarks.len(),
            total_m: layout.total_m(),
        })
        .insert_resource(M3Spawn {
            eye,
            yaw,
            street: street_name,
        })
        .insert_resource(m2_player::M2Grid(grid))
        .insert_resource(m2_player::ActionState::default())
        .add_systems(Startup, (m3_spawn_scene, m2_player::m2_setup_cursor))
        .add_systems(
            Update,
            (
                m2_player::m2_gather_action,
                m2_player::m2_escape_cursor,
                m2_player::m2_apply_player_action,
            ),
        )
        .run();
    Ok(())
}

/// M3 scene: city (ground + blocks + landmarks, 3 materials) + unlit sky
/// sphere + unlit cloud planes + first-person camera on the street +
/// intact directional light (M1/M2).
fn m3_spawn_scene(
    mut commands: Commands,
    m3city: Res<M3City>,
    spawn: Res<M3Spawn>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    use bridge::{material_from_spec, mesh_from_spec, MaterialSpec};

    // Street ground (asphalt) + box blocks + ARCH3D landmarks: the 3 separate
    // specs of the CityMesh design, one material each.
    let ground = mesh_from_spec(&m3city.city.ground, &mut meshes);
    let ground_mat = material_from_spec(
        &MaterialSpec {
            base_color: [0.24, 0.24, 0.26, 1.0],
            perceptual_roughness: 0.95,
            ..Default::default()
        },
        &mut materials,
    );
    commands.spawn((Mesh3d(ground), MeshMaterial3d(ground_mat)));

    let blocks = mesh_from_spec(&m3city.city.blocks, &mut meshes);
    let blocks_mat = material_from_spec(&MaterialSpec::default(), &mut materials);
    commands.spawn((Mesh3d(blocks), MeshMaterial3d(blocks_mat)));

    let landmarks = mesh_from_spec(&m3city.city.landmarks, &mut meshes);
    let landmarks_mat = material_from_spec(&MaterialSpec::default(), &mut materials);
    commands.spawn((Mesh3d(landmarks), MeshMaterial3d(landmarks_mat)));

    // Sky: unlit enclosing sphere (MaterialSpec::unlit M3), no culling
    // (material_from_spec already sets cull_mode: None) — seen from inside.
    let sky_radius = m3city.total_m * 1.5 + 100.0;
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
        Mesh3d(meshes.add(Sphere::new(sky_radius))),
        MeshMaterial3d(sky_mat),
    ));

    // Clouds: white unlit planes (thin horizontal slabs) over the city, at
    // different distances so the upper horizon shows them.
    let cloud_mat = material_from_spec(
        &MaterialSpec {
            base_color: [0.97, 0.97, 1.0, 1.0],
            unlit: true,
            ..Default::default()
        },
        &mut materials,
    );
    let total = m3city.total_m;
    for (fx, fz, half, height) in [
        (0.30, 0.22, 45.0, 58.0),
        (0.62, 0.46, 70.0, 66.0),
        (0.44, 0.76, 55.0, 60.0),
        (0.80, 0.30, 40.0, 72.0),
        (0.18, 0.88, 60.0, 64.0),
    ] {
        commands.spawn((
            Mesh3d(meshes.add(Plane3d::new(Vec3::Y, Vec2::splat(half)))),
            MeshMaterial3d(cloud_mat.clone()),
            Transform::from_xyz(total * fx, height, total * fz),
        ));
    }

    // First-person camera: eye + yaw on the street (M2 movement systems).
    commands.spawn((
        Camera3d::default(),
        Transform::from_translation(spawn.eye)
            .with_rotation(Quat::from_euler(EulerRot::YXZ, spawn.yaw, 0.0, 0.0)),
        m2_player::M2Player {
            yaw: spawn.yaw,
            pitch: 0.0,
            radius: bridge::PLAYER_RADIUS_M,
        },
    ));

    // Directional light (illuminance default = AMBIENT_DAYLIGHT 10,000 lux).
    commands.spawn((
        DirectionalLight::default(),
        Transform::from_rotation(Quat::from_euler(EulerRot::XYZ, -0.9, 0.7, 0.0)),
    ));

    println!(
        "M3 VE: 4x4 city window — {} ARCH3D landmarks · {} triangles · street {} · unlit sky+clouds",
        m3city.landmark_count,
        m3city.city.triangle_count(),
        spawn.street,
    );
}
