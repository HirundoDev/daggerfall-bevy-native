//! Mode A data-root resolution (PRE-2 §8 rule 3), shared by the binary's
//! probes/windows and by the library surface consumed by the integration
//! tests (`daggerfall_app::mode_a`).
//!
//! `PathChecker` (daggerfall-inventory) validates the root; this module only
//! resolves paths inside it. Root: `$DF_APP_ROOT` or by default
//! `$HOME/Games/Daggerfall/app` (field-config `data.game_dir`, both
//! capitalization variants). Retail bytes are read at runtime only
//! (INV-L99-6: 0 packaged assets).

use std::path::{Path, PathBuf};

use daggerfall_inventory::path_checker::PathChecker;

/// Resolves the Mode A ARENA2 directory.
///
/// Root `$DF_APP_ROOT` or by default `$HOME/Games/Daggerfall/app`, validated
/// with `PathChecker::looks_like_data_root` (both capitalization variants).
pub fn arena2_dir() -> Result<PathBuf, String> {
    let root: PathBuf = match std::env::var_os("DF_APP_ROOT") {
        Some(value) => PathBuf::from(value),
        None => {
            let home = std::env::var_os("HOME").ok_or("HOME not set (define $DF_APP_ROOT)")?;
            PathBuf::from(home).join("Games/Daggerfall/app")
        }
    };
    if !PathChecker::looks_like_data_root(&root) {
        return Err(format!(
            "invalid Mode A data root: {root:?} (expected <root>/ARENA2; override: $DF_APP_ROOT)"
        ));
    }
    // looks_like_data_root tries both capitalization variants (path_checker).
    ["ARENA2", "arena2"]
        .iter()
        .map(|dir| root.join(dir))
        .find(|path| path.is_dir())
        .ok_or_else(|| format!("ARENA2 directory not found under {root:?}"))
}

/// Resolves a file inside the Mode A ARENA2 (see [`arena2_dir`]).
///
/// The lookup is case-insensitive, as the source is: DFU opens the faction
/// file with `EndsWith(..., InvariantCultureIgnoreCase)`
/// (`FactionFile.cs:740-741`) and the GOG installation ships `faction.txt`
/// (lowercase) where the corpus declares `FACTION.TXT`. Any capitalization
/// variant of the requested name resolves on a case-sensitive filesystem.
pub fn arena2_path(name: &str) -> Result<PathBuf, String> {
    resolve_in_dir(&arena2_dir()?, name)
}

/// Resolves `name` inside `dir` honouring the source's case-insensitive open.
///
/// An exact-case match wins (deterministic precedence over any variant), then
/// the directory is scanned for entries whose name equals `name` under
/// `to_ascii_lowercase()`. Directory order is not stable across filesystems,
/// so candidates are sorted and the first one wins: two case variants of the
/// same name resolve to the same file on every run.
fn resolve_in_dir(dir: &Path, name: &str) -> Result<PathBuf, String> {
    let exact = dir.join(name);
    if exact.is_file() {
        return Ok(exact);
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Err(format!("{name} not found under {dir:?}"));
    };
    let wanted = name.to_ascii_lowercase();
    let mut candidates: Vec<PathBuf> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|found| found.to_str())
                .is_some_and(|found| found.to_ascii_lowercase() == wanted)
        })
        .filter(|path| path.is_file())
        .collect();
    candidates.sort();
    candidates
        .into_iter()
        .next()
        .ok_or_else(|| format!("{name} not found under {dir:?}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Scratch directory under `std::env::temp_dir()`, removed on drop so the
    /// tests clean up even if an assertion panics.
    struct Scratch(PathBuf);

    impl Scratch {
        fn new(tag: &str) -> Self {
            let stamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock after the Unix epoch")
                .as_nanos();
            let dir = std::env::temp_dir()
                .join(format!("df-mode-a-{tag}-{}-{stamp}", std::process::id()));
            std::fs::create_dir_all(&dir).expect("create scratch dir");
            Self(dir)
        }

        fn write(&self, name: &str) -> PathBuf {
            let path = self.0.join(name);
            std::fs::write(&path, b"scratch").expect("write scratch file");
            path
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    /// Case variants of the declared name resolve (retail GOG ships
    /// `faction.txt`; the corpus declares `FACTION.TXT`).
    #[test]
    fn mode_a_resolves_any_capitalization_variant() {
        let scratch = Scratch::new("variant");
        let on_disk = scratch.write("Faction.txt");
        let resolved = resolve_in_dir(&scratch.0, "FACTION.TXT").expect("variant resolves");
        assert_eq!(resolved, on_disk);
    }

    /// An exact-case file wins over its variants.
    #[test]
    fn mode_a_prefers_the_exact_name() {
        let scratch = Scratch::new("exact");
        let exact = scratch.write("MAPS.BSA");
        scratch.write("maps.bsa");
        let resolved = resolve_in_dir(&scratch.0, "MAPS.BSA").expect("exact name resolves");
        assert_eq!(resolved, exact);
    }

    /// Absent names keep the historical error text.
    #[test]
    fn mode_a_reports_absent_files() {
        let scratch = Scratch::new("absent");
        scratch.write("MAPS.BSA");
        let err = resolve_in_dir(&scratch.0, "ARCH3D.BSA").expect_err("absent file fails");
        assert_eq!(err, format!("ARCH3D.BSA not found under {:?}", scratch.0));
    }
}
