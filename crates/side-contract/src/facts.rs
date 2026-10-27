//! Derived facts for `facts/surfaces.json` and the mechanical coverage
//! assertion (DESIGN-v1 §4.3, F6-B pack §7).
//!
//! Nothing here is listed by hand: every value is parsed out of the closed
//! constants of the main workspace. A line with an unexpected shape, a block
//! without its terminator, or an unresolved identifier is
//! `contract_source_drift` — the generator refuses instead of blessing a
//! stale bundle. I/O and invalid UTF-8 are `internal_error`.

use std::collections::BTreeMap;
use std::path::Path;

use serde_json::{json, Value};

use crate::contract::ContractCfg;

/// Fixed sources of the derived facts, relative to `--main-root`.
pub const SCHEMA_RS: &str = "crates/content-api/src/schema.rs";
pub const ERROR_RS: &str = "crates/content-api/src/error.rs";
pub const PROTOCOL_RS: &str = "crates/game-runtime-api/src/protocol.rs";
pub const TAXONOMY_RS: &str = "crates/promotion-gate/src/taxonomy.rs";
pub const MANIFEST_RS: &str = "crates/promotion-gate/src/manifest.rs";

/// Why a derivation stopped: source drift vs the tool being unable to evaluate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FactsError {
    /// `contract_source_drift` with a deterministic detail.
    Drift(String),
    /// `internal_error` with a deterministic detail.
    Internal(String),
}

/// One published surface item (`name` + `kind`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SurfaceItemF {
    pub name: String,
    pub kind: String,
}

/// `engine{workspace_version, bevy, msrv}` of the bundle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EngineFacts {
    pub workspace_version: String,
    pub bevy: String,
    pub msrv: String,
}

/// The gate group of `facts/surfaces.json`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GateFacts {
    pub causes: Vec<String>,
    pub manifest_schema: String,
    pub taxonomy_version: String,
    pub verdict_schema_version: String,
}

/// The whole `facts/surfaces.json` payload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Facts {
    pub content_schema: String,
    pub engine: EngineFacts,
    pub error_codes: Vec<String>,
    pub gate: GateFacts,
    pub runtime_methods: Vec<String>,
    pub surface_items: Vec<SurfaceItemF>,
}

/// Reads a main-root-relative source file strictly: missing ⇒ drift (the
/// promised source is gone), invalid UTF-8 ⇒ internal, other I/O ⇒ internal.
fn read_source(main_root: &Path, rel: &str) -> Result<String, FactsError> {
    let path = main_root.join(rel);
    match std::fs::read(&path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            Err(FactsError::Drift(format!("fuente ausente: {rel}")))
        }
        Err(e) => Err(FactsError::Internal(format!(
            "error de IO ({:?})",
            e.kind()
        ))),
        Ok(bytes) => String::from_utf8(bytes)
            .map_err(|_| FactsError::Internal(format!("{rel}: UTF-8 inválido"))),
    }
}

fn drifted(block: &str) -> FactsError {
    FactsError::Drift(format!("bloque ilegible: {block}"))
}

/// `pub const NAME: &str = "value";` anywhere in `text` (strict: the line
/// ends right after the closing quote).
fn const_str_value(text: &str, name: &str) -> Result<String, FactsError> {
    let prefix = format!("pub const {name}: &str = \"");
    for line in text.lines() {
        let trimmed = line.trim_start();
        let Some(rest) = trimmed.strip_prefix(&prefix) else {
            continue;
        };
        let Some(end) = rest.find('"') else {
            continue;
        };
        if rest[end..].len() == "\";".len() && &rest[end..] == "\";" {
            let value = &rest[..end];
            if value.is_empty() {
                break;
            }
            return Ok(value.to_string());
        }
    }
    Err(drifted(name))
}

fn lines_of(text: &str) -> Vec<&str> {
    text.lines().collect()
}

fn find_line(lines: &[&str], exact: &str) -> Option<usize> {
    lines.iter().position(|l| l.trim_start() == exact)
}

/// Parses one `SurfaceItem { name: "X", kind: "y" },` line.
fn parse_surface_item(line: &str) -> Option<(String, String)> {
    let rest = line.strip_prefix("SurfaceItem { name: \"")?;
    let mid = rest.find("\", kind: \"")?;
    let name = &rest[..mid];
    let kind_rest = &rest[mid + "\", kind: \"".len()..];
    let end = kind_rest.find('"')?;
    let kind = &kind_rest[..end];
    if &kind_rest[end..] != "\" }," || name.is_empty() || kind.is_empty() {
        return None;
    }
    Some((name.to_string(), kind.to_string()))
}

/// `SURFACE` (schema.rs) → items in declaration order, block closed by `];`.
pub fn derive_surface(text: &str) -> Result<Vec<SurfaceItemF>, FactsError> {
    let lines = lines_of(text);
    let start = find_line(&lines, "pub const SURFACE: &[SurfaceItem] = &[")
        .ok_or_else(|| drifted("SURFACE"))?;
    let mut items = Vec::new();
    let mut closed = false;
    for line in &lines[start + 1..] {
        let trimmed = line.trim_start();
        if trimmed == "];" {
            closed = true;
            break;
        }
        let (name, kind) = parse_surface_item(trimmed).ok_or_else(|| drifted("SURFACE"))?;
        items.push(SurfaceItemF { name, kind });
    }
    if !closed || items.is_empty() {
        return Err(drifted("SURFACE"));
    }
    Ok(items)
}

/// `ERROR_CODES` (error.rs) → literals in declaration order.
pub fn derive_error_codes(text: &str) -> Result<Vec<String>, FactsError> {
    let lines = lines_of(text);
    let start = find_line(&lines, "pub const ERROR_CODES: &[&str] = &[")
        .ok_or_else(|| drifted("ERROR_CODES"))?;
    let mut codes = Vec::new();
    let mut closed = false;
    for line in &lines[start + 1..] {
        let trimmed = line.trim_start();
        if trimmed == "];" {
            closed = true;
            break;
        }
        let inner = trimmed
            .strip_prefix('"')
            .and_then(|r| r.strip_suffix("\","))
            .ok_or_else(|| drifted("ERROR_CODES"))?;
        if inner.is_empty() || inner.contains('"') {
            return Err(drifted("ERROR_CODES"));
        }
        codes.push(inner.to_string());
    }
    if !closed || codes.is_empty() {
        return Err(drifted("ERROR_CODES"));
    }
    Ok(codes)
}

fn is_const_ident(raw: &str) -> bool {
    let mut chars = raw.chars();
    match chars.next() {
        Some(c) if c.is_ascii_uppercase() || c == '_' => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
}

/// `methods::ALL` (protocol.rs): identifiers resolved against the
/// `pub const IDENT: &str = "value";` table of the same file, in `ALL` order.
/// An identifier that does not resolve is drift (§7 of the pack).
pub fn derive_runtime_methods(text: &str) -> Result<Vec<String>, FactsError> {
    let lines = lines_of(text);
    let start =
        find_line(&lines, "pub const ALL: &[&str] = &[").ok_or_else(|| drifted("methods::ALL"))?;
    let mut idents = Vec::new();
    let mut closed = false;
    for line in &lines[start + 1..] {
        let trimmed = line.trim_start();
        if trimmed == "];" {
            closed = true;
            break;
        }
        let ident = trimmed
            .strip_suffix(',')
            .map(str::trim)
            .ok_or_else(|| drifted("methods::ALL"))?;
        if !is_const_ident(ident) {
            return Err(drifted("methods::ALL"));
        }
        idents.push(ident.to_string());
    }
    if !closed || idents.is_empty() {
        return Err(drifted("methods::ALL"));
    }
    idents
        .iter()
        .map(|ident| const_str_value(text, ident).map_err(|_| drifted("methods::ALL")))
        .collect()
}

fn parse_cause_members(lines: &[&str]) -> Result<Vec<String>, FactsError> {
    let start_idx = lines
        .iter()
        .position(|l| l.trim_start().starts_with("pub const ALL: [Cause;"))
        .ok_or_else(|| drifted("Cause::ALL"))?;
    let decl = lines[start_idx].trim_start();
    let expected = decl
        .split("[Cause;")
        .nth(1)
        .and_then(|rest| rest.split(']').next())
        .and_then(|raw| raw.trim().parse::<usize>().ok())
        .ok_or_else(|| drifted("Cause::ALL"))?;
    let mut members = Vec::new();
    let mut closed = false;
    for line in &lines[start_idx + 1..] {
        let trimmed = line.trim_start();
        if trimmed == "];" {
            closed = true;
            break;
        }
        let variant = trimmed
            .strip_prefix("Cause::")
            .and_then(|r| r.strip_suffix(','))
            .map(str::trim)
            .ok_or_else(|| drifted("Cause::ALL"))?;
        if variant.is_empty() {
            return Err(drifted("Cause::ALL"));
        }
        members.push(variant.to_string());
    }
    if !closed || members.is_empty() || members.len() != expected {
        return Err(drifted("Cause::ALL"));
    }
    Ok(members)
}

fn parse_id_arms(lines: &[&str]) -> Result<Vec<(String, String)>, FactsError> {
    let start_idx = lines
        .iter()
        .position(|l| l.trim_start() == "pub fn id(self) -> &'static str {")
        .ok_or_else(|| drifted("id()"))?;
    let after_match = lines[start_idx..]
        .iter()
        .position(|l| l.trim_start() == "match self {")
        .ok_or_else(|| drifted("id()"))?;
    let body_start = start_idx + after_match + 1;
    let mut arms = Vec::new();
    let mut closed = false;
    for line in &lines[body_start..] {
        let trimmed = line.trim_start();
        if trimmed == "}" {
            closed = true;
            break;
        }
        let variant_part = trimmed
            .strip_prefix("Cause::")
            .ok_or_else(|| drifted("id()"))?;
        let (variant, rest) = variant_part
            .split_once(" => \"")
            .ok_or_else(|| drifted("id()"))?;
        let id = rest.strip_suffix("\",").ok_or_else(|| drifted("id()"))?;
        if variant.is_empty() || id.is_empty() {
            return Err(drifted("id()"));
        }
        arms.push((variant.to_string(), id.to_string()));
    }
    if !closed || arms.is_empty() {
        return Err(drifted("id()"));
    }
    Ok(arms)
}

/// The gate group: 19 (or whatever `Cause::ALL` declares) snake_case ids
/// crossed 1:1 with `Cause::ALL`, plus the three closed schema versions.
pub fn derive_gate(taxonomy_text: &str, manifest_text: &str) -> Result<GateFacts, FactsError> {
    let lines = lines_of(taxonomy_text);
    let members = parse_cause_members(&lines)?;
    let arms = parse_id_arms(&lines)?;
    if arms.len() != members.len() {
        return Err(drifted("id()"));
    }
    let mut by_variant: BTreeMap<&str, &str> = BTreeMap::new();
    for (variant, id) in &arms {
        if by_variant.insert(variant.as_str(), id.as_str()).is_some() {
            return Err(drifted("id()"));
        }
    }
    let mut causes = Vec::with_capacity(members.len());
    for member in &members {
        let id = by_variant
            .get(member.as_str())
            .ok_or_else(|| drifted("id()"))?;
        if id.is_empty()
            || !id
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
        {
            return Err(drifted("id()"));
        }
        causes.push((*id).to_string());
    }
    Ok(GateFacts {
        causes,
        manifest_schema: const_str_value(manifest_text, "MANIFEST_SCHEMA")?,
        taxonomy_version: const_str_value(taxonomy_text, "TAXONOMY_VERSION")?,
        verdict_schema_version: const_str_value(taxonomy_text, "VERDICT_SCHEMA_VERSION")?,
    })
}

fn toml_at(text: &str, block: &str) -> Result<toml::Value, FactsError> {
    toml::from_str::<toml::Value>(text)
        .map_err(|_| FactsError::Drift(format!("bloque ilegible: {block}")))
}

/// `engine{workspace_version, bevy, msrv}` from the identity sources of
/// `contract.toml` (all relative to `--main-root`).
pub fn derive_engine(main_root: &Path, cfg: &ContractCfg) -> Result<EngineFacts, FactsError> {
    let workspace_text = read_source(main_root, &cfg.identity.engine_version_source)?;
    let workspace = toml_at(&workspace_text, "workspace Cargo.toml")?;
    let workspace_version = workspace
        .get("workspace")
        .and_then(|w| w.get("package"))
        .and_then(|p| p.get("version"))
        .and_then(|v| v.as_str())
        .ok_or_else(|| drifted("workspace version"))?
        .to_string();

    let bevy_text = read_source(main_root, &cfg.identity.bevy_source)?;
    let bevy_toml = toml_at(&bevy_text, "workspace bevy pin")?;
    let bevy_dep = bevy_toml
        .get("workspace")
        .and_then(|w| w.get("dependencies"))
        .and_then(|d| d.get("bevy"))
        .ok_or_else(|| drifted("bevy pin"))?;
    let bevy = match bevy_dep {
        toml::Value::String(raw) => raw.clone(),
        toml::Value::Table(table) => table
            .get("version")
            .and_then(|v| v.as_str())
            .ok_or_else(|| drifted("bevy pin"))?
            .to_string(),
        _ => return Err(drifted("bevy pin")),
    };

    let msrv_text = read_source(main_root, &cfg.identity.msrv_source)?;
    let msrv_toml = toml_at(&msrv_text, "game-core rust-version")?;
    let msrv = msrv_toml
        .get("package")
        .and_then(|p| p.get("rust-version"))
        .and_then(|v| v.as_str())
        .ok_or_else(|| drifted("msrv"))?
        .to_string();

    Ok(EngineFacts {
        workspace_version,
        bevy,
        msrv,
    })
}

/// Derives every group of `facts/surfaces.json` from the closed constants.
pub fn derive_facts(main_root: &Path, cfg: &ContractCfg) -> Result<Facts, FactsError> {
    let schema_text = read_source(main_root, SCHEMA_RS)?;
    let surface_items = derive_surface(&schema_text)?;
    let content_schema = const_str_value(&schema_text, "SCHEMA_VERSION_LABEL")?;

    let error_text = read_source(main_root, ERROR_RS)?;
    let error_codes = derive_error_codes(&error_text)?;

    let protocol_text = read_source(main_root, PROTOCOL_RS)?;
    let runtime_methods = derive_runtime_methods(&protocol_text)?;

    let taxonomy_text = read_source(main_root, TAXONOMY_RS)?;
    let manifest_text = read_source(main_root, MANIFEST_RS)?;
    let gate = derive_gate(&taxonomy_text, &manifest_text)?;

    let engine = derive_engine(main_root, cfg)?;

    Ok(Facts {
        content_schema,
        engine,
        error_codes,
        gate,
        runtime_methods,
        surface_items,
    })
}

/// The canonical `facts/surfaces.json` object (keys lexicographic at every level).
pub fn facts_to_json(facts: &Facts) -> Value {
    json!({
        "content_schema": facts.content_schema,
        "engine": {
            "bevy": facts.engine.bevy,
            "msrv": facts.engine.msrv,
            "workspace_version": facts.engine.workspace_version,
        },
        "error_codes": facts.error_codes,
        "gate": {
            "causes": facts.gate.causes,
            "manifest_schema": facts.gate.manifest_schema,
            "taxonomy_version": facts.gate.taxonomy_version,
            "verdict_schema_version": facts.gate.verdict_schema_version,
        },
        "runtime_methods": facts.runtime_methods,
        "surface_items": facts
            .surface_items
            .iter()
            .map(|item| json!({"kind": item.kind, "name": item.name}))
            .collect::<Vec<Value>>(),
    })
}

/// Mechanical coverage assertion (§4.3): every derived name must appear as a
/// byte substring (case-sensitive) in the chapter its `[coverage]` mapping
/// names. Missing ⇒ `contract_source_drift` with
/// `coverage: sin citar en <cap>: <items ordenados>`.
pub fn check_coverage(
    cfg: &ContractCfg,
    contract_root: &Path,
    facts: &Facts,
) -> Result<(), FactsError> {
    let groups: [(&str, Vec<String>); 4] = [
        (
            &cfg.coverage.surface,
            facts.surface_items.iter().map(|i| i.name.clone()).collect(),
        ),
        (&cfg.coverage.error_codes, facts.error_codes.clone()),
        (&cfg.coverage.runtime_methods, facts.runtime_methods.clone()),
        (&cfg.coverage.gate_causes, facts.gate.causes.clone()),
    ];

    // chapter → missing items, groups appended in fixed order (deterministic).
    let mut missing_by_chapter: Vec<(String, Vec<String>)> = Vec::new();
    for (chapter, names) in groups {
        let path = contract_root.join("src").join("chapters").join(chapter);
        let text = match std::fs::read(&path) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Err(FactsError::Internal(format!(
                    "capítulo de cobertura ausente: {chapter}"
                )))
            }
            Err(_) => {
                return Err(FactsError::Internal(format!(
                    "capítulo de cobertura ilegible: {chapter}"
                )))
            }
            Ok(bytes) => String::from_utf8(bytes).map_err(|_| {
                FactsError::Internal(format!("capítulo de cobertura ilegible: {chapter}"))
            })?,
        };
        let absent: Vec<String> = names
            .into_iter()
            .filter(|name| !text.contains(name.as_str()))
            .collect();
        if absent.is_empty() {
            continue;
        }
        match missing_by_chapter.iter_mut().find(|(c, _)| c == chapter) {
            Some((_, items)) => items.extend(absent),
            None => missing_by_chapter.push((chapter.to_string(), absent)),
        }
    }

    if missing_by_chapter.is_empty() {
        return Ok(());
    }
    let parts: Vec<String> = missing_by_chapter
        .iter()
        .map(|(chapter, items)| format!("sin citar en {chapter}: {}", items.join(", ")))
        .collect();
    Err(FactsError::Drift(format!("coverage: {}", parts.join("; "))))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCHEMA: &str = r#"//! doc
pub const SCHEMA_VERSION_LABEL: &str = "1.0";

pub const SURFACE: &[SurfaceItem] = &[
    SurfaceItem { name: "Alpha", kind: "struct" },
    SurfaceItem { name: "Beta", kind: "enum" },
];
"#;

    #[test]
    fn surface_block_derives_in_order() {
        let items = derive_surface(SCHEMA).expect("surface");
        assert_eq!(
            items,
            vec![
                SurfaceItemF {
                    name: "Alpha".into(),
                    kind: "struct".into()
                },
                SurfaceItemF {
                    name: "Beta".into(),
                    kind: "enum".into()
                },
            ]
        );
        assert_eq!(
            const_str_value(SCHEMA, "SCHEMA_VERSION_LABEL").unwrap(),
            "1.0"
        );
    }

    #[test]
    fn surface_without_terminator_is_drift() {
        let broken = SCHEMA.replace("];", "");
        let err = derive_surface(&broken).expect_err("must drift");
        assert_eq!(err, FactsError::Drift("bloque ilegible: SURFACE".into()));
    }

    #[test]
    fn surface_bad_line_shape_is_drift() {
        let broken = SCHEMA.replace(
            "    SurfaceItem { name: \"Beta\", kind: \"enum\" },",
            "    SurfaceItem { name: \"Beta\" },",
        );
        assert!(matches!(derive_surface(&broken), Err(FactsError::Drift(_))));
    }

    #[test]
    fn error_codes_derive_in_order() {
        let text = "pub const ERROR_CODES: &[&str] = &[\n    \"one\",\n    \"two\",\n];\n";
        assert_eq!(derive_error_codes(text).unwrap(), vec!["one", "two"]);
        assert!(derive_error_codes("pub const ERROR_CODES: &[&str] = &[").is_err());
    }

    #[test]
    fn runtime_methods_resolve_through_all() {
        let text = r#"pub mod methods {
    pub const STATUS: &str = "status";
    pub const START: &str = "start";
    pub const LOAD_SIDE: &str = "load-side";

    pub const ALL: &[&str] = &[
        STATUS,
        START,
        LOAD_SIDE,
    ];
}
"#;
        assert_eq!(
            derive_runtime_methods(text).unwrap(),
            vec!["status", "start", "load-side"]
        );
    }

    #[test]
    fn unresolved_method_identifier_is_drift() {
        let text = r#"pub mod methods {
    pub const STATUS: &str = "status";
    pub const ALL: &[&str] = &[
        STATUS,
        GHOST,
    ];
}
"#;
        assert_eq!(
            derive_runtime_methods(text),
            Err(FactsError::Drift("bloque ilegible: methods::ALL".into()))
        );
    }

    #[test]
    fn gate_crosses_all_with_id_arms() {
        let taxonomy = r#"pub const TAXONOMY_VERSION: &str = "2";
pub const VERDICT_SCHEMA_VERSION: &str = "1.1";

impl Cause {
    pub const ALL: [Cause; 2] = [
        Cause::Alpha,
        Cause::Beta,
    ];

    pub fn id(self) -> &'static str {
        match self {
            Cause::Alpha => "alpha_fail",
            Cause::Beta => "beta_fail",
        }
    }
}
"#;
        let manifest = "pub const MANIFEST_SCHEMA: &str = \"1.2\";\n";
        let gate = derive_gate(taxonomy, manifest).expect("gate");
        assert_eq!(gate.causes, vec!["alpha_fail", "beta_fail"]);
        assert_eq!(gate.manifest_schema, "1.2");
        assert_eq!(gate.taxonomy_version, "2");
        assert_eq!(gate.verdict_schema_version, "1.1");
    }

    #[test]
    fn gate_count_mismatch_is_drift() {
        let taxonomy = r#"impl Cause {
    pub const ALL: [Cause; 3] = [
        Cause::Alpha,
        Cause::Beta,
    ];

    pub fn id(self) -> &'static str {
        match self {
            Cause::Alpha => "alpha_fail",
            Cause::Beta => "beta_fail",
        }
    }
}
"#;
        assert_eq!(
            derive_gate(taxonomy, ""),
            Err(FactsError::Drift("bloque ilegible: Cause::ALL".into()))
        );
    }

    #[test]
    fn gate_missing_arm_is_drift() {
        let taxonomy = r#"impl Cause {
    pub const ALL: [Cause; 1] = [
        Cause::Alpha,
    ];

    pub fn id(self) -> &'static str {
        match self {
            Cause::Ghost => "ghost",
        }
    }
}
"#;
        let err = derive_gate(taxonomy, "").expect_err("must drift");
        assert_eq!(err, FactsError::Drift("bloque ilegible: id()".into()));
    }

    #[test]
    fn facts_json_is_key_sorted_and_complete() {
        let facts = Facts {
            content_schema: "1.0".into(),
            engine: EngineFacts {
                workspace_version: "0.1.0".into(),
                bevy: "0.20.0".into(),
                msrv: "1.97.1".into(),
            },
            error_codes: vec!["not_found".into()],
            gate: GateFacts {
                causes: vec!["alpha".into()],
                manifest_schema: "1.2".into(),
                taxonomy_version: "2".into(),
                verdict_schema_version: "1.1".into(),
            },
            runtime_methods: vec!["status".into()],
            surface_items: vec![SurfaceItemF {
                name: "Alpha".into(),
                kind: "struct".into(),
            }],
        };
        let value = facts_to_json(&facts);
        let line = serde_json::to_string(&value).unwrap();
        let keys: Vec<&str> = value
            .as_object()
            .unwrap()
            .keys()
            .map(|k| k.as_str())
            .collect();
        assert_eq!(
            keys,
            [
                "content_schema",
                "engine",
                "error_codes",
                "gate",
                "runtime_methods",
                "surface_items"
            ]
        );
        assert!(line.contains("\"bevy\":\"0.20.0\""));
        assert!(line.contains("\"kind\":\"struct\",\"name\":\"Alpha\""));
    }
}
