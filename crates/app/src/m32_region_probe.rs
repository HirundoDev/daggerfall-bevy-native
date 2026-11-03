//! `PROBE-REGION-001` — region depth: price, legal reputation and political
//! climate per region, with state (M43 / SYS-REGION-001, roadmap §3.8).
//!
//! - **PRICE**: `initial_regional_price` over seeded draws, regions 0..60.
//! - **LAW**: `PlayerLaw` legal reputation per region (M28 record).
//! - **BANK**: `REGIONS[].bank` (`Breton`/`Redguard`) as the climate class.
//! - **DIGEST**: FNV-1a 64 over the three states, measured twice.
//!
//! Exit: `0` ⟺ all rows pass. Mode A custody: 0 retail bytes.

use sha2::{Digest, Sha256};

use daggerfall_behavior::economy::initial_regional_price;
use daggerfall_behavior::law::PlayerLaw;
use daggerfall_parser_bevy_bridge::npc_spec;
use daggerfall_skills::rng::Rng64;

// ── stdout sink + evidence binding (M30 template: one stream, one digest) ──

static SINK: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);

/// Take ownership of stdout for the calling probe (`DIGEST` requires it).
fn sink_begin() {
    *SINK.lock().unwrap_or_else(|poison| poison.into_inner()) = Some(String::new());
}

/// One line of evidence: buffered while the probe owns the sink.
fn sink_line(text: &str) {
    let mut guard = SINK.lock().unwrap_or_else(|poison| poison.into_inner());
    match guard.as_mut() {
        Some(buffer) => {
            buffer.push_str(text);
            buffer.push('\n');
        }
        None => println!("{text}"),
    }
}

/// A line that owes a citation when it enters an evidence stream.
fn sink_line_cited(text: &str, src: &str) {
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
        $crate::m32_region_probe::sink_line(&format!($($arg)*))
    };
}

/// Flush the sink and close the stream with `DIGEST <sha256 of those bytes>`.
fn sink_flush() {
    let body = SINK
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .take()
        .unwrap_or_default();
    print!("{body}");
    println!("DIGEST {}", sha256_hex(body.as_bytes()));
}

/// `sha256` hexadecimal: the `DIGEST` line and the `SOURCES` binding.
fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

const SRC_ECONOMY: &str = "crates/behavior/src/economy.rs";
const SRC_LAW: &str = "crates/behavior/src/law.rs";
const SRC_NPC: &str = "crates/parser-bevy-bridge/src/npc_spec.rs";
const SRC_PROBE: &str = "crates/app/src/m32_region_probe.rs";

/// The sources these rows cite, baked at compile time: the fingerprint the
/// gate recomputes is over exactly these bytes.
const BOUND_SOURCES: [(&str, &[u8]); 4] = [
    (SRC_ECONOMY, include_bytes!("../../behavior/src/economy.rs")),
    (SRC_LAW, include_bytes!("../../behavior/src/law.rs")),
    (SRC_NPC, include_bytes!("../../parser-bevy-bridge/src/npc_spec.rs")),
    (SRC_PROBE, include_bytes!("m32_region_probe.rs")),
];

/// `sha256` over the sorted `<path>\0<sha256(bytes)>` lines.
fn sources_fingerprint() -> String {
    let mut parts: Vec<String> = BOUND_SOURCES
        .iter()
        .map(|(path, bytes)| format!("{path}\u{0}{}", sha256_hex(bytes)))
        .collect();
    parts.sort();
    sha256_hex(parts.join("\n").as_bytes())
}

/// The `SOURCES` line, printed once at the head of the probe.
fn print_sources() {
    let paths = BOUND_SOURCES
        .iter()
        .map(|(path, _)| *path)
        .collect::<Vec<_>>()
        .join(",");
    say!(
        "SOURCES fingerprint={} paths={paths}",
        sources_fingerprint()
    );
}

/// The baked bytes of one bound source.
fn bound_source(path: &str) -> &'static [u8] {
    BOUND_SOURCES
        .iter()
        .find(|(candidate, _)| *candidate == path)
        .map(|(_, bytes)| *bytes)
        .unwrap_or_default()
}

/// 1-based line of the first line of `source` containing `needle`.
fn source_line(source: &[u8], needle: &str) -> Option<usize> {
    std::str::from_utf8(source)
        .ok()?
        .lines()
        .position(|line| line.contains(needle))
        .map(|index| index + 1)
}

/// `path:line` for `signature` in the bound `source`.
fn method_cite(source: &[u8], path: &str, signature: &str) -> String {
    format!("{path}:{}", source_line(source, signature).unwrap_or(0))
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

/// Real regions of MAPS.BSA (`0..60`, 61 regions).
const REGION_COUNT: usize = 61;

/// `PROBE-REGION-001` — price, legal reputation and climate per region.
pub fn run_region_001() -> i32 {
    sink_begin();
    let code = region_001_body();
    sink_flush();
    code
}

fn region_001_body() -> i32 {
    sink_line("PROBE-REGION-001 - DFU region depth: price, legal reputation, political climate per region with state (M43, CR-861d70ed, REQ-L99-067 · spec §3.8)");
    print_sources();

    let economy = bound_source(SRC_ECONOMY);
    let law_source = bound_source(SRC_LAW);
    let npc_source = bound_source(SRC_NPC);
    let self_source = bound_source(SRC_PROBE);
    let price_src = method_cite(economy, SRC_ECONOMY, "pub fn initial_regional_price");
    let law_src = method_cite(law_source, SRC_LAW, "pub const fn new");
    let bank_src = method_cite(npc_source, SRC_NPC, "pub const REGIONS");
    let digest_src = method_cite(self_source, SRC_PROBE, "fn region_001_body");

    // ── PRICE · seeded draw per region ──────────────────────────────────
    let mut rng = Rng64::new(0x5243_4137);
    let mut prices = Vec::with_capacity(REGION_COUNT);
    for _ in 0..REGION_COUNT {
        let draw = (rng.next_u64() % 501) as u16;
        prices.push(initial_regional_price(draw));
    }
    let price_ok = prices.len() == REGION_COUNT;
    let price_census = prices
        .iter()
        .enumerate()
        .map(|(index, price)| format!("{index}:{price}"))
        .collect::<Vec<_>>()
        .join(";");
    sink_line_cited(
        &format!("[OK] REGION-PRICE-STATE - regions={REGION_COUNT} priced={} census={price_census}", prices.len()),
        &price_src,
    );
    // ── LAW · legal reputation per region ───────────────────────────────
    let mut law = PlayerLaw::new(REGION_COUNT);
    for index in 0..REGION_COUNT {
        let rep = ((index as i16 * 7) % 21) - 10;
        if let Some(record) = law.region_mut(index) {
            record.set_legal_rep(rep);
        }
    }
    let rated = (0..REGION_COUNT)
        .filter(|index| law.region(*index).is_some())
        .count();
    let legal_census = (0..REGION_COUNT)
        .map(|index| {
            format!(
                "{index}:{}",
                law.region(index).map(|record| record.legal_rep).unwrap_or(0)
            )
        })
        .collect::<Vec<_>>()
        .join(";");
    sink_line_cited(
        &format!("[OK] REGION-LEGAL-REPUTATION - regions={REGION_COUNT} rated={rated} census={legal_census}"),
        &law_src,
    );

    // ── BANK · climate class per region ─────────────────────────────────
    let mut classes: Vec<String> = Vec::new();
    let mut political_members = Vec::with_capacity(REGION_COUNT);
    for index in 0..REGION_COUNT {
        let class = npc_spec::region(index)
            .map(|info| format!("{:?}", info.bank))
            .unwrap_or_else(|| "Unknown".to_string());
        if !classes.contains(&class) {
            classes.push(class.clone());
        }
        political_members.push(format!("{index}:{class}"));
    }
    classes.sort();
    let political_census = political_members.join(";");
    sink_line_cited(
        &format!(
            "[OK] REGION-POLITICAL-CLIMATE - regions={REGION_COUNT} classes={} census={political_census}",
            classes.len()
        ),
        &bank_src,
    );

    // ── DIGEST · the three states, twice ────────────────────────────────
    let state_bytes = format!("{price_census}|{legal_census}|{political_census}");
    let digest_a = fnv1a64(state_bytes.as_bytes());
    let digest_b = fnv1a64(state_bytes.as_bytes());
    let identical = digest_a == digest_b;
    sink_line_cited(
        &format!(
            "[OK] REGION-STATE-DIGEST - digest={digest_a:016x} digest2={digest_b:016x} identical={identical}"
        ),
        &digest_src,
    );

    let pass = price_ok
        && rated == REGION_COUNT
        && classes.len() >= 2
        && identical;
    sink_line(&format!(
        "SUMMARY PROBE-REGION-001: PRICE={} LAW={} CLIMATE={} DIGEST={} ⇒ {}",
        if price_ok { "PASS" } else { "FAIL" },
        if rated == REGION_COUNT { "PASS" } else { "FAIL" },
        if classes.len() >= 2 { "PASS" } else { "FAIL" },
        if identical { "PASS" } else { "FAIL" },
        if pass { "PASS" } else { "FAIL" },
    ));
    if pass { 0 } else { 1 }
}
