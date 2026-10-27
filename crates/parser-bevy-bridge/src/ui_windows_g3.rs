//! Window group **G3 — the 51 playable windows** (inventory, shop, bank, guild,
//! court, tavern, travel, spellbook/maker, item/potion maker, journal, automap
//! ×2, HUD, book, messages, pause, classic load, start, settings, controls,
//! video, teleport, daedra, witches, repair service…).
//!
//! **Census-contract slot, not a composition slot.** The 51 `WindowSpec`s were
//! composed by W34-2a..2f in [`crate::ui_playable_g1`] (10 `G1` + 7 `G2` = 17),
//! [`crate::ui_playable_g2`] (6 `G3` + 5 `G4` = 11) and
//! [`crate::ui_playable_g3`] (9 `G5` + 14 `G6` = 23) — 17 + 11 + 23 = **51**,
//! the `g3` column of [`crate::ui_spec::window_group_counts`] and the reason
//! `PROBE-UI-FULL-001` prints `grupos=[g1=0 g2=0 g3=51]` with `EXIT=0`.
//!
//! [`specs`] returns `&[]` **by design**: this module is retained for the
//! census contract (spec M34 §1.1 `:55`, §4(i) `:219`) and is no longer part of
//! the concatenation — the composition it was once reserved for lives in the
//! `ui_playable_g*` slots, not here.
//!
//! The single concatenation point is [`crate::ui_spec::window_specs`].

use crate::ui_spec::WindowSpec;

/// Specs of this census-contract slot: none, by design — the 51 playable
/// windows are declared by [`crate::ui_playable_g1`],
/// [`crate::ui_playable_g2`] and [`crate::ui_playable_g3`].
pub fn specs() -> &'static [WindowSpec] {
    &[]
}
