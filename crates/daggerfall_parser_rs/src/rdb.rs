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
/// Length of a `ModelReference` (`ModelId[5] + Description[3]`).
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

// ───────────────────────────────────────────────────────────────────────────
// Block type + DFU data fixes — M18 B2-a (M11 remainder, REQ-L99-043)
//
// Port of the two DFU entry points the M11 audit left missing
// (`campaign-final-reaudit-CR-acc1b551.md:216`):
//
// | DFU source (`/tmp` corpus `DaggerfallConnect/API/BlocksFile.cs`) | Here |
// |:---|:---|
// | `GetRdbType(string blockName)` `:243-258` | [`rdb_type`] |
// | `FixRdbData(int block)` `:500-626` | [`fix_rdb_data`] |
// | `private bool Read(int block) → … FixRdbData(block)` `:679-682` | applied by the caller right after [`parse`] |
//
// `DFBlock.RdbTypes` (`DFBlock.cs:92-113`) is mirrored with its exact
// discriminants instead of a fresh enum, so a future save/telemetry field can
// carry the value unchanged. `Start` stays declared for that reason only:
// `GetRdbType` never returns it (it is derived from map data, `:238-243`).
// ───────────────────────────────────────────────────────────────────────────

/// `DFBlock.RdbTypes` (`DFBlock.cs:92-113`), discriminants preserved.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RdbType {
    /// Not defined (`0`).
    Unknown = 0,
    /// Border block used to seal a dungeon (`1`).
    Border = 1,
    /// Normal block (`2`).
    Normal = 2,
    /// Flooded block (`3`).
    Wet = 3,
    /// Used in the main quest (`4`).
    Quest = 4,
    /// Crypt block (`5`).
    Mausoleum = 5,
    /// Start block — derived from map data, never by [`rdb_type`] (`6`).
    Start = 6,
}

impl RdbType {
    /// Every value in discriminant order (deterministic census order).
    pub const ALL: [RdbType; 7] = [
        RdbType::Unknown,
        RdbType::Border,
        RdbType::Normal,
        RdbType::Wet,
        RdbType::Quest,
        RdbType::Mausoleum,
        RdbType::Start,
    ];

    /// Discriminant (`DFBlock.RdbTypes` value).
    pub const fn to_u8(self) -> u8 {
        self as u8
    }

    /// Short label for evidence lines.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Unknown => "unknown",
            Self::Border => "border",
            Self::Normal => "normal",
            Self::Wet => "wet",
            Self::Quest => "quest",
            Self::Mausoleum => "mausoleum",
            Self::Start => "start",
        }
    }

    /// Name prefix that identifies the type (`"B"`, `"W"`, …; `"?"` for
    /// `Unknown`).
    pub const fn prefix(self) -> &'static str {
        match self {
            Self::Border => "B",
            Self::Wet => "W",
            Self::Quest => "S",
            Self::Mausoleum => "M",
            Self::Normal => "N",
            // No prefix classifies into these two.
            Self::Start | Self::Unknown => "?",
        }
    }
}

/// `BlocksFile.GetRdbType` (`BlocksFile.cs:243-258`): the block type is the
/// **first letter of the record name**, in the source's order of tests.
///
/// The comparison is case-sensitive, as the C# `StartsWith(string)` overload
/// is ordinal: `"n0000042.RDB"` is `Unknown`, not `Normal`. `Start` is never
/// returned (`BlocksFile.cs:238-243`).
pub fn rdb_type(block_name: &str) -> RdbType {
    match block_name.chars().next() {
        Some('B') => RdbType::Border,
        Some('W') => RdbType::Wet,
        Some('S') => RdbType::Quest,
        Some('M') => RdbType::Mausoleum,
        Some('N') => RdbType::Normal,
        _ => RdbType::Unknown,
    }
}

/// Synthetic `offset` of an object created by a data fix (DFU appends a new
/// `RdbObject` to the root's array, `BlocksFile.cs:566-575`): it has no byte
/// position in the record, so the identity lives above any real offset.
///
/// It must stay inside `i32` (the type of `next`/`previous`) AND above any
/// record length: 1 GiB is ~4 orders of magnitude over the largest `.RDB`
/// record in `BLOCKS.BSA` (32 MiB for the whole archive).
pub const SYNTHETIC_OFFSET_BASE: usize = 1 << 30;

/// One object chain of `ObjectRootList` — DFU `ObjectRootList[root].RdbObjects`
/// (`BlocksFile.cs:243…`), as indices into [`RdbBlock::objects`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RdbChain {
    /// Root cell index in `ObjectRootList` (`0..width*height`).
    pub root: usize,
    /// Object indices of the chain, in link order.
    pub objects: Vec<usize>,
}

impl RdbBlock {
    /// Rebuilds the per-root object chains from the flat, deduplicated
    /// [`RdbBlock::objects`] list.
    ///
    /// The image is reconstructed, not re-read: roots are walked in cell
    /// order and each chain follows `next` (cycle-guarded, like [`parse`]).
    /// A node already emitted by another root ends the chain — the same
    /// truncation [`parse`] performs on a shared chain.
    pub fn chains(&self) -> Vec<RdbChain> {
        let mut by_offset: std::collections::HashMap<usize, usize> =
            std::collections::HashMap::with_capacity(self.objects.len());
        for (index, object) in self.objects.iter().enumerate() {
            by_offset.entry(object.offset).or_insert(index);
        }
        let mut chains = Vec::new();
        for (root, head) in self.roots.iter().enumerate() {
            if *head < 0 {
                continue;
            }
            let mut seen: std::collections::HashSet<usize> = std::collections::HashSet::new();
            let mut objects = Vec::new();
            let mut cursor = *head as usize;
            while let Some(&index) = by_offset.get(&cursor) {
                if !seen.insert(index) {
                    break;
                }
                objects.push(index);
                let next = self.objects[index].next;
                if next < 0 {
                    break;
                }
                cursor = next as usize;
            }
            if !objects.is_empty() {
                chains.push(RdbChain { root, objects });
            }
        }
        chains
    }
}

/// The seven data fixes of `BlocksFile.FixRdbData` (`:502-625`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum RdbFix {
    /// `N0000071.RDB` — connect the lower western dead-end with models 10/11
    /// (`BlocksFile.cs:502-508`).
    WesternDeadEndConnected,
    /// `N0000022.RDB` / `N0000023.RDB` — drop the bad door chain
    /// (`BlocksFile.cs:509-512`).
    BadDoorRemoved,
    /// `N0000035.RDB` — correct a door height (`BlocksFile.cs:513-516`).
    DoorHeightCorrected,
    /// `N0000052.RDB` — correct a lever placement (`BlocksFile.cs:517-520`).
    LeverPlacementCorrected,
    /// `W0000009.RDB` — move the exit to another corner and seal it with a
    /// brick wall (`BlocksFile.cs:521-575`).
    ExitMovedAndSealed,
    /// `W0000018.RDB` — join the upper rooms with a corridor and fix a door
    /// (`BlocksFile.cs:576-601`).
    UpperRoomsJoined,
    /// `W0000020.RDB` — open the lower western dead-end with a stair
    /// (`BlocksFile.cs:602-625`).
    LowerDeadEndOpened,
}

impl RdbFix {
    /// Stable label for evidence/digest lines.
    pub const fn label(self) -> &'static str {
        match self {
            Self::WesternDeadEndConnected => "western_dead_end_connected",
            Self::BadDoorRemoved => "bad_door_removed",
            Self::DoorHeightCorrected => "door_height_corrected",
            Self::LeverPlacementCorrected => "lever_placement_corrected",
            Self::ExitMovedAndSealed => "exit_moved_and_sealed",
            Self::UpperRoomsJoined => "upper_rooms_joined",
            Self::LowerDeadEndOpened => "lower_dead_end_opened",
        }
    }
}

/// A fix that could not be applied to the record as parsed (missing root,
/// missing chain position, non-model resource, no free model reference).
///
/// This is NOT a parse error: DFU would throw `IndexOutOfRangeException` on
/// the same record; here the record stays usable and the gap is explicit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RdbFixGap {
    /// Fix that could not be applied.
    pub fix: RdbFix,
    /// Exact reason (root/position/what was missing).
    pub reason: String,
}

/// Outcome of [`fix_rdb_data`] on ONE record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RdbFixReport {
    /// Record name the report refers to.
    pub block: String,
    /// [`rdb_type`] of the name (computed even for unknown blocks).
    pub rdb_type: RdbType,
    /// `true` if the name is one of the eight records DFU repairs. An unknown
    /// record is an explicit no-op, never a silent PASS.
    pub known: bool,
    /// Fixes actually applied, in application order.
    pub applied: Vec<RdbFix>,
    /// Fixes the record could not accept (never a panic).
    pub unresolved: Vec<RdbFixGap>,
}

impl RdbFixReport {
    /// `true` if the record was repaired (≥1 fix applied, 0 gaps).
    pub fn pass(&self) -> bool {
        self.unresolved.is_empty() && !self.applied.is_empty()
    }
}

/// Record names repaired by `BlocksFile.FixRdbData` and the DFU directory
/// indices the corpus comments document (`:502`, `:509`, `:513`, `:517`,
/// `:521`, `:576`, `:602`). Keying by NAME instead of the index keeps the port
/// stable against directory-order replacements (`WorldDataReplacement`,
/// `BlocksFile.cs:270-273`).
pub const RDB_FIX_BLOCKS: [&str; 8] = [
    "N0000071.RDB",
    "N0000022.RDB",
    "N0000023.RDB",
    "N0000035.RDB",
    "N0000052.RDB",
    "W0000009.RDB",
    "W0000018.RDB",
    "W0000020.RDB",
];

/// Applies `BlocksFile.FixRdbData` (`:500-626`) to ONE parsed record.
///
/// DFU calls it at the end of `ReadBlock` (`:679-682`), i.e. right after the
/// record is parsed and before any layout uses it; callers here do the same.
/// The function is total: an unknown name returns an explicit
/// `known: false` report, and a known name whose image cannot accept a fix
/// returns the consumed fixes plus a typed [`RdbFixGap`] — never a panic and
/// never a silent PASS.
pub fn fix_rdb_data(block_name: &str, block: &mut RdbBlock) -> RdbFixReport {
    let mut report = RdbFixReport {
        block: block_name.to_string(),
        rdb_type: rdb_type(block_name),
        known: RDB_FIX_BLOCKS
            .iter()
            .any(|name| name.eq_ignore_ascii_case(block_name)),
        applied: Vec::new(),
        unresolved: Vec::new(),
    };
    let canonical = RDB_FIX_BLOCKS
        .iter()
        .copied()
        .find(|name| name.eq_ignore_ascii_case(block_name));
    let Some(name) = canonical else {
        return report;
    };

    match name {
        // `:502-508` — connect the western dead-end to the rest of the block.
        "N0000071.RDB" => {
            let fix = RdbFix::WesternDeadEndConnected;
            let mut gaps = Vec::new();
            for (root, position, index, x) in [
                (4usize, 18usize, Some(10u16), Some(640i32)),
                (5, 8, Some(11), None),
            ] {
                match object_at(block, root, position) {
                    Some(object) => {
                        if let Some(index) = index {
                            gaps.extend(set_model_index(block, object, index).err());
                        }
                        if let Some(x) = x {
                            block.objects[object].position[0] = x;
                        }
                    }
                    None => gaps.push(missing(block, root, position)),
                }
            }
            record(&mut report, fix, gaps);
        }
        // `:509-512` — remove the bad door chain.
        "N0000022.RDB" | "N0000023.RDB" => {
            let fix = RdbFix::BadDoorRemoved;
            let chains = block.chains();
            let Some(chain) = chains.iter().find(|chain| chain.root == 2) else {
                record(
                    &mut report,
                    fix,
                    vec!["root 2 has no object chain".to_string()],
                );
                return report;
            };
            let offsets: Vec<usize> = chain
                .objects
                .iter()
                .map(|index| block.objects[*index].offset)
                .collect();
            // Drop the chain: the objects disappear from the reachable image
            // and the root stops pointing at them (`RdbObjects = null`, `:511`).
            block
                .objects
                .retain(|object| !offsets.contains(&object.offset));
            block.roots[2] = -1;
            record(&mut report, fix, Vec::new());
        }
        // `:513-516` — correct the door height.
        "N0000035.RDB" => {
            let fix = RdbFix::DoorHeightCorrected;
            let gaps = match object_at(block, 0, 36) {
                Some(object) => {
                    block.objects[object].position[1] = -300;
                    Vec::new()
                }
                None => vec![missing(block, 0, 36)],
            };
            record(&mut report, fix, gaps);
        }
        // `:517-520` — correct the lever placement.
        "N0000052.RDB" => {
            let fix = RdbFix::LeverPlacementCorrected;
            let gaps = match object_at(block, 0, 24) {
                Some(object) => {
                    block.objects[object].position[0] = 543;
                    Vec::new()
                }
                None => vec![missing(block, 0, 24)],
            };
            record(&mut report, fix, gaps);
        }
        // `:521-575` — move the exit to another corner, add a brick wall door.
        "W0000009.RDB" => {
            let fix = RdbFix::ExitMovedAndSealed;
            let mut gaps = Vec::new();
            // First unused model reference becomes the wall door `72100` "DOR".
            let wall = block
                .model_references
                .iter()
                .position(|reference| reference.model_id.is_none())
                .filter(|index| *index < u16::MAX as usize);
            let Some(wall_index) = wall else {
                record(
                    &mut report,
                    fix,
                    vec!["ModelReferenceList without a free slot (0…750)".to_string()],
                );
                return report;
            };
            block.model_references[wall_index] = RdbModelReference {
                model_id: Some(72100),
                description: *b"DOR",
            };
            // Corner near the original exit gets a door model and moves 1 m.
            let Some(corner) = object_at(block, 5, 39) else {
                record(&mut report, fix, vec![missing(block, 5, 39)]);
                return report;
            };
            block.objects[corner].position[0] += 64;
            block.objects[corner].position[2] -= 64;
            let (x, y, z) = (
                block.objects[corner].position[0],
                block.objects[corner].position[1],
                block.objects[corner].position[2],
            );
            gaps.extend(set_model_index(block, corner, 35).err());
            gaps.extend(set_y_rotation(block, corner, -512).err());
            // The exit moves to the corner door.
            match object_at(block, 8, 0) {
                Some(object) => {
                    block.objects[object].position[0] = x;
                    block.objects[object].position[2] = z + 126;
                }
                None => gaps.push(missing(block, 8, 0)),
            }
            // The start marker follows the exit.
            match object_at(block, 4, 5) {
                Some(object) => {
                    block.objects[object].position[0] = x;
                    block.objects[object].position[2] = z;
                }
                None => gaps.push(missing(block, 4, 5)),
            }
            // A wall seals the door when the block is not the starting one.
            let wall_position = [x, y, z + 128];
            if gaps.is_empty() {
                gaps.extend(
                    append_model(block, 8, wall_position, wall_index as u16, -512).err(),
                );
            }
            record(&mut report, fix, gaps);
        }
        // `:576-601` — turn two upper corners into T junctions + corridor.
        "W0000018.RDB" => {
            let fix = RdbFix::UpperRoomsJoined;
            let mut gaps = Vec::new();
            for (root, position, rotation) in [(4usize, 38usize, -512i32), (5, 40, -2560)] {
                match object_at(block, root, position) {
                    Some(object) => {
                        gaps.extend(set_model_index(block, object, 10).err());
                        gaps.extend(set_y_rotation(block, object, rotation).err());
                    }
                    None => gaps.push(missing(block, root, position)),
                }
            }
            if gaps.is_empty() {
                gaps.extend(
                    append_model(block, 6, [1152, -1792, 1920], 11, -512).err(),
                );
            }
            // Fix a door.
            match object_at(block, 2, 0) {
                Some(object) => gaps.extend(set_model_index(block, object, 23).err()),
                None => gaps.push(missing(block, 2, 0)),
            }
            record(&mut report, fix, gaps);
        }
        // `:602-625` — open the lower western dead-end with a stair.
        "W0000020.RDB" => {
            let fix = RdbFix::LowerDeadEndOpened;
            let mut gaps = Vec::new();
            match object_at(block, 4, 15) {
                Some(object) => gaps.extend(set_model_index(block, object, 5).err()),
                None => gaps.push(missing(block, 4, 15)),
            }
            match object_at(block, 5, 34) {
                Some(object) => {
                    gaps.extend(set_model_index(block, object, 7).err());
                    gaps.extend(set_y_rotation(block, object, -512).err());
                }
                None => gaps.push(missing(block, 5, 34)),
            }
            if gaps.is_empty() {
                gaps.extend(append_model(block, 5, [1152, -384, 1408], 4, 512).err());
            }
            record(&mut report, fix, gaps);
        }
        other => {
            // `RDB_FIX_BLOCKS` and this match must stay in lockstep.
            report.unresolved.push(RdbFixGap {
                fix: RdbFix::BadDoorRemoved,
                reason: format!("fix table entry {other} without a ported branch"),
            });
        }
    }
    report
}

/// Moves a completed fix (or its gaps) into the report.
fn record(report: &mut RdbFixReport, fix: RdbFix, gaps: Vec<String>) {
    if gaps.is_empty() {
        report.applied.push(fix);
    } else {
        for reason in gaps {
            report.unresolved.push(RdbFixGap { fix, reason });
        }
    }
}

/// `(root, chain position)` → index into [`RdbBlock::objects`].
///
/// The lookup does NOT require a `Model`: DFU moves `XPos`/`YPos`/`ZPos` of
/// any object (e.g. `W0000009.RDB` root 4 position 5, `:570-573`). The setters
/// that need a model (`ModelIndex`, `YRotation`) report the type mismatch
/// themselves instead of hiding it behind "not found".
fn object_at(block: &RdbBlock, root: usize, position: usize) -> Option<usize> {
    let chains = block.chains();
    let chain = chains.iter().find(|chain| chain.root == root)?;
    chain.objects.get(position).copied()
}

/// Gap message for a missing `(root, position)`.
fn missing(block: &RdbBlock, root: usize, position: usize) -> String {
    let chains = block.chains();
    match chains.iter().find(|chain| chain.root == root) {
        Some(chain) => format!(
            "root {root} chain has {} objects (position {position} out of range)",
            chain.objects.len()
        ),
        None => format!(
            "root {root} has no chain (roots {} of {} used)",
            block.used_roots(),
            block.root_count()
        ),
    }
}

/// Sets `ModelResource.ModelIndex` of one object (`:503`, `:557`, …).
fn set_model_index(block: &mut RdbBlock, object: usize, index: u16) -> Result<(), String> {
    if index as usize >= block.model_references.len() {
        return Err(format!(
            "ModelIndex {index} outside the ModelReferenceList ({} entries)",
            block.model_references.len()
        ));
    }
    match &mut block.objects[object].resource {
        RdbResource::Model(model) => {
            model.model_index = index;
            Ok(())
        }
        other => Err(format!(
            "object {object} is a {}, not a Model",
            resource_label(other)
        )),
    }
}

/// Label of a resource for gap messages.
const fn resource_label(resource: &RdbResource) -> &'static str {
    match resource {
        RdbResource::Model(_) => "Model",
        RdbResource::Light(_) => "Light",
        RdbResource::Flat(_) => "Flat",
    }
}

/// Sets `ModelResource.YRotation` of one object (`:561`, `:591`).
fn set_y_rotation(block: &mut RdbBlock, object: usize, rotation: i32) -> Result<(), String> {
    match &mut block.objects[object].resource {
        RdbResource::Model(model) => {
            model.y_rotation = rotation;
            Ok(())
        }
        other => Err(format!(
            "object {object} is a {}, not a Model",
            resource_label(other)
        )),
    }
}

/// Appends a new `Model` object to the chain of `root` (`Index = count`,
/// `:566-575`), linking it so [`RdbBlock::chains`] reaches it.
fn append_model(
    block: &mut RdbBlock,
    root: usize,
    position: [i32; 3],
    model_index: u16,
    y_rotation: i32,
) -> Result<(), String> {
    let chains = block.chains();
    let Some(chain) = chains.iter().find(|chain| chain.root == root) else {
        return Err(format!("root {root} has no chain to append to"));
    };
    if model_index as usize >= block.model_references.len() {
        return Err(format!(
            "ModelIndex {model_index} outside the ModelReferenceList"
        ));
    }
    let previous = chain
        .objects
        .last()
        .map(|index| block.objects[*index].offset);
    let synthetic = SYNTHETIC_OFFSET_BASE
        + block
            .objects
            .iter()
            .filter(|object| object.offset >= SYNTHETIC_OFFSET_BASE)
            .count();
    if let Some(last) = chain.objects.last() {
        block.objects[*last].next = synthetic as i32;
    }
    block.objects.push(RdbObject {
        offset: synthetic,
        next: -1,
        previous: previous.map_or(-1, |offset| offset as i32),
        position,
        resource: RdbResource::Model(RdbModelResource {
            x_rotation: 0,
            y_rotation,
            z_rotation: 0,
            model_index,
            trigger_flag: 0,
            sound_index: 0,
            action_offset: 0,
            action: None,
        }),
    });
    Ok(())
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

    // ── M18 B2-a: `GetRdbType` / `FixRdbData` port ─────────────────────

    fn model_object(offset: usize, next: i32, position: [i32; 3], index: u16) -> RdbObject {
        RdbObject {
            offset,
            next,
            previous: -1,
            position,
            resource: RdbResource::Model(RdbModelResource {
                x_rotation: 0,
                y_rotation: 0,
                z_rotation: 0,
                model_index: index,
                trigger_flag: 0,
                sound_index: 0,
                action_offset: 0,
                action: None,
            }),
        }
    }

    /// Block image with `roots` (all -1 by default) and the given object
    /// chains, each linked in order (`next` = following offset).
    fn block_image(roots: usize, chains: &[&[(usize, [i32; 3], u16)]]) -> RdbBlock {
        let mut block = RdbBlock {
            header: RdbHeader {
                unknown1: 0,
                width: roots as u32,
                height: 1,
                object_root_offset: object_root_offset_expected() as u32,
                unknown2: 0,
            },
            model_references: (0..MODEL_REFERENCE_COUNT)
                .map(|_| RdbModelReference {
                    model_id: Some(1),
                    description: *b"WAL",
                })
                .collect(),
            model_data: vec![0; MODEL_REFERENCE_COUNT],
            roots: vec![-1; roots],
            objects: Vec::new(),
        };
        for (root, chain) in chains.iter().enumerate() {
            for (position, (offset, position_xyz, index)) in chain.iter().enumerate() {
                let next = chain.get(position + 1).map_or(-1, |(next, _, _)| *next as i32);
                block
                    .objects
                    .push(model_object(*offset, next, *position_xyz, *index));
            }
            if !chain.is_empty() {
                block.roots[root] = chain[0].0 as i32;
            }
        }
        block
    }

    #[test]
    fn rdb_type_matches_blocks_file_prefixes() {
        // `BlocksFile.cs:243-258`, in the source's order of tests.
        assert_eq!(rdb_type("B0000000.RDB"), RdbType::Border);
        assert_eq!(rdb_type("W0000009.RDB"), RdbType::Wet);
        assert_eq!(rdb_type("S0000001.RDB"), RdbType::Quest);
        assert_eq!(rdb_type("M0000001.RDB"), RdbType::Mausoleum);
        assert_eq!(rdb_type("N0000055.RDB"), RdbType::Normal);
        // Not a prefix of the source: Unknown (never Start, `:238-243`).
        assert_eq!(rdb_type("X0000001.RDB"), RdbType::Unknown);
        assert_eq!(rdb_type(""), RdbType::Unknown);
        // Case-sensitive, as C# `StartsWith(string)` is ordinal.
        assert_eq!(rdb_type("n0000055.RDB"), RdbType::Unknown);
        assert_eq!(RdbType::Border.to_u8(), 1);
        assert_eq!(RdbType::Start.to_u8(), 6);
        assert_eq!(RdbType::ALL.len(), 7);
    }

    #[test]
    fn chains_reconstruct_root_lists_in_cell_order() {
        let block = block_image(
            4,
            &[
                &[(100, [1, 2, 3], 1), (200, [4, 5, 6], 1)],
                &[],
                &[(300, [7, 8, 9], 1)],
            ],
        );
        let chains = block.chains();
        assert_eq!(chains.len(), 2);
        assert_eq!(chains[0].root, 0);
        assert_eq!(chains[0].objects, vec![0, 1]);
        assert_eq!(chains[1].root, 2);
        assert_eq!(chains[1].objects, vec![2]);
    }

    #[test]
    fn fix_rdb_data_is_an_explicit_no_op_for_unknown_blocks() {
        // Negative control: a real record that DFU does NOT repair.
        let mut block = block_image(1, &[&[(100, [1, 2, 3], 7)]]);
        let before = block.clone();
        let report = fix_rdb_data("N0000055.RDB", &mut block);
        assert!(!report.known, "N0000055 is not in FixRdbData");
        assert!(!report.pass(), "no-op is not a PASS");
        assert!(report.applied.is_empty());
        assert!(report.unresolved.is_empty());
        assert_eq!(block, before, "0 mutation without a fix");
        assert_eq!(report.rdb_type, RdbType::Normal);
    }

    #[test]
    fn fix_rdb_data_never_panics_on_a_malformed_known_block() {
        // Known name, empty image: every root is missing → typed gaps.
        let mut block = block_image(1, &[]);
        let report = fix_rdb_data("N0000071.RDB", &mut block);
        assert!(report.known);
        assert!(!report.applied.is_empty() || !report.unresolved.is_empty());
        assert!(!report.pass(), "gaps are not a PASS");
        assert!(report.unresolved.iter().all(|gap| !gap.reason.is_empty()));

        // Model reference table exhausted: explicit gap, not an index panic.
        let mut block = block_image(2, &[&[(100, [0, 0, 0], 0)]]);
        for reference in &mut block.model_references {
            reference.model_id = Some(1);
        }
        let report = fix_rdb_data("W0000009.RDB", &mut block);
        assert!(report.unresolved.iter().any(|gap| gap
            .reason
            .contains("ModelReferenceList without a free slot")));
    }

    #[test]
    fn fix_rdb_data_removes_the_bad_door_of_n0000022_and_n0000023() {
        for name in ["N0000022.RDB", "N0000023.RDB"] {
            let mut block = block_image(
                6,
                &[
                    &[(100, [0, 0, 0], 1)],
                    &[(200, [0, 0, 0], 1)],
                    &[(300, [0, 0, 0], 1), (400, [0, 0, 0], 1)],
                ],
            );
            let report = fix_rdb_data(name, &mut block);
            assert_eq!(report.applied, vec![RdbFix::BadDoorRemoved]);
            assert!(report.pass());
            assert_eq!(block.roots[2], -1, "root 2 stops pointing at the door");
            assert_eq!(block.objects.len(), 2, "door chain dropped");
            assert_eq!(block.resource_counts().0, 2);
        }
    }

    #[test]
    fn fix_rdb_data_restores_the_western_dead_end_of_n0000071() {
        // Roots 4 and 5 with ≥19 / ≥9 objects so positions 18 and 8 exist.
        let mut chain4: Vec<(usize, [i32; 3], u16)> = (0..19)
            .map(|i| (1000 + i * 10, [0, 0, 0], 1))
            .collect();
        chain4[18] = (1180, [0, 0, 0], 1);
        let mut chain5: Vec<(usize, [i32; 3], u16)> = (0..9)
            .map(|i| (2000 + i * 10, [0, 0, 0], 1))
            .collect();
        chain5[8] = (2080, [0, 0, 0], 1);
        let chains: Vec<&[(usize, [i32; 3], u16)]> = vec![
            &[],
            &[],
            &[],
            &[],
            &chain4,
            &chain5,
            &[],
        ];
        let mut block = block_image(7, &chains);
        let report = fix_rdb_data("N0000071.RDB", &mut block);
        assert_eq!(report.applied, vec![RdbFix::WesternDeadEndConnected]);
        let chains = block.chains();
        let root4 = chains.iter().find(|chain| chain.root == 4).unwrap();
        let object18 = &block.objects[root4.objects[18]];
        assert_eq!(object18.position[0], 640, "XPos connected");
        let RdbResource::Model(model) = object18.resource else {
            panic!("expected a Model");
        };
        assert_eq!(model.model_index, 10);
        let root5 = chains.iter().find(|chain| chain.root == 5).unwrap();
        let RdbResource::Model(model) = block.objects[root5.objects[8]].resource else {
            panic!("expected a Model");
        };
        assert_eq!(model.model_index, 11);
    }

    #[test]
    fn fix_rdb_data_appends_the_wall_and_links_the_chain_of_w0000009() {
        // Root 5 (39 objects), root 8 (exit), root 4 (start marker).
        let chain5: Vec<(usize, [i32; 3], u16)> = (0..40)
            .map(|i| (3000 + i * 10, [0, 0, 0], 1))
            .collect();
        let chain8: Vec<(usize, [i32; 3], u16)> = vec![(4000, [0, 0, 0], 1)];
        let chain4: Vec<(usize, [i32; 3], u16)> = (0..6)
            .map(|i| (5000 + i * 10, [0, 0, 0], 1))
            .collect();
        let chains: Vec<&[(usize, [i32; 3], u16)]> = vec![
            &[],
            &[],
            &[],
            &[],
            &chain4,
            &chain5,
            &[],
            &[],
            &chain8,
        ];
        let mut block = block_image(9, &chains);
        // One free model reference at slot 5, the rest used.
        for (index, reference) in block.model_references.iter_mut().enumerate() {
            reference.model_id = (index != 5).then_some(1);
        }
        let before = block.objects.len();
        let report = fix_rdb_data("W0000009.RDB", &mut block);
        assert_eq!(report.applied, vec![RdbFix::ExitMovedAndSealed]);
        assert!(report.pass(), "gaps: {:?}", report.unresolved);
        assert_eq!(block.objects.len(), before + 1, "the wall is appended");
        // Wall door model 72100 "DOR" landed on the free slot.
        assert_eq!(block.model_references[5].model_id, Some(72100));
        assert_eq!(&block.model_references[5].description, b"DOR");
        // The chain of root 8 reaches the new wall.
        let chains = block.chains();
        let root8 = chains.iter().find(|chain| chain.root == 8).unwrap();
        assert_eq!(root8.objects.len(), 2);
        let wall = &block.objects[root8.objects[1]];
        assert_eq!(wall.offset, SYNTHETIC_OFFSET_BASE);
        let RdbResource::Model(model) = wall.resource else {
            panic!("the wall is a Model");
        };
        assert_eq!(model.model_index, 5);
        assert_eq!(model.y_rotation, -512);
        // Exit and start marker followed the corner.
        let chains = block.chains();
        let root5 = chains.iter().find(|chain| chain.root == 5).unwrap();
        let corner = &block.objects[root5.objects[39]];
        assert_eq!(block.objects[root8.objects[0]].position[0], corner.position[0]);
        assert_eq!(
            block.objects[root8.objects[0]].position[2],
            corner.position[2] + 126
        );
        assert_eq!(wall.position[2], corner.position[2] + 128);
    }

    #[test]
    fn fix_rdb_data_appends_corridor_and_stair_models() {
        let chain6: Vec<(usize, [i32; 3], u16)> = (0..4)
            .map(|i| (6000 + i * 10, [0, 0, 0], 1))
            .collect();
        let chain2: Vec<(usize, [i32; 3], u16)> = vec![(6500, [0, 0, 0], 1)];
        let chain4: Vec<(usize, [i32; 3], u16)> = (0..39)
            .map(|i| (7000 + i * 10, [0, 0, 0], 1))
            .collect();
        let chain5: Vec<(usize, [i32; 3], u16)> = (0..41)
            .map(|i| (8000 + i * 10, [0, 0, 0], 1))
            .collect();
        let chains: Vec<&[(usize, [i32; 3], u16)]> = vec![
            &[],
            &[],
            &chain2,
            &[],
            &chain4,
            &chain5,
            &chain6,
        ];
        let mut block = block_image(7, &chains);
        let report = fix_rdb_data("W0000018.RDB", &mut block);
        assert_eq!(report.applied, vec![RdbFix::UpperRoomsJoined]);
        assert!(report.pass(), "gaps: {:?}", report.unresolved);
        let last = block.objects.last().unwrap();
        assert_eq!(last.position, [1152, -1792, 1920]);
        let RdbResource::Model(model) = last.resource else {
            panic!("the corridor is a Model");
        };
        assert_eq!(model.model_index, 11);
        assert_eq!(model.y_rotation, -512);

        let mut block = block_image(
            6,
            &[
                &[],
                &[],
                &[],
                &[],
                &(0..16).map(|i| (8000 + i * 10, [0, 0, 0], 1)).collect::<Vec<_>>(),
                &(0..35).map(|i| (9000 + i * 10, [0, 0, 0], 1)).collect::<Vec<_>>(),
            ],
        );
        let report = fix_rdb_data("W0000020.RDB", &mut block);
        assert_eq!(report.applied, vec![RdbFix::LowerDeadEndOpened]);
        let last = block.objects.last().unwrap();
        assert_eq!(last.position, [1152, -384, 1408]);
        let RdbResource::Model(model) = last.resource else {
            panic!("the stair is a Model");
        };
        assert_eq!(model.model_index, 4);
        assert_eq!(model.y_rotation, 512);
    }

    #[test]
    fn fix_rdb_data_is_deterministic_for_the_same_image() {
        let build = || {
            let chain5: Vec<(usize, [i32; 3], u16)> = (0..40)
                .map(|i| (3000 + i * 10, [0, 0, 0], 1))
                .collect();
            let chains: Vec<&[(usize, [i32; 3], u16)]> = vec![
                &[],
                &[],
                &[],
                &[],
                &[(5000, [0, 0, 0], 1)],
                &chain5,
                &[],
                &[],
                &[(4000, [0, 0, 0], 1)],
            ];
            block_image(9, &chains)
        };
        let mut first = build();
        let mut second = build();
        let a = fix_rdb_data("W0000009.RDB", &mut first);
        let b = fix_rdb_data("W0000009.RDB", &mut second);
        assert_eq!(a, b);
        assert_eq!(first, second);
    }
}
