//! `contract.lock` of a side (DESIGN-v1 §9.3, F6-B pack §6).
//!
//! The file is written **only** by `side-contract --sync`, with a manual
//! serializer that reproduces §9.3 byte for byte: two comment lines, four
//! scalar keys, `[[files]]` blocks ordered by path, zero timestamps. Reading is
//! strict: unknown/missing field, wrong `schema_version`/`bundle_schema`, bad
//! SemVer, malformed sha or unsorted `files[]` is `lock_invalid`; invalid
//! UTF-8 is `internal_error`.

use std::path::Path;

use serde::Deserialize;

use crate::contract::is_rel_posix_path;
use crate::fsutil::{io_detail, sha256_hex};
use crate::semver::SemVer;
use crate::Stop;

/// One `[[files]]` block of the lock.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawLockFile {
    path: String,
    sha256: String,
    bytes: u64,
}

/// The whole lock, strictly typed.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawLock {
    schema_version: String,
    contract_version: String,
    bundle_sha256: String,
    bundle_schema: String,
    #[serde(rename = "files")]
    files: Vec<RawLockFile>,
}

/// A validated lock entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LockFileEntry {
    pub path: String,
    pub sha256: String,
    pub bytes: u64,
}

/// A validated `contract.lock`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Lock {
    pub contract_version: String,
    pub bundle_sha256: String,
    pub files: Vec<LockFileEntry>,
}

/// Result of reading a side's lock without ever failing the process.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LockOutcome {
    /// Valid lock.
    Present(Lock),
    /// Side or lock absent (P3 `lock_missing`).
    Missing,
    /// Structurally invalid (P4 `lock_invalid`) with a deterministic reason.
    Invalid(String),
}

fn is_sha64(raw: &str) -> bool {
    raw.len() == 64
        && raw
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// Reads `<side>/contract.lock` strictly (see [`LockOutcome`]); I/O errors and
/// invalid UTF-8 surface as [`Stop::Internal`].
pub fn read_lock(path: &Path) -> Result<LockOutcome, Stop> {
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(LockOutcome::Missing),
        Err(e) => return Err(Stop::Internal(io_detail(&e))),
    };
    let text = String::from_utf8(bytes)
        .map_err(|_| Stop::Internal("contract.lock: UTF-8 inválido".to_string()))?;
    let raw: RawLock = match toml::from_str(&text) {
        Ok(raw) => raw,
        Err(e) => return Ok(LockOutcome::Invalid(format!("TOML: {e}"))),
    };
    match validate(raw) {
        Ok(lock) => Ok(LockOutcome::Present(lock)),
        Err(reason) => Ok(LockOutcome::Invalid(reason)),
    }
}

fn validate(raw: RawLock) -> Result<Lock, String> {
    if raw.schema_version != "1" {
        return Err(format!("schema_version {:?} != \"1\"", raw.schema_version));
    }
    if raw.bundle_schema != "1" {
        return Err(format!("bundle_schema {:?} != \"1\"", raw.bundle_schema));
    }
    if SemVer::parse(&raw.contract_version).is_none() {
        return Err(format!(
            "contract_version {:?} no es SemVer estricto",
            raw.contract_version
        ));
    }
    if !is_sha64(&raw.bundle_sha256) {
        return Err("bundle_sha256 malformado".to_string());
    }
    if raw.files.is_empty() {
        return Err("[[files]] vacío".to_string());
    }
    let mut files = Vec::with_capacity(raw.files.len());
    let mut previous: Option<String> = None;
    for file in raw.files {
        if !is_rel_posix_path(&file.path) {
            return Err(format!("path {:?} viola R4", file.path));
        }
        if !is_sha64(&file.sha256) {
            return Err(format!("sha256 malformado en {}", file.path));
        }
        if let Some(prev) = &previous {
            if prev.as_str() >= file.path.as_str() {
                return Err(format!("files[] desordenado en {}", file.path));
            }
        }
        previous = Some(file.path.clone());
        files.push(LockFileEntry {
            path: file.path,
            sha256: file.sha256,
            bytes: file.bytes,
        });
    }
    Ok(Lock {
        contract_version: raw.contract_version,
        bundle_sha256: raw.bundle_sha256,
        files,
    })
}

/// Minimal TOML basic-string escaper (paths, versions and shas need only the
/// quotes/backslash cases; control bytes are escaped defensively).
fn toml_basic(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len() + 2);
    out.push('"');
    for ch in raw.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 || c as u32 == 0x7f => {
                out.push_str(&format!("\\u{:04X}", c as u32));
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Renders the canonical `contract.lock` text of §9.3 (no timestamps; the two
/// files of two sides synchronized to the same contract are byte-identical).
pub fn render_lock(contract_version: &str, bundle_sha256: &str, files: &[LockFileEntry]) -> String {
    let mut out = String::new();
    out.push_str("# contract.lock — escrito SOLO por `side-contract --sync`. No editar a mano.\n");
    out.push_str("# Prueba de qué contrato compuso este side; el gate lo leerá tras §12.\n");
    out.push_str("schema_version = \"1\"\n");
    out.push_str(&format!(
        "contract_version = {}\n",
        toml_basic(contract_version)
    ));
    out.push_str(&format!("bundle_sha256 = {}\n", toml_basic(bundle_sha256)));
    out.push_str("bundle_schema = \"1\"\n");
    for file in files {
        out.push_str("\n[[files]]\n");
        out.push_str(&format!("path = {}\n", toml_basic(&file.path)));
        out.push_str(&format!("sha256 = {}\n", toml_basic(&file.sha256)));
        out.push_str(&format!("bytes = {}\n", file.bytes));
    }
    out
}

/// Convenience: sha256 hex re-export used by the sync path.
pub fn digest(bytes: &[u8]) -> String {
    sha256_hex(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_files() -> Vec<LockFileEntry> {
        vec![
            LockFileEntry {
                path: "README.md".into(),
                sha256: "1".repeat(64),
                bytes: 10,
            },
            LockFileEntry {
                path: "chapters/00.md".into(),
                sha256: "2".repeat(64),
                bytes: 20,
            },
        ]
    }

    #[test]
    fn render_then_parse_round_trips() {
        let text = render_lock("1.0.0", &"a".repeat(64), &sample_files());
        assert!(text.starts_with(
            "# contract.lock — escrito SOLO por `side-contract --sync`. No editar a mano.\n# Prueba de qué contrato compuso este side; el gate lo leerá tras §12.\n"
        ));
        assert!(text.ends_with('\n'));
        assert!(!text.ends_with("\n\n"));
        let path = std::env::temp_dir().join(format!(
            "side-contract-lock-roundtrip-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&path).unwrap();
        let file = path.join("contract.lock");
        std::fs::write(&file, &text).unwrap();
        match read_lock(&file).unwrap() {
            LockOutcome::Present(lock) => {
                assert_eq!(lock.contract_version, "1.0.0");
                assert_eq!(lock.bundle_sha256, "a".repeat(64));
                assert_eq!(lock.files.len(), 2);
                assert_eq!(lock.files[0].path, "README.md");
                assert_eq!(lock.files[1].bytes, 20);
            }
            other => panic!("expected valid lock, got {other:?}"),
        }
        // byte-identical re-render
        assert_eq!(
            render_lock(
                &lock_contract_version(&text),
                &"a".repeat(64),
                &sample_files()
            ),
            text
        );
        let _ = std::fs::remove_dir_all(&path);
    }

    fn lock_contract_version(_text: &str) -> String {
        "1.0.0".to_string()
    }

    #[test]
    fn no_timestamps_are_rendered() {
        let text = render_lock("1.0.0", &"a".repeat(64), &sample_files());
        for forbidden in ["ts", "created_at", "timestamp", "generated", "host"] {
            assert!(!text.contains(forbidden), "found {forbidden:?}");
        }
    }

    #[test]
    fn missing_lock_is_missing() {
        let path =
            std::env::temp_dir().join(format!("side-contract-no-lock-{}", std::process::id()));
        assert_eq!(read_lock(&path).unwrap(), LockOutcome::Missing);
    }

    #[test]
    fn strict_validation_rejects_bad_locks() {
        // wrong schema_version
        let mut text = render_lock("1.0.0", &"a".repeat(64), &sample_files());
        text = text.replace("schema_version = \"1\"", "schema_version = \"2\"");
        assert!(matches!(validate_case(&text), LockOutcome::Invalid(_)));

        // unsorted files
        let reversed: Vec<LockFileEntry> = sample_files().into_iter().rev().collect();
        let unsorted = render_lock("1.0.0", &"a".repeat(64), &reversed);
        match validate_case(&unsorted) {
            LockOutcome::Invalid(reason) => assert!(reason.contains("desordenado"), "{reason}"),
            other => panic!("expected invalid, got {other:?}"),
        }

        // malformed sha
        let bad_sha = render_lock(
            "1.0.0",
            &"a".repeat(64),
            &[LockFileEntry {
                path: "README.md".into(),
                sha256: "zz".into(),
                bytes: 1,
            }],
        );
        match validate_case(&bad_sha) {
            LockOutcome::Invalid(reason) => assert!(reason.contains("sha256"), "{reason}"),
            other => panic!("expected invalid, got {other:?}"),
        }

        // unknown field
        let extra = format!(
            "{}\nextra = true\n",
            render_lock("1.0.0", &"a".repeat(64), &sample_files())
        );
        assert!(matches!(validate_case(&extra), LockOutcome::Invalid(_)));
    }

    fn validate_case(text: &str) -> LockOutcome {
        let raw: RawLock = match toml::from_str(text) {
            Ok(raw) => raw,
            Err(e) => return LockOutcome::Invalid(format!("TOML: {e}")),
        };
        match validate(raw) {
            Ok(lock) => LockOutcome::Present(lock),
            Err(reason) => LockOutcome::Invalid(reason),
        }
    }
}
