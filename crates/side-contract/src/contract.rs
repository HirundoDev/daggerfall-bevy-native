//! Strict parser of the authored source of truth, `<contract-root>/contract.toml`
//! (DESIGN-v1 §3.2, F6-B pack schema).
//!
//! Strictness is the contract: an unknown field, a missing field, a duplicate
//! id/path, a non-SemVer `contract_version` or a non-R4 path means the tool
//! cannot evaluate — every failure surfaces as `internal_error` (INCOMPLETE),
//! never as a silently ignored input.

use std::path::Path;

use serde::Deserialize;

use crate::semver::SemVer;

/// `[identity]`: where the engine facts are derived from (relative to `--main-root`).
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IdentityCfg {
    pub engine_version_source: String,
    pub bevy_source: String,
    pub msrv_source: String,
}

/// One `[[chapter]]` entry of the authored source.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChapterCfg {
    pub path: String,
    pub class: String,
}

/// One `[[anchor]]` entry: a byte-exact promise about a source line span.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnchorCfg {
    pub id: String,
    pub path: String,
    pub symbol: String,
    pub line: u64,
    pub expect: String,
}

/// `[exclusions]`: the retail globs and the policy line they come from.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExclusionsCfg {
    pub globs: Vec<String>,
    pub policy: String,
}

/// `[coverage]`: which chapter must cite each derived fact group (§4.3).
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CoverageCfg {
    pub surface: String,
    pub error_codes: String,
    pub runtime_methods: String,
    pub gate_causes: String,
}

/// The whole `contract.toml`, schema `1`.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContractCfg {
    pub schema_version: String,
    pub contract_version: String,
    pub identity: IdentityCfg,
    #[serde(rename = "chapter")]
    pub chapters: Vec<ChapterCfg>,
    #[serde(rename = "anchor")]
    pub anchors: Vec<AnchorCfg>,
    pub exclusions: ExclusionsCfg,
    pub coverage: CoverageCfg,
}

/// R4: relative POSIX path — non-empty, no absolute prefix, no `.`/`..`
/// component, no `//`, no trailing `/`, no backslash.
pub fn is_rel_posix_path(path: &str) -> bool {
    if path.is_empty()
        || path.starts_with('/')
        || path.ends_with('/')
        || path.contains('\\')
        || path.contains("//")
    {
        return false;
    }
    path.split('/')
        .all(|seg| !seg.is_empty() && seg != "." && seg != "..")
}

impl ContractCfg {
    /// Reads and strictly parses `<contract-root>/contract.toml`.
    /// The error text is deterministic (no absolute paths, no timestamps).
    pub fn load(contract_root: &Path) -> Result<ContractCfg, String> {
        let path = contract_root.join("contract.toml");
        let text = std::fs::read(&path).map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => "contract.toml ausente".to_string(),
            kind => format!("contract.toml ilegible: {kind:?}"),
        })?;
        let text = String::from_utf8(text)
            .map_err(|_| "contract.toml ilegible: UTF-8 inválido".to_string())?;
        Self::parse(&text)
    }

    /// Strict TOML parse + semantic validation.
    pub fn parse(text: &str) -> Result<ContractCfg, String> {
        let cfg: ContractCfg =
            toml::from_str(text).map_err(|e| format!("contract.toml ilegible: {e}"))?;
        cfg.validate()?;
        Ok(cfg)
    }

    /// Semantic rules of the fixed schema (§3.2 of the pack).
    pub fn validate(&self) -> Result<(), String> {
        if self.schema_version != "1" {
            return Err(format!(
                "contract.toml: schema_version {} != \"1\"",
                self.schema_version
            ));
        }
        if SemVer::parse(&self.contract_version).is_none() {
            return Err(format!(
                "contract.toml: contract_version {:?} no es SemVer estricto M.m.p",
                self.contract_version
            ));
        }
        if self.chapters.is_empty() {
            return Err("contract.toml: 0 [[chapter]]".to_string());
        }
        let mut chapter_paths: Vec<&str> = Vec::new();
        for chapter in &self.chapters {
            if chapter.class != "chapter" {
                return Err(format!(
                    "contract.toml: chapter {} con class {:?} != \"chapter\"",
                    chapter.path, chapter.class
                ));
            }
            if !is_rel_posix_path(&chapter.path) {
                return Err(format!(
                    "contract.toml: chapter path {:?} viola R4",
                    chapter.path
                ));
            }
            if chapter_paths.contains(&chapter.path.as_str()) {
                return Err(format!("contract.toml: chapter duplicado {}", chapter.path));
            }
            chapter_paths.push(&chapter.path);
        }
        let mut anchor_ids: Vec<&str> = Vec::new();
        for anchor in &self.anchors {
            if anchor.id.is_empty() {
                return Err("contract.toml: anchor con id vacío".to_string());
            }
            if anchor_ids.contains(&anchor.id.as_str()) {
                return Err(format!("contract.toml: anchor id duplicado {}", anchor.id));
            }
            anchor_ids.push(&anchor.id);
            if !is_rel_posix_path(&anchor.path) {
                return Err(format!(
                    "contract.toml: anchor {} path {:?} viola R4",
                    anchor.id, anchor.path
                ));
            }
            if anchor.line == 0 {
                return Err(format!("contract.toml: anchor {} con line 0", anchor.id));
            }
            if anchor.expect.is_empty() {
                return Err(format!(
                    "contract.toml: anchor {} con expect vacío",
                    anchor.id
                ));
            }
        }
        if self.exclusions.globs.is_empty() {
            return Err("contract.toml: [exclusions].globs vacío".to_string());
        }
        for glob in &self.exclusions.globs {
            if glob.is_empty() || glob.starts_with('/') {
                return Err(format!("contract.toml: glob inválido {glob:?}"));
            }
        }
        if self.exclusions.policy.is_empty() {
            return Err("contract.toml: [exclusions].policy vacío".to_string());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn minimal() -> String {
        r#"schema_version = "1"
contract_version = "1.0.0"

[identity]
engine_version_source = "Cargo.toml"
bevy_source = "Cargo.toml"
msrv_source = "crates/game-core/Cargo.toml"

[[chapter]]
path = "00-what-is-the-game.md"
class = "chapter"

[[anchor]]
id = "test.thing"
path = "src/lib.rs"
symbol = "Thing"
line = 1
expect = "pub struct Thing {"

[exclusions]
globs = ["**/*.BSA"]
policy = "DATA-CLASSIFICATION.md:16-27"

[coverage]
surface = "00-what-is-the-game.md"
error_codes = "00-what-is-the-game.md"
runtime_methods = "00-what-is-the-game.md"
gate_causes = "00-what-is-the-game.md"
"#
        .to_string()
    }

    #[test]
    fn parses_the_minimal_contract() {
        let cfg = ContractCfg::parse(&minimal()).expect("valid contract");
        assert_eq!(cfg.contract_version, "1.0.0");
        assert_eq!(cfg.chapters.len(), 1);
        assert_eq!(cfg.anchors.len(), 1);
        assert_eq!(cfg.anchors[0].id, "test.thing");
    }

    #[test]
    fn unknown_field_is_rejected() {
        let bad = minimal().replace(
            "schema_version = \"1\"",
            "schema_version = \"1\"\nsurprise = true",
        );
        let err = ContractCfg::parse(&bad).expect_err("must reject unknown field");
        assert!(err.contains("ilegible"), "got: {err}");
    }

    #[test]
    fn missing_field_is_rejected() {
        let bad = minimal().replace("policy = \"DATA-CLASSIFICATION.md:16-27\"\n", "");
        let err = ContractCfg::parse(&bad).expect_err("must reject missing field");
        assert!(err.contains("ilegible"), "got: {err}");
    }

    #[test]
    fn bad_semver_is_rejected() {
        let bad = minimal().replace("contract_version = \"1.0.0\"", "contract_version = \"1.0\"");
        let err = ContractCfg::parse(&bad).expect_err("must reject non-SemVer");
        assert!(err.contains("SemVer"), "got: {err}");
    }

    #[test]
    fn duplicate_anchor_id_is_rejected() {
        let mut text = minimal();
        text.push_str(
            "\n[[anchor]]\nid = \"test.thing\"\npath = \"src/other.rs\"\nsymbol = \"X\"\nline = 2\nexpect = \"x\"\n",
        );
        let err = ContractCfg::parse(&text).expect_err("must reject duplicate id");
        assert!(err.contains("duplicado"), "got: {err}");
    }

    #[test]
    fn r4_path_rules() {
        assert!(is_rel_posix_path("chapters/a.md"));
        assert!(is_rel_posix_path("crates/game-core/src/lib.rs"));
        assert!(!is_rel_posix_path("/etc/passwd"));
        assert!(!is_rel_posix_path("../secrets"));
        assert!(!is_rel_posix_path("a/../../b"));
        assert!(!is_rel_posix_path("./a"));
        assert!(!is_rel_posix_path("a//b"));
        assert!(!is_rel_posix_path("a/"));
        assert!(!is_rel_posix_path(""));
        assert!(!is_rel_posix_path("a\\b"));
        assert!(!is_rel_posix_path("."));
    }

    #[test]
    fn class_must_be_chapter() {
        let bad = minimal().replace("class = \"chapter\"", "class = \"readme\"");
        let err = ContractCfg::parse(&bad).expect_err("must reject class");
        assert!(err.contains("class"), "got: {err}");
    }
}
