//! Stable errors of the content contract.
//!
//! Every variant maps to exactly one lowercase code in [`ERROR_CODES`]. The code
//! is the first token of [`ApiError`]'s `Display` output, so a consumer can
//! branch on the failure without parsing prose (plan `I1` §3.2 rule 4).

use std::fmt;

use crate::id::{ContentId, SideId};
use crate::overlay::OverlayHandle;
use crate::schema::SchemaVersion;

/// Stable, lowercase error codes published as `x-errors` in the schema.
pub const ERROR_CODES: &[&str] = &[
    "invalid_id",
    "invalid_side",
    "invalid_record",
    "duplicate_id",
    "id_reuse",
    "namespace_violation",
    "not_found",
    "limit_out_of_range",
    "too_many_overlays",
    "overlay_unknown",
    "unsupported_schema",
];

/// Every failure the content contract can produce.
///
/// The type is `Eq`, so callers may assert on exact values instead of matching
/// strings - the strings exist for humans and for the JSON projection only.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApiError {
    /// Raw text did not match the `dfc1:` id grammar.
    InvalidId {
        /// The offending raw text.
        raw: String,
        /// Static, stable reason.
        reason: &'static str,
    },
    /// Raw text did not match the side-id grammar or is reserved.
    InvalidSide {
        /// The offending raw text.
        raw: String,
        /// Static, stable reason.
        reason: &'static str,
    },
    /// A record (or an overlay label / entry count) violated its invariants.
    ///
    /// This is also the variant used for an unparsable [`SchemaVersion`], with a
    /// reason naming the offending text.
    InvalidRecord {
        /// Human-readable reason; never used for control flow.
        reason: String,
    },
    /// The same id was declared twice in a scope that requires uniqueness.
    DuplicateId {
        /// The repeated id.
        id: ContentId,
    },
    /// An id previously retired by [`crate::ContentApi::remove_overlay`] was
    /// offered again. Ids are never recycled (plan `I1` §3.2 rule 1).
    IdReuse {
        /// The recycled id.
        id: ContentId,
    },
    /// A record carried a namespace that its container does not own.
    NamespaceViolation {
        /// The offending record id.
        id: ContentId,
        /// The side that owns the container (the baseline is not a side).
        side: SideId,
    },
    /// No record with the requested id is visible.
    NotFound {
        /// The id that resolved to nothing.
        id: ContentId,
    },
    /// A page limit fell outside `1..=MAX_PAGE`.
    LimitOutOfRange {
        /// The requested limit.
        limit: usize,
        /// The largest accepted limit.
        max: usize,
    },
    /// The overlay budget of one [`crate::ContentApi`] is exhausted.
    TooManyOverlays {
        /// The largest accepted number of simultaneously applied overlays.
        max: usize,
    },
    /// The handle is not currently applied (including a double remove).
    OverlayUnknown {
        /// The handle that resolved to nothing.
        handle: OverlayHandle,
    },
    /// The consumer speaks a schema this build cannot negotiate.
    UnsupportedSchema {
        /// The version the consumer declared.
        declared: SchemaVersion,
        /// The version this build publishes.
        supported: SchemaVersion,
    },
}

impl fmt::Display for ApiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ApiError::InvalidId { raw, reason } => write!(f, "invalid_id: {raw} ({reason})"),
            ApiError::InvalidSide { raw, reason } => write!(f, "invalid_side: {raw} ({reason})"),
            ApiError::InvalidRecord { reason } => write!(f, "invalid_record: {reason}"),
            ApiError::DuplicateId { id } => write!(f, "duplicate_id: {id}"),
            ApiError::IdReuse { id } => write!(f, "id_reuse: {id}"),
            ApiError::NamespaceViolation { id, side } => {
                write!(f, "namespace_violation: {id} (side `{}`)", side.as_str())
            }
            ApiError::NotFound { id } => write!(f, "not_found: {id}"),
            ApiError::LimitOutOfRange { limit, max } => {
                write!(f, "limit_out_of_range: {limit} (max {max})")
            }
            ApiError::TooManyOverlays { max } => write!(f, "too_many_overlays: max {max}"),
            ApiError::OverlayUnknown { handle } => write!(f, "overlay_unknown: {handle}"),
            ApiError::UnsupportedSchema {
                declared,
                supported,
            } => write!(f, "unsupported_schema: declared {declared}, supported {supported}"),
        }
    }
}

impl std::error::Error for ApiError {}
