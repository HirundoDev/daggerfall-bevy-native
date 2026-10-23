//! Combat organ (Sintergic canon, ADR-FW-104) — M22: melee attack resolution,
//! enemy melee timer and death sequence with latch (spec M22 §1-§4).
//!
//! Crate structure (ORDER M22):
//!
//! - [`rng`] — `DFRandom` **per actor** (LCG u64 `next·1103515245 + 12345`,
//!   15 bits) + `Dice100` 0..99 over it. Delta D-M22-1: DFU shares the
//!   state in static fields; here each actor carries its own — the probe
//!   requires determinism per seed and sensitivity to the seed (spec §6).
//! - [`actor`] — [`Combatant`] (with `strength_modifier` composed by the
//!   app since M20, ORDER: 0 crate cycle) and [`Weapon`] (view composed
//!   since M21).
//! - [`engine`] — [`resolve_attack`] (material gate with 0 rolls; roll order
//!   part → critical → success → damage range), [`resolve_monster_attack`]
//!   (monster branch: part once, up to three reflex-gated bands),
//!   [`EnemyAttack`] (`ResetMeleeTimer`) and [`DeathSequence`] (latch
//!   `deathInProgress` + fall/fade/reset).
//!
//! - [`enchantments`] — M23 seam: the out-of-band item callbacks of a
//!   **resolved** strike (`Strikes` on the weapon, `WeaponManager.cs:618` /
//!   `EnemyAttack.cs:267,382`; `Struck` on the defender's artifacts,
//!   `FormulaHelper.cs:704-719`). The M22 formula stays pure; the
//!   app composes [`enchantments::strike_callbacks`] right after it, so the
//!   effect layer runs from the game's strike step and not only from a probe.
//!
//! Pure: 0 Bevy, 0 I/O; the PRNG enters by seed, never `thread_rng` nor
//! wall clock (spec §6 "Determinism").

pub mod actor;
pub mod enchantments;
pub mod engine;
pub mod rng;

pub use actor::{Combatant, EntityKind, Weapon};
pub use engine::{
    resolve_attack, resolve_monster_attack, AttackInput, AttackOutcome, DeathSequence, EnemyAttack,
    MonsterAttackInput, MonsterAttackOutcome, MATERIAL_SILVER,
};
pub use enchantments::{strike_callbacks, StrikeCallbacks};
pub use rng::{actor_seed, DfRandom, Dice100};

/// Canonical organ id (Sintergic organ map, ADR-FW-104).
pub const ORGAN: &str = "combat";

/// Human-readable organ name.
pub fn organ_name() -> &'static str {
    ORGAN
}
