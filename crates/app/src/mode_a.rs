//! Single data-root facade (data contract §4), shared by the binary's
//! probes/windows and by the library surface consumed by the integration
//! tests (`daggerfall_app::mode_a`).
//!
//! One resolver answers every "where are the retail bytes" question:
//! [`daggerfall_inventory::data_root::DataRoot`] — precedence (§4.1),
//! auto-detection (§4.2), phases (§4.3), messages (§4.4), persistence (§4.5).
//! This module is the **process facade**: it parses the flags once, resolves
//! the root once, prints the warnings once, and keeps the probes' historical
//! signatures (`Result<PathBuf, String>`) so their stdout does not change.
//!
//! Flags (`DataCli`):
//!
//! | Flag | Effect |
//! |---|---|
//! | `--data-root <dir>` / `--data-root=<dir>` | rank 1; ephemeral |
//! | `--remember` | persists the chosen root **after L2 passes** |
//! | `--forget-data-root` | removes the remembered root and exits 0 |
//! | `--strict-data` | L3 mismatches become fatal (D5) |
//! | `--verify-data` | prints the full L1/L2/L3 report and exits 0/2 |
//!
//! Retail bytes are read at runtime only (INV-L99-6: 0 packaged assets).

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;

use daggerfall_inventory::config_file;
use daggerfall_inventory::data_root::validation::{self, Audit};
use daggerfall_inventory::data_root::{DataRoot, Env, ResolveOptions};

/// Parsed command line of the data-root flags (§4.1/§4.5).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DataCli {
    /// `--data-root <dir>` (rank 1, ephemeral).
    pub explicit: Option<PathBuf>,
    /// `--remember`: persist the choice after L2 passes.
    pub remember: bool,
    /// `--forget-data-root`: drop the persisted choice.
    pub forget: bool,
    /// `--strict-data`: L3 mismatches are fatal.
    pub strict: bool,
    /// `--verify-data`: print the full report.
    pub verify: bool,
}

impl DataCli {
    /// Parses `args` (the flags this lane owns; every other argument is left
    /// to the existing dispatch).
    ///
    /// Both `--data-root <dir>` and `--data-root=<dir>` are accepted; a
    /// trailing `--data-root` without a value is an explicit error, never a
    /// silently ignored flag.
    pub fn parse<I: IntoIterator<Item = String>>(args: I) -> Result<Self, String> {
        let mut cli = Self::default();
        let mut iter = args.into_iter();
        while let Some(arg) = iter.next() {
            match arg.as_str() {
                "--data-root" => {
                    cli.explicit = Some(PathBuf::from(
                        iter.next()
                            .ok_or("--data-root needs a folder (the one containing ARENA2)")?,
                    ));
                }
                "--remember" => cli.remember = true,
                "--forget-data-root" => cli.forget = true,
                "--strict-data" => cli.strict = true,
                "--verify-data" => cli.verify = true,
                other => {
                    if let Some(value) = other.strip_prefix("--data-root=") {
                        if value.is_empty() {
                            return Err(
                                "--data-root= needs a folder (the one containing ARENA2)".into()
                            );
                        }
                        cli.explicit = Some(PathBuf::from(value));
                    }
                }
            }
        }
        Ok(cli)
    }
}

/// The flags of this process, parsed once.
pub fn cli() -> Result<&'static DataCli, String> {
    static CLI: OnceLock<Result<DataCli, String>> = OnceLock::new();
    CLI.get_or_init(|| DataCli::parse(std::env::args()))
        .as_ref()
        .map_err(|reason| reason.clone())
}

/// The resolved root of this process, with its warnings printed once.
///
/// Every caller gets the same root: one resolution per process, so the
/// probes, the windows and the audio runtime can never disagree.
pub fn data_root() -> Result<&'static DataRoot, String> {
    static ROOT: OnceLock<Result<DataRoot, String>> = OnceLock::new();
    let root = ROOT
        .get_or_init(|| {
            let options = ResolveOptions::from_process(cli()?.explicit.clone(), cli()?.strict);
            DataRoot::resolve_with(&options).map_err(|error| error.player_message())
        })
        .as_ref()
        .map_err(|reason| reason.clone())?;
    announce_warnings(root);
    Ok(root)
}

/// Prints the root's own warnings **once per process** (§4.4).
///
/// Every consumer (probes, windows, `--verify-data`, the product gate) goes
/// through here, so a dropped remembered root or an unreadable config is
/// announced exactly once instead of once per call site.
fn announce_warnings(root: &DataRoot) {
    static ANNOUNCED: AtomicBool = AtomicBool::new(false);
    if ANNOUNCED.swap(true, Ordering::Relaxed) {
        return;
    }
    for warning in root.warnings() {
        eprintln!("{}: {}", warning.code, warning.message);
    }
}

/// **§4.6** data directory of the resolved root (`ARENA2`/`arena2`, whichever
/// exists on disk).
pub fn arena2_dir() -> Result<PathBuf, String> {
    data_root().map(|root| root.arena2_dir().to_path_buf())
}

/// File inside the data directory, case-insensitively (the retail source opens
/// case-insensitively and the GOG install ships `faction.txt` where the corpus
/// declares `FACTION.TXT`).
///
/// The error text is the historical one (`<name> not found under <dir>`) so the
/// probes that print it keep their output.
pub fn arena2_path(name: &str) -> Result<PathBuf, String> {
    let root = data_root()?;
    let dir = root.arena2_dir();
    root.resolve_relative(&root.rel(name))
        .map_err(|_| format!("{name} not found under {dir:?}"))
}

/// Installation root (the folder that contains `ARENA2`) — e.g. for
/// `FALL.EXE`, which lives next to the data directory (spec M21 §0).
pub fn install_root() -> Result<PathBuf, String> {
    data_root().map(|root| root.base().to_path_buf())
}

/// Bytes of a file inside the data directory.
pub fn read_arena2(name: &str) -> Result<Vec<u8>, String> {
    let path = arena2_path(name)?;
    std::fs::read(&path).map_err(|err| format!("{name} unreadable: {err} ({path:?})"))
}

/// Bytes of a file inside the installation root (e.g. `FALL.EXE`).
///
/// The error text is the historical one
/// (`FALL.EXE unreadable: <err> (<path>)`) shared by every probe that used to
/// carry its own copy of this lookup.
pub fn read_install_file(name: &str) -> Result<Vec<u8>, String> {
    let root = data_root()?;
    let path = root
        .resolve_relative(name)
        .map_err(|err| {
            format!(
                "{name} unreadable: {} ({})",
                err.message(),
                root.base().display()
            )
        })?;
    std::fs::read(&path).map_err(|err| format!("{name} unreadable: {err} ({path:?})"))
}

/// **§4.3/§4.4** the product gate: L2 (L3 too when `--strict-data`) **before
/// the window exists**, then persistence and the `DATA ROOT` line.
///
/// Never persists a root that did not pass L2 (D4): `remember` runs after the
/// audit, and the audit's hard failures return first.
pub fn prepare_product_data() -> Result<(), String> {
    let cli = cli()?;
    let root = data_root()?;
    let audit = validation::audit_full(root);

    for warning in &audit.warnings {
        eprintln!("{}: {}", warning.code, warning.message);
    }
    for (rel, reason) in &audit.mismatches {
        eprintln!("W-DATA-005: {rel} {reason}");
    }
    if !audit.passes(root.strict()) {
        return Err(first_failure(&audit, root.base().display().to_string()));
    }

    if cli.remember {
        let path = root.remember(&Env::from_process())?;
        eprintln!("remembered data root in {}", path.display());
    }

    write_state_log(root, &audit);
    println!(
        "DATA ROOT = {} (origin: {})",
        root.base().display(),
        root.origin().label()
    );
    Ok(())
}

/// The first hard failure of an audit as a §4.4 line.
fn first_failure(audit: &Audit, root: String) -> String {
    if let Some(rel) = audit.missing.first() {
        return format!(
            "E-DATA-003: data folder \"{root}\" is incomplete: missing {rel}. \
             Run --verify-data for the full list."
        );
    }
    if let Some((rel, reason)) = audit.invalid.first() {
        return format!("E-DATA-004: {rel} {reason}");
    }
    if let Some((rel, reason)) = audit.mismatches.first() {
        return format!("E-DATA-004: {rel} {reason} (--strict-data)");
    }
    "E-DATA-004: the data folder did not pass validation (Run --verify-data).".to_string()
}

/// `data-root.log` next to the application state (§4.4): the chosen root, its
/// origin, its data directory and its fingerprint.
fn write_state_log(root: &DataRoot, audit: &Audit) {
    let Some(path) = Env::from_process().state_log_path() else {
        return;
    };
    if let Some(parent) = path.parent() {
        if std::fs::create_dir_all(parent).is_err() {
            return;
        }
    }
    let body = format!(
        "root: {}\norigin: {}\narena2: {}\nfingerprint: {}\nstrict: {}\n",
        root.base().display(),
        root.origin().label(),
        root.arena2_dir().display(),
        daggerfall_inventory::data_root::hex(&audit.identity),
        root.strict()
    );
    let _ = std::fs::write(&path, body);
}

/// `--verify-data`: the full L1/L2/L3 report. `0` pass, `2` fail (§4.4).
pub fn verify_data() -> i32 {
    match data_root() {
        Ok(root) => {
            let audit = validation::audit_full(root);
            // `render` already ends with the `W-DATA-00x` lines; printing them
            // again here duplicated every warning in the log.
            print!("{}", audit.render(root.strict()));
            if audit.passes(root.strict()) {
                // `--remember` is honoured here too: the audit just proved L2,
                // so persisting is legal (D4). Silently dropping the flag would
                // make `--verify-data --remember` a lie.
                if cli().map(|cli| cli.remember).unwrap_or(false) {
                    match root.remember(&Env::from_process()) {
                        Ok(path) => eprintln!("remembered data root in {}", path.display()),
                        Err(reason) => eprintln!("{reason}"),
                    }
                }
                0
            } else {
                2
            }
        }
        Err(reason) => {
            eprintln!("{reason}");
            2
        }
    }
}

/// `--forget-data-root` (§4.5 reset). Returns the line to show.
pub fn forget_data_root() -> Result<String, String> {
    let env = Env::from_process();
    let Some(path) = env.config_path() else {
        return Err(
            "E-DATA-001: no HOME/XDG_CONFIG_HOME — there is no config file to forget".into(),
        );
    };
    match config_file::forget_root(&path)? {
        true => Ok(format!("forgot the remembered data root ({})", path.display())),
        false => Ok(format!(
            "no remembered data root ({} does not exist)",
            path.display()
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Result<DataCli, String> {
        DataCli::parse(args.iter().map(|arg| arg.to_string()))
    }

    /// Rank 1 accepts both spellings of the flag.
    #[test]
    fn data_root_flag_accepts_both_spellings() {
        assert_eq!(
            parse(&["--data-root", "/games/DF/app"]).expect("space form"),
            DataCli {
                explicit: Some(PathBuf::from("/games/DF/app")),
                ..DataCli::default()
            }
        );
        assert_eq!(
            parse(&["--data-root=/games/DF/app"]).expect("equals form"),
            DataCli {
                explicit: Some(PathBuf::from("/games/DF/app")),
                ..DataCli::default()
            }
        );
    }

    /// Every persistence/verification flag is recognised, and unrelated
    /// arguments are left alone.
    #[test]
    fn flags_are_recognised_among_other_arguments() {
        let cli = parse(&[
            "--probe",
            "MENU-001",
            "--remember",
            "--strict-data",
            "--verify-data",
            "--forget-data-root",
        ])
        .expect("flags parse");
        assert!(cli.remember && cli.strict && cli.verify && cli.forget);
        assert_eq!(cli.explicit, None);
    }

    /// A flag without its value is an explicit error, never ignored.
    #[test]
    fn missing_values_are_errors() {
        assert!(parse(&["--data-root"]).is_err());
        assert!(parse(&["--data-root="]).is_err());
    }
}
