//! `CollisionGrid` — voxel collision grid derived from [`MeshSpec`] (M2).
//!
//! **Grid-vs-geometry decision (documented in the M2 note):** INV-L99-5 requires
//! "collisions coherent with the grid"; the M2 pack allows "the M1 geometry or a
//! derived grid". This lane chooses a **3D voxel grid derived from `MeshSpec`**
//! (triangle rasterization, 1.0 m cell, sparse storage):
//! - rejects (A) direct raycast against the triangle soup: it does not materialize
//!   the "grid" that INV-L99-5 names and costs O(T) per step;
//! - rejects (B) a 2D grid from MAPS.BSA: the MAPS parser belongs to M3 (it does not exist today);
//! - satisfies (C): grid↔layout coherence **executably verifiable**
//!   (bidirectional, [`CollisionGrid::verify_layout_coherence`]).
//!
//! Properties (the ones the probe and the tests assert):
//! - **Every free cell contains 0 geometry** — a cell is marked solid if
//!   some triangle touches it (inclusive rasterization); therefore a sphere
//!   whose AABB cells `[p-r, p+r]` are free cannot intersect any triangle
//!   (no-traversal guarantee for radius `r`).
//! - **Confinement**: `is_free` additionally requires the point AABB to be inside
//!   the `MeshSpec` bounds (the camera never leaves the parsed block).
//! - The **spawn uses exact geometry**: eye height = `floor_y` of the first
//!   vertical ray hit against triangles + `eye_height` (0 voxel
//!   offsets); "grounded" is re-verified with the ray itself.
//!
//! Bridge rules (PRE-2 §8) respected: **pure** module (only `std`, 0
//! Bevy, 0 input, 0 mode logic, 0 persistence) — testable headless.
//! `cell` is a parameter (default [`DEFAULT_CELL_M`]); fine grain ≤0.5 m is left
//! to M3 (delta documented in the M2 note).

use std::collections::HashSet;

use crate::mesh_spec::MeshSpec;

/// Default cell size (meters).
pub const DEFAULT_CELL_M: f32 = 1.0;

/// Tolerance for inclusive separating tests (touched cell = solid).
const SAT_EPS: f32 = 1e-4;

/// World state for a point/radius: can the camera occupy it?
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CellStatus {
    /// Free of geometry and inside the block bounds.
    Free,
    /// Solid cell (intersects parsed geometry).
    Solid,
    /// Outside the block bounds — blocked by confinement.
    Outside,
}

/// Result of [`CollisionGrid::resolve_step`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StepOutcome {
    /// Resulting position (always free).
    pub pos: [f32; 3],
    /// `true` if the position advanced relative to `from`.
    pub moved: bool,
    /// Reason the full attempt was blocked (if it was).
    pub blocked: Option<CellStatus>,
}

/// Spawn point: eye above the exact floor (geometry, not voxel).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SpawnPoint {
    /// Eye position (`floor_y + eye_height`).
    pub eye: [f32; 3],
    /// Exact floor Y height under the spawn (ray hit on triangles).
    pub floor_y: f32,
}

/// Ray hit on a solid cell (voxel DDA).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RayHit {
    /// Distance travelled from the origin to the hit cell.
    pub dist: f32,
    /// Solid cell that was hit.
    pub cell: [i32; 3],
}

/// Grid↔layout coherence report (INV-L99-5, executable check).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CoherenceReport {
    /// Total solid cells.
    pub solid_cells: usize,
    /// Triangles whose centroid must fall in a solid cell (geometry→grid).
    pub centroids_checked: usize,
    /// Centroids in a NON-solid cell (must be 0).
    pub centroids_missing: usize,
    /// Solid cells sampled with the inverse test (grid→geometry).
    pub phantom_probes: usize,
    /// Solid cells with no nearby triangle = phantoms (must be 0).
    pub phantom_found: usize,
}

impl CoherenceReport {
    /// Coherence PASS: cells > 0 · 0 missing centroid · 0 phantom.
    pub fn pass(&self) -> bool {
        self.solid_cells > 0 && self.centroids_missing == 0 && self.phantom_found == 0
    }
}

/// Sparse voxel collision grid (INV-L99-5).
#[derive(Debug, Clone)]
pub struct CollisionGrid {
    cell: f32,
    /// Minimum corner = `MeshSpec` bounds min.
    min: [f32; 3],
    /// Maximum corner = `MeshSpec` bounds max (strict confinement).
    max: [f32; 3],
    /// Dimensions in cells per axis (`floor(span/cell)+1`: the face at the
    /// maximum bounds falls in its own cell, without clamped ceiling/wall).
    dims: [i32; 3],
    solid: HashSet<[i32; 3]>,
}

impl CollisionGrid {
    /// Voxelizes the `MeshSpec` geometry into a grid of `cell` (m) cells.
    ///
    /// Per triangle: if its normal is axis-aligned (aligned retail faces),
    /// rasterization over the cell range of the triangle AABB (exact plane
    /// within its layer, conservative on a lateral layer); otherwise, inclusive
    /// SAT triangle↔box test per cell (Akenine-Möller, 15 axes + EPS).
    pub fn from_mesh_spec(spec: &MeshSpec, cell: f32) -> Self {
        assert!(cell > 0.0, "the cell must be positive");
        let Some((min, max)) = spec.bounds() else {
            return Self {
                cell,
                min: [0.0; 3],
                max: [0.0; 3],
                dims: [0, 0, 0],
                solid: HashSet::new(),
            };
        };
        let mut dims = [0i32; 3];
        for axis in 0..3 {
            let span = (max[axis] - min[axis]) / cell;
            dims[axis] = (span.floor() as i32).max(0) + 1;
        }
        let mut grid = Self {
            cell,
            min,
            max,
            dims,
            solid: HashSet::new(),
        };
        for tri in triangles(spec) {
            grid.rasterize_triangle(&tri);
        }
        grid
    }

    /// Cell size (m).
    pub fn cell_size(&self) -> f32 {
        self.cell
    }

    /// Grid dimensions in cells per axis.
    pub fn dims(&self) -> [i32; 3] {
        self.dims
    }

    /// Mesh bounds `[min, min + dims*cell]` (≥ the spec AABB).
    pub fn bounds(&self) -> ([f32; 3], [f32; 3]) {
        let grid_max = [
            self.min[0] + self.dims[0] as f32 * self.cell,
            self.min[1] + self.dims[1] as f32 * self.cell,
            self.min[2] + self.dims[2] as f32 * self.cell,
        ];
        (self.min, grid_max)
    }

    /// Block bounds = exact `MeshSpec` AABB (confinement).
    pub fn block_bounds(&self) -> ([f32; 3], [f32; 3]) {
        (self.min, self.max)
    }

    /// Number of solid cells.
    pub fn solid_cell_count(&self) -> usize {
        self.solid.len()
    }

    /// Cell index containing the point (negative outside the min).
    pub fn cell_of(&self, p: [f32; 3]) -> [i32; 3] {
        let mut c = [0i32; 3];
        for axis in 0..3 {
            c[axis] = ((p[axis] - self.min[axis]) / self.cell).floor() as i32;
        }
        c
    }

    /// Center of cell `c` (world coordinates).
    pub fn cell_center(&self, c: [i32; 3]) -> [f32; 3] {
        [
            self.min[0] + (c[0] as f32 + 0.5) * self.cell,
            self.min[1] + (c[1] as f32 + 0.5) * self.cell,
            self.min[2] + (c[2] as f32 + 0.5) * self.cell,
        ]
    }

    /// Is cell `c` solid? (cells outside the mesh are not here).
    pub fn is_solid_cell(&self, c: [i32; 3]) -> bool {
        self.solid.contains(&c)
    }

    /// Is the cell inside the mesh?
    pub fn in_grid(&self, c: [i32; 3]) -> bool {
        (0..3).all(|axis| c[axis] >= 0 && c[axis] < self.dims[axis])
    }

    /// Is the point inside the exact block bounds?
    pub fn in_block(&self, p: [f32; 3]) -> bool {
        (0..3).all(|axis| p[axis] >= self.min[axis] && p[axis] <= self.max[axis])
    }

    /// Classifies a point (no radius): Free / Solid / Outside.
    pub fn classify_point(&self, p: [f32; 3]) -> CellStatus {
        let c = self.cell_of(p);
        if !self.in_grid(c) || !self.in_block(p) {
            CellStatus::Outside
        } else if self.solid.contains(&c) {
            CellStatus::Solid
        } else {
            CellStatus::Free
        }
    }

    /// Free for a sphere of radius `radius` centered at `p`?
    ///
    /// Requires: AABB `[p±r]` inside the block bounds **and** all its
    /// cells in-mesh and not solid. Guarantee: free cells contain 0
    /// geometry → a sphere in the AABB of free cells touches no triangle.
    pub fn is_free(&self, p: [f32; 3], radius: f32) -> bool {
        let lo = [p[0] - radius, p[1] - radius, p[2] - radius];
        let hi = [p[0] + radius, p[1] + radius, p[2] + radius];
        if !(0..3).all(|axis| lo[axis] >= self.min[axis] && hi[axis] <= self.max[axis]) {
            return false;
        }
        let clo = self.cell_of(lo);
        let chi = self.cell_of(hi);
        for x in clo[0]..=chi[0] {
            for y in clo[1]..=chi[1] {
                for z in clo[2]..=chi[2] {
                    let c = [x, y, z];
                    if !self.in_grid(c) || self.solid.contains(&c) {
                        return false;
                    }
                }
            }
        }
        true
    }

    /// Classifies destination `p` with radius (block reason): any AABB
    /// cell outside the mesh → `Outside`; otherwise, any solid → `Solid`;
    /// otherwise, AABB outside bounds → `Outside`; otherwise → `Free`.
    pub fn classify(&self, p: [f32; 3], radius: f32) -> CellStatus {
        let lo = [p[0] - radius, p[1] - radius, p[2] - radius];
        let hi = [p[0] + radius, p[1] + radius, p[2] + radius];
        let clo = self.cell_of(lo);
        let chi = self.cell_of(hi);
        let mut solid_seen = false;
        for x in clo[0]..=chi[0] {
            for y in clo[1]..=chi[1] {
                for z in clo[2]..=chi[2] {
                    let c = [x, y, z];
                    if !self.in_grid(c) {
                        return CellStatus::Outside;
                    }
                    if self.solid.contains(&c) {
                        solid_seen = true;
                    }
                }
            }
        }
        if solid_seen {
            return CellStatus::Solid;
        }
        if !(0..3).all(|axis| lo[axis] >= self.min[axis] && hi[axis] <= self.max[axis]) {
            return CellStatus::Outside;
        }
        CellStatus::Free
    }

    /// Resolves a step: full attempt `from + delta`; if it is blocked,
    /// per-axis sliding (X only, then Z only); if everything is blocked, it stays
    /// at `from`. Invariant: the resulting position is **always** `is_free`
    /// (no-traversal, INV-L99-5).
    pub fn resolve_step(&self, from: [f32; 3], delta: [f32; 3], radius: f32) -> StepOutcome {
        let full = [from[0] + delta[0], from[1] + delta[1], from[2] + delta[2]];
        if self.is_free(full, radius) {
            return StepOutcome {
                pos: full,
                moved: full != from,
                blocked: None,
            };
        }
        let status = self.classify(full, radius);
        for slide in [
            [from[0] + delta[0], from[1] + delta[1], from[2]],
            [from[0], from[1] + delta[1], from[2] + delta[2]],
        ] {
            if self.is_free(slide, radius) && slide != from {
                return StepOutcome {
                    pos: slide,
                    moved: true,
                    blocked: Some(status),
                };
            }
        }
        StepOutcome {
            pos: from,
            moved: false,
            blocked: Some(status),
        }
    }

    /// Spawn inside the block: spiral column from the center; exact floor
    /// by a vertical ray against triangles (0 voxel offsets) with
    /// preference for the **roofed interior** (if there is a ceiling above the eye,
    /// that candidate wins) and a free eye with `radius`.
    pub fn find_spawn(
        &self,
        spec: &MeshSpec,
        eye_height: f32,
        radius: f32,
    ) -> Option<SpawnPoint> {
        if self.dims[0] == 0 || self.dims[1] == 0 || self.dims[2] == 0 {
            return None;
        }
        let cx = self.dims[0] / 2;
        let cz = self.dims[2] / 2;
        let max_ring = self.dims[0].max(self.dims[2]) / 2 + 1;
        for ring in 0..=max_ring {
            for x in (cx - ring).max(0)..=(cx + ring).min(self.dims[0] - 1) {
                for z in (cz - ring).max(0)..=(cz + ring).min(self.dims[2] - 1) {
                    if ring > 0
                        && x != cx - ring
                        && x != cx + ring
                        && z != cz - ring
                        && z != cz + ring
                    {
                        continue;
                    }
                    if let Some(spawn) = self.column_spawn(spec, x, z, eye_height, radius) {
                        return Some(spawn);
                    }
                }
            }
        }
        None
    }

    fn column_spawn(
        &self,
        spec: &MeshSpec,
        x: i32,
        z: i32,
        eye_height: f32,
        radius: f32,
    ) -> Option<SpawnPoint> {
        let px = self.min[0] + (x as f32 + 0.5) * self.cell;
        let pz = self.min[2] + (z as f32 + 0.5) * self.cell;
        let grid_top = self.min[1] + self.dims[1] as f32 * self.cell;
        let max_dist = grid_top - self.min[1] + 0.02;
        // Descending from just under the top face of the mesh; if the
        // column is flat at the maximum (e.g. floor only), retry from above.
        let mut origin_y = grid_top - 0.01;
        let mut ts = geometry_hits(spec, [px, origin_y, pz], [0.0, -1.0, 0.0], max_dist);
        if ts.is_empty() {
            origin_y = grid_top + 0.01;
            ts = geometry_hits(spec, [px, origin_y, pz], [0.0, -1.0, 0.0], max_dist);
        }
        // t → world height (dir points -Y): y = origin_y - t.
        let mut hits: Vec<f32> = ts.into_iter().map(|t| origin_y - t).collect();
        hits.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let mut fallback: Option<SpawnPoint> = None;
        for floor_y in hits {
            let eye = [px, floor_y + eye_height, pz];
            if !self.is_free(eye, radius) {
                continue;
            }
            let candidate = SpawnPoint { eye, floor_y };
            // Preference: surface with geometry above the eye (roofed
            // interior) — upward ray within the mesh height.
            if first_geometry_hit(spec, eye, [0.0, 1.0, 0.0], max_dist).is_some() {
                return Some(candidate);
            }
            fallback.get_or_insert(candidate);
        }
        fallback
    }

    /// DDA ray (Amanatides-Woo) over the grid: first solid cell within
    /// `max_dist`. `None` if it leaves the mesh or hits nothing.
    pub fn raycast(&self, origin: [f32; 3], dir: [f32; 3], max_dist: f32) -> Option<RayHit> {
        let len = (dir[0] * dir[0] + dir[1] * dir[1] + dir[2] * dir[2]).sqrt();
        if len <= f32::EPSILON {
            return None;
        }
        let d = [dir[0] / len, dir[1] / len, dir[2] / len];
        let mut cell = self.cell_of(origin);
        if !self.in_grid(cell) {
            return None;
        }
        if self.solid.contains(&cell) {
            return Some(RayHit { dist: 0.0, cell });
        }
        let mut t_max = [f32::INFINITY; 3];
        let mut t_delta = [f32::INFINITY; 3];
        for axis in 0..3 {
            if d[axis] == 0.0 {
                continue;
            }
            let next_boundary = self.min[axis]
                + if d[axis] > 0.0 {
                    (cell[axis] + 1) as f32 * self.cell
                } else {
                    cell[axis] as f32 * self.cell
                };
            t_max[axis] = (next_boundary - origin[axis]) / d[axis];
            t_delta[axis] = self.cell / d[axis].abs();
        }
        loop {
            let axis = (0..3).min_by(|&a, &b| t_max[a].partial_cmp(&t_max[b]).unwrap())?;
            if t_max[axis] > max_dist {
                return None;
            }
            cell[axis] += if d[axis] > 0.0 { 1 } else { -1 };
            if !self.in_grid(cell) {
                return None;
            }
            if self.solid.contains(&cell) {
                return Some(RayHit {
                    dist: t_max[axis],
                    cell,
                });
            }
            t_max[axis] += t_delta[axis];
        }
    }

    /// Bidirectional grid↔layout coherence (INV-L99-5, executable):
    /// - geometry→grid: the centroid of **every** triangle falls in a solid cell;
    /// - grid→geometry: **deterministic** sample (stride over the grid
    ///   index; 0 HashSet iteration → reproducible) of solid cells;
    ///   each one must overlap the AABB of some triangle (0 phantoms). The
    ///   predicate is evaluated through an exact bucket of `(triangle AABB,
    ///   cell)` overlaps (same inclusive test, no approximation) so the cost is
    ///   `Σ(triangle AABB cells)` instead of `solid cells × triangles` — the
    ///   latter was measured as 99% of PROBE-RDB-001 over retail `.RDB` data.
    pub fn verify_layout_coherence(&self, spec: &MeshSpec) -> CoherenceReport {
        let mut report = CoherenceReport {
            solid_cells: self.solid.len(),
            centroids_checked: 0,
            centroids_missing: 0,
            phantom_probes: 0,
            phantom_found: 0,
        };
        for tri in triangles(spec) {
            // Geometric invariant: the centroid belongs to the triangle, ⊆
            // its rasterized AABB. With non-dyadic coordinates (M10-FIX3, real
            // DFU scale) `sum/3` can round ~1 ULP outside the AABB of the
            // triangle itself (exact face of a model) → `cell_of` would fall
            // one cell outside the marked range (23/43124 in Horcroft, measured
            // at runtime). Clamp to the triangle AABB (a subset of the mesh
            // one): the resulting cell ALWAYS stays in the triangle's
            // rasterized range and is still required to be solid (same
            // invariant, without relaxing the verdict).
            let mut tmin = tri[0];
            let mut tmax = tri[0];
            for v in &tri[1..] {
                for axis in 0..3 {
                    tmin[axis] = tmin[axis].min(v[axis]);
                    tmax[axis] = tmax[axis].max(v[axis]);
                }
            }
            let centroid = [
                ((tri[0][0] + tri[1][0] + tri[2][0]) / 3.0).clamp(tmin[0], tmax[0]),
                ((tri[0][1] + tri[1][1] + tri[2][1]) / 3.0).clamp(tmin[1], tmax[1]),
                ((tri[0][2] + tri[1][2] + tri[2][2]) / 3.0).clamp(tmin[2], tmax[2]),
            ];
            report.centroids_checked += 1;
            let c = self.cell_of(centroid);
            if !self.in_grid(c) || !self.solid.contains(&c) {
                report.centroids_missing += 1;
            }
        }
        // grid→geometry index: for each triangle, the cells whose box its AABB
        // can touch, decided by the SAME inclusive `aabb_overlaps` predicate
        // used before (the bucket only avoids re-testing every triangle for
        // every solid cell: the old cost was O(solid cells × triangles) with
        // the triangle AABB recomputed per pair, measured as 99% of
        // PROBE-RDB-001's runtime over retail `.RDB` interiors). The cell
        // range starts one cell on the negative side because the inclusive
        // test also accepts a cell whose max face equals `tri_box[0]`.
        let mut covered: HashSet<[i32; 3]> = HashSet::new();
        for tri in triangles(spec) {
            let tri_box = tri_aabb(&tri);
            let mut i0 = [0i32; 3];
            let mut i1 = [0i32; 3];
            for axis in 0..3 {
                i0[axis] =
                    ((((tri_box[0][axis] - self.min[axis]) / self.cell).floor() as i32) - 1)
                        .max(0);
                i1[axis] = (((tri_box[1][axis] - self.min[axis]) / self.cell).floor() as i32)
                    .min(self.dims[axis] - 1);
            }
            for x in i0[0]..=i1[0] {
                for y in i0[1]..=i1[1] {
                    for z in i0[2]..=i1[2] {
                        let c = [x, y, z];
                        if covered.contains(&c) {
                            continue;
                        }
                        let lo = [
                            self.min[0] + x as f32 * self.cell,
                            self.min[1] + y as f32 * self.cell,
                            self.min[2] + z as f32 * self.cell,
                        ];
                        let hi = [lo[0] + self.cell, lo[1] + self.cell, lo[2] + self.cell];
                        if aabb_overlaps(&tri_box, &[lo, hi]) {
                            covered.insert(c);
                        }
                    }
                }
            }
        }
        let stride = [
            (self.dims[0] / 64).max(1),
            (self.dims[1] / 64).max(1),
            (self.dims[2] / 64).max(1),
        ];
        for x in (0..self.dims[0]).step_by(stride[0] as usize) {
            for y in (0..self.dims[1]).step_by(stride[1] as usize) {
                for z in (0..self.dims[2]).step_by(stride[2] as usize) {
                    let c = [x, y, z];
                    if !self.solid.contains(&c) {
                        continue;
                    }
                    report.phantom_probes += 1;
                    if !covered.contains(&c) {
                        report.phantom_found += 1;
                    }
                }
            }
        }
        report
    }

    fn rasterize_triangle(&mut self, tri: &[[f32; 3]; 3]) {
        let mut tmin = tri[0];
        let mut tmax = tri[0];
        for v in &tri[1..] {
            for axis in 0..3 {
                tmin[axis] = tmin[axis].min(v[axis]);
                tmax[axis] = tmax[axis].max(v[axis]);
            }
        }
        let mut i0 = [0i32; 3];
        let mut i1 = [0i32; 3];
        for axis in 0..3 {
            i0[axis] = (((tmin[axis] - self.min[axis]) / self.cell).floor() as i32)
                .clamp(0, self.dims[axis] - 1);
            i1[axis] = (((tmax[axis] - self.min[axis]) / self.cell).floor() as i32)
                .clamp(0, self.dims[axis] - 1);
        }
        if triangle_axis_aligned(tri) {
            for x in i0[0]..=i1[0] {
                for y in i0[1]..=i1[1] {
                    for z in i0[2]..=i1[2] {
                        self.solid.insert([x, y, z]);
                    }
                }
            }
            return;
        }
        for x in i0[0]..=i1[0] {
            for y in i0[1]..=i1[1] {
                for z in i0[2]..=i1[2] {
                    let center = [
                        self.min[0] + (x as f32 + 0.5) * self.cell,
                        self.min[1] + (y as f32 + 0.5) * self.cell,
                        self.min[2] + (z as f32 + 0.5) * self.cell,
                    ];
                    if tri_aabb_overlap(tri, &center, self.cell * 0.5) {
                        self.solid.insert([x, y, z]);
                    }
                }
            }
        }
    }
}

/// First ray hit against the **exact geometry** (Möller–Trumbore,
/// `t ≥ 0`); `None` without a hit within `max_dist`. Use: exact floor height at
/// spawn + probe readability verdict (real wall distances).
pub fn first_geometry_hit(
    spec: &MeshSpec,
    origin: [f32; 3],
    dir: [f32; 3],
    max_dist: f32,
) -> Option<f32> {
    let len = (dir[0] * dir[0] + dir[1] * dir[1] + dir[2] * dir[2]).sqrt();
    if len <= f32::EPSILON {
        return None;
    }
    let d = [dir[0] / len, dir[1] / len, dir[2] / len];
    let mut best: Option<f32> = None;
    for tri in triangles(spec) {
        if let Some(t) = ray_triangle(origin, d, &tri) {
            if t <= max_dist && best.is_none_or(|b| t < b) {
                best = Some(t);
            }
        }
    }
    best
}

/// All ray-hit distances `t ≥ 0` against the geometry
/// (unsorted). Use: spawn — every surface of the column.
pub fn geometry_hits(
    spec: &MeshSpec,
    origin: [f32; 3],
    dir: [f32; 3],
    max_dist: f32,
) -> Vec<f32> {
    let len = (dir[0] * dir[0] + dir[1] * dir[1] + dir[2] * dir[2]).sqrt();
    if len <= f32::EPSILON {
        return Vec::new();
    }
    let d = [dir[0] / len, dir[1] / len, dir[2] / len];
    let mut hits = Vec::new();
    for tri in triangles(spec) {
        if let Some(t) = ray_triangle(origin, d, &tri) {
            if t <= max_dist {
                hits.push(t);
            }
        }
    }
    hits
}

/// Bidirectional Möller–Trumbore (t ≥ 0).
fn ray_triangle(orig: [f32; 3], dir: [f32; 3], tri: &[[f32; 3]; 3]) -> Option<f32> {
    const EPS: f32 = 1e-7;
    let e1 = sub(tri[1], tri[0]);
    let e2 = sub(tri[2], tri[0]);
    let p = cross(dir, e2);
    let det = dot(e1, p);
    if det.abs() < EPS {
        return None;
    }
    let inv = 1.0 / det;
    let tvec = sub(orig, tri[0]);
    let u = dot(tvec, p) * inv;
    if !(0.0..=1.0).contains(&u) {
        return None;
    }
    let q = cross(tvec, e1);
    let v = dot(dir, q) * inv;
    if v < 0.0 || u + v > 1.0 {
        return None;
    }
    let t = dot(e2, q) * inv;
    if t < 0.0 {
        return None;
    }
    Some(t)
}

/// Triangle iterator of the spec (`indices` in triples).
fn triangles(spec: &MeshSpec) -> impl Iterator<Item = [[f32; 3]; 3]> + '_ {
    spec.indices.chunks_exact(3).filter_map(move |idx| {
        let a = spec.positions.get(idx[0] as usize)?;
        let b = spec.positions.get(idx[1] as usize)?;
        let c = spec.positions.get(idx[2] as usize)?;
        Some([*a, *b, *c])
    })
}

/// Is the face normal axis-aligned (|n_axial| ≥ 0.999 after normalizing)?
fn triangle_axis_aligned(tri: &[[f32; 3]; 3]) -> bool {
    let e0 = sub(tri[1], tri[0]);
    let e1 = sub(tri[2], tri[0]);
    let n = cross(e0, e1);
    let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
    if len <= f32::EPSILON {
        return true; // degenerate: conservative via AABB
    }
    let max_axis = n[0].abs().max(n[1].abs()).max(n[2].abs());
    max_axis / len >= 0.999
}

/// SAT triangle↔box test (Akenine-Möller, 15 axes, inclusive with EPS).
fn tri_aabb_overlap(tri: &[[f32; 3]; 3], center: &[f32; 3], h: f32) -> bool {
    let v = [
        sub(tri[0], *center),
        sub(tri[1], *center),
        sub(tri[2], *center),
    ];
    // Box normal axes (3 tests): vertex columns per axis.
    let cols = [
        [v[0][0], v[1][0], v[2][0]],
        [v[0][1], v[1][1], v[2][1]],
        [v[0][2], v[1][2], v[2][2]],
    ];
    for col in cols {
        if col.iter().all(|&c| c > h + SAT_EPS) {
            return false;
        }
        if col.iter().all(|&c| c < -h - SAT_EPS) {
            return false;
        }
    }
    let e0 = sub(v[1], v[0]);
    let e1 = sub(v[2], v[1]);
    let n = cross(e0, e1);
    let r = h * (n[0].abs() + n[1].abs() + n[2].abs()) + SAT_EPS;
    let d = dot(n, v[0]);
    if d > r || d < -r {
        return false;
    }
    let edges = [e0, e1, sub(v[0], v[2])];
    let box_axes = [[1.0f32, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
    for edge in edges {
        for u in box_axes {
            let a = cross(edge, u);
            if dot(a, a) <= f32::EPSILON {
                continue;
            }
            let p = [dot(a, v[0]), dot(a, v[1]), dot(a, v[2])];
            let r = h * (a[0].abs() + a[1].abs() + a[2].abs()) + SAT_EPS;
            let pmin = p[0].min(p[1]).min(p[2]);
            let pmax = p[0].max(p[1]).max(p[2]);
            if pmax < -r || pmin > r {
                return false;
            }
        }
    }
    true
}

/// AABB of a triangle `[[min],[max]]`.
fn tri_aabb(tri: &[[f32; 3]; 3]) -> [[f32; 3]; 2] {
    let mut lo = tri[0];
    let mut hi = tri[0];
    for v in &tri[1..] {
        for axis in 0..3 {
            lo[axis] = lo[axis].min(v[axis]);
            hi[axis] = hi[axis].max(v[axis]);
        }
    }
    [lo, hi]
}

/// Do two AABBs `[[lo],[hi]]` overlap (inclusive)?
fn aabb_overlaps(a: &[[f32; 3]; 2], b: &[[f32; 3]; 2]) -> bool {
    (0..3).all(|axis| a[0][axis] <= b[1][axis] && b[0][axis] <= a[1][axis])
}

#[inline]
fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

#[inline]
fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

#[inline]
fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Synthetic room 8×8 m, height 3 m (floor y=0, ceiling y=3).
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
        let normals = vec![[0.0, 1.0, 0.0]; positions.len()];
        MeshSpec {
            positions,
            normals,
            indices,
            uvs: Vec::new(),
            face_textures: Vec::new(),
        }
    }

    #[test]
    fn voxelizes_room_walls_and_keeps_interior_free() {
        let grid = CollisionGrid::from_mesh_spec(&room_spec(), DEFAULT_CELL_M);
        assert!(grid.solid_cell_count() > 0);
        // Wall x=0 → layer 0 solid; wall x=8 → layer 8 (face at max bounds).
        assert!(grid.is_solid_cell([0, 1, 4]));
        assert!(grid.is_solid_cell([8, 1, 4]));
        // Free interior.
        assert_eq!(grid.classify_point([4.0, 1.5, 4.0]), CellStatus::Free);
        // Below the floor (y<0) and beyond the bounds → outside.
        assert_eq!(grid.classify_point([4.0, -0.5, 4.0]), CellStatus::Outside);
        assert_eq!(grid.classify_point([9.5, 1.5, 4.0]), CellStatus::Outside);
        // Ceiling y=3 → layer 3 solid (no clamp: dims y = 4).
        assert!(grid.is_solid_cell([4, 3, 4]));
    }

    #[test]
    fn spawn_is_inside_room_on_exact_floor() {
        let spec = room_spec();
        let grid = CollisionGrid::from_mesh_spec(&spec, DEFAULT_CELL_M);
        let spawn = grid
            .find_spawn(&spec, 1.6, 0.3)
            .expect("the room has floor and air");
        let (min, max) = spec.bounds().unwrap();
        for axis in 0..3 {
            assert!(
                spawn.eye[axis] >= min[axis] && spawn.eye[axis] <= max[axis],
                "spawn outside the block bounds"
            );
        }
        assert!(grid.is_free(spawn.eye, 0.3), "eye must be free");
        // Exact floor: vertical ray against geometry gives y=0.
        assert!((spawn.floor_y - 0.0).abs() < 1e-4, "floor_y={}", spawn.floor_y);
        assert!((spawn.eye[1] - 1.6).abs() < 1e-4, "eye={:?}", spawn.eye);
        // Grounded re-verified with geometry: floor at eye_height below the eye.
        let down = first_geometry_hit(&spec, spawn.eye, [0.0, -1.0, 0.0], 10.0)
            .expect("floor under the spawn");
        assert!((down - 1.6).abs() < 1e-3, "distance to floor={down}");
    }

    #[test]
    fn march_toward_wall_is_blocked_without_traversal() {
        let spec = room_spec();
        let grid = CollisionGrid::from_mesh_spec(&spec, DEFAULT_CELL_M);
        let spawn = grid.find_spawn(&spec, 1.6, 0.3).unwrap();
        let mut pos = spawn.eye;
        let mut blocked = None;
        for _ in 0..200 {
            let out = grid.resolve_step(pos, [0.25, 0.0, 0.0], 0.3);
            pos = out.pos;
            if let Some(status) = out.blocked {
                blocked = Some(status);
                break;
            }
            assert!(grid.is_free(pos, 0.3), "traversal detected at {pos:?}");
        }
        assert_eq!(blocked, Some(CellStatus::Solid), "blocked by geometry");
        assert!(grid.is_free(pos, 0.3));
        assert!(pos[0] < 8.0, "must not cross the wall x=8: pos={}", pos[0]);
        assert!(
            pos[0] > 6.0,
            "must advance toward the wall before blocking: pos={}",
            pos[0]
        );
    }

    #[test]
    fn open_side_of_block_confines_at_bounds() {
        // Room WITHOUT the +X wall: marching toward the open side must block
        // by confinement (Outside), not by geometry.
        let mut positions: Vec<[f32; 3]> = Vec::new();
        let mut indices: Vec<u32> = Vec::new();
        let mut quad = |a: [f32; 3], b: [f32; 3], c: [f32; 3], d: [f32; 3]| {
            let base = positions.len() as u32;
            positions.extend([a, b, c, d]);
            indices.extend([base, base + 1, base + 2, base, base + 2, base + 3]);
        };
        quad([0.0, 0.0, 0.0], [0.0, 0.0, 8.0], [8.0, 0.0, 8.0], [8.0, 0.0, 0.0]); // floor
        quad([0.0, 3.0, 0.0], [8.0, 3.0, 0.0], [8.0, 3.0, 8.0], [0.0, 3.0, 8.0]); // ceiling
        quad([0.0, 0.0, 0.0], [0.0, 3.0, 0.0], [0.0, 3.0, 8.0], [0.0, 0.0, 8.0]); // wall x=0
        quad([0.0, 0.0, 0.0], [8.0, 0.0, 0.0], [8.0, 3.0, 0.0], [0.0, 3.0, 0.0]); // wall z=0
        quad([0.0, 0.0, 8.0], [0.0, 3.0, 8.0], [8.0, 3.0, 8.0], [8.0, 0.0, 8.0]); // wall z=8
        let spec = MeshSpec {
            normals: vec![[0.0, 1.0, 0.0]; positions.len()],
            positions,
            indices,
            uvs: Vec::new(),
            face_textures: Vec::new(),
        };
        let grid = CollisionGrid::from_mesh_spec(&spec, DEFAULT_CELL_M);
        let spawn = grid.find_spawn(&spec, 1.6, 0.3).unwrap();
        // Toward the open side (+X): confinement by bounds.
        let mut pos = spawn.eye;
        let mut blocked = None;
        for _ in 0..200 {
            let out = grid.resolve_step(pos, [0.25, 0.0, 0.0], 0.3);
            pos = out.pos;
            if let Some(status) = out.blocked {
                blocked = Some(status);
                break;
            }
        }
        assert_eq!(
            blocked,
            Some(CellStatus::Outside),
            "open side: confines at bounds"
        );
        assert!(grid.is_free(pos, 0.3));
        assert!(pos[0] <= 8.0, "never leaves the block: {}", pos[0]);
        // Toward the real wall (-X): blocked by geometry.
        let mut pos2 = spawn.eye;
        let mut blocked2 = None;
        for _ in 0..200 {
            let out = grid.resolve_step(pos2, [-0.25, 0.0, 0.0], 0.3);
            pos2 = out.pos;
            if let Some(status) = out.blocked {
                blocked2 = Some(status);
                break;
            }
        }
        assert_eq!(blocked2, Some(CellStatus::Solid), "wall x=0 = geometry");
        assert!(pos2[0] >= 0.0);
    }

    #[test]
    fn raycast_from_center_hits_wall() {
        let spec = room_spec();
        let grid = CollisionGrid::from_mesh_spec(&spec, DEFAULT_CELL_M);
        let spawn = grid.find_spawn(&spec, 1.6, 0.3).unwrap();
        let hit = grid
            .raycast(spawn.eye, [1.0, 0.0, 0.0], 100.0)
            .expect("the +X wall must be hit");
        assert!(hit.dist > 0.0 && hit.dist < 100.0);
        assert_eq!(hit.cell[0], 8, "wall layer x=8 = index 8");
        // Exact-geometry ray: real distance to the wall ≈ 8 - eye.x.
        let geo = first_geometry_hit(&spec, spawn.eye, [1.0, 0.0, 0.0], 100.0)
            .expect("geometry +X");
        assert!((geo - (8.0 - spawn.eye[0])).abs() < 1e-3, "geo={geo}");
        assert!(grid.raycast(spawn.eye, [0.0, 1.0, 0.0], 100.0).is_some());
    }

    #[test]
    fn resolve_step_slides_along_wall_without_entering_solid() {
        let spec = room_spec();
        let grid = CollisionGrid::from_mesh_spec(&spec, DEFAULT_CELL_M);
        // Free point near the wall x=8 (layer 7), at mid height.
        let start = [6.6, 1.5, 4.0];
        assert_eq!(grid.classify_point(start), CellStatus::Free);
        let out = grid.resolve_step(start, [1.2, 0.0, 0.4], 0.3);
        assert!(grid.is_free(out.pos, 0.3), "resulting position always free");
        assert!(
            (out.pos[0] - start[0]).abs() < 1e-6,
            "X does not advance (blocked by wall)"
        );
        assert!(out.pos[2] > start[2], "Z sliding advances");
        assert!(out.moved);
        assert_eq!(out.blocked, Some(CellStatus::Solid));
    }

    #[test]
    fn coherence_pass_and_deterministic_on_synthetic_room() {
        let spec = room_spec();
        let a = CollisionGrid::from_mesh_spec(&spec, DEFAULT_CELL_M);
        let b = CollisionGrid::from_mesh_spec(&spec, DEFAULT_CELL_M);
        assert_eq!(a.solid_cell_count(), b.solid_cell_count(), "deterministic");
        let report = a.verify_layout_coherence(&spec);
        assert_eq!(report.centroids_missing, 0, "geometry→grid: {report:?}");
        assert_eq!(report.phantom_found, 0, "grid→geometry: {report:?}");
        assert!(report.pass(), "coherence PASS: {report:?}");
    }
}
