//! PROBE-VIEW-001 over retail ARCH3D.BSA (M2) — INV-L99-5.
//!
//! Policy identical to `arch3d_bridge_runtime.rs`: runtime outside the repo
//! (`$DF_ARENA2` or `$HOME/Games/Daggerfall/app/ARENA2`); if absent, explicit
//! SKIP (no stubs: the logic runs in full when data is there).
//!
//! It is the SAME `run_view_probe` core that the CLI
//! `cargo run -p daggerfall-app --probe VIEW-001` runs, sharing
//! `resolve_step` with the M2 window system (Anti-Pantalla: no parallel test
//! disconnected from gameplay).

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
    let dir = arena2_dir()?;
    let path = dir.join("ARCH3D.BSA");
    match std::fs::read(&path) {
        Ok(bytes) => Some(bytes),
        Err(err) => {
            eprintln!("SKIP probe_view_001_runtime: runtime missing ({path:?}): {err}");
            None
        }
    }
}

/// Selection identical to the app (first convertible record with ≥1 triangle;
/// pinned in M1: idx=0, id=44005, 80 triangles).
fn first_convertible(data: &[u8]) -> Option<(usize, u32, bridge::MeshSpec)> {
    let archive = bridge::parse_bsa(data).ok()?;
    for (index, entry) in archive.entries.iter().enumerate() {
        let bridge::Entry::Numbered { record_id, .. } = entry else {
            continue;
        };
        let Ok(spec) = bridge::mesh_spec_at(&archive, data, index) else {
            continue;
        };
        if spec.triangle_count() > 0 {
            return Some((index, *record_id, spec));
        }
    }
    None
}

#[test]
fn probe_view_001_passes_on_retail_block() {
    let Some(data) = load_arch3d() else { return };
    let (index, record_id, spec) =
        first_convertible(&data).expect("at least one convertible record must exist");
    eprintln!(
        "PROBE-VIEW-001 runtime: record idx={index} id={record_id} ({} triangles)",
        spec.triangle_count()
    );
    let report = bridge::run_view_probe(&spec);
    for check in &report.checks {
        eprintln!(
            "[{}] {} — {}",
            if check.ok { "PASS" } else { "FAIL" },
            check.id,
            check.detail
        );
    }
    eprintln!(
        "VERDICT: PROBE-VIEW-001 {} (INV-L99-5)",
        if report.pass { "PASS" } else { "FAIL" }
    );
    for check in &report.checks {
        assert!(check.ok, "check {} FAIL: {}", check.id, check.detail);
    }
    assert!(
        report.pass,
        "PROBE-VIEW-001 FAIL on the pinned retail block: {:?}",
        report.checks
    );
}
