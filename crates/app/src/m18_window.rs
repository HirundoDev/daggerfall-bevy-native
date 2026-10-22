//! M18 — window of the **integrated demo** (feature `window`).
//!
//! It mounts [`crate::m18_demo`]'s composition in Bevy (same source as the
//! `DEMO-001` probe, 0 duplicated logic):
//!
//! - **D1** retail `.RMB` city with M12 `base_color_texture` materials
//!   (`split_by_texture` → `scale_uvs` → `image_from_rgba` →
//!   `material_from_texture`); the block's synthetic ground goes untextured
//!   (declared neutral color);
//! - **D2** M13 terrain ring (one mesh per band, unlit);
//! - **D3** M15 NPCs as 8-dir billboards with decoded flat
//!   (`AlphaMode::Mask`) and retail caption rendered with `.FNT` over the 2D
//!   canvas;
//! - **D4** the 4 M17 panels overlaid on a 2D camera (`order: 1`,
//!   `ClearColorConfig::None`), toggled with 1-4;
//! - **D5** live M20 HUD: the [`DemoScene::character`] status line
//!   (`SPELLSWORD - LV 1 - HP 37/37 - STA 110/110 - MAG 75 - RUN 5`) as a
//!   sprite on the 2D camera (top-left, 2× scale). **R** accumulates 109 real
//!   `Running` uses, opens the raise evaluation (`last + 361` min),
//!   re-renders the HUD (`m20_refresh_hud`), replaces the sprite image and
//!   rebuilds `M19Player.body` from `character.player_body()` (0 duplicated
//!   logic).
//!
//! Movement (M19): first person with WASD + mouse-look (M2 input systems,
//! D1) whose displacement is decided by the **M19 engine**
//! `behavior::locomotion::Locomotion::step` over the demo adapter
//! ([`crate::m18_demo::DemoLocomotionWorld`], 0 duplicated logic): retail
//! `.RMB` grid inside the city (INV-L99-5) and M13 sampler outside. Jump
//! (Space), run (Shift), crouch (Ctrl) and sneak (C, toggle) change the speed
//! resolved by the DFU source (`PlayerSpeedChanger`); the title bar shows the
//! engine mode, the breath and the M20 status line. The demo has no water
//! (§6.5): swimming and levitating are measured in `PROBE-LOCOMOTION-001`
//! with its synthetic world.
//!
//! **Custody (PRE-2 §8 rule 3 / INV-L99-6):** Mode A only; retail bytes are
//! read at runtime and 0 assets are packaged.

use bevy::camera::ClearColorConfig;
use bevy::image::ImagePlugin;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;

use daggerfall_behavior::locomotion::{
    max_breath, Dice100, Locomotion, MotionMode, MoveInput, PlayerBody,
};
use daggerfall_parser_bevy_bridge as bridge;
use daggerfall_skills::character::{Skill, SKILL_CHECK_INTERVAL_MINUTES};

use crate::m18_demo::{DemoScene, DemoTexture, CAPTION_RANGE_M, M19_SEED};
use crate::m2_player;

/// M19 player: engine + body + die + sneak toggle.
#[derive(Resource)]
struct M19Player {
    locomotion: Locomotion,
    body: PlayerBody,
    dice: Dice100,
    sneak: bool,
}

/// Composed scene inserted as a resource (the demo builds it once).
#[derive(Resource)]
struct DemoWorld(DemoScene);

/// World NPCs: billboard entity + position in the city frame.
#[derive(Resource)]
struct DemoNpcs(Vec<(Entity, Vec3)>);

/// Overlaid captions (one per NPC) and the visible index.
#[derive(Resource, Default)]
struct DemoCaptions {
    sprites: Vec<Entity>,
    visible: Option<usize>,
}

/// M17 panels on the 2D camera.
#[derive(Resource, Default)]
struct DemoUi {
    sprites: Vec<Entity>,
    current: usize,
}

/// M20 HUD on the 2D camera (sprite replaceable with the R key).
#[derive(Resource)]
struct DemoHud {
    sprite: Option<Entity>,
}

/// M18 integrated demo window.
pub fn run_windowed_m18(
    path_checker: &daggerfall_inventory::path_checker::PathChecker,
) -> Result<(), String> {
    if path_checker.use_custom_universe() {
        return Err("M18 supports Mode A only (ModeState=B is out of scope)".into());
    }
    let scene = crate::m18_demo::compose_demo().map_err(|err| err.to_string())?;
    let city_m = scene.city_m();
    println!(
        "M18 VE: {} · map-pixel ({},{}) · {} buildings · {} retail triangles · {} textures · \
         terrain {}×{} ({} triangles) · {} NPCs · {} UI panels · spawn=({:.1},{:.1},{:.1}) yaw={:.2}",
        scene.city_name,
        scene.map_pixel[0],
        scene.map_pixel[1],
        scene.city.buildings.len(),
        scene.city.triangle_count(),
        scene.textures.len(),
        scene.terrain.dim,
        scene.terrain.dim,
        scene.terrain.triangles,
        scene.npcs.len(),
        scene.panels.len(),
        scene.spawn.eye[0],
        scene.spawn.eye[1],
        scene.spawn.eye[2],
        scene.spawn.yaw,
    );
    println!(
        "M18 VE: terrain world-span x/z {:.1}..{:.1} m · city {:.1} m · WASD+mouse-look · \
         Esc releases the cursor · 1-4 1996 UI panels",
        scene.terrain.inner_neg_m(),
        scene.terrain.outer_m(),
        city_m,
    );

    App::new()
        .add_plugins(
            DefaultPlugins
                .set(ImagePlugin::default_nearest())
                .set(WindowPlugin {
                    primary_window: Some(Window {
                        title: "DAGGERFALL-LIKE — M20 demo (DFU character + HUD)".to_string(),
                        resolution: (1280u32, 720u32).into(),
                        ..default()
                    }),
                    ..default()
                }),
        )
        .insert_resource(ClearColor(Color::srgba(0.40, 0.62, 0.94, 1.0)))
        .insert_resource(GlobalAmbientLight {
            brightness: 320.0,
            ..default()
        })
        .insert_resource(DemoWorld(scene))
        .insert_resource(m2_player::ActionState::default())
        .add_systems(
            Startup,
            (m18_spawn_world, m2_player::m2_setup_cursor, m19_spawn_player),
        )
        .add_systems(
            Update,
            (
                m2_player::m2_gather_action,
                m2_player::m2_escape_cursor,
                m18_apply_player_action,
                m19_update_title,
                m18_billboards,
                m18_captions,
                m18_switch_panel,
                m20_key_use_skill,
            ),
        )
        .run();
    Ok(())
}

/// Builds the scene: textured city + terrain + NPCs + UI + cameras.
fn m18_spawn_world(
    mut commands: Commands,
    world: Res<DemoWorld>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
    windows: Query<&Window, With<PrimaryWindow>>,
) {
    let scene = &world.0;

    // ── D1 · retail city: groups by texture key (M12) ───────────────────
    let ground_material = bridge::material_from_spec(
        &bridge::MaterialSpec {
            base_color: [0.33, 0.31, 0.28, 1.0],
            ..default()
        },
        &mut materials,
    );
    let mut textured_groups = 0usize;
    let mut textured_triangles = 0usize;
    for group in bridge::split_by_texture(&scene.city.combined) {
        let (handle, textured) = if let Some(texture) = scene.textures.get(&group.texture_key) {
            match bridge::image_from_rgba(
                &texture.rgba,
                u32::from(texture.width),
                u32::from(texture.height),
                &mut images,
            ) {
                Ok(image) => {
                    let uvs = bridge::scale_uvs(
                        &group.mesh.uvs,
                        u32::from(texture.width),
                        u32::from(texture.height),
                    );
                    let spec = bridge::MeshSpec {
                        uvs,
                        ..group.mesh.clone()
                    };
                    let mesh = bridge::mesh_from_spec(&spec, &mut meshes);
                    let material = bridge::material_from_texture(
                        &bridge::MaterialSpec {
                            base_color: [1.0, 1.0, 1.0, 1.0],
                            ..default()
                        },
                        image,
                        &mut materials,
                    );
                    (Some((mesh, material)), true)
                }
                Err(_) => (None, false),
            }
        } else {
            (None, false)
        };
        let (mesh, material) = handle.unwrap_or_else(|| {
            (
                bridge::mesh_from_spec(&group.mesh, &mut meshes),
                ground_material.clone(),
            )
        });
        if textured {
            textured_groups += 1;
            textured_triangles += group.mesh.triangle_count();
        }
        commands.spawn((Mesh3d(mesh), MeshMaterial3d(material)));
    }
    println!(
        "M18 VE: city → {} textured groups ({textured_triangles} triangles with retail texture)",
        textured_groups
    );

    // ── D2 · M13 terrain (one unlit mesh per band) ──────────────────────
    for group in &scene.terrain.groups {
        let (color, label) = match group.band {
            daggerfall_parser_rs::woods::TerrainBand::Ocean => {
                ([0.10, 0.22, 0.42, 1.0], "ocean")
            }
            daggerfall_parser_rs::woods::TerrainBand::Beach => {
                ([0.72, 0.66, 0.45, 1.0], "beach")
            }
            daggerfall_parser_rs::woods::TerrainBand::Land => {
                ([0.24, 0.40, 0.20, 1.0], "land")
            }
        };
        let mesh = bridge::mesh_from_spec(&group.spec, &mut meshes);
        let material = bridge::material_from_spec(
            &bridge::MaterialSpec {
                base_color: color,
                unlit: true,
                ..default()
            },
            &mut materials,
        );
        println!(
            "M18 VE: terrain band {label} → {} triangles",
            group.spec.triangle_count()
        );
        commands.spawn((Mesh3d(mesh), MeshMaterial3d(material)));
    }

    // ── D3 · M15 NPCs (8-dir billboard) ────────────────────────────────
    let quad = unit_billboard(&mut meshes);
    let mut npcs = Vec::with_capacity(scene.npcs.len());
    for npc in &scene.npcs {
        let Ok(image) = bridge::image_from_rgba(
            &npc.flat.rgba,
            u32::from(npc.flat.width),
            u32::from(npc.flat.height),
            &mut images,
        ) else {
            continue;
        };
        let material = bridge::material_from_texture(
            &bridge::MaterialSpec {
                base_color: [1.0, 1.0, 1.0, 1.0],
                ..default()
            },
            image,
            &mut materials,
        );
        if let Some(mut mat) = materials.get_mut(&material) {
            mat.alpha_mode = AlphaMode::Mask(0.5);
        }
        let height = 2.2f32;
        let width = height * (f32::from(npc.flat.width) / f32::from(npc.flat.height.max(1)));
        let position = Vec3::from_array(npc.position);
        let entity = commands
            .spawn((
                Mesh3d(quad.clone()),
                MeshMaterial3d(material),
                Transform::from_translation(position)
                    .with_scale(Vec3::new(width, height, 1.0)),
            ))
            .id();
        npcs.push((entity, position));
        println!(
            "M18 VE: NPC {} ({}) at ({:.1},{:.1}) · flat {}.{} · caption \"{}\"",
            npc.spec.name,
            npc.spec.race.name(),
            npc.position[0],
            npc.position[2],
            npc.flat.width,
            npc.flat.height,
            npc.spec.caption.text,
        );
    }
    if npcs.is_empty() {
        eprintln!("M18 VE: 0 NPCs placed (unresolvable flats) — the NPC layer is not shown");
    }

    // ── 3D first-person camera + light ─────────────────────────────────
    commands.spawn((
        Camera3d::default(),
        Transform::from_translation(Vec3::from_array(scene.spawn.eye))
            .with_rotation(Quat::from_rotation_y(scene.spawn.yaw)),
        m2_player::M2Player {
            yaw: scene.spawn.yaw,
            pitch: 0.0,
            radius: bridge::PLAYER_RADIUS_M,
        },
    ));
    commands.spawn((
        DirectionalLight {
            illuminance: 12_000.0,
            ..default()
        },
        Transform::from_xyz(0.0, 200.0, 0.0)
            .with_rotation(Quat::from_euler(EulerRot::XYZ, -1.0, 0.4, 0.0)),
    ));

    // ── D4 · overlaid M17 UI (2D camera over the 3D one) ───────────────
    commands.spawn((
        Camera2d,
        Camera {
            order: 1,
            clear_color: ClearColorConfig::None,
            ..default()
        },
    ));
    let mut panel_sprites = Vec::with_capacity(scene.panels.len());
    for (index, panel) in scene.panels.iter().enumerate() {
        let Ok(image) = bridge::image_from_rgba(
            &panel.rgba,
            u32::from(panel.width),
            u32::from(panel.height),
            &mut images,
        ) else {
            continue;
        };
        let mut entity = commands.spawn((
            Sprite {
                image,
                custom_size: Some(Vec2::new(
                    f32::from(panel.width) * 2.0,
                    f32::from(panel.height) * 2.0,
                )),
                ..default()
            },
            Transform::from_xyz(0.0, 0.0, 0.0),
        ));
        if index != 0 {
            entity.insert(Visibility::Hidden);
        }
        panel_sprites.push(entity.id());
    }
    commands.insert_resource(DemoUi {
        sprites: panel_sprites,
        current: 0,
    });
    println!(
        "M18 VE: 1996 UI → {} overlaid panels (320×200 canvas ×2) · keys 1-4",
        scene.panels.len()
    );

    // ── D5 · live M20 HUD (top-left, 2× scale like the panels) ──────────
    let hud = &scene.m20_hud;
    let hud_sprite = match bridge::image_from_rgba(
        &hud.rgba,
        u32::from(hud.width),
        u32::from(hud.height),
        &mut images,
    ) {
        Ok(image) => Some(
            commands
                .spawn((
                    Sprite {
                        image,
                        custom_size: Some(Vec2::new(
                            f32::from(hud.width) * 2.0,
                            f32::from(hud.height) * 2.0,
                        )),
                        ..default()
                    },
                    m20_hud_transform(windows.single().ok(), hud),
                ))
                .id(),
        ),
        Err(err) => {
            eprintln!("M20 VE: HUD without texture ({err:?}) — the status line is not drawn");
            None
        }
    };
    commands.insert_resource(DemoHud {
        sprite: hud_sprite,
    });
    println!(
        "M20 VE: {} · HUD {}×{} px · {} opaque px · R key = real Running use · digest {:016x}",
        scene.m20_status_line(),
        hud.width,
        hud.height,
        hud.rgba.chunks_exact(4).filter(|px| px[3] != 0).count(),
        scene.m20_hud_digest(),
    );

    // NPC captions (transparent sprites; visible when approaching).
    let mut caption_sprites = Vec::with_capacity(scene.npcs.len());
    for npc in &scene.npcs {
        let Ok(image) = bridge::image_from_rgba(
            &npc.caption.rgba,
            u32::from(npc.caption.width),
            u32::from(npc.caption.height),
            &mut images,
        ) else {
            caption_sprites.push(Entity::PLACEHOLDER);
            continue;
        };
        let entity = commands
            .spawn((
                Sprite {
                    image,
                    custom_size: Some(Vec2::new(
                        f32::from(npc.caption.width) * 2.0,
                        f32::from(npc.caption.height) * 2.0,
                    )),
                    ..default()
                },
                Transform::from_xyz(0.0, -250.0, 10.0),
                Visibility::Hidden,
            ))
            .id();
        caption_sprites.push(entity);
    }
    commands.insert_resource(DemoCaptions {
        sprites: caption_sprites,
        visible: None,
    });
    commands.insert_resource(DemoNpcs(npcs));
}

/// Unit quad in XY (base at y=0, facing +Z) for NPC billboards.
fn unit_billboard(meshes: &mut Assets<Mesh>) -> Handle<Mesh> {
    let spec = bridge::MeshSpec {
        positions: vec![
            [-0.5, 0.0, 0.0],
            [0.5, 0.0, 0.0],
            [0.5, 1.0, 0.0],
            [-0.5, 1.0, 0.0],
        ],
        normals: vec![[0.0, 0.0, 1.0]; 4],
        indices: vec![0, 1, 2, 0, 2, 3],
        uvs: vec![[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]],
        face_textures: vec![bridge::mesh_spec::NO_TEXTURE; 4],
    };
    bridge::mesh_from_spec(&spec, meshes)
}

/// M20 HUD position: top-left of the 2D camera viewport
/// (`ScalingMode::WindowSize` ⇒ 1 unit = 1 logical px) with a 16 px margin.
fn m20_hud_transform(window: Option<&Window>, hud: &DemoTexture) -> Transform {
    let (width, height) = window
        .map(|window| (window.width(), window.height()))
        .unwrap_or((1280.0, 720.0));
    let hud_width = f32::from(hud.width) * 2.0;
    let hud_height = f32::from(hud.height) * 2.0;
    Transform::from_xyz(
        -width / 2.0 + 16.0 + hud_width / 2.0,
        height / 2.0 - 16.0 - hud_height / 2.0,
        100.0,
    )
}

/// **R** key: real `Running` use (109 accumulated) + raise evaluation
/// (`last check + 361` min) over [`DemoScene::character`], with HUD
/// re-render, sprite image replacement and rebuild of the M19 engine
/// `PlayerBody` (0 duplicated logic).
fn m20_key_use_skill(
    keys: Res<ButtonInput<KeyCode>>,
    mut world: ResMut<DemoWorld>,
    mut m19: ResMut<M19Player>,
    hud: Res<DemoHud>,
    mut images: ResMut<Assets<Image>>,
    mut sprites: Query<(&mut Sprite, &mut Transform)>,
    windows: Query<&Window, With<PrimaryWindow>>,
) {
    if !keys.just_pressed(KeyCode::KeyR) {
        return;
    }
    let Some(entity) = hud.sprite else {
        return;
    };
    let scene = &mut world.0;
    let digest_before = scene.m20_hud_digest();
    let running_before = scene.character.live_skill(Skill::Running);
    scene.character.tally_skill(Skill::Running, 109);
    let now_minutes = scene.character.last_skill_check_minutes + SKILL_CHECK_INTERVAL_MINUTES + 1;
    let report = scene.character.raise_skills(now_minutes);
    scene.m20_refresh_hud();
    let running_after = scene.character.live_skill(Skill::Running);
    let digest_after = scene.m20_hud_digest();
    m19.body = scene.character.player_body();

    let Ok((mut sprite, mut transform)) = sprites.get_mut(entity) else {
        return;
    };
    match bridge::image_from_rgba(
        &scene.m20_hud.rgba,
        u32::from(scene.m20_hud.width),
        u32::from(scene.m20_hud.height),
        &mut images,
    ) {
        Ok(image) => {
            sprite.image = image;
            sprite.custom_size = Some(Vec2::new(
                f32::from(scene.m20_hud.width) * 2.0,
                f32::from(scene.m20_hud.height) * 2.0,
            ));
            *transform = m20_hud_transform(windows.single().ok(), &scene.m20_hud);
        }
        Err(err) => eprintln!("M20 VE: HUD could not be uploaded to the GPU: {err:?}"),
    }
    println!(
        "M20 VE: Running {running_before} → {running_after} · HUD digest {digest_before:016x} → {digest_after:016x} · checked={} · raised={:?} · line \"{}\"",
        report.checked,
        report.raised,
        scene.m20_status_line(),
    );
}

/// M19 player at the retail spawn: engine + body + die (once).
fn m19_spawn_player(mut commands: Commands, world: Res<DemoWorld>) {
    let scene = &world.0;
    let locomotion = Locomotion::new(
        scene.spawn.eye,
        bridge::EYE_HEIGHT_M,
        bridge::PLAYER_RADIUS_M,
    );
    println!(
        "M19 VE: DFU locomotion engine active · eye=({:.2},{:.2},{:.2}) yaw={:.2} · radius {:.2} · \
         Space jump · Shift run · Ctrl crouch · C sneak",
        scene.spawn.eye[0],
        scene.spawn.eye[1],
        scene.spawn.eye[2],
        scene.spawn.yaw,
        bridge::PLAYER_RADIUS_M,
    );
    commands.insert_resource(M19Player {
        locomotion,
        // M20 (spec §7.9/§9): engine body from the real character.
        body: scene.character.player_body(),
        dice: Dice100::new(M19_SEED),
        sneak: false,
    });
    println!(
        "M20 VE: PlayerBody from {} · live_speed {} · running {} · jumping {} · stamina {} · line \"{}\"",
        scene.character.career.name,
        scene.character.player_body().live_speed,
        scene.character.player_body().running_skill,
        scene.character.player_body().jumping_skill,
        scene.character.rpg_stats().stamina,
        scene.m20_status_line(),
    );
}

/// First-person movement: the M19 engine decides (retail grid inside the
/// city, M13 sampler outside) through the demo adapter.
fn m18_apply_player_action(
    mut players: Query<(&mut Transform, &mut m2_player::M2Player)>,
    action: Res<m2_player::ActionState>,
    world: Res<DemoWorld>,
    mut m19: ResMut<M19Player>,
    time: Res<Time>,
    mut telemetry: Local<f32>,
) {
    let Ok((mut transform, mut player)) = players.single_mut() else {
        return;
    };
    if action.look_delta != Vec2::ZERO {
        player.yaw -= action.look_delta.x * m2_player::LOOK_SENSITIVITY;
        player.pitch = (player.pitch - action.look_delta.y * m2_player::LOOK_SENSITIVITY)
            .clamp(-m2_player::PITCH_LIMIT_RAD, m2_player::PITCH_LIMIT_RAD);
        transform.rotation = Quat::from_euler(EulerRot::YXZ, player.yaw, player.pitch, 0.0);
    }

    let M19Player {
        locomotion,
        body,
        dice,
        sneak,
    } = &mut *m19;
    if action.sneak_toggle {
        *sneak = !*sneak;
    }
    let input = MoveInput {
        forward: action.move_dir.y,
        strafe: action.move_dir.x,
        yaw: player.yaw,
        jump: action.jump,
        run: action.run,
        crouch: action.crouch,
        sneak: *sneak,
        // M23 (REQ-L99-056): levitation is a magic effect.
        levitating: false,
    };
    let adapter = world.0.locomotion_world();
    let frame = locomotion.step(&input, body, &adapter, dice, time.delta_secs());
    transform.translation = Vec3::from_array(locomotion.position);

    *telemetry += time.delta_secs();
    if *telemetry >= 2.0 {
        *telemetry = 0.0;
        let terrain = &world.0.terrain;
        println!(
            "M19 VE: pos=({:.1},{:.1},{:.1}) yaw={:.2} · mode={} · speed={:.2} m/s · breath={}/{} \
             · terrain band {} · blocked={}",
            locomotion.position[0],
            locomotion.position[1],
            locomotion.position[2],
            player.yaw,
            frame.mode.label(),
            frame.speed,
            locomotion.breath,
            max_breath(body.live_endurance),
            terrain.band_at(locomotion.position[0], locomotion.position[2]).name(),
            frame.blocked,
        );
    }
}

/// Live title bar: M19 engine mode, breath and M20 status line.
fn m19_update_title(
    mut windows: Query<&mut Window, With<PrimaryWindow>>,
    player: Res<M19Player>,
    world: Res<DemoWorld>,
) {
    let mode = player.locomotion.mode;
    let mut title = format!(
        "DAGGERFALL-LIKE — M20 demo · {} · {} · breath {}/{}",
        mode.label(),
        world.0.m20_status_line(),
        player.locomotion.breath,
        max_breath(player.body.live_endurance),
    );
    if mode != MotionMode::Swimming {
        title.push_str(" (no water)");
    }
    for mut window in &mut windows {
        if window.title != title {
            window.title.clone_from(&title);
        }
    }
}

/// NPC billboards: always facing the camera (Y rotation).
fn m18_billboards(
    npcs: Res<DemoNpcs>,
    cameras: Query<&Transform, (With<Camera3d>, Without<Mesh3d>)>,
    mut transforms: Query<&mut Transform, (With<Mesh3d>, Without<Camera3d>)>,
) {
    let Ok(camera) = cameras.single() else {
        return;
    };
    for (entity, position) in &npcs.0 {
        if let Ok(mut transform) = transforms.get_mut(*entity) {
            let dx = camera.translation.x - position.x;
            let dz = camera.translation.z - position.z;
            transform.rotation = Quat::from_rotation_y(dx.atan2(dz));
        }
    }
}

/// Shows the caption of the nearest NPC within [`CAPTION_RANGE_M`].
fn m18_captions(
    npcs: Res<DemoNpcs>,
    mut captions: ResMut<DemoCaptions>,
    cameras: Query<&Transform, With<Camera3d>>,
    mut visibilities: Query<&mut Visibility>,
    world: Res<DemoWorld>,
) {
    let Ok(camera) = cameras.single() else {
        return;
    };
    let mut nearest: Option<(f32, usize)> = None;
    for (index, (_, position)) in npcs.0.iter().enumerate() {
        let distance = ((camera.translation.x - position.x).powi(2)
            + (camera.translation.z - position.z).powi(2))
        .sqrt();
        if distance <= CAPTION_RANGE_M && nearest.is_none_or(|(best, _)| distance < best) {
            nearest = Some((distance, index));
        }
    }
    let wanted = nearest.map(|(_, index)| index);
    if wanted == captions.visible {
        return;
    }
    for (index, entity) in captions.sprites.iter().enumerate() {
        if *entity == Entity::PLACEHOLDER {
            continue;
        }
        if let Ok(mut visibility) = visibilities.get_mut(*entity) {
            *visibility = if Some(index) == wanted {
                Visibility::Visible
            } else {
                Visibility::Hidden
            };
        }
    }
    if let Some((distance, index)) = nearest {
        if let Some(npc) = world.0.npcs.get(index) {
            println!(
                "M18 VE: NPC caption \"{}: {}\" at {:.1} m",
                npc.spec.name, npc.spec.caption.text, distance
            );
        }
    }
    captions.visible = wanted;
}

/// Keys 1-4: toggle the overlaid M17 panel.
fn m18_switch_panel(
    keys: Res<ButtonInput<KeyCode>>,
    mut ui: ResMut<DemoUi>,
    mut visibilities: Query<&mut Visibility>,
) {
    let requested = if keys.just_pressed(KeyCode::Digit1) {
        Some(0)
    } else if keys.just_pressed(KeyCode::Digit2) {
        Some(1)
    } else if keys.just_pressed(KeyCode::Digit3) {
        Some(2)
    } else if keys.just_pressed(KeyCode::Digit4) {
        Some(3)
    } else {
        None
    };
    let Some(index) = requested else {
        return;
    };
    if index >= ui.sprites.len() || index == ui.current {
        return;
    }
    for (at, entity) in ui.sprites.iter().enumerate() {
        if let Ok(mut visibility) = visibilities.get_mut(*entity) {
            *visibility = if at == index {
                Visibility::Visible
            } else {
                Visibility::Hidden
            };
        }
    }
    ui.current = index;
}
