//! Deterministic RNG for the 🎮 skills organ (M20).
//!
//! **Declared delta (spec M20 §7.1):** the DFU source uses `UnityEngine.Random`
//! and `DFRandom.random_range_inclusive` seeded with the frame
//! (`FormulaHelper.cs:151-152,348`), sequences Unity neither specifies nor
//! guarantees across versions. The target preserves **the same distributions
//! and inclusive ranges** with its own reproducible generator (SplitMix64) and
//! an explicit seed: the result is deterministic ×2, not the concrete sequence
//! of a Unity playthrough.
//!
//! SplitMix64 (Steele, Lea & Flood): `u64` state, advance by the golden
//! constant, final mix of three shifts/XOR. Modulo bias over small ranges
//! (`3..=6`, `10..=20`, `4..=6`) is negligible and declared: no rejection is
//! introduced (0 hidden branches, 0 extra state).

/// Deterministic SplitMix64 generator (see module docs).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rng64 {
    state: u64,
}

impl Rng64 {
    /// Creates the generator with the given seed.
    pub fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    /// Next `u64` in the sequence.
    pub fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Integer in the inclusive range `[low, high]` (`UnityEngine.Random.Range(min, max+1)`
    /// in the source: `DaggerfallSkills.cs:95`, `FormulaHelper.cs:152`).
    ///
    /// # Panics
    /// If `low > high` (invalid caller programming).
    pub fn range_inclusive(&mut self, low: i32, high: i32) -> i32 {
        assert!(low <= high, "invalid range {low}..={high}");
        let span = (i64::from(high) - i64::from(low) + 1) as u64;
        low + (self.next_u64() % span) as i32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sequence_is_deterministic_and_seed_dependent() {
        let mut a = Rng64::new(20_261_020);
        let mut b = Rng64::new(20_261_020);
        let mut c = Rng64::new(20_261_021);
        let first: Vec<u64> = (0..8).map(|_| a.next_u64()).collect();
        let second: Vec<u64> = (0..8).map(|_| b.next_u64()).collect();
        assert_eq!(first, second);
        assert_ne!(first[0], c.next_u64());
    }

    #[test]
    fn range_inclusive_covers_bounds_and_is_stable() {
        let mut rng = Rng64::new(7);
        let mut seen = [false; 4];
        for _ in 0..256 {
            let value = rng.range_inclusive(3, 6);
            assert!((3..=6).contains(&value));
            seen[(value - 3) as usize] = true;
        }
        assert_eq!(seen, [true; 4]);
        let mut a = Rng64::new(20_261_020);
        let mut b = Rng64::new(20_261_020);
        let va: Vec<i32> = (0..35).map(|_| a.range_inclusive(3, 6)).collect();
        let vb: Vec<i32> = (0..35).map(|_| b.range_inclusive(3, 6)).collect();
        assert_eq!(va, vb);
    }

    #[test]
    fn single_value_range_is_constant() {
        let mut rng = Rng64::new(1);
        assert_eq!(rng.range_inclusive(5, 5), 5);
    }
}
