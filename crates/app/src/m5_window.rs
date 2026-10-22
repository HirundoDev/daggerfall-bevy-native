//! M5 — window of the **interactive commerce**: the M4 interior with the
//! "Vendel" NPC + **dialogue/menu in the title bar** (visible gold)
//! (feature `window`, Mode A — PRE-2 §8 rule 3 custody).
//!
//! It is opened from the `--probe COMMERCE-001` dispatch after a probe PASS
//! (see `main`): the Anti-Screen invariant (INV-L99-10 — gold deducted +
//! item added) is guaranteed by the **headless** probe with asserts; this
//! window is the visual evidence of criterion (a) NPC+menu. The title bar
//! follows the M4 pattern (❤️ [`GameClock`] clock with
//! [`TIME_SCALE_GAME_SECONDS_PER_REAL_SECOND`], refreshed every
//! [`TITLE_REFRESH_SECS`]) and adds the commerce session state: greeting +
//! `[B]`/`[V]` menu with **gold always visible** + DFU formula prices in
//! parentheses.
//!
//! World = **the same as the probe** (`m5_probe`): M3 city
//! (`CityLayout::new(3)`) with Mode A ARCH3D landmarks + synthetic shop
//! (`build_shop_mesh`) + NPC solid box ([`bridge::shop_merchant`]) →
//! `merchant_world` → `CollisionGrid` (same authority exercised by
//! PROBE-COMMERCE-001). The NPC is drawn with the **same** `MeshSpec` as its
//! collision box (visual↔solid parity, AB-06: 0 non-solid props).
//! Movement reuses the M2 systems (`m2_gather_action` +
//! `m2_apply_player_action` → `resolve_step`); E interaction and B/V commerce
//! consume [`m2_player::ActionState`] (D1 action layer: `ButtonInput` is
//! still read ONLY in `m2_gather_action`).
//!
//! Composition in `app` (the only one touching organs + bridge — bus-only):
//! commerce session = 📦 inventory ([`TradeSession`]) · prices = 🎭 skills
//! ([`calculate_trade_price`]) · geometry/raycast = bridge · clock = ❤️ heart.
//! Documented QW3 minimum scope: **0 persistence**.

use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use daggerfall_heart::clock::{GameClock, TIME_SCALE_GAME_SECONDS_PER_REAL_SECOND};
use daggerfall_inventory::commerce::TradeSession;
use daggerfall_parser_bevy_bridge as bridge;

use crate::m2_player;
use crate::m5_probe::{
    merchant_stock, starting_inventory, dfu_buy_price, dfu_sell_price, PIELES_BASE_VALUE,
};
use bridge::INTERACT_RANGE_M;

/// Title refresh (s) — same M4 pattern (winit `set_title` does not run per
/// frame; the clock advances 60 game seconds per real second).
const TITLE_REFRESH_SECS: f32 = 0.25;

/// M5 clock (❤️ heart): the SAME `GameClock` as block B of the M4 probe
/// (inherited hourly cycle — the M5 title bar keeps it visible).
#[derive(Resource, Debug)]
pub struct M5Clock(pub GameClock);

/// Commerce session of the window (📦 inventory — in-memory state, 0
/// persistence QW3; the SAME `TradeSession` as the probe).
#[derive(Resource)]
pub struct M5Session(pub TradeSession);

/// AABB box of the merchant NPC (bridge — the SAME as the probe; its
/// `spec()` is both visual mesh and grid solids).
#[derive(Resource)]
pub struct M5Merchant(pub bridge::MerchantProp);

/// World + spawn of the M5 scene (VE line and title bar).
#[derive(Resource)]
pub struct M5Scene {
    /// M3 city (3 specs: ground + blocks + landmarks) of the M5 probe.
    pub city: bridge::CityMesh,
    /// Real ARCH3D landmarks in the city (Mode A).
    pub landmark_count: usize,
    /// Triangles of the synthetic shop.
    pub shop_triangles: usize,
    /// Solid cells of the COMMERCE world (M4 world + NPC — probe grid).
    pub solid_cells: usize,
    /// Sky sphere radius (envelope).
    pub sky_radius: f32,
    /// Starting street name (VE / title).
    pub street: String,
    /// Eye position (inside the interior, within interaction range).
    pub eye: Vec3,
    /// Initial yaw (rad) — looking at the NPC chest.
    pub yaw: f32,
    /// Initial pitch (rad) — slight descent toward the chest.
    pub pitch: f32,
}

/// M5 window: shop interior with merchant NPC + dialogue/menu in the title
/// bar (visible gold). After the headless probe PASS (criteria (b)/(c)/(d)),
/// this window provides the visual evidence of criterion (a).
///
/// Source custody (PRE-2 §8 rule 3): Mode A only (PathChecker) — resolution
/// shared with the probe in [`crate::mode_a_arch3d_path`].
pub fn run_windowed_m5(
    path_checker: &daggerfall_inventory::path_checker::PathChecker,
) -> Result<(), String> {
    // Mode A only (milestone frontier), same as the M2/M3/M4 windows.
    if path_checker.use_custom_universe() {
        return Err("M5 supports Mode A only (ModeState=B is out of scope)".into());
    }

    let bsa_path = crate::mode_a_arch3d_path()?;
    let data = std::fs::read(&bsa_path).map_err(|e| format!("reading {bsa_path:?}: {e}"))?;
    let landmarks = crate::convertible_blocks(&data, 6)?;

    // Same world as the M5 probe (m5_probe): 3x3 city + synthetic shop +
    // solid NPC — grid identical to the one PROBE-COMMERCE-001 exercises
    // (window↔probe parity).
    let layout = bridge::CityLayout::new(3);
    let city = bridge::build_city_mesh(&layout, &landmarks);
    let shop = bridge::build_shop_mesh();
    let merchant = bridge::shop_merchant();
    let world = bridge::world_mesh(&city, &shop);
    let world_m5 = bridge::merchant_world(&world, &merchant);
    let grid = bridge::CollisionGrid::from_mesh_spec(&world_m5, bridge::DEFAULT_CELL_M);

    // Spawn inside the interior and within interaction range: first FREE
    // candidate whose chest is obtainable with `interact_target` (the SAME
    // primitive as the probe — if the grid or the raycast changed, the probe
    // would already fail; the fallback is verified, not improvised).
    let chest = merchant.chest();
    let candidates = [
        (
            Vec3::new(3.5, bridge::EYE_HEIGHT_M, 21.0),
            "north aisle — frames the merchant and the counter",
        ),
        (
            Vec3::new(3.0, bridge::EYE_HEIGHT_M, 18.0),
            "door line (verified by the M4 probe)",
        ),
    ];
    let mut spawn: Option<(Vec3, f32, f32, &str)> = None;
    for (eye, label) in candidates {
        let eye_arr = [eye.x, eye.y, eye.z];
        if !grid.is_free(eye_arr, bridge::PLAYER_RADIUS_M) {
            continue;
        }
        let dir = [
            chest[0] - eye.x,
            chest[1] - eye.y,
            chest[2] - eye.z,
        ];
        let dist = (dir[0] * dir[0] + dir[1] * dir[1] + dir[2] * dir[2]).sqrt();
        if dist > bridge::INTERACT_RANGE_M {
            continue;
        }
        let dir = [dir[0] / dist, dir[1] / dist, dir[2] / dist];
        if bridge::interact_target(&grid, eye_arr, dir, &merchant, bridge::INTERACT_RANGE_M)
            .is_none()
        {
            continue;
        }
        // Camera looking at the chest (Bevy: forward = −Z rotated by YXZ).
        let yaw = (-dir[0]).atan2(-dir[2]);
        let pitch = dir[1].asin();
        spawn = Some((eye, yaw, pitch, label));
        break;
    }
    let (eye, yaw, pitch, spawn_label) =
        spawn.ok_or_else(|| "M5: no spawn candidate has an obtainable chest".to_string())?;

    let (_, street_name) = bridge::street_spawn(&layout, 1);
    let clock = GameClock::classic_default();
    let mut session = TradeSession::new(merchant_stock(), starting_inventory());
    // Window state = **post-interaction** (dialogue open), M4 style (the M4
    // camera spawns already INSIDE the interior — evidence of the final
    // state; the street→interior transition is verified by the headless
    // probe). Here: the spawn is verified with an obtainable chest
    // (`interact_target` Some, candidate selection above), so the window
    // shows the [B]/[V] menu with gold already visible. The E transition
    // (close→open) is guaranteed by headless PROBE-COMMERCE-001 (checks
    // B1/B2: dialogue ONLY via valid interaction); synthetic key injection
    // (XTEST) does not reach winit's input pipeline on this display, a delta
    // documented in the M5 note.
    session.open_dialogue();

    println!(
        "M5 VE: {bsa_path:?} ({} bytes) -> {} ARCH3D landmarks · 3x3 city ({} triangles) + \
         synthetic shop ({} triangles) + NPC Vendel (box {:?} {:?} — solid) · {} solid \
         cells · interior {} eye=({:.2},{:.2},{:.2}) yaw={:.3} pitch={:.3} rad · chest obtainable \
         at ≤{INTERACT_RANGE_M} m · session gold={} · clock {}-{:02}-{:02} {} {} · title bar with \
         dialogue/menu [B]/[V] · WASD+mouse-look · E=talk · B=buy · V=sell",
        data.len(),
        landmarks.len(),
        city.triangle_count(),
        shop.triangle_count(),
        merchant.min,
        merchant.size,
        grid.solid_cell_count(),
        spawn_label,
        eye.x,
        eye.y,
        eye.z,
        yaw,
        pitch,
        session.inventory.gold,
        clock.year,
        clock.month,
        clock.day,
        clock.time_string(),
        clock.phase_label(),
    );

    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: m5_title(&clock, &session),
                resolution: (1280u32, 720u32).into(),
                ..default()
            }),
            ..default()
        }))
        .insert_resource(ClearColor(Color::srgba(0.40, 0.62, 0.94, 1.0)))
        // Legible interior: same fill as M4 (Bevy 0.20: the global resource
        // is GlobalAmbientLight). This window only; M1-M4 untouched.
        .insert_resource(GlobalAmbientLight {
            brightness: 400.0,
            ..default()
        })
        .insert_resource(M5Scene {
            city,
            landmark_count: landmarks.len(),
            shop_triangles: shop.triangle_count(),
            solid_cells: grid.solid_cell_count(),
            sky_radius: layout.total_m() * 1.5 + 100.0,
            street: street_name,
            eye,
            yaw,
            pitch,
        })
        .insert_resource(M5Clock(clock))
        .insert_resource(M5Session(session))
        .insert_resource(M5Merchant(merchant))
        .insert_resource(m2_player::M2Grid(grid))
        .insert_resource(m2_player::ActionState::default())
        .add_systems(Startup, (m5_spawn_scene, m2_player::m2_setup_cursor))
        .add_systems(
            Update,
            (
                m2_player::m2_gather_action,
                m2_player::m2_escape_cursor,
                m2_player::m2_apply_player_action,
                m5_trade_action,
                m5_tick_title,
            ),
        )
        .run();
    Ok(())
}

/// M5 title: clock (raw DFU date — M4 pattern) + commerce session state.
/// Closed: greeting not visible, gold visible, E hint.
/// Open: NPC greeting + `[B]`/`[V]` menu with **visible gold** and DFU
/// formula prices in parentheses (recomputed on every refresh —
/// deterministic, 0 state).
fn m5_title(clock: &GameClock, session: &TradeSession) -> String {
    let stamp = format!(
        "{}-{:02}-{:02} {} {}",
        clock.year,
        clock.month,
        clock.day,
        clock.time_string(),
        clock.phase_label(),
    );
    if !session.dialogue_open {
        return format!(
            "daggerfall-bevy-native M5 — Vendel's shop · {stamp} · gold={} · step closer and look \
             at the merchant, press E to talk (CR-bbd5db82)",
            session.inventory.gold,
        );
    }
    let buy_hint = session
        .first_stock()
        .map(|item| {
            let price = dfu_buy_price(item.base_cost);
            format!("{} ({price})", item.name)
        })
        .unwrap_or_else(|| "no stock".to_string());
    let sell_hint = session
        .first_good()
        .map(|(id, qty)| {
            let price = dfu_sell_price(&m5_base_value(id));
            format!("{id}×{qty} ({price})")
        })
        .unwrap_or_else(|| "nothing to sell".to_string());
    format!(
        "daggerfall-bevy-native M5 — Vendel: \"{}\" · gold={} · [B] buy {buy_hint} · [V] sell \
         {sell_hint} · [E] close · {stamp} (CR-bbd5db82)",
        session.greeting(),
        session.inventory.gold,
    )
}

/// Base value of a player good for the sell formula: cataloged goods use
/// their stock `base_cost` (buy → coherent resale); uncataloged ones
/// ("furs", initial inventory) use the declared synthetic value
/// [`PIELES_BASE_VALUE`] (M5 note §N — content of the content layer, not
/// extracted from DFU).
fn m5_base_value(good_id: &str) -> i32 {
    if good_id == "furs" {
        return PIELES_BASE_VALUE;
    }
    merchant_stock()
        .item(good_id)
        .map(|item| item.base_cost)
        .unwrap_or(PIELES_BASE_VALUE)
}

/// E: toggles the dialogue — **only** with the NPC obtainable by raycast
/// (the SAME `interact_target` primitive as the probe: range + first solid =
/// NPC cells; wall/wrong gaze/out of range ⇒ 0 opening).
/// B/V: buy/sell the first item/good of the deterministic menu paying the
/// DFU formula price (skills), same parameters as the probe
/// (SHOP_QUALITY/PLAYER_MERCANTILE/PLAYER_PERSONALITY). Atomic 📦 inventory
/// transitions: on `Err` 0 mutation (gold intact).
fn m5_trade_action(
    action: Res<m2_player::ActionState>,
    players: Query<&Transform, With<m2_player::M2Player>>,
    grid: Res<m2_player::M2Grid>,
    merchant: Res<M5Merchant>,
    mut session: ResMut<M5Session>,
) {
    let Ok(transform) = players.single() else {
        return;
    };
    let session = &mut session.0;

    if action.interact {
        if session.dialogue_open {
            session.close_dialogue();
            println!("M5 VE: dialogue closed (E) · {}", session.menu_line());
        } else {
            let eye = transform.translation;
            let forward = *transform.forward();
            let hit = bridge::interact_target(
                &grid.0,
                [eye.x, eye.y, eye.z],
                [forward.x, forward.y, forward.z],
                &merchant.0,
                bridge::INTERACT_RANGE_M,
            );
            match hit {
                Some(target) => {
                    session.open_dialogue();
                    println!(
                        "M5 VE: dialogue opened (E) — {} · cell {:?} at {:.2} m · {}",
                        session.greeting(),
                        target.cell,
                        target.dist,
                        session.menu_line(),
                    );
                }
                None => println!(
                    "M5 VE: E with no valid target (look at the merchant within ≤{INTERACT_RANGE_M} m, \
                     with no wall in between)"
                ),
            }
        }
    }

    if action.buy && session.dialogue_open {
        // Deterministic menu selection: first stock item (same rule as the
        // title hint and probe check B2).
        if let Some((item_id, base_cost)) = session
            .first_stock()
            .map(|item| (item.id.clone(), item.base_cost))
        {
            let price = dfu_buy_price(base_cost);
            match session.buy(&item_id, price) {
                Ok(paid) => println!(
                    "M5 VE: BUY {item_id} for {paid} gold (DFU formula) · {}",
                    session.menu_line()
                ),
                Err(err) => println!(
                    "M5 VE: buy {item_id} REJECTED ({err:?}) · gold intact={} · {}",
                    session.inventory.gold,
                    session.menu_line()
                ),
            }
        }
    }

    if action.sell && session.dialogue_open {
        // Deterministic menu selection: first owned good.
        if let Some(good_id) = session.first_good().map(|(id, _)| id.clone()) {
            let price = dfu_sell_price(&m5_base_value(&good_id));
            match session.sell(&good_id, price) {
                Ok(earned) => println!(
                    "M5 VE: SELL {good_id} for {earned} gold (DFU formula) · {}",
                    session.menu_line()
                ),
                Err(err) => println!(
                    "M5 VE: sell {good_id} REJECTED ({err:?}) · gold intact={} · {}",
                    session.inventory.gold,
                    session.menu_line()
                ),
            }
        }
    }
}

/// M5 tick: ❤️ heart clock with integer carry (DFU `RaiseTime` semantics,
/// M4 pattern) + title bar refresh (dialogue/menu + gold + clock).
fn m5_tick_title(
    time: Res<Time>,
    mut clock: ResMut<M5Clock>,
    session: Res<M5Session>,
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
        let title = m5_title(&clock.0, &session.0);
        for mut window in &mut windows {
            window.title = title.clone();
        }
    }
}

/// M5 scene: M3 city (3 specs) + synthetic shop (3 specs, rebuilt with
/// `build_shop_mesh`: deterministic 0 I/O) + **NPC box (the SAME `MeshSpec`
/// as its solids — visual↔collision parity)** + unlit sky sphere +
/// first-person camera looking at the chest + intact directional light
/// (M1-M4). No clouds (interior, M4 delta).
fn m5_spawn_scene(
    mut commands: Commands,
    scene: Res<M5Scene>,
    merchant: Res<M5Merchant>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    use bridge::{material_from_spec, mesh_from_spec, MaterialSpec};

    // ── M3 city (the same 3 specs split from the M3 window) ──────────────
    let ground = mesh_from_spec(&scene.city.ground, &mut meshes);
    let ground_mat = material_from_spec(
        &MaterialSpec {
            base_color: [0.24, 0.24, 0.26, 1.0],
            perceptual_roughness: 0.95,
            ..Default::default()
        },
        &mut materials,
    );
    commands.spawn((Mesh3d(ground), MeshMaterial3d(ground_mat)));

    let blocks = mesh_from_spec(&scene.city.blocks, &mut meshes);
    let blocks_mat = material_from_spec(&MaterialSpec::default(), &mut materials);
    commands.spawn((Mesh3d(blocks), MeshMaterial3d(blocks_mat)));

    let landmarks = mesh_from_spec(&scene.city.landmarks, &mut meshes);
    let landmarks_mat = material_from_spec(&MaterialSpec::default(), &mut materials);
    commands.spawn((Mesh3d(landmarks), MeshMaterial3d(landmarks_mat)));

    // ── Synthetic M4 shop (3 specs with their own material each) ──────────
    let shop = bridge::build_shop_mesh();
    let shell = mesh_from_spec(&shop.shell, &mut meshes);
    let shell_mat = material_from_spec(
        &MaterialSpec {
            base_color: [0.82, 0.76, 0.64, 1.0],
            perceptual_roughness: 0.85,
            ..Default::default()
        },
        &mut materials,
    );
    commands.spawn((Mesh3d(shell), MeshMaterial3d(shell_mat)));

    let floor = mesh_from_spec(&shop.floor, &mut meshes);
    let floor_mat = material_from_spec(
        &MaterialSpec {
            base_color: [0.46, 0.33, 0.21, 1.0],
            perceptual_roughness: 0.7,
            ..Default::default()
        },
        &mut materials,
    );
    commands.spawn((Mesh3d(floor), MeshMaterial3d(floor_mat)));

    let fixtures = mesh_from_spec(&shop.fixtures, &mut meshes);
    let fixtures_mat = material_from_spec(
        &MaterialSpec {
            base_color: [0.55, 0.40, 0.24, 1.0],
            perceptual_roughness: 0.6,
            ..Default::default()
        },
        &mut materials,
    );
    commands.spawn((Mesh3d(fixtures), MeshMaterial3d(fixtures_mat)));

    // ── NPC Vendel (M5): the SAME AABB box as its grid solids —
    //    purple tunic, the only material of its family in the scene (AB-06:
    //    real solid box, 0 billboard/sprite).
    let npc = mesh_from_spec(&merchant.0.spec(), &mut meshes);
    let npc_mat = material_from_spec(
        &MaterialSpec {
            base_color: [0.48, 0.22, 0.44, 1.0],
            perceptual_roughness: 0.8,
            ..Default::default()
        },
        &mut materials,
    );
    commands.spawn((Mesh3d(npc), MeshMaterial3d(npc_mat)));

    // ── Sky: unlit envelope sphere, cull None — visible through the door
    //    (M3 additive; no clouds, M4 delta).
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

    // ── First-person camera looking at the NPC chest (M2 systems) ─────────
    commands.spawn((
        Camera3d::default(),
        Transform::from_translation(scene.eye)
            .with_rotation(Quat::from_euler(EulerRot::YXZ, scene.yaw, scene.pitch, 0.0)),
        m2_player::M2Player {
            yaw: scene.yaw,
            pitch: scene.pitch,
            radius: bridge::PLAYER_RADIUS_M,
        },
    ));

    // ── Directional light (default illuminance = AMBIENT_DAYLIGHT; no
    //    shadows by default ⇒ the roof does not black out the interior) ────
    commands.spawn((
        DirectionalLight::default(),
        Transform::from_rotation(Quat::from_euler(EulerRot::XYZ, -0.9, 0.7, 0.0)),
    ));

    println!(
        "M5 VE: commerce window — {} ARCH3D landmarks · city {} triangles + shop {} \
         triangles + NPC Vendel (solid purple box) · {} solid cells · street {} · \
         eye=({:.2},{:.2},{:.2}) yaw={:.3} pitch={:.3} rad · title bar with dialogue/menu [B]/[V] \
         (visible gold) ❤️ clock · WASD+mouse-look (Esc releases the cursor) · E/B/V commerce",
        scene.landmark_count,
        scene.city.triangle_count(),
        scene.shop_triangles,
        scene.solid_cells,
        scene.street,
        scene.eye.x,
        scene.eye.y,
        scene.eye.z,
        scene.yaw,
        scene.pitch,
    );
}
