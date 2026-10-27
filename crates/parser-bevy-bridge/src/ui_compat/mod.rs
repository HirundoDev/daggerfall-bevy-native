//! M37/UI-1 — the seven-axis UI compatibility layer (plan
//! `docs/sintergic/l5-mission/m37-ui1-plan-CR-c45ab703-1122-4025-9780-7cc523433e9c.md`
//! §3; axis semantics frozen by the M36/UI-0 audit §3).
//!
//! **Boundary:** data + math, same as [`crate::ui_spec`] — 0 Bevy, 0 window,
//! 0 persistence. The layer sits **on top of** the M17 compositor: every pixel
//! still leaves through [`crate::ui_spec::Framebuffer::paint_nodes`], and every
//! presented frame still leaves through
//! [`crate::ui_spec::present_nearest`]. This module does not re-implement the
//! blitter; it adds the seven rules the compositor does not own.
//!
//! | Axis | Module | Rule this module enforces |
//! |:---|:---|:---|
//! | A1 | [`surface`] | every surface names a bank asset, fits the 320×200 canvas, covers its target and names an owner window that is open |
//! | A2 | [`palette`] | the palette identity is the asset's (never a default range) and every ink lies inside that range |
//! | A3 | [`typography`] | the five retail `.FNT` only, real advances, and no transparent ink |
//! | A4 | [`text`] | text is a `TEXT.RSC` record, a declared read-model path or a literal — never an empty placeholder |
//! | A5 | [`navigation`] | navigation is a typed stack over [`daggerfall_perception::ui_shell::UiShell`] — no string windows, no phantom closes |
//! | A6 | [`input`] | one retail event reaches at most one window, with a focus order and a total key map |
//! | A7 | [`presentation`] | integer presentation over 640×480, decided outside the composition |
//!
//! **Falsifiers:** every axis ships `falsifier_aN_*` tests. Each one is written
//! against the *violation* it catches (empty bank, out-of-range ink, constant
//! advance, unknown path, string window, double delivery, fractional factor);
//! a test that cannot fail would not be shipped.
//!
//! **Determinism:** every digest in this module is FNV-1a64 over an explicitly
//! ordered canonical byte form documented at its call site. No `HashMap` or
//! `HashSet` participates in a pixel path or a digest path — `BTreeMap` and
//! `Vec` only, so iteration order is defined by the data, never by hashing.

pub mod error;
pub mod input;
pub mod navigation;
pub mod palette;
pub mod presentation;
pub mod surface;
pub mod text;
pub mod typography;

#[cfg(test)]
mod test_support;

pub use error::UiCompatError;
pub use input::{
    map_drag, map_key, map_mouse, Direction, InputIntent, InputRouter, MouseButton, MouseEvent,
    RetailKey, Routed,
};
pub use navigation::Navigation;
pub use palette::{ink_contrast, ink_for, palette_identity, InkRole, PaletteIdentity};
pub use presentation::Presentation;
pub use surface::{
    place, place_nine_slice, verify_ownership, SurfaceKind, SurfaceNode, SurfaceOwnership,
    SurfaceSource, SurfaceSpec, WindowComposition,
};
pub use text::{is_read_model_path, resolve_text, resolve_texts, TextEnv, TextSource};
pub use typography::{glyphs, paint_runs, GlyphRun, RetailFont};

/// The seven axes, in plan order: `(axis id, module name)`.
///
/// The pilot windows (`plan §4`) and the two gates (`plan §5`) address the
/// axes by these identifiers, so the list is data, not documentation.
pub const AXES: [(&str, &str); 7] = [
    ("A1", "surface"),
    ("A2", "palette"),
    ("A3", "typography"),
    ("A4", "text"),
    ("A5", "navigation"),
    ("A6", "input"),
    ("A7", "presentation"),
];

/// FNV-1a64 (the offset basis and prime of the crate's other digests).
///
/// Callers build the canonical byte form of their payload first and hash it in
/// one pass: the digest is a function of `bytes` alone, so two runs over the
/// same declaration order produce the same value.
pub(crate) fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in bytes {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}
