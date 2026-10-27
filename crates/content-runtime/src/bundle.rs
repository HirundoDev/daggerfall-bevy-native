//! Bundle assembly, offline verification and immutable publication.
//!
//! This module is the mechanism behind the four rules of plan `I1` §3.4:
//!
//! 1. **Immutable** - [`Bundle::publish`] refuses to touch a published
//!    `(side, version)` whose digest differs ([`BundleError::VersionConflict`]).
//! 2. **Offline-verifiable** - [`Bundle::open`] re-hashes what is on disk against
//!    the manifest; no network, no registry, no clock.
//! 3. **Namespaced** - [`BundlePath`] forces every payload path under the side id.
//! 4. **No executable payload** - [`crate::detect_executable`] is applied when a
//!    file is added and again on every [`Bundle::verify`].

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use daggerfall_content_api::SideId;

use crate::error::{io, BundleError};
use crate::executable::detect_executable;
use crate::manifest::{
    is_sha256_hex, sha256_hex, Manifest, ManifestEntry, MANIFEST_FILE, MANIFEST_HASH_FILE,
    PAYLOAD_DIR,
};
use crate::path::BundlePath;
use crate::version::Version;

/// Monotonic suffix for temporary publish directories.
///
/// Deliberately not a clock or a random source: publication must be reproducible
/// and the only requirement is that a leftover temp directory from a crashed
/// publish is never reused.
static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

/// Incremental constructor of a bundle.
///
/// Files are validated as they are added, so a builder cannot reach [`Self::build`]
/// holding a path that escapes its namespace or a payload that is executable.
#[derive(Debug, Clone)]
pub struct BundleBuilder {
    side: SideId,
    version: Version,
    compat_core: Version,
    files: BTreeMap<BundlePath, Vec<u8>>,
}

impl BundleBuilder {
    /// Starts a bundle for `side` at `version`, built against `compat_core`.
    pub fn new(side: SideId, version: Version, compat_core: Version) -> Self {
        BundleBuilder {
            side,
            version,
            compat_core,
            files: BTreeMap::new(),
        }
    }

    /// Adds one payload file.
    ///
    /// Fails with [`BundleError::InvalidPath`] when the path is not relative,
    /// not namespaced under this bundle's side, or otherwise malformed, with
    /// [`BundleError::DuplicatePath`] when the path was already added, and with
    /// [`BundleError::ExecutablePayload`] when the bytes or the extension declare
    /// executable content.
    pub fn add_file(&mut self, path: &str, bytes: Vec<u8>) -> Result<(), BundleError> {
        let path = BundlePath::new(&self.side, path)?;
        if self.files.contains_key(&path) {
            return Err(BundleError::DuplicatePath {
                path: path.as_str().to_string(),
            });
        }
        if let Some(reason) = detect_executable(&bytes, &path) {
            return Err(BundleError::ExecutablePayload {
                path: path.as_str().to_string(),
                reason,
            });
        }
        self.files.insert(path, bytes);
        Ok(())
    }

    /// The number of files added so far.
    pub fn file_count(&self) -> usize {
        self.files.len()
    }

    /// Produces the bundle and its canonical manifest.
    pub fn build(self) -> Bundle {
        let entries = self
            .files
            .iter()
            .map(|(path, bytes)| ManifestEntry {
                path: path.clone(),
                sha256: sha256_hex(bytes),
                size: bytes.len() as u64,
            })
            .collect();
        let manifest = Manifest::assemble(self.side, self.version, self.compat_core, entries);
        Bundle {
            manifest,
            payload: self.files,
        }
    }
}

/// An immutable bundle: canonical manifest plus payload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bundle {
    manifest: Manifest,
    payload: BTreeMap<BundlePath, Vec<u8>>,
}

/// Result of a successful [`Bundle::verify`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifyReport {
    /// Number of entries verified.
    pub entries: usize,
    /// Total payload bytes verified.
    pub bytes: u64,
    /// Digest of the canonical manifest.
    pub manifest_sha256: String,
}

/// Result of [`Bundle::publish`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PublishOutcome {
    /// The bundle was written and is now published.
    Published {
        /// Digest of the canonical manifest that was written.
        manifest_sha256: String,
    },
    /// The identical bundle was already published; nothing was written.
    AlreadyPublished,
}

impl Bundle {
    /// The canonical manifest.
    pub fn manifest(&self) -> &Manifest {
        &self.manifest
    }

    /// Digest of the canonical manifest.
    pub fn manifest_sha256(&self) -> String {
        self.manifest.sha256()
    }

    /// The manifest entry table, ascending by path.
    pub fn entries(&self) -> &[ManifestEntry] {
        self.manifest.entries()
    }

    /// The bytes of `path`, if the payload holds them.
    pub fn file(&self, path: &BundlePath) -> Option<&[u8]> {
        self.payload.get(path).map(Vec::as_slice)
    }

    /// Recomputes every digest and size, re-scans for executable content and
    /// checks the payload against the entry table.
    ///
    /// The manifest is part of the bundle value, so this also proves that a
    /// tampered payload cannot hide behind a stale manifest: any byte change is
    /// [`BundleError::EntryHashMismatch`], a missing file is
    /// [`BundleError::EntryMissing`] and an extra file is
    /// [`BundleError::EntryUnexpected`].
    pub fn verify(&self) -> Result<VerifyReport, BundleError> {
        let mut bytes_total: u64 = 0;
        for entry in self.manifest.entries() {
            let path = entry.path.as_str().to_string();
            let data = self
                .payload
                .get(&entry.path)
                .ok_or_else(|| BundleError::EntryMissing { path: path.clone() })?;
            let actual = sha256_hex(data);
            if actual != entry.sha256 {
                return Err(BundleError::EntryHashMismatch {
                    path,
                    expected: entry.sha256.clone(),
                    actual,
                });
            }
            let size = data.len() as u64;
            if size != entry.size {
                return Err(BundleError::EntryHashMismatch {
                    path,
                    expected: format!("size {}", entry.size),
                    actual: format!("size {size}"),
                });
            }
            if let Some(reason) = detect_executable(data, &entry.path) {
                return Err(BundleError::ExecutablePayload { path, reason });
            }
            bytes_total += size;
        }
        for path in self.payload.keys() {
            if self.manifest.entry(path).is_none() {
                return Err(BundleError::EntryUnexpected {
                    path: path.as_str().to_string(),
                });
            }
        }
        let entries_found = self.manifest.entries().len() as u64;
        if entries_found != self.manifest.entry_count() || bytes_total != self.manifest.byte_count() {
            return Err(BundleError::ManifestMismatch {
                expected: format!(
                    "counts entries {} bytes {}",
                    self.manifest.entry_count(),
                    self.manifest.byte_count()
                ),
                actual: format!("entries {entries_found} bytes {bytes_total}"),
            });
        }
        Ok(VerifyReport {
            entries: self.manifest.entries().len(),
            bytes: bytes_total,
            manifest_sha256: self.manifest.sha256(),
        })
    }

    /// Checks the exact core pin: the bundle's `compat.core` must equal `core`.
    ///
    /// The posture is an exact pin, not a range (plan `I1` §7.3): a bundle
    /// authored against one core is refused by any other, because the manifest
    /// format and the content schema move together.
    pub fn check_core(&self, core: &Version) -> Result<(), BundleError> {
        let required = self.manifest.compat_core();
        if required != *core {
            return Err(BundleError::IncompatibleCore {
                required,
                actual: *core,
            });
        }
        Ok(())
    }

    /// Publishes the bundle under `root`, atomically and immutably.
    ///
    /// Layout: `<root>/<side>/<version>/manifest.json`,
    /// `<root>/<side>/<version>/manifest.sha256` and
    /// `<root>/<side>/<version>/payload/<bundle path>`.
    ///
    /// The bundle is first verified, then written to a sibling temporary
    /// directory and finally renamed onto the target, so a reader never sees a
    /// half-written bundle. If the target already exists, an identical manifest
    /// digest yields [`PublishOutcome::AlreadyPublished`] with no write at all; a
    /// different digest yields [`BundleError::VersionConflict`] and leaves the
    /// published bytes untouched.
    pub fn publish(&self, root: &Path) -> Result<PublishOutcome, BundleError> {
        let report = self.verify()?;
        let side_dir = root.join(self.manifest.side().as_str());
        let target = side_dir.join(self.manifest.version().to_string());
        if target.exists() {
            return match existing_manifest_hash(&target) {
                Some(existing) if existing == report.manifest_sha256 => {
                    Ok(PublishOutcome::AlreadyPublished)
                }
                existing => Err(BundleError::VersionConflict {
                    side: self.manifest.side().clone(),
                    version: self.manifest.version(),
                    existing: existing.unwrap_or_else(|| "<unreadable>".to_string()),
                    incoming: report.manifest_sha256,
                }),
            };
        }

        fs::create_dir_all(&side_dir).map_err(|error| io("create side directory", error))?;
        let temporary = unique_temporary(&side_dir, &self.manifest.version().to_string());
        if let Err(error) = write_bundle(self, &temporary) {
            let _ = fs::remove_dir_all(&temporary);
            return Err(error);
        }
        match fs::rename(&temporary, &target) {
            Ok(()) => Ok(PublishOutcome::Published {
                manifest_sha256: report.manifest_sha256,
            }),
            Err(error) => {
                let _ = fs::remove_dir_all(&temporary);
                if target.exists() {
                    return match existing_manifest_hash(&target) {
                        Some(existing) if existing == report.manifest_sha256 => {
                            Ok(PublishOutcome::AlreadyPublished)
                        }
                        existing => Err(BundleError::VersionConflict {
                            side: self.manifest.side().clone(),
                            version: self.manifest.version(),
                            existing: existing.unwrap_or_else(|| "<unreadable>".to_string()),
                            incoming: report.manifest_sha256,
                        }),
                    };
                }
                Err(io("rename bundle into place", error))
            }
        }
    }

    /// Opens a published bundle directory and verifies it offline.
    ///
    /// Reads `manifest.json` and `manifest.sha256`, checks that the recorded
    /// digest matches the manifest bytes ([`BundleError::ManifestMismatch`]),
    /// parses the canonical manifest, walks `payload/` collecting every regular
    /// file, and then runs [`Bundle::verify`]. A manifest entry without a file is
    /// [`BundleError::EntryMissing`]; a file without an entry is
    /// [`BundleError::EntryUnexpected`]; a non-regular payload entry (for example
    /// a symlink) is [`BundleError::Io`], and so is a `payload/` root that is not
    /// a real directory - following such a symlink would read files outside the
    /// bundle and attest them with this bundle's digests.
    pub fn open(dir: &Path) -> Result<Bundle, BundleError> {
        let manifest_bytes = fs::read(dir.join(MANIFEST_FILE))
            .map_err(|error| io("read manifest.json", error))?;
        let recorded_bytes = fs::read(dir.join(MANIFEST_HASH_FILE))
            .map_err(|error| io("read manifest.sha256", error))?;
        let recorded = std::str::from_utf8(&recorded_bytes)
            .map_err(|error| {
                BundleError::ManifestMismatch {
                    expected: "manifest.sha256 is 64 lowercase hex characters".to_string(),
                    actual: format!("not UTF-8: {error}"),
                }
            })?
            .trim()
            .to_string();
        if !is_sha256_hex(&recorded) {
            return Err(BundleError::ManifestMismatch {
                expected: "manifest.sha256 is 64 lowercase hex characters".to_string(),
                actual: format!("`{recorded}`"),
            });
        }
        let computed = sha256_hex(&manifest_bytes);
        if computed != recorded {
            return Err(BundleError::ManifestMismatch {
                expected: recorded,
                actual: computed,
            });
        }

        let manifest = Manifest::parse(&manifest_bytes)?;
        let payload_dir = dir.join(PAYLOAD_DIR);
        let mut payload: BTreeMap<BundlePath, Vec<u8>> = BTreeMap::new();
        // The payload root must be a real directory. `is_dir` follows symlinks,
        // so a tampered bundle could point `payload/` at any directory on the
        // machine and have `open` read files outside the bundle as if they were
        // its own; the entry digests would then attest content the bundle does
        // not contain. `symlink_metadata` inspects the entry itself.
        match fs::symlink_metadata(&payload_dir) {
            Ok(metadata) if metadata.is_dir() => {
                let mut collected = Vec::new();
                collect_files(&payload_dir, "", &mut collected)?;
                for (relative, bytes) in collected {
                    let path = BundlePath::new(manifest.side(), &relative)?;
                    payload.insert(path, bytes);
                }
            }
            Ok(_) => {
                return Err(BundleError::Io {
                    operation: "payload root",
                    message: format!(
                        "`{}` is not a real directory; a symlink is not a payload root",
                        payload_dir.display()
                    ),
                })
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                // No payload directory: an empty bundle is legal, and any
                // declared entry is reported as `entry_missing` below.
            }
            Err(error) => return Err(io("inspect payload root", error)),
        }
        for entry in manifest.entries() {
            if !payload.contains_key(&entry.path) {
                return Err(BundleError::EntryMissing {
                    path: entry.path.as_str().to_string(),
                });
            }
        }
        for path in payload.keys() {
            if manifest.entry(path).is_none() {
                return Err(BundleError::EntryUnexpected {
                    path: path.as_str().to_string(),
                });
            }
        }
        let bundle = Bundle { manifest, payload };
        bundle.verify()?;
        Ok(bundle)
    }
}

/// Writes a bundle tree into `temporary` (which must not exist yet).
fn write_bundle(bundle: &Bundle, temporary: &Path) -> Result<(), BundleError> {
    fs::create_dir_all(temporary.join(PAYLOAD_DIR))
        .map_err(|error| io("create payload directory", error))?;
    for (path, bytes) in &bundle.payload {
        let mut destination = temporary.join(PAYLOAD_DIR);
        for segment in path.segments() {
            destination.push(segment);
        }
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent).map_err(|error| io("create payload directory", error))?;
        }
        fs::write(&destination, bytes).map_err(|error| io("write payload file", error))?;
    }
    fs::write(temporary.join(MANIFEST_FILE), bundle.manifest.canonical_bytes())
        .map_err(|error| io("write manifest.json", error))?;
    fs::write(
        temporary.join(MANIFEST_HASH_FILE),
        format!("{}\n", bundle.manifest.sha256()),
    )
    .map_err(|error| io("write manifest.sha256", error))?;
    Ok(())
}

/// A temporary sibling directory for `version` that is guaranteed not to exist.
fn unique_temporary(side_dir: &Path, version: &str) -> PathBuf {
    loop {
        let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let candidate = side_dir.join(format!(".tmp-{version}-{sequence}"));
        if !candidate.exists() {
            return candidate;
        }
    }
}

/// The digest recorded by an already published bundle directory.
fn existing_manifest_hash(target: &Path) -> Option<String> {
    if let Ok(text) = fs::read_to_string(target.join(MANIFEST_HASH_FILE)) {
        let trimmed = text.trim();
        if is_sha256_hex(trimmed) {
            return Some(trimmed.to_string());
        }
    }
    fs::read(target.join(MANIFEST_FILE))
        .ok()
        .map(|bytes| sha256_hex(&bytes))
}

/// Collects every regular file under `dir` as `(relative/path, bytes)`.
fn collect_files(
    dir: &Path,
    prefix: &str,
    out: &mut Vec<(String, Vec<u8>)>,
) -> Result<(), BundleError> {
    let entries = fs::read_dir(dir).map_err(|error| io("list payload directory", error))?;
    let mut paths = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|error| io("list payload directory", error))?;
        paths.push(entry.path());
    }
    paths.sort();
    for path in paths {
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| BundleError::Io {
                operation: "payload name",
                message: format!("payload path is not valid UTF-8: {}", path.display()),
            })?;
        let relative = if prefix.is_empty() {
            name.to_string()
        } else {
            format!("{prefix}/{name}")
        };
        let metadata = fs::symlink_metadata(&path)
            .map_err(|error| io("inspect payload entry", error))?;
        if metadata.file_type().is_symlink() {
            return Err(BundleError::Io {
                operation: "payload entry",
                message: format!("symlinks are not allowed in a payload: {relative}"),
            });
        }
        if metadata.is_dir() {
            collect_files(&path, &relative, out)?;
        } else if metadata.is_file() {
            let bytes = fs::read(&path).map_err(|error| io("read payload file", error))?;
            out.push((relative, bytes));
        } else {
            return Err(BundleError::Io {
                operation: "payload entry",
                message: format!("not a regular file: {relative}"),
            });
        }
    }
    Ok(())
}
