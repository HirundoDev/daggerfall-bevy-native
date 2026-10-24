//! Perception organ — rendering, input and sensory pipelines (F0 scaffold).
//!
//! M7: [`read_models`] — the three session projections of PD4 T7
//! (HUD · Automap · inventory/quests panel — REQ-L99-038/039/040).
//! This crate still does not depend on bevy (and 0 organ dependencies: the
//! `app` composes, bus-only).
//!
//! Spine (contract `spine-contracts-CR-f716455c.md` §5): [`ui_shell`] — the
//! `UiShell` (registry + stack of windows, `open`/`close`/`toggle`/`top`/
//! `route_input`). It holds ids and captions only: the read models stay in
//! [`read_models`], so the shell cannot own prices, HP or quests.

pub mod read_models;
pub mod ui_shell;

/// Canonical organ id (Sintergic organ map, ADR-FW-104).
pub const ORGAN: &str = "perception";

/// Human-readable organ name.
pub fn organ_name() -> &'static str {
    ORGAN
}
