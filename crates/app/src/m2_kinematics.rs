//! `m2_kinematics` — the M2 input→displacement math, **without Bevy**.
//!
//! The action system (`m2_player::m2_apply_player_action`) used to inline this
//! math, and `m2_player` is `window`-gated: the input path therefore had **0
//! tests** (a check that cannot run is not a check). The displacement is now a
//! pure function of `(translation, yaw, move_dir, grid, radius, dt)` exported
//! from the library target, so the default (headless) test config exercises the
//! same code the window runs — one implementation, 0 duplicated math.
//!
//! DFU parity:
//! - `FrictionMotor.cs:84-86` builds the horizontal move vector from the input
//!   axes (`inputX` = strafe, `inputY` = forward), scales the diagonal by
//!   `0.7071` when both axes are pressed (i.e. clamps the digital vector to
//!   length 1) and rotates it by the player transform
//!   (`myTransform.TransformDirection(...) * playerMotor.Speed`).
//! - `PlayerGroundMotor.cs:86` advances `moveDirection * Time.deltaTime` per
//!   frame through the character controller — the displacement is
//!   `direction * speed * dt`, not a teleport.
//!
//! Authority (AB-06 / INV-GAME004-2): the "may it move?" verdict belongs to
//! [`CollisionGrid::resolve_step`] (pure, parser-derived), never to Bevy.

use daggerfall_parser_bevy_bridge::{CollisionGrid, StepOutcome};

/// Walk speed (m/s) — single source for the demo's locomotion
/// (`m2_player`/`m19_window` consume it from here).
pub const WALK_SPEED_MPS: f32 = 3.5;

/// WASD → `[strafe, forward]`, the input vector [`step_pose`] consumes.
///
/// `y = W − S`, `x = D − A` (the `ActionState.move_dir` convention:
/// `x` = A/D, `y` = W/S). Digital: each axis is `-1`, `0` or `+1`, and a held
/// pair cancels; the diagonal is **not** normalized here — `step_pose` clamps
/// the vector length to 1 (`FrictionMotor.cs:84`), so the 0.7071 scaling stays
/// in one place.
///
/// This lives in the Bevy-free library module because `m2_player` is
/// `window`-gated: the mapping runs in the default (headless) test config,
/// where inverting W/S or A/D fails `wasd_move_dir_is_not_inverted`.
pub fn wasd_move_dir(w: bool, s: bool, d: bool, a: bool) -> [f32; 2] {
    [f32::from(d) - f32::from(a), f32::from(w) - f32::from(s)]
}

/// One movement tick: input + pose → grid verdict.
///
/// - `translation`: current eye position (m).
/// - `yaw`: absolute yaw (rad, rotation around Y). Bevy forward is `-Z`, so
///   yaw 0 looks at `(0,0,-1)`.
/// - `move_dir`: `[strafe, forward]` in `[-1,1]` — the `ActionState` of the
///   action layer (`x` = A/D, `y` = W/S); clamped to length ≤ 1.
/// - `dt`: frame time (s).
///
/// Returns the grid verdict: the resulting position is always free of
/// geometry (no-traversal, INV-L99-5). A blocked full attempt still reports the
/// per-axis slide the grid allowed, and `blocked` names why.
pub fn step_pose(
    translation: [f32; 3],
    yaw: f32,
    move_dir: [f32; 2],
    grid: &CollisionGrid,
    radius: f32,
    dt: f32,
) -> StepOutcome {
    // Diagonal clamp: DFU scales the diagonal by 0.7071 (`FrictionMotor.cs:84`),
    // i.e. the digital vector never exceeds unit length.
    let mut dir = move_dir;
    let len = (dir[0] * dir[0] + dir[1] * dir[1]).sqrt();
    if len > 1.0 {
        dir = [dir[0] / len, dir[1] / len];
    }
    let (sin, cos) = (yaw.sin(), yaw.cos());
    // Bevy forward = -Z; with yaw = 0 → (0,0,-1).
    let forward = [-sin, 0.0, -cos];
    let right = [cos, 0.0, -sin];
    let move_speed = WALK_SPEED_MPS * dt;
    let delta = [
        (forward[0] * dir[1] + right[0] * dir[0]) * move_speed,
        0.0,
        (forward[2] * dir[1] + right[2] * dir[0]) * move_speed,
    ];
    grid.resolve_step(translation, delta, radius)
}

#[cfg(test)]
mod tests {
    use super::*;
    use daggerfall_parser_bevy_bridge::{
        CellStatus, MeshSpec, DEFAULT_CELL_M, EYE_HEIGHT_M, PLAYER_RADIUS_M,
    };

    /// Appends a quad (2 triangles) to a synthetic spec.
    fn push_quad(
        spec: &mut MeshSpec,
        a: [f32; 3],
        b: [f32; 3],
        c: [f32; 3],
        d: [f32; 3],
    ) {
        let base = spec.positions.len() as u32;
        spec.positions.extend([a, b, c, d]);
        spec.normals.extend([[0.0, 1.0, 0.0]; 4]);
        spec.indices
            .extend([base, base + 1, base + 2, base, base + 2, base + 3]);
    }

    /// 8×8×3 m closed room — the same room the `view_probe` tests use
    /// (floor, ceiling and the 4 walls as quads).
    fn room_spec() -> MeshSpec {
        let mut spec = MeshSpec::new(Vec::new(), Vec::new(), Vec::new());
        push_quad(&mut spec, [0.0, 0.0, 0.0], [0.0, 0.0, 8.0], [8.0, 0.0, 8.0], [8.0, 0.0, 0.0]);
        push_quad(&mut spec, [0.0, 3.0, 0.0], [8.0, 3.0, 0.0], [8.0, 3.0, 8.0], [0.0, 3.0, 8.0]);
        push_quad(&mut spec, [0.0, 0.0, 0.0], [0.0, 3.0, 0.0], [0.0, 3.0, 8.0], [0.0, 0.0, 8.0]);
        push_quad(&mut spec, [8.0, 0.0, 0.0], [8.0, 0.0, 8.0], [8.0, 3.0, 8.0], [8.0, 3.0, 0.0]);
        push_quad(&mut spec, [0.0, 0.0, 0.0], [8.0, 0.0, 0.0], [8.0, 3.0, 0.0], [0.0, 3.0, 0.0]);
        push_quad(&mut spec, [0.0, 0.0, 8.0], [0.0, 3.0, 8.0], [8.0, 3.0, 8.0], [8.0, 0.0, 8.0]);
        spec
    }

    /// The `ActionState` of a held W key: `m2_gather_action` does
    /// `if keys.pressed(KeyCode::KeyW) { dir.y += 1.0 }` (`m2_player.rs:118-119`)
    /// and assigns `action.move_dir = dir` (`m2_player.rs:130`), so W is
    /// `[x = 0.0, y = +1.0]` — this test IS the input path, run headless.
    const W_KEY: [f32; 2] = [0.0, 1.0];

    /// Input path: W walks forward at `WALK_SPEED_MPS` and the wall stops it.
    #[test]
    fn w_key_walks_forward_then_stops_at_the_wall() {
        let spec = room_spec();
        let grid = CollisionGrid::from_mesh_spec(&spec, DEFAULT_CELL_M);
        let start = [4.0, EYE_HEIGHT_M, 4.0];
        let dt = 1.0 / 60.0;

        // 30 ticks = 0.5 s → 3.5 m/s × 0.5 s = 1.75 m along -Z (yaw 0).
        let mut pos = start;
        for tick in 0..30 {
            let out = step_pose(pos, 0.0, W_KEY, &grid, PLAYER_RADIUS_M, dt);
            assert!(
                out.moved && out.blocked.is_none(),
                "tick {tick}: the open room must not block the walk: {out:?}"
            );
            pos = out.pos;
        }
        assert!(
            (pos[2] - (start[2] - WALK_SPEED_MPS * 0.5)).abs() < 1e-3,
            "30 ticks must advance exactly 1.75 m along -Z: z={}",
            pos[2]
        );
        assert!((pos[0] - start[0]).abs() < 1e-6, "pure forward, no strafe: {pos:?}");
        assert!((pos[1] - start[1]).abs() < 1e-6, "flat march, fixed eye height: {pos:?}");

        // 120 more ticks (2 s = 7 m attempted) → the south wall (z = 0) stops it.
        let mut last = None;
        for _ in 0..120 {
            let out = step_pose(pos, 0.0, W_KEY, &grid, PLAYER_RADIUS_M, dt);
            pos = out.pos;
            last = Some(out);
        }
        let last = last.expect("120 ticks ran");
        assert!(!last.moved, "the wall must stop the walk: {last:?}");
        assert_eq!(
            last.blocked,
            Some(CellStatus::Solid),
            "the stop reason is the wall's solid cell: {last:?}"
        );
        // The no-traversal guarantee is voxel-granular: the sphere must clear the
        // nearest SOLID CELL, and the wall plane lives inside cell 0
        // (`[0,1) m`), so the walk stops at `radius + cell` = 1.30 m, one tick
        // (0.058 m) from the boundary — not at `radius` from the plane.
        let boundary = PLAYER_RADIUS_M + DEFAULT_CELL_M;
        assert!(
            pos[2] >= boundary - 1e-3,
            "the eye never enters the blocked cell: z={} < {boundary}",
            pos[2]
        );
        assert!(
            pos[2] <= boundary + WALK_SPEED_MPS * dt + 1e-3,
            "it stops within one tick of the boundary: z={} vs {boundary}",
            pos[2]
        );
        assert!(
            (pos[0] - start[0]).abs() < 1e-6 && (pos[1] - start[1]).abs() < 1e-6,
            "blocked along -Z only: no slide on a perpendicular-free delta: {pos:?}"
        );
    }

    /// The yaw mapping is the same one the window applies to the camera:
    /// yaw 0 → -Z, yaw +90° → -X, and a diagonal input is clamped to unit length
    /// (never `√2 ×` speed).
    #[test]
    fn step_pose_follows_yaw_and_clamps_diagonals() {
        let spec = room_spec();
        let grid = CollisionGrid::from_mesh_spec(&spec, DEFAULT_CELL_M);
        let start = [4.0, EYE_HEIGHT_M, 4.0];
        let dt = 1.0 / 60.0;
        let one = WALK_SPEED_MPS * dt;

        let north = step_pose(start, 0.0, W_KEY, &grid, PLAYER_RADIUS_M, dt);
        assert!((north.pos[2] - (start[2] - one)).abs() < 1e-5, "{north:?}");
        assert!((north.pos[0] - start[0]).abs() < 1e-5, "{north:?}");

        // yaw +90° (counter-clockwise seen from +Y) → forward = (-1, 0, 0).
        let west = step_pose(
            start,
            std::f32::consts::FRAC_PI_2,
            W_KEY,
            &grid,
            PLAYER_RADIUS_M,
            dt,
        );
        assert!((west.pos[0] - (start[0] - one)).abs() < 1e-5, "{west:?}");
        assert!((west.pos[2] - start[2]).abs() < 1e-5, "{west:?}");

        // W+D: `[1.0, 1.0]` clamps to length 1 → the same distance, not √2 ×.
        let diagonal = step_pose(start, 0.0, [1.0, 1.0], &grid, PLAYER_RADIUS_M, dt);
        let moved = [
            diagonal.pos[0] - start[0],
            0.0,
            diagonal.pos[2] - start[2],
        ];
        let dist = (moved[0] * moved[0] + moved[2] * moved[2]).sqrt();
        assert!(
            (dist - one).abs() < 1e-5,
            "diagonal speed must equal the walk speed: moved {dist} m vs {one} m"
        );
        assert!(moved[0] > 0.0 && moved[2] < 0.0, "W+D is +X and -Z: {diagonal:?}");

        // No input at all: nothing moves and nothing is reported as blocked.
        let idle = step_pose(start, 0.0, [0.0, 0.0], &grid, PLAYER_RADIUS_M, dt);
        assert!(!idle.moved && idle.blocked.is_none(), "{idle:?}");
        assert_eq!(idle.pos, start);
    }

    /// The WASD → axis mapping of `m2_gather_action`, executed headless.
    ///
    /// Every assertion pins the **sign** of one key, so swapping W with S or A
    /// with D changes a `[0, 1]` into a `[0, -1]` and fails here (declared
    /// falsification: the mapping cannot regress silently as it did when it
    /// lived inside the `window`-gated system).
    #[test]
    fn wasd_move_dir_is_not_inverted() {
        assert_eq!(wasd_move_dir(true, false, false, false), [0.0, 1.0], "W → forward +y");
        assert_eq!(wasd_move_dir(false, true, false, false), [0.0, -1.0], "S → backward -y");
        assert_eq!(wasd_move_dir(false, false, true, false), [1.0, 0.0], "D → strafe +x");
        assert_eq!(wasd_move_dir(false, false, false, true), [-1.0, 0.0], "A → strafe -x");
        assert_eq!(wasd_move_dir(false, false, false, false), [0.0, 0.0], "no key → no move");
        // Opposite keys cancel on their own axis, and only on their own axis.
        assert_eq!(wasd_move_dir(true, true, false, false), [0.0, 0.0], "W+S cancel");
        assert_eq!(wasd_move_dir(false, false, true, true), [0.0, 0.0], "A+D cancel");
        // Diagonals keep the digital ±1s; `step_pose` owns the 0.7071 clamp.
        assert_eq!(wasd_move_dir(true, false, true, false), [1.0, 1.0], "W+D");
        assert_eq!(wasd_move_dir(false, true, false, true), [-1.0, -1.0], "S+A");
        // Full cross-check: flipping the key pair flips the axis, never the other.
        for (w, s) in [(true, false), (false, true)] {
            let dir = wasd_move_dir(w, s, false, false);
            assert_eq!(dir[0], 0.0, "forward/back never leaks into strafe");
        }
        for (d, a) in [(true, false), (false, true)] {
            let dir = wasd_move_dir(false, false, d, a);
            assert_eq!(dir[1], 0.0, "strafe never leaks into forward/back");
        }
    }

    /// The mapping feeds the same `step_pose` the window runs: W moves the eye
    /// along −Z and D along +X (yaw 0), so the composed input path is covered
    /// end to end without Bevy.
    #[test]
    fn wasd_move_dir_drives_step_pose() {
        let spec = room_spec();
        let grid = CollisionGrid::from_mesh_spec(&spec, DEFAULT_CELL_M);
        let start = [4.0, EYE_HEIGHT_M, 4.0];
        let dt = 1.0 / 60.0;
        let one = WALK_SPEED_MPS * dt;

        let forward = step_pose(start, 0.0, wasd_move_dir(true, false, false, false), &grid, PLAYER_RADIUS_M, dt);
        assert!((forward.pos[2] - (start[2] - one)).abs() < 1e-5, "{forward:?}");
        assert!((forward.pos[0] - start[0]).abs() < 1e-5, "{forward:?}");

        let back = step_pose(start, 0.0, wasd_move_dir(false, true, false, false), &grid, PLAYER_RADIUS_M, dt);
        assert!((back.pos[2] - (start[2] + one)).abs() < 1e-5, "{back:?}");

        let right = step_pose(start, 0.0, wasd_move_dir(false, false, true, false), &grid, PLAYER_RADIUS_M, dt);
        assert!((right.pos[0] - (start[0] + one)).abs() < 1e-5, "{right:?}");

        let left = step_pose(start, 0.0, wasd_move_dir(false, false, false, true), &grid, PLAYER_RADIUS_M, dt);
        assert!((left.pos[0] - (start[0] - one)).abs() < 1e-5, "{left:?}");
    }
}
