//! READ-ONLY runtime tests over the real ARCH3D.BSA (M1).
//!
//! Policy identical to the parser's (`arch3d_runtime.rs`): the runtime lives
//! outside the repo — `$DF_ARENA2` or `$HOME/Games/Daggerfall/app/ARENA2`;
//! 0 Bethesda bytes are copied into the repo (F0 guardrails, INV-L99-6). If
//! the runtime is not present, the tests are skipped with an explicit
//! message (they are not stubs: the logic runs in full when data is there).
//!
//! These tests are the FALSIFICATION of the converter against the GOG retail
//! (sha256 `a51a9e14…6a85`, input_manifest S1): every derived
//! `PointOffset ÷ divisor` must fall inside `[0, point_count)` across the
//! 10251 records — if the decoded dfbsa.txt layout were wrong, this blows up.

use std::path::PathBuf;

use daggerfall_parser_bevy_bridge as bridge;

fn arena2_dir() -> Option<PathBuf> {
    if let Ok(dir) = std::env::var("DF_ARENA2") {
        return Some(PathBuf::from(dir));
    }
    std::env::var("HOME")
        .ok()
        .map(|home| PathBuf::from(home).join("Games/Daggerfall/app/ARENA2"))
}

fn load_arch3d() -> Option<Vec<u8>> {
    let Some(dir) = arena2_dir() else {
        eprintln!("SKIP arch3d_bridge_runtime: $HOME not set");
        return None;
    };
    let path = dir.join("ARCH3D.BSA");
    match std::fs::read(&path) {
        Ok(bytes) => Some(bytes),
        Err(err) => {
            eprintln!("SKIP arch3d_bridge_runtime: runtime missing ({path:?}): {err}");
            None
        }
    }
}

#[test]
fn converts_every_arch3d_record_of_retail() {
    let Some(data) = load_arch3d() else { return };
    let archive = bridge::parse_bsa(&data).expect("ARCH3D.BSA must parse");
    assert_eq!(archive.entries.len(), 10251, "golden dfbsa.txt: 0x280B records");

    let mut converted = 0usize;
    let mut failures: Vec<(usize, String)> = Vec::new();
    for index in 0..archive.entries.len() {
        match bridge::mesh_spec_at(&archive, &data, index) {
            Ok(spec) => {
                // Invariants of the converted geometry.
                assert_eq!(spec.positions.len(), spec.normals.len(), "idx {index}");
                assert_eq!(spec.positions.len() % 3, 0, "triangle soup: idx {index}");
                assert_eq!(spec.indices.len() / 3, spec.triangle_count(), "idx {index}");
                for (v, p) in spec.positions.iter().enumerate() {
                    assert!(p.iter().all(|c| c.is_finite()), "non-finite position idx {index} v{v}");
                }
                for n in &spec.normals {
                    let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
                    assert!((len - 1.0).abs() < 1e-3, "non-unit normal idx {index}: {len}");
                }
                converted += 1;
            }
            Err(err) => failures.push((index, err.to_string())),
        }
    }
    eprintln!("bridge runtime: converted={converted}/10251 failures={}", failures.len());
    // Strong falsification: the converter must absorb the whole retail with
    // the dfbsa.txt layout (S1). If a failure shows up, it is a parity
    // finding — document it here with its id, never silence it.
    assert!(
        failures.is_empty(),
        "records that do not convert (new parity finding): {failures:?}"
    );
    assert_eq!(converted, 10251);
}

#[test]
fn m1_block_selection_is_deterministic_on_pinned_input() {
    let Some(data) = load_arch3d() else { return };
    let archive = bridge::parse_bsa(&data).expect("parse");

    // M1 selection policy (identical to app: first convertible record with
    // triangles, in directory order) — deterministic over the sha-pinned
    // input.
    let select = |archive: &bridge::BsaArchive| -> Option<(usize, u32, usize)> {
        for index in 0..archive.entries.len() {
            let bridge::Entry::Numbered { record_id, .. } = &archive.entries[index] else {
                continue;
            };
            let Ok(spec) = bridge::mesh_spec_at(archive, &data, index) else {
                continue;
            };
            if spec.triangle_count() > 0 {
                return Some((index, *record_id, spec.triangle_count()));
            }
        }
        None
    };

    let first = select(&archive).expect("at least one convertible block must exist");
    let second = select(&archive).expect("repeated selection");
    assert_eq!(first, second, "the selection must be deterministic");
    eprintln!(
        "M1 selection: record idx={} id={} triangles={}",
        first.0, first.1, first.2
    );
    // Empirical pin (sha-pinned input S1, GOG retail 1.9.3.2649): the first
    // record of the directory (idx=0, id=44005) converts to 80 triangles.
    assert_eq!(first, (0, 44005, 80), "M1 selection != S1 empirical pin");
}
