//! Versioned schema of the content contract.
//!
//! [`SCHEMA_VERSION_LABEL`] is the single source of truth for the published
//! version; [`SURFACE`] enumerates every public type of the contract and is
//! asserted against the published documents by `tests/schema_published.rs`
//! (anti-drift). Negotiation is minor-tolerant and major-strict: a consumer of
//! the same major with a lower-or-equal minor is accepted.

use std::fmt;

use crate::error::ApiError;

/// Canonical text of the published schema version (`"M.m"`).
pub const SCHEMA_VERSION_LABEL: &str = "1.0";

/// One published public item of the contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SurfaceItem {
    /// Type name exactly as published in the documents.
    pub name: &'static str,
    /// `"type"`, `"enum"` or `"struct"`.
    pub kind: &'static str,
}

/// Every public item of the content contract, in documentation order.
///
/// `type` covers enums and structs consumed as a unit; the two finer kinds are
/// kept so the published tables can distinguish closed sets (`enum`) from
/// value objects (`struct`).
pub const SURFACE: &[SurfaceItem] = &[
    SurfaceItem { name: "ContentApi", kind: "struct" },
    SurfaceItem { name: "Baseline", kind: "struct" },
    SurfaceItem { name: "Record", kind: "struct" },
    SurfaceItem { name: "AttrValue", kind: "enum" },
    SurfaceItem { name: "ContentId", kind: "struct" },
    SurfaceItem { name: "ContentKind", kind: "enum" },
    SurfaceItem { name: "SideId", kind: "struct" },
    SurfaceItem { name: "Namespace", kind: "enum" },
    SurfaceItem { name: "Revision", kind: "struct" },
    SurfaceItem { name: "Query", kind: "enum" },
    SurfaceItem { name: "Page", kind: "struct" },
    SurfaceItem { name: "Overlay", kind: "struct" },
    SurfaceItem { name: "OverlayHandle", kind: "struct" },
    SurfaceItem { name: "SchemaVersion", kind: "struct" },
    SurfaceItem { name: "StateHash", kind: "struct" },
    SurfaceItem { name: "ApiError", kind: "enum" },
];

/// A negotiated schema version (`major.minor`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SchemaVersion {
    /// Breaking-change counter.
    pub major: u32,
    /// Additive-change counter within a major.
    pub minor: u32,
}

impl SchemaVersion {
    /// The version this build publishes.
    pub const CURRENT: SchemaVersion = SchemaVersion { major: 1, minor: 0 };

    /// Parses the canonical `"M.m"` form.
    ///
    /// Rejects missing or extra dots, empty components, non-ASCII-digit
    /// characters, `u32` overflow and leading zeros (so `"01.0"` is invalid but
    /// `"0.1"` parses). A parse failure surfaces as
    /// [`ApiError::InvalidRecord`] whose reason names the offending text.
    pub fn parse(raw: &str) -> Result<Self, ApiError> {
        let invalid = || ApiError::InvalidRecord {
            reason: format!("invalid schema version `{raw}`: expected canonical `M.m`"),
        };
        let (major_raw, minor_raw) = raw.split_once('.').ok_or_else(invalid)?;
        let major = parse_component(major_raw).ok_or_else(invalid)?;
        let minor = parse_component(minor_raw).ok_or_else(invalid)?;
        Ok(SchemaVersion { major, minor })
    }

    /// Canonical `"M.m"` text.
    pub fn label(&self) -> String {
        format!("{}.{}", self.major, self.minor)
    }

    /// Whether a consumer speaking `consumer` can be served by `self`.
    ///
    /// Same major and `consumer.minor <= self.minor` - additive minors are
    /// forward-compatible, majors never are.
    pub fn accepts(&self, consumer: SchemaVersion) -> bool {
        self.major == consumer.major && consumer.minor <= self.minor
    }
}

impl fmt::Display for SchemaVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}", self.major, self.minor)
    }
}

fn parse_component(raw: &str) -> Option<u32> {
    let bytes = raw.as_bytes();
    if bytes.is_empty() || !bytes.iter().all(u8::is_ascii_digit) {
        return None;
    }
    if bytes.len() > 1 && bytes[0] == b'0' {
        return None;
    }
    raw.parse::<u32>().ok()
}
