//! `interior_retail` — navigable interior assembled from **real** `.RDB`
//! records of BLOCKS.BSA (M11, REQ-L99-043 · DoD §b · WI-2/WI-3 tickets).
//!
//! **Why it exists:** the M4 shop ([`crate::interior_spec`]) is 100%
//! synthetic because of the "DFON1.BSA absent" delta; the premise was FALSE (WI-3):
//! Daggerfall interiors are assembled procedurally from `.RDB`
//! + `ARCH3D.BSA`, both on disk. Here walls/props/lights come from the
//!   real objects of an `.RDB` (Model → ARCH3D, LightSource, Flat).
//!
//! Sources of the math (DoD §b declaration):
//! - UESP `Daggerfall Mod:BLOCKS.BSA/Dungeon_Block_Records` (verified
//!   2026-10-20): `RdbHeader` (Width/Height) · `ModelReferenceList[750]` →
//!   ARCH3D.BSA `ModelId` · `Object.Position` = Point3 · rotation in
//!   Daggerfall Angles · `LightSource.Radius` · `Flat.Texture`.
//! - `DFU:Assets/Scripts/Utility/RDBLayout.cs` (upstream MIT **cited**,
//!   the pinned corpus does not include it — same regime as `.RMB` in M10):
//!   `GetModelMatrix` ⇒ `position = (XPos, −YPos, ZPos) × GlobalScale` and
//!   `M = T(position)·Rz(dz)·Rx(dx)·Ry(dy)` with
//!   `dx,dy,dz = −X/Y/ZRotation ÷ RotationDivisor`; `RDBSide = 2048f ×
//!   GlobalScale`; light `range = Radius × GlobalScale`.
//! - `MeshReader.GlobalScale = 0.025 = 1/40` ⇒ **`UNITS_PER_METER = 40`**
//!   (M10-FIX3; do NOT reintroduce 64) and `RDBSide = 2048 ÷ 40 = 51.2 m`.
//! - Rotation to the Bevy frame (right-handed; `MeshSpec` already carries the
//!   df→bevy z-negation, see `mesh_spec.rs`): conjugating `M` by `S = diag(1,1,−1)`
//!   gives `Rb = Rz(dz)·Rx(−dx)·Ry(−dy)` ⇒ **yaw = +YRot÷Div** (the same
//!   positive sign as `city_retail`), pitch = +XRot÷Div,
//!   roll = −ZRot÷Div, applied Ry → Rx → Rz and translated.
//!
//! **Door (gap):** no synthetic geometry is carved — the gap is a
//! **real opening** of the `.RDB` mesh: the block perimeter is swept and
//! the first opening **flanked by solid walls** is selected on
//! the 4 sides (fixed order +X→−X→+Z→−Z, ascending samples). Around
//! the block only a **street strip** is added (4 flat quads y=0)
//! so the door can be reached on foot: walls/props/lights remain
//! 100% from the RDB record (documented delta, see [`retail_world`]).
//!
//! Anti-Pantalla (UBIQUITOUS §A-24): [`run_retail_interior_probe`] verifies
//! at runtime street→door→interior→door without traversing (INV-L99-5),
//! reusing the generic layer [`crate::run_view_probe`] (the same 5
//! PROBE-VIEW-001 checks) + 7 RDB/door/interior-specific checks.

use crate::city_retail::ROTATION_DIVISOR;
use crate::city_spec::push_quad;
use crate::collision_grid::{CellStatus, CollisionGrid, DEFAULT_CELL_M};
use crate::convert::UNITS_PER_METER;
use crate::interior_spec::concat_specs;
use crate::mesh_spec::MeshSpec;
use crate::view_probe::{run_view_probe, ProbeCheck, EYE_HEIGHT_M, PLAYER_RADIUS_M};
use daggerfall_parser_rs::rdb::{RdbBlock, RdbResource};

/// Side of an `.RDB` cell in meters (`DFU:RDBLayout.cs` `RDBSide =
/// 2048f × MeshReader.GlobalScale` = 2048 ÷ 40).
pub const RDB_SIDE_M: f32 = 2048.0 / UNITS_PER_METER;
/// Street strip margin around the block (m).
pub const STREET_MARGIN_M: f32 = 10.0;
/// Street approach distance to the door (m).
pub const APPROACH_M: f32 = 6.0;
/// Required entry depth inside the block (m).
pub const ENTER_DEPTH_M: f32 = 3.0;
/// Probe march step (m) — same grain as M2/M3/M4.
const MARCH_STEP_M: f32 = 0.25;
/// Step cap per segment (0.25 m × 4000 = 1000 m).
const MARCH_MAX_STEPS: usize = 4000;
/// Inset of the "inside the block" predicate (m).
const INSIDE_INSET_M: f32 = 1.0;

/// ARCH3D model placed from a real `Model` object.
#[derive(Debug, Clone, PartialEq)]
pub struct RetailModel {
    /// Object origin in meters (Bevy frame; native units ÷ 40).
    pub position: [f32; 3],
    /// ARCH3D record resolved from `ModelReferenceList[ModelIndex]`.
    pub model_id: u32,
    /// Raw rotation in Daggerfall Angles `[XRot, YRot, ZRot]`.
    pub rotation_da: [i32; 3],
    /// Triangles of the placed ARCH3D spec.
    pub triangles: usize,
    /// `TriggerFlag_StartingLock` (0 = no lock; 240 = magical, UESP).
    pub trigger_flag: u32,
}

/// Real light source (`LightSource`) converted to meters.
#[derive(Debug, Clone, PartialEq)]
pub struct RetailLight {
    /// Position in meters (Bevy frame).
    pub position: [f32; 3],
    /// Raw radius (`LightSource.Radius`, UInt16).
    pub radius_raw: u16,
    /// Radius in meters (`Radius × GlobalScale`, DFU `AddLight`).
    pub radius_m: f32,
}

/// Real flat (billboard/marker/monster) converted to meters.
#[derive(Debug, Clone, PartialEq)]
pub struct RetailFlat {
    /// Position in meters (Bevy frame).
    pub position: [f32; 3],
    /// Texture archive (`Texture.Archive`, UESP).
    pub texture_archive: u16,
    /// Texture record (`Texture.Record`, UESP).
    pub texture_record: u16,
}

/// Interior composed from ONE real `.RDB` record.
#[derive(Debug, Clone, PartialEq)]
pub struct RetailInterior {
    /// Record name in BLOCKS.BSA (e.g. `N0000019.RDB`).
    pub block_name: String,
    /// `RdbHeader.Width` (cells).
    pub width: u32,
    /// `RdbHeader.Height` (cells).
    pub height: u32,
    /// Placed models (`ObjectRootList` order, deterministic).
    pub models: Vec<RetailModel>,
    /// Placed lights.
    pub lights: Vec<RetailLight>,
    /// Placed flats.
    pub flats: Vec<RetailFlat>,
    /// Mesh of all placed ARCH3D models (real walls/props).
    pub mesh: MeshSpec,
    /// Placements without a resolvable ARCH3D (honest degradation).
    pub unresolved_models: usize,
}

impl RetailInterior {
    /// Counts per type `(model, light, flat)` of the record.
    pub fn counts(&self) -> (usize, usize, usize) {
        (self.models.len(), self.lights.len(), self.flats.len())
    }

    /// Nominal footprint of the block in the Bevy frame: `x ∈ [0, width·RDBSide]`,
    /// `z ∈ [−height·RDBSide, 0]` (df→bevy z-negation over `ZPos ≥ 0`).
    pub fn nominal_footprint(&self) -> ([f32; 3], [f32; 3]) {
        let w = self.width.max(1) as f32 * RDB_SIDE_M;
        let h = self.height.max(1) as f32 * RDB_SIDE_M;
        ([0.0, 0.0, -h], [w, 0.0, 0.0])
    }

    /// REAL footprint of the placed mesh `(min, max)`; `None` without vertices.
    pub fn mesh_bounds(&self) -> Option<([f32; 3], [f32; 3])> {
        self.mesh.bounds()
    }

    /// Canonical byte-for-byte serialization (M11 determinism): footprint +
    /// placements + mesh buffers with exact f32 bits.
    pub fn canonical_bytes(&self) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(&self.width.to_le_bytes());
        out.extend_from_slice(&self.height.to_le_bytes());
        out.extend_from_slice(&(self.unresolved_models as u32).to_le_bytes());
        out.extend_from_slice(&(self.models.len() as u32).to_le_bytes());
        for m in &self.models {
            for v in m.position {
                out.extend_from_slice(&v.to_bits().to_le_bytes());
            }
            out.extend_from_slice(&m.model_id.to_le_bytes());
            for r in m.rotation_da {
                out.extend_from_slice(&r.to_le_bytes());
            }
            out.extend_from_slice(&(m.triangles as u32).to_le_bytes());
            out.extend_from_slice(&m.trigger_flag.to_le_bytes());
        }
        out.extend_from_slice(&(self.lights.len() as u32).to_le_bytes());
        for l in &self.lights {
            for v in l.position {
                out.extend_from_slice(&v.to_bits().to_le_bytes());
            }
            out.extend_from_slice(&l.radius_raw.to_le_bytes());
        }
        out.extend_from_slice(&(self.flats.len() as u32).to_le_bytes());
        for f in &self.flats {
            for v in f.position {
                out.extend_from_slice(&v.to_bits().to_le_bytes());
            }
            out.extend_from_slice(&f.texture_archive.to_le_bytes());
            out.extend_from_slice(&f.texture_record.to_le_bytes());
        }
        out.extend_from_slice(&(self.mesh.positions.len() as u32).to_le_bytes());
        for p in &self.mesh.positions {
            for v in *p {
                out.extend_from_slice(&v.to_bits().to_le_bytes());
            }
        }
        out.extend_from_slice(&(self.mesh.normals.len() as u32).to_le_bytes());
        for n in &self.mesh.normals {
            for v in *n {
                out.extend_from_slice(&v.to_bits().to_le_bytes());
            }
        }
        out.extend_from_slice(&(self.mesh.indices.len() as u32).to_le_bytes());
        for i in &self.mesh.indices {
            out.extend_from_slice(&i.to_le_bytes());
        }
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

/// Rotation from Daggerfall Angles to the Bevy frame (see module doc):
/// `yaw = +YRot÷Div` · `pitch = +XRot÷Div` · `roll = −ZRot÷Div`,
/// applied Ry → Rx → Rz (RH matrices; `MeshSpec` already z-negated).
fn rotate_rdb(v: [f32; 3], rotation_da: [i32; 3]) -> [f32; 3] {
    let x_rot = rotation_da[0] as f32 / ROTATION_DIVISOR;
    let y_rot = rotation_da[1] as f32 / ROTATION_DIVISOR;
    let z_rot = rotation_da[2] as f32 / ROTATION_DIVISOR;
    let (yaw, pitch, roll) = (
        y_rot.to_radians(),
        x_rot.to_radians(),
        -z_rot.to_radians(),
    );

    let (sy, cy) = yaw.sin_cos();
    let v = [v[0] * cy + v[2] * sy, v[1], -v[0] * sy + v[2] * cy];
    let (sp, cp) = pitch.sin_cos();
    let v = [v[0], v[1] * cp - v[2] * sp, v[1] * sp + v[2] * cp];
    let (sr, cr) = roll.sin_cos();
    [v[0] * cr - v[1] * sr, v[0] * sr + v[1] * cr, v[2]]
}

/// Translation from native RDB position to the Bevy frame: `(XPos, −YPos, ZPos) ×
/// GlobalScale` (1/40) + df→bevy z-negation.
fn translate_rdb(position_raw: [i32; 3]) -> [f32; 3] {
    [
        position_raw[0] as f32 / UNITS_PER_METER,
        -position_raw[1] as f32 / UNITS_PER_METER,
        -position_raw[2] as f32 / UNITS_PER_METER,
    ]
}

/// Accumulates `spec` rotated (3 axes) and translated into the interior buffers.
///
/// UVs (invariant under rotation/translation) and texture keys
/// (M12) are copied per vertex; without data (synthetic spec) they are filled
/// with defaults to preserve 1:1 alignment with `positions` — same
/// pusher as [`crate::city_retail::push_placed`].
///
/// 8 args by structural shape (5 buffers + spec + position + rotation) —
/// the pusher is internal and its shape is deliberate (same documented allow
/// as `city_spec::push_quad`, clippy `-D warnings` gate).
#[allow(clippy::too_many_arguments)]
fn push_placed(
    positions: &mut Vec<[f32; 3]>,
    normals: &mut Vec<[f32; 3]>,
    indices: &mut Vec<u32>,
    uvs: &mut Vec<[f32; 2]>,
    face_textures: &mut Vec<u32>,
    spec: &MeshSpec,
    position: [f32; 3],
    rotation_da: [i32; 3],
) {
    if spec.indices.is_empty() {
        return;
    }
    let base = positions.len() as u32;
    for (i, v) in spec.positions.iter().enumerate() {
        let r = rotate_rdb(*v, rotation_da);
        positions.push([r[0] + position[0], r[1] + position[1], r[2] + position[2]]);
        let n = rotate_rdb(spec.normals[i], rotation_da);
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

/// Composes the interior from ONE already-parsed `.RDB`.
///
/// `resolve_model` maps an ARCH3D record → [`MeshSpec`] (Mode A custody of the
/// caller; without a spec the placement is counted in
/// [`RetailInterior::unresolved_models`]). Iteration = order of
/// `RdbBlock::objects` (roots in cell order, chains in link
/// order — parser determinism).
pub fn build_retail_interior(
    block_name: &str,
    block: &RdbBlock,
    resolve_model: &mut dyn FnMut(u32) -> Option<MeshSpec>,
) -> RetailInterior {
    let mut models = Vec::new();
    let mut lights = Vec::new();
    let mut flats = Vec::new();
    let mut mp = Vec::new();
    let mut mn = Vec::new();
    let mut mi = Vec::new();
    let mut muv = Vec::new();
    let mut mft = Vec::new();
    let mut unresolved = 0usize;

    for object in &block.objects {
        let position = translate_rdb(object.position);
        match object.resource {
            RdbResource::Model(model) => {
                let rotation_da = [model.x_rotation, model.y_rotation, model.z_rotation];
                let resolved = block
                    .model_id(model.model_index)
                    .and_then(|id| resolve_model(id).map(|spec| (id, spec)));
                match resolved {
                    Some((id, spec)) => {
                        let triangles = spec.triangle_count();
                        push_placed(&mut mp, &mut mn, &mut mi, &mut muv, &mut mft, &spec, position, rotation_da);
                        models.push(RetailModel {
                            position,
                            model_id: id,
                            rotation_da,
                            triangles,
                            trigger_flag: model.trigger_flag,
                        });
                    }
                    None => unresolved += 1,
                }
            }
            RdbResource::Light(light) => lights.push(RetailLight {
                position,
                radius_raw: light.radius,
                radius_m: light.radius as f32 / UNITS_PER_METER,
            }),
            RdbResource::Flat(flat) => flats.push(RetailFlat {
                position,
                texture_archive: flat.texture_archive(),
                texture_record: flat.texture_record(),
            }),
        }
    }

    RetailInterior {
        block_name: block_name.to_string(),
        width: block.header.width,
        height: block.header.height,
        models,
        lights,
        flats,
        mesh: MeshSpec {
            positions: mp,
            normals: mn,
            indices: mi,
            uvs: muv,
            face_textures: mft,
        },
        unresolved_models: unresolved,
    }
}

/// World of the real variant: **street strip** (4 flat quads y=0
/// around the block — documented delta: the street is context, the
/// interior belongs to the `.RDB`) + the record mesh. Input of
/// [`crate::CollisionGrid`] for the headless probe.
pub fn retail_world(interior: &RetailInterior) -> MeshSpec {
    let Some((bmin, bmax)) = interior.mesh.bounds() else {
        return interior.mesh.clone();
    };
    let (x0, x1) = (bmin[0] - STREET_MARGIN_M, bmax[0] + STREET_MARGIN_M);
    let (z0, z1) = (bmin[2] - STREET_MARGIN_M, bmax[2] + STREET_MARGIN_M);
    let mut gp = Vec::new();
    let mut gn = Vec::new();
    let mut gi = Vec::new();
    // North, south, east and west strips — without stepping on the block
    // footprint (the interior floor comes from the `.RDB` itself).
    let strips: [[f32; 4]; 4] = [
        [x0, z1, x1, z1 + STREET_MARGIN_M],
        [x0, z0 - STREET_MARGIN_M, x1, z0],
        [x1, z0, x1 + STREET_MARGIN_M, z1],
        [x0 - STREET_MARGIN_M, z0, x0, z1],
    ];
    for [sx0, sz0, sx1, sz1] in strips {
        push_quad(
            &mut gp,
            &mut gn,
            &mut gi,
            [0.0, 1.0, 0.0],
            [sx0, 0.0, sz0],
            [sx1, 0.0, sz0],
            [sx1, 0.0, sz1],
            [sx0, 0.0, sz1],
        );
    }
    let ground = MeshSpec {
        positions: gp,
        normals: gn,
        indices: gi,
        // Synthetic street strip (M12): no retail texture —
        // `concat_specs` fills defaults and preserves alignment.
        uvs: Vec::new(),
        face_textures: Vec::new(),
    };
    concat_specs(&[&ground, &interior.mesh])
}

/// Side of the block perimeter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    /// Face `x = x_max` (entered walking along −X).
    PosX,
    /// Face `x = x_min` (entered walking along +X).
    NegX,
    /// Face `z = z_max` (entered walking along −Z).
    PosZ,
    /// Face `z = z_min` (entered walking along +Z).
    NegZ,
}

impl Side {
    /// Canonical sweep order (determinism).
    pub const ALL: [Side; 4] = [Side::PosX, Side::NegX, Side::PosZ, Side::NegZ];

    /// Axis normal to the face (0 = X, 2 = Z).
    pub const fn axis(self) -> usize {
        match self {
            Self::PosX | Self::NegX => 0,
            Self::PosZ | Self::NegZ => 2,
        }
    }

    /// Lateral axis of the face (sweep traversal).
    pub const fn lateral_axis(self) -> usize {
        2 - self.axis()
    }

    /// Sign of the march toward the interior.
    pub const fn inward_sign(self) -> f32 {
        match self {
            Self::PosX | Self::PosZ => -1.0,
            Self::NegX | Self::NegZ => 1.0,
        }
    }

    /// Face label for evidence.
    pub const fn label(self) -> &'static str {
        match self {
            Self::PosX => "+X",
            Self::NegX => "-X",
            Self::PosZ => "+Z",
            Self::NegZ => "-Z",
        }
    }
}

/// Result of a march with a parametric plane (like
/// [`crate::march_axis`], but the crossing is measured against the block face
/// plane — the M4 march with `SHOP_X_MAX` remains intact).
///
/// **No local traversal counter (N1):** `resolve_step` guarantees the resulting
/// position is `is_free`, so a local `!grid.is_free` counter can only report 0.
/// The no-traversal invariant of this variant is judged ONCE by the generic
/// `view_no_traversal_walk` (independent geometry oracle over the same world) —
/// the duplicate counter was removed instead of kept as dead evidence.
#[derive(Debug, Clone)]
struct RealMarch {
    pos: [f32; 3],
    steps: usize,
    reached: bool,
    blocked: Option<CellStatus>,
    /// Positions that crossed the face plane (either direction).
    crossings: Vec<[f32; 3]>,
}

/// Single-axis march to `target` or until it hits something (no sliding: pure
/// per-axis delta, INV-L99-5 invariant — the resulting position is always
/// `is_free`). `plane` is the face plane used to detect crossings.
fn march_real(
    grid: &CollisionGrid,
    from: [f32; 3],
    axis: usize,
    sign: f32,
    target: f32,
    plane: f32,
) -> RealMarch {
    let mut pos = from;
    let mut steps = 0usize;
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
        // `rel > 0` = interior side of the face (`sign` points inward).
        let was_inside = (pos[axis] - plane) * sign > 0.0;
        pos = outcome.pos;
        steps += 1;
        let now_inside = (pos[axis] - plane) * sign > 0.0;
        if was_inside != now_inside {
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
    RealMarch {
        pos,
        steps,
        reached,
        blocked,
        crossings,
    }
}

/// Classification of a lateral sample from the sweep of a face.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Sample {
    /// The march entered `ENTER_DEPTH_M` inside the footprint.
    Entered,
    /// Solid geometry stopped the march (real wall).
    Wall,
    /// Other blockage (outside the mesh / boundary) — does not count as a wall.
    Other,
}

/// Evidence of the lateral sweep of a face.
#[derive(Debug, Clone)]
struct SideScan {
    side: Side,
    /// `(lateral sample, classification)` in ascending order.
    samples: Vec<(f32, Sample)>,
}

impl SideScan {
    /// First opening flanked by walls: lateral `(start, end)`.
    fn flanked_gap(&self) -> Option<(f32, f32)> {
        let mut i = 0usize;
        while i < self.samples.len() {
            if self.samples[i].1 != Sample::Entered {
                i += 1;
                continue;
            }
            let start = i;
            while i + 1 < self.samples.len() && self.samples[i + 1].1 == Sample::Entered {
                i += 1;
            }
            let end = i;
            let wall_before = start > 0 && self.samples[start - 1].1 == Sample::Wall;
            let wall_after =
                end + 1 < self.samples.len() && self.samples[end + 1].1 == Sample::Wall;
            if wall_before && wall_after {
                return Some((self.samples[start].0, self.samples[end].0));
            }
            i += 1;
        }
        None
    }

    fn walls(&self) -> usize {
        self.samples
            .iter()
            .filter(|(_, s)| *s == Sample::Wall)
            .count()
    }

    fn entered(&self) -> usize {
        self.samples
            .iter()
            .filter(|(_, s)| *s == Sample::Entered)
            .count()
    }
}

/// Mesh footprint in `(x, z)` (without the Y axis).
fn xz_bounds(interior: &RetailInterior) -> Option<((f32, f32), (f32, f32))> {
    let (bmin, bmax) = interior.mesh_bounds()?;
    Some(((bmin[0], bmax[0]), (bmin[2], bmax[2])))
}

/// Is the point inside the block footprint (with inset)?
fn inside_footprint(p: [f32; 3], x: (f32, f32), z: (f32, f32)) -> bool {
    p[0] >= x.0 + INSIDE_INSET_M
        && p[0] <= x.1 - INSIDE_INSET_M
        && p[2] >= z.0 + INSIDE_INSET_M
        && p[2] <= z.1 - INSIDE_INSET_M
}

/// Plane of a face and the ends of its lateral axis.
fn side_geometry(side: Side, x: (f32, f32), z: (f32, f32)) -> (f32, (f32, f32)) {
    match side {
        Side::PosX => (x.1, z),
        Side::NegX => (x.0, z),
        Side::PosZ => (z.1, x),
        Side::NegZ => (z.0, x),
    }
}

/// Lateral sweep of a face: for each lateral grid **cell** (sample at the
/// cell center, aligned with the 1 m grain — a 1 m passage is only
/// passable through its center), march from the street to
/// `ENTER_DEPTH_M` inside the footprint.
fn scan_side(grid: &CollisionGrid, side: Side, x: (f32, f32), z: (f32, f32)) -> SideScan {
    let (plane, lateral) = side_geometry(side, x, z);
    let axis = side.axis();
    let sign = side.inward_sign();
    let lat = side.lateral_axis();
    let mut probe = [0.0f32; 3];
    probe[axis] = plane;
    probe[1] = EYE_HEIGHT_M;
    probe[lat] = lateral.0;
    let c_lo = grid.cell_of(probe)[lat];
    probe[lat] = lateral.1;
    let c_hi = grid.cell_of(probe)[lat];
    let base_cell = grid.cell_of(probe);
    let mut samples = Vec::new();
    for c in c_lo..=c_hi {
        let mut cell = base_cell;
        cell[lat] = c;
        let l = grid.cell_center(cell)[lat];
        let mut from = [0.0f32; 3];
        from[axis] = plane - sign * APPROACH_M;
        from[lat] = l;
        from[1] = EYE_HEIGHT_M;
        let march = march_real(grid, from, axis, sign, plane + sign * ENTER_DEPTH_M, plane);
        let sample = if march.reached {
            Sample::Entered
        } else if march.blocked == Some(CellStatus::Solid) {
            Sample::Wall
        } else {
            Sample::Other
        };
        samples.push((l, sample));
    }
    SideScan { side, samples }
}

/// Full report of the **real variant** of PROBE-INTERIOR-001.
#[derive(Debug, Clone)]
pub struct RetailInteriorReport {
    /// Executed checks, in order (the first 5 are the generic M2 layer).
    pub checks: Vec<ProbeCheck>,
    /// Global PASS = every check ok.
    pub pass: bool,
    /// Side and lateral range of the real door (`None` if there is none).
    pub door: Option<(Side, f32, f32)>,
}

/// Runs the real variant of PROBE-INTERIOR-001 over an interior
/// assembled from `.RDB` (DoD §b): street → real door → interior →
/// door → street with 0 traversals (INV-L99-5).
pub fn run_retail_interior_probe(interior: &RetailInterior) -> RetailInteriorReport {
    let mut checks: Vec<ProbeCheck> = Vec::with_capacity(12);
    let world = retail_world(interior);

    // ── Generic M2 layer (literal reuse of run_view_probe — 5 original ids)
    for check in run_view_probe(&world).checks {
        checks.push(check);
    }

    let skip = |checks: &mut Vec<ProbeCheck>, ids: [&'static str; 5]| {
        for id in ids {
            checks.push(ProbeCheck {
                id,
                ok: false,
                detail: "SKIP: interior without geometry".into(),
            });
        }
    };

    // ── 1. Census: walls/props/lights come from the `.RDB` record ────────
    let (n_models, n_lights, n_flats) = interior.counts();
    checks.push(ProbeCheck {
        id: "retail_from_rdb_records",
        ok: n_models >= 1
            && n_lights >= 1
            && n_flats >= 1
            && interior.unresolved_models == 0
            && interior.mesh.triangle_count() > 0,
        detail: format!(
            "record {} ({}×{} cells · nominal footprint {:.1}×{:.1} m) · models {} · lights {} · \
             flats {} · pending ARCH3D {} (0 req) · {} real triangles · ids [{}]",
            interior.block_name,
            interior.width,
            interior.height,
            interior.width.max(1) as f32 * RDB_SIDE_M,
            interior.height.max(1) as f32 * RDB_SIDE_M,
            n_models,
            n_lights,
            n_flats,
            interior.unresolved_models,
            interior.mesh.triangle_count(),
            interior
                .models
                .iter()
                .map(|m| m.model_id.to_string())
                .collect::<Vec<_>>()
                .join(","),
        ),
    });

    let Some(((x0, x1), (z0, z1))) = xz_bounds(interior) else {
        checks.push(ProbeCheck {
            id: "retail_placement_in_bounds",
            ok: false,
            detail: "FAIL: interior without geometry (undefined footprint)".into(),
        });
        skip(
            &mut checks,
            [
                "retail_door_flanked_by_walls",
                "retail_street_to_door",
                "retail_wall_blocks_without_door",
                "retail_interior_walk_contained",
                "retail_exit_through_door",
            ],
        );
        return RetailInteriorReport {
            checks,
            pass: false,
            door: None,
        };
    };

    // ── 2. Scale/placement: every origin inside the nominal footprint ────
    let (nx0, nx1) = (0.0f32, interior.width.max(1) as f32 * RDB_SIDE_M);
    let (nz0, nz1) = (-(interior.height.max(1) as f32 * RDB_SIDE_M), 0.0f32);
    let tol = 1.0f32;
    let placements = interior
        .models
        .iter()
        .map(|m| ("model", m.position))
        .chain(interior.lights.iter().map(|l| ("light", l.position)))
        .chain(interior.flats.iter().map(|f| ("flat", f.position)));
    let mut total = 0usize;
    let mut out_of_bounds = 0usize;
    let mut first_offender = String::new();
    for (kind, p) in placements {
        total += 1;
        let inside = p[0] >= nx0 - tol
            && p[0] <= nx1 + tol
            && p[2] >= nz0 - tol
            && p[2] <= nz1 + tol;
        if !inside {
            out_of_bounds += 1;
            if first_offender.is_empty() {
                first_offender = format!("{kind} ({:.1},{:.1},{:.1})", p[0], p[1], p[2]);
            }
        }
    }
    checks.push(ProbeCheck {
        id: "retail_placement_in_bounds",
        ok: out_of_bounds == 0 && total > 0,
        detail: format!(
            "{total} placements inside the nominal footprint x[{nx0:.1},{nx1:.1}] \
             z[{nz0:.1},{nz1:.1}] (tol {tol} m · scale 40 u/m) · outside {} (0 req){extra} · \
             mesh x[{x0:.1},{x1:.1}] z[{z0:.1},{z1:.1}]",
            out_of_bounds,
            extra = if first_offender.is_empty() {
                String::new()
            } else {
                format!(" · first: {first_offender}")
            },
        ),
    });

    let grid = CollisionGrid::from_mesh_spec(&world, DEFAULT_CELL_M);
    let scans: Vec<SideScan> = Side::ALL
        .iter()
        .map(|side| scan_side(&grid, *side, (x0, x1), (z0, z1)))
        .collect();

    // ── 3. Real door flanked by walls (sweep of the 4 sides) ────────────
    let door = scans
        .iter()
        .find_map(|scan| scan.flanked_gap().map(|(from, to)| (scan.side, from, to)));
    let scan_summary = scans
        .iter()
        .map(|s| {
            format!(
                "{}: {} samples · {} walls · {} entered",
                s.side.label(),
                s.samples.len(),
                s.walls(),
                s.entered()
            )
        })
        .collect::<Vec<_>>()
        .join(" · ");
    checks.push(ProbeCheck {
        id: "retail_door_flanked_by_walls",
        ok: door.is_some(),
        detail: match door {
            Some((side, from, to)) => format!(
                "real door on face {} lateral [{from:.1},{to:.1}] m flanked by walls · \
                 {scan_summary}",
                side.label(),
            ),
            None => format!(
                "no wall-flanked opening on the 4 sides · {scan_summary}"
            ),
        },
    });

    let Some((side, gap_from, gap_to)) = door else {
        for id in [
            "retail_street_to_door",
            "retail_wall_blocks_without_door",
            "retail_interior_walk_contained",
            "retail_exit_through_door",
        ] {
            checks.push(ProbeCheck {
                id,
                ok: false,
                detail: "SKIP: no real flanked door".into(),
            });
        }
        return RetailInteriorReport {
            checks,
            pass: false,
            door: None,
        };
    };

    let (plane, lateral) = side_geometry(side, (x0, x1), (z0, z1));
    let axis = side.axis();
    let sign = side.inward_sign();
    let lat_axis = side.lateral_axis();
    let gap_mid = (gap_from + gap_to) * 0.5;
    let in_gap = |l: f32| l + PLAYER_RADIUS_M >= gap_from && l - PLAYER_RADIUS_M <= gap_to;
    let span = if axis == 0 { x1 - x0 } else { z1 - z0 };

    // ── 4. Street → door → interior (0 traversals · 1 crossing at the door)
    let mut approach = [0.0f32; 3];
    approach[axis] = plane - sign * APPROACH_M;
    approach[lat_axis] = gap_mid;
    approach[1] = EYE_HEIGHT_M;
    let enter = march_real(
        &grid,
        approach,
        axis,
        sign,
        plane + sign * ENTER_DEPTH_M,
        plane,
    );
    let entered_inside = inside_footprint(enter.pos, (x0, x1), (z0, z1));
    checks.push(ProbeCheck {
        id: "retail_street_to_door",
        ok: enter.reached
            && enter.crossings.len() == 1
            && enter.crossings.iter().all(|c| in_gap(c[lat_axis]))
            && entered_inside,
        detail: format!(
            "street {:.1} → interior through the door on {}: {steps}p at {MARCH_STEP_M} m · \
             pos=({:.1},{:.1},{:.1}) · inside_footprint={inside} · crossings={ncross} (expected 1) · \
             z crossing [{zs}]",
            plane - sign * APPROACH_M,
            side.label(),
            enter.pos[0],
            enter.pos[1],
            enter.pos[2],
            steps = enter.steps,
            inside = entered_inside,
            ncross = enter.crossings.len(),
            zs = enter
                .crossings
                .iter()
                .map(|c| format!("{:.2}", c[lat_axis]))
                .collect::<Vec<_>>()
                .join(", "),
        ),
    });

    // ── 5. Real wall WITHOUT a door: lateral approach blocked ────────────
    let scan = scans.iter().find(|s| s.side == side).unwrap();
    let wall_lateral = scan
        .samples
        .iter()
        .filter(|(l, s)| *s == Sample::Wall && (*l - gap_mid).abs() > 1.0)
        .max_by(|a, b| a.0.abs().partial_cmp(&b.0.abs()).unwrap())
        .or_else(|| scan.samples.iter().find(|(_, s)| *s == Sample::Wall))
        .map(|(l, _)| *l);
    let wall_check = match wall_lateral {
        Some(l) => {
            let mut from = [0.0f32; 3];
            from[axis] = plane - sign * APPROACH_M;
            from[lat_axis] = l;
            from[1] = EYE_HEIGHT_M;
            let m = march_real(&grid, from, axis, sign, plane + sign * ENTER_DEPTH_M, plane);
            ProbeCheck {
                id: "retail_wall_blocks_without_door",
                ok: m.blocked == Some(CellStatus::Solid)
                    && !m.reached
                    && !inside_footprint(m.pos, (x0, x1), (z0, z1)),
                detail: format!(
                    "march on {} toward face {} at lateral {l:.1}: blocked at {:.2} \
                     (reason {:?}) · entered={}",
                    side.label(),
                    side.label(),
                    m.pos[axis],
                    m.blocked,
                    inside_footprint(m.pos, (x0, x1), (z0, z1)),
                ),
            }
        }
        None => ProbeCheck {
            id: "retail_wall_blocks_without_door",
            ok: false,
            detail: "no wall sample on the door face".into(),
        },
    };
    checks.push(wall_check);

    // ── 6. Contained interior: marches inside without traversing + real wall
    let deep = march_real(
        &grid,
        enter.pos,
        axis,
        sign,
        plane + sign * (span * 0.6),
        plane,
    );
    let (lat_min, lat_max) = lateral;
    let lat_plus = march_real(
        &grid,
        enter.pos,
        lat_axis,
        1.0,
        if lat_axis == 2 { z1 + 2.0 } else { x1 + 2.0 },
        plane,
    );
    let lat_minus = march_real(
        &grid,
        enter.pos,
        lat_axis,
        -1.0,
        if lat_axis == 2 { z0 - 2.0 } else { x0 - 2.0 },
        plane,
    );
    let inner_blocked = [deep.blocked, lat_plus.blocked, lat_minus.blocked]
        .iter()
        .filter(|b| **b == Some(CellStatus::Solid))
        .count();
    checks.push(ProbeCheck {
        id: "retail_interior_walk_contained",
        ok: inner_blocked >= 1
            && inside_footprint(deep.pos, (x0, x1), (z0, z1)),
        detail: format!(
            "inside: {} {d_steps}p → {:.2} (block {:?}, target {:.1}) · +lateral {p_steps}p → \
             {:.2} (block {:?}) · −lateral {m_steps}p → {:.2} (block {:?}) · lateral \
             [{lat_min:.1},{lat_max:.1}] · solid blocks {inner_blocked}/3 (≥1 req)",
            side.label(),
            deep.pos[axis],
            deep.blocked,
            plane + sign * (span * 0.6),
            lat_plus.pos[lat_axis],
            lat_plus.blocked,
            lat_minus.pos[lat_axis],
            lat_minus.blocked,
            d_steps = deep.steps,
            p_steps = lat_plus.steps,
            m_steps = lat_minus.steps,
        ),
    });

    // ── 7. Exit through the SAME door ────────────────────────────────────
    let exit = march_real(
        &grid,
        enter.pos,
        axis,
        -sign,
        plane - sign * APPROACH_M,
        plane,
    );
    let exited = !inside_footprint(exit.pos, (x0, x1), (z0, z1));
    checks.push(ProbeCheck {
        id: "retail_exit_through_door",
        ok: exit.reached
            && exit.crossings.len() == 1
            && exit.crossings.iter().all(|c| in_gap(c[lat_axis]))
            && exited,
        detail: format!(
            "exit {} from {:.1}: {steps}p · lateral {:.1} · outside_footprint={exited} · \
             crossings={ncross} (expected 1) · z crossing [{zs}]",
            side.label(),
            enter.pos[axis],
            exit.pos[lat_axis],
            steps = exit.steps,
            exited = exited,
            ncross = exit.crossings.len(),
            zs = exit
                .crossings
                .iter()
                .map(|c| format!("{:.2}", c[lat_axis]))
                .collect::<Vec<_>>()
                .join(", "),
        ),
    });

    let pass = checks.iter().all(|c| c.ok);
    RetailInteriorReport {
        checks,
        pass,
        door: Some((side, gap_from, gap_to)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use daggerfall_parser_rs::rdb::{
        object_root_offset_expected, RdbFlatResource, RdbHeader, RdbLightResource,
        RdbModelReference, RdbModelResource, RdbObject,
    };

    /// Elements of the fixture's `ModelReferenceList`.
    const MODEL_REFS: usize = 750;

    /// `record` model = 20×20×3 m room with a **real 2 m opening** on
    /// the +X face (z ∈ [−11,−9]) — the opening the probe must find
    /// flanked by walls.
    fn room_spec() -> MeshSpec {
        let mut positions = Vec::new();
        let mut normals = Vec::new();
        let mut indices = Vec::new();
        // Floor (y=0) and ceiling (y=3).
        push_quad(
            &mut positions,
            &mut normals,
            &mut indices,
            [0.0, 1.0, 0.0],
            [0.0, 0.0, -20.0],
            [20.0, 0.0, -20.0],
            [20.0, 0.0, 0.0],
            [0.0, 0.0, 0.0],
        );
        push_quad(
            &mut positions,
            &mut normals,
            &mut indices,
            [0.0, -1.0, 0.0],
            [0.0, 3.0, -20.0],
            [20.0, 3.0, -20.0],
            [20.0, 3.0, 0.0],
            [0.0, 3.0, 0.0],
        );
        // Full wall at x=0; the x=20 wall only leaves the flanking spans
        // of the opening (z ∈ [−20,−11] and z ∈ [−9,0] further below).
        push_quad(
            &mut positions,
            &mut normals,
            &mut indices,
            [1.0, 0.0, 0.0],
            [0.0, 0.0, -20.0],
            [0.0, 0.0, 0.0],
            [0.0, 3.0, 0.0],
            [0.0, 3.0, -20.0],
        );
        // Full walls at z=0 and z=−20.
        for (z, nz) in [(0.0f32, -1.0f32), (-20.0, 1.0)] {
            push_quad(
                &mut positions,
                &mut normals,
                &mut indices,
                [0.0, 0.0, nz],
                [0.0, 0.0, z],
                [20.0, 0.0, z],
                [20.0, 3.0, z],
                [0.0, 3.0, z],
            );
        }
        // Opening at x=20: spans z ∈ [−20,−11] and z ∈ [−9,0].
        for (z_from, z_to) in [(-20.0f32, -11.0), (-9.0, 0.0)] {
            push_quad(
                &mut positions,
                &mut normals,
                &mut indices,
                [-1.0, 0.0, 0.0],
                [20.0, 0.0, z_from],
                [20.0, 0.0, z_to],
                [20.0, 3.0, z_to],
                [20.0, 3.0, z_from],
            );
        }
        MeshSpec {
            positions,
            normals,
            indices,
            uvs: Vec::new(),
            face_textures: Vec::new(),
        }
    }

    /// Synthetic `.RDB` block (0 I/O): 1 model (the room) + 1 light +
    /// 1 flat, with the exact shape of the real record.
    fn synthetic_rdb() -> RdbBlock {
        let mut model_references = vec![
            RdbModelReference {
                model_id: Some(44005),
                description: *b"ROO",
            };
            MODEL_REFS
        ];
        model_references[0].description = *b"DOR";
        RdbBlock {
            header: RdbHeader {
                unknown1: 0,
                width: 1,
                height: 1,
                object_root_offset: object_root_offset_expected() as u32,
                unknown2: 0,
            },
            model_references,
            model_data: vec![0; MODEL_REFS],
            roots: vec![532],
            objects: vec![
                RdbObject {
                    offset: 532,
                    next: -1,
                    previous: -1,
                    position: [0, 0, 0],
                    resource: RdbResource::Model(RdbModelResource {
                        x_rotation: 0,
                        y_rotation: 0,
                        z_rotation: 0,
                        model_index: 0,
                        trigger_flag: 0,
                        sound_index: 0,
                        action_offset: -1,
                        action: None,
                    }),
                },
                RdbObject {
                    offset: 1044,
                    next: -1,
                    previous: -1,
                    position: [400, 0, 400],
                    resource: RdbResource::Light(RdbLightResource {
                        unknown1: 0,
                        unknown2: 0,
                        radius: 1024,
                    }),
                },
                RdbObject {
                    offset: 1556,
                    next: -1,
                    previous: -1,
                    position: [800, 0, 800],
                    resource: RdbResource::Flat(RdbFlatResource {
                        texture_bitfield: 0x1234,
                        flags: 0,
                        magnitude: 0,
                        sound_index: 0,
                        next_object_offset: -1,
                        action: 0,
                    }),
                },
            ],
        }
    }

    fn build_synthetic() -> RetailInterior {
        let block = synthetic_rdb();
        let mut resolve = |_id: u32| Some(room_spec());
        build_retail_interior("SYNTHETIC0.RDB", &block, &mut resolve)
    }

    #[test]
    fn assembly_counts_and_scale() {
        let interior = build_synthetic();
        assert_eq!(interior.counts(), (1, 1, 1));
        assert_eq!(interior.unresolved_models, 0);
        assert!(interior.mesh.triangle_count() > 0);
        // Light: 1024 raw ÷ 40 = 25.6 m (GlobalScale 0.025).
        assert!((interior.lights[0].radius_m - 25.6).abs() < 1e-4);
        // Position: 400 raw ÷ 40 = 10.0 m on x; ZPos 400 → z = −10.0 m
        // (df→bevy z-negation).
        assert!((interior.lights[0].position[0] - 10.0).abs() < 1e-4);
        assert!((interior.lights[0].position[2] + 10.0).abs() < 1e-4);
        // Flat: 800 raw ÷ 40 = 20.0 m on x.
        assert!((interior.flats[0].position[0] - 20.0).abs() < 1e-4);
        assert!((interior.flats[0].position[2] + 20.0).abs() < 1e-4);
    }

    #[test]
    fn rotation_da_maps_to_bevy_frame() {
        // 512 DA = 90° (2048 DA = 360°). YRot → positive Bevy yaw: +X → −Z.
        let v = rotate_rdb([1.0, 0.0, 0.0], [0, 512, 0]);
        assert!(v[0].abs() < 1e-4, "x={}", v[0]);
        assert!((v[2] + 1.0).abs() < 1e-4, "z={}", v[2]);
        // XRot → pitch: +Y → +Z.
        let v = rotate_rdb([0.0, 1.0, 0.0], [512, 0, 0]);
        assert!(v[1].abs() < 1e-4, "y={}", v[1]);
        assert!((v[2] - 1.0).abs() < 1e-4, "z={}", v[2]);
        // ZRot = −512 raw DA ⇒ Bevy roll = +90°: +X → +Y.
        let v = rotate_rdb([1.0, 0.0, 0.0], [0, 0, -512]);
        assert!((v[1] - 1.0).abs() < 1e-4, "y={}", v[1]);
        assert!(v[0].abs() < 1e-4, "x={}", v[0]);
    }

    #[test]
    fn probe_passes_on_synthetic_block() {
        let interior = build_synthetic();
        let report = run_retail_interior_probe(&interior);
        for check in &report.checks {
            assert!(check.ok, "check {} FAIL: {}", check.id, check.detail);
        }
        assert!(report.pass, "real variant must PASS: {:?}", report.checks);
        let (side, from, to) = report.door.expect("real door detected");
        assert_eq!(side, Side::PosX);
        // A 1-cell (1 m) opening is a valid door: `from == to` OK.
        assert!(to >= from, "inverted opening: from={from} to={to}");
        assert_eq!(
            report.checks.len(),
            12,
            "generic layer (5) + RDB/door/interior layer (7)"
        );
    }

    #[test]
    fn probe_is_deterministic() {
        let a = run_retail_interior_probe(&build_synthetic());
        let b = run_retail_interior_probe(&build_synthetic());
        assert_eq!(a.pass, b.pass);
        assert_eq!(a.door, b.door);
        assert_eq!(
            a.checks
                .iter()
                .map(|c| (c.id, c.ok, c.detail.as_str()))
                .collect::<Vec<_>>(),
            b.checks
                .iter()
                .map(|c| (c.id, c.ok, c.detail.as_str()))
                .collect::<Vec<_>>(),
            "same input → same report"
        );
        assert_eq!(build_synthetic().digest(), build_synthetic().digest());
    }

    #[test]
    fn probe_fails_without_real_geometry() {
        // Tamper: with no resolvable ARCH3D there is no interior — the probe MUST
        // fail (anti-false-positive: 0 PASS with an empty interior).
        let block = synthetic_rdb();
        let mut resolve = |_id: u32| -> Option<MeshSpec> { None };
        let interior = build_retail_interior("SYNTHETIC0.RDB", &block, &mut resolve);
        assert_eq!(interior.mesh.triangle_count(), 0);
        assert_eq!(interior.unresolved_models, 1);
        let report = run_retail_interior_probe(&interior);
        assert!(!report.pass, "an empty interior cannot PASS");
        assert!(
            report
                .checks
                .iter()
                .any(|c| c.id == "retail_from_rdb_records" && !c.ok),
            "the census must detect the empty interior"
        );
    }
}
