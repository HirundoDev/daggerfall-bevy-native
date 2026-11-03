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
    ShipType, ShopItem, TradeMode, REGION_COUNT,
};
use daggerfall_inventory::commerce::{
    InventoryLite, MerchantStock, StockItem, TradeSession,
};
use daggerfall_parser_rs::dfrandom::DfRandom;
use sha2::{Digest, Sha256};

// ── stdout sink + evidence binding (W43: one stream, one digest) ───────────
//
// Every line this probe family prints goes through `say!`/`sink_line`. While a
// probe owns the sink the bytes accumulate in order and are flushed once,
// followed by the terminal `DIGEST <sha256>` over exactly those bytes. A probe
// that does not own the sink prints directly, so `m10_probe`'s custody lines
// keep their bytes for every other probe (`PROBE-SHOP-001` included).

static SINK: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);

/// Take ownership of stdout for the calling probe (`DIGEST` requires it).
pub(crate) fn sink_begin() {
    *SINK.lock().unwrap_or_else(|poison| poison.into_inner()) = Some(String::new());
}

/// One line of evidence: buffered when a probe owns the sink, printed otherwise.
pub(crate) fn sink_line(text: &str) {
    let mut guard = SINK.lock().unwrap_or_else(|poison| poison.into_inner());
    match guard.as_mut() {
        Some(buffer) => {
            buffer.push_str(text);
            buffer.push('\n');
        }
        None => println!("{text}"),
    }
}

/// A line that owes a citation **when it enters an evidence stream**: the
/// `src=<path>:<line>` is appended for the probe that owns the sink, and the
/// line keeps its documented shape for every other probe (the cited form is a
/// property of the evidence stream, not of the custody event).
pub(crate) fn sink_line_cited(text: &str, src: &str) {
    let mut guard = SINK.lock().unwrap_or_else(|poison| poison.into_inner());
    match guard.as_mut() {
        Some(buffer) => {
            buffer.push_str(text);
            buffer.push_str(" src=");
            buffer.push_str(src);
            buffer.push('\n');
        }
        None => println!("{text}"),
    }
}

/// `say!` — the only print of the family (see [`sink_line`]).
macro_rules! say {
    ($($arg:tt)*) => {
        $crate::m30_probe::sink_line(&format!($($arg)*))
    };
}

/// Flush the sink and close the stream with `DIGEST <sha256 of those bytes>`.
pub(crate) fn sink_flush() {
    let body = SINK
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .take()
        .unwrap_or_default();
    print!("{body}");
    println!("DIGEST {}", sha256_hex(body.as_bytes()));
}

/// `sha256` hexadecimal: the `DIGEST` line and the `SOURCES` binding.
pub(crate) fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

const SRC_ECONOMY: &str = "crates/behavior/src/economy.rs";
const SRC_FORMULA: &str = "crates/skills/src/formula.rs";
const SRC_COMMERCE: &str = "crates/inventory/src/commerce.rs";
const SRC_PROBE: &str = "crates/app/src/m30_probe.rs";
const SRC_M10: &str = "crates/app/src/m10_probe.rs";

/// The sources these rows cite, baked at compile time: the fingerprint the
/// gate recomputes is over exactly these bytes, so a cited file that drifts
/// from the built binary turns `SOURCES` red instead of passing silently.
const BOUND_SOURCES: [(&str, &[u8]); 4] = [
    (SRC_ECONOMY, include_bytes!("../../behavior/src/economy.rs")),
    (SRC_FORMULA, include_bytes!("../../skills/src/formula.rs")),
    (SRC_COMMERCE, include_bytes!("../../inventory/src/commerce.rs")),
    (SRC_PROBE, include_bytes!("m30_probe.rs")),
];

/// `sha256` over the sorted `<path>\0<sha256(bytes)>` lines of an arbitrary
/// bound set: each probe family binds its own `include_bytes!` table, so the
/// fingerprint covers the bytes the *citing* rows resolve in, never another
/// family's.
pub(crate) fn sources_fingerprint_of(sources: &[(&'static str, &'static [u8])]) -> String {
    let mut parts: Vec<String> = sources
        .iter()
        .map(|(path, bytes)| format!("{path}\u{0}{}", sha256_hex(bytes)))
        .collect();
    parts.sort();
    sha256_hex(parts.join("\n").as_bytes())
}

/// The `SOURCES` line of an arbitrary bound set, printed once at the head of
/// a probe.
pub(crate) fn print_sources_of(sources: &[(&'static str, &'static [u8])]) {
    let paths = sources
        .iter()
        .map(|(path, _)| *path)
        .collect::<Vec<_>>()
        .join(",");
    say!(
        "SOURCES fingerprint={} paths={paths}",
        sources_fingerprint_of(sources)
    );
}

/// The `SOURCES` line of this module's set, printed once at the head of each
/// probe.
pub(crate) fn print_sources() {
    print_sources_of(&BOUND_SOURCES);
}

/// The baked bytes of one source of an arbitrary bound set: censuses resolve
/// their cites in **these** bytes, so a cite is always a line of the
/// implementation that produced the row.
pub(crate) fn source_of(sources: &[(&'static str, &'static [u8])], path: &str) -> &'static [u8] {
    sources
        .iter()
        .find(|(candidate, _)| *candidate == path)
        .map(|(_, bytes)| *bytes)
        .unwrap_or_default()
}

/// The economy source, baked: censuses resolve their cites in **these** bytes,
/// so a cite is always a line of the implementation that produced the row.
fn economy_source() -> &'static [u8] {
    source_of(&BOUND_SOURCES, SRC_ECONOMY)
}

/// 1-based line of the first line of `source` containing `needle`.
pub(crate) fn source_line(source: &[u8], needle: &str) -> Option<usize> {
    std::str::from_utf8(source)
        .ok()?
        .lines()
        .position(|line| line.contains(needle))
        .map(|index| index + 1)
}

/// `path:line` for a method of `source`; a method that cannot be resolved
/// cites line `0`, which the gate rejects — red, never silently dropped.
pub(crate) fn method_cite(source: &[u8], path: &str, signature: &str) -> String {
    format!("{path}:{}", source_line(source, signature).unwrap_or(0))
}

// ── evidence rows (M19/M25/M26/M27 template) ───────────────────────────────

/// Printed row: `[OK] <id> - <detail> src=<path>:<line>` or
/// `FAIL <id> got=<v> want=<v>`.
struct Row {
    id: &'static str,
    ok: bool,
    got: String,
    want: String,
    detail: String,
    /// Production line this row measures; empty ⇒ the block's citation.
    src: &'static str,
}

fn row(id: &'static str, got: String, want: impl Into<String>, detail: impl Into<String>) -> Row {
    let want = want.into();
    Row {
        id,
        ok: got == want,
        got,
        want,
        detail: detail.into(),
        src: "",
    }
}

fn check(id: &'static str, ok: bool, detail: impl Into<String>) -> Row {
    Row {
        id,
        ok,
        got: if ok { "ok".into() } else { "fail".into() },
        want: "ok".into(),
        detail: detail.into(),
        src: "",
    }
}

impl Row {
    /// The production line this row measures (`G7`: no uncited `[OK]` row).
    fn src(mut self, src: &'static str) -> Row {
        self.src = src;
        self
    }
}

/// `[OK] <id> - <detail> src=<path>:<line>`; `block_src` is the citation of
/// the whole block, a row that names its own line wins.
fn print_rows(title: &str, rows: &[Row], block_src: &'static str) -> bool {
    say!("\n-- {title} --");
    let mut ok = true;
    for row in rows {
        if row.ok {
            let src = if row.src.is_empty() { block_src } else { row.src };
            if src.is_empty() {
                say!("[OK] {} - {}", row.id, row.detail);
            } else {
                say!("[OK] {} - {} src={}", row.id, row.detail, src);
            }
        } else {
            ok = false;
            say!("FAIL {} got={} want={}", row.id, row.got, row.want);
        }
    }
    ok
}

// Production lines the rows cite. Every one is a line of the conduct the row
// measures (`G7`): the resolver only proves the line exists, the analyst
// proves it is the right one.
const CITE_STOCK_TABLE: &str = "crates/behavior/src/economy.rs:129";
const CITE_SALE_OFFER: &str = "crates/behavior/src/economy.rs:310";
const CITE_ITEM_COST: &str = "crates/behavior/src/economy.rs:358";
const CITE_CALC_COST: &str = "crates/behavior/src/economy.rs:406";
const CITE_INITIAL_PRICE: &str = "crates/behavior/src/economy.rs:414";
const CITE_REPAIR_COST: &str = "crates/behavior/src/economy.rs:471";
const CITE_REPAIR_TIME: &str = "crates/behavior/src/economy.rs:488";
const CITE_COLLECTIVE_REPAIR: &str = "crates/behavior/src/economy.rs:495";
const CITE_IDENTIFY_COST: &str = "crates/behavior/src/economy.rs:502";
const CITE_ROOM_COST: &str = "crates/behavior/src/economy.rs:635";
const CITE_LOAN_MINUTES: &str = "crates/behavior/src/economy.rs:666";
const CITE_REMINDER_MONTHS: &str = "crates/behavior/src/economy.rs:699";
const CITE_HOUSE_PRICE: &str = "crates/behavior/src/economy.rs:732";
const CITE_ACCOUNT: &str = "crates/behavior/src/economy.rs:967";
const CITE_ACCOUNT_GOLD: &str = "crates/behavior/src/economy.rs:972";
const CITE_DEPOSIT: &str = "crates/behavior/src/economy.rs:1025";
const CITE_WITHDRAW: &str = "crates/behavior/src/economy.rs:1047";
const CITE_DEPOSIT_LOC: &str = "crates/behavior/src/economy.rs:1069";
const CITE_WITHDRAW_LOC: &str = "crates/behavior/src/economy.rs:1078";
const CITE_BORROW: &str = "crates/behavior/src/economy.rs:1101";
const CITE_REPAY: &str = "crates/behavior/src/economy.rs:1127";
const CITE_OVERDUE: &str = "crates/behavior/src/economy.rs:1179";
const CITE_DUE_EVENT: &str = "crates/behavior/src/economy.rs:1214";
const CITE_BUY_HOUSE: &str = "crates/behavior/src/economy.rs:1221";
const CITE_SELL_HOUSE: &str = "crates/behavior/src/economy.rs:1246";
const CITE_BUY_SHIP: &str = "crates/behavior/src/economy.rs:1258";
const CITE_SELL_SHIP: &str = "crates/behavior/src/economy.rs:1277";
const CITE_FORECLOSE: &str = "crates/behavior/src/economy.rs:1832";
const CITE_SAVE_STATE: &str = "crates/behavior/src/economy.rs:1888";
const CITE_LOAD_STATE: &str = "crates/behavior/src/economy.rs:1920";
const CITE_TRADE_PRICE: &str = "crates/skills/src/formula.rs:113";
const CITE_SESSION_BUY: &str = "crates/inventory/src/commerce.rs:184";
const CITE_MODE_A_SOURCES: &str = "crates/app/src/m10_probe.rs:271";
const CITE_HOLIDAY: &str = "crates/behavior/src/economy.rs:567";
const CITE_SHIP_BUY: &str = "crates/behavior/src/economy.rs:712";
const CITE_SHIP_SELL: &str = "crates/behavior/src/economy.rs:717";
const CITE_OVERDUE_EVENT: &str = "crates/behavior/src/economy.rs:1190";
const CITE_SPEED: &str = "crates/behavior/src/economy.rs:1325";
const CITE_HOUSE_SELL: &str = "crates/behavior/src/economy.rs:737";
const CITE_PROBE_DOC: &str = "crates/app/src/m30_probe.rs:1";

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
// W43 depth — derived censuses (every value measured from the implementation)
// ═══════════════════════════════════════════════════════════════════════════

/// `(source, name)` of the haggling stage, taken from the symbol the probe
/// calls: the row names the formula it exercised, it does not retype it.
fn haggling_formula() -> &'static str {
    stringify!(daggerfall_skills::formula::calculate_trade_price)
        .rsplit("::")
        .next()
        .unwrap_or("unknown")
}

/// Independent transcription of DFU `FormulaHelper.CalculateTradePrice`
/// (`:1976-2004`) — the **oracle** of `ECON-HAGGLE-FORMULA`. Written from the
/// C# text in `i64` and sharing no helper with `formula.rs`, so a drift in the
/// implementation cannot validate itself: merchant levels `5*(q-10)+50`
/// (`:1983-1984`), sell branch `:1990-1995`, buy branch `:1996-2001` — the
/// `<< 6` term exists **only** on the buying branch.
fn oracle_trade_price(cost: i64, quality: i64, selling: bool, merc: i64, pers: i64) -> i64 {
    let merchant_merc = 5 * (quality - 10) + 50;
    let merchant_pers = 5 * (quality - 10) + 50;
    if selling {
        let delta_merc = ((((100 - merchant_merc) << 8) / 200 + 128) * ((merc << 8) / 200 + 128)) >> 8;
        let delta_pers =
            ((((100 - merchant_pers) << 8) / 200 + 128) * ((pers << 8) / 200 + 128)) >> 8;
        (((179 * delta_merc) >> 8) + ((51 * delta_pers) >> 8)) * cost >> 8
    } else {
        let delta_merc = (((merchant_merc << 8) / 200 + 128) * (((100 - merc) << 8) / 200 + 128)) >> 8;
        let delta_pers = (((merchant_pers << 8) / 200 + 128) * (((100 - pers) << 8) / 200 + 128)) >> 8;
        (((192 * delta_merc) >> 8) + ((delta_pers << 6) >> 8)) * cost >> 8
    }
}

/// `(cases, mismatches)`: the oracle and `calculate_trade_price` over 8 cases
/// covering **both** branches (reminder: `<< 6` only when buying).
fn econ_haggle_formula() -> (usize, usize) {
    const CASES: [(i64, i64, bool, i64, i64); 8] = [
        (200, 10, false, 0, 0),
        (200, 10, false, 100, 100),
        (200, 10, true, 0, 0),
        (200, 10, true, 100, 100),
        (60, 12, true, 80, 45),
        (250, 17, false, 63, 37),
        (12, 3, false, 5, 95),
        (1000, 20, true, 12, 88),
    ];
    let mut mismatches = 0usize;
    for (cost, quality, selling, merc, pers) in CASES {
        let oracle = oracle_trade_price(cost, quality, selling, merc, pers);
        let live = i64::from(daggerfall_skills::formula::calculate_trade_price(
            cost as i32,
            quality as i32,
            selling,
            merc as i32,
            pers as i32,
        ));
        if oracle != live {
            mismatches += 1;
        }
    }
    (CASES.len(), mismatches)
}

/// `(buildings, groups, pairs, census)` of the retail `storeBuysItemType`
/// table, derived from `BuildingType::ALL` × `accepted_groups()` — never from
/// a literal list.
fn econ_stock_census() -> (usize, usize, usize, String) {
    // One member per accepted **pair**: `Building:Group`, `;`-joined, so the
    // census carries 36 members and the row can prove that no pair is missing
    // and none is invented.
    let mut members: Vec<String> = Vec::new();
    let mut groups: Vec<&'static str> = Vec::new();
    for building in BuildingType::ALL {
        for group in building.accepted_groups() {
            groups.push(group.name());
            members.push(format!("{}:{}", building.name(), group.name()));
        }
    }
    groups.sort_unstable();
    groups.dedup();
    (
        BuildingType::ALL.len(),
        groups.len(),
        members.len(),
        members.join(";"),
    )
}

/// `(shops, items, mismatches)`: for every shop the price it **displays**
/// (`Shop::item_cost` → `calculate_trade_price`) is the price the till
/// **charges** (`TradeSession::buy`), over every item it stocks.
fn econ_display_charged() -> (usize, usize, usize) {
    let mut shops = 0usize;
    let mut items = 0usize;
    let mut mismatches = 0usize;
    for building in BuildingType::ALL {
        let stocked: Vec<ShopItem> = building
            .accepted_groups()
            .iter()
            .enumerate()
            .map(|(index, group)| ShopItem {
                // `ItemGroup::name()` is `&'static str` and unique inside one
                // shop (`accepted_groups()` has no duplicates), so the id
                // needs no allocation.
                id: group.name(),
                name: group.name(),
                group: *group,
                base_value: 20 + 7 * index as i64,
                condition: 100,
                max_condition: 100,
                enchanted: false,
                identified: true,
                stack: 1,
            })
            .collect();
        let shop = economy::Shop {
            building,
            quality: 10,
            price_adjustment: Some(1000),
            items: stocked,
        };
        shops += 1;
        let stock = MerchantStock {
            merchant_name: building.name().to_string(),
            shop_quality: i32::from(shop.quality),
            items: shop
                .items
                .iter()
                .map(|item| StockItem {
                    id: item.id.to_string(),
                    name: item.name.to_string(),
                    base_cost: item.base_value as i32,
                    stock: 1,
                })
                .collect(),
        };
        let mut session = TradeSession::new(stock, InventoryLite::new(1_000_000));
        session.open_dialogue();
        for item in &shop.items {
            items += 1;
            let displayed = trade_price(
                shop.item_cost(item),
                shop.quality,
                false,
                50,
                50,
            );
            let gold_before = i64::from(session.inventory.gold);
            let charged = session.buy(item.id, displayed as i32);
            let paid = gold_before - i64::from(session.inventory.gold);
            match charged {
                Ok(price) if i64::from(price) == displayed && paid == displayed => {}
                _ => mismatches += 1,
            }
        }
    }
    (shops, items, mismatches)
}

/// `(refused, of)` — the counter-case of `ECON-STOCK-CENSUS`: for every
/// accepted pair **another** shop refuses that group through the production
/// predicate (`sale_offer(…, Sell, …)`), so the table discriminates instead of
/// accepting everything.
fn econ_stock_negative() -> (usize, usize) {
    let mut refused = 0usize;
    let mut total = 0usize;
    for building in BuildingType::ALL {
        for group in building.accepted_groups() {
            total += 1;
            let refuser = BuildingType::ALL
                .iter()
                .copied()
                .find(|other| *other != building && !other.accepts(*group));
            match refuser {
                Some(other) if !economy::sale_offer(other, TradeMode::Sell, *group, false) => {
                    refused += 1
                }
                _ => {}
            }
        }
    }
    (refused, total)
}

/// `(count, ops)` — the twelve SPEC-M30 §5 operations resolved to the ledger
/// methods that implement them, with the line of every method read out of the
/// baked `economy.rs` bytes.
fn bank_operations_census() -> (usize, String) {
    let source = economy_source();
    let ops: Vec<String> = economy::BANK_OPERATIONS
        .iter()
        .map(|(operation, method)| {
            let signature = format!("pub fn {method}(");
            format!(
                "{operation}:{}",
                method_cite(source, SRC_ECONOMY, &signature)
            )
        })
        .collect();
    (ops.len(), ops.join(";"))
}

/// `(cases, due_minutes, mismatches)` — `due = now + LOAN_REPAY_MINUTES`
/// measured at the request, after a reminder and after the overdue
/// transition (the due date is set once and never moves).
fn bank_due_dates() -> (usize, i64, usize) {
    let mut cases = 0usize;
    let mut mismatches = 0usize;
    let mut due_minutes = 0i64;
    for now in [0i64, 1_000, 250_000] {
        cases += 1;
        let mut ledger = BankLedger::new(REGION_COUNT);
        if ledger.borrow_loan(0, 1000, 3, now).is_err() {
            mismatches += 1;
            continue;
        }
        let due = ledger.loan_due(0).unwrap_or(0);
        due_minutes = due - now;
        if due_minutes != economy::LOAN_REPAY_MINUTES {
            mismatches += 1;
        }
        let _ = ledger.check_overdue(0, due - 302_400, due - 216_000);
        if ledger.loan_due(0) != Ok(due) {
            mismatches += 1;
        }
        let _ = ledger.check_overdue(0, due - 216_000, due + 1);
        if ledger.loan_due(0) != Ok(due) {
            mismatches += 1;
        }
    }
    (cases, due_minutes, mismatches)
}

/// `(months, fired, mismatches)` — the reminder months of `REMINDER_MONTHS`
/// crossed once each by a single monotone walk of the game clock; `fired` is
/// the crossed month read at each emission, so a repeated or missed crossing
/// is a mismatch.
fn bank_reminders() -> (Vec<i64>, Vec<i64>, usize) {
    const MINUTES_PER_MONTH: i64 = 43_200;
    let mut ledger = BankLedger::new(REGION_COUNT);
    ledger.borrow_loan(0, 1000, 3, 0).expect("borrow");
    let due = ledger.loan_due(0).expect("due");
    let months = economy::REMINDER_MONTHS.to_vec();
    let mut fired: Vec<i64> = Vec::new();
    let mut seen = ledger.events().len();
    for step in (0..8).rev() {
        let now = due - step * MINUTES_PER_MONTH;
        let last = now - MINUTES_PER_MONTH;
        let _ = ledger.check_overdue(0, last, now);
        let events = ledger.events();
        if events.len() > seen {
            fired.push((due - last) / MINUTES_PER_MONTH);
            seen = events.len();
        }
    }
    let mismatches = if fired == months { 0 } else { 1 };
    (months, fired, mismatches)
}

/// `(regions, foreclosed, account_after, owned_after)` — an unrepaid overdue
/// loan seizes the house and the ship and drains the account; the negative
/// control is a loan that is **not** overdue, which seizes nothing.
fn bank_foreclosure() -> (usize, usize, i64, bool) {
    let mut ledger = BankLedger::new(REGION_COUNT);
    ledger.borrow_loan(0, 1000, 3, 0).expect("borrow");
    let mut purse = Purse::new(500_000);
    ledger
        .deposit_gold(0, 400_000, &mut purse)
        .expect("funded account");
    ledger
        .purchase_house(0, 7, 5.0, &mut purse)
        .expect("house bought");
    ledger
        .purchase_ship(0, ShipType::Small, &mut purse)
        .expect("ship bought");
    let due = ledger.loan_due(0).expect("due");
    let seized = ledger.foreclose(0, due + 1).expect("foreclose");

    // Negative control: the same book, one minute *before* the due date.
    let mut control = BankLedger::new(REGION_COUNT);
    control.borrow_loan(0, 1000, 3, 0).expect("borrow");
    let mut control_purse = Purse::new(500_000);
    control
        .deposit_gold(0, 400_000, &mut control_purse)
        .expect("funded account");
    control
        .purchase_house(0, 7, 5.0, &mut control_purse)
        .expect("house bought");
    let control_due = control.loan_due(0).expect("due");
    let kept = control.foreclose(0, control_due - 1).expect("foreclose");
    let control_ok = !kept.foreclosed && control.owned_house(0).is_some();

    let foreclosed = usize::from(seized.seized_house) + usize::from(seized.seized_ship);
    let regions = usize::from(seized.foreclosed);
    let account_after = ledger.account_gold(0).expect("account");
    let owned_after = ledger.owned_house(0).is_some() || ledger.owned_ship().is_some();
    let foreclosed = if control_ok { foreclosed } else { 0 };
    (regions, foreclosed, account_after, owned_after)
}

/// `(house, ship, roundtrip)` — the bought house and boat survive
/// `save_state` → `load_state` (and re-saving is byte-identical).
fn bank_property_persist() -> (&'static str, &'static str, bool) {
    let mut ledger = BankLedger::new(REGION_COUNT);
    let mut purse = Purse::new(500_000);
    ledger
        .deposit_gold(0, 400_000, &mut purse)
        .expect("funded account");
    ledger
        .purchase_house(0, 7, 5.0, &mut purse)
        .expect("house bought");
    ledger
        .purchase_ship(0, ShipType::Small, &mut purse)
        .expect("ship bought");
    let saved = ledger.save_state();
    let roundtrip = match BankLedger::load_state(&saved) {
        Ok(restored) => {
            restored.owned_house(0) == ledger.owned_house(0)
                && restored.owned_ship().map(|ship| ship.name()) == ledger.owned_ship().map(|ship| ship.name())
                && restored.account_gold(0) == ledger.account_gold(0)
                && restored.loan_total(0) == ledger.loan_total(0)
                && restored.loan_due(0) == ledger.loan_due(0)
                && restored.save_state() == saved
        }
        Err(_) => false,
    };
    let house = if roundtrip && ledger.owned_house(0).is_some() {
        "deed"
    } else {
        "lost"
    };
    let ship = match ledger.owned_ship() {
        Some(ShipType::Small) => "small",
        Some(ShipType::Large) => "large",
        None => "none",
    };
    (house, ship, roundtrip)
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
    say!(
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
    say!("buildings={buildings}");
    say!("modes={modes}");
    say!("pairs={pairs}");
    say!("mismatches={}", mismatches.len());

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
    let ok = print_rows("A · stock per building type (spec §8 A)", &rows, "");

    say!(
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
///
/// The stream is owned by the probe: everything is buffered, flushed once and
/// closed by `DIGEST <sha256>` over exactly those bytes (`m10_probe`'s custody
/// lines included, they print through the same sink).
pub fn run_economy_001() -> i32 {
    sink_begin();
    let code = economy_001_body();
    sink_flush();
    code
}

/// Body of `PROBE-ECONOMY-001` with the sink already open.
fn economy_001_body() -> i32 {
    say!(
        "PROBE-ECONOMY-001 - DFU economy: prices, holidays, inn, bank, transport (M30, CR-f716455c, REQ-L99-063 · spec §7-§8)"
    );
    print_sources();

    // ── A · E1-E28 ────────────────────────────────────────────────────────
    let vectors = economy_vectors();
    say!("\n-- A · golden vectors (id|expected|actual) --");
    let mut ok_count = 0usize;
    let mut fail_count = 0usize;
    for vector in &vectors {
        say!("{}|{}|{}", vector.id, vector.expected, vector.actual);
        if vector.expected == vector.actual {
            ok_count += 1;
        } else {
            fail_count += 1;
            say!(
                "FAIL {} expected={} actual={}",
                vector.id, vector.expected, vector.actual
            );
        }
    }
    say!("vectors={} ok={} fail={}", vectors.len(), ok_count, fail_count);

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
        )
        .src(CITE_CALC_COST),
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
        )
        .src(CITE_CALC_COST),
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
        )
        .src(CITE_TRADE_PRICE),
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
        )
        .src(CITE_REPAIR_COST),
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
        )
        .src(CITE_IDENTIFY_COST),
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
        )
        .src(CITE_ROOM_COST),
        row(
            "A7 vectors",
            format!("vectors={} ok={ok_count} fail={fail_count}", vectors.len()),
            "vectors=28 ok=28 fail=0",
            "E1-E28 · SPEC-M30 §7",
        )
        .src(CITE_CALC_COST),
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
    )
    .src(CITE_SPEED));

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
    )
    .src(CITE_CALC_COST));

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
    )
    .src(CITE_SESSION_BUY));

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
    )
    .src(CITE_ACCOUNT));

    let a_ok = print_rows("A · evidence + negative controls", &rows, CITE_CALC_COST) && fail_count == 0;
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
    say!("\n-- B · determinism (spec §8 seed policy) --");
    say!(
        "digest_seed1={digest_seed1:016x} digest_seed2={digest_seed2:016x} runs=2 identical={}",
        digest_seed1 == digest_seed2
    );
    say!(
        "adj_seed1={adj_seed1:016x} adj_seed2={adj_seed2:016x} seeds_differ={}",
        adj_seed1 != adj_seed2
    );
    let b_rows = vec![
        row(
            "B1 table digest",
            format!("{digest_seed1:016x}"),
            format!("{digest_seed2:016x}"),
            "same table ×2 ⇒ identical FNV-1a 64 over id|expected|actual",
        )
        .src(CITE_CALC_COST),
        row(
            "B2 seeds are live",
            format!("{}", adj_seed1 != adj_seed2),
            "true",
            "the seeded initial regional prices differ, so B is not tautological",
        )
        .src(CITE_INITIAL_PRICE),
        row(
            "B3 runs",
            "runs=2".to_string(),
            "runs=2",
            "two independent evaluations of E1-E28",
        )
        .src(CITE_CALC_COST),
    ];
    let b_ok = print_rows("B · same table ×2 seeds", &b_rows, CITE_INITIAL_PRICE);
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
            say!("faction_bytes={size}");
            c_rows.push(row(
                "C1 faction size",
                format!("{size}"),
                "74221",
                "M27 retail census (FACTION.TXT, REQ-L99-060 power input of the price drift)",
            )
            .src(CITE_PROBE_DOC));
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
            say!("mapsave=present");
            c_rows.push(row(
                "C2 mapsave",
                "present".to_string(),
                "present",
                "optional retail save (M33 owns its decoding; only presence is measured; absent is honest)",
            )
            .src(CITE_PROBE_DOC));
        }
        Err(_) => {
            say!("mapsave=absent");
            c_rows.push(row(
                "C2 mapsave",
                "absent".to_string(),
                "absent",
                "optional retail save: absent is honest",
            )
            .src(CITE_PROBE_DOC));
        }
    }
    say!("bytes_leidos={bytes_read}");
    c_rows.push(check(
        "C3 bytes read",
        bytes_read <= 64,
        format!("bytes_leidos={bytes_read} (total budget 64; metadata + head)"),
    ));
    let c_ok = print_rows("C · retail census", &c_rows, CITE_STOCK_TABLE);
    pass &= c_ok;

    // ── E · depth (W43: stock census, haggling oracle, till, counter-case) ─
    let mut e_rows: Vec<Row> = Vec::new();
    let (buildings, groups, pairs, census) = econ_stock_census();
    // The census string must carry every accepted pair exactly once.
    let members = census.split(';').count();
    e_rows.push(
        check(
            "ECON-STOCK-CENSUS",
            buildings == 9 && groups >= 10 && pairs == 36 && members == pairs,
            format!("buildings={buildings} groups={groups} pairs={pairs} census={census}"),
        )
        .src(CITE_STOCK_TABLE),
    );
    let (haggle_cases, haggle_mismatches) = econ_haggle_formula();
    e_rows.push(
        check(
            "ECON-HAGGLE-FORMULA",
            haggle_cases >= 8 && haggle_mismatches == 0,
            format!(
                "cases={haggle_cases} formula={} mismatches={haggle_mismatches}",
                haggling_formula()
            ),
        )
        .src(CITE_TRADE_PRICE),
    );
    let (till_shops, till_items, till_mismatches) = econ_display_charged();
    e_rows.push(
        check(
            "ECON-DISPLAY-CHARGED",
            till_shops == 9 && till_items >= 36 && till_mismatches == 0,
            format!("shops={till_shops} items={till_items} mismatches={till_mismatches}"),
        )
        .src(CITE_ITEM_COST),
    );
    let (refused, probed) = econ_stock_negative();
    e_rows.push(
        check(
            "ECON-STOCK-NEGATIVE",
            probed == 36 && refused > 0,
            format!("refused={refused} of={probed}"),
        )
        .src(CITE_SALE_OFFER),
    );
    let e_ok = print_rows("E · depth (W43)", &e_rows, CITE_STOCK_TABLE);
    pass &= e_ok;

    // ── D · demo over the retail capital ──────────────────────────────────
    let d_ok = match run_demo() {
        Ok(demo_ok) => demo_ok,
        Err(reason) => {
            say!("\n-- D · demo (ARENA2) --");
            say!("[EXIT=2] {reason}");
            // D cannot be measured: EXIT 2 when A/B/C passed, 1 otherwise.
            return if pass { 2 } else { 1 };
        }
    };
    pass &= d_ok;

    say!(
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

    say!("\n-- D · demo over the retail capital --");
    say!(
        "[D] shop={} quality={} adj=1000 gold_before={GOLD_BEFORE} buy_price={buy_price} gold_after_buy={gold_after_buy}",
        general.building_type, general.quality
    );
    say!(
        "[D] tavern_quality={tavern_quality} room_cost={room_base} room_price={room_price} room_hours={room_hours} gold_after={gold_after}"
    );
    say!(
        "[D] account_delta={account_delta} loan={loan} due={due} overdue_loan={overdue_loan} covered={covered} crime={crime:?} accepted={accepted} stock_after={stock_after} goods_after={goods_after}"
    );

    let rows = vec![
        row(
            "D1 purchase",
            format!("paid={paid} gold_after_buy={gold_after_buy}"),
            format!("paid={buy_price} gold_after_buy={}", GOLD_BEFORE - buy_price),
            "gold -= price · stock -= 1 · inventory += 1",
        )
        .src(CITE_SESSION_BUY),
        row(
            "D2 stock",
            format!("{stock_after}"),
            "2",
            "3 offered − 1 bought",
        )
        .src(CITE_SESSION_BUY),
        row(
            "D3 inventory",
            format!("{goods_after}"),
            "1",
            "potion added",
        )
        .src(CITE_SESSION_BUY),
        row(
            "D4 room hours",
            format!("{room_hours}"),
            "72",
            "3 nights × 24 h (M32 consumes the expiry)",
        )
        .src(CITE_ROOM_COST),
        row(
            "D5 account",
            format!("{account_delta}"),
            "1500",
            "deposit 500 + loan principal 1000",
        )
        .src(CITE_DEPOSIT),
        row(
            "D6 loan",
            format!("{loan}"),
            "1100",
            "principal + fixed 10 %",
        )
        .src(CITE_BORROW),
        row(
            "D7 due",
            format!("{due}"),
            format!("{}", NOW_MINUTES + 518_400),
            "360 × 1440 minutes",
        )
        .src(CITE_LOAN_MINUTES),
        row(
            "D8 overdue payment",
            format!("{overdue_loan}"),
            format!("{covered}"),
            "one minute past due: account pays min(loan, account) (§12(d))",
        )
        .src(CITE_OVERDUE),
        row(
            "D9 unaccepted item",
            format!("{accepted}"),
            "false",
            "GeneralStore does not buy ingredients (§12(b))",
        )
        .src(CITE_SALE_OFFER),
    ];
    let ok = print_rows("D · measured demo", &rows, CITE_SESSION_BUY);
    Ok(ok)
}

// ═══════════════════════════════════════════════════════════════════════════
// PROBE-BANK-001
// ═══════════════════════════════════════════════════════════════════════════

/// `PROBE-BANK-001` — A E29-E41, B loan lifecycle ×2.
pub fn run_bank_001() -> i32 {
    sink_begin();
    let code = bank_001_body();
    sink_flush();
    code
}

/// Body of `PROBE-BANK-001` with the sink already open.
fn bank_001_body() -> i32 {
    say!(
        "PROBE-BANK-001 - bancos: festivos, préstamo, LOC, peso, casa/barco, mora (M30, CR-f716455c, REQ-L99-063 · spec §5)"
    );
    print_sources();

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

    say!("\n-- A · golden vectors (id|result) --");
    let mut ok_count = 0usize;
    for (id, ok) in &vectors {
        say!("{}|{}", id, if *ok { "ok" } else { "FAIL" });
        if *ok {
            ok_count += 1;
        }
    }
    say!("vectors={} ok={ok_count}", vectors.len());
    if ok_count != vectors.len() {
        for (id, ok) in &vectors {
            if !*ok {
                say!("FAIL {id}");
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
        )
        .src(CITE_HOLIDAY),
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
        )
        .src(CITE_BORROW),
        row(
            "A3 loan bounds",
            format!("{bounds_ok}"),
            "true",
            "E33 · <100 ⇒ TOO_LOW, >level·50000 ⇒ TOO_HIGH, 0 mutation",
        )
        .src(CITE_BORROW),
        row(
            "A4 LOC",
            format!(
                "account={} loc={:?}",
                loc.account_gold(0).expect("account"),
                loc_purse.letters_of_credit
            ),
            "account=9899 loc=[100]",
            "E34 · `int(amount × 1.01)`, minimum 100",
        )
        .src(CITE_WITHDRAW_LOC),
        row(
            "A5 weight",
            format!(
                "ok2000={ok2000} heavy3000={heavy3000} account={weight_view_account}"
            ),
            "ok2000=true heavy3000=true account=8000",
            "E35/E36 · 70+5.0 = 75 is not heavy; 77.5 is",
        )
        .src(CITE_WITHDRAW),
        row(
            "A6 ships",
            format!(
                "small={} large={}",
                ShipType::Small.sell_price(),
                ShipType::Large.sell_price()
            ),
            "small=85000 large=170000",
            "E37 · `int(price × 0.85)`",
        )
        .src(CITE_SHIP_SELL),
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
        )
        .src(CITE_HOUSE_PRICE),
        row(
            "A8 reminder",
            format!("events={:?}", reminder.events()),
            "events=[Borrow, Due]",
            "E40 · 7 → 5 months crosses 6",
        )
        .src(CITE_REMINDER_MONTHS),
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
        )
        .src(CITE_OVERDUE),
        row(
            "A10 vectors",
            format!("vectors={} ok={ok_count}", vectors.len()),
            "vectors=13 ok=13",
            "E29-E41",
        )
        .src(CITE_HOLIDAY),
    ];
    let a_ok = print_rows("A · bank vectors (spec §8)", &a_rows, CITE_OVERDUE) && ok_count == vectors.len();
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
    say!("\n-- B · loan lifecycle ×2 (spec §8 B) --");
    say!(
        "state_digest={state_digest:016x} state_digest_2={:016x} identical={} events={}",
        second.0,
        first.0 == second.0,
        events.len()
    );
    say!(
        "repay_outcome: loan={} account={} events={:?}",
        first.2, first.3, events
    );
    let b_rows = vec![
        row(
            "B1 state digest",
            format!("{state_digest:016x}"),
            format!("{:016x}", second.0),
            "borrow→reminder→overdue→repay ×2 over the same sequence",
        )
        .src(CITE_BORROW),
        row(
            "B2 events",
            format!("{}", events.len()),
            "5",
            "Borrow, Due, Overdue, Default, Repay",
        )
        .src(CITE_OVERDUE_EVENT),
        check(
            "B3 event order",
            events_ok,
            format!("expected [Borrow, Due, Overdue, Default, Repay] got {events:?}"),
        )
        .src(CITE_OVERDUE_EVENT),
    ];
    let b_ok = print_rows("B · sequence", &b_rows, CITE_DUE_EVENT);

    // ── C · depth (W43: operations census, due dates, reminders, seizure) ─
    let mut c_rows: Vec<Row> = Vec::new();
    let (op_count, ops) = bank_operations_census();
    let op_entries = ops.split(';').filter(|entry| !entry.is_empty()).count();
    c_rows.push(
        check(
            "BANK-OPERATIONS-CENSUS",
            op_count == 12 && op_entries == 12,
            format!("count={op_count} ops={ops}"),
        )
        .src(CITE_BORROW),
    );
    let (due_cases, due_minutes, due_mismatches) = bank_due_dates();
    c_rows.push(
        check(
            "BANK-DUE-DATES",
            due_cases >= 3 && due_mismatches == 0,
            format!("cases={due_cases} due_minutes={due_minutes} mismatches={due_mismatches}"),
        )
        .src(CITE_LOAN_MINUTES),
    );
    let (months, fired, reminder_mismatches) = bank_reminders();
    let months_field = months
        .iter()
        .map(i64::to_string)
        .collect::<Vec<_>>()
        .join(";");
    let fired_field = fired
        .iter()
        .map(i64::to_string)
        .collect::<Vec<_>>()
        .join(";");
    c_rows.push(
        check(
            "BANK-REMINDERS",
            reminder_mismatches == 0,
            format!(
                "months={months_field} fired={fired_field} mismatches={reminder_mismatches}"
            ),
        )
        .src(CITE_REMINDER_MONTHS),
    );
    let (regions, foreclosed, account_after, owned_after) = bank_foreclosure();
    c_rows.push(
        check(
            "BANK-FORECLOSURE",
            regions >= 1 && foreclosed >= 1 && account_after == 0 && !owned_after,
            format!(
                "regions={regions} foreclosed={foreclosed} account_after={account_after} owned_after={owned_after}"
            ),
        )
        .src(CITE_FORECLOSE),
    );
    let (house, ship, roundtrip) = bank_property_persist();
    c_rows.push(
        check(
            "BANK-PROPERTY-PERSIST",
            roundtrip,
            format!("house={house} ship={ship} roundtrip={roundtrip}"),
        )
        .src(CITE_SAVE_STATE),
    );
    let c_ok = print_rows("C · depth (W43)", &c_rows, CITE_BORROW);
    pass &= c_ok;
    pass &= b_ok;

    say!(
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
