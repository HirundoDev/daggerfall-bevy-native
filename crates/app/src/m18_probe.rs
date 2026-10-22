//! M18 — `PROBE-MAP-001`: **full map coverage** of the `MAPS.BSA` index
//! (REQ-L99-051, DoD §a/§i).
//!
//! Binding definition: pack wave-2 §5.3.1 **hardened (2026-10-20)** — the
//! probe **RESOLVES all 15,251 locations** (it does not sample 512). Binary
//! gate: `resolved=15251/15251` · `regions_in_resolve=61/61` · `skipped=0` ·
//! `panics=0` · FNV-1a 64 digest identical over 2 passes · `MAPS.BSA` absent ⇒
//! `EXIT=2`.
//!
//! **What it resolves per location** (index order, deterministic):
//! 1. region + `mapIndex` (position in `MAPNAMES/MAPTABLE/MAPPITEM`),
//! 2. world coordinate (`DFU:MapsFile.cs:1088-1094` +
//!    `LongitudeLatitudeToMapPixel:330-336`),
//! 3. city from BLOCKS: `.RMB` names of each cell with the M10 API
//!    (`resolve_rmb_names`: exact name + counted UESP fallback) and real
//!    parsing of each record (`parse_resolved_blocks` → `rmb::parse_rmb`).
//!
//! **Custody (PRE-2 §8 rule 3):** Mode A only — `MAPS.BSA`/`BLOCKS.BSA` are
//! read at runtime from the user's ARENA2; 0 packaged bytes (INV-L99-6).
//! `panics=0` is proven by survival: a panic aborts the process before the
//! verdict is printed (never a false PASS).
//!
//! **Does not reimplement the index** (M10Batch provides it): it consumes
//! `daggerfall_parser_rs::maps` + the M10 helpers as they are.

use std::collections::HashMap;
use std::time::Instant;

use daggerfall_parser_bevy_bridge as bridge;
use daggerfall_parser_rs::{bsa, maps};

use crate::m10_probe::{named_entry, parse_resolved_blocks, resolve_rmb_names, rmb_name_index};

/// Dungeon location types (`DFU:DFRegion.cs:66-96`):
/// `DungeonLabyrinth=4` · `DungeonKeep=7` · `DungeonRuin=10`.
const DUNGEON_TYPES: [u8; 3] = [4, 7, 10];

/// Retail index totals. `MAPS.BSA` (22,129,312 B, `Name18` directory) has
/// **248 records / 4 = 62 region slots**; the sum of `LocationCount` is
/// **15,251** — the literal PASS criterion. The labels "61 regions / 4,232
/// dungeons" come from `UESP:dfbsa.txt` (source declaration), **not** from the
/// measured file: they are printed as declared, never written as measured
/// (plan M16-M18 D-4; pack §5.3.1 counting rule).
const EXPECTED_REGION_SLOTS: usize = 62;
const EXPECTED_LOCATIONS: usize = 15_251;
/// Declared by UESP `dfbsa.txt` (reference only — not a gate).
const UESP_DECLARED_REGIONS: usize = 61;
const UESP_DECLARED_DUNGEONS: usize = 4_232;

/// FNV-1a 64 — deterministic dependency-free digest (identical input →
/// identical digest, byte by byte).
struct Fnv1a64(u64);

impl Fnv1a64 {
    fn new() -> Self {
        Self(0xcbf2_9ce4_8422_2325)
    }
    fn feed(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            self.0 ^= u64::from(byte);
            self.0 = self.0.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }
    fn feed_u16(&mut self, value: u16) {
        self.feed(&value.to_le_bytes());
    }
    fn feed_i32(&mut self, value: i32) {
        self.feed(&value.to_le_bytes());
    }
    /// NUL-terminated string (avoids concatenation collisions).
    fn feed_str(&mut self, value: &str) {
        self.feed(value.as_bytes());
        self.feed(&[0]);
    }
}

/// Result of one full pass over the index.
struct PassStats {
    regions: usize,
    regions_fully_resolved: usize,
    locations: usize,
    dungeons: usize,
    dungeons_by_header: usize,
    dungeons_by_dungeon_type: usize,
    type_histogram: [usize; 32],
    slots_empty: usize,
    slots_zero: usize,
    resolved: usize,
    skipped: usize,
    invalid_coord: usize,
    cells_exact: usize,
    cells_fallback: usize,
    digest: u64,
    elapsed_ms: u128,
    errors: Vec<String>,
}

/// World coordinate of a location. Literal replica of
/// `DFU:MapsFile.cs:1088-1094` (the 2nd MapTable u32 is a *bitfield* with
/// length+type; the 3rd is latitude) + `LongitudeLatitudeToMapPixel`
/// (`:330-336`): 1000×500 map-pixel world.
pub(crate) fn map_pixel(table: &maps::MapTableEntry) -> (i32, i32) {
    let longitude = ((table.latitude_type & 0x01ff_ffff) >> 8) as i32;
    let latitude = ((table.longitude_type & 0x00ff_ffff) >> 8) as i32;
    (longitude / 128, 499 - latitude / 128)
}

/// Region suffixes present in `MAPS.BSA` (`MAPNAMES.<nnn>`), ascending and
/// deduplicated — index order, no RNG/clock/hash.
fn region_suffixes(archive: &bsa::BsaArchive) -> Vec<(u16, String)> {
    let mut out: Vec<(u16, String)> = Vec::new();
    for entry in &archive.entries {
        if let bsa::Entry::Named { name, .. } = entry {
            let upper = name.to_ascii_uppercase();
            if let Some(suffix) = upper.strip_prefix("MAPNAMES.") {
                if let Ok(index) = suffix.parse::<u16>() {
                    out.push((index, suffix.to_string()));
                }
            }
        }
    }
    out.sort_by_key(|(index, _)| *index);
    out.dedup_by_key(|(index, _)| *index);
    out
}

/// One full pass: enumerate regions → locations → resolutions and digest.
/// Does not print per location (only bounded errors).
fn run_pass(
    maps_archive: &bsa::BsaArchive,
    maps_data: &[u8],
    blocks_archive: &bsa::BsaArchive,
    blocks_data: &[u8],
    name_index: &HashMap<String, usize>,
    all_rmb_names: &[String],
) -> Result<PassStats, String> {
    let started = Instant::now();
    let mut digest = Fnv1a64::new();
    let mut stats = PassStats {
        regions: 0,
        regions_fully_resolved: 0,
        locations: 0,
        dungeons: 0,
        dungeons_by_header: 0,
        dungeons_by_dungeon_type: 0,
        type_histogram: [0usize; 32],
        slots_empty: 0,
        slots_zero: 0,
        resolved: 0,
        skipped: 0,
        invalid_coord: 0,
        cells_exact: 0,
        cells_fallback: 0,
        digest: 0,
        elapsed_ms: 0,
        errors: Vec::new(),
    };

    for (region_index, suffix) in region_suffixes(maps_archive) {
        let names_record = named_entry(maps_archive, maps_data, &format!("MAPNAMES.{suffix}"))?;
        let table_record = named_entry(maps_archive, maps_data, &format!("MAPTABLE.{suffix}"))?;
        let pitem_record = named_entry(maps_archive, maps_data, &format!("MAPPITEM.{suffix}"))?;
        // Region slots without data: DFU `MapsFile.LoadRegion:653-658` rejects the
        // empty record (it is not an index region). Consistency is required: the
        // three records empty or none — a partial empty is corruption.
        if names_record.is_empty() || table_record.is_empty() || pitem_record.is_empty() {
            if names_record.is_empty() && table_record.is_empty() && pitem_record.is_empty() {
                stats.slots_empty += 1;
                continue;
            }
            return Err(format!(
                "region {region_index}: partially empty records (names={} table={} pitem={})",
                names_record.len(),
                table_record.len(),
                pitem_record.len()
            ));
        }
        let names = maps::parse_map_names(names_record)
            .map_err(|e| format!("MAPNAMES.{suffix}: {e}"))?;
        if names.is_empty() {
            // Slot without locations (region unknown to the index).
            stats.slots_zero += 1;
            continue;
        }
        let table = maps::parse_map_table(table_record, names.len())
            .map_err(|e| format!("MAPTABLE.{suffix}: {e}"))?;
        let locations = maps::parse_pitem(pitem_record, names.len())
            .map_err(|e| format!("MAPPITEM.{suffix}: {e}"))?;
        if table.len() != locations.len() || names.len() != locations.len() {
            return Err(format!(
                "region {region_index}: names={} table={} pitem={}",
                names.len(),
                table.len(),
                locations.len()
            ));
        }

        stats.regions += 1;
        let mut region_resolved = 0usize;
        for (map_index, loc) in locations.iter().enumerate() {
            stats.locations += 1;
            let entry = table[map_index];
            let location_type = entry.location_type();
            stats.type_histogram[usize::from(location_type & 0x1f)] += 1;
            if DUNGEON_TYPES.contains(&location_type) {
                stats.dungeons += 1;
            }
            // Measured alternative rule: `dungeonType` (byte @+12 of MapTable,
            // DFU `MapsFile.cs:1095`) ∉ {0, 255} = the location has a dungeon.
            if entry.flavor != 0 && entry.flavor != 255 {
                stats.dungeons_by_dungeon_type += 1;
            }
            if DUNGEON_TYPES.contains(&loc.location_type) {
                stats.dungeons_by_header += 1;
            }
            let (pixel_x, pixel_y) = map_pixel(&entry);
            if !(0..1000).contains(&pixel_x) || !(0..500).contains(&pixel_y) {
                stats.invalid_coord += 1;
                if stats.errors.len() < 8 {
                    stats.errors.push(format!(
                        "region={region_index} mapIndex={map_index} {}: coordinate outside the world ({pixel_x},{pixel_y})",
                        loc.name
                    ));
                }
            }

            match resolve_rmb_names(loc, name_index, all_rmb_names).and_then(|(resolved_names, exact, fallback)| {
                parse_resolved_blocks(loc, &resolved_names, blocks_archive, blocks_data, name_index)
                    .map(|city| (resolved_names, exact, fallback, city))
            }) {
                Ok((resolved_names, exact, fallback, city)) => {
                    debug_assert_eq!(city.cells_exact, exact);
                    debug_assert_eq!(city.cells_fallback, fallback);
                    stats.resolved += 1;
                    region_resolved += 1;
                    stats.cells_exact += exact;
                    stats.cells_fallback += fallback;
                    // Digest of (region, mapIndex, coord, rmb_block) in index
                    // order — the same input produces the same digest.
                    digest.feed_u16(region_index);
                    digest.feed_u16(map_index as u16);
                    digest.feed_i32(pixel_x);
                    digest.feed_i32(pixel_y);
                    digest.feed_str(&loc.name);
                    digest.feed_u16(resolved_names.len() as u16);
                    for name in &resolved_names {
                        digest.feed_str(name);
                    }
                }
                Err(reason) => {
                    stats.skipped += 1;
                    if stats.errors.len() < 8 {
                        stats.errors.push(format!(
                            "region={region_index} mapIndex={map_index}: {reason}"
                        ));
                    }
                }
            }
        }
        if region_resolved == locations.len() {
            stats.regions_fully_resolved += 1;
        }
    }

    stats.digest = digest.0;
    stats.elapsed_ms = started.elapsed().as_millis();
    Ok(stats)
}

/// Runs PROBE-MAP-001. `EXIT=0` ⟺ 7/7 of §5.3.1; `EXIT=2` if `MAPS.BSA` or
/// `BLOCKS.BSA` are not accessible (literal reason, never a false PASS).
pub fn run_map_001() -> i32 {
    println!("PROBE-MAP-001 - full MAPS.BSA index (REQ-L99-051, M18, CR-46a43aba)");
    println!("================================================================");
    println!("[info] current definition: §5.3.1 HARDENED — resolves all 15,251 locations (no sampling)");

    let maps_path = match crate::mode_a_arena2_path("MAPS.BSA") {
        Ok(path) => path,
        Err(reason) => {
            println!("MAPS.BSA not accessible: {reason}");
            println!("EXIT=2 (retail source absent — never a false PASS or silent skip)");
            return 2;
        }
    };
    let blocks_path = match crate::mode_a_arena2_path("BLOCKS.BSA") {
        Ok(path) => path,
        Err(reason) => {
            println!("BLOCKS.BSA not accessible: {reason}");
            println!("EXIT=2 (retail source absent — the .RMB resolve is impossible without BLOCKS)");
            return 2;
        }
    };

    let maps_data = match std::fs::read(&maps_path) {
        Ok(data) => data,
        Err(e) => {
            println!("MAPS.BSA not accessible: {} ({e})", maps_path.display());
            return 2;
        }
    };
    let blocks_data = match std::fs::read(&blocks_path) {
        Ok(data) => data,
        Err(e) => {
            println!("BLOCKS.BSA not accessible: {} ({e})", blocks_path.display());
            return 2;
        }
    };
    println!(
        "[OK] mode_a MAPS.BSA: {} ({} bytes)",
        maps_path.display(),
        maps_data.len()
    );
    println!(
        "[OK] mode_a BLOCKS.BSA: {} ({} bytes)",
        blocks_path.display(),
        blocks_data.len()
    );

    let maps_archive = match bridge::parse_bsa(&maps_data) {
        Ok(archive) => archive,
        Err(e) => {
            println!("[FAIL] invalid MAPS.BSA: {e}");
            return 1;
        }
    };
    let blocks_archive = match bridge::parse_bsa(&blocks_data) {
        Ok(archive) => archive,
        Err(e) => {
            println!("[FAIL] invalid BLOCKS.BSA: {e}");
            return 1;
        }
    };
    let name_index = rmb_name_index(&blocks_archive);
    let mut all_rmb_names: Vec<String> = name_index.keys().cloned().collect();
    all_rmb_names.sort();

    let pass1 = match run_pass(
        &maps_archive,
        &maps_data,
        &blocks_archive,
        &blocks_data,
        &name_index,
        &all_rmb_names,
    ) {
        Ok(stats) => stats,
        Err(reason) => {
            println!("[FAIL] pass 1: {reason}");
            return 1;
        }
    };
    println!(
        "[info] pass 1: {:.3} s · {} exact cells · {} fallback cells (UESP name)",
        pass1.elapsed_ms as f64 / 1000.0,
        pass1.cells_exact,
        pass1.cells_fallback
    );

    let pass2 = match run_pass(
        &maps_archive,
        &maps_data,
        &blocks_archive,
        &blocks_data,
        &name_index,
        &all_rmb_names,
    ) {
        Ok(stats) => stats,
        Err(reason) => {
            println!("[FAIL] pass 2: {reason}");
            return 1;
        }
    };
    println!(
        "[info] pass 2: {:.3} s · identical digest required (index determinism)",
        pass2.elapsed_ms as f64 / 1000.0
    );

    println!(
        "[info] region_slots={} (with locations={} · empty={} · count=0={})",
        pass1.regions + pass1.slots_empty + pass1.slots_zero,
        pass1.regions,
        pass1.slots_empty,
        pass1.slots_zero
    );
    let histogram: Vec<String> = pass1
        .type_histogram
        .iter()
        .enumerate()
        .filter(|(_, count)| **count > 0)
        .map(|(location_type, count)| format!("{location_type}×{count}"))
        .collect();
    println!("[info] location_types: {}", histogram.join(", "));
    println!("regions={}", pass1.regions);
    println!("locations={}", pass1.locations);
    println!("dungeons={}", pass1.dungeons);
    println!("[info] dungeon_rule: location_type ∈ {{4,7,10}} (DFU:DFRegion.cs:66-96) = {}", pass1.dungeons);
    println!("[info] dungeons_by_dungeon_type={} (dungeonType ∉ {{0,255}}, DFU:MapsFile.cs:1095)", pass1.dungeons_by_dungeon_type);
    println!("[info] dungeons_by_header={} (LocationRecordElementHeader)", pass1.dungeons_by_header);
    println!(
        "[info] uesp_declared (dfbsa.txt, NOT measured here): regions={} · dungeons={} · locations={}",
        UESP_DECLARED_REGIONS, UESP_DECLARED_DUNGEONS, EXPECTED_LOCATIONS
    );
    println!("resolved={}", pass1.resolved);
    println!(
        "regions_in_resolve={}/{}",
        pass1.regions_fully_resolved, pass1.regions
    );
    println!("skipped={}", pass1.skipped);
    println!("panics=0");
    println!("invalid_coord={}", pass1.invalid_coord);
    println!("cells_exact={}", pass1.cells_exact);
    println!("cells_fallback={}", pass1.cells_fallback);
    println!("digest1=0x{:016x}", pass1.digest);
    println!("digest2=0x{:016x}", pass2.digest);
    println!(
        "[info] full resolve: {:.3} s (2 passes) — coverage without trimming",
        (pass1.elapsed_ms + pass2.elapsed_ms) as f64 / 1000.0
    );
    for error in &pass1.errors {
        println!("[fail-detail] {error}");
    }

    let mut failed: Vec<&str> = Vec::new();
    // Gate: measured invariants of the file (not source labels). "61 regions /
    // 4,232 dungeons" are UESP declarations — printed above, never written as
    // measured (plan D-4 prohibition).
    if pass1.regions + pass1.slots_empty + pass1.slots_zero != EXPECTED_REGION_SLOTS {
        failed.push("region_slots != 62");
    }
    if pass1.regions == 0 {
        failed.push("regions_with_locations == 0");
    }
    if pass1.locations != EXPECTED_LOCATIONS {
        failed.push("locations != 15251");
    }
    if pass1.resolved != EXPECTED_LOCATIONS {
        failed.push("resolved != 15251");
    }
    if pass1.regions_fully_resolved != pass1.regions {
        failed.push("regions_in_resolve != regions_with_locations/regions_with_locations");
    }
    if pass1.skipped != 0 {
        failed.push("skipped != 0");
    }
    if pass1.invalid_coord != 0 {
        failed.push("invalid_coord != 0");
    }
    if pass1.digest != pass2.digest {
        failed.push("digest1 != digest2");
    }
    if pass1.locations != pass2.locations || pass1.resolved != pass2.resolved {
        failed.push("inconsistent passes");
    }

    if failed.is_empty() {
        println!(
            "VERDICT: PROBE-MAP-001 PASS (resolved={}/{}, regions_in_resolve={}/{}, skipped=0, panics=0, stable digest)",
            pass1.resolved, EXPECTED_LOCATIONS, pass1.regions_fully_resolved, pass1.regions
        );
        0
    } else {
        println!("VERDICT: PROBE-MAP-001 FAIL — {}", failed.join(" · "));
        1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn map_pixel_matches_dfu_formula() {
        // longitude=128·x, latitude=(499-y)·128 → (x, y) (DFU:MapsFile.cs:330-336).
        let mut table = maps::MapTableEntry {
            map_id: 0,
            latitude_type: 0,
            longitude_type: 0,
            flavor: 0,
            services: 0,
        };
        table.latitude_type = (128u32 * 123) << 8;
        table.longitude_type = (128u32 * (499 - 45)) << 8;
        assert_eq!(map_pixel(&table), (123, 45));
    }

    #[test]
    fn digest_is_deterministic_and_order_sensitive() {
        let mut a = Fnv1a64::new();
        a.feed_u16(17);
        a.feed_str("Daggerfall");
        let mut b = Fnv1a64::new();
        b.feed_u16(17);
        b.feed_str("Daggerfall");
        assert_eq!(a.0, b.0);
        let mut c = Fnv1a64::new();
        c.feed_str("Daggerfall");
        c.feed_u16(17);
        assert_ne!(a.0, c.0);
    }

    #[test]
    fn location_type_helpers_are_stable() {
        // The 2nd u32 (bitfield) packs the type in bits 25..30 — DFU `MapsFile.cs:1089`.
        let mut table = maps::MapTableEntry {
            map_id: 0,
            latitude_type: 10u32 << 25,
            longitude_type: 0,
            flavor: 0,
            services: 0,
        };
        assert_eq!(table.location_type(), 10);
        table.latitude_type = 4u32 << 25;
        assert!(DUNGEON_TYPES.contains(&table.location_type()));
    }
}
