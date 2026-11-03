//! `m42_faction_probe` — `PROBE-SYS-FACTION-001` (M42 / W42-3): faction
//! tree parsed with a census, ranks and services as a table, versioned
//! reputation (roadmap `docs/acknowledgments/roadmap-m18-m48.md:319`).
//!
//! This probe **extends** M27's `FACTION-001` + `GUILD-001` + `M27-DEMO-001`
//! (`crates/app/src/m27_probe.rs`) instead of repeating them: M27 proves the
//! parse formulas, guild gating goldens and the demo path, so the rows here
//! are the **W42-3 daily layer** the M27 walk lacks: (A) the tree parsed
//! with a census through the new [`FactionCensus`] API (roots, leaves,
//! depth, per-type table); (B) ranks as a table ([`rank_table`]) plus the
//! measured join → +28d promotion path; (C) services as a table
//! ([`service_table`]) per guild across the 10 ranks; (D) versioned
//! reputation ([`ReputationLedger`]) with digest linkage and replay ×2;
//! (E) adversarial NEGATIVE rows with on-disk evidence paths.
//!
//! **Retail:** `FACTION.TXT` via Mode A (`$DF_APP_ROOT` or
//! `$HOME/Games/Daggerfall/app`); unreadable file ⇒ **exit 2** with
//! `VERDICT: PROBE-SYS-FACTION-001 NO DATA` (same custody as M26/M27).
//!
//! **Determinism:** no floats are printed, all iteration is over fixed
//! arrays, and the last line is `DIGEST <sha256>` over every byte printed
//! before it. The ×2 reproducibility is enforced by the follow-up runner
//! lane running two separate processes with byte-identical stdout.

use daggerfall_behavior::factions::{
    FactionCensus, FactionGraph, ReputationLedger, FACTION_FILENAME,
    PROVINCE_TYPE,
};
use daggerfall_behavior::guild::{
    can_access_library, can_access_service, day_since_zero, guild, rank_table,
    service_table, services_visible, Deity, GuildDirectory, GuildKind,
    GuildService,
};
use sha2::{Digest, Sha256};

/// Lane correlation id (W42-3, same lane as W42-1/W42-2).
const CR: &str = "CR-b434f896-2fe9-4750-b70e-9b748370e6c3";

/// Citations (every one resolves on disk).
const SRC_FACTIONS: &str = "crates/behavior/src/factions.rs";
const SRC_GUILD: &str = "crates/behavior/src/guild.rs";
const SPEC_M27: &str =
    "docs/sintergic/game/programming/m27-guilds-factions-spec-CR-5243af63.md";
const SPEC_ROADMAP: &str = "docs/acknowledgments/roadmap-m18-m48.md:319";

/// Promotion cadence (`Guild.cs:74`).
const PROMOTION_DAYS: i64 = 28;

/// Retail witness: faction 10 and its exact ±half delta (`+10` propagated).
const RETAIL_PROP_ID: i32 = 10;
const RETAIL_ALLY_ID: i32 = 106;
const RETAIL_ENEMY_ID: i32 = 25;

// ── evidence rows ───────────────────────────────────────────────────────

/// One printed evidence row.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Row {
    id: String,
    ok: bool,
    detail: String,
}

/// Row whose verdict is computed by the caller.
fn check(id: impl Into<String>, ok: bool, detail: String) -> Row {
    Row {
        id: id.into(),
        ok,
        detail,
    }
}

/// Hexadecimal `sha256` (the `DIGEST` line and nothing else needs it).
fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

/// Loads a Mode A ARENA2 file (same custody as the M26/M27 probes).
fn load(name: &str) -> Result<(std::path::PathBuf, Vec<u8>), String> {
    let path = crate::mode_a_arena2_path(name)?;
    let bytes = std::fs::read(&path).map_err(|err| format!("reading {path:?}: {err}"))?;
    Ok((path, bytes))
}

/// `t[7]=46 t[15]=44 …` in one stable line.
fn type_line(census: &FactionCensus) -> String {
    census
        .by_type
        .iter()
        .enumerate()
        .map(|(index, count)| format!("t[{index}]={count}"))
        .collect::<Vec<_>>()
        .join(" ")
}
/// `5,5,5,6,7,8,9,9,10,10` in one stable line.
fn counts_line(counts: &[usize]) -> String {
    counts
        .iter()
        .map(|count| count.to_string())
        .collect::<Vec<_>>()
        .join(",")
}

// ── entrypoint ──────────────────────────────────────────────────────────

/// `--probe SYS-FACTION-001`: blocks A–E, `EXIT=0` ⟺ every row OK,
/// `EXIT=2` when retail is not accessible.
pub fn run_sys_faction_001() -> i32 {
    let mut out = String::new();
    out.push_str(&format!(
        "PROBE-SYS-FACTION-001 - faction tree with census, rank/service tables, versioned reputation (M42/W42-3, {CR})\nroadmap={SPEC_ROADMAP} spec={SPEC_M27}\n"
    ));

    let (path, bytes) = match load(FACTION_FILENAME) {
        Ok(loaded) => loaded,
        Err(reason) => {
            out.push_str(&format!("ARENA2 not accessible: {reason}\n"));
            out.push_str("reason=missing_faction_txt\nEXIT=2\n");
            out.push_str("VERDICT: PROBE-SYS-FACTION-001 NO DATA - FACTION.TXT not accessible\n");
            print!("{out}");
            return 2;
        }
    };
    let text = match std::str::from_utf8(&bytes) {
        Ok(text) => text,
        Err(err) => {
            out.push_str(&format!("FAIL faction_txt_utf8 got={err} want=ok\n"));
            out.push_str("reason=missing_faction_txt\nEXIT=2\n");
            out.push_str("VERDICT: PROBE-SYS-FACTION-001 NO DATA - unreadable FACTION.TXT\n");
            print!("{out}");
            return 2;
        }
    };
    let first = match FactionGraph::parse(text, 0x1234) {
        Ok(graph) => graph,
        Err(err) => {
            out.push_str(&format!("FAIL faction_parse got={err:?} want=ok\nEXIT=1\n"));
            out.push_str("VERDICT: PROBE-SYS-FACTION-001 FAIL\n");
            print!("{out}");
            return 1;
        }
    };
    let second = match FactionGraph::parse(text, 0x1234) {
        Ok(graph) => graph,
        Err(err) => {
            out.push_str(&format!("FAIL faction_parse_x2 got={err:?} want=ok\nEXIT=1\n"));
            out.push_str("VERDICT: PROBE-SYS-FACTION-001 FAIL\n");
            print!("{out}");
            return 1;
        }
    };
    let census = first.census();
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(FACTION_FILENAME);
    out.push_str(&format!(
        "DISK:{file_name} bytes={} blocks={} digest={:016x}\n",
        bytes.len(),
        first.len(),
        first.digest(),
    ));
    out.push_str(&format!(
        "CENSUS total={} dup={} roots={} leaves={} depth={} unknown={}\n",
        census.total,
        census.duplicates,
        census.roots,
        census.leaves,
        census.max_depth,
        census.unknown_type,
    ));
    out.push_str(&format!("TYPES {}\n", type_line(&census)));

    let mut ok = true;

    // ── A · census through the new API ──────────────────────────────────
    out.push_str("\n-- A · tree parsed with a census --\n");
    let provinces = first
        .find_factions(Some(PROVINCE_TYPE), None, None, None)
        .len();
    let ggroup11 = first.find_factions(None, None, Some(11), None).len();
    let ggroup17 = first.find_factions(None, None, Some(17), None).len();
    out.push_str(&format!(
        "RETAIL provinces={provinces} ggroup[11]={ggroup11} ggroup[17]={ggroup17}\n"
    ));

    // ── B · rank table + promotion path ─────────────────────────────────
    out.push_str("\n-- B · ranks as a table + measured promotion --\n");
    let table = rank_table();
    out.push_str(&format!(
        "RANKS rows={} row0=({},{},{}) row9=({},{},{})\n",
        table.len(),
        table[0].reputation,
        table[0].skill_high,
        table[0].skill_low,
        table[9].reputation,
        table[9].skill_high,
        table[9].skill_low,
    ));
    let mages = guild(GuildKind::Mages);
    let day_zero = day_since_zero(405, 1);
    let mut directory = GuildDirectory::new();
    let joined = directory.join(&mages, day_zero);
    let services_before = services_visible(&mages, joined.rank).len();
    let library_rank0 = can_access_library(&mages, joined.rank);
    let skills = [100i16; 35];
    let promotion = directory.update(&mages, 30, &skills, day_zero + PROMOTION_DAYS);
    let rank3 = directory.rank(mages.group).unwrap_or(-1);
    let services_after = services_visible(&mages, rank3).len();
    let library_rank3 = can_access_library(&mages, rank3);
    out.push_str(&format!(
        "PROMOTION join_rank={} event={promotion:?} rank={rank3} services={services_before}->{services_after} library={library_rank0}->{library_rank3}\n",
        joined.rank,
    ));

    // ── C · service tables ──────────────────────────────────────────────
    out.push_str("\n-- C · services as a table (10 ranks each) --\n");
    let mages_counts: Vec<usize> = (0..10)
        .map(|rank| service_table(&mages)[rank as usize].len())
        .collect();
    out.push_str(&format!("MAGES counts={}\n", counts_line(&mages_counts)));
    let fighters = guild(GuildKind::Fighters);
    let knights = guild(GuildKind::KnightlyOrder);
    let fighters_counts: Vec<usize> = (0..10)
        .map(|rank| services_visible(&fighters, rank).len())
        .collect();
    let knights_counts: Vec<usize> = (0..10)
        .map(|rank| services_visible(&knights, rank).len())
        .collect();
    out.push_str(&format!(
        "FIGHTERS counts={} KNIGHTS counts={}\n",
        counts_line(&fighters_counts),
        counts_line(&knights_counts),
    ));
    let arkay = guild(GuildKind::Temple(Deity::Arkay));
    out.push_str(&format!(
        "ARKAY soulgems@3={} soulgems@4={} rank0_has_magic={}\n",
        can_access_service(&arkay, 3, GuildService::BuySoulgems),
        can_access_service(&arkay, 4, GuildService::BuySoulgems),
        can_access_service(&arkay, 0, GuildService::BuyMagicItems),
    ));
    for deity in Deity::ALL {
        let def = guild(GuildKind::Temple(deity));
        let counts: Vec<usize> = (0..10)
            .map(|rank| services_visible(&def, rank).len())
            .collect();
        out.push_str(&format!(
            "TEMPLE/{} counts={}\n",
            deity.name(),
            counts_line(&counts)
        ));
    }

    // ── D · versioned reputation ────────────────────────────────────────
    out.push_str("\n-- D · versioned reputation ledger --\n");
    let mut ledger = ReputationLedger::new();
    let mut retail = first.clone();
    let applied = ledger.apply(&mut retail, RETAIL_PROP_ID, 10, true);
    let entry = ledger.last().cloned();
    let digest_v1 = retail.digest();
    if let Some(change) = &entry {
        out.push_str(&format!(
            "LEDGER v{} id={} amount={} propagate={} deltas={} digest={:016x}->{:016x}\n",
            change.version,
            change.faction_id,
            change.amount,
            change.propagate,
            change.deltas.len(),
            change.digest_before,
            change.digest_after,
        ));
    }
    // Replay ×2: same seed ⇒ same deltas, digests link across versions.
    let mut replay = first.clone();
    let mut ledger2 = ReputationLedger::new();
    ledger2.apply(&mut replay, RETAIL_PROP_ID, 10, true);
    let chained = ledger.apply(&mut retail, RETAIL_PROP_ID, -4, false);
    let chain = ledger.last().cloned();

    // ── verdict rows ────────────────────────────────────────────────────
    let mut rows: Vec<Row> = Vec::new();
    rows.push(check(
        "census-blocks",
        census.total == 366
            && census.duplicates == 1
            && bytes.len() == 74221
            && first.len() == 366,
        format!(
            "blocks=366 dup=1 bytes=74221 (spec §1.4) src={SRC_FACTIONS}"
        ),
    ));
    rows.push(check(
        "census-determinism-x2",
        first.digest() == second.digest() && census == second.census(),
        format!(
            "parse x2 ⇒ same digest {:016x} + same census src={SRC_FACTIONS}",
            first.digest(),
        ),
    ));
    rows.push(check(
        "census-shape",
        census.roots == 102
            && census.leaves == 270
            && census.max_depth == 4
            && census.unknown_type == 0,
        format!(
            "roots=102 leaves=270 depth=4 unknown=0 (measured, stable under 0x1234) src={SRC_FACTIONS}"
        ),
    ));
    rows.push(check(
        "census-types",
        census.by_type[7] == 46
            && census.by_type[15] == 44
            && census.by_type.iter().sum::<usize>() == census.total,
        format!(
            "type[7]=46 type[15]=44, 16 buckets sum to total src={SRC_FACTIONS}"
        ),
    ));
    rows.push(check(
        "census-ggroups",
        provinces == 46 && ggroup11 == 5 && ggroup17 == 51,
        format!("provinces=46 ggroup[11]=5 ggroup[17]=51 (M27 C block) src={SRC_FACTIONS}"),
    ));
    rows.push(check(
        "rank-table",
        table.len() == 10
            && (table[0].reputation, table[0].skill_high, table[0].skill_low) == (0, 22, 4)
            && (table[9].reputation, table[9].skill_high, table[9].skill_low) == (90, 87, 37),
        format!("10 rows; row0=(0,22,4) row9=(90,87,37) (Guild.cs:37-39) src={SRC_GUILD}"),
    ));
    rows.push(check(
        "promotion-path",
        joined.rank == 0
            && rank3 == 3
            && services_before == 5
            && services_after == 6
            && !library_rank0
            && library_rank3,
        format!(
            "join rank 0 ⇒ +28d rank 3, services 5→6, library locked→open src={SRC_GUILD} spec={SPEC_M27}"
        ),
    ));
    rows.push(check(
        "service-tables",
        mages_counts == vec![5, 5, 5, 6, 7, 8, 9, 9, 10, 10]
            && fighters_counts == vec![7; 10]
            && knights_counts == vec![3; 10]
            && service_table(&mages)[9].contains(&GuildService::Teleport)
            && !service_table(&mages)[0].contains(&GuildService::Teleport)
            && service_table(&knights)[0].contains(&GuildService::ReceiveHouse),
        format!(
            "mages 5→10 (teleport@8), fighters ×7, knights ×3 + house (Services.cs:19-49) src={SRC_GUILD}"
        ),
    ));
    rows.push(check(
        "service-thresholds",
        can_access_service(&arkay, 4, GuildService::BuySoulgems)
            && !can_access_service(&arkay, 3, GuildService::BuySoulgems),
        format!("Arkay BuySoulgems opens at rank 4 (Temple.cs:132-141) src={SRC_GUILD}"),
    ));
    let witness_ok = entry.as_ref().is_some_and(|change| {
        change.version == 1
            && change.faction_id == RETAIL_PROP_ID
            && change.deltas.contains(&(RETAIL_PROP_ID, 0, 10))
            && change.deltas.contains(&(RETAIL_ALLY_ID, 0, 5))
            && change.deltas.contains(&(RETAIL_ENEMY_ID, 0, -5))
            && change.digest_after == digest_v1
    });
    rows.push(check(
        "ledger-versioned",
        applied && witness_ok,
        format!(
            "v1 id=10 ±10 ⇒ (10,0,10) (106,0,5) (25,0,-5), digest linked src={SRC_FACTIONS}"
        ),
    ));
    let replay_ok = ledger2.last().cloned().is_some_and(|second_change| {
        entry.as_ref().is_some_and(|first_change| {
            second_change.deltas == first_change.deltas
                && second_change.digest_after == first_change.digest_after
        })
    });
    let chain_ok = chain.as_ref().is_some_and(|linked| {
        entry.as_ref().is_some_and(|first_change| {
            linked.version == 2
                && linked.digest_before == first_change.digest_after
                && linked.deltas == vec![(RETAIL_PROP_ID, 10, 6)]
        })
    });
    rows.push(check(
        "ledger-replay-x2",
        replay_ok && chained && chain_ok,
        format!(
            "same seed x2 ⇒ same deltas; v2 chains on v1 digest (−4 ⇒ 10→6) src={SRC_FACTIONS}"
        ),
    ));
    let mut unknown_graph = first.clone();
    let mut unknown_ledger = ReputationLedger::new();
    let refused = !unknown_ledger.apply(&mut unknown_graph, 999_999, 10, true);
    rows.push(check(
        "unknown-refused",
        refused
            && unknown_ledger.version() == 0
            && unknown_ledger.history().is_empty()
            && unknown_graph.digest() == first.digest(),
        format!("id 999999 ⇒ false, version 0, digest untouched src={SRC_FACTIONS}"),
    ));
    let empty = FactionGraph::parse("", 0x1234)
        .map(|graph| graph.census())
        .unwrap_or(FactionCensus {
            total: usize::MAX,
            duplicates: usize::MAX,
            roots: usize::MAX,
            leaves: usize::MAX,
            max_depth: usize::MAX,
            by_type: [usize::MAX; 16],
            unknown_type: usize::MAX,
        });
    rows.push(check(
        "empty-census-zero",
        empty.total == 0
            && empty.roots == 0
            && empty.leaves == 0
            && empty.max_depth == 0
            && empty.unknown_type == 0,
        format!("empty file ⇒ zero census, no panic src={SRC_FACTIONS}"),
    ));

    // ── E · adversarial (declared non-goals with on-disk evidence) ─────
    out.push_str("\n-- E · adversarial (non-goals, negative evidence) --\n");
    out.push_str(&format!("NEGATIVE unknown-faction-rep reason={SRC_FACTIONS}\n"));
    out.push_str(&format!("NEGATIVE invented-rank-row reason={SRC_GUILD}\n"));

    for row in &rows {
        if row.ok {
            out.push_str(&format!("[OK] {} - {}\n", row.id, row.detail));
            ok &= true;
        } else {
            out.push_str(&format!("[FAIL] {} got=fail want=ok - {}\n", row.id, row.detail));
            ok = false;
        }
    }

    out.push_str(if ok {
        "\nVEREDICTO: PASS\n"
    } else {
        "\nVEREDICTO: FAIL\n"
    });
    print!("{out}");
    println!("DIGEST {}", sha256_hex(out.as_bytes()));
    if ok { 0 } else { 1 }
}
