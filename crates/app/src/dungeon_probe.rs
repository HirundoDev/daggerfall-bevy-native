//! PROBE-DUNGEON-001 — dungeon grid assembled from `MAPDITEM` + `BLOCKS.BSA`
//! (M34-D / G-N1, geometry only; spec
//! `docs/sintergic/game/programming/m34d-dungeon-grid-spec-CR-2d5c8f41.md`).
//!
//! **What it proves:** every dungeon location of the retail index is resolved
//! end to end — `MAPNAMES`/`MAPPITEM`/`MAPDITEM` → `maps::parse_ditem` →
//! `bridge::dungeon_grid::assemble_dungeon` over the real `.RDB` cells of
//! `BLOCKS.BSA` — and that the placement is the one the reference engine uses:
//! cells on a **51.2 m** lattice (`RDBSide = 2048 × 1/40`), action doors
//! filtered by description/model, overlaps under **1.4 m** removed greedily,
//! and `Start`/`Enter` markers read from the starting block only.
//!
//! **Numbers are measured, never adjusted:** the goldens below come from the
//! two-process run of this probe on the canonical ARENA2
//! (`$HOME/Games/Daggerfall/app`, `DF_APP_ROOT` overrides). A mismatch is a
//! FAIL, and the measured value is printed next to the expected one.
//!
//! `EXIT=2` when ARENA2 (`MAPS.BSA`/`BLOCKS.BSA`) is not accessible — never a
//! silent skip. `EXIT=1` on any failed check or on a digest mismatch between
//! the two in-process passes (the cross-process digest is compared by the
//! operator: run the probe twice and compare `DIGEST=`).
//!
//! **Scope:** no business logic (M34 frontier): no formulas, no AI, no loot.
//! The overlap registry is not seeded with the static `DungeonExit` door
//! (declared delta in `dungeon_grid`), so `doors_removed` counts action-door
//! overlaps only.

use std::time::Instant;

use daggerfall_parser_rs::rdi;

use crate::interior_retail::{RetailDungeon, RetailSource};

/// Dungeons with a `MAPDITEM` entry (measured; UESP declares 4,232).
const EXPECTED_DUNGEONS: usize = 4_232;
/// Locations without a dungeon (15,251 locations total).
const EXPECTED_WITHOUT_DUNGEON: usize = 15_251 - 4_232;
/// Action doors removed by the 1.4 m overlap rule (measured).
///
/// The 380,869 candidates match the independent census byte for byte; the
/// 3-door delta against the earlier f64 census (4,325) is a boundary effect:
/// DFU/Unity `Vector3.Distance` is `f32`, which is what this probe uses.
const EXPECTED_DOORS_REMOVED: usize = 4_322;
/// Dungeons with at least one removed door (measured).
const EXPECTED_DUNGEONS_WITH_DUP: usize = 1_303;
/// Dungeons whose starting block carries a `Start` flat (measured: all).
const EXPECTED_START_MARKERS: usize = 4_232;
/// Dungeons whose starting block carries an `Enter` flat (measured 18.3 %;
/// DFU leaves `enterMarker` null for the rest).
const EXPECTED_ENTER_MARKERS: usize = 775;

/// One fixed sample row: the first five dungeon locations in index order.
struct Sample {
    region: u16,
    index: usize,
    location_id: u16,
    blocks: usize,
    extent: [i8; 4],
    start_block: &'static str,
    enter: bool,
    doors: usize,
    removed: usize,
}

/// Measured sample (spec §4.3; deterministic index order, no sampling).
const SAMPLE: [Sample; 5] = [
    Sample {
        region: 0,
        index: 2,
        location_id: 26_219,
        blocks: 12,
        extent: [-3, 1, -1, 2],
        start_block: "N0000006.RDB",
        enter: false,
        doors: 129,
        removed: 0,
    },
    Sample {
        region: 0,
        index: 6,
        location_id: 26_278,
        blocks: 12,
        extent: [-2, 1, -2, 2],
        start_block: "N0000050.RDB",
        enter: false,
        doors: 117,
        removed: 6,
    },
    Sample {
        region: 0,
        index: 8,
        location_id: 26_310,
        blocks: 11,
        extent: [-1, 3, -1, 1],
        start_block: "W0000011.RDB",
        enter: false,
        doors: 108,
        removed: 0,
    },
    Sample {
        region: 0,
        index: 11,
        location_id: 26_381,
        blocks: 5,
        extent: [-1, 1, -1, 1],
        start_block: "M0000001.RDB",
        enter: true,
        doors: 34,
        removed: 0,
    },
    Sample {
        region: 0,
        index: 16,
        location_id: 26_536,
        blocks: 5,
        extent: [-1, 1, -1, 1],
        start_block: "M0000000.RDB",
        enter: true,
        doors: 26,
        removed: 0,
    },
];

/// Census of one full pass.
struct Pass {
    maps_bytes: usize,
    blocks_bytes: usize,
    blocks_total: usize,
    doors_total: usize,
    doors_removed: usize,
    dungeons_with_dup: usize,
    start_markers: usize,
    enter_markers: usize,
    missing_blocks: usize,
    unparsed_blocks: usize,
    sample: Vec<String>,
    orsinium: Option<String>,
    digest: u64,
    elapsed_ms: u128,
}

/// FNV-1a 64 over the canonical grid of every dungeon, in index order.
fn run_pass(source: &RetailSource, dungeons: &[RetailDungeon]) -> Pass {
    let started = Instant::now();
    let mut digest = rdi::fnv1a64(b"");
    let mut pass = Pass {
        maps_bytes: source.maps_bytes(),
        blocks_bytes: source.blocks_bytes(),
        blocks_total: 0,
        doors_total: 0,
        doors_removed: 0,
        dungeons_with_dup: 0,
        start_markers: 0,
        enter_markers: 0,
        missing_blocks: 0,
        unparsed_blocks: 0,
        sample: Vec::new(),
        orsinium: None,
        digest: 0,
        elapsed_ms: 0,
    };
    for dungeon in dungeons {
        let grid = source.assemble(&dungeon.layout);
        pass.blocks_total += grid.blocks.len();
        pass.doors_total += grid.doors.len();
        pass.doors_removed += grid.doors_removed;
        if grid.doors_removed > 0 {
            pass.dungeons_with_dup += 1;
        }
        if grid.start_marker.is_some() {
            pass.start_markers += 1;
        }
        if grid.enter_marker.is_some() {
            pass.enter_markers += 1;
        }
        pass.missing_blocks += grid.missing_blocks.len();
        pass.unparsed_blocks += grid.unparsed_blocks.len();
        // Digest (region, index, LocationId) + the canonical grid: no
        // permutation collision, no clock/RNG/hash input.
        let key = format!(
            "{}:{}:{}:",
            dungeon.region_index, dungeon.location_index, dungeon.location_id
        );
        digest = rdi::fnv1a64_update(digest, key.as_bytes());
        digest = rdi::fnv1a64_update(digest, &grid.canonical_bytes());

        if pass.sample.len() < SAMPLE.len() {
            let expected = &SAMPLE[pass.sample.len()];
            let start_block = dungeon
                .layout
                .blocks
                .iter()
                .find(|b| b.is_starting_block)
                .map(|b| b.name.as_str())
                .unwrap_or("<none>");
            let extent_ok = [grid.min_x, grid.max_x, grid.min_z, grid.max_z] == expected.extent;
            let row_ok = extent_ok
                && dungeon.region_index == expected.region
                && dungeon.location_index == expected.index
                && dungeon.location_id == expected.location_id
                && grid.blocks.len() == expected.blocks
                && start_block == expected.start_block
                && grid.enter_marker.is_some() == expected.enter
                && grid.doors.len() == expected.doors
                && grid.doors_removed == expected.removed;
            pass.sample.push(format!(
                "sample[{}] region={:03} idx={} location_id={} blocks={} extent=({},{},{},{}) \
                 start={} enter={} doors={} removed={} ⇒ {}",
                pass.sample.len(),
                dungeon.region_index,
                dungeon.location_index,
                dungeon.location_id,
                grid.blocks.len(),
                grid.min_x,
                grid.max_x,
                grid.min_z,
                grid.max_z,
                start_block,
                grid.enter_marker.is_some(),
                grid.doors.len(),
                grid.doors_removed,
                if row_ok { "PASS" } else { "FAIL" }
            ));
        }

        // Orsinium fix (`DFU:API/MapsFile.cs:1339-1341`): the only record
        // with header LocationId 50015 must place block 13 at Z = −2.
        if dungeon.layout.record.location_id == 50_015 && pass.orsinium.is_none() {
            let block13 = dungeon.layout.blocks.get(13);
            let ok = dungeon.layout.blocks.len() == 16
                && block13.map(|b| (b.x, b.z, b.name.as_str())) == Some((-1, -2, "B0000003.RDB"));
            pass.orsinium = Some(format!(
                "orsinium region={:03} idx={} location_id={} blocks={} block13={:?} ⇒ {}",
                dungeon.region_index,
                dungeon.location_index,
                dungeon.layout.record.location_id,
                dungeon.layout.blocks.len(),
                block13.map(|b| (b.x, b.z, b.name.clone())),
                if ok { "PASS" } else { "FAIL" }
            ));
        }
    }
    pass.digest = digest;
    pass.elapsed_ms = started.elapsed().as_millis();
    pass
}

/// Runs `PROBE-DUNGEON-001`.
pub fn run_dungeon_001() -> i32 {
    println!("PROBE-DUNGEON-001 — MAPDITEM dungeon grid (M34-D / G-N1), geometry only");
    println!(
        "contract=docs/sintergic/game/programming/m34d-dungeon-grid-spec-CR-2d5c8f41.md \
         · cells=51.2 m · door tolerance=1.4 m"
    );

    let source = match RetailSource::load() {
        Ok(source) => source,
        Err(reason) => {
            println!("ARENA2 not accessible: {reason}");
            println!("EXIT=2 (retail source absent — never a false PASS or silent skip)");
            return 2;
        }
    };
    let (census, dungeons) = match source.dungeons() {
        Ok(pair) => pair,
        Err(reason) => {
            println!("FAIL DUNGEON-001 index - {reason}");
            println!("SUMMARY PROBE-DUNGEON-001: passed=0/0 ⇒ FAIL EXIT=1");
            return 1;
        }
    };

    let first = run_pass(&source, &dungeons);
    let second = run_pass(&source, &dungeons);

    println!(
        "index regions={} locations={} dungeons={} without_dungeon={} \
         maps={} B blocks={} B",
        census.regions,
        census.locations,
        census.dungeons,
        census.without_dungeon,
        first.maps_bytes,
        first.blocks_bytes
    );
    println!(
        "grid blocks={} doors={} removed={} with_dup={} start_markers={} \
         enter_markers={} missing_blocks={} unparsed_blocks={}",
        first.blocks_total,
        first.doors_total,
        first.doors_removed,
        first.dungeons_with_dup,
        first.start_markers,
        first.enter_markers,
        first.missing_blocks,
        first.unparsed_blocks
    );
    for row in &first.sample {
        println!("{row}");
    }
    if let Some(row) = &first.orsinium {
        println!("{row}");
    } else {
        println!("orsinium missing (expected region 026 idx 18, LocationId 50015) ⇒ FAIL");
    }

    let deterministic = first.digest == second.digest;
    println!(
        "\n-- determinism (full index ×2, same process) --\ndigest1={:016x} digest2={:016x} identical={deterministic}",
        first.digest, second.digest
    );
    println!(
        "DIGEST={:016x} (compare across two separate processes)",
        first.digest
    );
    println!(
        "elapsed pass1={} ms pass2={} ms",
        first.elapsed_ms, second.elapsed_ms
    );

    let checks: [(&str, bool); 12] = [
        ("dungeons", census.dungeons == EXPECTED_DUNGEONS),
        (
            "without_dungeon",
            census.without_dungeon == EXPECTED_WITHOUT_DUNGEON,
        ),
        ("locations", census.locations == 15_251),
        ("blocks_resolved", first.blocks_total > 0 && first.missing_blocks == 0),
        ("blocks_parsed", first.unparsed_blocks == 0),
        ("doors_removed", first.doors_removed == EXPECTED_DOORS_REMOVED),
        (
            "dungeons_with_dup",
            first.dungeons_with_dup == EXPECTED_DUNGEONS_WITH_DUP,
        ),
        ("start_markers", first.start_markers == EXPECTED_START_MARKERS),
        ("enter_markers", first.enter_markers == EXPECTED_ENTER_MARKERS),
        (
            "sample",
            first.sample.len() == SAMPLE.len() && first.sample.iter().all(|r| r.ends_with("PASS")),
        ),
        (
            "orsinium",
            first.orsinium.as_deref().is_some_and(|r| r.ends_with("PASS")),
        ),
        ("determinism", deterministic),
    ];
    let passed = checks.iter().filter(|(_, ok)| *ok).count();
    let failed = checks.len() - passed;
    for (name, ok) in checks {
        println!("{} {name}", if ok { "PASS" } else { "FAIL" });
    }
    let exit = if failed == 0 { 0 } else { 1 };
    println!(
        "\nSUMMARY PROBE-DUNGEON-001: passed={passed}/{} ⇒ {} EXIT={exit}",
        checks.len(),
        if exit == 0 { "PASS" } else { "FAIL" }
    );
    exit
}
