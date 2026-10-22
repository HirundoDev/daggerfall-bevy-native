//! `DFRandom` — Daggerfall's shared LCG (M15, REQ-L99-047).
//!
//! 1:1 port of the pinned executable spec `reference/dfu-spec/Assets/Scripts/
//! API/DFRandom.cs` (MIT v1.1.1, upstream `0038a1e6…`):
//!
//! ```text
//! static ulong next = 1;            // DFRandom.cs:19-20
//! srand(int seed) => next = (uint)seed;      // :34-38
//! srand(uint seed) => next = seed;           // :43-47
//! rand() => next = next * 1103515245 + 12345; // :52-56 (wrap 2^64)
//!           return (uint)((next >> 16) & 0x7FFF);   // 0…32767
//! SaveSeed/RestoreSeed => savedNext/next       // :90-98
//! ```
//!
//! **M15 extraction (criterion b of the pack §2.3):** the implementation used
//! to live privately in `crates/app/src/m14_probe.rs:333-343`; M14 and M15 now
//! consume this single copy (0 semantic change, inherited test
//! `dfrandom_matches_dfu_lcg` green + `--probe AUDIO-001` EXIT=0).
//!
//! State is an explicit `u64` (not global) so that each chain
//! (name, population, song) has its own reproducible stream: DFU uses
//! a static because Unity has a single game thread; here
//! determinism must be local and testable without an execution order.

/// `DFU:API/DFRandom.cs` LCG with explicit state.
///
/// The `next` field is public to preserve the construction literal that
/// M14 used (`DfRandom { next: 1 }`, equivalent to `DFRandom.next = 1`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DfRandom {
    /// DFU's `ulong next` state (`DFRandom.cs:19`).
    pub next: u64,
}

impl Default for DfRandom {
    fn default() -> Self {
        Self::new()
    }
}

impl DfRandom {
    /// Initial DFU state (`static ulong next = 1`, `DFRandom.cs:19`).
    pub const fn new() -> Self {
        Self { next: 1 }
    }

    /// `DFRandom.srand(int)` — `next = (uint)seed` (`DFRandom.cs:34-38`).
    ///
    /// A negative seed is truncated to its low 32 bits (same `(uint)` cast).
    pub fn srand(&mut self, seed: u32) {
        self.next = u64::from(seed);
    }

    /// `DFRandom.srand(int)` with sign: `next = (uint)(int)`.
    pub fn srand_i32(&mut self, seed: i32) {
        self.srand(seed as u32);
    }

    /// `DFRandom.rand()` — advances and returns `(next >> 16) & 0x7FFF`
    /// (`DFRandom.cs:52-56`). Range `0…32767`, no bias correction.
    pub fn rand(&mut self) -> u32 {
        self.next = self
            .next
            .wrapping_mul(1_103_515_245)
            .wrapping_add(12_345);
        ((self.next >> 16) & 0x7FFF) as u32
    }

    /// `DFRandom.Seed` getter — `(uint)next` (`DFRandom.cs:25-28`).
    pub fn seed(&self) -> u32 {
        self.next as u32
    }

    /// `DFRandom.SaveSeed()` — pushes the state (`DFRandom.cs:90-94`).
    pub fn save_seed(&self) -> u64 {
        self.next
    }

    /// `DFRandom.RestoreSeed()` — restores the pushed state
    /// (`DFRandom.cs:96-98`).
    pub fn restore_seed(&mut self, saved: u64) {
        self.next = saved;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Parity vectors ported from DFU (`srand(0)→0`, `srand(1)→16838`):
    /// the SAME pair as the inherited M14 test, now over the shared
    /// module.
    #[test]
    fn dfrandom_matches_dfu_lcg() {
        let mut rng = DfRandom::new();
        rng.srand(0);
        assert_eq!(rng.rand(), 0);
        rng.srand(1);
        assert_eq!(rng.rand(), 16838);
        // Sequence of 4 draws with seed 1000 (M15 golden GV#1).
        rng.srand(1000);
        assert_eq!(rng.rand(), 28322);
        assert_eq!(rng.rand(), 7943);
        assert_eq!(rng.rand(), 4653);
        assert_eq!(rng.rand(), 8133);
    }

    /// Negative `srand(int)` truncates to 32 bits (`(uint)seed`), and
    /// `SaveSeed`/`RestoreSeed` reproduce the exact stream.
    #[test]
    fn srand_i32_truncates_and_seed_roundtrip() {
        let mut a = DfRandom::new();
        a.srand_i32(-1);
        assert_eq!(a.seed(), u32::MAX);
        let saved = a.save_seed();
        let first = a.rand();
        a.restore_seed(saved);
        assert_eq!(a.rand(), first);
    }

    /// State never leaves 64 bits: 100,000 draws without panic.
    #[test]
    fn rand_is_total_over_long_stream() {
        let mut rng = DfRandom::new();
        rng.srand(20261020);
        for _ in 0..100_000 {
            assert!(rng.rand() <= 0x7FFF);
        }
    }
}
