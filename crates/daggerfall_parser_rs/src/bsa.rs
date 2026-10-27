//! Daggerfall BSA container — clean-room from UESP `dfbsa.txt`
//! ("General File Layout", "BSA Header", "BSA 0x0100 Directory",
//! "BSA 0x0200 Directory" sections).
//!
//! File layout:
//! ```text
//! [BSA Header (4 bytes)][packed data records][directory at the end]
//! ```
//! - Header: `short DirectoryCount` (bytes 0-1) + `short DirectoryType` (bytes 2-3),
//!   little-endian. `0x0100` = directory with names (18-byte records);
//!   `0x0200` = directory with numeric IDs (8-byte records).
//! - The first data record starts at offset 4; subsequent offsets
//!   are computed by accumulating `DataSize` in directory order (dfbsa.txt:
//!   "The subsequent section offsets can be calculated from this").

/// Known directory types (dfbsa.txt: "two known types").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DirectoryType {
    /// Directory with names: 18-byte records (`char FileName[14] + long DataSize`).
    Name18,
    /// Numeric directory: 8-byte records (long RecordID + long DataSize).
    Number8,
}

impl DirectoryType {
    pub fn from_u16(value: u16) -> Option<Self> {
        match value {
            0x0100 => Some(Self::Name18),
            0x0200 => Some(Self::Number8),
            _ => None,
        }
    }

    /// Size in bytes of each directory record.
    pub const fn record_size(self) -> usize {
        match self {
            Self::Name18 => 18,
            Self::Number8 => 8,
        }
    }
}

/// Directory entry of a BSA.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Entry {
    /// Type 0x0100: DOS 8.3 name (14 bytes with NULs) + data size.
    Named { name: String, data_size: u32 },
    /// Type 0x0200: numeric identifier + data size.
    Numbered { record_id: u32, data_size: u32 },
}

impl Entry {
    pub const fn data_size(&self) -> u32 {
        match self {
            Self::Named { data_size, .. } | Self::Numbered { data_size, .. } => *data_size,
        }
    }
}

/// Parsed BSA file (metadata; the bytes live outside, in the caller's buffer).
#[derive(Debug, Clone)]
pub struct BsaArchive {
    pub directory_type: DirectoryType,
    /// Start of the data: always 4 (after the header).
    pub data_start: u64,
    /// Directory offset (end of the data section).
    pub directory_offset: u64,
    pub entries: Vec<Entry>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BsaError {
    /// File shorter than the 4-byte header.
    TooShort { len: usize },
    /// DirectoryType outside {0x0100, 0x0200}.
    UnknownDirectoryType { raw: u16 },
    /// The directory does not fit in the file.
    DirectoryOutOfBounds { len: usize, directory_offset: u64, needed: u64 },
    /// 4 + Σ DataSize ≠ directory offset (layout invariant, dfbsa.txt).
    DataLayoutMismatch { computed_data_end: u64, directory_offset: u64 },
    /// Empty DOS 8.3 name after trimming NULs.
    EmptyName { index: usize },
    /// Entry that runs past the buffer when resolving its bytes.
    EntryOutOfBounds { index: usize, start: u64, end: u64, len: usize },
}

impl std::fmt::Display for BsaError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TooShort { len } => write!(f, "BSA too short for header: {len} bytes"),
            Self::UnknownDirectoryType { raw } => write!(f, "unknown DirectoryType: 0x{raw:04X}"),
            Self::DirectoryOutOfBounds { len, directory_offset, needed } => write!(
                f,
                "directory out of bounds: offset {directory_offset} + {needed} bytes > {len}"
            ),
            Self::DataLayoutMismatch { computed_data_end, directory_offset } => write!(
                f,
                "inconsistent layout: computed data end {computed_data_end} != directory at {directory_offset}"
            ),
            Self::EmptyName { index } => write!(f, "empty name in entry {index}"),
            Self::EntryOutOfBounds { index, start, end, len } => write!(
                f,
                "data of entry {index} out of bounds: [{start}, {end}) > {len}"
            ),
        }
    }
}

impl std::error::Error for BsaError {}

/// Parses a complete BSA from its in-memory buffer (read-only, 0 copies).
pub fn parse(data: &[u8]) -> Result<BsaArchive, BsaError> {
    if data.len() < 4 {
        return Err(BsaError::TooShort { len: data.len() });
    }
    let raw_count = u16::from_le_bytes([data[0], data[1]]);
    let raw_type = u16::from_le_bytes([data[2], data[3]]);
    let directory_type =
        DirectoryType::from_u16(raw_type).ok_or(BsaError::UnknownDirectoryType { raw: raw_type })?;

    let record_size = directory_type.record_size() as u64;
    let needed = u64::from(raw_count) * record_size;
    let directory_offset = data.len() as u64 - needed;
    if directory_offset < 4 {
        return Err(BsaError::DirectoryOutOfBounds {
            len: data.len(),
            directory_offset,
            needed,
        });
    }

    let dir = &data[directory_offset as usize..];
    let mut entries = Vec::with_capacity(raw_count as usize);
    let mut computed_data_end = 4u64;
    for i in 0..raw_count as usize {
        let rec = &dir[i * record_size as usize..(i + 1) * record_size as usize];
        let entry = match directory_type {
            DirectoryType::Name18 => {
                let name_bytes = &rec[0..14];
                let end = name_bytes.iter().position(|&b| b == 0).unwrap_or(14);
                let name = String::from_utf8_lossy(&name_bytes[..end]).into_owned();
                if name.is_empty() {
                    return Err(BsaError::EmptyName { index: i });
                }
                // 0x0100: char FileName[14] (bytes 0-13) + long DataSize (bytes 14-17).
                let data_size = u32::from_le_bytes([rec[14], rec[15], rec[16], rec[17]]);
                Entry::Named { name, data_size }
            }
            DirectoryType::Number8 => {
                // 0x0200: long RecordID (bytes 0-3) + long DataSize (bytes 4-7).
                let record_id = u32::from_le_bytes([rec[0], rec[1], rec[2], rec[3]]);
                let data_size = u32::from_le_bytes([rec[4], rec[5], rec[6], rec[7]]);
                Entry::Numbered { record_id, data_size }
            }
        };
        computed_data_end += u64::from(entry.data_size());
        entries.push(entry);
    }

    // dfbsa.txt invariant: data starts at offset 4 and each section follows
    // the previous one; the directory lives at the end of the file.
    if computed_data_end != directory_offset {
        return Err(BsaError::DataLayoutMismatch {
            computed_data_end,
            directory_offset,
        });
    }

    Ok(BsaArchive {
        directory_type,
        data_start: 4,
        directory_offset,
        entries,
    })
}

/// Returns the raw bytes of record `index` (0-based, directory order).
pub fn entry_data<'a>(archive: &BsaArchive, data: &'a [u8], index: usize) -> Result<&'a [u8], BsaError> {
    let entry = archive.entries.get(index).ok_or(BsaError::EntryOutOfBounds {
        index,
        start: 0,
        end: 0,
        len: data.len(),
    })?;
    let mut start = archive.data_start;
    for prev in &archive.entries[..index] {
        start += u64::from(prev.data_size());
    }
    let end = start + u64::from(entry.data_size());
    if end > data.len() as u64 {
        return Err(BsaError::EntryOutOfBounds {
            index,
            start,
            end,
            len: data.len(),
        });
    }
    Ok(&data[start as usize..end as usize])
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds a synthetic 0x0100-type BSA (14-byte name + size).
    fn build_named(files: &[(&str, &[u8])]) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(&(files.len() as u16).to_le_bytes());
        out.extend_from_slice(&0x0100u16.to_le_bytes());
        for (_, payload) in files {
            out.extend_from_slice(payload);
        }
        for (name, payload) in files {
            let mut rec = [0u8; 18];
            let bytes = name.as_bytes();
            assert!(bytes.len() <= 13, "test: DOS 8.3 name too long");
            rec[..bytes.len()].copy_from_slice(bytes);
            rec[14..18].copy_from_slice(&(payload.len() as u32).to_le_bytes());
            out.extend_from_slice(&rec);
        }
        out
    }

    #[test]
    fn parses_named_directory_layout() {
        let data = build_named(&[("MESH1.NCF", b"abcde"), ("$OBJ2.NCF", b"1234567")]);
        let archive = parse(&data).expect("parse");
        assert_eq!(archive.directory_type, DirectoryType::Name18);
        assert_eq!(archive.entries.len(), 2);
        assert_eq!(archive.directory_offset, 4 + 5 + 7);
        assert_eq!(
            archive.entries[0],
            Entry::Named { name: "MESH1.NCF".into(), data_size: 5 }
        );
        assert_eq!(entry_data(&archive, &data, 0).unwrap(), b"abcde");
        assert_eq!(entry_data(&archive, &data, 1).unwrap(), b"1234567");
    }

    #[test]
    fn parses_numbered_directory_layout() {
        let mut data = Vec::new();
        data.extend_from_slice(&2u16.to_le_bytes());
        data.extend_from_slice(&0x0200u16.to_le_bytes());
        data.extend_from_slice(&[0xDE, 0xAD, 0xBE, 0xEF]); // data: record 0 (4 B), record 1 (0 B)
        for (id, size) in [(100u32, 4u32), (200, 0)] {
            let mut rec = [0u8; 8];
            rec[0..4].copy_from_slice(&id.to_le_bytes());
            rec[4..8].copy_from_slice(&size.to_le_bytes());
            data.extend_from_slice(&rec);
        }
        let archive = parse(&data).expect("parse");
        assert_eq!(archive.directory_type, DirectoryType::Number8);
        assert_eq!(archive.entries[1], Entry::Numbered { record_id: 200, data_size: 0 });
        assert_eq!(entry_data(&archive, &data, 0).unwrap(), &[0xDE, 0xAD, 0xBE, 0xEF]);
    }

    #[test]
    fn rejects_unknown_directory_type() {
        let mut data = vec![2, 0, 0x00, 0x03, 0, 0, 0, 0];
        data.extend_from_slice(&[0; 16]);
        assert!(matches!(parse(&data), Err(BsaError::UnknownDirectoryType { raw: 0x0300 })));
    }

    #[test]
    fn rejects_truncated_file() {
        assert!(matches!(parse(&[0x01, 0x00]), Err(BsaError::TooShort { len: 2 })));
    }

    #[test]
    fn rejects_size_layout_mismatch() {
        // Header says 1 entry but DataSize does not reach the directory.
        let mut data = Vec::new();
        data.extend_from_slice(&1u16.to_le_bytes());
        data.extend_from_slice(&0x0200u16.to_le_bytes());
        data.extend_from_slice(&[0u8; 7]); // actual payload: 7 bytes…
        let mut rec = [0u8; 8];
        rec[4..8].copy_from_slice(&99u32.to_le_bytes()); // …but the directory says 99.
        data.extend_from_slice(&rec);
        assert!(matches!(
            parse(&data),
            Err(BsaError::DataLayoutMismatch { computed_data_end: 103, directory_offset: 11 })
        ));
        // Sanity: with the correct size it does parse.
        let mut data_ok = data;
        let rec_off = data_ok.len() - 8;
        data_ok[rec_off + 4..rec_off + 8].copy_from_slice(&7u32.to_le_bytes());
        assert!(parse(&data_ok).is_ok());
    }

    #[test]
    fn rejects_empty_directory_name() {
        let mut data = Vec::new();
        data.extend_from_slice(&1u16.to_le_bytes());
        data.extend_from_slice(&0x0100u16.to_le_bytes());
        data.extend_from_slice(&[0; 3]); // 3 data bytes
        let mut rec = [0u8; 18];
        rec[14..18].copy_from_slice(&3u32.to_le_bytes());
        data.extend_from_slice(&rec);
        assert!(matches!(parse(&data), Err(BsaError::EmptyName { index: 0 })));
    }
}
