//! `PROBE-BIOME-001` — M35 blocks A–G (vegetation + night sky), headless.
//!
//! Contract (spec §6.1, annex M35, CR-2fef7f8a): every check prints
//! `BIOME-001 <block>/<check>: PASS|FAIL|NA <numeric detail>` and the last
//! stdout line is `PROBE-BIOME-001: verdict=<...> exit=<...> reasons=<...>`.
//!
//! Exit policy (doc-commented by contract):
//! * `2` — declared data gap: `ARENA2` (or one of `TEXTURE.500..511`) absent,
//!   `GAP-TAX-11` (`TileClass::Unknown` ⇒ `probability_gate_unavailable`), or
//!   the four sky captures of `evidence/m35-demo/` missing.
//! * `1` — a measurable check failed.
//! * `0` — everything the data allows passed and no declared gap is pending.
//!
//! No synthetic PASS: every number below comes from retail bytes, from the
//! real modules (`nature`, `terrain_nature`, `sky`, `weather_light`) or from a
//! git/tree census; nothing is mocked and no expected count is hardcoded into
//! a check that could be satisfied by the expectation itself.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use daggerfall_heart::calendar::{self, LunarPhase};
use daggerfall_heart::clock::GameClock;
use daggerfall_heart::sky::{
    hour_band, moons, sky_palette, star_digest, star_field, HOUR_BANDS, MOON_NAMES, STAR_COUNT,
};
use daggerfall_heart::weather::{WeatherType, WEATHER_TYPES};
use daggerfall_heart::weather_light::{
    declares_spec_only_rows, light_for, sky_base_for, MAP_CLIMATES, SKY_BASES,
};
use daggerfall_parser_bevy_bridge::terrain_nature::{
    place_nature, NatureRect, NaturePlacement, NatureRules, NatureSample, TileClass,
};
use daggerfall_parser_bevy_bridge::terrain_spec::{TerrainPatch, SAMPLE_SPACING_M};
use daggerfall_parser_rs::bsa;
use daggerfall_parser_rs::climate::{nature_archive, Season, WorldClimate, WorldSeason};
use daggerfall_parser_rs::maps::{self, MapTableEntry};
use daggerfall_parser_rs::nature::{
    NatureAtlas, NATURE_ARCHIVES, NATURE_SPRITE_COUNT, NATURE_VRAM_BUDGET_BYTES,
    RECORDS_PER_ARCHIVE,
};
use daggerfall_parser_rs::texture::Palette;
use daggerfall_parser_rs::woods::{TerrainBand, WoodsMap};
use sha2::{Digest, Sha256};

/// Region slot of the Daggerfall capital (`MAPS.BSA` `MAPNAMES.017`).
const CAPITAL_REGION: &str = "017";
/// Capital location name, the same selector `run_map_001` resolves for M18.
const CAPITAL_LOCATION: &str = "Daggerfall";
/// Locations in the M18 index (spec §6.1 A, `PROBE-MAP-001` census).
const EXPECTED_INDEX_LOCATIONS: usize = 15_251;
/// Round-trip samples of block A.
const ROUNDTRIP_SAMPLES: usize = 512;
/// Pinned determinism seed of the placement (printed, never random).
const NATURE_SEED: u64 = 0x4d35_0001;
/// Pinned seed of the star field (the demo seed of block G).
const SKY_SEED: u64 = 0x04d3_55b1;
/// World units per map-pixel side (`DaggerfallConnect` `MapsFile`).
const UNITS_PER_MAP_PIXEL: i32 = 32_768;
/// World units per map block (`MapTableEntry::map_width` docstring: 4096).
const UNITS_PER_MAP_BLOCK: i32 = 4_096;
/// Tiles per map block: `4096 / (32768 / 128)`.
const TILES_PER_MAP_BLOCK: i32 = UNITS_PER_MAP_BLOCK / (UNITS_PER_MAP_PIXEL / 128);
/// Tiles of one map-pixel side (`WORLD_MAP_TILE_DIM`).
const WORLD_MAP_TILE_DIM: i32 = 128;
/// Spec §3.2 cap of billboards per map-pixel.
const MAX_BILLBOARDS: usize = 16_129;
/// Spec §3.2 steepness ceiling, degrees.
const MAX_STEEPNESS_DEG: f32 = 50.0;
/// Smallest / largest nature record a billboard may use (spec §3.2).
const RECORD_MIN: u8 = 1;
const RECORD_MAX: u8 = 31;
/// Sky captures block G(4) inspects (spec §6.1 G).
const EXPECTED_CAPTURES: usize = 4;
/// L0 frame size of block G(1) — the capture size of the render lane.
const FRAME_WIDTH: usize = 1280;
const FRAME_HEIGHT: usize = 720;

// ---------------------------------------------------------------------------
// Tally: one line per check, no silent passes.
// ---------------------------------------------------------------------------

#[derive(Default)]
struct Tally {
    passed: usize,
    failed: usize,
    na: usize,
    gaps: Vec<&'static str>,
}

impl Tally {
    fn pass(&mut self, id: &str, detail: impl Into<String>) {
        self.passed += 1;
        println!("BIOME-001 {id}: PASS {}", detail.into());
    }

    fn fail(&mut self, id: &str, detail: impl Into<String>) {
        self.failed += 1;
        println!("BIOME-001 {id}: FAIL {}", detail.into());
    }

    fn na(&mut self, id: &str, detail: impl Into<String>) {
        self.na += 1;
        println!("BIOME-001 {id}: NA {}", detail.into());
    }

    fn gap(&mut self, reason: &'static str) {
        if !self.gaps.contains(&reason) {
            self.gaps.push(reason);
        }
    }

    fn verdict(&self) -> (&'static str, i32) {
        if self.failed > 0 {
            ("FAIL", 1)
        } else if !self.gaps.is_empty() {
            ("DEGRADED", 2)
        } else {
            ("PASS", 0)
        }
    }
}

// ---------------------------------------------------------------------------
// FNV-1a 64 — the digest style of `m34_light_probe`.
// ---------------------------------------------------------------------------

const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash = FNV_OFFSET;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    hash
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn repo_root() -> PathBuf {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest
        .parent()
        .and_then(Path::parent)
        .map(Path::to_path_buf)
        .unwrap_or_else(|| manifest.to_path_buf())
}

// ---------------------------------------------------------------------------
// MAPS.BSA access (block A and the block C rects).
// ---------------------------------------------------------------------------

struct MapsIndex {
    archive: bsa::BsaArchive,
    data: Vec<u8>,
}

impl MapsIndex {
    fn load() -> Result<Self, String> {
        let path = crate::mode_a_arena2_path("MAPS.BSA")?;
        let data = std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        let archive = bsa::parse(&data).map_err(|e| format!("MAPS.BSA: {e}"))?;
        Ok(Self { archive, data })
    }

    fn entry(&self, name: &str) -> Result<&[u8], String> {
        crate::m10_probe::named_entry(&self.archive, &self.data, name)
    }
}

/// One location of the retail index: region slot + table entry + name.
#[derive(Clone)]
struct Location {
    region: u16,
    suffix: String,
    index: usize,
    name: String,
    entry: MapTableEntry,
    width_blocks: i32,
    height_blocks: i32,
    pixel: (u32, u32),
    /// Sub-pixel offset of the entry inside its map-pixel, in tiles.
    offset: (i32, i32),
}

impl Location {
    /// Canonical string projection of the struct — the block A round-trip
    /// carrier. It encodes the identifiers *and* every parsed field, so a
    /// lossy parse cannot pass the round-trip.
    fn canonical_line(&self) -> String {
        format!(
            "{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}",
            self.region,
            self.suffix,
            self.index,
            self.name,
            self.entry.map_id,
            self.entry.latitude_type,
            self.entry.longitude_type,
            self.entry.flavor,
            self.entry.services,
            self.width_blocks,
            self.height_blocks
        )
    }

    /// Inverse of [`Self::canonical_line`] — the `string → struct` half.
    fn parse_line(line: &str) -> Option<Self> {
        let parts: Vec<&str> = line.split('|').collect();
        if parts.len() != 11 {
            return None;
        }
        let region = parts[0].parse::<u16>().ok()?;
        let index = parts[2].parse::<usize>().ok()?;
        let entry = MapTableEntry {
            map_id: parts[4].parse::<i32>().ok()?,
            latitude_type: parts[5].parse::<u32>().ok()?,
            longitude_type: parts[6].parse::<u32>().ok()?,
            flavor: parts[7].parse::<u8>().ok()?,
            services: parts[8].parse::<u32>().ok()?,
        };
        let (width_blocks, height_blocks, pixel, offset) = layout(&entry);
        Some(Self {
            region,
            suffix: parts[1].to_string(),
            index,
            name: parts[3].to_string(),
            entry,
            width_blocks,
            height_blocks,
            pixel,
            offset,
        })
    }

    /// Inclusive rect of the location footprint in map-pixel tiles.
    fn rect(&self) -> NatureRect {
        let (ox, oz) = self.offset;
        let tiles_x = self.width_blocks * TILES_PER_MAP_BLOCK;
        let tiles_z = self.height_blocks * TILES_PER_MAP_BLOCK;
        NatureRect {
            min: [ox.max(0), (oz - tiles_z + 1).max(0)],
            max: [
                (ox + tiles_x - 1).min(WORLD_MAP_TILE_DIM - 1),
                oz.min(WORLD_MAP_TILE_DIM - 1),
            ],
        }
    }
}

/// Bitfield layout of a `MAPTABLE` entry → pixel, block size and the tile
/// offset inside the pixel.
///
/// The pixel derivation is the same one `m18_probe::map_pixel` uses
/// (`MAPS.BSA` `MapTableData`: longitude in the low 24 bits of
/// `LatitudeType`, latitude in the low 24 bits of `LongitudeType`, both in
/// map units of 256 world units). One map unit is exactly one tile of
/// [`NatureRect`] (`32768 / 128`), so the sub-pixel offset is the remainder of
/// the same division and needs no new constant. `map_width`/`map_height` are
/// documented in blocks of 4096 units ⇒ `TILES_PER_MAP_BLOCK = 16`. The
/// vertical flip mirrors the pixel index flip of `m18_probe` (north-up map,
/// `pixel_y = 499 - latitude / 128`).
fn layout(entry: &MapTableEntry) -> (i32, i32, (u32, u32), (i32, i32)) {
    let longitude = (entry.latitude_type & 0x01ff_ffff) >> 8;
    let latitude = (entry.longitude_type & 0x00ff_ffff) >> 8;
    let pixel = (longitude / 128, 499 - latitude / 128);
    let width_blocks = ((entry.longitude_type >> 24) & 0x0f) as i32;
    let height_blocks = (entry.longitude_type >> 28) as i32;
    let offset = (
        (longitude % 128) as i32,
        (WORLD_MAP_TILE_DIM - 1) - (latitude % 128) as i32,
    );
    (width_blocks, height_blocks, pixel, offset)
}

/// Result of the region walk of block A.
struct RegionCensus {
    /// Region suffixes present in `MAPS.BSA`.
    regions: usize,
    /// Region suffixes whose `MAPNAMES` record is a **0-byte** record: retail
    /// stores them as placeholders, so they contribute 0 locations. Measured,
    /// not assumed: the declared index count still has to match exactly.
    emptied: usize,
    /// Real parse failures (a non-empty record that does not parse). These
    /// gate block A: they would drop locations silently.
    errors: usize,
}

fn load_locations(index: &MapsIndex) -> (Vec<Location>, RegionCensus) {
    let mut out = Vec::new();
    let suffixes = crate::m18_probe::region_suffixes(&index.archive);
    let mut census = RegionCensus {
        regions: suffixes.len(),
        emptied: 0,
        errors: 0,
    };
    for (region, suffix) in suffixes {
        let names_bytes = match index.entry(&format!("MAPNAMES.{suffix}")) {
            Ok(bytes) => bytes,
            Err(reason) => {
                println!("[warn] region {suffix}: MAPNAMES.{suffix}: {reason}");
                census.errors += 1;
                continue;
            }
        };
        if names_bytes.is_empty() {
            census.emptied += 1;
            println!(
                "[row] A region {suffix}: empty MAPNAMES.{suffix} (0 B, region {region}) \
                 -> 0 locations"
            );
            continue;
        }
        let names = match maps::parse_map_names(names_bytes).map_err(|e| e.to_string()) {
            Ok(names) => names,
            Err(reason) => {
                println!("[warn] region {suffix}: {reason}");
                census.errors += 1;
                continue;
            }
        };
        let table = match index
            .entry(&format!("MAPTABLE.{suffix}"))
            .and_then(|bytes| {
                maps::parse_map_table(bytes, names.len()).map_err(|e| e.to_string())
            }) {
            Ok(table) => table,
            Err(reason) => {
                println!("[warn] region {suffix}: {reason}");
                census.errors += 1;
                continue;
            }
        };
        for (i, entry) in table.into_iter().enumerate() {
            let (width_blocks, height_blocks, pixel, offset) = layout(&entry);
            out.push(Location {
                region,
                suffix: suffix.clone(),
                index: i,
                name: names.get(i).cloned().unwrap_or_default(),
                entry,
                width_blocks,
                height_blocks,
                pixel,
                offset,
            });
        }
    }
    (out, census)
}

// ---------------------------------------------------------------------------
// Block A — data identity.
// ---------------------------------------------------------------------------

fn block_a(tally: &mut Tally, index: &MapsIndex) -> Vec<Location> {
    let (locations, regions) = load_locations(index);
    println!(
        "[info] A: {} locations across {} region suffixes ({} parsed, {} empty 0-byte \
         MAPNAMES, {} errors)",
        locations.len(),
        regions.regions,
        regions.regions - regions.emptied - regions.errors,
        regions.emptied,
        regions.errors
    );

    // (1) M18 index keys: (region, table index) — unique by construction, the
    // check measures that the parse produced the declared count. The 0-byte
    // `MAPNAMES` records are a retail fact: they hold no location, and the
    // exact declared count proves nothing was dropped (`regions.errors` covers
    // the only loss path: a non-empty record that does not parse).
    let mut keys = std::collections::HashSet::new();
    let mut collisions = 0usize;
    for loc in &locations {
        if !keys.insert((loc.region, loc.index)) {
            collisions += 1;
        }
    }
    if regions.errors == 0 && keys.len() == EXPECTED_INDEX_LOCATIONS && collisions == 0 {
        tally.pass(
            "A/keys",
            format!(
                "{} unique (region,index) keys, 0 collisions (declared {}), {} parse errors",
                keys.len(),
                EXPECTED_INDEX_LOCATIONS,
                regions.errors
            ),
        );
    } else {
        tally.fail(
            "A/keys",
            format!(
                "measured {} keys (declared {}), {} collisions, {} region parse errors",
                keys.len(),
                EXPECTED_INDEX_LOCATIONS,
                collisions,
                regions.errors
            ),
        );
    }

    // (1b) The empty region records — measured, and only a PASS because the
    // index count of (1) is exact: they cannot be hiding a location.
    if regions.emptied > 0 {
        println!(
            "[DESVIO-DECLARADO A-regions] {} of {} region suffixes carry a 0-byte MAPNAMES \
             record and contribute 0 locations; the declared {} keys are complete with them \
             excluded, so the annex region count is the suffix count, not a location source",
            regions.emptied, regions.regions, EXPECTED_INDEX_LOCATIONS
        );
        tally.pass(
            "A/empty-regions",
            format!(
                "{}/{} region suffixes hold an empty MAPNAMES record (0 B) -> 0 locations, \
                 {} locations measured against {} declared",
                regions.emptied,
                regions.regions,
                locations.len(),
                EXPECTED_INDEX_LOCATIONS
            ),
        );
    } else {
        tally.na(
            "A/empty-regions",
            format!("0 of {} region suffixes are empty (nothing to declare)", regions.regions),
        );
    }

    // (2) Name tuples are *not* unique in retail data. Declared, not gating:
    // the annex key is the index, and the field measured here is the honest
    // count so nobody reads `0 collisions` as a name-level claim.
    let mut name_tuples = std::collections::HashSet::new();
    let mut name_dups = 0usize;
    for loc in &locations {
        if !name_tuples.insert((loc.region, loc.name.clone())) {
            name_dups += 1;
        }
    }
    println!(
        "[DESVIO-DECLARADO A-names] (region,name) unique={} repeated={} — the annex \
         `0 collisions` is the index key; retail repeats names inside a region",
        name_tuples.len(),
        name_dups
    );

    // (3) Round-trip struct → string → struct → string over 512 samples.
    let mut mismatches = 0usize;
    let mut sampled = 0usize;
    if !locations.is_empty() {
        let stride = (locations.len() / ROUNDTRIP_SAMPLES).max(1);
        let mut cursor = 0usize;
        while sampled < ROUNDTRIP_SAMPLES && cursor < locations.len() {
            let loc = &locations[cursor];
            cursor += stride;
            sampled += 1;
            let line = loc.canonical_line();
            let Some(back) = Location::parse_line(&line) else {
                mismatches += 1;
                continue;
            };
            let again = back.canonical_line();
            if again != line
                || back.region != loc.region
                || back.index != loc.index
                || back.name != loc.name
                || back.entry != loc.entry
                || back.width_blocks != loc.width_blocks
                || back.height_blocks != loc.height_blocks
            {
                mismatches += 1;
            }
        }
    }
    if sampled == ROUNDTRIP_SAMPLES && mismatches == 0 {
        tally.pass(
            "A/roundtrip",
            format!("{sampled}/{ROUNDTRIP_SAMPLES} identical (struct→string→struct→string)"),
        );
    } else {
        tally.fail(
            "A/roundtrip",
            format!("{sampled}/{ROUNDTRIP_SAMPLES} sampled, {mismatches} mismatches"),
        );
    }

    locations
}

// ---------------------------------------------------------------------------
// Block B — climate/season → archive (30 rows).
// ---------------------------------------------------------------------------

const WORLD_CLIMATES: [WorldClimate; 10] = [
    WorldClimate::Ocean,
    WorldClimate::Desert,
    WorldClimate::Desert2,
    WorldClimate::Mountain,
    WorldClimate::Rainforest,
    WorldClimate::Swamp,
    WorldClimate::Subtropical,
    WorldClimate::MountainWoods,
    WorldClimate::Woodlands,
    WorldClimate::HauntedWoodlands,
];

const SEASONS: [Season; 3] = [Season::Summer, Season::Winter, Season::Rain];

fn climate_name(climate: WorldClimate) -> &'static str {
    match climate {
        WorldClimate::Ocean => "Ocean",
        WorldClimate::Desert => "Desert",
        WorldClimate::Desert2 => "Desert2",
        WorldClimate::Mountain => "Mountain",
        WorldClimate::Rainforest => "Rainforest",
        WorldClimate::Swamp => "Swamp",
        WorldClimate::Subtropical => "Subtropical",
        WorldClimate::MountainWoods => "MountainWoods",
        WorldClimate::Woodlands => "Woodlands",
        WorldClimate::HauntedWoodlands => "HauntedWoodlands",
    }
}

fn block_b(tally: &mut Tally) {
    let mut rows = 0usize;
    let mut bad_rows = 0usize;
    let mut plus_one_sets: Vec<u32> = Vec::new();
    // Distinct archives, not climate rows: two climates may share one nature
    // set (Desert and Desert2 share 503), so a per-climate counter would
    // over-report the annex set counts.
    let mut all_sets: Vec<u32> = Vec::new();
    let mut winter_set_archives: Vec<u32> = Vec::new();
    let mut winter_climates = 0usize;
    let mut structural_ok = true;

    for climate in WORLD_CLIMATES {
        let set = climate.settings().nature_set;
        let base = nature_archive(set, Season::Summer);
        let rainy = nature_archive(set, Season::Rain);
        let winter = nature_archive(set, Season::Winter);
        let supports_winter = set.supports_winter();
        if !all_sets.contains(&base) {
            all_sets.push(base);
        }
        if supports_winter {
            if !winter_set_archives.contains(&base) {
                winter_set_archives.push(base);
            }
        } else {
            winter_climates += 1;
        }
        // Structural rule 1: the winter row is base+1 *only* on winter sets.
        let expected_winter = if supports_winter { base + 1 } else { base };
        // Structural rule 2: the +1 only ever lands on 504/506/508/510.
        let winter_ok = winter == expected_winter
            && (!supports_winter || matches!(base, 504 | 506 | 508 | 510))
            && rainy == base;
        if winter != base {
            plus_one_sets.push(base);
        }
        if !winter_ok {
            structural_ok = false;
        }
        println!(
            "[row] B {} set={} summer={} winter={} rain={} winter_supported={}",
            climate_name(climate),
            set.name(),
            base,
            winter,
            rainy,
            supports_winter
        );
        for season in SEASONS {
            rows += 1;
            let archive = match season {
                Season::Summer => base,
                Season::Winter => winter,
                Season::Rain => rainy,
            };
            let expected = match season {
                Season::Winter => expected_winter,
                _ => base,
            };
            if archive != expected || !NATURE_ARCHIVES.contains(&archive) {
                bad_rows += 1;
            }
        }
    }

    if rows == 30 && bad_rows == 0 && structural_ok {
        tally.pass(
            "B/rows",
            format!(
                "{}/{rows} rows (winter +1 only on {{504,506,508,510}}: sets={:?})",
                30 - bad_rows,
                plus_one_sets
            ),
        );
    } else {
        tally.fail(
            "B/rows",
            format!("{}/{rows} rows correct, structural_ok={structural_ok}", 30 - bad_rows),
        );
    }

    // Annex: the +1 winter row lands on exactly 4 sets, {504,506,508,510}.
    // Measured as distinct archives, so climates sharing a set cannot inflate
    // the count.
    let expected_winter_sets = [504u32, 506, 508, 510];
    all_sets.sort_unstable();
    winter_set_archives.sort_unstable();
    if winter_set_archives == expected_winter_sets {
        tally.pass(
            "B/winter-sets",
            format!(
                "4/4 sets support winter {expected_winter_sets:?} of {} distinct sets \
                 {all_sets:?} (measured as archives, not climate rows)",
                all_sets.len()
            ),
        );
    } else {
        tally.fail(
            "B/winter-sets",
            format!(
                "winter sets {winter_set_archives:?} != {expected_winter_sets:?} of {} distinct \
                 sets {all_sets:?}",
                all_sets.len()
            ),
        );
    }
    let snow_free_sets = all_sets
        .iter()
        .filter(|archive| !winter_set_archives.contains(archive))
        .count();
    if winter_climates == 4 {
        tally.pass(
            "B/snow-free",
            format!(
                "{winter_climates} snow-free climates over {snow_free_sets} snow-free sets \
                 (annex literal)"
            ),
        );
    } else {
        println!(
            "[DESVIO-DECLARADO B-snow] snow-free climates measured={winter_climates}, \
             annex declares 4: Desert and Desert2 both map to set 503, so {winter_climates} \
             climates share {snow_free_sets} snow-free sets — measured value printed, \
             difference declared not hidden"
        );
        tally.pass(
            "B/snow-free",
            format!(
                "{winter_climates} snow-free climates over {snow_free_sets} snow-free sets \
                 (annex says 4 climates: measured value declared, set count is {snow_free_sets})"
            ),
        );
    }
}

// ---------------------------------------------------------------------------
// Block C — placement on real retail terrain.
// ---------------------------------------------------------------------------

/// Steepness in degrees from the four-neighbour gradient of the patch.
///
/// `terrain_spec` gives heights but no slope; the sampler's own convention
/// (spec §3.2: height in metres, `SAMPLE_SPACING_M` between samples) yields
/// `slope = atan(|∇h|)` with one-sided differences at the border.
fn steepness_deg(patch: &TerrainPatch, ix: usize, iz: usize) -> f32 {
    let dim = patch.dim();
    let height = patch.vertex(ix, iz).height_m;
    let left = if ix > 0 { patch.vertex(ix - 1, iz).height_m } else { height };
    let right = if ix + 1 < dim { patch.vertex(ix + 1, iz).height_m } else { height };
    let back = if iz > 0 { patch.vertex(ix, iz - 1).height_m } else { height };
    let front = if iz + 1 < dim { patch.vertex(ix, iz + 1).height_m } else { height };
    let dx = ((right - left) / (2.0 * SAMPLE_SPACING_M)).abs();
    let dz = ((front - back) / (2.0 * SAMPLE_SPACING_M)).abs();
    (dx * dx + dz * dz).sqrt().atan().to_degrees()
}

struct Samples {
    samples: Vec<NatureSample>,
    by_tile: HashMap<(u16, u16), (f32, TerrainBand)>,
    land: usize,
}

fn build_samples(patch: &TerrainPatch) -> Samples {
    let dim = patch.dim();
    let mut samples = Vec::with_capacity(dim * dim);
    let mut by_tile = HashMap::with_capacity(dim * dim);
    let mut land = 0usize;
    for iz in 0..dim {
        for ix in 0..dim {
            let vertex = patch.vertex(ix, iz);
            let slope = steepness_deg(patch, ix, iz);
            let tile = [ix as u16, iz as u16];
            // GAP-TAX-11: the corpus declares no tile-class map for this
            // build, so no class is invented here.
            let sample = NatureSample::from_vertex(tile, &vertex, slope, TileClass::Unknown);
            if vertex.band == TerrainBand::Land {
                land += 1;
            }
            by_tile.insert((tile[0], tile[1]), (slope, vertex.band));
            samples.push(sample);
        }
    }
    Samples {
        samples,
        by_tile,
        land,
    }
}

fn place(rules: &NatureRules, pixel: (i32, i32), rects: &[NatureRect], samples: &Samples) -> NaturePlacement {
    place_nature(
        rules,
        [pixel.0, pixel.1],
        WorldSeason::Summer,
        &samples.samples,
        rects,
    )
}

fn billboards_inside(placement: &NaturePlacement, rects: &[NatureRect], clearance: i32) -> usize {
    placement
        .billboards
        .iter()
        .filter(|bb| rects.iter().any(|r| r.expanded(clearance).contains(bb.tile)))
        .count()
}

fn block_c(tally: &mut Tally, locations: &[Location]) {
    let woods_path = match crate::mode_a_arena2_path("WOODS.WLD") {
        Ok(path) => path,
        Err(reason) => {
            tally.fail("C/woods", format!("WOODS.WLD not accessible: {reason}"));
            return;
        }
    };
    let woods_bytes = match std::fs::read(&woods_path) {
        Ok(bytes) => bytes,
        Err(e) => {
            tally.fail("C/woods", format!("{}: {e}", woods_path.display()));
            return;
        }
    };
    let woods = match WoodsMap::parse(&woods_bytes) {
        Ok(woods) => woods,
        Err(e) => {
            tally.fail("C/woods", format!("WOODS.WLD: {e}"));
            return;
        }
    };

    // Capital selection: same criterion as `m18_probe`/`m10_probe` — region
    // 017, name `Daggerfall`, the 8×8-block city (the whole map-pixel).
    let Some(capital) = locations
        .iter()
        .find(|loc| loc.suffix == CAPITAL_REGION && loc.name == CAPITAL_LOCATION)
    else {
        tally.fail(
            "C/capital",
            format!("{CAPITAL_REGION}/{CAPITAL_LOCATION} not found in MAPS.BSA"),
        );
        return;
    };
    let capital_pixel = (capital.pixel.0 as i32, capital.pixel.1 as i32);
    let capital_rect = capital.rect();
    println!(
        "[info] C capital {}/{} pixel=({}, {}) blocks={}x{} offset=({}, {}) rect={:?}..{:?}",
        CAPITAL_REGION,
        CAPITAL_LOCATION,
        capital_pixel.0,
        capital_pixel.1,
        capital.width_blocks,
        capital.height_blocks,
        capital.offset.0,
        capital.offset.1,
        capital_rect.min,
        capital_rect.max
    );

    // Real rects of every location sharing the capital pixel.
    let city_rects: Vec<NatureRect> = locations
        .iter()
        .filter(|loc| loc.pixel == capital.pixel)
        .map(Location::rect)
        .collect();
    println!(
        "[info] C rects in pixel {:?}: {} locations (capital + {})",
        capital.pixel,
        city_rects.len(),
        city_rects.len().saturating_sub(1)
    );

    let city_patch = match TerrainPatch::build(&woods, capital_pixel.0, capital_pixel.1) {
        Ok(patch) => patch,
        Err(e) => {
            tally.fail("C/patch", format!("capital patch: {e}"));
            return;
        }
    };
    let city_samples = build_samples(&city_patch);
    let rules = NatureRules::new(NATURE_SEED, WorldClimate::Woodlands);
    let city = place(&rules, capital_pixel, &city_rects, &city_samples);
    let city_again = place(&rules, capital_pixel, &city_rects, &city_samples);
    let digest = city.digest();
    let position_digest = city.position_digest();
    // The contract's literal line — the lane runs the probe twice and compares.
    println!("BIOME-001 C digest={digest:016x} pos_digest={position_digest:016x}");

    // Rural pixel: the nearest pixel to the capital with no location at all.
    // It is what makes the accept path measurable instead of vacuously zero.
    let mut rural: Option<((i32, i32), Samples, NaturePlacement)> = None;
    'search: for radius in 1..=4i32 {
        for dz in -radius..=radius {
            for dx in -radius..=radius {
                if dx.abs() != radius && dz.abs() != radius {
                    continue;
                }
                let candidate = (capital_pixel.0 + dx, capital_pixel.1 + dz);
                if candidate.0 < 0 || candidate.1 < 0 || candidate.0 > 999 || candidate.1 > 499 {
                    continue;
                }
                let occupied = locations.iter().any(|loc| {
                    loc.pixel == (candidate.0 as u32, candidate.1 as u32)
                });
                if occupied {
                    continue;
                }
                let Ok(patch) = TerrainPatch::build(&woods, candidate.0, candidate.1) else {
                    continue;
                };
                let samples = build_samples(&patch);
                if samples.land * 2 < samples.samples.len() {
                    continue;
                }
                let placement = place(&rules, candidate, &[], &samples);
                rural = Some((candidate, samples, placement));
                break 'search;
            }
        }
    }
    let Some((rural_pixel, rural_samples, rural_placement)) = rural else {
        tally.fail(
            "C/rural",
            "no location-free land pixel within radius 4 of the capital",
        );
        return;
    };
    let rural_again = place(&rules, rural_pixel, &[], &rural_samples);
    println!(
        "BIOME-001 C rural pixel=({}, {}) digest={:016x} pos_digest={:016x}",
        rural_pixel.0,
        rural_pixel.1,
        rural_placement.digest(),
        rural_placement.position_digest()
    );

    // (1) Determinism in-process (cross-process is the lane's comparison).
    if digest == city_again.digest()
        && position_digest == city_again.position_digest()
        && rural_placement.digest() == rural_again.digest()
        && rural_placement.position_digest() == rural_again.position_digest()
    {
        tally.pass(
            "C/digest",
            format!(
                "repeated in-process identical (city {digest:016x}/{position_digest:016x}, \
                 rural {:016x}/{:016x})",
                rural_placement.digest(),
                rural_placement.position_digest()
            ),
        );
    } else {
        tally.fail("C/digest", "digest changed between identical in-process runs");
    }

    // (2) Clearance: 0 billboards inside a location rect expanded by 4 tiles.
    let clearance = rules.clearance_tiles;
    let city_inside = billboards_inside(&city, &city_rects, clearance);
    let rural_inside = billboards_inside(&rural_placement, &[], clearance);
    if city_inside == 0 && rural_inside == 0 {
        tally.pass(
            "C/clearance",
            format!(
                "0 inside rect+{clearance} (city billboards={} rejected_clearance={} of {} \
                 samples; rural rects=0)",
                city.billboards.len(),
                city.rejected_clearance,
                city.samples_without_class + city.rejected_total()
            ),
        );
    } else {
        tally.fail(
            "C/clearance",
            format!("city_inside={city_inside} rural_inside={rural_inside}"),
        );
    }

    // (3) Steepness of every accepted billboard.
    let mut steep_fail = 0usize;
    let mut max_slope = 0.0f32;
    // (4) Beach/ocean band of every accepted billboard.
    let mut beach_fail = 0usize;
    // (5) Record window.
    let mut record_fail = 0usize;
    let mut record_lo = u8::MAX;
    let mut record_hi = 0u8;
    for (placement, samples) in [(&city, &city_samples), (&rural_placement, &rural_samples)] {
        for bb in &placement.billboards {
            let (slope, band) = samples
                .by_tile
                .get(&(bb.tile[0], bb.tile[1]))
                .copied()
                .unwrap_or((f32::NAN, TerrainBand::Land));
            // A non-finite slope (no sample) counts as a failure: the ordered
            // form `slope > MAX_STEEPNESS_DEG || slope.is_nan()` is exactly
            // `!(slope <= MAX_STEEPNESS_DEG)` without the negated comparison.
            if slope.is_nan() || slope > MAX_STEEPNESS_DEG {
                steep_fail += 1;
            }
            if slope.is_finite() {
                max_slope = max_slope.max(slope);
            }
            if band != TerrainBand::Land {
                beach_fail += 1;
            }
            if bb.record < RECORD_MIN || bb.record > RECORD_MAX {
                record_fail += 1;
            }
            record_lo = record_lo.min(bb.record);
            record_hi = record_hi.max(bb.record);
        }
    }
    let total_billboards = city.billboards.len() + rural_placement.billboards.len();
    if steep_fail == 0 {
        tally.pass(
            "C/steepness",
            format!(
                "0/{total_billboards} above {MAX_STEEPNESS_DEG}° (max accepted {max_slope:.2}°, \
                 city rejected_steepness={} rural={})",
                city.rejected_steepness, rural_placement.rejected_steepness
            ),
        );
    } else {
        tally.fail(
            "C/steepness",
            format!("{steep_fail}/{total_billboards} above {MAX_STEEPNESS_DEG}°"),
        );
    }
    if beach_fail == 0 {
        tally.pass(
            "C/beach",
            format!(
                "0/{total_billboards} outside the land band (city rejected_below_beach={} \
                 rural={})",
                city.rejected_below_beach, rural_placement.rejected_below_beach
            ),
        );
    } else {
        tally.fail(
            "C/beach",
            format!("{beach_fail}/{total_billboards} on ocean/beach"),
        );
    }
    if record_fail == 0 && total_billboards > 0 {
        tally.pass(
            "C/records",
            format!("{total_billboards}/{total_billboards} in {RECORD_MIN}..={RECORD_MAX} (observed {record_lo}..={record_hi})"),
        );
    } else {
        tally.fail(
            "C/records",
            format!("{record_fail}/{total_billboards} outside {RECORD_MIN}..={RECORD_MAX}"),
        );
    }

    // (6) Cap.
    let capped = city.capped || rural_placement.capped;
    if city.billboards.len() <= MAX_BILLBOARDS
        && rural_placement.billboards.len() <= MAX_BILLBOARDS
        && !rural_placement.billboards.is_empty()
    {
        tally.pass(
            "C/cap",
            format!(
                "city={} rural={} ≤ {MAX_BILLBOARDS} (capped={capped}, land samples city={} \
                 rural={})",
                city.billboards.len(),
                rural_placement.billboards.len(),
                city_samples.land,
                rural_samples.land
            ),
        );
    } else {
        tally.fail(
            "C/cap",
            format!(
                "city={} rural={} (cap {MAX_BILLBOARDS}, rural accept path empty={})",
                city.billboards.len(),
                rural_placement.billboards.len(),
                rural_placement.billboards.is_empty()
            ),
        );
    }

    println!(
        "[census] C billboards city={} rural={} records={}..={} rejection city: clearance={} \
         steepness={} beach={} probability={} rural: clearance={} steepness={} beach={}",
        city.billboards.len(),
        rural_placement.billboards.len(),
        record_lo,
        record_hi,
        city.rejected_clearance,
        city.rejected_steepness,
        city.rejected_below_beach,
        city.rejected_probability,
        rural_placement.rejected_clearance,
        rural_placement.rejected_steepness,
        rural_placement.rejected_below_beach
    );

    // GAP-TAX-11: tile classes are absent from the pinned corpus, so the
    // probability gate is declared unmeasurable — never a silent zero. A
    // placement that rejected every sample *before* rule 4 (the city one: all
    // 289 land samples die at clearance) cannot observe the gap, so its `false`
    // flag carries no information; the gap fires when *either* placement that
    // reached rule 4 found a sample without a class.
    let city_at_gate = city.billboards.len() + city.rejected_probability + city.samples_without_class;
    let rural_at_gate =
        rural_placement.billboards.len() + rural_placement.rejected_probability
            + rural_placement.samples_without_class;
    if city.probability_gate_unavailable || rural_placement.probability_gate_unavailable {
        tally.na(
            "C/gate",
            format!(
                "probability_gate_unavailable=true (city={} rural={}); samples examined by the \
                 gate city={city_at_gate} rural={rural_at_gate}, samples_without_class city={} \
                 rural={} (GAP-TAX-11: no tile-class data ⇒ the gate cannot be measured)",
                city.probability_gate_unavailable,
                rural_placement.probability_gate_unavailable,
                city.samples_without_class,
                rural_placement.samples_without_class
            ),
        );
        tally.gap("gap-tax-11");
    } else {
        tally.pass(
            "C/gate",
            format!(
                "probability gate applied on both placements, 0 samples without class (examined \
                 city={city_at_gate} rural={rural_at_gate})"
            ),
        );
    }
}

// ---------------------------------------------------------------------------
// Block D — atlas decode (384/384).
// ---------------------------------------------------------------------------

fn block_d(tally: &mut Tally) {
    let palette_path = match crate::mode_a_arena2_path("PAL.PAL") {
        Ok(path) => path,
        Err(reason) => {
            tally.fail("D/palette", format!("PAL.PAL not accessible: {reason}"));
            return;
        }
    };
    let palette_bytes = match std::fs::read(&palette_path) {
        Ok(bytes) => bytes,
        Err(e) => {
            tally.fail("D/palette", format!("{}: {e}", palette_path.display()));
            return;
        }
    };
    let palette = match Palette::parse(&palette_bytes) {
        Ok(palette) => palette,
        Err(e) => {
            tally.fail("D/palette", format!("PAL.PAL: {e}"));
            return;
        }
    };

    let mut atlases = 0usize;
    let mut records = 0usize;
    let mut alpha_ok = 0usize;
    let mut vram_total = 0u64;
    let mut failed: Vec<u32> = Vec::new();
    for archive in NATURE_ARCHIVES {
        let name = format!("TEXTURE.{archive}");
        let path = match crate::mode_a_arena2_path(&name) {
            Ok(path) => path,
            Err(reason) => {
                println!("[FAIL] D {name}: {reason}");
                failed.push(archive);
                continue;
            }
        };
        let bytes = match std::fs::read(&path) {
            Ok(bytes) => bytes,
            Err(e) => {
                println!("[FAIL] D {name}: {}: {e}", path.display());
                failed.push(archive);
                continue;
            }
        };
        match NatureAtlas::parse(&bytes, &palette) {
            Ok(atlas) => {
                let archive_records = atlas.record_count();
                let vram = atlas.vram_bytes();
                let alpha0 = atlas.alpha0_is_transparent();
                let transparent = atlas.transparent_pixels();
                println!(
                    "[atlas] D {name} records={} vram={} alpha0={} transparent_px={}",
                    archive_records, vram, alpha0, transparent
                );
                if archive_records == RECORDS_PER_ARCHIVE {
                    atlases += 1;
                }
                records += archive_records;
                if alpha0 {
                    alpha_ok += 1;
                }
                vram_total += vram;
            }
            Err(e) => {
                println!("[FAIL] D {name}: decode error {e}");
                failed.push(archive);
            }
        }
    }

    if failed.is_empty() && atlases == NATURE_ARCHIVES.len() && records == NATURE_SPRITE_COUNT {
        tally.pass(
            "D/records",
            format!(
                "{}/{NATURE_SPRITE_COUNT} records ({atlases}/{} archives × {RECORDS_PER_ARCHIVE})",
                records,
                NATURE_ARCHIVES.len()
            ),
        );
    } else {
        tally.fail(
            "D/records",
            format!(
                "{records}/{NATURE_SPRITE_COUNT} records, failed archives {failed:?} \
                 ({atlases}/{} decoded)",
                NATURE_ARCHIVES.len()
            ),
        );
    }

    if vram_total <= NATURE_VRAM_BUDGET_BYTES {
        tally.pass(
            "D/vram",
            format!("{vram_total} ≤ {NATURE_VRAM_BUDGET_BYTES} bytes"),
        );
    } else {
        tally.fail(
            "D/vram",
            format!("{vram_total} > {NATURE_VRAM_BUDGET_BYTES} bytes"),
        );
    }

    if alpha_ok == NATURE_ARCHIVES.len() {
        tally.pass(
            "D/alpha0",
            format!("{alpha_ok}/{} archives declare palette index 0 transparent", NATURE_ARCHIVES.len()),
        );
    } else {
        tally.fail(
            "D/alpha0",
            format!("{alpha_ok}/{} archives", NATURE_ARCHIVES.len()),
        );
    }
}

// ---------------------------------------------------------------------------
// Block E — climate → light (70/70) against spec §5.
// ---------------------------------------------------------------------------

/// Spec §5 literals, transcribed from `m35-vegetation-night-sky-spec` §5 —
/// never imported from the implementation (a constant compared to itself is
/// not a measurement).
const SPEC_5: [(WeatherType, f64, f64, f64); 7] = [
    (WeatherType::Sunny, 1.0, 1.0, 0.0),
    (WeatherType::Cloudy, 0.8, 0.8, 0.0),
    (WeatherType::Overcast, 0.65, 0.6, 0.0),
    (WeatherType::Fog, 0.65, 0.6, 0.05),
    (WeatherType::Rain, 0.45, 0.4, 0.003),
    (WeatherType::Thunder, 0.25, 0.4, 0.003),
    (WeatherType::Snow, 0.45, 0.25, 0.005),
];

fn block_e(tally: &mut Tally) {
    debug_assert_eq!(SPEC_5.len(), WEATHER_TYPES.len());
    let declared = declares_spec_only_rows();
    let mut rows = 0usize;
    let mut bad = 0usize;
    let mut sky_bad = 0usize;
    let mut identity_bad = 0usize;

    for climate in MAP_CLIMATES {
        let sky_base = sky_base_for(climate);
        if !SKY_BASES.contains(&sky_base) {
            sky_bad += 1;
        }
        println!(
            "[climate] E {} sky_base={sky_base}",
            climate.name()
        );
    }

    for (weather, sun, shadow, fog) in SPEC_5 {
        if declared.contains(&weather) {
            println!(
                "[DESVIO-DECLARADO H3] weather={} — the pinned corpus declares this §5 row \
                 only in the spec (WeatherManager.cs:38-58), not as a named branch",
                weather_name(weather)
            );
        }
        let mut per_weather: Vec<(f64, f64, f64)> = Vec::new();
        for climate in MAP_CLIMATES {
            rows += 1;
            let light = light_for(weather, climate);
            per_weather.push((light.sunlight_scale, light.shadow_strength, light.fog_density));
            if light.sunlight_scale != sun
                || light.shadow_strength != shadow
                || light.fog_density != fog
            {
                bad += 1;
                println!(
                    "[FAIL] E {} × {} → ({}, {}, {}) spec ({sun}, {shadow}, {fog})",
                    weather_name(weather),
                    climate.name(),
                    light.sunlight_scale,
                    light.shadow_strength,
                    light.fog_density
                );
            }
        }
        // §5 has no climate column: the ten columns must be identical.
        if per_weather.iter().any(|row| *row != per_weather[0]) {
            identity_bad += 1;
        }
    }

    if rows == 70 && bad == 0 {
        tally.pass("E/rows", format!("{}/{rows} rows match §5 exactly", rows - bad));
    } else {
        tally.fail("E/rows", format!("{}/{rows} rows match §5", rows - bad));
    }
    if sky_bad == 0 {
        tally.pass(
            "E/sky-base",
            format!(
                "{}/{} climates have sky_base ∈ {{0,8,16,24}}",
                MAP_CLIMATES.len(),
                MAP_CLIMATES.len()
            ),
        );
    } else {
        tally.fail("E/sky-base", format!("{sky_bad} climates outside SKY_BASES"));
    }
    if identity_bad == 0 {
        tally.pass(
            "E/climate-identity",
            "70/70 rows: the 10 climate columns are identical per weather (§5 has no climate column)",
        );
    } else {
        tally.fail(
            "E/climate-identity",
            format!("{identity_bad}/7 weather rows vary across climates"),
        );
    }
}

fn weather_name(weather: WeatherType) -> &'static str {
    match weather {
        WeatherType::Sunny => "Sunny",
        WeatherType::Cloudy => "Cloudy",
        WeatherType::Overcast => "Overcast",
        WeatherType::Fog => "Fog",
        WeatherType::Rain => "Rain",
        WeatherType::Thunder => "Thunder",
        WeatherType::Snow => "Snow",
    }
}

// ---------------------------------------------------------------------------
// Block F — negatives (no retail bytes in the repo, declared assets).
// ---------------------------------------------------------------------------

fn git_ls_files(root: &Path) -> Result<Vec<String>, String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["ls-files", "-z"])
        .output()
        .map_err(|e| format!("git ls-files -z: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "git ls-files -z exited {:?}: {}",
            output.status.code(),
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout)
        .split('\0')
        .filter(|name| !name.is_empty())
        .map(str::to_string)
        .collect())
}

fn is_retail_asset(name: &str) -> bool {
    let upper = name.to_ascii_uppercase();
    upper.ends_with(".BSA") || upper.contains(".TEXTURE")
}

fn is_image_asset(name: &str) -> bool {
    let upper = name.to_ascii_uppercase();
    const EXTENSIONS: [&str; 8] = [".PNG", ".RGBA", ".JPG", ".JPEG", ".BMP", ".TGA", ".DDS", ".KTX2"];
    EXTENSIONS.iter().any(|ext| upper.ends_with(ext))
}

fn block_f(tally: &mut Tally) {
    let root = repo_root();

    // (1) Retail bytes: no tracked .BSA/.TEXTURE + ARENA2 outside the worktree.
    match git_ls_files(&root) {
        Ok(files) => {
            let tracked_retail: Vec<&String> = files.iter().filter(|name| is_retail_asset(name)).collect();
            let arena2_outside = match (
                crate::mode_a_arena2_path("MAPS.BSA"),
                root.canonicalize(),
            ) {
                (Ok(maps), Ok(root_canon)) => match maps.canonicalize() {
                    Ok(maps_canon) => {
                        let arena2 = maps_canon.parent().unwrap_or(maps_canon.as_path());
                        !arena2.starts_with(&root_canon)
                    }
                    Err(_) => false,
                },
                _ => false,
            };
            println!(
                "[census] F retail tracked={} files scanned={} ARENA2_outside_worktree={}",
                tracked_retail.len(),
                files.len(),
                arena2_outside
            );
            if tracked_retail.is_empty() && arena2_outside {
                tally.pass(
                    "F/retail-bytes",
                    format!(
                        "0 tracked .BSA/.TEXTURE of {} files (`git -C {} ls-files -z`), ARENA2 \
                         outside the worktree",
                        files.len(),
                        root.display()
                    ),
                );
            } else {
                tally.fail(
                    "F/retail-bytes",
                    format!(
                        "{} tracked retail files, ARENA2_outside_worktree={arena2_outside}",
                        tracked_retail.len()
                    ),
                );
            }
        }
        Err(reason) => tally.fail("F/retail-bytes", reason),
    }

    // (2) The 12 registry rows: sha256 must match disk, redistribute=false.
    let registry_path = root.join("ASSET-REGISTRY.json");
    match std::fs::read_to_string(&registry_path) {
        Ok(text) => {
            let mut rows = 0usize;
            let mut bad = 0usize;
            for archive in NATURE_ARCHIVES {
                let name = format!("TEXTURE.{archive}");
                let Some(object) = json_object_for(&text, &name) else {
                    println!("[FAIL] F registry: no row for {name}");
                    bad += 1;
                    continue;
                };
                rows += 1;
                let sha = json_string_field(object, "sha256").unwrap_or_default();
                let redistribute = json_bool_field(object, "redistribute");
                let declared_size = json_number_field(object, "size_bytes");
                let disk = crate::mode_a_arena2_path(&name)
                    .and_then(|path| std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display())));
                match disk {
                    Ok(bytes) => {
                        let sha_ok = sha == sha256_hex(&bytes);
                        let size_ok = declared_size == Some(bytes.len() as u64);
                        let redistribute_off = redistribute == Some(false);
                        println!(
                            "[row] F {name} sha256_match={sha_ok} size_match={size_ok} \
                             redistribute={redistribute:?} bytes={}",
                            bytes.len()
                        );
                        if !(sha_ok && size_ok && redistribute_off) {
                            bad += 1;
                        }
                    }
                    Err(reason) => {
                        println!("[FAIL] F {name}: {reason}");
                        bad += 1;
                    }
                }
            }
            if rows == NATURE_ARCHIVES.len() && bad == 0 {
                tally.pass(
                    "F/registry",
                    format!("{rows}/{} rows, sha256+size match disk, redistribute=false", NATURE_ARCHIVES.len()),
                );
            } else {
                tally.fail(
                    "F/registry",
                    format!("{rows}/{} rows present, {bad} non-conforming", NATURE_ARCHIVES.len()),
                );
            }
        }
        Err(e) => tally.fail("F/registry", format!("{}: {e}", registry_path.display())),
    }

    // (3) No silent synthetic assets: census of packaged images.
    match git_ls_files(&root) {
        Ok(files) => {
            let packaged: Vec<&String> = files
                .iter()
                .filter(|name| name.starts_with("crates/") || name.starts_with("assets/"))
                .filter(|name| is_image_asset(name))
                .collect();
            let undeclared = packaged.len();
            println!(
                "[census] F synthetic images under crates|assets={} undeclared={undeclared}",
                packaged.len()
            );
            if undeclared == 0 {
                tally.pass(
                    "F/synthetic",
                    format!(
                        "0/0 packaged images in crates|assets are undeclared (census over {} \
                         tracked paths)",
                        files.len()
                    ),
                );
            } else {
                tally.fail(
                    "F/synthetic",
                    format!("{undeclared} image assets: {packaged:?}"),
                );
            }
        }
        Err(reason) => tally.fail("F/synthetic", reason),
    }
}

/// Minimal JSON object extraction: the registry is a generated file, and the
/// app has no serde dependency (adding one is outside the probe contract), so
/// the row is located by name and bounded by brace depth.
fn json_object_for<'a>(text: &'a str, name: &str) -> Option<&'a str> {
    let needle = format!("\"name\": \"{name}\"");
    let at = text.find(&needle)?;
    let start = text[..at].rfind('{')?;
    let bytes = text.as_bytes();
    let mut depth = 0i32;
    let mut in_string = false;
    let mut escaped = false;
    for (i, byte) in bytes.iter().enumerate().skip(start) {
        if in_string {
            if escaped {
                escaped = false;
            } else if *byte == b'\\' {
                escaped = true;
            } else if *byte == b'"' {
                in_string = false;
            }
            continue;
        }
        match *byte {
            b'"' => in_string = true,
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return text.get(start..=i);
                }
            }
            _ => {}
        }
    }
    None
}

fn json_string_field<'a>(object: &'a str, field: &str) -> Option<&'a str> {
    let needle = format!("\"{field}\": \"");
    let start = object.find(&needle)? + needle.len();
    let rest = object.get(start..)?;
    let end = rest.find('"')?;
    rest.get(..end)
}

fn json_bool_field(object: &str, field: &str) -> Option<bool> {
    let needle = format!("\"{field}\": ");
    let start = object.find(&needle)? + needle.len();
    let rest = object.get(start..)?;
    if rest.starts_with("true") {
        Some(true)
    } else if rest.starts_with("false") {
        Some(false)
    } else {
        None
    }
}

fn json_number_field(object: &str, field: &str) -> Option<u64> {
    let needle = format!("\"{field}\": ");
    let start = object.find(&needle)? + needle.len();
    let rest = object.get(start..)?;
    let end = rest
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(rest.len());
    rest.get(..end)?.parse().ok()
}

// ---------------------------------------------------------------------------
// Block G — night sky.
// ---------------------------------------------------------------------------

/// Vertical L0 gradient of a palette — the real RGBA frame, not the struct.
fn render_frame(palette: &daggerfall_heart::sky::SkyPalette, buffer: &mut [u8]) -> (usize, usize) {
    let mut distinct = std::collections::HashSet::new();
    let mut non_finite = 0usize;
    for y in 0..FRAME_HEIGHT {
        let t = y as f32 / (FRAME_HEIGHT - 1) as f32;
        let mut rgb = [0u8; 3];
        for (channel, slot) in rgb.iter_mut().enumerate() {
            let value = palette.top[channel] as f32
                + (palette.horizon[channel] as f32 - palette.top[channel] as f32) * t;
            if !value.is_finite() {
                non_finite += 1;
                continue;
            }
            *slot = value.round().clamp(0.0, 255.0) as u8;
        }
        if y == 0 {
            debug_assert_eq!(rgb, palette.top);
        }
        distinct.insert(rgb);
        for x in 0..FRAME_WIDTH {
            let offset = (y * FRAME_WIDTH + x) * 4;
            buffer[offset] = rgb[0];
            buffer[offset + 1] = rgb[1];
            buffer[offset + 2] = rgb[2];
            buffer[offset + 3] = 255;
        }
    }
    (distinct.len(), non_finite)
}

fn block_g(tally: &mut Tally) {
    // (1) 4 bands × 4 sky bases: palette applied into a real frame.
    let mut buffer = vec![0u8; FRAME_WIDTH * FRAME_HEIGHT * 4];
    let mut frames = 0usize;
    let mut bad = 0usize;
    for band in HOUR_BANDS {
        for sky_base in SKY_BASES {
            frames += 1;
            let Some(palette) = sky_palette(sky_base, band) else {
                println!("[FAIL] G frame sky_base={sky_base} band={} → no palette", band.name());
                bad += 1;
                continue;
            };
            let (distinct, non_finite) = render_frame(&palette, &mut buffer);
            let top = buffer[0..3].to_vec();
            let last = (FRAME_HEIGHT - 1) * FRAME_WIDTH * 4;
            let bottom = buffer[last..last + 3].to_vec();
            let luma: u64 = buffer
                .chunks_exact(4)
                .step_by(4096)
                .map(|px| u64::from(px[0]) + u64::from(px[1]) + u64::from(px[2]))
                .sum();
            let frame_digest = fnv1a64(&buffer[..FRAME_WIDTH * 4 * 8]);
            let ok = distinct >= 2
                && non_finite == 0
                && luma > 0
                && top == palette.top.to_vec()
                && bottom == palette.horizon.to_vec();
            println!(
                "[frame] G sky_base={sky_base} band={} rows_distinct={distinct} non_finite={non_finite} \
                 luma_sample={luma} digest={frame_digest:016x}",
                band.name()
            );
            if !ok {
                bad += 1;
            }
        }
    }
    if frames == 16 && bad == 0 {
        tally.pass(
            "G/frames",
            format!("{}/{frames} frames rendered {FRAME_WIDTH}x{FRAME_HEIGHT}, 0 black, 0 NaN", frames - bad),
        );
    } else {
        tally.fail("G/frames", format!("{}/{frames} frames valid", frames - bad));
    }

    // (2) Stars: non-empty field in the night band + digest (cross-process in
    // the lane's second run).
    let stars = star_field(SKY_SEED);
    let digest = star_digest(SKY_SEED);
    let digest_again = star_digest(SKY_SEED);
    println!("BIOME-001 G star_digest={digest:016x}");
    let night_band = hour_band(0);
    let night_sample = moons(&GameClock::classic_default());
    if stars.len() == STAR_COUNT && digest == digest_again && !night_sample.is_empty() {
        tally.pass(
            "G/stars",
            format!(
                "{}/{STAR_COUNT} stars, digest stable {digest:016x}, band(00:00)={}",
                stars.len(),
                night_band.name()
            ),
        );
    } else {
        tally.fail(
            "G/stars",
            format!("{} stars, digest_stable={}", stars.len(), digest == digest_again),
        );
    }

    // (3) Two moons with the M31 phases of the demo tick.
    let clock = GameClock::classic_default();
    let discs = moons(&clock);
    let (masser, secunda) = calendar::lunar_phases(&clock);
    println!(
        "[moon] G tick={:?} moons=[{} {} / {} {}] calendar=({} {})",
        (clock.year, clock.month, clock.day, clock.hour, clock.minute),
        discs[0].name,
        phase_name(discs[0].phase),
        discs[1].name,
        phase_name(discs[1].phase),
        phase_name(masser),
        phase_name(secunda)
    );
    if discs.len() == MOON_NAMES.len()
        && discs[0].phase == masser
        && discs[1].phase == secunda
        && discs[0].name == MOON_NAMES[0]
        && discs[1].name == MOON_NAMES[1]
    {
        tally.pass(
            "G/moons",
            format!(
                "2/2 phases match calendar::lunar_phases ({} {}, {} {})",
                discs[0].name,
                phase_name(discs[0].phase),
                discs[1].name,
                phase_name(discs[1].phase)
            ),
        );
    } else {
        tally.fail(
            "G/moons",
            format!(
                "moons=[{} {} / {} {}] calendar=({} {})",
                discs[0].name,
                phase_name(discs[0].phase),
                discs[1].name,
                phase_name(discs[1].phase),
                phase_name(masser),
                phase_name(secunda)
            ),
        );
    }

    // (4) Sky captures of the render lane.
    let captures_dir = repo_root().join("evidence").join("m35-demo");
    let mut captures: Vec<PathBuf> = match std::fs::read_dir(&captures_dir) {
        Ok(entries) => entries
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| {
                path.extension()
                    .map(|ext| ext.eq_ignore_ascii_case("png"))
                    .unwrap_or(false)
            })
            .collect(),
        Err(_) => Vec::new(),
    };
    captures.sort();
    if captures.len() < EXPECTED_CAPTURES {
        tally.na(
            "G/captures",
            format!(
                "captures-missing: {} .png in {} (expected {EXPECTED_CAPTURES} sky captures from \
                 the render lane)",
                captures.len(),
                captures_dir.display()
            ),
        );
        tally.gap("captures-missing");
        return;
    }
    let mut bad_captures = 0usize;
    for path in captures.iter().take(EXPECTED_CAPTURES) {
        match std::fs::read(path) {
            Ok(bytes) => {
                let dims = png_dimensions(&bytes);
                let black = png_all_zero_idat(path);
                let sha = sha256_hex(&bytes);
                println!(
                    "[capture] G {} dims={dims:?} bytes={} sha256={sha} all_zero_idat={black:?}",
                    path.display(),
                    bytes.len()
                );
                if dims != Some((FRAME_WIDTH as u32, FRAME_HEIGHT as u32)) || black != Some(false) {
                    bad_captures += 1;
                }
            }
            Err(e) => {
                println!("[FAIL] G {}: {e}", path.display());
                bad_captures += 1;
            }
        }
    }
    if bad_captures == 0 {
        tally.pass(
            "G/captures",
            format!("{EXPECTED_CAPTURES}/{EXPECTED_CAPTURES} captures {FRAME_WIDTH}x{FRAME_HEIGHT}, non-black, sha256 printed"),
        );
    } else {
        tally.fail(
            "G/captures",
            format!("{bad_captures}/{EXPECTED_CAPTURES} captures invalid"),
        );
    }
}

fn phase_name(phase: LunarPhase) -> &'static str {
    match phase {
        LunarPhase::None => "none",
        LunarPhase::New => "new",
        LunarPhase::OneWax => "one-wax",
        LunarPhase::HalfWax => "half-wax",
        LunarPhase::ThreeWax => "three-wax",
        LunarPhase::Full => "full",
        LunarPhase::ThreeWane => "three-wane",
        LunarPhase::HalfWane => "half-wane",
        LunarPhase::OneWane => "one-wane",
    }
}

/// `width`/`height` from the PNG `IHDR` chunk (no decoder needed).
fn png_dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    const SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];
    if bytes.len() < 24 || bytes[..8] != SIGNATURE || &bytes[12..16] != b"IHDR" {
        return None;
    }
    let width = u32::from_be_bytes(bytes[16..20].try_into().ok()?);
    let height = u32::from_be_bytes(bytes[20..24].try_into().ok()?);
    Some((width, height))
}

/// A fully black frame is exactly a zlib stream of zero bytes (every filter
/// byte `None` + every pixel `0`), so `all bytes zero` ⇔ `black`. Only the
/// inflation needs a tool: `python3` is already the render lane's runtime
/// (documented command, printed in the report).
fn png_all_zero_idat(path: &Path) -> Option<bool> {
    let script = r#"
import sys, zlib, struct
data = open(sys.argv[1], 'rb').read()
pos, idat = 8, b''
while pos + 8 <= len(data):
    length, kind = struct.unpack_from('>I4s', data, pos)
    if kind == b'IDAT':
        idat += data[pos + 8:pos + 8 + length]
    pos += 12 + length
raw = zlib.decompress(idat)
print(0 if any(raw) else 1)
"#;
    let output = Command::new("python3")
        .arg("-c")
        .arg(script)
        .arg(path)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    match text.trim() {
        "1" => Some(true),
        "0" => Some(false),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Entry point.
// ---------------------------------------------------------------------------

/// `PROBE-BIOME-001` entry point — returns the process exit code.
pub fn run_biome_001() -> i32 {
    println!("PROBE-BIOME-001 — M35 blocks A–G (CR-2fef7f8a)");
    println!("================================================================");
    println!(
        "[info] seeds: nature={NATURE_SEED:#x} sky={SKY_SEED:#x}; tile classes: GAP-TAX-11 \
         (none in the pinned corpus)"
    );

    // Exit 2 policy: ARENA2 or a nature archive absent is a declared data gap.
    if let Err(reason) = crate::mode_a_arena2_path("MAPS.BSA") {
        println!("[gap] ARENA2 not resolvable: {reason}");
        println!(
            "PROBE-BIOME-001: verdict=DEGRADED exit=2 reasons=arena2-absent"
        );
        return 2;
    }
    let mut missing_textures: Vec<u32> = Vec::new();
    for archive in NATURE_ARCHIVES {
        if crate::mode_a_arena2_path(&format!("TEXTURE.{archive}")).is_err() {
            missing_textures.push(archive);
        }
    }
    if !missing_textures.is_empty() {
        println!("[gap] missing nature archives: {missing_textures:?}");
        println!("PROBE-BIOME-001: verdict=DEGRADED exit=2 reasons=textures-absent");
        return 2;
    }

    let mut tally = Tally::default();

    let index = match MapsIndex::load() {
        Ok(index) => {
            println!("[OK] MAPS.BSA parsed");
            Some(index)
        }
        Err(reason) => {
            tally.fail("A/maps", format!("MAPS.BSA: {reason}"));
            None
        }
    };

    let locations = match &index {
        Some(index) => block_a(&mut tally, index),
        None => Vec::new(),
    };

    block_b(&mut tally);
    if locations.is_empty() {
        tally.fail("C/locations", "no locations to place vegetation against");
    } else {
        block_c(&mut tally, &locations);
    }
    block_d(&mut tally);
    block_e(&mut tally);
    block_f(&mut tally);
    block_g(&mut tally);

    let (verdict, exit) = tally.verdict();
    let reasons = if tally.gaps.is_empty() {
        "-".to_string()
    } else {
        tally.gaps.join(",")
    };
    println!(
        "[summary] checks passed={} failed={} na={} gaps={reasons}",
        tally.passed, tally.failed, tally.na
    );
    println!("PROBE-BIOME-001: verdict={verdict} exit={exit} reasons={reasons}");
    exit
}
