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
//!
//! M34 §1.6 (W34-6): [`lighting`] — torch, ambient, fade, sun, underwater fog
//! and dungeon-light rules (GV-10/11/12/14), consumed from the DFU sources
//! with no formula of its own.

pub mod lighting;
pub mod read_models;
pub mod ui_census;
pub mod ui_shell;

/// Canonical organ id (Sintergic organ map, ADR-FW-104).
pub const ORGAN: &str = "perception";

/// Human-readable organ name.
pub fn organ_name() -> &'static str {
    ORGAN
}
