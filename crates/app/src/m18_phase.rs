//! M18 · **B1-d (O8)** — startup phase before the world: `Menu` → `Playing`.
//!
//! The 1996 game opens on the main menu; the world starts only after the player
//! enters it. This module is the **single** state machine both consumers share:
//!
//! - the window (`m18_window`, feature `window`) inserts [`AppPhase::Menu`] at
//!   startup and gates every **world** system with [`phase_playing`]: while the
//!   phase is `Menu` the locomotion, combat, progression, audio and law systems
//!   do not tick, so the game clock advances **0 minutes** and the spawn is the
//!   untouched composition;
//! - `--probe MENU-001` (`m18_b1_probe`) drives the SAME [`AppPhase`] and the
//!   SAME run condition in a headless Bevy `Schedule` (0 GPU, 0 window): N
//!   frames without `Enter` leave the phase and the tick counter at 0, one
//!   `Enter` switches to `Playing` and the spawn/HUD digests equal the ones of
//!   the default composition.
//!
//! Besides gating systems, the phase decides **what 2D surface is on screen**
//! ([`visible_panel_index`] + [`hud_visible`]): `Menu` shows the retail
//! `main-menu` panel (`TITL00I0` + `MENU.IMG` + buttons, index 3 of
//! `ui_spec::build_all_panels`) with the live HUD hidden; `Playing` shows the
//! `hud` panel and the M20 HUD sprite, i.e. the default world view. The window
//! and `MENU-001` consume the same selector, so the panel id the probe reads
//! from the composed scene is the panel the screen shows (never the title).
//!
//! [`phase_after_enter`] is the pure transition: one-way (`Menu` → `Playing`,
//! `Playing` stays `Playing`), so a stray `Enter` while playing can never reset
//! a session.
//!
//! **Default behavior is byte-identical to the pre-B1 build:** the composition
//! (`compose_demo_with(None, None)`), the spawn and the HUD of the first frame
//! are the same; the only difference is that the world systems wait for the
//! `Enter` the original also waits for.

use bevy::prelude::Resource;

/// Stable id of the retail panel the 1996 game shows while the menu is up
/// (`ui_spec::UiPanelId::MainMenu`, index 3 of `ui_spec::build_all_panels`:
/// `TITL00I0.IMG` + `MENU.IMG` + `BUTN00`/`BUTN03`).
pub const MENU_PANEL_ID: &str = "main-menu";
/// Stable id of the world-view panel (`ui_spec::UiPanelId::Hud`, index 0).
pub const HUD_PANEL_ID: &str = "hud";

/// Startup phase of the demo window.
///
/// `Menu` is the default: the world composes (the scene is the menu backdrop)
/// but no world system ticks. `Playing` is entered with `Enter` and is final.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AppPhase {
    /// Before entering the world: world systems idle, game clock at 0 minutes.
    #[default]
    Menu,
    /// The world is live (identical to the pre-B1 behavior).
    Playing,
}

impl AppPhase {
    /// Uppercase tag printed in the window title and the evidence lines
    /// (`MENU` / `PLAYING`).
    pub fn label(self) -> &'static str {
        match self {
            Self::Menu => "MENU",
            Self::Playing => "PLAYING",
        }
    }

    /// Does this phase run the world systems?
    pub fn allows_world(self) -> bool {
        matches!(self, Self::Playing)
    }
}

/// Run condition of every **world** system: `true` only in [`AppPhase::Playing`].
///
/// Consumed by `m18_window` (`add_systems(..., ().run_if(phase_playing))`) and by
/// `MENU-001`, so the gate the probe measures IS the gate the window uses.
pub fn phase_playing(phase: bevy::prelude::Res<AppPhase>) -> bool {
    phase.allows_world()
}

/// Pure transition of the `Enter` key: `Menu` ⇒ `Playing`; `Playing` is stable
/// (0 way back, a session is never reset by a stray keypress).
pub fn phase_after_enter(phase: AppPhase) -> AppPhase {
    match phase {
        AppPhase::Menu => AppPhase::Playing,
        AppPhase::Playing => AppPhase::Playing,
    }
}

/// Index (inside `panel_ids`) of the M17 panel the 2D layer shows in `phase`.
///
/// `Menu` ⇒ [`MENU_PANEL_ID`] (the retail `main-menu` composition), `Playing`
/// ⇒ [`HUD_PANEL_ID`] (the default world view). The window (`m18_phase_ui`)
/// and `MENU-001` consume **this** function, so the panel id the probe reads
/// from the composed scene IS the panel the screen shows — never a label.
///
/// `None` when the composition carries no panel with that id: the consumer
/// leaves the surface as it is instead of inventing an index.
pub fn visible_panel_index(phase: AppPhase, panel_ids: &[&str]) -> Option<usize> {
    let wanted = match phase {
        AppPhase::Menu => MENU_PANEL_ID,
        AppPhase::Playing => HUD_PANEL_ID,
    };
    panel_ids.iter().position(|id| *id == wanted)
}

/// Is the M20 HUD sprite part of the phase's surface?
///
/// The world view owns it (`Playing`); the menu shows the retail menu
/// composition only, so the live status line is hidden while the phase is
/// `Menu` (CS-20 judges the menu, not the world).
pub fn hud_visible(phase: AppPhase) -> bool {
    phase.allows_world()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `Enter` enters the world exactly once; a second press is a no-op.
    #[test]
    fn enter_is_one_way() {
        assert_eq!(phase_after_enter(AppPhase::Menu), AppPhase::Playing);
        assert_eq!(phase_after_enter(AppPhase::Playing), AppPhase::Playing);
    }

    /// The default phase is the menu and it does NOT run world systems.
    #[test]
    fn menu_is_default_and_idle() {
        assert_eq!(AppPhase::default(), AppPhase::Menu);
        assert!(!AppPhase::Menu.allows_world());
        assert!(AppPhase::Playing.allows_world());
        assert_eq!(AppPhase::Menu.label(), "MENU");
        assert_eq!(AppPhase::Playing.label(), "PLAYING");
    }

    /// The menu shows the retail `main-menu` panel (index 3 of the composed
    /// panels, exactly as `ui_spec::build_all_panels` returns it) and the
    /// world view shows the `hud`; the HUD sprite belongs to `Playing` only.
    ///
    /// Negative control: feeding the panel list without `main-menu` yields
    /// `None` — the selector never falls back to an invented index.
    #[test]
    fn phase_selects_the_retail_panel() {
        let ids = ["hud", "inventory", "quests", "main-menu"];
        assert_eq!(visible_panel_index(AppPhase::Menu, &ids), Some(3));
        assert_eq!(visible_panel_index(AppPhase::Playing, &ids), Some(0));
        assert!(!hud_visible(AppPhase::Menu));
        assert!(hud_visible(AppPhase::Playing));
        let without_menu = ["hud", "inventory", "quests"];
        assert_eq!(visible_panel_index(AppPhase::Menu, &without_menu), None);
    }
}
