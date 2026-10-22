//! M2 — action layer + first-person player (`window` feature).
//!
//! **D1 CR57 (action layer):** `ButtonInput`/`MouseMotion` are read ONLY in
//! [`m2_gather_action`] (single input surface); the movement and look systems
//! consume [`ActionState`]. The module contains NO `winit` types
//! (AB-13: `winit` grep = 0 across crates/).
//!
//! **Authority (AB-06 / INV-GAME004-2):** Bevy primitives do NOT decide —
//! `Transform` only applies the verdict of [`CollisionGrid::resolve_step`]
//! (data derived from the parser, pure in the bridge). The decision "can it
//! move?" belongs to the grid, not to Bevy.
//!
//! `resolve_step` is the SAME one exercised by PROBE-VIEW-001 (anti-screen).

use bevy::input::keyboard::KeyCode;
use bevy::input::mouse::MouseMotion;
use bevy::prelude::*;
use bevy::window::{CursorOptions, CursorGrabMode, PrimaryWindow};

use daggerfall_parser_bevy_bridge::{CellStatus, CollisionGrid};

/// Walk speed (m/s).
pub const WALK_SPEED_MPS: f32 = 3.5;
/// Mouse-look sensitivity (rad/px).
pub const LOOK_SENSITIVITY: f32 = 0.0025;
/// Pitch limit (rad, ≈±89°).
pub const PITCH_LIMIT_RAD: f32 = 1.55;
const TELEMETRY_SECS: f32 = 1.0;
const BLOCK_LOG_SECS: f32 = 0.5;

/// Action layer (D1): per-frame input state.
#[derive(Resource, Default, Debug)]
pub struct ActionState {
    /// `x` = strafe (A/D), `y` = forward (W/S).
    pub move_dir: Vec2,
    /// Frame mouse delta (px).
    pub look_delta: Vec2,
    /// M5: `just_pressed(E)` — interact (commerce dialogue).
    pub interact: bool,
    /// M5: `just_pressed(B)` — buy (menu `[B]`).
    pub buy: bool,
    /// M5: `just_pressed(V)` — sell (menu `[V]`).
    pub sell: bool,
    /// M19: `just_pressed(Space)` — jump (or float up).
    pub jump: bool,
    /// M19: `pressed(ShiftL|ShiftR)` — run.
    pub run: bool,
    /// M19: `pressed(CtrlL|CtrlR)` — crouch (or descend).
    pub crouch: bool,
    /// M19: `just_pressed(C)` — sneak toggle.
    pub sneak_toggle: bool,
}

/// First-person player (camera component).
#[derive(Component, Debug)]
pub struct M2Player {
    /// Absolute yaw (rad, rotation around Y).
    pub yaw: f32,
    /// Absolute pitch (rad, rotation around X).
    pub pitch: f32,
    /// Collision radius (m) — identical to the probe's.
    pub radius: f32,
}

/// Collision grid of the loaded block (Bevy resource over a pure bridge type).
#[derive(Resource)]
pub struct M2Grid(pub CollisionGrid);

/// M2 spawn computed at load (eye inside the block + yaw toward geometry).
#[derive(Resource)]
pub struct M2Spawn {
    /// Eye position.
    pub eye: Vec3,
    /// Initial yaw (rad).
    pub yaw: f32,
}

/// Cursor confined + invisible for mouse-look (Esc releases it).
pub fn m2_setup_cursor(mut windows: Query<&mut CursorOptions, With<PrimaryWindow>>) {
    for mut opts in &mut windows {
        opts.grab_mode = CursorGrabMode::Confined;
        opts.visible = false;
    }
}

/// Esc releases the cursor (exit from mouse-look).
pub fn m2_escape_cursor(
    keys: Res<ButtonInput<KeyCode>>,
    mut windows: Query<&mut CursorOptions, With<PrimaryWindow>>,
) {
    if keys.just_pressed(KeyCode::Escape) {
        for mut opts in &mut windows {
            opts.grab_mode = CursorGrabMode::None;
            opts.visible = true;
        }
    }
}

/// Bevy input → [`ActionState`] (single input surface, D1 CR57).
pub fn m2_gather_action(
    keys: Res<ButtonInput<KeyCode>>,
    mut motion: MessageReader<MouseMotion>,
    mut action: ResMut<ActionState>,
) {
    let mut dir = Vec2::ZERO;
    if keys.pressed(KeyCode::KeyW) {
        dir.y += 1.0;
    }
    if keys.pressed(KeyCode::KeyS) {
        dir.y -= 1.0;
    }
    if keys.pressed(KeyCode::KeyD) {
        dir.x += 1.0;
    }
    if keys.pressed(KeyCode::KeyA) {
        dir.x -= 1.0;
    }
    action.move_dir = dir;
    let mut look = Vec2::ZERO;
    for event in motion.read() {
        look += event.delta;
    }
    action.look_delta = look;
    // M5: commerce keys — they follow the D1 CR57 contract:
    // `ButtonInput` is read ONLY here (single input surface); the M5
    // systems consume these flags (0 winit types in the logic).
    action.interact = keys.just_pressed(KeyCode::KeyE);
    action.buy = keys.just_pressed(KeyCode::KeyB);
    action.sell = keys.just_pressed(KeyCode::KeyV);
    // M19: DFU locomotion — same flags, same single input surface
    // (0 `ButtonInput` outside this system).
    action.jump = keys.just_pressed(KeyCode::Space);
    action.run = keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight);
    action.crouch = keys.pressed(KeyCode::ControlLeft) || keys.pressed(KeyCode::ControlRight);
    action.sneak_toggle = keys.just_pressed(KeyCode::KeyC);
}

/// Look (mouse) + movement (WASD) resolved by the grid: first-person camera.
///
/// Horizontal movement at fixed eye height (M2 flat, no gravity/step-up —
/// delta documented in the M2 note); the `resolve_step` result is always free
/// of geometry (INV-L99-5, same function as PROBE-VIEW-001).
pub fn m2_apply_player_action(
    mut players: Query<(&mut Transform, &mut M2Player)>,
    action: Res<ActionState>,
    grid: Res<M2Grid>,
    time: Res<Time>,
    mut telemetry: Local<f32>,
    mut block_log: Local<f32>,
) {
    let Ok((mut transform, mut player)) = players.single_mut() else {
        return;
    };

    if action.look_delta != Vec2::ZERO {
        player.yaw -= action.look_delta.x * LOOK_SENSITIVITY;
        player.pitch = (player.pitch - action.look_delta.y * LOOK_SENSITIVITY)
            .clamp(-PITCH_LIMIT_RAD, PITCH_LIMIT_RAD);
        transform.rotation = Quat::from_euler(EulerRot::YXZ, player.yaw, player.pitch, 0.0);
    }

    if action.move_dir != Vec2::ZERO {
        let dir = action.move_dir.clamp_length_max(1.0);
        // Bevy forward = -Z; with yaw=0 → (0,0,-1).
        let forward = Vec3::new(-player.yaw.sin(), 0.0, -player.yaw.cos());
        let right = Vec3::new(player.yaw.cos(), 0.0, -player.yaw.sin());
        let delta = (forward * dir.y + right * dir.x) * (WALK_SPEED_MPS * time.delta_secs());
        let from = [transform.translation.x, transform.translation.y, transform.translation.z];
        let outcome = grid
            .0
            .resolve_step(from, [delta.x, delta.y, delta.z], player.radius);
        if outcome.moved {
            transform.translation = Vec3::from_array(outcome.pos);
        } else if let Some(status) = outcome.blocked {
            *block_log += time.delta_secs();
            if *block_log >= BLOCK_LOG_SECS {
                *block_log = 0.0;
                let reason = match status {
                    CellStatus::Solid => "geometry",
                    CellStatus::Outside => "bounds",
                    CellStatus::Free => "no_progress",
                };
                println!(
                    "M2 VE: step blocked ({reason}) at ({:.2},{:.2},{:.2})",
                    from[0], from[1], from[2]
                );
            }
        }
    }

    // 1 Hz telemetry: reproducible evidence that input moves/rotates.
    *telemetry += time.delta_secs();
    if *telemetry >= TELEMETRY_SECS {
        *telemetry = 0.0;
        println!(
            "M2 VE: pos=({:.2},{:.2},{:.2}) yaw={:.2} rad pitch={:.2} rad",
            transform.translation.x,
            transform.translation.y,
            transform.translation.z,
            player.yaw,
            player.pitch
        );
    }
}
