//! Main-workspace baseline the gate measures against (plan §4.3 step 2).
//!
//! Gathered **before** any step runs: without `main_head`, `game-core`'s
//! current version and the workspace Bevy version no verdict row is complete,
//! so a failure here is `gate_internal_error`, not a defect of the side.

use std::path::Path;
use std::process::Command;

use semver::Version;

/// Bevy is pinned by the main workspace; the side must declare the same
/// `major.minor` (and the same patch when it spells one out).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BevyVersion {
    pub major: u64,
    pub minor: u64,
    pub patch: Option<u64>,
}

impl BevyVersion {
    /// Parse `"0.20"`, `"0.20.0"` or `"0.20.0+build"`.
    pub fn parse(raw: &str) -> Result<Self, String> {
        let core = raw.split('+').next().unwrap_or(raw);
        let mut parts = core.split('.');
        let major = parts
            .next()
            .and_then(|p| p.trim().parse::<u64>().ok())
            .ok_or_else(|| format!("versión bevy sin major: {raw:?}"))?;
        let minor = parts
            .next()
            .and_then(|p| p.trim().parse::<u64>().ok())
            .ok_or_else(|| format!("versión bevy sin minor: {raw:?}"))?;
        let third = parts.next();
        if parts.next().is_some() {
            return Err(format!("bevy con más de 3 componentes: {raw:?}"));
        }
        let patch = match third {
            None => None,
            Some(p) => Some(
                p.trim()
                    .parse::<u64>()
                    .map_err(|_| format!("bevy con patch no numérico: {raw:?}"))?,
            ),
        };
        Ok(BevyVersion { major, minor, patch })
    }

    /// The declared side version matches the main one when `major.minor` agree
    /// and any declared patch agrees too (plan §7.3: exact pin, no range).
    pub fn matches(&self, declared: &str) -> Result<bool, String> {
        let side = BevyVersion::parse(declared)?;
        if side.major != self.major || side.minor != self.minor {
            return Ok(false);
        }
        match (side.patch, self.patch) {
            (Some(a), Some(b)) => Ok(a == b),
            (Some(_), None) => Ok(true),
            (None, _) => Ok(true),
        }
    }
}

/// Baseline identity of the main workspace.
#[derive(Debug, Clone)]
pub struct MainIdentity {
    /// `git rev-parse HEAD` of `main_root`.
    pub head: String,
    /// Current `game-core` version (workspace inheritance resolved).
    pub core_version: Version,
    /// Workspace Bevy pin.
    pub bevy: BevyVersion,
}

/// Read `main_root` and derive the three facts every verdict row needs.
pub fn gather(main_root: &Path) -> Result<MainIdentity, String> {
    let root = main_root.display().to_string();
    let head_output = Command::new("git")
        .args(["-C", &root, "rev-parse", "HEAD"])
        .output()
        .map_err(|e| format!("identidad del main: no se pudo ejecutar git en {root}: {e}"))?;
    if !head_output.status.success() {
        return Err(format!(
            "identidad del main: git rev-parse HEAD falló en {root} ({})",
            String::from_utf8_lossy(&head_output.stderr).trim()
        ));
    }
    let head = String::from_utf8_lossy(&head_output.stdout).trim().to_string();
    if head.is_empty() {
        return Err(format!("identidad del main: git devolvió HEAD vacío en {root}"));
    }

    let workspace_toml = read_toml(&main_root.join("Cargo.toml"))?;
    let workspace_version = workspace_toml
        .get("workspace")
        .and_then(|w| w.get("package"))
        .and_then(|p| p.get("version"))
        .and_then(|v| v.as_str())
        .ok_or("identidad del main: falta [workspace.package] version en Cargo.toml")?
        .to_string();

    let core_toml = read_toml(&main_root.join("crates/game-core/Cargo.toml"))?;
    let core_version = match core_toml.get("version") {
        Some(toml::Value::String(s)) => Version::parse(s)
            .map_err(|e| format!("identidad del main: version de game-core inválida ({s}): {e}"))?,
        Some(_) => Version::parse(&workspace_version).map_err(|e| {
            format!("identidad del main: version de workspace inválida ({workspace_version}): {e}")
        })?,
        None => Version::parse(&workspace_version).map_err(|e| {
            format!("identidad del main: game-core no declara version ni hereda ({workspace_version}): {e}")
        })?,
    };

    let bevy_decl = workspace_toml
        .get("workspace")
        .and_then(|w| w.get("dependencies"))
        .and_then(|d| d.get("bevy"))
        .ok_or("identidad del main: falta [workspace.dependencies] bevy")?;
    let bevy_raw = match bevy_decl {
        toml::Value::String(s) => s.clone(),
        toml::Value::Table(t) => t
            .get("version")
            .and_then(|v| v.as_str())
            .ok_or("identidad del main: [workspace.dependencies] bevy sin version")?
            .to_string(),
        _ => return Err("identidad del main: [workspace.dependencies] bevy con forma desconocida".into()),
    };
    let bevy = BevyVersion::parse(&bevy_raw)?;

    Ok(MainIdentity { head, core_version, bevy })
}

fn read_toml(path: &Path) -> Result<toml::Value, String> {
    let raw = std::fs::read_to_string(path)
        .map_err(|e| format!("identidad del main: no se pudo leer {}: {e}", path.display()))?;
    toml::from_str::<toml::Value>(&raw)
        .map_err(|e| format!("identidad del main: {} no es TOML válido: {e}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bevy_parse_forms() {
        let v = BevyVersion::parse("0.20").unwrap();
        assert_eq!(v.major, 0);
        assert_eq!(v.minor, 20);
        assert_eq!(v.patch, None);
        assert!(BevyVersion::parse("0.20.0").unwrap().patch.is_some());
        assert!(BevyVersion::parse("banana").is_err());
        assert!(BevyVersion::parse("0.20.0.1").is_err());
    }

    #[test]
    fn bevy_match_rules() {
        let main = BevyVersion::parse("0.20.0").unwrap();
        assert!(main.matches("0.20").unwrap());
        assert!(main.matches("0.20.0").unwrap());
        assert!(!main.matches("0.19").unwrap());
        assert!(!main.matches("0.20.1").unwrap());
        assert!(!main.matches("0.21").unwrap());
        assert!(BevyVersion::parse("0.19").unwrap().matches("0.19").unwrap());
    }

    #[test]
    fn gather_reads_this_main() {
        let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let main_root = manifest.parent().unwrap().parent().unwrap();
        let id = gather(main_root).expect("main real identificable");
        assert_eq!(id.head.len(), 40);
        assert_eq!(id.core_version.major, 0);
        assert_eq!(id.bevy.major, 0);
        assert_eq!(id.bevy.minor, 20);
    }
}
