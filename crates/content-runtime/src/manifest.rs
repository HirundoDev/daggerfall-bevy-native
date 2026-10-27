//! The canonical bundle manifest.
//!
//! `manifest.json` is the whole identity of a bundle: side, version, the exact
//! core version it was built against, the entry table (path + SHA-256 + size) and
//! the totals. It is written in one canonical form - sorted keys, sorted entries,
//! no whitespace - so two independently built identical bundles hash identically
//! and so "this file is canonical" is a property that `parse` checks instead of a
//! convention that reviewers hope for.
//!
//! Schema version: [`SCHEMA_VERSION_LABEL`] (`"1.0"`).

use daggerfall_content_api::SideId;
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};

use crate::error::{map_side, BundleError};
use crate::path::BundlePath;
use crate::version::Version;

/// The manifest schema version this build writes and accepts.
pub const SCHEMA_VERSION_LABEL: &str = "1.0";

/// File name of the manifest inside a bundle directory.
pub const MANIFEST_FILE: &str = "manifest.json";

/// File name of the recorded manifest digest inside a bundle directory.
pub const MANIFEST_HASH_FILE: &str = "manifest.sha256";

/// Directory name of the payload inside a bundle directory.
pub const PAYLOAD_DIR: &str = "payload";

/// The exact engine version the whole graph is pinned to (plan `I1` §7.3).
///
/// Declared, never resolved: a bundle records which engine build it was authored
/// against so a later reader can refuse a mismatched pair without guessing.
pub const ENGINE_PIN: &str = "0.20.0";

/// JSON key of the engine pin inside `compat`.
///
/// A plain literal. The "zero engine dependency" claim of this crate is about
/// its build graph, and `tests/surface_published.rs` measures it in
/// `Cargo.toml`; the sources are scanned for actual behaviour (network and
/// process execution), never for a name that a manifest legitimately publishes.
pub const COMPAT_ENGINE_KEY: &str = "bevy";

/// One entry of the manifest: a payload file, its digest and its size.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManifestEntry {
    /// Validated, side-namespaced path.
    pub path: BundlePath,
    /// Lowercase hex SHA-256 of the payload bytes.
    pub sha256: String,
    /// Payload size in bytes.
    pub size: u64,
}

/// The canonical manifest of one bundle.
///
/// Constructed by [`crate::BundleBuilder::build`] or by [`Manifest::parse`];
/// every field is private because the canonical form is what makes the digest
/// meaningful.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Manifest {
    side: SideId,
    version: Version,
    compat_core: Version,
    counts_entries: u64,
    counts_bytes: u64,
    entries: Vec<ManifestEntry>,
}

impl Manifest {
    /// Assembles a manifest from an entry table, sorting entries by path and
    /// recomputing the totals.
    ///
    /// `entries` must not contain the same path twice; the builder and `parse`
    /// both enforce that before calling this.
    pub(crate) fn assemble(
        side: SideId,
        version: Version,
        compat_core: Version,
        mut entries: Vec<ManifestEntry>,
    ) -> Self {
        entries.sort_by(|a, b| a.path.cmp(&b.path));
        let counts_bytes = entries.iter().map(|entry| entry.size).sum();
        let counts_entries = entries.len() as u64;
        Manifest {
            side,
            version,
            compat_core,
            counts_entries,
            counts_bytes,
            entries,
        }
    }

    /// The owning side.
    pub fn side(&self) -> &SideId {
        &self.side
    }

    /// The bundle version.
    pub fn version(&self) -> Version {
        self.version
    }

    /// The canonical bundle id, `<side>@<version>`.
    pub fn bundle_id(&self) -> String {
        format!("{}@{}", self.side.as_str(), self.version)
    }

    /// The exact core version the bundle was built against.
    pub fn compat_core(&self) -> Version {
        self.compat_core
    }

    /// The engine pin this build declares ([`ENGINE_PIN`]).
    pub fn compat_engine(&self) -> &'static str {
        ENGINE_PIN
    }

    /// Declared entry count.
    pub fn entry_count(&self) -> u64 {
        self.counts_entries
    }

    /// Declared payload byte total.
    pub fn byte_count(&self) -> u64 {
        self.counts_bytes
    }

    /// The entry table, ascending by path.
    pub fn entries(&self) -> &[ManifestEntry] {
        &self.entries
    }

    /// The entry for `path`, if the manifest declares one.
    ///
    /// Binary search over the sorted entry table.
    pub fn entry(&self, path: &BundlePath) -> Option<&ManifestEntry> {
        self.entries
            .binary_search_by(|entry| entry.path.cmp(path))
            .ok()
            .map(|index| &self.entries[index])
    }

    /// The canonical bytes of the manifest.
    ///
    /// Compact JSON with keys inserted in ascending order: `bundle_id`, `compat`,
    /// `counts`, `entries`, `schema_version`, `side`, `version`; inside `compat`
    /// the engine pin precedes `core`; inside `counts` `bytes` precedes
    /// `entries`; each entry is `path`, `sha256`, `size`. Entries ascend by path.
    /// There is no trailing newline.
    pub fn canonical_bytes(&self) -> Vec<u8> {
        let mut compat = Map::new();
        compat.insert(
            COMPAT_ENGINE_KEY.to_string(),
            Value::String(ENGINE_PIN.to_string()),
        );
        compat.insert(
            "core".to_string(),
            Value::String(self.compat_core.to_string()),
        );

        let mut counts = Map::new();
        counts.insert("bytes".to_string(), Value::from(self.counts_bytes));
        counts.insert("entries".to_string(), Value::from(self.counts_entries));

        let entries: Vec<Value> = self
            .entries
            .iter()
            .map(|entry| {
                let mut item = Map::new();
                item.insert(
                    "path".to_string(),
                    Value::String(entry.path.as_str().to_string()),
                );
                item.insert("sha256".to_string(), Value::String(entry.sha256.clone()));
                item.insert("size".to_string(), Value::from(entry.size));
                Value::Object(item)
            })
            .collect();

        let mut root = Map::new();
        root.insert("bundle_id".to_string(), Value::String(self.bundle_id()));
        root.insert("compat".to_string(), Value::Object(compat));
        root.insert("counts".to_string(), Value::Object(counts));
        root.insert("entries".to_string(), Value::Array(entries));
        root.insert(
            "schema_version".to_string(),
            Value::String(SCHEMA_VERSION_LABEL.to_string()),
        );
        root.insert(
            "side".to_string(),
            Value::String(self.side.as_str().to_string()),
        );
        root.insert(
            "version".to_string(),
            Value::String(self.version.to_string()),
        );

        serde_json::to_vec(&Value::Object(root)).expect("a JSON value always serialises")
    }

    /// Lowercase hex SHA-256 of [`Manifest::canonical_bytes`].
    pub fn sha256(&self) -> String {
        sha256_hex(&self.canonical_bytes())
    }

    /// Parses `bytes` as a canonical v1 manifest.
    ///
    /// Validation order: JSON shape, `schema_version`, side grammar, version
    /// grammar, `bundle_id`, `compat`, entry paths and digests, `counts`, entry
    /// uniqueness, and finally the canonical check - the parsed manifest is
    /// re-serialised and must equal `bytes`. That last step is what makes any
    /// other ordering or formatting [`BundleError::ManifestMismatch`] instead of
    /// silently accepted.
    pub fn parse(bytes: &[u8]) -> Result<Self, BundleError> {
        let value: Value = serde_json::from_slice(bytes).map_err(|error| {
            mismatch(
                "a v1 manifest JSON document",
                format!("unparsable bytes: {error}"),
            )
        })?;
        let root = value.as_object().ok_or_else(|| {
            mismatch(
                "a v1 manifest JSON object",
                format!("found {}", kind_of(&value)),
            )
        })?;

        let schema = field_str(root, "schema_version")?;
        if schema != SCHEMA_VERSION_LABEL {
            return Err(BundleError::UnsupportedSchema {
                declared: schema.to_string(),
                supported: SCHEMA_VERSION_LABEL,
            });
        }

        let side = SideId::parse(field_str(root, "side")?).map_err(map_side)?;
        let version = Version::parse(field_str(root, "version")?)?;

        let bundle_id = field_str(root, "bundle_id")?;
        let expected_id = format!("{}@{version}", side.as_str());
        if bundle_id != expected_id {
            return Err(mismatch(
                "`bundle_id` = `<side>@<version>`",
                format!("`{bundle_id}` instead of `{expected_id}`"),
            ));
        }

        let compat = field_object(root, "compat")?;
        let engine = field_str(compat, COMPAT_ENGINE_KEY)?;
        if engine != ENGINE_PIN {
            return Err(mismatch(
                &format!("`compat.{COMPAT_ENGINE_KEY}` = \"{ENGINE_PIN}\""),
                format!("`{engine}`"),
            ));
        }
        let compat_core = Version::parse(field_str(compat, "core")?)?;

        let counts = field_object(root, "counts")?;
        let declared_entries = field_u64(counts, "entries")?;
        let declared_bytes = field_u64(counts, "bytes")?;

        let entries_value = field(root, "entries")?;
        let array = entries_value.as_array().ok_or_else(|| {
            mismatch(
                "`entries` is a JSON array",
                format!("found {}", kind_of(entries_value)),
            )
        })?;
        let mut parsed = Vec::with_capacity(array.len());
        for item in array {
            let object = item.as_object().ok_or_else(|| {
                mismatch(
                    "every `entries` element is a JSON object",
                    format!("found {}", kind_of(item)),
                )
            })?;
            let path = BundlePath::new(&side, field_str(object, "path")?)?;
            let digest = field_str(object, "sha256")?;
            if !is_sha256_hex(digest) {
                return Err(mismatch(
                    "`sha256` is 64 lowercase hex characters",
                    format!("`{digest}`"),
                ));
            }
            parsed.push(ManifestEntry {
                path,
                sha256: digest.to_string(),
                size: field_u64(object, "size")?,
            });
        }

        parsed.sort_by(|a, b| a.path.cmp(&b.path));
        for pair in parsed.windows(2) {
            if pair[0].path == pair[1].path {
                return Err(BundleError::DuplicatePath {
                    path: pair[0].path.as_str().to_string(),
                });
            }
        }

        let candidate = Manifest::assemble(side, version, compat_core, parsed);
        if candidate.counts_entries != declared_entries {
            return Err(mismatch(
                "`counts.entries` equal to the number of entries",
                format!(
                    "{declared_entries} instead of {}",
                    candidate.counts_entries
                ),
            ));
        }
        if candidate.counts_bytes != declared_bytes {
            return Err(mismatch(
                "`counts.bytes` equal to the sum of entry sizes",
                format!("{declared_bytes} instead of {}", candidate.counts_bytes),
            ));
        }
        if candidate.canonical_bytes() != bytes {
            return Err(mismatch(
                "canonical v1 manifest bytes (sorted keys, sorted entries, no whitespace)",
                "input bytes differ from their canonical form".to_string(),
            ));
        }
        Ok(candidate)
    }
}

/// Lowercase hex SHA-256 of `bytes`.
pub(crate) fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut hex = String::with_capacity(digest.len() * 2);
    for byte in digest {
        hex.push_str(&format!("{byte:02x}"));
    }
    hex
}

/// Whether `value` is 64 lowercase hex characters.
pub(crate) fn is_sha256_hex(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

/// Builds a [`BundleError::ManifestMismatch`] with a static expectation.
fn mismatch(expected: &str, actual: String) -> BundleError {
    BundleError::ManifestMismatch {
        expected: expected.to_string(),
        actual,
    }
}

/// A human description of a JSON value's shape, for diagnostics only.
fn kind_of(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "a boolean",
        Value::Number(_) => "a number",
        Value::String(_) => "a string",
        Value::Array(_) => "an array",
        Value::Object(_) => "an object",
    }
}

/// Fetches a required field from a JSON object.
fn field<'a>(object: &'a Map<String, Value>, key: &str) -> Result<&'a Value, BundleError> {
    object
        .get(key)
        .ok_or_else(|| mismatch(&format!("a v1 manifest field `{key}`"), "missing".to_string()))
}

/// Fetches a required string field.
fn field_str<'a>(object: &'a Map<String, Value>, key: &str) -> Result<&'a str, BundleError> {
    let value = field(object, key)?;
    value.as_str().ok_or_else(|| {
        mismatch(
            &format!("`{key}` is a JSON string"),
            format!("found {}", kind_of(value)),
        )
    })
}

/// Fetches a required unsigned integer field.
fn field_u64(object: &Map<String, Value>, key: &str) -> Result<u64, BundleError> {
    let value = field(object, key)?;
    value.as_u64().ok_or_else(|| {
        mismatch(
            &format!("`{key}` is a non-negative JSON integer"),
            format!("found {}", kind_of(value)),
        )
    })
}

/// Fetches a required object field.
fn field_object<'a>(
    object: &'a Map<String, Value>,
    key: &str,
) -> Result<&'a Map<String, Value>, BundleError> {
    let value = field(object, key)?;
    value.as_object().ok_or_else(|| {
        mismatch(
            &format!("`{key}` is a JSON object"),
            format!("found {}", kind_of(value)),
        )
    })
}
