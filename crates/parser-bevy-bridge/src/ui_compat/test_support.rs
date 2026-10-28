//! Test fixtures for the seven-axis falsifiers (`#[cfg(test)]` only).
//!
//! Every fixture is **synthetic**: no byte comes from `ARENA2`, so the
//! falsifiers run on any machine and the retail tree stays read-only. The
//! shapes follow the measured retail ones (a 320×46 HUD strip, a 66×66
//! nine-slice container, 8 644-byte `.FNT` files with 240 glyphs) so a rule
//! that holds here holds on the real bank — the values are synthetic, the
//! geometry is not.

use daggerfall_parser_rs::image::{DecodedImage, FntFile, ImageKind, FNT_FILE_LEN};
use daggerfall_parser_rs::text_rsc::TextRsc;
use daggerfall_parser_rs::texture::Palette;

use crate::ui_spec::UiAssets;

/// Deterministic 256-colour table: `[seed, index, 0]`.
pub(crate) fn palette(seed: u8) -> Palette {
    let mut colors = [[0u8; 3]; 256];
    for (index, color) in colors.iter_mut().enumerate() {
        *color = [seed, index as u8, 0];
    }
    Palette { colors }
}

/// A header-less `IMG`-style image filled with `fill`.
pub(crate) fn image(width: u16, height: u16, fill: u8) -> DecodedImage {
    DecodedImage {
        x_offset: 0,
        y_offset: 0,
        width,
        height,
        compression: 0,
        pixels: vec![fill; width as usize * height as usize],
        embedded_palette: None,
        kind: ImageKind::ImgHeader,
    }
}

/// An `RCI` image (record container) filled with `fill`.
pub(crate) fn rci_image(width: u16, height: u16, fill: u8) -> DecodedImage {
    DecodedImage {
        kind: ImageKind::Rci,
        ..image(width, height, fill)
    }
}

/// A synthetic 8 644-byte `.FNT`: 240 glyphs of `glyph_width`, one ink row.
///
/// `cell_width`/`cell_height` are the face metrics read from bytes 0..4, and
/// `glyph_width` is the per-glyph advance stored in the glyph table — they are
/// separate on purpose, so a test can prove the run width comes from the
/// advance and not from the cell.
pub(crate) fn synthetic_fnt(cell_width: u16, cell_height: u16, glyph_width: u16) -> FntFile {
    let mut bytes = vec![0u8; FNT_FILE_LEN];
    bytes[0..2].copy_from_slice(&cell_width.to_le_bytes());
    bytes[2..4].copy_from_slice(&cell_height.to_le_bytes());
    for index in 0..240usize {
        let bitmap = (964 + index * 32) as u16;
        let entry = 4 + index * 4;
        bytes[entry..entry + 2].copy_from_slice(&bitmap.to_le_bytes());
        bytes[entry + 2..entry + 4].copy_from_slice(&glyph_width.to_le_bytes());
        // One ink row per glyph, so every run has at least one set bit.
        bytes[964 + index * 32 + 1] = 0x80;
    }
    FntFile::parse(&bytes).expect("synthetic fnt")
}

/// The fixture bank: one HUD strip, one mask, one map asset, one nine-slice
/// container and the five retail fonts.
pub(crate) fn bank() -> UiAssets {
    let mut assets = UiAssets::new(palette(10), palette(20), palette(30));
    assets = assets
        .with_image("MAIN00I0.IMG", image(320, 46, 1))
        .expect("main strip");
    assets = assets
        .with_image("MASK99I0.IMG", image(64, 64, 150))
        .expect("mask");
    assets = assets
        .with_image("MAP100I0.IMG", image(320, 200, 20))
        .expect("map");
    assets = assets
        .with_image("SPOP.RCI", rci_image(66, 66, 1))
        .expect("nine-slice container");
    for (name, cell_width, cell_height, glyph_width) in [
        ("FONT0000.FNT", 14u16, 11u16, 8u16),
        ("FONT0001.FNT", 14, 14, 7),
        ("FONT0002.FNT", 5, 5, 4),
        ("FONT0003.FNT", 5, 7, 5),
        ("FONT0004.FNT", 5, 6, 4),
    ] {
        assets = assets.with_font(name, synthetic_fnt(cell_width, cell_height, glyph_width));
    }
    assets
}

/// A synthetic `TEXT.RSC` with three records (`"HELLO"`, `"WORLD"`, empty).
///
/// Retail layout: `u16 header_length` (a multiple of 6), then one
/// `(u16 id, u32 offset)` entry per record, six bytes of gap and the records
/// terminated by `0xFE`.
pub(crate) fn text_rsc() -> (Vec<u8>, TextRsc) {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&24u16.to_le_bytes());
    for (id, offset) in [(0u16, 26u32), (1, 32), (2, 38)] {
        bytes.extend_from_slice(&id.to_le_bytes());
        bytes.extend_from_slice(&offset.to_le_bytes());
    }
    bytes.extend_from_slice(&[0u8; 6]);
    bytes.extend_from_slice(b"HELLO\xFE");
    bytes.extend_from_slice(b"WORLD\xFE");
    bytes.push(0xFE);
    let table = TextRsc::parse(&bytes).expect("synthetic text.rsc");
    (bytes, table)
}
