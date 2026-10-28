//! `formula` — **pure** gameplay formulas of the 🎮 Skills organ (M5):
//! DFU *clean-room* trade price + executable core of
//! **PROBE-FORMULA-001** (INV-L99-3 "causal formulas", VSIM §4 row 4).
//!
//! **Source (Mode A, clean-room):** Daggerfall Unity pinned at
//! `Interkarma/daggerfall-unity@2343305d1d83ccc0de57a81e3b1e61188a997e34`
//! (`reference/dfu-spec/CLASSIFICATION.md` §1; MIT), local mirror
//! `reference/dfu-spec/Assets/Scripts/Game/Formulas/FormulaHelper.cs`, sha256
//! `b7ba3b2dfdf3e10826b6c5532d593e35125731afac5fd2524ddae6c1c3a2a09d`:
//!
//! - `CalculateTradePrice(int cost, int shopQuality, bool selling)`
//!   — `FormulaHelper.cs:1976-2004` (override `:1979-1980`; merchant levels
//!   `:1983-1984`; sell branch `:1990-1995`; buy branch `:1996-2001`;
//!   `return amount` `:2003`).
//! - `cost` is the **shelf cost**, i.e. `CalculateCost(item.value, shopQuality)`
//!   — `FormulaHelper.cs:1884-1899` (clamp `:1890-1893`, regional `:1895`,
//!   `2 * (c * (q - 10) / 100 + c)` `:1896`, return `:1898`). Behaviour lives in
//!   `behavior::durability::calculate_cost`; this crate stays a pure leaf and
//!   the **app** composes the two stages (`m5_probe::dfu_buy_price` /
//!   `dfu_sell_price`, and the 40-vector pipeline gate).
//!
//! **Exact integer** port of the C# arithmetic (`i32`, shifts `<<`/`>>`,
//! division toward zero — same operands as C# over the documented domain):
//! 0 floating point (NFR NUMERIC-never-FLOAT), 0 chance (the function rolls
//! no dice ⇒ **pure formula**, eligible for 100%-parity golden vectors).
//!
//! **Documented domain:** `cost ≥ 0`, `shop_quality ∈ [0,20]`,
//! `player_* ∈ [0,100]` (DFU ranges). Outside the domain it is undefined.
//!
//! Rules: pure leaf 0-dependencies (precedent `heart::TimeCheck`
//! duplicates `ProbeCheck` — own stable ids, 0 skills→bridge dependency)
//! · 0 persistence · 0 Bevy/winit types.

/// Executable check of PROBE-FORMULA-001 (own ids, `TimeCheck` pattern in
/// heart — skills does NOT depend on the bridge).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormulaCheck {
    /// Stable identifier of the check.
    pub id: &'static str,
    /// `true` if the check passed.
    pub ok: bool,
    /// Human-readable evidence (raw for the note/handoff).
    pub detail: String,
}

/// Full PROBE-FORMULA-001 report.
#[derive(Debug, Clone)]
pub struct FormulaProbeReport {
    /// Checks executed, in order.
    pub checks: Vec<FormulaCheck>,
    /// Global PASS = every check ok.
    pub pass: bool,
}

/// Golden parity vector: input + expected output (transcribed from the C#
/// source with an **independent** transcription — parity is verified against
/// this value, not against the implementation itself).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FormulaVector {
    /// Base cost of the item (`cost` input).
    pub cost: i32,
    /// Shop quality (`shopQuality` input).
    pub shop_quality: i32,
    /// `true` = the player **sells** to the merchant (`selling`).
    pub selling: bool,
    /// Player mercantile (input).
    pub player_mercantile: i32,
    /// Player personality (input).
    pub player_personality: i32,
    /// Expected output (`CalculateTradePrice`).
    pub expected: i32,
}

/// DFU parity golden vectors (11 vectors: buy/sell + domain edges: quality
/// 0/20, stats 0/100, cost 0). Independent transcription of
/// `FormulaHelper.cs:1976-2004` on the pinned mirror (`2343305d…`, sha256
/// `b7ba3b2d…`) — the earlier header named the older snapshot `0038a1e6…` and
/// a `:1960-1988` range that starts before the function's real first line.
///
/// Cross-checked 2026-10-27 by an **independent Python oracle** that re-derives
/// every `expected` from the inputs alone (C# truncation-toward-zero):
/// 11/11 reproduced, 0 divergence — evidence
/// `docs/sintergic/l5-mission/m5-calculate-cost-evidence-CR-112b1058/`.
pub const GOLDEN_VECTORS: &[FormulaVector] = &[
    FormulaVector { cost: 100, shop_quality: 10, selling: false, player_mercantile: 50, player_personality: 50, expected: 56 },
    FormulaVector { cost: 100, shop_quality: 10, selling: true, player_mercantile: 50, player_personality: 50, expected: 50 },
    FormulaVector { cost: 80, shop_quality: 10, selling: false, player_mercantile: 50, player_personality: 50, expected: 45 },
    FormulaVector { cost: 50, shop_quality: 10, selling: true, player_mercantile: 50, player_personality: 50, expected: 25 },
    FormulaVector { cost: 275, shop_quality: 5, selling: false, player_mercantile: 45, player_personality: 60, expected: 127 },
    FormulaVector { cost: 1000, shop_quality: 15, selling: true, player_mercantile: 70, player_personality: 55, expected: 460 },
    FormulaVector { cost: 12, shop_quality: 0, selling: false, player_mercantile: 30, player_personality: 40, expected: 4 },
    FormulaVector { cost: 300, shop_quality: 20, selling: true, player_mercantile: 100, player_personality: 100, expected: 133 },
    FormulaVector { cost: 1, shop_quality: 10, selling: false, player_mercantile: 0, player_personality: 0, expected: 0 },
    FormulaVector { cost: 0, shop_quality: 10, selling: false, player_mercantile: 50, player_personality: 50, expected: 0 },
    FormulaVector { cost: 60, shop_quality: 12, selling: true, player_mercantile: 80, player_personality: 45, expected: 31 },
];

/// DFU `CalculateTradePrice` — exact integer port (clean-room, Mode A) of
/// `FormulaHelper.cs:1976-2004`.
///
/// Terms and their lines: merchant levels `:1983-1984`
/// (`5 * (shopQuality - 10) + 50`, identical for mercantile and personality) ·
/// sell branch `:1990-1995` (`dm = ((100-ml)<<8)/200 + 128` ×
/// `(merc<<8)/200 + 128 >> 8`; `amount = ((179*dm>>8) + (51*dp>>8)) * cost >> 8`)
/// · buy branch `:1996-2001` (`dm = (ml<<8)/200 + 128` ×
/// `((100-merc)<<8)/200 + 128 >> 8`; `dp` likewise then `<< 6`;
/// `amount = ((192*dm>>8) + (dp>>8)) * cost >> 8`) · `return amount` `:2003`.
/// `cost` here is the shelf stage `CalculateCost` output (`:1884-1899`).
///
/// Verified C#↔Rust precedence parity: `*`/`/` > `>>`/`<<` in both languages
/// (parenthesized explicitly here for readability); C# arithmetic `>>` = Rust
/// `>>` on `i32`; division toward zero in both.
pub fn calculate_trade_price(
    cost: i32,
    shop_quality: i32,
    selling: bool,
    player_mercantile: i32,
    player_personality: i32,
) -> i32 {
    let merchant_mercantile_level = 5 * (shop_quality - 10) + 50;
    let merchant_personality_level = 5 * (shop_quality - 10) + 50;

    if selling {
        // DFU factors: (level<<8)/200+128 per attribute — the product/shift
        // semantics are exactly the C# ones (the golden vectors pin them).
        let factor_merchant_merc = ((100 - merchant_mercantile_level) << 8) / 200 + 128;
        let factor_player_merc = (player_mercantile << 8) / 200 + 128;
        let delta_mercantile = (factor_merchant_merc * factor_player_merc) >> 8;
        let factor_merchant_pers = ((100 - merchant_personality_level) << 8) / 200 + 128;
        let factor_player_pers = (player_personality << 8) / 200 + 128;
        let delta_personality = (factor_merchant_pers * factor_player_pers) >> 8;
        ((((179 * delta_mercantile) >> 8) + ((51 * delta_personality) >> 8)) * cost) >> 8
    } else {
        let factor_merchant_merc = (merchant_mercantile_level << 8) / 200 + 128;
        let factor_player_merc = ((100 - player_mercantile) << 8) / 200 + 128;
        let delta_mercantile = (factor_merchant_merc * factor_player_merc) >> 8;
        let factor_merchant_pers = (merchant_personality_level << 8) / 200 + 128;
        let factor_player_pers = ((100 - player_personality) << 8) / 200 + 128;
        let delta_personality = ((factor_merchant_pers * factor_player_pers) >> 8) << 6;
        ((((192 * delta_mercantile) >> 8) + (delta_personality >> 8)) * cost) >> 8
    }
}

/// Parity check against `vectors` (public for the anti-false-positive test:
/// a tampered vector MUST break the check).
pub fn trade_price_check(vectors: &[FormulaVector]) -> FormulaCheck {
    let mut mismatches = Vec::new();
    for vector in vectors {
        let got = calculate_trade_price(
            vector.cost,
            vector.shop_quality,
            vector.selling,
            vector.player_mercantile,
            vector.player_personality,
        );
        if got != vector.expected {
            mismatches.push(format!(
                "(cost={},q={},selling={},merc={},pers={}) → {got}, expected {}",
                vector.cost, vector.shop_quality, vector.selling,
                vector.player_mercantile, vector.player_personality, vector.expected,
            ));
        }
    }
    let ok = mismatches.is_empty();
    FormulaCheck {
        id: "dfu_trade_price_golden_parity",
        ok,
        detail: if ok {
            format!(
                "{}/{} golden vectors = C# CalculateTradePrice (buy and sell, edges q=0/20 stats=0/100 cost=0)",
                vectors.len(),
                vectors.len()
            )
        } else {
            format!("{} vectors diverge: {}", mismatches.len(), mismatches.join(" · "))
        },
    }
}

/// Executable core of **PROBE-FORMULA-001** (M5 price slice, N2).
pub fn run_formula_probe() -> FormulaProbeReport {
    let mut checks: Vec<FormulaCheck> = Vec::with_capacity(3);

    checks.push(trade_price_check(GOLDEN_VECTORS));

    // Determinism: same input → same output (pure formula, 0 state).
    let inputs: [(i32, i32, bool, i32, i32); 3] = [
        (80, 10, false, 50, 50),
        (50, 10, true, 50, 50),
        (275, 5, false, 45, 60),
    ];
    let det_ok = inputs.iter().all(|(c, q, s, m, p)| {
        let a = calculate_trade_price(*c, *q, *s, *m, *p);
        let b = calculate_trade_price(*c, *q, *s, *m, *p);
        a == b
    });
    checks.push(FormulaCheck {
        id: "dfu_trade_price_deterministic",
        ok: det_ok,
        detail: format!(
            "3 inputs evaluated 2× → identical={det_ok} (0 state, 0 chance: outcome = rules(stats)) · {:?}",
            inputs.map(|(c, q, s, m, p)| (c, q, s, m, p))
        ),
    });

    // Monotonicity in cost (integer multiplier fixed by parameters).
    let costs = [10i32, 20, 50, 100, 500];
    let prices: Vec<i32> = costs
        .iter()
        .map(|cost| calculate_trade_price(*cost, 10, false, 50, 50))
        .collect();
    let mono_ok = prices.windows(2).all(|pair| pair[1] >= pair[0]);
    checks.push(FormulaCheck {
        id: "dfu_trade_price_monotonic_in_cost",
        ok: mono_ok,
        detail: format!(
            "costs {costs:?} → prices {prices:?} · non-decreasing={mono_ok} (buy q=10 merc=50 pers=50)"
        ),
    });

    let pass = checks.iter().all(|c| c.ok);
    FormulaProbeReport { checks, pass }
}
