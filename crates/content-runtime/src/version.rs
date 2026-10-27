//! Version of a bundle and of the core it pins.

use std::fmt;

use crate::error::BundleError;

/// A numeric `major.minor.patch` version.
///
/// Used twice, for the same reason: a bundle is identified by its version and it
/// declares the exact core version it was built against (plan `I1` §7.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Version {
    /// Breaking-change counter.
    pub major: u32,
    /// Additive-change counter.
    pub minor: u32,
    /// Fix counter.
    pub patch: u32,
}

impl Version {
    /// Parses the strict `major.minor.patch` form.
    ///
    /// Three numeric components, no more and no fewer; leading zeros are
    /// rejected (`"01.0.0"` is invalid, `"0.1.0"` is valid), so a version has
    /// exactly one spelling. Anything else is
    /// [`BundleError::InvalidVersion`].
    pub fn parse(raw: &str) -> Result<Self, BundleError> {
        let invalid = |reason: &'static str| BundleError::InvalidVersion {
            raw: raw.to_string(),
            reason,
        };
        let mut parts = raw.split('.');
        let major = parts
            .next()
            .and_then(parse_component)
            .ok_or_else(|| invalid("major component must be a number without leading zeros"))?;
        let minor = parts
            .next()
            .and_then(parse_component)
            .ok_or_else(|| invalid("minor component must be a number without leading zeros"))?;
        let patch = parts
            .next()
            .and_then(parse_component)
            .ok_or_else(|| invalid("patch component must be a number without leading zeros"))?;
        if parts.next().is_some() {
            return Err(invalid(
                "version must have exactly three dot-separated components",
            ));
        }
        Ok(Version {
            major,
            minor,
            patch,
        })
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

/// Parses one numeric component: ASCII digits only, no leading zero unless the
/// component is exactly `"0"`, and it must fit a `u32`.
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
