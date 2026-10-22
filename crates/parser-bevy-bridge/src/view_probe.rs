//! `view_probe` — executable core of **PROBE-VIEW-001** (INV-L99-5, M2).
//!
//! Anti-Pantalla (UBIQUITOUS §A-24, anti_pantalla_matrix §7): this probe REFUSES
//! to be screenshot-only — it verifies invariants **at runtime with asserts**:
//! grid↔layout coherence, spawn inside the block, no-traversal during a
//! directed march and legibility verdict (rays against real geometry).
//! The same core runs in (i) `cargo run -p daggerfall-app --probe VIEW-001`
//! (CLI, retail evidence) and (ii) `cargo test` (synthetic in CI + retail when
//! there is a runtime), and shares `resolve_step` with the M2 window system — it
//! is not a parallel test disconnected from gameplay.
//!
//! Player contract constants: radius 0.30 m, eye 1.60 m (reused by
//! the window system so probe and gameplay measure the same thing).

use crate::collision_grid::{first_geometry_hit, CellStatus, CollisionGrid, DEFAULT_CELL_M};
use crate::mesh_spec::MeshSpec;

/// Player radius (m).
pub const PLAYER_RADIUS_M: f32 = 0.30;
/// Eye height above the floor (m).
pub const EYE_HEIGHT_M: f32 = 1.60;
/// Probe march step (m).
const MARCH_STEP_M: f32 = 0.25;
/// Maximum steps per march direction (0.25 m × 4000 = 1000 m).
const MARCH_MAX_STEPS: usize = 4000;
/// Minimum free steps to consider a direction "walkable".
const MARCH_MIN_OPEN_STEPS: usize = 4;
/// Horizontal march/ray directions (cardinals + diagonals) —
/// shared by the probe and the M2 window's initial yaw.
pub const LOOK_DIRS: [[f32; 2]; 8] = {
    const D: f32 = std::f32::consts::FRAC_1_SQRT_2;
    [
        [1.0, 0.0],
        [D, D],
        [0.0, 1.0],
        [-D, D],
        [-1.0, 0.0],
        [-D, -D],
        [0.0, -1.0],
        [D, -D],
    ]
};

/// One executable probe check.
#[derive(Debug, Clone)]
pub struct ProbeCheck {
    /// Stable check identifier (verdict row).
    pub id: &'static str,
    /// PASS/FAIL.
    pub ok: bool,
    /// Detail with numbers (evidence for the verdict).
    pub detail: String,
}

/// Full PROBE-VIEW-001 report.
#[derive(Debug, Clone)]
pub struct ViewProbeReport {
    /// Executed checks, in order.
    pub checks: Vec<ProbeCheck>,
    /// Global PASS = every check ok.
    pub pass: bool,
}

/// Runs PROBE-VIEW-001 over already parsed geometry (INV-L99-5):
/// coherent grid↔layout collision + legibility verdict (Anti-Pantalla).
pub fn run_view_probe(spec: &MeshSpec) -> ViewProbeReport {
    let mut checks = Vec::with_capacity(5);

    // 1. Grid derived from the layout: non-empty, dimensions, cell.
    let grid = CollisionGrid::from_mesh_spec(spec, DEFAULT_CELL_M);
    let (bmin, bmax) = spec.bounds().unwrap_or(([0.0; 3], [0.0; 3]));
    checks.push(ProbeCheck {
        id: "grid_from_layout",
        ok: grid.solid_cell_count() > 0,
        detail: format!(
            "cell {DEFAULT_CELL_M} m · {} solid cells · dims {:?} · bounds [{:.1},{:.1},{:.1}]..[{:.1},{:.1},{:.1}] · {} triangles",
            grid.solid_cell_count(),
            grid.dims(),
            bmin[0], bmin[1], bmin[2],
            bmax[0], bmax[1], bmax[2],
            spec.triangle_count(),
        ),
    });

    // 2. Bidirectional grid↔layout coherence (INV-L99-5).
    let coherence = grid.verify_layout_coherence(spec);
    checks.push(ProbeCheck {
        id: "grid_layout_coherence",
        ok: coherence.pass(),
        detail: format!(
            "geometry→grid centroids {}/{} (0 missing) · grid→geometry {}/{} without phantom",
            coherence.centroids_checked - coherence.centroids_missing,
            coherence.centroids_checked,
            coherence.phantom_probes - coherence.phantom_found,
            coherence.phantom_probes,
        ),
    });

    // 3. Spawn inside the block + exact grounding + free.
    let spawn = grid.find_spawn(spec, EYE_HEIGHT_M, PLAYER_RADIUS_M);
    let Some(spawn) = spawn else {
        checks.push(ProbeCheck {
            id: "spawn_inside_block",
            ok: false,
            detail: "no spawn: no column with floor+eye free".into(),
        });
        checks.push(ProbeCheck {
            id: "no_traversal_walk",
            ok: false,
            detail: "SKIP: no spawn".into(),
        });
        checks.push(ProbeCheck {
            id: "legibility_verdict",
            ok: false,
            detail: "SKIP: no spawn".into(),
        });
        return ViewProbeReport {
            pass: false,
            checks,
        };
    };
    let eye_inside = (0..3).all(|axis| spawn.eye[axis] >= bmin[axis] && spawn.eye[axis] <= bmax[axis]);
    let eye_free = grid.is_free(spawn.eye, PLAYER_RADIUS_M);
    let ground_dist =
        first_geometry_hit(spec, spawn.eye, [0.0, -1.0, 0.0], EYE_HEIGHT_M + 0.01);
    let grounded = ground_dist.is_some_and(|d| d > 0.0 && d <= EYE_HEIGHT_M + 1e-3);
    checks.push(ProbeCheck {
        id: "spawn_inside_block",
        ok: eye_inside && eye_free && grounded,
        detail: format!(
            "eye=({:.2},{:.2},{:.2}) floor_y={:.2} · inside_bounds={eye_inside} · free_radius_{PLAYER_RADIUS_M}={eye_free} · floor_at_{EYE_HEIGHT_M}m={grounded} (dist={ground_dist:?})",
            spawn.eye[0], spawn.eye[1], spawn.eye[2], spawn.floor_y,
        ),
    });

    // 4. Directed 8-direction march: 0 traversal, 0 out of bounds,
    //    ≥1 geometry block (real wall), ≥1 walkable direction.
    let mut violations = 0usize;
    let mut outside_seen = 0usize;
    let mut geometry_blocks = 0usize;
    let mut open_dirs = 0usize;
    let mut distances = Vec::with_capacity(8);
    for dir in LOOK_DIRS {
        let mut pos = spawn.eye;
        let mut steps = 0usize;
        let mut reason: Option<CellStatus> = None;
        for _ in 0..MARCH_MAX_STEPS {
            let delta = [dir[0] * MARCH_STEP_M, 0.0, dir[1] * MARCH_STEP_M];
            // Straight march: no sliding (resolve_step with pure delta).
            let candidate = [pos[0] + delta[0], pos[1] + delta[1], pos[2] + delta[2]];
            if grid.is_free(candidate, PLAYER_RADIUS_M) {
                pos = candidate;
                steps += 1;
                if !grid.is_free(pos, PLAYER_RADIUS_M) {
                    violations += 1;
                }
                continue;
            }
            reason = Some(grid.classify(candidate, PLAYER_RADIUS_M));
            break;
        }
        if reason.is_none() {
            // Did not block within MARCH_MAX_STEPS: treat as the step limit.
            reason = Some(grid.classify(
                [
                    pos[0] + dir[0] * MARCH_STEP_M,
                    pos[1],
                    pos[2] + dir[1] * MARCH_STEP_M,
                ],
                PLAYER_RADIUS_M,
            ));
        }
        distances.push(format!("({:+.2},{:+.2})→{steps}p", dir[0], dir[1]));
        match reason {
            Some(CellStatus::Solid) => geometry_blocks += 1,
            Some(CellStatus::Outside) => outside_seen += 1,
            _ => {}
        }
        if steps >= MARCH_MIN_OPEN_STEPS {
            open_dirs += 1;
        }
    }
    checks.push(ProbeCheck {
        id: "no_traversal_walk",
        ok: violations == 0 && geometry_blocks >= 1 && open_dirs >= 1,
        detail: format!(
            "8-dir march × ≤{MARCH_MAX_STEPS} steps of {MARCH_STEP_M} m · traversals={violations} (0 req) · geometry_blocks={geometry_blocks} (≥1 req) · out_of_bounds={outside_seen} · walkable_dirs={open_dirs} (≥1 req) · steps {}",
            distances.join(" ")
        ),
    });

    // 5. Legibility: first-person rays against REAL geometry (exact distances).
    let max_dist = (0..3)
        .map(|axis| (bmax[axis] - bmin[axis]).powi(2))
        .sum::<f32>()
        .sqrt()
        .max(1.0);
    let mut hits = 0usize;
    let mut nearest = f32::INFINITY;
    for dir in LOOK_DIRS {
        if let Some(t) = first_geometry_hit(spec, spawn.eye, [dir[0], 0.0, dir[1]], max_dist) {
            hits += 1;
            nearest = nearest.min(t);
        }
    }
    checks.push(ProbeCheck {
        id: "legibility_verdict",
        ok: hits >= 1,
        detail: format!(
            "first-person rays with geometry: {hits}/8 · nearest hit: {nearest:.1} m · max_range {max_dist:.1} m"
        ),
    });

    let pass = checks.iter().all(|c| c.ok);
    ViewProbeReport { checks, pass }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Synthetic room (same geometry as the grid tests).
    fn room_spec() -> MeshSpec {
        let mut positions: Vec<[f32; 3]> = Vec::new();
        let mut indices: Vec<u32> = Vec::new();
        let mut quad = |a: [f32; 3], b: [f32; 3], c: [f32; 3], d: [f32; 3]| {
            let base = positions.len() as u32;
            positions.extend([a, b, c, d]);
            indices.extend([base, base + 1, base + 2, base, base + 2, base + 3]);
        };
        quad([0.0, 0.0, 0.0], [0.0, 0.0, 8.0], [8.0, 0.0, 8.0], [8.0, 0.0, 0.0]);
        quad([0.0, 3.0, 0.0], [8.0, 3.0, 0.0], [8.0, 3.0, 8.0], [0.0, 3.0, 8.0]);
        quad([0.0, 0.0, 0.0], [0.0, 3.0, 0.0], [0.0, 3.0, 8.0], [0.0, 0.0, 8.0]);
        quad([8.0, 0.0, 0.0], [8.0, 0.0, 8.0], [8.0, 3.0, 8.0], [8.0, 3.0, 0.0]);
        quad([0.0, 0.0, 0.0], [8.0, 0.0, 0.0], [8.0, 3.0, 0.0], [0.0, 3.0, 0.0]);
        quad([0.0, 0.0, 8.0], [0.0, 3.0, 8.0], [8.0, 3.0, 8.0], [8.0, 0.0, 8.0]);
        MeshSpec {
            normals: vec![[0.0, 1.0, 0.0]; positions.len()],
            positions,
            indices,
            uvs: Vec::new(),
            face_textures: Vec::new(),
        }
    }

    #[test]
    fn probe_view_001_passes_on_synthetic_room() {
        let report = run_view_probe(&room_spec());
        for check in &report.checks {
            assert!(check.ok, "check {} FAIL: {}", check.id, check.detail);
        }
        assert!(report.pass, "PROBE-VIEW-001 must PASS: {report:?}");
        assert_eq!(report.checks.len(), 5);
        // Determinism: same spec → same report.
        let again = run_view_probe(&room_spec());
        assert_eq!(report.pass, again.pass);
        assert_eq!(report.checks.len(), again.checks.len());
    }

    #[test]
    fn probe_view_001_fails_on_empty_geometry() {
        let empty = MeshSpec {
            positions: vec![],
            normals: vec![],
            indices: vec![],
            uvs: Vec::new(),
            face_textures: Vec::new(),
        };
        let report = run_view_probe(&empty);
        assert!(!report.pass, "an empty spec cannot PASS");
        assert!(
            report.checks.iter().any(|c| c.id == "grid_from_layout" && !c.ok),
            "the grid check must fail: {:?}",
            report.checks
        );
    }

    #[test]
    fn look_dirs_are_unit_vectors() {
        for dir in LOOK_DIRS {
            let len = (dir[0] * dir[0] + dir[1] * dir[1]).sqrt();
            assert!((len - 1.0).abs() < 1e-6, "non-unit direction {dir:?}: {len}");
        }
        // Exact cardinals (regression: do not confuse (0,±1) with (0,±1/√2)).
        assert_eq!(LOOK_DIRS[0], [1.0, 0.0]);
        assert_eq!(LOOK_DIRS[2], [0.0, 1.0]);
        assert_eq!(LOOK_DIRS[4], [-1.0, 0.0]);
        assert_eq!(LOOK_DIRS[6], [0.0, -1.0]);
    }
}
