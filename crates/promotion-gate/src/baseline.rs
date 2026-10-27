//! Gate step 6b — the main's baseline surface is intact (plan
//! `I1-INFRA-1-TRUST-DUAL-SIDE-PROJECTS.md` §4.3 step 6, F4-B).
//!
//! Step 6 asks for two things: the preview composes, **and** the main's baseline
//! still holds. The preview alone could be green while the main it composed
//! against was edited under it, so the gate re-reads the main's declared
//! baseline surface and compares it byte-for-byte with `HEAD`.
//!
//! ## The check (deterministic, documented)
//!
//! 1. The main owns `docs/sintergic/promotion-gate/main-baseline.toml`, schema
//!    `1`, listing the files that *are* the baseline (law/contract surface the
//!    side composes against). The list is main-owned: a side cannot weaken it.
//! 2. Every listed file must exist, be tracked, and its bytes must equal
//!    `git show HEAD:<path>`. A dirty contract surface is a failed baseline,
//!    even when the file "looks" fine.
//! 3. The gate records `surface_sha256`: sha256 over `path\n<sha256 of the
//!    working file>\n` in declaration order. The same tree always produces the
//!    same digest, so two runs are comparable without diffing the files.
//!
//! Why `HEAD` instead of a pinned digest: a committed change to the main is a
//! legitimate main-side act (the baseline moves with the main), while an
//! *uncommitted* change under a promotion is exactly the drift this step must
//! catch. The result is deterministic, needs no maintenance and no build.

use std::path::{Component, Path};
use std::process::Command;

use serde::Deserialize;

use crate::probes::sha256_hex;
use crate::verdict::BaselineEvidence;

/// Main-owned declaration of the baseline surface, relative to `main_root`.
pub const BASELINE_FILE: &str = "docs/sintergic/promotion-gate/main-baseline.toml";

/// Schema of the declaration this gate reads.
pub const BASELINE_SCHEMA: &str = "1";

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Declaration {
    schema_version: String,
    files: Vec<String>,
}

/// Read the declaration and verify every declared file against `HEAD`.
///
/// `Err` is the `main_baseline_failed` detail; it must say *which* file drifted
/// and *how*, never just "failed".
pub fn check(main_root: &Path) -> Result<BaselineEvidence, String> {
    let declaration_path = main_root.join(BASELINE_FILE);
    let raw = std::fs::read(&declaration_path).map_err(|e| {
        format!(
            "baseline del main: no se pudo leer {}: {e}",
            declaration_path.display()
        )
    })?;
    let text = String::from_utf8(raw.clone())
        .map_err(|_| format!("baseline del main: {BASELINE_FILE} no es UTF-8"))?;
    let declaration: Declaration = toml::from_str(&text)
        .map_err(|e| format!("baseline del main: {BASELINE_FILE} inválido: {e}"))?;
    if declaration.schema_version != BASELINE_SCHEMA {
        return Err(format!(
            "baseline del main: schema_version={:?} no soportado (gate lee {BASELINE_SCHEMA})",
            declaration.schema_version
        ));
    }
    if declaration.files.is_empty() {
        return Err("baseline del main: la declaración no lista ningún archivo".into());
    }

    let mut surface = String::new();
    for file in &declaration.files {
        if !is_baseline_path(file) {
            return Err(format!(
                "baseline del main: {file:?} no es una ruta relativa dentro del workspace"
            ));
        }
        let working_path = main_root.join(file);
        let working = std::fs::read(&working_path).map_err(|e| {
            format!(
                "baseline del main: no se pudo leer {}: {e}",
                working_path.display()
            )
        })?;
        let head = git_show(main_root, file)?;
        if working != head {
            return Err(format!(
                "baseline del main: {file} difiere de HEAD (superficie del main editada sin commit)"
            ));
        }
        surface.push_str(file);
        surface.push('\n');
        surface.push_str(&sha256_hex(&working));
        surface.push('\n');
    }

    Ok(BaselineEvidence {
        files: declaration.files,
        declaration_sha256: sha256_hex(&raw),
        surface_sha256: sha256_hex(surface.as_bytes()),
    })
}

/// `git show HEAD:<path>` — the committed bytes of one baseline file.
fn git_show(main_root: &Path, file: &str) -> Result<Vec<u8>, String> {
    let out = Command::new("git")
        .args([
            "-C",
            &main_root.display().to_string(),
            "show",
            &format!("HEAD:{file}"),
        ])
        .output()
        .map_err(|e| format!("baseline del main: no se pudo ejecutar git: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "baseline del main: {file} no está en HEAD ({})",
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    Ok(out.stdout)
}

/// A baseline path is relative, non-empty and free of `..`/root/prefix parts.
pub fn is_baseline_path(file: &str) -> bool {
    if file.trim().is_empty() {
        return false;
    }
    let path = Path::new(file);
    if path.is_absolute() {
        return false;
    }
    path.components().all(|c| matches!(c, Component::Normal(_)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn git(root: &Path, args: &[&str]) {
        let status = Command::new("git")
            .args(["-C", &root.display().to_string()])
            .args(["-c", "user.email=f4b@example.test", "-c", "user.name=f4b"])
            .args(args)
            .status()
            .expect("git runs");
        assert!(status.success(), "git {args:?} failed");
    }

    fn temp_main(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "f4b-baseline-{tag}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("crates/game-core/src")).unwrap();
        std::fs::create_dir_all(dir.join("docs/sintergic/promotion-gate")).unwrap();
        std::fs::write(dir.join("crates/game-core/src/lib.rs"), b"pub fn law() {}\n").unwrap();
        std::fs::write(
            dir.join(BASELINE_FILE),
            "schema_version = \"1\"\nfiles = [\"crates/game-core/src/lib.rs\"]\n",
        )
        .unwrap();
        git(&dir, &["init", "-q"]);
        git(&dir, &["add", "."]);
        git(&dir, &["commit", "-q", "-m", "baseline"]);
        dir
    }

    #[test]
    fn clean_surface_matches_head() {
        let dir = temp_main("clean");
        let evidence = check(&dir).unwrap();
        assert_eq!(evidence.files, vec!["crates/game-core/src/lib.rs"]);
        assert_eq!(evidence.surface_sha256.len(), 64);
        // Deterministic: two runs agree byte for byte.
        assert_eq!(check(&dir).unwrap().surface_sha256, evidence.surface_sha256);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn dirty_surface_fails_with_the_file_name() {
        let dir = temp_main("dirty");
        std::fs::write(dir.join("crates/game-core/src/lib.rs"), b"pub fn changed() {}\n").unwrap();
        let err = check(&dir).unwrap_err();
        assert!(err.contains("crates/game-core/src/lib.rs"), "{err}");
        assert!(err.contains("HEAD"), "{err}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn untracked_baseline_file_fails() {
        let dir = temp_main("untracked");
        std::fs::write(
            dir.join(BASELINE_FILE),
            "schema_version = \"1\"\nfiles = [\"crates/game-core/src/lib.rs\", \"crates/new.rs\"]\n",
        )
        .unwrap();
        std::fs::write(dir.join("crates/new.rs"), b"x\n").unwrap();
        let err = check(&dir).unwrap_err();
        assert!(err.contains("crates/new.rs"), "{err}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_or_empty_declaration_fails() {
        let dir = temp_main("empty");
        std::fs::write(dir.join(BASELINE_FILE), "schema_version = \"1\"\nfiles = []\n").unwrap();
        assert!(check(&dir).unwrap_err().contains("no lista ningún archivo"));
        std::fs::remove_file(dir.join(BASELINE_FILE)).unwrap();
        assert!(check(&dir).unwrap_err().contains("no se pudo leer"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn baseline_paths_stay_inside_the_workspace() {
        assert!(is_baseline_path("crates/game-core/src/lib.rs"));
        assert!(!is_baseline_path("/etc/passwd"));
        assert!(!is_baseline_path("../main/Cargo.toml"));
        assert!(!is_baseline_path("crates/../../x"));
        assert!(!is_baseline_path(""));
    }
}
