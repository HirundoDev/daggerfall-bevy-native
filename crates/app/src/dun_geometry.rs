//! `dun_geometry` (app, `probes`) — the **single consumer** of the assembled
//! dungeon geometry (M44, `CR-bd972066-59e6-4768-9985-de75f69f6798`).
//!
//! **Why it exists:** the assembled grid (`interior_retail::RetailSource` →
//! `assemble`, i.e. `MAPDITEM` + the real `.RDB` cells of `BLOCKS.BSA`) had no
//! consumer: `PROBE-DUNGEON-001` measured it and nothing else read it. This
//! module turns that measurement into the two lines the gate reads and is the
//! **only** place the geometry is assembled for publication — the window demo
//! and the probe both go through here (0 duplicated logic).
//!
//! **Digest:** FNV-1a 64 over the canonical census key of every dungeon in
//! index order — `"{region}:{index}:{location_id}:"` followed by the grid's
//! `canonical_bytes()`. That is byte for byte the key of the existing census,
//! so the published digest **must** stay `5573c56afce6749a`: a drift means the
//! geometry changed, and the gate turns red instead of hiding it.
//!
//! **Overlaps are counted, never asserted:** the pair count is the real number
//! of kept action doors closer than 1.4 m in the assembled geometry (the grid
//! already dropped the duplicate action doors greedily; the golden of the
//! census is 0, and a non-zero value is **reported** with its arithmetic).

use daggerfall_parser_bevy_bridge::dungeon_grid::DungeonGrid;
use daggerfall_parser_rs::rdi;

use crate::dun_canal;
use crate::interior_retail::RetailSource;

/// Side of one dungeon cell in millimetres (`RDBSide = 2048 × 1/40` m).
pub const CELL_MM: u32 = 51_200;
/// Door overlap tolerance in millimetres (`DOOR_OVERLAP_TOLERANCE_M = 1.4`).
pub const TOLERANCE_MM: u32 = 1_400;

/// One assembly pass over the whole dungeon index.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GeometryCensus {
    /// Dungeon locations with a `MAPDITEM` entry.
    pub dungeons: usize,
    /// Blocks placed across every dungeon.
    pub blocks: usize,
    /// Pairs of kept doors closer than [`TOLERANCE_MM`] (the whole index).
    pub overlap_pairs: usize,
    /// FNV-1a 64 of the canonical census key (golden `5573c56afce6749a`).
    pub digest: u64,
}

/// Assembles every dungeon of the retail index once and returns the census.
///
/// Deterministic: `RetailSource::dungeons` is index-ordered and the digest is
/// updated in that order, with no clock, RNG or hash-map iteration.
pub fn census() -> Result<GeometryCensus, String> {
    let source = RetailSource::load()?;
    let (_, dungeons) = source.dungeons()?;
    let mut digest = rdi::fnv1a64(b"");
    let mut blocks = 0usize;
    let mut overlap_pairs = 0usize;
    for dungeon in &dungeons {
        let grid = source.assemble(&dungeon.layout);
        blocks += grid.blocks.len();
        overlap_pairs += overlaps(&grid, TOLERANCE_MM);
        let key = format!(
            "{}:{}:{}:",
            dungeon.region_index, dungeon.location_index, dungeon.location_id
        );
        digest = rdi::fnv1a64_update(digest, key.as_bytes());
        digest = rdi::fnv1a64_update(digest, &grid.canonical_bytes());
    }
    Ok(GeometryCensus {
        dungeons: dungeons.len(),
        blocks,
        overlap_pairs,
        digest,
    })
}

/// Number of pairs of kept doors closer than `tolerance_mm`, measured on the
/// assembled grid (3-D distance of the door centres, `f32` like DFU).
pub fn overlaps(grid: &DungeonGrid, tolerance_mm: u32) -> usize {
    let tolerance_m = tolerance_mm as f32 / 1000.0;
    let limit = tolerance_m * tolerance_m;
    let mut pairs = 0usize;
    for (i, door) in grid.doors.iter().enumerate() {
        for other in &grid.doors[i + 1..] {
            let dx = door.position_m[0] - other.position_m[0];
            let dy = door.position_m[1] - other.position_m[1];
            let dz = door.position_m[2] - other.position_m[2];
            if dx * dx + dy * dy + dz * dz < limit {
                pairs += 1;
            }
        }
    }
    pairs
}

/// Publishes the two `DUN VE: geo …` lines through the canal.
pub fn publish() {
    match census() {
        Ok(census) => {
            dun_canal::emit(&format!(
                "DUN VE: geo dungeons={} blocks={} cells_mm={CELL_MM} tolerance_mm={TOLERANCE_MM} \
                 digest={:016x}",
                census.dungeons, census.blocks, census.digest
            ));
            dun_canal::emit(&format!(
                "DUN VE: geo overlaps dungeons={} overlaps={} tolerance_mm={TOLERANCE_MM}",
                census.dungeons, census.overlap_pairs
            ));
        }
        // No `overlaps=` and no tolerance on the failure path: an unmeasured
        // geometry must never satisfy the gate by looking like a zero.
        Err(reason) => dun_canal::emit(&format!("DUN VE: geo unavailable — {reason}")),
    }
}
