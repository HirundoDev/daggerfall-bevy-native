//! Images and fonts of the 1996 original UI — `.IMG`, `.CIF`, `.FNT`,
//! `.RCI`/`.GFX`, `PAINT.DAT` and the verifiable projection of `PACKED.DAT`
//! (M17, DoD §h / WI-9 · REQ-L99-050).
//!
//! Spec anchors (UESP *Image formats/IMG · CIF · RCI · Palette*, cited in
//! row 00c REQ-L99-050) + own empirical dump of the user's ARENA2
//! (2026-10-20, Mode A read-only):
//!
//! | Fact | Verification |
//! |---|---|
//! | `.IMG` header 12 B: `XOffset/YOffset/Width/Height i16`, `Compression u16`, `PixelDataLength i16` | 188/263 headers satisfy `12 + len == size` |
//! | `PixelDataLength` overflows as `i16` (`FMAP0I05` = 51 200 → −14 336) | reinterpreted as `u16` |
//! | Actual `Compression` in `.IMG`: `0` (186), `0x0800` (2), `2` (0) | 0 RLE in `.IMG`; `0x0800` → uncompressed |
//! | `.IMG` without header: raw RCI `W×H` (66) or `W×H + 768` with embedded PalFile (6) | [`RCI_DIMENSIONS`] table closes 263/263 |
//! | Empty `.IMG` (12 B of zeros) | 3 (`FMAP0I00/01/16`) |
//! | `.CIF` = list of `ImgFile` up to EOF, without its own palette | 50/70 files |
//! | CIF RLE: `count > 127` → repeat `count − 127`; otherwise copy `count + 1` | RLE frames of `SHOK/FIRE/FRST/MJIC/POIS` |
//! | `WEAPON*.CIF` = root ImgFile + 6 `AnimationHeader` (76 B; `TotalSize` @+74) | 18/70 files; last `TotalSize` closes at EOF |
//! | `FACES.CIF` = RCI 61 × 64×64 | 249 856 / 4 096 = 61 exactly |
//! | `.FNT` = 4 B `(FixedWidth, FixedHeight)` + 240 × 4 B `(offset, width)` + 240 × 32 B | `FONT0000` 8 644 B, 14×11 cell |
//! | Glyph bitmap 1 bpp 16×16: row `y` = `byte[2y+1]` (left 8) + `byte[2y]` (right 8) | glyph 0 rows `0x4000 0xE000 0x6000 0x6000` |
//! | Glyph→ASCII map **undocumented** (NEGATIVE-6) | derived empirically: `index = ASCII − 33` (verified on `!`, `=`, `?`, `@`, `A`, `I`, `J`, `[`, `a`, `z`); declared in the HH-1 |
//! | Default ink | index 127 (`PAL.PAL` = `(44,44,44)`) |
//! | Palettes: `PalFile` 768 B · `ColFile` 776 B (`Length=776`, `major=0xB123`) | `texture::Palette` (M12) |
//! | `PAINT.DAT` = 180 × 40 B | 7 200 B exactly |
//! | `PACKED.DAT` format **unresolved** (NEGATIVE-2) | only verifiable projection: initial 16 `u32` + trailing ASCII directory |
//!
//! **NEGATIVE-2 (declared):** the `PACKED.DAT` compression format has
//! neither a UESP page nor a parser in the DFU corpus. This module does **not**
//! invent a decompressor: [`project_packed`] exposes only what is verifiable
//! (`u32` header and directory tail with the real offsets).
//!
//! **NEGATIVE-6 (declared):** the DFU corpus does not contain the UI code
//! that maps glyph index → character. The implemented mapping
//! ([`FNT_ASCII_FIRST`] = 33, `index = ASCII − 33`) was **derived from usage**
//! by rendering the retail glyphs and comparing them with their shape (0 = `!`,
//! 28 = `=`, 30 = `?`, 31 = `@`, 32 = `A`, 40 = `I`, 41 = `J`, 58 = `[`,
//! 64 = `a`, 89 = `z`). Any glyph outside `33..=272` does not exist.
//!
//! **Custody (INV-L99-6):** 0 retail bytes in git — this module is code +
//! its own synthetic fixtures; the real files are read at runtime from
//! the user's ARENA2 (Mode A). Crate with 0 external dependencies.

use crate::texture::{Palette, TextureError};

/// Length of the `ImgHeader`.
pub const IMG_HEADER_LEN: usize = 12;

/// `Compression = 2` → Daggerfall RLE (UESP *Image formats/IMG*).
pub const RLE_COMPRESSION: u16 = 2;

/// `Compression = 0x0800` observed in retail (`TALK00I0`, `FRAM00I0`):
/// UESP treats it as **uncompressed**.
pub const UNCOMPRESSED_FLAG: u16 = 0x0800;

/// Known RCI dimensions (UESP *Image formats/RCI* §"RCI File Image
/// Dimensions"), verified byte by byte against the 72 `.IMG` files without a header.
pub const RCI_DIMENSIONS: [(u16, u16); 14] = [
    (320, 200),
    (512, 219),
    (320, 215),
    (184, 144),
    (179, 22),
    (107, 40),
    (107, 20),
    (45, 22),
    (43, 40),
    (40, 80),
    (36, 81),
    (9, 80),
    (322, 14),
    (320, 64),
];

/// Length of an embedded `PalFile` after a palettized RCI.
pub const PAL_FILE_LEN: usize = 768;

/// `AnimationHeader` of `WEAPON*.CIF`: 12 B of fields + 31 offsets + `TotalSize`.
pub const ANIMATION_HEADER_LEN: usize = 76;

/// Number of frame offsets in an `AnimationHeader`.
pub const ANIMATION_FRAME_OFFSETS: usize = 31;

/// Side of a `FACES.CIF` RCI frame.
pub const FACES_FRAME_SIDE: usize = 64;

/// Length of the `.FNT` header.
pub const FNT_HEADER_LEN: usize = 4;

/// Glyphs per `.FNT`.
pub const FNT_GLYPH_COUNT: usize = 240;

/// Glyph table entry (`offset u16`, `width u16`).
pub const FNT_ENTRY_LEN: usize = 4;

/// Bitmap bytes per glyph (16 rows × 16 bits).
pub const FNT_GLYPH_BYTES: usize = 32;

/// Exact length of a retail `.FNT` (`4 + 240×4 + 240×32`).
pub const FNT_FILE_LEN: usize = FNT_HEADER_LEN + FNT_GLYPH_COUNT * FNT_ENTRY_LEN + FNT_GLYPH_COUNT * FNT_GLYPH_BYTES;

/// First character with a glyph (`!`): `index = ASCII − 33` (see NEGATIVE-6).
pub const FNT_ASCII_FIRST: u8 = 33;

/// Default ink index (`FntFile.cs:203`, `colorIndex = 127`).
pub const FNT_INK_INDEX: u8 = 127;

/// Space advance (no glyph of its own): declared, 4 classic px.
pub const FNT_SPACE_ADVANCE: u16 = 4;

/// `PAINT.DAT` records.
pub const PAINT_RECORD_COUNT: usize = 180;

/// Length of a `PAINT.DAT` record (`PaintFile.cs:24`).
pub const PAINT_RECORD_LEN: usize = 40;

/// `u32` words of the `PACKED.DAT` header that are projected.
pub const PACKED_HEADER_WORDS: usize = 16;

/// Tail window scanned for the `PACKED.DAT` ASCII directory.
pub const PACKED_TAIL_SCAN: usize = 512;

/// Minimum characters of a `PACKED.DAT` directory name.
pub const PACKED_MIN_NAME: usize = 5;

/// Parse/decode error for UI assets.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImageError {
    /// Buffer shorter than the minimum structure.
    TooShort {
        /// Available bytes.
        len: usize,
        /// Required bytes.
        need: usize,
        /// Structure that was being read.
        what: &'static str,
    },
    /// Header with implausible dimensions/compression.
    BadHeader {
        /// Literal reason.
        reason: &'static str,
    },
    /// `12 + PixelDataLength` does not match the real size.
    LengthMismatch {
        /// Declared size.
        declared: usize,
        /// Real size.
        actual: usize,
    },
    /// `Compression` outside `{0, 2, 0x0800}`.
    UnsupportedCompression {
        /// Observed value.
        compression: u16,
    },
    /// RLE run that exhausts the buffer.
    RleTruncated {
        /// Offset where bytes were missing.
        offset: usize,
    },
    /// RLE run that exceeds the expected pixels.
    RleOverrun {
        /// Pixels produced.
        produced: usize,
        /// Expected pixels.
        expected: usize,
    },
    /// `PixelDataLength` different from `Width × Height`.
    PixelCount {
        /// Declared pixels.
        declared: usize,
        /// Expected pixels.
        expected: usize,
    },
    /// Invalid embedded palette.
    Palette(TextureError),
    /// `.CIF` with unresolved structure (e.g. `WEAPON09.CIF`).
    UnresolvedCif {
        /// Observed size.
        len: usize,
    },
    /// `AnimationHeader` outside the file or with an invalid `TotalSize`.
    BadAnimationHeader {
        /// Header offset.
        offset: usize,
        /// Declared `TotalSize`.
        total_size: u16,
    },
    /// `.FNT` that does not measure exactly [`FNT_FILE_LEN`].
    FntLength {
        /// Observed length.
        len: usize,
    },
    /// Glyph whose bitmap falls outside the file.
    GlyphOutOfRange {
        /// Glyph index.
        index: usize,
        /// Declared offset.
        offset: u16,
    },
    /// `PAINT.DAT` that does not measure `180 × 40`.
    PaintLength {
        /// Observed length.
        len: usize,
    },
    /// `PACKED.DAT` too short for the projection.
    PackedTooShort {
        /// Observed length.
        len: usize,
    },
    /// `.IMG` that is neither a valid header nor a known RCI.
    UnrecognizedImg {
        /// Observed length.
        len: usize,
    },
}

impl std::fmt::Display for ImageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TooShort { len, need, what } => {
                write!(f, "{what}: buffer of {len} B < {need} B required")
            }
            Self::BadHeader { reason } => write!(f, "invalid header: {reason}"),
            Self::LengthMismatch { declared, actual } => write!(
                f,
                "declared size {declared} B ≠ real {actual} B (12 + PixelDataLength)"
            ),
            Self::UnsupportedCompression { compression } => {
                write!(f, "unsupported compression {compression} (0 · 2 · 0x0800)")
            }
            Self::RleTruncated { offset } => write!(f, "RLE truncated at byte {offset}"),
            Self::RleOverrun { produced, expected } => {
                write!(f, "RLE overrun ({produced} > {expected} pixels)")
            }
            Self::PixelCount { declared, expected } => write!(
                f,
                "PixelDataLength {declared} ≠ Width×Height {expected}"
            ),
            Self::Palette(err) => write!(f, "embedded palette: {err}"),
            Self::UnresolvedCif { len } => write!(
                f,
                "CIF of {len} B with unresolved structure (frames · animations · RCI) — NEGATIVE-2 declared"
            ),
            Self::BadAnimationHeader { offset, total_size } => write!(
                f,
                "AnimationHeader @{offset} with TotalSize {total_size} outside the file"
            ),
            Self::FntLength { len } => {
                write!(f, ".FNT of {len} B (expected {FNT_FILE_LEN})")
            }
            Self::GlyphOutOfRange { index, offset } => write!(
                f,
                "glyph {index} with bitmap @{offset} outside the file"
            ),
            Self::PaintLength { len } => write!(
                f,
                "PAINT.DAT of {len} B (expected {} B)",
                PAINT_RECORD_COUNT * PAINT_RECORD_LEN
            ),
            Self::PackedTooShort { len } => write!(f, "PACKED.DAT of {len} B (minimum {PACKED_TAIL_SCAN})"),
            Self::UnrecognizedImg { len } => write!(
                f,
                "IMG of {len} B: neither a valid 12 B header nor a known RCI dimension"
            ),
        }
    }
}

impl std::error::Error for ImageError {}

impl From<TextureError> for ImageError {
    fn from(err: TextureError) -> Self {
        Self::Palette(err)
    }
}

fn u16_le(bytes: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([bytes[at], bytes[at + 1]])
}

fn i16_le(bytes: &[u8], at: usize) -> i16 {
    i16::from_le_bytes([bytes[at], bytes[at + 1]])
}

fn u32_le(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]])
}

/// 12 B header of an `ImgFile` (UESP *Image formats/IMG*).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ImgHeader {
    /// `XOffset` (destination on the 320×200 canvas).
    pub x_offset: i16,
    /// `YOffset`.
    pub y_offset: i16,
    /// `Width`.
    pub width: i16,
    /// `Height`.
    pub height: i16,
    /// `Compression` (`0`, `2` RLE, `0x0800` observed).
    pub compression: u16,
    /// `PixelDataLength` read as `i16` (retail overflows: use
    /// [`Self::pixel_data_len_u16`]).
    pub pixel_data_len: i16,
}

impl ImgHeader {
    /// Parses the 12 B header.
    pub fn parse(bytes: &[u8]) -> Result<Self, ImageError> {
        if bytes.len() < IMG_HEADER_LEN {
            return Err(ImageError::TooShort {
                len: bytes.len(),
                need: IMG_HEADER_LEN,
                what: "ImgHeader",
            });
        }
        Ok(Self {
            x_offset: i16_le(bytes, 0),
            y_offset: i16_le(bytes, 2),
            width: i16_le(bytes, 4),
            height: i16_le(bytes, 6),
            compression: u16_le(bytes, 8),
            pixel_data_len: i16_le(bytes, 10),
        })
    }

    /// Width as an unsigned integer (0 if the `i16` is ≤ 0).
    pub fn width_u16(&self) -> u16 {
        if self.width > 0 {
            self.width as u16
        } else {
            0
        }
    }

    /// Height as an unsigned integer (0 if the `i16` is ≤ 0).
    pub fn height_u16(&self) -> u16 {
        if self.height > 0 {
            self.height as u16
        } else {
            0
        }
    }

    /// `PixelDataLength` reinterpreted as `u16` (retail: the `i16` overflows
    /// above 32 767 — `FMAP0I05` declares −14 336 = 51 200).
    pub fn pixel_data_len_u16(&self) -> u16 {
        self.pixel_data_len as u16
    }

    /// `true` if the header is the "empty IMG" (12 B of zeros).
    pub fn is_empty(&self) -> bool {
        self.x_offset == 0
            && self.y_offset == 0
            && self.width == 0
            && self.height == 0
            && self.compression == 0
            && self.pixel_data_len == 0
    }
}

/// How an `.IMG` file is stored (verified classification).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImgClass {
    /// 12 B `ImgFile` + pixel data (`12 + len == size`).
    Header,
    /// "Empty IMG": 12 B of zeros (3 in retail: `FMAP0I00/01/16`).
    Empty,
    /// Raw RCI (no header) with dimensions from [`RCI_DIMENSIONS`].
    Rci {
        /// Width.
        width: u16,
        /// Height.
        height: u16,
        /// `true` if it carries a 768 B `PalFile` embedded at the end.
        palettized: bool,
    },
}

/// Nature of the container of a decoded image.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageKind {
    /// `ImgFile` with a 12 B header.
    ImgHeader,
    /// "Empty IMG" (0 pixels).
    Empty,
    /// Raw RCI.
    Rci,
    /// RCI with an embedded `PalFile`.
    RciPalettized,
    /// Frame of a `.CIF`.
    CifFrame,
    /// RCI frame of `FACES.CIF`.
    CifRci,
}

/// Image decoded to **palette indices** (row-major).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecodedImage {
    /// `XOffset` (0 for RCI/CIF without a header).
    pub x_offset: i16,
    /// `YOffset`.
    pub y_offset: i16,
    /// Width in pixels.
    pub width: u16,
    /// Height in pixels.
    pub height: u16,
    /// Source compression (`0` in RCI).
    pub compression: u16,
    /// Palette indices (`width × height` bytes).
    pub pixels: Vec<u8>,
    /// Embedded `PalFile` (palettized RCI only).
    pub embedded_palette: Option<Palette>,
    /// Nature of the container.
    pub kind: ImageKind,
}

impl DecodedImage {
    /// Empty image ("empty IMG" case).
    pub fn empty() -> Self {
        Self {
            x_offset: 0,
            y_offset: 0,
            width: 0,
            height: 0,
            compression: 0,
            pixels: Vec::new(),
            embedded_palette: None,
            kind: ImageKind::Empty,
        }
    }

    /// `true` if it has no pixels.
    pub fn is_empty(&self) -> bool {
        self.pixels.is_empty()
    }

    /// Palette index at `(x, y)` (`None` out of range).
    pub fn pixel(&self, x: u16, y: u16) -> Option<u8> {
        if x >= self.width || y >= self.height {
            return None;
        }
        self.pixels.get(y as usize * self.width as usize + x as usize).copied()
    }
}

/// Decodes an `.IMG` with a 12 B header.
///
/// Accepts `Compression ∈ {0, 0x0800}` (uncompressed) and `2` (RLE). Any
/// other value, or an inconsistent length, is a typed error.
pub fn decode_img(bytes: &[u8]) -> Result<DecodedImage, ImageError> {
    let header = ImgHeader::parse(bytes)?;
    if header.is_empty() && bytes.len() == IMG_HEADER_LEN {
        return Ok(DecodedImage::empty());
    }
    let width = header.width_u16();
    let height = header.height_u16();
    if width == 0 || height == 0 {
        return Err(ImageError::BadHeader {
            reason: "Width/Height must be > 0",
        });
    }
    let declared = header.pixel_data_len_u16() as usize;
    let expected_file = IMG_HEADER_LEN + declared;
    if bytes.len() != expected_file {
        return Err(ImageError::LengthMismatch {
            declared: expected_file,
            actual: bytes.len(),
        });
    }
    let expected_pixels = width as usize * height as usize;
    let pixels = match header.compression {
        0 | UNCOMPRESSED_FLAG => {
            if declared != expected_pixels {
                return Err(ImageError::PixelCount {
                    declared,
                    expected: expected_pixels,
                });
            }
            bytes[IMG_HEADER_LEN..IMG_HEADER_LEN + declared].to_vec()
        }
        RLE_COMPRESSION => {
            let (pixels, next) = decode_rle(bytes, IMG_HEADER_LEN, expected_pixels)?;
            if next - IMG_HEADER_LEN != declared {
                return Err(ImageError::PixelCount {
                    declared,
                    expected: next - IMG_HEADER_LEN,
                });
            }
            pixels
        }
        other => return Err(ImageError::UnsupportedCompression { compression: other }),
    };
    Ok(DecodedImage {
        x_offset: header.x_offset,
        y_offset: header.y_offset,
        width,
        height,
        compression: header.compression,
        pixels,
        embedded_palette: None,
        kind: ImageKind::ImgHeader,
    })
}

/// Daggerfall RLE (UESP *Image formats/IMG* §Compression):
/// `count > 127` → repeat the next byte `count − 127` times; otherwise
/// copy the next `count + 1` bytes. Up to `pixel_count` pixels.
///
/// Returns `(pixels, next offset)`.
pub fn decode_rle(
    bytes: &[u8],
    offset: usize,
    pixel_count: usize,
) -> Result<(Vec<u8>, usize), ImageError> {
    let mut out: Vec<u8> = Vec::with_capacity(pixel_count);
    let mut at = offset;
    while out.len() < pixel_count {
        if at >= bytes.len() {
            return Err(ImageError::RleTruncated { offset: at });
        }
        let count = bytes[at];
        at += 1;
        if count > 127 {
            if at >= bytes.len() {
                return Err(ImageError::RleTruncated { offset: at });
            }
            let value = bytes[at];
            at += 1;
            let repeat = count as usize - 127;
            if out.len() + repeat > pixel_count {
                return Err(ImageError::RleOverrun {
                    produced: out.len() + repeat,
                    expected: pixel_count,
                });
            }
            out.extend(std::iter::repeat_n(value, repeat));
        } else {
            let copy = count as usize + 1;
            if at + copy > bytes.len() {
                return Err(ImageError::RleTruncated { offset: at });
            }
            if out.len() + copy > pixel_count {
                return Err(ImageError::RleOverrun {
                    produced: out.len() + copy,
                    expected: pixel_count,
                });
            }
            out.extend_from_slice(&bytes[at..at + copy]);
            at += copy;
        }
    }
    Ok((out, at))
}

/// Classifies an `.IMG` file (valid header · empty · known RCI).
pub fn classify_img(bytes: &[u8]) -> Result<ImgClass, ImageError> {
    if bytes.len() == IMG_HEADER_LEN {
        let header = ImgHeader::parse(bytes)?;
        if header.is_empty() {
            return Ok(ImgClass::Empty);
        }
    }
    if let Ok(header) = ImgHeader::parse(bytes) {
        let width = header.width_u16();
        let height = header.height_u16();
        let declared = header.pixel_data_len_u16() as usize;
        let compression_ok =
            header.compression == 0 || header.compression == RLE_COMPRESSION || header.compression == UNCOMPRESSED_FLAG;
        if width > 0
            && height > 0
            && compression_ok
            && IMG_HEADER_LEN + declared == bytes.len()
            && (header.compression == RLE_COMPRESSION
                || declared == width as usize * height as usize)
        {
            return Ok(ImgClass::Header);
        }
    }
    for &(width, height) in &RCI_DIMENSIONS {
        let raw = width as usize * height as usize;
        if bytes.len() == raw {
            return Ok(ImgClass::Rci {
                width,
                height,
                palettized: false,
            });
        }
        if bytes.len() == raw + PAL_FILE_LEN {
            return Ok(ImgClass::Rci {
                width,
                height,
                palettized: true,
            });
        }
    }
    Err(ImageError::UnrecognizedImg { len: bytes.len() })
}

/// Decodes a raw RCI (optionally with a 768 B `PalFile` at the end).
pub fn decode_rci(
    bytes: &[u8],
    width: u16,
    height: u16,
    palettized: bool,
) -> Result<DecodedImage, ImageError> {
    let raw = width as usize * height as usize;
    let expected = raw + if palettized { PAL_FILE_LEN } else { 0 };
    if bytes.len() != expected {
        return Err(ImageError::LengthMismatch {
            declared: expected,
            actual: bytes.len(),
        });
    }
    let (embedded_palette, kind) = if palettized {
        (
            Some(Palette::parse(&bytes[raw..])?),
            ImageKind::RciPalettized,
        )
    } else {
        (None, ImageKind::Rci)
    };
    Ok(DecodedImage {
        x_offset: 0,
        y_offset: 0,
        width,
        height,
        compression: 0,
        pixels: bytes[..raw].to_vec(),
        embedded_palette,
        kind,
    })
}

/// Materializes a pure RCI container as **one vertical atlas**
/// (`width × height·frames`), the form the surface axis selects records from.
/// The bytes are the container's own, in file order (no interpolation, no
/// re-layout): `FACES.CIF` is 61 records of 64×64 = 249 856 B.
/// `None` when the layout does not tile exactly or the atlas does not fit a
/// `DecodedImage` (`height · frames > u16::MAX`).
pub fn rci_atlas(bytes: &[u8], width: u16, height: u16, frames: usize) -> Option<DecodedImage> {
    if width == 0 || height == 0 || frames == 0 {
        return None;
    }
    let cell = width as usize * height as usize;
    if bytes.len() != cell * frames {
        return None;
    }
    let total = height.checked_mul(u16::try_from(frames).ok()?)?;
    Some(DecodedImage {
        x_offset: 0,
        y_offset: 0,
        width,
        height: total,
        compression: 0,
        pixels: bytes.to_vec(),
        embedded_palette: None,
        kind: ImageKind::Rci,
    })
}

/// Decodes any `.IMG` (valid header · empty · RCI).
pub fn decode_img_file(bytes: &[u8]) -> Result<DecodedImage, ImageError> {
    match classify_img(bytes)? {
        ImgClass::Empty => Ok(DecodedImage::empty()),
        ImgClass::Header => decode_img(bytes),
        ImgClass::Rci {
            width,
            height,
            palettized,
        } => decode_rci(bytes, width, height, palettized),
    }
}

/// `AnimationHeader` of `WEAPON*.CIF` (76 B, `TotalSize` @+74).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnimationHeader {
    /// Absolute offset of the header in the file.
    pub offset: usize,
    /// Animation `Width`.
    pub width: u16,
    /// Animation `Height`.
    pub height: u16,
    /// `LastFrameWidth` (observed = `Width`; **not** the root ImgFile one — D3).
    pub last_frame_width: u16,
    /// `XOffset`.
    pub x_offset: u16,
    /// `LastFrameYOffset`.
    pub last_frame_y_offset: u16,
    /// `DataLength` field (constant 1 288 in retail — **not** usable; D4).
    pub data_length: u16,
    /// Frame offsets (`31 × u16`).
    pub frame_offsets: [u16; ANIMATION_FRAME_OFFSETS],
    /// Real total size of the block (bytes 74-75).
    pub total_size: u16,
}

impl AnimationHeader {
    /// Parses an `AnimationHeader` at `offset`.
    pub fn parse(bytes: &[u8], offset: usize) -> Result<Self, ImageError> {
        if offset + ANIMATION_HEADER_LEN > bytes.len() {
            return Err(ImageError::TooShort {
                len: bytes.len(),
                need: offset + ANIMATION_HEADER_LEN,
                what: "AnimationHeader",
            });
        }
        let mut frame_offsets = [0u16; ANIMATION_FRAME_OFFSETS];
        for (i, slot) in frame_offsets.iter_mut().enumerate() {
            *slot = u16_le(bytes, offset + 12 + i * 2);
        }
        let header = Self {
            offset,
            width: u16_le(bytes, offset),
            height: u16_le(bytes, offset + 2),
            last_frame_width: u16_le(bytes, offset + 4),
            x_offset: u16_le(bytes, offset + 6),
            last_frame_y_offset: u16_le(bytes, offset + 8),
            data_length: u16_le(bytes, offset + 10),
            frame_offsets,
            total_size: u16_le(bytes, offset + 74),
        };
        if header.width == 0
            || header.height == 0
            || header.total_size < ANIMATION_HEADER_LEN as u16
            || offset + header.total_size as usize > bytes.len()
        {
            return Err(ImageError::BadAnimationHeader {
                offset,
                total_size: header.total_size,
            });
        }
        Ok(header)
    }
}

/// Walks `AnimationHeader`s chained by `TotalSize` up to EOF.
pub fn walk_animations(bytes: &[u8], start: usize) -> Result<Vec<AnimationHeader>, ImageError> {
    let mut out = Vec::new();
    let mut at = start;
    while at < bytes.len() {
        let header = AnimationHeader::parse(bytes, at)?;
        at += header.total_size as usize;
        out.push(header);
    }
    Ok(out)
}

/// Content of a `.CIF` (one of the three verified layouts).
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(clippy::large_enum_variant)] // parse result, not a hot path
pub enum CifContent {
    /// Chain of `ImgFile` up to EOF (50/70 retail files).
    Frames(Vec<DecodedImage>),
    /// Root `ImgFile` + weapon animations (18/70).
    Animations {
        /// Root ImgFile.
        root: DecodedImage,
        /// Chained animations.
        animations: Vec<AnimationHeader>,
    },
    /// Pure RCI (`FACES.CIF`: 61 × 64×64).
    Rci {
        /// Frame side.
        width: u16,
        /// Frame height.
        height: u16,
        /// Number of frames.
        frames: usize,
    },
}

/// Decoded `.CIF` (structure + content).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CifFile {
    /// File bytes.
    pub len: usize,
    /// Classified content.
    pub content: CifContent,
}

impl CifFile {
    /// Number of ImgFile frames (0 for animations and RCI).
    pub fn frame_count(&self) -> usize {
        match &self.content {
            CifContent::Frames(frames) => frames.len(),
            _ => 0,
        }
    }
}

fn cif_frame_chain(bytes: &[u8]) -> Option<Vec<DecodedImage>> {
    let mut frames: Vec<DecodedImage> = Vec::new();
    let mut at = 0usize;
    while at < bytes.len() {
        if at + IMG_HEADER_LEN > bytes.len() {
            return None;
        }
        let header = ImgHeader::parse(&bytes[at..]).ok()?;
        let width = header.width_u16();
        let height = header.height_u16();
        let declared = header.pixel_data_len_u16() as usize;
        let compression_ok = header.compression == 0
            || header.compression == RLE_COMPRESSION
            || header.compression == UNCOMPRESSED_FLAG;
        if width == 0 || height == 0 || !compression_ok {
            return None;
        }
        let expected = width as usize * height as usize;
        let (pixels, next) = match header.compression {
            RLE_COMPRESSION => {
                // In RLE the `PixelDataLength` field stores the **encoded**
                // bytes (SHOK f0: 1 500), not `W×H`; the decoder is validated
                // to consume exactly those bytes.
                let (pixels, next) = decode_rle(bytes, at + IMG_HEADER_LEN, expected).ok()?;
                if next - (at + IMG_HEADER_LEN) != declared {
                    return None;
                }
                (pixels, next)
            }
            _ => {
                if declared != expected {
                    return None;
                }
                let end = at + IMG_HEADER_LEN + expected;
                if end > bytes.len() {
                    return None;
                }
                (bytes[at + IMG_HEADER_LEN..end].to_vec(), end)
            }
        };
        if next > bytes.len() {
            return None;
        }
        frames.push(DecodedImage {
            x_offset: header.x_offset,
            y_offset: header.y_offset,
            width,
            height,
            compression: header.compression,
            pixels,
            embedded_palette: None,
            kind: ImageKind::CifFrame,
        });
        at = next;
    }
    if frames.is_empty() {
        None
    } else {
        Some(frames)
    }
}

fn cif_animations(bytes: &[u8]) -> Option<(DecodedImage, Vec<AnimationHeader>)> {
    let header = ImgHeader::parse(bytes).ok()?;
    if !(header.compression == 0 || header.compression == UNCOMPRESSED_FLAG) {
        return None;
    }
    let width = header.width_u16();
    let height = header.height_u16();
    let declared = header.pixel_data_len_u16() as usize;
    let expected = width as usize * height as usize;
    if width == 0 || height == 0 || declared != expected {
        return None;
    }
    let root_end = IMG_HEADER_LEN + expected;
    if root_end > bytes.len() {
        return None;
    }
    let root = DecodedImage {
        x_offset: header.x_offset,
        y_offset: header.y_offset,
        width,
        height,
        compression: header.compression,
        pixels: bytes[IMG_HEADER_LEN..root_end].to_vec(),
        embedded_palette: None,
        kind: ImageKind::CifFrame,
    };
    let animations = walk_animations(bytes, root_end).ok()?;
    if animations.is_empty() {
        None
    } else {
        Some((root, animations))
    }
}

/// Decodes a `.CIF`: chain of `ImgFile` → weapon animations → RCI.
///
/// A file with an unresolved structure (retail: `WEAPON09.CIF`, 61 951 B)
/// returns [`ImageError::UnresolvedCif`] — a layout is never guessed.
pub fn decode_cif(bytes: &[u8]) -> Result<CifFile, ImageError> {
    if let Some(frames) = cif_frame_chain(bytes) {
        return Ok(CifFile {
            len: bytes.len(),
            content: CifContent::Frames(frames),
        });
    }
    if let Some((root, animations)) = cif_animations(bytes) {
        return Ok(CifFile {
            len: bytes.len(),
            content: CifContent::Animations { root, animations },
        });
    }
    let frame_bytes = FACES_FRAME_SIDE * FACES_FRAME_SIDE;
    if bytes.len() >= frame_bytes && bytes.len().is_multiple_of(frame_bytes) {
        return Ok(CifFile {
            len: bytes.len(),
            content: CifContent::Rci {
                width: FACES_FRAME_SIDE as u16,
                height: FACES_FRAME_SIDE as u16,
                frames: bytes.len() / frame_bytes,
            },
        });
    }
    Err(ImageError::UnresolvedCif { len: bytes.len() })
}

/// Glyph of a `.FNT`: width + 1 bpp 16×16 bitmap.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Glyph {
    /// Bitmap offset in the file.
    pub offset: u16,
    /// Declared width (3..=14 in `FONT0000`).
    pub width: u16,
    /// Packed rows: bit 15 = pixel 0 (left).
    pub rows: [u16; 16],
}

impl Glyph {
    /// Bitmap pixel (0 = transparent).
    pub fn pixel(&self, x: u16, y: u16) -> bool {
        if x >= 16 || y >= 16 {
            return false;
        }
        let row = self.rows[y as usize];
        (row >> (15 - x)) & 1 == 1
    }
}

/// `.FNT` font of 240 glyphs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FntFile {
    /// Header `FixedWidth`.
    pub fixed_width: u16,
    /// Header `FixedHeight`.
    pub fixed_height: u16,
    /// Glyphs in index order.
    pub glyphs: Vec<Glyph>,
}

impl FntFile {
    /// Parses a retail `.FNT` (exactly `4 + 960 + 7 680` B).
    pub fn parse(bytes: &[u8]) -> Result<Self, ImageError> {
        if bytes.len() != FNT_FILE_LEN {
            return Err(ImageError::FntLength { len: bytes.len() });
        }
        let fixed_width = u16_le(bytes, 0);
        let fixed_height = u16_le(bytes, 2);
        let mut glyphs = Vec::with_capacity(FNT_GLYPH_COUNT);
        for index in 0..FNT_GLYPH_COUNT {
            let entry = FNT_HEADER_LEN + index * FNT_ENTRY_LEN;
            let offset = u16_le(bytes, entry);
            let width = u16_le(bytes, entry + 2);
            let start = offset as usize;
            if start + FNT_GLYPH_BYTES > bytes.len() {
                return Err(ImageError::GlyphOutOfRange { index, offset });
            }
            let mut rows = [0u16; 16];
            for (y, row) in rows.iter_mut().enumerate() {
                let left = bytes[start + 2 * y + 1] as u16;
                let right = bytes[start + 2 * y] as u16;
                *row = (left << 8) | right;
            }
            glyphs.push(Glyph { offset, width, rows });
        }
        Ok(Self {
            fixed_width,
            fixed_height,
            glyphs,
        })
    }

    /// Glyph index of an ASCII byte (`ASCII − 33`, see NEGATIVE-6).
    pub fn ascii_index(code: u8) -> Option<usize> {
        if code < FNT_ASCII_FIRST {
            return None;
        }
        let index = (code - FNT_ASCII_FIRST) as usize;
        (index < FNT_GLYPH_COUNT).then_some(index)
    }

    /// Glyph of a character (`None` for space and out of range).
    pub fn glyph_for_char(&self, ch: char) -> Option<&Glyph> {
        let code = u8::try_from(ch as u32).ok()?;
        let index = Self::ascii_index(code)?;
        self.glyphs.get(index)
    }

    /// Width of a string (advance per glyph = `width + 1`, space = 4).
    pub fn text_width(&self, text: &str) -> u16 {
        let mut width = 0u16;
        let mut glyphs = 0usize;
        for ch in text.chars() {
            if ch == ' ' {
                width = width.saturating_add(FNT_SPACE_ADVANCE);
                glyphs += 1;
            } else if let Some(glyph) = self.glyph_for_char(ch) {
                width = width.saturating_add(glyph.width + 1);
                glyphs += 1;
            }
        }
        if glyphs > 0 {
            width.saturating_sub(1)
        } else {
            0
        }
    }

    /// Renders a string to a 1 bpp mask (no antialiasing).
    pub fn render_text(&self, text: &str) -> TextMask {
        let width = self.text_width(text);
        let height = self.fixed_height;
        let mut bits = vec![0u8; width as usize * height as usize];
        let mut pen_x = 0u16;
        for ch in text.chars() {
            if ch == ' ' {
                pen_x = pen_x.saturating_add(FNT_SPACE_ADVANCE);
                continue;
            }
            let Some(glyph) = self.glyph_for_char(ch) else {
                continue;
            };
            for y in 0..height.min(16) {
                for x in 0..glyph.width.min(16) {
                    if !glyph.pixel(x, y) {
                        continue;
                    }
                    let px = pen_x + x;
                    if px >= width {
                        continue;
                    }
                    bits[y as usize * width as usize + px as usize] = 1;
                }
            }
            pen_x = pen_x.saturating_add(glyph.width + 1);
        }
        TextMask {
            width,
            height,
            bits,
        }
    }
}

/// 1 bpp text mask (0 = background, 1 = ink).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextMask {
    /// Width in pixels.
    pub width: u16,
    /// Height in pixels.
    pub height: u16,
    /// Row-major bits (1 = ink).
    pub bits: Vec<u8>,
}

impl TextMask {
    /// Mask pixel.
    pub fn pixel(&self, x: u16, y: u16) -> bool {
        if x >= self.width || y >= self.height {
            return false;
        }
        self.bits[y as usize * self.width as usize + x as usize] != 0
    }

    /// Number of ink pixels.
    pub fn ink_pixels(&self) -> usize {
        self.bits.iter().filter(|bit| **bit != 0).count()
    }
}

/// `PAINT.DAT`: 180 raw records of 40 B (undocumented semantics —
/// NEGATIVE-7; the layout is preserved, not interpreted).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaintFile {
    /// Records in file order.
    pub records: Vec<[u8; PAINT_RECORD_LEN]>,
}

impl PaintFile {
    /// Parses `PAINT.DAT` (exactly 7 200 B: 180 × 40).
    pub fn parse(bytes: &[u8]) -> Result<Self, ImageError> {
        if bytes.len() != PAINT_RECORD_COUNT * PAINT_RECORD_LEN {
            return Err(ImageError::PaintLength { len: bytes.len() });
        }
        let mut records = Vec::with_capacity(PAINT_RECORD_COUNT);
        for chunk in bytes.chunks_exact(PAINT_RECORD_LEN) {
            let mut record = [0u8; PAINT_RECORD_LEN];
            record.copy_from_slice(chunk);
            records.push(record);
        }
        Ok(Self { records })
    }

    /// Number of records.
    pub fn len(&self) -> usize {
        self.records.len()
    }

    /// `true` if there are no records (impossible in retail, symmetric API).
    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }

    /// Bytes different from `0xFF` (dominant padding; useful as a footprint).
    pub fn non_padding_bytes(&self) -> usize {
        self.records
            .iter()
            .flat_map(|record| record.iter())
            .filter(|byte| **byte != 0xFF)
            .count()
    }
}

/// Entry of the `PACKED.DAT` trailing ASCII directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackedDirEntry {
    /// Absolute offset of the name.
    pub offset: usize,
    /// ASCII name (uppercase, digits, `.`, `_`).
    pub name: String,
}

/// **Verifiable** projection of `PACKED.DAT` (compression format
/// unresolved — NEGATIVE-2: no decompressor is invented).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackedProjection {
    /// File length.
    pub len: usize,
    /// First [`PACKED_HEADER_WORDS`] `u32` words.
    pub header_u32s: Vec<u32>,
    /// Names of the trailing ASCII directory.
    pub directory: Vec<PackedDirEntry>,
}

fn is_packed_name_byte(byte: u8) -> bool {
    byte.is_ascii_uppercase() || byte.is_ascii_digit() || byte == b'.' || byte == b'_'
}

/// Projects `PACKED.DAT`: `u32` header + trailing ASCII directory.
///
/// In retail (10 436 188 B) the trailing directory contains
/// `ARCH3D.BSA`@10 436 086, `DAGGER.SND`@10 436 111 and `ARENA2`@10 436 128.
pub fn project_packed(bytes: &[u8]) -> Result<PackedProjection, ImageError> {
    if bytes.len() < PACKED_TAIL_SCAN {
        return Err(ImageError::PackedTooShort { len: bytes.len() });
    }
    let words = PACKED_HEADER_WORDS.min(bytes.len() / 4);
    let header_u32s = (0..words).map(|i| u32_le(bytes, i * 4)).collect();
    let start = bytes.len() - PACKED_TAIL_SCAN;
    let mut directory = Vec::new();
    let mut at = start;
    while at < bytes.len() {
        if is_packed_name_byte(bytes[at]) {
            let mut end = at;
            while end < bytes.len() && is_packed_name_byte(bytes[end]) {
                end += 1;
            }
            if end - at >= PACKED_MIN_NAME {
                if let Ok(name) = std::str::from_utf8(&bytes[at..end]) {
                    directory.push(PackedDirEntry {
                        offset: at,
                        name: name.to_string(),
                    });
                }
            }
            at = end;
        } else {
            at += 1;
        }
    }
    Ok(PackedProjection {
        len: bytes.len(),
        header_u32s,
        directory,
    })
}

/// Palette family of a UI asset (**per-family** index map,
/// C2: a single global mapping breaks fidelity).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaletteFamily {
    /// HUD / inventory / scrolls: `PAL.PAL` (0..=254).
    Hud,
    /// Maps: `FMAP_PAL.COL` / `MAP.PAL` (63..=254).
    Map,
    /// 3D world: `ART_PAL.COL` (0..=255).
    Art,
    /// Own palette embedded in the asset (`TITL00I0` only).
    Embedded,
}

/// Index range of the family (per the retail histogram, §2.8).
pub fn family_index_range(family: PaletteFamily) -> std::ops::RangeInclusive<u8> {
    match family {
        PaletteFamily::Hud => 0..=254,
        PaletteFamily::Map => 63..=254,
        PaletteFamily::Art | PaletteFamily::Embedded => 0..=255,
    }
}

/// Palette family from the retail file name.
pub fn family_for_asset(name: &str) -> PaletteFamily {
    let stem = std::path::Path::new(name)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or(name);
    let upper = stem.to_ascii_uppercase();
    if upper.starts_with("TITL") {
        return PaletteFamily::Embedded;
    }
    if upper.starts_with("ART") || upper.starts_with("TEXTURE") {
        return PaletteFamily::Art;
    }
    if upper.starts_with("FMAP")
        || upper.starts_with("AMAP")
        || upper.starts_with("TMAP")
        || upper.starts_with("TAMR")
        || upper.starts_with("NITE")
        || upper.starts_with("DANK")
        || upper == "MAP1"
        || upper.starts_with("MAP")
    {
        return PaletteFamily::Map;
    }
    PaletteFamily::Hud
}

/// **Asset-specific** index range within its family
/// (masks 144..=158 · bodies 0..=127 — §2.8).
pub fn asset_index_range(name: &str) -> std::ops::RangeInclusive<u8> {
    let stem = std::path::Path::new(name)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or(name);
    let upper = stem.to_ascii_uppercase();
    if upper.starts_with("MASK") {
        return 144..=158;
    }
    if upper.starts_with("BODY") {
        return 0..=127;
    }
    family_index_range(family_for_asset(name))
}

/// **Measured per-asset index-range derivations** (M18 B3-c, M39 A2).
///
/// A handful of retail assets paint palette indices **outside** the window of
/// [`family_index_range`]: most below its floor, `PICK03I0` above its ceiling.
/// Every row was derived by measuring the asset's own bytes
/// (`observed_index_range`), never by widening the family rule:
///
/// | Asset | Family rule | Bytes | Span of the non-zero indices | Measurement |
/// |:---|:---|:---|---:|:---|
/// | `MAP100I0.IMG` | `Map` `63..=254` | 64 000 B (raw 320×200 `RCI`, no `ImgFile` header) | `16..=254` | 8 475 painted px `< 63` |
/// | `TRAV0I00.IMG` | `Map` `63..=254` | 64 000 B (raw 320×200 `RCI`, no `ImgFile` header) | `33..=254` | 2 628 painted px `< 63` |
/// | `PICK03I0.IMG` | `Hud` `0..=254` | 64 768 B (raw 320×200 `RCI` + 768 B `PalFile`) | `1..=255` | 49 px at index 255 (x∈[4,314], y∈[114,162]); `PalFile[255]` = `(16,30,25)` |
///
/// (`MAP100I0` also carries 4 624 transparent pixels — index 0 — so a count
/// `≤ 62` that includes them reads 13 099; only the 8 475 painted ones matter.
/// `PICK03I0` carries 145 transparent pixels and 20 535 painted `< 63`, so the
/// same count including index 0 reads 20 680.)
///
/// **Why the family rule does not cover them:** `PaletteFamily::Map` is
/// `63..=254` — the span measured on the `FMAP*` region maps, which is where
/// the rule comes from. The travel screen is a different asset family: its
/// bytes prove it legally uses the low indices, so the compositor resolves
/// **that asset's** range from that asset. The derivation is a table entry
/// with the measurement that justifies it, so the gate can never relax
/// silently: an asset that is not listed here and paints outside its declared
/// range still raises `PaletteViolation`.
///
/// `PICK03I0` fails at the **other** edge: `PaletteFamily::Hud` stops at 254
/// because that is where the `PAL.PAL` histogram of the panel set stops, and
/// the pick screen's bytes prove it legally writes the palette's last index
/// (`255`, inside the 256-entry `PalFile`, whose `PAL.PAL` colour `(16,30,25)`
/// is a real dark green and not a sentinel). The ceiling — not the palette —
/// was the wrong part of the rule for that asset, and the row records exactly
/// the span the bytes show (`1..=255`), so a `Hud` asset without a measurement
/// that paints 255 is still a violation.
///
/// The palette family of the `Map` rows stays `Map` (`FMAP_PAL.COL`): the
/// measured colours of the indices they use agree with `PAL.PAL` (the two
/// palettes share those entries), so the index range — not the palette — was
/// the wrong part of the rule. `PICK03I0` keeps `Hud` (`PAL.PAL`), which is the
/// palette it actually uses.
///
/// **The `MASK` prefix rule is likewise per asset, not per family.** It comes
/// from the paper-doll cut-outs, and it holds for exactly one retail file:
///
/// | Asset | Bytes | Span of the non-zero indices | Painted px inside `144..=158` |
/// |:---|---:|---:|---:|
/// | `MASK01I0.IMG` | 3 200 B | `144..=158` | 3 200 / 3 200 |
/// | `MASK00I0.IMG` | 64 000 B (raw 320×200 `RCI`) | `12..=254` | 609 / 64 000 |
/// | `MASK02I0.IMG` | 64 000 B (raw 320×200 `RCI`) | `12..=246` | 845 / 64 000 |
/// | `MASK04I0.IMG` | 3 200 B | `36..=247` | 2 190 / 3 200 |
/// | `MASK05I0.IMG` | 64 000 B (raw 320×200 `RCI`) | `12..=218` | 0 / 64 000 |
/// | `MASK06I0.IMG` | 64 000 B (raw 320×200 `RCI`) | `1..=254` | 120 / 63 995 |
///
/// The other five are menu backgrounds of the `Hud` family (`0..=254`), so the
/// prefix rule would reject their own bytes. Only the two assets the M39
/// contract paints as full screens get an entry here; the prefix rule keeps
/// governing every unlisted `MASK*` asset, and widening it beyond the
/// measurement would be exactly the silent relaxation this table forbids.
pub const ASSET_RANGE_DERIVATIONS: [(&str, u8, u8, &str); 5] = [
    (
        "MAP100I0",
        16,
        254,
        "raw 320x200 RCI (64000 B, no ImgFile header): non-zero span 16..=254, 8475 painted px < 63 (measured 2026-10-23)",
    ),
    (
        "TRAV0I00",
        33,
        254,
        "raw 320x200 RCI (64000 B, no ImgFile header): non-zero span 33..=254, 2628 painted px < 63 (measured 2026-10-23)",
    ),
    (
        "MASK00I0",
        12,
        254,
        "raw 320x200 RCI (64000 B, no ImgFile header): non-zero span 12..=254, 62685 painted px < 144 (measured 2026-10-27); menu background of the Hud family (0..=254), not a paper-doll cut-out",
    ),
    (
        "MASK05I0",
        12,
        218,
        "raw 320x200 RCI (64000 B, no ImgFile header): non-zero span 12..=218, 63971 painted px < 144 (measured 2026-10-27); menu background of the Hud family (0..=254), not a paper-doll cut-out",
    ),
    (
        "PICK03I0",
        1,
        255,
        "RCI 320x200 palettized (64000 B pixels + 768 B PalFile): non-zero span 1..=255, 49 px at index 255 (x 4..=314, y 114..=162) and PalFile[255]=(16,30,25) is a real colour, not a sentinel; 145 transparent px and 20535 painted px < 63 (measured 2026-10-27)",
    ),
];

/// Stem of a retail asset name (`MAP100I0.IMG` ⇒ `MAP100I0`, uppercase).
fn asset_stem(name: &str) -> String {
    std::path::Path::new(name)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or(name)
        .to_ascii_uppercase()
}

/// Range derived from the asset itself, when the asset has a recorded
/// measurement in [`ASSET_RANGE_DERIVATIONS`] (`None` ⇒ the declared
/// `asset_index_range` is authoritative).
pub fn asset_index_range_derived(name: &str) -> Option<std::ops::RangeInclusive<u8>> {
    let stem = asset_stem(name);
    ASSET_RANGE_DERIVATIONS
        .iter()
        .find(|(asset, _, _, _)| *asset == stem)
        .map(|(_, low, high, _)| *low..=*high)
}

/// **Range the compositor enforces for an asset**: the recorded derivation if
/// the asset has one, else the declared [`asset_index_range`].
///
/// This is the single entry point of the per-asset rule (M18 B3-c): a new
/// asset that paints outside its declared range does **not** inherit another
/// asset's derivation, it violates the gate.
pub fn asset_index_range_resolved(name: &str) -> std::ops::RangeInclusive<u8> {
    asset_index_range_derived(name).unwrap_or_else(|| asset_index_range(name))
}

/// Span of the **non-zero** palette indices of a decoded image (`None` when
/// every pixel is 0, i.e. the asset paints nothing).
///
/// Index 0 is the transparent colour, so it never belongs to the painted span.
pub fn observed_index_range(image: &DecodedImage) -> Option<std::ops::RangeInclusive<u8>> {
    let mut low: Option<u8> = None;
    let mut high: Option<u8> = None;
    for index in image.pixels.iter().copied().filter(|index| *index != 0) {
        low = Some(low.map_or(index, |current: u8| current.min(index)));
        high = Some(high.map_or(index, |current: u8| current.max(index)));
    }
    Some(low?..=high?)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// OWN fixture (0 retail bytes): 3×2 uncompressed ImgFile.
    fn synthetic_img() -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&7i16.to_le_bytes());
        bytes.extend_from_slice(&9i16.to_le_bytes());
        bytes.extend_from_slice(&3i16.to_le_bytes());
        bytes.extend_from_slice(&2i16.to_le_bytes());
        bytes.extend_from_slice(&0u16.to_le_bytes());
        bytes.extend_from_slice(&6i16.to_le_bytes());
        bytes.extend_from_slice(&[1, 2, 3, 4, 5, 6]);
        bytes
    }

    #[test]
    fn img_header_roundtrip() {
        let image = decode_img(&synthetic_img()).expect("valid img");
        assert_eq!((image.x_offset, image.y_offset), (7, 9));
        assert_eq!((image.width, image.height), (3, 2));
        assert_eq!(image.pixels, vec![1, 2, 3, 4, 5, 6]);
        assert_eq!(image.pixel(2, 1), Some(6));
        assert_eq!(image.pixel(3, 1), None);
    }

    #[test]
    fn img_empty_header_is_empty_image() {
        let image = decode_img(&[0u8; 12]).expect("empty IMG");
        assert!(image.is_empty());
        assert_eq!(image.kind, ImageKind::Empty);
    }

    #[test]
    fn img_length_mismatch_is_typed_error() {
        let mut bytes = synthetic_img();
        bytes.push(0);
        assert!(matches!(
            decode_img(&bytes),
            Err(ImageError::LengthMismatch { .. })
        ));
    }

    #[test]
    fn img_rle_header_decodes_encoded_length() {
        // OWN fixture: 2×2 with `PixelDataLength` = encoded bytes (5).
        let mut fixed = Vec::new();
        fixed.extend_from_slice(&0i16.to_le_bytes());
        fixed.extend_from_slice(&0i16.to_le_bytes());
        fixed.extend_from_slice(&2i16.to_le_bytes());
        fixed.extend_from_slice(&2i16.to_le_bytes());
        fixed.extend_from_slice(&RLE_COMPRESSION.to_le_bytes());
        fixed.extend_from_slice(&5i16.to_le_bytes());
        fixed.extend_from_slice(&[129, 9, 1, 4, 5]);
        let image = decode_img(&fixed).expect("IMG RLE");
        assert_eq!(image.pixels, vec![9, 9, 4, 5]);
        assert_eq!(classify_img(&fixed), Ok(ImgClass::Header));
    }

    #[test]
    fn rle_expands_runs_and_literals() {
        // 130 → repeat 0x07 three times; 1 → copy 2 bytes 0x08,0x09.
        let data = [130u8, 7, 1, 8, 9];
        let (pixels, next) = decode_rle(&data, 0, 5).expect("valid RLE");
        assert_eq!(pixels, vec![7, 7, 7, 8, 9]);
        assert_eq!(next, 5);
    }

    #[test]
    fn rle_truncated_is_typed_error() {
        assert!(matches!(
            decode_rle(&[130u8], 0, 3),
            Err(ImageError::RleTruncated { .. })
        ));
    }

    #[test]
    fn rci_classification_uses_dimension_table() {
        let raw = vec![0u8; 40 * 80];
        assert_eq!(
            classify_img(&raw).expect("RCI 40×80"),
            ImgClass::Rci {
                width: 40,
                height: 80,
                palettized: false
            }
        );
        let mut palettized = raw;
        palettized.extend(std::iter::repeat_n(0u8, PAL_FILE_LEN));
        assert_eq!(
            classify_img(&palettized).expect("RCI 40×80 + PalFile"),
            ImgClass::Rci {
                width: 40,
                height: 80,
                palettized: true
            }
        );
    }

    #[test]
    fn unrecognized_img_is_typed_error() {
        assert!(matches!(
            classify_img(&[1u8, 2, 3]),
            Err(ImageError::UnrecognizedImg { .. })
        ));
    }

    /// OWN fixture: two 2×2 uncompressed ImgFile frames.
    fn synthetic_cif() -> Vec<u8> {
        let mut bytes = Vec::new();
        for frame in 0..2u8 {
            bytes.extend_from_slice(&0i16.to_le_bytes());
            bytes.extend_from_slice(&0i16.to_le_bytes());
            bytes.extend_from_slice(&2i16.to_le_bytes());
            bytes.extend_from_slice(&2i16.to_le_bytes());
            bytes.extend_from_slice(&0u16.to_le_bytes());
            bytes.extend_from_slice(&4i16.to_le_bytes());
            bytes.extend_from_slice(&[frame, frame, frame, frame]);
        }
        bytes
    }

    #[test]
    fn cif_frame_chain_decodes_two_frames() {
        let cif = decode_cif(&synthetic_cif()).expect("CIF of 2 frames");
        assert_eq!(cif.frame_count(), 2);
        match cif.content {
            CifContent::Frames(frames) => assert_eq!(frames[1].pixels, vec![1, 1, 1, 1]),
            other => panic!("expected frames, {other:?}"),
        }
    }

    #[test]
    fn cif_rci_faces_layout_is_61_frames() {
        let bytes = vec![0u8; 61 * FACES_FRAME_SIDE * FACES_FRAME_SIDE];
        let cif = decode_cif(&bytes).expect("RCI of 61 frames");
        assert_eq!(
            cif.content,
            CifContent::Rci {
                width: 64,
                height: 64,
                frames: 61
            }
        );
    }

    #[test]
    fn cif_unresolved_is_typed_error() {
        assert!(matches!(
            decode_cif(&[0xAAu8; 1234]),
            Err(ImageError::UnresolvedCif { .. })
        ));
    }

    /// OWN fixture: the `FACES.CIF` layout (61 × 64×64 = 249 856 B) with a
    /// per-record pattern, so a record boundary is visible in the bytes.
    fn synthetic_faces() -> Vec<u8> {
        let cell = FACES_FRAME_SIDE * FACES_FRAME_SIDE;
        let mut bytes = vec![0u8; 61 * cell];
        for (at, byte) in bytes.iter_mut().enumerate() {
            *byte = (1 + (at / cell) % 254) as u8;
        }
        bytes
    }

    /// The atlas is one vertical stack of the container's own records: the
    /// bytes are not interpolated, re-laid-out or re-encoded.
    #[test]
    fn rci_atlas_is_the_file_verbatim_vertically() {
        let bytes = synthetic_faces();
        let atlas = rci_atlas(&bytes, 64, 64, 61).expect("FACES.CIF atlas");
        assert_eq!(atlas.width, 64);
        assert_eq!(atlas.height, 3904);
        assert_eq!(atlas.pixels, bytes);
        assert_eq!(atlas.kind, ImageKind::Rci);
        // Record `r` is the cell `(0, r·64, 64, 64)` of the atlas.
        assert_eq!(atlas.pixel(0, 64), Some(bytes[4096]));
        assert_eq!(atlas.pixel(63, 64 + 63), Some(bytes[2 * 4096 - 1]));
        assert_eq!(atlas.pixel(0, 3903), Some(bytes[bytes.len() - 1]));
        assert_eq!(atlas.pixel(0, 3904), None, "beyond the last record");
    }

    /// A layout that does not tile exactly is refused, never padded or cut.
    #[test]
    fn rci_atlas_rejects_a_partial_tile() {
        let mut bytes = synthetic_faces();
        bytes.push(0);
        assert!(rci_atlas(&bytes, 64, 64, 61).is_none());
        bytes.pop();
        assert!(rci_atlas(&bytes, 64, 64, 62).is_none());
        assert!(rci_atlas(&bytes, 63, 64, 61).is_none());
    }

    /// A container without records has no atlas.
    #[test]
    fn rci_atlas_rejects_zero_frames_and_zero_sides() {
        assert!(rci_atlas(&[], 64, 64, 0).is_none());
        assert!(rci_atlas(&[], 0, 64, 1).is_none());
        assert!(rci_atlas(&[], 64, 0, 1).is_none());
    }

    /// The atlas must fit a `DecodedImage` height (`u16`): 64 records of a
    /// 1024-high cell are 65 536 rows and cannot be expressed.
    #[test]
    fn rci_atlas_rejects_an_overflowing_height() {
        let bytes = vec![1u8; 1024 * 64];
        assert!(rci_atlas(&bytes, 1, 1024, 64).is_none());
        // 1023 rows × 64 records = 65 472 fits, so the boundary is measured,
        // not guessed: 1024 · 64 = 65 536 > u16::MAX is the only rejection.
        let bytes = vec![1u8; 1023 * 64];
        assert_eq!(rci_atlas(&bytes, 1, 1023, 64).expect("fits").height, 65_472);
    }

    /// OWN fixture: a minimal `.FNT` but with the exact retail structure.
    fn synthetic_fnt() -> Vec<u8> {
        let mut bytes = vec![0u8; FNT_FILE_LEN];
        bytes[0..2].copy_from_slice(&14u16.to_le_bytes());
        bytes[2..4].copy_from_slice(&11u16.to_le_bytes());
        for index in 0..FNT_GLYPH_COUNT {
            let offset = (FNT_HEADER_LEN + FNT_GLYPH_COUNT * FNT_ENTRY_LEN + index * FNT_GLYPH_BYTES) as u16;
            let entry = FNT_HEADER_LEN + index * FNT_ENTRY_LEN;
            bytes[entry..entry + 2].copy_from_slice(&offset.to_le_bytes());
            bytes[entry + 2..entry + 4].copy_from_slice(&3u16.to_le_bytes());
        }
        bytes
    }

    #[test]
    fn fnt_header_and_glyph_layout() {
        let font = FntFile::parse(&synthetic_fnt()).expect("synthetic FNT");
        assert_eq!((font.fixed_width, font.fixed_height), (14, 11));
        assert_eq!(font.glyphs.len(), FNT_GLYPH_COUNT);
        assert_eq!(font.glyphs[0].offset, 964);
        assert_eq!(FntFile::ascii_index(b'!'), Some(0));
        assert_eq!(FntFile::ascii_index(b'='), Some(28));
        assert_eq!(FntFile::ascii_index(b'A'), Some(32));
        assert_eq!(FntFile::ascii_index(b'a'), Some(64));
        assert_eq!(FntFile::ascii_index(b' '), None);
    }

    #[test]
    fn glyph_bitmap_reads_left_byte_first() {
        let mut bytes = synthetic_fnt();
        let base = 964 + 7 * FNT_GLYPH_BYTES;
        bytes[base] = 0x00; // right 8 pixels
        bytes[base + 1] = 0x40; // left 8 pixels: bit 14 → x=1
        let font = FntFile::parse(&bytes).expect("FNT");
        let glyph = &font.glyphs[7];
        assert!(glyph.pixel(1, 0));
        assert!(!glyph.pixel(0, 0));
        assert!(!glyph.pixel(9, 0));
    }

    #[test]
    fn render_text_is_deterministic_and_has_ink() {
        let mut bytes = synthetic_fnt();
        let base = 964 + FntFile::ascii_index(b'A').expect("glyph A") * FNT_GLYPH_BYTES;
        bytes[base + 1] = 0x80;
        let font = FntFile::parse(&bytes).expect("FNT");
        let mask = font.render_text("A A");
        assert_eq!(mask.height, 11);
        assert_eq!(mask.ink_pixels(), 2);
        assert_eq!(mask, font.render_text("A A"));
        assert!(mask.pixel(0, 0));
        assert!(mask.pixel(8, 0));
    }

    #[test]
    fn paint_dat_requires_exact_length() {
        let bytes = vec![0xFFu8; PAINT_RECORD_COUNT * PAINT_RECORD_LEN];
        let paint = PaintFile::parse(&bytes).expect("PAINT.DAT");
        assert_eq!(paint.len(), 180);
        assert_eq!(paint.non_padding_bytes(), 0);
        assert!(matches!(
            PaintFile::parse(&bytes[..100]),
            Err(ImageError::PaintLength { .. })
        ));
    }

    #[test]
    fn packed_projection_finds_tail_directory() {
        let mut bytes = vec![0u8; 2048];
        bytes[0..4].copy_from_slice(&2048u32.to_le_bytes());
        let tail = b"ARCH3D.BSA\0\0DAGGER.SND\0\0ARENA2\0\0";
        let start = bytes.len() - PACKED_TAIL_SCAN + 10;
        bytes[start..start + tail.len()].copy_from_slice(tail);
        let projection = project_packed(&bytes).expect("projection");
        let names: Vec<&str> = projection
            .directory
            .iter()
            .map(|entry| entry.name.as_str())
            .collect();
        assert!(names.contains(&"ARCH3D.BSA"));
        assert!(names.contains(&"DAGGER.SND"));
        assert!(names.contains(&"ARENA2"));
    }

    #[test]
    fn palette_family_mapping_is_per_family() {
        assert_eq!(family_for_asset("MAIN00I0.IMG"), PaletteFamily::Hud);
        assert_eq!(family_for_asset("FMAP0I05.IMG"), PaletteFamily::Map);
        assert_eq!(family_for_asset("AMAP00I0.IMG"), PaletteFamily::Map);
        assert_eq!(family_for_asset("TITL00I0.IMG"), PaletteFamily::Embedded);
        assert_eq!(family_for_asset("MASK01I0.IMG"), PaletteFamily::Hud);
        assert_eq!(asset_index_range("MASK01I0.IMG"), 144..=158);
        assert_eq!(asset_index_range("BODY00I0.IMG"), 0..=127);
        assert_eq!(family_index_range(PaletteFamily::Map), 63..=254);
    }

    /// OWN fixture (0 retail bytes): a raw `RCI` 320×200 index image.
    fn synthetic_raw_canvas(indices: &[(usize, u8)]) -> DecodedImage {
        let mut pixels = vec![0u8; RCI_DIMENSIONS[0].0 as usize * RCI_DIMENSIONS[0].1 as usize];
        for (at, index) in indices {
            pixels[*at] = *index;
        }
        DecodedImage {
            x_offset: 0,
            y_offset: 0,
            width: RCI_DIMENSIONS[0].0,
            height: RCI_DIMENSIONS[0].1,
            compression: 0,
            pixels,
            embedded_palette: None,
            kind: ImageKind::Rci,
        }
    }

    #[test]
    fn observed_index_range_ignores_transparency() {
        let image = synthetic_raw_canvas(&[(0, 0), (1, 5), (2, 0), (3, 200), (4, 63)]);
        assert_eq!(observed_index_range(&image), Some(5..=200));
        assert_eq!(
            observed_index_range(&synthetic_raw_canvas(&[(0, 0), (1, 0)])),
            None,
            "a fully transparent asset paints nothing"
        );
    }

    /// **Per-asset rule (M18 B3-c):** only the assets with a recorded
    /// measurement resolve outside their family range; every other asset of
    /// the same family keeps the declared range (0 silent relaxation).
    #[test]
    fn per_asset_range_derivation_is_explicit() {
        assert_eq!(asset_index_range("MAP100I0.IMG"), 63..=254);
        assert_eq!(asset_index_range_resolved("MAP100I0.IMG"), 16..=254);
        assert_eq!(asset_index_range_resolved("TRAV0I00.IMG"), 33..=254);
        // Same family, no derivation ⇒ the family floor is untouched.
        assert_eq!(asset_index_range_resolved("FMAP0I17.IMG"), 63..=254);
        assert_eq!(asset_index_range_resolved("AMAP00I0.IMG"), 63..=254);
        assert_eq!(asset_index_range_resolved("MASK01I0.IMG"), 144..=158);
        assert_eq!(asset_index_range_resolved("MAIN00I0.IMG"), 0..=254);
        // `PICK03I0` is the row measured **above** its family ceiling: the Hud
        // rule (0..=254) rejects its own bytes (index 255), so the entry
        // exists and the family rule itself is untouched.
        assert_eq!(family_for_asset("PICK03I0.IMG"), PaletteFamily::Hud);
        assert_eq!(asset_index_range("PICK03I0.IMG"), 0..=254);
        assert_eq!(asset_index_range_resolved("PICK03I0.IMG"), 1..=255);
        assert!(!asset_index_range("PICK03I0.IMG").contains(&255));
        // Every derivation starts below the `Map` floor and stays inside the
        // complete UI palette range (`Hud`, 0..=254) — the palette authority of
        // these screens (`TRAV0I00` resolves Hud by its name, `MAP100I0` Map).
        let map_floor = *family_index_range(PaletteFamily::Map).start();
        for (asset, low, high, evidence) in ASSET_RANGE_DERIVATIONS {
            assert!(
                matches!(
                    family_for_asset(asset),
                    PaletteFamily::Map | PaletteFamily::Hud
                ),
                "{asset}: unexpected family"
            );
            assert!(low < map_floor, "{asset}: derivation must reach below 63");
            // The row is the measured span, so its top is the asset's own last
            // painted index (`MAP*`/`TRAV0I00` reach 254, `MASK05I0` stops at
            // 218, `PICK03I0` reaches the palette's last index) — never a
            // widened ceiling. 255 is the only index outside the Hud band a
            // measurement may claim: it is the last entry of the `PalFile`.
            let hud = family_index_range(PaletteFamily::Hud);
            assert!(
                hud.contains(&high) || high == u8::MAX,
                "{asset}: derivation ceiling {high} is neither inside the UI palette nor the last palette index"
            );
            assert!(high > low, "{asset}: derivation must span more than one index");
            assert!(!evidence.is_empty(), "{asset}: derivation carries no measurement");
        }
    }

    /// The derivation covers exactly the measured span of the recorded asset.
    #[test]
    fn derivation_covers_the_measured_span() {
        let (_, low, high, _) = ASSET_RANGE_DERIVATIONS[0];
        let image = synthetic_raw_canvas(&[(0, low), (1, 62), (2, high)]);
        let span = observed_index_range(&image).expect("span");
        let resolved = asset_index_range_resolved("MAP100I0.IMG");
        assert!(resolved.contains(span.start()));
        assert!(resolved.contains(span.end()));
        // The family floor alone would reject the measured span (that is why
        // the derivation exists) …
        assert!(!asset_index_range("FMAP0I17.IMG").contains(span.start()));
        // … and the recorded derivation is exactly the measured span.
        assert_eq!(low..=high, 16..=254);
    }

    /// Read-only `ARENA2` of the user's retail install (`$DAGGERFALL_ARENA2`,
    /// `$DF_ARENA2`, `$DF_APP_ROOT/ARENA2`, then the canonical autodetection).
    fn retail_arena2() -> Option<std::path::PathBuf> {
        for var in ["DAGGERFALL_ARENA2", "DF_ARENA2"] {
            if let Ok(dir) = std::env::var(var) {
                if !dir.trim().is_empty() {
                    return Some(std::path::PathBuf::from(dir));
                }
            }
        }
        if let Ok(root) = std::env::var("DF_APP_ROOT") {
            return Some(std::path::PathBuf::from(root).join("ARENA2"));
        }
        std::env::var("HOME")
            .ok()
            .map(|home| std::path::PathBuf::from(home).join("Games/Daggerfall/app/ARENA2"))
    }

    /// **Retail measurement of the M39 A2 fix:** `PICK03I0.IMG` paints the
    /// palette's last index, so its recorded derivation is the span its own
    /// bytes show (`1..=255`) and the `Hud` ceiling (`0..=254`) does not apply.
    ///
    /// Skips (with a message) when the read-only `ARENA2` tree is absent, as
    /// every other runtime probe of the repo does; 0 retail bytes in git.
    #[test]
    fn pick03i0_retail_span_matches_its_derivation() {
        let Some(dir) = retail_arena2() else {
            eprintln!("SKIP pick03i0_retail_span_matches_its_derivation: no ARENA2");
            return;
        };
        let path = dir.join("PICK03I0.IMG");
        let Ok(bytes) = std::fs::read(&path) else {
            eprintln!(
                "SKIP pick03i0_retail_span_matches_its_derivation: {} absent",
                path.display()
            );
            return;
        };
        assert_eq!(
            bytes.len(),
            RCI_DIMENSIONS[0].0 as usize * RCI_DIMENSIONS[0].1 as usize + PAL_FILE_LEN,
            "retail PICK03I0.IMG is 64 000 B of 320×200 pixels plus its 768 B PalFile"
        );
        let image = decode_img_file(&bytes).expect("PICK03I0.IMG decodes as a raw palettized RCI");
        assert_eq!(
            observed_index_range(&image),
            Some(1..=255),
            "the asset's own non-zero span is what the derivation records"
        );
        assert_eq!(asset_index_range_resolved("PICK03I0.IMG"), 1..=255);
        let at_last = image.pixels.iter().filter(|index| **index == u8::MAX).count();
        assert!(
            at_last > 0,
            "index 255 is painted ({at_last} px), so the family ceiling must not reject it"
        );
        assert!(image.embedded_palette.is_some(), "the trailing 768 B PalFile is embedded");
    }
}

