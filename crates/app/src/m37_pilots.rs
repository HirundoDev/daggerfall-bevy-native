//! M37/UI-1 **F2** — the three pilot windows of plan §4, composed on the
//! seven-axis layer (`daggerfall_parser_bevy_bridge::ui_compat`).
//!
//! The pilots are *declarations*: a `WindowId` from the census, ordered
//! `SurfaceSpec`s, `TextSource`s and `FocusTarget`s. Nothing here paints by
//! hand — surfaces leave through `WindowComposition::paint` (which delegates to
//! the M17 compositor) and glyphs through `typography::paint_runs`.
//!
//! | # | Pilot | Asset | Status (M36 audit) | Text | Buttons |
//! |:--|:---|:---|:---|:---|:---|
//! | 028 | `DaggerfallCharacterSheetWindow` | `INFO00I0.IMG` | PARCIAL | A4 `ReadModel` | — |
//! | 034 | `DaggerfallGuildServiceCureDisease` | `SPOP.RCI` (9-slice) | AUSENTE | A4 `Rsc` | `Activate` / `Cancel` |
//! | 043 | `DaggerfallListPickerWindow` | `PICK00I0.IMG` | AUSENTE | A4 `Literal` + `panel.item_count` | `Cancel` |
//!
//! **Why the windows are resolved by name:** `WindowId` is a typed handle over
//! the 85-window census; hard-coding a numeric constant would let a pilot point
//! at a window the registry does not know (`ui_shell::canonical_id_for` is the
//! single source). An unknown name is an error, never a default.

use daggerfall_nervous::events::WindowId;
use daggerfall_parser_bevy_bridge::ui_compat::{
    glyphs, paint_runs, resolve_text, GlyphRun, RetailFont, SurfaceNode, SurfaceOwnership,
    SurfaceSource, SurfaceSpec, TextEnv, TextSource, UiCompatError, WindowComposition,
};
use daggerfall_parser_bevy_bridge::ui_spec::{Framebuffer, UiAssets, UiRect};
use daggerfall_perception::ui_shell::canonical_id_for;

/// Coded ink used for body text (HUD window, index 0 is transparent).
const INK_BODY: u8 = 127;
/// Coded ink used for titles (still inside the HUD window `0..=254`).
const INK_TITLE: u8 = 208;

/// Error raised when a pilot names a window the census does not register.
const RULE_PILOT_WINDOW: &str = "pilot-window-not-in-census";

/// Where one text run comes from and where it starts.
///
/// `max_lines` caps the number of rendered lines (`0` = every line of the
/// resolved text); each line becomes its own `GlyphRun`, one font height apart.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextDecl {
    /// Declared source (A4): `Rsc`, `Literal` or `ReadModel`.
    pub source: TextSource,
    /// Retail face.
    pub font: RetailFont,
    /// Coded ink (A3: never 0, never above the HUD window).
    pub ink: u8,
    /// Top-left corner of the first line.
    pub at: (i16, i16),
    /// Maximum lines to render (`0` = all).
    pub max_lines: usize,
}

impl TextDecl {
    /// A single-line declaration.
    pub fn line(source: TextSource, font: RetailFont, ink: u8, at: (i16, i16)) -> Self {
        Self {
            source,
            font,
            ink,
            at,
            max_lines: 1,
        }
    }

    /// A multi-line declaration, capped at `max_lines` (`0` = all).
    pub fn block(
        source: TextSource,
        font: RetailFont,
        ink: u8,
        at: (i16, i16),
        max_lines: usize,
    ) -> Self {
        Self {
            source,
            font,
            ink,
            at,
            max_lines,
        }
    }
}

/// One focusable row of a pilot (A5/A6: the list the router walks).
///
/// The layer owns focus through `InputRouter::set_list(rows)`; this type is the
/// declaration side of the same list — row index, owning window and the canvas
/// rectangle the row occupies.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FocusTarget {
    /// Window that owns the row.
    pub window: WindowId,
    /// Row index inside the window (0-based, list order).
    pub row: usize,
    /// Row rectangle in the 320×200 canvas.
    pub rect: UiRect,
}

/// One composed pilot window.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PilotWindow {
    /// Stable key used by the gate evidence (`p1`/`p2`/`p3`).
    pub key: &'static str,
    /// Census class name.
    pub name: &'static str,
    /// Primary retail asset.
    pub asset: &'static str,
    /// Typed window handle resolved from the census.
    pub window: WindowId,
    /// Ordered surface declaration.
    pub composition: WindowComposition,
    /// Ordered text declarations.
    pub texts: Vec<TextDecl>,
    /// Focusable rows.
    pub focus: Vec<FocusTarget>,
}

impl PilotWindow {
    /// Resolves the declaration into paint-ready surface nodes.
    pub fn nodes(&self, assets: &UiAssets) -> Result<Vec<SurfaceNode>, UiCompatError> {
        self.composition.nodes(assets)
    }

    /// Resolves the text declarations into glyph runs (A3/A4).
    ///
    /// Every line is measured by the retail face (`text_width × fixed_height`);
    /// an empty resolution is an error (`resolve_text`), so a blank run can
    /// never reach the screen.
    pub fn glyphs(
        &self,
        assets: &UiAssets,
        env: &TextEnv<'_>,
    ) -> Result<Vec<GlyphRun>, UiCompatError> {
        let mut runs = Vec::new();
        for decl in &self.texts {
            let text = resolve_text(decl.source, env)?;
            let lines: Vec<&str> = text
                .lines()
                .map(str::trim_end)
                .filter(|line| !line.is_empty())
                .collect();
            let lines = if lines.is_empty() {
                vec![text.trim()]
            } else {
                lines
            };
            let mut y = decl.at.1;
            for (index, line) in lines.iter().enumerate() {
                if decl.max_lines > 0 && index >= decl.max_lines {
                    break;
                }
                let run = glyphs(decl.font, decl.ink, line, assets)?.at(decl.at.0, y);
                y += run.height as i16 + 1;
                runs.push(run);
            }
        }
        Ok(runs)
    }

    /// Paints surfaces then glyphs (surfaces first: text sits on top).
    pub fn paint(
        &self,
        framebuffer: &mut Framebuffer,
        assets: &UiAssets,
        env: &TextEnv<'_>,
    ) -> Result<(), UiCompatError> {
        self.composition.paint(framebuffer, assets)?;
        let runs = self.glyphs(assets, env)?;
        paint_runs(framebuffer, &runs, assets)
    }

    /// Number of focus rows the router must declare.
    pub fn focus_rows(&self) -> usize {
        self.focus.len()
    }
}

fn window(name: &'static str) -> Result<WindowId, UiCompatError> {
    canonical_id_for(name).ok_or(UiCompatError::Navigation(RULE_PILOT_WINDOW))
}

/// **P1 — #028 `DaggerfallCharacterSheetWindow`** (PARCIAL on `INFO00I0.IMG`).
///
/// The sheet is the full 320×200 raw RCI; the two satellites are owned
/// surfaces placed 1:1 at their declared offsets (`MAIN00I0.IMG` bottom bar,
/// `INVE03I0.IMG` the 39×66 paper doll). A4 reads the live HUD read models.
pub fn character_sheet() -> Result<PilotWindow, UiCompatError> {
    let name = "DaggerfallCharacterSheetWindow";
    let window = window(name)?;
    let composition = WindowComposition::new(
        window,
        vec![
            SurfaceSpec::owned(
                "INFO00I0.IMG",
                UiRect::new(0, 0, 320, 200),
                SurfaceSource::Whole,
            ),
            SurfaceSpec::owned(
                "MAIN00I0.IMG",
                UiRect::new(0, 154, 320, 46),
                SurfaceSource::Whole,
            ),
            SurfaceSpec::owned(
                "INVE03I0.IMG",
                UiRect::new(61, 60, 39, 66),
                SurfaceSource::Whole,
            ),
        ],
    );
    let texts = vec![
        TextDecl::line(TextSource::ReadModel("hud.health"), RetailFont::Default, INK_BODY, (8, 8)),
        TextDecl::line(
            TextSource::ReadModel("hud.stamina"),
            RetailFont::Default,
            INK_BODY,
            (8, 18),
        ),
        TextDecl::line(TextSource::ReadModel("hud.level"), RetailFont::Default, INK_TITLE, (8, 28)),
    ];
    let focus = vec![FocusTarget {
        window,
        row: 0,
        rect: UiRect::new(0, 0, 320, 200),
    }];
    Ok(PilotWindow {
        key: "p1",
        name,
        asset: "INFO00I0.IMG",
        window,
        composition,
        texts,
        focus,
    })
}

/// **P2 — #034 `DaggerfallGuildServiceCureDisease`** (AUSENTE) on `SPOP.RCI`.
///
/// `SPOP.RCI` decodes to a 66×66 3×3 grid, so the declaration is one `Whole`
/// surface at 198×132: the layer expands it to its **nine** pieces row-major
/// (4 corners 1:1, 4 edges stretched on one axis, 1 centre stretched on both).
/// A4 is the retail `TEXT.RSC` record 1509 ("Cure -- Disease").
pub fn guild_service_cure_disease() -> Result<PilotWindow, UiCompatError> {
    let name = "DaggerfallGuildServiceCureDisease";
    let window = window(name)?;
    let composition = WindowComposition::new(
        window,
        vec![SurfaceSpec::owned(
            "SPOP.RCI",
            UiRect::new(61, 34, 198, 132),
            SurfaceSource::Whole,
        )],
    );
    let texts = vec![
        TextDecl::line(
            TextSource::Rsc(1509),
            RetailFont::Small,
            INK_TITLE,
            (78, 44),
        ),
        TextDecl::block(
            TextSource::Rsc(1509),
            RetailFont::Default,
            INK_BODY,
            (78, 58),
            5,
        ),
        TextDecl::line(
            TextSource::Literal("ACTIVATE"),
            RetailFont::Default,
            INK_TITLE,
            (78, 140),
        ),
        TextDecl::line(
            TextSource::Literal("CANCEL"),
            RetailFont::Default,
            INK_TITLE,
            (170, 140),
        ),
    ];
    let focus = vec![
        FocusTarget {
            window,
            row: 0,
            rect: UiRect::new(78, 140, 80, 10),
        },
        FocusTarget {
            window,
            row: 1,
            rect: UiRect::new(170, 140, 60, 10),
        },
    ];
    Ok(PilotWindow {
        key: "p2",
        name,
        asset: "SPOP.RCI",
        window,
        composition,
        texts,
        focus,
    })
}

/// **P3 — #043 `DaggerfallListPickerWindow`** (AUSENTE) on `PICK00I0.IMG`.
///
/// The picker is 200×128 placed 1:1 at its declared header offset (60,36).
/// The six rows are the focusable list (A5/A6); `panel.item_count` is the A4
/// read-model line, and `Cancel` is the last focus row.
pub fn list_picker() -> Result<PilotWindow, UiCompatError> {
    let name = "DaggerfallListPickerWindow";
    let window = window(name)?;
    let composition = WindowComposition::new(
        window,
        vec![SurfaceSpec::owned(
            "PICK00I0.IMG",
            UiRect::new(60, 36, 200, 128),
            SurfaceSource::Whole,
        )],
    );
    let rows: [&'static str; 6] = ["SABRE", "CUIRASS", "POTION", "TORCH", "ROPE", "LANTERN"];
    let mut texts = vec![
        TextDecl::line(
            TextSource::Literal("ITEMS"),
            RetailFont::Small,
            INK_TITLE,
            (72, 44),
        ),
        TextDecl::line(
            TextSource::ReadModel("panel.item_count"),
            RetailFont::Default,
            INK_BODY,
            (232, 44),
        ),
    ];
    let mut focus = Vec::with_capacity(rows.len() + 1);
    for (row, label) in rows.iter().enumerate() {
        let y = 60 + row as i16 * 12;
        texts.push(TextDecl::line(
            TextSource::Literal(label),
            RetailFont::Default,
            INK_BODY,
            (78, y),
        ));
        focus.push(FocusTarget {
            window,
            row,
            rect: UiRect::new(72, y - 1, 176, 11),
        });
    }
    texts.push(TextDecl::line(
        TextSource::Literal("CANCEL"),
        RetailFont::Default,
        INK_TITLE,
        (78, 148),
    ));
    focus.push(FocusTarget {
        window,
        row: rows.len(),
        rect: UiRect::new(72, 147, 60, 11),
    });
    Ok(PilotWindow {
        key: "p3",
        name,
        asset: "PICK00I0.IMG",
        window,
        composition,
        texts,
        focus,
    })
}

/// The three pilots of plan §4 — exactly three, in evidence order.
pub fn pilots() -> Result<[PilotWindow; 3], UiCompatError> {
    Ok([
        character_sheet()?,
        guild_service_cure_disease()?,
        list_picker()?,
    ])
}

/// `true` when every node is owned by its window or nested in an open one.
///
/// The gate calls this after `WindowComposition::check_ownership`; it exists so
/// the evidence can state the ownership counts, not just the verdict.
pub fn ownership_counts(nodes: &[SurfaceNode]) -> (usize, usize, usize) {
    let mut owned = 0;
    let mut nested = 0;
    let mut external = 0;
    for node in nodes {
        match node.ownership {
            SurfaceOwnership::Owned => owned += 1,
            SurfaceOwnership::Nested(_) => nested += 1,
            SurfaceOwnership::External => external += 1,
        }
    }
    (owned, nested, external)
}
