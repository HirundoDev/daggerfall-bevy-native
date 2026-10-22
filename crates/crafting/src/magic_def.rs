// MAGIC.DEF — retail magic-item templates (SPEC M24 §1).
//
// Layout transcribed from Daggerfall Unity `@0038a1e61999ad2c73fb1f33b706cae9a88ec66f`
// `API/MagicItemsFile.cs:27,76-105` (MIT):
//
// - `nameLength = 32`, header `int recordCount`, then `recordCount` records of
//   62 bytes each, little-endian throughout.
// - one record: `name` (CString 32, NUL-padded) | `type` u8 | `group` u8 |
//   `groupIndex` u8 | `enchantments[10]` (10 × (s8 type, s8 param) = 20 B) |
//   `uses` i16 | `value` i32 | `material` u8. 32+1+1+1+20+2+4+1 = 62.
//
// `type == 0` is `RegularMagicItem`; 1/2 are `ArtifactClass1/2`
// (`ItemsFile.cs:101-106`). The item maker only ever offers `type == 0`
// (`ItemBuilder.cs:538`); artifacts carry a fixed enchantment set and no
// budget (`SetArtifact`, §2.1). This module parses the file; the budget
// accounting is [`crate::enchanting`].

/// Expected retail file name (`MagicItemsFile.cs:57`, `EndsWith`, case-
/// insensitive).
pub const MAGIC_DEF_FILENAME: &str = "MAGIC.DEF";

/// Bytes of the NUL-padded name field (`MagicItemsFile.cs:27`).
pub const MAGIC_NAME_LENGTH: usize = 32;

/// Number of enchantment slots per record (`DaggerfallItemMakerWindow.cs:629`,
/// the `== 10` guard).
pub const MAGIC_ENCHANT_SLOTS: usize = 10;

/// Encoded size of one record: `32 + 1 + 1 + 1 + 20 + 2 + 4 + 1`
/// (`MagicItemsFile.cs:83-105`).
pub const MAGIC_RECORD_SIZE: usize = 62;

/// One `(type, param)` enchantment pair (`DaggerfallEnchantment`,
/// `ItemsFile.cs:94-99`): `type` is an `EnchantmentTypes` value, `param` a
/// spell id / artifact id / group id depending on the type.
pub type LegacyEnchantment = (i8, i8);

/// One decoded `MAGIC.DEF` record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MagicItemTemplate {
    /// `TrimEnd('\0')` of the 32-byte ASCII name field.
    pub name: String,
    /// `type`: 0 = `RegularMagicItem`, 1/2 = `ArtifactClass1/2`
    /// (`ItemsFile.cs:101-106`).
    pub kind: u8,
    /// `group` (`ItemBuilder.cs:529-558`): 0/1 random sets, 2 = `Weapons`.
    pub group: u8,
    /// `groupIndex` within the group.
    pub group_index: u8,
    /// The 10 `(type, param)` enchantment slots, in file order.
    pub enchantments: [LegacyEnchantment; MAGIC_ENCHANT_SLOTS],
    /// `uses` — charge count (i16).
    pub uses: i16,
    /// `value` — gold price (i32).
    pub value: i32,
    /// `material` — armor/weapon material byte.
    pub material: u8,
}

/// Parse error for a malformed `MAGIC.DEF` blob.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MagicDefError {
    /// Fewer than 4 bytes: the `recordCount` header does not fit.
    TruncatedHeader { len: usize },
    /// `4 + recordCount · 62` exceeds the blob: a record is cut short
    /// (`recordCount` and the measured size disagree).
    TruncatedRecords {
        /// Declared record count from the header.
        record_count: u32,
        /// Bytes present after the 4-byte header.
        available: usize,
    },
}

impl core::fmt::Display for MagicDefError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            MagicDefError::TruncatedHeader { len } => {
                write!(f, "MAGIC.DEF: {len} B < 4 B header")
            }
            MagicDefError::TruncatedRecords {
                record_count,
                available,
            } => write!(
                f,
                "MAGIC.DEF: recordCount={record_count} needs {} B of records, {available} B present",
                *record_count as usize * MAGIC_RECORD_SIZE
            ),
        }
    }
}

/// Decodes the NUL-padded 32-byte name field (`FileProxy.ReadCString`).
fn decode_name(raw: &[u8]) -> String {
    let end = raw.iter().position(|&b| b == 0).unwrap_or(raw.len());
    String::from_utf8_lossy(&raw[..end]).into_owned()
}

/// Parses a full `MAGIC.DEF` blob into its record vector
/// (`MagicItemsFile.cs:73-105`). The header is `recordCount` (u32 LE); each
/// record is exactly [`MAGIC_RECORD_SIZE`] bytes, little-endian. The total
/// implied size `4 + recordCount · 62` must not exceed the blob — a mismatch
/// is [`MagicDefError::TruncatedRecords`] (never a partial/garbage record).
pub fn parse_magic_def(data: &[u8]) -> Result<Vec<MagicItemTemplate>, MagicDefError> {
    if data.len() < 4 {
        return Err(MagicDefError::TruncatedHeader { len: data.len() });
    }
    let record_count = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
    let available = data.len() - 4;
    let needed = record_count as usize * MAGIC_RECORD_SIZE;
    if available < needed {
        return Err(MagicDefError::TruncatedRecords {
            record_count,
            available,
        });
    }

    let mut records = Vec::with_capacity(record_count as usize);
    for index in 0..record_count as usize {
        let base = 4 + index * MAGIC_RECORD_SIZE;
        let rec = &data[base..base + MAGIC_RECORD_SIZE];
        let name = decode_name(&rec[0..MAGIC_NAME_LENGTH]);
        let kind = rec[32];
        let group = rec[33];
        let group_index = rec[34];
        let mut enchantments = [(0i8, 0i8); MAGIC_ENCHANT_SLOTS];
        for (slot, entry) in enchantments.iter_mut().enumerate() {
            let off = 35 + slot * 2;
            *entry = (rec[off] as i8, rec[off + 1] as i8);
        }
        let uses = i16::from_le_bytes([rec[55], rec[56]]);
        let value = i32::from_le_bytes([rec[57], rec[58], rec[59], rec[60]]);
        let material = rec[61];
        records.push(MagicItemTemplate {
            name,
            kind,
            group,
            group_index,
            enchantments,
            uses,
            value,
            material,
        });
    }
    Ok(records)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds one 62-byte record from its fields (LE), for synthetic tests.
    #[allow(clippy::too_many_arguments)] // synthetic builder: 1 parameter per layout byte, like `spells_std::record_bytes`
    fn synth_record(
        name: &str,
        kind: u8,
        group: u8,
        group_index: u8,
        enchantments: &[LegacyEnchantment],
        uses: i16,
        value: i32,
        material: u8,
    ) -> [u8; MAGIC_RECORD_SIZE] {
        let mut rec = [0u8; MAGIC_RECORD_SIZE];
        let bytes = name.as_bytes();
        let n = bytes.len().min(MAGIC_NAME_LENGTH);
        rec[0..n].copy_from_slice(&bytes[..n]);
        rec[32] = kind;
        rec[33] = group;
        rec[34] = group_index;
        for (slot, &(kind8, param)) in enchantments.iter().enumerate() {
            rec[35 + slot * 2] = kind8 as u8;
            rec[35 + slot * 2 + 1] = param as u8;
        }
        rec[55..57].copy_from_slice(&uses.to_le_bytes());
        rec[57..61].copy_from_slice(&value.to_le_bytes());
        rec[61] = material;
        rec
    }

    fn synth_blob(records: &[[u8; MAGIC_RECORD_SIZE]]) -> Vec<u8> {
        let mut blob = Vec::with_capacity(4 + records.len() * MAGIC_RECORD_SIZE);
        blob.extend_from_slice(&(records.len() as u32).to_le_bytes());
        for rec in records {
            blob.extend_from_slice(rec);
        }
        blob
    }

    /// GV-MAG-01..03, 09, 10: header count, record size, implied total,
    /// 20 B of enchantments, and a 62-byte read stride — all on a synthetic
    /// two-record blob (no retail bytes).
    #[test]
    fn gv_mag_layout_synthetic() {
        let a = synth_record("TestA", 0, 2, 0, &[(11, 0), (8, 0)], 1500, 50000, 0);
        let b = synth_record("TestB", 1, 3, 1, &[(26, 1)], 3, 0, 9);
        let blob = synth_blob(&[a, b]);
        assert_eq!(blob.len(), 4 + 2 * MAGIC_RECORD_SIZE);
        let records = parse_magic_def(&blob).expect("synthetic blob parses");
        assert_eq!(records.len(), 2);
        assert_eq!(MAGIC_ENCHANT_SLOTS * 2, 20); // GV-MAG-09
        assert_eq!(MAGIC_RECORD_SIZE, 62); // GV-MAG-02
        assert_eq!(4 + 59 * MAGIC_RECORD_SIZE, 3662); // GV-MAG-03
        assert_eq!(records[0].name, "TestA");
        assert_eq!(records[0].enchantments[0], (11, 0));
        assert_eq!(records[1].kind, 1);
    }

    /// A short blob is a clean error, never a partial record.
    #[test]
    fn rejects_truncated() {
        assert_eq!(
            parse_magic_def(&[1, 2]),
            Err(MagicDefError::TruncatedHeader { len: 2 })
        );
        // Header says 2 records but only 1 is present.
        let a = synth_record("OnlyOne", 0, 2, 0, &[], 0, 0, 0);
        let mut blob = (2u32).to_le_bytes().to_vec();
        blob.extend_from_slice(&a);
        assert!(matches!(
            parse_magic_def(&blob),
            Err(MagicDefError::TruncatedRecords {
                record_count: 2,
                ..
            })
        ));
    }
}
