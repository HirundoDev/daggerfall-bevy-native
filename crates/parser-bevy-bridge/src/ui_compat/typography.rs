//! A3 — typography axis: the five retail fonts, real advances and readable ink
//! (plan §3.3).
//!
//! **The rule:** the type system of the UI is exactly the five `.FNT` files of
//! [`crate::ui_spec::FONT_FILES`] — no system font, no face invented by the
//! layer. A [`GlyphRun`] measures itself from the font (`text_width` ×
//! `fixed_height`), so its extent is an *advance*, not a constant, and its ink
//! is a palette index that cannot be 0 (transparent) and must fall inside the
//! HUD window.
//!
//! **Role mapping** (from the M34 window census, `font` column of
//! `docs/sintergic/l5-mission/m34-window-assets-CR-2d5c8f41.tsv` — 53 rows
//! label `FONT0003` as `DefaultFont`, plus `SmallFont`/`LargeFont` and one
//! window **title** on `FONT0001`):
//!
//! | Role | File | Cell (measured, 8 644 B each) |
//! |:---|:---|:---|
//! | `Default` | `FONT0003.FNT` | 5×7 |
//! | `Small` | `FONT0002.FNT` | 5×5 |
//! | `Large` | `FONT0000.FNT` | 14×11 |
//! | `Title` | `FONT0001.FNT` | 14×14 |
//! | `Big` | `FONT0004.FNT` | 5×6 |
//!
//! `FONT0004` is the only file the census leaves unlabelled, so it takes the
//! remaining slot. [`crate::ui_spec::DEFAULT_FONT`] (`FONT0000`) is the M17
//! panel constant, **not** the retail role registry: the two are different
//! facts and the layer keeps both.
//!
//! **Painting:** a run is painted by
//! [`crate::ui_spec::Framebuffer::paint_nodes`] with the run's text and font —
//! the mask below is the *measurement* artifact (digest, extent, count), so
//! there is still exactly one blitter in the crate and the painted pixels
//! cannot drift from the measured ones.

use daggerfall_parser_rs::image::{family_index_range, FntFile, PaletteFamily};

use super::error::{
    UiCompatError, RULE_FONT_REGISTRY, RULE_GLYPH_ADVANCE, RULE_GLYPH_CANVAS, RULE_GLYPH_EMPTY,
    RULE_INK_RANGE, RULE_INK_TRANSPARENT,
};
use super::fnv1a64;
use crate::ui_spec::{
    palette_hash, Framebuffer, UiAnchor, UiAssets, UiNode, UiNodeKind, UiRect,
};

/// The five retail fonts, by role.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum RetailFont {
    /// Body text (`FONT0003.FNT`, 5×7).
    Default,
    /// Small captions (`FONT0002.FNT`, 5×5).
    Small,
    /// Large captions (`FONT0000.FNT`, 14×11).
    Large,
    /// Window titles (`FONT0001.FNT`, 14×14).
    Title,
    /// Largest face (`FONT0004.FNT`, 5×6).
    Big,
}

impl RetailFont {
    /// Every role, in declaration order.
    pub fn all() -> [RetailFont; 5] {
        [
            Self::Default,
            Self::Small,
            Self::Large,
            Self::Title,
            Self::Big,
        ]
    }

    /// File name in the bank (`FONT0003.FNT`).
    pub fn file(self) -> &'static str {
        match self {
            Self::Default => "FONT0003.FNT",
            Self::Small => "FONT0002.FNT",
            Self::Large => "FONT0000.FNT",
            Self::Title => "FONT0001.FNT",
            Self::Big => "FONT0004.FNT",
        }
    }

    /// Stem without the extension (`FONT0003`).
    pub fn stem(self) -> &'static str {
        self.file().trim_end_matches(".FNT")
    }

    /// Stable role name (for reports).
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::Small => "small",
            Self::Large => "large",
            Self::Title => "title",
            Self::Big => "big",
        }
    }

    /// Role of a file name, accepting the stem or the full name.
    ///
    /// `None` for anything that is not one of the five retail files — the
    /// registry is closed, so an unknown face is a caller error, not a default.
    pub fn parse(name: &str) -> Option<Self> {
        let stem = name
            .trim()
            .trim_end_matches(".fnt")
            .trim_end_matches(".FNT")
            .to_ascii_uppercase();
        Self::all()
            .into_iter()
            .find(|font| font.stem() == stem)
    }
}

/// A measured glyph run: extent, 1 bpp mask and digest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GlyphRun {
    /// Font of the run.
    pub font: RetailFont,
    /// Ink index written for every set bit.
    pub ink: u8,
    /// Placement rectangle (width/height are the font advance).
    pub rect: UiRect,
    /// Text the run renders.
    pub text: String,
    /// Row-major mask, `true` = ink.
    pub mask: Vec<bool>,
    /// Advance width in pixels (`FntFile::text_width`).
    pub width: u16,
    /// Face height in pixels (`FntFile::fixed_height`).
    pub height: u16,
    /// FNV-1a64 of the canonical form (see [`glyph_digest`]).
    pub digest: u64,
}

impl GlyphRun {
    /// Mask pixel (`false` outside the mask).
    pub fn pixel(&self, x: u16, y: u16) -> bool {
        if x >= self.width || y >= self.height {
            return false;
        }
        self.mask[y as usize * self.width as usize + x as usize]
    }

    /// Number of ink pixels.
    pub fn ink_pixels(&self) -> usize {
        self.mask.iter().filter(|bit| **bit).count()
    }

    /// Places the run at `(x, y)`, keeping its measured extent.
    pub fn at(mut self, x: i16, y: i16) -> Self {
        self.rect = UiRect::new(x, y, self.width, self.height);
        self
    }

    /// The compositor node of this run (`GlyphRun` primitive).
    pub fn ui_node(&self, palette: u64) -> UiNode {
        UiNode {
            id: format!("{}@{}", self.font.file(), self.rect.y),
            kind: UiNodeKind::GlyphRun,
            rect: self.rect,
            anchor: UiAnchor::Declared("ui_compat::typography::glyphs"),
            asset: self.font.file().to_string(),
            family: PaletteFamily::Hud,
            palette_hash: palette,
            source: None,
            text: Some(self.text.clone()),
            ink: self.ink,
        }
    }
}

/// Canonical digest form of a run, fed to FNV-1a64 in this exact order:
///
/// ```text
/// b"ui_compat/glyph-run/v1\0"   // domain separator (24 B)
/// font.file() as ASCII bytes     // e.g. b"FONT0003.FNT" (12 B)
/// 0x00                           // terminator of the name
/// ink                            // 1 B
/// width.to_le_bytes()            // 2 B
/// height.to_le_bytes()           // 2 B
/// mask bytes, row-major          // 1 B per pixel: 1 = ink, 0 = transparent
/// ```
///
/// The text itself is **not** hashed: two strings with the same mask, font and
/// ink paint the same pixels, and the digest names pixels.
pub fn glyph_digest(font: RetailFont, ink: u8, width: u16, height: u16, mask: &[bool]) -> u64 {
    let mut bytes = Vec::with_capacity(24 + font.file().len() + 1 + 1 + 4 + mask.len());
    bytes.extend_from_slice(b"ui_compat/glyph-run/v1\0");
    bytes.extend_from_slice(font.file().as_bytes());
    bytes.push(0);
    bytes.push(ink);
    bytes.extend_from_slice(&width.to_le_bytes());
    bytes.extend_from_slice(&height.to_le_bytes());
    bytes.extend(mask.iter().map(|bit| u8::from(*bit)));
    fnv1a64(&bytes)
}

/// Measures `text` with `font` and validates the ink (plan §3.3 rules 1-4).
///
/// - rule 1: the font is one of the five retail files present in the bank;
/// - rule 2: `ink != 0` (index 0 is transparent) and inside the HUD window;
/// - rule 4: the extent is `text_width × fixed_height`, never a constant, and
///   the rendered mask agrees with it (a run is measured once, not re-derived
///   by the consumer).
pub fn glyphs(
    font: RetailFont,
    ink: u8,
    text: &str,
    assets: &UiAssets,
) -> Result<GlyphRun, UiCompatError> {
    if text.is_empty() {
        return Err(UiCompatError::Typography(RULE_GLYPH_EMPTY));
    }
    if ink == 0 {
        return Err(UiCompatError::Typography(RULE_INK_TRANSPARENT));
    }
    if !family_index_range(PaletteFamily::Hud).contains(&ink) {
        return Err(UiCompatError::Typography(RULE_INK_RANGE));
    }
    let face: &FntFile = assets
        .font(font.file())
        .ok_or(UiCompatError::Typography(RULE_FONT_REGISTRY))?;
    let width = face.text_width(text);
    let height = face.fixed_height;
    if width == 0 || height == 0 {
        return Err(UiCompatError::Typography(RULE_GLYPH_ADVANCE));
    }
    let rendered = face.render_text(text);
    if rendered.width != width || rendered.height != height {
        return Err(UiCompatError::Typography(RULE_GLYPH_ADVANCE));
    }
    let mut mask = Vec::with_capacity(width as usize * height as usize);
    for y in 0..height {
        for x in 0..width {
            mask.push(rendered.pixel(x, y));
        }
    }
    let digest = glyph_digest(font, ink, width, height, &mask);
    Ok(GlyphRun {
        font,
        ink,
        rect: UiRect::new(0, 0, width, height),
        text: text.to_string(),
        mask,
        width,
        height,
        digest,
    })
}

/// Paints runs in declaration order through the M17 compositor.
///
/// Each run must fit the canvas: the compositor's glyph loop treats
/// coordinates as unsigned, so a negative or overflowing rectangle is refused
/// here rather than painted at the wrong place.
pub fn paint_runs(
    framebuffer: &mut Framebuffer,
    runs: &[GlyphRun],
    assets: &UiAssets,
) -> Result<(), UiCompatError> {
    for run in runs {
        if !run.rect.fits_canvas() {
            return Err(UiCompatError::Typography(RULE_GLYPH_CANVAS));
        }
    }
    let palette = palette_hash(assets.palette_for(PaletteFamily::Hud));
    let nodes: Vec<UiNode> = runs.iter().map(|run| run.ui_node(palette)).collect();
    framebuffer
        .paint_nodes(&nodes, assets)
        .map_err(UiCompatError::from_spec)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui_compat::test_support::{bank, palette, synthetic_fnt};
    use crate::ui_spec::FONT_FILES;

    /// A3 rule 1 — the registry is exactly the five retail `.FNT` files and the
    /// role↔file map is a bijection.
    ///
    /// Violation caught: a sixth font (or a substituted system face), two roles
    /// mapping to the same file, and an unknown name being silently accepted
    /// (which a `parse` returning a default would do).
    #[test]
    fn falsifier_a3_font_registry_is_closed() {
        let mut declared: Vec<&str> = RetailFont::all().iter().map(|font| font.file()).collect();
        declared.sort_unstable();
        let mut expected: Vec<&str> = FONT_FILES.to_vec();
        expected.sort_unstable();
        assert_eq!(declared, expected);

        let mut files: Vec<&str> = RetailFont::all().iter().map(|font| font.file()).collect();
        files.sort_unstable();
        files.dedup();
        assert_eq!(files.len(), 5, "role -> file must be a bijection");

        for font in RetailFont::all() {
            assert_eq!(RetailFont::parse(font.file()), Some(font));
            assert_eq!(RetailFont::parse(font.stem()), Some(font));
        }
        assert_eq!(RetailFont::parse("ARIAL"), None);
        assert_eq!(RetailFont::parse("FONT0005.FNT"), None);
        assert_eq!(RetailFont::parse(""), None);

        // A bank without the role's file is an error, never a default face.
        let sparse = UiAssets::new(palette(1), palette(2), palette(3))
            .with_font("FONT0000.FNT", synthetic_fnt(14, 11, 8));
        assert_eq!(
            glyphs(RetailFont::Default, 127, "AB", &sparse),
            Err(UiCompatError::Typography(RULE_FONT_REGISTRY))
        );
        assert!(glyphs(RetailFont::Large, 127, "AB", &sparse).is_ok());
    }

    /// A3 rule 2 — ink 0 is transparent and 255 is outside the HUD window.
    ///
    /// Violation caught: an invisible run (ink 0 accepted) and an index the
    /// HUD palette cannot resolve (255 is not a `PAL.PAL` index).
    #[test]
    fn falsifier_a3_ink_must_be_visible_and_in_range() {
        let assets = bank();
        assert_eq!(
            glyphs(RetailFont::Default, 0, "AB", &assets),
            Err(UiCompatError::Typography(RULE_INK_TRANSPARENT))
        );
        assert_eq!(
            glyphs(RetailFont::Default, 255, "AB", &assets),
            Err(UiCompatError::Typography(RULE_INK_RANGE))
        );
        let run = glyphs(RetailFont::Default, 127, "AB", &assets).expect("run");
        assert_eq!(run.ink, 127);
        assert!(run.ink_pixels() > 0);
    }

    /// A3 rule 4 — the extent is the `.FNT` advance, not the cell size and not
    /// a constant.
    ///
    /// Violation caught: `width = fixed_width` (cell width) or any hard-coded
    /// width. Two faces with the same cell *height* and different glyph widths
    /// must produce different widths, and each must equal `text_width`.
    #[test]
    fn falsifier_a3_advance_comes_from_the_font() {
        let assets = bank();
        let wide = glyphs(RetailFont::Large, 127, "AB", &assets).expect("wide run");
        let narrow = glyphs(RetailFont::Small, 127, "AB", &assets).expect("narrow run");
        let wide_face = assets.font(RetailFont::Large.file()).expect("wide face");
        let narrow_face = assets.font(RetailFont::Small.file()).expect("narrow face");

        assert_eq!(wide.width, wide_face.text_width("AB"));
        assert_eq!(narrow.width, narrow_face.text_width("AB"));
        assert_ne!(wide.width, narrow.width);
        assert_ne!(wide.width, wide_face.fixed_width);
        assert_eq!(wide.height, wide_face.fixed_height);
        assert_ne!(wide.height, narrow.height);
        assert_eq!(wide.mask.len(), wide.width as usize * wide.height as usize);
        assert_eq!(narrow.mask.len(), narrow.width as usize * narrow.height as usize);

        let again = glyphs(RetailFont::Large, 127, "AB", &assets).expect("again");
        assert_eq!(wide.digest, again.digest);
        assert_ne!(wide.digest, narrow.digest);
        let other_ink = glyphs(RetailFont::Large, 246, "AB", &assets).expect("alert ink");
        assert_ne!(wide.digest, other_ink.digest);

        assert_eq!(
            glyphs(RetailFont::Default, 127, "", &assets),
            Err(UiCompatError::Typography(RULE_GLYPH_EMPTY))
        );
    }

    /// A3 paint — the run reaches the framebuffer with its own ink and only
    /// inside its own rectangle.
    ///
    /// Violation caught: painting the mask with a default ink, painting outside
    /// the rect, or refusing a run that fits.
    #[test]
    fn paint_runs_writes_the_run_ink_only() {
        let assets = bank();
        let run = glyphs(RetailFont::Default, 246, "AB", &assets)
            .expect("run")
            .at(10, 20);
        let mut framebuffer = Framebuffer::new();
        paint_runs(&mut framebuffer, std::slice::from_ref(&run), &assets).expect("paint");
        let mut painted = 0usize;
        for y in 0..200u16 {
            for x in 0..320u16 {
                let index = framebuffer.pixel(x, y);
                if index == 0 {
                    continue;
                }
                painted += 1;
                assert_eq!(index, 246);
                assert!(x >= 10 && x < 10 + run.width && y >= 20 && y < 20 + run.height);
            }
        }
        assert!(painted > 0);
        assert!(crate::ui_spec::palette_compliance(&framebuffer).is_empty());

        let outside = glyphs(RetailFont::Large, 127, "AAAAAAAAAA", &assets)
            .expect("run")
            .at(300, 190);
        assert_eq!(
            paint_runs(&mut Framebuffer::new(), &[outside], &assets),
            Err(UiCompatError::Typography(RULE_GLYPH_CANVAS))
        );
    }
}
