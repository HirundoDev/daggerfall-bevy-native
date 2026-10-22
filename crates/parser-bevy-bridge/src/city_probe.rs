//! `city_probe` — executable core of **PROBE-CITY-001** (M3).
//!
//! Anti-Pantalla (UBIQUITOUS §A-24): verifies invariants **at runtime with
//! asserts**, not screenshots — walking along a street between two blocks without
//! crossing geometry (INV-L99-5 extended to the city), collision with blocks
//! on both sides of the corridor and legibility from the street. The same core
//! runs in (i) `cargo run -p daggerfall-app -- --probe CITY-001` (CLI,
//! retail evidence) and (ii) `cargo test` (synthetic CI + retail when there is a
//! runtime), and shares `resolve_step`/`CollisionGrid` with the M2/M3 window
//! system — it is not a parallel test disconnected from gameplay.
//!
//! Generic layer: reuses [`crate::run_view_probe`] (M2) over the city's
//! **combined** mesh; specific layer: street checks (spawn on the
//! street, full end-to-end traversal, block collision).

use crate::city_spec::{
    build_city_mesh, street_spawn, CityLayout, CityMesh, CITY_BLOCK_M,
};
use crate::collision_grid::{first_geometry_hit, CollisionGrid, DEFAULT_CELL_M};
use crate::mesh_spec::MeshSpec;
use crate::view_probe::{run_view_probe, ProbeCheck, EYE_HEIGHT_M, PLAYER_RADIUS_M};

/// Probe march step (m) — same grain as PROBE-VIEW-001.
const MARCH_STEP_M: f32 = 0.25;
/// Arrival margin at the street ends (m).
const STREET_END_MARGIN_M: f32 = 2.0;

/// Full PROBE-CITY-001 report (VIEW generic layer + street layer).
#[derive(Debug, Clone)]
pub struct CityProbeReport {
    /// Executed checks, in order (`view_` prefix = M2 generic layer).
    pub checks: Vec<ProbeCheck>,
    /// Global PASS = every check ok.
    pub pass: bool,
}

/// Runs PROBE-CITY-001 over an already built city (INV-L99-5).
///
/// The generic layer [`run_view_probe`] runs over `city.combined` (full
/// mesh: ground + blocks + landmarks); the street layer uses the layout for
/// a deterministic end-to-end march along the internal street `gap` and the
/// collision of the blocks flanking it.
pub fn run_city_probe(city: &CityMesh, layout: &CityLayout, gap: usize) -> CityProbeReport {
    let mut checks: Vec<ProbeCheck> = Vec::with_capacity(9);

    // ── M2 generic layer (literal reuse of run_view_probe) ──────────────
    let generic = run_view_probe(&city.combined);
    for check in &generic.checks {
        checks.push(ProbeCheck {
            id: match check.id {
                "grid_from_layout" => "view_grid_from_layout",
                "grid_layout_coherence" => "view_grid_layout_coherence",
                "spawn_inside_block" => "view_spawn_generic",
                "no_traversal_walk" => "view_no_traversal_walk",
                "legibility_verdict" => "view_legibility_verdict",
                other => other,
            },
            ok: check.ok,
            detail: check.detail.clone(),
        });
    }

    let grid = CollisionGrid::from_mesh_spec(&city.combined, DEFAULT_CELL_M);
    let (street_point, street_name) = street_spawn(layout, gap);
    let eye = [
        street_point[0],
        street_point[1] + EYE_HEIGHT_M,
        street_point[2],
    ];

    // ── Street check-1: spawn on the street (eye free + exact floor) ────
    let eye_free = grid.is_free(eye, PLAYER_RADIUS_M);
    let ground = first_geometry_hit(&city.combined, eye, [0.0, -1.0, 0.0], EYE_HEIGHT_M + 0.01);
    let grounded = ground.is_some_and(|d| d > 0.0 && d <= EYE_HEIGHT_M + 1e-3);
    checks.push(ProbeCheck {
        id: "street_spawn",
        ok: eye_free && grounded,
        detail: format!(
            "{street_name} · eye=({:.2},{:.2},{:.2}) · free_radius_{PLAYER_RADIUS_M}={eye_free} · floor_at_{EYE_HEIGHT_M}m={grounded} (dist={ground:?})",
            eye[0], eye[1], eye[2],
        ),
    });

    // ── Street check-2: full end-to-end traversal without crossing ──────
    //    ±Z march along the street center up to the city ends:
    //    EVERY step must advance (street clear) and NONE may end up
    //    inside geometry (INV-L99-5: resulting pos always free).
    let total = layout.total_m();
    let mut violations = 0usize;
    let mut blocked_steps = 0usize;
    let mut reached_ends = 0usize;
    let mut walk_detail = Vec::new();
    for (label, sign) in [("+Z", 1.0f32), ("-Z", -1.0f32)] {
        let target_z = if sign > 0.0 {
            total - STREET_END_MARGIN_M
        } else {
            STREET_END_MARGIN_M
        };
        let mut pos = eye;
        let mut steps = 0usize;
        loop {
            let delta = [0.0, 0.0, sign * MARCH_STEP_M];
            let outcome = grid.resolve_step(pos, delta, PLAYER_RADIUS_M);
            if !outcome.moved {
                blocked_steps += 1;
                break;
            }
            pos = outcome.pos;
            steps += 1;
            if !grid.is_free(pos, PLAYER_RADIUS_M) {
                violations += 1;
            }
            let done = if sign > 0.0 {
                pos[2] >= target_z
            } else {
                pos[2] <= target_z
            };
            if done {
                reached_ends += 1;
                break;
            }
            if steps > (total / MARCH_STEP_M) as usize + 10 {
                break;
            }
        }
        walk_detail.push(format!("{label}:{steps}p→z={:.1}", pos[2]));
    }
    checks.push(ProbeCheck {
        id: "street_walk_no_traversal",
        ok: violations == 0 && blocked_steps == 0 && reached_ends == 2,
        detail: format!(
            "±Z march along {street_name} steps of {MARCH_STEP_M} m · traversals={violations} (0 req) · blocked_steps={blocked_steps} (0 req · street clear) · ends_reached={reached_ends}/2 · {}",
            walk_detail.join(" · ")
        ),
    });

    // ── Street check-3: solid blocks on both sides of the corridor ──────
    //    Lateral ±X march from the street center toward the center of
    //    each block: MUST hit geometry before arriving (box
    //    or fit-to-lot landmark, with or without setback) and NEVER end up
    //    inside solid (INV-L99-5: no traversal). A street with no blocks
    //    (empty city) fails here.
    let half_gap = layout.street_gap(gap) * 0.5;
    let to_lot_center = half_gap + CITY_BLOCK_M * 0.5;
    let flank_blocked = [-1.0f32, 1.0].iter().all(|&side| {
        let mut pos = eye;
        let mut stopped_before_center = false;
        let mut traversed = false;
        for _ in 0..(to_lot_center / MARCH_STEP_M) as usize {
            let outcome = grid.resolve_step(pos, [side * MARCH_STEP_M, 0.0, 0.0], PLAYER_RADIUS_M);
            if !outcome.moved {
                stopped_before_center = true;
                break;
            }
            pos = outcome.pos;
            if !grid.is_free(pos, PLAYER_RADIUS_M) {
                traversed = true;
                break;
            }
        }
        stopped_before_center && !traversed
    });
    checks.push(ProbeCheck {
        id: "manzana_collision",
        ok: flank_blocked,
        detail: format!(
            "{street_name} · lateral ±X march toward each block center hits before arriving (radius {PLAYER_RADIUS_M} m at eye height · 0 traversals · both blocks block)",
        ),
    });

    // ── Street check-4: legibility from the street (first-person rays) ──
    //    Guarantee: BOTH flanks (±X, the facades that define the street)
    //    hit geometry + ≥3 of the 4 diagonals. The street's 2 axes
    //    (±Z) are expected OPEN (the street runs to the city edge
    //    and the street layer already proved it is clear).
    //
    //    **M3 delta (discovered in the retail runtime** — test
    //    `probe_city_001_runtime`; synthetic tests with full-lot box
    //    blocks do not see it): the previous requirement `hits >= 6`
    //    assumed fully occupied lots. With ARCH3D
    //    fit-to-lot landmarks the height-cap scaling (record 44005: hy=504 m
    //    → scale 28/504≈0.0556 → footprint 28.5 m in a 64 m lot ⇒ setbacks
    //    ≈14 m) opens the lot corners, and a 45° diagonal ray escapes to
    //    the perimeter street without touching geometry (measured: `(-0.707,-0.707)`
    //    → None with flanks 2/2 and hitting). The "legible street"
    //    invariant is facades on both sides, not 4 blind corners — the real
    //    containment of the block is guaranteed by `manzana_collision`
    //    (lateral march, both sides, 0 traversals).
    let max_dist = total * 2.0;
    let mut hits = 0usize;
    let mut flank_hits = 0usize;
    let mut diag_hits = 0usize;
    for dir in crate::view_probe::LOOK_DIRS {
        let hit =
            first_geometry_hit(&city.combined, eye, [dir[0], 0.0, dir[1]], max_dist).is_some();
        if hit {
            hits += 1;
        }
        let flank = dir[1].abs() < 0.5; // ±X against the facades
        let axis = !flank && dir[0].abs() < 0.5; // ±Z along the street
        if flank && hit {
            flank_hits += 1;
        } else if !flank && !axis && hit {
            diag_hits += 1;
        }
    }
    checks.push(ProbeCheck {
        id: "street_legibility",
        ok: flank_hits == 2 && diag_hits >= 3,
        detail: format!(
            "first-person rays from {street_name}: flanks {flank_hits}/2 · diagonals {diag_hits}/4 · ±Z axes open · {hits}/8 with geometry in total (req: facades on BOTH flanks + ≥3/4 diagonals — M3 delta: fit-to-lot landmark corners with height-cap setback, see the check comment)"
        ),
    });

    let pass = checks.iter().all(|c| c.ok);
    CityProbeReport { checks, pass }
}

/// Synthetic cities without retail for CI tests (0 I/O): same code the
/// CLI runs over the city built from ARCH3D.BSA.
pub fn synthetic_city(blocks: usize) -> (CityLayout, CityMesh) {
    let layout = CityLayout::new(blocks);
    let city = build_city_mesh(&layout, &[]);
    (layout, city)
}

/// Builds a city from a list of specs (test use: city built
/// inside the test itself). Returns the city for extra asserts.
pub fn city_from_landmarks(blocks: usize, landmarks: &[MeshSpec]) -> (CityLayout, CityMesh) {
    let layout = CityLayout::new(blocks);
    let city = build_city_mesh(&layout, landmarks);
    (layout, city)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn probe_city_001_passes_on_synthetic_city() {
        let (layout, city) = synthetic_city(3);
        let report = run_city_probe(&city, &layout, 1);
        for check in &report.checks {
            assert!(check.ok, "check {} FAIL: {}", check.id, check.detail);
        }
        assert!(report.pass, "PROBE-CITY-001 must PASS: {report:?}");
        assert_eq!(report.checks.len(), 9, "generic layer (5) + street layer (4)");
    }

    #[test]
    fn probe_city_001_is_deterministic() {
        let (layout, city) = synthetic_city(2);
        let a = run_city_probe(&city, &layout, 1);
        let b = run_city_probe(&city, &layout, 1);
        assert_eq!(a.pass, b.pass);
        assert_eq!(a.checks.len(), b.checks.len());
        assert_eq!(
            a.checks.iter().map(|c| c.detail.as_str()).collect::<Vec<_>>(),
            b.checks.iter().map(|c| c.detail.as_str()).collect::<Vec<_>>(),
            "same input → same report (determinism)"
        );
    }

    #[test]
    fn probe_city_001_passes_with_synthetic_landmark() {
        // Large synthetic landmark (512×100×512 m cube, analogous to the
        // pinned record) — fit-to-lot must leave the street walkable.
        let (s, h) = (256.0f32, 100.0f32);
        let mut positions = Vec::new();
        let mut normals = Vec::new();
        let mut indices = Vec::new();
        let faces: [([f32; 3], [[f32; 3]; 4]); 6] = [
            ([0.0, -1.0, 0.0], [[-s, 0.0, s], [s, 0.0, s], [s, 0.0, -s], [-s, 0.0, -s]]),
            ([0.0, 1.0, 0.0], [[-s, h, -s], [s, h, -s], [s, h, s], [-s, h, s]]),
            ([0.0, 0.0, -1.0], [[s, 0.0, -s], [-s, 0.0, -s], [-s, h, -s], [s, h, -s]]),
            ([0.0, 0.0, 1.0], [[-s, 0.0, s], [s, 0.0, s], [s, h, s], [-s, h, s]]),
            ([-1.0, 0.0, 0.0], [[-s, 0.0, -s], [-s, 0.0, s], [-s, h, s], [-s, h, -s]]),
            ([1.0, 0.0, 0.0], [[s, 0.0, s], [s, 0.0, -s], [s, h, -s], [s, h, s]]),
        ];
        for (normal, quad) in faces {
            let base = positions.len() as u32;
            positions.extend(quad);
            normals.extend([normal; 4]);
            indices.extend([base, base + 1, base + 2, base, base + 2, base + 3]);
        }
        let landmark = MeshSpec {
            positions,
            normals,
            indices,
            uvs: Vec::new(),
            face_textures: Vec::new(),
        };
        let (layout, city) = city_from_landmarks(2, std::slice::from_ref(&landmark));
        let report = run_city_probe(&city, &layout, 1);
        for check in &report.checks {
            assert!(check.ok, "check {} FAIL: {}", check.id, check.detail);
        }
        assert!(report.pass);
    }

    #[test]
    fn probe_city_001_fails_on_empty_city() {
        let layout = CityLayout::new(2);
        let empty = CityMesh {
            ground: MeshSpec {
                positions: vec![],
                normals: vec![],
                indices: vec![],
                uvs: Vec::new(),
                face_textures: Vec::new(),
            },
            blocks: MeshSpec {
                positions: vec![],
                normals: vec![],
                indices: vec![],
                uvs: Vec::new(),
                face_textures: Vec::new(),
            },
            landmarks: MeshSpec {
                positions: vec![],
                normals: vec![],
                indices: vec![],
                uvs: Vec::new(),
                face_textures: Vec::new(),
            },
            combined: MeshSpec {
                positions: vec![],
                normals: vec![],
                indices: vec![],
                uvs: Vec::new(),
                face_textures: Vec::new(),
            },
        };
        let report = run_city_probe(&empty, &layout, 1);
        assert!(!report.pass, "an empty city cannot PASS");
        assert!(
            report.checks.iter().any(|c| !c.ok),
            "at least one check must fail"
        );
    }
}
