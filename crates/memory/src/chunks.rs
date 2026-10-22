//! `chunks` — registry of chunks loaded in the session (M7).
//!
//! **Declared minimum contract** (C3 pack M7): the source of the PD4 T7
//! Automap read model ("Automap → projection from **Chunks**", anchor
//! REQ-L99-039 — memory→perception). `ChunkMap` IS that set of session
//! Chunks, with integer coordinates `(x, z)` (world column/row, same
//! convention as the collision grid).
//!
//! 🧠 Memory organ rules (bevy-bridge-matrix): session
//! state/assets/history · **0 Bevy** · **0 persistence** (the save/load
//! roundtrip is M8, REQ-L99-041) · deterministic order (iteration over
//! `BTreeSet`: stable top-left, 0 randomness — the automap is reproducible).

use std::collections::BTreeSet;

/// Coordinate of a chunk on the world X/Z plane (integers, NFR
/// NUMERIC-never-FLOAT).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ChunkCoord {
    /// Chunk X column.
    pub x: i32,
    /// Chunk Z row.
    pub z: i32,
}

impl ChunkCoord {
    /// `(x, z)` coordinate.
    pub const fn new(x: i32, z: i32) -> Self {
        Self { x, z }
    }
}

/// Chunks loaded in the session (source of the Automap read model).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ChunkMap {
    loaded: BTreeSet<ChunkCoord>,
}

impl ChunkMap {
    /// Empty map (0 chunks).
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers a loaded chunk. Returns `true` if it is fresh (`false` =
    /// it was already there — idempotent, no duplicates).
    pub fn load(&mut self, coord: ChunkCoord) -> bool {
        self.loaded.insert(coord)
    }

    /// Is the chunk loaded?
    pub fn is_loaded(&self, coord: ChunkCoord) -> bool {
        self.loaded.contains(&coord)
    }

    /// Loaded chunks in deterministic order (x, then z).
    pub fn iter(&self) -> impl Iterator<Item = ChunkCoord> + '_ {
        self.loaded.iter().copied()
    }

    /// Number of loaded chunks.
    pub fn len(&self) -> usize {
        self.loaded.len()
    }

    /// Are there no loaded chunks?
    pub fn is_empty(&self) -> bool {
        self.loaded.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_is_idempotent_and_ordered() {
        let mut map = ChunkMap::new();
        assert!(map.is_empty());
        assert!(map.load(ChunkCoord::new(1, 0)));
        assert!(map.load(ChunkCoord::new(0, 0)));
        assert!(!map.load(ChunkCoord::new(1, 0)));
        assert!(map.load(ChunkCoord::new(0, 1)));
        assert_eq!(map.len(), 3);
        let order: Vec<ChunkCoord> = map.iter().collect();
        assert_eq!(
            order,
            vec![
                ChunkCoord::new(0, 0),
                ChunkCoord::new(0, 1),
                ChunkCoord::new(1, 0),
            ]
        );
        assert!(map.is_loaded(ChunkCoord::new(1, 0)));
        assert!(!map.is_loaded(ChunkCoord::new(9, 9)));
    }
}
