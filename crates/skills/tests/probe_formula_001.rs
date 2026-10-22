//! PROBE-FORMULA-001 (M5) — INV-L99-3 "causal formulas",
//! **trade price** slice: 100% DFU parity golden vectors
//! (Mode A only, `reference/dfu-spec` corpus on disk).
//!
//! It is the SAME `run_formula_probe` core that the CLI
//! `cargo run -p daggerfall-app -- --probe COMMERCE-001` prints (block C) —
//! Anti-Screen: runtime invariant with asserts, 0 screenshot-only.
//!
//! VSIM §4 row 4 command (`cargo test -p daggerfall-skills --test
//! probe_formula_001`) — the baseline's `[INFERENCE command]` becomes REAL.

use daggerfall_skills::formula::{run_formula_probe, trade_price_check, GOLDEN_VECTORS};

#[test]
fn probe_formula_001_passes_on_dfu_golden() {
    let report = run_formula_probe();
    for check in &report.checks {
        eprintln!(
            "[{}] {} — {}",
            if check.ok { "PASS" } else { "FAIL" },
            check.id,
            check.detail
        );
    }
    eprintln!(
        "VERDICT: PROBE-FORMULA-001 {} (INV-L99-3)",
        if report.pass { "PASS" } else { "FAIL" }
    );
    for check in &report.checks {
        assert!(check.ok, "check {} FAIL: {}", check.id, check.detail);
    }
    assert!(report.pass, "PROBE-FORMULA-001 FAIL: {:?}", report.checks);
}

/// Anti-false-positive: a tampered vector (expected +1) MUST break the
/// parity check — the probe cannot pass by construction.
#[test]
fn probe_formula_001_fails_on_wrong_expectation() {
    let mut tampered = GOLDEN_VECTORS.to_vec();
    tampered[0].expected += 1;
    let check = trade_price_check(&tampered);
    assert!(
        !check.ok,
        "parity must fail with a tampered expectation: {}",
        check.detail
    );
}

/// The vectors used by M5 commerce (buy potion 45 · sell pelts
/// 25) are in the golden table — the COMMERCE-001 probe uses no
/// invented prices.
#[test]
fn m5_catalog_prices_are_golden() {
    let buy = GOLDEN_VECTORS.iter().any(|v| {
        v.cost == 80 && v.shop_quality == 10 && !v.selling && v.expected == 45
    });
    let sell = GOLDEN_VECTORS
        .iter()
        .any(|v| v.cost == 50 && v.shop_quality == 10 && v.selling && v.expected == 25);
    assert!(buy, "potion (cost 80, buy) must have golden 45");
    assert!(sell, "pelts (cost 50, sell) must have golden 25");
}
