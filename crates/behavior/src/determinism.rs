//! Deterministic helpers shared by the M26 modules: FNV-1a 64
//! for digests, SplitMix64 for seeded draws and the pinned Daggerfall LCG
//! ([`DfRandom`]) where the cited source rolls with `DFRandom`.
//!
//! The organ `behavior` stays dependency-free (0 Bevy, 0 I/O, 0 RNG global);
//! these primitives are the minimum needed so that "same seed ⇒ same
//! result" is testable. SplitMix64 is bit-identical to
//! `daggerfall-skills::rng::Rng64` (M20) — kept local so that the
//! behavior organ does not take a dependency on the skills organ; the shared
//! contract is the algorithm, not the crate.

/// FNV-1a 64 (offset basis `0xcbf2_9ce4_8422_2325`, prime `0x100000001b3`).
pub(crate) fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// SplitMix64 (Steele, Lea & Flood): `u64` state, advance by the golden
/// constant and final mix of three shifts/XORs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SplitMix64 {
    state: u64,
}

impl SplitMix64 {
    /// Seeded generator.
    pub(crate) fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    /// Next `u64` in the sequence.
    pub(crate) fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Uniform integer in `[0, span)`; `span == 0` returns 0.
    pub(crate) fn below(&mut self, span: u64) -> u64 {
        if span == 0 {
            0
        } else {
            self.next_u64() % span
        }
    }

    /// Uniform fraction in `[0, 1)` (`UnityEngine.Random.Range(0f, 1f)`).
    pub(crate) fn unit(&mut self) -> f32 {
        // 24 bits of mantissa: enough for thresholds 0.025 / 0.20.
        ((self.next_u64() >> 40) as f32) / ((1u32 << 24) as f32)
    }
}

/// `DFRandom` — Daggerfall LCG (`DFU:API/DFRandom.cs`, 1:1 port):
/// `srand(uint) => next = seed` (`:43-47`) · `rand() => next = next·1103515245
/// + 12345 (wrap 2^64); (uint)((next >> 16) & 0x7FFF)` (`:52-56`) ·
/// `random_range_inclusive(min,max) => (int)rand() % (max-min+1) + min` (`:73-80`).
///
/// Same sequence as `daggerfall_parser_rs::dfrandom::DfRandom` (M15); kept
/// local for the same reason as SplitMix64 — the `behavior` organ does not
/// depend on another organ and the contract is the algorithm.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct DfRandom {
    next: u64,
}

impl DfRandom {
    /// `DFRandom.srand(uint)` — `next = seed`.
    pub(crate) fn srand(seed: u32) -> Self {
        Self {
            next: u64::from(seed),
        }
    }

    /// `DFRandom.rand()` — `0…32767`, without bias correction.
    pub(crate) fn rand(&mut self) -> u32 {
        self.next = self
            .next
            .wrapping_mul(1_103_515_245)
            .wrapping_add(12_345);
        ((self.next >> 16) & 0x7FFF) as u32
    }

    /// `DFRandom.random_range_inclusive(min, max)`.
    pub(crate) fn random_range_inclusive(&mut self, min: i32, max: i32) -> i32 {
        (self.rand() % (max - min + 1) as u32) as i32 + min
    }
}
