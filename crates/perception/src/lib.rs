//! Perception organ — rendering, input and sensory pipelines (F0 scaffold).
//!
//! M7: [`read_models`] — the three session projections of PD4 T7
//! (HUD · Automap · inventory/quests panel — REQ-L99-038/039/040).
//! This crate still does not depend on bevy (and 0 organ dependencies: the
//! `app` composes, bus-only).

pub mod read_models;

/// Canonical organ id (Sintergic organ map, ADR-FW-104).
pub const ORGAN: &str = "perception";

/// Human-readable organ name.
pub fn organ_name() -> &'static str {
    ORGAN
}
