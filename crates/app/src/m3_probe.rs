//! PROBE-CITY-001 — app CLI executable (M3).
//!
//! Runs in the **default headless** config (without the `window` feature): the
//! INV-L99-5 city invariant does not depend on rendering. Usage: `cargo run -p
//! daggerfall-app -- --probe CITY-001`.
//!
//! **Documented delta (criterion (h)):** the planned pack was
//! `cargo run -p daggerfall-app -- --probe CITY-001` — no orphan vision
//! feature, same scheme as VIEW-001: runs headless by default (CI-
//! runnable, 0 GPU) and also with `--features window`. The `run_city_probe`
//! core is the one run by the bridge runtime test and the M3 window — same
//! code, not a disconnected parallel test.
//!
//! **Source custody (PRE-2 §8 rule 3):** Mode A only (PathChecker) —
//! ARCH3D.BSA records are read from the retail disk; the city is built with
//! the deterministic layout (SPEC-MISSING GRIDCITY, delta in the M3 note).
use daggerfall_parser_bevy_bridge::{
    build_city_mesh, run_city_probe, street_spawn, CityLayout,
};

/// Runs PROBE-CITY-001 - INV-L99-5 (city first-person contract, M3).
pub fn run_city_001() -> i32 {
    println!("PROBE-CITY-001 - INV-L99-5 (city first-person, M3, CR-bbd5db82)");
    // Source custody (PRE-2 §8 rule 3): Mode A only (PathChecker).
    let path_checker = daggerfall_inventory::path_checker::PathChecker::new();
    if path_checker.use_custom_universe() {
        println!("[FAIL] modo_a_arch3d_path: custom universe (PathChecker) not supported in this probe");
        return 1;
    }
    let data = match crate::mode_a_arch3d_path() {
        Ok(path) => {
            println!("[OK] modo_a_arch3d_path: {}", path.display());
            match std::fs::read(&path) {
                Ok(data) => data,
                Err(err) => {
                    println!("[FAIL] reading ARCH3D.BSA: {err}");
                    return 1;
                }
            }
        }
        Err(err) => {
            println!("[FAIL] modo_a_arch3d_path: {err}");
            return 1;
        }
    };
    // 3x3 city with landmarks from retail ARCH3D: first convertible records.
    let specs = match crate::convertible_blocks(&data, 6) {
        Ok(specs) => specs,
        Err(err) => {
            println!("[FAIL] convertible_blocks: {err}");
            return 1;
        }
    };
    println!("[OK] ARCH3D landmarks: {} records", specs.len());
    let layout = CityLayout::new(3);
    let city = build_city_mesh(&layout, &specs);
    let report = run_city_probe(&city, &layout, 1);
    for check in &report.checks {
        let status = if check.ok { "PASS" } else { "FAIL" };
        println!("[{status}] {} - {}", check.id, check.detail);
    }
    let (_, street_name) = street_spawn(&layout, 1);
    println!(
        "[OK] 3x3 city · 64 m blocks · 8-16 m streets · ARCH3D landmarks · {} combined triangles",
        city.combined.triangle_count()
    );
    if report.pass {
        println!("VERDICT: PROBE-CITY-001 PASS - street {street_name} walkable end-to-end, solid blocks, 0 crossings");
        0
    } else {
        let failed: Vec<&str> = report.checks.iter().filter(|c| !c.ok).map(|c| c.id).collect();
        println!("VERDICT: PROBE-CITY-001 FAIL - failed checks: {}", failed.join(", "));
        1
    }
}
