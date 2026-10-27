//! The side-side of the contract: reading `side_id`, the read-only scan of
//! `<side>/.side-contract/` (P7/P8/P9) and the atomic `--sync` write sequence
//! of DESIGN-v1 §10.6 (F6-B pack §8).
//!
//! `--check` never opens a descriptor for writing on the side (§10.7): every
//! helper here used by the checker is read-only. Only `perform_sync` writes,
//! and it does so in the mandatory order: tmp tree + fsync → rename old aside
//! → rename tmp into place → `contract.lock` last (commit point) → drop `.old`.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

use crate::bundle::ManifestFileEntry;
use crate::fsutil::{fsync_dir, io_detail, sha256_hex, walk_entries, write_fsync, Payload};
use crate::lockfile::{Lock, LockFileEntry};
use crate::taxonomy::Cause;
use crate::verdict::{FilesCounts, Obs};
use crate::Stop;

/// Reads `[side] id` from `<side>/side-project.toml` best effort: any failure
/// yields `None` (the row carries `side_id: null`, never a crash).
pub fn read_side_id(side: &Path) -> Option<String> {
    let bytes = fs::read(side.join("side-project.toml")).ok()?;
    let text = String::from_utf8(bytes).ok()?;
    let value: toml::Value = toml::from_str(&text).ok()?;
    value.get("side")?.get("id")?.as_str().map(str::to_string)
}

/// Outcome of the read-only `.side-contract/` scan.
#[derive(Debug, Clone, Default)]
pub struct SideScan {
    pub obs: Vec<Obs>,
    pub files: FilesCounts,
}

fn record_missing(scan: &mut SideScan, rel: &str) {
    scan.files.missing += 1;
    scan.obs.push(Obs::new(
        Cause::FileMissing,
        format!("falta .side-contract/{rel}"),
    ));
}

fn record_drift(scan: &mut SideScan, rel: &str) {
    scan.files.drifted += 1;
    scan.obs.push(Obs::new(
        Cause::FileDrift,
        format!("bytes distintos en .side-contract/{rel}"),
    ));
}

fn check_one(tree: &Path, rel: &str, expected_sha: &str, scan: &mut SideScan) -> Result<(), Stop> {
    let path = tree.join(rel);
    // `metadata` follows symlinks: the copy must resolve to a regular file
    // whose bytes hash to the lock digest; anything else is a local edit.
    let metadata = match fs::metadata(&path) {
        Ok(metadata) => metadata,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            record_missing(scan, rel);
            return Ok(());
        }
        Err(e) => return Err(Stop::Internal(io_detail(&e))),
    };
    if !metadata.is_file() {
        record_drift(scan, rel);
        return Ok(());
    }
    match fs::read(&path) {
        Ok(bytes) => {
            if sha256_hex(&bytes) != expected_sha {
                record_drift(scan, rel);
            }
            Ok(())
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            record_missing(scan, rel);
            Ok(())
        }
        Err(e) => Err(Stop::Internal(io_detail(&e))),
    }
}

/// Read-only evaluation of `<side>/.side-contract/` against the lock.
///
/// `CONTRACT.json` is expected even though the lock does not list it: it is
/// identified by `lock.bundle_sha256` (§9.3). Everything else that the lock
/// does not record is `file_unexpected`.
pub fn scan_side(side: &Path, lock: &Lock) -> Result<SideScan, Stop> {
    let tree = side.join(".side-contract");
    let walked = match walk_entries(&tree) {
        Ok(walked) => walked,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Vec::new(),
        Err(e) => return Err(Stop::Internal(io_detail(&e))),
    };

    let mut scan = SideScan {
        obs: Vec::new(),
        files: FilesCounts {
            checked: lock.files.len(),
            ..FilesCounts::default()
        },
    };

    let mut expected: BTreeSet<&str> = BTreeSet::new();
    for entry in &lock.files {
        expected.insert(entry.path.as_str());
        check_one(&tree, &entry.path, &entry.sha256, &mut scan)?;
    }
    expected.insert("CONTRACT.json");
    check_one(&tree, "CONTRACT.json", &lock.bundle_sha256, &mut scan)?;

    for entry in &walked {
        if expected.contains(entry.rel.as_str()) {
            continue;
        }
        scan.files.unexpected += 1;
        scan.obs.push(Obs::new(
            Cause::FileUnexpected,
            format!("archivo no registrado: .side-contract/{}", entry.rel),
        ));
    }
    Ok(scan)
}

/// Per-file expected/actual digests for `--diff`, ordered by path. The lock's
/// files come first in byte order, then any unexpected actual file.
pub struct DiffEntry {
    pub path: String,
    pub expected: Option<String>,
    pub actual: Option<String>,
}

/// Computes the diff table of a side against its lock (read-only).
pub fn diff_entries(side: &Path, lock: &Lock) -> Result<Vec<DiffEntry>, Stop> {
    let tree = side.join(".side-contract");
    let walked = match walk_entries(&tree) {
        Ok(walked) => walked,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Vec::new(),
        Err(e) => return Err(Stop::Internal(io_detail(&e))),
    };
    let actuals: BTreeMap<&str, Option<String>> = walked
        .iter()
        .map(|entry| {
            let sha = match fs::read(&entry.path) {
                Ok(bytes) => Some(sha256_hex(&bytes)),
                Err(_) => None,
            };
            (entry.rel.as_str(), sha)
        })
        .collect();

    let mut rows: Vec<DiffEntry> = Vec::new();
    for entry in &lock.files {
        rows.push(DiffEntry {
            path: entry.path.clone(),
            expected: Some(entry.sha256.clone()),
            actual: actuals.get(entry.path.as_str()).cloned().flatten(),
        });
    }
    rows.push(DiffEntry {
        path: "CONTRACT.json".to_string(),
        expected: Some(lock.bundle_sha256.clone()),
        actual: actuals.get("CONTRACT.json").cloned().flatten(),
    });
    for (rel, sha) in &actuals {
        if *rel == "CONTRACT.json" || lock.files.iter().any(|f| f.path == **rel) {
            continue;
        }
        rows.push(DiffEntry {
            path: (*rel).to_string(),
            expected: None,
            actual: sha.clone(),
        });
    }
    rows.sort_by(|a, b| a.path.as_bytes().cmp(b.path.as_bytes()));
    Ok(rows)
}

/// Steps 3-4 of §10.6: a fresh tmp tree (every byte fsynced, then the
/// directories), the old copy moved aside, the new tree renamed into place.
/// The caller still has to [`commit_lock`] — that ordering is the guarantee.
pub fn write_tree(side: &Path, payload: &Payload, contract_json: &[u8]) -> std::io::Result<()> {
    let tree = side.join(".side-contract");
    let tmp = side.join(".side-contract.tmp");
    let old = side.join(".side-contract.old");

    // 3) fresh tmp tree, every byte fsynced, then the directories themselves.
    if tmp.exists() {
        fs::remove_dir_all(&tmp)?;
    }
    fs::create_dir_all(&tmp)?;
    let mut sync_dirs: BTreeSet<String> = BTreeSet::new();
    for (rel, bytes) in payload {
        write_fsync(&tmp.join(rel), bytes)?;
        if let Some(parent) = std::path::Path::new(rel).parent() {
            if let Some(parent) = parent.to_str() {
                sync_dirs.insert(parent.to_string());
            }
        }
    }
    write_fsync(&tmp.join("CONTRACT.json"), contract_json)?;
    for dir in &sync_dirs {
        fsync_dir(&tmp.join(dir))?;
    }
    fsync_dir(&tmp)?;

    // 4) move the previous copy aside, then put the new tree in place.
    if old.exists() {
        fs::remove_dir_all(&old)?;
    }
    if tree.exists() {
        fs::rename(&tree, &old)?;
    }
    fs::rename(&tmp, &tree)
}

/// Step 5 of §10.6 — the **commit point**: `contract.lock` written and
/// fsynced after the tree replacement, then the previous copy is dropped. A
/// failure here leaves old-lock-vs-new-tree: the next `--check` can never
/// read OK.
pub fn commit_lock(side: &Path, lock_text: &str) -> std::io::Result<()> {
    let old = side.join(".side-contract.old");
    write_fsync(&side.join("contract.lock"), lock_text.as_bytes())?;
    if old.exists() {
        let _ = fs::remove_dir_all(&old);
    }
    Ok(())
}

/// Converts manifest entries to the lock's `[[files]]` blocks (same order,
/// same digests; `CONTRACT.json` excluded — M1/§9.3).
pub fn manifest_to_lock_files(files: &[ManifestFileEntry]) -> Vec<LockFileEntry> {
    files
        .iter()
        .map(|entry| LockFileEntry {
            path: entry.path.clone(),
            sha256: entry.sha256.clone(),
            bytes: entry.bytes,
        })
        .collect()
}

/// Snapshot of `(mtime_sec, mtime_nsec, inode)` of every file under `path`
/// (recursive), used by the read-only antibody test (§10.7). Read-only by
/// construction: it never opens a descriptor for writing.
pub fn snapshot_tree(path: &Path) -> BTreeMap<String, (i64, i64, u64)> {
    use std::os::unix::fs::MetadataExt;

    let mut out = BTreeMap::new();
    let Ok(walked) = walk_entries(path) else {
        return out;
    };
    for entry in walked {
        let Ok(meta) = fs::metadata(&entry.path) else {
            continue;
        };
        out.insert(entry.rel, (meta.mtime(), meta.mtime_nsec(), meta.ino()));
    }
    out
}
