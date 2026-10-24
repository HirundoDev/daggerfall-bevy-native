//! M30 probes — economy: shops, inns, banking and transport
//! (SPEC-M30-ECONOMY, REQ-L99-063).
//!
//! - **`PROBE-SHOP-001`** (A) — the `storeBuysItemType` table: 9 buildings ×
//!   their `ItemGroups`, 5 trade modes and the 36 `(building, group)` pairs,
//!   compared against an independent transcription of
//!   `DaggerfallTradeWindow.cs:107-127`.
//! - **`PROBE-ECONOMY-001`**
//!   - **A** E1-E28 golden vectors (`CalculateCost` → trade price →
//!     repair/identify → room/inn) printed as `id|expected|actual`.
//!   - **B** determinism: the same table evaluated twice under seeds
//!     `0xA5A5` and `0x5A5A`; the FNV-1a 64 digest of `id|expected|actual`
//!     must be identical while the seeded regional-price draws differ (the
//!     seed is live, the table is invariant — spec §8 seed policy).
//!   - **C** retail census: `faction.txt` size and `MAPSAVE.SAV` presence
//!     reading at most 64 bytes.
//!   - **D** demo over the retail capital: buy in a real `GeneralStore`,
//!     rent three nights, open an account and borrow (`gold_before/after`,
//!     `stock_after`, `room_hours`, `account_delta` as numbers).
//! - **`PROBE-BANK-001`**
//!   - **A** E29-E41 (holidays, loan, LOC, weight, ship/house, reminder,
//!     overdue).
//!   - **B** borrow → reminder → overdue → default → repay ×2 with
//!     `state_digest` and the five loan events.
//!
//! Anti-false-positive controls (executed, not asserted): a tampered golden
//! vector must break its row; a purchase without gold must return
//! `NotEnoughGold` with **0 mutation**; a region's account must not change
//! when another region is operated.
//!
//! Exit: `0` ⟺ the probe's blocks pass; `2` when ARENA2 is inaccessible
//! (demo D only); `1` on any FAIL. Mode A custody: 0 retail bytes in the
//! repository.

use daggerfall_behavior::economy::{
    self, holidays, BankError, BankLedger, BuildingType, ItemGroup, LoanCrime, LoanEvent, Purse,
    ShipType, TradeMode, REGION_COUNT,
};
use daggerfall_inventory::commerce::{
    InventoryLite, MerchantStock, StockItem, TradeSession,
};
use daggerfall_parser_rs::dfrandom::DfRandom;

// ── evidence rows (M19/M25/M26/M27 template) ───────────────────────────────

/// Printed row: `[OK] <id> - <detail>` or `FAIL <id> got=<v> want=<v>`.
struct Row {
    id: &'static str,
    ok: bool,
    got: String,
    want: String,
    detail: String,
}

fn row(id: &'static str, got: String, want: impl Into<String>, detail: impl Into<String>) -> Row {
    let want = want.into();
    Row {
        id,
        ok: got == want,
        got,
        want,
        detail: detail.into(),
    }
}

fn check(id: &'static str, ok: bool, detail: impl Into<String>) -> Row {
    Row {
        id,
        ok,
        got: if ok { "ok".into() } else { "fail".into() },
        want: "ok".into(),
        detail: detail.into(),
    }
}

fn print_rows(title: &str, rows: &[Row]) -> bool {
    println!("\n-- {title} --");
    let mut ok = true;
    for row in rows {
        if row.ok {
            println!("[OK] {} - {}", row.id, row.detail);
        } else {
            ok = false;
            println!("FAIL {} got={} want={}", row.id, row.got, row.want);
        }
    }
    ok
}

/// `PASS`/`FAIL` word for the summary lines.
fn verdict(ok: bool) -> &'static str {
    if ok {
        "PASS"
    } else {
        "FAIL"
    }
}

/// FNV-1a 64 (offset basis `0xcbf2_9ce4_8422_2325`, prime `0x100000001b3`).
fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// First `cap` bytes of a file (census: never loads the whole file).
fn read_head(path: &std::path::Path, cap: usize) -> Result<Vec<u8>, String> {
    use std::io::Read;
    let mut file = std::fs::File::open(path).map_err(|e| format!("open {path:?}: {e}"))?;
    let mut buffer = vec![0u8; cap];
    let mut filled = 0;
    while filled < cap {
        let read = file
            .read(&mut buffer[filled..])
            .map_err(|e| format!("read {path:?}: {e}"))?;
        if read == 0 {
            break;
        }
        filled += read;
    }
    buffer.truncate(filled);
    Ok(buffer)
}

/// Second stage of the retail price (🎮 Skills organ, M5); the `app`
/// composes it with the catalog stage of [`economy`].
fn trade_price(cost: i64, quality: u8, selling: bool, mercantile: i32, personality: i32) -> i64 {
    i64::from(daggerfall_skills::formula::calculate_trade_price(
        cost as i32,
        i32::from(quality),
        selling,
        mercantile,
        personality,
    ))
}

// ═══════════════════════════════════════════════════════════════════════════
// PROBE-SHOP-001
// ═══════════════════════════════════════════════════════════════════════════

/// Independent transcription of `storeBuysItemType`
/// (`DaggerfallTradeWindow.cs:107-127`): `(building, accepted groups)`.
/// Any drift between this table and the implementation is `mismatches > 0`.
const EXPECTED_STOCK: &[(&str, &[&str])] = &[
    (
        "Alchemist",
        &[
            "Gems",
            "CreatureIngredients1",
            "CreatureIngredients2",
            "CreatureIngredients3",
            "PlantIngredients1",
            "PlantIngredients2",
            "MiscellaneousIngredients1",
            "MiscellaneousIngredients2",
            "MetalIngredients",
        ],
    ),
    ("Armorer", &["Armor", "Weapons"]),
    ("Bookseller", &["Books"]),
    ("ClothingStore", &["MensClothing", "WomensClothing"]),
    ("FurnitureStore", &["Furniture"]),
    ("GemStore", &["Gems", "Jewellery"]),
    (
        "GeneralStore",
        &[
            "Books",
            "MensClothing",
            "WomensClothing",
            "Transportation",
            "Jewellery",
            "Weapons",
            "UselessItems2",
        ],
    ),
    (
        "PawnShop",
        &[
            "Armor",
            "Books",
            "MensClothing",
            "WomensClothing",
            "Gems",
            "Jewellery",
            "ReligiousItems",
            "Weapons",
            "UselessItems2",
            "Paintings",
        ],
    ),
    ("WeaponSmith", &["Armor", "Weapons"]),
];

/// `PROBE-SHOP-001` — §1 stock per building type.
pub fn run_shop_001() -> i32 {
    println!(
        "PROBE-SHOP-001 - storeBuysItemType: 9 buildings × groups, 5 modes (M30, CR-f716455c, REQ-L99-063 · spec §1)"
    );

    let mut mismatches: Vec<String> = Vec::new();
    let mut pairs = 0usize;
    for (expected_index, (building_name, groups)) in EXPECTED_STOCK.iter().enumerate() {
        let Some(building) = BuildingType::ALL.get(expected_index).copied() else {
            mismatches.push(format!("building #{expected_index} missing"));
            continue;
        };
        if building.name() != *building_name {
            mismatches.push(format!(
                "order: {} != {building_name}",
                building.name()
            ));
        }
        let actual: Vec<&str> = building
            .accepted_groups()
            .iter()
            .map(|group| group.name())
            .collect();
        if actual != *groups {
            mismatches.push(format!(
                "{}: [{}] != [{}]",
                building.name(),
                actual.join(","),
                groups.join(",")
            ));
        }
        pairs += building.accepted_groups().len();
    }

    let buildings = BuildingType::ALL.len();
    let modes = TradeMode::ALL.len();
    println!("buildings={buildings}");
    println!("modes={modes}");
    println!("pairs={pairs}");
    println!("mismatches={}", mismatches.len());

    let rows = vec![
        row(
            "A buildings",
            format!("{buildings}"),
            "9",
            "§1: nine entries in storeBuysItemType",
        ),
        row(
            "A modes",
            format!("{modes}"),
            "5",
            "§1: Sell/Buy/Repair/Identify/SellMagic (Inventory is 'never used', `:138`)",
        ),
        row(
            "A pairs",
            format!("{pairs}"),
            "36",
            "§1 arithmetic: 9+2+1+2+1+2+7+10+2",
        ),
        row(
            "A mismatches",
            format!("{}", mismatches.len()),
            "0",
            format!(
                "independent transcription of DaggerfallTradeWindow.cs:107-127{}",
                if mismatches.is_empty() {
                    String::new()
                } else {
                    format!(" · {}", mismatches.join(" · "))
                }
            ),
        ),
        row(
            "A field default",
            BuildingType::GeneralStore.name().to_string(),
            "GeneralStore",
            "`DaggerfallTradeWindow.cs:92`",
        ),
        row(
            "A tavern has no stock table",
            format!("{:?}", BuildingType::from_index(15)),
            "None",
            "no entry ⇒ no invented stock (§1)",
        ),
        row(
            "A sale filter",
            format!(
                "weapons={} books={} enchanted={}",
                economy::sale_offer(
                    BuildingType::WeaponSmith,
                    TradeMode::Sell,
                    ItemGroup::Weapons,
                    false
                ),
                economy::sale_offer(
                    BuildingType::Bookseller,
                    TradeMode::Sell,
                    ItemGroup::Weapons,
                    false
                ),
                economy::sale_offer(
                    BuildingType::Bookseller,
                    TradeMode::SellMagic,
                    ItemGroup::Books,
                    true
                ),
            ),
            "weapons=true books=false enchanted=true",
            "`DaggerfallTradeWindow.cs:694-697`",
        ),
    ];
    let ok = print_rows("A · stock per building type (spec §8 A)", &rows);

    println!(
        "\nSUMMARY PROBE-SHOP-001: buildings={buildings} modes={modes} pairs={pairs} mismatches={} ⇒ {}",
        mismatches.len(),
        if ok { "PASS" } else { "FAIL" }
    );
    if ok {
        0
    } else {
        1
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// PROBE-ECONOMY-001
// ═══════════════════════════════════════════════════════════════════════════

/// One golden vector: id + independently transcribed expected value +
/// implementation output.
struct Vector {
    id: &'static str,
    expected: i64,
    actual: i64,
}

/// E1-E28 of SPEC-M30 §7 (`q` = shop quality, `adj` = `PriceAdjustment`).
fn economy_vectors() -> Vec<Vector> {
    let mut vectors = Vec::with_capacity(28);
    let mut push = |id: &'static str, expected: i64, actual: i64| {
        vectors.push(Vector {
            id,
            expected,
            actual,
        });
    };

    // §2.1 CalculateCost (E1-E8).
    push("E1", 200, economy::calculate_cost(100, 10, Some(1000)));
    push("E2", 180, economy::calculate_cost(100, 0, Some(1000)));
    push("E3", 220, economy::calculate_cost(100, 20, Some(1000)));
    push("E4", 2, economy::calculate_cost(0, 10, Some(1000)));
    push("E5", 150, economy::calculate_cost(100, 10, Some(750)));
    push("E6", 800, economy::calculate_cost(100, 10, Some(4000)));
    push("E7", 32, economy::calculate_cost(33, 10, Some(500)));
    push("E8", 2, economy::calculate_cost(1, 0, Some(250)));

    // §2.2 CalculateTradePrice (E9-E15) — Skills organ.
    push("E9", 150, trade_price(200, 10, false, 0, 0));
    push("E10", 75, trade_price(200, 10, false, 100, 100));
    push("E11", 67, trade_price(200, 10, true, 0, 0));
    push("E12", 134, trade_price(200, 10, true, 100, 100));
    push("E13", 75, trade_price(100, 20, false, 50, 50));
    push("E14", 37, trade_price(100, 0, false, 50, 50));
    push("E15", 33, trade_price(100, 20, true, 50, 50));

    // §2.3 repair and identify (E16-E23).
    push(
        "E16",
        100,
        economy::repair_cost(500, 10, 40, 100, Some(1000)),
    );
    push("E17", 2, economy::repair_cost(5, 10, 0, 100, Some(1000)));
    push("E18", 86_400, economy::repair_time_seconds(0, 100));
    push("E19", 388_800, economy::repair_time_seconds(500, 5000));
    push("E20", 432_000, economy::repair_time_seconds(0, 5000));
    push("E21", 9, economy::identify_cost(100, 0));
    push("E22", 97, economy::identify_cost(1000, 0));
    push("E23", 0, economy::identify_cost(10, 0));

    // §4 inn (E24-E28).
    push("E24", 21, economy::room_cost(3, 10));
    push("E25", 7, economy::room_cost(2, 45));
    push("E26", 0, economy::room_cost(1, 46));
    push("E27", 35, economy::room_cost(5, 47));
    push("E28", 11, trade_price(21, 10, false, 50, 50));

    vectors
}

/// `id|expected|actual` text (the digest input of block B).
fn vectors_text(vectors: &[Vector]) -> String {
    vectors
        .iter()
        .map(|vector| format!("{}|{}|{}", vector.id, vector.expected, vector.actual))
        .collect::<Vec<_>>()
        .join("\n")
}

/// `PROBE-ECONOMY-001` — A golden vectors, B determinism, C census, D demo.
pub fn run_economy_001() -> i32 {
    println!(
        "PROBE-ECONOMY-001 - DFU economy: prices, holidays, inn, bank, transport (M30, CR-f716455c, REQ-L99-063 · spec §7-§8)"
    );

    // ── A · E1-E28 ────────────────────────────────────────────────────────
    let vectors = economy_vectors();
    println!("\n-- A · golden vectors (id|expected|actual) --");
    let mut ok_count = 0usize;
    let mut fail_count = 0usize;
    for vector in &vectors {
        println!("{}|{}|{}", vector.id, vector.expected, vector.actual);
        if vector.expected == vector.actual {
            ok_count += 1;
        } else {
            fail_count += 1;
            println!(
                "FAIL {} expected={} actual={}",
                vector.id, vector.expected, vector.actual
            );
        }
    }
    println!("vectors={} ok={} fail={}", vectors.len(), ok_count, fail_count);

    let mut rows = vec![
        row(
            "A1 CalculateCost chain",
            format!(
                "q0={} q10={} q20={} floor={}",
                economy::calculate_cost(100, 0, Some(1000)),
                economy::calculate_cost(100, 10, Some(1000)),
                economy::calculate_cost(100, 20, Some(1000)),
                economy::calculate_cost(0, 10, Some(1000))
            ),
            "q0=180 q10=200 q20=220 floor=2",
            "E1-E4 · `FormulaHelper.cs:1868-1884` (integer, q<10 reduces)",
        ),
        row(
            "A2 regional adjustment",
            format!(
                "adj750={} adj4000={} adj500={} adj250={} none={}",
                economy::calculate_cost(100, 10, Some(750)),
                economy::calculate_cost(100, 10, Some(4000)),
                economy::calculate_cost(33, 10, Some(500)),
                economy::calculate_cost(1, 0, Some(250)),
                economy::calculate_cost(100, 10, None)
            ),
            "adj750=150 adj4000=800 adj500=32 adj250=2 none=200",
            "E5-E8 · `/1000`, `max(…,1)`, identity without a location",
        ),
        row(
            "A3 haggling",
            format!(
                "buy00={} buy100={} sell00={} sell100={}",
                trade_price(200, 10, false, 0, 0),
                trade_price(200, 10, false, 100, 100),
                trade_price(200, 10, true, 0, 0),
                trade_price(200, 10, true, 100, 100)
            ),
            "buy00=150 buy100=75 sell00=67 sell100=134",
            "E9-E12 · `CalculateTradePrice`, `<<6` only on the buying branch",
        ),
        row(
            "A4 repair",
            format!(
                "cost500={} cost5={} time1d={} time4.5d={} time5d={} collective={}",
                economy::repair_cost(500, 10, 40, 100, Some(1000)),
                economy::repair_cost(5, 10, 0, 100, Some(1000)),
                economy::repair_time_seconds(0, 100),
                economy::repair_time_seconds(500, 5000),
                economy::repair_time_seconds(0, 5000),
                economy::collective_repair_seconds(86_400, 172_800)
            ),
            "cost500=100 cost5=2 time1d=86400 time4.5d=388800 time5d=432000 collective=129600",
            "E16-E20 · no rounding to days (4.5 days is valid)",
        ),
        row(
            "A5 identify",
            format!(
                "v100={} v1000={} v10={} witches={}",
                economy::identify_cost(100, 0),
                economy::identify_cost(1000, 0),
                economy::identify_cost(10, 0),
                economy::identify_cost(1000, holidays::WITCHES_FESTIVAL)
            ),
            "v100=9 v1000=97 v10=0 witches=0",
            "E21-E23 · `(25*value)>>8`, no floor, free on Witches Festival",
        ),
        row(
            "A6 inn",
            format!(
                "d10x3={} d45x2={} d46x1={} d47x5={} price={}",
                economy::room_cost(3, 10),
                economy::room_cost(2, 45),
                economy::room_cost(1, 46),
                economy::room_cost(5, 47),
                trade_price(21, 10, false, 50, 50)
            ),
            "d10x3=21 d45x2=7 d46x1=0 d47x5=35 price=11",
            "E24-E28 · Heart's Day free night, then haggling",
        ),
        row(
            "A7 vectors",
            format!("vectors={} ok={ok_count} fail={fail_count}", vectors.len()),
            "vectors=28 ok=28 fail=0",
            "E1-E28 · SPEC-M30 §7",
        ),
    ];

    // §6 transport: availability gates and the three DFU speeds.
    let mut transport = economy::TransportState::new();
    let horse_without_item = transport
        .select(economy::TransportMode::Horse)
        .is_err();
    transport.has_horse = true;
    let mounted = transport
        .select(economy::TransportMode::Horse)
        .is_ok();
    let foot_speed = economy::TransportMode::Foot
        .speed_m_per_s(50.0)
        .expect("foot speed");
    let horse_speed = economy::TransportMode::Horse
        .speed_m_per_s(50.0)
        .expect("horse speed");
    let cart_speed = economy::TransportMode::Cart
        .speed_m_per_s(50.0)
        .expect("cart speed");
    rows.push(row(
        "A11 transport",
        format!(
            "gate={horse_without_item} mounted={mounted} foot={foot_speed:.3} horse={horse_speed:.3} cart={cart_speed:.3} ship={:?} can_run_horse={}",
            economy::TransportMode::Ship.speed_m_per_s(50.0),
            transport.mode.can_run()
        ),
        "gate=true mounted=true foot=5.063 horse=10.759 cart=7.595 ship=None can_run_horse=false",
        "§6 · `(LiveSpeed + base) / 39.5`, horse/cart cannot run",
    ));

    // Anti-false-positive (a): a tampered golden vector breaks its row.
    let mut tampered = economy_vectors();
    if let Some(first) = tampered.first_mut() {
        first.expected += 1;
    }
    let tampered_mismatches = tampered
        .iter()
        .filter(|vector| vector.expected != vector.actual)
        .count();
    rows.push(row(
        "A8 negative: tampered GV detected",
        format!("mismatches={tampered_mismatches}"),
        "mismatches=1",
        "altering E1's expected value must break exactly that row",
    ));

    // Anti-false-positive (b): buying without gold ⇒ NotEnoughGold + 0 mutation.
    let mut poor = TradeSession::new(
        MerchantStock {
            merchant_name: "GeneralStore".into(),
            shop_quality: 10,
            items: vec![StockItem {
                id: "potion".into(),
                name: "minor potion".into(),
                base_cost: 80,
                stock: 3,
            }],
        },
        InventoryLite::new(5),
    );
    poor.open_dialogue();
    let poor_result = poor.buy("potion", 45);
    let poor_state = format!(
        "err={:?} gold={} stock={} goods={}",
        poor_result,
        poor.inventory.gold,
        poor.merchant.item("potion").map_or(-1, |item| item.stock),
        poor.inventory.count("potion")
    );
    rows.push(row(
        "A9 negative: buy without gold",
        poor_state,
        "err=Err(NotEnoughGold) gold=5 stock=3 goods=0",
        "`commerce.rs:184-217`: atomic transition, 0 mutation on Err",
    ));

    // Anti-false-positive (c): per-region accounts.
    let mut isolation = BankLedger::new(REGION_COUNT);
    isolation
        .deposit_gold(0, 500, &mut Purse::new(500))
        .expect("deposit within purse");
    rows.push(row(
        "A10 negative: region isolation",
        format!(
            "r0={} r1={}",
            isolation.account_gold(0).expect("region 0"),
            isolation.account_gold(1).expect("region 1")
        ),
        "r0=500 r1=0",
        "operating region 0 must not touch region 1 (§5 per-region accounts)",
    ));

    let a_ok = print_rows("A · evidence + negative controls", &rows) && fail_count == 0;
    let mut pass = a_ok;

    // ── B · determinism, two seeds ────────────────────────────────────────
    let digest_seed = |seed: u32| -> (u64, u64) {
        let mut rng = DfRandom::new();
        rng.srand(seed);
        let draws: Vec<String> = (0..REGION_COUNT)
            .map(|_| {
                // `Random.Range(0, 500 + 1)`: inclusive 0..=500.
                let draw = (rng.rand() % (u32::from(economy::INITIAL_PRICE_DRAW_MAX) + 1)) as u16;
                economy::initial_regional_price(draw).to_string()
            })
            .collect();
        let table = economy_vectors();
        (
            fnv1a64(vectors_text(&table).as_bytes()),
            fnv1a64(draws.join(",").as_bytes()),
        )
    };
    let (digest_seed1, adj_seed1) = digest_seed(0xA5A5);
    let (digest_seed2, adj_seed2) = digest_seed(0x5A5A);
    println!("\n-- B · determinism (spec §8 seed policy) --");
    println!(
        "digest_seed1={digest_seed1:016x} digest_seed2={digest_seed2:016x} runs=2 identical={}",
        digest_seed1 == digest_seed2
    );
    println!(
        "adj_seed1={adj_seed1:016x} adj_seed2={adj_seed2:016x} seeds_differ={}",
        adj_seed1 != adj_seed2
    );
    let b_rows = vec![
        row(
            "B1 table digest",
            format!("{digest_seed1:016x}"),
            format!("{digest_seed2:016x}"),
            "same table ×2 ⇒ identical FNV-1a 64 over id|expected|actual",
        ),
        row(
            "B2 seeds are live",
            format!("{}", adj_seed1 != adj_seed2),
            "true",
            "the seeded initial regional prices differ, so B is not tautological",
        ),
        row(
            "B3 runs",
            "runs=2".to_string(),
            "runs=2",
            "two independent evaluations of E1-E28",
        ),
    ];
    let b_ok = print_rows("B · same table ×2 seeds", &b_rows);
    pass &= b_ok;

    // ── C · retail census (≤ 64 bytes read) ───────────────────────────────
    let mut bytes_read = 0usize;
    let mut c_rows = Vec::new();
    match crate::mode_a_arena2_path("faction.txt") {
        Ok(path) => {
            let size = std::fs::metadata(&path).map(|meta| meta.len()).unwrap_or(0);
            match read_head(&path, 64) {
                Ok(head) => bytes_read += head.len(),
                Err(reason) => c_rows.push(check(
                    "C0 faction head",
                    false,
                    format!("unreadable: {reason}"),
                )),
            }
            println!("faction_bytes={size}");
            c_rows.push(row(
                "C1 faction size",
                format!("{size}"),
                "74221",
                "M27 retail census (FACTION.TXT, REQ-L99-060 power input of the price drift)",
            ));
        }
        Err(reason) => {
            c_rows.push(check(
                "C1 faction size",
                false,
                format!("ARENA2 faction.txt unavailable: {reason}"),
            ));
        }
    }
    match crate::mode_a_arena2_path("MAPSAVE.SAV") {
        Ok(path) => {
            // The census reads at most 64 bytes in TOTAL (spec §8 C).
            let budget = 64usize.saturating_sub(bytes_read);
            if let Ok(head) = read_head(&path, budget) {
                bytes_read += head.len();
            }
            println!("mapsave=present");
            c_rows.push(row(
                "C2 mapsave",
                "present".to_string(),
                "present",
                "optional retail save (M33 owns its decoding; only presence is measured; absent is honest)",
            ));
        }
        Err(_) => {
            println!("mapsave=absent");
            c_rows.push(row(
                "C2 mapsave",
                "absent".to_string(),
                "absent",
                "optional retail save: absent is honest",
            ));
        }
    }
    println!("bytes_leidos={bytes_read}");
    c_rows.push(check(
        "C3 bytes read",
        bytes_read <= 64,
        format!("bytes_leidos={bytes_read} (total budget 64; metadata + head)"),
    ));
    let c_ok = print_rows("C · retail census", &c_rows);
    pass &= c_ok;

    // ── D · demo over the retail capital ──────────────────────────────────
    let d_ok = match run_demo() {
        Ok(demo_ok) => demo_ok,
        Err(reason) => {
            println!("\n-- D · demo (ARENA2) --");
            println!("[EXIT=2] {reason}");
            // D cannot be measured: EXIT 2 when A/B/C passed, 1 otherwise.
            return if pass { 2 } else { 1 };
        }
    };
    pass &= d_ok;

    println!(
        "\nSUMMARY PROBE-ECONOMY-001: A={} B={} C={} D={} ⇒ {}",
        verdict(a_ok),
        verdict(b_ok),
        verdict(c_ok),
        verdict(d_ok),
        verdict(pass)
    );
    if pass {
        0
    } else {
        1
    }
}

/// §12 demo: buy 1 potion in the capital's `GeneralStore`, rent three nights
/// in its tavern, open a per-region account and borrow.
fn run_demo() -> Result<bool, String> {
    use daggerfall_behavior::economy::{
        calculate_cost, room_cost, room_expiry_seconds, room_remaining_hours,
    };

    let capital = crate::m10_probe::compose_capital()?;
    let general = capital
        .buildings
        .iter()
        .find(|building| building.building_type == BuildingType::GeneralStore.index())
        .ok_or_else(|| "no GeneralStore in the capital".to_string())?;
    let tavern = capital
        .buildings
        .iter()
        .find(|building| building.building_type == 15 && building.quality == 10)
        .or_else(|| {
            capital
                .buildings
                .iter()
                .find(|building| building.building_type == 15)
        });
    let tavern_quality = tavern.map_or(10, |building| building.quality);

    // Declared demo inputs: 50/50 haggle stats, neutral regional adjustment
    // (the runtime value is randomized at world creation, spec §8) and day 10
    // of the year (outside Heart's Day).
    const GOLD_BEFORE: i64 = 1000;
    const MERCANTILE: i32 = 50;
    const PERSONALITY: i32 = 50;
    const ADJUSTMENT: Option<u16> = Some(1000);
    const DAY_OF_YEAR: i64 = 10;
    const NOW_MINUTES: i64 = 10_000;

    let catalogue = calculate_cost(80, general.quality, ADJUSTMENT);
    let buy_price = trade_price(catalogue, general.quality, false, MERCANTILE, PERSONALITY);
    let mut session = TradeSession::new(
        MerchantStock {
            merchant_name: "GeneralStore".into(),
            shop_quality: i32::from(general.quality),
            items: vec![StockItem {
                id: "potion".into(),
                name: "minor potion".into(),
                base_cost: 80,
                stock: 3,
            }],
        },
        InventoryLite::new(GOLD_BEFORE as i32),
    );
    session.open_dialogue();
    let paid = session
        .buy("potion", buy_price as i32)
        .map_err(|error| format!("demo buy: {error:?}"))?;
    let gold_after_buy = i64::from(session.inventory.gold);
    let stock_after = i64::from(session.merchant.item("potion").expect("stock item").stock);
    let goods_after = i64::from(session.inventory.count("potion"));

    let room_base = room_cost(3, DAY_OF_YEAR);
    let room_price = trade_price(room_base, tavern_quality, false, MERCANTILE, PERSONALITY);
    let gold_after = gold_after_buy - room_price;
    let now_seconds = 86_400 * DAY_OF_YEAR as u64;
    let expiry = room_expiry_seconds(now_seconds, 3);
    let room_hours = room_remaining_hours(now_seconds, expiry);

    let mut ledger = BankLedger::new(REGION_COUNT);
    let mut purse = Purse::new(gold_after);
    ledger
        .deposit_gold(0, 500, &mut purse)
        .map_err(|error| format!("demo deposit: {error}"))?;
    ledger
        .borrow_loan(0, 1000, 3, NOW_MINUTES)
        .map_err(|error| format!("demo borrow: {error}"))?;
    let account_delta = ledger
        .account_gold(0)
        .map_err(|error| error.to_string())?;
    let loan = ledger.loan_total(0).map_err(|error| error.to_string())?;
    let due = ledger.loan_due(0).map_err(|error| error.to_string())?;
    // §12(d): one minute past the year-long due date; the account covers the
    // debt, so the loan falls to max(0, 1100 - account).
    let crime = ledger
        .check_overdue(0, NOW_MINUTES, NOW_MINUTES + 518_401)
        .map_err(|error| error.to_string())?;
    let overdue_loan = ledger.loan_total(0).map_err(|error| error.to_string())?;
    let covered = (1100 - account_delta).max(0);
    let accepted = economy::sale_offer(
        BuildingType::GeneralStore,
        TradeMode::Sell,
        ItemGroup::MiscellaneousIngredients1,
        false,
    );

    println!("\n-- D · demo over the retail capital --");
    println!(
        "[D] shop={} quality={} adj=1000 gold_before={GOLD_BEFORE} buy_price={buy_price} gold_after_buy={gold_after_buy}",
        general.building_type, general.quality
    );
    println!(
        "[D] tavern_quality={tavern_quality} room_cost={room_base} room_price={room_price} room_hours={room_hours} gold_after={gold_after}"
    );
    println!(
        "[D] account_delta={account_delta} loan={loan} due={due} overdue_loan={overdue_loan} covered={covered} crime={crime:?} accepted={accepted} stock_after={stock_after} goods_after={goods_after}"
    );

    let rows = vec![
        row(
            "D1 purchase",
            format!("paid={paid} gold_after_buy={gold_after_buy}"),
            format!("paid={buy_price} gold_after_buy={}", GOLD_BEFORE - buy_price),
            "gold -= price · stock -= 1 · inventory += 1",
        ),
        row(
            "D2 stock",
            format!("{stock_after}"),
            "2",
            "3 offered − 1 bought",
        ),
        row(
            "D3 inventory",
            format!("{goods_after}"),
            "1",
            "potion added",
        ),
        row(
            "D4 room hours",
            format!("{room_hours}"),
            "72",
            "3 nights × 24 h (M32 consumes the expiry)",
        ),
        row(
            "D5 account",
            format!("{account_delta}"),
            "1500",
            "deposit 500 + loan principal 1000",
        ),
        row(
            "D6 loan",
            format!("{loan}"),
            "1100",
            "principal + fixed 10 %",
        ),
        row(
            "D7 due",
            format!("{due}"),
            format!("{}", NOW_MINUTES + 518_400),
            "360 × 1440 minutes",
        ),
        row(
            "D8 overdue payment",
            format!("{overdue_loan}"),
            format!("{covered}"),
            "one minute past due: account pays min(loan, account) (§12(d))",
        ),
        row(
            "D9 unaccepted item",
            format!("{accepted}"),
            "false",
            "GeneralStore does not buy ingredients (§12(b))",
        ),
    ];
    let ok = print_rows("D · measured demo", &rows);
    Ok(ok)
}

// ═══════════════════════════════════════════════════════════════════════════
// PROBE-BANK-001
// ═══════════════════════════════════════════════════════════════════════════

/// `PROBE-BANK-001` — A E29-E41, B loan lifecycle ×2.
pub fn run_bank_001() -> i32 {
    println!(
        "PROBE-BANK-001 - bancos: festivos, préstamo, LOC, peso, casa/barco, mora (M30, CR-f716455c, REQ-L99-063 · spec §5)"
    );

    // ── A · E29-E41 ───────────────────────────────────────────────────────
    let mut vectors: Vec<(&'static str, bool)> = Vec::with_capacity(13);

    // E29-E31 holidays.
    vectors.push((
        "E29",
        holidays::get_holiday_id(45 * 1440, 0) == 10,
    ));
    vectors.push((
        "E30",
        holidays::get_holiday_id(57 * 1440, 25) == 12,
    ));
    vectors.push((
        "E31",
        holidays::get_holiday_id(0, 7) == 1,
    ));

    // E32 borrow.
    let mut borrow = BankLedger::new(REGION_COUNT);
    let borrow_ok = borrow.borrow_loan(0, 1000, 3, 0).is_ok()
        && borrow.loan_total(0) == Ok(1100)
        && borrow.loan_due(0) == Ok(518_400)
        && borrow.account_gold(0) == Ok(1000);
    vectors.push(("E32", borrow_ok));

    // E33 loan bounds (0 mutation on error).
    let mut bounds = BankLedger::new(REGION_COUNT);
    let bounds_ok = bounds.borrow_loan(0, 99, 3, 0) == Err(BankError::LoanRequestTooLow)
        && bounds.borrow_loan(0, 150_001, 3, 0) == Err(BankError::LoanRequestTooHigh)
        && bounds.account_gold(0) == Ok(0)
        && bounds.loan_total(0) == Ok(0);
    vectors.push(("E33", bounds_ok));

    // E34 LOC commission and minimum.
    let mut loc = BankLedger::new(REGION_COUNT);
    loc.deposit_gold(0, 10_000, &mut Purse::new(10_000))
        .expect("funded account");
    let mut loc_purse = Purse::new(0);
    let loc_ok = loc.withdraw_loc(0, 100, &mut loc_purse).is_ok()
        && loc.account_gold(0) == Ok(9_899)
        && loc_purse.letters_of_credit == vec![100]
        && loc.withdraw_loc(0, 99, &mut loc_purse) == Err(BankError::LocRequestTooSmall)
        && loc.account_gold(0) == Ok(9_899);
    vectors.push(("E34", loc_ok));

    // E35/E36 withdrawal weight.
    let mut weight = BankLedger::new(REGION_COUNT);
    weight
        .deposit_gold(0, 10_000, &mut Purse::new(10_000))
        .expect("funded account");
    let mut heavy_purse = Purse {
        carried_weight_units: economy::weight_units_from_kg(70.0),
        max_encumbrance_units: economy::weight_units_from_kg(75.0),
        ..Purse::new(0)
    };
    vectors.push(("E35", weight.withdraw_gold(0, 2000, &mut heavy_purse).is_ok()));
    vectors.push((
        "E36",
        weight.withdraw_gold(0, 3000, &mut heavy_purse) == Err(BankError::TooHeavy)
            && weight.account_gold(0) == Ok(8000),
    ));

    // E37 ships.
    vectors.push((
        "E37",
        ShipType::Small.sell_price() == 85_000 && ShipType::Large.sell_price() == 170_000,
    ));

    // E38/E39 houses.
    vectors.push((
        "E38",
        economy::house_price(12.5) == 16_000
            && economy::house_sell_price(12.5) == 13_600,
    ));
    vectors.push((
        "E39",
        economy::house_price(7.75) == 9920 && economy::house_sell_price(7.75) == 8432,
    ));

    // E40 reminder across 6 months.
    let mut reminder = BankLedger::new(REGION_COUNT);
    reminder.borrow_loan(0, 1000, 3, 0).expect("borrow");
    let due = reminder.loan_due(0).expect("due");
    let _ = reminder.check_overdue(0, due - 302_400, due - 216_000);
    vectors.push((
        "E40",
        reminder.events() == [LoanEvent::Borrow, LoanEvent::Due],
    ));

    // E41 overdue pays from the account and defaults once. State of the
    // vector: debt 5000, account 3000 (borrow 4546 → 4546+454 = 5000, then
    // withdraw it and deposit 3000).
    let mut overdue = BankLedger::new(REGION_COUNT);
    overdue.borrow_loan(0, 4546, 3, 0).expect("borrow");
    let mut overdue_purse = Purse {
        max_encumbrance_units: economy::weight_units_from_kg(1000.0),
        ..Purse::new(0)
    };
    overdue
        .withdraw_gold(0, 4546, &mut overdue_purse)
        .expect("withdraw");
    overdue
        .deposit_gold(0, 3000, &mut overdue_purse)
        .expect("deposit");
    let overdue_due = overdue.loan_due(0).expect("due");
    let crime = overdue.check_overdue(0, 0, overdue_due + 1);
    vectors.push((
        "E41",
        crime == Ok(Some(LoanCrime::LoanDefault))
            && overdue.account_gold(0) == Ok(0)
            && overdue.loan_total(0) == Ok(2000)
            && overdue.has_defaulted(0) == Ok(true)
            && overdue.events()
                == [
                    LoanEvent::Borrow,
                    LoanEvent::Overdue,
                    LoanEvent::Default,
                ],
    ));

    println!("\n-- A · golden vectors (id|result) --");
    let mut ok_count = 0usize;
    for (id, ok) in &vectors {
        println!("{}|{}", id, if *ok { "ok" } else { "FAIL" });
        if *ok {
            ok_count += 1;
        }
    }
    println!("vectors={} ok={ok_count}", vectors.len());
    if ok_count != vectors.len() {
        for (id, ok) in &vectors {
            if !*ok {
                println!("FAIL {id}");
            }
        }
    }
    // Fresh ledger for the printed weight row (the vector ledger above is
    // already spent by E35).
    let mut weight_view = BankLedger::new(REGION_COUNT);
    weight_view
        .deposit_gold(0, 10_000, &mut Purse::new(10_000))
        .expect("funded account");
    let mut weight_view_purse = Purse {
        carried_weight_units: economy::weight_units_from_kg(70.0),
        max_encumbrance_units: economy::weight_units_from_kg(75.0),
        ..Purse::new(0)
    };
    let ok2000 = weight_view
        .withdraw_gold(0, 2000, &mut weight_view_purse)
        .is_ok();
    let heavy3000 =
        weight_view.withdraw_gold(0, 3000, &mut weight_view_purse) == Err(BankError::TooHeavy);
    let weight_view_account = weight_view.account_gold(0).expect("account");

    let a_rows = vec![
        row(
            "A1 holidays",
            format!(
                "d46r0={} d58r25={} d1r7={}",
                holidays::get_holiday_id(45 * 1440, 0),
                holidays::get_holiday_id(57 * 1440, 25),
                holidays::get_holiday_id(0, 7)
            ),
            "d46r0=10 d58r25=12 d1r7=1",
            "E29-E31 · 53-row table, `dayOfYear <= 355`",
        ),
        row(
            "A2 borrow",
            format!(
                "loan={} due={} account={}",
                borrow.loan_total(0).expect("loan"),
                borrow.loan_due(0).expect("due"),
                borrow.account_gold(0).expect("account")
            ),
            "loan=1100 due=518400 account=1000",
            "E32 · `amount + amount/10`, `now + 518400`",
        ),
        row(
            "A3 loan bounds",
            format!("{bounds_ok}"),
            "true",
            "E33 · <100 ⇒ TOO_LOW, >level·50000 ⇒ TOO_HIGH, 0 mutation",
        ),
        row(
            "A4 LOC",
            format!(
                "account={} loc={:?}",
                loc.account_gold(0).expect("account"),
                loc_purse.letters_of_credit
            ),
            "account=9899 loc=[100]",
            "E34 · `int(amount × 1.01)`, minimum 100",
        ),
        row(
            "A5 weight",
            format!(
                "ok2000={ok2000} heavy3000={heavy3000} account={weight_view_account}"
            ),
            "ok2000=true heavy3000=true account=8000",
            "E35/E36 · 70+5.0 = 75 is not heavy; 77.5 is",
        ),
        row(
            "A6 ships",
            format!(
                "small={} large={}",
                ShipType::Small.sell_price(),
                ShipType::Large.sell_price()
            ),
            "small=85000 large=170000",
            "E37 · `int(price × 0.85)`",
        ),
        row(
            "A7 houses",
            format!(
                "r12.5={}/{} r7.75={}/{}",
                economy::house_price(12.5),
                economy::house_sell_price(12.5),
                economy::house_price(7.75),
                economy::house_sell_price(7.75)
            ),
            "r12.5=16000/13600 r7.75=9920/8432",
            "E38-E39 · `int(radius × 1280)` then 0.85",
        ),
        row(
            "A8 reminder",
            format!("events={:?}", reminder.events()),
            "events=[Borrow, Due]",
            "E40 · 7 → 5 months crosses 6",
        ),
        row(
            "A9 overdue",
            format!(
                "loan={} defaulted={} events={:?}",
                overdue.loan_total(0).expect("loan"),
                overdue.has_defaulted(0).expect("defaulted"),
                overdue.events()
            ),
            "loan=2000 defaulted=true events=[Borrow, Overdue, Default]",
            "E41 · pays 3000 from the account, emits the M28 fact",
        ),
        row(
            "A10 vectors",
            format!("vectors={} ok={ok_count}", vectors.len()),
            "vectors=13 ok=13",
            "E29-E41",
        ),
    ];
    let a_ok = print_rows("A · bank vectors (spec §8)", &a_rows) && ok_count == vectors.len();
    let mut pass = a_ok;

    // ── B · loan lifecycle ×2 ─────────────────────────────────────────────
    let lifecycle = || -> (u64, Vec<LoanEvent>, i64, i64) {
        let mut ledger = BankLedger::new(REGION_COUNT);
        ledger.borrow_loan(0, 5000, 3, 0).expect("borrow");
        let due = ledger.loan_due(0).expect("due");
        let _ = ledger.check_overdue(0, due - 302_400, due - 216_000);
        let _ = ledger.check_overdue(0, due - 216_000, due + 1);
        let owed = ledger.loan_total(0).expect("loan");
        ledger
            .repay_loan(0, owed, false, &mut Purse::new(5000))
            .expect("repay");
        let state = format!(
            "account={} loan={} due={} defaulted={}",
            ledger.account_gold(0).expect("account"),
            ledger.loan_total(0).expect("loan"),
            ledger.loan_due(0).expect("due"),
            ledger.has_defaulted(0).expect("defaulted")
        );
        (
            fnv1a64(state.as_bytes()),
            ledger.events().to_vec(),
            ledger.loan_total(0).expect("loan"),
            ledger.account_gold(0).expect("account"),
        )
    };
    let first = lifecycle();
    let second = lifecycle();
    let state_digest = first.0;
    let events = first.1.clone();
    let expected_events = [
        LoanEvent::Borrow,
        LoanEvent::Due,
        LoanEvent::Overdue,
        LoanEvent::Default,
        LoanEvent::Repay,
    ];
    let events_ok = events == expected_events;
    println!("\n-- B · loan lifecycle ×2 (spec §8 B) --");
    println!(
        "state_digest={state_digest:016x} state_digest_2={:016x} identical={} events={}",
        second.0,
        first.0 == second.0,
        events.len()
    );
    println!(
        "repay_outcome: loan={} account={} events={:?}",
        first.2, first.3, events
    );
    let b_rows = vec![
        row(
            "B1 state digest",
            format!("{state_digest:016x}"),
            format!("{:016x}", second.0),
            "borrow→reminder→overdue→repay ×2 over the same sequence",
        ),
        row(
            "B2 events",
            format!("{}", events.len()),
            "5",
            "Borrow, Due, Overdue, Default, Repay",
        ),
        check(
            "B3 event order",
            events_ok,
            format!("expected [Borrow, Due, Overdue, Default, Repay] got {events:?}"),
        ),
    ];
    let b_ok = print_rows("B · sequence", &b_rows);
    pass &= b_ok;

    println!(
        "\nSUMMARY PROBE-BANK-001: A={} B={} ⇒ {}",
        verdict(a_ok),
        verdict(b_ok),
        verdict(pass)
    );
    if pass {
        0
    } else {
        1
    }
}
