//! Bundle construction (M1-M7, R1-R10) and offline verification
//! (DESIGN-v1 §3.3, §4, §8; F6-B pack §§2, 6, 7, 8).
//!
//! `--generate` resolves every authored anchor against `--main-root`, derives
//! the closed facts, asserts coverage, and only then writes — `CONTRACT.json`
//! last. `--verify` re-hashes a received bundle against its own manifest with
//! zero dependency on a side: non-canonical or unknown fields are
//! `bundle_manifest_invalid`, digests and R1-R3 are `bundle_tampered`, retail
//! paths are `bundle_retail_violation`, and I/O or invalid UTF-8 abort as
//! `internal_error`.

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::Path;

use serde_json::{json, Value};

use crate::contract::{is_rel_posix_path, ContractCfg};
use crate::facts::{self, FactsError};
use crate::fsutil::{canonical_line, io_detail, sha256_hex, walk_entries, Payload, Walked};
use crate::globmatch::glob_matches;
use crate::semver::SemVer;
use crate::verdict::{FilesCounts, Obs};
use crate::Stop;

/// Why a generation stopped before writing anything.
pub enum GenErrKind {
    /// `internal_error` (the tool could not evaluate).
    Internal(String),
    /// A typed single-cause failure: `bundle_retail_violation` (P10) or
    /// `contract_source_drift` (P11).
    Failed(Obs),
}

/// Generation failure carrying the `contract_version` already parsed (so the
/// row can still report it).
pub struct GenErr {
    pub contract_version: Option<String>,
    pub kind: GenErrKind,
}

/// A fully built bundle held in memory, before any byte hits the disk.
pub struct BuiltBundle {
    /// Every file except `CONTRACT.json` (M1), keyed by relative path.
    pub files: Payload,
    /// Exact bytes of the manifest (canonical JSON + one final `\n`).
    pub contract_json: Vec<u8>,
    pub bundle_sha256: String,
    pub contract_version: String,
    pub anchors_count: usize,
}

/// One `files[]` entry of a validated manifest.
#[derive(Debug, Clone)]
pub struct ManifestFileEntry {
    pub path: String,
    pub class: String,
    pub sha256: String,
    pub bytes: u64,
}

/// A validated `CONTRACT.json`.
#[derive(Debug, Clone)]
pub struct Manifest {
    pub contract_version: String,
    pub files: Vec<ManifestFileEntry>,
    /// The `anchors[]` array exactly as parsed (cross-checked against
    /// `facts/anchors.json`).
    pub anchors: Value,
    pub excluded_globs: Vec<String>,
}

/// Outcome of verifying one bundle.
#[derive(Debug, Clone)]
pub struct VerifyReport {
    /// Observed causes (may hold P1 cross-check findings together with P2).
    pub obs: Vec<Obs>,
    pub files: FilesCounts,
    pub contract_version: Option<String>,
    pub bundle_sha256: Option<String>,
    /// Manifest `files[]`, used by `--sync` to copy the tree.
    pub manifest_files: Vec<ManifestFileEntry>,
    /// Deterministic OK detail (`{n} files verificados`).
    pub ok_detail: String,
}

impl VerifyReport {
    /// Report used when verification itself aborted (`internal_error`): no
    /// observations, nothing counted, no manifest.
    pub fn empty() -> VerifyReport {
        VerifyReport {
            obs: Vec::new(),
            files: FilesCounts::default(),
            contract_version: None,
            bundle_sha256: None,
            manifest_files: Vec::new(),
            ok_detail: String::new(),
        }
    }
}

/// Extracts the exact source lines `[line..line+len(expect)-1]` and compares
/// them byte for byte with `expect` (indentation and any stray `\r` included).
pub fn anchor_matches(text: &str, line: u64, expect: &str) -> bool {
    if line == 0 {
        return false;
    }
    let lines: Vec<&str> = text.split('\n').collect();
    let start = (line - 1) as usize;
    let count = expect.split('\n').count();
    if start + count > lines.len() {
        return false;
    }
    lines[start..start + count].join("\n") == expect
}

fn map_facts_err(err: FactsError, contract_version: Option<String>) -> GenErr {
    let kind = match err {
        FactsError::Drift(detail) => GenErrKind::Failed(Obs::new(
            crate::taxonomy::Cause::ContractSourceDrift,
            detail,
        )),
        FactsError::Internal(detail) => GenErrKind::Internal(detail),
    };
    GenErr {
        contract_version,
        kind,
    }
}

fn read_authored(
    path: &Path,
    what: &str,
    contract_version: Option<String>,
) -> Result<Vec<u8>, GenErr> {
    match fs::read(path) {
        Ok(bytes) => Ok(bytes),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Err(GenErr {
            contract_version,
            kind: GenErrKind::Internal(format!("fuente de autor ausente: {what}")),
        }),
        Err(e) => Err(GenErr {
            contract_version,
            kind: GenErrKind::Internal(io_detail(&e)),
        }),
    }
}

/// Full `--generate` evaluation, in memory (§8 flow of the pack).
///
/// Order: parse `contract.toml` → retail scan of the whole contract-root →
/// 36 anchors → derived facts → coverage → build files + `CONTRACT.json`.
pub fn build_bundle(
    contract_root: &Path,
    main_root: &Path,
    trace: &mut Vec<String>,
) -> Result<BuiltBundle, GenErr> {
    let cfg = ContractCfg::load(contract_root).map_err(|detail| GenErr {
        contract_version: None,
        kind: GenErrKind::Internal(detail),
    })?;
    trace.push(format!(
        "phase: contract.toml leído (schema 1, {} anchors, {} capítulos)",
        cfg.anchors.len(),
        cfg.chapters.len()
    ));

    // --- P10: retail scan of the tree that would be packaged ---------------
    let walked = walk_entries(contract_root).map_err(|e| GenErr {
        contract_version: Some(cfg.contract_version.clone()),
        kind: GenErrKind::Internal(io_detail(&e)),
    })?;
    let retail: Vec<&str> = walked
        .iter()
        .filter(|w| cfg.exclusions.globs.iter().any(|g| glob_matches(g, &w.rel)))
        .map(|w| w.rel.as_str())
        .collect();
    if !retail.is_empty() {
        return Err(GenErr {
            contract_version: Some(cfg.contract_version.clone()),
            kind: GenErrKind::Failed(Obs::new(
                crate::taxonomy::Cause::BundleRetailViolation,
                format!("retail empaquetado: {}", retail.join(", ")),
            )),
        });
    }
    trace.push("phase: exclusiones verificadas (0 retail)".to_string());

    // --- P11: anchors against main-root ------------------------------------
    let mut source_cache: BTreeMap<String, (Vec<u8>, String)> = BTreeMap::new();
    for anchor in &cfg.anchors {
        if !source_cache.contains_key(&anchor.path) {
            let path = main_root.join(&anchor.path);
            let bytes = match fs::read(&path) {
                Ok(bytes) => bytes,
                Err(e) if e.kind() == io::ErrorKind::NotFound => {
                    return Err(GenErr {
                        contract_version: Some(cfg.contract_version.clone()),
                        kind: GenErrKind::Failed(Obs::new(
                            crate::taxonomy::Cause::ContractSourceDrift,
                            format!(
                                "anchor {} no casa en {}:{} (fuente ausente bajo main-root: \
                                 revisa el cwd o --main-root)",
                                anchor.id, anchor.path, anchor.line
                            ),
                        )),
                    })
                }
                Err(e) => {
                    return Err(GenErr {
                        contract_version: Some(cfg.contract_version.clone()),
                        kind: GenErrKind::Internal(io_detail(&e)),
                    })
                }
            };
            let text = match String::from_utf8(bytes.clone()) {
                Ok(text) => text,
                Err(_) => {
                    return Err(GenErr {
                        contract_version: Some(cfg.contract_version.clone()),
                        kind: GenErrKind::Internal(format!("{}: UTF-8 inválido", anchor.path)),
                    })
                }
            };
            source_cache.insert(anchor.path.clone(), (bytes, text));
        }
        let (_, text) = &source_cache[&anchor.path];
        if !anchor_matches(text, anchor.line, &anchor.expect) {
            return Err(GenErr {
                contract_version: Some(cfg.contract_version.clone()),
                kind: GenErrKind::Failed(Obs::new(
                    crate::taxonomy::Cause::ContractSourceDrift,
                    format!(
                        "anchor {} no casa en {}:{}",
                        anchor.id, anchor.path, anchor.line
                    ),
                )),
            });
        }
    }
    trace.push(format!("phase: {} anchors verificados", cfg.anchors.len()));

    // --- P11: derived facts + coverage -------------------------------------
    let facts = facts::derive_facts(main_root, &cfg)
        .map_err(|err| map_facts_err(err, Some(cfg.contract_version.clone())))?;
    trace.push(format!(
        "phase: facts derivados ({} superficies, {} errores, {} métodos, {} causas)",
        facts.surface_items.len(),
        facts.error_codes.len(),
        facts.runtime_methods.len(),
        facts.gate.causes.len()
    ));
    facts::check_coverage(&cfg, contract_root, &facts)
        .map_err(|err| map_facts_err(err, Some(cfg.contract_version.clone())))?;
    trace.push("phase: cobertura verificada (4 capítulos)".to_string());

    // --- assemble the payload ----------------------------------------------
    let readme = read_authored(
        &contract_root.join("src").join("README.md"),
        "src/README.md",
        Some(cfg.contract_version.clone()),
    )?;
    let mut files: Payload = BTreeMap::new();
    let mut classes: BTreeMap<String, &'static str> = BTreeMap::new();
    files.insert("README.md".to_string(), readme);
    classes.insert("README.md".to_string(), "readme");

    for chapter in &cfg.chapters {
        let rel = format!("chapters/{}", chapter.path);
        let path = contract_root
            .join("src")
            .join("chapters")
            .join(&chapter.path);
        let bytes = read_authored(&path, &rel, Some(cfg.contract_version.clone()))?;
        files.insert(rel.clone(), bytes);
        classes.insert(rel, "chapter");
    }

    // anchors[] sorted by id (M3), with the sha256 of each source file.
    let mut anchor_values: Vec<(String, Value)> = Vec::with_capacity(cfg.anchors.len());
    for anchor in &cfg.anchors {
        let (bytes, _) = &source_cache[&anchor.path];
        anchor_values.push((
            anchor.id.clone(),
            json!({
                "file_sha256": sha256_hex(bytes),
                "id": anchor.id,
                "line": anchor.line,
                "literal": anchor.expect,
                "path": anchor.path,
                "symbol": anchor.symbol,
            }),
        ));
    }
    anchor_values.sort_by(|a, b| a.0.as_bytes().cmp(b.0.as_bytes()));
    let anchors_array: Vec<Value> = anchor_values.into_iter().map(|(_, v)| v).collect();
    let anchors_count = anchors_array.len();
    let anchors_json = canonical_line(&json!({ "anchors": anchors_array.clone() }));
    files.insert("facts/anchors.json".to_string(), anchors_json.into_bytes());
    classes.insert("facts/anchors.json".to_string(), "facts");

    let surfaces_json = canonical_line(&facts::facts_to_json(&facts));
    files.insert(
        "facts/surfaces.json".to_string(),
        surfaces_json.into_bytes(),
    );
    classes.insert("facts/surfaces.json".to_string(), "facts");

    // --- CONTRACT.json (M1-M7), canonical, written last by `write_bundle` ---
    let files_array: Vec<Value> = files
        .iter()
        .map(|(path, bytes)| {
            json!({
                "bytes": bytes.len(),
                "class": classes[path],
                "path": path,
                "sha256": sha256_hex(bytes),
            })
        })
        .collect();
    let mut globs = cfg.exclusions.globs.clone();
    globs.sort();
    let excluded_array: Vec<Value> = globs
        .iter()
        .map(|glob| {
            json!({
                "glob": glob,
                "reason": format!("{} — retail, nunca empaquetado", cfg.exclusions.policy),
            })
        })
        .collect();
    let manifest = json!({
        "anchors": anchors_array,
        "contract_version": cfg.contract_version,
        "engine": {
            "bevy": facts.engine.bevy,
            "msrv": facts.engine.msrv,
            "workspace_version": facts.engine.workspace_version,
        },
        "excluded": excluded_array,
        "files": files_array,
        "generator": {
            "name": "side-contract",
            "version": env!("CARGO_PKG_VERSION"),
        },
        "schema": "side-contract-bundle",
        "schema_version": "1",
    });
    let contract_json = canonical_line(&manifest).into_bytes();
    let bundle_sha256 = sha256_hex(&contract_json);

    Ok(BuiltBundle {
        files,
        contract_json,
        bundle_sha256,
        contract_version: cfg.contract_version,
        anchors_count,
    })
}

/// Writes the payload first and `CONTRACT.json` **last** (§8 flow).
pub fn write_bundle(out: &Path, built: &BuiltBundle) -> io::Result<()> {
    fs::create_dir_all(out)?;
    for (rel, bytes) in &built.files {
        let path = out.join(rel);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, bytes)?;
    }
    fs::write(out.join("CONTRACT.json"), &built.contract_json)?;
    Ok(())
}

fn is_sha64(raw: &str) -> bool {
    raw.len() == 64
        && raw
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

fn check_exact_keys(obj: &serde_json::Map<String, Value>, expected: &[&str]) -> Vec<String> {
    let mut problems = Vec::new();
    for key in obj.keys() {
        if !expected.contains(&key.as_str()) {
            problems.push(format!("campo desconocido {key}"));
        }
    }
    for key in expected {
        if !obj.contains_key(*key) {
            problems.push(format!("campo faltante {key}"));
        }
    }
    problems
}

fn as_str<'a>(value: &'a Value, problems: &mut Vec<String>, what: &str) -> Option<&'a str> {
    match value.as_str() {
        Some(raw) => Some(raw),
        None => {
            problems.push(format!("{what} no es string"));
            None
        }
    }
}

fn as_u64(value: &Value, problems: &mut Vec<String>, what: &str) -> Option<u64> {
    match value.as_u64() {
        Some(raw) => Some(raw),
        None => {
            problems.push(format!("{what} no es entero"));
            None
        }
    }
}

/// M1-M7 + R4 + strict types + strict key sets, in one pass. The first
/// problem becomes the `bundle_manifest_invalid` detail.
pub fn validate_manifest(value: &Value) -> Result<Manifest, Vec<String>> {
    let mut problems: Vec<String> = Vec::new();
    let Some(root) = value.as_object() else {
        return Err(vec!["CONTRACT.json no es un objeto".to_string()]);
    };
    problems.extend(check_exact_keys(
        root,
        &[
            "anchors",
            "contract_version",
            "engine",
            "excluded",
            "files",
            "generator",
            "schema",
            "schema_version",
        ],
    ));

    if let Some(schema) = as_str(&root["schema"], &mut problems, "schema") {
        if schema != "side-contract-bundle" {
            problems.push(format!("schema {schema:?} != \"side-contract-bundle\""));
        }
    }
    if let Some(version) = as_str(&root["schema_version"], &mut problems, "schema_version") {
        if version != "1" {
            problems.push(format!("schema_version {version:?} != \"1\""));
        }
    }
    let contract_version =
        match as_str(&root["contract_version"], &mut problems, "contract_version") {
            Some(raw) => {
                if SemVer::parse(raw).is_none() {
                    problems.push(format!("contract_version {raw:?} no es SemVer estricto"));
                }
                raw.to_string()
            }
            None => String::new(),
        };

    if let Some(engine) = root.get("engine").and_then(Value::as_object) {
        problems.extend(check_exact_keys(
            engine,
            &["bevy", "msrv", "workspace_version"],
        ));
        for key in ["bevy", "msrv", "workspace_version"] {
            as_str(&engine[key], &mut problems, &format!("engine.{key}"));
        }
    } else {
        problems.push("engine no es objeto".to_string());
    }
    if let Some(generator) = root.get("generator").and_then(Value::as_object) {
        problems.extend(check_exact_keys(generator, &["name", "version"]));
        if let Some(name) = as_str(&generator["name"], &mut problems, "generator.name") {
            if name != "side-contract" {
                problems.push(format!("generator.name {name:?} != \"side-contract\""));
            }
        }
        as_str(&generator["version"], &mut problems, "generator.version");
    } else {
        problems.push("generator no es objeto".to_string());
    }

    // files[] (M1/M2/R4 + types)
    let mut files: Vec<ManifestFileEntry> = Vec::new();
    match root.get("files").and_then(Value::as_array) {
        None => problems.push("files no es array".to_string()),
        Some(entries) => {
            let mut previous: Option<&str> = None;
            for entry in entries {
                let Some(obj) = entry.as_object() else {
                    problems.push("files[] no es objeto".to_string());
                    continue;
                };
                problems.extend(check_exact_keys(obj, &["bytes", "class", "path", "sha256"]));
                let path = as_str(&obj["path"], &mut problems, "files[].path").unwrap_or("");
                if !is_rel_posix_path(path) {
                    problems.push(format!("path {path:?} viola R4"));
                }
                if path == "CONTRACT.json" {
                    problems.push("files[] incluye CONTRACT.json".to_string());
                }
                let class = as_str(&obj["class"], &mut problems, "files[].class").unwrap_or("");
                if !matches!(class, "readme" | "chapter" | "facts") {
                    problems.push(format!("class {class:?} fuera de {{readme,chapter,facts}}"));
                }
                let sha256 = as_str(&obj["sha256"], &mut problems, "files[].sha256").unwrap_or("");
                if !is_sha64(sha256) {
                    problems.push(format!("sha256 malformado en {path:?}"));
                }
                let bytes = as_u64(&obj["bytes"], &mut problems, "files[].bytes").unwrap_or(0);
                if let Some(prev) = previous {
                    if prev.as_bytes() >= path.as_bytes() {
                        problems.push(format!("files[] desordenado en {path:?}"));
                    }
                }
                previous = Some(path);
                files.push(ManifestFileEntry {
                    path: path.to_string(),
                    class: class.to_string(),
                    sha256: sha256.to_string(),
                    bytes,
                });
            }
        }
    }

    // anchors[] (M3 + types)
    match root.get("anchors").and_then(Value::as_array) {
        None => problems.push("anchors no es array".to_string()),
        Some(entries) => {
            let mut previous: Option<&str> = None;
            for entry in entries {
                let Some(obj) = entry.as_object() else {
                    problems.push("anchors[] no es objeto".to_string());
                    continue;
                };
                problems.extend(check_exact_keys(
                    obj,
                    &["file_sha256", "id", "line", "literal", "path", "symbol"],
                ));
                let id = as_str(&obj["id"], &mut problems, "anchors[].id").unwrap_or("");
                if id.is_empty() {
                    problems.push("anchors[].id vacío".to_string());
                }
                let path = as_str(&obj["path"], &mut problems, "anchors[].path").unwrap_or("");
                if !is_rel_posix_path(path) {
                    problems.push(format!("anchor path {path:?} viola R4"));
                }
                as_u64(&obj["line"], &mut problems, "anchors[].line");
                as_str(&obj["literal"], &mut problems, "anchors[].literal");
                as_str(&obj["symbol"], &mut problems, "anchors[].symbol");
                let file_sha256 =
                    as_str(&obj["file_sha256"], &mut problems, "anchors[].file_sha256")
                        .unwrap_or("");
                if !is_sha64(file_sha256) {
                    problems.push(format!("file_sha256 malformado en anchor {id:?}"));
                }
                if let Some(prev) = previous {
                    if prev.as_bytes() >= id.as_bytes() {
                        problems.push(format!("anchors[] desordenado en {id:?}"));
                    }
                }
                previous = Some(id);
            }
        }
    }

    // excluded[] (M4)
    let mut excluded_globs: Vec<String> = Vec::new();
    match root.get("excluded").and_then(Value::as_array) {
        None => problems.push("excluded no es array".to_string()),
        Some(entries) => {
            let mut previous: Option<&str> = None;
            for entry in entries {
                let Some(obj) = entry.as_object() else {
                    problems.push("excluded[] no es objeto".to_string());
                    continue;
                };
                problems.extend(check_exact_keys(obj, &["glob", "reason"]));
                let glob = as_str(&obj["glob"], &mut problems, "excluded[].glob").unwrap_or("");
                as_str(&obj["reason"], &mut problems, "excluded[].reason");
                if glob.is_empty() {
                    problems.push("excluded[].glob vacío".to_string());
                }
                if let Some(prev) = previous {
                    if prev.as_bytes() >= glob.as_bytes() {
                        problems.push(format!("excluded[] desordenado en {glob:?}"));
                    }
                }
                previous = Some(glob);
                excluded_globs.push(glob.to_string());
            }
        }
    }

    if problems.is_empty() {
        Ok(Manifest {
            contract_version,
            files,
            anchors: root.get("anchors").cloned().unwrap_or(Value::Null),
            excluded_globs,
        })
    } else {
        Err(problems)
    }
}

/// Reads one file of the bundle as strict UTF-8, mapping I/O to [`Stop`].
fn read_bundle_file(path: &Path, rel: &str) -> Result<Vec<u8>, Stop> {
    match fs::read(path) {
        Ok(bytes) => {
            if std::str::from_utf8(&bytes).is_err() {
                return Err(Stop::Internal(format!("{rel}: UTF-8 inválido")));
            }
            Ok(bytes)
        }
        Err(e) => Err(Stop::Internal(format!("{rel}: {}", io_detail(&e)))),
    }
}

/// `--verify`: manifest → retail → digests/R1-R3 → anchors cross-check.
///
/// Short-circuit order (fixed by the F6-B matrix): P1 (manifest) first, then
/// the retail scan (P10 stops before the per-file walk, so an injected retail
/// file reports `bundle_retail_violation`, not "extra file"), then all P2
/// findings together with the P1 anchors cross-check — precedence decides the
/// primary cause.
pub fn verify_bundle(bundle_dir: &Path, trace: &mut Vec<String>) -> Result<VerifyReport, Stop> {
    let mut report = VerifyReport::empty();

    // --- P1: CONTRACT.json ---------------------------------------------------
    let manifest_path = bundle_dir.join("CONTRACT.json");
    let bytes = match fs::read(&manifest_path) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == io::ErrorKind::NotFound || e.raw_os_error() == Some(20) => {
            report.obs.push(Obs::new(
                crate::taxonomy::Cause::BundleManifestInvalid,
                "CONTRACT.json ausente",
            ));
            return Ok(report);
        }
        Err(e) => return Err(Stop::Internal(io_detail(&e))),
    };
    report.bundle_sha256 = Some(sha256_hex(&bytes));
    let text = String::from_utf8(bytes)
        .map_err(|_| Stop::Internal("CONTRACT.json: UTF-8 inválido".to_string()))?;
    let value: Value = match serde_json::from_str(&text) {
        Ok(value) => value,
        Err(e) => {
            report.obs.push(Obs::new(
                crate::taxonomy::Cause::BundleManifestInvalid,
                format!("CONTRACT.json ilegible: {e}"),
            ));
            return Ok(report);
        }
    };
    if canonical_line(&value) != text {
        report.obs.push(Obs::new(
            crate::taxonomy::Cause::BundleManifestInvalid,
            "CONTRACT.json no canónico",
        ));
        return Ok(report);
    }
    let manifest = match validate_manifest(&value) {
        Ok(manifest) => manifest,
        Err(problems) => {
            report.obs.push(Obs::new(
                crate::taxonomy::Cause::BundleManifestInvalid,
                format!("CONTRACT.json inválido: {}", problems[0]),
            ));
            return Ok(report);
        }
    };
    report.contract_version = Some(manifest.contract_version.clone());
    report.manifest_files = manifest.files.clone();
    trace.push("phase: manifiesto canónico verificado".to_string());

    // --- P10: retail scan -----------------------------------------------------
    let walked: Vec<Walked> =
        walk_entries(bundle_dir).map_err(|e| Stop::Internal(io_detail(&e)))?;
    if let Some(hit) = walked.iter().find(|w| {
        manifest
            .excluded_globs
            .iter()
            .any(|g| glob_matches(g, &w.rel))
    }) {
        report.obs.push(Obs::new(
            crate::taxonomy::Cause::BundleRetailViolation,
            format!("retail empaquetado: {}", hit.rel),
        ));
        return Ok(report);
    }
    trace.push("phase: exclusiones verificadas (0 retail)".to_string());

    // --- P2: per-file digests + R1/R2/R3 --------------------------------------
    let listed: BTreeMap<&str, &ManifestFileEntry> = manifest
        .files
        .iter()
        .map(|entry| (entry.path.as_str(), entry))
        .collect();
    let mut issues: Vec<String> = Vec::new();
    for entry in &manifest.files {
        let path = bundle_dir.join(&entry.path);
        // `metadata` follows symlinks: a listed path must be a regular file
        // (directory, FIFO or socket ⇒ tamper, never read — no EISDIR, no block).
        let metadata = match fs::metadata(&path) {
            Ok(metadata) => metadata,
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                report.files.missing += 1;
                issues.push(format!("falta {}", entry.path));
                continue;
            }
            Err(e) => return Err(Stop::Internal(io_detail(&e))),
        };
        if !metadata.is_file() {
            report.files.drifted += 1;
            issues.push(format!("entrada no regular en {}", entry.path));
            continue;
        }
        let file_bytes = match fs::read(&path) {
            Ok(bytes) => bytes,
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                report.files.missing += 1;
                issues.push(format!("falta {}", entry.path));
                continue;
            }
            Err(e) => return Err(Stop::Internal(io_detail(&e))),
        };
        if std::str::from_utf8(&file_bytes).is_err() {
            return Err(Stop::Internal(format!("{}: UTF-8 inválido", entry.path)));
        }
        let rule = if file_bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
            Some(format!("R3: BOM en {}", entry.path))
        } else if file_bytes.contains(&b'\r') {
            Some(format!("R1: \\r en {}", entry.path))
        } else if !file_bytes.ends_with(b"\n") {
            Some(format!("R2: sin \\n final en {}", entry.path))
        } else if file_bytes.ends_with(b"\n\n") {
            Some(format!("R2: \\n final doble en {}", entry.path))
        } else {
            None
        };
        match rule {
            Some(message) => {
                report.files.drifted += 1;
                issues.push(message);
            }
            None if sha256_hex(&file_bytes) != entry.sha256 => {
                report.files.drifted += 1;
                issues.push(format!("sha256 distinto en {}", entry.path));
            }
            None => {}
        }
    }
    for walked_entry in &walked {
        if walked_entry.rel == "CONTRACT.json" {
            continue;
        }
        if listed.contains_key(walked_entry.rel.as_str()) {
            continue;
        }
        report.files.unexpected += 1;
        issues.push(format!("sobra {}", walked_entry.rel));
    }
    report.files.checked = manifest.files.len();

    // --- P1 cross-check: anchors[] == facts/anchors.json ----------------------
    if listed.contains_key("facts/anchors.json") {
        let facts_path = bundle_dir.join("facts").join("anchors.json");
        match fs::read(&facts_path) {
            Err(e) if e.kind() == io::ErrorKind::NotFound => { /* already counted missing */ }
            Err(e) => return Err(Stop::Internal(io_detail(&e))),
            Ok(facts_bytes) => {
                let facts_text = String::from_utf8(facts_bytes).map_err(|_| {
                    Stop::Internal("facts/anchors.json: UTF-8 inválido".to_string())
                })?;
                match serde_json::from_str::<Value>(&facts_text) {
                    Err(_) => report.obs.push(Obs::new(
                        crate::taxonomy::Cause::BundleManifestInvalid,
                        "facts/anchors.json ilegible",
                    )),
                    Ok(facts_value) => {
                        let facts_obj = facts_value.as_object();
                        let shape_ok = facts_obj
                            .map(|obj| obj.len() == 1 && obj.contains_key("anchors"))
                            .unwrap_or(false);
                        if !shape_ok {
                            report.obs.push(Obs::new(
                                crate::taxonomy::Cause::BundleManifestInvalid,
                                "facts/anchors.json con campos desconocidos",
                            ));
                        } else if canonical_line(&facts_value) != facts_text {
                            report.obs.push(Obs::new(
                                crate::taxonomy::Cause::BundleManifestInvalid,
                                "facts/anchors.json no canónico",
                            ));
                        } else if facts_value.get("anchors") != value.get("anchors") {
                            report.obs.push(Obs::new(
                                crate::taxonomy::Cause::BundleManifestInvalid,
                                "anchors[] ≠ facts/anchors.json",
                            ));
                        }
                    }
                }
            }
        }
    }

    if let Some(first) = issues.first() {
        report.obs.push(Obs::new(
            crate::taxonomy::Cause::BundleTampered,
            first.clone(),
        ));
    }
    report.ok_detail = format!("{} files verificados", manifest.files.len());
    trace.push(format!(
        "phase: {} archivos hasheados",
        manifest.files.len()
    ));
    Ok(report)
}

/// Reads the payload of a verified bundle (for `--sync`): every manifest file
/// plus the manifest bytes themselves.
pub fn read_payload(
    bundle_dir: &Path,
    manifest_files: &[ManifestFileEntry],
) -> Result<(Payload, Vec<u8>), Stop> {
    let contract_json = read_bundle_file(&bundle_dir.join("CONTRACT.json"), "CONTRACT.json")?;
    let mut payload: Payload = BTreeMap::new();
    for entry in manifest_files {
        let bytes = read_bundle_file(&bundle_dir.join(&entry.path), &entry.path)?;
        payload.insert(entry.path.clone(), bytes);
    }
    Ok((payload, contract_json))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn anchor_extraction_is_byte_exact() {
        let text = "one\ntwo\nthree\nfour\n";
        assert!(anchor_matches(text, 1, "one"));
        assert!(anchor_matches(text, 2, "two"));
        assert!(anchor_matches(text, 3, "three\nfour"));
        assert!(anchor_matches(text, 3, "three"));
        assert!(!anchor_matches(text, 4, "three"));
        assert!(!anchor_matches(text, 99, "one"));
        assert!(!anchor_matches(text, 0, "one"));
        assert!(!anchor_matches(text, 2, "tw0"));
    }

    #[test]
    fn anchor_extraction_keeps_indentation_and_cr() {
        let text = "fn a() {\n    let x = 1;\r\n}\n";
        assert!(anchor_matches(text, 2, "    let x = 1;\r"));
        assert!(!anchor_matches(text, 2, "    let x = 1;"));
    }

    #[test]
    fn manifest_order_violation_is_rejected() {
        let good = json!({
            "anchors": [],
            "contract_version": "1.0.0",
            "engine": {"bevy": "0.20.0", "msrv": "1.97.1", "workspace_version": "0.1.0"},
            "excluded": [{"glob": "**/*.BSA", "reason": "policy"}],
            "files": [
                {"bytes": 1, "class": "chapter", "path": "chapters/b.md", "sha256": "a".repeat(64)},
                {"bytes": 1, "class": "readme", "path": "README.md", "sha256": "b".repeat(64)},
            ],
            "generator": {"name": "side-contract", "version": "0.1.0"},
            "schema": "side-contract-bundle",
            "schema_version": "1",
        });
        let problems = validate_manifest(&good).expect_err("files out of order");
        assert!(problems[0].contains("desordenado"), "{:?}", problems[0]);
    }

    #[test]
    fn manifest_absolute_path_is_rejected() {
        let mut value = json!({
            "anchors": [],
            "contract_version": "1.0.0",
            "engine": {"bevy": "0.20.0", "msrv": "1.97.1", "workspace_version": "0.1.0"},
            "excluded": [{"glob": "**/*.BSA", "reason": "policy"}],
            "files": [
                {"bytes": 1, "class": "readme", "path": "/etc/passwd", "sha256": "a".repeat(64)},
            ],
            "generator": {"name": "side-contract", "version": "0.1.0"},
            "schema": "side-contract-bundle",
            "schema_version": "1",
        });
        let problems = validate_manifest(&value).expect_err("absolute path");
        assert!(problems.iter().any(|p| p.contains("R4")), "{problems:?}");

        value["files"][0]["path"] = json!("../outside.md");
        let problems = validate_manifest(&value).expect_err("parent path");
        assert!(problems.iter().any(|p| p.contains("R4")), "{problems:?}");

        value["files"][0]["path"] = json!("CONTRACT.json");
        let problems = validate_manifest(&value).expect_err("self reference");
        assert!(
            problems.iter().any(|p| p.contains("incluye CONTRACT.json")),
            "{problems:?}"
        );
    }

    #[test]
    fn manifest_unknown_field_and_bad_types_are_rejected() {
        let mut value = json!({
            "anchors": [],
            "contract_version": "1.0.0",
            "engine": {"bevy": "0.20.0", "msrv": "1.97.1", "workspace_version": "0.1.0"},
            "excluded": [{"glob": "**/*.BSA", "reason": "policy"}],
            "files": [
                {"bytes": 1, "class": "binary", "path": "x.bin", "sha256": "a".repeat(64)},
            ],
            "generator": {"name": "side-contract", "version": "0.1.0"},
            "schema": "side-contract-bundle",
            "schema_version": "1",
        });
        let problems = validate_manifest(&value).expect_err("bad class");
        assert!(problems.iter().any(|p| p.contains("class")), "{problems:?}");

        value["files"][0]["class"] = json!("readme");
        value["files"][0]["sha256"] = json!("nothex");
        let problems = validate_manifest(&value).expect_err("bad sha");
        assert!(
            problems.iter().any(|p| p.contains("sha256")),
            "{problems:?}"
        );

        value["files"][0]["sha256"] = json!("a".repeat(64));
        value["extra"] = json!(1);
        let problems = validate_manifest(&value).expect_err("unknown field");
        assert!(
            problems.iter().any(|p| p.contains("campo desconocido")),
            "{problems:?}"
        );

        value.as_object_mut().unwrap().remove("extra");
        value["contract_version"] = json!("1.0");
        let problems = validate_manifest(&value).expect_err("bad semver");
        assert!(
            problems.iter().any(|p| p.contains("SemVer")),
            "{problems:?}"
        );
    }

    #[test]
    fn manifest_valid_round_trip() {
        let value = json!({
            "anchors": [{
                "file_sha256": "c".repeat(64),
                "id": "a.b",
                "line": 3,
                "literal": "pub struct X {",
                "path": "src/lib.rs",
                "symbol": "X",
            }],
            "contract_version": "1.0.0",
            "engine": {"bevy": "0.20.0", "msrv": "1.97.1", "workspace_version": "0.1.0"},
            "excluded": [{"glob": "**/*.BSA", "reason": "policy"}],
            "files": [
                {"bytes": 4, "class": "readme", "path": "README.md", "sha256": "e".repeat(64)},
                {"bytes": 12, "class": "facts", "path": "facts/anchors.json", "sha256": "d".repeat(64)},
            ],
            "generator": {"name": "side-contract", "version": "0.1.0"},
            "schema": "side-contract-bundle",
            "schema_version": "1",
        });
        let manifest = validate_manifest(&value).expect("valid");
        assert_eq!(manifest.contract_version, "1.0.0");
        assert_eq!(manifest.files.len(), 2);
        assert_eq!(manifest.excluded_globs, vec!["**/*.BSA"]);
        assert_eq!(canonical_line(&value).bytes().last(), Some(b'\n'));
    }
}
