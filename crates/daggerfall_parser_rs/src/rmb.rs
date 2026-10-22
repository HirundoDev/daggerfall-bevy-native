//! `.RMB` (Map Block Records of BLOCKS.BSA) — clean-room parser (M10,
//! REQ-L99-042).
//!
//! Spec: UESP `Daggerfall_Mod:BLOCKS.BSA/Map_Block_Records` (verified
//! 2026-10-19) + reference reader `DFU:Assets/Scripts/API/BlocksFile.cs`
//! (`ReadRmbFldHeader`/`ReadRmbBlockData`/`ReadRmbBlockSubRecord`, the
//! pinned DFU MIT corpus is the origin of the subset; the full reader is
//! not in the subcorpus → spec = UESP + cited MIT upstream).
//!
//! Layout (UESP Map Block Records; fixed `RmbHeader` of 6776 B):
//! ```text
//! ObjectCount (3 B: BlockCount u8 · ModelCount u8 · FlatCount u8)
//! BlockPositionList (32 × 20 B: Unknown1 u32 · Unknown2 u32 · X i32 · Z i32
//!   · RotationAngle i32 — Da, 720 per revolution, BlocksFile.RotationDivisor
//!   = 5.68888888888889)
//! BuildingDataList (32 × 26 B — [`crate::maps::BuildingData`])
//! Section2 (128 B BlockPtr + 128 B BlockSize[32] + 8 B ModelPtr/FlatPtr)
//! GroundData (256 B tiles 16×16 + 256 B scenery 16×16)
//! Automap (4096 B = bitmap 64×64)
//! FileNameList (13 B own name + 32 × 13 B)
//! BlockList (BlockCount sub-records; the step of each one is fixed by
//!   BlockSize[i] — DFU ignores padding and skips by accumulated size)
//! ?Misc3dObjectRecords (66 B each) · ?MiscFlatObjectRecords (17 B each)
//! ```
//!
//! Semantics (DFU `BlocksFile.cs` + `RMBLayout.GetBuildingData`): each
//! sub-record `i` IS a building of the block: `BlockPositions[i]` gives its
//! translation (X, Z) and Y rotation; `BuildingDataList[i]` its metadata
//! (type/quality/seed/faction); the **first** 3D model of its
//! `Exterior.BlockData.ModelList` is the ARCH3D model of the building
//! (`RMBLayout.cs:550-590`, `GetBuildingData`: "First model of record is
//! building model"). `ObjectCount.BlockCount` = number of sub-records =
//! actual number of buildings in the block.
//!
//! Sub-`BlockData` (17 B header: 5×u8 counts + 6×i16):
//! Model 66 B (ModelId: i16@0 ×100 + u8@2 → ARCH3D record; Point2 position
//! i32×3@36; YRotation i16@52) · Flat 17 B · Section3 16 B · Person 17 B ·
//! Door 19 B (position i32×3, YRotation i16, OpenRotation i16,
//! DoorModelIndex u8).

use crate::maps::BuildingData;

/// Fixed length of RmbHeader (UESP: 6776 B).
pub const RMB_HEADER_LEN: usize = 6776;
/// Maximum sub-records/buildings per block (ObjectCount.BlockCount ≤ 32).
pub const MAX_SUB_RECORDS: usize = 32;
/// Size of the automap bitmap (64×64).
pub const AUTOMAP_LEN: usize = 4096;
/// Ground tiles per axis (16×16 per block — `DFU:BlocksFile.cs` reads 16×16).
pub const GROUND_TILES_PER_EDGE: usize = 16;

/// `.RMB` parse errors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RmbError {
    /// Record shorter than a required section.
    TooShort { len: usize, need: usize },
    /// BlockCount > 32 (UESP invariant).
    BlockCountTooLarge { count: u8 },
    /// Sub-record `index` goes outside the buffer.
    SubRecordOutOfBounds { index: usize, start: usize, end: usize, len: usize },
    /// Sub-record `index` with size ≤ 0 in BlockDataSizes.
    BadSubRecordSize { index: usize, size: i32 },
    /// Truncated section inside a BlockData.
    SectionTruncated { what: &'static str, offset: usize, need: usize, len: usize },
    /// The final misc section exceeds the buffer.
    MiscOutOfBounds { offset: usize, need: usize, len: usize },
}

impl std::fmt::Display for RmbError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TooShort { len, need } => write!(f, "RMB too short: {len} < {need}"),
            Self::BlockCountTooLarge { count } => write!(f, "BlockCount {count} > 32"),
            Self::SubRecordOutOfBounds { index, start, end, len } => write!(
                f,
                "sub-record {index} out of bounds: [{start}, {end}) > {len}"
            ),
            Self::BadSubRecordSize { index, size } => {
                write!(f, "sub-record {index}: BlockDataSizes {size} <= 0")
            }
            Self::SectionTruncated { what, offset, need, len } => {
                write!(f, "{what} truncated at {offset}: +{need} > {len}")
            }
            Self::MiscOutOfBounds { offset, need, len } => {
                write!(f, "misc section at {offset}: +{need} > {len}")
            }
        }
    }
}

impl std::error::Error for RmbError {}

fn u16_le(data: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([data[at], data[at + 1]])
}

fn i16_le(data: &[u8], at: usize) -> i16 {
    i16::from_le_bytes([data[at], data[at + 1]])
}

fn i32_le(data: &[u8], at: usize) -> i32 {
    i32::from_le_bytes([data[at], data[at + 1], data[at + 2], data[at + 3]])
}

/// Position of a sub-record (building) inside the block.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct BlockPosition {
    /// Building X translation (native units, 0..4096).
    pub x_pos: i32,
    /// Building Z translation (native units).
    pub z_pos: i32,
    /// Y rotation in Da (720 = full revolution).
    pub y_rotation: i32,
}

/// Ground tile (bitfield 0-5 texture · 6 rotated · 7 flipped — DFU reads
/// `IsRotated = 0x40` and `IsFlipped = 0x80` separately).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct GroundTile {
    /// Texture index (0-63).
    pub texture_record: u8,
    /// Rotated 90°.
    pub rotated: bool,
    /// Flipped.
    pub flipped: bool,
}

/// 3D model (66 B) of a BlockData.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Model3d {
    /// Referenced ARCH3D.BSA record (`Id1 * 100 + Id2`).
    pub model_id: u32,
    /// Object type (byte@3, uninterpreted).
    pub object_type: u8,
    /// Local X position (native units).
    pub x_pos: i32,
    /// Local Y position.
    pub y_pos: i32,
    /// Local Z position.
    pub z_pos: i32,
    /// Y rotation (Da).
    pub y_rotation: i16,
}

/// Flat/billboard (17 B).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Flat {
    /// Local X position.
    pub x_pos: i32,
    /// Local Y position.
    pub y_pos: i32,
    /// Local Z position.
    pub z_pos: i32,
    /// Texture archive (bitfield >> 7).
    pub texture_archive: u16,
    /// Texture record (bitfield & 0x7f).
    pub texture_record: u16,
    /// Faction (0 = no NPC).
    pub faction_id: i16,
}

/// Door (19 B).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Door {
    /// Local X position.
    pub x_pos: i32,
    /// Local Y position.
    pub y_pos: i32,
    /// Local Z position.
    pub z_pos: i32,
    /// Y rotation (Da).
    pub y_rotation: i16,
    /// Opening angle (degrees).
    pub open_rotation: i16,
    /// Door model index within the building model.
    pub door_model_index: u8,
}

/// A BlockData (exterior or interior of a sub-record).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BlockData {
    /// 3D models ([0] of the exterior = building model).
    pub models: Vec<Model3d>,
    /// Flats/billboards.
    pub flats: Vec<Flat>,
    /// Section 3 (count; content uninterpreted in M10).
    pub section3_count: u8,
    /// Persons (count; M15 uses them — only the count is kept here).
    pub person_count: u8,
    /// Doors.
    pub doors: Vec<Door>,
}

/// Sub-record = one building of the block (exterior + interior).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SubRecord {
    /// Building X translation (copied from the header by DFU).
    pub x_pos: i32,
    /// Building Z translation.
    pub z_pos: i32,
    /// Building Y rotation (Da).
    pub y_rotation: i32,
    /// exterior geometry (facade + building surroundings).
    pub exterior: BlockData,
    /// interior geometry (pending M11).
    pub interior: BlockData,
}

/// Parsed Map Block Record.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MapBlockRecord {
    /// Number of sub-records (buildings) — BlockCount of ObjectCount.
    pub num_block_data_records: u8,
    /// Misc models (block props).
    pub num_misc_3d_object_records: u8,
    /// Misc flats.
    pub num_misc_flat_object_records: u8,
    /// Positions of the 32 sub-records (only the first `num_…` are valid).
    pub block_positions: Vec<BlockPosition>,
    /// BuildingDataList (32 entries; only the first `num_…` are valid).
    pub buildings: Vec<BuildingData>,
    /// Sizes in bytes of each sub-record (read step).
    pub block_data_sizes: Vec<i32>,
    /// 16×16 ground tiles (read order: y outer, x inner).
    pub ground_tiles: Vec<GroundTile>,
    /// 16×16 ground scenery (raw bitfields; 0xff = empty).
    pub ground_scenery: Vec<u8>,
    /// 64×64 automap (0 = empty; value−1 = building type, UESP).
    pub automap: Vec<u8>,
    /// Name of the record itself (FileNameList[0]).
    pub block_file_name: String,
    /// Sub-records (buildings) in order.
    pub sub_records: Vec<SubRecord>,
    /// 3D props of the block (after BlockList).
    pub misc_3d_models: Vec<Model3d>,
    /// Misc flats of the block.
    pub misc_flats: Vec<Flat>,
}

impl MapBlockRecord {
    /// Building `i` (convenience: BuildingDataList[i] if present).
    pub fn building(&self, i: usize) -> Option<&BuildingData> {
        self.buildings.get(i)
    }

    /// Ground tile at (x, y) (out of range → `None`).
    pub fn ground_tile(&self, x: usize, y: usize) -> Option<&GroundTile> {
        if x >= GROUND_TILES_PER_EDGE || y >= GROUND_TILES_PER_EDGE {
            return None;
        }
        self.ground_tiles.get(y * GROUND_TILES_PER_EDGE + x)
    }

    /// Automap pixel (x, y) 0..64 (out of range → 0).
    pub fn automap_pixel(&self, x: usize, y: usize) -> u8 {
        if x >= 64 || y >= 64 {
            return 0;
        }
        self.automap.get(y * 64 + x).copied().unwrap_or(0)
    }

    /// Building type of the automap pixel (UESP: value − 1; 0 = empty).
    pub fn automap_building_type(&self, x: usize, y: usize) -> Option<u8> {
        match self.automap_pixel(x, y) {
            0 => None,
            v => Some(v - 1),
        }
    }
}

/// Parses a complete Map Block Record (read-only, 0 panics: everything
/// indexed with bounds checks that return [`RmbError`]).
pub fn parse_rmb(data: &[u8]) -> Result<MapBlockRecord, RmbError> {
    if data.len() < RMB_HEADER_LEN {
        return Err(RmbError::TooShort { len: data.len(), need: RMB_HEADER_LEN });
    }
    let num_block_data_records = data[0];
    if num_block_data_records as usize > MAX_SUB_RECORDS {
        return Err(RmbError::BlockCountTooLarge { count: num_block_data_records });
    }

    // BlockPositionList (3 @ 3 + 32×20).
    let mut block_positions = Vec::with_capacity(MAX_SUB_RECORDS);
    for i in 0..MAX_SUB_RECORDS {
        let at = 3 + i * 20;
        block_positions.push(BlockPosition {
            x_pos: i32_le(data, at + 8),
            z_pos: i32_le(data, at + 12),
            y_rotation: i32_le(data, at + 16),
        });
    }

    // BuildingDataList (3 @ 3+640 + 32×26).
    let mut buildings = Vec::with_capacity(MAX_SUB_RECORDS);
    for i in 0..MAX_SUB_RECORDS {
        let at = 643 + i * 26;
        buildings.push(BuildingData {
            name_seed: u16_le(data, at),
            faction_id: u16_le(data, at + 18),
            sector: i16_le(data, at + 20),
            location_id: u16_le(data, at + 22),
            building_type: data[at + 24],
            quality: data[at + 25],
        });
    }

    // Section2: BlockPtr 128 B (skipped) + BlockDataSizes 32×4 + 8 B reserved.
    let mut block_data_sizes = Vec::with_capacity(MAX_SUB_RECORDS);
    for i in 0..MAX_SUB_RECORDS {
        block_data_sizes.push(i32_le(data, 1475 + 128 + i * 4));
    }

    // GroundData: 8 B header (ModelPtr/FlatPtr, skipped) + 256 tiles + 256 scenery.
    let mut ground_tiles = Vec::with_capacity(256);
    for i in 0..256 {
        let bitfield = data[1739 + i];
        ground_tiles.push(GroundTile {
            texture_record: bitfield & 0x3f,
            rotated: bitfield & 0x40 != 0,
            flipped: bitfield & 0x80 != 0,
        });
    }
    let ground_scenery = data[1995..1995 + 256].to_vec();

    // Automap (4096) + FileNameList (13 + 32×13).
    let automap = data[2251..2251 + AUTOMAP_LEN].to_vec();
    let block_file_name = read_c_string(data, 6347, 13);

    // BlockList: sub-records stepped by BlockDataSizes (padding ignored,
    // same scheme as DFU `ReadRmbBlockData`).
    let count = num_block_data_records as usize;
    let mut sub_records = Vec::with_capacity(count);
    let mut cursor = RMB_HEADER_LEN;
    for i in 0..count {
        let size = block_data_sizes[i];
        if size <= 0 {
            return Err(RmbError::BadSubRecordSize { index: i, size });
        }
        let exterior = parse_block_data(data, cursor)?;
        let interior_start = exterior.consumed;
        let interior = parse_block_data(data, cursor + interior_start)?;
        sub_records.push(SubRecord {
            x_pos: block_positions[i].x_pos,
            z_pos: block_positions[i].z_pos,
            y_rotation: block_positions[i].y_rotation,
            exterior: exterior.data,
            interior: interior.data,
        });
        cursor = cursor
            .checked_add(size as usize)
            .ok_or(RmbError::SubRecordOutOfBounds {
                index: i,
                start: cursor,
                end: usize::MAX,
                len: data.len(),
            })?;
        if cursor > data.len() {
            return Err(RmbError::SubRecordOutOfBounds {
                index: i,
                start: cursor,
                end: cursor,
                len: data.len(),
            });
        }
    }

    // Misc 3D objects + misc flat objects (at the end of the record).
    let mut misc_3d_models = Vec::with_capacity(data[1] as usize);
    for i in 0..data[1] as usize {
        let at = cursor + i * 66;
        if at + 66 > data.len() {
            return Err(RmbError::MiscOutOfBounds {
                offset: at,
                need: 66,
                len: data.len(),
            });
        }
        misc_3d_models.push(parse_model(data, at));
    }
    cursor += data[1] as usize * 66;
    let mut misc_flats = Vec::with_capacity(data[2] as usize);
    for i in 0..data[2] as usize {
        let at = cursor + i * 17;
        if at + 17 > data.len() {
            return Err(RmbError::MiscOutOfBounds {
                offset: at,
                need: 17,
                len: data.len(),
            });
        }
        misc_flats.push(parse_flat(data, at));
    }

    Ok(MapBlockRecord {
        num_block_data_records,
        num_misc_3d_object_records: data[1],
        num_misc_flat_object_records: data[2],
        block_positions,
        buildings,
        block_data_sizes,
        ground_tiles,
        ground_scenery,
        automap,
        block_file_name,
        sub_records,
        misc_3d_models,
        misc_flats,
    })
}

fn read_c_string(data: &[u8], at: usize, width: usize) -> String {
    let slot = &data[at..(at + width).min(data.len())];
    let end = slot.iter().position(|&b| b == 0).unwrap_or(slot.len());
    String::from_utf8_lossy(&slot[..end]).into_owned()
}

fn parse_model(data: &[u8], at: usize) -> Model3d {
    Model3d {
        // Id1 is read as Int16 in DFU and UInt16 in UESP; the real retail
        // range is non-negative → a cast to u16 preserves both meanings.
        model_id: u32::from(i16_le(data, at) as u16) * 100 + u32::from(data[at + 2]),
        object_type: data[at + 3],
        x_pos: i32_le(data, at + 36),
        y_pos: i32_le(data, at + 40),
        z_pos: i32_le(data, at + 44),
        y_rotation: i16_le(data, at + 52),
    }
}

fn parse_flat(data: &[u8], at: usize) -> Flat {
    let texture = u16_le(data, at + 12);
    Flat {
        x_pos: i32_le(data, at),
        y_pos: i32_le(data, at + 4),
        z_pos: i32_le(data, at + 8),
        texture_archive: texture >> 7,
        texture_record: texture & 0x7f,
        faction_id: i16_le(data, at + 14),
    }
}

struct ParsedBlockData {
    data: BlockData,
    consumed: usize,
}

fn parse_block_data(data: &[u8], at: usize) -> Result<ParsedBlockData, RmbError> {
    let len = data.len();
    let trunc = |what: &'static str, offset: usize, need: usize| RmbError::SectionTruncated {
        what,
        offset,
        need,
        len,
    };
    if at + 17 > len {
        return Err(trunc("block_data_header", at, 17));
    }
    let model_count = data[at] as usize;
    let flat_count = data[at + 1] as usize;
    let section3_count = data[at + 2];
    let person_count = data[at + 3];
    let door_count = data[at + 4] as usize;
    let mut p = at + 17;

    if p + model_count * 66 > len {
        return Err(trunc("model_list", p, model_count * 66));
    }
    let mut models = Vec::with_capacity(model_count);
    for i in 0..model_count {
        models.push(parse_model(data, p + i * 66));
    }
    p += model_count * 66;

    if p + flat_count * 17 > len {
        return Err(trunc("flat_list", p, flat_count * 17));
    }
    let mut flats = Vec::with_capacity(flat_count);
    for i in 0..flat_count {
        flats.push(parse_flat(data, p + i * 17));
    }
    p += flat_count * 17;

    p += section3_count as usize * 16;
    p += person_count as usize * 17;

    if p + door_count * 19 > len {
        return Err(trunc("door_list", p, door_count * 19));
    }
    let mut doors = Vec::with_capacity(door_count);
    for i in 0..door_count {
        let d = p + i * 19;
        doors.push(Door {
            x_pos: i32_le(data, d),
            y_pos: i32_le(data, d + 4),
            z_pos: i32_le(data, d + 8),
            y_rotation: i16_le(data, d + 12),
            open_rotation: i16_le(data, d + 14),
            door_model_index: data[d + 16],
        });
    }
    p += door_count * 19;

    Ok(ParsedBlockData {
        data: BlockData {
            models,
            flats,
            section3_count,
            person_count,
            doors,
        },
        consumed: p - at,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Synthetic fixture: minimal RMB with 1 sub-record (building) of 1 model
    /// + 1 door, ground tiles and an automap with one Tavern-type pixel (16).
    fn fixture_rmb() -> Vec<u8> {
        let mut rec = vec![0u8; 2048];
        // Fixed 6776 B header + room for the sub-record.
        rec.resize(RMB_HEADER_LEN + 256, 0);
        rec[0] = 1; // BlockCount
        rec[1] = 0; // Misc3d
        rec[2] = 0; // MiscFlat
        // BlockPositions[0]: X=2048, Z=1024, rot=180.
        rec[11..15].copy_from_slice(&2048i32.to_le_bytes());
        rec[15..19].copy_from_slice(&1024i32.to_le_bytes());
        rec[19..23].copy_from_slice(&180i32.to_le_bytes());
        // BuildingData[0]: seed 7, faction 2, sector 0, location 55, Tavern, quality 12.
        rec[643..645].copy_from_slice(&7u16.to_le_bytes());
        rec[661..663].copy_from_slice(&2u16.to_le_bytes()); // FactionId
        rec[663..665].copy_from_slice(&0i16.to_le_bytes()); // Sector
        rec[665..667].copy_from_slice(&55u16.to_le_bytes()); // LocationId
        rec[667] = 15;
        rec[668] = 12;
        // BlockDataSizes[0] = exterior(17+66+19=102) + interior(17) = 119.
        let sub_size: i32 = 17 + 66 + 19 + 17;
        rec[1603..1607].copy_from_slice(&sub_size.to_le_bytes());
        // Ground tile (0,0): texture 3.
        rec[1739] = 3;
        // Scenery (0,0): empty.
        rec[1995] = 0xff;
        // Automap (0,0) = 16 → type 15 (Tavern).
        rec[2251] = 16;
        // FileNameList[0].
        rec[6347..6356].copy_from_slice(b"TVRNAA00\0");
        // Sub-record: exterior BlockData.
        let mut p = RMB_HEADER_LEN;
        rec[p] = 1; // models
        rec[p + 1] = 0; // flats
        rec[p + 2] = 0; // s3
        rec[p + 3] = 0; // persons
        rec[p + 4] = 1; // doors
        p += 17;
        // Model: id1=44, id2=5 → 4405; pos (100, 0, 200); rot 256.
        rec[p..p + 2].copy_from_slice(&44i16.to_le_bytes());
        rec[p + 2] = 5;
        rec[p + 36..p + 40].copy_from_slice(&100i32.to_le_bytes());
        rec[p + 40..p + 44].copy_from_slice(&0i32.to_le_bytes());
        rec[p + 44..p + 48].copy_from_slice(&200i32.to_le_bytes());
        rec[p + 52..p + 54].copy_from_slice(&256i16.to_le_bytes());
        p += 66;
        // Door: (10, 0, 30), rot 0, open 90, model index 0.
        rec[p..p + 4].copy_from_slice(&10i32.to_le_bytes());
        rec[p + 8..p + 12].copy_from_slice(&30i32.to_le_bytes());
        rec[p + 14..p + 16].copy_from_slice(&90i16.to_le_bytes());
        p += 19;
        // Empty interior BlockData (header only).
        // (17 bytes already zero)
        p += 17;
        rec.truncate(p);
        rec
    }

    #[test]
    fn parse_fixture_rmb() {
        let rec = parse_rmb(&fixture_rmb()).unwrap();
        assert_eq!(rec.num_block_data_records, 1);
        assert_eq!(rec.block_file_name, "TVRNAA00");
        assert_eq!(rec.buildings[0].building_type, 15);
        assert_eq!(rec.buildings[0].quality, 12);
        assert_eq!(rec.buildings[0].name_seed, 7);
        assert_eq!(rec.buildings[0].faction_id, 2);
        assert_eq!(rec.buildings[0].location_id, 55);
        assert_eq!(rec.block_positions[0].x_pos, 2048);
        assert_eq!(rec.block_positions[0].z_pos, 1024);
        assert_eq!(rec.block_positions[0].y_rotation, 180);
        assert_eq!(rec.sub_records.len(), 1);
        let sub = &rec.sub_records[0];
        assert_eq!(sub.x_pos, 2048);
        assert_eq!(sub.exterior.models.len(), 1);
        assert_eq!(sub.exterior.models[0].model_id, 4405);
        assert_eq!(sub.exterior.models[0].x_pos, 100);
        assert_eq!(sub.exterior.models[0].y_rotation, 256);
        assert_eq!(sub.exterior.doors.len(), 1);
        assert_eq!(sub.exterior.doors[0].x_pos, 10);
        assert_eq!(sub.exterior.doors[0].open_rotation, 90);
        assert_eq!(sub.interior.models.len(), 0);
        let tile = rec.ground_tile(0, 0).unwrap();
        assert_eq!(tile.texture_record, 3);
        assert_eq!(rec.automap_building_type(0, 0), Some(15));
        assert_eq!(rec.automap_building_type(1, 0), None);
    }

    #[test]
    fn truncated_rmb_is_error_not_panic() {
        let rec = fixture_rmb();
        assert!(matches!(parse_rmb(&rec[..100]), Err(RmbError::TooShort { .. })));
        // Sub-record that goes outside the buffer.
        let mut short = fixture_rmb();
        short[1603..1607].copy_from_slice(&10_000i32.to_le_bytes());
        assert!(parse_rmb(&short).is_err());
        // BlockCount > 32.
        let mut bad = fixture_rmb();
        bad[0] = 33;
        assert!(matches!(parse_rmb(&bad), Err(RmbError::BlockCountTooLarge { count: 33 })));
    }

    #[test]
    fn all_zero_rmb_header_is_safe() {
        // Fully zeroed header: 0 sub-records, 0 misc — parses without panic.
        let mut rec = vec![0u8; RMB_HEADER_LEN];
        rec.resize(RMB_HEADER_LEN, 0);
        let parsed = parse_rmb(&rec).unwrap();
        assert_eq!(parsed.num_block_data_records, 0);
        assert!(parsed.sub_records.is_empty());
    }
}
