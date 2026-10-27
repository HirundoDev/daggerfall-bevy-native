//! Per-axis errors of the UI compatibility layer.
//!
//! The plan (§3.9) fixes the **shape**: one variant per axis, each carrying a
//! `&'static str` rule. The rule string is not prose, it is the axis contract:
//! the named constructors ([`UiCompatError::unowned_surface`],
//! [`UiCompatError::palette_violation`], [`UiCompatError::missing_record`],
//! [`UiCompatError::empty_text`], [`UiCompatError::unknown_path`]) embed the
//! plan's own names (`UnownedSurface`, `PaletteViolation`, `MissingRecord`,
//! `UnknownPath`, `EmptyText`), so a consumer can match on the axis *and*
//! assert which rule fired without parsing English.
//!
//! Every constant in this module is public: the falsifier tests assert that
//! the exact rule they violate is the rule reported, which is what makes a
//! falsifier falsifiable (a test that only checks `is_err()` would pass after a
//! silent mis-route to another axis).

use std::fmt;

use crate::ui_spec::UiSpecError;

/// A1 rule 1: the declared asset is not in the bank.
pub const RULE_SURFACE_ORPHAN: &str = "A1 rule 1: the surface names an asset that is not in the bank";
/// A1 rule 2: the surface does not fit the 320×200 canvas (or its source cell
/// is outside the asset).
pub const RULE_SURFACE_CANVAS: &str = "A1 rule 2: the surface rectangle does not fit the 320x200 canvas";
/// A1 rule 2: a whole-source surface must place the whole asset.
pub const RULE_SURFACE_WHOLE: &str =
    "A1 rule 2: a whole-source surface must place the whole asset (declare a rect to clip)";
/// A1 rule 2: the source cell is outside the decoded image.
pub const RULE_SURFACE_SOURCE: &str =
    "A1 rule 2: the source cell is outside the decoded image (no silent partial blit)";
/// A1 rule 2: the asset geometry does not declare records.
pub const RULE_SURFACE_RECORD: &str =
    "A1 rule 2: the asset geometry does not declare the requested record";
/// A1 rule 3: the nine pieces do not cover the target exactly once.
pub const RULE_SURFACE_COVERAGE: &str =
    "A1 rule 3: the nine pieces do not cover the target rectangle (area sum != target area)";
/// A1 rule 3: the target is smaller than its own corner cells.
pub const RULE_SURFACE_MINIMUM: &str =
    "A1 rule 3: the nine-slice target is smaller than its corner cells (2 x side per axis)";
/// A1 rule 4: the nested surface owner window is not open.
pub const RULE_UNOWNED_SURFACE: &str =
    "A1 rule 4: the owning window of a nested surface is not open (UnownedSurface)";

/// A2: the asset named by the identity is not in the bank.
pub const RULE_PALETTE_ASSET: &str =
    "A2: the asset named by the palette identity is not in the bank";
/// A2 rule 1: the reported range is not the asset-derived one.
pub const RULE_PALETTE_DERIVED: &str =
    "A2 rule 1: the reported range is not derived from the asset (never 1..=255)";
/// A2 rule 2: the ink index is outside the asset palette range.
pub const RULE_PALETTE_INK: &str =
    "A2 rule 2: the ink index is outside the asset palette range (PaletteViolation)";
/// A2 rule 3: the asset exposes fewer than two distinct indices.
pub const RULE_PALETTE_FLAT: &str =
    "A2 rule 3: the asset exposes fewer than two distinct indices (no readable ink)";

/// A3 rule 1: the font is not one of the five retail `.FNT` files.
pub const RULE_FONT_REGISTRY: &str =
    "A3 rule 1: the font is not one of the five retail .FNT files";
/// A3 rule 2: `ink == 0` is transparent.
pub const RULE_INK_TRANSPARENT: &str =
    "A3 rule 2: ink 0 is transparent, so a run with ink 0 would paint nothing";
/// A3 rule 2: the run ink is outside the HUD palette range.
pub const RULE_INK_RANGE: &str = "A3 rule 2: the run ink is outside the HUD palette range";
/// A3 rule 4: the run extent is not the `.FNT` advance.
pub const RULE_GLYPH_ADVANCE: &str =
    "A3 rule 4: the run extent is not the .FNT advance (text_width x fixed_height)";
/// A3 rule 4: a run with no text has no advance.
pub const RULE_GLYPH_EMPTY: &str =
    "A3 rule 4: a glyph run with no text has no advance (declare no run at all)";
/// A3 rule 4: the run rectangle does not fit the canvas.
pub const RULE_GLYPH_CANVAS: &str =
    "A3 rule 4: the glyph run rectangle does not fit the 320x200 canvas";

/// A4 rule 1: the `TEXT.RSC` record does not exist.
pub const RULE_MISSING_RECORD: &str =
    "A4 rule 1: the TEXT.RSC record does not exist (MissingRecord)";
/// A4 rule 2: the read-model path is not declared.
pub const RULE_UNKNOWN_PATH: &str =
    "A4 rule 2: the read-model path is not one of the declared paths (UnknownPath)";
/// A4 rule 3: the resolved text is empty.
pub const RULE_EMPTY_TEXT: &str = "A4 rule 3: the resolved text is empty (EmptyText)";
/// A4 rule 1: the source needs an environment that was not supplied.
pub const RULE_NO_ENV: &str =
    "A4 rule 1: this text source needs an environment that was not supplied";
/// A4 rule 1: the record table was supplied without the file bytes.
pub const RULE_NO_RSC_BYTES: &str =
    "A4 rule 1: the record table was supplied without the TEXT.RSC bytes";

/// A5 rule 1: only a registered `WindowId` addresses a window.
pub const RULE_WINDOW_TYPED: &str =
    "A5 rule 1: only a registered WindowId addresses a window (0 string windows)";
/// A5 rule 2: closing an absent window is an error, not a silent no-op.
pub const RULE_CLOSE_ABSENT: &str =
    "A5 rule 2: closing a window that is not open is an error, not a silent no-op";
/// A5 rule 3: no duplicate window in the stack.
pub const RULE_OPEN_DUPLICATE: &str =
    "A5 rule 3: the window is already open (no duplicate in the stack)";
/// A5: the shell refused the operation.
pub const RULE_SHELL_REFUSED: &str = "A5: the perception shell refused the operation";

/// A6 rule 1: one event is delivered to at most one window.
pub const RULE_SINGLE_WINDOW: &str = "A6 rule 1: one event is delivered to at most one window";
/// A6 rule 5: every retail key and direction has a mapping.
pub const RULE_TOTAL_MAP: &str = "A6 rule 5: every retail key and direction has a mapping";

/// Drop reason: nothing is open, so there is no focus to deliver to.
pub const DROP_NO_FOCUS: &str = "no-focus";
/// Drop reason: `Cancel` on the single root window (§3.6 rule 3).
pub const DROP_ROOT_WINDOW: &str = "root-window";
/// Drop reason: `Scroll` with no rows to move through.
pub const DROP_EMPTY_LIST: &str = "empty-list";

/// A7 rule 2: only an integer factor that fits 640×480 is admissible.
pub const RULE_FACTOR_FIT: &str =
    "A7 rule 2: only an integer presentation factor that fits 640x480 (1 or 2) is admissible";

/// UI compatibility layer error — one variant per axis (§3.9).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UiCompatError {
    /// A1: surfaces.
    Surface(&'static str),
    /// A2: palette index ranges.
    Palette(&'static str),
    /// A3: fonts, glyph runs and ink.
    Typography(&'static str),
    /// A4: text resolution.
    Text(&'static str),
    /// A5: window navigation.
    Navigation(&'static str),
    /// A6: input routing.
    Input(&'static str),
    /// A7: presentation.
    Presentation(&'static str),
}

impl UiCompatError {
    /// Axis identifier (`"A1"`..`"A7"`), so a consumer can group failures
    /// without matching every variant.
    pub fn axis(&self) -> &'static str {
        match self {
            Self::Surface(_) => "A1",
            Self::Palette(_) => "A2",
            Self::Typography(_) => "A3",
            Self::Text(_) => "A4",
            Self::Navigation(_) => "A5",
            Self::Input(_) => "A6",
            Self::Presentation(_) => "A7",
        }
    }

    /// The rule string carried by the variant.
    pub fn rule(&self) -> &'static str {
        match self {
            Self::Surface(rule)
            | Self::Palette(rule)
            | Self::Typography(rule)
            | Self::Text(rule)
            | Self::Navigation(rule)
            | Self::Input(rule)
            | Self::Presentation(rule) => rule,
        }
    }

    /// A1 rule 4 — the nested owner of a surface is not open.
    pub fn unowned_surface() -> Self {
        Self::Surface(RULE_UNOWNED_SURFACE)
    }

    /// A2 rule 2 — an ink outside the asset palette range.
    pub fn palette_violation() -> Self {
        Self::Palette(RULE_PALETTE_INK)
    }

    /// A4 rule 1 — a `TEXT.RSC` record that does not exist.
    pub fn missing_record() -> Self {
        Self::Text(RULE_MISSING_RECORD)
    }

    /// A4 rule 3 — an empty resolved text.
    pub fn empty_text() -> Self {
        Self::Text(RULE_EMPTY_TEXT)
    }

    /// A4 rule 2 — a read-model path that is not declared.
    pub fn unknown_path() -> Self {
        Self::Text(RULE_UNKNOWN_PATH)
    }

    /// Maps a compositor error onto the axis whose preconditions should have
    /// made it impossible.
    ///
    /// The layer validates assets, rectangles and inks *before* painting, so a
    /// [`UiSpecError`] here means a declaration slipped past those checks; the
    /// mapping keeps the failure typed per axis instead of leaking the
    /// compositor type into the compatibility API.
    pub(crate) fn from_spec(error: UiSpecError) -> Self {
        match error {
            UiSpecError::MissingAsset { .. } => Self::Surface(RULE_SURFACE_ORPHAN),
            UiSpecError::MissingFont { .. } => Self::Typography(RULE_FONT_REGISTRY),
            UiSpecError::PaletteViolation { .. } => Self::palette_violation(),
            UiSpecError::OutOfCanvas { .. } => Self::Surface(RULE_SURFACE_CANVAS),
            UiSpecError::SourceOutOfBounds { .. } => Self::Surface(RULE_SURFACE_SOURCE),
            UiSpecError::GenericWidget { .. } => Self::Surface(RULE_SURFACE_ORPHAN),
        }
    }
}

impl fmt::Display for UiCompatError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.axis(), self.rule())
    }
}

impl std::error::Error for UiCompatError {}
