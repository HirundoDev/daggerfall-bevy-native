//! `commerce_spec` — merchant NPC (solid prop) + **raycast
//! interaction** (M5).
//!
//! The merchant is a **solid axis-aligned AABB box** (same family of
//! props as M4: they occupy the eye layer ⇒ they truly block — AB-06:
//! 0 non-solid props, 0 billboards/sprites). The M5 grid =
//! [`crate::world_mesh`] + [`merchant_world`]; `build_shop_mesh` and
//! the M4 `world_mesh` remain intact (PARITY-FIRST: PROBE-INTERIOR-001 runs
//! over the world without the merchant, with no regression).
//!
//! The interaction (E key in the window) is resolved with the **same
//! `CollisionGrid::raycast`** the probes exercise: a target is
//! valid if the first solid hit is a merchant cell and the
//! chest is in range ([`INTERACT_RANGE_M`] — wall/range in between ⇒ 0).
//! PRE-2 §8 rules: 0 mode logic · 0 persistence · 0 winit types —
//! the M5 window maps the E key onto this pure primitive.

use crate::collision_grid::CollisionGrid;
use crate::interior_spec::{concat_specs, push_rect_box};
use crate::mesh_spec::MeshSpec;

/// Interaction range (m) — merchant chest to player eye.
pub const INTERACT_RANGE_M: f32 = 6.0;
/// Comparison eps for the range (avoids an exact float edge).
const RANGE_EPS_M: f32 = 1e-3;

/// Axis-aligned box of the merchant NPC (minimum corner + size, m).
#[derive(Debug, Clone, PartialEq)]
pub struct MerchantProp {
    /// Minimum corner `[x, y, z]` of the box.
    pub min: [f32; 3],
    /// Size `[w, h, d]` of the box (h ≥ 1.4 ⇒ solid eye layer).
    pub size: [f32; 3],
}

impl MerchantProp {
    /// Chest point (X/Z center, 80% of the height — eye layer `y=1`
    /// with the canonical 1.9 m height): target of the interaction raycast.
    pub fn chest(&self) -> [f32; 3] {
        [
            self.min[0] + self.size[0] * 0.5,
            self.min[1] + self.size[1] * 0.8,
            self.min[2] + self.size[2] * 0.5,
        ]
    }

    /// Grid cells occupied by the box (cartesian product of cells
    /// overlapped per axis; the grid's own `cell_of` ⇒ coherent with its
    /// origin/dimensions).
    ///
    /// **Parity with the grid rasterization** (measured in M5): axis-aligned
    /// faces mark the cell range of the triangle AABB with the
    /// **inclusive** plane (a face at `x=2.0` falls in cell `2`), so the
    /// AABB maximum is taken INCLUSIVE (`cell_of(max)`, no eps): the
    /// box `[1,2]×[0,1.9]×[20,21]` occupies exactly the cells the
    /// grid voxelizes — raycast membership = real solids.
    pub fn cells(&self, grid: &CollisionGrid) -> Vec<[i32; 3]> {
        let lo = grid.cell_of(self.min);
        let hi = grid.cell_of([
            self.min[0] + self.size[0],
            self.min[1] + self.size[1],
            self.min[2] + self.size[2],
        ]);
        let mut cells = Vec::new();
        for x in lo[0]..=hi[0] {
            for y in lo[1]..=hi[1] {
                for z in lo[2]..=hi[2] {
                    cells.push([x, y, z]);
                }
            }
        }
        cells
    }

    /// Visual+collision mesh of the box (6 faces, same pusher as the
    /// M4 props).
    pub fn spec(&self) -> MeshSpec {
        let mut positions = Vec::new();
        let mut normals = Vec::new();
        let mut indices = Vec::new();
        push_rect_box(
            &mut positions,
            &mut normals,
            &mut indices,
            self.min,
            self.size,
        );
        MeshSpec {
            positions,
            normals,
            indices,
            // M12: synthetic merchant box — no retail texture.
            uvs: Vec::new(),
            face_textures: Vec::new(),
        }
    }
}

/// Default M5 merchant: in the shop's north interior corridor
/// (`x ∈ (0,5)`, `z ∈ (10,26)` — see `interior_spec`), 1.9 m tall.
///
/// Resulting cells (inclusive voxelization) `(x∈{1,2}, y∈{0,1},
/// z∈{20,21})`: **free** in the M4 grid (counter `z≤16` · island `z≤14` ·
/// shelves `z∈{10,11}/{24,25}` ⇒ check `merchant_no_fixture_overlap`)
/// and **outside the door path** `z∈{17,18}` (the entry march stays
/// free — measured in M5 against the real rasterization).
pub fn shop_merchant() -> MerchantProp {
    MerchantProp {
        min: [1.0, 0.0, 20.0],
        size: [1.0, 1.9, 1.0],
    }
}

/// M5 world: M4 world (`world_mesh`) + merchant box — the
/// `CollisionGrid` input shared by the headless probe and the M5 window.
pub fn merchant_world(world: &MeshSpec, merchant: &MerchantProp) -> MeshSpec {
    concat_specs(&[world, &merchant.spec()])
}

/// Valid interaction hit (first solid = merchant cell).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct InteractHit {
    /// Distance travelled to the merchant cell.
    pub dist: f32,
    /// Merchant cell that was hit.
    pub cell: [i32; 3],
}

/// Does the gaze `dir` from `eye` acquire the merchant within range?
///
/// Rules: (1) chest at distance ≤ `range` (otherwise → `None`, out of
/// range); (2) the **first** solid of the raycast is a merchant
/// cell — a wall/prop in between gives `None` (blocked), a wrong gaze
/// leaves the world or hits something else → `None`.
pub fn interact_target(
    grid: &CollisionGrid,
    eye: [f32; 3],
    dir: [f32; 3],
    merchant: &MerchantProp,
    range: f32,
) -> Option<InteractHit> {
    let chest = merchant.chest();
    let delta = [
        chest[0] - eye[0],
        chest[1] - eye[1],
        chest[2] - eye[2],
    ];
    let chest_dist = (delta[0] * delta[0] + delta[1] * delta[1] + delta[2] * delta[2]).sqrt();
    if chest_dist > range + RANGE_EPS_M {
        return None;
    }
    // The raycast reaches the chest (+1 m cell tolerance): any
    // solid BEFORE the merchant (wall, prop) is recorded as a hit.
    let hit = grid.raycast(eye, dir, chest_dist + 1.0)?;
    if merchant.cells(grid).contains(&hit.cell) {
        Some(InteractHit {
            dist: hit.dist,
            cell: hit.cell,
        })
    } else {
        None
    }
}
