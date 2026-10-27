//! Write allowlist audit — plan §4.3 step 5 over the side's C1 ledger
//! (§6.1-6.2: `.sintergic/registries/paths/<sujeto>.jsonl`, rows with
//! `ts`, `sujeto`, `repo_root`, `ruta`, `sha256`, `accion`).
//!
//! Fail-closed: a row that cannot be parsed or resolved counts as a
//! violation, because an unauditable write is indistinguishable from an
//! out-of-contract one.

use std::path::{Component, Path, PathBuf};

/// Files live under this directory of the side workspace (plan §6.2).
pub const C1_DIR: &str = ".sintergic/registries/paths";

const REQUIRED_FIELDS: [&str; 6] = ["ts", "sujeto", "repo_root", "ruta", "sha256", "accion"];

/// Audit every C1 row of the side. Returns one message per violation.
pub fn check(side_root: &Path) -> Result<Vec<String>, String> {
    let dir = side_root.join(C1_DIR);
    if !dir.exists() {
        return Ok(Vec::new()); // nothing recorded, nothing to violate
    }
    let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
        .map_err(|e| format!("no se pudo leer {}: {e}", dir.display()))?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|ext| ext == "jsonl"))
        .collect();
    files.sort();

    let side_root = std::fs::canonicalize(side_root).unwrap_or_else(|_| side_root.to_path_buf());
    let mut violations = Vec::new();
    for file in files {
        let text = std::fs::read_to_string(&file)
            .map_err(|e| format!("no se pudo leer {}: {e}", file.display()))?;
        for (idx, line) in text.lines().enumerate() {
            if line.trim().is_empty() {
                continue;
            }
            let label = format!("{}:{}", file.file_name().unwrap_or_default().to_string_lossy(), idx + 1);
            match check_row(line, &side_root) {
                Ok(None) => {}
                Ok(Some(v)) => violations.push(format!("{label}: {v}")),
                Err(v) => violations.push(format!("{label}: {v}")),
            }
        }
    }
    Ok(violations)
}

/// `Ok(None)` clean row, `Ok(Some(reason))` resolved violation, `Err(reason)`
/// malformed row (fail-closed).
fn check_row(line: &str, side_root: &Path) -> Result<Option<String>, String> {
    let value: serde_json::Value =
        serde_json::from_str(line).map_err(|e| format!("fila C1 ilegible: {e}"))?;
    let obj = value.as_object().ok_or("fila C1 no es un objeto")?;
    for field in REQUIRED_FIELDS {
        let present = obj
            .get(field)
            .map(|v| !v.is_null() && v.as_str().is_some_and(|s| !s.trim().is_empty()))
            .unwrap_or(false);
        if !present {
            return Err(format!("fila C1 sin campo {field:?}"));
        }
    }
    let repo_root = obj["repo_root"].as_str().expect("checked");
    let ruta = obj["ruta"].as_str().expect("checked");

    let base = resolve(side_root, repo_root);
    let target = if Path::new(ruta).is_absolute() {
        normalize(Path::new(ruta))
    } else {
        normalize(&base.join(ruta))
    };
    if target == base {
        return Ok(Some(format!("ruta {ruta:?} resuelve al propio repo_root")));
    }
    if !target.starts_with(&base) {
        return Ok(Some(format!(
            "ruta {ruta:?} escribe fuera de repo_root ({})",
            base.display()
        )));
    }
    Ok(None)
}

/// `repo_root` is absolute in the wild; a relative value is resolved against
/// the side workspace so fixtures (and relocated checkouts) stay portable.
fn resolve(side_root: &Path, repo_root: &str) -> PathBuf {
    let p = Path::new(repo_root);
    if p.is_absolute() {
        normalize(p)
    } else {
        normalize(&side_root.join(p))
    }
}

/// Lexical canonicalisation: drop `.`, collapse `..`, keep the root. Purely
/// textual — no symlink resolution, so it also works for paths that do not exist.
pub fn normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                if !out.pop() {
                    out.push("..");
                }
            }
            other => out.push(other.as_os_str()),
        }
    }
    if out.as_os_str().is_empty() {
        out.push(".");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn fixture(name: &str, rows: &[String]) -> PathBuf {
        let root = std::env::temp_dir().join(format!("f4a-allowlist-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let dir = root.join(C1_DIR);
        fs::create_dir_all(&dir).unwrap();
        for (idx, row) in rows.iter().enumerate() {
            fs::write(dir.join(format!("Ledger{idx}.jsonl")), format!("{row}\n")).unwrap();
        }
        root
    }

    fn row(repo_root: &str, ruta: &str) -> String {
        format!(
            r#"{{"ts": "2026-10-25T00:00:00Z", "sujeto": "f4a", "repo_root": "{repo_root}", "ruta": "{ruta}", "sha256": "aa", "accion": "write"}}"#
        )
    }

    #[test]
    fn clean_relative_rows_pass() {
        let root = fixture(
            "clean",
            &[row(".", "docs/x.md"), row(".", "crates/y/src/z.rs")],
        );
        assert_eq!(check(&root).unwrap(), Vec::<String>::new());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn escape_via_dotdot_is_violation() {
        let root = fixture("escape", &[row(".", "../outside/evil.md")]);
        let v = check(&root).unwrap();
        assert_eq!(v.len(), 1, "{v:?}");
        assert!(v[0].contains("fuera de repo_root"), "{}", v[0]);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn absolute_path_outside_is_violation() {
        let root = fixture("absolute", &[row(".", "/etc/passwd")]);
        let v = check(&root).unwrap();
        assert_eq!(v.len(), 1, "{v:?}");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn malformed_rows_fail_closed() {
        let root = fixture(
            "malformed",
            &[r#"{"ts": "x", "sujeto": "y"}"#.into(), "not json".into()],
        );
        let v = check(&root).unwrap();
        assert_eq!(v.len(), 2, "{v:?}");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn missing_ledger_dir_is_clean() {
        let root = std::env::temp_dir().join(format!("f4a-noc1-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        assert_eq!(check(&root).unwrap(), Vec::<String>::new());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn normalize_rules() {
        assert_eq!(normalize(Path::new("/a/b/../c/./d")), PathBuf::from("/a/c/d"));
        assert_eq!(normalize(Path::new("a/../../b")), PathBuf::from("../b"));
        assert_eq!(normalize(Path::new("")), PathBuf::from("."));
    }
}
