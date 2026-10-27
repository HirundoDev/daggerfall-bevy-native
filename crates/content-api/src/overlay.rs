//! The authoring verb: side-owned, additive overlays.
//!
//! An overlay is a named, bounded batch of records that all live in one side's
//! namespace. It can only add visibility - never edit, shadow or delete a
//! baseline record - and it is reversible: applying then removing it leaves the
//! world bit-identical (plan `I1` §3.2 rule 2).

use std::fmt;

use crate::error::ApiError;
use crate::id::{Namespace, SideId};
use crate::record::Record;

/// Maximum entries in one overlay.
pub const MAX_OVERLAY_ENTRIES: usize = 256;

/// Opaque handle of an applied overlay. Handles are per-[`crate::ContentApi`]
/// counters starting at `1`; they never appear in a state hash.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct OverlayHandle(u64);

impl OverlayHandle {
    /// Builds a handle from the API's counter (crate-internal only: consumers
    /// receive handles from `apply_overlay` and must not forge them).
    pub(crate) fn from_counter(counter: u64) -> Self {
        OverlayHandle(counter)
    }
}

impl fmt::Display for OverlayHandle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ov-{}", self.0)
    }
}

impl fmt::Debug for OverlayHandle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "OverlayHandle(ov-{})", self.0)
    }
}

/// A side-authored, additive content layer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Overlay {
    side: SideId,
    label: String,
    entries: Vec<Record>,
}

impl Overlay {
    /// Builds an overlay for `side`.
    ///
    /// # Errors
    ///
    /// - [`ApiError::InvalidRecord`] if `label` does not match
    ///   `^[a-z0-9][a-z0-9._-]{0,63}$` or `entries` is not `1..=MAX_OVERLAY_ENTRIES`.
    /// - [`ApiError::NamespaceViolation`] if any entry belongs to `base` or to
    ///   another side.
    /// - [`ApiError::DuplicateId`] if two entries share an id.
    pub fn new(side: SideId, label: &str, mut entries: Vec<Record>) -> Result<Self, ApiError> {
        if !is_valid_label(label) {
            return Err(ApiError::InvalidRecord {
                reason: format!(
                    "overlay label `{label}` must match ^[a-z0-9][a-z0-9._-]{{0,63}}$"
                ),
            });
        }
        if entries.is_empty() || entries.len() > MAX_OVERLAY_ENTRIES {
            return Err(ApiError::InvalidRecord {
                reason: format!("overlay must carry 1..={MAX_OVERLAY_ENTRIES} entries"),
            });
        }
        for entry in &entries {
            match entry.id().namespace() {
                Namespace::Side(owner) if owner == &side => {}
                _ => {
                    return Err(ApiError::NamespaceViolation {
                        id: entry.id().clone(),
                        side: side.clone(),
                    })
                }
            }
        }
        entries.sort_by(|left, right| left.id().cmp(right.id()));
        for pair in entries.windows(2) {
            if pair[0].id() == pair[1].id() {
                return Err(ApiError::DuplicateId {
                    id: pair[0].id().clone(),
                });
            }
        }
        debug_assert!(entries
            .iter()
            .all(|entry| matches!(entry.id().namespace(), Namespace::Side(owner) if owner == &side)));
        Ok(Overlay {
            side,
            label: label.to_string(),
            entries,
        })
    }

    /// The owning side.
    pub fn side(&self) -> &SideId {
        &self.side
    }

    /// The author's label for this layer.
    pub fn label(&self) -> &str {
        &self.label
    }

    /// Entries in ascending [`crate::ContentId`] order.
    pub fn entries(&self) -> &[Record] {
        &self.entries
    }
}

fn is_valid_label(label: &str) -> bool {
    let bytes = label.as_bytes();
    if bytes.is_empty() || bytes.len() > 64 {
        return false;
    }
    if !(bytes[0].is_ascii_lowercase() || bytes[0].is_ascii_digit()) {
        return false;
    }
    bytes[1..].iter().all(|b| {
        b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'.' || *b == b'_' || *b == b'-'
    })
}
