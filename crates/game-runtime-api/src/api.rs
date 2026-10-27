//! Version label and negotiation of the runtime contract.
//!
//! Negotiation is major-strict and minor-tolerant, mirroring
//! [`daggerfall_content_api::SchemaVersion`]: a client of the same major with a
//! lower-or-equal minor is served, anything else receives a typed
//! [`RpcError::UnsupportedApi`] before the server looks up a method. An old
//! client therefore never gets a different behaviour, only a named refusal.

use std::fmt;

use crate::error::RpcError;

/// Canonical text of the published API version (`"M.m"`).
pub const API_LABEL: &str = "1.0";

/// A negotiated API version (`major.minor`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ApiVersion {
    /// Breaking-change counter.
    pub major: u32,
    /// Additive-change counter within a major.
    pub minor: u32,
}

impl ApiVersion {
    /// The version this build publishes.
    pub const CURRENT: ApiVersion = ApiVersion { major: 1, minor: 0 };

    /// Canonical text of [`ApiVersion::CURRENT`].
    pub const LABEL: &'static str = API_LABEL;

    /// Parses the canonical `"M.m"` form.
    ///
    /// Rejects a missing or extra dot, empty components, non-ASCII digits,
    /// `u32` overflow and leading zeros (so `"01.0"` is invalid but `"0.1"`
    /// parses). A parse failure is already the wire refusal: it surfaces as
    /// [`RpcError::UnsupportedApi`] naming the offending text.
    pub fn parse(raw: &str) -> Result<Self, RpcError> {
        let unsupported = || RpcError::UnsupportedApi {
            declared: raw.to_string(),
            supported: API_LABEL.to_string(),
        };
        let (major, minor) = raw.split_once('.').ok_or_else(unsupported)?;
        Ok(ApiVersion {
            major: parse_component(major).ok_or_else(unsupported)?,
            minor: parse_component(minor).ok_or_else(unsupported)?,
        })
    }

    /// Canonical `"M.m"` text.
    pub fn label(&self) -> String {
        format!("{}.{}", self.major, self.minor)
    }

    /// Whether a client speaking `declared` can be served by `self`.
    ///
    /// Same major and `declared.minor <= self.minor`: additive minors are
    /// forward-compatible, majors never are.
    pub const fn accepts(&self, declared: ApiVersion) -> bool {
        self.major == declared.major && declared.minor <= self.minor
    }
}

impl fmt::Display for ApiVersion {
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
