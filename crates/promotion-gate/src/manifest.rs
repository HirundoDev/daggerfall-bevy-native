//! `side-project.toml` — schema v1.3 parsing and the §2 validation table
//! (plan `I1-INFRA-1-TRUST-DUAL-SIDE-PROJECTS.md` §2), gate step 1.
//!
//! A manifest that fails any check here yields `manifest_invalid`; a manifest
//! that parses and validates but is not declared `ready` yields `side_not_ready`.
//!
//! Schema v1.2 (F4-B) added the mandatory `[preview]` block: a side that claims
//! `ready` must also declare how its preview is launched (argv without shell)
//! and how long the gate may wait for the runtime `status` of that preview.
//!
//! Schema v1.3 (F6-D, DESIGN-v1 §12.2) adds the mandatory `[contract]` block:
//! the side declares which contract it composed against (`version` + bundle
//! `sha256`), mirroring its `contract.lock`. Step 0 is what *verifies* that
//! claim against the main's releases; step 1 only requires the declaration to
//! be well-formed.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use semver::{Version, VersionReq};
use serde::Deserialize;

use crate::main_identity::MainIdentity;

/// The manifest file name at the root of a side workspace (plan §2).
pub const MANIFEST_FILE: &str = "side-project.toml";

/// Manifest schema this gate reads (plan §2 `schema_version`).
///
/// v1.1 → v1.2 (F4-B): `[preview]` is mandatory. A v1.1 side is rejected at
/// step 1 with `manifest_invalid`, never silently promoted without a preview.
///
/// v1.2 → v1.3 (F6-D): `[contract]` is mandatory. A v1.2 side is rejected at
/// step 1 with `manifest_invalid`, never promoted without declaring which
/// contract it composed against.
pub const MANIFEST_SCHEMA: &str = "1.3";

/// Placeholders the gate substitutes in `preview.argv` (and only these).
///
/// Substitution happens **after** the argv array is taken verbatim from the
/// manifest and **before** `exec`: no shell, no globbing, no environment.
pub const PREVIEW_PLACEHOLDERS: [&str; 3] = ["{side}", "{socket}", "{instance}"];

/// Closed grant catalog the gate accepts (plan §2 `capabilities.grants`;
/// derived from the plan surfaces §3.2 content query/overlay and §3.3
/// control/observation/probes). F1 never shipped a catalog, so the gate owns
/// this list until one does.
pub const GRANT_CATALOG: [&str; 5] = [
    "content:read",
    "content:write",
    "runtime:observe",
    "runtime:control",
    "runtime:probes",
];

/// `side-project.toml`, schema v1.3. Unknown keys are rejected: a typo in a
/// trust-relevant file must fail loudly, not silently default.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub schema_version: String,
    pub side: Side,
    pub compat: Compat,
    /// Schema v1.3 (F6-D): which contract this side composed against.
    /// Step 0 verifies the same facts against the lock and the main's releases.
    pub contract: ContractDecl,
    pub plugin: Plugin,
    pub capabilities: Capabilities,
    #[serde(default)]
    pub reads: Reads,
    pub writes: Writes,
    pub probes: Probes,
    pub readiness: Readiness,
    pub preview: Preview,
}

/// `[contract]` block (schema v1.3): the side's own declaration of the
/// contract it composed against, mirroring `contract.lock`.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContractDecl {
    /// Strict `M.m.p` contract version (`contract.toml`'s `contract_version`).
    pub version: String,
    /// sha256 of `CONTRACT.json` (the bundle root), 64 lowercase hex.
    pub sha256: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Side {
    /// `^[a-z][a-z0-9-]{2,40}$`, unique in the fleet.
    pub id: String,
    /// Semver of the side's crate.
    pub version: String,
    pub display_name: String,
    /// Responsible human; without an owner there is no promotion (plan §2).
    pub owner: String,
    pub kind: SideKind,
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum SideKind {
    Plugin,
    ContentPack,
    Tool,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Compat {
    /// Semver requirement that must contain the current `game-core`.
    pub core: String,
    /// Bevy pin; must equal the main workspace pin (plan §7.3).
    pub bevy: String,
    /// Manifest schema the side understands.
    pub manifest: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Plugin {
    pub entry_crate: String,
    pub plugin_type: String,
    pub load_order: i64,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Capabilities {
    pub grants: Vec<String>,
}

#[derive(Debug, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct Reads {
    #[serde(default)]
    pub resources: Vec<String>,
    #[serde(default)]
    pub events: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Writes {
    pub paths: Vec<String>,
    pub state: WriteState,
    #[serde(default)]
    pub overlays: Vec<String>,
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum WriteState {
    SideOwned,
    /// Reserved: OD-2 (a) keeps side state out of `SaveStateV2`, so a v1
    /// manifest cannot claim a save section.
    SaveSection,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Probes {
    /// Probes that must exist, be seed-parameterised and deterministic.
    pub required: Vec<String>,
    /// OD-8 (b): argv arrays, no shell, `{seed}` placeholder, hard timeout.
    #[serde(default)]
    pub commands: BTreeMap<String, ProbeCommand>,
}

/// One probe command: an argv array executed directly (no shell) and its
/// per-run timeout in seconds.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProbeCommand {
    pub argv: Vec<String>,
    pub timeout_secs: u64,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Readiness {
    pub state: ReadinessState,
    pub report: String,
}

/// Step 6 launch contract of the side's preview (schema v1.2).
///
/// The gate owns neither the preview binary nor its composition: it only knows
/// how to start it and where to look. `argv` is executed **directly** (never
/// through a shell) after placeholder substitution, so a side cannot smuggle a
/// pipeline, a redirect or an environment expansion into the gate.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Preview {
    /// Argv array, no shell. Must contain `{socket}`; `{side}` and `{instance}`
    /// are substituted too. `argv[0]` may be bare: the gate resolves it against
    /// the side root, then beside the gate binary, then `PATH`.
    pub argv: Vec<String>,
    /// Runtime instance id, `^[a-z0-9][a-z0-9._-]{0,63}$` (same grammar as the
    /// control socket contract). The gate asks for exactly this instance.
    pub instance: String,
    /// Budget for "spawned → socket published → `status` accepted", in seconds.
    pub timeout_secs: u64,
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum ReadinessState {
    NotDeclared,
    Ready,
    Promoted,
    Rejected,
}

/// Loaded manifest plus the raw bytes hashed into the verdict row.
#[derive(Debug)]
pub struct LoadedManifest {
    pub manifest: Manifest,
    pub raw: Vec<u8>,
    pub path: PathBuf,
}

/// Parse `side-project.toml`. Any I/O or TOML/shape error is `manifest_invalid`.
pub fn parse(side_root: &Path) -> Result<LoadedManifest, String> {
    let path = side_root.join(MANIFEST_FILE);
    let raw = std::fs::read(&path)
        .map_err(|e| format!("no se pudo leer {}: {e}", path.display()))?;
    let text =
        String::from_utf8(raw.clone()).map_err(|_| "side-project.toml no es UTF-8".to_string())?;
    let manifest: Manifest = toml::from_str(&text)
        .map_err(|e| format!("{} no cumple schema v{MANIFEST_SCHEMA}: {e}", MANIFEST_FILE))?;
    Ok(LoadedManifest { manifest, raw, path })
}

/// Run the §2 validation table plus fleet uniqueness. Returns `Ok(side_id)` or
/// the first failing rule as `manifest_invalid` detail.
pub fn validate(loaded: &LoadedManifest, side_root: &Path, fleet_dir: &Path) -> Result<(), String> {
    let m = &loaded.manifest;

    if m.schema_version != MANIFEST_SCHEMA {
        return Err(format!(
            "schema_version={:?} no soportado (gate lee {MANIFEST_SCHEMA})",
            m.schema_version
        ));
    }
    check_contract_decl(&m.contract)?;
    if !is_side_id(&m.side.id) {
        return Err(format!(
            "side.id={:?} no cumple ^[a-z][a-z0-9-]{{2,40}}$",
            m.side.id
        ));
    }
    Version::parse(&m.side.version)
        .map_err(|e| format!("side.version={:?} no es semver: {e}", m.side.version))?;
    if m.side.owner.trim().is_empty() {
        return Err("side.owner vacío: sin owner no hay promoción (plan §2)".into());
    }
    if m.compat.manifest != MANIFEST_SCHEMA {
        return Err(format!(
            "compat.manifest={:?} no soportado (side y gate deben hablar {MANIFEST_SCHEMA})",
            m.compat.manifest
        ));
    }
    VersionReq::parse(&m.compat.core)
        .map_err(|e| format!("compat.core={:?} no es un rango semver: {e}", m.compat.core))?;
    crate::main_identity::BevyVersion::parse(&m.compat.bevy)
        .map_err(|e| format!("compat.bevy={:?}: {e}", m.compat.bevy))?;

    for grant in &m.capabilities.grants {
        if !GRANT_CATALOG.contains(&grant.as_str()) {
            return Err(format!(
                "capabilities.grants contiene {grant:?} fuera del catálogo cerrado {GRANT_CATALOG:?}"
            ));
        }
    }

    for p in &m.writes.paths {
        if !is_side_local_path(p) {
            return Err(format!(
                "writes.paths contiene {p:?}: solo rutas bajo el workspace del side (plan §2)"
            ));
        }
    }
    if m.writes.state == WriteState::SaveSection {
        return Err(
            "writes.state=save-section: OD-2 (a) mantiene el estado del side fuera de SaveStateV2"
                .into(),
        );
    }

    if m.probes.required.is_empty() {
        return Err("probes.required vacío: un side sin probes no puede declararse ready".into());
    }
    for name in &m.probes.required {
        if let Some(cmd) = m.probes.commands.get(name) {
            if cmd.argv.is_empty() {
                return Err(format!("probes.commands.{name}.argv vacío"));
            }
            if cmd.argv.iter().any(|a| a.is_empty()) {
                return Err(format!("probes.commands.{name}.argv contiene argumento vacío"));
            }
            if cmd.timeout_secs == 0 {
                return Err(format!("probes.commands.{name}.timeout_secs debe ser >= 1"));
            }
        }
    }

    check_preview(&m.preview)?;

    check_entry_crate(&m.plugin.entry_crate, side_root)?;
    check_plugin_type(&m.plugin.plugin_type, side_root, &m.plugin.entry_crate)?;
    check_fleet_uniqueness(&m.side.id, side_root, fleet_dir)?;

    if m.readiness.report.trim().is_empty() {
        return Err("readiness.report vacío".into());
    }

    Ok(())
}

/// Step 1 (schema v1.3): the `[contract]` block must be well-formed. Whether
/// the claim is *true* is step 0's job, not this one's.
fn check_contract_decl(decl: &ContractDecl) -> Result<(), String> {
    let strict = crate::contract::parse_version(&decl.version);
    if strict.is_none() {
        return Err(format!(
            "contract.version={:?} no es un M.m.p estricto (schema v1.3)",
            decl.version
        ));
    }
    if !crate::contract::is_sha256_hex(&decl.sha256) {
        return Err(format!(
            "contract.sha256={:?} no es un sha256 de 64 hex (schema v1.3)",
            decl.sha256
        ));
    }
    Ok(())
}

/// Step 1 (schema v1.3): the `[preview]` block must be launchable as declared.
fn check_preview(preview: &Preview) -> Result<(), String> {
    if preview.argv.is_empty() {
        return Err("preview.argv vacío: el gate no sabría cómo arrancar el preview".into());
    }
    if preview.argv.iter().any(|a| a.is_empty()) {
        return Err("preview.argv contiene argumento vacío".into());
    }
    if preview.timeout_secs == 0 {
        return Err("preview.timeout_secs debe ser >= 1".into());
    }
    if !is_instance_id(&preview.instance) {
        return Err(format!(
            "preview.instance={:?} no cumple ^[a-z0-9][a-z0-9._-]{{0,63}}$",
            preview.instance
        ));
    }
    if !preview.argv.iter().any(|a| a.contains("{socket}")) {
        return Err(
            "preview.argv no contiene {socket}: el preview no sabría dónde publicar su socket"
                .into(),
        );
    }
    for arg in &preview.argv {
        check_placeholders(arg)?;
    }
    Ok(())
}

/// Every `{` in a preview argument must open one of [`PREVIEW_PLACEHOLDERS`].
///
/// A brace typo (`{sockett}`, `{sock`) would otherwise reach the preview process
/// verbatim and fail as an opaque runtime error instead of at step 1.
fn check_placeholders(arg: &str) -> Result<(), String> {
    let mut rest = arg;
    while let Some(open) = rest.find('{') {
        let tail = &rest[open..];
        let close = tail
            .find('}')
            .ok_or_else(|| format!("preview.argv {arg:?}: placeholder sin cerrar"))?;
        let candidate = &tail[..=close];
        if !PREVIEW_PLACEHOLDERS.contains(&candidate) {
            return Err(format!(
                "preview.argv {arg:?}: placeholder {candidate:?} desconocido (solo {PREVIEW_PLACEHOLDERS:?})"
            ));
        }
        rest = &tail[close + 1..];
    }
    Ok(())
}

/// `^[a-z0-9][a-z0-9._-]{0,63}$` — the instance grammar of the control socket.
pub fn is_instance_id(raw: &str) -> bool {
    let mut chars = raw.chars();
    match chars.next() {
        Some(c) if c.is_ascii_lowercase() || c.is_ascii_digit() => {}
        _ => return false,
    }
    raw.len() <= 64
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '.' | '_' | '-'))
}

/// Step 1 second half: `readiness.state == ready` (plan §4.3.1).
pub fn require_ready(loaded: &LoadedManifest) -> Result<(), String> {
    if loaded.manifest.readiness.state == ReadinessState::Ready {
        Ok(())
    } else {
        Err("readiness.state != ready (el side no declara evidencia completa)".to_string())
    }
}

/// Step 2: `compat.core` contains the current core; `compat.bevy` equals main's.
/// Returns the typed cause so the verdict row says which pin broke.
pub fn check_compat(
    loaded: &LoadedManifest,
    identity: &MainIdentity,
) -> Result<(), (crate::taxonomy::Cause, String)> {
    use crate::taxonomy::Cause;
    let m = &loaded.manifest;
    let req = VersionReq::parse(&m.compat.core).map_err(|e| {
        (
            Cause::ManifestInvalid,
            format!("compat.core={:?}: {e}", m.compat.core),
        )
    })?;
    if !req.matches(&identity.core_version) {
        return Err((
            Cause::CoreIncompatible,
            format!(
                "compat.core={:?} no contiene la versión actual de game-core {}",
                m.compat.core, identity.core_version
            ),
        ));
    }
    let bevy_ok = identity.bevy.matches(&m.compat.bevy).map_err(|e| {
        (Cause::ManifestInvalid, format!("compat.bevy={:?}: {e}", m.compat.bevy))
    })?;
    if !bevy_ok {
        return Err((
            Cause::BevyMismatch,
            format!(
                "compat.bevy={:?} difiere del bevy del main {}.{}",
                m.compat.bevy, identity.bevy.major, identity.bevy.minor
            ),
        ));
    }
    Ok(())
}

/// `^[a-z][a-z0-9-]{2,40}$` without a regex crate.
pub fn is_side_id(id: &str) -> bool {
    let bytes = id.as_bytes();
    if !(3..=41).contains(&bytes.len()) {
        return false;
    }
    let first = bytes[0];
    if !first.is_ascii_lowercase() {
        return false;
    }
    bytes[1..]
        .iter()
        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'-')
}

/// A write path stays inside the side workspace: relative, no `..`, or rooted
/// at the `${side}` placeholder (plan §2 `writes.paths`).
fn is_side_local_path(path: &str) -> bool {
    if path.starts_with('/') {
        return false;
    }
    if path == "${side}" || path.starts_with("${side}/") {
        return !path[7..].contains("..");
    }
    if path.starts_with("${") {
        return false;
    }
    !path.split('/').any(|c| c == "..")
}

/// Collect `(package name, package dir)` for the side workspace: the package
/// itself, or the literal `[workspace.members]` (globs are not expanded in v1).
fn side_packages(side_root: &Path) -> Result<Vec<(String, PathBuf)>, String> {
    let manifest_path = side_root.join("Cargo.toml");
    let text = std::fs::read_to_string(&manifest_path)
        .map_err(|e| format!("no se pudo leer {}: {e}", manifest_path.display()))?;
    let value: toml::Value = toml::from_str(&text)
        .map_err(|e| format!("{} no es TOML válido: {e}", manifest_path.display()))?;
    let mut packages = Vec::new();
    if let Some(name) = value.get("package").and_then(|p| p.get("name")).and_then(|n| n.as_str()) {
        packages.push((name.to_string(), side_root.to_path_buf()));
    }
    if let Some(members) = value.get("workspace").and_then(|w| w.get("members")) {
        if let Some(list) = members.as_array() {
            for member in list.iter().filter_map(|m| m.as_str()) {
                if member.contains('*') {
                    continue; // v1: literal members only (documented).
                }
                let dir = side_root.join(member);
                if let Ok(text) = std::fs::read_to_string(dir.join("Cargo.toml")) {
                    if let Ok(member_value) = toml::from_str::<toml::Value>(&text) {
                        if let Some(name) = member_value
                            .get("package")
                            .and_then(|p| p.get("name"))
                            .and_then(|n| n.as_str())
                        {
                            packages.push((name.to_string(), dir));
                        }
                    }
                }
            }
        }
    }
    Ok(packages)
}

/// §2: `plugin.entry_crate` must exist as a package of the side workspace.
fn check_entry_crate(entry_crate: &str, side_root: &Path) -> Result<(), String> {
    let packages = side_packages(side_root)?;
    if packages.iter().any(|(name, _)| name == entry_crate) {
        Ok(())
    } else {
        Err(format!(
            "plugin.entry_crate={entry_crate:?} no es ningún paquete del side ({:?})",
            packages.iter().map(|(n, _)| n.as_str()).collect::<Vec<_>>()
        ))
    }
}

/// §2: `plugin.plugin_type` must be a *public* item of the entry crate. v1 is
/// a textual check over the crate's `src/` (documented in PROMOTION-GATE-v1);
/// compiling the side is F4-B's preview job, not the gate's.
fn check_plugin_type(plugin_type: &str, side_root: &Path, entry_crate: &str) -> Result<(), String> {
    let packages = side_packages(side_root)?;
    let (_, crate_dir) = packages
        .iter()
        .find(|(name, _)| name == entry_crate)
        .ok_or_else(|| format!("plugin.entry_crate={entry_crate:?} no encontrado"))?;
    let src = crate_dir.join("src");
    let mut files = Vec::new();
    collect_rs_files(&src, &mut files)
        .map_err(|e| format!("no se pudo leer {}: {e}", src.display()))?;
    if files.is_empty() {
        return Err(format!("plugin.entry_crate={entry_crate:?} sin fuentes en {}", src.display()));
    }
    for file in files {
        if let Ok(text) = std::fs::read_to_string(&file) {
            if line_declares_public(&text, plugin_type) {
                return Ok(());
            }
        }
    }
    Err(format!(
        "plugin.plugin_type={plugin_type:?} no es un item público en el crate {entry_crate:?}"
    ))
}

fn collect_rs_files(dir: &Path, out: &mut Vec<PathBuf>) -> std::io::Result<()> {
    if !dir.exists() {
        return Ok(());
    }
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect_rs_files(&path, out)?;
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
    Ok(())
}

/// True when `name` appears as a whole word on a line that also says `pub`.
fn line_declares_public(text: &str, name: &str) -> bool {
    text.lines().any(|line| {
        if !line.contains("pub ") {
            return false;
        }
        let mut rest = line;
        while let Some(pos) = rest.find(name) {
            let before = rest.as_bytes().get(pos.wrapping_sub(1));
            let after = rest.as_bytes().get(pos + name.len());
            let before_ok = pos == 0 || before.is_some_and(|b| !is_ident_byte(*b));
            let after_ok = after.is_none_or(|b| !is_ident_byte(*b));
            if before_ok && after_ok {
                return true;
            }
            rest = &rest[pos + name.len()..];
        }
        false
    })
}

fn is_ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'-'
}

/// Fleet uniqueness (plan §2): every sibling workspace declaring a parseable
/// `side-project.toml` must use a different `side.id`. The fleet is the side's
/// parent directory unless `--fleet-dir` says otherwise.
fn check_fleet_uniqueness(side_id: &str, side_root: &Path, fleet_dir: &Path) -> Result<(), String> {
    let entries = match std::fs::read_dir(fleet_dir) {
        Ok(e) => e,
        Err(_) => return Ok(()), // no fleet directory -> nothing to collide with
    };
    let side_root = std::fs::canonicalize(side_root).unwrap_or_else(|_| side_root.to_path_buf());
    for entry in entries.flatten() {
        let dir = entry.path();
        if !dir.is_dir() {
            continue;
        }
        let canonical = std::fs::canonicalize(&dir).unwrap_or(dir.clone());
        if canonical == side_root {
            continue;
        }
        let manifest_path = dir.join(MANIFEST_FILE);
        let Ok(text) = std::fs::read_to_string(&manifest_path) else {
            continue;
        };
        let Ok(value) = toml::from_str::<toml::Value>(&text) else {
            continue; // a broken sibling is not this side's violation
        };
        if let Some(other) = value
            .get("side")
            .and_then(|s| s.get("id"))
            .and_then(|i| i.as_str())
        {
            if other == side_id {
                return Err(format!(
                    "side.id={side_id:?} ya declarado por {} (unicidad en la flota, plan §2)",
                    dir.display()
                ));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn minimal_manifest(id: &str) -> String {
        format!(
            r#"
schema_version = "1.3"
[side]
id = "{id}"
version = "0.1.0"
display_name = "x"
owner = "owner"
kind = "plugin"
[compat]
core = "^0.1"
bevy = "0.20"
manifest = "1.3"
[contract]
version = "1.0.0"
sha256 = "8b8b16c62d314d623a8493386ea67cb750c001683600f7622b2b02c5495bb604"
[plugin]
entry_crate = "x"
plugin_type = "XPlugin"
load_order = 1
[capabilities]
grants = ["content:read"]
[reads]
resources = []
events = []
[writes]
paths = ["${{side}}/cache"]
state = "side-owned"
overlays = []
[probes]
required = ["smoke"]
[probes.commands.smoke]
argv = ["printf", "{{seed}}"]
timeout_secs = 5
[preview]
argv = ["preview-bin", "--socket", "{{socket}}", "--instance", "{{instance}}"]
instance = "preview"
timeout_secs = 15
[readiness]
state = "ready"
report = "readiness-report.md"
"#
        )
    }

    #[test]
    fn preview_block_is_parsed_and_validated() {
        let parsed: Manifest = toml::from_str(&minimal_manifest("f4b-unit")).unwrap();
        assert_eq!(parsed.preview.instance, "preview");
        assert_eq!(parsed.preview.timeout_secs, 15);
        assert_eq!(parsed.preview.argv[0], "preview-bin");
        check_preview(&parsed.preview).unwrap();
    }

    #[test]
    fn preview_rules_fail_loudly() {
        let base: Manifest = toml::from_str(&minimal_manifest("f4b-unit")).unwrap();
        // argv must carry {socket}.
        let mut p = Preview {
            argv: vec!["preview-bin".into()],
            instance: base.preview.instance.clone(),
            timeout_secs: 1,
        };
        assert!(check_preview(&p).unwrap_err().contains("{socket}"));
        // unknown placeholder (with {socket} satisfied, so this rule is reached)
        p.argv = vec!["preview-bin".into(), "{socket}".into(), "{sockett}".into()];
        assert!(check_preview(&p).unwrap_err().contains("desconocido"));
        // unterminated placeholder
        p.argv = vec!["preview-bin".into(), "{socket}".into(), "{sock".into()];
        assert!(check_preview(&p).unwrap_err().contains("sin cerrar"));
        // empty argv / empty arg / zero timeout
        p.argv = vec![];
        assert!(check_preview(&p).is_err());
        p.argv = vec!["preview-bin".into(), String::new(), "{socket}".into()];
        assert!(check_preview(&p).is_err());
        p.argv = vec!["preview-bin".into(), "{socket}".into()];
        p.timeout_secs = 0;
        assert!(check_preview(&p).is_err());
    }

    #[test]
    fn instance_id_grammar() {
        assert!(is_instance_id("preview"));
        assert!(is_instance_id("i1.preview_2-a"));
        assert!(!is_instance_id(""));
        assert!(!is_instance_id("Preview"));
        assert!(!is_instance_id("-preview"));
        assert!(!is_instance_id("/tmp/x"));
        assert!(!is_instance_id(&"a".repeat(65)));
    }

    #[test]
    fn missing_preview_block_is_rejected_by_schema() {
        let raw = minimal_manifest("f4b-unit");
        let cut = raw.split("[preview]").next().unwrap().to_string()
            + "[readiness]\nstate = \"ready\"\nreport = \"readiness-report.md\"\n";
        assert!(toml::from_str::<Manifest>(&cut).is_err());
    }

    /// F6-D: schema 1.3 makes the `[contract]` block mandatory.
    /// A manifest that declares only `[preview]` (schema 1.2) no longer parses:
    /// a side must state which contract it composed against, even as a claim
    /// that step 0 is in charge of verifying.
    #[test]
    fn contract_block_is_mandatory() {
        let raw = minimal_manifest("f4b-unit");
        // Leaves the rest of the manifest intact and removes ONLY the [contract] block.
        let (antes, despues) = raw.split_once("[contract]").expect("el bloque existe");
        let sin_contract =
            antes.to_string() + despues.split_once("[plugin]").expect("[plugin] sigue").1;
        assert!(
            !sin_contract.contains("[contract]"),
            "el bloque fue retirado del manifiesto de prueba"
        );
        assert!(toml::from_str::<Manifest>(&sin_contract).is_err());
    }

    #[test]
    fn side_id_regex() {
        assert!(is_side_id("wiki-daggerfall"));
        assert!(is_side_id("abc"));
        assert!(!is_side_id("Abc"));
        assert!(!is_side_id("ab"));
        assert!(!is_side_id("a-b-c-".repeat(20).as_str()));
        assert!(!is_side_id("9side"));
    }

    #[test]
    fn manifest_parses() {
        let parsed: Manifest = toml::from_str(&minimal_manifest("f4a-unit")).unwrap();
        assert_eq!(parsed.side.id, "f4a-unit");
        assert_eq!(parsed.readiness.state, ReadinessState::Ready);
        assert_eq!(parsed.probes.commands["smoke"].timeout_secs, 5);
        assert_eq!(parsed.writes.state, WriteState::SideOwned);
    }

    #[test]
    fn unknown_key_is_rejected() {
        let raw = minimal_manifest("f4a-unit").replace("kind = \"plugin\"", "kind = \"plugin\"\nextra = 1");
        assert!(toml::from_str::<Manifest>(&raw).is_err());
    }

    #[test]
    fn write_path_rules() {
        assert!(is_side_local_path("${side}/cache"));
        assert!(is_side_local_path("${side}"));
        assert!(is_side_local_path("cache/x"));
        assert!(!is_side_local_path("/etc/passwd"));
        assert!(!is_side_local_path("../main/src"));
        assert!(!is_side_local_path("${side}/../../main"));
        assert!(!is_side_local_path("${other}/x"));
    }

    #[test]
    fn public_item_detection() {
        let src = "pub struct WikiPlugin;\nfn helper() {}\n";
        assert!(line_declares_public(src, "WikiPlugin"));
        assert!(!line_declares_public("struct WikiPlugin;\n", "WikiPlugin"));
        assert!(!line_declares_public("pub struct OtherPlugin;\n", "WikiPlugin"));
        // Identifier boundary: `MyWikiPlugin` is not `WikiPlugin`.
        assert!(!line_declares_public("pub struct MyWikiPlugin;\n", "WikiPlugin"));
    }
}
