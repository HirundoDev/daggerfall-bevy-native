//! World terrain outside the city: mesh + `CollisionGrid` (M13,
//! DoD §e / WI-6 · REQ-L99-044).
//!
//! **Scope decision:** the M13 terrain is built **only** from the
//! world sampler ([`daggerfall_parser_rs::woods`] over `WOODS.WLD`), with no
//! city input (0 `.RMB`, 0 `.BSA`): "outside the city" is a property
//! of the data origin, not a geometric clip — the city is M10 and this
//! module neither consumes nor rebuilds it. The collision authority is the
//! **existing** one ([`CollisionGrid::from_mesh_spec`], M2/INV-L99-5): the terrain
//! does not introduce a second collision engine.
//!
//! Scale and geometry anchors (corpus `reference/dfu-spec/`, tag `v1.1.1`):
//!
//! | Fact | Anchor |
//! |---|---|
//! | `WorldMapTerrainDim = 32768` world units per map-pixel | `MapsFile.cs:32` |
//! | `UNITS_PER_METER = 40` (1 Unity unit = 1 m) | `MeshReader.cs:58` via `convert.rs` |
//! | normalized height × `MaxTerrainHeight` = terrain Y axis | `TerrainSampler.cs:36-39` + `DefaultTerrainSampler.cs:118-120` |
//! | `HeightmapDimension` default 129 (`hDim - 1` intervals per map-pixel) | `TerrainSampler.cs:96` + `DefaultTerrainSampler.cs:131` |
//! | target eye height / player radius | `view_probe.rs:19-21` (`EYE_HEIGHT_M`/`PLAYER_RADIUS_M`) |
//!
//! Resulting scale: 1 map-pixel = `32768 / 40 = 819.2 m`; spacing between
//! samples of the 129 grid = `819.2 / 128 = 6.4 m`. The walkable patch covers
//! [`PATCH_DIM`]² samples = `16 × 6.4 = 102.4 m` — enough for the 20-step
//! × 2 m walk with margin.
//!
//! Bridge rules (PRE-2 §8): **pure** module (0 Bevy, 0 input, 0
//! persistence) — testable headless.

use daggerfall_parser_rs::woods::{
    HeightSample, TerrainBand, WoodsMap, HEIGHTMAP_DIM, MAX_TERRAIN_HEIGHT,
};

use crate::collision_grid::{first_geometry_hit, CellStatus, CollisionGrid};
use crate::convert::UNITS_PER_METER;
use crate::mesh_spec::MeshSpec;
use crate::view_probe::{EYE_HEIGHT_M, PLAYER_RADIUS_M};

/// `MapsFile.WorldMapTerrainDim`: world units per map-pixel
/// (`MapsFile.cs:32`).
pub const UNITS_PER_TERRAIN_TILE: f32 = 32_768.0;

/// Extent of one map-pixel in meters (`WorldMapTerrainDim / 40`).
pub const TERRAIN_TILE_M: f32 = UNITS_PER_TERRAIN_TILE / UNITS_PER_METER;

/// Spacing between sampler grid samples (129 points per map-pixel).
pub const SAMPLE_SPACING_M: f32 = TERRAIN_TILE_M / (HEIGHTMAP_DIM as f32 - 1.0);

/// Samples per side of the walkable patch (17 × 6.4 m = 102.4 m).
pub const PATCH_DIM: usize = 17;

/// Side of the walkable patch in meters.
pub const PATCH_SPAN_M: f32 = (PATCH_DIM as f32 - 1.0) * SAMPLE_SPACING_M;

/// Probe walk steps (DoD §e: "20-step walk").
pub const WALK_STEPS: usize = 20;

/// Length of one step (m).
pub const WALK_STRIDE_M: f32 = 2.0;

/// Sub-samples of one step trajectory (≤ 0.4 m between samples).
pub const WALK_SUBSAMPLES: usize = 5;

/// Eye height tolerance against the sampler, in collision cells
/// (`DoD §e`: "eye height = sampler height (±1 cell)").
pub const EYE_HEIGHT_TOLERANCE_CELLS: f32 = 1.0;

/// Patch construction error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TerrainError {
    /// `PATCH_DIM` must be ≥ 2 to form at least one quad.
    PatchTooSmall { dim: usize },
}

impl std::fmt::Display for TerrainError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PatchTooSmall { dim } => {
                write!(f, "terrain patch with dim {dim} < 2 (no quads)")
            }
        }
    }
}

impl std::error::Error for TerrainError {}

/// Sample of one patch vertex: height in meters and band.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TerrainVertex {
    /// Height above sea level in meters (Bevy Y axis).
    pub height_m: f32,
    /// Band (ocean/beach/inland).
    pub band: TerrainBand,
    /// Normalized sampler height (`[0,1]`).
    pub normalized: f32,
}

/// Terrain patch outside the city: mesh + collision + sampler.
#[derive(Debug, Clone)]
pub struct TerrainPatch {
    spec: MeshSpec,
    grid: CollisionGrid,
    map_pixel: [i32; 2],
    dim: usize,
    vertices: Vec<TerrainVertex>,
}

impl TerrainPatch {
    /// Builds the patch of map-pixel `(map_x, map_y)` from the
    /// `WOODS.WLD` sampler (`DefaultTerrainSampler.cs:92-120`).
    pub fn build(woods: &WoodsMap, map_x: i32, map_y: i32) -> Result<Self, TerrainError> {
        Self::build_with_dim(woods, map_x, map_y, PATCH_DIM)
    }

    /// Same as [`Self::build`] with an explicit dimension (tests).
    pub fn build_with_dim(
        woods: &WoodsMap,
        map_x: i32,
        map_y: i32,
        dim: usize,
    ) -> Result<Self, TerrainError> {
        if dim < 2 {
            return Err(TerrainError::PatchTooSmall { dim });
        }
        let mut vertices = Vec::with_capacity(dim * dim);
        let mut positions = Vec::with_capacity(dim * dim);
        for iz in 0..dim {
            for ix in 0..dim {
                // Sub-pixel = whole cell of the 129 grid (6.4 m spacing).
                let sample: HeightSample = woods.sample_grid(map_x, map_y, ix, iz);
                let height_m = sample.normalized * MAX_TERRAIN_HEIGHT;
                vertices.push(TerrainVertex {
                    height_m,
                    band: sample.band,
                    normalized: sample.normalized,
                });
                positions.push([ix as f32 * SAMPLE_SPACING_M, height_m, iz as f32 * SAMPLE_SPACING_M]);
            }
        }
        let (indices, normals) = triangulate(&positions, dim);
        let spec = MeshSpec::new(positions, normals, indices);
        let grid = CollisionGrid::from_mesh_spec(&spec, crate::collision_grid::DEFAULT_CELL_M);
        Ok(Self {
            spec,
            grid,
            map_pixel: [map_x, map_y],
            dim,
            vertices,
        })
    }

    /// Terrain mesh (meters, Y up).
    pub fn mesh(&self) -> &MeshSpec {
        &self.spec
    }

    /// Collision authority (the existing one, no new engine).
    pub fn grid(&self) -> &CollisionGrid {
        &self.grid
    }

    /// Origin map-pixel.
    pub fn map_pixel(&self) -> [i32; 2] {
        self.map_pixel
    }

    /// Patch dimension (samples per side).
    pub fn dim(&self) -> usize {
        self.dim
    }

    /// Patch samples (order `iz * dim + ix`).
    pub fn vertices(&self) -> &[TerrainVertex] {
        &self.vertices
    }

    /// Sample at `(ix, iz)` of the patch grid.
    pub fn vertex(&self, ix: usize, iz: usize) -> TerrainVertex {
        self.vertices[iz * self.dim + ix]
    }

    /// Band of the vertex nearest to a world position.
    pub fn band_at(&self, x: f32, z: f32) -> TerrainBand {
        let ix = ((x / SAMPLE_SPACING_M).round() as isize).clamp(0, self.dim as isize - 1) as usize;
        let iz = ((z / SAMPLE_SPACING_M).round() as isize).clamp(0, self.dim as isize - 1) as usize;
        self.vertex(ix, iz).band
    }

    /// Sampler surface at `(x, z)` (bilinear over the quad's 4 samples;
    /// the same quad that triangulates the mesh).
    pub fn sampler_height_at(&self, x: f32, z: f32) -> Option<f32> {
        let fx = x / SAMPLE_SPACING_M;
        let fz = z / SAMPLE_SPACING_M;
        if fx < 0.0 || fz < 0.0 {
            return None;
        }
        let ix = fx.floor() as usize;
        let iz = fz.floor() as usize;
        if ix + 1 >= self.dim || iz + 1 >= self.dim {
            return None;
        }
        let tx = fx - ix as f32;
        let tz = fz - iz as f32;
        let h00 = self.vertex(ix, iz).height_m;
        let h10 = self.vertex(ix + 1, iz).height_m;
        let h01 = self.vertex(ix, iz + 1).height_m;
        let h11 = self.vertex(ix + 1, iz + 1).height_m;
        let top = h00 * (1.0 - tx) + h10 * tx;
        let bottom = h01 * (1.0 - tx) + h11 * tx;
        Some(top * (1.0 - tz) + bottom * tz)
    }

    /// `true` if the point is traversable: beach/inland band, existing
    /// surface and eye outside a solid terrain voxel.
    ///
    /// **Heightfield vs block semantics (M13 decision):** `CollisionGrid`
    /// confines *blocks* (M2-M5) with a ceiling, but a heightfield has
    /// no ceiling: the floor is the surface and the air above it is
    /// traversable. The terrain's vertical authority is the sampler itself
    /// (`sampler_height_at` + `first_geometry_hit`); the grid provides the
    /// solidity query ([`CollisionGrid::classify_point`]) used to
    /// detect any step that puts the eye inside the terrain.
    pub fn is_walkable(&self, x: f32, z: f32) -> bool {
        let band = self.band_at(x, z);
        if !band.is_walkable() {
            return false;
        }
        match self.sampler_height_at(x, z) {
            Some(height) => {
                let eye = [x, height + EYE_HEIGHT_M, z];
                self.grid.classify_point(eye) != CellStatus::Solid
            }
            None => false,
        }
    }

    /// Deterministic walk of [`WALK_STEPS`] steps in direction `dir`
    /// (normalized on XZ) with the eye reinserted on the surface.
    pub fn walk(&self, start: [f32; 2], dir: [f32; 2]) -> WalkReport {
        let norm = (dir[0] * dir[0] + dir[1] * dir[1]).sqrt();
        let dir = if norm > 0.0 {
            [dir[0] / norm, dir[1] / norm]
        } else {
            [1.0, 0.0]
        };
        let start_band = self.band_at(start[0], start[1]);
        let start_height = self.sampler_height_at(start[0], start[1]);
        let mut report = WalkReport {
            steps: 0,
            completed: 0,
            blocked: 0,
            crossings: 0,
            max_eye_error_cells: 0.0,
            start_band,
            end_band: start_band,
            eye: [start[0], 0.0, start[1]],
            walked: false,
            reason: None,
        };
        let Some(start_height) = start_height else {
            report.reason = Some("start outside the patch");
            return report;
        };
        if !start_band.is_walkable() {
            // DoD §e: the ocean is NOT walkable — 0 steps attempted.
            report.reason = Some("ocean not walkable");
            report.eye = [start[0], start_height + EYE_HEIGHT_M, start[1]];
            return report;
        }
        let cell = self.grid.cell_size();
        let mut pos = start;
        let mut eye = [start[0], start_height + EYE_HEIGHT_M, start[1]];
        for _ in 0..WALK_STEPS {
            report.steps += 1;
            let target = [
                pos[0] + dir[0] * WALK_STRIDE_M,
                pos[1] + dir[1] * WALK_STRIDE_M,
            ];
            // ── 1. Does the horizontal trajectory stay above the ground? ────
            // The segment is sub-sampled at ≤ 0.5 m: if the terrain rises
            // above the feet before arriving, the step is blocked
            // (the slope is never crossed).
            let mut blocked_by_terrain = false;
            for sub in 1..=WALK_SUBSAMPLES {
                let t = sub as f32 / WALK_SUBSAMPLES as f32;
                let px = pos[0] + (target[0] - pos[0]) * t;
                let pz = pos[1] + (target[1] - pos[1]) * t;
                match self.sampler_height_at(px, pz) {
                    Some(surface) if surface > eye[1] - PLAYER_RADIUS_M => {
                        blocked_by_terrain = true;
                        break;
                    }
                    Some(_) => {}
                    None => {
                        blocked_by_terrain = true;
                        break;
                    }
                }
            }
            if blocked_by_terrain {
                report.blocked += 1;
                report.reason = Some("slope above the feet");
                break;
            }
            // ── 2. Real mesh floor at the destination (triangles) ──────────
            let Some(sampler_height) = self.sampler_height_at(target[0], target[1]) else {
                report.blocked += 1;
                report.reason = Some("step outside the patch");
                break;
            };
            let probe_origin = [target[0], sampler_height + EYE_HEIGHT_M + 8.0, target[1]];
            // `first_geometry_hit` returns DISTANCE from the origin (m).
            let Some(floor_dist) = first_geometry_hit(
                &self.spec,
                probe_origin,
                [0.0, -1.0, 0.0],
                EYE_HEIGHT_M + 8.0 + MAX_TERRAIN_HEIGHT,
            ) else {
                report.blocked += 1;
                report.reason = Some("no floor under the eye");
                break;
            };
            let floor_y = probe_origin[1] - floor_dist;
            pos = target;
            let error_m = (floor_y - sampler_height).abs();
            report.max_eye_error_cells = report.max_eye_error_cells.max(error_m / cell);
            eye = [pos[0], floor_y + EYE_HEIGHT_M, pos[1]];
            // ── 3. Traversal: eye inside a solid voxel or below the floor ────
            if self.grid.classify_point(eye) == CellStatus::Solid || eye[1] < sampler_height - 1e-3
            {
                report.crossings += 1;
            }
            report.completed += 1;
        }
        report.end_band = self.band_at(pos[0], pos[1]);
        report.eye = eye;
        report.walked = report.completed > 0;
        report
    }

    /// Eye state in the grid (for reports).
    pub fn eye_status(&self, eye: [f32; 3]) -> CellStatus {
        self.grid.classify(eye, PLAYER_RADIUS_M)
    }
}

/// Walk report (binary DoD §e criterion).
#[derive(Debug, Clone, PartialEq)]
pub struct WalkReport {
    /// Steps attempted.
    pub steps: usize,
    /// Steps with advanced position.
    pub completed: usize,
    /// Steps blocked by the grid.
    pub blocked: usize,
    /// Detected traversals (eye inside solid or below the surface).
    pub crossings: usize,
    /// Worst deviation `|mesh floor − sampler height|` in cells.
    pub max_eye_error_cells: f32,
    /// Starting band.
    pub start_band: TerrainBand,
    /// Final band.
    pub end_band: TerrainBand,
    /// Final eye.
    pub eye: [f32; 3],
    /// `true` if at least one step completed.
    pub walked: bool,
    /// Stop reason (if the walk did not complete the 20 steps).
    pub reason: Option<&'static str>,
}

impl WalkReport {
    /// DoD §e criterion: 20/20 steps, 0 traversals, eye height within
    /// ±1 cell of the sampler.
    pub fn pass(&self) -> bool {
        self.walked
            && self.completed == WALK_STEPS
            && self.crossings == 0
            && self.max_eye_error_cells <= EYE_HEIGHT_TOLERANCE_CELLS
    }
}

/// Triangulates a `dim × dim` grid with flat per-triangle normals.
fn triangulate(positions: &[[f32; 3]], dim: usize) -> (Vec<u32>, Vec<[f32; 3]>) {
    let mut indices = Vec::with_capacity((dim - 1) * (dim - 1) * 6);
    let mut normals = Vec::with_capacity((dim - 1) * (dim - 1) * 6);
    let at = |ix: usize, iz: usize| -> u32 { (iz * dim + ix) as u32 };
    for iz in 0..dim - 1 {
        for ix in 0..dim - 1 {
            let a = at(ix, iz);
            let b = at(ix + 1, iz);
            let c = at(ix, iz + 1);
            let d = at(ix + 1, iz + 1);
            for tri in [[a, b, d], [a, d, c]] {
                let normal = triangle_normal(
                    positions[tri[0] as usize],
                    positions[tri[1] as usize],
                    positions[tri[2] as usize],
                );
                for index in tri {
                    indices.push(index);
                    normals.push(normal);
                }
            }
        }
    }
    (indices, normals)
}

/// Unit normal of a triangle (`cross(b - a, c - a)` with Y up).
fn triangle_normal(a: [f32; 3], b: [f32; 3], c: [f32; 3]) -> [f32; 3] {
    let ab = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let ac = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
    let mut normal = [
        ab[1] * ac[2] - ab[2] * ac[1],
        ab[2] * ac[0] - ab[0] * ac[2],
        ab[0] * ac[1] - ab[1] * ac[0],
    ];
    let len = (normal[0] * normal[0] + normal[1] * normal[1] + normal[2] * normal[2]).sqrt();
    if len > 0.0 {
        for axis in normal.iter_mut() {
            *axis /= len;
        }
    }
    normal
}

#[cfg(test)]
mod tests {
    use super::*;
    use daggerfall_parser_rs::woods::{HEADER_LEN, HEIGHTMAP_LEN, MAP_HEIGHT, MAP_WIDTH, OFFSET_TABLE_LEN, OFFICIAL_SIZE};

    /// Flat `WOODS.WLD` fixture with the exact layout (26 001 168 B).
    fn synthetic_woods(height: u8) -> Vec<u8> {
        let mut bytes = vec![0u8; OFFICIAL_SIZE];
        let height_map_offset = 2_001_168u32;
        bytes[0..4].copy_from_slice(&(OFFSET_TABLE_LEN as u32).to_le_bytes());
        bytes[4..8].copy_from_slice(&(MAP_WIDTH as u32).to_le_bytes());
        bytes[8..12].copy_from_slice(&(MAP_HEIGHT as u32).to_le_bytes());
        bytes[28..32].copy_from_slice(&height_map_offset.to_le_bytes());
        for i in 0..HEIGHTMAP_LEN {
            let offset = 2_501_168u32 + (i as u32) * 47;
            let at = HEADER_LEN + i * 4;
            bytes[at..at + 4].copy_from_slice(&offset.to_le_bytes());
            let block = offset as usize;
            for b in 0..47usize {
                bytes[block + b] = height;
            }
        }
        let start = height_map_offset as usize;
        for b in bytes[start..start + HEIGHTMAP_LEN].iter_mut() {
            *b = height;
        }
        bytes
    }

    #[test]
    fn patch_scales_match_the_world_dimensions() {
        assert!((TERRAIN_TILE_M - 819.2).abs() < 1e-3);
        assert!((SAMPLE_SPACING_M - 6.4).abs() < 1e-3);
        assert!((PATCH_SPAN_M - 102.4).abs() < 1e-3);
    }

    #[test]
    fn flat_land_patch_is_walkable_and_stepped() {
        // Flat 10 ⇒ 120 units + extraNoise (≤2.5) ⇒ walkable inland.
        let woods = WoodsMap::parse(&synthetic_woods(10)).expect("valid fixture");
        let patch = TerrainPatch::build(&woods, 400, 200).expect("valid patch");
        assert_eq!(patch.vertex(0, 0).band, TerrainBand::Land);
        assert!(patch.is_walkable(30.0, 30.0));
        let report = patch.walk([20.0, 20.0], [1.0, 0.0]);
        assert_eq!(report.steps, WALK_STEPS);
        assert_eq!(report.completed, WALK_STEPS);
        assert_eq!(report.crossings, 0);
        assert!(report.max_eye_error_cells <= EYE_HEIGHT_TOLERANCE_CELLS);
        assert!(report.pass());
    }

    #[test]
    fn ocean_patch_is_not_walkable() {
        // Flat 1 ⇒ 12 units ⇒ clamp 27.2 ⇒ ocean.
        let woods = WoodsMap::parse(&synthetic_woods(1)).expect("valid fixture");
        let patch = TerrainPatch::build(&woods, 500, 250).expect("valid patch");
        assert_eq!(patch.vertex(0, 0).band, TerrainBand::Ocean);
        assert!(!patch.is_walkable(20.0, 20.0));
        let report = patch.walk([20.0, 20.0], [1.0, 0.0]);
        assert_eq!(report.steps, 0);
        assert_eq!(report.completed, 0);
        assert!(!report.pass());
        assert_eq!(report.reason, Some("ocean not walkable"));
    }

    #[test]
    fn beach_patch_is_walkable() {
        // Flat 3 ⇒ 36 units + extraNoise (≤2.5) ⇒ beach.
        let woods = WoodsMap::parse(&synthetic_woods(3)).expect("valid fixture");
        let patch = TerrainPatch::build(&woods, 573, 292).expect("valid patch");
        assert_eq!(patch.vertex(0, 0).band, TerrainBand::Beach);
        let report = patch.walk([30.0, 30.0], [0.0, 1.0]);
        assert_eq!(report.completed, WALK_STEPS);
        assert_eq!(report.crossings, 0);
        assert!(report.pass());
    }

    #[test]
    fn patch_too_small_is_a_typed_error() {
        let woods = WoodsMap::parse(&synthetic_woods(10)).expect("valid fixture");
        assert_eq!(
            TerrainPatch::build_with_dim(&woods, 0, 0, 1).unwrap_err(),
            TerrainError::PatchTooSmall { dim: 1 }
        );
    }
}
