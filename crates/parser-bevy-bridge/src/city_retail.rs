//! `city_retail` — city composition from real `.RMB` Map Block Records
//! (M10, REQ-L99-042).
//!
//! **The seed is no longer the layout source** (DoD §a): the city layout
//! comes from the `.RMB` records of BLOCKS.BSA (real position of every
//! building, its BuildingData and its ARCH3D model); the synthetic M3 city
//! ([`crate::city_spec`]) remains as a procedural fallback for CI without retail.
//!
//! Sources of the placement math (DoD §a declaration):
//! - `DFU:Assets/Scripts/Utility/RMBLayout.cs` (`GetBuildingData` — the
//!   `sub-record[i]` is `building[i]`: `Position = (XPos, 0, RMBDimension −
//!   ZPos) × GlobalScale`, `Rotation = −YRotation / RotationDivisor` degrees;
//!   `AddModels` — local model `(XPos, −YPos, ZPos) × GlobalScale`; misc
//!   props `(XPos, −YPos + (−4), ZPos + RMBDimension) × GlobalScale`;
//!   `RotationDivisor = 5.68888888888889` Da = 720/revolution —
//!   `DFU:Assets/Scripts/API/BlocksFile.cs:143-146`);
//! - `DFU:.../API/BlocksFile.cs:151` `RMBDimension = 4096` (native side of the
//!   block).
//!
//! **Scale (M10-FIX3 — parity correction):** `MeshSpec` is already in
//! meters with `UNITS_PER_METER = 40` (`convert.rs`, `DFU:MeshReader.cs:58`
//! `GlobalScale = 0.025f`) and with Bevy handedness via z-negation (df→bevy).
//! The ARCH3D vertices additionally carry a 16.8 fixed point (`Arch3dFile.cs:39`
//! `pointDivisor = 256` ⇒ 10240 raw/m); the RMB positions of this
//! module do NOT (40 raw/m). The module applies `UNITS_PER_METER` to the
//! RMB positions (4096 ÷ 40 = **102.4 m per block**, `RMBLayout.cs:34`
//! `RMBSide = 4096f × GlobalScale`) and the SAME z-negation to the Z sign.
//! (The M1 convention `64 u/m` and `RETAIL_BLOCK_M = 64` were the latent parity
//! bug: they inflated/deflated the world 1.6×.) Unity's Y rotation
//! (left-handed) flips the sign when moving to Bevy (right-handed):
//! `yaw_bevy = +YRotation / RotationDivisor`.
//!
//! Determinism: fixed iteration (blocks in grid order, sub-records
//! in order, models in order); [`RetailCity::canonical_bytes`] serializes
//! everything with exact f32 bits — same input ⇒ same bytes ⇒ same
//! [`RetailCity::digest`] (FNV-1a 64, no dependencies).
//!
//! M37 (additive, streaming): [`build_retail_cell`] composes **one** block
//! by calling the same [`build_retail_city`] loop with the global
//! `width/height` (the f32 placement math stays bit-identical to the global
//! composition — P-1 parity), and [`cell_canonical_bytes`] serializes a
//! block's identity + geometry for the global-vs-cell parity check.
//! [`RetailCity::block_ranges`] records each block's half-open vertex/index
//! ranges; it is **not** serialized by [`RetailCity::canonical_bytes`], whose
//! bytes stay frozen (P-2, golden `2b8d8fa5a18c0602`).

use crate::convert::UNITS_PER_METER;
use crate::mesh_spec::MeshSpec;
use crate::stream_policy::CellKey;
use daggerfall_parser_rs::rmb::MapBlockRecord;

/// Side of a city block in meters (4096 raw ÷ 40 =
/// `UNITS_PER_METER`; `DFU:RMBLayout.cs:34` `RMBSide = 4096f × GlobalScale`).
/// M10-FIX3: was `64.0` (wrong M1 convention `4096 ÷ 64`).
pub const RETAIL_BLOCK_M: f32 = 4096.0 / UNITS_PER_METER;
/// Da→degrees rotation divisor (`DFU:BlocksFile.cs` `RotationDivisor`).
pub const ROTATION_DIVISOR: f32 = 5.688_889;
/// Native Y offset of misc props (`DFU:RMBLayout.cs` `propsOffsetY = -4f`).
const PROPS_OFFSET_Y: f32 = -4.0;

/// Resolved retail building (one `BuildingDataList[i]` + its `sub-record[i]`).
#[derive(Debug, Clone, PartialEq)]
pub struct RetailBuilding {
    /// Block X cell in the location grid.
    pub block_x: usize,
    /// Block Y cell in the grid.
    pub block_y: usize,
    /// Sub-record index inside the block.
    pub record_index: usize,
    /// Building type (`daggerfall_parser_rs::maps::building_type_name`).
    pub building_type: u8,
    /// Quality 1-20.
    pub quality: u8,
    /// Name seed.
    pub name_seed: u16,
    /// Faction (0 = none).
    pub faction_id: u16,
    /// Sector (block index in the location).
    pub sector: i16,
    /// Building LocationId.
    pub location_id: u16,
    /// ARCH3D record of the sub-record's first model (0 = no model).
    pub model_id: u32,
    /// Position in meters (Bevy frame; y=0 = block floor).
    pub position: [f32; 3],
    /// Y rotation in radians (Bevy frame).
    pub yaw_rad: f32,
    /// Positions of its doors (Bevy frame, meters).
    pub doors: Vec<[f32; 3]>,
}

/// Half-open vertex/index ranges of one block's contribution to a composed
/// [`RetailCity`] (M37 streaming parity, design §2.7 P-1).
///
/// Ground ranges index into [`RetailCity::ground`], building ranges into
/// [`RetailCity::buildings_mesh`] (the two are concatenated into
/// [`RetailCity::combined`], so per-spec coordinates keep extraction trivial
/// and independent of the concatenation offset).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlockMeshRange {
    /// Block X cell in the location grid.
    pub block_x: usize,
    /// Block Y cell in the grid.
    pub block_y: usize,
    /// Half-open vertex range of this block's ground quad.
    pub ground_vertices: (usize, usize),
    /// Half-open index range of this block's ground quad.
    pub ground_indices: (usize, usize),
    /// Half-open vertex range of this block's placed building models.
    pub building_vertices: (usize, usize),
    /// Half-open index range of this block's placed building models.
    pub building_indices: (usize, usize),
    /// Models of this block that resolved to no ARCH3D spec (per-block
    /// attribution of [`RetailCity::unresolved_models`]).
    pub unresolved: usize,
}

/// Retail city composed from real `.RMB` records.
#[derive(Debug, Clone, PartialEq)]
pub struct RetailCity {
    /// Blocks along the X axis.
    pub width_blocks: usize,
    /// Blocks along the Y axis.
    pub height_blocks: usize,
    /// Buildings (BuildingData + sub-record) in grid order.
    pub buildings: Vec<RetailBuilding>,
    /// Mesh of every placed ARCH3D model (facades + props).
    pub buildings_mesh: MeshSpec,
    /// Ground (one quad per block, y=0).
    pub ground: MeshSpec,
    /// `ground + buildings_mesh` — input of [`crate::CollisionGrid`].
    pub combined: MeshSpec,
    /// Model placements omitted for lacking a resolvable ARCH3D
    /// (honest degradation: the building is still composed with its data).
    pub unresolved_models: usize,
    /// Per-block mesh ranges (M37 streaming parity). Additive bookkeeping:
    /// **not** serialized by [`RetailCity::canonical_bytes`] — P-2 frozen.
    pub block_ranges: Vec<BlockMeshRange>,
}

impl RetailCity {
    /// Total city side in meters (longest side).
    pub fn total_m(&self) -> f32 {
        self.width_blocks.max(self.height_blocks) as f32 * RETAIL_BLOCK_M
    }

    /// Triangles of the combined mesh.
    pub fn triangle_count(&self) -> usize {
        self.combined.triangle_count()
    }

    /// Building type histogram `(type, count)` ordered by type.
    pub fn building_type_histogram(&self) -> Vec<(u8, usize)> {
        let mut counts = [0usize; 256];
        for b in &self.buildings {
            counts[b.building_type as usize] += 1;
        }
        counts
            .into_iter()
            .enumerate()
            .filter(|(_, c)| *c > 0)
            .map(|(t, c)| (t as u8, c))
            .collect()
    }

    /// Canonical byte-by-byte serialization (M10 determinism): building
    /// fields + combined mesh buffers with exact f32 bits.
    pub fn canonical_bytes(&self) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(&(self.width_blocks as u32).to_le_bytes());
        out.extend_from_slice(&(self.height_blocks as u32).to_le_bytes());
        out.extend_from_slice(&(self.unresolved_models as u32).to_le_bytes());
        out.extend_from_slice(&(self.buildings.len() as u32).to_le_bytes());
        for b in &self.buildings {
            out.extend_from_slice(&(b.block_x as u32).to_le_bytes());
            out.extend_from_slice(&(b.block_y as u32).to_le_bytes());
            out.extend_from_slice(&(b.record_index as u32).to_le_bytes());
            out.push(b.building_type);
            out.push(b.quality);
            out.extend_from_slice(&b.name_seed.to_le_bytes());
            out.extend_from_slice(&b.faction_id.to_le_bytes());
            out.extend_from_slice(&b.sector.to_le_bytes());
            out.extend_from_slice(&b.location_id.to_le_bytes());
            out.extend_from_slice(&b.model_id.to_le_bytes());
            for v in b.position {
                out.extend_from_slice(&v.to_bits().to_le_bytes());
            }
            out.extend_from_slice(&b.yaw_rad.to_bits().to_le_bytes());
            out.extend_from_slice(&(b.doors.len() as u32).to_le_bytes());
            for d in &b.doors {
                for v in *d {
                    out.extend_from_slice(&v.to_bits().to_le_bytes());
                }
            }
        }
        push_mesh(&mut out, &self.combined);
        out
    }

    /// FNV-1a 64 digest of [`Self::canonical_bytes`] (hex).
    pub fn digest(&self) -> String {
        const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
        const PRIME: u64 = 0x0000_0100_0000_01b3;
        let mut hash = OFFSET;
        for byte in self.canonical_bytes() {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(PRIME);
        }
        format!("{hash:016x}")
    }
}

fn push_mesh(out: &mut Vec<u8>, spec: &MeshSpec) {
    out.extend_from_slice(&(spec.positions.len() as u32).to_le_bytes());
    for p in &spec.positions {
        for v in *p {
            out.extend_from_slice(&v.to_bits().to_le_bytes());
        }
    }
    for n in &spec.normals {
        for v in *n {
            out.extend_from_slice(&v.to_bits().to_le_bytes());
        }
    }
    out.extend_from_slice(&(spec.indices.len() as u32).to_le_bytes());
    for i in &spec.indices {
        out.extend_from_slice(&i.to_le_bytes());
    }
}

/// Rotation about Y (Bevy frame, radians).
fn rot_y(p: [f32; 3], yaw: f32) -> [f32; 3] {
    let (sin, cos) = yaw.sin_cos();
    [p[0] * cos + p[2] * sin, p[1], -p[0] * sin + p[2] * cos]
}

/// Accumulates `spec` translated by `pos` and rotated by `yaw` about Y into the buffers.
/// UVs (invariant to rotation/translation) and texture keys
/// (M12) are copied per vertex; without data (synthetic spec) they are filled with
/// defaults to preserve the 1:1 alignment with `positions`.
///
/// 8 args for a structural shape (5 buffers + spec + position + yaw) — the
/// pusher is internal and its shape is deliberate (same documented allow as
/// `city_spec::push_quad`, clippy gate `-D warnings`).
#[allow(clippy::too_many_arguments)]
fn push_placed(
    positions: &mut Vec<[f32; 3]>,
    normals: &mut Vec<[f32; 3]>,
    indices: &mut Vec<u32>,
    uvs: &mut Vec<[f32; 2]>,
    face_textures: &mut Vec<u32>,
    spec: &MeshSpec,
    pos: [f32; 3],
    yaw: f32,
) {
    if spec.indices.is_empty() {
        return;
    }
    let base = positions.len() as u32;
    for (i, v) in spec.positions.iter().enumerate() {
        let r = rot_y(*v, yaw);
        positions.push([r[0] + pos[0], r[1] + pos[1], r[2] + pos[2]]);
        let n = rot_y(spec.normals[i], yaw);
        let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
        if len > f32::EPSILON {
            normals.push([n[0] / len, n[1] / len, n[2] / len]);
        } else {
            normals.push([0.0, 1.0, 0.0]);
        }
    }
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
    for i in &spec.indices {
        indices.push(base + *i);
    }
}

/// Horizontal quad y=0 (2 triangles, +Y normal) — same pusher as
/// [`crate::city_spec::push_quad`] (pub(crate), M4).
fn push_ground_quad(
    positions: &mut Vec<[f32; 3]>,
    normals: &mut Vec<[f32; 3]>,
    indices: &mut Vec<u32>,
    x0: f32,
    z0: f32,
    x1: f32,
    z1: f32,
) {
    crate::city_spec::push_quad(
        positions,
        normals,
        indices,
        [0.0, 1.0, 0.0],
        [x0, 0.0, z0],
        [x1, 0.0, z0],
        [x1, 0.0, z1],
        [x0, 0.0, z1],
    );
}

/// Composes the retail city from already parsed `.RMB` blocks.
///
/// `blocks` carries (cell_x, cell_y, record) for each cell of the
/// `width_blocks × height_blocks` grid (gaps are omitted). `resolve_model`
/// maps an ARCH3D record → [`MeshSpec`] (caller's Mode A custody; without a
/// spec the building is still composed with its data and the model is counted in
/// [`RetailCity::unresolved_models`]).
pub fn build_retail_city(
    width_blocks: usize,
    height_blocks: usize,
    blocks: &[(usize, usize, MapBlockRecord)],
    resolve_model: &mut dyn FnMut(u32) -> Option<MeshSpec>,
) -> RetailCity {
    let h = height_blocks.max(1);
    let mut buildings_out: Vec<RetailBuilding> = Vec::new();
    let mut bp = Vec::new();
    let mut bn = Vec::new();
    let mut bi = Vec::new();
    let mut butex = Vec::new();
    let mut btex = Vec::new();
    let mut gp = Vec::new();
    let mut gn = Vec::new();
    let mut gi = Vec::new();
    let mut gutex = Vec::new();
    let mut gttex = Vec::new();
    let mut unresolved = 0usize;
    let mut block_ranges: Vec<BlockMeshRange> = Vec::new();

    for (bx, by, record) in blocks {
        // Block origin in the Bevy frame: x = bx·102.4 (east); z maps block
        // y to [(H−by−1)·102.4, (H−by)·102.4] (the engine's df→bevy
        // z-negation over the DFU Unity frame).
        let origin_x = *bx as f32 * RETAIL_BLOCK_M;
        let origin_z = (h - by) as f32 * RETAIL_BLOCK_M;

        // M37: half-open ranges of this block's contribution (parity P-1).
        let ground_vertices_start = gp.len();
        let ground_indices_start = gi.len();
        let building_vertices_start = bp.len();
        let building_indices_start = bi.len();
        let unresolved_start = unresolved;

        // Block ground (quad y=0) — synthetic, untextured (defaults).
        push_ground_quad(
            &mut gp,
            &mut gn,
            &mut gi,
            origin_x,
            origin_z - RETAIL_BLOCK_M,
            origin_x + RETAIL_BLOCK_M,
            origin_z,
        );
        gutex.extend(std::iter::repeat_n([0.0, 0.0], 4));
        gttex.extend(std::iter::repeat_n(crate::mesh_spec::NO_TEXTURE, 4));

        let count = record.num_block_data_records as usize;
        for i in 0..count.min(record.sub_records.len()) {
            let sub = &record.sub_records[i];
            // The sub-record translations are already copied from
            // BlockPositions[i] in the parser (same scheme as DFU).
            let bld = record.buildings[i];

            // Building translation (RMBLayout.GetBuildingData):
            // (XPos, 0, RMBDimension − ZPos) × scale, with bevy z-negation.
            let pos = [
                origin_x + sub.x_pos as f32 / UNITS_PER_METER,
                0.0,
                origin_z - (4096.0 - sub.z_pos as f32) / UNITS_PER_METER,
            ];
            let sub_yaw = (sub.y_rotation as f32 / ROTATION_DIVISOR).to_radians();

            // First model = the building model (RMBLayout.GetBuildingData).
            let model_id = sub
                .exterior
                .models
                .first()
                .map_or(0, |m| m.model_id);

            // Exterior models placed in the sub-record frame
            // (RMBLayout.AddModels: local model (XPos, −YPos, ZPos)×scale).
            for model in &sub.exterior.models {
                let local = [
                    model.x_pos as f32 / UNITS_PER_METER,
                    -model.y_pos as f32 / UNITS_PER_METER,
                    -model.z_pos as f32 / UNITS_PER_METER,
                ];
                let rotated = rot_y(local, sub_yaw);
                let world = [pos[0] + rotated[0], pos[1] + rotated[1], pos[2] + rotated[2]];
                let model_yaw = (model.y_rotation as f32 / ROTATION_DIVISOR).to_radians();
                match resolve_model(model.model_id) {
                    Some(spec) => {
                        push_placed(
                            &mut bp,
                            &mut bn,
                            &mut bi,
                            &mut butex,
                            &mut btex,
                            &spec,
                            world,
                            sub_yaw + model_yaw,
                        );
                    }
                    None => unresolved += 1,
                }
            }

            // Block misc props (RMBLayout.AddProps): no rotation,
            // (XPos, −YPos + propsOffsetY, ZPos + RMBDimension)×scale.
            for model in &record.misc_3d_models {
                let local = [
                    model.x_pos as f32 / UNITS_PER_METER,
                    (-model.y_pos as f32 + PROPS_OFFSET_Y) / UNITS_PER_METER,
                    -(model.z_pos as f32 + 4096.0) / UNITS_PER_METER,
                ];
                let world = [origin_x + local[0], local[1], origin_z + local[2]];
                match resolve_model(model.model_id) {
                    Some(spec) => {
                        push_placed(&mut bp, &mut bn, &mut bi, &mut butex, &mut btex, &spec, world, 0.0);
                    }
                    None => unresolved += 1,
                }
            }

            // Building doors (sub-record frame, no Y flip).
            let doors = sub
                .exterior
                .doors
                .iter()
                .map(|d| {
                    let local = [
                        d.x_pos as f32 / UNITS_PER_METER,
                        d.y_pos as f32 / UNITS_PER_METER,
                        -d.z_pos as f32 / UNITS_PER_METER,
                    ];
                    let rotated = rot_y(local, sub_yaw);
                    [pos[0] + rotated[0], pos[1] + rotated[1], pos[2] + rotated[2]]
                })
                .collect();

            buildings_out.push(RetailBuilding {
                block_x: *bx,
                block_y: *by,
                record_index: i,
                building_type: bld.building_type,
                quality: bld.quality,
                name_seed: bld.name_seed,
                faction_id: bld.faction_id,
                sector: bld.sector,
                location_id: bld.location_id,
                model_id,
                position: pos,
                yaw_rad: sub_yaw,
                doors,
            });
        }

        block_ranges.push(BlockMeshRange {
            block_x: *bx,
            block_y: *by,
            ground_vertices: (ground_vertices_start, gp.len()),
            ground_indices: (ground_indices_start, gi.len()),
            building_vertices: (building_vertices_start, bp.len()),
            building_indices: (building_indices_start, bi.len()),
            unresolved: unresolved - unresolved_start,
        });
    }

    let buildings_mesh = MeshSpec {
        positions: bp,
        normals: bn,
        indices: bi,
        uvs: butex,
        face_textures: btex,
    };
    let ground = MeshSpec::new(gp, gn, gi);
    // ground carries UV/texture defaults to preserve alignment.
    let ground = MeshSpec {
        uvs: gutex,
        face_textures: gttex,
        ..ground
    };
    let combined = concat_specs(&ground, &buildings_mesh);
    RetailCity {
        width_blocks,
        height_blocks,
        buildings: buildings_out,
        buildings_mesh,
        ground,
        combined,
        unresolved_models: unresolved,
        block_ranges,
    }
}

/// Composes **one** retail cell (M37 streaming, design §2.3) by calling the
/// same [`build_retail_city`] loop with a single block.
///
/// `width_blocks`/`height_blocks` must be the **global** dimensions of the
/// location the cell belongs to: the placement math (`origin_z` depends on
/// `height_blocks`) then produces bit-identical geometry to the global
/// composition, which is what makes [`cell_canonical_bytes`] parity (P-1)
/// exact instead of approximately equal. `cell.x`/`cell.z` are the grid
/// coordinates `(bx, by)` of the block (negative values are a caller bug:
/// grids start at 0).
pub fn build_retail_cell(
    width_blocks: usize,
    height_blocks: usize,
    cell: CellKey,
    record: MapBlockRecord,
    resolve_model: &mut dyn FnMut(u32) -> Option<MeshSpec>,
) -> RetailCity {
    let bx =
        usize::try_from(cell.x).expect("build_retail_cell: cell.x is a grid coordinate (>= 0)");
    let by =
        usize::try_from(cell.z).expect("build_retail_cell: cell.z is a grid coordinate (>= 0)");
    build_retail_city(
        width_blocks,
        height_blocks,
        &[(bx, by, record)],
        resolve_model,
    )
}

/// Canonical serialization of the block `cell` inside `city` (M37 parity
/// P-1): cell identity + the block's building records + the block's ground
/// and building geometry extracted via [`RetailCity::block_ranges`] with
/// rebased indices. Comparing this against the same call over
/// [`build_retail_cell`]'s output is the byte-level falsifier — any
/// difference is a parity break.
///
/// Absence is explicit: an empty `Vec` means "this city has no such cell"
/// (wrong coordinates or `level != 0` — a composed retail city is a
/// single-level world; dungeon cells serialize through
/// `DungeonGrid::canonical_bytes`, not through this function).
pub fn cell_canonical_bytes(city: &RetailCity, cell: CellKey) -> Vec<u8> {
    let (Ok(bx), Ok(by)) = (usize::try_from(cell.x), usize::try_from(cell.z)) else {
        return Vec::new();
    };
    if cell.level != 0 {
        return Vec::new();
    }
    let Some(range) = city
        .block_ranges
        .iter()
        .find(|r| r.block_x == bx && r.block_y == by)
    else {
        return Vec::new();
    };

    let mut out = Vec::new();
    out.extend_from_slice(b"M37CELL1");
    out.extend_from_slice(&cell.x.to_le_bytes());
    out.extend_from_slice(&cell.z.to_le_bytes());
    out.extend_from_slice(&cell.level.to_le_bytes());
    out.extend_from_slice(&(range.unresolved as u32).to_le_bytes());

    let buildings: Vec<&RetailBuilding> = city
        .buildings
        .iter()
        .filter(|b| b.block_x == bx && b.block_y == by)
        .collect();
    out.extend_from_slice(&(buildings.len() as u32).to_le_bytes());
    for b in buildings {
        out.extend_from_slice(&(b.record_index as u32).to_le_bytes());
        out.push(b.building_type);
        out.push(b.quality);
        out.extend_from_slice(&b.name_seed.to_le_bytes());
        out.extend_from_slice(&b.faction_id.to_le_bytes());
        out.extend_from_slice(&b.sector.to_le_bytes());
        out.extend_from_slice(&b.location_id.to_le_bytes());
        out.extend_from_slice(&b.model_id.to_le_bytes());
        for v in b.position {
            out.extend_from_slice(&v.to_bits().to_le_bytes());
        }
        out.extend_from_slice(&b.yaw_rad.to_bits().to_le_bytes());
        out.extend_from_slice(&(b.doors.len() as u32).to_le_bytes());
        for d in &b.doors {
            for v in *d {
                out.extend_from_slice(&v.to_bits().to_le_bytes());
            }
        }
    }

    push_mesh(
        &mut out,
        &sub_spec(&city.ground, range.ground_vertices, range.ground_indices),
    );
    push_mesh(
        &mut out,
        &sub_spec(
            &city.buildings_mesh,
            range.building_vertices,
            range.building_indices,
        ),
    );
    out
}

/// The block's slice of a mesh spec: vertices by vertex range, indices by
/// index range rebased to the slice (so the result is a self-contained mesh
/// of the cell). UVs/textures stay out — [`push_mesh`] (the same serializer
/// as [`RetailCity::canonical_bytes`]) consumes positions/normals/indices.
fn sub_spec(spec: &MeshSpec, vertices: (usize, usize), indices: (usize, usize)) -> MeshSpec {
    let base = vertices.0 as u32;
    MeshSpec {
        positions: spec.positions[vertices.0..vertices.1].to_vec(),
        normals: spec.normals[vertices.0..vertices.1].to_vec(),
        indices: spec.indices[indices.0..indices.1]
            .iter()
            .map(|i| i - base)
            .collect(),
        uvs: Vec::new(),
        face_textures: Vec::new(),
    }
}

/// Concatenates two specs (corrected indices). UVs/textures (M12) are
/// concatenated with defaults for synthetic specs.
fn concat_specs(a: &MeshSpec, b: &MeshSpec) -> MeshSpec {
    let mut positions = a.positions.clone();
    positions.extend_from_slice(&b.positions);
    let mut normals = a.normals.clone();
    normals.extend_from_slice(&b.normals);
    let base = a.positions.len() as u32;
    let mut indices = a.indices.clone();
    indices.extend(b.indices.iter().map(|i| base + *i));
    let mut uvs = Vec::with_capacity(positions.len());
    let mut face_textures = Vec::with_capacity(positions.len());
    for spec in [a, b] {
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
    use daggerfall_parser_rs::rmb::{BlockPosition, BlockData, SubRecord};

    /// Synthetic spec: horizontal 4×4 m quad centered at the origin.
    fn quad_spec() -> MeshSpec {
        MeshSpec {
            positions: vec![
                [-2.0, 0.0, -2.0],
                [2.0, 0.0, -2.0],
                [2.0, 0.0, 2.0],
                [-2.0, 0.0, 2.0],
            ],
            normals: vec![[0.0, 1.0, 0.0]; 4],
            indices: vec![0, 2, 1, 0, 3, 2],
            uvs: Vec::new(),
            face_textures: Vec::new(),
        }
    }

    fn one_building_record() -> MapBlockRecord {
        let mut rec = MapBlockRecord {
            num_block_data_records: 1,
            ..Default::default()
        };
        rec.block_positions = vec![
            BlockPosition { x_pos: 2048, z_pos: 1024, y_rotation: 0 };
            32
        ];
        let mut buildings =
            vec![daggerfall_parser_rs::maps::BuildingData::default(); 32];
        buildings[0].building_type = 15;
        buildings[0].quality = 12;
        buildings[0].name_seed = 7;
        buildings[0].faction_id = 2;
        buildings[0].location_id = 55;
        rec.buildings = buildings;
        let mut exterior = BlockData::default();
        exterior.models.push(daggerfall_parser_rs::rmb::Model3d {
            model_id: 4405,
            object_type: 0,
            x_pos: 0,
            y_pos: 0,
            z_pos: 0,
            y_rotation: 0,
        });
        exterior.doors.push(daggerfall_parser_rs::rmb::Door {
            x_pos: 64,
            y_pos: 0,
            z_pos: 128,
            y_rotation: 0,
            open_rotation: 0,
            door_model_index: 0,
        });
        rec.sub_records = vec![SubRecord {
            x_pos: 2048,
            z_pos: 1024,
            y_rotation: 0,
            exterior,
            interior: BlockData::default(),
        }];
        rec
    }

    fn resolve(_: u32) -> Option<MeshSpec> {
        Some(quad_spec())
    }

    #[test]
    fn building_position_matches_dfu_math() {
        let rec = one_building_record();
        let city = build_retail_city(1, 1, &[(0, 0, rec)], &mut resolve);
        assert_eq!(city.buildings.len(), 1);
        let b = &city.buildings[0];
        // M10-FIX3 (DFU scale 40 u/m — `RMBLayout.cs:570` × `GlobalScale`):
        // origin (0,0,102.4) + (2048/40, 0, −(4096−1024)/40)
        // → (51.2, 0, 102.4 − 76.8 = 25.6) m. (Tolerance: 102.4 and 76.8 are
        // not dyadic in f32; the subtraction rounds ~1e-6 away from the decimal.)
        for (axis, (got, want)) in b.position.iter().zip([51.2, 0.0, 25.6]).enumerate() {
            assert!((got - want).abs() < 1e-4, "position[{axis}] = {got}, expected {want}");
        }
        assert_eq!(b.building_type, 15);
        assert_eq!(b.model_id, 4405);
        assert_eq!(b.doors.len(), 1);
        // Local door (64,0,128) → relative (1.6, 0, −3.2) → (52.8, 0, 22.4).
        for (axis, (got, want)) in b.doors[0].iter().zip([52.8, 0.0, 22.4]).enumerate() {
            assert!((got - want).abs() < 1e-4, "door[{axis}] = {got}, expected {want}");
        }
        assert_eq!(city.unresolved_models, 0);
        assert!(city.buildings_mesh.triangle_count() > 0);
        // Block ground covers z ∈ [0, 102.4] (`RMBSide = 4096 × 0.025`).
        let (gmin, gmax) = city.ground.bounds().unwrap();
        assert_eq!(gmin, [0.0, 0.0, 0.0]);
        assert_eq!(gmax, [RETAIL_BLOCK_M, 0.0, RETAIL_BLOCK_M]);
    }

    #[test]
    fn same_input_same_digest() {
        let rec_a = one_building_record();
        let rec_b = one_building_record();
        let city_a = build_retail_city(1, 1, &[(0, 0, rec_a)], &mut resolve);
        let city_b = build_retail_city(1, 1, &[(0, 0, rec_b)], &mut resolve);
        assert_eq!(city_a.canonical_bytes(), city_b.canonical_bytes());
        assert_eq!(city_a.digest(), city_b.digest());
    }

    #[test]
    fn different_input_different_digest() {
        let rec_a = one_building_record();
        let mut rec_b = one_building_record();
        rec_b.sub_records[0].x_pos = 1024; // building moved
        let city_a = build_retail_city(1, 1, &[(0, 0, rec_a)], &mut resolve);
        let city_b = build_retail_city(1, 1, &[(0, 0, rec_b)], &mut resolve);
        assert_ne!(city_a.digest(), city_b.digest());
    }

    #[test]
    fn unresolved_model_degrades_honestly() {
        let rec = one_building_record();
        let mut none = |_id: u32| None;
        let city = build_retail_city(1, 1, &[(0, 0, rec)], &mut none);
        assert_eq!(city.unresolved_models, 1);
        assert_eq!(city.buildings.len(), 1);
        assert_eq!(city.buildings_mesh.triangle_count(), 0);
    }

    /// P-2 (M37 §2.7): `canonical_bytes`/`digest` of the 1×1 fixture are a
    /// frozen golden. The constant was computed on the pristine tree before
    /// the M37 streaming additions (`block_ranges`), so any byte change in
    /// this serialization fails here first (R6 falsifier).
    #[test]
    fn golden_canonical_bytes_p2_frozen() {
        let rec = one_building_record();
        let city = build_retail_city(1, 1, &[(0, 0, rec)], &mut resolve);
        assert_eq!(city.digest(), "2b8d8fa5a18c0602");
    }

    /// M37 §2.7 P-1, synthetic falsifier: for every block of a 2×2 city,
    /// `cell_canonical_bytes(global, key)` is byte-identical to
    /// `cell_canonical_bytes(build_retail_cell(...), key)`.
    #[test]
    fn cell_parity_matches_global_composition() {
        let records: Vec<MapBlockRecord> = (0..4u32)
            .map(|i| {
                let mut rec = one_building_record();
                rec.sub_records[0].x_pos = 1024 + 256 * i as i32;
                rec.buildings[0].building_type = 10 + i as u8;
                rec
            })
            .collect();
        let blocks: Vec<(usize, usize, MapBlockRecord)> = records
            .iter()
            .enumerate()
            .map(|(i, rec)| (i % 2, i / 2, rec.clone()))
            .collect();
        let global = build_retail_city(2, 2, &blocks, &mut resolve);
        assert_eq!(global.block_ranges.len(), 4);

        for (i, (bx, by, rec)) in blocks.iter().enumerate() {
            let key = CellKey {
                x: *bx as i32,
                z: *by as i32,
                level: 0,
            };
            let cell_city = build_retail_cell(2, 2, key, rec.clone(), &mut resolve);
            let from_global = cell_canonical_bytes(&global, key);
            let from_cell = cell_canonical_bytes(&cell_city, key);
            assert!(!from_global.is_empty(), "block {i} present in the global city");
            assert_eq!(
                from_global, from_cell,
                "P-1 parity of block ({bx},{by}): global vs per-cell composition"
            );
            assert_eq!(
                from_global,
                cell_canonical_bytes(&global, key),
                "cell canonical bytes are deterministic ×2"
            );
        }
    }

    #[test]
    fn cell_canonical_bytes_absence_is_explicit() {
        let rec = one_building_record();
        let city = build_retail_city(1, 1, &[(0, 0, rec)], &mut resolve);
        assert!(
            cell_canonical_bytes(&city, CellKey { x: 9, z: 9, level: 0 }).is_empty(),
            "wrong coordinates ⇒ empty, never fabricated bytes"
        );
        assert!(cell_canonical_bytes(&city, CellKey { x: -1, z: 0, level: 0 }).is_empty());
        assert!(
            cell_canonical_bytes(&city, CellKey { x: 0, z: 0, level: 1 }).is_empty(),
            "a retail city is single-level: level != 0 has no cell here"
        );
    }

    #[test]
    fn cell_canonical_bytes_distinguishes_cells() {
        let rec_a = one_building_record();
        let mut rec_b = one_building_record();
        rec_b.sub_records[0].x_pos = 1024;
        let blocks = vec![(0usize, 0usize, rec_a), (1, 0, rec_b)];
        let city = build_retail_city(2, 1, &blocks, &mut resolve);
        let a = cell_canonical_bytes(&city, CellKey { x: 0, z: 0, level: 0 });
        let b = cell_canonical_bytes(&city, CellKey { x: 1, z: 0, level: 0 });
        assert!(!a.is_empty() && !b.is_empty());
        assert_ne!(a, b, "different blocks serialize differently");
    }

    #[test]
    fn block_ranges_track_the_mesh_slices() {
        let rec_a = one_building_record();
        let mut rec_b = one_building_record();
        rec_b.sub_records[0].x_pos = 1024;
        let city = build_retail_city(
            2,
            1,
            &[(0, 0, rec_a), (1, 0, rec_b)],
            &mut resolve,
        );
        assert_eq!(city.block_ranges.len(), 2);
        let r0 = city.block_ranges[0];
        let r1 = city.block_ranges[1];
        // Ground: one quad (4 verts, 6 indices) per block, in grid order.
        assert_eq!(r0.ground_vertices, (0, 4));
        assert_eq!(r0.ground_indices, (0, 6));
        assert_eq!(r1.ground_vertices, (4, 8));
        assert_eq!(r1.ground_indices, (6, 12));
        // Buildings: one placed quad spec (4 verts, 6 indices) per building.
        assert_eq!(r0.building_vertices, (0, 4));
        assert_eq!(r0.building_indices, (0, 6));
        assert_eq!(r1.building_vertices, (4, 8));
        assert_eq!(r1.building_indices, (6, 12));
        assert_eq!(r1.building_vertices.1, city.buildings_mesh.positions.len());
        assert_eq!(r0.unresolved, 0);
    }

    #[test]
    fn block_ranges_count_unresolved_per_block() {
        let rec_a = one_building_record();
        let rec_b = one_building_record();
        let mut none = |_id: u32| None;
        let city = build_retail_city(
            2,
            1,
            &[(0, 0, rec_a), (1, 0, rec_b)],
            &mut none,
        );
        assert_eq!(city.unresolved_models, 2);
        assert_eq!(city.block_ranges[0].unresolved, 1, "per-block attribution");
        assert_eq!(city.block_ranges[1].unresolved, 1);
        // The empty geometry still serializes (honest degradation is part
        // of the cell's canonical form).
        let cell = cell_canonical_bytes(&city, CellKey { x: 0, z: 0, level: 0 });
        assert!(!cell.is_empty());
    }

    #[test]
    fn histogram_counts_types() {
        let rec = one_building_record();
        let mut rec2 = one_building_record();
        rec2.buildings[0].building_type = 9;
        let city = build_retail_city(
            2,
            1,
            &[(0, 0, rec), (1, 0, rec2)],
            &mut resolve,
        );
        assert_eq!(
            city.building_type_histogram(),
            vec![(9, 1), (15, 1)]
        );
    }
}
