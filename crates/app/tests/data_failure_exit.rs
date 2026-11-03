//! Antibody for D3 (data contract §3/§4.4): a product launch that cannot
//! compose the retail data must fail **loudly**.
//!
//! Measured on the real binary (not on a constant): exit code, the §4.4 code
//! that names what is missing, and the absence of the retired headless
//! fallback line. Before D3 the binary printed the reason, ran the empty F0
//! app (0 plugins, no window) and exited 0 — a gate could not tell "it
//! started" from "it degraded".
//!
//! `window` is an OPT-IN feature (`Cargo.toml`), so the antibody runs on the
//! product build:
//!
//! ```text
//! cargo test -p daggerfall-app --features window --test data_failure_exit
//! ```
//!
//! The failure happens in the preflight, before any window is created, so the
//! test needs no display server.

#![cfg(feature = "window")]

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const BIN: &str = env!("CARGO_BIN_EXE_daggerfall-app");

/// Line the retired fallback printed. D3 removed it from the failure path.
const RETIRED_FALLBACK: &str = "fallback to the F0 headless app";

/// Scratch data root, removed on drop so the test cleans up on panic too.
struct Scratch(PathBuf);

impl Scratch {
    fn new(tag: &str) -> Self {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock after the Unix epoch")
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "df-data-failure-{tag}-{}-{stamp}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).expect("create scratch data root");
        Self(dir)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Runs the product binary against `root` with no display attached.
///
/// The environment is isolated: the config file outranks every env variable
/// (§4.1 rank 2), so a `config.toml` from the machine running the test would
/// decide the outcome instead of `DF_APP_ROOT`. `HOME`/`XDG_*` point at an
/// empty scratch tree, and the deprecated `$DF_ARENA2` aliases are removed.
fn launch(root: &Path) -> Output {
    let env = Scratch::new("env");
    let config_home = env.path().join("config");
    let state_home = env.path().join("state");
    std::fs::create_dir_all(&config_home).expect("create XDG_CONFIG_HOME");
    std::fs::create_dir_all(&state_home).expect("create XDG_STATE_HOME");

    Command::new(BIN)
        .env("DF_APP_ROOT", root)
        .env("HOME", env.path())
        .env("XDG_CONFIG_HOME", &config_home)
        .env("XDG_STATE_HOME", &state_home)
        .env_remove("DF_ARENA2")
        .env_remove("DAGGERFALL_ARENA2")
        .env_remove("DF_ASSET_REGISTRY")
        .env_remove("DISPLAY")
        .env_remove("WAYLAND_DISPLAY")
        .output()
        .expect("run the product binary")
}

fn transcript(out: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

/// A root that is not a data folder at all: `E-DATA-002`, exit 2, no fallback.
#[test]
fn unknown_data_root_exits_2_and_names_the_folder() {
    let root = Scratch::new("no-root");
    let out = launch(root.path());
    let text = transcript(&out);

    assert_eq!(
        out.status.code(),
        Some(2),
        "a launch without retail data must exit 2, got {:?}\n{text}",
        out.status
    );
    assert!(
        text.contains("E-DATA-002"),
        "the message must carry the §4.4 code\n{text}"
    );
    assert!(
        text.contains(&root.path().display().to_string()),
        "the message must name the data folder that was checked\n{text}"
    );
    assert!(
        !text.contains(RETIRED_FALLBACK),
        "the failure path must not fall back to the empty F0 app\n{text}"
    );
}

/// A root that has `ARENA2` but is missing a mandatory file: the message names
/// the file (`ARENA2/MAPS.BSA`) and the run still exits 2.
#[test]
fn incomplete_data_root_exits_2_and_names_the_missing_file() {
    let root = Scratch::new("incomplete");
    let arena2 = root.path().join("ARENA2");
    std::fs::create_dir_all(&arena2).expect("create ARENA2");
    // `PathChecker::looks_like_data_root` authenticates the root with this one
    // file, so the composition reaches the mandatory-file preflight.
    std::fs::write(arena2.join("ARCH3D.BSA"), b"").expect("seed ARCH3D.BSA");

    let out = launch(root.path());
    let text = transcript(&out);

    assert_eq!(
        out.status.code(),
        Some(2),
        "an incomplete data folder must exit 2, got {:?}\n{text}",
        out.status
    );
    let expected = format!(
        "E-DATA-003: data folder \"{}\" is incomplete: missing ARENA2/MAPS.BSA.",
        root.path().display()
    );
    assert!(
        text.contains(&expected),
        "the message must name the missing file (§4.4)\nwant: {expected}\ngot:\n{text}"
    );
    assert!(
        !text.contains(RETIRED_FALLBACK),
        "the failure path must not fall back to the empty F0 app\n{text}"
    );
}
