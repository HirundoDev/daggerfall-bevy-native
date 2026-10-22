//! PROBE-VIEW-001 — app CLI executable (M2).
//!
//! Runs in the **default headless** config (no `window` feature): invariant
//! INV-L99-5 does not depend on rendering. Usage:
//!
//! ```text
//! cargo run -p daggerfall-app -- --probe VIEW-001
//! ```
//!
//! **Documented delta (criterion (g)):** the command planned in VSIM §4 row 6
//! was `cargo run -p daggerfall-app --features vision -- --probe VIEW-001`
//! (labelled `[INFERENCE]`); this lane does NOT create an orphan `vision`
//! feature — the probe runs headless by default (stronger: CI-runnable, 0
//! GPU) and also with `--features window`. Same `run_view_probe` core as the
//! bridge runtime test and `resolve_step` shared with the M2 window.
//!
//! Source custody (PRE-2 §8 rule 3): Mode A only (PathChecker).

use daggerfall_parser_bevy_bridge as bridge;

/// Runs PROBE-VIEW-001 and returns the exit code (0 PASS · 1 FAIL).
pub fn run_view_001() -> i32 {
    println!("PROBE-VIEW-001 — INV-L99-5 (first-person contract) · CR-bbd5db82");
    let path_checker = daggerfall_inventory::path_checker::PathChecker::new();
    if path_checker.use_custom_universe() {
        println!("[FAIL] mode_a_only — Mode B out of M2 scope (REQ-L99-028 is PHASE 4)");
        println!("VERDICT: PROBE-VIEW-001 FAIL");
        return 1;
    }
    let bsa_path = match crate::mode_a_arch3d_path() {
        Ok(path) => path,
        Err(reason) => {
            println!("[FAIL] runtime_input — {reason}");
            println!("VERDICT: PROBE-VIEW-001 FAIL");
            return 1;
        }
    };
    let data = match std::fs::read(&bsa_path) {
        Ok(bytes) => bytes,
        Err(err) => {
            println!("[FAIL] runtime_input — reading {bsa_path:?}: {err}");
            println!("VERDICT: PROBE-VIEW-001 FAIL");
            return 1;
        }
    };
    let (record_index, record_id, spec) = match crate::first_convertible_block(&data) {
        Ok(found) => found,
        Err(reason) => {
            println!("[FAIL] runtime_input — {reason}");
            println!("VERDICT: PROBE-VIEW-001 FAIL");
            return 1;
        }
    };
    println!(
        "input: {bsa_path:?} ({} bytes) · record idx={record_index} id={record_id} · {} triangles",
        data.len(),
        spec.triangle_count()
    );

    let report = bridge::run_view_probe(&spec);
    for check in &report.checks {
        println!(
            "[{}] {} — {}",
            if check.ok { "PASS" } else { "FAIL" },
            check.id,
            check.detail
        );
    }
    if report.pass {
        println!(
            "VERDICT: PROBE-VIEW-001 PASS — INV-L99-5 coherent (grid↔layout + legibility) · anti-screen: executable runtime invariant, 0 screenshot"
        );
        0
    } else {
        let failed: Vec<&str> = report
            .checks
            .iter()
            .filter(|c| !c.ok)
            .map(|c| c.id)
            .collect();
        println!("VERDICT: PROBE-VIEW-001 FAIL — failed checks: {}", failed.join(", "));
        1
    }
}
