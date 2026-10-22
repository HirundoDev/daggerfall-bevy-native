//! PROBE-INTERIOR-001 — app CLI executable (M4).
//!
//! Runs in the **default headless** config (without the `window` feature): the
//! street→interior invariant does not depend on rendering. Usage: `cargo run
//! -p daggerfall-app -- --probe INTERIOR-001`. Block A = street→interior
//! transition through the door (bridge `run_interior_probe` core, same code as
//! the runtime test); block B = **time system** with 24 h cycle evidence (core
//! `daggerfall_heart::clock::run_time_cycle_probe`, same code as its tests).
//! PASS requires BOTH blocks.
//!
//! **Source custody (PRE-2 §8 rule 3):** Mode A only (PathChecker) —
//! ARCH3D.BSA records are read from the retail disk for the city. Block A is
//! the reference **synthetic** shop (0 I/O); the DoD **real** interior is
//! block C (`.RDB` + ARCH3D, M11). The premise "DFON1.BSA absent" was FALSE —
//! corrected 2026-10-20 (WI-3).
//!
//! M11: **real block C** in addition to synthetic A — the `.RDB` directory of
//! BLOCKS.BSA is enumerated and the deterministic subject by directory order
//! passes the real 12-check probe (single core
//! `m11_probe::evaluate_real_interior`, shared with `--probe RDB-001`;
//! DoD mapping: PROBE-INTERIOR-001 = INTERIOR-RDB-001). PASS requires A+B+C.
use daggerfall_parser_bevy_bridge::{
    build_city_mesh, build_shop_mesh, run_interior_probe, CityLayout,
};

/// Runs PROBE-INTERIOR-001 — INV-L99-5 extended to the interior (M4).
pub fn run_interior_001() -> i32 {
    println!("PROBE-INTERIOR-001 - INV-L99-5 (street→interior through the door, M4, CR-bbd5db82)");
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
    let specs = match crate::convertible_blocks(&data, 6) {
        Ok(specs) => specs,
        Err(err) => {
            println!("[FAIL] convertible_blocks: {err}");
            return 1;
        }
    };

    // M3 city intact + synthetic M4 shop (0 assets, 0 new .BSA).
    let layout = CityLayout::new(3);
    let city = build_city_mesh(&layout, &specs);
    let shop = build_shop_mesh();
    println!(
        "[OK] 3x3 city ({} triangles) + synthetic interior shop ({} triangles · \
         x=[0,5] z=[10,26] · door gap z=[16.8,19.2] · solid props ≥1.4 m)",
        city.triangle_count(),
        shop.triangle_count(),
    );

    println!("-- Block A: street → interior through the door --");
    let report = run_interior_probe(&city, &layout, &shop, 1);
    for check in &report.checks {
        let status = if check.ok { "OK" } else { "FAIL" };
        println!("[{status}] {} - {}", check.id, check.detail);
    }

    println!("-- Block B: time system (cycle) --");
    let time = daggerfall_heart::clock::run_time_cycle_probe();
    for check in &time.checks {
        let status = if check.ok { "OK" } else { "FAIL" };
        println!("[{status}] {} - {}", check.id, check.detail);
    }

    // M11: block C = REAL variant in addition to synthetic A —
    // enumeration of the BLOCKS.BSA `.RDB` directory + deterministic subject
    // by directory order + real 12-check probe (single core shared with
    // PROBE-RDB-001: `m11_probe::evaluate_real_interior`).
    println!("-- Block C: real .RDB interior (M11) --");
    let rdb_ok = crate::m11_probe::run_real_interior_block();

    let passed = report.checks.iter().filter(|c| c.ok).count()
        + time.checks.iter().filter(|c| c.ok).count()
        + usize::from(rdb_ok);
    let total = report.checks.len() + time.checks.len() + 1;
    if report.pass && time.pass && rdb_ok {
        println!(
            "M4 VE: first-person navigable interior — {passed}/{total} checks · \
             street→door→interior→door→street with 0 crossings · 24 h day/night cycle \
             verified · real block C .RDB (enumeration + 12-check probe) PASS"
        );
        println!("VERDICT: PROBE-INTERIOR-001 PASS - door transition without crossing a wall + time cycle + real .RDB interior");
        0
    } else {
        let mut failed: Vec<&str> = report
            .checks
            .iter()
            .filter(|c| !c.ok)
            .map(|c| c.id)
            .chain(time.checks.iter().filter(|c| !c.ok).map(|c| c.id))
            .collect();
        if !rdb_ok {
            failed.push("interior_real_rdb (block C)");
        }
        println!(
            "VERDICT: PROBE-INTERIOR-001 FAIL - failed checks: {}",
            failed.join(", ")
        );
        1
    }
}
