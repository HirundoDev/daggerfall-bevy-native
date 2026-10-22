//! `interior_spec` — navigable shop interior (M4).
//!
//! **Source (correction 2026-10-20, WI-3):** the
//! M4 pack expected a `DFON1.BSA` block from the GOG retail. That file **does
//! not exist** in any Daggerfall distribution (0 hits
//! across 5 distributions, in `ARENA2` and in the DFU corpus); the **real
//! interiors** are assembled from `.RDB` (BLOCKS.BSA) + `ARCH3D.BSA` — see
//! [`crate::interior_retail`] (M11), which is the interior rendered by the
//! M4 window. This module remains as the probe's **synthetic block A**
//! (deterministic, 0 I/O, basis of the historical M3 delta) and is **not** the
//! source of the window world: 0 assets in the repo, 0 `.BSA` read by this piece.
//!
//! **Placement (design decision, verified against M3):** the shop is a
//! NEW building inside the **west** perimeter street of the M3 city
//! (`x ∈ [0,5]`, `z ∈ [10,26]` — `street_gap(0) = 8 m` always). Being
//! **inside the city bounds**, the combined world AABB is
//! identical to M3's (same [`crate::CollisionGrid`] `dims`/bounds → 0
//! regression of the probe's generic layer, 0 walkable "void" outside
//! bounds) and **no** M1-M3 file is touched (`build_city_mesh` intact:
//! PARITY-FIRST). The free lane `x ∈ [6,8)` (2 cells of `1 m`) separates the
//! shop's east wall (`x=5` → solid layer 5) from the blocks (`x≥8` →
//! solid layer 8+).
//!
//! **Door:** opening `z ∈ [16.8,19.2]` with a lintel above `y=2.1` in the east
//! wall — with a `1 m` cell the effective passage is layers `z=17,18`
//! (2 m) and the eye (`1.6 ± 0.3 → layer y=1`) passes under the lintel (layer 2).
//!
//! Rules: axis-aligned geometry only (grid AABB rasterization →
//! INV-L99-5 coherence without phantoms), **solid props ≥ 1.4 m** (they must
//! occupy the eye layer `y=1` to truly block — AB-06) and 0 persistence
//! (runtime-only, PRE-2 §8).

use crate::city_spec::{push_quad, CityMesh};
use crate::mesh_spec::MeshSpec;

/// Shop X origin (west wall, at the city edge).
pub const SHOP_X_MIN: f32 = 0.0;
/// Shop X end (east wall with the door).
pub const SHOP_X_MAX: f32 = 5.0;
/// Shop Z start (south rear wall).
pub const SHOP_Z_MIN: f32 = 10.0;
/// Shop Z end (north rear wall).
pub const SHOP_Z_MAX: f32 = 26.0;
/// Enclosure height (m).
pub const SHOP_HEIGHT_M: f32 = 2.6;
/// Door opening start (z).
pub const DOOR_Z_MIN: f32 = 16.8;
/// Door opening end (z).
pub const DOOR_Z_MAX: f32 = 19.2;
/// Clear height of the opening (lintel above).
pub const DOOR_HEIGHT_M: f32 = 2.1;
/// Minimum height of a solid prop (m): it must reach the eye layer `y=1`.
pub const FIXTURE_MIN_H_M: f32 = 1.4;
/// Interior floor height (mm above the city ground — same cell-0 layer).
pub const FLOOR_Y: f32 = 0.02;

/// Shop as separate specs (rendered with 3 materials) + combined
/// (collision). Shares the coordinate system with [`CityMesh`].
#[derive(Debug, Clone, PartialEq)]
pub struct ShopMesh {
    /// Enclosure: 4 walls (with a door opening) + lintel + ceiling.
    pub shell: MeshSpec,
    /// Interior floor (visual slab over the city ground).
    pub floor: MeshSpec,
    /// Solid interior props (counter, 2 shelves, island).
    pub fixtures: MeshSpec,
    /// Concatenation `shell + floor + fixtures` — grid input over the
    /// combined M4 world.
    pub combined: MeshSpec,
}

impl ShopMesh {
    /// Total triangle count of the combined spec.
    pub fn triangle_count(&self) -> usize {
        self.combined.triangle_count()
    }

    /// Center of the door opening on Z.
    pub fn door_center_z(&self) -> f32 {
        (DOOR_Z_MIN + DOOR_Z_MAX) * 0.5
    }

    /// Is the point in the shop **interior** (exterior→interior region)?
    ///
    /// Strict insets on X/Z (walls) and ceiling on Y: a point in the lane
    /// or in the door opening (x>5) counts as exterior.
    pub fn contains_interior(&self, p: [f32; 3]) -> bool {
        p[0] > SHOP_X_MIN
            && p[0] < SHOP_X_MAX
            && p[1] >= 0.0
            && p[1] < SHOP_HEIGHT_M
            && p[2] > SHOP_Z_MIN
            && p[2] < SHOP_Z_MAX
    }
}

/// Builds the M4 shop (deterministic, synthetic — 0 I/O, 0 assets).
pub fn build_shop_mesh() -> ShopMesh {
    let (x0, x1) = (SHOP_X_MIN, SHOP_X_MAX);
    let (z0, z1) = (SHOP_Z_MIN, SHOP_Z_MAX);
    let h = SHOP_HEIGHT_M;

    // ── Enclosure (normals pointing inward; cull None + double_sided
    //    material also makes the outer faces visible) ────────────────────
    let mut shell_pos = Vec::new();
    let mut shell_nrm = Vec::new();
    let mut shell_idx = Vec::new();

    // West wall (x=0), inner face +X.
    push_quad(
        &mut shell_pos,
        &mut shell_nrm,
        &mut shell_idx,
        [1.0, 0.0, 0.0],
        [x0, 0.0, z1],
        [x0, 0.0, z0],
        [x0, h, z0],
        [x0, h, z1],
    );
    // East wall, south span (up to the opening), inner face -X.
    push_quad(
        &mut shell_pos,
        &mut shell_nrm,
        &mut shell_idx,
        [-1.0, 0.0, 0.0],
        [x1, 0.0, z0],
        [x1, 0.0, DOOR_Z_MIN],
        [x1, h, DOOR_Z_MIN],
        [x1, h, z0],
    );
    // East wall, north span (from the opening).
    push_quad(
        &mut shell_pos,
        &mut shell_nrm,
        &mut shell_idx,
        [-1.0, 0.0, 0.0],
        [x1, 0.0, DOOR_Z_MAX],
        [x1, 0.0, z1],
        [x1, h, z1],
        [x1, h, DOOR_Z_MAX],
    );
    // Lintel above the opening (lets the eye through at 1.6 m).
    push_quad(
        &mut shell_pos,
        &mut shell_nrm,
        &mut shell_idx,
        [-1.0, 0.0, 0.0],
        [x1, DOOR_HEIGHT_M, DOOR_Z_MIN],
        [x1, DOOR_HEIGHT_M, DOOR_Z_MAX],
        [x1, h, DOOR_Z_MAX],
        [x1, h, DOOR_Z_MIN],
    );
    // South wall (z0).
    push_quad(
        &mut shell_pos,
        &mut shell_nrm,
        &mut shell_idx,
        [0.0, 0.0, 1.0],
        [x0, 0.0, z0],
        [x1, 0.0, z0],
        [x1, h, z0],
        [x0, h, z0],
    );
    // North wall (z1).
    push_quad(
        &mut shell_pos,
        &mut shell_nrm,
        &mut shell_idx,
        [0.0, 0.0, -1.0],
        [x0, 0.0, z1],
        [x1, 0.0, z1],
        [x1, h, z1],
        [x0, h, z1],
    );
    // Ceiling (face seen from the inside, -Y).
    push_quad(
        &mut shell_pos,
        &mut shell_nrm,
        &mut shell_idx,
        [0.0, -1.0, 0.0],
        [x0, h, z0],
        [x1, h, z0],
        [x1, h, z1],
        [x0, h, z1],
    );
    let shell = MeshSpec {
        positions: shell_pos,
        normals: shell_nrm,
        indices: shell_idx,
        // M12: synthetic shop (M4/M5) — no retail texture.
        uvs: Vec::new(),
        face_textures: Vec::new(),
    };

    // ── Interior floor (visual slab; the city ground already provides collision) ─
    let mut floor_pos = Vec::new();
    let mut floor_nrm = Vec::new();
    let mut floor_idx = Vec::new();
    push_quad(
        &mut floor_pos,
        &mut floor_nrm,
        &mut floor_idx,
        [0.0, 1.0, 0.0],
        [x0, FLOOR_Y, z0],
        [x1, FLOOR_Y, z0],
        [x1, FLOOR_Y, z1],
        [x0, FLOOR_Y, z1],
    );
    let floor = MeshSpec {
        positions: floor_pos,
        normals: floor_nrm,
        indices: floor_idx,
        uvs: Vec::new(),
        face_textures: Vec::new(),
    };

    // ── Solid props (≥ FIXTURE_MIN_H_M: they occupy the eye layer y=1) ───
    let mut fixture_pos = Vec::new();
    let mut fixture_nrm = Vec::new();
    let mut fixture_idx = Vec::new();
    // Counter next to the west wall (leaves a south aisle along the corridor).
    push_rect_box(
        &mut fixture_pos,
        &mut fixture_nrm,
        &mut fixture_idx,
        [1.0, 0.0, 12.0],
        [1.4, 1.4, 4.0],
    );
    // Shelf against the south wall.
    push_rect_box(
        &mut fixture_pos,
        &mut fixture_nrm,
        &mut fixture_idx,
        [1.0, 0.0, 10.4],
        [3.6, 2.0, 1.0],
    );
    // Shelf against the north wall.
    push_rect_box(
        &mut fixture_pos,
        &mut fixture_nrm,
        &mut fixture_idx,
        [1.0, 0.0, 24.6],
        [3.6, 2.0, 1.0],
    );
    // Central island.
    push_rect_box(
        &mut fixture_pos,
        &mut fixture_nrm,
        &mut fixture_idx,
        [3.0, 0.0, 13.0],
        [1.5, 1.4, 2.0],
    );
    let fixtures = MeshSpec {
        positions: fixture_pos,
        normals: fixture_nrm,
        indices: fixture_idx,
        uvs: Vec::new(),
        face_textures: Vec::new(),
    };

    let combined = concat_specs(&[&shell, &floor, &fixtures]);
    ShopMesh {
        shell,
        floor,
        fixtures,
        combined,
    }
}

/// Combined M4 world: M3 city (`combined`) + shop (`combined`) — the
/// `CollisionGrid` input shared by the headless probe and the window.
pub fn world_mesh(city: &CityMesh, shop: &ShopMesh) -> MeshSpec {
    concat_specs(&[&city.combined, &shop.combined])
}

/// Closed rectangular box (6 faces) with minimum corner at `min`.
///
/// Variant of `city_spec::push_box` with independent sides (counter
/// 1.4×1.4×4, shelves 3.6×2×1, island 1.5×1.4×2); reuses
/// [`push_quad`] (per-face normals, same M3 pusher). `pub(crate)`
/// since M5: the merchant box (`commerce_spec`) reuses the same
/// pusher (0 geometry duplication).
pub(crate) fn push_rect_box(
    positions: &mut Vec<[f32; 3]>,
    normals: &mut Vec<[f32; 3]>,
    indices: &mut Vec<u32>,
    min: [f32; 3],
    size: [f32; 3],
) {
    let [x, y, z] = min;
    let (x1, y1, z1) = (x + size[0], y + size[1], z + size[2]);
    // Bottom (-Y) and top (+Y).
    push_quad(positions, normals, indices, [0.0, -1.0, 0.0], [x, y, z1], [x1, y, z1], [x1, y, z], [x, y, z]);
    push_quad(positions, normals, indices, [0.0, 1.0, 0.0], [x, y1, z], [x1, y1, z], [x1, y1, z1], [x, y1, z1]);
    // -Z and +Z.
    push_quad(positions, normals, indices, [0.0, 0.0, -1.0], [x1, y, z], [x, y, z], [x, y1, z], [x1, y1, z]);
    push_quad(positions, normals, indices, [0.0, 0.0, 1.0], [x, y, z1], [x1, y, z1], [x1, y1, z1], [x, y1, z1]);
    // -X and +X.
    push_quad(positions, normals, indices, [-1.0, 0.0, 0.0], [x, y, z], [x, y, z1], [x, y1, z1], [x, y1, z]);
    push_quad(positions, normals, indices, [1.0, 0.0, 0.0], [x1, y, z1], [x1, y, z], [x1, y1, z], [x1, y1, z1]);
}

/// Concatenates N specs preserving positions/normals/indices (corrected
/// index offsets). `pub(crate)` since M5: `commerce_spec::merchant_world`
/// concatenates the M4 world + merchant box.
pub(crate) fn concat_specs(specs: &[&MeshSpec]) -> MeshSpec {
    let total: usize = specs.iter().map(|spec| spec.positions.len()).sum();
    let mut positions = Vec::with_capacity(total);
    let mut normals = Vec::with_capacity(total);
    let mut indices: Vec<u32> = Vec::new();
    let mut uvs = Vec::with_capacity(total);
    let mut face_textures = Vec::with_capacity(total);
    for spec in specs {
        let base = positions.len() as u32;
        positions.extend_from_slice(&spec.positions);
        normals.extend_from_slice(&spec.normals);
        indices.extend(spec.indices.iter().map(|idx| idx + base));
        // UVs/textures (M12): per-vertex copy if the spec brings them
        // aligned 1:1; otherwise (synthetic mesh) fill defaults to
        // preserve the [`MeshSpec::texture_alignment_valid`] invariant.
        if spec.uvs.len() == spec.positions.len() && spec.face_textures.len() == spec.positions.len() {
            uvs.extend_from_slice(&spec.uvs);
            face_textures.extend_from_slice(&spec.face_textures);
        } else {
            uvs.extend(std::iter::repeat_n([0.0, 0.0], spec.positions.len()));
            face_textures.extend(std::iter::repeat_n(
                crate::mesh_spec::NO_TEXTURE,
                spec.positions.len(),
            ));
        }
    }
    MeshSpec {
        positions,
        normals,
        indices,
        uvs,
        face_textures,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collision_grid::{CellStatus, CollisionGrid, DEFAULT_CELL_M};

    #[test]
    fn shop_shell_has_door_hole_and_closed_walls() {
        let shop = build_shop_mesh();
        assert!(shop.triangle_count() > 0);
        let grid = CollisionGrid::from_mesh_spec(&shop.combined, DEFAULT_CELL_M);
        // Sampling at the wall layer: the shop AABB reaches
        // x=5 (the point stays inside bounds, strict `classify_point`).
        // Solid outside the opening, free in the opening (layers 17/18).
        assert_eq!(grid.classify_point([SHOP_X_MAX, 1.6, 13.0]), CellStatus::Solid);
        assert_eq!(grid.classify_point([SHOP_X_MAX, 1.6, 24.0]), CellStatus::Solid);
        assert_eq!(grid.classify_point([SHOP_X_MAX, 1.6, 17.5]), CellStatus::Free);
        assert_eq!(grid.classify_point([SHOP_X_MAX, 1.6, 18.5]), CellStatus::Free);
        // Lintel: the opening's ceiling layer is solid (upper closure).
        assert_eq!(grid.classify_point([SHOP_X_MAX, 2.3, 18.0]), CellStatus::Solid);
        // South/north/west walls closed at eye height.
        assert_eq!(grid.classify_point([3.0, 1.6, 10.5]), CellStatus::Solid);
        assert_eq!(grid.classify_point([3.0, 1.6, 25.5]), CellStatus::Solid);
        assert_eq!(grid.classify_point([0.5, 1.6, 18.0]), CellStatus::Solid);
        // Interior free at eye height (central aisle with no props).
        assert_eq!(grid.classify_point([3.5, 1.6, 21.0]), CellStatus::Free);
    }

    #[test]
    fn fixtures_are_solid_at_eye_layer() {
        let shop = build_shop_mesh();
        let grid = CollisionGrid::from_mesh_spec(&shop.combined, DEFAULT_CELL_M);
        for (label, point) in [
            ("counter", [1.7, 1.6, 14.0]),
            ("shelf_south", [2.8, 1.6, 11.0]),
            ("shelf_north", [2.8, 1.6, 25.0]),
            ("island", [3.7, 1.6, 14.0]),
        ] {
            assert_eq!(
                grid.classify_point(point),
                CellStatus::Solid,
                "{label} must be solid at eye height (props ≥ {FIXTURE_MIN_H_M} m)"
            );
        }
    }

    #[test]
    fn interior_region_is_strict() {
        let shop = build_shop_mesh();
        assert!(shop.contains_interior([3.0, 1.6, 20.0]));
        assert!(!shop.contains_interior([6.0, 1.6, 20.0]), "lane = exterior");
        assert!(!shop.contains_interior([3.0, 1.6, 8.0]), "outside in z");
        assert!(!shop.contains_interior([3.0, 3.5, 20.0]), "above the ceiling");
    }

    #[test]
    fn build_is_deterministic() {
        let a = build_shop_mesh();
        let b = build_shop_mesh();
        assert_eq!(a, b, "same input → same mesh byte by byte");
    }
}
