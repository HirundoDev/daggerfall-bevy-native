//! The read verb: keyset queries that never expose the store.
//!
//! Order is always ascending [`ContentId`] inside a kind; `after` is an
//! exclusive keyset cursor (never an offset), which keeps paging stable when an
//! overlay appears or disappears between pages.

use crate::error::ApiError;
use crate::id::{ContentId, ContentKind};
use crate::record::{AttrValue, Record};
use crate::schema::SchemaVersion;

/// Largest accepted page size.
pub const MAX_PAGE: usize = 200;

/// A read request. There is no "list everything" and no SQL surface.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Query {
    /// One record by exact id.
    ById {
        /// The id to resolve.
        id: ContentId,
    },
    /// Records of one kind, keyset-paginated.
    ByKind {
        /// Kind to return.
        kind: ContentKind,
        /// Exclusive lower bound; `None` starts at the first match.
        after: Option<ContentId>,
        /// Page size, `1..=MAX_PAGE`.
        limit: usize,
    },
    /// Records of one kind carrying an exact typed attribute value.
    ByAttr {
        /// Kind to return.
        kind: ContentKind,
        /// Attribute key.
        key: String,
        /// Exact value, variant included.
        value: AttrValue,
        /// Exclusive lower bound; `None` starts at the first match.
        after: Option<ContentId>,
        /// Page size, `1..=MAX_PAGE`.
        limit: usize,
    },
}

/// One page of results, stamped with the schema version that produced it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Page {
    /// Version of the contract that produced this page.
    pub schema_version: SchemaVersion,
    /// Records in ascending id order.
    pub records: Vec<Record>,
    /// Exclusive cursor for the next page; `Some` only when more matches remain.
    pub next: Option<ContentId>,
}

/// Validates a requested page size.
///
/// # Errors
///
/// [`ApiError::LimitOutOfRange`] when `limit` is `0` or greater than
/// [`MAX_PAGE`].
pub(crate) fn validate_limit(limit: usize) -> Result<(), ApiError> {
    if limit == 0 || limit > MAX_PAGE {
        return Err(ApiError::LimitOutOfRange {
            limit,
            max: MAX_PAGE,
        });
    }
    Ok(())
}
