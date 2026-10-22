//! `.RDB` (Dungeon/Interior Block Records of BLOCKS.BSA) — clean-room parser
//! (M11, REQ-L99-043).
//!
//! Spec: UESP `Daggerfall:BLOCKS.BSA/Dungeon_Block_Records` (verified
//! 2026-10-20) + reference reader `DFU:Assets/Scripts/API/BlocksFile.cs`
//! (`ReadRdbHeader`/`ReadRdbModelReferenceList`/`ReadRdbModelDataList`/
//! `ReadRdbObjectSectionHeader`/`ReadRdbObjectSectionRootList`/
//! `ReadRdbObjectLists`, the pinned DFU MIT corpus does not include this
//! reader → spec = UESP + cited MIT upstream, same regimen as `.RMB` in M10).
//!
//! Layout (UESP Dungeon Block Records; `DISK:BLOCKS.BSA 32884997` — the
//! `.RDB` files live in the SAME BSA as the `.RMB` ones, 187/1295 records
//! measured):
//! ```text
//! RdbHeader (20 B: Unknown1 u32 · Width u32 · Height u32 ·
//!   ObjectRootOffset u32 · Unknown2 u32)
//! ModelReferenceList (750 × 8 B: ModelId ASCII[5] + Description ASCII[3];
//!   unused = 0xff)
//! ModelDataList (750 × 4 B: unknown UInt32 — DFU reads it and ignores it)
//! ObjectSection:
//!   ObjectHeader (512 B: 5×u32 + Unknown4[32] + "DAGR" + Unknown5[456];
//!   ObjectRootOffset MUST point right after — DFU throws otherwise)
//!   ObjectRootList (Width×Height × i32: absolute offset of the head of the
//!   linked list; negative = empty cell)
//!   ObjectList (25 B nodes: Next i32 · Previous i32 · Position Point3
//!   i32×3 · Type u8 · ResourceOffset u32) + resources per type:
//!   Model (23 B: XRot/YRot/ZRot i32 · ModelIndex u16 · TriggerFlag u32 ·
//!     SoundIndex u8 · ActionOffset i32) + Action (10 B if ActionOffset > 0:
//!     Axis u8 · Duration u16 · Magnitude u16 · NextObjectOffset i32 ·
//!     Flags u8)
//!   Light (10 B: Unknown1 u32 · Unknown2 u32 · Radius u16)
//!   Flat (11 B: TextureBitfield u16 · Flags u16 · Magnitude u8 ·
//!     SoundIndex u8 · NextObjectOffset i32 · Action u8)
//! ```
//!
//! Semantics (DFU `BlocksFile.cs` + `RDBLayout.cs`): `Type` 0x01=Model,
//! 0x02=LightSource, 0x03=Flat; the `ModelIndex` of the Model indexes
//! `ModelReferenceList`, whose `ModelId` is the ARCH3D.BSA record; each
//! `ObjectRootList[i]` is the head of a doubly linked list whose
//! negative `Next` terminates the chain (DFU counts and re-reads separately
//! — here it is resolved in a single pass with dedup by object offset, the
//! same iteration order).
//!
//! Determinism: `ObjectRootList` is iterated in cell order and each chain
//! in link order; an object reached by two roots is emitted ONCE
//! (file offset as identity).

/// Fixed length of `RdbHeader` (UESP: 20 B).
pub const RDB_HEADER_LEN: usize = 20;
/// Elements of `ModelReferenceList` (UESP: always 750).
pub const MODEL_REFERENCE_COUNT: usize = 750;
/// Length of a `ModelReference` (ModelId[5] + Description[3]).
pub const MODEL_REFERENCE_LEN: usize = 8;
/// Length of a `ModelData` (unknown UInt32).
pub const MODEL_DATA_LEN: usize = 4;
/// Length of `ObjectHeader` (5 + 32 + 4 + 456 bytes after the 20 of
/// numbered headers → 512 total; `DFU:BlocksFile.cs` reads 512 B).
pub const OBJECT_SECTION_HEADER_LEN: usize = 512;
/// Length of an `Object` node (Next+Previous+Point3+Type+ResourceOffset).
pub const OBJECT_LEN: usize = 25;
/// Length of the `Model` resource (rotations + ModelIndex + lock + action).
pub const MODEL_RESOURCE_LEN: usize = 23;
/// Length of the `Light` resource (Unknown1 + Unknown2 + Radius).
pub const LIGHT_RESOURCE_LEN: usize = 10;
/// Length of the `Flat` resource (texture + flags + magnitude + sound + …).
pub const FLAT_RESOURCE_LEN: usize = 11;
/// Length of the `Action` record (axis + duration + magnitude + target).
pub const ACTION_LEN: usize = 10;

/// Offset of `ObjectSection` (after header + 750 references + 750
/// model data): `20 + 6000 + 3000 = 9020`.
pub const fn object_section_offset() -> usize {
    RDB_HEADER_LEN + MODEL_REFERENCE_COUNT * (MODEL_REFERENCE_LEN + MODEL_DATA_LEN)
}

/// Offset that `RdbHeader.ObjectRootOffset` MUST report: end of
/// `ObjectHeader` (`DFU:BlocksFile.cs` throws if the position does not match).
pub const fn object_root_offset_expected() -> usize {
    object_section_offset() + OBJECT_SECTION_HEADER_LEN
}

/// Resource type of an RDB object (UESP `ObjectType`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RdbResourceType {
    /// 0x01 — ARCH3D model (walls, props, doors, …).
    Model,
    /// 0x02 — light source.
    Light,
    /// 0x03 — flat (billboard/marker/monster; texture in TEXTURE.256).
    Flat,
}

impl RdbResourceType {
    pub fn from_u8(value: u8) -> Option<Self> {
        match value {
            0x01 => Some(Self::Model),
            0x02 => Some(Self::Light),
            0x03 => Some(Self::Flat),
            _ => None,
        }
    }

    /// Raw enum value in the format.
    pub const fn to_u8(self) -> u8 {
        match self {
            Self::Model => 0x01,
            Self::Light => 0x02,
            Self::Flat => 0x03,
        }
    }
}

/// `.RDB` parse errors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RdbError {
    /// Record shorter than a required section.
    TooShort { len: usize, need: usize },
    /// `ObjectRootOffset` ≠ expected offset (DFU invariant).
    RootOffsetMismatch { found: u32, expected: usize },
    /// `ObjectRootList` (`width*height` i32) exceeds the buffer.
    RootListOutOfBounds { count: usize, offset: usize, len: usize },
    /// Chain head outside the buffer.
    RootOutOfBounds { root: i32, len: usize },
    /// `Object` node outside the buffer (or `Next` points outside).
    ObjectOutOfBounds { offset: usize, len: usize },
    /// `Next` inside a chain revisits an already emitted node (cycle).
    ChainCycle { offset: usize },
    /// Unknown resource type (DFU throws in the same case).
    UnknownResourceType { offset: usize, ty: u8 },
    /// Resource (or action) outside the buffer.
    ResourceOutOfBounds { offset: usize, need: usize, len: usize },
    /// `ModelIndex` outside the range of `ModelReferenceList` (750).
    ModelIndexOutOfRange { model_index: u16 },
}

impl std::fmt::Display for RdbError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TooShort { len, need } => write!(f, "RDB too short: {len} < {need}"),
            Self::RootOffsetMismatch { found, expected } => write!(
                f,
                "ObjectRootOffset {found} != expected position {expected}"
            ),
            Self::RootListOutOfBounds { count, offset, len } => write!(
                f,
                "ObjectRootList of {count} roots at {offset} exceeds the buffer ({len})"
            ),
            Self::RootOutOfBounds { root, len } => {
                write!(f, "root {root} outside the {len}-byte buffer")
            }
            Self::ObjectOutOfBounds { offset, len } => {
                write!(f, "object at {offset} outside the {len}-byte buffer")
            }
            Self::ChainCycle { offset } => {
                write!(f, "cycle in the object chain: revisits {offset}")
            }
            Self::UnknownResourceType { offset, ty } => {
                write!(f, "object at {offset}: unknown resource type 0x{ty:02x}")
            }
            Self::ResourceOutOfBounds { offset, need, len } => write!(
                f,
                "resource at {offset}: +{need} exceeds the {len}-byte buffer"
            ),
            Self::ModelIndexOutOfRange { model_index } => write!(
                f,
                "ModelIndex {model_index} outside the ModelReferenceList (0..{MODEL_REFERENCE_COUNT})"
            ),
        }
    }
}

impl std::error::Error for RdbError {}

fn u16_le(data: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([data[at], data[at + 1]])
}

fn i32_le(data: &[u8], at: usize) -> i32 {
    i32::from_le_bytes([data[at], data[at + 1], data[at + 2], data[at + 3]])
}

fn u32_le(data: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([data[at], data[at + 1], data[at + 2], data[at + 3]])
}

/// `RdbHeader` (20 B).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RdbHeader {
    /// Unknown (UESP `Unknown1`).
    pub unknown1: u32,
    /// Width of the root grid (measured: 4 or 8 in retail).
    pub width: u32,
    /// Height of the root grid.
    pub height: u32,
    /// Absolute offset of `ObjectRootList` (invariant: 9532).
    pub object_root_offset: u32,
    /// Unknown (UESP `Unknown2`).
    pub unknown2: u32,
}

/// An entry of `ModelReferenceList`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RdbModelReference {
    /// Referenced ARCH3D.BSA record; `None` if the entry is unused
    /// (0xff padding — DFU parses it as an empty string → id 0).
    pub model_id: Option<u32>,
    /// Short ASCII description (3 bytes, e.g. `"COL"`).
    pub description: [u8; 3],
}

impl RdbModelReference {
    /// Is the entry unused (ModelId all 0xff)?
    pub fn is_unused(&self) -> bool {
        self.model_id.is_none()
    }
}

/// Action linked to a model (`ActionOffset > 0`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RdbAction {
    /// Action axis (1..=6: ±X/±Y/±Z per UESP).
    pub axis: u8,
    /// Duration.
    pub duration: u16,
    /// Movement magnitude.
    pub magnitude: u16,
    /// Target object offset (negative = no target).
    pub next_object_offset: i32,
    /// Flags.
    pub flags: u8,
}

/// `Model` object resource.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RdbModelResource {
    /// X rotation (Da, 720 per revolution — `BlocksFile.RotationDivisor`).
    pub x_rotation: i32,
    /// Y rotation.
    pub y_rotation: i32,
    /// Z rotation.
    pub z_rotation: i32,
    /// Index into `ModelReferenceList`.
    pub model_index: u16,
    /// Lock/trigger (0 = unlocked; 240 = magical, UESP).
    pub trigger_flag: u32,
    /// Sound/effect when the action runs.
    pub sound_index: u8,
    /// Offset of the `ActionRecord` (negative = none).
    pub action_offset: i32,
    /// Parsed action if `action_offset > 0`.
    pub action: Option<RdbAction>,
}

/// `Light` object resource.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RdbLightResource {
    /// Unknown (UESP `Unknown1`).
    pub unknown1: u32,
    /// Unknown (UESP `Unknown2`).
    pub unknown2: u32,
    /// Native radius (the bridge applies the scale to metres: `× GlobalScale`).
    pub radius: u16,
}

/// `Flat` object resource.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RdbFlatResource {
    /// Texture bitfield: archive = `>> 7`, record = `& 0x7f`
    /// (`DFU:BlocksFile.cs ReadRdbFlatResource`).
    pub texture_bitfield: u16,
    /// Flags (UESP: gender for NPC flats).
    pub flags: u16,
    /// Magnitude (byte 4).
    pub magnitude: u8,
    /// Sound index (byte 5).
    pub sound_index: u8,
    /// Offset of the next flat in the group (negative = end).
    pub next_object_offset: i32,
    /// Final action byte.
    pub action: u8,
}

impl RdbFlatResource {
    /// Texture archive of the flat (`TextureBitfield >> 7`).
    pub const fn texture_archive(&self) -> u16 {
        self.texture_bitfield >> 7
    }

    /// Texture record of the flat (`TextureBitfield & 0x7f`).
    pub const fn texture_record(&self) -> u16 {
        self.texture_bitfield & 0x7f
    }
}

/// Typed resources of an RDB object.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RdbResource {
    /// ARCH3D model.
    Model(RdbModelResource),
    /// Light source.
    Light(RdbLightResource),
    /// Flat (billboard/marker).
    Flat(RdbFlatResource),
}

/// An `Object` node of a chain in `ObjectRootList`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RdbObject {
    /// Absolute offset inside the record (node identity).
    pub offset: usize,
    /// Offset of the next node (negative = end of chain).
    pub next: i32,
    /// Offset of the previous node (negative = head).
    pub previous: i32,
    /// Position in native units (Daggerfall frame).
    pub position: [i32; 3],
    /// Typed resource.
    pub resource: RdbResource,
}

impl RdbObject {
    /// Resource type.
    pub const fn resource_type(&self) -> RdbResourceType {
        match self.resource {
            RdbResource::Model(_) => RdbResourceType::Model,
            RdbResource::Light(_) => RdbResourceType::Light,
            RdbResource::Flat(_) => RdbResourceType::Flat,
        }
    }
}

/// Parsed `.RDB` block (metadata + objects reachable per root).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RdbBlock {
    /// 20-byte header.
    pub header: RdbHeader,
    /// `ModelReferenceList` (750 entries; unused ones are `None`).
    pub model_references: Vec<RdbModelReference>,
    /// `ModelDataList` (750 × unknown u32 — preserved, DFU reads it).
    pub model_data: Vec<u32>,
    /// `ObjectRootList` (one i32 per `width×height` cell).
    pub roots: Vec<i32>,
    /// Objects reachable from some root, in deterministic order
    /// (roots in cell order, chains in link order; dedup by
    /// offset).
    pub objects: Vec<RdbObject>,
}

impl RdbBlock {
    /// Total number of root cells.
    pub fn root_count(&self) -> usize {
        self.roots.len()
    }

    /// Root cells with a chain present (offset ≥ 0).
    pub fn used_roots(&self) -> usize {
        self.roots.iter().filter(|r| **r >= 0).count()
    }

    /// ARCH3D model referenced by `model_index` (validated against
    /// `ModelReferenceList`; `None` if the entry is unused).
    pub fn model_id(&self, model_index: u16) -> Option<u32> {
        self.model_references
            .get(model_index as usize)
            .and_then(|r| r.model_id)
    }

    /// Number of objects per type (Model, Light, Flat).
    pub fn resource_counts(&self) -> (usize, usize, usize) {
        let mut counts = (0, 0, 0);
        for object in &self.objects {
            match object.resource {
                RdbResource::Model(_) => counts.0 += 1,
                RdbResource::Light(_) => counts.1 += 1,
                RdbResource::Flat(_) => counts.2 += 1,
            }
        }
        counts
    }
}

/// Parses a complete `.RDB` record from its buffer (read-only, 0 copies
/// of the raw bytes; objects are materialized as structs).
pub fn parse(data: &[u8]) -> Result<RdbBlock, RdbError> {
    let len = data.len();
    if len < object_root_offset_expected() + 4 {
        return Err(RdbError::TooShort {
            len,
            need: object_root_offset_expected() + 4,
        });
    }

    let header = RdbHeader {
        unknown1: u32_le(data, 0),
        width: u32_le(data, 4),
        height: u32_le(data, 8),
        object_root_offset: u32_le(data, 12),
        unknown2: u32_le(data, 16),
    };

    // ModelReferenceList: ModelId ASCII[5] (decimal of the ARCH3D record;
    // 0xff = unused) + Description ASCII[3].
    let mut model_references = Vec::with_capacity(MODEL_REFERENCE_COUNT);
    for i in 0..MODEL_REFERENCE_COUNT {
        let at = RDB_HEADER_LEN + i * MODEL_REFERENCE_LEN;
        let slot = &data[at..at + 5];
        let model_id = if slot[0] == 0xff {
            None
        } else {
            let end = slot.iter().position(|&b| b == 0).unwrap_or(slot.len());
            let text = std::str::from_utf8(&slot[..end]).ok();
            text.and_then(|t| t.trim().parse::<u32>().ok())
        };
        let mut description = [0u8; 3];
        description.copy_from_slice(&data[at + 5..at + 8]);
        model_references.push(RdbModelReference {
            model_id,
            description,
        });
    }

    // ModelDataList: 750 × unknown u32 (DFU reads it and ignores it).
    let data_base = RDB_HEADER_LEN + MODEL_REFERENCE_COUNT * MODEL_REFERENCE_LEN;
    let mut model_data = Vec::with_capacity(MODEL_REFERENCE_COUNT);
    for i in 0..MODEL_REFERENCE_COUNT {
        model_data.push(u32_le(data, data_base + i * MODEL_DATA_LEN));
    }

    // ObjectHeader (512 B): DFU requires that `ObjectRootList` start right
    // after it (otherwise it throws "Start of ObjectRoot section does not
    // match header offset").
    let expected_root_offset = object_root_offset_expected();
    if header.object_root_offset as usize != expected_root_offset {
        return Err(RdbError::RootOffsetMismatch {
            found: header.object_root_offset,
            expected: expected_root_offset,
        });
    }

    // ObjectRootList: width×height i32 with absolute offsets.
    let root_count = (header.width as usize).saturating_mul(header.height as usize);
    let root_bytes = root_count.saturating_mul(4);
    if expected_root_offset.saturating_add(root_bytes) > len {
        return Err(RdbError::RootListOutOfBounds {
            count: root_count,
            offset: expected_root_offset,
            len,
        });
    }
    let mut roots = Vec::with_capacity(root_count);
    for i in 0..root_count {
        roots.push(i32_le(data, expected_root_offset + i * 4));
    }

    // Object chains: per root in cell order, in link order,
    // with dedup by offset (an object reachable from two roots is emitted
    // once) and a cycle guard.
    let mut objects: Vec<RdbObject> = Vec::new();
    let mut emitted: std::collections::HashSet<usize> = std::collections::HashSet::new();
    for root in &roots {
        if *root < 0 {
            continue;
        }
        // Revisiting a node inside the SAME chain = cycle (DFU would stay
        // in an infinite loop; here it is a typed error, no panic or hang).
        let mut chain: std::collections::HashSet<usize> = std::collections::HashSet::new();
        let mut offset = *root as usize;
        loop {
            if offset + OBJECT_LEN > len {
                return Err(RdbError::ObjectOutOfBounds { offset, len });
            }
            if !chain.insert(offset) {
                return Err(RdbError::ChainCycle { offset });
            }
            if !emitted.insert(offset) {
                // Node already emitted by another root (shared chain): the
                // remaining tail is too — move on to the next root.
                break;
            }
            let next = i32_le(data, offset);
            let previous = i32_le(data, offset + 4);
            let position = [
                i32_le(data, offset + 8),
                i32_le(data, offset + 12),
                i32_le(data, offset + 16),
            ];
            let ty = data[offset + 20];
            let resource_offset = u32_le(data, offset + 21) as usize;
            let resource_type = RdbResourceType::from_u8(ty)
                .ok_or(RdbError::UnknownResourceType { offset, ty })?;
            let resource = parse_resource(data, resource_type, resource_offset)?;
            objects.push(RdbObject {
                offset,
                next,
                previous,
                position,
                resource,
            });
            if next < 0 {
                break;
            }
            offset = next as usize;
        }
    }

    Ok(RdbBlock {
        header,
        model_references,
        model_data,
        roots,
        objects,
    })
}

/// Parses the typed resource at `at` (bounds-checked, no panic).
fn parse_resource(
    data: &[u8],
    resource_type: RdbResourceType,
    at: usize,
) -> Result<RdbResource, RdbError> {
    let len = data.len();
    match resource_type {
        RdbResourceType::Model => {
            if at + MODEL_RESOURCE_LEN > len {
                return Err(RdbError::ResourceOutOfBounds {
                    offset: at,
                    need: MODEL_RESOURCE_LEN,
                    len,
                });
            }
            let model_index = u16_le(data, at + 12);
            if model_index as usize >= MODEL_REFERENCE_COUNT {
                return Err(RdbError::ModelIndexOutOfRange { model_index });
            }
            let action_offset = i32_le(data, at + 19);
            let action = if action_offset > 0 {
                let a = action_offset as usize;
                if a + ACTION_LEN > len {
                    return Err(RdbError::ResourceOutOfBounds {
                        offset: a,
                        need: ACTION_LEN,
                        len,
                    });
                }
                Some(RdbAction {
                    axis: data[a],
                    duration: u16_le(data, a + 1),
                    magnitude: u16_le(data, a + 3),
                    next_object_offset: i32_le(data, a + 5),
                    flags: data[a + 9],
                })
            } else {
                None
            };
            Ok(RdbResource::Model(RdbModelResource {
                x_rotation: i32_le(data, at),
                y_rotation: i32_le(data, at + 4),
                z_rotation: i32_le(data, at + 8),
                model_index,
                trigger_flag: u32_le(data, at + 14),
                sound_index: data[at + 18],
                action_offset,
                action,
            }))
        }
        RdbResourceType::Light => {
            if at + LIGHT_RESOURCE_LEN > len {
                return Err(RdbError::ResourceOutOfBounds {
                    offset: at,
                    need: LIGHT_RESOURCE_LEN,
                    len,
                });
            }
            Ok(RdbResource::Light(RdbLightResource {
                unknown1: u32_le(data, at),
                unknown2: u32_le(data, at + 4),
                radius: u16_le(data, at + 8),
            }))
        }
        RdbResourceType::Flat => {
            if at + FLAT_RESOURCE_LEN > len {
                return Err(RdbError::ResourceOutOfBounds {
                    offset: at,
                    need: FLAT_RESOURCE_LEN,
                    len,
                });
            }
            Ok(RdbResource::Flat(RdbFlatResource {
                texture_bitfield: u16_le(data, at),
                flags: u16_le(data, at + 2),
                magnitude: data[at + 4],
                sound_index: data[at + 5],
                next_object_offset: i32_le(data, at + 6),
                action: data[at + 10],
            }))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Deterministic `.RDB` fixture (0 I/O, 0 retail bytes): 2×2 grid with
    /// a chain of 3 objects (Model with action · Light · Flat) and one empty
    /// root.
    fn fixture_rdb() -> Vec<u8> {
        let root_offset = object_root_offset_expected();
        // Chained objects: Model@9600 → Light@9700 → Flat@9800 (Next<0).
        let mut rec = vec![0u8; 9900];

        // RdbHeader: 2×2 roots, ObjectRootOffset = 9532.
        rec[4..8].copy_from_slice(&2u32.to_le_bytes());
        rec[8..12].copy_from_slice(&2u32.to_le_bytes());
        rec[12..16].copy_from_slice(&(root_offset as u32).to_le_bytes());

        // ModelReferenceList[7] = "01234" (record ARCH3D 1234), desc "WAL".
        let mref = RDB_HEADER_LEN + 7 * MODEL_REFERENCE_LEN;
        rec[mref..mref + 5].copy_from_slice(b"01234");
        rec[mref + 5..mref + 8].copy_from_slice(b"WAL");
        // Unused entry (8): 0xff.
        let unused = RDB_HEADER_LEN + 8 * MODEL_REFERENCE_LEN;
        rec[unused..unused + 8].fill(0xff);

        // ObjectHeader: "DAGR" at the UESP offset (52 of the ObjectHeader).
        let dagr = object_section_offset() + 52;
        rec[dagr..dagr + 4].copy_from_slice(b"DAGR");
        // ObjectHeader FileSize (offset 16).
        let fsize = object_section_offset() + 16;
        let total = rec.len() as u32;
        rec[fsize..fsize + 4].copy_from_slice(&total.to_le_bytes());

        // ObjectRootList (at root_offset): root 0 → 9600; roots 1..3 = -1.
        rec[root_offset..root_offset + 4].copy_from_slice(&9600i32.to_le_bytes());
        for i in 1..4 {
            let at = root_offset + i * 4;
            rec[at..at + 4].copy_from_slice(&(-1i32).to_le_bytes());
        }

        // Model object @9600: Next=9700, Previous=-1, pos (100,-200,300),
        // Type=0x01, ResourceOffset=9640.
        rec[9600..9604].copy_from_slice(&9700i32.to_le_bytes());
        rec[9604..9608].copy_from_slice(&(-1i32).to_le_bytes());
        rec[9608..9612].copy_from_slice(&100i32.to_le_bytes());
        rec[9612..9616].copy_from_slice(&(-200i32).to_le_bytes());
        rec[9616..9620].copy_from_slice(&300i32.to_le_bytes());
        rec[9620] = 0x01;
        rec[9621..9625].copy_from_slice(&9640u32.to_le_bytes());
        // Model resource @9640: rot (0, -512, 0), ModelIndex=7, lock=0,
        // ActionOffset=9680.
        rec[9644..9648].copy_from_slice(&(-512i32).to_le_bytes());
        rec[9652..9654].copy_from_slice(&7u16.to_le_bytes());
        rec[9659..9663].copy_from_slice(&9680i32.to_le_bytes());
        // Action @9680: axis=1, duration=10, magnitude=256, target=-1.
        rec[9680] = 1;
        rec[9681..9683].copy_from_slice(&10u16.to_le_bytes());
        rec[9683..9685].copy_from_slice(&256u16.to_le_bytes());
        rec[9685..9689].copy_from_slice(&(-1i32).to_le_bytes());

        // Light object @9700: Next=9800, pos (0,0,0), Type=0x02, Res=9740.
        rec[9700..9704].copy_from_slice(&9800i32.to_le_bytes());
        rec[9704..9708].copy_from_slice(&9600i32.to_le_bytes());
        rec[9720] = 0x02;
        rec[9721..9725].copy_from_slice(&9740u32.to_le_bytes());
        // Light resource @9740: radius=1024.
        rec[9748..9750].copy_from_slice(&1024u16.to_le_bytes());

        // Flat object @9800: Next=-1, pos (64, 32, 16), Type=0x03, Res=9840.
        rec[9800..9804].copy_from_slice(&(-1i32).to_le_bytes());
        rec[9804..9808].copy_from_slice(&9700i32.to_le_bytes());
        rec[9808..9812].copy_from_slice(&64i32.to_le_bytes());
        rec[9812..9816].copy_from_slice(&32i32.to_le_bytes());
        rec[9816..9820].copy_from_slice(&16i32.to_le_bytes());
        rec[9820] = 0x03;
        rec[9821..9825].copy_from_slice(&9840u32.to_le_bytes());
        // Flat resource @9840: bitfield = (17 << 7) | 5 (archive 17, record 5).
        rec[9840..9842].copy_from_slice(&((17u16 << 7) | 5).to_le_bytes());

        rec
    }

    #[test]
    fn parse_fixture_rdb() {
        let block = parse(&fixture_rdb()).unwrap();
        assert_eq!(block.header.width, 2);
        assert_eq!(block.header.height, 2);
        assert_eq!(block.root_count(), 4);
        assert_eq!(block.used_roots(), 1);
        assert_eq!(block.objects.len(), 3, "single chain of 3 objects");

        // Model references.
        assert_eq!(block.model_references[7].model_id, Some(1234));
        assert_eq!(&block.model_references[7].description, b"WAL");
        assert!(block.model_references[8].is_unused());
        assert_eq!(block.model_data.len(), MODEL_REFERENCE_COUNT);

        // Model object (with action).
        let model = &block.objects[0];
        assert_eq!(model.offset, 9600);
        assert_eq!(model.position, [100, -200, 300]);
        assert_eq!(model.resource_type(), RdbResourceType::Model);
        let RdbResource::Model(m) = &model.resource else {
            panic!("expected Model resource");
        };
        assert_eq!(m.y_rotation, -512);
        assert_eq!(m.model_index, 7);
        assert_eq!(block.model_id(m.model_index), Some(1234));
        let action = m.action.expect("parsed action");
        assert_eq!(action.axis, 1);
        assert_eq!(action.duration, 10);
        assert_eq!(action.magnitude, 256);
        assert_eq!(action.next_object_offset, -1);

        // Light object.
        let light = &block.objects[1];
        let RdbResource::Light(l) = &light.resource else {
            panic!("expected Light resource");
        };
        assert_eq!(l.radius, 1024);

        // Flat object.
        let flat = &block.objects[2];
        assert_eq!(flat.next, -1);
        let RdbResource::Flat(fl) = &flat.resource else {
            panic!("expected Flat resource");
        };
        assert_eq!(fl.texture_archive(), 17);
        assert_eq!(fl.texture_record(), 5);
        assert_eq!(block.resource_counts(), (1, 1, 1));
    }

    #[test]
    fn dedup_shared_object_across_roots() {
        // Two roots pointing at the SAME object: it is emitted only once.
        let mut rec = fixture_rdb();
        let root_offset = object_root_offset_expected();
        rec[root_offset + 4..root_offset + 8].copy_from_slice(&9600i32.to_le_bytes());
        let block = parse(&rec).unwrap();
        assert_eq!(block.objects.len(), 3, "0 duplicates from a shared root");
        assert_eq!(block.used_roots(), 2);
    }

    #[test]
    fn truncated_and_malformed_rdb_is_error_not_panic() {
        let rec = fixture_rdb();
        assert!(matches!(
            parse(&rec[..100]),
            Err(RdbError::TooShort { .. })
        ));
        // ObjectRootOffset that does not match (DFU invariant).
        let mut bad = fixture_rdb();
        bad[12..16].copy_from_slice(&123u32.to_le_bytes());
        assert!(matches!(
            parse(&bad),
            Err(RdbError::RootOffsetMismatch {
                found: 123,
                ..
            })
        ));
        // Unknown resource type.
        let mut bad = fixture_rdb();
        bad[9620] = 0x42;
        assert!(matches!(
            parse(&bad),
            Err(RdbError::UnknownResourceType { ty: 0x42, .. })
        ));
        // Object outside the buffer (Next points to 9_999_999).
        let mut bad = fixture_rdb();
        bad[9800..9804].copy_from_slice(&9_999_999i32.to_le_bytes());
        assert!(matches!(parse(&bad), Err(RdbError::ObjectOutOfBounds { .. })));
        // ModelIndex outside the ModelReferenceList.
        let mut bad = fixture_rdb();
        bad[9652..9654].copy_from_slice(&800u16.to_le_bytes());
        assert!(matches!(
            parse(&bad),
            Err(RdbError::ModelIndexOutOfRange { model_index: 800 })
        ));
    }

    #[test]
    fn chain_cycle_is_error_not_hang() {
        // Chain that revisits itself (Next returns to 9600).
        let mut rec = fixture_rdb();
        rec[9800..9804].copy_from_slice(&9600i32.to_le_bytes());
        assert!(matches!(parse(&rec), Err(RdbError::ChainCycle { .. })));
    }

    #[test]
    fn all_zero_rdb_is_error_not_panic() {
        // Zeroed header: ObjectRootOffset 0 ≠ 9532 → typed error.
        let rec = vec![0u8; object_root_offset_expected() + 16];
        assert!(matches!(
            parse(&rec),
            Err(RdbError::RootOffsetMismatch { found: 0, .. })
        ));
    }
}
