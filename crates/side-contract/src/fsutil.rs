//! Shared byte-level helpers: sha256, canonical JSON, ordered tree walks and
//! fsync'd writes. No clock, no environment, no locale: everything here is
//! deterministic by construction.

use std::collections::BTreeMap;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use serde_json::Value;
use sha2::{Digest, Sha256};

/// Lowercase hex sha256 of `bytes` (same helper as `promotion-gate`).
pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// Canonical serialization of a JSON value: compact, lexicographic keys at
/// every level, `/` unescaped, non-ASCII raw. **No** trailing newline — callers
/// add exactly one when they materialize a file.
pub fn canonical_string(value: &Value) -> String {
    serde_json::to_string(value).expect("Value serializes")
}

/// Canonical file bytes: canonical form plus exactly one final `\n` (R2, §8.2).
pub fn canonical_line(value: &Value) -> String {
    let mut text = canonical_string(value);
    text.push('\n');
    text
}

/// How an entry of a walk is stored on disk (never followed while walking).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryKind {
    File,
    Dir,
    /// Symlink (kept as an entry; never recursed into, so no loops).
    Symlink,
    /// FIFO, socket, device… never read.
    Other,
}

/// One walked entry: POSIX-relative path plus its kind.
#[derive(Debug, Clone)]
pub struct Walked {
    pub rel: String,
    pub path: PathBuf,
    pub kind: EntryKind,
}

/// Recursively lists every non-directory entry under `root` with its
/// `/`-relative path, **sorted by UTF-8 bytes** (R5). Symlinks are never
/// followed while walking; directories named by a symlink are not recursed.
pub fn walk_entries(root: &Path) -> io::Result<Vec<Walked>> {
    let mut out = Vec::new();
    walk_rec(root, "", &mut out)?;
    out.sort_by(|a, b| a.rel.as_bytes().cmp(b.rel.as_bytes()));
    Ok(out)
}

fn walk_rec(dir: &Path, prefix: &str, out: &mut Vec<Walked>) -> io::Result<()> {
    let mut entries: Vec<fs::DirEntry> = fs::read_dir(dir)?.collect::<Result<Vec<_>, _>>()?;
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        let name = entry.file_name().to_string_lossy().into_owned();
        let rel = if prefix.is_empty() {
            name
        } else {
            format!("{prefix}/{name}")
        };
        let meta = fs::symlink_metadata(entry.path())?;
        let file_type = meta.file_type();
        if file_type.is_dir() {
            walk_rec(&entry.path(), &rel, out)?;
        } else {
            let kind = if file_type.is_symlink() {
                EntryKind::Symlink
            } else if file_type.is_file() {
                EntryKind::File
            } else {
                EntryKind::Other
            };
            out.push(Walked {
                rel,
                path: entry.path(),
                kind,
            });
        }
    }
    Ok(())
}

/// Sorted list of relative file paths in the tree (convenience over
/// [`walk_entries`]).
pub fn walk_rel_files(root: &Path) -> io::Result<Vec<String>> {
    Ok(walk_entries(root)?.into_iter().map(|w| w.rel).collect())
}

/// True when the directory exists and contains at least one entry.
pub fn dir_is_nonempty(dir: &Path) -> io::Result<bool> {
    match fs::read_dir(dir) {
        Ok(mut entries) => match entries.next() {
            Some(Ok(_)) => Ok(true),
            Some(Err(e)) => Err(e),
            None => Ok(false),
        },
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(e),
    }
}

/// Writes `bytes` to `path` (creating parents) and fsyncs the file.
pub fn write_fsync(path: &Path, bytes: &[u8]) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(path)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    Ok(())
}

/// fsyncs a directory so a rename/create inside it is durable.
pub fn fsync_dir(path: &Path) -> io::Result<()> {
    fs::File::open(path)?.sync_all()
}

/// Deterministic I/O detail for `internal_error`: only the `ErrorKind`,
/// never a path (paths may be absolute and the row must stay reproducible).
pub fn io_detail(error: &io::Error) -> String {
    format!("error de IO ({:?})", error.kind())
}

/// Counts of a canonical bundle built in memory: every file except the
/// manifest itself (M1).
pub type Payload = BTreeMap<String, Vec<u8>>;

/// Serializes a map of canonical documents to bytes for comparison.
pub fn payload_fingerprint(payload: &Payload, contract_json: &[u8]) -> String {
    let mut hasher = Sha256::new();
    for (path, bytes) in payload {
        hasher.update(path.as_bytes());
        hasher.update([0]);
        hasher.update(bytes);
        hasher.update([0]);
    }
    hasher.update(contract_json);
    hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
