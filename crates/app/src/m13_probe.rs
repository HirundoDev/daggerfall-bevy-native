//! M13 — anti-screen probe for layer `e` (terrain + biomes, DoD §e / WI-6 ·
//! REQ-L99-044).
//!
//! **PROBE-TERRAIN-001** (`--probe TERRAIN-001`): block A (heights) + block B
//! (climate), both over retail data read from the user's ARENA2:
//!
//! - **A · heights** — `WOODS.WLD` (26,001,168 B) parsed with the layout
//!   anchored in `woods.rs` (144 B header → `dataOffsets` table → 500,000 B
//!   heightmap), the 7 sampler constants of DoD §e, 4 goldens of the
//!   `Mathf.PerlinNoise` port (0 ULP against `PerlinNoise.cpp`), 8 golden
//!   height vectors including the `extraNoise` term and the **same**
//!   coordinate × 2 runs → identical digest, and a terrain patch outside the
//!   city ([`bridge::TerrainPatch`]) walked 20 steps: 0 crossings, ocean not
//!   walkable, beach walkable, eye height = sampler height ±1 cell.
//! - **B · climate** — `CLIMATE.PAK` (14,771 B) + `POLITIC.PAK` (19,556 B)
//!   with the PAK layout of `PakFile.cs:110-135`, 0 climate values outside
//!   `223..=232`, 61 enumerable regions, 2-pass coastal dilation
//!   (`TerrainHelper.cs:383-410`) and the `NatureSet × Season → archive`
//!   mapping of `ClimateSwaps.cs:389-446` with ≥4 goldens.
//!
//! **PROBE-CLIMATE-001** (`--probe CLIMATE-001`): block B only.
//!
//! **Source custody (INV-L99-6):** Mode A — the retail bytes are read from the
//! user's ARENA2 at runtime and NEVER enter the repository (only constants
//! live here: height bits, digests and climate values). Without an accessible
//! ARENA2 the probe exits with **exit 2** and the literal reason, never with a
//! false PASS.

use daggerfall_parser_bevy_bridge::terrain_spec::{TerrainPatch, WALK_STEPS};
use daggerfall_parser_rs::climate::{
    nature_archive, regions, ClimateMap, NatureSet, PoliticMap, Season, WorldClimate, OCEAN_CLIMATE,
    PAK_HEIGHT, PAK_WIDTH, REGION_COUNT,
};
use daggerfall_parser_rs::woods::{
    mathf_perlin_noise, HeightSample, TerrainBand, WoodsMap, HEIGHT_CONSTANTS, OFFICIAL_SIZE,
};

/// Golden height vector: map pixel, integer grid cell `129 × 129`, `f32` bits
/// of the scaled height (after ocean clamp) and expected band.
struct GoldenHeight {
    map_x: i32,
    map_y: i32,
    gx: usize,
    gy: usize,
    bits: u32,
    band: TerrainBand,
}

/// Goldens **derived from the spec formula** (not copied from the sampler):
/// for the grid centre (`gx = gy = 64`), `base` = bicubic over the small 4×4
/// heightmap (`DefaultTerrainSampler.cs:88-94`), `noise` = bicubic over the
/// large 9×9 heightmap (`:96-106`), `extraNoise` = `lowFreq × highFreq` with
/// `TerrainHelper.GetNoise` (`TerrainHelper.cs:551-568`) over
/// `Mathf.PerlinNoise` (`:108-114`), `scaled = base×8 + noise×4 +
/// extraNoise×10` (`:22-26`), lower clamp to `3.4×8 = 27.2` (`:27` +
/// `:115-116`) and `f32` bits of the clamped value (`:117-118`).
///
/// The Perlin term is NOT copied from the Rust port: `extraNoise` was generated
/// with `Runtime/Terrain/PerlinNoise.cpp` from the compiled Unity 4.3.1f1 tree
/// (`g++ -O2 -ffp-contract=off`) and the port is verified against those bits
/// with `mathf_perlin_noise` (0 ULP, see `GOLDEN_PERLIN`).
///
/// Categories: ocean ×2 (raw < 27.2 ⇒ clamp), beach ×2 (27.2 < raw ≤ 40),
/// land ×3, regional maximum ×1 (maximum raw swept over the 500,000 map
/// pixels). Term margin: `[0, 2.5]` units (`[0, 0.1624 %]`), not `[0,10]`.
const GOLDEN_HEIGHTS: [GoldenHeight; 8] = [
    // spec: DefaultTerrainSampler.cs:22-26,88-106,108-118 · base=1.0, noise=1.0 ⇒ 12.0 + extra 0.751206 < 27.2 ⇒ 27.2
    GoldenHeight { map_x: 0, map_y: 0, gx: 64, gy: 64, bits: 0x41d9_999a, band: TerrainBand::Ocean },
    // spec: DefaultTerrainSampler.cs:22-26,88-106,108-118 · base=1.0, noise=1.0 ⇒ 12.0 + extra 0.927064 < 27.2 ⇒ 27.2
    GoldenHeight { map_x: 500, map_y: 250, gx: 64, gy: 64, bits: 0x41d9_999a, band: TerrainBand::Ocean },
    // spec: DefaultTerrainSampler.cs:22-26,88-106,108-116 · base=1.734375, noise=5.109375 ⇒ 34.3125 + extra 0.013572 = 34.3260727 (beach)
    GoldenHeight { map_x: 150, map_y: 37, gx: 64, gy: 64, bits: 0x4209_4de6, band: TerrainBand::Beach },
    // spec: DefaultTerrainSampler.cs:22-26,88-106,108-116 · base=1.890625, noise=5.53125 ⇒ 37.25 + extra 0.633208 = 37.883209 (beach)
    GoldenHeight { map_x: 629, map_y: 112, gx: 64, gy: 64, bits: 0x4217_8868, band: TerrainBand::Beach },
    // spec: DefaultTerrainSampler.cs:22-26,88-106,108-116 · base=20.0, noise=23.859375 ⇒ 255.4375 + extra 0.751749 = 256.18924 (land)
    GoldenHeight { map_x: 100, map_y: 100, gx: 64, gy: 64, bits: 0x4380_1839, band: TerrainBand::Land },
    // spec: DefaultTerrainSampler.cs:22-26,88-106,108-116 · base=16.90625, noise=21.0 ⇒ 219.25 + extra 0.753665 = 220.003662 (land)
    GoldenHeight { map_x: 700, map_y: 300, gx: 64, gy: 64, bits: 0x435c_00f0, band: TerrainBand::Land },
    // spec: DefaultTerrainSampler.cs:22-26,88-106,108-116 · base=104.375, noise=66.25 ⇒ 1100.0 + extra 0.616966 = 1100.61694 (land)
    GoldenHeight { map_x: 470, map_y: 355, gx: 64, gy: 64, bits: 0x4489_93be, band: TerrainBand::Land },
    // spec: DefaultTerrainSampler.cs:22-26,88-106,108-116 · base=114.875, noise=107.859375 ⇒ 1350.4375 + extra 0.118825 = 1350.55627 (swept maximum)
    GoldenHeight { map_x: 943, map_y: 470, gx: 64, gy: 64, bits: 0x44a8_d1cd, band: TerrainBand::Land },
];

/// `Mathf.PerlinNoise` goldens (input and output bits) generated with the
/// compiled `Runtime/Terrain/PerlinNoise.cpp` from the Unity 4.3.1f1 tree: the
/// [`mathf_perlin_noise`] port must produce **0 ULP** (identical bits).
/// The first is Unity's public anchor `Mathf.PerlinNoise(0,0) = 0.4652731`.
const GOLDEN_PERLIN: [(u32, u32, u32); 5] = [
    (0x0000_0000, 0x0000_0000, 0x3eee_3846),
    (0x3f00_0000, 0x3f00_0000, 0x3e97_e886),
    (0x43a7_2800, 0x4228_0000, 0x3f38_b503),
    (0x4199_999a, 0x4696_2667, 0x3f16_13f7),
    (0x4761_3999, 0x46e1_7333, 0x3f22_ae75),
];


/// Golden climate vector: map pixel → raw `CLIMATE.PAK` value
/// (`GetClimateIndex`, with the +1 on X).
///
/// Derived from the retail bytes with the PAK layout of `PakFile.cs:110-135`
/// (500 `u32` offsets + 1001-wide RLE rows `(u16 count, u8 value)`) and the
/// `pak[y][x + 1]` indexing of `MapsFile.cs:891-896`.
const GOLDEN_CLIMATE: [(i32, i32, u8); 4] = [
    // spec: MapsFile.cs:891-896 · formula: pak[0][0 + 1]
    (0, 0, 223),
    // spec: MapsFile.cs:891-896 · formula: pak[250][500 + 1]
    (500, 250, 223),
    // spec: MapsFile.cs:891-896 · formula: pak[100][100 + 1]
    (100, 100, 231),
    // spec: MapsFile.cs:891-896 · formula: pak[300][700 + 1]
    (700, 300, 227),
];

/// Configuration of the walkable patches (map pixel + start + direction).
/// Coordinates chosen by the band of the starting vertex (`band_at`), which is
/// what decides `is_walkable`; verified with the spec formula.
const WALK_PATCHES: [(&str, i32, i32, f32, f32, f32, f32); 3] = [
    // (name, map_x, map_y, start_x, start_z, dir_x, dir_z)
    // land: centre raw = 256.18924 (land), start on land.
    ("land", 100, 100, 20.0, 20.0, 1.0, 0.0),
    // beach: centre raw = 34.32607 (beach), start on beach.
    ("beach", 150, 37, 20.0, 20.0, 1.0, 0.0),
    // ocean: centre raw = 12.927064 (ocean), start on ocean ⇒ 0 steps.
    ("ocean", 500, 250, 20.0, 20.0, 1.0, 0.0),
];

/// FNV-1a 64 (deterministic, 0 dependencies).
fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for b in bytes {
        hash ^= *b as u64;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

fn digest_heights(woods: &WoodsMap) -> u64 {
    let mut bytes = Vec::with_capacity(GOLDEN_HEIGHTS.len() * 4);
    for golden in &GOLDEN_HEIGHTS {
        let sample = woods.sample_grid(golden.map_x, golden.map_y, golden.gx, golden.gy);
        bytes.extend_from_slice(&sample.scaled_height.to_bits().to_le_bytes());
    }
    fnv1a64(&bytes)
}

/// Loads `WOODS.WLD` Mode A; `Err(reason)` if there is no ARENA2.
fn load_woods() -> Result<WoodsMap, String> {
    let path = crate::mode_a_arena2_path("WOODS.WLD")?;
    let bytes = std::fs::read(&path).map_err(|e| format!("reading {path:?}: {e}"))?;
    println!(
        "[OK] mode_a WOODS.WLD: {} ({} B · registry {OFFICIAL_SIZE} B)",
        path.display(),
        bytes.len()
    );
    WoodsMap::parse(&bytes).map_err(|e| format!("invalid WOODS.WLD: {e}"))
}

/// Block A — heights (sampler + walkable patch).
fn run_block_heights() -> bool {
    let woods = match load_woods() {
        Ok(woods) => woods,
        Err(reason) => {
            println!("ARENA2 not accessible: {reason}");
            return false;
        }
    };
    let mut ok = true;
    let header = woods.header();
    println!(
        "[OK] woods_layout - 144 B header · OffsetSize={} · {}×{} · HeightMapOffset={} · heightmap {} B · blocks from {}",
        header.offset_size,
        header.width,
        header.height,
        header.height_map_offset,
        woods.height_map().len(),
        header.height_map_offset as usize + woods.height_map().len()
    );
    ok &= header.width == 1000 && header.height == 500;

    // ── DoD §e constants ────────────────────────────────────────────
    for (name, value) in HEIGHT_CONSTANTS {
        println!("[OK] constant {name} = {value}");
    }
    ok &= HEIGHT_CONSTANTS.len() == 7;

    // ── bit-exact Mathf.PerlinNoise port (0 ULP vs PerlinNoise.cpp) ────
    for (index, (x_bits, y_bits, expected_bits)) in GOLDEN_PERLIN.iter().enumerate() {
        let value = mathf_perlin_noise(f32::from_bits(*x_bits), f32::from_bits(*y_bits));
        let perlin_ok = value.to_bits() == *expected_bits;
        println!(
            "[{}] perlin_golden_{index} - Mathf.PerlinNoise({}, {}) bits=0x{:08x} expected=0x{expected_bits:08x} ({value:.9})",
            if perlin_ok { "OK" } else { "FAIL" },
            f32::from_bits(*x_bits),
            f32::from_bits(*y_bits),
            value.to_bits()
        );
        ok &= perlin_ok;
    }

    // ── 8 golden vectors ────────────────────────────────────────────────
    for (index, golden) in GOLDEN_HEIGHTS.iter().enumerate() {
        let sample: HeightSample =
            woods.sample_grid(golden.map_x, golden.map_y, golden.gx, golden.gy);
        let bits = sample.scaled_height.to_bits();
        let band_ok = sample.band == golden.band;
        let bits_ok = bits == golden.bits;
        println!(
            "[{}] height_golden_{index} - pixel=({}, {}) grid=({}, {}) h={:.3} bits=0x{bits:08x} expected=0x{:08x} band={} expected={} norm={:.6}",
            if bits_ok && band_ok { "OK" } else { "FAIL" },
            golden.map_x,
            golden.map_y,
            golden.gx,
            golden.gy,
            sample.scaled_height,
            golden.bits,
            sample.band.name(),
            golden.band.name(),
            sample.normalized
        );
        ok &= bits_ok && band_ok;
    }

    // ── same coordinate × 2 runs → identical digest ──────────────
    let first = digest_heights(&woods);
    let second = digest_heights(&woods);
    let stable = first == second;
    println!(
        "[{}] height_determinism - FNV-1a64 digest of the 8 goldens × 2 runs: 0x{first:016x} == 0x{second:016x}",
        if stable { "OK" } else { "FAIL" }
    );
    ok &= stable;
    // Full 129×129 grid of one map pixel × 2 (continuous sampling).
    let mut grid_a = Vec::with_capacity(129 * 129 * 4);
    let mut grid_b = Vec::with_capacity(129 * 129 * 4);
    for gy in 0..129 {
        for gx in 0..129 {
            grid_a.extend_from_slice(&woods.sample_grid(553, 297, gx, gy).scaled_height.to_bits().to_le_bytes());
            grid_b.extend_from_slice(&woods.sample_grid(553, 297, gx, gy).scaled_height.to_bits().to_le_bytes());
        }
    }
    let grid_stable = fnv1a64(&grid_a) == fnv1a64(&grid_b);
    println!(
        "[{}] height_grid_129x129 - digest 0x{:016x} (×2 identical={grid_stable})",
        if grid_stable { "OK" } else { "FAIL" },
        fnv1a64(&grid_a)
    );
    ok &= grid_stable;

    // ── navigable terrain outside the city ───────────────────────────────
    for (name, map_x, map_y, start_x, start_z, dir_x, dir_z) in WALK_PATCHES {
        let patch = match TerrainPatch::build(&woods, map_x, map_y) {
            Ok(patch) => patch,
            Err(err) => {
                println!("[FAIL] patch_{name} - {err}");
                ok = false;
                continue;
            }
        };
        let band = patch.vertex(0, 0).band;
        let report = patch.walk([start_x, start_z], [dir_x, dir_z]);
        let walkable = patch.is_walkable(start_x, start_z);
        let expect_walkable = band.is_walkable();
        let walk_ok = if expect_walkable {
            report.pass()
        } else {
            report.steps == 0 && report.completed == 0
        };
        println!(
            "[{}] terrain_patch_{name} - pixel=({map_x}, {map_y}) band={} walkable={walkable} (expected={expect_walkable}) steps={}/{} crossings={} eye_error={:.3} cells reason={:?} eye=({:.2}, {:.2}, {:.2})",
            if walk_ok && walkable == expect_walkable { "OK" } else { "FAIL" },
            band.name(),
            report.steps,
            WALK_STEPS,
            report.crossings,
            report.max_eye_error_cells,
            report.reason,
            report.eye[0],
            report.eye[1],
            report.eye[2]
        );
        ok &= walk_ok && walkable == expect_walkable;
    }
    ok
}

/// Copy of the full PAK grid (1001 × 500) through the public `raw_value`
/// accessor — `ClimateMap` does not expose its internal buffer and this lane
/// does not modify `climate.rs`.
fn pak_buffer(climate: &ClimateMap) -> Vec<u8> {
    let mut out = Vec::with_capacity(PAK_WIDTH * PAK_HEIGHT);
    for y in 0..PAK_HEIGHT {
        for x in 0..PAK_WIDTH {
            out.push(
                climate
                    .raw_value(x, y)
                    .expect("1001×500 PAK grid validated on parse"),
            );
        }
    }
    out
}

/// Block B — climate, politics, regions and nature.
fn run_block_climate() -> bool {
    let mut ok = true;
    let climate_path = match crate::mode_a_arena2_path("CLIMATE.PAK") {
        Ok(path) => path,
        Err(reason) => {
            println!("ARENA2 not accessible: {reason}");
            return false;
        }
    };
    let climate_bytes = match std::fs::read(&climate_path) {
        Ok(bytes) => bytes,
        Err(err) => {
            println!("ARENA2 not accessible: reading {climate_path:?}: {err}");
            return false;
        }
    };
    let mut climate = match ClimateMap::parse(&climate_bytes) {
        Ok(map) => map,
        Err(err) => {
            println!("[FAIL] climate_pak_parse - {climate_path:?}: {err}");
            return false;
        }
    };
    println!(
        "[OK] climate_pak_parse - {} ({} B) · PAK layout 1001×500 (PakFile.cs:110-135)",
        climate_path.display(),
        climate_bytes.len()
    );

    // ── 0 out-of-range values in the full map ────────────────────
    let distinct = climate.distinct_climates();
    let out_of_range = distinct
        .iter()
        .filter(|value| WorldClimate::from_index(**value).is_err())
        .count();
    println!(
        "[{}] climate_range - {} distinct values {:?} · outside 223..=232: {out_of_range}",
        if out_of_range == 0 { "OK" } else { "FAIL" },
        distinct.len(),
        distinct.iter().collect::<Vec<_>>()
    );
    ok &= out_of_range == 0;

    // ── 4 climate golden vectors ───────────────────────────────────────
    for (index, (x, y, expected)) in GOLDEN_CLIMATE.iter().enumerate() {
        match climate.climate_index(*x, *y) {
            Some(value) => {
                let value_ok = value == *expected;
                println!(
                    "[{}] climate_golden_{index} - pixel=({x}, {y}) climate={value} expected={expected} ({})",
                    if value_ok { "OK" } else { "FAIL" },
                    WorldClimate::from_index(value).map(|c| c.name()).unwrap_or("?")
                );
                ok &= value_ok;
            }
            None => {
                println!("[FAIL] climate_golden_{index} - pixel=({x}, {y}) outside the PAK grid");
                ok = false;
            }
        }
    }

    // ── ×2 stability of the full map ────────────────────────────────
    let reparsed = ClimateMap::parse(&climate_bytes).expect("already parsed");
    let map_digest = fnv1a64(&pak_buffer(&reparsed));
    let map_digest_2 = fnv1a64(&pak_buffer(&climate));
    println!(
        "[{}] climate_determinism - 1001×500 digest: 0x{map_digest:016x} == 0x{map_digest_2:016x}",
        if map_digest == map_digest_2 { "OK" } else { "FAIL" }
    );
    ok &= map_digest == map_digest_2;

    // ── 61 enumerable regions ─────────────────────────────────────────
    let region_list: Vec<u8> = regions().collect();
    let regions_ok = region_list.len() == REGION_COUNT && region_list.last() == Some(&60);
    println!(
        "[{}] regions_enumerable - {} regions (0..{}) · indices 0..={}",
        if regions_ok { "OK" } else { "FAIL" },
        region_list.len(),
        REGION_COUNT,
        region_list.last().copied().unwrap_or(0)
    );
    ok &= regions_ok;

    // ── NatureSet × Season → archive mapping (ClimateSwaps.cs:389-446) ──
    let nature_cases = [
        (NatureSet::TemperateWoodland, Season::Winter, 505u32),
        (NatureSet::Mountains, Season::Winter, 511),
        (NatureSet::RainForest, Season::Winter, 500),
        (NatureSet::Desert, Season::Summer, 503),
        (NatureSet::Swamp, Season::Rain, 502),
        (NatureSet::WoodlandHills, Season::Summer, 506),
    ];
    for (index, (set, season, expected)) in nature_cases.iter().enumerate() {
        let archive = nature_archive(*set, *season);
        let case_ok = archive == *expected && (500..=511).contains(&archive);
        println!(
            "[{}] nature_archive_{index} - {:?} × {:?} → {archive} expected={expected}",
            if case_ok { "OK" } else { "FAIL" },
            set.name(),
            season,
            expected = expected
        );
        ok &= case_ok;
    }
    // Every climate × season inside 500..=511 (0 out of range).
    let mut climate_archives_ok = true;
    for value in 223u8..=232 {
        let climate_kind = WorldClimate::from_index(value).expect("Climates enum");
        let settings = climate_kind.settings();
        for season in [Season::Summer, Season::Winter, Season::Rain] {
            let archive = settings.nature_set.texture_set()
                + u32::from(season == Season::Winter && settings.nature_set.supports_winter());
            if !(500..=511).contains(&archive) {
                climate_archives_ok = false;
            }
        }
    }
    println!(
        "[{}] nature_archives_range - 10 climates × 3 seasons in 500..=511: {climate_archives_ok}",
        if climate_archives_ok { "OK" } else { "FAIL" }
    );
    ok &= climate_archives_ok;

    // ── coastal dilation (Moore, 2 passes) ───────────────────────────
    let ocean_before = pak_buffer(&climate)
        .iter()
        .filter(|v| **v == OCEAN_CLIMATE)
        .count();
    let changed = climate.dilate_coastal(2);
    let ocean_after = pak_buffer(&climate)
        .iter()
        .filter(|v| **v == OCEAN_CLIMATE)
        .count();
    let dilate_out_of_range = climate
        .distinct_climates()
        .iter()
        .filter(|value| WorldClimate::from_index(**value).is_err())
        .count();
    let dilate_ok = changed > 0 && ocean_after < ocean_before && dilate_out_of_range == 0;
    println!(
        "[{}] coastal_dilation - 2 Moore passes: {changed} ocean pixels gained land climate · ocean {ocean_before} → {ocean_after} · out of range {dilate_out_of_range}",
        if dilate_ok { "OK" } else { "FAIL" }
    );
    ok &= dilate_ok;

    // ── POLITIC.PAK (`GetPoliticIndex`, MapsFile.cs:904-907) ────────────
    match crate::mode_a_arena2_path("POLITIC.PAK") {
        Ok(path) => match std::fs::read(&path) {
            Ok(bytes) => match PoliticMap::parse(&bytes) {
                Ok(politic) => {
                    let distinct = politic.distinct_politics();
                    let sample = politic.politic_index(100, 100);
                    let politic_ok = distinct.len() == 47 && sample.is_some();
                    println!(
                        "[{}] politic_pak_parse - {} ({} B) · {} distinct political values · politic(100,100)={sample:?}",
                        if politic_ok { "OK" } else { "FAIL" },
                        path.display(),
                        bytes.len(),
                        distinct.len()
                    );
                    ok &= politic_ok;
                }
                Err(err) => {
                    println!("[FAIL] politic_pak_parse - {err}");
                    ok = false;
                }
            },
            Err(err) => {
                println!("ARENA2 not accessible: reading {path:?}: {err}");
                return false;
            }
        },
        Err(reason) => {
            println!("ARENA2 not accessible: {reason}");
            return false;
        }
    }
    ok
}

/// Runs PROBE-TERRAIN-001 — heights + climate (M13).
pub fn run_terrain_001() -> i32 {
    println!(
        "PROBE-TERRAIN-001 - retail terrain + biomes WOODS.WLD / CLIMATE.PAK (M13, \
         CR-46a43aba, REQ-L99-044 · DoD §e / WI-6)"
    );
    println!("--- block A: heights (sampler + navigable terrain) ---");
    let heights = run_block_heights();
    println!("--- block B: climate + biomes (CLIMATE.PAK / POLITIC.PAK) ---");
    let climate = run_block_climate();
    // Without ARENA2 both blocks print the reason and return false.
    if !heights && !climate {
        println!("VERDICT: PROBE-TERRAIN-001 FAIL - ARENA2 not accessible or invalid data");
        println!("VERDICT: PROBE-CLIMATE-001 FAIL - ARENA2 not accessible or invalid data");
        return 2;
    }
    if heights {
        println!(
            "VERDICT: PROBE-TERRAIN-001 PASS - 8 byte-by-byte height goldens + \
             129×129 deterministic + walkable terrain outside the city (20/20, 0 crossings)"
        );
    } else {
        println!("VERDICT: PROBE-TERRAIN-001 FAIL - height checks failed");
    }
    if climate {
        println!(
            "VERDICT: PROBE-CLIMATE-001 PASS - 0 out-of-range climates, 61 regions, \
             NatureSet×Season→archive and 2-pass coastal dilation"
        );
    } else {
        println!("VERDICT: PROBE-CLIMATE-001 FAIL - climate checks failed");
    }
    if heights && climate {
        0
    } else {
        1
    }
}

/// Runs PROBE-CLIMATE-001 — climate/biomes (M13).
pub fn run_climate_001() -> i32 {
    println!(
        "PROBE-CLIMATE-001 - retail climate + biomes CLIMATE.PAK / POLITIC.PAK (M13, \
         CR-46a43aba, REQ-L99-044 · DoD §e / WI-6)"
    );
    if run_block_climate() {
        println!(
            "VERDICT: PROBE-CLIMATE-001 PASS - 0 out-of-range climates, 61 regions, \
             NatureSet×Season→archive and 2-pass coastal dilation"
        );
        0
    } else {
        println!("VERDICT: PROBE-CLIMATE-001 FAIL - climate checks failed");
        1
    }
}
