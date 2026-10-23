//! `.RDI` (Dungeon Flag Records of BLOCKS.BSA) — clean-room parser (M11,
//! REQ-L99-043).
//!
//! Spec anchors (all verified against the pinned reference corpus, commit
//! `0038a1e6`):
//!
//! - UESP `Daggerfall_Mod:BLOCKS.BSA` § "RDI Record"
//!   (<https://en.uesp.net/wiki/Daggerfall_Mod:BLOCKS.BSA>): fixed-length
//!   record of 512 bytes; the bytes "appear to contain only 00's and 01's";
//!   there is one RDI record per Dungeon Block Record (`.RDB`).
//! - `DFU:Assets/Scripts/API/BlocksFile.cs:1334-1345` — `ReadRdiRecord`
//!   does exactly `blocks[block].DFBlock.RdiBlock.Data = reader.ReadBytes(512)`:
//!   it reads 512 bytes and infers no further structure.
//! - `DFU:Assets/Scripts/API/BlocksFile.cs:229-231` — `GetBlockType`
//!   derives the record type from the `.RDI` suffix; `:683-686` dispatches
//!   `ReadBlock` to `ReadRdiRecord`.
//! - `DFU:Assets/Scripts/API/DFBlock.cs:1115-1125` — `RdiBlockDesc { byte[]
//!   Data; }` is documented as "512 bytes of unknown data — Format of Data
//!   is currently unknown".
//!
//! Measured facts in the retail `BLOCKS.BSA` (32,884,997 B, directory at
//! 0x0100, 1295 records): 187 `.RDI` records of **exactly 512 bytes** each,
//! **every byte ∈ {0,1}** (0 records with a byte > 1), 14,487 active flags,
//! 2 all-zero records, and a **1:1 pairing by base name** with the 187
//! `.RDB` records (0 orphans in either direction).
//!
//! **The semantics of the 512 flags are NOT documented and NOT invented
//! here.** The reference implementation loads the record and never consumes
//! `RdiBlock.Data` beyond that load (0 semantic readers over `0038a1e6`), so
//! there is no consumer to mirror. This module implements only the verified
//! mechanism: fixed length, the boolean-flag invariant, the base-name
//! pairing and a deterministic digest — no meaning is assigned to any flag
//! position. Fine-grained semantics remain declared technical debt; the M11
//! probe uses the module to (a) census the 187 companions and (b) gate the
//! subject selection on the presence/usability of the companion record.
//!
//! Zero dependencies, zero `unsafe`: the parser validates in place and keeps
//! an owned 512-byte array (the record is fixed-size, so no allocation
//! strategy is exposed).

/// Fixed length of an `.RDI` record (UESP "RDI Record"; `DFU:BlocksFile.cs`
/// reads exactly 512 bytes, `:1341`).
pub const RDI_RECORD_LEN: usize = 512;

/// Number of Boolean flags in a record (one per byte — the measured
/// invariant of the format: every byte ∈ {0,1}).
pub const RDI_FLAG_COUNT: usize = RDI_RECORD_LEN;

/// FNV-1a 64-bit offset basis (used for the record and directory digests).
pub const FNV1A64_OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;

/// FNV-1a 64-bit prime.
pub const FNV1A64_PRIME: u64 = 0x0000_0100_0000_01b3;

/// `.RDI` parse errors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RdiError {
    /// Record is not exactly [`RDI_RECORD_LEN`] bytes.
    WrongLength { len: usize },
    /// Byte at `offset` is neither 0 nor 1 (the measured boolean invariant).
    NonBooleanFlag { offset: usize, value: u8 },
}

impl std::fmt::Display for RdiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::WrongLength { len } => write!(
                f,
                "RDI record of {len} bytes != the fixed {RDI_RECORD_LEN} bytes"
            ),
            Self::NonBooleanFlag { offset, value } => write!(
                f,
                "RDI flag at offset {offset} = 0x{value:02x} is not Boolean (0 or 1)"
            ),
        }
    }
}

impl std::error::Error for RdiError {}

/// A validated `.RDI` record: 512 Boolean flags of undocumented meaning.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RdiFlags {
    flags: [u8; RDI_RECORD_LEN],
}

impl RdiFlags {
    /// Validates one `.RDI` record: exact length and the Boolean invariant.
    ///
    /// The first non-Boolean byte in ascending offset order is reported, so a
    /// malformed retail record is diagnosable to one position.
    pub fn parse(data: &[u8]) -> Result<Self, RdiError> {
        let bytes: [u8; RDI_RECORD_LEN] = data
            .try_into()
            .map_err(|_| RdiError::WrongLength { len: data.len() })?;
        if let Some((offset, value)) = bytes
            .iter()
            .copied()
            .enumerate()
            .find(|(_, value)| *value > 1)
        {
            return Err(RdiError::NonBooleanFlag { offset, value });
        }
        Ok(Self { flags: bytes })
    }

    /// Flag at `index`: `None` when the index is ≥ [`RDI_FLAG_COUNT`].
    /// The value of a flag is not interpreted (format undocumented).
    pub fn flag(&self, index: usize) -> Option<bool> {
        self.flags.get(index).map(|value| *value != 0)
    }

    /// Number of flags that are set (`!= 0`).
    pub fn active_flags(&self) -> usize {
        self.flags.iter().filter(|value| **value != 0).count()
    }

    /// `true` for an all-zero record (measured: 2 in the retail directory).
    pub fn is_empty(&self) -> bool {
        self.flags.iter().all(|value| *value == 0)
    }

    /// Raw record bytes (512 B).
    pub fn as_bytes(&self) -> &[u8; RDI_RECORD_LEN] {
        &self.flags
    }

    /// FNV-1a64 digest of the raw record ([`fnv1a64`]).
    pub fn fnv1a64(&self) -> u64 {
        fnv1a64(&self.flags)
    }
}

/// FNV-1a 64-bit hash of `bytes` (fresh state — see [`fnv1a64_update`]).
pub fn fnv1a64(bytes: &[u8]) -> u64 {
    fnv1a64_update(FNV1A64_OFFSET_BASIS, bytes)
}

/// FNV-1a 64-bit fold: continues a digest from `state` over `bytes`, in place
/// (no allocation — the directory digest accumulates 187 records).
pub fn fnv1a64_update(mut state: u64, bytes: &[u8]) -> u64 {
    for byte in bytes {
        state ^= u64::from(*byte);
        state = state.wrapping_mul(FNV1A64_PRIME);
    }
    state
}

/// Companion `.RDI` name of an `.RDB` record (`N0000055.RDB` →
/// `N0000055.RDI`). `None` when `rdb_name` does not end in `.RDB`
/// (case-insensitive); the stem keeps its original case and the extension is
/// emitted uppercase, matching the retail directory.
pub fn companion_rdi_name(rdb_name: &str) -> Option<String> {
    name_ends_with(rdb_name, ".RDB").then(|| replace_extension(rdb_name, ".RDI"))
}

/// Companion `.RDB` name of an `.RDI` record (inverse of
/// [`companion_rdi_name`]).
pub fn companion_rdb_name(rdi_name: &str) -> Option<String> {
    name_ends_with(rdi_name, ".RDI").then(|| replace_extension(rdi_name, ".RDB"))
}

/// `true` when `name` ends in `suffix` (ASCII case-insensitive).
fn name_ends_with(name: &str, suffix: &str) -> bool {
    name.len() >= suffix.len()
        && name[name.len() - suffix.len()..].eq_ignore_ascii_case(suffix)
}

/// `stem + new_extension`, keeping the stem's original case (caller has
/// already validated the 4-byte extension).
fn replace_extension(name: &str, new_extension: &str) -> String {
    format!("{}{new_extension}", &name[..name.len() - 4])
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Synthetic Boolean record: flags at the given indices set.
    fn record_with(set: &[usize]) -> Vec<u8> {
        let mut bytes = vec![0u8; RDI_RECORD_LEN];
        for index in set {
            bytes[*index] = 1;
        }
        bytes
    }

    #[test]
    fn rejects_record_shorter_than_512() {
        assert_eq!(
            RdiFlags::parse(&vec![0u8; RDI_RECORD_LEN - 1]),
            Err(RdiError::WrongLength { len: 511 })
        );
    }

    #[test]
    fn rejects_record_longer_than_512() {
        assert_eq!(
            RdiFlags::parse(&vec![0u8; RDI_RECORD_LEN + 1]),
            Err(RdiError::WrongLength { len: 513 })
        );
    }

    #[test]
    fn rejects_non_boolean_flag_at_its_offset() {
        let mut bytes = record_with(&[3, 200]);
        bytes[37] = 2;
        assert_eq!(
            RdiFlags::parse(&bytes),
            Err(RdiError::NonBooleanFlag {
                offset: 37,
                value: 2
            })
        );
    }

    #[test]
    fn all_zero_record_is_empty() {
        let flags = RdiFlags::parse(&vec![0u8; RDI_RECORD_LEN]).expect("512 zero bytes parse");
        assert!(flags.is_empty());
        assert_eq!(flags.active_flags(), 0);
        assert_eq!(flags.flag(0), Some(false));
        assert_eq!(flags.flag(RDI_FLAG_COUNT), None);
    }

    #[test]
    fn boolean_pattern_counts_and_is_deterministic() {
        let set = [0usize, 1, 55, 511];
        let bytes = record_with(&set);
        let flags = RdiFlags::parse(&bytes).expect("Boolean record parses");
        assert_eq!(flags.active_flags(), set.len());
        assert!(!flags.is_empty());
        for index in 0..RDI_FLAG_COUNT {
            assert_eq!(flags.flag(index), Some(set.contains(&index)), "flag {index}");
        }
        assert_eq!(flags.as_bytes(), bytes.as_slice());
        let again = RdiFlags::parse(&bytes).expect("same input parses again");
        assert_eq!(flags, again);
        assert_eq!(flags.fnv1a64(), again.fnv1a64());
        assert_eq!(flags.fnv1a64(), fnv1a64(&bytes));
    }

    #[test]
    fn fnv1a64_reference_vectors() {
        assert_eq!(fnv1a64(b""), 0xcbf2_9ce4_8422_2325);
        assert_eq!(fnv1a64(b"a"), 0xaf63_dc4c_8601_ec8c);
    }

    #[test]
    fn fnv1a64_update_folds_without_restarting() {
        // Two-step fold == one-shot hash of the concatenation.
        let half = b"daggerfall";
        let other = b"-rdi";
        let folded = fnv1a64_update(fnv1a64(half), other);
        assert_eq!(folded, fnv1a64(b"daggerfall-rdi"));
    }

    #[test]
    fn companion_names_round_trip() {
        assert_eq!(
            companion_rdi_name("N0000055.RDB"),
            Some("N0000055.RDI".to_string())
        );
        assert_eq!(
            companion_rdi_name("n0000055.rdb"),
            Some("n0000055.RDI".to_string())
        );
        assert_eq!(companion_rdi_name("X.RMB"), None);
        assert_eq!(
            companion_rdb_name("n0000055.rdi"),
            Some("n0000055.RDB".to_string())
        );
        assert_eq!(companion_rdb_name("X.RDB"), None);
    }
}