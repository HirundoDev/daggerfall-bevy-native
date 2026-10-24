//! Nervous organ — event kinds and inter-organ messaging (F0 scaffold).
//!
//! Lanes L99 replace this placeholder with the lattice event bus integration
//! (publish/subscribe only; organs never call each other directly).
//!
//! **Spine (contract `spine-contracts-CR-f716455c.md` §6):** [`events`] types
//! the nine spine events and the scene vocabulary they carry. The vocabulary
//! lives here because §1 forbids the edge `nervous → app`: the crate that
//! types the messages cannot depend on the crate that orchestrates them.

pub mod events;

/// Canonical organ id (Sintergic organ map, ADR-FW-104).
pub const ORGAN: &str = "nervous";

/// Human-readable organ name.
pub fn organ_name() -> &'static str {
    ORGAN
}
