//! `DataRoot` — the single resolver of the retail data folder
//! (data contract §4, decisions D1-D5/D11/D12).
//!
//! **Why one resolver.** The contract measured nine places deciding the root
//! (§1.3) — one canonical resolver plus four unvalidated copies and three
//! competing environment conventions. This module is the only one: precedence,
//! auto-detection, validation and persistence live here, and every reader
//! (`daggerfall_app::mode_a`, the M18 preflight, `ui_assets`, `audio`, the
//! probes) goes through it.
//!
//! **Precedence (§4.1):**
//!
//! | Rank | Source | Failure |
//! |---|---|---|
//! | 1 | `--data-root <dir>` | hard `E-DATA-002`, the next rank is **not** tried |
//! | 2 | `[data].root` of `config.toml` | ignored with `W-DATA-002` (see *deviation* below) |
//! | 3 | `$DF_APP_ROOT` | hard `E-DATA-002` |
//! | 4 | `$DF_ARENA2`, `$DAGGERFALL_ARENA2` (deprecated aliases, normalized to the parent) | hard `E-DATA-002` + `W-DATA-008` |
//! | 5 | auto-detection (§4.2) | `E-DATA-001` (none / ambiguous / below threshold) |
//!
//! An explicit rank that fails never falls through silently: the player asked
//! for that folder, so the answer is a message naming it, not another folder.
//!
//! **Deviation (declared, §4.1 rank 2).** The contract fixes a *hard error*
//! with a reset instruction when the remembered root fails. The owner's order
//! for this lane falsifies the opposite behaviour (`d`: "raíz recordada
//! inválida ⇒ se ignora y NO se reescribe otra vez sin pasar L2"), so an
//! invalid remembered root is **ignored with `W-DATA-002`**, the config file is
//! left byte-for-byte untouched, and the next rank decides. Rationale: a
//! remembered root is a convenience, not a per-run request; a stale mount must
//! not block a machine that holds a valid second copy. Ranks 1/3/4 keep the
//! hard error, which is where the contract's "override that fails in silence"
//! cost actually lives.
//!
//! **Normalization (§4.1):** a directory whose basename is `ARENA2`/`arena2` is
//! accepted and its parent is used, so `--data-root …/ARENA2` and `$DF_ARENA2`
//! mean the same installation as `--data-root …`.
//!
//! **Zero guessing (§4.2 / the order):** a tie between valid candidates, or a
//! best score below [`retail_manifest::MAX_SCORE`], lists the candidates
//! (numbered, with scores) and returns an error that asks for a choice. Nothing
//! is auto-picked.
//!
//! **Persistence (§4.5):** [`DataRoot::remember`] writes `config.toml`
//! atomically and is called by the binary only after L2 passed. Resolution
//! itself never writes.

use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::config_file::{self, ConfigState};
use crate::retail_manifest::{self, ARENA2_NAMES, CORE_REQUIRED, FALL_EXE, MAX_SCORE};

pub mod validation;

pub use validation::{Phase, Report};

/// Where the accepted root came from (§4.1). Printed by the product as
/// `DATA ROOT = <path> (origin: <label>)`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Origin {
    /// Rank 1 — `--data-root`.
    Cli,
    /// Rank 2 — `[data].root` of `config.toml`.
    Config,
    /// Rank 3 — `$DF_APP_ROOT`.
    EnvAppRoot,
    /// Rank 4 — a deprecated `$DF_ARENA2` / `$DAGGERFALL_ARENA2`.
    EnvLegacy(String),
    /// Rank 5 — auto-detection.
    AutoDetected,
    /// A folder a caller already resolved (`DataRoot::from_arena2`).
    Given,
}

impl Origin {
    /// Short label for the `DATA ROOT` line and the log.
    pub fn label(&self) -> String {
        match self {
            Self::Cli => "--data-root".into(),
            Self::Config => "config".into(),
            Self::EnvAppRoot => "DF_APP_ROOT".into(),
            Self::EnvLegacy(name) => format!("{name} (deprecated)"),
            Self::AutoDetected => "auto-detection".into(),
            Self::Given => "caller".into(),
        }
    }
}

/// The process environment the resolver reads, captured as data so tests can
/// build a fake machine (a fake `HOME` is the only way to measure §4.2).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Env {
    /// `$HOME`.
    pub home: Option<PathBuf>,
    /// `$XDG_CONFIG_HOME` (fallback: `$HOME/.config`).
    pub xdg_config_home: Option<PathBuf>,
    /// `$XDG_STATE_HOME` (fallback: `$HOME/.local/state`).
    pub xdg_state_home: Option<PathBuf>,
    /// `$DF_APP_ROOT` (rank 3).
    pub app_root: Option<PathBuf>,
    /// Deprecated aliases pointing at `ARENA2` itself (rank 4), in the order
    /// they are honoured.
    pub legacy_arena2: Vec<(String, PathBuf)>,
    /// `$DF_ASSET_REGISTRY` — reference hashes for L3.
    pub asset_registry: Option<PathBuf>,
}

impl Env {
    /// Reads the ambient process environment.
    pub fn from_process() -> Self {
        let path = |name: &str| std::env::var_os(name).map(PathBuf::from);
        let mut legacy_arena2 = Vec::new();
        for name in ["DF_ARENA2", "DAGGERFALL_ARENA2"] {
            if let Some(value) = path(name) {
                legacy_arena2.push((name.to_string(), value));
            }
        }
        Self {
            home: path("HOME"),
            xdg_config_home: path("XDG_CONFIG_HOME"),
            xdg_state_home: path("XDG_STATE_HOME"),
            app_root: path("DF_APP_ROOT"),
            legacy_arena2,
            asset_registry: path("DF_ASSET_REGISTRY"),
        }
    }

    /// `config.toml` path for this environment (§4.5).
    pub fn config_path(&self) -> Option<PathBuf> {
        config_file::config_path(self.xdg_config_home.as_deref(), self.home.as_deref())
    }

    /// `data-root.log` path for this environment (§4.4: "junto al estado").
    pub fn state_log_path(&self) -> Option<PathBuf> {
        let base = self
            .xdg_state_home
            .clone()
            .or_else(|| self.home.as_ref().map(|home| home.join(".local/state")))?;
        Some(
            base.join(config_file::CONFIG_DIR)
                .join("data-root.log"),
        )
    }

    /// `$HOME/Games/Daggerfall/app` — the historic default, now one more
    /// auto-detection candidate (§4.1 row "Default histórico").
    pub fn historic_default(&self) -> Option<PathBuf> {
        self.home
            .as_ref()
            .map(|home| home.join("Games/Daggerfall/app"))
    }
}

/// Options of one resolution. The contract's `resolve(explicit)` is the
/// convenience form of this.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ResolveOptions {
    /// Rank 1 — the folder the player passed explicitly.
    pub explicit: Option<PathBuf>,
    /// `--strict-data`: L3 mismatches become hard (§4.3 D5).
    pub strict: bool,
    /// The environment to read (process environment by default).
    pub env: Env,
}

impl ResolveOptions {
    /// Options from a real launch.
    pub fn from_process(explicit: Option<PathBuf>, strict: bool) -> Self {
        Self {
            explicit,
            strict,
            env: Env::from_process(),
        }
    }
}

/// A scored auto-detection candidate (§4.2). `score == None` means L0 failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    /// Install root (the folder that contains `ARENA2`).
    pub path: PathBuf,
    /// [`retail_manifest::score`]; `None` when [`looks_like_data_root`] failed.
    pub score: Option<usize>,
}

/// A non-fatal finding (`W-DATA-00x`). Never aborts a launch; always printed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Warning {
    /// `W-DATA-00x` code.
    pub code: &'static str,
    /// Player-facing line.
    pub message: String,
}

/// Data layer provenance. This lane ships the retail layer only: mods
/// (D6-D10) are another lane, and when they land `open` gains the layering
/// while `LayerId::Base` stays the retail layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayerId {
    /// Retail installation — the only layer this lane resolves.
    Base,
}

/// Where a file came from: layer, absolute path and digest of the bytes read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Provenance {
    /// Layer that served the bytes.
    pub layer: LayerId,
    /// Absolute path that was opened.
    pub path: PathBuf,
    /// SHA-256 of the returned bytes.
    pub sha256: [u8; 32],
}

impl Provenance {
    /// Lowercase hex digest (`W-DATA-005` messages and the log).
    pub fn sha256_hex(&self) -> String {
        hex(&self.sha256)
    }
}

/// A failure of [`DataRoot::open`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DataError {
    /// The relative name is not in the installation.
    Missing { rel: String },
    /// The file exists but cannot be read.
    Unreadable { rel: String, reason: String },
}

impl DataError {
    /// §4.4 code of this failure.
    pub fn code(&self) -> &'static str {
        "E-DATA-003"
    }

    /// Player-facing line.
    pub fn message(&self) -> String {
        match self {
            Self::Missing { rel } => format!("E-DATA-003: missing {rel}."),
            Self::Unreadable { rel, reason } => {
                format!("E-DATA-004: {rel} cannot be read: {reason}.")
            }
        }
    }
}

/// A failure of [`DataRoot::resolve`] or of validation. Every variant carries
/// its §4.4 code and the exact line the player sees.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DataRootError {
    /// `E-DATA-001` — nothing usable was found; `searched` is the numbered list.
    NoDataRoot {
        /// Every candidate that was examined, in order.
        searched: Vec<Candidate>,
        /// Remembered roots that were ignored ([`W-DATA-002`]).
        ignored: Vec<(PathBuf, String)>,
        /// Warnings collected before the failure (an ignored config file is
        /// announced even when nothing else was found).
        warnings: Vec<Warning>,
    },
    /// `E-DATA-001` — two or more valid candidates tie: ask, never guess.
    Ambiguous {
        /// The tied candidates (same score).
        candidates: Vec<Candidate>,
        /// Warnings collected before the failure.
        warnings: Vec<Warning>,
    },
    /// `E-DATA-001` — a candidate was found but no candidate reaches the
    /// §4.2 threshold: ask, never guess.
    BelowThreshold {
        /// The best score measured.
        best: usize,
        /// Every scored candidate.
        candidates: Vec<Candidate>,
        /// Warnings collected before the failure.
        warnings: Vec<Warning>,
    },
    /// `E-DATA-002` — an explicit root (ranks 1/3/4) is not a data folder.
    RootRejected {
        /// Which rank supplied it.
        origin: Origin,
        /// The folder that was checked.
        path: PathBuf,
        /// What was missing (e.g. `<path>/ARENA2/ARCH3D.BSA`).
        reason: String,
        /// Warnings collected before the failure.
        warnings: Vec<Warning>,
    },
    /// `E-DATA-003` — a mandatory file is absent.
    Missing {
        /// Install root the relative path is under.
        root: PathBuf,
        /// Path relative to the root, e.g. `ARENA2/MAPS.BSA`.
        rel: String,
    },
    /// `E-DATA-004` — a mandatory file is present but unusable.
    Invalid {
        /// Install root the relative path is under.
        root: PathBuf,
        /// Path relative to the root.
        rel: String,
        /// Why it is unusable.
        reason: String,
    },
}

impl DataRootError {
    /// §4.4 code.
    pub fn code(&self) -> &'static str {
        match self {
            Self::NoDataRoot { .. } | Self::Ambiguous { .. } | Self::BelowThreshold { .. } => {
                "E-DATA-001"
            }
            Self::RootRejected { .. } => "E-DATA-002",
            Self::Missing { .. } => "E-DATA-003",
            Self::Invalid { .. } => "E-DATA-004",
        }
    }

    /// Exit code for the product path (§4.4 D3).
    pub fn exit_code(&self) -> i32 {
        2
    }

    /// The line the player sees. Stable, greppable, and it names the path.
    pub fn player_message(&self) -> String {
        match self {
            Self::NoDataRoot {
                searched,
                ignored,
                warnings,
            } => {
                let mut out = warning_lines(warnings);
                out.push_str("E-DATA-001: no Daggerfall data folder found.\n");
                out.push_str("Searched:\n");
                for (index, candidate) in searched.iter().enumerate() {
                    out.push_str(&format!(
                        "  {}. {}{}\n",
                        index + 1,
                        candidate.path.display(),
                        match candidate.score {
                            Some(score) => format!(" (score {score}/{MAX_SCORE})"),
                            None => String::from(" (no ARENA2/ARCH3D.BSA)"),
                        }
                    ));
                }
                for (path, reason) in ignored {
                    out.push_str(&format!(
                        "  (remembered) {} — ignored: {reason}\n",
                        path.display()
                    ));
                }
                out.push_str("Pass --data-root \"<folder containing ARENA2>\"");
                out
            }
            Self::Ambiguous {
                candidates,
                warnings,
            } => {
                let score = candidates
                    .first()
                    .and_then(|candidate| candidate.score)
                    .unwrap_or(0);
                let mut out = warning_lines(warnings);
                out.push_str(&format!(
                    "E-DATA-001: no single Daggerfall data folder could be chosen \
                     ({} candidates score {score}/{MAX_SCORE}); refusing to guess.\n",
                    candidates.len()
                ));
                out.push_str("Choose one:\n");
                for (index, candidate) in candidates.iter().enumerate() {
                    out.push_str(&format!(
                        "  {}. {}\n",
                        index + 1,
                        candidate.path.display()
                    ));
                }
                out.push_str("Pass --data-root \"<folder containing ARENA2>\"");
                out
            }
            Self::BelowThreshold {
                best,
                candidates,
                warnings,
            } => {
                let mut out = warning_lines(warnings);
                out.push_str(&format!(
                    "E-DATA-001: no complete Daggerfall data folder found \
                     (best score {best}/{MAX_SCORE}); refusing to guess.\n"
                ));
                for (index, candidate) in candidates.iter().enumerate() {
                    out.push_str(&format!(
                        "  {}. {} (score {}/{MAX_SCORE})\n",
                        index + 1,
                        candidate.path.display(),
                        candidate.score.unwrap_or(0)
                    ));
                }
                out.push_str("Pass --data-root \"<folder containing ARENA2>\"");
                out
            }
            Self::RootRejected {
                origin,
                path,
                reason,
                warnings,
            } => format!(
                "{}E-DATA-002: {}={:?} is not a Daggerfall data folder ({reason}).",
                warning_lines(warnings),
                match origin {
                    Origin::Cli => "--data-root".to_string(),
                    Origin::EnvAppRoot => "DF_APP_ROOT".to_string(),
                    Origin::EnvLegacy(name) => name.clone(),
                    origin => origin.label(),
                },
                path.display().to_string()
            ),
            Self::Missing { root, rel } => format!(
                "E-DATA-003: data folder \"{}\" is incomplete: missing {rel}. \
                 Run --verify-data for the full list.",
                root.display()
            ),
            Self::Invalid { rel, reason, .. } => format!("E-DATA-004: {rel} {reason}"),
        }
    }

    /// The candidate list, when the error carries one (UI selectors, §4.4 M2).
    pub fn candidates(&self) -> &[Candidate] {
        match self {
            Self::NoDataRoot { searched, .. } => searched,
            Self::Ambiguous { candidates, .. } | Self::BelowThreshold { candidates, .. } => {
                candidates
            }
            _ => &[],
        }
    }
}

/// Renders the warnings collected *before* a failure (`W-DATA-00x` lines).
///
/// They travel with the error (§4.4) so a player who ends up with `E-DATA-001`
/// still learns that the remembered root was dropped or the config was
/// unreadable — otherwise the warning only exists on the happy path.
fn warning_lines(warnings: &[Warning]) -> String {
    let mut out = String::new();
    for warning in warnings {
        out.push_str(&format!("{}: {}\n", warning.code, warning.message));
    }
    out
}

impl std::fmt::Display for DataRootError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.player_message())
    }
}

impl std::error::Error for DataRootError {}

/// The resolved installation: the folder that contains `ARENA2`, the directory
/// itself (either spelling) and how it was chosen.
#[derive(Debug)]
pub struct DataRoot {
    base: PathBuf,
    arena2: PathBuf,
    origin: Origin,
    strict: bool,
    warnings: Vec<Warning>,
    registry: Option<PathBuf>,
    fingerprint: std::sync::OnceLock<[u8; 32]>,
}

impl Clone for DataRoot {
    fn clone(&self) -> Self {
        let clone = Self {
            base: self.base.clone(),
            arena2: self.arena2.clone(),
            origin: self.origin.clone(),
            strict: self.strict,
            warnings: self.warnings.clone(),
            registry: self.registry.clone(),
            fingerprint: std::sync::OnceLock::new(),
        };
        if let Some(value) = self.fingerprint.get() {
            let _ = clone.fingerprint.set(*value);
        }
        clone
    }
}

impl DataRoot {
    /// Contract entry point (§4.6): resolves the root honouring the process
    /// environment and the §4.1 precedence. `explicit` is the CLI rank.
    pub fn resolve(explicit: Option<&Path>) -> Result<Self, DataRootError> {
        Self::resolve_with(&ResolveOptions::from_process(
            explicit.map(Path::to_path_buf),
            false,
        ))
    }

    /// Resolves with explicit options (used by the binary for `--data-root`,
    /// `--strict-data`, and by the tests to inject a fake environment).
    pub fn resolve_with(options: &ResolveOptions) -> Result<Self, DataRootError> {
        let mut warnings = Vec::new();
        let mut ignored_remembered = Vec::new();

        // Rank 1 — the CLI. An explicit folder that fails is fatal (§4.1).
        if let Some(explicit) = &options.explicit {
            return Self::accept(explicit, Origin::Cli, options, warnings);
        }

        // Rank 2 — the remembered root. Ignored with a warning when it rotted
        // (deviation declared in the module docs); never rewritten here.
        let config = options
            .env
            .config_path()
            .map(|path| (path.clone(), config_file::load(&path)));
        if let Some((path, state)) = &config {
            if let Some(warning) = state.warning() {
                warnings.push(warning);
            }
            if let ConfigState::Loaded(file) = state {
                if let Some(remembered) = &file.root {
                    match Self::accept(
                        remembered,
                        Origin::Config,
                        options,
                        warnings.clone(),
                    ) {
                        Ok(root) => return Ok(root),
                        Err(err) => {
                            let reason = err.player_message();
                            warnings.push(Warning {
                                code: "W-DATA-002",
                                message: format!(
                                    "remembered data folder \"{}\" ignored: {reason}",
                                    remembered.display()
                                ),
                            });
                            ignored_remembered.push((remembered.clone(), reason));
                            let _ = path;
                        }
                    }
                }
                if let Some(strict) = file.strict {
                    if !options.strict {
                        warnings.push(Warning {
                            code: "W-DATA-010",
                            message: format!(
                                "config strict={strict} is not applied: pass --strict-data \
                                 to make identity mismatches fatal"
                            ),
                        });
                    }
                }
            }
        }

        // Rank 3 — `$DF_APP_ROOT`.
        if let Some(app_root) = &options.env.app_root {
            return Self::accept(app_root, Origin::EnvAppRoot, options, warnings);
        }

        // Rank 4 — the deprecated aliases, which point at `ARENA2` itself.
        for (name, value) in &options.env.legacy_arena2 {
            warnings.push(Warning {
                code: "W-DATA-008",
                message: format!(
                    "${name} is deprecated: it points at ARENA2 and is only honoured as a \
                     fallback. Pass --data-root \"<folder containing ARENA2>\" instead."
                ),
            });
            return Self::accept(value, Origin::EnvLegacy(name.clone()), options, warnings);
        }

        // Rank 5 — auto-detection.
        let candidates = detect(&options.env);
        match choose(&candidates) {
            Ok(path) => {
                let mut root = Self::make(path, Origin::AutoDetected, options, warnings)?;
                root.warnings.shrink_to_fit();
                Ok(root)
            }
            Err(error) => Err(match error {
                ChooseError::None => DataRootError::NoDataRoot {
                    searched: candidates,
                    ignored: ignored_remembered,
                    warnings,
                },
                ChooseError::Ambiguous(tied) => DataRootError::Ambiguous {
                    candidates: tied,
                    warnings,
                },
                ChooseError::BelowThreshold { best, scored } => {
                    DataRootError::BelowThreshold {
                        best,
                        candidates: scored,
                        warnings,
                    }
                }
            }),
        }
    }

    /// Builds a root from an already-resolved `ARENA2` directory.
    ///
    /// For callers that hold the directory (the UI loader): the result is the
    /// same object, so `open` keeps one name-resolution rule and one
    /// provenance. Fails when `dir` is not a data directory.
    pub fn from_arena2(dir: &Path) -> Result<Self, DataError> {
        if !dir.join("ARCH3D.BSA").is_file() {
            return Err(DataError::Missing {
                rel: format!("{}/ARCH3D.BSA", dir.display()),
            });
        }
        let base = dir
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| dir.to_path_buf());
        Ok(Self {
            base,
            arena2: dir.to_path_buf(),
            origin: Origin::Given,
            strict: false,
            warnings: Vec::new(),
            registry: None,
            fingerprint: std::sync::OnceLock::new(),
        })
    }

    fn accept(
        candidate: &Path,
        origin: Origin,
        options: &ResolveOptions,
        warnings: Vec<Warning>,
    ) -> Result<Self, DataRootError> {
        let normalized = normalize(candidate);
        match Self::make(normalized, origin.clone(), options, warnings.clone()) {
            Ok(root) => Ok(root),
            Err(DataRootError::RootRejected {
                path, warnings: inner, ..
            }) => {
                Err(DataRootError::RootRejected {
                    origin,
                    reason: format!("missing {}/ARENA2/ARCH3D.BSA", path.display()),
                    path,
                    warnings: if inner.is_empty() { warnings } else { inner },
                })
            }
            Err(other) => Err(other),
        }
    }

    fn make(
        candidate: PathBuf,
        origin: Origin,
        options: &ResolveOptions,
        warnings: Vec<Warning>,
    ) -> Result<Self, DataRootError> {
        let Some(arena2) = retail_manifest::arena2_dir(&candidate) else {
            return Err(DataRootError::RootRejected {
                origin,
                path: candidate,
                reason: "missing ARENA2/ARCH3D.BSA".into(),
                warnings,
            });
        };
        let registry = registry_path(options, &candidate);
        Ok(Self {
            base: candidate,
            arena2,
            origin,
            strict: options.strict,
            warnings,
            registry,
            fingerprint: std::sync::OnceLock::new(),
        })
    }

    /// Install root: the folder that contains `ARENA2`.
    pub fn base(&self) -> &Path {
        &self.base
    }

    /// The data directory itself (`ARENA2` or `arena2`, whichever exists).
    pub fn arena2_dir(&self) -> &Path {
        &self.arena2
    }

    /// Which rank produced this root.
    pub fn origin(&self) -> &Origin {
        &self.origin
    }

    /// `--strict-data` in force (§4.3 D5).
    pub fn strict(&self) -> bool {
        self.strict
    }

    /// Findings collected while resolving (ignored config, deprecated env…).
    pub fn warnings(&self) -> &[Warning] {
        &self.warnings
    }

    /// The `ARENA2/<name>` path relative to the install root, using the real
    /// spelling of the directory on disk.
    pub fn rel(&self, name: &str) -> String {
        let dir = self
            .arena2
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| ARENA2_NAMES[0].to_string());
        format!("{dir}/{name}")
    }

    /// Resolves a path relative to the install root with the one
    /// case-insensitive rule (§1.1 step 5, reused by every reader).
    ///
    /// The first component may name the data directory (`ARENA2/MAPS.BSA`) or
    /// be any other root entry (`FALL.EXE`). `..` and absolute paths are
    /// rejected: the resolver never leaves the installation.
    pub fn resolve_relative(&self, relative: &str) -> Result<PathBuf, DataError> {
        let relative = relative.trim_start_matches("./");
        if relative.starts_with('/') || Path::new(relative).is_absolute() {
            return Err(DataError::Missing {
                rel: relative.to_string(),
            });
        }
        let mut parts = relative.split('/').filter(|part| !part.is_empty());
        let Some(first) = parts.next() else {
            return Err(DataError::Missing {
                rel: relative.to_string(),
            });
        };
        let mut current;
        let rest: Vec<&str> = if ARENA2_NAMES
            .iter()
            .any(|name| first.eq_ignore_ascii_case(name))
        {
            current = self.arena2.clone();
            parts.collect()
        } else {
            current = self.base.clone();
            std::iter::once(first).chain(parts).collect()
        };
        for part in rest {
            if part == ".." {
                return Err(DataError::Missing {
                    rel: relative.to_string(),
                });
            }
            match retail_manifest::find_child_ci(&current, part) {
                Some(path) => current = path,
                None => {
                    return Err(DataError::Missing {
                        rel: relative.to_string(),
                    });
                }
            }
        }
        Ok(current)
    }

    /// Reads a relative path without hashing it (L2, bulk readers).
    pub fn read(&self, relative: &str) -> Result<Vec<u8>, DataError> {
        let path = self.resolve_relative(relative)?;
        std::fs::read(&path).map_err(|err| DataError::Unreadable {
            rel: self.normalize_rel(relative),
            reason: err.to_string(),
        })
    }

    /// Contract API (§4.6): reads a relative path and reports its provenance.
    pub fn open(&self, relative: &str) -> Result<(Vec<u8>, Provenance), DataError> {
        let path = self.resolve_relative(relative)?;
        let bytes = std::fs::read(&path).map_err(|err| DataError::Unreadable {
            rel: self.normalize_rel(relative),
            reason: err.to_string(),
        })?;
        let provenance = Provenance {
            layer: LayerId::Base,
            path,
            sha256: sha256(&bytes),
        };
        Ok((bytes, provenance))
    }

    fn normalize_rel(&self, relative: &str) -> String {
        let first = relative.split('/').next().unwrap_or(relative);
        if ARENA2_NAMES
            .iter()
            .any(|name| first.eq_ignore_ascii_case(name))
        {
            self.rel(relative.splitn(2, '/').nth(1).unwrap_or(""))
        } else {
            relative.to_string()
        }
    }

    /// Contract API (§4.6): L1 presence of `names`, which are `ARENA2`-relative.
    pub fn require(&self, names: &[&str]) -> Result<(), DataRootError> {
        for name in names {
            if self.resolve_relative(&self.rel(name)).is_err() {
                return Err(DataRootError::Missing {
                    root: self.base.clone(),
                    rel: self.rel(name),
                });
            }
        }
        Ok(())
    }

    /// Contract API (§4.3/§4.6): runs a validation phase. `L2`/`L3` run the
    /// earlier phases first, so a report is never given over an incomplete
    /// installation.
    pub fn verify(&self, phase: Phase) -> Result<Report, DataRootError> {
        validation::verify(self, phase)
    }

    /// Contract API (§4.6): digest of the mandatory set (identity, §5.5).
    ///
    /// SHA-256 over `ARENA2/<required>` in manifest order (core, UI images,
    /// fonts) followed by `FALL.EXE`; each file contributes its name and its
    /// bytes, so a rename or a content change changes the digest. Cached: L3
    /// and the log ask for it in the same run.
    pub fn fingerprint(&self) -> [u8; 32] {
        *self.fingerprint.get_or_init(|| {
            let mut hasher = Sha256::new();
            let mut names: Vec<String> = CORE_REQUIRED
                .iter()
                .chain(retail_manifest::PANEL_ASSETS.iter())
                .chain(retail_manifest::FONT_FILES.iter())
                .map(|name| self.rel(name))
                .collect();
            names.sort();
            names.push(FALL_EXE.to_string());
            for name in names {
                hasher.update(name.as_bytes());
                hasher.update([0u8]);
                if let Ok(bytes) = self.read(&name) {
                    hasher.update((bytes.len() as u64).to_le_bytes());
                    hasher.update(&bytes);
                }
            }
            hasher.finalize().into()
        })
    }

    /// Reference registry for L3 (§4.3), when this machine has one.
    pub fn registry(&self) -> Option<&Path> {
        self.registry.as_deref()
    }

    /// Persists this root in `config.toml` (§4.5). The binary calls it only
    /// after L2 passed — the resolver never writes.
    pub fn remember(&self, env: &Env) -> Result<PathBuf, String> {
        let Some(path) = env.config_path() else {
            return Err("no HOME/XDG_CONFIG_HOME: there is nowhere to remember the root".into());
        };
        config_file::remember_root(&path, &self.base, self.strict)?;
        Ok(path)
    }
}

/// Why [`choose`] refused to pick a candidate.
enum ChooseError {
    None,
    Ambiguous(Vec<Candidate>),
    BelowThreshold {
        best: usize,
        scored: Vec<Candidate>,
    },
}

/// Applies the §4.2 rule to the scanned candidates: exactly one best score at
/// the ceiling, or refuse (never guess).
fn choose(candidates: &[Candidate]) -> Result<PathBuf, ChooseError> {
    let scored: Vec<Candidate> = candidates
        .iter()
        .filter(|candidate| candidate.score.is_some())
        .cloned()
        .collect();
    if scored.is_empty() {
        return Err(ChooseError::None);
    }
    let best = scored
        .iter()
        .filter_map(|candidate| candidate.score)
        .max()
        .unwrap_or(0);
    if best < MAX_SCORE {
        return Err(ChooseError::BelowThreshold { best, scored });
    }
    let tied: Vec<Candidate> = scored
        .into_iter()
        .filter(|candidate| candidate.score == Some(best))
        .collect();
    match tied.as_slice() {
        [single] => Ok(single.path.clone()),
        _ => Err(ChooseError::Ambiguous(tied)),
    }
}

/// The §4.2 candidate list: Steam (both spellings), GOG, Wine, dosbox, a loose
/// `~/Games/daggerfall`, and the historic default `~/Games/Daggerfall/app`.
/// Duplicates (same canonical path) are dropped, order preserved.
pub fn detect(env: &Env) -> Vec<Candidate> {
    let mut paths: Vec<PathBuf> = Vec::new();
    if let Some(home) = &env.home {
        for relative in [
            ".steam/steam/steamapps/common/Daggerfall",
            ".steam/steam/steamapps/common/The Elder Scrolls Daggerfall",
            ".local/share/Steam/steamapps/common/Daggerfall",
            ".local/share/Steam/steamapps/common/The Elder Scrolls Daggerfall",
            "GOG Games/Daggerfall",
            ".wine/drive_c/GOG Games/Daggerfall",
            ".wine/drive_c/Program Files (x86)/GOG.com/Daggerfall",
            ".wine/drive_c/Program Files (x86)/Daggerfall",
            "dosbox/daggerfall",
            "Games/daggerfall",
        ] {
            paths.push(home.join(relative));
        }
    }
    if let Some(default) = env.historic_default() {
        paths.push(default);
    }
    let mut seen: Vec<PathBuf> = Vec::new();
    let mut candidates = Vec::new();
    for path in paths {
        let canonical = std::fs::canonicalize(&path).unwrap_or_else(|_| path.clone());
        if seen.contains(&canonical) {
            continue;
        }
        seen.push(canonical);
        let score = retail_manifest::score(&path);
        candidates.push(Candidate { path, score });
    }
    candidates
}

/// Accepts `<root>/ARENA2` as `<root>` (§4.1 normalization).
pub fn normalize(candidate: &Path) -> PathBuf {
    let is_arena2 = candidate
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| ARENA2_NAMES.iter().any(|known| name.eq_ignore_ascii_case(known)));
    if !is_arena2 {
        return candidate.to_path_buf();
    }
    match candidate.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent.to_path_buf(),
        _ => candidate.to_path_buf(),
    }
}

fn registry_path(options: &ResolveOptions, base: &Path) -> Option<PathBuf> {
    if let Some(path) = &options.env.asset_registry {
        return path.is_file().then(|| path.clone());
    }
    for candidate in [
        std::env::current_dir().ok().map(|cwd| cwd.join("ASSET-REGISTRY.json")),
        Some(base.join("ASSET-REGISTRY.json")),
    ]
    .into_iter()
    .flatten()
    {
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

/// SHA-256 of `bytes`.
pub fn sha256(bytes: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hasher.finalize().into()
}

/// Lowercase hex of a digest.
pub fn hex(digest: &[u8; 32]) -> String {
    let mut out = String::with_capacity(64);
    for byte in digest {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

#[cfg(test)]
mod tests;
