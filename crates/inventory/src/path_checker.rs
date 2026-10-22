//! PathChecker — data-universe path resolution (Mode A/B seed, F0).
//!
//! The engine reads Daggerfall game data either from the default
//! installation (Mode A) or from a folder the user selected (Mode B). This
//! is the compilable seed of that toggle: it stores the switch and the
//! custom path, and can probe a candidate directory for a Daggerfall data
//! layout. Universe resolution completes in the L99 content lane.

use std::path::{Path, PathBuf};

/// Selects which data universe the engine reads game assets from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DataUniverse {
    /// Mode A — the default Daggerfall data installation.
    Default,
    /// Mode B — a user-selected data folder.
    Custom(PathBuf),
}

/// Resolves the active data universe from the user's toggle and selection.
#[derive(Debug, Clone, Default)]
pub struct PathChecker {
    use_custom_universe: bool,
    custom_universe: Option<PathBuf>,
}

impl PathChecker {
    /// Mode A by default: no custom universe selected.
    pub fn new() -> Self {
        Self {
            use_custom_universe: false,
            custom_universe: None,
        }
    }

    /// Mode B, seeded directly with a user-selected folder.
    pub fn with_custom_universe(path: impl Into<PathBuf>) -> Self {
        Self {
            use_custom_universe: true,
            custom_universe: Some(path.into()),
        }
    }

    /// Whether the user-selected universe (Mode B) is active.
    pub fn use_custom_universe(&self) -> bool {
        self.use_custom_universe
    }

    /// Toggles between Mode A (false) and Mode B (true).
    pub fn set_use_custom_universe(&mut self, enabled: bool) {
        self.use_custom_universe = enabled;
    }

    /// Stores the user-selected data folder.
    pub fn set_custom_universe(&mut self, path: impl Into<PathBuf>) {
        self.custom_universe = Some(path.into());
    }

    /// The user-selected folder, regardless of the toggle.
    pub fn custom_universe(&self) -> Option<&Path> {
        self.custom_universe.as_deref()
    }

    /// The universe currently in effect: the custom folder when the toggle
    /// is on, `None` (engine default) otherwise.
    pub fn active_universe(&self) -> Option<&Path> {
        if self.use_custom_universe {
            self.custom_universe.as_deref()
        } else {
            None
        }
    }

    /// Snapshot of the current mode as a `DataUniverse` value.
    pub fn universe(&self) -> DataUniverse {
        match self.active_universe() {
            Some(path) => DataUniverse::Custom(path.to_path_buf()),
            None => DataUniverse::Default,
        }
    }

    /// True when `dir` looks like a Daggerfall data root: it contains an
    /// `ARENA2` directory with `ARCH3D.BSA` inside. Case is tried both ways
    /// for installations copied from case-insensitive filesystems.
    pub fn looks_like_data_root(dir: &Path) -> bool {
        dir.join("ARENA2").join("ARCH3D.BSA").is_file()
            || dir.join("arena2").join("ARCH3D.BSA").is_file()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_to_mode_a() {
        let pc = PathChecker::new();
        assert!(!pc.use_custom_universe());
        assert!(pc.active_universe().is_none());
        assert_eq!(pc.universe(), DataUniverse::Default);
    }

    #[test]
    fn toggle_activates_mode_b() {
        let mut pc = PathChecker::new();
        pc.set_custom_universe("/data/df");
        assert!(!pc.use_custom_universe());
        pc.set_use_custom_universe(true);
        assert_eq!(pc.active_universe(), Some(Path::new("/data/df")));
        assert_eq!(pc.universe(), DataUniverse::Custom(PathBuf::from("/data/df")));
    }

    #[test]
    fn with_custom_universe_starts_in_mode_b() {
        let pc = PathChecker::with_custom_universe("/data/df");
        assert!(pc.use_custom_universe());
        assert_eq!(pc.custom_universe(), Some(Path::new("/data/df")));
    }

    #[test]
    fn toggle_off_returns_to_default() {
        let mut pc = PathChecker::with_custom_universe("/data/df");
        pc.set_use_custom_universe(false);
        assert!(pc.active_universe().is_none());
        // The selection itself is preserved for the next toggle.
        assert_eq!(pc.custom_universe(), Some(Path::new("/data/df")));
    }

    #[test]
    fn data_root_probe_requires_arena2_and_arch3d() {
        let tmp = std::env::temp_dir().join("pathchecker-probe-nonexistent");
        assert!(!PathChecker::looks_like_data_root(&tmp));
    }
}
