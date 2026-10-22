//! Inventory organ — configuration, dependency wiring and asset/path
//! bookkeeping (F0 scaffold).
//!
//! Holds the PathChecker seed of the A/B data-universe mode (see
//! [`path_checker`]); further configuration surfaces land with their lanes.
//! M5: [`commerce`] — inventory-lite trade (gold + goods list, dialogue
//! session; 0 persistence, price-agnostic).

//! M21: [`items`] — `FALL.EXE` template + instance/material (spec M21 §1-§2),
//! [`equipment`] — 27-slot table (§3) and [`loot`] — matrices and deterministic
//! generation (§5). Pure: 0 Bevy; the catalog comes in as a parameter and the
//! RNG is explicit per container.

pub mod commerce;
pub mod equipment;
pub mod items;
pub mod loot;
pub mod path_checker;

/// Canonical organ id (Sintergic organ map, ADR-FW-104).
pub const ORGAN: &str = "inventory";

/// Human-readable organ name.
pub fn organ_name() -> &'static str {
    ORGAN
}
