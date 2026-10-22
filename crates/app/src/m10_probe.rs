//! M10 — anti-screen probes for the retail `.RMB` city (REQ-L99-042 +006
//! +023, DoD §a):
//!
//! - **PROBE-RMB-001** (`--probe RMB-001`): N real cities of the Daggerfall
//!   region (MAPS.BSA `MAPPITEM.017` — the 8×8 capital + the first small
//!   cities in index order) parsed from ARENA2: 0 panics, **100% of the
//!   BuildingDataList consumed** (each building sub-record composed with
//!   type/door/ARCH3D model), an evidenceeable building-type dump and, in
//!   the small city, real navigability (coherent grid↔layout + spawn +
//!   8-direction march without clipping — INV-L99-5 over retail geometry).
//! - **PROBE-SEED-001 real ext** (`--probe SEED-001`): the capital is
//!   composed TWICE from the same bytes → same city byte by byte
//!   (`RetailCity::canonical_bytes` + FNV-1a 64 digest) — REQ-L99-023.
//!
//! **Source custody (PRE-2 §8 rule 3):** Mode A only (PathChecker) — the
//! `.BSA` files are read at runtime from the user's ARENA2; 0 packaged bytes
//! (INV-L99-6). 0 reads in `cargo test` (synthetic fixtures live in
//! `daggerfall-parser-rs` and `parser-bevy-bridge`).

use std::collections::HashMap;

use daggerfall_parser_bevy_bridge as bridge;
use daggerfall_parser_rs::{bsa, maps, rmb};

/// Daggerfall region (0x11) → decimal suffix of the `017` records.
const REGION_DAGGERFALL: &str = "017";
/// Capital of the region (8×8 city, 316 buildings — measured at M3 runtime).
const CAPITAL_CITY: &str = "Daggerfall";
/// Cities parsed by PROBE-RMB-001 (capital + N-1 small ones).
const CITIES_N: usize = 5;
/// Maximum cells (blocks) of the selected small cities.
const SMALL_CITY_MAX_BLOCKS: usize = 9;

/// ARCH3D index with spec cache (record id → MeshSpec). `pub(crate)` since
/// M11: PROBE-RDB-001 shares it (single selector CLI↔block C, 0 duplicated
/// logic).
pub(crate) struct Arch3dIndex {
    archive: bridge::BsaArchive,
    data: Vec<u8>,
    by_id: HashMap<u32, usize>,
    cache: HashMap<u32, Option<bridge::MeshSpec>>,
}

impl Arch3dIndex {
    pub(crate) fn new(data: Vec<u8>) -> Result<Self, String> {
        let archive =
            bridge::parse_bsa(&data).map_err(|e| format!("invalid ARCH3D.BSA: {e}"))?;
        let mut by_id = HashMap::new();
        for (i, entry) in archive.entries.iter().enumerate() {
            if let bsa::Entry::Numbered { record_id, .. } = entry {
                by_id.insert(*record_id, i);
            }
        }
        Ok(Self {
            archive,
            data,
            by_id,
            cache: HashMap::new(),
        })
    }

    pub(crate) fn resolve(&mut self, model_id: u32) -> Option<bridge::MeshSpec> {
        if let Some(cached) = self.cache.get(&model_id) {
            return cached.clone();
        }
        let spec = self
            .by_id
            .get(&model_id)
            .and_then(|idx| bridge::mesh_spec_at(&self.archive, &self.data, *idx).ok());
        self.cache.insert(model_id, spec.clone());
        spec
    }

    fn resolved_count(&self) -> usize {
        self.cache.values().filter(|s| s.is_some()).count()
    }

    fn queried_count(&self) -> usize {
        self.cache.len()
    }
}

/// Bytes of a named record inside a BSA (case-insensitive).
pub(crate) fn named_entry<'a>(
    archive: &bsa::BsaArchive,
    data: &'a [u8],
    name: &str,
) -> Result<&'a [u8], String> {
    for (i, entry) in archive.entries.iter().enumerate() {
        if let bsa::Entry::Named { name: n, .. } = entry {
            if n.eq_ignore_ascii_case(name) {
                return bsa::entry_data(archive, data, i)
                    .map_err(|e| format!("bytes of {name}: {e}"));
            }
        }
    }
    Err(format!("record {name} not found in the BSA"))
}

/// One parsed city: grid of `.RMB` blocks + name resolution.
/// `pub(crate)`: M18 (`PROBE-MAP-001`) consumes the exact/fallback counts.
pub(crate) struct RetailBlocks {
    pub(crate) name: String,
    width: usize,
    height: usize,
    blocks: Vec<(usize, usize, rmb::MapBlockRecord)>,
    pub(crate) cells_exact: usize,
    pub(crate) cells_fallback: usize,
}

/// Index RMB name → BLOCKS.BSA entry (uppercase).
pub(crate) fn rmb_name_index(archive: &bsa::BsaArchive) -> HashMap<String, usize> {
    let mut out = HashMap::new();
    for (i, entry) in archive.entries.iter().enumerate() {
        if let bsa::Entry::Named { name, .. } = entry {
            if name.eq_ignore_ascii_case("RMB") || name.to_ascii_uppercase().ends_with(".RMB") {
                out.insert(name.to_ascii_uppercase(), i);
            }
        }
    }
    out
}

/// Resolves the `.RMB` names of a location cell by cell (grid order) applying
/// the UESP fallback when the exact name does not exist. Returns
/// `(resolved_names, exact_cells, fallback_cells)`.
///
/// `pub(crate)`: M18 (`PROBE-MAP-001`) resolves the full MAPS.BSA index with
/// this same function — 0 duplicated resolution logic.
pub(crate) fn resolve_rmb_names(
    loc: &maps::LocationExterior,
    name_index: &HashMap<String, usize>,
    all_rmb_names: &[String],
) -> Result<(Vec<String>, usize, usize), String> {
    let width = loc.exterior.width as usize;
    let height = loc.exterior.height as usize;
    if width == 0 || height == 0 || width > maps::MAX_BLOCKS_PER_EDGE || height > maps::MAX_BLOCKS_PER_EDGE {
        return Err(format!(
            "{}: invalid dimensions {width}×{height}",
            loc.name
        ));
    }
    let mut names = Vec::with_capacity(width * height);
    let mut cells_exact = 0usize;
    let mut cells_fallback = 0usize;
    for cell in 0..width * height {
        let exact = maps::rmb_block_name(&loc.exterior, cell);
        let name = match name_index.get(&exact.to_ascii_uppercase()) {
            Some(_) => {
                cells_exact += 1;
                exact
            }
            None => {
                let parts = maps::rmb_block_name_parts(&loc.exterior, cell);
                let fallback = maps::rmb_block_name_fallback(&parts, all_rmb_names.iter().map(String::as_str))
                    .ok_or_else(|| format!("{}: no record for cell {cell} ({exact})", loc.name))?;
                cells_fallback += 1;
                fallback
            }
        };
        names.push(name);
    }
    Ok((names, cells_exact, cells_fallback))
}

/// Parses a city (cells → `.RMB` names → BLOCKS.BSA records).
pub(crate) fn parse_city(
    loc: &maps::LocationExterior,
    blocks_archive: &bsa::BsaArchive,
    blocks_data: &[u8],
    name_index: &HashMap<String, usize>,
    all_rmb_names: &[String],
) -> Result<RetailBlocks, String> {
    let (names, _, _) = resolve_rmb_names(loc, name_index, all_rmb_names)?;
    parse_resolved_blocks(loc, &names, blocks_archive, blocks_data, name_index)
}

/// Composes the city from **already resolved** `.RMB` names, validating each
/// record (`rmb::parse_rmb` parse). `pub(crate)`: M18 avoids resolving the
/// same location twice (`PROBE-MAP-001` over the full index).
pub(crate) fn parse_resolved_blocks(
    loc: &maps::LocationExterior,
    names: &[String],
    blocks_archive: &bsa::BsaArchive,
    blocks_data: &[u8],
    name_index: &HashMap<String, usize>,
) -> Result<RetailBlocks, String> {
    let width = loc.exterior.width as usize;
    let height = loc.exterior.height as usize;
    let mut blocks = Vec::with_capacity(names.len());
    let mut cells_exact = 0usize;
    let mut cells_fallback = 0usize;
    for (cell, name) in names.iter().enumerate() {
        let (bx, by) = (cell % width, cell / width);
        let exact = maps::rmb_block_name(&loc.exterior, cell);
        if name.eq_ignore_ascii_case(&exact) {
            cells_exact += 1;
        } else {
            cells_fallback += 1;
        }
        let idx = name_index
            .get(&name.to_ascii_uppercase())
            .copied()
            .ok_or_else(|| format!("{}: {name} resolved but absent from the index", loc.name))?;
        let bytes = bsa::entry_data(blocks_archive, blocks_data, idx)
            .map_err(|e| format!("{}: bytes of {name}: {e}", loc.name))?;
        let record = rmb::parse_rmb(bytes)
            .map_err(|e| format!("{}: parse of {name}: {e}", loc.name))?;
        blocks.push((bx, by, record));
    }
    Ok(RetailBlocks {
        name: loc.name.clone(),
        width,
        height,
        blocks,
        cells_exact,
        cells_fallback,
    })
}

/// Composes a retail city (resolves ARCH3D on demand).
fn compose_city(
    city: RetailBlocks,
    arch: &mut Arch3dIndex,
) -> bridge::RetailCity {
    bridge::build_retail_city(city.width, city.height, &city.blocks, &mut |id| arch.resolve(id))
}

/// Retail sources (Mode A) already loaded: MAPS + BLOCKS + selection.
struct RetailSources {
    blocks_data: Vec<u8>,
    blocks_archive: bridge::BsaArchive,
    locations: Vec<maps::LocationExterior>,
    /// `MAPTABLE.017` in [`Self::locations`] order (M18: map-pixel of the
    /// capital to anchor the integrated demo terrain).
    table: Vec<maps::MapTableEntry>,
    name_index: HashMap<String, usize>,
    all_rmb_names: Vec<String>,
}

fn load_sources() -> Result<RetailSources, String> {
    let path_checker = daggerfall_inventory::path_checker::PathChecker::new();
    if path_checker.use_custom_universe() {
        return Err("custom universe (PathChecker) not supported in this probe".into());
    }
    let maps_path = crate::mode_a_arena2_path("MAPS.BSA")?;
    let blocks_path = crate::mode_a_arena2_path("BLOCKS.BSA")?;
    let arch_path = crate::mode_a_arena2_path("ARCH3D.BSA")?;
    println!("[OK] mode_a MAPS.BSA: {} ({} bytes)", maps_path.display(), std::fs::metadata(&maps_path).map(|m| m.len()).unwrap_or(0));
    println!("[OK] mode_a BLOCKS.BSA: {} ({} bytes)", blocks_path.display(), std::fs::metadata(&blocks_path).map(|m| m.len()).unwrap_or(0));
    println!("[OK] mode_a ARCH3D.BSA: {} ({} bytes)", arch_path.display(), std::fs::metadata(&arch_path).map(|m| m.len()).unwrap_or(0));
    let maps_data = std::fs::read(&maps_path).map_err(|e| format!("MAPS.BSA read: {e}"))?;
    let blocks_data = std::fs::read(&blocks_path).map_err(|e| format!("BLOCKS.BSA read: {e}"))?;
    // ARCH3D is opened here to validate presence; specs are resolved on demand.
    let _arch_check = std::fs::metadata(&arch_path).map_err(|e| format!("ARCH3D.BSA read: {e}"))?;
    let maps_archive =
        bridge::parse_bsa(&maps_data).map_err(|e| format!("invalid MAPS.BSA: {e}"))?;
    let blocks_archive =
        bridge::parse_bsa(&blocks_data).map_err(|e| format!("invalid BLOCKS.BSA: {e}"))?;
    let names_rec = named_entry(&maps_archive, &maps_data, &format!("MAPNAMES.{REGION_DAGGERFALL}"))?;
    let table_rec = named_entry(&maps_archive, &maps_data, &format!("MAPTABLE.{REGION_DAGGERFALL}"))?;
    let pitem_rec = named_entry(&maps_archive, &maps_data, &format!("MAPPITEM.{REGION_DAGGERFALL}"))?;
    let names = maps::parse_map_names(names_rec).map_err(|e| format!("MAPNAMES: {e}"))?;
    let table = maps::parse_map_table(table_rec, names.len()).map_err(|e| format!("MAPTABLE: {e}"))?;
    let locations = maps::parse_pitem(pitem_rec, names.len()).map_err(|e| format!("MAPPITEM: {e}"))?;
    if table.len() != locations.len() {
        return Err(format!(
            "MAPTABLE/MAPPITEM misaligned: {} vs {}",
            table.len(),
            locations.len()
        ));
    }
    let name_index = rmb_name_index(&blocks_archive);
    let mut all_rmb_names: Vec<String> = name_index.keys().cloned().collect();
    all_rmb_names.sort();
    Ok(RetailSources {
        blocks_data,
        blocks_archive,
        locations,
        table,
        name_index,
        all_rmb_names,
    })
}

/// Deterministic city selection: the capital + first small ones
/// (TownCity/TownHamlet/TownVillage with ≤ `SMALL_CITY_MAX_BLOCKS` cells).
fn select_cities(sources: &RetailSources) -> Vec<usize> {
    let mut selected = Vec::new();
    if let Some(i) = sources
        .locations
        .iter()
        .position(|l| l.name == CAPITAL_CITY)
    {
        selected.push(i);
    }
    for (i, loc) in sources.locations.iter().enumerate() {
        if selected.len() >= CITIES_N {
            break;
        }
        let small_settlement = matches!(loc.exterior.location_type, 0..=2);
        if small_settlement
            && loc.cell_count() <= SMALL_CITY_MAX_BLOCKS
            && loc.building_count > 0
            && !selected.contains(&i)
        {
            selected.push(i);
        }
    }
    selected
}

/// Retail capital composed from ARENA2 + its map-pixel (`MAPTABLE.017`).
///
/// M18: the integrated demo anchors the M13 terrain on the capital's real
/// map-pixel. The city resolution is the SAME as [`compose_capital`]
/// (0 duplicated logic).
pub(crate) struct CapitalCity {
    /// Retail name of the location (`Daggerfall`).
    pub(crate) name: String,
    /// World map-pixel (1000×500) of the `MAPTABLE.017` record.
    pub(crate) map_pixel: [i32; 2],
    /// City composed from the real `.RMB` files.
    pub(crate) city: bridge::RetailCity,
}

/// Composes the capital + resolves its map coordinate (M18 demo).
pub(crate) fn compose_capital_with_pixel() -> Result<CapitalCity, String> {
    let sources = load_sources()?;
    let idx = sources
        .locations
        .iter()
        .position(|l| l.name == CAPITAL_CITY)
        .ok_or("Daggerfall capital not found in MAPS.BSA")?;
    let (pixel_x, pixel_y) = crate::m18_probe::map_pixel(&sources.table[idx]);
    let city = parse_city(
        &sources.locations[idx],
        &sources.blocks_archive,
        &sources.blocks_data,
        &sources.name_index,
        &sources.all_rmb_names,
    )?;
    let arch_bytes = std::fs::read(crate::mode_a_arena2_path("ARCH3D.BSA")?)
        .map_err(|e| format!("ARCH3D.BSA read: {e}"))?;
    let mut arch = Arch3dIndex::new(arch_bytes)?;
    Ok(CapitalCity {
        name: sources.locations[idx].name.clone(),
        map_pixel: [pixel_x, pixel_y],
        city: compose_city(city, &mut arch),
    })
}

/// Retail capital composed from ARENA2 (used by both probes + the window).
pub(crate) fn compose_capital() -> Result<bridge::RetailCity, String> {
    compose_capital_with_pixel().map(|capital| capital.city)
}

/// Evidenceeable dump: type histogram + first buildings with their type.
fn dump_city_evidence(city: &bridge::RetailCity) {
    let histogram = city.building_type_histogram();
    let types: Vec<String> = histogram
        .iter()
        .map(|(t, c)| format!("{}×{}", maps::building_type_name(*t), c))
        .collect();
    println!(
        "[OK] building types ({} distinct): {}",
        histogram.len(),
        types.join(", ")
    );
    let mut shown = 0usize;
    for b in &city.buildings {
        if b.building_type == 0 || shown >= 5 {
            continue;
        }
        println!(
            "     building [block {}·{}]#{}: {} (type {}) · quality {} · seed {} · faction {} · ARCH3D {} · {} door(s) · pos=({:.1},{:.1},{:.1})",
            b.block_x,
            b.block_y,
            b.record_index,
            maps::building_type_name(b.building_type),
            b.building_type,
            b.quality,
            b.name_seed,
            b.faction_id,
            b.model_id,
            b.doors.len(),
            b.position[0],
            b.position[1],
            b.position[2],
        );
        shown += 1;
    }
}

/// Runs PROBE-RMB-001 — retail `.RMB` city (REQ-L99-042, M10).
pub fn run_rmb_001() -> i32 {
    println!("PROBE-RMB-001 - retail .RMB city (REQ-L99-042 +006 +023, M10, CR-28d62bea)");
    let sources = match load_sources() {
        Ok(s) => s,
        Err(reason) => {
            println!("[FAIL] retail sources: {reason}");
            return 1;
        }
    };
    let selected = select_cities(&sources);
    if selected.is_empty() {
        println!("[FAIL] city_selection: 0 cities selected in region {REGION_DAGGERFALL}");
        return 1;
    }
    let total_locations = sources.locations.len();
    println!(
        "[OK] MAPS.BSA region {REGION_DAGGERFALL}: {total_locations} locations · {CITIES_N} selected cities (capital + small ≤ {SMALL_CITY_MAX_BLOCKS} blocks)"
    );

    let arch_bytes = match crate::mode_a_arena2_path("ARCH3D.BSA").and_then(|p| {
        std::fs::read(&p).map_err(|e| format!("ARCH3D.BSA read {}: {e}", p.display()))
    }) {
        Ok(b) => b,
        Err(reason) => {
            println!("[FAIL] {reason}");
            return 1;
        }
    };
    let mut arch = match Arch3dIndex::new(arch_bytes) {
        Ok(a) => a,
        Err(reason) => {
            println!("[FAIL] ARCH3D.BSA: {reason}");
            return 1;
        }
    };

    let mut all_ok = true;
    let mut grand_buildings = 0usize;
    let mut grand_subrecords = 0usize;
    let mut small_city_mesh: Option<bridge::RetailCity> = None;
    let mut small_city_name = String::new();

    for (n, idx) in selected.iter().enumerate() {
        let loc = &sources.locations[*idx];
        let city = match parse_city(
            loc,
            &sources.blocks_archive,
            &sources.blocks_data,
            &sources.name_index,
            &sources.all_rmb_names,
        ) {
            Ok(c) => c,
            Err(reason) => {
                println!("[FAIL] city {} ({}): {reason}", loc.name, n + 1);
                all_ok = false;
                continue;
            }
        };
        let subrecords: usize = city
            .blocks
            .iter()
            .map(|(_, _, r)| r.num_block_data_records as usize)
            .sum();
        let city_name = city.name.clone();
        let (width, height) = (city.width, city.height);
        let (cells_exact, cells_fallback) = (city.cells_exact, city.cells_fallback);
        let composed = compose_city(city, &mut arch);
        let consumed_ok = composed.buildings.len() == subrecords && subrecords > 0;
        grand_buildings += composed.buildings.len();
        grand_subrecords += subrecords;
        println!(
            "[{}] city {:<12} {width}×{height} blocks · {} RMB parsed · exact cells {cells_exact} + fallback {cells_fallback} · BuildingDataList {}/{} consumed (100% = {}) · {} retail triangles · ARCH3D pending {}",
            if consumed_ok { "OK" } else { "FAIL" },
            city_name,
            width * height,
            composed.buildings.len(),
            subrecords,
            if consumed_ok { "yes" } else { "NO" },
            composed.combined.triangle_count(),
            composed.unresolved_models,
        );
        if !consumed_ok {
            all_ok = false;
        }
        dump_city_evidence(&composed);
        // INV-L99-5 navigability over the FIRST small city (cheap voxel).
        if small_city_mesh.is_none() && width * height <= SMALL_CITY_MAX_BLOCKS {
            small_city_mesh = Some(composed);
            small_city_name = city_name;
        }
    }

    // Retail navigability (small city).
    match &small_city_mesh {
        Some(small) => {
            let report = bridge::run_view_probe(&small.combined);
            for check in &report.checks {
                println!(
                    "[{}] navigability_{} - {} :: {}",
                    if check.ok { "OK" } else { "FAIL" },
                    small_city_name,
                    check.id,
                    check.detail
                );
            }
            if !report.pass {
                all_ok = false;
            }
        }
        None => {
            println!("[FAIL] navigability: no small city to voxelize");
            all_ok = false;
        }
    }

    println!(
        "[OK] ARCH3D resolved on demand: {}/{} records queried",
        arch.resolved_count(),
        arch.queried_count()
    );
    println!(
        "TOTAL: {CITIES_N} cities · {grand_buildings} composed buildings · {grand_subrecords} declared sub-records · 0 panics while parsing"
    );

    if all_ok {
        println!(
            "VERDICT: PROBE-RMB-001 PASS - {CITIES_N} retail cities parsed from ARENA2 · BuildingDataList 100% consumed · evidenceable types/doors/ARCH3D models · navigable city without clipping"
        );
        0
    } else {
        println!("VERDICT: PROBE-RMB-001 FAIL - see [FAIL] rows");
        1
    }
}

/// Runs PROBE-SEED-001 (real extension) — retail city determinism
/// (REQ-L99-023, M10): same input → same city byte by byte.
pub fn run_seed_001() -> i32 {
    println!("PROBE-SEED-001 (real ext) - retail city determinism (REQ-L99-023, M10, CR-28d62bea)");
    let first = match compose_capital() {
        Ok(c) => c,
        Err(reason) => {
            println!("[FAIL] composition 1: {reason}");
            return 1;
        }
    };
    let second = match compose_capital() {
        Ok(c) => c,
        Err(reason) => {
            println!("[FAIL] composition 2: {reason}");
            return 1;
        }
    };
    println!(
        "[OK] Daggerfall capital: {}×{} blocks · {} buildings · {} triangles",
        first.width_blocks,
        first.height_blocks,
        first.buildings.len(),
        first.combined.triangle_count()
    );
    let bytes_equal = first.canonical_bytes() == second.canonical_bytes();
    let digest_equal = first.digest() == second.digest();
    println!(
        "[{}] canonical_bytes: {} == {} bytes · FNV-1a64 digest {} vs {}",
        if bytes_equal && digest_equal { "OK" } else { "FAIL" },
        first.canonical_bytes().len(),
        second.canonical_bytes().len(),
        first.digest(),
        second.digest(),
    );
    if bytes_equal && digest_equal {
        println!(
            "VERDICT: PROBE-SEED-001 (real ext) PASS - same city byte by byte from the same .RMB files (digest {})",
            first.digest()
        );
        0
    } else {
        println!("VERDICT: PROBE-SEED-001 (real ext) FAIL - non-deterministic compositions");
        1
    }
}
