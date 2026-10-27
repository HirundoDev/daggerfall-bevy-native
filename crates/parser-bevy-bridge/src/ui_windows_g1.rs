//! Window group **G1 — character creation** (`CreateChar*.cs`, 16 windows).
//!
//! Census group, **not** a `WindowSpec` group: spec M34 §4 scopes its per-window
//! criterion to the **51 playable** windows — criterion (i)
//! (`docs/sintergic/game/programming/m34-e2e-integration-presentation-spec-CR-5243af63.md:219`)
//! asks a `WindowSpec` with `id`, `asset_retail` and `rect` of those 51. These 16
//! rows are the character-creation category of the 85-row census (§1.1 `:52`),
//! disjoint from its playable-with-retail-assets category (`:55`), and the spec
//! states no composition requirement for them: their contract is the census plus
//! the shell registry — `85/85` registered and navigable in
//! `PROBE-UI-FULL-001`.
//!
//! [`specs`] therefore returns `&[]` **by design**, not as a composition
//! pending from another wave: a [`WindowSpec`] invented here would be a
//! fabricated golden. `PROBE-UI-FULL-001` prints `grupos=[g1=0 g2=0 g3=51]`
//! with `EXIT=0` on that basis — §4(i) counts the 51 playable windows.
//!
//! The single concatenation point is [`crate::ui_spec::window_specs`]; callers
//! never chain the groups themselves.
//!
//! [`WindowSpec`]: crate::ui_spec::WindowSpec

use crate::ui_spec::WindowSpec;

/// Specs of this census group: none, by design — spec M34 §4(i) binds
/// [`WindowSpec`] to the 51 playable windows only.
pub fn specs() -> &'static [WindowSpec] {
    &[]
}
