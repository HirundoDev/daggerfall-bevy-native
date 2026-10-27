//! MAPS.BSA — region records (MapNames / MapTable / MapPItem) + `.RMB` block
//! name resolution (M10).
//!
//! Clean-room implementation of the UESP spec `Daggerfall_Mod:MAPS.BSA` and
//! `Daggerfall_Mod:Block_Record_Indexes` (verified 2026-10-19). The block
//! name algorithm replicates the reference behavior
//! `DFU:Assets/Scripts/Utility/MapsFile.cs` (`ResolveRmbBlockName`: the DFU
//! MIT corpus pinned in `reference/dfu-spec/` is the origin of the subset;
//! the complete MAPS.BSA reader is not in the pinned subcorpus → spec
//! = UESP + cited MIT upstream, mandatory DoD §a declaration).
//!
//! Layout per record (UESP MAPS.BSA):
//! - `MAPNAMES.nnn`: `UInt32 LocationCount` + `LocationCount` 32-byte names
//!   (null-terminated 7-bit ASCII). Size 4 + 32·count.
//! - `MAPTABLE.nnn`: 17-byte `MapTable` per location (count = the one from
//!   MapNames): MapId i32 · LatitudeType u32 · LongitudeType u32 · Flavor u8
//!   · Services u32. `LocationType = (LatitudeType >> 25) & 0x1f` ·
//!   `Width = (LongitudeType >> 24) & 0xf` · `Height = LongitudeType >> 28`
//!   (4096 units; only LocationType is reused in M10).
//! - `MAPPITEM.nnn`: list of `UInt32` offsets (LocationCount elements,
//!   relative to the end of the list) + one `LocationExterior` per offset:
//!   `DoorHeader u32` + `Door`×6 B + `ObjectHeader` 71 B +
//!   `LocationRecordElementHeader` 48 B + `BuildingData`×26 B +
//!   `ExteriorData` 416 B.
//! - `MAPDITEM.nnn` (M34-D, G-N1): `UInt32 DungeonCount`, then a table of
//!   `DungeonCount` × (`Offset u32`, `IsDungeon u16`, `ExteriorLocationId
//!   u16`), then one `LocationDungeon` per offset: `LocationRecordElement`,
//!   `DungeonHeader` and `DungeonBlock`×BlockCount — [`parse_ditem`], layout
//!   replicated from the pinned `DFU:API/MapsFile.cs:1275-1343`.
//!
//! `BuildingData` (26 B, identical in MAPS.BSA and in the `.RMB` header —
//! `DFU:.../API/DFLocation.cs:511-546`, `DFLocation.BuildingData`):
//! NameSeed u16@0 · ServiceTimeLimit u32@2 · Unknown u16@6 · Unknown2 u16@8 ·
//! Unknown3 u32@10 · Unknown4 u32@14 · FactionId u16@18 · Sector i16@20 ·
//! LocationId u16@22 · BuildingType u8@24 · Quality u8@25.
//!
//! `ExteriorData` (416 B, offsets used): MapId i32@32 · Width u8@40 ·
//! Height u8@41 · RegionIndex u8@45 · Letter1ForRMBName u8@46 · LocationType
//! `u8@48 · BlockIndex[64]@49 · BlockNumber[64]@113 · BlockCharacter[64]@177`.

/// Location name size (32 bytes, 7-bit ASCII).
pub const LOCATION_NAME_LEN: usize = 32;
/// Max blocks per edge in a location's exterior (UESP: ≤ 8).
pub const MAX_BLOCKS_PER_EDGE: usize = 8;
/// Block grid cells (8×8, west→east south→north).
pub const BLOCK_GRID_CELLS: usize = 64;
/// RMB prefixes per BlockIndex (`DFU:MapsFile.cs:94-101` = UESP Block Record Indexes).
const RMB_BLOCK_PREFIXES: [&str; 45] = [
    "TVRN", "GENR", "RESI", "WEAP", "ARMR", "ALCH", "BANK", "BOOK", "CLOT", "FURN", "GEMS",
    "LIBR", "PAWN", "TEMP", "TEMP", "PALA", "FARM", "DUNG", "CAST", "MANR", "SHRI", "RUIN",
    "SHCK", "GRVE", "FILL", "KRAV", "KDRA", "KOWL", "KMOO", "KCAN", "KFLA", "KHOR", "KROS",
    "KWHE", "KSCA", "KHAW", "MAGE", "THIE", "DARK", "FIGH", "CUST", "WALL", "MARK", "SHIP",
    "WITC",
];
/// Second letter of the RMB name (`DFU:MapsFile.cs:106`).
const LETTER2_ARRAY: [&str; 4] = ["A", "L", "M", "S"];

/// MAPS.BSA parsing errors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MapsError {
    /// Record shorter than its structural minimum.
    TooShort { len: usize, min: usize },
    /// Unreasonable LocationCount (> 4096): anti-allocation defense.
    LocationCountTooLarge { count: u32 },
    /// Element truncated while parsing a list.
    Truncated { what: &'static str, index: usize, offset: usize },
    /// LocationExterior offset outside the record.
    OffsetOutOfBounds { index: usize, offset: u32, len: usize },
    /// `DungeonBlock.BlockIndex` outside the 6 RDB letters
    /// (`DFU:API/MapsFile.cs:110`, `rdbBlockLetters`); the reference
    /// implementation would index out of range.
    InvalidBlockIndex { index: usize, block_index: u8 },
}

impl std::fmt::Display for MapsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TooShort { len, min } => write!(f, "record too short: {len} < {min}"),
            Self::LocationCountTooLarge { count } => {
                write!(f, "unreasonable LocationCount: {count} > 4096")
            }
            Self::Truncated { what, index, offset } => {
                write!(f, "{what}[{index}] truncated at offset {offset}")
            }
            Self::OffsetOutOfBounds { index, offset, len } => {
                write!(f, "LocationExterior[{index}] offset = {offset} > {len}")
            }
            Self::InvalidBlockIndex { index, block_index } => {
                write!(f, "DungeonBlock[{index}] BlockIndex = {block_index} > 5")
            }
        }
    }
}

impl std::error::Error for MapsError {}

fn u16_le(data: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([data[at], data[at + 1]])
}

fn i16_le(data: &[u8], at: usize) -> i16 {
    i16::from_le_bytes([data[at], data[at + 1]])
}

fn u32_le(data: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([data[at], data[at + 1], data[at + 2], data[at + 3]])
}

fn i32_le(data: &[u8], at: usize) -> i32 {
    i32::from_le_bytes([data[at], data[at + 1], data[at + 2], data[at + 3]])
}

fn c_string_32(data: &[u8]) -> String {
    let end = data.iter().position(|&b| b == 0).unwrap_or(data.len());
    String::from_utf8_lossy(&data[..end]).into_owned()
}

/// 26 B `BuildingData` element (shared MAPS.BSA ↔ `.RMB` header).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct BuildingData {
    /// Building name seed.
    pub name_seed: u16,
    /// Faction ID (0 = none).
    pub faction_id: u16,
    /// Block index inside the location (DFU Sector).
    pub sector: i16,
    /// Building ID (matches LocationId of the location header).
    pub location_id: u16,
    /// Building type ([`building_type_name`]).
    pub building_type: u8,
    /// Quality 1-20 (0 in empty slots of the RMB header).
    pub quality: u8,
}

fn parse_building_data(data: &[u8], at: usize) -> BuildingData {
    BuildingData {
        name_seed: u16_le(data, at),
        faction_id: u16_le(data, at + 18),
        sector: i16_le(data, at + 20),
        location_id: u16_le(data, at + 22),
        building_type: data[at + 24],
        quality: data[at + 25],
    }
}

/// Building type name (`DFU:DFLocation.cs:106-139`, `BuildingTypes`;
/// values ≥ 0x74 are special; the rest = `Unknown`).
pub fn building_type_name(building_type: u8) -> &'static str {
    match building_type {
        0 => "Alchemist",
        1 => "HouseForSale",
        2 => "Armorer",
        3 => "Bank",
        4 => "Town4",
        5 => "Bookseller",
        6 => "ClothingStore",
        7 => "FurnitureStore",
        8 => "GemStore",
        9 => "GeneralStore",
        10 => "Library",
        11 => "GuildHall",
        12 => "PawnShop",
        13 => "WeaponSmith",
        14 => "Temple",
        15 => "Tavern",
        16 => "Palace",
        17 => "House1",
        18 => "House2",
        19 => "House3",
        20 => "House4",
        21 => "House5",
        22 => "House6",
        23 => "Town23",
        24 => "Ship",
        0x74 => "Special1",
        0xdf => "Special2",
        0xf9 => "Special3",
        0xfa => "Special4",
        _ => "Unknown",
    }
}

/// MapTable element (17 B).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MapTableEntry {
    /// Numeric location ID.
    pub map_id: i32,
    /// LatitudeType field (latitude + type + flags).
    pub latitude_type: u32,
    /// LongitudeType field (longitude + width/height).
    pub longitude_type: u32,
    /// Dungeon discovery message (if applicable).
    pub flavor: u8,
    /// Services bitfield (guilds/shops).
    pub services: u32,
}

impl MapTableEntry {
    /// Location type (`DFU:DFRegion.cs:66+`, `LocationTypes`).
    pub fn location_type(&self) -> u8 {
        ((self.latitude_type >> 25) & 0x1f) as u8
    }
    /// Map width (4096 units).
    pub fn map_width(&self) -> u8 {
        ((self.longitude_type >> 24) & 0xf) as u8
    }
    /// Map height (4096 units).
    pub fn map_height(&self) -> u8 {
        (self.longitude_type >> 28) as u8
    }
}

/// Parses a MapTable record (17 B per element; `count` = LocationCount from
/// MapNames of the same region).
pub fn parse_map_table(record: &[u8], count: usize) -> Result<Vec<MapTableEntry>, MapsError> {
    let need = count
        .checked_mul(17)
        .ok_or(MapsError::LocationCountTooLarge { count: count as u32 })?;
    if record.len() < need {
        return Err(MapsError::TooShort {
            len: record.len(),
            min: need,
        });
    }
    let mut out = Vec::with_capacity(count);
    for i in 0..count {
        let at = i * 17;
        out.push(MapTableEntry {
            map_id: i32_le(record, at),
            latitude_type: u32_le(record, at + 4),
            longitude_type: u32_le(record, at + 8),
            flavor: record[at + 12],
            services: u32_le(record, at + 13),
        });
    }
    Ok(out)
}

/// Parses a MapNames record (`UInt32 count` + 32 B names).
pub fn parse_map_names(record: &[u8]) -> Result<Vec<String>, MapsError> {
    if record.len() < 4 {
        return Err(MapsError::TooShort { len: record.len(), min: 4 });
    }
    let count = u32_le(record, 0);
    if count > 4096 {
        return Err(MapsError::LocationCountTooLarge { count });
    }
    let count = count as usize;
    let need = 4 + count * LOCATION_NAME_LEN;
    if record.len() < need {
        return Err(MapsError::TooShort { len: record.len(), min: need });
    }
    let mut out = Vec::with_capacity(count);
    for i in 0..count {
        out.push(c_string_32(&record[4 + i * LOCATION_NAME_LEN..4 + (i + 1) * LOCATION_NAME_LEN]));
    }
    Ok(out)
}

/// Subset of `ExteriorData` needed to resolve `.RMB` names and
/// summarize the location (UESP MAPS.BSA §ExteriorData offsets).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExteriorData {
    /// MapId (associates with MapTable).
    pub map_id: i32,
    /// Width in blocks (1..=8).
    pub width: u8,
    /// Height in blocks (1..=8).
    pub height: u8,
    /// Region index of the location.
    pub region_index: u8,
    /// Letter 1 of the RMB name (offset 46; `DFU:ExteriorData.Letter1ForRMBName`).
    pub letter1_for_rmb_name: u8,
    /// Location type.
    pub location_type: u8,
    /// Block index per cell (64, west→east south→north).
    pub block_index: [u8; BLOCK_GRID_CELLS],
    /// Block number per cell.
    pub block_number: [u8; BLOCK_GRID_CELLS],
    /// Block character per cell.
    pub block_character: [u8; BLOCK_GRID_CELLS],
}

/// `LocationExterior` (one MapPItem element).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocationExterior {
    /// `LocationRecordElementHeader.LocationId` (`DFU:API/DFLocation.cs:560-600`):
    /// the value MAPDITEM matches against its offset table
    /// (`DFU:API/MapsFile.cs:1287-1296`).
    pub location_id: u16,
    /// Display name of the location.
    pub name: String,
    /// Location type (LocationRecordElementHeader).
    pub location_type: u8,
    /// Header BuildingCount (0x7fff max.).
    pub building_count: u16,
    /// Building list of the complete location.
    pub buildings: Vec<BuildingData>,
    /// Exterior data (block grid).
    pub exterior: ExteriorData,
}

impl LocationExterior {
    /// Number of block cells in use (`width * height`).
    pub fn cell_count(&self) -> usize {
        self.exterior.width as usize * self.exterior.height as usize
    }
}

/// Parses a complete MapPItem record (`count` = LocationCount of the region).
pub fn parse_pitem(record: &[u8], count: usize) -> Result<Vec<LocationExterior>, MapsError> {
    let list_bytes = count
        .checked_mul(4)
        .ok_or(MapsError::LocationCountTooLarge { count: count as u32 })?;
    if record.len() < list_bytes {
        return Err(MapsError::TooShort {
            len: record.len(),
            min: list_bytes,
        });
    }
    let mut out = Vec::with_capacity(count);
    for i in 0..count {
        let offset = u32_le(record, i * 4) as usize;
        let base = list_bytes
            .checked_add(offset)
            .ok_or(MapsError::OffsetOutOfBounds {
                index: i,
                offset: offset as u32,
                len: record.len(),
            })?;
        out.push(parse_location_exterior(record, i, base)?);
    }
    Ok(out)
}

fn parse_location_exterior(
    record: &[u8],
    index: usize,
    mut p: usize,
) -> Result<LocationExterior, MapsError> {
    let trunc = |what, offset| MapsError::Truncated { what, index, offset };
    // DoorHeader + Doors.
    if p + 4 > record.len() {
        return Err(trunc("door_header", p));
    }
    let door_count = u32_le(record, p) as usize;
    p += 4 + door_count
        .checked_mul(6)
        .ok_or(MapsError::LocationCountTooLarge { count: door_count as u32 })?;
    // ObjectHeader (71) + LocationRecordElementHeader (48).
    if p + 71 + 48 > record.len() {
        return Err(trunc("location_header", p));
    }
    // The 112-byte `LocationRecordElementHeader` starts at `p`: the "71-byte
    // ObjectHeader" of the UESP layout is its first half, so `LocationId` is
    // `p+33` and the 48-byte slice below is `Name`..`ExteriorData`
    // (`DFU:API/DFLocation.cs:560-600` — UInt16 LocationId, UInt32
    // ExteriorLocationId, Byte[32] LocationName).
    if p + 71 > record.len() {
        return Err(trunc("record_header", p));
    }
    let location_id = u16_le(record, p + 33);
    let header = &record[p + 71..p + 71 + 48];
    let name = c_string_32(&header[0..LOCATION_NAME_LEN]);
    let location_type = header[34];
    let building_count = u16_le(header, 41);
    p += 71 + 48;
    // BuildingData × building_count.
    let buildings_end = (building_count as usize)
        .checked_mul(26)
        .and_then(|n| p.checked_add(n))
        .ok_or(MapsError::LocationCountTooLarge {
            count: building_count as u32,
        })?;
    if buildings_end > record.len() {
        return Err(trunc("buildings", p));
    }
    let mut buildings = Vec::with_capacity(building_count as usize);
    for b in 0..building_count as usize {
        buildings.push(parse_building_data(record, p + b * 26));
    }
    p = buildings_end;
    // ExteriorData (416 B).
    if p + 416 > record.len() {
        return Err(trunc("exterior_data", p));
    }
    let ext = &record[p..p + 416];
    let exterior = ExteriorData {
        map_id: i32_le(ext, 32),
        width: ext[40],
        height: ext[41],
        region_index: ext[45],
        letter1_for_rmb_name: ext[46],
        location_type: ext[48],
        block_index: ext[49..113].try_into().expect("64-byte slice"),
        block_number: ext[113..177].try_into().expect("64-byte slice"),
        block_character: ext[177..241].try_into().expect("64-byte slice"),
    };
    Ok(LocationExterior {
        location_id,
        name,
        location_type,
        building_count,
        buildings,
        exterior,
    })
}

/// Components of an `.RMB` name: prefix + numbers (the resolved letter
/// depends on the location's `ExteriorData`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RmbBlockNameParts {
    /// 4-character prefix (table by BlockIndex).
    pub prefix: String,
    /// Trailing numeric portion of the name (without `.RMB`).
    pub numbers: String,
}

/// Computes the `.RMB` name of a block cell
/// (`DFU:MapsFile.cs:855-893`, `ResolveRmbBlockName`; UESP Block Record
/// Indexes for the prefix table).
pub fn rmb_block_name(ext: &ExteriorData, cell: usize) -> String {
    let parts = rmb_block_name_parts(ext, cell);
    let block_character = ext.block_character[cell];
    let letter1: char = if block_character & 0x10 != 0 {
        char::from_u32(ext.letter1_for_rmb_name as u32).unwrap_or('A')
    } else {
        'A'
    };
    // letter2Array[(byte)(2 * blockCharacter) >> 6] — DFU's byte cast
    // wraps for blockCharacter ≥ 128 (faithful replication).
    let l2 = (((2u16 * block_character as u16) & 0xff) >> 6) as usize;
    format!("{}{}{}{}.RMB", parts.prefix, letter1, LETTER2_ARRAY[l2], parts.numbers)
}

/// Prefix + numbers of cell `cell` (shared with the UESP fallback).
pub fn rmb_block_name_parts(ext: &ExteriorData, cell: usize) -> RmbBlockNameParts {
    let block_index = ext.block_index[cell];
    let block_number = ext.block_number[cell];
    let block_character = ext.block_character[cell];
    let prefix = RMB_BLOCK_PREFIXES
        .get(block_index as usize)
        .copied()
        .unwrap_or("FILL")
        .to_string();
    let numbers = if block_index == 13 || block_index == 14 {
        // Temple logic (`DFU:MapsFile.cs:885-891`): (bc & 0xF) + 65 → 'A'..'P'
        // + blockNumber in decimal without padding.
        format!("{}{}", ((block_character & 0x0f) + 65) as char, block_number)
    } else {
        format!("{block_number:02}")
    };
    RmbBlockNameParts { prefix, numbers }
}

/// UESP "Reconciling Missing Records" fallback: if the exact name does not
/// exist, the RMB records with the same Prefix+Number are listed, sorted
/// alphabetically and the first one is returned.
pub fn rmb_block_name_fallback<'a>(
    parts: &RmbBlockNameParts,
    available: impl Iterator<Item = &'a str>,
) -> Option<String> {
    let suffix = format!("{}.RMB", parts.numbers);
    let mut candidates: Vec<String> = available
        .filter(|name| name.starts_with(&parts.prefix) && name.ends_with(&suffix))
        .map(str::to_string)
        .collect();
    candidates.sort();
    candidates.into_iter().next()
}

/// `.RMB` names of all the location's cells in grid order
/// (index = y·width + x; west→east, south→north — UESP MAPS.BSA §ExteriorData).
pub fn rmb_block_names(ext: &ExteriorData) -> Vec<String> {
    let cells = ext.width as usize * ext.height as usize;
    (0..cells.min(BLOCK_GRID_CELLS)).map(|c| rmb_block_name(ext, c)).collect()
}

// ---------------------------------------------------------------------------
// MAPDITEM — dungeon layout (M34-D / G-N1), replicated from the pinned
// `DFU:API/MapsFile.cs:1275-1343` (`ReadMapDItem`).
// ---------------------------------------------------------------------------

/// First letter of a dungeon `.RDB` name by `BlockIndex`
/// (`DFU:API/MapsFile.cs:110`, `rdbBlockLetters`).
const RDB_BLOCK_LETTERS: [u8; 6] = *b"NWLSBM";

/// `LocationId` that triggers the Orsinium block fix
/// (`DFU:API/MapsFile.cs:1339-1341`).
const ORSINIUM_LOCATION_ID: u16 = 50015;

/// `LocationRecordElementHeader`: `UInt16 LocationId` @33 ·
/// `UInt32 ExteriorLocationId` @41 · `Byte[32] LocationName` @71
/// (`DFU:API/DFLocation.cs:560-600`).
const RECORD_HEADER_LEN: usize = 112;
/// `DungeonHeader`: `UInt16 NullValue1` + `UInt32 Unknown1` +
/// `UInt32 Unknown2` + `UInt16 BlockCount` + `Byte[5] Unknown3`
/// (`DFU:API/DFLocation.cs:637-654`).
const DUNGEON_HEADER_LEN: usize = 17;
/// Max `DungeonCount` accepted (anti-allocation defence, same bound as
/// `LocationCount`).
const MAX_DUNGEON_COUNT: u32 = 4096;

/// `LocationRecordElement` of a `LocationDungeon` (subset used by the grid).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocationRecordElement {
    /// `Header.LocationId` (the value the Orsinium fix compares).
    pub location_id: u16,
    /// `Header.ExteriorLocationId`.
    pub exterior_location_id: u32,
    /// `Header.LocationName` (32 B, 7-bit ASCII, NUL padded).
    pub location_name: String,
}

/// `DungeonHeader` (`DFU:API/DFLocation.cs:637-654`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DungeonHeader {
    /// `NullValue1` (always 0).
    pub null_value1: u16,
    /// `Unknown1` (preserved, not interpreted).
    pub unknown1: u32,
    /// `Unknown2` (preserved, not interpreted).
    pub unknown2: u32,
    /// Count of `DungeonBlock` elements.
    pub block_count: u16,
}

/// One `DungeonBlock` (`DFU:API/DFLocation.cs:659-681`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DungeonBlock {
    /// X position of the block (grid cells, may be negative).
    pub x: i8,
    /// Z position of the block (grid cells, may be negative).
    pub z: i8,
    /// `BlockNumber` (`bitfield & 0x3ff`).
    pub number: u16,
    /// `IsStartingBlock` (`(bitfield & 0x400) != 0`).
    pub is_starting_block: bool,
    /// `BlockIndex` (`bitfield >> 11`, 0..=5).
    pub block_index: u8,
    /// `BlockName`: `{letter}{number:07}.RDB` (`MapsFile.cs:1331-1334`).
    pub name: String,
}

/// `LocationDungeon` (`DFU:API/DFLocation.cs:617-630`).
///
/// `DungeonBlock.WaterLevel`/`CastleBlock` are NOT parsed: DFU fills them at
/// load time from the starting block's flat billboard
/// (`DaggerfallDungeon.cs:412-434`), not from MAPDITEM.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocationDungeon {
    /// Record header (the Orsinium key lives here).
    pub record: LocationRecordElement,
    /// Dungeon layout header.
    pub header: DungeonHeader,
    /// Blocks in file order (index = the DFU `Blocks[i]` order).
    pub blocks: Vec<DungeonBlock>,
}

/// Parses a complete MAPDITEM record and returns the dungeon whose offset
/// table entry has `ExteriorLocationId == location_id`
/// (`DFU:API/MapsFile.cs:1275-1343`, `ReadMapDItem`).
///
/// `Ok(None)` = the record has no dungeon for that location (`HasDungeon =
/// false` in DFU: empty record, `DungeonCount == 0` or no matching entry).
/// `DungeonCount` comes from the record itself, so `location_count` is not a
/// parameter (DFU reads it the same way; `MAPNAMES` count is only used for
/// MAPPITEM offsets).
pub fn parse_ditem(record: &[u8], location_id: u16) -> Result<Option<LocationDungeon>, MapsError> {
    if record.len() < 4 {
        return Err(MapsError::TooShort { len: record.len(), min: 4 });
    }
    let dungeon_count = u32_le(record, 0);
    if dungeon_count > MAX_DUNGEON_COUNT {
        return Err(MapsError::LocationCountTooLarge { count: dungeon_count });
    }
    let dungeon_count = dungeon_count as usize;
    let table_end = 4 + dungeon_count * 8;
    if record.len() < table_end {
        return Err(MapsError::Truncated {
            what: "dungeon_offsets",
            index: 0,
            offset: 4,
        });
    }
    // The u16 at `at+4` is `IsDungeon`: preserved in the file but not
    // consulted by DFU's lookup (`MapsFile.cs:1287-1296`).
    let mut found: Option<(usize, usize)> = None;
    for i in 0..dungeon_count {
        let at = 4 + i * 8;
        if u16_le(record, at + 6) == location_id {
            found = Some((i, u32_le(record, at) as usize));
            break;
        }
    }
    let Some((index, offset)) = found else {
        return Ok(None);
    };

    // Body position: `4 + DungeonCount·8 + Offset`.
    let base = table_end
        .checked_add(offset)
        .ok_or(MapsError::OffsetOutOfBounds {
            index,
            offset: offset as u32,
            len: record.len(),
        })?;
    let trunc = |what, offset| MapsError::Truncated { what, index, offset };
    if base + 4 > record.len() {
        return Err(trunc("dungeon_record", base));
    }
    let door_count = u32_le(record, base) as usize;
    let header_at = base
        + 4
        + door_count
            .checked_mul(6)
            .ok_or(MapsError::LocationCountTooLarge {
                count: door_count as u32,
            })?;
    if header_at + RECORD_HEADER_LEN > record.len() {
        return Err(trunc("dungeon_record_header", header_at));
    }
    let record_element = LocationRecordElement {
        location_id: u16_le(record, header_at + 33),
        exterior_location_id: u32_le(record, header_at + 41),
        location_name: c_string_32(
            &record[header_at + 71..header_at + 71 + LOCATION_NAME_LEN],
        ),
    };

    let h = header_at + RECORD_HEADER_LEN;
    if h + DUNGEON_HEADER_LEN > record.len() {
        return Err(trunc("dungeon_header", h));
    }
    let header = DungeonHeader {
        null_value1: u16_le(record, h),
        unknown1: u32_le(record, h + 2),
        unknown2: u32_le(record, h + 6),
        block_count: u16_le(record, h + 10),
    };

    let blocks_at = h + DUNGEON_HEADER_LEN;
    let blocks_end = blocks_at + 4 * header.block_count as usize;
    if blocks_end > record.len() {
        return Err(trunc("dungeon_blocks", blocks_at));
    }
    let mut blocks = Vec::with_capacity(header.block_count as usize);
    for b in 0..header.block_count as usize {
        let at = blocks_at + 4 * b;
        let bitfield = u16_le(record, at + 2);
        let block_index = (bitfield >> 11) as u8;
        let letter = RDB_BLOCK_LETTERS
            .get(block_index as usize)
            .copied()
            .ok_or(MapsError::InvalidBlockIndex { index: b, block_index })?;
        let number = bitfield & 0x03ff;
        blocks.push(DungeonBlock {
            x: record[at] as i8,
            z: record[at + 1] as i8,
            number,
            is_starting_block: (bitfield & 0x400) != 0,
            block_index,
            name: format!("{}{:07}.RDB", letter as char, number),
        });
    }

    // Orsinium hack, reversed from classic (`MapsFile.cs:1339-1341`):
    // the border block overlaps another one. DFU writes `Blocks[13]`
    // unconditionally; the `get_mut` guard only avoids a panic on a
    // malformed record (retail has 16 blocks).
    if record_element.location_id == ORSINIUM_LOCATION_ID {
        if let Some(block) = blocks.get_mut(13) {
            block.z = -2;
        }
    }

    Ok(Some(LocationDungeon {
        record: record_element,
        header,
        blocks,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Synthetic fixture: MapNAMES with 2 locations.
    fn names_record() -> Vec<u8> {
        let mut rec = Vec::new();
        rec.extend_from_slice(&2u32.to_le_bytes());
        for name in ["Alpha", "Beta"] {
            let mut slot = [0u8; 32];
            slot[..name.len()].copy_from_slice(name.as_bytes());
            rec.extend_from_slice(&slot);
        }
        rec
    }

    #[test]
    fn map_names_fixture() {
        let names = parse_map_names(&names_record()).unwrap();
        assert_eq!(names, vec!["Alpha".to_string(), "Beta".to_string()]);
    }

    #[test]
    fn map_names_count_too_large() {
        let mut rec = 5000u32.to_le_bytes().to_vec();
        rec.extend_from_slice(&[0u8; 8]);
        assert!(matches!(
            parse_map_names(&rec),
            Err(MapsError::LocationCountTooLarge { count: 5000 })
        ));
    }

    #[test]
    fn map_table_derived_fields() {
        // location_type 0 (TownCity), width 3, height 2 (4096 units).
        let latitude_type = 0u32 << 25;
        let longitude_type = (3u32 << 24) | (2u32 << 28);
        let mut rec = Vec::new();
        rec.extend_from_slice(&7i32.to_le_bytes());
        rec.extend_from_slice(&latitude_type.to_le_bytes());
        rec.extend_from_slice(&longitude_type.to_le_bytes());
        rec.push(0);
        rec.extend_from_slice(&0u32.to_le_bytes());
        let table = parse_map_table(&rec, 1).unwrap();
        assert_eq!(table[0].map_id, 7);
        assert_eq!(table[0].location_type(), 0);
        assert_eq!(table[0].map_width(), 3);
        assert_eq!(table[0].map_height(), 2);
    }

    /// Synthetic MapPItem fixture: 1 location, 2 buildings, 1 block cell.
    fn pitem_record(letter1: u8) -> Vec<u8> {
        let exterior = {
            let mut ext = vec![0u8; 416];
            ext[32..36].copy_from_slice(&213207i32.to_le_bytes()); // MapId
            ext[40] = 1; // Width
            ext[41] = 1; // Height
            ext[45] = 17; // RegionIndex
            ext[46] = letter1; // Letter1ForRMBName
            ext[48] = 0; // LocationType
            ext[49] = 0; // BlockIndex[0] = TVRN
            ext[113] = 0; // BlockNumber[0]
            ext[177] = 0; // BlockCharacter[0]
            ext
        };
        let mut buildings = Vec::new();
        for (seed, quality) in [(42u16, 7u8), (43, 9)] {
            let mut bd = vec![0u8; 26];
            bd[0..2].copy_from_slice(&seed.to_le_bytes());
            bd[18..20].copy_from_slice(&3u16.to_le_bytes()); // faction
            bd[20..22].copy_from_slice(&0i16.to_le_bytes()); // sector
            bd[22..24].copy_from_slice(&99u16.to_le_bytes()); // location id
            bd[24] = 15; // Tavern
            bd[25] = quality;
            buildings.extend_from_slice(&bd);
        }
        let mut le = Vec::new();
        le.extend_from_slice(&0u32.to_le_bytes()); // DoorCount
        // First half of the 112-byte `LocationRecordElementHeader`.
        let mut oh = [0u8; 71];
        oh[33..35].copy_from_slice(&26219u16.to_le_bytes()); // LocationId
        le.extend_from_slice(&oh);
        let mut lh = vec![0u8; 48];
        lh[..5].copy_from_slice(b"Alpha");
        lh[32] = 1; // Width
        lh[33] = 1; // Height
        lh[34] = 0; // LocationType
        lh[41..43].copy_from_slice(&2u16.to_le_bytes()); // BuildingCount
        le.extend_from_slice(&lh);
        le.extend_from_slice(&buildings);
        le.extend_from_slice(&exterior);
        let mut rec = Vec::new();
        rec.extend_from_slice(&0u32.to_le_bytes()); // offset[0] = 0
        rec.extend_from_slice(&le);
        rec
    }

    #[test]
    fn pitem_fixture_parse() {
        let locs = parse_pitem(&pitem_record(b'B'), 1).unwrap();
        assert_eq!(locs.len(), 1);
        let loc = &locs[0];
        assert_eq!(loc.name, "Alpha");
        assert_eq!(loc.location_id, 26219);
        assert_eq!(loc.building_count, 2);
        assert_eq!(loc.buildings[0].name_seed, 42);
        assert_eq!(loc.buildings[0].faction_id, 3);
        assert_eq!(loc.buildings[0].building_type, 15);
        assert_eq!(loc.buildings[1].quality, 9);
        assert_eq!(loc.exterior.map_id, 213207);
        assert_eq!(loc.exterior.width, 1);
        assert_eq!(loc.exterior.region_index, 17);
        assert_eq!(loc.cell_count(), 1);
    }

    #[test]
    fn rmb_name_default_letter_a() {
        let locs = parse_pitem(&pitem_record(b'B'), 1).unwrap();
        // bc=0: letter1='A' (0x10 bit clear), letter2='A' (bc>>5=0) → TVRNAA00.RMB
        assert_eq!(rmb_block_name(&locs[0].exterior, 0), "TVRNAA00.RMB");
    }

    #[test]
    fn rmb_name_uses_letter1_and_letter2() {
        let locs = parse_pitem(&pitem_record(b'I'), 1).unwrap();
        let mut ext = locs[0].exterior.clone();
        // bc = 0x10 | 0x20 = 0x30: letter1='I' (bit 0x10), letter2=(0x60>>6)='L'.
        ext.block_character[0] = 0x30;
        ext.block_number[0] = 10;
        assert_eq!(rmb_block_name(&ext, 0), "TVRNIL10.RMB");
    }

    #[test]
    fn rmb_name_temple_logic() {
        let locs = parse_pitem(&pitem_record(b'A'), 1).unwrap();
        let mut ext = locs[0].exterior.clone();
        ext.block_index[0] = 13; // TEMP
        ext.block_character[0] = 0x03; // < 0x10 → 'A'; &7=3 → 'D'; temple &0xF=3 → 'D'
        ext.block_number[0] = 0;
        // Temple: letter1='A', letter2 from the high bits ('A'), numbers='D' + "0".
        assert_eq!(rmb_block_name(&ext, 0), "TEMPAAD0.RMB");
    }

    #[test]
    fn rmb_name_fallback_sorted_first() {
        let parts = RmbBlockNameParts {
            prefix: "BOOK".to_string(),
            numbers: "02".to_string(),
        };
        let available = ["BOOKBA02.RMB", "BOOKAA05.RMB", "BOOKAA02.RMB"];
        assert_eq!(
            rmb_block_name_fallback(&parts, available.into_iter()),
            Some("BOOKAA02.RMB".to_string())
        );
    }

    #[test]
    fn building_type_names() {
        assert_eq!(building_type_name(15), "Tavern");
        assert_eq!(building_type_name(0), "Alchemist");
        assert_eq!(building_type_name(0x74), "Special1");
        assert_eq!(building_type_name(200), "Unknown");
    }

    /// Synthetic MAPDITEM fixture: one dungeon entry pointing at the body
    /// (`MapsFile.cs:1275-1343`). `blocks` = `(X, Z, BlockNumberStartIndexBitfield)`.
    fn ditem_record(
        location_id: u16,
        exterior_location_id: u16,
        blocks: &[(i8, i8, u16)],
    ) -> Vec<u8> {
        let table_end = 4 + 8;
        let mut body = Vec::new();
        body.extend_from_slice(&0u32.to_le_bytes()); // LocationRecordElement.DoorCount
        let mut rh = vec![0u8; 112];
        rh[33..35].copy_from_slice(&location_id.to_le_bytes());
        rh[41..45].copy_from_slice(&u32::from(exterior_location_id).to_le_bytes());
        rh[71..76].copy_from_slice(b"Dunge");
        body.extend_from_slice(&rh);
        let mut dh = vec![0u8; 17];
        dh[10..12].copy_from_slice(&(blocks.len() as u16).to_le_bytes());
        body.extend_from_slice(&dh);
        for &(x, z, bitfield) in blocks {
            body.push(x as u8);
            body.push(z as u8);
            body.extend_from_slice(&bitfield.to_le_bytes());
        }
        let mut rec = Vec::new();
        rec.extend_from_slice(&1u32.to_le_bytes()); // DungeonCount
        rec.extend_from_slice(&0u32.to_le_bytes()); // Offset
        rec.extend_from_slice(&1u16.to_le_bytes()); // IsDungeon
        rec.extend_from_slice(&exterior_location_id.to_le_bytes());
        assert_eq!(rec.len(), table_end);
        rec.extend_from_slice(&body);
        rec
    }

    #[test]
    fn ditem_bitfield_decomposition_and_name() {
        // N0000001 (index 0 + start bit 0x400) · W0000010 (index 1) ·
        // M0000003 (index 5, Z = −3).
        let rec = ditem_record(
            26219,
            26219,
            &[(0, 0, 0x0401), (-1, 2, 0x080a), (1, -3, 0x2803)],
        );
        let dungeon = parse_ditem(&rec, 26219).unwrap().expect("dungeon");
        assert_eq!(dungeon.record.location_id, 26219);
        assert_eq!(dungeon.record.exterior_location_id, 26219);
        assert_eq!(dungeon.record.location_name, "Dunge");
        assert_eq!(dungeon.header.block_count, 3);
        assert_eq!(dungeon.blocks[0].name, "N0000001.RDB");
        assert!(dungeon.blocks[0].is_starting_block);
        assert_eq!((dungeon.blocks[1].x, dungeon.blocks[1].z), (-1, 2));
        assert_eq!(dungeon.blocks[1].name, "W0000010.RDB");
        assert_eq!(dungeon.blocks[2].name, "M0000003.RDB");
        assert_eq!(dungeon.blocks[2].z, -3);
        assert_eq!(dungeon.blocks[2].block_index, 5);
    }

    #[test]
    fn ditem_no_matching_location_is_none() {
        let rec = ditem_record(26219, 26219, &[(0, 0, 0)]);
        assert_eq!(parse_ditem(&rec, 9999).unwrap(), None);
        // Empty MAPDITEM record → DFU `HasDungeon = false`.
        assert_eq!(parse_ditem(&0u32.to_le_bytes(), 1).unwrap(), None);
        assert!(matches!(
            parse_ditem(&[], 1),
            Err(MapsError::TooShort { len: 0, min: 4 })
        ));
    }

    #[test]
    fn ditem_truncated_body_is_error() {
        let mut rec = ditem_record(26219, 26219, &[(0, 0, 0)]);
        rec.truncate(rec.len() - 2);
        assert!(matches!(
            parse_ditem(&rec, 26219),
            Err(MapsError::Truncated {
                what: "dungeon_blocks",
                ..
            })
        ));
    }

    #[test]
    fn ditem_offset_past_end_is_error() {
        let mut rec = ditem_record(26219, 26219, &[(0, 0, 0)]);
        rec[4..8].copy_from_slice(&0xffff_ffffu32.to_le_bytes());
        assert!(matches!(
            parse_ditem(&rec, 26219),
            Err(MapsError::Truncated { .. })
        ));
    }

    #[test]
    fn ditem_invalid_block_index_is_error() {
        let rec = ditem_record(26219, 26219, &[(0, 0, 6 << 11)]);
        assert!(matches!(
            parse_ditem(&rec, 26219),
            Err(MapsError::InvalidBlockIndex { block_index: 6, .. })
        ));
    }

    #[test]
    fn ditem_orsinium_block_13_moves_to_z_minus_2() {
        let mut blocks = vec![(0i8, 0i8, 0u16); 16];
        blocks[13] = (-1, -1, 8195);
        // Header LocationId 50015 triggers the fix; the table entry matches
        // the *exterior* LocationId (retail: 50014 → 50015).
        let rec = ditem_record(50015, 50014, &blocks);
        let dungeon = parse_ditem(&rec, 50014).unwrap().expect("dungeon");
        assert_eq!(dungeon.blocks[13].z, -2);
        assert_eq!(dungeon.blocks[13].name, "B0000003.RDB");
        // The same layout without the Orsinium key keeps the raw Z (−1).
        let rec = ditem_record(50014, 50014, &blocks);
        assert_eq!(
            parse_ditem(&rec, 50014).unwrap().unwrap().blocks[13].z,
            -1
        );
    }
}
