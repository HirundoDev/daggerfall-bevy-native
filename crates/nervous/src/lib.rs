//! Nervous organ — event kinds and inter-organ messaging (F0 scaffold).
//!
//! Lanes L99 replace this placeholder with the lattice event bus integration
//! (publish/subscribe only; organs never call each other directly).

/// Canonical organ id (Sintergic organ map, ADR-FW-104).
pub const ORGAN: &str = "nervous";

/// Human-readable organ name.
pub fn organ_name() -> &'static str {
    ORGAN
}
