//! Heart organ — attention focus and long-running coordination (F0 scaffold).
//!
//! Placeholder until the gameplay-loop lane implements attention focus
//! (FDC) and orchestration of multi-step flows.
//!
//! M4: [`clock`] — minimal playable time system (owned by Heart
//! per PRE-3 §3: time orchestration `DaggerfallDateTime`/day-night →
//! heart); pure Rust, 0 dependencies, DFU classic start Mode A.
//!
//! M9: [`scheduler`] — deterministic phase scheduler
//! (`Input`/`Simulation`/`Presentation`) draining pending events in
//! `(priority, seq)` order over the real [`clock::GameClock`].

pub mod clock;
pub mod scheduler;

/// Canonical organ id (Sintergic organ map, ADR-FW-104).
pub const ORGAN: &str = "heart";

/// Human-readable organ name.
pub fn organ_name() -> &'static str {
    ORGAN
}
