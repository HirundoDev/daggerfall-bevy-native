//! `interior_probe` — executable core of **PROBE-INTERIOR-001** (M4).
//!
//! Anti-Pantalla (UBIQUITOUS §A-24): verifies **at runtime with asserts** the
//! street→interior transition **walking through the door** without crossing a wall
//! (INV-L99-5 extended to the interior): street → lane → door opening →
//! interior route, interior region reached, wall solid outside the opening, props
//! solid at eye height and exit through the same door. The same core
//! runs in (i) `cargo run -p daggerfall-app -- --probe INTERIOR-001` (CLI,
//! retail evidence) and (ii) `cargo test` (synthetic CI + retail when there is a
//! runtime), and shares `resolve_step`/`CollisionGrid` with the M4 window —
//! it is not a parallel test disconnected from gameplay.
//!
//! Generic layer: reuses [`crate::run_view_probe`] (M2) over the **combined
//! world** city+shop (without renaming its checks: they are the same 5 ids
//! of PROBE-VIEW-001); specific layer: the 7 door/interior checks.
//!
//! **Source:** block A = deterministic synthetic interior of
//! [`crate::interior_spec`] (reference geometry for the probe, 0 I/O); the
//! city is the intact M3 (0 touching M1-M3). The DoD's **real interior** is
//! block C (`.RDB` + ARCH3D, [`crate::interior_retail`], M11): the premise
//! "DFON1.BSA absent" was false — corrected 2026-10-20 (WI-3).

use crate::city_spec::{build_city_mesh, street_spawn, CityLayout, CityMesh};
use crate::collision_grid::{CellStatus, CollisionGrid, DEFAULT_CELL_M};
use crate::interior_spec::{
    build_shop_mesh, world_mesh, ShopMesh, DOOR_Z_MAX, DOOR_Z_MIN, SHOP_X_MAX,
};
use crate::mesh_spec::MeshSpec;
use crate::view_probe::{run_view_probe, ProbeCheck, EYE_HEIGHT_M, PLAYER_RADIUS_M};

/// Probe march step (m) — same grain as PROBE-VIEW-001/CITY-001.
const MARCH_STEP_M: f32 = 0.25;
/// Step cap per leg (0.25 m × 4000 = 1000 m).
const MARCH_MAX_STEPS: usize = 4000;

/// Full PROBE-INTERIOR-001 report (generic layer + door layer).
#[derive(Debug, Clone)]
pub struct InteriorProbeReport {
    /// Executed checks, in order (the first 5 are the M2 generic layer).
    pub checks: Vec<ProbeCheck>,
    /// Global PASS = every check ok.
    pub pass: bool,
}

/// Single-axis march to `target` or until hitting something (no sliding: the
/// delta is pure per axis, same as the M2/M3 marches). Exported `pub`
/// since M5: PROBE-COMMERCE-001 reuses the same street→door→
/// interior march (0 helper duplication; 0 touching its logic).
#[derive(Debug, Clone)]
pub struct March {
    /// Final position (always `is_free` — INV-L99-5).
    pub pos: [f32; 3],
    /// Steps taken.
    pub steps: usize,
    /// `true` if it reached `target`.
    pub reached: bool,
    /// Blocking reason if it could not advance further.
    pub blocked: Option<CellStatus>,
    /// Resulting positions that are NOT free (must be 0).
    pub violations: usize,
    /// Crossings of the plane `x = SHOP_X_MAX` (door), with their direction.
    pub crossings: Vec<[f32; 3]>,
}

/// Single-axis march (M5: exported for PROBE-COMMERCE-001; see [`March`]).
pub fn march_axis(
    grid: &CollisionGrid,
    from: [f32; 3],
    axis: usize,
    sign: f32,
    target: f32,
) -> March {
    let mut pos = from;
    let mut steps = 0usize;
    let mut violations = 0usize;
    let mut blocked = None;
    let mut reached = false;
    let mut crossings = Vec::new();
    for _ in 0..MARCH_MAX_STEPS {
        let mut delta = [0.0f32; 3];
        delta[axis] = sign * MARCH_STEP_M;
        let outcome = grid.resolve_step(pos, delta, PLAYER_RADIUS_M);
        if !outcome.moved {
            blocked = outcome.blocked.or(Some(CellStatus::Free));
            break;
        }
        let was_inside = pos[0] < SHOP_X_MAX;
        pos = outcome.pos;
        steps += 1;
        if !grid.is_free(pos, PLAYER_RADIUS_M) {
            violations += 1;
        }
        if axis == 0 && (pos[0] < SHOP_X_MAX) != was_inside {
            crossings.push(pos);
        }
        let done = if sign > 0.0 {
            pos[axis] >= target
        } else {
            pos[axis] <= target
        };
        if done {
            reached = true;
            break;
        }
    }
    March {
        pos,
        steps,
        reached,
        blocked,
        violations,
        crossings,
    }
}

/// Runs PROBE-INTERIOR-001 over an already built world (INV-L99-5).
///
/// `gap` is the starting internal street (same contract as
/// [`crate::run_city_probe`]); the shop travels in `shop` (same
/// fixed position in the west perimeter street).
pub fn run_interior_probe(
    city: &CityMesh,
    layout: &CityLayout,
    shop: &ShopMesh,
    gap: usize,
) -> InteriorProbeReport {
    let mut checks: Vec<ProbeCheck> = Vec::with_capacity(12);

    // ── M2 generic layer (literal reuse of run_view_probe over the combined
    //    world — original ids, 0 renaming) ───────────────────────────────
    let world = world_mesh(city, shop);
    for check in run_view_probe(&world).checks {
        checks.push(check);
    }

    let grid = CollisionGrid::from_mesh_spec(&world, DEFAULT_CELL_M);
    let (street_point, street_name) = street_spawn(layout, gap);
    let eye = [
        street_point[0],
        street_point[1] + EYE_HEIGHT_M,
        street_point[2],
    ];

    // Effective door opening in cells (1 m grain): the opening's header
    // layer is free only between the last cell of the south span and the
    // first of the north span → [17,19) m with DOOR_Z_MIN=16.8/DOOR_Z_MAX=19.2.
    let cell = DEFAULT_CELL_M;
    let door_min = ((DOOR_Z_MIN / cell).floor() + 1.0) * cell;
    let door_max = ((DOOR_Z_MAX / cell).floor()) * cell;
    let z_ok = |z: f32| z - PLAYER_RADIUS_M >= door_min && z + PLAYER_RADIUS_M <= door_max;

    // ── Street → lane → door route ───────────────────────────────────────
    //    R1 -Z to the south perimeter · R2 -X to the west lane (x=7) ·
    //    R3 +Z up to z=13 in the lane (under the door).
    let r1 = march_axis(&grid, eye, 2, -1.0, 6.0);
    let r2 = march_axis(&grid, r1.pos, 0, -1.0, 7.0);
    let r3 = march_axis(&grid, r2.pos, 2, 1.0, 13.0);
    let street_violations = r1.violations + r2.violations + r3.violations;
    checks.push(ProbeCheck {
        id: "street_to_door_walk",
        ok: street_violations == 0 && r1.reached && r2.reached && r3.reached,
        detail: format!(
            "{street_name} → west lane x=7 → z=13 (steps of {MARCH_STEP_M} m) · \
             R1 -Z {steps_a}p → z={za:.1} · R2 -X {steps_b}p → x={xb:.1} · R3 +Z {steps_c}p → z={zc:.1} · \
             traversals={street_violations} (0 req) · targets reached {reached}/3 · eye=({:.2},{:.2},{:.2})",
            eye[0], eye[1], eye[2],
            steps_a = r1.steps,
            za = r1.pos[2],
            steps_b = r2.steps,
            xb = r2.pos[0],
            steps_c = r3.steps,
            zc = r3.pos[2],
            reached = [r1.reached, r2.reached, r3.reached].iter().filter(|r| **r).count(),
        ),
    });

    // ── Wall WITHOUT a door: entry attempt at z=13 → must block ─────────
    let r4 = march_axis(&grid, r3.pos, 0, -1.0, SHOP_X_MAX);
    checks.push(ProbeCheck {
        id: "wall_blocks_without_door",
        ok: r4.blocked == Some(CellStatus::Solid)
            && !r4.reached
            && !shop.contains_interior(r4.pos)
            && r4.violations == 0,
        detail: format!(
            "-X march from the lane at z=13.0 toward the interior: blocked at x={:.2} \
             (reason {:?}) · entered={entrada} · traversals={} (0 req)",
            r4.pos[0],
            r4.blocked,
            r4.violations,
            entrada = shop.contains_interior(r4.pos),
        ),
    });

    // ── R4/R5: +Z to the door line (z=18) and -X crossing through the opening ──
    let r5 = march_axis(&grid, r4.pos, 2, 1.0, 18.0);
    let r6 = march_axis(&grid, r5.pos, 0, -1.0, 3.0);
    checks.push(ProbeCheck {
        id: "interior_reached",
        ok: r5.reached
            && r6.reached
            && r6.violations == 0
            && shop.contains_interior(r6.pos)
            && grid.is_free(r6.pos, PLAYER_RADIUS_M),
        detail: format!(
            "R4 +Z {steps_a}p → z={za:.1} · R5 -X through the door {steps_b}p → x={xb:.2} · \
             region=interior={inside} · free={free} · traversals={} (0 req) · pos=({:.2},{:.2},{:.2})",
            r6.violations,
            r6.pos[0],
            r6.pos[1],
            r6.pos[2],
            steps_a = r5.steps,
            za = r5.pos[2],
            steps_b = r6.steps,
            xb = r6.pos[0],
            inside = shop.contains_interior(r6.pos),
            free = grid.is_free(r6.pos, PLAYER_RADIUS_M),
        ),
    });

    // ── Opening: wall solid outside the door, free in the opening ───────
    let solid_samples = [11.5f32, 13.5, 15.5, 21.0, 23.0, 25.5];
    let open_samples = [17.5f32, 18.5];
    let mut wall_ok = true;
    let mut wall_detail = Vec::new();
    for z in solid_samples {
        let status = grid.classify_point([SHOP_X_MAX + 0.5, EYE_HEIGHT_M, z]);
        wall_detail.push(format!("z={z:.1}→{status:?}"));
        if status != CellStatus::Solid {
            wall_ok = false;
        }
    }
    for z in open_samples {
        let status = grid.classify_point([SHOP_X_MAX + 0.5, EYE_HEIGHT_M, z]);
        wall_detail.push(format!("z={z:.1}→{status:?}"));
        if status != CellStatus::Free {
            wall_ok = false;
        }
    }
    checks.push(ProbeCheck {
        id: "door_opening_only",
        ok: wall_ok && r6.crossings.len() == 1 && r6.crossings.iter().all(|c| z_ok(c[2])),
        detail: format!(
            "east wall (x=5.5) at eye height: {walls} · plane x={plane} crossings recorded: {count} \
             (expected 1: entry through the door) · crossing z [{door_min:.1},{door_max:.1}]: {zs} ⇒ {ok}",
            walls = wall_detail.join(" · "),
            plane = SHOP_X_MAX,
            count = r6.crossings.len(),
            zs = r6
                .crossings
                .iter()
                .map(|c| format!("{:.2}", c[2]))
                .collect::<Vec<_>>()
                .join(", "),
            ok = r6.crossings.iter().all(|c| z_ok(c[2])),
        ),
    });

    // ── Contained interior: wall + counter block, aisle free ────────────
    let r7 = march_axis(&grid, r6.pos, 0, -1.0, 1.0);
    let r8 = march_axis(&grid, r7.pos, 2, -1.0, 15.0);
    let r9 = march_axis(&grid, r8.pos, 2, 1.0, 18.0);
    let inner_violations = r7.violations + r8.violations + r9.violations;
    checks.push(ProbeCheck {
        id: "interior_walk_contained",
        ok: inner_violations == 0
            && r7.blocked == Some(CellStatus::Solid)
            && r8.blocked == Some(CellStatus::Solid)
            && r9.reached,
        detail: format!(
            "inside: -X {s7}p → x={x7:.2} (block {:?}, target 1.0: {r7}) · \
             -Z {s8}p → z={z8:.2} (block {:?}, counter/shelf, target 15.0: {r8}) · \
             +Z {s9}p → z={z9:.1} (target 18.0: {r9}) · traversals={inner_violations} (0 req)",
            r7.blocked,
            r8.blocked,
            s7 = r7.steps,
            x7 = r7.pos[0],
            r7 = r7.reached,
            s8 = r8.steps,
            z8 = r8.pos[2],
            r8 = r8.reached,
            s9 = r9.steps,
            z9 = r9.pos[2],
            r9 = r9.reached,
        ),
    });

    // ── Exit through the same door ───────────────────────────────────────
    let r10 = march_axis(&grid, r9.pos, 0, 1.0, 7.0);
    let exited = !shop.contains_interior(r10.pos);
    checks.push(ProbeCheck {
        id: "exit_to_street",
        ok: r10.reached
            && r10.violations == 0
            && exited
            && grid.is_free(r10.pos, PLAYER_RADIUS_M)
            && r10.crossings.iter().all(|c| z_ok(c[2])),
        detail: format!(
            "+X from the interior {s}p → x={x:.2} · region=interior={inside} (must be false) · \
             exit crossing z=[{cz}] · traversals={} (0 req)",
            r10.violations,
            s = r10.steps,
            x = r10.pos[0],
            inside = shop.contains_interior(r10.pos),
            cz = r10
                .crossings
                .iter()
                .map(|c| format!("{:.2}", c[2]))
                .collect::<Vec<_>>()
                .join(", "),
        ),
    });

    // ── Solid props at eye height (AB-06: 0 non-solid props) ────────────
    let fixtures = [
        ("counter", [1.7f32, 1.6, 14.0]),
        ("shelf_south", [2.8, 1.6, 11.0]),
        ("shelf_north", [2.8, 1.6, 25.0]),
        ("island", [3.7, 1.6, 14.0]),
    ];
    let solid_fixtures = fixtures
        .iter()
        .filter(|(_, p)| grid.classify_point(*p) == CellStatus::Solid)
        .count();
    checks.push(ProbeCheck {
        id: "interior_fixtures_solid",
        ok: solid_fixtures == fixtures.len(),
        detail: format!(
            "props at eye y=1.6: {}/{} solid · {} (req: 4/4 — props ≥ 1.4 m occupy the eye layer)",
            solid_fixtures,
            fixtures.len(),
            fixtures
                .iter()
                .map(|(label, p)| format!(
                    "{label}({},{},{})",
                    p[0], p[1], p[2]
                ))
                .collect::<Vec<_>>()
                .join(" · ")
        ),
    });

    let pass = checks.iter().all(|c| c.ok);
    InteriorProbeReport { checks, pass }
}

/// Synthetic world without retail for CI tests (0 I/O): 100% synthetic
/// M3 city + M4 shop — same code the CLI runs over the retail.
pub fn synthetic_world(blocks: usize) -> (CityLayout, CityMesh, ShopMesh) {
    let layout = CityLayout::new(blocks);
    let city = build_city_mesh(&layout, &[]);
    let shop = build_shop_mesh();
    (layout, city, shop)
}

/// World from already parsed ARCH3D landmarks (Mode A, caller custody).
pub fn world_from_landmarks(blocks: usize, landmarks: &[MeshSpec]) -> (CityLayout, CityMesh, ShopMesh) {
    let layout = CityLayout::new(blocks);
    let city = build_city_mesh(&layout, landmarks);
    let shop = build_shop_mesh();
    (layout, city, shop)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn probe_interior_001_passes_on_synthetic_world() {
        let (layout, city, shop) = synthetic_world(3);
        let report = run_interior_probe(&city, &layout, &shop, 1);
        for check in &report.checks {
            assert!(check.ok, "check {} FAIL: {}", check.id, check.detail);
        }
        assert!(report.pass, "PROBE-INTERIOR-001 must PASS: {report:?}");
        assert_eq!(
            report.checks.len(),
            12,
            "generic layer (5) + door/interior layer (7)"
        );
    }

    #[test]
    fn probe_interior_001_is_deterministic() {
        let (layout, city, shop) = synthetic_world(3);
        let a = run_interior_probe(&city, &layout, &shop, 1);
        let b = run_interior_probe(&city, &layout, &shop, 1);
        assert_eq!(a.pass, b.pass);
        assert_eq!(
            a.checks
                .iter()
                .map(|c| (c.id, c.ok, c.detail.as_str()))
                .collect::<Vec<_>>(),
            b.checks
                .iter()
                .map(|c| (c.id, c.ok, c.detail.as_str()))
                .collect::<Vec<_>>(),
            "same input → same report (determinism)"
        );
    }

    #[test]
    fn probe_interior_001_fails_without_shop() {
        // No shop (world = M3 city only): the door layer must fail
        // — the gap and the interior region do not exist.
        let layout = CityLayout::new(3);
        let city = build_city_mesh(&layout, &[]);
        let empty_shop = ShopMesh {
            shell: MeshSpec {
                positions: Vec::new(),
                normals: Vec::new(),
                indices: Vec::new(),
                uvs: Vec::new(),
                face_textures: Vec::new(),
            },
            floor: MeshSpec {
                positions: Vec::new(),
                normals: Vec::new(),
                indices: Vec::new(),
                uvs: Vec::new(),
                face_textures: Vec::new(),
            },
            fixtures: MeshSpec {
                positions: Vec::new(),
                normals: Vec::new(),
                indices: Vec::new(),
                uvs: Vec::new(),
                face_textures: Vec::new(),
            },
            combined: MeshSpec {
                positions: Vec::new(),
                normals: Vec::new(),
                indices: Vec::new(),
                uvs: Vec::new(),
                face_textures: Vec::new(),
            },
        };
        assert!(build_shop_mesh().triangle_count() > 0);
        let report = run_interior_probe(&city, &layout, &empty_shop, 1);
        assert!(!report.pass, "a world without a shop cannot PASS");
        assert!(
            report.checks.iter().any(|c| !c.ok),
            "at least one check must fail"
        );
    }
}
