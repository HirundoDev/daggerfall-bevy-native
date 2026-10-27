//! Gate step 0 — the side's composed contract is the main's published one
//! (`docs/sintergic/side-contract/DESIGN-v1.md` §12.1, F6-D).
//!
//! Runs after the main baseline (a row needs the main's identity) and **before
//! step 1**: a side that cannot prove which contract it composed against never
//! reaches its own manifest. The gate reads files and hashes — nothing else
//! (§12.3: no red, no shell, no process spawn).
//!
//! What is read:
//!
//! * `<side>/contract.lock` — written only by `side-contract --sync`: the
//!   `contract_version` and `bundle_sha256` (sha256 of `CONTRACT.json`) the
//!   side composed, plus the per-file inventory under `<side>/.side-contract/`.
//! * `<main-root>/docs/sintergic/side-contract/releases/*.json` — the main's
//!   append-only release records (§9.4): which bundles it has published.
//! * `<side>/.side-contract/CONTRACT.json` and every file the lock declares —
//!   re-hashed against the lock, so a tampered copy cannot borrow a valid lock.
//!
//! | Observation | Cause (taxonomy v3) | Verdict | Exit |
//! |:---|:---|:---|:---:|
//! | no lock / unreadable lock | `contract_lock_missing` | FAIL | 2 |
//! | lock `major` behind or ahead | `contract_stale` | FAIL | 2 |
//! | lock's `bundle_sha256` is not in `releases/` | `contract_unknown` | FAIL | 2 |
//! | side bundle tampered (`CONTRACT.json` or a declared file) | `contract_tampered` | FAIL | 2 |
//! | lock `patch`/`minor` behind or ahead | **does not fail** — `WARN` in `detail`, `contract` block with the observed `drift_class` | PASS possible | 0 |
//!
//! `drift_class` reuses the `side-contract` vocabulary (§9.2):
//! `"none" | "patch" | "minor" | "major" | "tamper" | "unknown"`.

use std::path::Path;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::taxonomy::Cause;

/// Lock file at the root of the side workspace, written only by `side-contract --sync`.
pub const LOCK_FILE: &str = "contract.lock";
/// Bundle copy directory inside the side workspace (`--sync` owns it).
pub const SIDE_BUNDLE_DIR: &str = ".side-contract";
/// The bundle root: `bundle_sha256` is defined as the sha256 of these bytes (§9.3).
pub const BUNDLE_MANIFEST: &str = "CONTRACT.json";
/// Release records directory, relative to `main_root` (§9.4, main-owned).
pub const RELEASES_DIR: &str = "docs/sintergic/side-contract/releases";
/// `contract.lock` schema this gate reads (`side-contract` writes `"1"`).
pub const LOCK_SCHEMA: &str = "1";
/// Bundle schema this gate accepts inside a lock (§9.3: `bundle_schema = "1"`).
pub const LOCK_BUNDLE_SCHEMA: &str = "1";

/// `contract.lock`, schema 1. The gate reads only what it needs to decide.
///
/// `bundle_schema` is read but never compared against anything but `"1"`: the
/// normative lock format (DESIGN-v1 §9.3, written by `side-contract --sync`)
/// carries it as a fourth scalar key, and a lock without it is not a §9.3 lock.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct LockFile {
    schema_version: String,
    contract_version: String,
    bundle_sha256: String,
    bundle_schema: String,
    #[serde(default)]
    files: Vec<LockEntry>,
}

/// One declared file of the bundle copy, relative to `.side-contract/`.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct LockEntry {
    path: String,
    sha256: String,
    bytes: u64,
}

/// One release record of `<main-root>/releases/<contract_version>.json` (§9.4).
///
/// Main-owned and append-only; unknown fields are ignored on purpose (the
/// record gained `cr`/`ts`/`chapters_changed` in F6-B and may gain more), but
/// the two trust fields must be present and well-formed or the record does not
/// count as published.
#[derive(Debug, Deserialize)]
struct ReleaseRecord {
    contract_version: String,
    bundle_sha256: String,
}

/// The `contract` block of a verdict row (schema 1.2, DESIGN-v1 §12.2).
///
/// Every field is optional: a row that failed before the lock was read carries
/// `null`s; a `contract_lock_missing` row carries `null`s because there was no
/// lock to quote.
#[derive(Debug, Clone, Serialize, Default, PartialEq, Eq)]
pub struct ContractEvidence {
    /// `contract_version` of the side's lock, when one was read.
    pub lock_version: Option<String>,
    /// `bundle_sha256` of the side's lock, when one was read.
    pub bundle_sha256: Option<String>,
    /// Observed drift class (`none|patch|minor|major|tamper|unknown`), when one was observed.
    pub drift_class: Option<String>,
}

/// Step 0 failure: typed cause (taxonomy v3), human detail, and the contract
/// evidence observed up to the failure point.
#[derive(Debug)]
pub struct ContractFailure {
    pub cause: Cause,
    pub detail: String,
    pub evidence: ContractEvidence,
}

/// Result of step 0: the evidence for the verdict block plus an optional
/// non-blocking `WARN` (lock `patch`/`minor` atrasado o adelantado, §12.1).
#[derive(Debug)]
pub struct ContractOk {
    pub evidence: ContractEvidence,
    pub warn: Option<String>,
}

/// Strict `major.minor.patch` (DESIGN-v1 §9.2 grammar, mirroring
/// `side-contract`'s semver: three numeric components, no pre-release, no
/// build metadata). Implemented here because this crate is forbidden to grow a
/// dependency for it. Also used by step 1 to validate `[contract].version`.
pub fn parse_version(raw: &str) -> Option<(u64, u64, u64)> {
    let mut parts = raw.split('.');
    let mut component = || -> Option<u64> {
        let part = parts.next()?;
        if part.is_empty() || !part.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        if part.len() > 1 && part.starts_with('0') {
            return None;
        }
        part.parse().ok()
    };
    let major = component()?;
    let minor = component()?;
    let patch = component()?;
    parts.next().is_none().then_some((major, minor, patch))
}

pub fn is_sha256_hex(raw: &str) -> bool {
    raw.len() == 64 && raw.bytes().all(|b| b.is_ascii_hexdigit())
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// How two contract versions differ (§9.2 bump classes; `None` = same version).
fn version_diff(lock: (u64, u64, u64), published: (u64, u64, u64)) -> Option<&'static str> {
    if lock.0 != published.0 {
        Some("major")
    } else if lock.1 != published.1 {
        Some("minor")
    } else if lock.2 != published.2 {
        Some("patch")
    } else {
        None
    }
}

/// Is `lock` behind `published` in the same major (a stale but honest lock)?
fn is_behind(lock: (u64, u64, u64), published: (u64, u64, u64)) -> bool {
    lock < published
}

/// Step 0 of the gate (DESIGN-v1 §12.1). See the module docs for the table.
pub fn check(side_root: &Path, main_root: &Path) -> Result<ContractOk, ContractFailure> {
    // --- 1. the lock exists and is a proof this gate can read -------------
    let lock_path = side_root.join(LOCK_FILE);
    let raw = std::fs::read(&lock_path).map_err(|e| ContractFailure {
        cause: Cause::ContractLockMissing,
        detail: format!(
            "contract.lock ausente o ilegible ({}): {e}",
            lock_path.display()
        ),
        evidence: ContractEvidence::default(),
    })?;
    let lock: LockFile = {
        let text = String::from_utf8(raw).map_err(|e| ContractFailure {
            cause: Cause::ContractLockMissing,
            detail: format!("contract.lock no es UTF-8: {e}"),
            evidence: ContractEvidence::default(),
        })?;
        toml::from_str(&text).map_err(|e| ContractFailure {
            cause: Cause::ContractLockMissing,
            detail: format!("contract.lock no parsea como TOML: {e}"),
            evidence: ContractEvidence::default(),
        })?
    };
    let observed = || ContractEvidence {
        lock_version: Some(lock.contract_version.clone()),
        bundle_sha256: Some(lock.bundle_sha256.clone()),
        drift_class: None,
    };
    if lock.schema_version != LOCK_SCHEMA {
        return Err(ContractFailure {
            cause: Cause::ContractLockMissing,
            detail: format!(
                "contract.lock schema_version={:?} no soportado (gate lee {LOCK_SCHEMA})",
                lock.schema_version
            ),
            evidence: observed(),
        });
    }
    if lock.bundle_schema != LOCK_BUNDLE_SCHEMA {
        return Err(ContractFailure {
            cause: Cause::ContractLockMissing,
            detail: format!(
                "contract.lock bundle_schema={:?} no soportado (gate lee {LOCK_BUNDLE_SCHEMA})",
                lock.bundle_schema
            ),
            evidence: observed(),
        });
    }
    let Some(lock_version) = parse_version(&lock.contract_version) else {
        return Err(ContractFailure {
            cause: Cause::ContractLockMissing,
            detail: format!(
                "contract.lock contract_version={:?} no es un M.m.p estricto",
                lock.contract_version
            ),
            evidence: observed(),
        });
    };
    if !is_sha256_hex(&lock.bundle_sha256) {
        return Err(ContractFailure {
            cause: Cause::ContractLockMissing,
            detail: format!(
                "contract.lock bundle_sha256={:?} no es un sha256 de 64 hex",
                lock.bundle_sha256
            ),
            evidence: observed(),
        });
    }
    // Lowercase-normalized: `side-contract` writes lowercase hex; compare case-insensitively.
    let lock_bundle = lock.bundle_sha256.to_ascii_lowercase();

    // --- 2. what the main has published (§9.4 release records) ------------
    let releases_root = main_root.join(RELEASES_DIR);
    let mut releases: Vec<ReleaseRecord> = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&releases_root) {
        let mut paths: Vec<_> = entries.flatten().map(|e| e.path()).collect();
        paths.sort();
        for path in paths {
            let Ok(bytes) = std::fs::read(&path) else {
                continue;
            };
            // One record per release; the newest release adds one file (§9.4).
            if let Ok(record) = serde_json::from_slice::<ReleaseRecord>(&bytes) {
                if parse_version(&record.contract_version).is_some()
                    && is_sha256_hex(&record.bundle_sha256)
                {
                    releases.push(record);
                }
            }
        }
    }
    let Some(published_max) = releases
        .iter()
        .filter_map(|r| parse_version(&r.contract_version))
        .max()
    else {
        return Err(ContractFailure {
            cause: Cause::ContractUnknown,
            detail: format!(
                "el main no publica ningún release record válido en {RELEASES_DIR}/ — \
el lock cita un contrato que no puede verificarse"
            ),
            evidence: ContractEvidence {
                drift_class: Some("unknown".into()),
                ..observed()
            },
        });
    };

    // --- 3. major mismatch is stale (hard fail); patch/minor is WARN ------
    let drift = version_diff(lock_version, published_max).unwrap_or("none");
    let evidence = ContractEvidence {
        drift_class: Some(drift.to_string()),
        ..observed()
    };
    if drift == "major" {
        return Err(ContractFailure {
            cause: Cause::ContractStale,
            detail: format!(
                "contract.lock {} difiere en MAJOR del contrato publicado {}.{}.{} — \
el side compuso contra otro contrato",
                lock.contract_version, published_max.0, published_max.1, published_max.2
            ),
            evidence,
        });
    }
    let warn = (drift != "none").then(|| {
        let sentido = if is_behind(lock_version, published_max) {
            "atrasado"
        } else {
            "adelantado"
        };
        format!(
            "lock {sentido}: contract.lock {} vs publicado {}.{}.{} (drift_class {drift}, no bloqueante)",
            lock.contract_version, published_max.0, published_max.1, published_max.2
        )
    });

    // --- 4. the bundle the side locked is one the main published ----------
    if !releases
        .iter()
        .any(|r| r.bundle_sha256.to_ascii_lowercase() == lock_bundle)
    {
        return Err(ContractFailure {
            cause: Cause::ContractUnknown,
            detail: format!(
                "bundle_sha256 {lock_bundle} del lock no está en ningún release record de {RELEASES_DIR}/"
            ),
            evidence: ContractEvidence {
                drift_class: Some("unknown".into()),
                ..evidence
            },
        });
    }

    // --- 5. the side's bundle copy still matches the lock (tamper) --------
    let bundle_dir = side_root.join(SIDE_BUNDLE_DIR);
    let bundle_root = bundle_dir.join(BUNDLE_MANIFEST);
    let bundle_bytes = std::fs::read(&bundle_root).map_err(|e| ContractFailure {
        cause: Cause::ContractTampered,
        detail: format!(
            "bundle_tampered: {BUNDLE_MANIFEST} ausente o ilegible ({}): {e}",
            bundle_root.display()
        ),
        evidence: ContractEvidence {
            drift_class: Some("tamper".into()),
            ..evidence.clone()
        },
    })?;
    let bundle_hash = sha256_hex(&bundle_bytes);
    if bundle_hash != lock_bundle {
        return Err(ContractFailure {
            cause: Cause::ContractTampered,
            detail: format!(
                "bundle_tampered: sha256 de {SIDE_BUNDLE_DIR}/{BUNDLE_MANIFEST} ({bundle_hash}) \
≠ bundle_sha256 del lock ({lock_bundle})"
            ),
            evidence: ContractEvidence {
                drift_class: Some("tamper".into()),
                ..evidence.clone()
            },
        });
    }
    for entry in &lock.files {
        let path = bundle_dir.join(&entry.path);
        let bytes = std::fs::read(&path).map_err(|e| ContractFailure {
            cause: Cause::ContractTampered,
            detail: format!(
                "file_drift: {SIDE_BUNDLE_DIR}/{} ausente ({}): {e}",
                entry.path,
                path.display()
            ),
            evidence: ContractEvidence {
                drift_class: Some("tamper".into()),
                ..evidence.clone()
            },
        })?;
        let hash = sha256_hex(&bytes);
        if hash != entry.sha256.to_ascii_lowercase() || bytes.len() as u64 != entry.bytes {
            return Err(ContractFailure {
                cause: Cause::ContractTampered,
                detail: format!(
                    "file_drift: {SIDE_BUNDLE_DIR}/{} difiere del lock (sha256 {hash}, {} bytes)",
                    entry.path,
                    bytes.len()
                ),
                evidence: ContractEvidence {
                    drift_class: Some("tamper".into()),
                    ..evidence.clone()
                },
            });
        }
    }

    Ok(ContractOk { evidence, warn })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strict_version_parser() {
        assert_eq!(parse_version("1.0.0"), Some((1, 0, 0)));
        assert_eq!(parse_version("0.20.3"), Some((0, 20, 3)));
        assert_eq!(parse_version("1.0"), None);
        assert_eq!(parse_version("1.0.0.1"), None);
        assert_eq!(parse_version("01.0.0"), None);
        assert_eq!(parse_version("1.0.0-rc1"), None);
        assert_eq!(parse_version("v1.0.0"), None);
        assert_eq!(parse_version(""), None);
    }

    #[test]
    fn drift_classes_follow_9_2() {
        assert_eq!(version_diff((1, 0, 0), (1, 0, 0)), None);
        assert_eq!(version_diff((1, 0, 0), (1, 0, 1)), Some("patch"));
        assert_eq!(version_diff((1, 0, 1), (1, 0, 0)), Some("patch"));
        assert_eq!(version_diff((1, 0, 0), (1, 1, 0)), Some("minor"));
        assert_eq!(version_diff((1, 1, 0), (1, 0, 0)), Some("minor"));
        assert_eq!(version_diff((1, 9, 9), (2, 0, 0)), Some("major"));
        assert_eq!(version_diff((3, 0, 0), (1, 0, 0)), Some("major"));
        assert!(is_behind((1, 0, 0), (1, 0, 1)));
        assert!(!is_behind((1, 0, 1), (1, 0, 0)));
        assert!(!is_behind((1, 0, 0), (1, 0, 0)));
    }

    #[test]
    fn sha256_hex_validates() {
        assert!(is_sha256_hex(&"a".repeat(64)));
        assert!(is_sha256_hex(&"0".repeat(64)));
        assert!(!is_sha256_hex(
            &"A".repeat(64).to_ascii_lowercase().replace('a', "g")
        ));
        assert!(!is_sha256_hex(&"a".repeat(63)));
        assert!(!is_sha256_hex(&"a".repeat(65)));
    }

    #[test]
    fn missing_lock_is_typed_not_panic() {
        let dir = std::env::temp_dir().join(format!("f6d-contract-none-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let failure = check(&dir, &dir).expect_err("sin lock hay fallo tipado");
        assert_eq!(failure.cause, Cause::ContractLockMissing);
        assert_eq!(failure.evidence.lock_version, None);
        assert_eq!(failure.evidence.bundle_sha256, None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn garbage_lock_is_contract_lock_missing() {
        let dir = std::env::temp_dir().join(format!("f6d-contract-garbage-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(LOCK_FILE), "esto no es toml [\n").unwrap();
        let failure = check(&dir, &dir).expect_err("lock ilegible");
        assert_eq!(failure.cause, Cause::ContractLockMissing);
        let _ = std::fs::remove_dir_all(&dir);
    }

    // --- F6-D: step 0 paths against a fake main ---------

    /// One temporary root per test (tests run in parallel inside one process).
    fn temp_root(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("f6d-contract-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// `contract.lock` §9.3 in the exact format `side-contract --sync` writes.
    fn write_lock(
        side: &std::path::Path,
        version: &str,
        bundle: &str,
        files: &[(String, String, u64)],
    ) {
        let mut text = format!(
            "# contract.lock — escrito SOLO por `side-contract --sync`. No editar a mano.\n\
             # Prueba de qué contrato compuso este side; el gate lo leerá tras §12.\n\
             schema_version = \"1\"\ncontract_version = \"{version}\"\n\
             bundle_sha256 = \"{bundle}\"\nbundle_schema = \"1\"\n"
        );
        for (path, sha, bytes) in files {
            text.push_str(&format!(
                "\n[[files]]\npath = \"{path}\"\nsha256 = \"{sha}\"\nbytes = {bytes}\n"
            ));
        }
        std::fs::write(side.join(LOCK_FILE), text).unwrap();
    }

    /// `<main>/docs/sintergic/side-contract/releases/<version>.json` (§9.4).
    fn write_release(main: &std::path::Path, version: &str, bundle: &str) {
        let dir = main.join(RELEASES_DIR);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join(format!("{version}.json")),
            format!("{{\"contract_version\":\"{version}\",\"bundle_sha256\":\"{bundle}\"}}"),
        )
        .unwrap();
    }

    #[test]
    fn stale_major_fails_hard() {
        let root = temp_root("stale-major");
        let (side, main) = (root.join("side"), root.join("main"));
        std::fs::create_dir_all(&side).unwrap();
        std::fs::create_dir_all(&main).unwrap();
        let bundle = "b".repeat(64);
        write_lock(&side, "2.0.0", &bundle, &[]);
        write_release(&main, "1.0.0", &bundle);
        let failure = check(&side, &main).expect_err("lock en major distinto es fallo duro");
        assert_eq!(failure.cause, Cause::ContractStale);
        assert_eq!(failure.evidence.drift_class.as_deref(), Some("major"));
        assert_eq!(failure.evidence.lock_version.as_deref(), Some("2.0.0"));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn unknown_bundle_is_contract_unknown() {
        let root = temp_root("unknown-bundle");
        let (side, main) = (root.join("side"), root.join("main"));
        std::fs::create_dir_all(&side).unwrap();
        std::fs::create_dir_all(&main).unwrap();
        // The lock cites a bundle no release record ever published; it fails BEFORE
        // the tamper check (there is no copy to verify yet).
        write_lock(&side, "1.0.0", &"0".repeat(64), &[]);
        write_release(&main, "1.0.0", &"8".repeat(64));
        let failure = check(&side, &main).expect_err("bundle no publicado");
        assert_eq!(failure.cause, Cause::ContractUnknown);
        assert_eq!(failure.evidence.drift_class.as_deref(), Some("unknown"));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn declared_file_drift_is_contract_tampered() {
        let root = temp_root("file-drift");
        let (side, main) = (root.join("side"), root.join("main"));
        std::fs::create_dir_all(&side).unwrap();
        std::fs::create_dir_all(&main).unwrap();
        // CONTRACT.json about to be written, hashed: it is the sha the lock and
        // the release record declare (the bundle root is identified by its sha).
        let bundle_bytes = b"{\"contract\":\"falso\"}";
        let bundle = sha256_hex(bundle_bytes);
        let (real, declared) = (
            sha256_hex(b"contenido sincronizado\n"),
            sha256_hex(b"otro contenido\n"),
        );
        assert_ne!(real, declared, "sanity: el archivo difiere de lo declarado");
        let dir = side.join(SIDE_BUNDLE_DIR).join("chapters");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            side.join(SIDE_BUNDLE_DIR).join(BUNDLE_MANIFEST),
            bundle_bytes,
        )
        .unwrap();
        std::fs::write(dir.join("10-world-model.md"), "contenido sincronizado\n").unwrap();
        write_lock(
            &side,
            "1.0.0",
            &bundle,
            &[("chapters/10-world-model.md".into(), declared, 23)],
        );
        write_release(&main, "1.0.0", &bundle);
        let failure = check(&side, &main).expect_err("archivo declarado mutado");
        assert_eq!(failure.cause, Cause::ContractTampered);
        assert_eq!(failure.evidence.drift_class.as_deref(), Some("tamper"));
        assert!(
            failure.detail.contains("file_drift"),
            "el detalle nombra file_drift: {}",
            failure.detail
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn bundle_manifest_drift_is_contract_tampered() {
        let root = temp_root("bundle-drift");
        let (side, main) = (root.join("side"), root.join("main"));
        std::fs::create_dir_all(&side).unwrap();
        std::fs::create_dir_all(&main).unwrap();
        // The lock and the release record publish the original bundle sha; the
        // side copy was mutated after the sync.
        let published = sha256_hex(b"CONTRACT.json original publicado\n");
        std::fs::create_dir_all(side.join(SIDE_BUNDLE_DIR)).unwrap();
        std::fs::write(
            side.join(SIDE_BUNDLE_DIR).join(BUNDLE_MANIFEST),
            "CONTRACT.json mutado tras el sync\n",
        )
        .unwrap();
        write_lock(&side, "1.0.0", &published, &[]);
        write_release(&main, "1.0.0", &published);
        let failure = check(&side, &main).expect_err("CONTRACT.json mutado");
        assert_eq!(failure.cause, Cause::ContractTampered);
        assert_eq!(failure.evidence.drift_class.as_deref(), Some("tamper"));
        assert!(
            failure.detail.contains("bundle_tampered"),
            "el detalle nombra bundle_tampered: {}",
            failure.detail
        );
        let _ = std::fs::remove_dir_all(&root);
    }
}
