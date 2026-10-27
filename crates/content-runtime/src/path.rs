//! Namespaced relative paths of a bundle payload.
//!
//! A bundle cannot write outside its own namespace: the first segment of every
//! path must be the bundle's side id (plan `I1` §3.4 rule 3). Two sides therefore
//! can never collide, and a manifest cannot address a file of another side or an
//! absolute location.

use daggerfall_content_api::SideId;

use crate::error::BundleError;

/// Largest accepted path, in bytes, relative to the payload root.
pub const MAX_PATH_BYTES: usize = 512;

/// Largest accepted single segment, in bytes.
pub const MAX_SEGMENT_BYTES: usize = 255;

/// A validated, relative, side-namespaced bundle path such as
/// `alpha/data/items.json`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BundlePath(String);

impl BundlePath {
    /// Validates `raw` against the relative-path rule and `side`'s namespace.
    ///
    /// Rejected: empty paths, more than [`MAX_PATH_BYTES`], a leading `/`, a
    /// backslash, control or space characters, empty segments, `.` or `..`
    /// segments, segments longer than [`MAX_SEGMENT_BYTES`] or containing
    /// anything outside `[A-Za-z0-9._-]`, and any path whose first segment is not
    /// exactly `side`.
    pub fn new(side: &SideId, raw: &str) -> Result<Self, BundleError> {
        let invalid = |reason: &'static str| BundleError::InvalidPath {
            raw: raw.to_string(),
            reason,
        };
        if raw.is_empty() {
            return Err(invalid("bundle path must not be empty"));
        }
        if raw.len() > MAX_PATH_BYTES {
            return Err(invalid("bundle path must be at most 512 bytes"));
        }
        if raw.starts_with('/') {
            return Err(invalid("bundle path must be relative"));
        }
        if raw.contains('\\') {
            return Err(invalid("bundle path must use `/` separators"));
        }
        if raw.chars().any(|c| c.is_control() || c == ' ') {
            return Err(invalid(
                "bundle path must not contain control or space characters",
            ));
        }
        match raw.split_once('/') {
            Some((head, _)) if head == side.as_str() => {}
            _ => {
                return Err(invalid(
                    "bundle path must start with `<side>/` for the bundle's own side",
                ))
            }
        }
        for segment in raw.split('/') {
            if segment.is_empty() {
                return Err(invalid("bundle path must not contain an empty segment"));
            }
            if segment == "." || segment == ".." {
                return Err(invalid("bundle path must not contain `.` or `..` segments"));
            }
            if segment.len() > MAX_SEGMENT_BYTES {
                return Err(invalid("every segment must be at most 255 bytes"));
            }
            if !segment
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'_' || b == b'-')
            {
                return Err(invalid("segments may only contain A-Za-z0-9._-"));
            }
        }
        Ok(BundlePath(raw.to_string()))
    }

    /// The canonical text of the path.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The `/`-separated segments, the first of which is the side id.
    pub fn segments(&self) -> impl Iterator<Item = &str> {
        self.0.split('/')
    }

    /// The last segment, i.e. the file name.
    pub fn file_name(&self) -> &str {
        self.0.rsplit('/').next().unwrap_or(&self.0)
    }
}

impl std::fmt::Display for BundlePath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
