//! Deterministic nature placement per map-pixel (M35 / V2,
//! `PROBE-BIOME-001` block C).
//!
//! Normative source:
//! `docs/sintergic/game/programming/m35-vegetation-night-sky-spec-CR-f716455c.md`
//! §3.2 — every constant of the placement table is ported literally, each one
//! carrying its spec row in the doc-comment. §3.1 is the climate → archive map
//! used by the seasonal swap (positions never move, only the archive changes).
//!
//! Gap declared by the spec itself (§9 R3, `GAP-TAX-11`): the tile-type data
//! (`tilemapSamples & 0x3F`) is **not** in the pinned corpus, so the density
//! gate is measurable only for samples whose class the caller actually has.
//! [`place_nature`] therefore never invents a tile type: a
//! [`TileClass::Unknown`] sample skips the probability gate, is counted in
//! [`NaturePlacement::samples_without_class`] and sets
//! [`NaturePlacement::probability_gate_unavailable`]. The other three rules
//! (band, clearance, steepness) still apply.
//!
//! Determinism: the output is a pure function of
//! `(map pixel, seed, season, samples, locations)`. The random stream is
//! seeded from [`make_terrain_key`] (spec §3.2 "Seed" row) through a
//! SplitMix64 generator; the RNG algorithm itself is **not** in the corpus
//! (gap H1: `TerrainNature.cs` is absent from the pinned corpus), so the probe
//! measures determinism, record range and rejection rules — never bit parity
//! with the original RNG.
//!
//! Bridge rules (PRE-2 §8): pure module — 0 Bevy, 0 input, 0 persistence.

use daggerfall_parser_rs::climate::{nature_archive, Season, WorldClimate, WorldSeason};
use daggerfall_parser_rs::woods::TerrainBand;

use crate::terrain_spec::TerrainVertex;

/// `WorldMapTileDim`: terrain tiles per map-pixel (spec §3.2 row "Grid",
/// `MapsFile.cs:33`).
pub const WORLD_MAP_TILE_DIM: i32 = 128;

/// Clearance around a location rect, in tiles (spec §3.2 row "Location
/// exclusion", `TerrainNature.cs:52,58-64`).
pub const NATURE_CLEARANCE_TILES: i32 = 4;

/// Steepness rejection threshold in degrees (spec §3.2 row "Steepness",
/// `TerrainNature.cs:47,117-119`): `steepness > 50°` is rejected.
pub const MAX_STEEPNESS_DEG: f32 = 50.0;

/// Base acceptance probability on dirt tiles (spec §3.2 row "Base
/// probability", `TerrainNature.cs:49-51`).
pub const PROB_DIRT: f32 = 0.2;

/// Base acceptance probability on grass tiles (spec §3.2 row "Base
/// probability", `TerrainNature.cs:49-51`).
pub const PROB_GRASS: f32 = 0.9;

/// Base acceptance probability on stone tiles (spec §3.2 row "Base
/// probability", `TerrainNature.cs:49-51`).
pub const PROB_STONE: f32 = 0.05;

/// Lower bound of the height scale (spec §3.2 row "Scale by height",
/// `TerrainNature.cs:70-72`).
pub const HEIGHT_SCALE_MIN: f32 = 0.4;

/// Upper bound of the height scale (spec §3.2 row "Scale by height",
/// `TerrainNature.cs:70-72`).
pub const HEIGHT_SCALE_MAX: f32 = 1.0;

/// Divisor of the height scale (spec §3.2 row "Scale by height":
/// `clamp(worldHeight / 128, 0.4, 1.0)`).
pub const HEIGHT_SCALE_DIVISOR: f32 = 128.0;

/// Climate scale multiplier of desert climates (spec §3.2 row "Scale by
/// climate", `TerrainNature.cs:74-81`).
pub const DESERT_SCALE: f32 = 0.25;

/// Smallest record a billboard may use (spec §2 row "Placed record" and §3.2
/// row "Record": uniform `1..31`, record 0 never).
pub const RECORD_MIN: u8 = 1;

/// Largest record a billboard may use (spec §3.2 row "Record",
/// `TerrainNature.cs:163`).
pub const RECORD_MAX: u8 = 31;

/// Divisor of the sink offset (spec §3.2 row "Sinking":
/// `y = height − steepness / 70`).
pub const SINK_DIVISOR: f32 = 70.0;

/// Batch cap per map-pixel (spec §3.2 row "Batch cap": `16 129 = 127²`,
/// `TerrainNature.cs:104`).
pub const MAX_BILLBOARDS: usize = 16_129;

/// FNV-1a 64 offset basis (canonical determinism hash of the workspace, the
/// same one `crates/app/src/m34_light_probe.rs` uses).
const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;

/// FNV-1a 64 prime.
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

/// SplitMix64 increment.
const SPLITMIX_INCREMENT: u64 = 0x9e37_79b9_7f4a_7c15;

/// FNV-1a 64 continued from `state`.
fn fnv1a64(bytes: &[u8], state: u64) -> u64 {
    let mut hash = state;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    hash
}

/// `MakeTerrainKey(mx, my)`: `(int16 my << 16) + int16 mx` (spec §3.2 row
/// "Seed", `TerrainHelper.cs:477-480`).
pub fn make_terrain_key(mx: i32, my: i32) -> u32 {
    ((my as i16 as u16 as u32) << 16) | (mx as i16 as u16 as u32)
}

/// Terrain key of a map-pixel: the seed of every deterministic draw of that
/// pixel (spec §3.2 row "Seed").
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TerrainKey(pub u32);

impl TerrainKey {
    /// Key of map-pixel `(mx, my)` through [`make_terrain_key`].
    pub fn of(mx: i32, my: i32) -> Self {
        Self(make_terrain_key(mx, my))
    }

    /// Raw `u32` value (the `int` of `TerrainHelper.cs:477-480`).
    pub fn value(self) -> u32 {
        self.0
    }
}

/// Source tile class of a sample.
///
/// The original derives it from `tilemapSamples & 0x3F` — `1` dirt, `2` grass,
/// `3` stone (spec §3.2 row "Source tiles", `TerrainNature.cs:128-146`). That
/// data is **not** in the pinned corpus (`GAP-TAX-11`, spec §9 R3), so
/// [`TileClass::Unknown`] is a first-class value: it is measured, never
/// simulated.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TileClass {
    /// `tilemapSamples & 0x3F == 1`.
    Dirt,
    /// `tilemapSamples & 0x3F == 2`.
    Grass,
    /// `tilemapSamples & 0x3F == 3`.
    Stone,
    /// No tile-type source available (`GAP-TAX-11`): the probability gate is
    /// not measurable for this sample.
    Unknown,
}

impl TileClass {
    /// Class of a raw `tilemapSamples & 0x3F` value (spec §3.2 row "Source
    /// tiles"). Any value other than `1..=3` is [`TileClass::Unknown`].
    pub fn from_tilemap_sample(sample: u8) -> Self {
        match sample {
            1 => Self::Dirt,
            2 => Self::Grass,
            3 => Self::Stone,
            _ => Self::Unknown,
        }
    }

    /// Acceptance probability of the class, or `None` when the class is not
    /// known (`GAP-TAX-11`).
    pub fn probability(self) -> Option<f32> {
        match self {
            Self::Dirt => Some(PROB_DIRT),
            Self::Grass => Some(PROB_GRASS),
            Self::Stone => Some(PROB_STONE),
            Self::Unknown => None,
        }
    }

    /// Stable name for probes and digests.
    pub fn name(self) -> &'static str {
        match self {
            Self::Dirt => "dirt",
            Self::Grass => "grass",
            Self::Stone => "stone",
            Self::Unknown => "unknown",
        }
    }
}

/// One terrain sample the placement rules can decide on.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NatureSample {
    /// Tile inside the map-pixel (`0..WORLD_MAP_TILE_DIM`, same tile space as
    /// [`NatureRect`]).
    pub tile: [u16; 2],
    /// Terrain height in meters — the unit of
    /// [`crate::terrain_spec::TerrainVertex::height_m`], which is what the
    /// spec §3.2 literals (`/ 128`, `/ 70`) operate on.
    pub height: f32,
    /// Terrain steepness in degrees, as measured by the sampler.
    pub steepness_deg: f32,
    /// Terrain band of the sample, as measured by the `WOODS.WLD` sampler
    /// (`DefaultTerrainSampler`: ocean / beach / land).
    pub band: TerrainBand,
    /// Source tile class (`GAP-TAX-11` when unknown).
    pub tile_class: TileClass,
}

impl NatureSample {
    /// Builds a sample from a terrain patch vertex, taking `height` and `band`
    /// from the bridge's measurement (the caller supplies the steepness and
    /// the tile class it actually has).
    pub fn from_vertex(
        tile: [u16; 2],
        vertex: &TerrainVertex,
        steepness_deg: f32,
        tile_class: TileClass,
    ) -> Self {
        Self {
            tile,
            height: vertex.height_m,
            steepness_deg,
            band: vertex.band,
            tile_class,
        }
    }
}

/// Location footprint in tiles, the rectangle the original expands by
/// [`NATURE_CLEARANCE_TILES`] (spec §3.2 row "Location exclusion").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NatureRect {
    /// Inclusive minimum corner in tiles.
    pub min: [i32; 2],
    /// Inclusive maximum corner in tiles.
    pub max: [i32; 2],
}

impl NatureRect {
    /// Rectangle grown by `tiles` on every side.
    pub fn expanded(self, tiles: i32) -> Self {
        Self {
            min: [
                self.min[0].saturating_sub(tiles),
                self.min[1].saturating_sub(tiles),
            ],
            max: [
                self.max[0].saturating_add(tiles),
                self.max[1].saturating_add(tiles),
            ],
        }
    }

    /// `true` when `tile` lies inside the rectangle (inclusive bounds).
    pub fn contains(self, tile: [u16; 2]) -> bool {
        let x = i32::from(tile[0]);
        let y = i32::from(tile[1]);
        x >= self.min[0] && x <= self.max[0] && y >= self.min[1] && y <= self.max[1]
    }
}

/// Placement rules of one map-pixel: the seed and every constant of spec §3.2
/// grouped so the probe can print each one with its source.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NatureRules {
    /// Determinism seed (`(mx, my, season, seed)` is the full input set).
    pub seed: u64,
    /// Climate of the map-pixel (decides the desert scale and the archive).
    pub climate: WorldClimate,
    /// Clearance around a location rect, in tiles.
    pub clearance_tiles: i32,
    /// Steepness rejection threshold in degrees.
    pub max_steepness_deg: f32,
    /// Acceptance probability on dirt.
    pub prob_dirt: f32,
    /// Acceptance probability on grass.
    pub prob_grass: f32,
    /// Acceptance probability on stone.
    pub prob_stone: f32,
    /// Lower bound of the height scale.
    pub height_scale_min: f32,
    /// Upper bound of the height scale.
    pub height_scale_max: f32,
    /// Divisor of the height scale.
    pub height_scale_divisor: f32,
    /// Multiplier applied to desert climates.
    pub desert_scale: f32,
    /// Smallest record a billboard may use.
    pub record_min: u8,
    /// Largest record a billboard may use.
    pub record_max: u8,
    /// Divisor of the sink offset.
    pub sink_divisor: f32,
    /// Batch cap per map-pixel.
    pub max_billboards: usize,
}

impl NatureRules {
    /// Every constant of spec §3.2 at its documented value, with `seed` and
    /// `climate` supplied by the caller.
    pub fn new(seed: u64, climate: WorldClimate) -> Self {
        Self {
            seed,
            climate,
            clearance_tiles: NATURE_CLEARANCE_TILES,
            max_steepness_deg: MAX_STEEPNESS_DEG,
            prob_dirt: PROB_DIRT,
            prob_grass: PROB_GRASS,
            prob_stone: PROB_STONE,
            height_scale_min: HEIGHT_SCALE_MIN,
            height_scale_max: HEIGHT_SCALE_MAX,
            height_scale_divisor: HEIGHT_SCALE_DIVISOR,
            desert_scale: DESERT_SCALE,
            record_min: RECORD_MIN,
            record_max: RECORD_MAX,
            sink_divisor: SINK_DIVISOR,
            max_billboards: MAX_BILLBOARDS,
        }
    }

    /// `true` for the desert climates (spec §3.2 row "Scale by climate":
    /// `Desert → 0.25`; `Desert2` shares the climate settings of `Desert`).
    pub fn is_desert(&self) -> bool {
        matches!(self.climate, WorldClimate::Desert | WorldClimate::Desert2)
    }

    /// Billboard scale of a height (spec §3.2 rows "Scale by height" and
    /// "Scale by climate").
    pub fn height_scale(&self, height: f32) -> f32 {
        let base = (height / self.height_scale_divisor)
            .clamp(self.height_scale_min, self.height_scale_max);
        if self.is_desert() {
            base * self.desert_scale
        } else {
            base
        }
    }

    /// Sink offset of a billboard (spec §3.2 row "Sinking").
    pub fn sink(&self, height: f32, steepness_deg: f32) -> f32 {
        height - steepness_deg / self.sink_divisor
    }

    /// Acceptance probability of a tile class, `None` when the class is
    /// unknown (`GAP-TAX-11`: the gate is not measurable).
    pub fn accept_probability(&self, class: TileClass) -> Option<f32> {
        match class {
            TileClass::Dirt => Some(self.prob_dirt),
            TileClass::Grass => Some(self.prob_grass),
            TileClass::Stone => Some(self.prob_stone),
            TileClass::Unknown => None,
        }
    }
}

/// One documented constant of spec §3.2, with its value and anchor.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DocumentedConstant {
    /// Constant name as the probe prints it.
    pub name: &'static str,
    /// Value.
    pub value: f64,
    /// Spec row and original anchor.
    pub source: &'static str,
}

/// Every constant of spec §3.2, in table order, so the probe prints each one
/// with its source (gap H1: the literals are ported from the spec, which
/// quotes the original lines).
pub fn documented_constants() -> [DocumentedConstant; 14] {
    [
        DocumentedConstant {
            name: "worldMapTileDim",
            value: WORLD_MAP_TILE_DIM as f64,
            source: "spec §3.2 'Grid' · MapsFile.cs:33",
        },
        DocumentedConstant {
            name: "natureClearanceTiles",
            value: NATURE_CLEARANCE_TILES as f64,
            source: "spec §3.2 'Location exclusion' · TerrainNature.cs:52,58-64",
        },
        DocumentedConstant {
            name: "maxSteepnessDeg",
            value: MAX_STEEPNESS_DEG as f64,
            source: "spec §3.2 'Steepness' · TerrainNature.cs:47,117-119",
        },
        DocumentedConstant {
            name: "probDirt",
            value: PROB_DIRT as f64,
            source: "spec §3.2 'Base probability' · TerrainNature.cs:49-51",
        },
        DocumentedConstant {
            name: "probGrass",
            value: PROB_GRASS as f64,
            source: "spec §3.2 'Base probability' · TerrainNature.cs:49-51",
        },
        DocumentedConstant {
            name: "probStone",
            value: PROB_STONE as f64,
            source: "spec §3.2 'Base probability' · TerrainNature.cs:49-51",
        },
        DocumentedConstant {
            name: "heightScaleMin",
            value: HEIGHT_SCALE_MIN as f64,
            source: "spec §3.2 'Scale by height' · TerrainNature.cs:70-72",
        },
        DocumentedConstant {
            name: "heightScaleMax",
            value: HEIGHT_SCALE_MAX as f64,
            source: "spec §3.2 'Scale by height' · TerrainNature.cs:70-72",
        },
        DocumentedConstant {
            name: "heightScaleDivisor",
            value: HEIGHT_SCALE_DIVISOR as f64,
            source: "spec §3.2 'Scale by height' · TerrainNature.cs:70-72",
        },
        DocumentedConstant {
            name: "desertScale",
            value: DESERT_SCALE as f64,
            source: "spec §3.2 'Scale by climate' · TerrainNature.cs:74-81",
        },
        DocumentedConstant {
            name: "recordMin",
            value: RECORD_MIN as f64,
            source: "spec §3.2 'Record' · TerrainNature.cs:163",
        },
        DocumentedConstant {
            name: "recordMax",
            value: RECORD_MAX as f64,
            source: "spec §3.2 'Record' · TerrainNature.cs:163",
        },
        DocumentedConstant {
            name: "sinkDivisor",
            value: SINK_DIVISOR as f64,
            source: "spec §3.2 'Sinking' · TerrainNature.cs:48,158",
        },
        DocumentedConstant {
            name: "maxBillboards",
            value: MAX_BILLBOARDS as f64,
            source: "spec §3.2 'Batch cap' · TerrainNature.cs:104",
        },
    ]
}

/// One placed billboard: a unit quad with a record, a scale and a sink offset.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NatureBillboard {
    /// Tile inside the map-pixel.
    pub tile: [u16; 2],
    /// Record inside the archive (`RECORD_MIN..=RECORD_MAX`).
    pub record: u8,
    /// Height scale (height term × climate term).
    pub scale: f32,
    /// Sink offset in meters (`height − steepness / 70`).
    pub sink_y: f32,
}

impl NatureBillboard {
    /// Canonical digest line: `x,y=record|scale_bits|sink_bits` with the two
    /// floats as `u32` bit patterns in hex (byte-stable across processes).
    pub fn canonical_line(&self) -> String {
        format!(
            "{},{}={}|{:08x}|{:08x}",
            self.tile[0],
            self.tile[1],
            self.record,
            self.scale.to_bits(),
            self.sink_y.to_bits()
        )
    }
}

/// Result of one map-pixel placement: the billboards plus every rejection
/// counter, so a rejection is always visible instead of silent.
#[derive(Debug, Clone, PartialEq)]
pub struct NaturePlacement {
    /// Map-pixel the placement belongs to.
    pub map: [i32; 2],
    /// Terrain key of the map-pixel (the stream seed).
    pub key: TerrainKey,
    /// Climate of the map-pixel.
    pub climate: WorldClimate,
    /// Season of the placement (drives the archive, never the positions).
    pub season: WorldSeason,
    /// Determinism seed.
    pub seed: u64,
    /// Accepted billboards, in sample order, capped at
    /// [`NatureRules::max_billboards`].
    pub billboards: Vec<NatureBillboard>,
    /// Rejections by rule 1 (band ocean/beach: the measured equivalent of
    /// `height < beachLine`, spec §3.2 row "Minimum height").
    pub rejected_below_beach: usize,
    /// Rejections by rule 2 (inside a location rect expanded by
    /// [`NATURE_CLEARANCE_TILES`]).
    pub rejected_clearance: usize,
    /// Rejections by rule 3 (`steepness > MAX_STEEPNESS_DEG`).
    pub rejected_steepness: usize,
    /// Rejections by rule 4, only for samples whose tile class is known.
    pub rejected_probability: usize,
    /// Samples that reached rule 4 with [`TileClass::Unknown`]: no
    /// probability gate was applied to them (`GAP-TAX-11`).
    pub samples_without_class: usize,
    /// `true` when at least one sample reached rule 4 without a tile class,
    /// i.e. the probability gate is declared **not measurable** for this
    /// map-pixel (spec §9 R3 + `GAP-TAX-11`). Never a silent zero.
    pub probability_gate_unavailable: bool,
    /// `true` when the cap cut the placement (`len == max_billboards`).
    pub capped: bool,
}

impl NaturePlacement {
    /// Nature archive of the placement's own climate and season.
    pub fn archive(&self) -> u32 {
        Self::archive_for(self.climate, self.season)
    }

    /// Nature archive of a `(climate, season)` pair (spec §3.1: the seasonal
    /// swap changes the archive, never the positions).
    pub fn archive_for(climate: WorldClimate, season: WorldSeason) -> u32 {
        nature_archive(
            climate.settings().nature_set,
            Season::from_world_season(season),
        )
    }

    /// Accepted billboards.
    pub fn accepted(&self) -> usize {
        self.billboards.len()
    }

    /// Total rejections over the four rules.
    pub fn rejected_total(&self) -> usize {
        self.rejected_below_beach
            + self.rejected_clearance
            + self.rejected_steepness
            + self.rejected_probability
    }

    /// Determinism digest of the whole placement: archive, season, climate,
    /// key and the canonical billboard lines (FNV-1a 64).
    ///
    /// It **changes** with the season because the archive is part of it; use
    /// [`Self::position_digest`] to compare positions only.
    pub fn digest(&self) -> u64 {
        let mut lines = Vec::with_capacity(self.billboards.len() + 4);
        lines.push(format!("archive={}", self.archive()));
        lines.push(format!("climate={}", self.climate.name()));
        lines.push(format!("season={}", season_name(self.season)));
        lines.push(format!("key={}", self.key.value()));
        lines.extend(self.billboards.iter().map(NatureBillboard::canonical_line));
        fnv1a64(lines.join("\n").as_bytes(), FNV_OFFSET)
    }

    /// Digest of the positions only (tile, record, scale, sink): identical
    /// across seasons for the same map-pixel, seed and samples (spec §3.3
    /// rule 5, "0 popping").
    pub fn position_digest(&self) -> u64 {
        placement_digest(self)
    }
}

/// Digest of the placement positions: FNV-1a 64 over the canonical billboard
/// lines `x,y=record|scale_bits|sink_bits`, one per billboard, `\n`-joined.
pub fn placement_digest(placement: &NaturePlacement) -> u64 {
    let lines: Vec<String> = placement
        .billboards
        .iter()
        .map(NatureBillboard::canonical_line)
        .collect();
    fnv1a64(lines.join("\n").as_bytes(), FNV_OFFSET)
}

/// Stable season name for digests and reports.
fn season_name(season: WorldSeason) -> &'static str {
    match season {
        WorldSeason::Spring => "Spring",
        WorldSeason::Summer => "Summer",
        WorldSeason::Autumn => "Autumn",
        WorldSeason::Winter => "Winter",
    }
}

/// Deterministic stream of one map-pixel: SplitMix64 seeded from `seed` and
/// the terrain key (spec §3.2 row "Seed").
struct TerrainStream {
    state: u64,
}

impl TerrainStream {
    /// Stream of a map-pixel, seeded by `seed` and the terrain key.
    fn new(seed: u64, key: u32) -> Self {
        let state = fnv1a64(&key.to_le_bytes(), fnv1a64(&seed.to_le_bytes(), FNV_OFFSET));
        Self { state }
    }

    /// Next 64-bit draw (SplitMix64).
    fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(SPLITMIX_INCREMENT);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    /// Next draw in `[0, 1)`.
    fn next_unit(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }

    /// Next record in `min..=max` (uniform modulo span).
    fn next_record(&mut self, min: u8, max: u8) -> u8 {
        let span = u64::from(max) - u64::from(min) + 1;
        min + (self.next_u64() % span) as u8
    }
}

/// Places the nature billboards of one map-pixel (spec §3.2, port literal).
///
/// The rules are applied **in this order**, each one counting its rejection:
///
/// 1. band ocean/beach → [`NaturePlacement::rejected_below_beach`] (the
///    measured equivalent of `height < beachLine`: `TerrainBand` is this
///    engine's authority, spec §3.2 row "Minimum height");
/// 2. inside a location rect expanded by `NATURE_CLEARANCE_TILES` →
///    [`NaturePlacement::rejected_clearance`];
/// 3. `steepness_deg > MAX_STEEPNESS_DEG` →
///    [`NaturePlacement::rejected_steepness`];
/// 4. probability gate of the tile class → [`NaturePlacement::rejected_probability`];
///    a [`TileClass::Unknown`] sample skips the gate, is counted in
///    [`NaturePlacement::samples_without_class`] and sets
///    [`NaturePlacement::probability_gate_unavailable`] (`GAP-TAX-11`).
///
/// Accepted samples emit a [`NatureBillboard`] with `scale =
/// clamp(height / 128, 0.4, 1.0) × (desert ? 0.25 : 1.0)` and `sink_y =
/// height − steepness / 70`, until the cap is reached.
pub fn place_nature(
    rules: &NatureRules,
    map: [i32; 2],
    season: WorldSeason,
    samples: &[NatureSample],
    locations: &[NatureRect],
) -> NaturePlacement {
    let key = TerrainKey::of(map[0], map[1]);
    let mut stream = TerrainStream::new(rules.seed, key.value());
    let cleared: Vec<NatureRect> = locations
        .iter()
        .map(|rect| rect.expanded(rules.clearance_tiles))
        .collect();
    let mut placement = NaturePlacement {
        map,
        key,
        climate: rules.climate,
        season,
        seed: rules.seed,
        billboards: Vec::new(),
        rejected_below_beach: 0,
        rejected_clearance: 0,
        rejected_steepness: 0,
        rejected_probability: 0,
        samples_without_class: 0,
        probability_gate_unavailable: false,
        capped: false,
    };
    for sample in samples {
        // Rule 1 — below the beach line (measured as the terrain band).
        if !matches!(sample.band, TerrainBand::Land) {
            placement.rejected_below_beach += 1;
            continue;
        }
        // Rule 2 — location clearance.
        if cleared.iter().any(|rect| rect.contains(sample.tile)) {
            placement.rejected_clearance += 1;
            continue;
        }
        // Rule 3 — steepness.
        if sample.steepness_deg > rules.max_steepness_deg {
            placement.rejected_steepness += 1;
            continue;
        }
        // Rule 4 — probability gate, only when the tile class is known.
        match rules.accept_probability(sample.tile_class) {
            Some(probability) => {
                if stream.next_unit() as f32 >= probability {
                    placement.rejected_probability += 1;
                    continue;
                }
            }
            None => {
                placement.samples_without_class += 1;
                placement.probability_gate_unavailable = true;
            }
        }
        // Batch cap: cutting is reported, never silent.
        if placement.billboards.len() >= rules.max_billboards {
            placement.capped = true;
            break;
        }
        placement.billboards.push(NatureBillboard {
            tile: sample.tile,
            record: stream.next_record(rules.record_min, rules.record_max),
            scale: rules.height_scale(sample.height),
            sink_y: rules.sink(sample.height, sample.steepness_deg),
        });
    }
    placement
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Rules with the probability gate forced open, to isolate one boundary
    /// per test (the documented values are asserted by
    /// `documented_constants_match_spec_3_2`).
    fn open_rules(seed: u64, climate: WorldClimate) -> NatureRules {
        let mut rules = NatureRules::new(seed, climate);
        rules.prob_dirt = 1.0;
        rules.prob_grass = 1.0;
        rules.prob_stone = 1.0;
        rules
    }

    fn sample(tile: [u16; 2], steepness_deg: f32, band: TerrainBand) -> NatureSample {
        NatureSample {
            tile,
            height: 200.0,
            steepness_deg,
            band,
            tile_class: TileClass::Grass,
        }
    }

    /// Deterministic samples: tile `(i % 16, i / 16)`, land, flat, grade 10 m.
    fn land_samples(count: usize, tile_class: TileClass) -> Vec<NatureSample> {
        (0..count)
            .map(|i| NatureSample {
                tile: [(i % 16) as u16, (i / 16) as u16],
                height: 10.0 + (i % 7) as f32,
                steepness_deg: (i % 50) as f32,
                band: TerrainBand::Land,
                tile_class,
            })
            .collect()
    }

    #[test]
    fn documented_constants_match_spec_3_2() {
        let constants = documented_constants();
        assert_eq!(constants.len(), 14, "every §3.2 constant is documented");
        let value = |name: &str| {
            constants
                .iter()
                .find(|constant| constant.name == name)
                .map(|constant| constant.value)
                .unwrap_or(f64::NAN)
        };
        assert!((value("worldMapTileDim") - 128.0).abs() < f64::EPSILON);
        assert!((value("natureClearanceTiles") - 4.0).abs() < f64::EPSILON);
        assert!((value("maxSteepnessDeg") - 50.0).abs() < f64::EPSILON);
        assert!((value("probDirt") - 0.2).abs() < 1e-6);
        assert!((value("probGrass") - 0.9).abs() < 1e-6);
        assert!((value("probStone") - 0.05).abs() < 1e-6);
        assert!((value("desertScale") - 0.25).abs() < 1e-6);
        assert!((value("sinkDivisor") - 70.0).abs() < f64::EPSILON);
        assert!((value("maxBillboards") - 16_129.0).abs() < f64::EPSILON);
        assert!(constants.iter().all(|constant| !constant.source.is_empty()));
    }

    #[test]
    fn terrain_key_matches_terrain_helper_formula() {
        assert_eq!(make_terrain_key(1, 2), (2 << 16) | 1);
        assert_eq!(make_terrain_key(0, 0), 0);
        // Negative coordinates keep their two's-complement 16-bit pattern.
        assert_eq!(make_terrain_key(-1, 0), 0x0000_FFFF);
        assert_eq!(make_terrain_key(0, -1), 0xFFFF_0000);
        assert_eq!(TerrainKey::of(3, 4).value(), make_terrain_key(3, 4));
    }

    #[test]
    fn same_inputs_produce_the_same_digest() {
        let rules = NatureRules::new(0xDEAD_BEEF, WorldClimate::Woodlands);
        let samples = land_samples(4_000, TileClass::Grass);
        let rects = [NatureRect {
            min: [2, 2],
            max: [6, 6],
        }];
        let first = place_nature(&rules, [12, 34], WorldSeason::Summer, &samples, &rects);
        let second = place_nature(&rules, [12, 34], WorldSeason::Summer, &samples, &rects);
        assert_eq!(first, second, "identical inputs, identical placement");
        assert_eq!(first.digest(), second.digest());
        assert_eq!(first.position_digest(), second.position_digest());
        assert!(!first.billboards.is_empty());
        assert!(first.rejected_clearance > 0);
    }

    #[test]
    fn a_rect_covering_the_patch_rejects_every_sample_by_clearance() {
        let rules = open_rules(7, WorldClimate::Woodlands);
        let samples = land_samples(64, TileClass::Grass);
        let rects = [NatureRect {
            min: [-64, -64],
            max: [64, 64],
        }];
        let placement = place_nature(&rules, [0, 0], WorldSeason::Summer, &samples, &rects);
        assert!(
            placement.billboards.is_empty(),
            "0 billboards inside rect+4"
        );
        assert_eq!(placement.rejected_clearance, samples.len());
        assert_eq!(placement.rejected_below_beach, 0);
        assert_eq!(placement.rejected_steepness, 0);
        assert_eq!(placement.rejected_probability, 0);
    }

    #[test]
    fn steepness_boundary_is_fifty_degrees() {
        let rules = open_rules(11, WorldClimate::Woodlands);
        let steep = [sample([1, 1], 51.0, TerrainBand::Land)];
        let rejected = place_nature(&rules, [0, 0], WorldSeason::Summer, &steep, &[]);
        assert_eq!(rejected.rejected_steepness, 1);
        assert_eq!(rejected.rejected_probability, 0);
        assert!(rejected.billboards.is_empty());
        let accepted_steepness = [sample([1, 1], 49.0, TerrainBand::Land)];
        let accepted = place_nature(
            &rules,
            [0, 0],
            WorldSeason::Summer,
            &accepted_steepness,
            &[],
        );
        assert_eq!(accepted.rejected_steepness, 0);
        assert_eq!(accepted.billboards.len(), 1, "49° is accepted");
        // The threshold constant itself is the documented 50°.
        assert!((rules.max_steepness_deg - 50.0).abs() < f32::EPSILON);
    }

    #[test]
    fn ocean_and_beach_bands_are_rejected_below_the_beach_line() {
        let rules = open_rules(13, WorldClimate::Woodlands);
        let samples = [
            sample([1, 1], 10.0, TerrainBand::Ocean),
            sample([2, 2], 10.0, TerrainBand::Beach),
            sample([3, 3], 10.0, TerrainBand::Land),
        ];
        let placement = place_nature(&rules, [0, 0], WorldSeason::Summer, &samples, &[]);
        assert_eq!(placement.rejected_below_beach, 2);
        assert_eq!(placement.billboards.len(), 1);
        assert_eq!(placement.billboards[0].tile, [3, 3]);
    }

    #[test]
    fn records_stay_inside_one_to_thirty_one() {
        let rules = open_rules(0x5EED, WorldClimate::Woodlands);
        let samples = land_samples(12_000, TileClass::Grass);
        let placement = place_nature(&rules, [5, 9], WorldSeason::Summer, &samples, &[]);
        assert!(placement.billboards.len() >= 10_000);
        for billboard in &placement.billboards {
            assert!(
                (RECORD_MIN..=RECORD_MAX).contains(&billboard.record),
                "record {} outside 1..=31",
                billboard.record
            );
        }
        // Scale and sink follow the §3.2 literals.
        let billboard = placement.billboards[0];
        let sample = samples[0];
        assert!((billboard.scale - rules.height_scale(sample.height)).abs() < 1e-6);
        assert!((billboard.sink_y - rules.sink(sample.height, sample.steepness_deg)).abs() < 1e-6);
    }

    #[test]
    fn cap_cuts_at_max_billboards_and_reports_it() {
        let rules = open_rules(0xC0FFEE, WorldClimate::Woodlands);
        let samples = land_samples(20_000, TileClass::Grass);
        let placement = place_nature(&rules, [1, 1], WorldSeason::Summer, &samples, &[]);
        assert_eq!(placement.billboards.len(), MAX_BILLBOARDS);
        assert_eq!(placement.billboards.len(), 16_129);
        assert!(placement.capped, "cutting the batch is reported");
        // Same input below the cap is not reported as capped.
        let small = land_samples(MAX_BILLBOARDS, TileClass::Grass);
        let uncapped = place_nature(&rules, [1, 1], WorldSeason::Summer, &small, &[]);
        assert_eq!(uncapped.billboards.len(), MAX_BILLBOARDS);
        assert!(!uncapped.capped);
    }

    #[test]
    fn unknown_tile_class_never_uses_an_invented_probability() {
        let rules = NatureRules::new(21, WorldClimate::Woodlands);
        let samples = land_samples(1_000, TileClass::Unknown);
        let first = place_nature(&rules, [7, 8], WorldSeason::Summer, &samples, &[]);
        let second = place_nature(&rules, [7, 8], WorldSeason::Summer, &samples, &[]);
        assert_eq!(first.rejected_probability, 0, "no gate applied");
        assert!(first.probability_gate_unavailable);
        assert_eq!(first.samples_without_class, samples.len());
        assert_eq!(first.billboards.len(), samples.len(), "all pass rules 1-3");
        assert_eq!(
            first.digest(),
            second.digest(),
            "digest stays deterministic"
        );
        assert_eq!(first.position_digest(), second.position_digest());
        // Unknown samples are still subject to the other three rules.
        let mut mixed = land_samples(10, TileClass::Unknown);
        mixed[0].steepness_deg = 51.0;
        mixed[1].band = TerrainBand::Beach;
        let mixed_placement = place_nature(&rules, [7, 8], WorldSeason::Summer, &mixed, &[]);
        assert_eq!(mixed_placement.rejected_steepness, 1);
        assert_eq!(mixed_placement.rejected_below_beach, 1);
        assert_eq!(mixed_placement.samples_without_class, 8);
    }

    #[test]
    fn grass_class_uses_the_probability_gate() {
        let rules = NatureRules::new(21, WorldClimate::Woodlands);
        let samples = land_samples(10_000, TileClass::Grass);
        let placement = place_nature(&rules, [7, 8], WorldSeason::Summer, &samples, &[]);
        assert!(!placement.probability_gate_unavailable);
        assert_eq!(placement.samples_without_class, 0);
        assert!(
            placement.rejected_probability > 0,
            "0.9 must reject some of the 10 000 samples"
        );
        assert!(placement.rejected_probability < samples.len());
        assert_eq!(
            placement.billboards.len() + placement.rejected_probability,
            samples.len()
        );
    }

    #[test]
    fn seasonal_swap_changes_the_archive_not_the_positions() {
        let rules = NatureRules::new(0xABCD, WorldClimate::Woodlands);
        let samples = land_samples(2_000, TileClass::Grass);
        let summer = place_nature(&rules, [3, 4], WorldSeason::Summer, &samples, &[]);
        let winter = place_nature(&rules, [3, 4], WorldSeason::Winter, &samples, &[]);
        assert_eq!(summer.archive(), 504);
        assert_eq!(winter.archive(), 505);
        assert_eq!(
            summer.position_digest(),
            winter.position_digest(),
            "same map-pixel and seed: identical positions"
        );
        assert_ne!(
            summer.digest(),
            winter.digest(),
            "the archive is part of it"
        );
        assert_eq!(summer.billboards, winter.billboards);
    }

    #[test]
    fn archive_for_matches_the_climate_table() {
        assert_eq!(
            NaturePlacement::archive_for(WorldClimate::Woodlands, WorldSeason::Winter),
            505
        );
        assert_eq!(
            NaturePlacement::archive_for(WorldClimate::Desert, WorldSeason::Winter),
            503
        );
        assert_eq!(
            NaturePlacement::archive_for(WorldClimate::Rainforest, WorldSeason::Winter),
            500
        );
        assert_eq!(
            NaturePlacement::archive_for(WorldClimate::Mountain, WorldSeason::Winter),
            511
        );
        assert_eq!(
            NaturePlacement::archive_for(WorldClimate::Woodlands, WorldSeason::Summer),
            504
        );
    }

    #[test]
    fn desert_climate_scales_billboards_down() {
        let temperate = NatureRules::new(1, WorldClimate::Woodlands);
        let desert = NatureRules::new(1, WorldClimate::Desert);
        assert!(
            (temperate.height_scale(200.0) - 1.0).abs() < 1e-6,
            "clamped top"
        );
        assert!(
            (temperate.height_scale(0.0) - 0.4).abs() < 1e-6,
            "clamped bottom"
        );
        assert!((temperate.height_scale(64.0) - 0.5).abs() < 1e-6);
        assert!((desert.height_scale(64.0) - 0.125).abs() < 1e-6);
        assert!(desert.is_desert());
        assert!(NatureRules::new(1, WorldClimate::Desert2).is_desert());
    }

    #[test]
    fn rect_expansion_admits_the_four_tile_clearance() {
        let rect = NatureRect {
            min: [10, 10],
            max: [20, 20],
        };
        let expanded = rect.expanded(NATURE_CLEARANCE_TILES);
        assert_eq!(expanded.min, [6, 6]);
        assert_eq!(expanded.max, [24, 24]);
        assert!(expanded.contains([6, 6]));
        assert!(!expanded.contains([5, 6]));
        assert!(expanded.contains([24, 24]));
        assert!(!expanded.contains([25, 24]));
        // Saturating growth never panics on extreme rectangles.
        let extreme = NatureRect {
            min: [i32::MIN, i32::MIN],
            max: [i32::MAX, i32::MAX],
        };
        assert_eq!(extreme.expanded(4), extreme);
    }

    #[test]
    fn tile_class_reads_the_low_six_bits_mapping() {
        assert_eq!(TileClass::from_tilemap_sample(1), TileClass::Dirt);
        assert_eq!(TileClass::from_tilemap_sample(2), TileClass::Grass);
        assert_eq!(TileClass::from_tilemap_sample(3), TileClass::Stone);
        assert_eq!(TileClass::from_tilemap_sample(0), TileClass::Unknown);
        assert_eq!(TileClass::from_tilemap_sample(4), TileClass::Unknown);
        assert_eq!(TileClass::Unknown.probability(), None);
        assert_eq!(TileClass::Dirt.probability(), Some(PROB_DIRT));
        assert_eq!(TileClass::Grass.name(), "grass");
    }

    #[test]
    fn sample_from_vertex_takes_the_bridge_measurement() {
        let vertex = TerrainVertex {
            height_m: 123.5,
            band: TerrainBand::Land,
            normalized: 0.08,
        };
        let sample = NatureSample::from_vertex([2, 3], &vertex, 12.5, TileClass::Stone);
        assert_eq!(sample.tile, [2, 3]);
        assert!((sample.height - 123.5).abs() < f32::EPSILON);
        assert!((sample.steepness_deg - 12.5).abs() < f32::EPSILON);
        assert_eq!(sample.band, TerrainBand::Land);
        assert_eq!(sample.tile_class, TileClass::Stone);
    }
}
