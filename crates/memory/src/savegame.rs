//! `savegame` — the classic SAVEGAME container formats M33 can read and write
//! on its own terms: `SAVETREE.DAT` and `MAPSAVE.SAV` (spec
//! `m33-total-persistence-savegame-spec-CR-5243af63.md` §2.2-§2.3, §6.4).
//!
//! ## What is derived from the source and what is not
//!
//! | Artifact | Layout | Source |
//! |:---|:---|:---|
//! | `SAVETREE.DAT` header | 19 B: `i32 version` (`0x126`) + `i32 x,y,z` + `u16 mapId` + `u8 environment` | `SaveTreeHeader.cs:24-25,59-81` |
//! | buildings block | `i32 recordLength` + `recordLength` B; `N = recordLength / 26` | `SaveTreeBuildingRecords.cs:58,62,70-82` |
//! | records | `[i32 length][length B]`, first byte = type, then the 71 B `RecordRoot` | `SaveTree.cs:212-270`; `SaveTreeBaseRecord.cs:26,220-283` |
//! | `MAPSAVE.SAV` | `i16 count` + `u16 dirType` (`0x0100`), data, then `count` × 18 B directory entries (`name[12]`, `u16`, `u32 length`) | `BsaFile.cs:374-406`; measured on disk |
//!
//! **Declared limit (spec §2.4 N1, §6.4):** the upstream project writes only
//! the `SAVETREE` header (`SaveTree.cs:103` still carries the TODO for the
//! records), so the write half of the format is verified against **our own**
//! reader, never against the original executable. This module therefore makes
//! no claim of compatibility: it is a byte-stable reader/writer of the layout
//! the source describes.
//!
//! The light-record rule of the source (`streamLength *= 39` for type `0x07`,
//! `SaveTreeBaseRecord.cs:172-174`) is **not** implemented: it is a reader
//! expansion of the original's light data, not a framing rule, and our writer
//! never emits type `0x07`. A record of that type decodes with its stored
//! length, and the probe declares the divergence instead of pretending.

use crate::session_store::SaveError;

/// `SAVETREE.DAT` version the classic game writes (`SaveTreeHeader.cs:24`).
pub const SAVETREE_VERSION: i32 = 0x126;
/// Fixed header of `SAVETREE.DAT` (GV-13).
pub const SAVETREE_HEADER_BYTES: usize = 19;
/// Bytes of one `RecordRoot` (GV-12).
pub const RECORD_ROOT_BYTES: usize = 71;
/// Bytes of one building record (GV-14).
pub const BUILDING_BYTES: usize = 26;
/// Bytes of one `MAPSAVE` directory entry (GV-16).
pub const MAPSAVE_DIR_ENTRY_BYTES: usize = 18;
/// Directory type of `MAPSAVE.SAV` (`NameRecord`).
pub const MAPSAVE_DIR_TYPE: u16 = 0x0100;
/// Bit that marks a region byte as discovered (`SaveGames.cs:242`).
pub const MAPSAVE_DISCOVERED_BIT: u8 = 0x40;
/// Longest `MAPSAVE` name the 12-byte field holds.
pub const MAPSAVE_NAME_BYTES: usize = 12;

// ═══════════════════════════════════════════════════════════════════════════
// Byte helpers (little-endian, bounds-checked, 0 panics on hostile input)
// ═══════════════════════════════════════════════════════════════════════════

fn take(bytes: &[u8], at: usize, len: usize) -> Result<&[u8], SaveError> {
    let end = at.checked_add(len).ok_or_else(|| {
        SaveError::Corrupt(format!("offset overflow at {at} (+{len})"))
    })?;
    bytes.get(at..end).ok_or_else(|| {
        SaveError::Corrupt(format!(
            "truncated file: need {len} bytes at offset {at}, have {}",
            bytes.len().saturating_sub(at)
        ))
    })
}

fn read_i16(bytes: &[u8], at: usize) -> Result<i16, SaveError> {
    let slice = take(bytes, at, 2)?;
    Ok(i16::from_le_bytes([slice[0], slice[1]]))
}

fn read_u16(bytes: &[u8], at: usize) -> Result<u16, SaveError> {
    let slice = take(bytes, at, 2)?;
    Ok(u16::from_le_bytes([slice[0], slice[1]]))
}

fn read_u32(bytes: &[u8], at: usize) -> Result<u32, SaveError> {
    let slice = take(bytes, at, 4)?;
    Ok(u32::from_le_bytes([slice[0], slice[1], slice[2], slice[3]]))
}

fn read_i32(bytes: &[u8], at: usize) -> Result<i32, SaveError> {
    let slice = take(bytes, at, 4)?;
    Ok(i32::from_le_bytes([slice[0], slice[1], slice[2], slice[3]]))
}

fn write_i16(out: &mut Vec<u8>, value: i16) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn write_u16(out: &mut Vec<u8>, value: u16) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn write_i32(out: &mut Vec<u8>, value: i32) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn write_u32(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_le_bytes());
}

// ═══════════════════════════════════════════════════════════════════════════
// SAVETREE.DAT
// ═══════════════════════════════════════════════════════════════════════════

/// Header of `SAVETREE.DAT` (19 B).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SaveTreeHeader {
    /// Format version; the classic game writes [`SAVETREE_VERSION`].
    pub version: i32,
    /// Player X in classic world units.
    pub x: i32,
    /// Player Y.
    pub y: i32,
    /// Player Z.
    pub z: i32,
    /// Map id the tree belongs to.
    pub map_id: u16,
    /// 1 exterior, 2 building, 3 dungeon (`SaveTree.Environments`).
    pub environment: u8,
}

impl Default for SaveTreeHeader {
    fn default() -> Self {
        Self {
            version: SAVETREE_VERSION,
            x: 0,
            y: 0,
            z: 0,
            map_id: 0,
            environment: 1,
        }
    }
}

impl SaveTreeHeader {
    /// Encodes the 19 header bytes.
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(SAVETREE_HEADER_BYTES);
        write_i32(&mut out, self.version);
        write_i32(&mut out, self.x);
        write_i32(&mut out, self.y);
        write_i32(&mut out, self.z);
        write_u16(&mut out, self.map_id);
        out.push(self.environment);
        out
    }

    /// Decodes the 19 header bytes.
    pub fn decode(bytes: &[u8]) -> Result<Self, SaveError> {
        Ok(Self {
            version: read_i32(bytes, 0)?,
            x: read_i32(bytes, 4)?,
            y: read_i32(bytes, 8)?,
            z: read_i32(bytes, 12)?,
            map_id: read_u16(bytes, 16)?,
            environment: *take(bytes, 18, 1)?
                .first()
                .ok_or_else(|| SaveError::Corrupt("truncated header".into()))?,
        })
    }
}

/// One building record (26 B), mirroring `SaveTreeBuildingRecords`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct BuildingRecord {
    /// Name seed used to derive the building name.
    pub name_seed: u16,
    /// Service time limit in game minutes.
    pub service_time_limit: u32,
    /// Reserved word 1 (layout-preserving).
    pub reserved_1: u16,
    /// Reserved word 2 (layout-preserving).
    pub reserved_2: u16,
    /// Reserved dword 1 (layout-preserving).
    pub reserved_3: u32,
    /// Reserved dword 2 (layout-preserving).
    pub reserved_4: u32,
    /// Faction that owns the building.
    pub faction_id: u16,
    /// Sector inside the location.
    pub sector: i16,
    /// Location id.
    pub location_id: u16,
    /// Building type (0..8).
    pub building_type: u8,
    /// Building quality.
    pub quality: u8,
}

impl BuildingRecord {
    fn encode(&self, out: &mut Vec<u8>) {
        write_u16(out, self.name_seed);
        write_u32(out, self.service_time_limit);
        write_u16(out, self.reserved_1);
        write_u16(out, self.reserved_2);
        write_u32(out, self.reserved_3);
        write_u32(out, self.reserved_4);
        write_u16(out, self.faction_id);
        write_i16(out, self.sector);
        write_u16(out, self.location_id);
        out.push(self.building_type);
        out.push(self.quality);
    }

    fn decode(bytes: &[u8], at: usize) -> Result<Self, SaveError> {
        Ok(Self {
            name_seed: read_u16(bytes, at)?,
            service_time_limit: read_u32(bytes, at + 2)?,
            reserved_1: read_u16(bytes, at + 6)?,
            reserved_2: read_u16(bytes, at + 8)?,
            reserved_3: read_u32(bytes, at + 10)?,
            reserved_4: read_u32(bytes, at + 14)?,
            faction_id: read_u16(bytes, at + 18)?,
            sector: read_i16(bytes, at + 20)?,
            location_id: read_u16(bytes, at + 22)?,
            building_type: *take(bytes, at + 24, 1)?
                .first()
                .ok_or_else(|| SaveError::Corrupt("truncated building".into()))?,
            quality: *take(bytes, at + 25, 1)?
                .first()
                .ok_or_else(|| SaveError::Corrupt("truncated building".into()))?,
        })
    }
}

/// The 71-byte `RecordRoot` (`SaveTreeBaseRecord.cs:220-283`).
///
/// The two gaps of the layout (offsets 19-27 and 35-38) are preserved as
/// reserved bytes so any input re-encodes byte for byte.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RecordRoot {
    /// Record type (`SaveTree.RecordTypes`: 0x02 item, 0x03 character, …).
    pub record_type: u8,
    /// Pitch (i16).
    pub pitch: i16,
    /// Yaw (i16).
    pub yaw: i16,
    /// Roll (i16).
    pub roll: i16,
    /// X position.
    pub x: i32,
    /// Y position.
    pub y: i32,
    /// Z position.
    pub z: i32,
    /// Bytes at offsets 19-26 (not named by the derived layout).
    pub reserved_19_27: [u8; 8],
    /// Sprite index.
    pub sprite_index: u16,
    /// Picture 2 (compass/none).
    pub picture2: u16,
    /// Record id inside the tree.
    pub record_id: u32,
    /// Bytes at offsets 35-37 (not named by the derived layout).
    pub reserved_35_38: [u8; 3],
    /// Quest id the record belongs to.
    pub quest_id: u8,
    /// Parent record id.
    pub parent_id: u32,
    /// Time value (game minutes).
    pub time: u32,
    /// Item object id.
    pub item_object: u32,
    /// Quest object id.
    pub quest_object_id: u32,
    /// Next sibling.
    pub next: u32,
    /// First child.
    pub child: u32,
    /// Sublist head.
    pub sublist_head: u32,
    /// Parent type.
    pub parent_type: i32,
}

impl RecordRoot {
    /// Encodes the 71 bytes.
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(RECORD_ROOT_BYTES);
        out.push(self.record_type);
        write_i16(&mut out, self.pitch);
        write_i16(&mut out, self.yaw);
        write_i16(&mut out, self.roll);
        write_i32(&mut out, self.x);
        write_i32(&mut out, self.y);
        write_i32(&mut out, self.z);
        out.extend_from_slice(&self.reserved_19_27);
        write_u16(&mut out, self.sprite_index);
        write_u16(&mut out, self.picture2);
        write_u32(&mut out, self.record_id);
        out.extend_from_slice(&self.reserved_35_38);
        out.push(self.quest_id);
        write_u32(&mut out, self.parent_id);
        write_u32(&mut out, self.time);
        write_u32(&mut out, self.item_object);
        write_u32(&mut out, self.quest_object_id);
        write_u32(&mut out, self.next);
        write_u32(&mut out, self.child);
        write_u32(&mut out, self.sublist_head);
        write_i32(&mut out, self.parent_type);
        out
    }

    /// Decodes the 71 bytes.
    pub fn decode(bytes: &[u8]) -> Result<Self, SaveError> {
        let mut reserved_19_27 = [0u8; 8];
        reserved_19_27.copy_from_slice(take(bytes, 19, 8)?);
        let mut reserved_35_38 = [0u8; 3];
        reserved_35_38.copy_from_slice(take(bytes, 35, 3)?);
        Ok(Self {
            record_type: *take(bytes, 0, 1)?
                .first()
                .ok_or_else(|| SaveError::Corrupt("truncated record root".into()))?,
            pitch: read_i16(bytes, 1)?,
            yaw: read_i16(bytes, 3)?,
            roll: read_i16(bytes, 5)?,
            x: read_i32(bytes, 7)?,
            y: read_i32(bytes, 11)?,
            z: read_i32(bytes, 15)?,
            reserved_19_27,
            sprite_index: read_u16(bytes, 27)?,
            picture2: read_u16(bytes, 29)?,
            record_id: read_u32(bytes, 31)?,
            reserved_35_38,
            quest_id: *take(bytes, 38, 1)?
                .first()
                .ok_or_else(|| SaveError::Corrupt("truncated record root".into()))?,
            parent_id: read_u32(bytes, 39)?,
            time: read_u32(bytes, 43)?,
            item_object: read_u32(bytes, 47)?,
            quest_object_id: read_u32(bytes, 51)?,
            next: read_u32(bytes, 55)?,
            child: read_u32(bytes, 59)?,
            sublist_head: read_u32(bytes, 63)?,
            parent_type: read_i32(bytes, 67)?,
        })
    }
}

/// One framed record: `[i32 length][RecordRoot 71][data…]`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SaveTreeRecord {
    /// The 71-byte root (the type is `root.record_type`).
    pub root: RecordRoot,
    /// Payload after the root (sub-record fields of the classic layout).
    pub data: Vec<u8>,
}

impl SaveTreeRecord {
    /// Bytes the record occupies on disk, framing included.
    pub fn framed_len(&self) -> usize {
        4 + RECORD_ROOT_BYTES + self.data.len()
    }
}

/// A whole `SAVETREE.DAT` as far as the source derives it.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SaveTree {
    /// The 19-byte header.
    pub header: SaveTreeHeader,
    /// Building records (`recordLength / 26` of them).
    pub buildings: Vec<BuildingRecord>,
    /// Framed records in file order.
    pub records: Vec<SaveTreeRecord>,
}

impl SaveTree {
    /// Encodes header + buildings block + framed records.
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(
            SAVETREE_HEADER_BYTES
                + 4
                + self.buildings.len() * BUILDING_BYTES
                + self.records.iter().map(SaveTreeRecord::framed_len).sum::<usize>(),
        );
        out.extend_from_slice(&self.header.encode());
        let record_length = (self.buildings.len() * BUILDING_BYTES) as i32;
        write_i32(&mut out, record_length);
        for building in &self.buildings {
            building.encode(&mut out);
        }
        for record in &self.records {
            let length = (RECORD_ROOT_BYTES + record.data.len()) as i32;
            write_i32(&mut out, length);
            out.extend_from_slice(&record.root.encode());
            out.extend_from_slice(&record.data);
        }
        out
    }

    /// Decodes a `SAVETREE.DAT`.
    ///
    /// The record guards mirror the source (`SaveTree.cs:219-225`): a length
    /// `<= 0` is skipped and a record that would run past the end of the file
    /// stops the scan instead of reading out of bounds.
    pub fn decode(bytes: &[u8]) -> Result<Self, SaveError> {
        let header = SaveTreeHeader::decode(bytes)?;
        let record_length = read_i32(bytes, SAVETREE_HEADER_BYTES)?;
        if record_length < 0 {
            return Err(SaveError::Corrupt(format!(
                "negative building record length {record_length}"
            )));
        }
        let record_length = record_length as usize;
        let buildings_start = SAVETREE_HEADER_BYTES + 4;
        let buildings_bytes = take(bytes, buildings_start, record_length)?;
        if !record_length.is_multiple_of(BUILDING_BYTES) {
            return Err(SaveError::Corrupt(format!(
                "building block of {record_length} B is not a multiple of {BUILDING_BYTES}"
            )));
        }
        let mut buildings = Vec::with_capacity(record_length / BUILDING_BYTES);
        let mut at = 0usize;
        while at + BUILDING_BYTES <= record_length {
            buildings.push(BuildingRecord::decode(buildings_bytes, at)?);
            at += BUILDING_BYTES;
        }

        let mut records = Vec::new();
        let mut pos = buildings_start + record_length;
        while pos + 4 <= bytes.len() {
            let length = read_i32(bytes, pos)?;
            if length <= 0 {
                pos += 4;
                continue;
            }
            let length = length as usize;
            let start = pos + 4;
            if start + length > bytes.len() {
                break;
            }
            if length < RECORD_ROOT_BYTES {
                // A short block cannot hold a root: the source never reads one.
                break;
            }
            let block = take(bytes, start, length)?;
            records.push(SaveTreeRecord {
                root: RecordRoot::decode(block)?,
                data: block[RECORD_ROOT_BYTES..].to_vec(),
            });
            pos = start + length;
        }

        Ok(Self {
            header,
            buildings,
            records,
        })
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// MAPSAVE.SAV
// ═══════════════════════════════════════════════════════════════════════════

/// One `MAPSAVE` region entry: 12-byte name, reserved word and data.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MapSaveEntry {
    /// Name as stored (`MAPSAVE.000`…, NUL padded).
    pub name: [u8; MAPSAVE_NAME_BYTES],
    /// Reserved word of the directory entry (layout-preserving; the retail
    /// file stores 0 in all 62 entries).
    pub reserved: u16,
    /// One byte per location of the region, bit [`MAPSAVE_DISCOVERED_BIT`] set
    /// when discovered.
    pub data: Vec<u8>,
}

impl MapSaveEntry {
    /// Builds an entry from its name and data (`reserved` = 0).
    pub fn new(name: &str, data: Vec<u8>) -> Self {
        let mut field = [0u8; MAPSAVE_NAME_BYTES];
        for (slot, byte) in field.iter_mut().zip(name.bytes()) {
            *slot = byte;
        }
        Self {
            name: field,
            reserved: 0,
            data,
        }
    }

    /// Name without the NUL padding.
    pub fn name_str(&self) -> String {
        let end = self
            .name
            .iter()
            .position(|byte| *byte == 0)
            .unwrap_or(MAPSAVE_NAME_BYTES);
        String::from_utf8_lossy(&self.name[..end]).into_owned()
    }

    /// Discovered locations in this region.
    pub fn discovered(&self) -> usize {
        self.data
            .iter()
            .filter(|byte| *byte & MAPSAVE_DISCOVERED_BIT != 0)
            .count()
    }
}

/// The whole `MAPSAVE.SAV` (`BSA` `NameRecord` with `dirType = 0x0100`).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MapSave {
    /// Directory type (the retail file stores [`MAPSAVE_DIR_TYPE`]).
    pub dir_type: u16,
    /// One entry per region, in file order.
    pub entries: Vec<MapSaveEntry>,
}

impl MapSave {
    /// Encodes `count` + `dirType` + concatenated data + 18-byte directory.
    pub fn encode(&self) -> Vec<u8> {
        let data_len: usize = self.entries.iter().map(|entry| entry.data.len()).sum();
        let mut out = Vec::with_capacity(
            4 + data_len + self.entries.len() * MAPSAVE_DIR_ENTRY_BYTES,
        );
        write_i16(&mut out, self.entries.len() as i16);
        write_u16(&mut out, self.dir_type);
        for entry in &self.entries {
            out.extend_from_slice(&entry.data);
        }
        for entry in &self.entries {
            out.extend_from_slice(&entry.name);
            write_u16(&mut out, entry.reserved);
            write_u32(&mut out, entry.data.len() as u32);
        }
        out
    }

    /// Decodes a `MAPSAVE.SAV`, verifying that the directory lengths add up to
    /// the data region (`4 + data + count·18` = file length).
    pub fn decode(bytes: &[u8]) -> Result<Self, SaveError> {
        let count = read_i16(bytes, 0)?;
        if count < 0 {
            return Err(SaveError::Corrupt(format!("negative record count {count}")));
        }
        let count = count as usize;
        let dir_type = read_u16(bytes, 2)?;
        let dir_bytes = count
            .checked_mul(MAPSAVE_DIR_ENTRY_BYTES)
            .ok_or_else(|| SaveError::Corrupt("directory size overflow".into()))?;
        let data_end = bytes.len().checked_sub(dir_bytes).ok_or_else(|| {
            SaveError::Corrupt(format!(
                "{count} directory entries do not fit in {} bytes",
                bytes.len()
            ))
        })?;
        if data_end < 4 {
            return Err(SaveError::Corrupt("header larger than the data region".into()));
        }
        let mut offset = 4usize;
        let mut entries = Vec::with_capacity(count);
        for index in 0..count {
            let at = data_end + index * MAPSAVE_DIR_ENTRY_BYTES;
            let mut name = [0u8; MAPSAVE_NAME_BYTES];
            name.copy_from_slice(take(bytes, at, MAPSAVE_NAME_BYTES)?);
            let reserved = read_u16(bytes, at + MAPSAVE_NAME_BYTES)?;
            let length = read_u32(bytes, at + MAPSAVE_NAME_BYTES + 2)? as usize;
            let end = offset
                .checked_add(length)
                .ok_or_else(|| SaveError::Corrupt("entry length overflow".into()))?;
            if end > data_end {
                return Err(SaveError::Corrupt(format!(
                    "entry {index} ends at {end}, past the data region {data_end}"
                )));
            }
            entries.push(MapSaveEntry {
                name,
                reserved,
                data: bytes[offset..end].to_vec(),
            });
            offset = end;
        }
        if offset != data_end {
            return Err(SaveError::Corrupt(format!(
                "directory covers {offset} of {data_end} data bytes"
            )));
        }
        Ok(Self { dir_type, entries })
    }

    /// `(discovered, total)` bytes across every region.
    pub fn discovered(&self) -> (usize, usize) {
        (
            self.entries.iter().map(MapSaveEntry::discovered).sum(),
            self.entries.iter().map(|entry| entry.data.len()).sum(),
        )
    }

    /// Total data bytes (the directory excluded).
    pub fn data_bytes(&self) -> usize {
        self.entries.iter().map(|entry| entry.data.len()).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_tree() -> SaveTree {
        SaveTree {
            header: SaveTreeHeader {
                version: SAVETREE_VERSION,
                x: 1000,
                y: -20,
                z: 2500,
                map_id: 19,
                environment: 3,
            },
            buildings: vec![
                BuildingRecord {
                    name_seed: 0x1234,
                    service_time_limit: 600,
                    faction_id: 150,
                    sector: -1,
                    location_id: 7,
                    building_type: 3,
                    quality: 2,
                    ..BuildingRecord::default()
                },
                BuildingRecord::default(),
            ],
            records: vec![
                SaveTreeRecord {
                    root: RecordRoot {
                        record_type: 0x02,
                        record_id: 1,
                        parent_id: 0,
                        time: 42,
                        ..RecordRoot::default()
                    },
                    data: vec![0xAA; 107],
                },
                SaveTreeRecord {
                    root: RecordRoot {
                        record_type: 0x03,
                        record_id: 2,
                        ..RecordRoot::default()
                    },
                    data: Vec::new(),
                },
            ],
        }
    }

    #[test]
    fn header_is_nineteen_bytes_and_roundtrips() {
        let header = sample_tree().header;
        let bytes = header.encode();
        assert_eq!(bytes.len(), SAVETREE_HEADER_BYTES);
        assert_eq!(SaveTreeHeader::decode(&bytes).expect("decode"), header);
        assert_eq!(&bytes[..4], &0x126i32.to_le_bytes());
    }

    #[test]
    fn record_root_keeps_its_gaps_and_totals_seventy_one_bytes() {
        let mut root = RecordRoot {
            record_type: 0x24,
            pitch: -1,
            yaw: 2,
            roll: -3,
            x: -4000,
            y: 5,
            z: 6000,
            sprite_index: 9,
            picture2: 3,
            record_id: 77,
            quest_id: 1,
            parent_id: 12,
            time: 999,
            item_object: 4,
            quest_object_id: 5,
            next: 6,
            child: 7,
            sublist_head: 8,
            parent_type: -2,
            ..RecordRoot::default()
        };
        root.reserved_19_27 = [1, 2, 3, 4, 5, 6, 7, 8];
        root.reserved_35_38 = [9, 10, 11];
        let bytes = root.encode();
        assert_eq!(bytes.len(), RECORD_ROOT_BYTES);
        assert_eq!(RecordRoot::decode(&bytes).expect("decode"), root);
    }

    #[test]
    fn save_tree_roundtrips_byte_identically() {
        let tree = sample_tree();
        let bytes = tree.encode();
        let decoded = SaveTree::decode(&bytes).expect("decode");
        assert_eq!(decoded, tree);
        assert_eq!(decoded.encode(), bytes);
        // 19 header + 4 + 2·26 buildings + (4+71+107) + (4+71)
        assert_eq!(bytes.len(), 19 + 4 + 52 + 182 + 75);
    }

    #[test]
    fn truncated_save_tree_is_an_error_not_a_panic() {
        let bytes = sample_tree().encode();
        for cut in [0, 1, 10, 18, 19, 20, 30, 60] {
            assert!(SaveTree::decode(&bytes[..cut]).is_err(), "cut {cut}");
        }
        // Cut 100 lands inside record[0]'s frame: the source (SaveTree.cs:
        // 219-225) stops the scan instead of reading past EOF, so the decode
        // succeeds with zero records rather than erroring.
        let partial = SaveTree::decode(&bytes[..100]).expect("stop-scan");
        assert!(partial.records.is_empty(), "cut 100 keeps 0 records");
    }

    fn sample_mapsave() -> MapSave {
        MapSave {
            dir_type: MAPSAVE_DIR_TYPE,
            entries: vec![
                MapSaveEntry::new("MAPSAVE.000", vec![0x00, 0x40, 0xC0, 0x41]),
                MapSaveEntry::new("MAPSAVE.001", vec![0x40]),
            ],
        }
    }

    #[test]
    fn mapsave_roundtrips_and_counts_discovery() {
        let map = sample_mapsave();
        let bytes = map.encode();
        assert_eq!(bytes.len(), 4 + 5 + 2 * MAPSAVE_DIR_ENTRY_BYTES);
        let decoded = MapSave::decode(&bytes).expect("decode");
        assert_eq!(decoded, map);
        assert_eq!(decoded.encode(), bytes);
        assert_eq!(decoded.entries[0].name_str(), "MAPSAVE.000");
        assert_eq!(decoded.data_bytes(), 5);
        assert_eq!(decoded.discovered(), (4, 5));
    }

    #[test]
    fn mapsave_directory_must_cover_the_data() {
        let mut bytes = sample_mapsave().encode();
        let last = bytes.len() - 4;
        bytes[last..].copy_from_slice(&99u32.to_le_bytes());
        assert!(matches!(MapSave::decode(&bytes), Err(SaveError::Corrupt(_))));
    }

    /// The retail census **shape**, built in code: 62 region entries
    /// `MAPSAVE.000`…`MAPSAVE.061`, `dirType 0x0100` and the documented
    /// framing `4 + data + count·18` (spec §2.3, `BsaFile.cs:374-406`).
    ///
    /// Every payload byte here is **generated**, never copied: 0 retail bytes
    /// live in this repository (Mode A). What this pins is the *structure* the
    /// probe compares against the retail file at runtime
    /// (`crates/app/src/m33_probe.rs:1340-1353`), so the shape is covered
    /// even without `ARENA2`.
    #[test]
    fn mapsave_retail_census_shape_roundtrips_synthetically() {
        let mut entries: Vec<MapSaveEntry> = Vec::with_capacity(62);
        let mut expected_found = 0usize;
        for index in 0..62u16 {
            // Lengths cycle 1..=7; bit 0x40 lands on a deterministic
            // (index, slot) pattern — synthetic, never retail-derived.
            let len = usize::from(index % 7) + 1;
            let mut data = Vec::with_capacity(len);
            for slot in 0..len {
                let byte = if (usize::from(index) + slot) % 3 == 0 {
                    MAPSAVE_DISCOVERED_BIT
                } else {
                    0x00
                };
                if byte & MAPSAVE_DISCOVERED_BIT != 0 {
                    expected_found += 1;
                }
                data.push(byte);
            }
            entries.push(MapSaveEntry::new(&format!("MAPSAVE.{index:03}"), data));
        }

        let map = MapSave {
            dir_type: MAPSAVE_DIR_TYPE,
            entries,
        };
        let data = map.data_bytes();
        assert_eq!(data, 245, "8 cycles of 1..=7 (224) + 1..=6 (21)");
        let bytes = map.encode();
        // 4 header + 245 data + 62·18 directory.
        assert_eq!(bytes.len(), 4 + data + 62 * MAPSAVE_DIR_ENTRY_BYTES);
        assert_eq!(bytes.len(), 1365, "documented synthetic census size");

        let decoded = MapSave::decode(&bytes).expect("decode");
        assert_eq!(decoded, map, "byte-identical round-trip");
        assert_eq!(decoded.encode(), bytes);
        assert_eq!(decoded.dir_type, MAPSAVE_DIR_TYPE);
        assert_eq!(decoded.entries.len(), 62, "one entry per region");
        for (index, entry) in decoded.entries.iter().enumerate() {
            assert_eq!(entry.name_str(), format!("MAPSAVE.{index:03}"));
        }
        assert_eq!(decoded.discovered(), (expected_found, data));
    }
}
