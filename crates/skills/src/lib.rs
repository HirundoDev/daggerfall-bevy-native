//! Skills organ — reusable capabilities composed into gameplay flows (F0 scaffold).
//!
//! M5: [`formula`] — DFU trade price formula, *clean-room*, plus the
//! executable core of PROBE-FORMULA-001 (INV-L99-3, VSIM §4 row 4;
//! bevy-bridge-matrix canon: "Skills → formulas … PROBE-FORMULA-001").
//! M7: [`stats`] — `RpgStats` (health/stamina/level), source of the PD4 T7
//! HUD read model (REQ-L99-038, declared minimal contract).
//! M20: [`bio`], [`career`], [`character`], [`rng`] — living character:
//! biography (`BIO.DAT`/`BIOGnnT0.TXT`), career (`CLASSnn.CFG`),
//! attributes/skills with DFU progression and deterministic RNG (REQ-L99-053;
//! spec `m20-character-spec.md`).
//! M20: [`progression`] — gameplay signals (running, swimming, jumping,
//! climbing, dodging, strikes) → skill uses → `RaiseSkills` → level-up
//! (closes audit P1-3); [`humanoid`] — career/level contract of
//! a hostile humanoid for M25 HP/attributes (closes residual R1 #4).
//! The remaining capabilities arrive with their own lanes.

pub mod bio;
pub mod career;
pub mod character;
pub mod formula;
pub mod humanoid;
pub mod progression;
pub mod rng;
pub mod stats;

/// Canonical organ id (Sintergic organ map, ADR-FW-104).
pub const ORGAN: &str = "skills";

/// Human-readable organ name.
pub fn organ_name() -> &'static str {
    ORGAN
}
