//! `chunk_content` — pure, deterministic chunk content (PROBE-CHUNK-001,
//! INV-L99-2, M9).
//!
//! 🧠 Memory organ: chunk payload generation and content-addressed digests.
//! **0 I/O, 0 Bevy, 0 `f32`, 0 global state**: every cell is derived from
//! `(seed, coord.x, coord.z, cell_index)` through an integer mixer, so two
//! calls with the same arguments produce byte-identical content and the
//! result never depends on the call order.
//!
//! The set digest is **order invariant by construction** (keyed per-chunk
//! components combined with XOR + coordinate-dependent rotation) — never the
//! hash of an order-dependent concatenation. [`set_digest`] therefore detects
//! membership changes (a mutated chunk, a different seed) while staying
//! identical for `[a, b, c]` and `[c, a, b]`.

use std::collections::BTreeMap;

use crate::chunks::ChunkCoord;

/// Cells per chunk side (16 × 16 grid, DFU block sizing).
pub const CHUNK_CELLS: usize = 16;
/// Cells per chunk (height field and tile field length).
pub const CELL_COUNT: usize = CHUNK_CELLS * CHUNK_CELLS;

/// splitmix64 increment (`SM_GAMMA`).
const SM_GAMMA: u64 = 0x9e37_79b9_7f4a_7c15;
/// Offset FNV-1a 64.
const FNV_OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
/// Prime FNV-1a 64.
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;
/// Salt of the height draw.
const SALT_HEIGHT: u64 = 0x68_65_69_67_68_74;
/// Salt of the tile draw.
const SALT_TILES: u64 = 0x74_69_6c_65_73;
/// Height amplitude (`0..=23`).
const HEIGHT_MODULUS: u64 = 24;
/// Tile family modulus (`0..=3`).
const TILE_MODULUS: u64 = 4;

/// Content of one chunk: integer height field + tile family field.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChunkContent {
    /// Coordinate of this chunk.
    pub coord: ChunkCoord,
    /// `CELL_COUNT` heights (one per cell, top-left → bottom-right).
    pub heights: Vec<i32>,
    /// `CELL_COUNT` tile families (one per cell, same order as `heights`).
    pub tiles: Vec<u8>,
}

/// One splitmix64 finalizer round (integer, deterministic).
fn splitmix64(mut z: u64) -> u64 {
    z = z.wrapping_add(SM_GAMMA);
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^ (z >> 31)
}

/// Packs a chunk coordinate into a single 64-bit key (`x` high, `z` low).
fn coord_key(coord: ChunkCoord) -> u64 {
    (u64::from(coord.x as u32) << 32) ^ u64::from(coord.z as u32)
}

/// Per-cell draw: a pure function of `(seed, coord, cell, salt)`.
fn cell_value(seed: u64, coord: ChunkCoord, cell: u32, salt: u64) -> u64 {
    let coord_mixed = splitmix64(coord_key(coord) ^ seed);
    let cell_mixed = u64::from(cell).wrapping_mul(SM_GAMMA);
    splitmix64(coord_mixed ^ cell_mixed ^ salt)
}

/// Generates the content of `coord` for `seed` (pure, deterministic).
///
/// Every height/tile is derived from `(seed, coord.x, coord.z, cell_index)`;
/// no call order and no global state take part.
pub fn generate_chunk(seed: u64, coord: ChunkCoord) -> ChunkContent {
    let mut heights = Vec::with_capacity(CELL_COUNT);
    let mut tiles = Vec::with_capacity(CELL_COUNT);
    for cell in 0..CELL_COUNT as u32 {
        let h = cell_value(seed, coord, cell, SALT_HEIGHT);
        heights.push(((h >> 13) % HEIGHT_MODULUS) as i32);
        let t = cell_value(seed, coord, cell, SALT_TILES);
        tiles.push(((t >> 19) % TILE_MODULUS) as u8);
    }
    ChunkContent {
        coord,
        heights,
        tiles,
    }
}

/// Canonical byte layout of a chunk (stable across runs and platforms):
///
/// `coord.x i32 LE · coord.z i32 LE · len u32 LE · heights i32 LE… · tiles`.
pub fn canonical_bytes(chunk: &ChunkContent) -> Vec<u8> {
    let mut out = Vec::with_capacity(4 + 4 + 4 + chunk.heights.len() * 4 + chunk.tiles.len());
    out.extend_from_slice(&chunk.coord.x.to_le_bytes());
    out.extend_from_slice(&chunk.coord.z.to_le_bytes());
    out.extend_from_slice(&(chunk.heights.len() as u32).to_le_bytes());
    for height in &chunk.heights {
        out.extend_from_slice(&height.to_le_bytes());
    }
    out.extend_from_slice(&chunk.tiles);
    out
}

/// FNV-1a 64 over `bytes` (deterministic, 0 dependencies).
fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash = FNV_OFFSET_BASIS;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    hash
}

/// Digest of a chunk's canonical bytes.
fn content_digest(chunk: &ChunkContent) -> u64 {
    fnv1a64(&canonical_bytes(chunk))
}

/// Digest of `generate_chunk(seed, coord)` — a function of `(seed, coord)`.
pub fn chunk_digest(seed: u64, coord: ChunkCoord) -> u64 {
    content_digest(&generate_chunk(seed, coord))
}

/// Keyed, order-invariant contribution of one chunk to a set digest.
///
/// The rotation depends on the coordinate (not on the position in the slice),
/// so XOR combination never degenerates into an order-dependent sum.
fn keyed_component(seed: u64, coord: ChunkCoord, digest: u64) -> u64 {
    let mix = coord_key(coord) ^ seed;
    let rotation = (mix % 64) as u32;
    splitmix64(digest ^ splitmix64(mix)).rotate_left(rotation)
}

/// Combines `(coord, seed, digest)` triples order-independently (XOR of keyed
/// components, each keyed with the seed that produced its chunk).
fn combine(items: impl IntoIterator<Item = (ChunkCoord, u64, u64)>) -> u64 {
    items
        .into_iter()
        .fold(0u64, |acc, (coord, seed, digest)| {
            acc ^ keyed_component(seed, coord, digest)
        })
}

/// Order-invariant digest of a monoseed chunk set: sorts + deduplicates the
/// coordinates and combines the per-chunk digests as an XOR of keyed
/// components, each rotated by an amount derived from its own coordinate (so
/// the value never depends on the position in the slice).
///
/// `set_digest(seed, &[a, b, c]) == set_digest(seed, &[c, a, b])` for any
/// permutation of the same coordinates (duplicates are folded once). The
/// invariance is not a consequence of the sort: the probe and the tests
/// compare it against a naive FNV-1a digest over the per-chunk digests **in
/// load order**, which does change between two orders.
pub fn set_digest(seed: u64, coords: &[ChunkCoord]) -> u64 {
    let mut unique = coords.to_vec();
    unique.sort_unstable();
    unique.dedup();
    combine(
        unique
            .into_iter()
            .map(|coord| (coord, seed, chunk_digest(seed, coord))),
    )
}

/// One stored chunk: the seed that generated its content, plus the content.
#[derive(Debug, Clone, PartialEq, Eq)]
struct ChunkEntry {
    seed: u64,
    content: ChunkContent,
}

/// Chunks loaded in the session plus their content (keyed by coordinate, so
/// the load order never leaks into iteration or digests).
///
/// The seed is stored **per chunk**: a mixed-seed set (two loads with
/// different seeds) is digested over the seeds that actually produced each
/// chunk, so its digest matches neither monoseed set.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ChunkSet {
    chunks: BTreeMap<ChunkCoord, ChunkEntry>,
}

impl ChunkSet {
    /// Empty set.
    pub fn new() -> Self {
        Self::default()
    }

    /// Loads `coord` (generating its content if absent) in any order and
    /// returns a reference to it.
    ///
    /// First seed wins: a coordinate already loaded keeps its content AND its
    /// stored seed, so a re-load with a different seed changes nothing — the
    /// digest always matches the seed that produced the stored content.
    pub fn load(&mut self, seed: u64, coord: ChunkCoord) -> &ChunkContent {
        self.chunks.entry(coord).or_insert_with(|| ChunkEntry {
            seed,
            content: generate_chunk(seed, coord),
        });
        &self.chunks[&coord].content
    }

    /// Digest of the stored content of `coord` (`None` if not loaded).
    pub fn per_chunk_digest(&self, coord: ChunkCoord) -> Option<u64> {
        self.chunks
            .get(&coord)
            .map(|entry| content_digest(&entry.content))
    }

    /// Order-invariant digest of the loaded set, keyed with the seed stored
    /// per chunk.
    ///
    /// For a monoseed set it equals [`set_digest`] over the same coordinates
    /// and seed; for a mixed-seed set it equals neither monoseed digest.
    pub fn set_digest(&self) -> u64 {
        combine(self.chunks.iter().map(|(coord, entry)| {
            (*coord, entry.seed, content_digest(&entry.content))
        }))
    }

    /// Damages exactly one chunk: every height of `coord` is shifted by
    /// `delta` (saturating). Returns `false` if the chunk is not loaded.
    pub fn damage(&mut self, coord: ChunkCoord, delta: i32) -> bool {
        match self.chunks.get_mut(&coord) {
            Some(entry) => {
                for height in &mut entry.content.heights {
                    *height = height.saturating_add(delta);
                }
                true
            }
            None => false,
        }
    }

    /// Number of loaded chunks.
    pub fn len(&self) -> usize {
        self.chunks.len()
    }

    /// Are there no loaded chunks?
    pub fn is_empty(&self) -> bool {
        self.chunks.is_empty()
    }

    /// Loaded coordinates in deterministic order (`x`, then `z`).
    pub fn coords(&self) -> Vec<ChunkCoord> {
        self.chunks.keys().copied().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn c(x: i32, z: i32) -> ChunkCoord {
        ChunkCoord::new(x, z)
    }

    #[test]
    fn generation_is_deterministic_and_shaped() {
        let a = generate_chunk(7, c(2, 3));
        let b = generate_chunk(7, c(2, 3));
        assert_eq!(a, b);
        assert_eq!(a.heights.len(), CELL_COUNT);
        assert_eq!(a.tiles.len(), CELL_COUNT);
        assert!(a.heights.iter().all(|h| (0..24).contains(h)));
        assert!(a.tiles.iter().all(|t| *t < 4));
        // Different coordinates differ; different seeds differ.
        assert_ne!(a, generate_chunk(7, c(3, 3)));
        assert_ne!(a, generate_chunk(8, c(2, 3)));
    }

    #[test]
    fn canonical_layout_is_stable() {
        let chunk = generate_chunk(1, c(-2, 5));
        let bytes = canonical_bytes(&chunk);
        assert_eq!(bytes.len(), 12 + chunk.heights.len() * 4 + chunk.tiles.len());
        assert_eq!(&bytes[0..4], &(-2i32).to_le_bytes());
        assert_eq!(&bytes[4..8], &5i32.to_le_bytes());
        assert_eq!(&bytes[8..12], &(CELL_COUNT as u32).to_le_bytes());
        // Trailing tile bytes are verbatim.
        assert_eq!(&bytes[12 + CELL_COUNT * 4..], &chunk.tiles[..]);
        assert_eq!(chunk_digest(1, c(-2, 5)), fnv1a64(&bytes));
    }

    #[test]
    fn set_digest_is_order_invariant() {
        let coords = [c(0, 0), c(1, 0), c(0, 1), c(2, 3)];
        let forward = set_digest(9, &coords);
        let reversed: Vec<ChunkCoord> = coords.iter().rev().copied().collect();
        assert_eq!(forward, set_digest(9, &reversed));
        assert_eq!(forward, set_digest(9, &[c(2, 3), c(0, 1), c(1, 0), c(0, 0)]));
        // Duplicates fold once.
        assert_eq!(forward, set_digest(9, &[c(2, 3), c(0, 0), c(1, 0), c(0, 1), c(0, 0)]));
        // Membership and seed are detected.
        assert_ne!(forward, set_digest(9, &[c(0, 0), c(1, 0)]));
        assert_ne!(forward, set_digest(10, &coords));
    }

    #[test]
    fn chunk_set_matches_free_set_digest_any_load_order() {
        let coords = [c(0, 0), c(1, 1), c(2, 0)];
        let mut a = ChunkSet::new();
        for coord in coords {
            a.load(3, coord);
        }
        let mut b = ChunkSet::new();
        for coord in coords.iter().rev() {
            b.load(3, *coord);
        }
        assert_eq!(a.set_digest(), b.set_digest());
        assert_eq!(a.set_digest(), set_digest(3, &coords));
        assert_eq!(a.per_chunk_digest(c(1, 1)), b.per_chunk_digest(c(1, 1)));
    }

    #[test]
    fn mixed_seed_set_is_detected_by_the_stored_seeds() {
        let coords = [c(0, 0), c(1, 1)];
        let mut monoseed_11 = ChunkSet::new();
        let mut monoseed_12 = ChunkSet::new();
        for coord in coords {
            monoseed_11.load(11, coord);
            monoseed_12.load(12, coord);
        }
        // Monoseed sets agree with the free function over their own seed.
        assert_eq!(monoseed_11.set_digest(), set_digest(11, &coords));
        assert_eq!(monoseed_12.set_digest(), set_digest(12, &coords));
        assert_ne!(monoseed_11.set_digest(), monoseed_12.set_digest());

        // Mixed set: chunk 0 from seed 11, chunk 1 from seed 12.
        let mut mixed = ChunkSet::new();
        mixed.load(11, coords[0]);
        mixed.load(12, coords[1]);
        assert_ne!(mixed.set_digest(), monoseed_11.set_digest());
        assert_ne!(mixed.set_digest(), monoseed_12.set_digest());

        // First seed wins: re-loading with a different seed changes nothing.
        let before = monoseed_11.set_digest();
        monoseed_11.load(99, coords[0]);
        assert_eq!(
            monoseed_11.per_chunk_digest(coords[0]),
            Some(chunk_digest(11, coords[0]))
        );
        assert_eq!(monoseed_11.set_digest(), before);
    }

    #[test]
    fn damage_alters_one_chunk_only() {
        let coords = [c(0, 0), c(1, 1)];
        let mut set = ChunkSet::new();
        for coord in coords {
            set.load(4, coord);
        }
        let before_set = set.set_digest();
        let before_neighbour = set.per_chunk_digest(c(0, 0));
        let before_target = set.per_chunk_digest(c(1, 1));
        assert!(set.damage(c(1, 1), 5));
        assert_ne!(set.per_chunk_digest(c(1, 1)), before_target);
        assert_eq!(set.per_chunk_digest(c(0, 0)), before_neighbour);
        assert_ne!(set.set_digest(), before_set);
        assert!(!set.damage(c(9, 9), 1));
        assert_eq!(set.coords(), vec![c(0, 0), c(1, 1)]);
        assert_eq!(set.len(), 2);
        assert!(!set.is_empty());
    }
}
