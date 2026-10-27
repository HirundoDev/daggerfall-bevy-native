//! Window group **G2 — engine pages** (11 `*ConfigPage`/`GameEffectsConfigWindow`
//! + the 7 infrastructure windows = 18 rows, all `N/A-engine`).
//!
//! Census group, **not** a `WindowSpec` group. Spec M34 §4
//! (`docs/sintergic/game/programming/m34-e2e-integration-presentation-spec-CR-5243af63.md:224-225`)
//! declares these 18 rows `N/A-engine` — they are **not** 1996 UI — and demands
//! only that they **appear in the census**: omitting them is a census FAIL, not
//! a silent exclusion. No retail composition is asked of them, so there is no
//! `asset_retail`/`rect` to declare and [`specs`] returns `&[]` **by design**,
//! not as pending work.
//!
//! `PROBE-UI-FULL-001` counts them in `census: 85 = 16 createchar + 11 config +
//! 7 infra + 51 jugables` (spec `:230`), registers them in the shell (`85/85`)
//! and prints `grupos=[g1=0 g2=0 g3=51]` with `EXIT=0`.
//!
//! The single concatenation point is [`crate::ui_spec::window_specs`].

use crate::ui_spec::WindowSpec;

/// Specs of this census group: none, by design — the 18 engine rows are
/// `N/A-engine` and carry no retail composition (spec M34 §4, §9.5).
pub fn specs() -> &'static [WindowSpec] {
    &[]
}
