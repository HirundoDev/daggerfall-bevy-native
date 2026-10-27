//! F1 criterion (c): the 11 organ crates must stay free of bevy.
//!
//! The extraction of `game-core` is allowed to depend on bevy (it *is* the
//! bevy composition surface); the organs are not. This test is the antibody:
//! it fails the day someone adds `bevy` to an organ manifest or a `use bevy`
//! to an organ source file, instead of letting the drift be discovered by a
//! side.

use std::path::{Path, PathBuf};

/// The 11 organ/leaf crates (`crates/app` and `crates/game-core` are excluded:
/// both are bevy crates by design).
const ORGAN_CRATES: [&str; 11] = [
    "nervous",
    "heart",
    "memory",
    "behavior",
    "perception",
    "skills",
    "inventory",
    "crafting",
    "combat",
    "antibodies",
    "daggerfall_parser_rs",
];

fn crates_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crates dir")
        .to_path_buf()
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|err| panic!("read {}: {err}", path.display()))
}

#[test]
fn no_organ_crate_depends_on_bevy() {
    for organ in ORGAN_CRATES {
        let manifest = crates_dir().join(organ).join("Cargo.toml");
        assert!(
            manifest.is_file(),
            "missing manifest: {}",
            manifest.display()
        );
        let text = read(&manifest);
        for line in text.lines() {
            let code = line.trim();
            if code.starts_with('#') {
                continue;
            }
            assert!(
                !code.starts_with("bevy") && !code.contains("\"bevy/"),
                "{organ}: bevy leaked into the organ manifest: {code}"
            );
        }
    }
}

#[test]
fn no_organ_source_uses_bevy() {
    for organ in ORGAN_CRATES {
        let src = crates_dir().join(organ).join("src");
        let mut stack = vec![src];
        while let Some(dir) = stack.pop() {
            let entries = std::fs::read_dir(&dir)
                .unwrap_or_else(|err| panic!("read_dir {}: {err}", dir.display()));
            for entry in entries {
                let path = entry.expect("dir entry").path();
                if path.is_dir() {
                    stack.push(path);
                    continue;
                }
                if path.extension().and_then(|ext| ext.to_str()) != Some("rs") {
                    continue;
                }
                let text = read(&path);
                assert!(
                    !text.contains("use bevy"),
                    "{organ}: `use bevy` found in {}",
                    path.display()
                );
            }
        }
    }
}
