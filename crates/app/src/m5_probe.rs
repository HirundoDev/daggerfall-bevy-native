//! PROBE-COMMERCE-001 — core + CLI dispatch (M5).
//!
//! Anti-screen sequence of the C3 pack: **enter the shop → interact
//! (raycast) → buy 1 item → gold deducted + item added**, with invariants
//! verified at runtime (0 screenshot-only). Runs in the **default headless**
//! config; with `--features window` and PASS, the M5 window (`m5_window`)
//! opens the visual evidence (NPC + menu in the title bar).
//!
//! Blocks: A = world+interaction (bridge geometry/raycast) ·
//! B = dialogue+commerce (`daggerfall_inventory::commerce` + DFU price from
//! `daggerfall_skills::formula`) · C (in the CLI) = pure PROBE-FORMULA-001.
//!
//! Composition in `app` (the `app` depends on all organs + bridge — the
//! organs do NOT call each other; see bevy-bridge-matrix and bus-only).
//! Source custody: Mode A only (PathChecker), same scheme as M4.

use daggerfall_inventory::commerce::{InventoryLite, MerchantStock, StockItem, TradeError, TradeSession};
use daggerfall_parser_bevy_bridge as bridge;
use daggerfall_parser_bevy_bridge::{
    march_axis, CellStatus, CityLayout, CityMesh, ProbeCheck, ShopMesh, MerchantProp,
    INTERACT_RANGE_M, DEFAULT_CELL_M, EYE_HEIGHT_M, PLAYER_RADIUS_M,
};

/// M5 merchant shop quality (DFU `shopQuality` parameter).
pub(crate) const SHOP_QUALITY: i32 = 10;
/// Player mercantile (declared synthetic defaults — M5 note).
pub(crate) const PLAYER_MERCANTILE: i32 = 50;
/// Player personality (declared synthetic defaults — M5 note).
pub(crate) const PLAYER_PERSONALITY: i32 = 50;
/// Session starting gold (inventory-lite QW3).
pub(crate) const STARTING_GOLD: i32 = 100;
/// Base value of the "furs" good (declared synthetic — M5 note; the SAME 50
/// used by the probe's B5/B6 checks and by the M5 window sale — window and
/// probe cannot diverge: a single constant).
pub(crate) const PIELES_BASE_VALUE: i32 = 50;

/// Complete PROBE-COMMERCE-001 report (blocks A + B).
#[derive(Debug, Clone)]
pub struct CommerceProbeReport {
    /// Checks executed, in order.
    pub checks: Vec<ProbeCheck>,
    /// Global PASS = all checks ok.
    pub pass: bool,
}

/// M5 merchant catalog (declared synthetic content — M5 note §N; prices come
/// from the DFU formula, not from tables). Shared with the M5 window (same
/// catalog in probe and title bar — 0 divergence).
pub(crate) fn merchant_stock() -> MerchantStock {
    MerchantStock {
        merchant_name: "Vendel".into(),
        shop_quality: SHOP_QUALITY,
        items: vec![
            StockItem { id: "potion".into(), name: "minor potion".into(), base_cost: 80, stock: 5 },
            StockItem { id: "torch".into(), name: "torch".into(), base_cost: 12, stock: 10 },
            StockItem { id: "longsword".into(), name: "long sword".into(), base_cost: 300, stock: 2 },
        ],
    }
}

/// Session starting inventory (gold + 1 sellable good for the sale).
/// Shared with the M5 window (same starting state).
pub(crate) fn starting_inventory() -> InventoryLite {
    InventoryLite {
        gold: STARTING_GOLD,
        goods: vec![("furs".into(), 1)],
    }
}

/// DFU shop catalogue cost (`FormulaHelper.CalculateCost`,
/// `FormulaHelper.cs:1868-1883`): `c = max(c,1)` →
/// `ApplyRegionalPriceAdjustment` (identity without a location,
/// `:2010-2028`) → `c = 2 * (c*(quality-10)/100 + c)` with `int` truncation.
///
/// **First stage of the retail price.** DFU charges
/// `CalculateTradePrice(CalculateCost(item.value, quality), quality, selling)`:
/// `DaggerfallTradeWindow.UpdateCostAndGold` builds the basket with
/// `CalculateCost(item.value, buildingDiscoveryData.quality)`
/// (`DaggerfallTradeWindow.cs:443,462-466`) and `GetTradePrice` applies the
/// bargaining formula on top (`:493-505` → `FormulaHelper.cs:1960-1988`).
/// This step lives in `behavior::durability` because the repair path already
/// ports the same function (`durability.rs:93`); the app composes both stages
/// (no organ calls another).
pub(crate) fn dfu_shop_cost(base_value: i32) -> i32 {
    daggerfall_behavior::durability::calculate_cost(base_value, SHOP_QUALITY)
}

/// DFU BUY price of a catalog item (`selling=false`) with the M5 parameters
/// (shopQuality/mercantile/personality): the **complete** two-stage retail
/// price (`CalculateCost` then `CalculateTradePrice`). Single source of truth:
/// the probe (check B3/B6) and the M5 window (title bar + B key) pay exactly
/// this number — 0 probe↔window divergence.
pub(crate) fn dfu_buy_price(base_cost: i32) -> i32 {
    daggerfall_skills::formula::calculate_trade_price(
        dfu_shop_cost(base_cost),
        SHOP_QUALITY,
        false,
        PLAYER_MERCANTILE,
        PLAYER_PERSONALITY,
    )
}

/// DFU SELL price of a good (`selling=true`) — same two-stage price and
/// parameters; single source of truth probe (B5) ↔ window (V key).
pub(crate) fn dfu_sell_price(base_value: &i32) -> i32 {
    daggerfall_skills::formula::calculate_trade_price(
        dfu_shop_cost(*base_value),
        SHOP_QUALITY,
        true,
        PLAYER_MERCANTILE,
        PLAYER_PERSONALITY,
    )
}

/// Executable core of **PROBE-COMMERCE-001** over an already-built world —
/// the CLI and the tests run the SAME code (Anti-Screen).
///
/// `merchant` is parametric on purpose: the anti-false-positive tests run the
/// same core with the tampered NPC and require `!pass`.
pub fn run_commerce_probe(
    city: &CityMesh,
    layout: &CityLayout,
    shop: &ShopMesh,
    gap: usize,
    merchant: &MerchantProp,
) -> CommerceProbeReport {
    let mut checks: Vec<ProbeCheck> = Vec::with_capacity(13);

    // ── M5 world: M4 world + merchant solid box ─────────────────────────
    let world = bridge::world_mesh(city, shop);
    let world_m5 = bridge::merchant_world(&world, merchant);
    let grid = bridge::CollisionGrid::from_mesh_spec(&world_m5, DEFAULT_CELL_M);
    // Grid WITHOUT merchant: proves its cells were free (does not clobber M4
    // fixtures — 0 touch to inherited props).
    let base_grid = bridge::CollisionGrid::from_mesh_spec(&world, DEFAULT_CELL_M);
    let (street_point, street_name) = bridge::street_spawn(layout, gap);
    let chest = merchant.chest();

    // ── A1: enter the shop (street → lane → door → interior march)
    let eye0 = [street_point[0], street_point[1] + EYE_HEIGHT_M, street_point[2]];
    let r1 = march_axis(&world_m5, &grid, eye0, 2, -1.0, 6.0);
    let r2 = march_axis(&world_m5, &grid, r1.pos, 0, -1.0, 7.0);
    let r3 = march_axis(&world_m5, &grid, r2.pos, 2, 1.0, 13.0);
    let r4 = march_axis(&world_m5, &grid, r3.pos, 2, 1.0, 18.0);
    let r5 = march_axis(&world_m5, &grid, r4.pos, 0, -1.0, 3.0);
    let walk_violations = r1.violations + r2.violations + r3.violations + r4.violations + r5.violations;
    let walk_samples = r1.oracle_samples
        + r2.oracle_samples
        + r3.oracle_samples
        + r4.oracle_samples
        + r5.oracle_samples;
    let stance = r5.pos;
    checks.push(ProbeCheck {
        id: "enter_shop_walk",
        ok: [r1.reached, r2.reached, r3.reached, r4.reached, r5.reached].iter().all(|r| *r)
            && walk_violations == 0
            && shop.contains_interior(stance)
            && grid.is_free(stance, PLAYER_RADIUS_M),
        detail: format!(
            "{street_name} → lane x=7 → z=18 → interior (5 marches of 0.25 m) · \
             targets reached {reached}/5 · violations={walk_violations} (0 req, geometry oracle) · \
             oracle samples={walk_samples} · \
             region=interior={inside} · free={free} · interaction pos=({:.2},{:.2},{:.2}) · \
             steps R1={s1} R2={s2} R3={s3} R4={s4} R5={s5}",
            stance[0], stance[1], stance[2],
            reached = [r1.reached, r2.reached, r3.reached, r4.reached, r5.reached]
                .iter().filter(|r| **r).count(),
            inside = shop.contains_interior(stance),
            free = grid.is_free(stance, PLAYER_RADIUS_M),
            s1 = r1.steps, s2 = r2.steps, s3 = r3.steps, s4 = r4.steps, s5 = r5.steps,
        ),
    });

    // ── A2: the NPC is solid at eye height and inside the interior
    let feet = [merchant.min[0] + 0.1, EYE_HEIGHT_M, merchant.min[2] + 0.1];
    let cells = merchant.cells(&grid);
    let chest_solid = grid.classify_point(chest) == CellStatus::Solid;
    let feet_solid = grid.classify_point(feet) == CellStatus::Solid;
    checks.push(ProbeCheck {
        id: "merchant_solid_in_shop",
        ok: chest_solid && feet_solid && shop.contains_interior(chest) && !cells.is_empty(),
        detail: format!(
            "box min={:?} size={:?} → {} cells {:?} · chest=({:.2},{:.2},{:.2}) solid={chest_solid} \
             · foot y={} solid={feet_solid} · region=interior={}",
            merchant.min, merchant.size, cells.len(), cells,
            chest[0], chest[1], chest[2], feet[1],
            shop.contains_interior(chest),
        ),
    });

    // ── A3: the NPC does NOT overlap the M4 fixtures (base grid = free) ──
    //    Eye layer `y≥1` (where the M4 fixtures compete — AB-06); the
    //    `y=0` row is solid because of the M4 visual floor (slab at `y=0.02`,
    //    voxel of cell 0 — not a fixture, documented in the detail).
    let eye_cells: Vec<[i32; 3]> = cells.iter().copied().filter(|cell| cell[1] >= 1).collect();
    let overlap_free = eye_cells
        .iter()
        .filter(|cell| base_grid.classify_point(base_grid.cell_center(**cell)) == CellStatus::Free)
        .count();
    checks.push(ProbeCheck {
        id: "merchant_no_fixture_overlap",
        ok: overlap_free == eye_cells.len() && !eye_cells.is_empty(),
        detail: format!(
            "M4 grid WITHOUT merchant: {overlap_free}/{} NPC cells in the eye layer (y≥1) \
             free (req: all — the NPC does not step on the M4 counter/shelves/island) · row y=0 \
             solid because of the M4 visual floor (slab y=0.02, not a fixture) · cells={cells:?}",
            eye_cells.len(),
        ),
    });

    // ── A4: valid interaction (LOS + range from the entry position) ──────
    let dir_chest = [chest[0] - stance[0], chest[1] - stance[1], chest[2] - stance[2]];
    let chest_dist = (dir_chest[0] * dir_chest[0]
        + dir_chest[1] * dir_chest[1]
        + dir_chest[2] * dir_chest[2])
        .sqrt();
    let hit_valid = bridge::interact_target(&grid, stance, dir_chest, merchant, INTERACT_RANGE_M);
    checks.push(ProbeCheck {
        id: "interact_valid_merchant",
        ok: hit_valid.is_some() && chest_dist <= INTERACT_RANGE_M,
        detail: format!(
            "from ({:.2},{:.2},{:.2}) → chest at {chest_dist:.2} m (range {INTERACT_RANGE_M} m) · \
             first solid = NPC cell {:?} at {:.2} m · target={hit_valid:?}",
            stance[0], stance[1], stance[2],
            hit_valid.map(|h| h.cell),
            hit_valid.map(|h| h.dist).unwrap_or(0.0),
        ),
    });

    // ── A5: interaction BLOCKED by the wall (outside the door) ──────────
    let wall_eye = [6.5, EYE_HEIGHT_M, 20.5];
    let dir_wall = [chest[0] - wall_eye[0], chest[1] - wall_eye[1], chest[2] - wall_eye[2]];
    let wall_range = (dir_wall[0] * dir_wall[0] + dir_wall[1] * dir_wall[1] + dir_wall[2] * dir_wall[2]).sqrt();
    let raw_wall = grid.raycast(wall_eye, dir_wall, wall_range + 1.0);
    let hit_wall = bridge::interact_target(&grid, wall_eye, dir_wall, merchant, INTERACT_RANGE_M);
    checks.push(ProbeCheck {
        id: "interact_blocked_through_wall",
        ok: hit_wall.is_none() && raw_wall.is_some() && grid.is_free(wall_eye, PLAYER_RADIUS_M),
        detail: format!(
            "from the lane (6.5,1.6,20.5) → chest at {wall_range:.2} m (in range) · raw ray \
             hits {:?} at {:.2} m (east wall x=5) ≠ NPC ⇒ target=None · \
             interaction through the wall REJECTED",
            raw_wall.map(|h| h.cell),
            raw_wall.map(|h| h.dist).unwrap_or(0.0),
        ),
    });

    // ── A6: interaction OUT OF RANGE (in the far lane) ──────────────────
    let far_eye = [7.5, EYE_HEIGHT_M, 17.0];
    let dir_far = [chest[0] - far_eye[0], chest[1] - far_eye[1], chest[2] - far_eye[2]];
    let far_dist = (dir_far[0] * dir_far[0] + dir_far[1] * dir_far[1] + dir_far[2] * dir_far[2]).sqrt();
    let hit_far = bridge::interact_target(&grid, far_eye, dir_far, merchant, INTERACT_RANGE_M);
    checks.push(ProbeCheck {
        id: "interact_out_of_range",
        ok: far_dist > INTERACT_RANGE_M && hit_far.is_none() && grid.is_free(far_eye, PLAYER_RADIUS_M),
        detail: format!(
            "from (7.5,1.6,17.0) → chest at {far_dist:.2} m > range {INTERACT_RANGE_M} m · \
             target={hit_far:?} (None req: out of reach even if the line of sight is clear)"
        ),
    });

    // ── A7: interaction with the WRONG direction (facing the door) ──────
    let hit_away = bridge::interact_target(&grid, stance, [1.0, 0.0, 0.0], merchant, INTERACT_RANGE_M);
    checks.push(ProbeCheck {
        id: "interact_wrong_facing",
        ok: hit_away.is_none() && chest_dist <= INTERACT_RANGE_M,
        detail: format!(
            "same position ({:.2},{:.2},{:.2}) but facing +X (door/street): target={hit_away:?} \
             (None req) · chest still {chest_dist:.2} m away ⇒ the GAZE fails, not the range",
            stance[0], stance[1], stance[2],
        ),
    });

    // ── B1: dialogue opens ONLY on valid interaction (E key) ─────────────
    let mut session = TradeSession::new(merchant_stock(), starting_inventory());
    let mut b1_ok = hit_wall.is_none() && hit_valid.is_some();
    let gate = session.buy("potion", dfu_buy_price(80));
    b1_ok &= gate == Err(TradeError::DialogueClosed);
    if hit_valid.is_some() {
        session.open_dialogue();
    }
    let greeting = session.greeting();
    b1_ok &= session.dialogue_open && !greeting.is_empty() && greeting.contains("Vendel");
    session.close_dialogue();
    let closed_now = !session.dialogue_open;
    // 2nd valid interaction (second E press) → reopens for trading.
    if hit_valid.is_some() {
        session.open_dialogue();
    }
    b1_ok &= closed_now && session.dialogue_open;
    checks.push(ProbeCheck {
        id: "dialogue_only_via_interact",
        ok: b1_ok,
        detail: format!(
            "buy with dialogue closed → {gate:?} (req DialogueClosed) · valid interaction → \
             dialogue={} greeting={greeting:?} · close → {closed_now} · reopen (2nd E) → {}",
            b1_ok && session.dialogue_open,
            session.dialogue_open,
        ),
    });

    // ── B2: buy/sell menu with gold (VE / title evidence) ───────────────
    let menu = session.menu_line();
    checks.push(ProbeCheck {
        id: "menu_lists_buy_and_sell",
        ok: session.dialogue_open
            && menu.contains("buy")
            && menu.contains("sell")
            && menu.contains(format!("gold={STARTING_GOLD}").as_str()),
        detail: format!("menu: {menu} · first stock item={:?} · first good={:?}",
            session.first_stock().map(|i| i.id.as_str()),
            session.first_good().map(|(id, qty)| format!("{id}×{qty}"))),
    });

    // ── B3: BUY — gold deducted + item added (pack invariant) ───────────
    let gold_before = session.inventory.gold;
    let price = dfu_buy_price(80);
    let bought = session.buy("potion", price);
    let gold_after = session.inventory.gold;
    checks.push(ProbeCheck {
        id: "buy_deducts_gold_and_adds_item",
        ok: bought == Ok(90)
            && gold_after == gold_before - price
            && gold_after < gold_before
            && session.inventory.count("potion") == 1,
        detail: format!(
            "DFU price = CalculateTradePrice(CalculateCost(80,q10)=160,q10,buy) = {price} \
             (req 90) · buy → {bought:?} · gold {gold_before} → {gold_after} \
             (Δ={}, req −{price}) · potions 0 → {} (req +1) · shop stock 5 → {}",
            gold_after - gold_before,
            session.inventory.count("potion"),
            session.merchant.item("potion").map(|i| i.stock).unwrap_or(-1),
        ),
    });

    // ── B4: purchase without enough gold REJECTED, state intact ─────────
    let mut poor = TradeSession::new(merchant_stock(), InventoryLite::new(5));
    poor.open_dialogue();
    let poor_gold = poor.inventory.gold;
    let price = dfu_buy_price(80);
    let rejected = poor.buy("potion", price);
    checks.push(ProbeCheck {
        id: "buy_insufficient_gold_rejected",
        ok: rejected == Err(TradeError::NotEnoughGold)
            && poor.inventory.gold == poor_gold
            && poor.inventory.count("potion") == 0,
        detail: format!(
            "gold=5 < price {price} → {rejected:?} (req NotEnoughGold) · gold after attempt={} \
             (intact) · potions={} (0 added) — 0 mutation on Err",
            poor.inventory.gold,
            poor.inventory.count("potion"),
        ),
    });

    // ── B5: SELL — gold credited + good removed ─────────────────────────
    let sell_price = dfu_sell_price(&PIELES_BASE_VALUE);
    let gold_mid = session.inventory.gold;
    let sold = session.sell("furs", sell_price);
    let gold_end = session.inventory.gold;
    let resell = session.sell("furs", sell_price);
    checks.push(ProbeCheck {
        id: "sell_credits_gold_and_removes_good",
        ok: sold == Ok(50)
            && gold_end == gold_mid + sell_price
            && session.inventory.count("furs") == 0
            && resell == Err(TradeError::NotOwned)
            && session.inventory.gold == gold_end,
        detail: format!(
            "DFU sell price = CalculateTradePrice(CalculateCost(50,q10),q10,sell,50,50) = \
             {sell_price} (req 50) · sell → {sold:?} · gold {gold_mid} → {gold_end} \
             (Δ=+{sell_price}) · furs 1 → 0 · resale → {resell:?} (req NotOwned, gold intact {gold_end})",
        ),
    });

    // ── B6: the paid prices are the pure DFU formula (N2 link) ──────────
    // The item-level price is the TWO-stage DFU pipeline:
    // `CalculateCost(value, quality)` (`DaggerfallTradeWindow.cs:443,462-466`)
    // fed into `CalculateTradePrice(cost, quality, selling)` (`:493-505`).
    let potion_shelf_cost = dfu_shop_cost(80);
    let fur_shelf_cost = dfu_shop_cost(PIELES_BASE_VALUE);
    let buy_price = dfu_buy_price(80);
    let sell_again = dfu_sell_price(&PIELES_BASE_VALUE);
    // Same inputs, one stage only: the number the engine charged BEFORE the
    // fix (≈½ of retail). Kept as an explicit anti-regression anchor.
    let buy_without_cost = daggerfall_skills::formula::calculate_trade_price(
        80, SHOP_QUALITY, false, PLAYER_MERCANTILE, PLAYER_PERSONALITY,
    );
    let sell_without_cost = daggerfall_skills::formula::calculate_trade_price(
        PIELES_BASE_VALUE, SHOP_QUALITY, true, PLAYER_MERCANTILE, PLAYER_PERSONALITY,
    );
    let golden_shelf_sell = daggerfall_skills::formula::GOLDEN_VECTORS.iter().any(|v| {
        v.cost == fur_shelf_cost
            && v.shop_quality == SHOP_QUALITY
            && v.selling
            && v.expected == sell_again
    });
    checks.push(ProbeCheck {
        id: "trade_price_is_dfu_formula",
        ok: potion_shelf_cost == 160
            && fur_shelf_cost == 100
            && buy_price == 90
            && sell_again == 50
            && buy_without_cost == 45
            && sell_without_cost == 25
            && buy_price == price
            && sell_again == sell_price
            && golden_shelf_sell,
        detail: format!(
            "CalculateCost(80,q10)=160 (FormulaHelper.cs:1868-1883) → buy={buy_price} \
             (req 90) · CalculateCost(50,q10)={fur_shelf_cost} → sell={sell_again} (req 50) · \
             one-stage control (the pre-fix price) = {buy_without_cost}/{sell_without_cost} \
             (45/25, ≈½ retail) · determinism (2nd call) identical to the prices paid in \
             B3/B5 ✓ · the sell shelf cost is a skills GOLDEN_VECTORS input \
             (cost={fur_shelf_cost}→{sell_again})={golden_shelf_sell} (INV-L99-3: outcome = \
             rules(stats), 0 randomness, 0 state)",
        ),
    });

    let pass = checks.iter().all(|c| c.ok);
    CommerceProbeReport { checks, pass }
}

/// CLI dispatch of PROBE-COMMERCE-001 (Mode A retail, M4 scheme) —
/// block A+B (core shared with the tests) + block C (FORMULA-001).
pub fn run_commerce_001() -> i32 {
    println!("PROBE-COMMERCE-001 - M5 interactive commerce (INV-L99-10, CR-bbd5db82)");
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
                    println!("[FAIL] ARCH3D.BSA read: {err}");
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

    // Intact M4 world (city + synthetic shop) + M5 NPC.
    let layout = CityLayout::new(3);
    let city = bridge::build_city_mesh(&layout, &specs);
    let shop = bridge::build_shop_mesh();
    let merchant = bridge::shop_merchant();
    println!(
        "[OK] 3x3 city ({} triangles) + shop ({} triangles) + merchant NPC \
         (box {:?} {:?} — solid, 0 billboard)",
        city.triangle_count(),
        shop.triangle_count(),
        merchant.min,
        merchant.size,
    );

    println!("-- Block A+B: enter → interact → buy/sell --");
    let report = run_commerce_probe(&city, &layout, &shop, 1, &merchant);
    for check in &report.checks {
        let status = if check.ok { "OK" } else { "FAIL" };
        println!("[{status}] {} - {}", check.id, check.detail);
    }

    println!("-- Block C: PROBE-FORMULA-001 (DFU golden vectors, N2) --");
    let formula = daggerfall_skills::formula::run_formula_probe();
    for check in &formula.checks {
        let status = if check.ok { "OK" } else { "FAIL" };
        println!("[{status}] {} - {}", check.id, check.detail);
    }

    let passed = report.checks.iter().filter(|c| c.ok).count()
        + formula.checks.iter().filter(|c| c.ok).count();
    let total = report.checks.len() + formula.checks.len();
    if report.pass && formula.pass {
        println!(
            "M5 VE: interactive commerce — {passed}/{total} checks · NPC Vendel (solid box) \
             · dialogue+menu [B]/[V] with gold {STARTING_GOLD} → buy potion 90 → gold \
             deducted + item added · sell furs +50 · prices = DFU CalculateCost + \
             CalculateTradePrice (11 golden vectors)"
        );
        println!("VERDICT: PROBE-COMMERCE-001 PASS - enter→interact→buy (gold−item) + DFU formula");
        0
    } else {
        let failed: Vec<&str> = report
            .checks
            .iter()
            .filter(|c| !c.ok)
            .map(|c| c.id)
            .chain(formula.checks.iter().filter(|c| !c.ok).map(|c| c.id))
            .collect();
        println!(
            "VERDICT: PROBE-COMMERCE-001 FAIL - failed checks: {}",
            failed.join(", ")
        );
        1
    }
}

/// CLI dispatch of PROBE-FORMULA-001 (pricing slice, N2) — pure, no world
/// and no retail: runs in any headless config.
pub fn run_formula_001() -> i32 {
    println!("PROBE-FORMULA-001 - INV-L99-3 causal formulas (M5 pricing slice, CR-bbd5db82)");
    let formula = daggerfall_skills::formula::run_formula_probe();
    for check in &formula.checks {
        let status = if check.ok { "OK" } else { "FAIL" };
        println!("[{status}] {} - {}", check.id, check.detail);
    }
    if formula.pass {
        println!(
            "M5 VE: {} DFU golden vectors of CalculateTradePrice + determinism + monotonicity",
            daggerfall_skills::formula::GOLDEN_VECTORS.len()
        );
        println!("VERDICT: PROBE-FORMULA-001 PASS - 100% parity (Mode A only, corpus on disk)");
        0
    } else {
        let failed: Vec<&str> = formula
            .checks
            .iter()
            .filter(|c| !c.ok)
            .map(|c| c.id)
            .collect();
        println!("VERDICT: PROBE-FORMULA-001 FAIL - failed checks: {}", failed.join(", "));
        1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Synthetic 0 I/O world (same as the M4 interior tests): full PASS with
    /// the default NPC.
    #[test]
    fn probe_commerce_001_passes_on_synthetic_world() {
        let (layout, city, shop) = bridge::synthetic_world(3);
        let report = run_commerce_probe(&city, &layout, &shop, 1, &bridge::shop_merchant());
        for check in &report.checks {
            eprintln!(
                "[{}] {} — {}",
                if check.ok { "PASS" } else { "FAIL" },
                check.id,
                check.detail
            );
        }
        eprintln!(
            "VERDICT: PROBE-COMMERCE-001 {} (synthetic, CR-bbd5db82)",
            if report.pass { "PASS" } else { "FAIL" }
        );
        for check in &report.checks {
            assert!(check.ok, "check {} FAIL: {}", check.id, check.detail);
        }
        assert!(report.pass, "PROBE-COMMERCE-001 FAIL: {:?}", report.checks);
        assert_eq!(report.checks.len(), 13, "13 stable A+B checks");
    }

    /// Anti-false-positive: the same core with the NPC **outside the interior**
    /// (tamper) STOPS passing — the probe does not PASS by construction.
    #[test]
    fn probe_commerce_001_fails_without_merchant_on_spot() {
        let (layout, city, shop) = bridge::synthetic_world(3);
        let tampered = MerchantProp {
            min: [6.5, 0.0, 20.0],
            size: [1.0, 1.9, 1.0],
        };
        let report = run_commerce_probe(&city, &layout, &shop, 1, &tampered);
        assert!(!report.pass, "tampered NPC CANNOT give PASS");
        let failed: Vec<&str> = report
            .checks
            .iter()
            .filter(|c| !c.ok)
            .map(|c| c.id)
            .collect();
        assert!(
            failed.contains(&"merchant_solid_in_shop"),
            "the check that must break is merchant_solid_in_shop (NPC outside the interior); failed: {failed:?}"
        );
        eprintln!("anti-false-positive: failed checks with tampered NPC = {failed:?}");
    }

    /// M5 retail price is the **two-stage** DFU pipeline, in the source's order:
    /// `CalculateCost(item.value, quality)` (`FormulaHelper.cs:1868-1883`, the
    /// basket stage of `DaggerfallTradeWindow.cs:443,462-466`) and then
    /// `CalculateTradePrice(cost, quality, selling)` (`FormulaHelper.cs:1960-1988`,
    /// `DaggerfallTradeWindow.cs:493-505`).
    ///
    /// Each row pins the intermediate **and** the final number, so a regression
    /// to the one-stage price (what the engine charged before the fix: ≈½ of
    /// retail) fails on values, not on a self-comparison.
    #[test]
    fn shop_prices_are_the_two_stage_dfu_pipeline() {
        // (id, base value, CalculateCost(base, 10), final retail price, one-stage price)
        // CalculateCost(c,10) = 2*(c*(10-10)/100 + c) = 2c (FormulaHelper.cs:1868-1883).
        // Buy  = ((192*dm>>8) + (dp>>8)) * c >> 8 with dm=144, dp=9216 ⇒ 144*c>>8.
        // Sell = ((179*dm>>8) + (51*dp>>8)) * c >> 8 with dm=dp=144 ⇒ 128*c>>8.
        let buy_rows = [("potion", 80, 160, 90, 45), ("torch", 12, 24, 13, 6), ("longsword", 300, 600, 337, 168)];
        for (id, base, shelf, retail, one_stage) in buy_rows {
            assert_eq!(dfu_shop_cost(base), shelf, "{id}: CalculateCost stage");
            assert_eq!(dfu_buy_price(base), retail, "{id}: CalculateTradePrice stage");
            assert_eq!(
                daggerfall_skills::formula::calculate_trade_price(
                    base, SHOP_QUALITY, false, PLAYER_MERCANTILE, PLAYER_PERSONALITY
                ),
                one_stage,
                "{id}: the one-stage control is the pre-fix price"
            );
            assert_ne!(dfu_buy_price(base), one_stage, "{id}: the two stages must differ");
        }
        // The sale uses the same two stages with `selling = true`.
        assert_eq!(dfu_shop_cost(PIELES_BASE_VALUE), 100);
        assert_eq!(dfu_sell_price(&PIELES_BASE_VALUE), 50);
        assert_eq!(
            daggerfall_skills::formula::calculate_trade_price(
                PIELES_BASE_VALUE, SHOP_QUALITY, true, PLAYER_MERCANTILE, PLAYER_PERSONALITY
            ),
            25,
            "one-stage sale = the pre-fix price"
        );
        // The probe catalogue and the window share these helpers: every catalog
        // item is priced through the same two stages (no second convention).
        for item in merchant_stock().items {
            assert!(
                dfu_shop_cost(item.base_cost) >= item.base_cost,
                "{}: the shelf stage never lowers the base value",
                item.id
            );
            assert_ne!(
                dfu_buy_price(item.base_cost),
                daggerfall_skills::formula::calculate_trade_price(
                    item.base_cost, SHOP_QUALITY, false, PLAYER_MERCANTILE, PLAYER_PERSONALITY
                ),
                "{}: the catalogue price is not the one-stage price",
                item.id
            );
        }
    }
}
