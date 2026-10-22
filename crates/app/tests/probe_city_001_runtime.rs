//! PROBE-CITY-001 over retail ARCH3D.BSA (M3) — INV-L99-5 extended to the
//! city: walk along a street between two blocks without clipping through
//! geometry (runtime invariant with asserts, Anti-Screen).
//!
//! Identical policy to `probe_view_001_runtime.rs` (bridge): runtime outside
//! the repo (`$DF_ARENA2` or `$HOME/Games/Daggerfall/app/ARENA2`); if absent,
//! an explicit SKIP (no stubs: the logic runs in full when data is present).
//!
//! It is the SAME `run_city_probe` core the CLI
//! `cargo run -p daggerfall-app -- --probe CITY-001` runs, and the same
//! `resolve_step`/`CollisionGrid` the M3 window uses (Anti-Screen: 0 parallel
//! test disconnected from the gameplay).

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
            eprintln!("SKIP probe_city_001_runtime: missing runtime ({path:?}): {err}");
            None
        }
    }
}

#[test]
fn probe_city_001_passes_on_retail_city() {
    let Some(data) = load_arch3d() else { return };
    // Same selection as the CLI (`--probe CITY-001`): ONE shared function —
    // the CLI gets here via `main.rs::convertible_blocks` and this test calls
    // it directly: `bridge::solid_convertible_blocks` (guardrail §5a.2, 0
    // divergent duplicated logic). Solids because the M3 checks assume CLOSED
    // landmarks (§5a.3 — the convertible `44005` opens an 11 m passage along
    // `+X` at `z=40` and the ray and the r=0.3 march thread it); the criterion
    // measures the r=0.3 sphere at eye y=1.6 in the fit-to-lot WORLD (R-M10-FIN
    // warning: in raw local meters y=1.6 is not the same section).
    let solid = bridge::solid_convertible_blocks(&data, 6)
        .expect("retail ARCH3D.BSA must have solid convertible records");
    let ids: Vec<u32> = solid.iter().map(|(id, _)| *id).collect();
    eprintln!("PROBE-CITY-001 runtime: solid landmarks {ids:?}");
    let landmarks: Vec<bridge::MeshSpec> = solid.into_iter().map(|(_, spec)| spec).collect();

    // Same city as the CLI (3x3, internal street gap 1) — same core.
    let layout = bridge::CityLayout::new(3);
    let city = bridge::build_city_mesh(&layout, &landmarks);
    eprintln!(
        "PROBE-CITY-001 runtime: {} landmarks · 3x3 city · {} combined triangles",
        landmarks.len(),
        city.triangle_count()
    );

    let report = bridge::run_city_probe(&city, &layout, 1);
    for check in &report.checks {
        eprintln!(
            "[{}] {} — {}",
            if check.ok { "PASS" } else { "FAIL" },
            check.id,
            check.detail
        );
    }
    eprintln!(
        "VERDICT: PROBE-CITY-001 {} (INV-L99-5 city, CR-bbd5db82)",
        if report.pass { "PASS" } else { "FAIL" }
    );
    for check in &report.checks {
        assert!(check.ok, "check {} FAIL: {}", check.id, check.detail);
    }
    assert!(
        report.pass,
        "PROBE-CITY-001 FAIL on retail city: {:?}",
        report.checks
    );
}
