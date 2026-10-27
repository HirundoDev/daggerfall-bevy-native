//! PROBE-AUTOMAP-001 — automapa de mazmorra y de ciudad (M34/W34-5; spec
//! `docs/sintergic/game/programming/m34-e2e-integration-presentation-spec-CR-5243af63.md`
//! §1.5).
//!
//! **What it proves:** the presentation automap of every retail dungeon is
//! built from the **real** block grid closed in W34-D
//! ([`crate::interior_retail::RetailSource::assemble`] over `MAPDITEM` +
//! `BLOCKS.BSA`) and then round-trips through the **classic
//! `0x33 DungeonAutomapData`** projection of savegame v2 row 11
//! (`DungeonAutomapData::from_v2` / `to_v2`) without touching the M33 key
//! format (`automap.<i>=<location>,<blocks…>`).
//!
//! State model (`DFU-UP:Assets/Scripts/Game/Automap.cs:66-78`,
//! `AutomapModel.cs:22,46-100`):
//! `discovered` = block present in the persisted automap record;
//! `visited_in_this_run` = block visited since the dungeon was entered →
//! `enabled = discovered`, `RENDER_IN_GRAYSCALE = discovered && !visited`.
//!
//! **Declared delta (no invented gameplay):** the retail index holds no player
//! history, so the persisted record of each location is the only block the
//! data identifies as known — its **starting block** (`IsStartingBlock`). The
//! probe therefore reports `blocks_descubiertos` for the state "the player has
//! entered the dungeon once": 1 discovered block per location, the rest
//! undiscovered. `visited_in_this_run` is that same block (the run begins in
//! the starting block). Notes and teleporter connections are user-authored
//! (`PlayerNotebook`), so the retail pass reports `notes=0 teleports=0`; their
//! round-trip is covered by the memory unit tests.
//!
//! Exterior nameplates (`ExteriorAutomap.cs:59-76,116,1365`): depth **4.0**
//! for every plate and a hidden plate rasterised as `*`. **Declared delta:**
//! DFU detects the collision from screen-space label overlap; this engine has
//! no label layout yet, so a collision is two nameplates of the same map with
//! the same name (see `perception::read_models::ExteriorAutomapReadModel`).
//!
//! **Numbers are measured, never adjusted:** the goldens come from the
//! two-process run on the canonical ARENA2 (`$HOME/Games/Daggerfall/app`,
//! `DF_APP_ROOT` overrides). A mismatch is a FAIL and the measured value is
//! printed next to the expected one.
//!
//! `EXIT=2` when ARENA2 (`MAPS.BSA`/`BLOCKS.BSA`) is not accessible — never a
//! silent skip. `EXIT=1` on any failed check or on a digest mismatch between
//! the two in-process passes (the cross-process digest is compared by the
//! operator: run the probe twice and compare `DIGEST=`).
//!
//! **Scope:** geometry/presentation only — no formulas, no AI, no loot.

use std::collections::BTreeMap;
use std::time::Instant;

use daggerfall_memory::savegame_v2::{
    AutomapExtV2, AutomapV2, DungeonAutomapData, MapSection, SaveGameV2,
    DUNGEON_AUTOMAP_RECORD_ID,
};
use daggerfall_parser_bevy_bridge::dungeon_grid::DungeonGrid;
use daggerfall_parser_rs::rdi;
use daggerfall_parser_rs::{bsa, maps};
use daggerfall_perception::read_models::{
    BlockMaterial, BlockSource, DungeonAutomapReadModel, DungeonAutomapSource,
    ExteriorAutomapReadModel, ExteriorAutomapSource, NameplateSource, ASCII_MAX_SIDE,
    NAMEPLATES_PLACEMENT_DEPTH,
};

use crate::interior_retail::{DungeonCensus, RetailDungeon, RetailSource};

/// Dungeons of the retail `MAPDITEM` index (measured; UESP declares 4,232).
const EXPECTED_DUNGEONS: usize = 4_232;
/// Automap rows: one per dungeon location (the row key is `LocationId`).
const EXPECTED_AUTOMAP_COUNT: usize = 4_232;
/// Persisted discovered blocks when every location has been entered once:
/// exactly its starting block (measured: every dungeon has one).
const EXPECTED_BLOCKS_DISCOVERED: usize = 4_232;
/// Dungeons whose starting block carries an `Enter` flat (measured 18.3 %;
/// DFU leaves `enterMarker` null for the rest) — the same value PROBE-DUNGEON-001
/// closes, re-measured here through the automap projection.
const EXPECTED_ENTER_MARKERS: usize = 775;

/// Location-name index of every region (`MAPNAMES.<nnn>`): the input of the
/// **exterior** automap. This is not the dungeon index — it enumerates every
/// location of the region (towns, temples, graves, dungeons), which is exactly
/// what a city map labels. Custody is the same Mode A path as
/// [`RetailSource::load`] (0 packaged bytes).
fn name_index() -> Result<Vec<(u16, Vec<String>)>, String> {
    let path = daggerfall_app::mode_a::arena2_path("MAPS.BSA")?;
    let data = std::fs::read(&path).map_err(|e| format!("{} ({e})", path.display()))?;
    let archive = bsa::parse(&data).map_err(|e| format!("MAPS.BSA: {e}"))?;
    let mut out: Vec<(u16, Vec<String>)> = Vec::new();
    for (region, suffix) in crate::m18_probe::region_suffixes(&archive) {
        let record = crate::m10_probe::named_entry(&archive, &data, &format!("MAPNAMES.{suffix}"))?;
        // Measured: some regions ship an empty `MAPNAMES.<nnn>` record (0 bytes,
        // e.g. region 002) — a region with no locations, not a parse failure.
        if record.is_empty() {
            continue;
        }
        let names = maps::parse_map_names(record)
            .map_err(|e| format!("MAPNAMES.{suffix}: {e}"))?;
        if names.is_empty() {
            continue;
        }
        out.push((region, names));
    }
    Ok(out)
}

/// One fixed sample row: the first five dungeon locations in index order.
struct Sample {
    region: u16,
    index: usize,
    location_id: u16,
    name: &'static str,
    blocks: usize,
    extent: [i8; 4],
    start_block: &'static str,
    discovered: usize,
    enter: bool,
}

/// Measured sample (spec §1.5; deterministic index order, no sampling).
const SAMPLE: [Sample; 5] = [
    Sample {
        region: 0,
        index: 2,
        location_id: 26_219,
        name: "Ruins of The Caorchom Farmstead",
        blocks: 12,
        extent: [-3, 1, -1, 2],
        start_block: "N0000006.RDB",
        discovered: 1,
        enter: false,
    },
    Sample {
        region: 0,
        index: 6,
        location_id: 26_278,
        name: "Ruins of The F'igta Farmstead",
        blocks: 12,
        extent: [-2, 1, -2, 2],
        start_block: "N0000050.RDB",
        discovered: 1,
        enter: false,
    },
    Sample {
        region: 0,
        index: 8,
        location_id: 26_310,
        name: "Maorn's Guard",
        blocks: 11,
        extent: [-1, 3, -1, 1],
        start_block: "W0000011.RDB",
        discovered: 1,
        enter: false,
    },
    Sample {
        region: 0,
        index: 11,
        location_id: 26_381,
        name: "The Tombs of Klerd",
        blocks: 5,
        extent: [-1, 1, -1, 1],
        start_block: "M0000001.RDB",
        discovered: 1,
        enter: true,
    },
    Sample {
        region: 0,
        index: 16,
        location_id: 26_536,
        name: "THe Crypts of Rhajom",
        blocks: 5,
        extent: [-1, 1, -1, 1],
        start_block: "M0000000.RDB",
        discovered: 1,
        enter: true,
    },
];

/// Census of one full pass.
struct Pass {
    automap_count: usize,
    duplicates_by_location: usize,
    blocks_placed: usize,
    blocks_discovered: usize,
    blocks_visited: usize,
    blocks_grayscale: usize,
    entrance_discovered: usize,
    ascii_rendered: usize,
    ascii_legend_only: usize,
    ascii_mismatches: usize,
    phantom_blocks: usize,
    subset_violations: usize,
    material_violations: usize,
    entrance_violations: usize,
    record_violations: usize,
    ext_rows: usize,
    plates: usize,
    plates_hidden: usize,
    plate_violations: usize,
    sample: Vec<String>,
    doc_bytes: usize,
    doc_lines: usize,
    doc_roundtrip: bool,
    digest: u64,
    elapsed_ms: u128,
}

/// Builds the automap source of one dungeon from its **real** grid: persisted
/// discovered block = its starting block (declared: the location has been
/// entered once), run-visited = that same block.
fn automap_source(dungeon: &RetailDungeon, grid: &DungeonGrid) -> DungeonAutomapSource {
    let start_index = dungeon
        .layout
        .blocks
        .iter()
        .position(|block| block.is_starting_block);
    let visited: Vec<(i32, i32)> = start_index
        .and_then(|index| grid.blocks.get(index))
        .map(|block| vec![(i32::from(block.x), i32::from(block.z))])
        .unwrap_or_default();
    DungeonAutomapSource {
        location: i64::from(dungeon.location_id),
        location_name: dungeon.location_name.clone(),
        time_in_seconds_last_visited: 0,
        entrance_discovered: grid.enter_marker.is_some(),
        blocks: grid
            .blocks
            .iter()
            .enumerate()
            .map(|(index, block)| BlockSource {
                x: i32::from(block.x),
                z: i32::from(block.z),
                discovered: Some(index) == start_index,
            })
            .collect(),
        visited_in_this_run: visited,
        note_markers: Vec::new(),
        teleporter_connections: Vec::new(),
    }
}

/// Persisted block indices of the same state (the `blocks` column of row 11).
fn persisted_blocks(source: &DungeonAutomapSource) -> Vec<i64> {
    source
        .blocks
        .iter()
        .enumerate()
        .filter(|(_, block)| block.discovered)
        .map(|(index, _)| index as i64)
        .collect()
}

/// One full pass over the retail index.
fn run_pass(
    source: &RetailSource,
    dungeons: &[RetailDungeon],
    census: &DungeonCensus,
) -> Result<Pass, String> {
    let started = Instant::now();
    let mut digest = rdi::fnv1a64(b"");
    let mut pass = Pass {
        automap_count: 0,
        duplicates_by_location: 0,
        blocks_placed: 0,
        blocks_discovered: 0,
        blocks_visited: 0,
        blocks_grayscale: 0,
        entrance_discovered: 0,
        ascii_rendered: 0,
        ascii_legend_only: 0,
        ascii_mismatches: 0,
        phantom_blocks: 0,
        subset_violations: 0,
        material_violations: 0,
        entrance_violations: 0,
        record_violations: 0,
        ext_rows: 0,
        plates: 0,
        plates_hidden: 0,
        plate_violations: 0,
        sample: Vec::new(),
        doc_bytes: 0,
        doc_lines: 0,
        doc_roundtrip: false,
        digest: 0,
        elapsed_ms: 0,
    };
    let mut map = MapSection::default();
    let mut seen_locations: BTreeMap<i64, usize> = BTreeMap::new();

    for dungeon in dungeons {
        let grid = source.assemble(&dungeon.layout);
        let automap = automap_source(dungeon, &grid);
        let model = DungeonAutomapReadModel::project(&automap);

        // ── state per block ────────────────────────────────────────────
        pass.blocks_placed += model.block_count();
        pass.blocks_discovered += model.blocks_discovered();
        pass.blocks_visited += model.blocks_visited();
        pass.blocks_grayscale += model.blocks_grayscale();
        if model.entrance_discovered == grid.enter_marker.is_some() {
            pass.entrance_discovered += usize::from(model.entrance_discovered);
        } else {
            pass.entrance_violations += 1;
        }
        // The projection is total over the grid and keeps the two flags
        // consistent: `enabled == discovered`, `grayscale == discovered &&
        // !visited`; a visited block is necessarily discovered.
        if model.block_count() != grid.blocks.len() {
            pass.subset_violations += 1;
        }
        for block in &model.blocks {
            let material = BlockMaterial::of(block);
            if material.enabled != block.discovered
                || material.grayscale != (block.discovered && !block.visited_in_this_run)
                || (block.visited_in_this_run && !block.discovered)
            {
                pass.material_violations += 1;
            }
        }

        // ── ASCII presentation (GV-16 budget) ──────────────────────────
        let rendered = model.render_ascii();
        match model.bounds() {
            Some(bounds) if bounds.width() > ASCII_MAX_SIDE || bounds.height() > ASCII_MAX_SIDE => {
                pass.ascii_legend_only += 1;
                if !rendered.starts_with("(bbox ") || rendered.contains('#') {
                    pass.ascii_mismatches += 1;
                }
            }
            Some(bounds) => {
                pass.ascii_rendered += 1;
                let lines: Vec<&str> = rendered.lines().collect();
                let width_ok = lines
                    .iter()
                    .all(|line| line.chars().count() == bounds.width() as usize);
                if lines.len() != bounds.height() as usize || !width_ok {
                    pass.ascii_mismatches += 1;
                }
            }
            None => pass.ascii_mismatches += 1,
        }

        // ── persistence: row 11 + classic 0x33 extension ───────────────
        let blocks = persisted_blocks(&automap);
        if blocks.iter().any(|index| *index < 0 || *index >= grid.blocks.len() as i64) {
            pass.phantom_blocks += 1;
        }
        let row = AutomapV2 {
            location: automap.location,
            blocks,
        };
        *seen_locations.entry(row.location).or_insert(0) += 1;
        let mut ext = AutomapExtV2 {
            location: automap.location,
            location_name: automap.location_name.clone(),
            time_in_seconds_last_visited: automap.time_in_seconds_last_visited,
            entrance_discovered: automap.entrance_discovered,
            note_markers: Vec::new(),
            teleporter_connections: Vec::new(),
        };
        ext.normalize();
        let record = DungeonAutomapData::from_v2(&row, Some(&ext));
        if record.record_id != DUNGEON_AUTOMAP_RECORD_ID
            || record.blocks != row.blocks
            || record.location_name != automap.location_name
            || record.entrance_discovered != automap.entrance_discovered
        {
            pass.record_violations += 1;
        }
        let (back_row, back_ext) = record.to_v2();
        let ext_ok = if ext.is_default() {
            back_ext.is_none()
        } else {
            back_ext.as_ref() == Some(&ext)
        };
        if back_row != row || !ext_ok {
            pass.record_violations += 1;
        }
        map.upsert_automap(row, Some(ext));

        // Digest: the record projection + the ASCII presentation, in index
        // order. No clock, no RNG, no hash-map iteration.
        let key = format!(
            "{}:{}:{}|{}|{}|{}|{}|{}\n",
            dungeon.region_index,
            dungeon.location_index,
            automap.location,
            automap.location_name,
            u8::from(automap.entrance_discovered),
            model.blocks_discovered(),
            model.blocks_visited(),
            rendered
        );
        digest = rdi::fnv1a64_update(digest, key.as_bytes());

        if pass.sample.len() < SAMPLE.len() {
            let expected = &SAMPLE[pass.sample.len()];
            let start_block = dungeon
                .layout
                .blocks
                .iter()
                .find(|block| block.is_starting_block)
                .map(|block| block.name.as_str())
                .unwrap_or("<none>");
            let bounds = model.bounds();
            let extent = bounds
                .map(|b| [b.min_x as i8, b.max_x as i8, b.min_z as i8, b.max_z as i8])
                .unwrap_or([0; 4]);
            let row_ok = dungeon.region_index == expected.region
                && dungeon.location_index == expected.index
                && dungeon.location_id == expected.location_id
                && dungeon.location_name == expected.name
                && grid.blocks.len() == expected.blocks
                && extent == expected.extent
                && start_block == expected.start_block
                && model.blocks_discovered() == expected.discovered
                && model.blocks_visited() == expected.discovered
                && model.entrance_discovered == expected.enter;
            pass.sample.push(format!(
                "sample[{}] region={:03} idx={} location_id={} name={:?} blocks={} \
                 discovered={} visited={} grayscale={} extent=({},{},{},{}) start={} enter={} ⇒ {}",
                pass.sample.len(),
                dungeon.region_index,
                dungeon.location_index,
                dungeon.location_id,
                dungeon.location_name,
                grid.blocks.len(),
                model.blocks_discovered(),
                model.blocks_visited(),
                model.blocks_grayscale(),
                extent[0],
                extent[1],
                extent[2],
                extent[3],
                start_block,
                model.entrance_discovered,
                if row_ok { "PASS" } else { "FAIL" }
            ));
        }
    }

    pass.automap_count = map.automaps.len();
    pass.duplicates_by_location = seen_locations.values().filter(|count| **count > 1).count();
    pass.ext_rows = map.automap_ext.len();

    // ── savegame v2 round-trip (M33 format untouched) ─────────────────
    let mut document = SaveGameV2::new();
    document.map = map;
    let text = document.to_text();
    pass.doc_bytes = text.len();
    pass.doc_lines = text.lines().count();
    pass.doc_roundtrip = match SaveGameV2::from_text(&text) {
        Ok(parsed) => parsed == document && parsed.to_text() == text,
        Err(_) => false,
    };
    match document.validate() {
        Ok(()) => {}
        Err(_) => pass.doc_roundtrip = false,
    }
    if document.line_count() != pass.doc_lines {
        pass.doc_roundtrip = false;
    }
    digest = rdi::fnv1a64_update(digest, text.as_bytes());

    // ── exterior nameplates (every region, real MAPNAMES index) ───────
    // Input = the full location-name index of the region (15,251 names), not
    // the dungeon subset: a city automap labels every location it contains.
    let name_regions = name_index()?;
    for (region, names) in &name_regions {
        let source = ExteriorAutomapSource {
            nameplates: names
                .iter()
                .enumerate()
                .map(|(index, name)| NameplateSource {
                    location_index: index,
                    name: name.clone(),
                })
                .collect(),
        };
        let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
        for plate in &source.nameplates {
            *counts.entry(plate.name.as_str()).or_insert(0) += 1;
        }
        let model = ExteriorAutomapReadModel::project(&source);
        pass.plates += model.nameplates.len();
        pass.plates_hidden += model.hidden_count();
        let mut previous = None;
        for (plate, input) in model.nameplates.iter().zip(&source.nameplates) {
            let duplicated = counts.get(input.name.as_str()).copied().unwrap_or(0) > 1;
            let expected_text = if duplicated { "*" } else { input.name.as_str() };
            if plate.depth.to_bits() != NAMEPLATES_PLACEMENT_DEPTH.to_bits()
                || plate.hidden != duplicated
                || plate.text != expected_text
                || plate.location_index != input.location_index
                || previous.is_some_and(|last| plate.location_index < last)
            {
                pass.plate_violations += 1;
            }
            previous = Some(plate.location_index);
            digest = rdi::fnv1a64_update(
                digest,
                format!("{}|{}|{}\n", region, plate.location_index, plate.text).as_bytes(),
            );
        }
    }

    // ── M33 key format is untouched ───────────────────────────────────
    // The first automap row must still be the M33 shape (`,<blocks…>`), and
    // the extension must live in its own family.
    let key_ok = text.contains("\nautomap.0=")
        && !text.contains("\nautomap.0.")
        && (pass.ext_rows == 0 || text.contains("\nautomap_ext_count="));
    if !key_ok {
        pass.doc_roundtrip = false;
    }

    pass.digest = digest;
    pass.elapsed_ms = started.elapsed().as_millis();
    // Census is consumed by the caller for the index checks; keep it explicit.
    debug_assert!(census.dungeons >= pass.automap_count);
    Ok(pass)
}

/// Runs `PROBE-AUTOMAP-001`.
pub fn run_automap_001() -> i32 {
    println!("PROBE-AUTOMAP-001 — dungeon + city automap (M34/W34-5), presentation only");
    println!(
        "contract=docs/sintergic/game/programming/m34-e2e-integration-presentation-spec-CR-5243af63.md §1.5 \
         · record=0x{DUNGEON_AUTOMAP_RECORD_ID:02X} DungeonAutomapData · ascii_max_side={ASCII_MAX_SIDE} \
         · nameplate_depth={NAMEPLATES_PLACEMENT_DEPTH}"
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
            println!("FAIL AUTOMAP-001 index - {reason}");
            println!("SUMMARY PROBE-AUTOMAP-001: passed=0/0 ⇒ FAIL EXIT=1");
            return 1;
        }
    };

    let (first, second) = match (run_pass(&source, &dungeons, &census), run_pass(&source, &dungeons, &census)) {
        (Ok(first), Ok(second)) => (first, second),
        (Err(reason), _) | (_, Err(reason)) => {
            println!("FAIL AUTOMAP-001 exterior - {reason}");
            println!("SUMMARY PROBE-AUTOMAP-001: passed=0/0 ⇒ FAIL EXIT=1");
            return 1;
        }
    };

    println!(
        "index regions={} locations={} dungeons={} · automap_count={} duplicates_by_location={} ext_rows={}",
        census.regions,
        census.locations,
        census.dungeons,
        first.automap_count,
        first.duplicates_by_location,
        first.ext_rows
    );
    println!(
        "blocks placed={} descubiertos={} visitados_en_esta_pasada={} grises={} entrance_discovered={} phantom={}",
        first.blocks_placed,
        first.blocks_discovered,
        first.blocks_visited,
        first.blocks_grayscale,
        first.entrance_discovered,
        first.phantom_blocks
    );
    println!(
        "ascii renderizados={} solo_leyenda={} mismatches={} (bbox > {ASCII_MAX_SIDE} ⇒ leyenda)",
        first.ascii_rendered, first.ascii_legend_only, first.ascii_mismatches
    );
    println!(
        "exterior nameplates={} (locations={}) ocultos={} depth={} violaciones={}",
        first.plates, census.locations, first.plates_hidden, NAMEPLATES_PLACEMENT_DEPTH, first.plate_violations
    );
    println!(
        "savegame v2 filas={} ext={} bytes={} lineas={} roundtrip={} (claves M33 intactas)",
        first.automap_count, first.ext_rows, first.doc_bytes, first.doc_lines, first.doc_roundtrip
    );
    for row in &first.sample {
        println!("{row}");
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

    let checks: [(&str, bool); 13] = [
        ("dungeons", census.dungeons == EXPECTED_DUNGEONS),
        ("automap_count", first.automap_count == EXPECTED_AUTOMAP_COUNT),
        (
            "automap_count_per_location",
            first.automap_count == census.dungeons && first.duplicates_by_location == 0,
        ),
        (
            "blocks_descubiertos",
            first.blocks_discovered == EXPECTED_BLOCKS_DISCOVERED,
        ),
        (
            "entrance_discovered",
            first.entrance_discovered == EXPECTED_ENTER_MARKERS,
        ),
        (
            "block_state_invariants",
            first.phantom_blocks == 0
                && first.subset_violations == 0
                && first.material_violations == 0
                && first.entrance_violations == 0,
        ),
        (
            "record_0x33_projection",
            first.record_violations == 0 && first.ext_rows == first.automap_count,
        ),
        (
            "ascii_budget",
            first.ascii_mismatches == 0
                && first.ascii_rendered + first.ascii_legend_only == first.automap_count,
        ),
        (
            "ascii_max_side",
            ASCII_MAX_SIDE == 24 && NAMEPLATES_PLACEMENT_DEPTH == 4.0,
        ),
        ("savegame_roundtrip", first.doc_roundtrip && first.doc_lines > 0),
        (
            "exterior_nameplates",
            first.plate_violations == 0 && first.plates == census.locations,
        ),
        (
            "sample",
            first.sample.len() == SAMPLE.len() && first.sample.iter().all(|row| row.ends_with("PASS")),
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
        "\nSUMMARY PROBE-AUTOMAP-001: passed={passed}/{} ⇒ {} EXIT={exit}",
        checks.len(),
        if exit == 0 { "PASS" } else { "FAIL" }
    );
    exit
}
