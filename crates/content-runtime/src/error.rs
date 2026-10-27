//! Stable errors of the bundle contract.
//!
//! Every variant maps to exactly one lowercase code in [`ERROR_CODES`]. The code
//! is the first token of [`BundleError`]'s `Display` output, so a consumer can
//! branch on the failure without parsing prose (plan `I1` §3.4).
//!
//! The type is `Eq`: callers assert on exact values. The strings exist for humans
//! and for diagnostics only.

use std::fmt;

use daggerfall_content_api::SideId;

use crate::version::Version;

/// Stable, lowercase error codes, published in `CONTENT-RUNTIME-v1.md`.
///
/// The order is the order of the published table; `tests/surface_published.rs`
/// fails if the document drops or renames a code.
pub const ERROR_CODES: &[&str] = &[
    "invalid_side",
    "invalid_version",
    "invalid_path",
    "duplicate_path",
    "executable_payload",
    "version_conflict",
    "unsupported_schema",
    "manifest_mismatch",
    "entry_hash_mismatch",
    "entry_missing",
    "entry_unexpected",
    "incompatible_core",
    "io",
];

/// Every failure the bundle contract can produce.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BundleError {
    /// A side id did not match the `SideId` grammar of `daggerfall-content-api`.
    InvalidSide {
        /// The offending raw text.
        raw: String,
        /// Static, stable reason.
        reason: &'static str,
    },
    /// A version did not match the strict `major.minor.patch` grammar.
    InvalidVersion {
        /// The offending raw text.
        raw: String,
        /// Static, stable reason.
        reason: &'static str,
    },
    /// A bundle path violated the relative-path or namespacing rule.
    InvalidPath {
        /// The offending raw text.
        raw: String,
        /// Static, stable reason.
        reason: &'static str,
    },
    /// The same bundle path was declared twice.
    DuplicatePath {
        /// The repeated path.
        path: String,
    },
    /// The payload carries executable content or an executable file extension.
    ExecutablePayload {
        /// The offending path.
        path: String,
        /// Static, stable reason naming the detected magic or extension class.
        reason: &'static str,
    },
    /// The `(side, version)` already exists on disk with different content.
    VersionConflict {
        /// The side that was being published.
        side: SideId,
        /// The version that was being published.
        version: Version,
        /// Hash recorded by the already published bundle.
        existing: String,
        /// Hash of the bundle that was offered.
        incoming: String,
    },
    /// The manifest declares a schema this build does not speak.
    UnsupportedSchema {
        /// The declared schema version.
        declared: String,
        /// The schema version this build publishes.
        supported: &'static str,
    },
    /// The manifest bytes are not the canonical v1 manifest: not JSON, not the v1
    /// shape, or not byte-identical to its own canonical form.
    ///
    /// `Bundle::open` also uses this variant when `manifest.sha256` does not
    /// match `manifest.json`.
    ManifestMismatch {
        /// What was expected.
        expected: String,
        /// What was found.
        actual: String,
    },
    /// A payload entry does not hash to the value recorded in the manifest, or
    /// its size differs from the recorded size.
    EntryHashMismatch {
        /// The entry path.
        path: String,
        /// Recorded hash or recorded size.
        expected: String,
        /// Recomputed hash or recomputed size.
        actual: String,
    },
    /// A manifest entry has no file on disk (or in the payload).
    EntryMissing {
        /// The entry path.
        path: String,
    },
    /// A payload file is not described by the manifest.
    EntryUnexpected {
        /// The file path.
        path: String,
    },
    /// The bundle pins a core version other than the one the caller runs.
    IncompatibleCore {
        /// The core version the bundle requires.
        required: Version,
        /// The core version the caller declared.
        actual: Version,
    },
    /// A filesystem operation failed.
    Io {
        /// The static operation label, e.g. `"read manifest.json"`.
        operation: &'static str,
        /// The underlying message.
        message: String,
    },
}

impl fmt::Display for BundleError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BundleError::InvalidSide { raw, reason } => write!(f, "invalid_side: {raw} ({reason})"),
            BundleError::InvalidVersion { raw, reason } => {
                write!(f, "invalid_version: {raw} ({reason})")
            }
            BundleError::InvalidPath { raw, reason } => write!(f, "invalid_path: {raw} ({reason})"),
            BundleError::DuplicatePath { path } => write!(f, "duplicate_path: {path}"),
            BundleError::ExecutablePayload { path, reason } => {
                write!(f, "executable_payload: {path} ({reason})")
            }
            BundleError::VersionConflict {
                side,
                version,
                existing,
                incoming,
            } => write!(
                f,
                "version_conflict: {}@{version} is already published as {existing}, offered {incoming}",
                side.as_str()
            ),
            BundleError::UnsupportedSchema {
                declared,
                supported,
            } => write!(
                f,
                "unsupported_schema: declared {declared}, supported {supported}"
            ),
            BundleError::ManifestMismatch { expected, actual } => {
                write!(f, "manifest_mismatch: expected {expected}, actual {actual}")
            }
            BundleError::EntryHashMismatch {
                path,
                expected,
                actual,
            } => write!(f, "entry_hash_mismatch: {path} expected {expected}, actual {actual}"),
            BundleError::EntryMissing { path } => write!(f, "entry_missing: {path}"),
            BundleError::EntryUnexpected { path } => write!(f, "entry_unexpected: {path}"),
            BundleError::IncompatibleCore { required, actual } => write!(
                f,
                "incompatible_core: bundle requires core {required}, caller is {actual}"
            ),
            BundleError::Io { operation, message } => write!(f, "io: {operation}: {message}"),
        }
    }
}

impl std::error::Error for BundleError {}

/// Maps a `SideId::parse` failure onto the bundle contract's stable code.
pub(crate) fn map_side(error: daggerfall_content_api::ApiError) -> BundleError {
    match error {
        daggerfall_content_api::ApiError::InvalidSide { raw, reason } => {
            BundleError::InvalidSide { raw, reason }
        }
        other => BundleError::InvalidSide {
            raw: other.to_string(),
            reason: "side id rejected by the content-api grammar",
        },
    }
}

/// Wraps a filesystem failure under a static operation label.
pub(crate) fn io(operation: &'static str, error: std::io::Error) -> BundleError {
    BundleError::Io {
        operation,
        message: error.to_string(),
    }
}
