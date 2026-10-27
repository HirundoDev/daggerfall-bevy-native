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
//! **No-traversal oracle (check 4, [`march_oracle`]):** the march is judged by
//! an oracle **independent of `CollisionGrid`** — [`first_geometry_hit`] against
//! the `MeshSpec` triangles. A counter that re-asks the grid about the position
//! the grid just approved is tautological (it can only report 0), so it cannot
//! witness a grid that fails to index a wall. The oracle instead samples the
//! swept step `pos → candidate` and asks the **geometry**: DFU advances the
//! player with a swept character controller (`PlayerGroundMotor.cs:86`,
//! `controller.Move(moveDirection * Time.deltaTime)`), never by teleporting
//! through a wall, so a discontinuity the grid accepts is a real traversal.
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

/// Clearance tolerance of the geometry oracle (m): a hit at `t ≤ radius − eps`
/// blocks (the inclusive Möller–Trumbore test is `t ≤ max_dist`).
const ORACLE_EPS_M: f32 = 1e-4;
/// Bucket edge of the oracle's triangle-AABB prefilter (m) — coarser than the
/// player sphere, so a query touches at most 8 buckets per axis pair.
const ORACLE_INDEX_CELL_M: f32 = 1.0;
/// Buckets one triangle AABB may occupy before [`TriAabbIndex`] keeps it as a
/// distance-tested AABB instead (bounded memory, same guarantee).
const ORACLE_INDEX_MAX_BUCKETS: usize = 4096;
/// The six axis directions of the sampled sphere clearance.
const ORACLE_AXES: [[f32; 3]; 6] = [
    [1.0, 0.0, 0.0],
    [-1.0, 0.0, 0.0],
    [0.0, 1.0, 0.0],
    [0.0, -1.0, 0.0],
    [0.0, 0.0, 1.0],
    [0.0, 0.0, -1.0],
];

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

/// One direction of the oracle march ([`march_oracle`]).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DirMarch {
    /// Horizontal unit direction.
    pub dir: [f32; 2],
    /// Accepted steps (grid-free destinations).
    pub steps: usize,
    /// Why the march stopped.
    pub end: CellStatus,
}

/// Result of the **independent geometry-oracle march** (check `no_traversal_walk`).
///
/// Every accepted step is judged twice: the grid says the destination is free
/// (that is what gameplay would do) and the geometry oracle samples the swept
/// segment. A step the grid refuses is not a traversal; a step the grid accepts
/// while the geometry is inside the player sphere **is** one.
#[derive(Debug, Clone, PartialEq)]
pub struct MarchReport {
    /// One entry per [`LOOK_DIRS`] direction, in order.
    pub dirs: Vec<DirMarch>,
    /// Accepted steps whose swept segment reached geometry (must be 0).
    pub violations: usize,
    /// Oracle point evaluations (sphere-clearance samples).
    pub samples: usize,
    /// Steps the grid refused (destination not free).
    pub grid_blocks: usize,
    /// Oracle point evaluations that found geometry within the radius.
    pub oracle_blocked_samples: usize,
    /// Directions that ended against a solid cell.
    pub geometry_blocks: usize,
    /// Directions that ended outside the block bounds.
    pub outside_seen: usize,
    /// Directions with at least `MARCH_MIN_OPEN_STEPS` accepted steps.
    pub open_dirs: usize,
}

impl MarchReport {
    /// Check predicate: 0 traversals, ≥1 real geometry block, ≥1 walkable direction.
    pub fn pass(&self) -> bool {
        self.violations == 0 && self.geometry_blocks >= 1 && self.open_dirs >= 1
    }
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

    // 4. Directed 8-direction march judged by the INDEPENDENT geometry oracle:
    //    the grid must never accept a step that reaches the geometry. Re-asking
    //    the grid about the position it just approved (`is_free(pos)` with
    //    `pos == candidate`) is tautological — it can only report 0 — so the
    //    verdict comes from `first_geometry_hit` on the swept segment instead
    //    ([`march_oracle`]).
    let march = march_oracle(spec, &grid, spawn.eye);
    let steps: Vec<String> = march
        .dirs
        .iter()
        .map(|d| {
            format!(
                "({:+.2},{:+.2})→{}p",
                d.dir[0], d.dir[1], d.steps
            )
        })
        .collect();
    checks.push(ProbeCheck {
        id: "no_traversal_walk",
        ok: march.pass(),
        detail: format!(
            "8-dir march × ≤{MARCH_MAX_STEPS} steps of {MARCH_STEP_M} m · traversals={violations} (0 req) · geometry_blocks={geometry_blocks} (≥1 req) · out_of_bounds={outside_seen} · walkable_dirs={open_dirs} (≥1 req) · oracle samples={samples} · grid_blocks={grid_blocks} · oracle_blocked_samples={oracle_blocked} · steps {}",
            steps.join(" "),
            violations = march.violations,
            geometry_blocks = march.geometry_blocks,
            outside_seen = march.outside_seen,
            open_dirs = march.open_dirs,
            samples = march.samples,
            grid_blocks = march.grid_blocks,
            oracle_blocked = march.oracle_blocked_samples,
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

/// Coarse **triangle-AABB index** of a `MeshSpec` — a *sound absence* prefilter
/// for [`geometry_blocks_point`].
///
/// The index answers only "can a triangle possibly be within `radius` of this
/// point?". `false` is a proof of absence, so the 6 rays of
/// [`first_geometry_hit`] — each `O(triangles)` — are only spent where geometry
/// is actually near. `true` is not a verdict: the rays still decide. Without it
/// the oracle costs `6 × triangles` per sample, which is what made the interior
/// search (`m11_probe` `SEARCH_BUDGET`) exceed its wall-clock ceiling.
///
/// Soundness: every triangle AABB is inserted into **all** buckets it overlaps
/// and the query covers every bucket of `[p − reach − eps, p + reach + eps]`, so
/// a triangle inside `reach` of `p` cannot be missed. A triangle spanning more
/// than `ORACLE_INDEX_MAX_BUCKETS` buckets (ground planes, long walls) is kept
/// as one AABB in `TriAabbIndex::big` and distance-tested per query: the same
/// guarantee with bounded memory.
#[derive(Debug, Clone, Default)]
pub struct TriAabbIndex {
    /// Lower corner of the bucket lattice (the spec bounds min).
    min: [f32; 3],
    /// Occupied buckets (a bucket key is the triangle AABB's floor cell).
    occupied: std::collections::HashSet<[i32; 3]>,
    /// Triangle AABBs too large to bucket (`[[lo], [hi]]`), tested per query.
    big: Vec<[[f32; 3]; 2]>,
}

impl TriAabbIndex {
    /// Buckets every triangle AABB of `spec` (triangle soup, `indices` triples).
    pub fn build(spec: &MeshSpec) -> Self {
        let mut index = Self::default();
        let Some((min, _max)) = spec.bounds() else {
            return index;
        };
        index.min = min;
        for tri in spec.indices.chunks_exact(3) {
            let (Some(a), Some(b), Some(c)) = (
                spec.positions.get(tri[0] as usize),
                spec.positions.get(tri[1] as usize),
                spec.positions.get(tri[2] as usize),
            ) else {
                continue;
            };
            let mut lo = *a;
            let mut hi = *a;
            for vertex in [b, c] {
                for axis in 0..3 {
                    lo[axis] = lo[axis].min(vertex[axis]);
                    hi[axis] = hi[axis].max(vertex[axis]);
                }
            }
            let c0 = index.cell_of(lo);
            let c1 = index.cell_of(hi);
            let mut buckets: u64 = 1;
            for axis in 0..3 {
                buckets = buckets.saturating_mul((c1[axis] - c0[axis] + 1).max(1) as u64);
            }
            if buckets > ORACLE_INDEX_MAX_BUCKETS as u64 {
                index.big.push([lo, hi]);
                continue;
            }
            for x in c0[0]..=c1[0] {
                for y in c0[1]..=c1[1] {
                    for z in c0[2]..=c1[2] {
                        index.occupied.insert([x, y, z]);
                    }
                }
            }
        }
        index
    }

    /// Bucket containing `p` (may be negative).
    fn cell_of(&self, p: [f32; 3]) -> [i32; 3] {
        let mut cell = [0i32; 3];
        for axis in 0..3 {
            cell[axis] = ((p[axis] - self.min[axis]) / ORACLE_INDEX_CELL_M).floor() as i32;
        }
        cell
    }

    /// `false` only when no triangle AABB can reach within `reach` of `p`.
    fn may_block(&self, p: [f32; 3], reach: f32) -> bool {
        let margin = reach + ORACLE_EPS_M;
        let lo = self.cell_of([p[0] - margin, p[1] - margin, p[2] - margin]);
        let hi = self.cell_of([p[0] + margin, p[1] + margin, p[2] + margin]);
        for x in lo[0]..=hi[0] {
            for y in lo[1]..=hi[1] {
                for z in lo[2]..=hi[2] {
                    if self.occupied.contains(&[x, y, z]) {
                        return true;
                    }
                }
            }
        }
        // Oversized triangles: point↔AABB distance (0 inside the box).
        let reach2 = margin * margin;
        self.big.iter().any(|aabb| {
            let mut dist2 = 0.0f32;
            for axis in 0..3 {
                let gap = if p[axis] < aabb[0][axis] {
                    aabb[0][axis] - p[axis]
                } else if p[axis] > aabb[1][axis] {
                    p[axis] - aabb[1][axis]
                } else {
                    0.0
                };
                dist2 += gap * gap;
            }
            dist2 <= reach2
        })
    }
}

/// Sampled **sphere clearance against the exact geometry** — the primitive of
/// the independent oracle.
///
/// A point is geometrically blocked when any of the 6 axis rays from it hits a
/// `MeshSpec` triangle at `t ≤ radius − eps` ([`first_geometry_hit`]); `index`
/// only spares the rays where no triangle can reach. This is a *sampled*
/// clearance, not an exact capsule test: it can miss a thin sliver that no axis
/// ray crosses, but it never invents a block the geometry does not have.
pub fn geometry_blocks_point(
    spec: &MeshSpec,
    index: &TriAabbIndex,
    p: [f32; 3],
    radius: f32,
) -> bool {
    let reach = radius - ORACLE_EPS_M;
    if reach <= 0.0 || !index.may_block(p, reach) {
        return false;
    }
    ORACLE_AXES
        .iter()
        .any(|dir| first_geometry_hit(spec, p, *dir, reach).is_some())
}

/// One step of a march judged by the independent geometry oracle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SweptStep {
    /// Sphere-clearance samples taken on the segment (both endpoints included).
    pub samples: usize,
    /// Samples that found geometry within the player radius.
    pub blocked_samples: usize,
}

impl SweptStep {
    /// `true` when any sample of the swept segment reached geometry.
    pub fn blocked(&self) -> bool {
        self.blocked_samples > 0
    }
}

/// Judges the swept segment `from → to` against the **exact geometry** — never
/// against the grid (N1: a counter that re-asks `CollisionGrid` about a position
/// the grid itself approved can only report 0).
///
/// The segment is sampled at spacing `≤ radius/2`, both endpoints included, and
/// every sample is a [`geometry_blocks_point`] sphere-clearance probe. This is the
/// single primitive shared by every no-traversal march ([`march_oracle`],
/// `interior_probe::march_axis` and the street walk of `city_probe`) — one
/// implementation, 0 divergent oracles.
pub fn swept_step_oracle(
    spec: &MeshSpec,
    index: &TriAabbIndex,
    from: [f32; 3],
    to: [f32; 3],
    radius: f32,
) -> SweptStep {
    let d = [to[0] - from[0], to[1] - from[1], to[2] - from[2]];
    let dist = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
    // ≤ radius/2 between samples: with radius 0.30 m and a 0.25 m march step the
    // segment is split in 2 (spacing 0.125 m, 3 samples including both ends).
    let max_gap = (radius * 0.5).max(f32::EPSILON);
    let per_step = ((dist / max_gap).ceil() as usize).max(1);
    let mut verdict = SweptStep {
        samples: 0,
        blocked_samples: 0,
    };
    for i in 0..=per_step {
        let t = i as f32 / per_step as f32;
        let sample = [
            from[0] + d[0] * t,
            from[1] + d[1] * t,
            from[2] + d[2] * t,
        ];
        verdict.samples += 1;
        if geometry_blocks_point(spec, index, sample, radius) {
            verdict.blocked_samples += 1;
        }
    }
    verdict
}

/// Marches the 8 [`LOOK_DIRS`] and judges every grid-accepted step against the
/// geometry, not against the grid.
///
/// For each accepted step `pos → candidate` the segment is sampled at spacing
/// `≤ radius/2` (both endpoints included). If any sample is geometrically
/// blocked while `grid.is_free(candidate, radius)` said the destination was
/// free, the step is a **real traversal** (`violations += 1`) — the grid let the
/// player enter geometry it did not index. A step the grid refuses
/// (`grid_blocks += 1`) is not a traversal: gameplay never reaches it.
///
/// The spawn point is the first sample of every direction, so a spawn inside
/// geometry is reported too (it is the same traversal, seen from 8 sides).
///
/// Pure: `&MeshSpec` in, counters out — shared by `run_view_probe` and the
/// tests, one implementation (0 divergence between probe and evidence).
pub fn march_oracle(spec: &MeshSpec, grid: &CollisionGrid, spawn_eye: [f32; 3]) -> MarchReport {
    // One O(triangles) index for the whole march: the geometry is the same for
    // every direction (prefilter of `geometry_blocks_point`, not a verdict).
    let index = TriAabbIndex::build(spec);
    let mut report = MarchReport {
        dirs: Vec::with_capacity(LOOK_DIRS.len()),
        violations: 0,
        samples: 0,
        grid_blocks: 0,
        oracle_blocked_samples: 0,
        geometry_blocks: 0,
        outside_seen: 0,
        open_dirs: 0,
    };
    for dir in LOOK_DIRS {
        let mut pos = spawn_eye;
        let mut steps = 0usize;
        let mut reason: Option<CellStatus> = None;
        for _ in 0..MARCH_MAX_STEPS {
            let delta = [dir[0] * MARCH_STEP_M, 0.0, dir[1] * MARCH_STEP_M];
            // Straight march: no sliding (same pure delta as a held W key).
            let candidate = [pos[0] + delta[0], pos[1] + delta[1], pos[2] + delta[2]];
            if !grid.is_free(candidate, PLAYER_RADIUS_M) {
                report.grid_blocks += 1;
                reason = Some(grid.classify(candidate, PLAYER_RADIUS_M));
                break;
            }
            // The grid accepted the destination: sample the swept segment.
            let step = swept_step_oracle(spec, &index, pos, candidate, PLAYER_RADIUS_M);
            report.samples += step.samples;
            report.oracle_blocked_samples += step.blocked_samples;
            if step.blocked() {
                report.violations += 1;
            }
            pos = candidate;
            steps += 1;
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
        let end = reason.unwrap_or(CellStatus::Free);
        match end {
            CellStatus::Solid => report.geometry_blocks += 1,
            CellStatus::Outside => report.outside_seen += 1,
            CellStatus::Free => {}
        }
        if steps >= MARCH_MIN_OPEN_STEPS {
            report.open_dirs += 1;
        }
        report.dirs.push(DirMarch { dir, steps, end });
    }
    report
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Appends a quad (2 triangles, per-vertex normal) to a synthetic spec.
    fn push_quad(
        spec: &mut MeshSpec,
        a: [f32; 3],
        b: [f32; 3],
        c: [f32; 3],
        d: [f32; 3],
        normal: [f32; 3],
    ) {
        let base = spec.positions.len() as u32;
        spec.positions.extend([a, b, c, d]);
        spec.normals.extend([normal; 4]);
        spec.indices
            .extend([base, base + 1, base + 2, base, base + 2, base + 3]);
    }

    /// Synthetic room (same geometry as the grid tests).
    fn room_spec() -> MeshSpec {
        let mut spec = MeshSpec::new(Vec::new(), Vec::new(), Vec::new());
        push_quad(&mut spec, [0.0, 0.0, 0.0], [0.0, 0.0, 8.0], [8.0, 0.0, 8.0], [8.0, 0.0, 0.0], [0.0, 1.0, 0.0]);
        push_quad(&mut spec, [0.0, 3.0, 0.0], [8.0, 3.0, 0.0], [8.0, 3.0, 8.0], [0.0, 3.0, 8.0], [0.0, -1.0, 0.0]);
        push_quad(&mut spec, [0.0, 0.0, 0.0], [0.0, 3.0, 0.0], [0.0, 3.0, 8.0], [0.0, 0.0, 8.0], [1.0, 0.0, 0.0]);
        push_quad(&mut spec, [8.0, 0.0, 0.0], [8.0, 0.0, 8.0], [8.0, 3.0, 8.0], [8.0, 3.0, 0.0], [-1.0, 0.0, 0.0]);
        push_quad(&mut spec, [0.0, 0.0, 0.0], [8.0, 0.0, 0.0], [8.0, 3.0, 0.0], [0.0, 3.0, 0.0], [0.0, 0.0, 1.0]);
        push_quad(&mut spec, [0.0, 0.0, 8.0], [0.0, 3.0, 8.0], [8.0, 3.0, 8.0], [8.0, 0.0, 8.0], [0.0, 0.0, -1.0]);
        spec
    }

    /// Thin closed slab (~0.1 m thick) across the +Z march corridor, at eye
    /// height: x ∈ [3,5], y ∈ [1.3,1.9], z ∈ [5.95,6.05].
    fn add_corridor_wall(spec: &mut MeshSpec) {
        let (x0, x1) = (3.0, 5.0);
        let (y0, y1) = (1.3, 1.9);
        let (z0, z1) = (5.95, 6.05);
        push_quad(spec, [x0, y0, z1], [x1, y0, z1], [x1, y1, z1], [x0, y1, z1], [0.0, 0.0, 1.0]);
        push_quad(spec, [x1, y0, z0], [x0, y0, z0], [x0, y1, z0], [x1, y1, z0], [0.0, 0.0, -1.0]);
        push_quad(spec, [x1, y0, z1], [x1, y0, z0], [x1, y1, z0], [x1, y1, z1], [1.0, 0.0, 0.0]);
        push_quad(spec, [x0, y0, z0], [x0, y0, z1], [x0, y1, z1], [x0, y1, z0], [-1.0, 0.0, 0.0]);
        push_quad(spec, [x0, y1, z1], [x1, y1, z1], [x1, y1, z0], [x0, y1, z0], [0.0, 1.0, 0.0]);
        push_quad(spec, [x0, y0, z0], [x1, y0, z0], [x1, y0, z1], [x0, y0, z1], [0.0, -1.0, 0.0]);
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

    /// Positive control: the oracle agrees with a grid that indexed the very
    /// geometry the oracle sees — 0 traversals, and the probe keeps its 5 ids.
    #[test]
    fn oracle_agrees_with_a_grid_built_from_the_same_spec() {
        let spec = room_spec();
        let grid = CollisionGrid::from_mesh_spec(&spec, DEFAULT_CELL_M);
        let march = march_oracle(&spec, &grid, [4.0, 1.6, 4.0]);
        assert_eq!(
            march.violations, 0,
            "no grid-free step of the room reaches the geometry: {march:?}"
        );
        assert_eq!(
            march.oracle_blocked_samples, 0,
            "a grid-free point keeps the sphere clear of the triangles: {march:?}"
        );
        assert!(march.samples > 0, "the oracle must actually sample: {march:?}");
        assert!(march.pass(), "march verdict: {march:?}");

        let report = run_view_probe(&spec);
        let ids: Vec<&str> = report.checks.iter().map(|c| c.id).collect();
        assert_eq!(
            ids,
            [
                "grid_from_layout",
                "grid_layout_coherence",
                "spawn_inside_block",
                "no_traversal_walk",
                "legibility_verdict"
            ],
            "check ids are a downstream contract (city/interior probes reuse them)"
        );
        assert!(report.pass, "PROBE-VIEW-001 must still PASS: {report:?}");
        let walk = report
            .checks
            .iter()
            .find(|c| c.id == "no_traversal_walk")
            .expect("check 4 exists");
        assert!(
            walk.detail.contains("oracle samples=")
                && walk.detail.contains("grid_blocks=")
                && walk.detail.contains("oracle_blocked_samples="),
            "oracle numbers belong in the evidence line: {}",
            walk.detail
        );
    }

    /// NEGATIVE CONTROL — the oracle is not tautological.
    ///
    /// The grid is built from the ORIGINAL room; the geometry handed to the
    /// march is a CLONE with one extra thin wall in the corridor. The grid cannot
    /// see a wall it never indexed (`is_free` says every destination is free, so
    /// a grid-based counter would still report 0 traversals), therefore only an
    /// independent geometric oracle can flag the step. If this test ever reports
    /// `violations == 0`, check 4 is blind again and the counter has regressed to
    /// the tautology.
    #[test]
    fn oracle_flags_a_wall_the_grid_never_indexed() {
        let original = room_spec();
        let grid = CollisionGrid::from_mesh_spec(&original, DEFAULT_CELL_M);
        // The grid's own verdict on the wall's position: free.
        assert!(
            grid.is_free([4.0, 1.6, 6.0], PLAYER_RADIUS_M),
            "precondition: the grid built before the wall says the wall's cell is free"
        );

        let mut clone = original.clone();
        add_corridor_wall(&mut clone);
        let march = march_oracle(&clone, &grid, [4.0, 1.6, 4.0]);

        assert!(
            march.oracle_blocked_samples > 0,
            "the geometry oracle must see the unindexed wall: {march:?}"
        );
        assert!(
            march.violations >= 1,
            "an accepted step crossing the unindexed wall is a real traversal: {march:?}"
        );
        assert!(!march.pass(), "the oracle must be able to fail: {march:?}");
    }

    /// The absence prefilter may never hide geometry: wherever a triangle AABB is
    /// within `reach` of a point (brute force over every triangle of the spec),
    /// [`TriAabbIndex::may_block`] must say "maybe". This is what keeps the
    /// optimization from turning the oracle back into a no-op.
    #[test]
    fn prefilter_never_hides_geometry_within_reach() {
        let mut spec = room_spec();
        // A long wall and a room-spanning plate exercise the oversized-AABB path
        // (`big`); the room quads above exercise the bucketed one.
        add_corridor_wall(&mut spec);
        // 200 m plate at y = 2.0: 201 × 1 × 201 buckets > the cap → `big`.
        push_quad(
            &mut spec,
            [-100.0, 2.0, -100.0],
            [-100.0, 2.0, 100.0],
            [100.0, 2.0, 100.0],
            [100.0, 2.0, -100.0],
            [0.0, 1.0, 0.0],
        );
        let index = TriAabbIndex::build(&spec);
        let reach = PLAYER_RADIUS_M - ORACLE_EPS_M;
        let margin = reach + ORACLE_EPS_M;
        let mut checked = 0usize;
        let mut near = 0usize;
        let mut z = -0.5f32;
        while z <= 8.5 {
            let mut x = -0.5f32;
            while x <= 8.5 {
                for y in [0.9f32, 1.6, 1.9, 2.4] {
                    let p = [x, y, z];
                    // Brute force: distance from p to every triangle AABB.
                    let mut brute = false;
                    for tri in spec.indices.chunks_exact(3) {
                        let tri: Vec<[f32; 3]> = tri
                            .iter()
                            .map(|i| spec.positions[*i as usize])
                            .collect();
                        let mut dist2 = 0.0f32;
                        for axis in 0..3 {
                            let lo = tri[0][axis].min(tri[1][axis]).min(tri[2][axis]);
                            let hi = tri[0][axis].max(tri[1][axis]).max(tri[2][axis]);
                            let gap = if p[axis] < lo {
                                lo - p[axis]
                            } else if p[axis] > hi {
                                p[axis] - hi
                            } else {
                                0.0
                            };
                            dist2 += gap * gap;
                        }
                        if dist2 <= margin * margin {
                            brute = true;
                            break;
                        }
                    }
                    checked += 1;
                    if brute {
                        near += 1;
                        assert!(
                            index.may_block(p, reach),
                            "prefilter hid geometry at {p:?} (brute force found a triangle within {margin} m)"
                        );
                    }
                }
                x += 0.5;
            }
            z += 0.5;
        }
        assert!(checked > 1000, "the sweep must be dense: {checked} points");
        assert!(near > 0, "the sweep must reach the geometry: {near} near points");
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
