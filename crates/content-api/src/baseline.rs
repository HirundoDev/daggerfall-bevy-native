//! The MAIN baseline: the immutable, sorted set of `base`-namespace records a
//! game build ships with, plus the canonical state hash.
//!
//! [`Baseline`] exposes no `&mut` accessor at all, so "the baseline is never
//! mutated" is enforced structurally by the type system rather than by review.

use std::collections::BTreeMap;
use std::fmt;

use sha2::{Digest, Sha256};

use crate::error::ApiError;
use crate::id::{ContentId, Namespace};
use crate::record::{AttrValue, Record};
use crate::schema::SCHEMA_VERSION_LABEL;

/// A 256-bit SHA-256 digest of a canonical content state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct StateHash([u8; 32]);

impl StateHash {
    /// Lowercase hexadecimal form (64 characters, no prefix).
    pub fn to_hex(&self) -> String {
        const HEX: &[u8; 16] = b"0123456789abcdef";
        let mut out = String::with_capacity(64);
        for byte in self.0 {
            out.push(HEX[usize::from(byte >> 4)] as char);
            out.push(HEX[usize::from(byte & 0x0f)] as char);
        }
        out
    }
}

impl fmt::Display for StateHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_hex())
    }
}

/// The immutable MAIN baseline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Baseline {
    records: Vec<Record>,
    index: BTreeMap<ContentId, usize>,
}

impl Baseline {
    /// Builds a baseline from records, rejecting duplicates and non-`base`
    /// namespaces.
    ///
    /// # Errors
    ///
    /// [`ApiError::DuplicateId`] if two records share an id;
    /// [`ApiError::NamespaceViolation`] if a record belongs to a side.
    pub fn new(mut records: Vec<Record>) -> Result<Self, ApiError> {
        records.sort_by(|left, right| left.id().cmp(right.id()));
        let mut index = BTreeMap::new();
        for (position, record) in records.iter().enumerate() {
            if let Namespace::Side(side) = record.id().namespace() {
                return Err(ApiError::NamespaceViolation {
                    id: record.id().clone(),
                    side: side.clone(),
                });
            }
            if index.insert(record.id().clone(), position).is_some() {
                return Err(ApiError::DuplicateId {
                    id: record.id().clone(),
                });
            }
        }
        Ok(Baseline { records, index })
    }

    /// The baseline record with this id, if any.
    pub fn get(&self, id: &ContentId) -> Option<&Record> {
        self.index.get(id).map(|position| &self.records[*position])
    }

    /// All records in ascending [`ContentId`] order.
    pub fn iter(&self) -> impl Iterator<Item = &Record> {
        self.records.iter()
    }

    /// Number of baseline records.
    pub fn len(&self) -> usize {
        self.records.len()
    }

    /// Whether the baseline is empty.
    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }

    /// Canonical state hash of the baseline alone - constant for the life of the
    /// value, by construction.
    pub fn hash(&self) -> StateHash {
        state_hash_of(self.records.iter())
    }
}

/// Canonical state hash over `records`.
///
/// The iterator MUST already be in ascending [`ContentId`] order and iterate all
/// records of the state; the function is `pub(crate)` precisely because that
/// pre-condition cannot be checked cheaply. Encoding (all lengths little-endian
/// where multi-byte):
///
/// `b"dfc1-content-state\x00"` · `u8` len + [`SCHEMA_VERSION_LABEL`] · `u64` count ·
/// per record: `u8` len + `id.as_str()`, `u32` attr count, then per attribute in
/// key order: `u8` len + key, `u8` tag (`0` Text, `1` Int, `2` Bool, `3` Ref) and
/// the payload (`Text`: `u32` len + UTF-8; `Int`: `i64` LE; `Bool`: `u8`; `Ref`:
/// `u8` len + `id.as_str()`).
///
/// Handles and the order in which overlays were applied are deliberately absent:
/// two equal views hash equal.
pub(crate) fn state_hash_of<'a, I>(records: I) -> StateHash
where
    I: ExactSizeIterator<Item = &'a Record> + Clone,
{
    debug_assert!(
        {
            let mut previous: Option<&ContentId> = None;
            records.clone().all(|record| {
                let ordered = previous.is_none_or(|last| last <= record.id());
                previous = Some(record.id());
                ordered
            })
        },
        "state_hash_of requires ascending ContentId order"
    );

    let mut hasher = Sha256::new();
    hasher.update(b"dfc1-content-state\x00");
    update_len_prefixed(&mut hasher, SCHEMA_VERSION_LABEL.as_bytes());
    hasher.update((records.len() as u64).to_le_bytes());
    for record in records {
        update_len_prefixed(&mut hasher, record.id().as_str().as_bytes());
        hasher.update((record.attrs().len() as u32).to_le_bytes());
        for (key, value) in record.attrs() {
            update_len_prefixed(&mut hasher, key.as_bytes());
            match value {
                AttrValue::Text(text) => {
                    hasher.update([0u8]);
                    hasher.update((text.len() as u32).to_le_bytes());
                    hasher.update(text.as_bytes());
                }
                AttrValue::Int(number) => {
                    hasher.update([1u8]);
                    hasher.update(number.to_le_bytes());
                }
                AttrValue::Bool(flag) => {
                    hasher.update([2u8]);
                    hasher.update([u8::from(*flag)]);
                }
                AttrValue::Ref(id) => {
                    hasher.update([3u8]);
                    update_len_prefixed(&mut hasher, id.as_str().as_bytes());
                }
            }
        }
    }
    StateHash(hasher.finalize().into())
}

fn update_len_prefixed(hasher: &mut Sha256, bytes: &[u8]) {
    debug_assert!(bytes.len() <= u8::MAX as usize, "length prefix overflow");
    hasher.update([bytes.len() as u8]);
    hasher.update(bytes);
}
