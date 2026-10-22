//! `DFRandom` per actor (spec M22 §0/§6 · delta D-M22-1).
//!
//! In DFU `DFRandom` is a **global static** LCG (`DFRandom.cs:21-22`:
//! `next`/`savedNext` fields shared between actors, R-22.1). The probe
//! contract requires an identical `digest` with the same seed and a
//! **different** one with an alternate seed, with no shared state: delta
//! D-M22-1 gives each actor its own generator seeded with
//! `(session_seed, actor_id)`.

use daggerfall_behavior::combat::success_roll;

/// DFU LCG (`DFRandom.cs:52-56`): `next = next·1103515245 + 12345` and
/// `rand() = (next >> 16) & 0x7FFF` — **15 bits**, 0..32767.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DfRandom {
    /// LCG `next` state (u64, natural wrap-around — ORDER M22).
    next: u64,
}

impl DfRandom {
    /// Initial state seed (`srand` from `DFRandom.cs:47-50`).
    pub fn with_seed(seed: u64) -> Self {
        Self { next: seed }
    }

    /// `rand()` — ALWAYS advances the LCG and returns the high 15 bits of
    /// the state (`DFRandom.cs:52-56`).
    pub fn rand(&mut self) -> i32 {
        self.next = self
            .next
            .wrapping_mul(1_103_515_245)
            .wrapping_add(12_345);
        ((self.next >> 16) & 0x7FFF) as i32
    }

    /// `Range(low..=high)` modulo over [`Self::rand`] — deterministic and
    /// exclusively per actor (body part, damage range, enemy timer roll
    /// `Range(1500,3001)`).
    pub fn range_inclusive(&mut self, low: i32, high: i32) -> i32 {
        debug_assert!(high >= low, "inverted range ({low}, {high})");
        let span = high - low + 1;
        low + self.rand() % span
    }
}

/// `Dice100` (0..99) over the **actor's** `DFRandom` — the roll does not come
/// from a global RNG: `Dice100.SuccessRoll(c)` uses `roll < c`
/// (`Dice100.cs:15-18`) with the roll supplied by this generator (delta
/// D-M22-1; spec §6 forbids `thread_rng`/wall clock).
#[derive(Debug, Clone)]
pub struct Dice100 {
    rng: DfRandom,
}

impl Dice100 {
    /// Explicit actor seed (see [`actor_seed`]).
    pub fn with_seed(seed: u64) -> Self {
        Self {
            rng: DfRandom::with_seed(seed),
        }
    }

    /// `roll(0..99)` — `rand() % 100`.
    pub fn roll(&mut self) -> i32 {
        self.rng.rand() % 100
    }

    /// `Dice100.SuccessRoll(chance)` (`Dice100.cs:15-18`), via the pure
    /// [`success_roll`] from `behavior::combat`.
    pub fn success(&mut self, chance: i32) -> bool {
        success_roll(self.roll(), chance)
    }

    /// `Range(low..=high)` from the actor PRNG (body part / damage).
    pub fn roll_range(&mut self, low: i32, high: i32) -> i32 {
        self.rng.range_inclusive(low, high)
    }
}

/// Per-actor seed: `(session_seed, actor_id)` ⇒ a unique seed per pair
/// (spec §6). Final splitmix64 mix (local state, 0 global tables): the
/// same session+actor always reproduces the same chain; different actors
/// diverge without needing to reserve ranges.
pub fn actor_seed(session_seed: u64, actor_id: u32) -> u64 {
    let mixed = session_seed ^ (u64::from(actor_id).wrapping_mul(0x9E37_79B9_7F4A_7C15));
    let mut state = mixed;
    state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
    state = (state ^ (state >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    state = (state ^ (state >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    state ^ (state >> 31)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lcg_matches_dfu_15_bit_contract() {
        // `next = next·1103515245 + 12345`, bits `(next >> 16) & 0x7FFF`.
        let mut rng = DfRandom::with_seed(1);
        let first = rng.rand();
        assert!((0..=32_767).contains(&first), "15 bits: {first}");
        // Reproducible with the same seed, different with another one.
        let mut same = DfRandom::with_seed(1);
        assert_eq!(same.rand(), first);
        let mut other = DfRandom::with_seed(2);
        assert_ne!(other.rand(), first);
    }

    #[test]
    fn dice100_stays_in_0_99_and_is_seed_stable() {
        let rolls: Vec<i32> = {
            let mut dice = Dice100::with_seed(20_261_022);
            (0..200).map(|_| dice.roll()).collect()
        };
        assert!(rolls.iter().all(|roll| (0..100).contains(roll)));
        let mut again = Dice100::with_seed(20_261_022);
        assert_eq!(rolls, (0..200).map(|_| again.roll()).collect::<Vec<_>>());
    }

    #[test]
    fn actor_seed_is_stable_and_distinct_per_actor() {
        let a = actor_seed(20_261_022, 1);
        assert_eq!(a, actor_seed(20_261_022, 1), "same session+actor ⇒ same seed");
        assert_ne!(a, actor_seed(20_261_022, 2), "different actor ⇒ different seed");
        assert_ne!(a, actor_seed(20_261_023, 1), "different session ⇒ different seed");
    }

    #[test]
    fn range_inclusive_respects_bounds() {
        let mut rng = DfRandom::with_seed(42);
        for _ in 0..500 {
            let value = rng.range_inclusive(1500, 3000);
            assert!((1500..=3000).contains(&value), "{value}");
        }
        assert_eq!(rng.range_inclusive(7, 7), 7, "single-element range");
    }
}
