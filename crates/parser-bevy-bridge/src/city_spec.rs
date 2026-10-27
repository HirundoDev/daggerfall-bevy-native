//! `city_spec` — deterministic city layout + combined mesh (M3).
//!
//! **Street source (documented M3 delta, criterion (h)):** the
//! GRIDCITY spec (`city_1_2.dat`, PRE-2 §5) does NOT exist in the retail build nor in the repo
//! (MAPS.BSA 248 entries MAPPITEM.. · BLOCKS.BSA 1295 RMB/RDI · 0 "city" —
//! verified by the prior M3 pass, HH-1). The M3 context-close HH-1 authorizes
//! choosing the source in IMPLEMENT (frontmatter `decisions`: "street source
//! is decided in IMPLEMENT", 0 code, `escalation: coordinator`) and partitioning
//! declaratively; this module chooses
//! **synthetic deterministic layout over real ARCH3D**: grid of blocks
//! (64 m lots) separated by 8-16 m streets (deterministic width per gap),
//! with the first `k` convertible ARCH3D.BSA records reused as
//! *landmarks* fit-to-lot inside their blocks (Mode A, custody
//! resolved by the app — rule 3 PRE-2 §8: this pure module only receives
//! already-parsed `MeshSpec`s, 0 I/O, 0 mode, 0 persistence).
//!
//! Frozen scale (M3 HH-1): 64 u/m · 64 m block · 8-16 m street ·
//! ARCH3D fitted to the block. **Delta with respect to the design `×1/256`:**
//! the literal factor applied to the `MeshSpec` (already in meters, record 44005 measures
//! 512 m of footprint) would give an unusable 2 m landmark; the
//! intent "the ARCH3D fits in its block" is implemented as a deterministic
//! fit-to-lot scale `min(0.85·lot/hx, 0.85·lot/hz, 28 m/hy)` — the pure XZ
//! factor for the pinned record 44005 would be 64·0.85/512 ≈ 0.106, but its
//! height hy=504 m makes the cap govern (`28/504 ≈ 0.0556`, measured on
//! retail runtime: footprint 512×0.0556 = 28.44 m ≈ 28.5 in a 64 m lot ⇒
//! setbacks ≈17.8 m = (64−28.44)/2 per side, not 4.8 m) and the cap avoids
//! disproportionate towers. Delta declared in the M3 note (criterion (h)).
//!
//! M2 constants intact (reuse): 1.0 m cell, 1.60 m eye, 0.30 m radius —
//! collision is the SAME M2 [`crate::CollisionGrid`] over the combined
//! mesh (city + landmarks), without modifying `collision_grid.rs`.

use crate::mesh_spec::MeshSpec;

/// Lot/block side (m) — frozen M3 scale.
pub const CITY_BLOCK_M: f32 = 64.0;
/// Minimum street width (m).
pub const CITY_STREET_MIN_M: f32 = 8.0;
/// Maximum street width (m).
pub const CITY_STREET_MAX_M: f32 = 16.0;
/// Maximum ARCH3D landmark height after fit-to-lot (m).
pub const CITY_MAX_LANDMARK_H_M: f32 = 28.0;
/// Minimum synthetic block height (m).
pub const CITY_MIN_BUILDING_H_M: f32 = 6.0;

/// City grid: `blocks × blocks` blocks of [`CITY_BLOCK_M`] m
/// separated by deterministic-width streets (8-16 m), surrounded by a
/// perimeter street. Total determinism: same layout → same mesh byte for byte.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CityLayout {
    /// Blocks per axis (N×N).
    pub blocks: usize,
}

impl CityLayout {
    /// `blocks × blocks` city (minimum 2 so that at least one
    /// internal walkable street exists between blocks).
    pub fn new(blocks: usize) -> Self {
        Self {
            blocks: blocks.max(2),
        }
    }

    /// Street width of gap `g` (0..=blocks): deterministic within
    /// [`CITY_STREET_MIN_M`]..=[`CITY_STREET_MAX_M`] (sequence 8,13,9,14,10…).
    pub fn street_gap(&self, g: usize) -> f32 {
        CITY_STREET_MIN_M + ((g * 5) % 9) as f32
    }

    /// Deterministic height of the synthetic block (i,j): 6..14 m.
    pub fn block_height(&self, i: usize, j: usize) -> f32 {
        CITY_MIN_BUILDING_H_M + ((i * 3 + j * 5) % 9) as f32
    }

    /// Start (x) of street gap `g` (0..=blocks). Layout:
    /// `gap0 · lot0 · gap1 · lot1 · … · lotN-1 · gapN`.
    pub fn gap_start(&self, g: usize) -> f32 {
        let mut x = 0.0;
        for k in 0..g.min(self.blocks) {
            x += self.street_gap(k) + CITY_BLOCK_M;
        }
        x
    }

    /// Center (x) of the street of gap `g` (1..blocks = internal).
    pub fn gap_center(&self, g: usize) -> f32 {
        self.gap_start(g) + self.street_gap(g) * 0.5
    }

    /// Origin (x,z) of lot (i,j) (minimum corner).
    pub fn lot_origin(&self, i: usize, j: usize) -> [f32; 2] {
        [
            self.gap_start(i) + self.street_gap(i),
            self.gap_start(j) + self.street_gap(j),
        ]
    }

    /// Total city extent in meters (side of the square).
    pub fn total_m(&self) -> f32 {
        self.gap_start(self.blocks) + CITY_BLOCK_M + self.street_gap(self.blocks)
    }

    /// Is lot (i,j) a *landmark* one (carries a real ARCH3D instead of a box)?
    /// Deterministic: half of the lots, on the diagonal of even coordinates.
    pub fn is_landmark_lot(&self, i: usize, j: usize) -> bool {
        (i + j).is_multiple_of(2)
    }
}

/// City as 3 separate specs (render with 3 materials) + combined one
/// (collision). All of them share the same coordinate system.
#[derive(Debug, Clone, PartialEq)]
pub struct CityMesh {
    /// Street floor (plane y=0).
    pub ground: MeshSpec,
    /// Synthetic blocks (closed boxes).
    pub blocks: MeshSpec,
    /// Real ARCH3D landmarks fit-to-lot (empty if there are no specs).
    pub landmarks: MeshSpec,
    /// Concatenation `ground + blocks + landmarks` — input of the
    /// [`crate::CollisionGrid`] (combined-city collision M3).
    pub combined: MeshSpec,
}

impl CityMesh {
    /// Total triangle count of the combined mesh.
    pub fn triangle_count(&self) -> usize {
        self.combined.triangle_count()
    }
}

/// Builds the deterministic city: floor + box-blocks + landmarks.
///
/// `landmarks` are already-parsed ARCH3D `MeshSpec`s (Mode A, caller
/// custody); they are placed on the landmark lots in order until the
/// list runs out (the remaining lots stay as boxes). Without landmarks the city
/// is 100% synthetic (CI tests without retail).
pub fn build_city_mesh(layout: &CityLayout, landmarks: &[MeshSpec]) -> CityMesh {
    let total = layout.total_m();

    // Floor: y=0 plane covering the whole extent (2 triangles).
    let mut ground_positions = Vec::with_capacity(4);
    let mut ground_normals = Vec::with_capacity(4);
    let mut ground_indices = Vec::with_capacity(6);
    push_quad(
        &mut ground_positions,
        &mut ground_normals,
        &mut ground_indices,
        [0.0, 1.0, 0.0],
        [0.0, 0.0, 0.0],
        [total, 0.0, 0.0],
        [total, 0.0, total],
        [0.0, 0.0, total],
    );
    let ground = MeshSpec::new(ground_positions, ground_normals, ground_indices);

    // Blocks + landmarks (on the landmark lots with an available spec).
    let mut block_positions = Vec::new();
    let mut block_normals = Vec::new();
    let mut block_indices = Vec::new();
    let mut landmark_specs: Vec<&MeshSpec> = landmarks.iter().collect();
    let mut landmark_positions = Vec::new();
    let mut landmark_normals = Vec::new();
    let mut landmark_indices = Vec::new();
    let mut landmark_uvs = Vec::new();
    let mut landmark_textures = Vec::new();

    for j in 0..layout.blocks {
        for i in 0..layout.blocks {
            let origin = layout.lot_origin(i, j);
            if layout.is_landmark_lot(i, j) {
                if let Some(spec) = landmark_specs.first().copied() {
                    landmark_specs.remove(0);
                    push_transformed(
                        &mut landmark_positions,
                        &mut landmark_normals,
                        &mut landmark_indices,
                        &mut landmark_uvs,
                        &mut landmark_textures,
                        spec,
                        origin,
                    );
                    continue;
                }
            }
            let height = layout.block_height(i, j);
            push_box(
                &mut block_positions,
                &mut block_normals,
                &mut block_indices,
                [origin[0], 0.0, origin[1]],
                CITY_BLOCK_M,
                height,
            );
        }
    }

    let blocks = MeshSpec::new(block_positions, block_normals, block_indices);
    // Landmarks keep their original flat normals (flat-shaded M1) and
    // their ARCH3D UVs/textures if they carry them (M12).
    let landmarks = MeshSpec {
        positions: landmark_positions,
        normals: landmark_normals,
        indices: landmark_indices,
        uvs: landmark_uvs,
        face_textures: landmark_textures,
    };

    let combined = concat_specs(&ground, &blocks, &landmarks);
    CityMesh {
        ground,
        blocks,
        landmarks,
        combined,
    }
}

/// Spawn point on the internal `gap` street (1..blocks), at the height
/// of the center of the first row of blocks (not at the intersection: the
/// street's X axes stay flanked by facades, as in a real
/// street between blocks): `([street_center_x, 0.0, lot_z], name)`. The
/// caller raises the eye `EYE_HEIGHT_M` above the floor (y=0 by
/// construction).
pub fn street_spawn(layout: &CityLayout, gap: usize) -> ([f32; 3], String) {
    let g = gap.clamp(1, layout.blocks - 1);
    let x = layout.gap_center(g);
    let z = layout.lot_origin(0, 0)[1] + CITY_BLOCK_M * 0.5;
    (
        [x, 0.0, z],
        format!("Street-{g} (gap x={x:.1} m, width {:.0} m)", layout.street_gap(g)),
    )
}

/// Concatenates 3 specs into one (index offsets corrected). The UVs and
/// texture keys (M12) are concatenated with defaults for the synthetic
/// specs, preserving the 1:1 alignment with `positions`.
fn concat_specs(a: &MeshSpec, b: &MeshSpec, c: &MeshSpec) -> MeshSpec {
    let mut positions = Vec::with_capacity(a.positions.len() + b.positions.len() + c.positions.len());
    let mut indices =
        Vec::with_capacity(a.indices.len() + b.indices.len() + c.indices.len());
    let mut uvs = Vec::with_capacity(positions.capacity());
    let mut face_textures = Vec::with_capacity(positions.capacity());
    for spec in [a, b, c] {
        let base = positions.len() as u32;
        positions.extend_from_slice(&spec.positions);
        indices.extend(spec.indices.iter().map(|idx| idx + base));
        if spec.uvs.len() == spec.positions.len() && spec.face_textures.len() == spec.positions.len() {
            uvs.extend_from_slice(&spec.uvs);
            face_textures.extend_from_slice(&spec.face_textures);
        } else {
            uvs.extend(std::iter::repeat_n([0.0, 0.0], spec.positions.len()));
            face_textures.extend(std::iter::repeat_n(crate::mesh_spec::NO_TEXTURE, spec.positions.len()));
        }
    }
    let normals = vec![[0.0, 1.0, 0.0]; positions.len()];
    MeshSpec {
        positions,
        normals,
        indices,
        uvs,
        face_textures,
    }
}

/// Quad with a flat per-face normal (4 vertices, 2 triangles, CCW winding).
///
/// 8 args by structural shape (3 buffers + normal + 4 corners) — the
/// pusher is internal to `city_spec` and its shape is deliberate (clippy
/// gate `-D warnings` M3, documented allow). `pub(crate)` since M4
/// so that `interior_spec` reuses the same pusher (0 duplication,
/// 0 behavior change).
#[allow(clippy::too_many_arguments)]
pub(crate) fn push_quad(
    positions: &mut Vec<[f32; 3]>,
    normals: &mut Vec<[f32; 3]>,
    indices: &mut Vec<u32>,
    normal: [f32; 3],
    a: [f32; 3],
    b: [f32; 3],
    c: [f32; 3],
    d: [f32; 3],
) {
    let base = positions.len() as u32;
    positions.extend([a, b, c, d]);
    normals.extend([normal; 4]);
    indices.extend([base, base + 1, base + 2, base, base + 2, base + 3]);
}

/// Closed box (6 faces, 12 triangles) with minimum corner at `min`.
fn push_box(
    positions: &mut Vec<[f32; 3]>,
    normals: &mut Vec<[f32; 3]>,
    indices: &mut Vec<u32>,
    min: [f32; 3],
    side: f32,
    height: f32,
) {
    let [x, y, z] = min;
    let (x1, y1, z1) = (x + side, y + height, z + side);
    // Bottom face (normal -Y) — keeps the floor ray from falling into the void.
    push_quad(positions, normals, indices, [0.0, -1.0, 0.0], [x, y, z1], [x1, y, z1], [x1, y, z], [x, y, z]);
    // Top (+Y).
    push_quad(positions, normals, indices, [0.0, 1.0, 0.0], [x, y1, z], [x1, y1, z], [x1, y1, z1], [x, y1, z1]);
    // -Z.
    push_quad(positions, normals, indices, [0.0, 0.0, -1.0], [x1, y, z], [x, y, z], [x, y1, z], [x1, y1, z]);
    // +Z.
    push_quad(positions, normals, indices, [0.0, 0.0, 1.0], [x, y, z1], [x1, y, z1], [x1, y1, z1], [x, y1, z1]);
    // -X.
    push_quad(positions, normals, indices, [-1.0, 0.0, 0.0], [x, y, z], [x, y, z1], [x, y1, z1], [x, y1, z]);
    // +X.
    push_quad(positions, normals, indices, [1.0, 0.0, 0.0], [x1, y, z1], [x1, y, z], [x1, y1, z], [x1, y1, z1]);
}

/// Sampling step (WORLD meters fit-to-lot) of the landmark solidity test
/// over each AABB face — 0.5 m ≪ 1.0 m probe cell: detects
/// face gaps from ~1.1 m with the `PLAYER_RADIUS_M` sphere.
const LANDMARK_FACE_STEP_M: f32 = 0.5;

/// Deterministic fit-to-lot scale of the landmark (documented in the module
/// header): `min(0.85·lot/hx, 0.85·lot/hz, 28 m/hy)`.
///
/// **Single source of the transformation**: it is used by [`push_transformed`]
/// (the city) and [`landmark_is_solid`] (the landmark selection) — the
/// evaluation frame and the scale cannot diverge.
pub(crate) fn fit_to_lot_scale(spec: &MeshSpec) -> Option<f32> {
    let (min, max) = spec.bounds()?;
    let (hx, hz) = (max[0] - min[0], max[2] - min[2]);
    if !(hx > 0.0 && hz > 0.0) {
        return None;
    }
    let hy = (max[1] - min[1]).max(1e-3);
    Some(
        (CITY_BLOCK_M * 0.85 / hx)
            .min(CITY_BLOCK_M * 0.85 / hz)
            .min(CITY_MAX_LANDMARK_H_M / hy),
    )
}

/// Is the block **solid** (closed) at the player's eye height in the
/// WORLD fit-to-lot, from the 4 cardinal faces? — city landmark selection
/// criterion (§5a).
///
/// It is evaluated **in the placed world** (scale `fit_to_lot_scale`, seated
/// at y=0): eye `EYE_HEIGHT_M` and sphere `PLAYER_RADIUS_M` converted to
/// local meters (`/scale`), mesh cell `DEFAULT_CELL_M/scale` = 1 m of
/// world (same granularity as the probe mesh over the combined
/// city) and face sampling every `LANDMARK_FACE_STEP_M` world m.
/// In raw local meters y=1.6 does not mean the same (a 3.15 m model
/// scaled ×8.89 has the local eye at 0.18 — R-M10-FIN warning).
///
/// Criterion: for each of the 4 AABB faces, at **no** point of the
/// face line does the player sphere fit. If some face admits the sphere,
/// the block is OPEN and a fit-to-lot landmark leaves a passage through
/// which the flank ray and the probe `r=0.3` walk thread it.
///
/// **Why the test subject changed (guardrail §5a.3 — 0 relaxation of
/// the M3 checks):** the previous calibration of `manzana_collision` /
/// `street_legibility` was done with inverted landmarks: the first
/// convertible `44005` (OPEN 11 m passage along the `+X` axis at `z=40`,
/// exactly the `street_spawn` eye height) escaped both the flank ray and
/// the `r=0.3` walk. Evidence: the HH-1 M10 partial note.
///
/// Returns `false` (not eligible) if the spec has no usable
/// bounds/scale or if its world height does not reach the eye with its
/// radius.
pub fn landmark_is_solid(spec: &MeshSpec) -> bool {
    use crate::collision_grid::{CollisionGrid, DEFAULT_CELL_M};
    use crate::view_probe::{EYE_HEIGHT_M, PLAYER_RADIUS_M};

    let Some((min, max)) = spec.bounds() else {
        return false;
    };
    let Some(scale) = fit_to_lot_scale(spec) else {
        return false;
    };
    let eye = min[1] + EYE_HEIGHT_M / scale;
    let r = PLAYER_RADIUS_M / scale;
    if eye + r > max[1] {
        return false;
    }
    let grid = CollisionGrid::from_mesh_spec(spec, DEFAULT_CELL_M / scale);
    let step = LANDMARK_FACE_STEP_M / scale;
    // ±X faces: the face line runs along z.
    let mut t = min[2] + r;
    while t <= max[2] - r {
        if grid.is_free([min[0] + r, eye, t], r) || grid.is_free([max[0] - r, eye, t], r) {
            return false;
        }
        t += step;
    }
    // ±Z faces: the face line runs along x.
    let mut t = min[0] + r;
    while t <= max[0] - r {
        if grid.is_free([t, eye, min[2] + r], r) || grid.is_free([t, eye, max[2] - r], r) {
            return false;
        }
        t += step;
    }
    true
}

/// Copies a spec inside the lot (origin): deterministic fit-to-lot scale
/// (documented in the header) and seated at y=0, centered on the lot.
/// Returns 0 triangles if the spec has no usable bounds. The UVs and
/// ARCH3D texture keys (M12) are copied per vertex unchanged — the
/// scale/rotation does not affect the texture mapping.
fn push_transformed(
    positions: &mut Vec<[f32; 3]>,
    normals: &mut Vec<[f32; 3]>,
    indices: &mut Vec<u32>,
    uvs: &mut Vec<[f32; 2]>,
    face_textures: &mut Vec<u32>,
    spec: &MeshSpec,
    origin: [f32; 2],
) {
    let Some((min, max)) = spec.bounds() else {
        return;
    };
    let Some(scale) = fit_to_lot_scale(spec) else {
        return;
    };
    let center_x = (min[0] + max[0]) * 0.5;
    let center_z = (min[2] + max[2]) * 0.5;
    let lot_center = [origin[0] + CITY_BLOCK_M * 0.5, origin[1] + CITY_BLOCK_M * 0.5];
    let base = positions.len() as u32;
    for (p, n) in spec.positions.iter().zip(spec.normals.iter()) {
        positions.push([
            lot_center[0] + (p[0] - center_x) * scale,
            (p[1] - min[1]) * scale,
            lot_center[1] + (p[2] - center_z) * scale,
        ]);
        // Uniform scale + translation: the normal does not change (only
        // position translation; the scale is uniform and positive).
        normals.push(*n);
    }
    // UVs/textures (M12): per-vertex copy; without data (synthetic spec) it
    // is filled with defaults to preserve the 1:1 alignment with positions.
    if spec.uvs.len() == spec.positions.len() && spec.face_textures.len() == spec.positions.len() {
        uvs.extend_from_slice(&spec.uvs);
        face_textures.extend_from_slice(&spec.face_textures);
    } else {
        uvs.extend(std::iter::repeat_n([0.0, 0.0], spec.positions.len()));
        face_textures.extend(std::iter::repeat_n(crate::mesh_spec::NO_TEXTURE, spec.positions.len()));
    }
    indices.extend(spec.indices.iter().map(|idx| idx + base));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn street_widths_are_in_frozen_range() {
        let layout = CityLayout::new(4);
        for g in 0..=layout.blocks {
            let w = layout.street_gap(g);
            assert!(
                (CITY_STREET_MIN_M..=CITY_STREET_MAX_M).contains(&w),
                "gap {g}: width {w} outside 8..=16 m"
            );
        }
    }

    #[test]
    fn layout_is_deterministic() {
        let a = CityLayout::new(3);
        let b = CityLayout::new(3);
        assert_eq!(a, b);
        assert_eq!(a.total_m(), b.total_m());
        assert_eq!(a.gap_center(1), b.gap_center(1));
    }

    #[test]
    fn city_mesh_is_deterministic_and_combined_is_concat() {
        let layout = CityLayout::new(2);
        let city_a = build_city_mesh(&layout, &[]);
        let city_b = build_city_mesh(&layout, &[]);
        assert_eq!(city_a, city_b, "same input → same mesh (determinism)");
        assert_eq!(
            city_a.combined.triangle_count(),
            city_a.ground.triangle_count()
                + city_a.blocks.triangle_count()
                + city_a.landmarks.triangle_count(),
            "combined = ground + blocks + landmarks"
        );
        assert!(city_a.ground.triangle_count() >= 2, "floor present");
        assert!(city_a.blocks.triangle_count() > 0, "blocks present");
    }

    #[test]
    fn city_mesh_bounds_match_layout() {
        let layout = CityLayout::new(2);
        let city = build_city_mesh(&layout, &[]);
        let (min, max) = city.combined.bounds().expect("non-empty mesh");
        let total = layout.total_m();
        for axis in [0usize, 2] {
            assert!((min[axis] - 0.0).abs() < 1e-4, "min[{axis}] = {}", min[axis]);
            assert!(
                (max[axis] - total).abs() < 1e-4,
                "max[{axis}] = {} ≠ total {total}",
                max[axis]
            );
        }
        assert!(max[1] > 0.0, "blocks with height > 0");
    }

    #[test]
    fn landmark_fits_lot_when_spec_provided() {
        let layout = CityLayout::new(2);
        // Large cubic synthetic spec (512 m footprint, 100 m tall) — analogous
        // to the pinned record 44005; fit-to-lot must shrink it to the lot.
        let s = 256.0f32;
        let h = 100.0f32;
        let mut positions = Vec::new();
        let mut normals = Vec::new();
        let mut indices = Vec::new();
        // 6 cube faces (quads).
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
        let spec = MeshSpec::new(positions, normals, indices);
        let city = build_city_mesh(&layout, std::slice::from_ref(&spec));
        assert!(
            city.landmarks.triangle_count() > 0,
            "landmark placed on a landmark lot"
        );
        let (min, max) = city.landmarks.bounds().expect("non-empty landmark");
        let width = max[0] - min[0];
        assert!(
            width <= CITY_BLOCK_M,
            "landmark width {width} m exceeds the block {CITY_BLOCK_M} m"
        );
        assert!(
            max[1] <= CITY_MAX_LANDMARK_H_M + 1e-3,
            "landmark height {} m exceeds the cap {CITY_MAX_LANDMARK_H_M} m",
            max[1]
        );
        assert!(min[1].abs() < 1e-3, "landmark seated at y=0");
    }

    #[test]
    fn street_spawn_is_on_a_street() {
        let layout = CityLayout::new(2);
        let (eye, name) = street_spawn(&layout, 1);
        let x = eye[0];
        let gap_lo = layout.gap_start(1);
        let gap_hi = gap_lo + layout.street_gap(1);
        assert!(
            x > gap_lo && x < gap_hi,
            "spawn x={x} outside the street [{gap_lo},{gap_hi}]"
        );
        assert!(name.contains("Street-1"), "deterministic street name: {name}");
    }
}
