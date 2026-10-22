//! ARENA2 `TEXTURE.*` files — clean-room from UESP
//! `Daggerfall Mod:Image formats/Texture` + `.../Palette` (M12,
//! DoD §c / WI-4 sheet · REQ-L99-007/005/010/022/011/026).
//!
//! Layout (UESP *Image formats/Texture* → "General File Layout"):
//!
//! ```text
//! [TextureFileHeader 26 B][RecordHeader × N (20 B each)][RecordData...]
//! ```
//! - `TextureFileHeader`: `RecordCount u16 LE` + 24 B ASCII name
//!   terminated by a null.
//! - `RecordHeader` (20 B): `Unknown1 u16 · RecordPosition i32 (absolute
//!   offset) · Unknown2 u16 · Unknown3 u32 · NullValue i64 = 0` (spec:
//!   "Always 0x0000000000000000" — verified 0 deviations in 6 676 retail
//!   records, 2026-10-20).
//! - At `RecordPosition` (if ≠ 0) lives a 28 B `TextureRecord`:
//!   `OffsetX/OffsetY/Width/Height i16 · Compression u16 · RecordSize u32 ·
//!   DataOffset u32 (from the start of the TextureRecord) · IsNormal u16 ·
//!   FrameCount u16 · Unknown1 u16 · XScale/YScale i16 (/256)`.
//!
//! **Four decoders** selected by `Compression`/`FrameCount`
//! (UESP §"Record Data"; dispatch verified against the 4 72 ARENA2 retail
//! files — 6 676 records, 2026-10-20):
//!
//! 1. **UncompressedSingleFrame** (`FrameCount = 1`, non-RLE comp): rows of
//!    `Width` bytes at `pos + DataOffset + row·256` (spec: read `Width` and
//!    skip `256 − Width`; the retail interleaving between records of the same
//!    block fills exactly that gap). Max. `Width = 256`.
//! 2. **UncompressedMultiframe** (`FrameCount > 1`): at `pos + DataOffset`
//!    a table of `FrameCount` `i32` (relative to that base) → `FrameData`
//!    `{Width u16, Height u16}` + data with zero/non-zero alternation
//!    (UESP pseudocode reproduced EXACTLY, including the extra byte that
//!    a row ending in a zero run consumes — verified: 1 101 retail
//!    multiframe records decode to exactly `W×H×frames` bytes).
//! 3. **RecordRle / ImageRle** (`Compression ∈ {0x1108, 0x0108}`): list of
//!    `SpecialFrameHeader {RowOffset i16, RowEncoding u16}` with
//!    `FrameCount × Height` elements at `pos + DataOffset`; each row at
//!    `pos + RowOffset` (**offset relative to the TextureRecord** — UESP
//!    wording delta confirmed empirically on 1 359/1 359 retail records):
//!    `RowEncoding = 0` → `Width` raw bytes; `0x8000` → modified RLE
//!    (`i16 target` + probes: `< 0` repeats the next byte `−probe`
//!    times, `> 0` copies `probe` bytes).
//! 4. **Special**: `RecordPosition = 0` → solid image
//!    (`TEXTURE.000/001`, palette index = high byte of `Unknown1`);
//!    `RecordPosition` outside the file → no image
//!    (`TEXTURE.215/217`: header + list only, "most likely not used").
//!
//! **Palettes** (UESP *Image formats/Palette*): `PalFile` = 768 B (256 RGB);
//! `ColFile` = 776 B = `ColHeader {Length i32 = 776, FileFormatMajor u16 =
//! 0xB123, FileFormatMinor u16 = 0}` + `PalFile`. Index 0 = transparent
//! (α = 0); the rest opaque.
//!
//! **Custody (INV-L99-6):** 0 retail bytes in git — this module is
//! code + its own synthetic fixtures; the real files are read at
//! runtime from the user's ARENA2 (Mode A), as in the rest of the
//! parser. Parity-first: crate with 0 dependencies (these types use only
//! `core`/`std`).

/// `TextureFileHeader` is 26 bytes (2 + 24 for the name).
pub const TEXTURE_FILE_HEADER_LEN: usize = 26;
/// `RecordHeader` is 20 bytes.
pub const RECORD_HEADER_LEN: usize = 20;
/// `TextureRecord` is 28 bytes.
pub const TEXTURE_RECORD_LEN: usize = 28;
/// Row chunk of the single-frame decoder (256 bytes, UESP).
pub const ROW_CHUNK: usize = 256;
/// Maximum width of a single-frame image (UESP: "maximum width ... 256").
pub const MAX_SINGLE_FRAME_WIDTH: u16 = 256;

/// `Compression = ImageRle` (UESP *Image formats/IMG* §Compression).
pub const COMPRESSION_IMAGE_RLE: u16 = 0x0108;
/// `Compression = RecordRle` (UESP *Image formats/IMG* §Compression).
pub const COMPRESSION_RECORD_RLE: u16 = 0x1108;
/// `SpecialFrameHeader.RowEncoding` with an RLE row (UESP §RowEncoding).
pub const ROW_ENCODING_RLE: u16 = 0x8000;
/// `SpecialFrameHeader.RowEncoding` with a raw row.
pub const ROW_ENCODING_RAW: u16 = 0x0000;

/// `ColHeader.FileFormatMajor` of a `ColFile` (UESP §ColHeader).
pub const COL_FILE_FORMAT_MAJOR: u16 = 0xB123;
/// Length of a `ColFile` (8 header + 768 ColorList).
pub const COL_FILE_LEN: usize = 776;
/// Length of a `PalFile` (256 × 3).
pub const PAL_FILE_LEN: usize = 768;

/// Parse/decode error for textures.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TextureError {
    /// File shorter than the minimum header.
    TooShort { len: usize, need: usize },
    /// The RecordHeader list does not fit in the file.
    HeaderOutOfBounds { record_count: u16, need: usize, len: usize },
    /// RecordHeader `NullValue` ≠ 0 (spec: always 0).
    NonZeroNullValue { index: usize, value: i64 },
    /// `RecordPosition` points to a TextureRecord that does not fit in the file.
    RecordOutOfBounds { index: usize, position: u32, len: usize },
    /// Single-frame row that falls outside the file.
    RowOutOfBounds { index: usize, row: u32, need: usize, len: usize },
    /// `Width > 256` in the single-frame decoder (the spec forbids it).
    WidthTooLarge { index: usize, width: u16 },
    /// Multiframe frame table outside the file.
    FrameTableOutOfBounds { index: usize, need: usize, len: usize },
    /// Frame/FrameData outside the file or invalid dimensions.
    FrameOutOfBounds { index: usize, frame: u16, need: usize, len: usize },
    /// Multiframe row that does not end exactly at `Width` (corrupt data).
    MultiframeRowMismatch { index: usize, frame: u16, row: u16, got: u32, want: u16 },
    /// SpecialFrameHeader list outside the file.
    RleHeaderOutOfBounds { index: usize, need: usize, len: usize },
    /// RLE row unreachable or out of range.
    RleRowOutOfBounds { index: usize, row: usize, need: usize, len: usize },
    /// `RowEncoding` that is not {0x0000, 0x8000} (the spec defines only those two).
    RleUnknownEncoding { index: usize, row: usize, encoding: u16 },
    /// RLE probe = 0 (the UESP pseudocode would loop forever: 0 is
    /// corrupt, so it is reported instead of hanging) or overrun run.
    RleProbeInvalid { index: usize, row: usize, probe: i16 },
    /// An RLE row does not end exactly at its declared length.
    RleRowMismatch { index: usize, row: usize, got: u32, want: u16 },
    /// A frame that does not exist was requested (`frame ≥ FrameCount`).
    FrameOutOfRange { index: usize, frame: u16, frame_count: u16 },
    /// The record has no image data (solid or no image).
    NoImageData { index: usize },
    /// Palette with a length that is neither 768 (PalFile) nor 776 (ColFile).
    PaletteLength { len: usize },
    /// Invalid ColHeader (Length ≠ 776 or magic ≠ 0xB123).
    BadColHeader { length: u32, format_major: u16 },
}

impl std::fmt::Display for TextureError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TooShort { len, need } => {
                write!(f, "texture file too short ({len} < {need} bytes)")
            }
            Self::HeaderOutOfBounds {
                record_count,
                need,
                len,
            } => write!(
                f,
                "RecordCount={record_count} does not fit in the file ({need} > {len} bytes)"
            ),
            Self::NonZeroNullValue { index, value } => {
                write!(f, "record {index}: NullValue={value:#x} (spec: always 0)")
            }
            Self::RecordOutOfBounds { index, position, len } => write!(
                f,
                "record {index}: TextureRecord at position {position} outside the file ({len} bytes)"
            ),
            Self::RowOutOfBounds { index, row, need, len } => write!(
                f,
                "record {index}: row {row} needs {need} bytes > {len}"
            ),
            Self::WidthTooLarge { index, width } => write!(
                f,
                "record {index}: Width={width} > {MAX_SINGLE_FRAME_WIDTH} in single-frame decoder"
            ),
            Self::FrameTableOutOfBounds { index, need, len } => {
                write!(f, "record {index}: frame table {need} > {len} bytes")
            }
            Self::FrameOutOfBounds { index, frame, need, len } => {
                write!(f, "record {index} frame {frame}: {need} > {len} bytes")
            }
            Self::MultiframeRowMismatch {
                index,
                frame,
                row,
                got,
                want,
            } => write!(
                f,
                "record {index} frame {frame} row {row}: {got} pixels ≠ {want}"
            ),
            Self::RleHeaderOutOfBounds { index, need, len } => {
                write!(f, "record {index}: SpecialFrameHeaders {need} > {len} bytes")
            }
            Self::RleRowOutOfBounds { index, row, need, len } => {
                write!(f, "record {index} RLE row {row}: offset {need} outside {len}")
            }
            Self::RleUnknownEncoding { index, row, encoding } => write!(
                f,
                "record {index} row {row}: RowEncoding={encoding:#06x} (spec: 0x0000/0x8000)"
            ),
            Self::RleProbeInvalid { index, row, probe } => {
                write!(f, "record {index} row {row}: invalid RLE probe {probe}")
            }
            Self::RleRowMismatch { index, row, got, want } => {
                write!(f, "record {index} row {row}: {got} ≠ {want} pixels")
            }
            Self::FrameOutOfRange {
                index,
                frame,
                frame_count,
            } => write!(
                f,
                "record {index}: frame {frame} out of range (FrameCount={frame_count})"
            ),
            Self::NoImageData { index } => {
                write!(f, "record {index}: no image data (solid or no image)")
            }
            Self::PaletteLength { len } => write!(
                f,
                "palette of length {len} (expected 768 PalFile or 776 ColFile)"
            ),
            Self::BadColHeader {
                length,
                format_major,
            } => write!(
                f,
                "invalid ColHeader: Length={length} (expected {COL_FILE_LEN}) FileFormatMajor={format_major:#06x} (expected {COL_FILE_FORMAT_MAJOR:#06x})"
            ),
        }
    }
}

impl std::error::Error for TextureError {}

fn u16_le(bytes: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([bytes[at], bytes[at + 1]])
}

fn i16_le(bytes: &[u8], at: usize) -> i16 {
    i16::from_le_bytes([bytes[at], bytes[at + 1]])
}

fn u32_le(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]])
}

/// `TextureRecord` (28 bytes) at a `RecordPosition`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextureRecordMeta {
    /// Display offset in X.
    pub offset_x: i16,
    /// Display offset in Y.
    pub offset_y: i16,
    /// Image width (pixels).
    pub width: i16,
    /// Image height (pixels).
    pub height: i16,
    /// `Compression` (see `COMPRESSION_*`; unknown values → no RLE,
    /// UESP: "Any unknown values should be interpretted as Uncompressed").
    pub compression: u16,
    /// Total record size including these 28 bytes.
    pub record_size: u32,
    /// Offset of the image data from the start of the TextureRecord.
    pub data_offset: u32,
    /// `IsNormal` flag (UESP declares it "required to decode" but defines
    /// no different decoder — 0 branches on this flag, spec honesty).
    pub is_normal: u16,
    /// Number of animation frames.
    pub frame_count: u16,
    /// Unknown field of the spec.
    pub unknown1: u16,
    /// Width modifier ÷256 (scene flat).
    pub x_scale: i16,
    /// Height modifier ÷256 (scene flat; always = XScale in retail).
    pub y_scale: i16,
}

/// What is at a record's position.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordKind {
    /// Parseable `TextureRecord` at `position`.
    Image(TextureRecordMeta),
    /// `RecordPosition = 0`: solid image (`TEXTURE.000/001`) — the palette
    /// index is the high byte of `RecordHeader.Unknown1`.
    Solid { palette_index: u8 },
    /// `RecordPosition` outside the file: header + list only
    /// (`TEXTURE.215/217`, UESP: "most likely not used").
    NoImage,
}

/// `RecordHeader` (20 bytes).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecordHeader {
    /// Unknown purpose (in `TEXTURE.000/001` its high byte is the
    /// palette index of the solid color).
    pub unknown1: u16,
    /// Absolute offset of the TextureRecord within the file (0 = solid).
    pub position: u32,
    /// Unknown field of the spec.
    pub unknown2: u16,
    /// Unknown field of the spec.
    pub unknown3: u32,
    /// Always 0 (spec "confirmed" + M12 retail verification).
    pub null_value: i64,
    /// What the record's position contains.
    pub kind: RecordKind,
}

impl RecordHeader {
    /// Palette index of the solid color, if the record is solid.
    pub fn solid_palette_index(&self) -> Option<u8> {
        match self.kind {
            RecordKind::Solid { palette_index } => Some(palette_index),
            _ => None,
        }
    }

    /// TextureRecord metadata, if there is a decodable image.
    pub fn meta(&self) -> Option<&TextureRecordMeta> {
        match &self.kind {
            RecordKind::Image(meta) => Some(meta),
            _ => None,
        }
    }
}

/// Parsed `TEXTURE.*` file (header + record headers; the original bytes
/// are NOT copied — the caller retains them for [`decode_frame`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextureArchive {
    /// Number of records (`TextureFileHeader.RecordCount`).
    pub record_count: u16,
    /// File name (ASCII, up to the first null).
    pub name: String,
    /// `RecordHeader`s in file order (canonical iteration order).
    pub records: Vec<RecordHeader>,
}

impl TextureArchive {
    /// Parses a complete `TEXTURE.*` file.
    pub fn parse(file: &[u8]) -> Result<Self, TextureError> {
        if file.len() < TEXTURE_FILE_HEADER_LEN {
            return Err(TextureError::TooShort {
                len: file.len(),
                need: TEXTURE_FILE_HEADER_LEN,
            });
        }
        let record_count = u16_le(file, 0);
        let need = TEXTURE_FILE_HEADER_LEN + record_count as usize * RECORD_HEADER_LEN;
        if need > file.len() {
            return Err(TextureError::HeaderOutOfBounds {
                record_count,
                need,
                len: file.len(),
            });
        }
        let raw_name = &file[2..TEXTURE_FILE_HEADER_LEN];
        let name_len = raw_name.iter().position(|&b| b == 0).unwrap_or(raw_name.len());
        let name = raw_name[..name_len]
            .iter()
            .map(|&b| b as char)
            .collect::<String>();

        let mut records = Vec::with_capacity(record_count as usize);
        for index in 0..record_count as usize {
            let at = TEXTURE_FILE_HEADER_LEN + index * RECORD_HEADER_LEN;
            let unknown1 = u16_le(file, at);
            let position = u32_le(file, at + 2);
            let unknown2 = u16_le(file, at + 6);
            let unknown3 = u32_le(file, at + 8);
            let null_value = i64::from_le_bytes(
                file[at + 12..at + 20].try_into().expect("8-byte slice"),
            );
            if null_value != 0 {
                return Err(TextureError::NonZeroNullValue {
                    index,
                    value: null_value,
                });
            }
            let kind = if position == 0 {
                RecordKind::Solid {
                    palette_index: (unknown1 >> 8) as u8,
                }
            } else if position as usize + TEXTURE_RECORD_LEN > file.len() {
                // TEXTURE.215/217: position == length (header+list only).
                RecordKind::NoImage
            } else {
                let p = position as usize;
                RecordKind::Image(TextureRecordMeta {
                    offset_x: i16_le(file, p),
                    offset_y: i16_le(file, p + 2),
                    width: i16_le(file, p + 4),
                    height: i16_le(file, p + 6),
                    compression: u16_le(file, p + 8),
                    record_size: u32_le(file, p + 10),
                    data_offset: u32_le(file, p + 14),
                    is_normal: u16_le(file, p + 18),
                    frame_count: u16_le(file, p + 20),
                    unknown1: u16_le(file, p + 22),
                    x_scale: i16_le(file, p + 24),
                    y_scale: i16_le(file, p + 26),
                })
            };
            records.push(RecordHeader {
                unknown1,
                position,
                unknown2,
                unknown3,
                null_value,
                kind,
            });
        }
        Ok(Self {
            record_count,
            name,
            records,
        })
    }
}

/// Decoded image: **palette index** pixels in row-major order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecodedTexture {
    /// Width in pixels.
    pub width: u16,
    /// Height in pixels.
    pub height: u16,
    /// `width × height` palette indices (0..=255).
    pub pixels: Vec<u8>,
}

fn read_u8(bytes: &[u8], at: &mut usize) -> Option<u8> {
    let b = *bytes.get(*at)?;
    *at += 1;
    Some(b)
}

fn read_i16(bytes: &[u8], at: &mut usize) -> Option<i16> {
    let b = bytes.get(*at..*at + 2)?;
    *at += 2;
    Some(i16::from_le_bytes([b[0], b[1]]))
}

/// Decodes ONE frame of a record to palette indices.
///
/// `file` are the raw file bytes; `archive` is its parse (for the record's
/// kind). `frame` is 0 for single-frame records.
pub fn decode_frame(
    file: &[u8],
    archive: &TextureArchive,
    index: usize,
    frame: u16,
) -> Result<DecodedTexture, TextureError> {
    let header = archive
        .records
        .get(index)
        .copied()
        .ok_or(TextureError::FrameOutOfRange {
            index,
            frame,
            frame_count: 0,
        })?;
    let meta = header.meta().ok_or(TextureError::NoImageData { index })?;
    let position = header.position as usize;
    if frame >= meta.frame_count.max(1) {
        return Err(TextureError::FrameOutOfRange {
            index,
            frame,
            frame_count: meta.frame_count,
        });
    }

    match meta.compression {
        COMPRESSION_RECORD_RLE | COMPRESSION_IMAGE_RLE => {
            decode_rle_rows(file, index, position, meta, frame)
        }
        _ if meta.frame_count > 1 => decode_multiframe(file, index, position, meta, frame),
        _ => decode_single_frame(file, index, position, meta),
    }
}

/// Decoder 1 — single-frame interleaved in 256 byte chunks (UESP §
/// "UncompressedSingleFrame Records").
fn decode_single_frame(
    file: &[u8],
    index: usize,
    position: usize,
    meta: &TextureRecordMeta,
) -> Result<DecodedTexture, TextureError> {
    let width = meta.width.max(0) as u16;
    let height = meta.height.max(0) as u16;
    let mut pixels = Vec::new();
    if height == 0 {
        return Ok(DecodedTexture {
            width,
            height,
            pixels,
        });
    }
    if width > MAX_SINGLE_FRAME_WIDTH {
        return Err(TextureError::WidthTooLarge { index, width });
    }
    let base = position + meta.data_offset as usize;
    pixels.reserve(width as usize * height as usize);
    for row in 0..height as usize {
        let at = base + row * ROW_CHUNK;
        let need = at + width as usize;
        if need > file.len() {
            return Err(TextureError::RowOutOfBounds {
                index,
                row: row as u32,
                need,
                len: file.len(),
            });
        }
        pixels.extend_from_slice(&file[at..at + width as usize]);
    }
    Ok(DecodedTexture {
        width,
        height,
        pixels,
    })
}

/// Decoder 2 — multiframe with zero/non-zero alternation (UESP § "Reading
/// UncompressedMultiframe Records"; pseudocode reproduced exactly,
/// including the `c = readByte()` that a row ending in a zero run
/// consumes before exiting — verified against 1 101 retail records).
fn decode_multiframe(
    file: &[u8],
    index: usize,
    position: usize,
    meta: &TextureRecordMeta,
    frame: u16,
) -> Result<DecodedTexture, TextureError> {
    let frames = meta.frame_count;
    let base = position + meta.data_offset as usize;
    let table_bytes = frames as usize * 4;
    if base + table_bytes > file.len() {
        return Err(TextureError::FrameTableOutOfBounds {
            index,
            need: base + table_bytes,
            len: file.len(),
        });
    }
    let rel = u32_le(file, base + frame as usize * 4) as usize;
    let frame_at = base + rel;
    if frame_at + 4 > file.len() {
        return Err(TextureError::FrameOutOfBounds {
            index,
            frame,
            need: frame_at + 4,
            len: file.len(),
        });
    }
    let frame_width = u16_le(file, frame_at);
    let frame_height = u16_le(file, frame_at + 2);
    let mut at = frame_at + 4;
    let mut pixels = Vec::with_capacity(frame_width as usize * frame_height as usize);

    for row in 0..frame_height {
        let Some(mut c) = read_u8(file, &mut at) else {
            return Err(TextureError::FrameOutOfBounds {
                index,
                frame,
                need: at + 1,
                len: file.len(),
            });
        };
        let mut is_zero = true;
        let mut p: usize = 0;
        while p < frame_width as usize {
            for _ in 0..c as usize {
                if p >= frame_width as usize {
                    // Run that overflows the row: corrupt data (the
                    // UESP pseudocode would write outside the buffer).
                    return Err(TextureError::MultiframeRowMismatch {
                        index,
                        frame,
                        row,
                        got: p as u32 + 1,
                        want: frame_width,
                    });
                }
                if is_zero {
                    pixels.push(0);
                } else {
                    match read_u8(file, &mut at) {
                        Some(b) => pixels.push(b),
                        None => {
                            return Err(TextureError::FrameOutOfBounds {
                                index,
                                frame,
                                need: at + 1,
                                len: file.len(),
                            })
                        }
                    }
                }
                p += 1;
            }
            if p < frame_width as usize || (p == frame_width as usize && is_zero) {
                match read_u8(file, &mut at) {
                    Some(next) => c = next,
                    None => {
                        return Err(TextureError::FrameOutOfBounds {
                            index,
                            frame,
                            need: at + 1,
                            len: file.len(),
                        })
                    }
                }
            }
            is_zero = !is_zero;
        }
        let produced = pixels.len();
        let expect = (row as usize + 1) * frame_width as usize;
        if produced != expect {
            return Err(TextureError::MultiframeRowMismatch {
                index,
                frame,
                row,
                got: produced as u32,
                want: frame_width,
            });
        }
    }
    Ok(DecodedTexture {
        width: frame_width,
        height: frame_height,
        pixels,
    })
}

/// Decoder 3 — RecordRle/ImageRle rows (UESP § "Reading RecordRle
/// Records" + § "Reading modified Rle row data").
///
/// `RowOffset` is **relative to the start of the TextureRecord** (UESP
/// prose ties it to the DataOffset value; retail requires this other base —
/// 1 359 / 1 359 RLE records decode with this reading, M12 verification).
fn decode_rle_rows(
    file: &[u8],
    index: usize,
    position: usize,
    meta: &TextureRecordMeta,
    frame: u16,
) -> Result<DecodedTexture, TextureError> {
    let width = meta.width.max(0) as u16;
    let height = meta.height.max(0) as u16;
    let headers_at = position + meta.data_offset as usize;
    let total_rows = meta.frame_count as usize * height as usize;
    let headers_need = headers_at + total_rows * 4;
    if headers_need > file.len() {
        return Err(TextureError::RleHeaderOutOfBounds {
            index,
            need: headers_need,
            len: file.len(),
        });
    }
    let mut pixels = Vec::with_capacity(width as usize * height as usize);
    for row in 0..height as usize {
        let header_index = frame as usize * height as usize + row;
        let at = headers_at + header_index * 4;
        let row_offset = i16_le(file, at);
        let encoding = u16_le(file, at + 2);
        let row_at = position as isize + row_offset as isize;
        if row_at < 0 {
            return Err(TextureError::RleRowOutOfBounds {
                index,
                row: header_index,
                need: row_at as usize,
                len: file.len(),
            });
        }
        let row_at = row_at as usize;
        match encoding {
            ROW_ENCODING_RAW => {
                let need = row_at + width as usize;
                if need > file.len() {
                    return Err(TextureError::RleRowOutOfBounds {
                        index,
                        row: header_index,
                        need,
                        len: file.len(),
                    });
                }
                pixels.extend_from_slice(&file[row_at..need]);
            }
            ROW_ENCODING_RLE => {
                if row_at + 2 > file.len() {
                    return Err(TextureError::RleRowOutOfBounds {
                        index,
                        row: header_index,
                        need: row_at + 2,
                        len: file.len(),
                    });
                }
                let mut cursor = row_at;
                let target = read_i16(file, &mut cursor).unwrap_or(0);
                if target < 0 {
                    return Err(TextureError::RleRowMismatch {
                        index,
                        row: header_index,
                        got: 0,
                        want: width,
                    });
                }
                let mut produced: u32 = 0;
                while (produced as i16) < target {
                    let Some(probe) = read_i16(file, &mut cursor) else {
                        return Err(TextureError::RleRowOutOfBounds {
                            index,
                            row: header_index,
                            need: cursor + 2,
                            len: file.len(),
                        });
                    };
                    if probe == 0 {
                        return Err(TextureError::RleProbeInvalid {
                            index,
                            row: header_index,
                            probe,
                        });
                    }
                    if probe < 0 {
                        let Some(pixel) = read_u8(file, &mut cursor) else {
                            return Err(TextureError::RleRowOutOfBounds {
                                index,
                                row: header_index,
                                need: cursor + 1,
                                len: file.len(),
                            });
                        };
                        for _ in 0..probe.unsigned_abs() {
                            pixels.push(pixel);
                        }
                        produced += probe.unsigned_abs() as u32;
                    } else {
                        let count = probe as usize;
                        match file.get(cursor..cursor + count) {
                            Some(bytes) => {
                                pixels.extend_from_slice(bytes);
                                cursor += count;
                                produced += count as u32;
                            }
                            None => {
                                return Err(TextureError::RleRowOutOfBounds {
                                    index,
                                    row: header_index,
                                    need: cursor + count,
                                    len: file.len(),
                                })
                            }
                        }
                    }
                    if produced > u16::MAX as u32 {
                        return Err(TextureError::RleRowMismatch {
                            index,
                            row: header_index,
                            got: produced,
                            want: width,
                        });
                    }
                }
                if produced != target as u32 {
                    return Err(TextureError::RleRowMismatch {
                        index,
                        row: header_index,
                        got: produced,
                        want: target as u16,
                    });
                }
                if produced != width as u32 {
                    return Err(TextureError::RleRowMismatch {
                        index,
                        row: header_index,
                        got: produced,
                        want: width,
                    });
                }
            }
            other => {
                return Err(TextureError::RleUnknownEncoding {
                    index,
                    row: header_index,
                    encoding: other,
                })
            }
        }
    }
    Ok(DecodedTexture {
        width,
        height,
        pixels,
    })
}

/// 256-color RGB palette (UESP *Image formats/Palette*).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Palette {
    /// RGB per index (256 × 3).
    pub colors: [[u8; 3]; 256],
}

impl Palette {
    /// Parses a `PalFile` (768 B) or `ColFile` (776 B with Autodesk ColHeader
    /// `Length = 776 · FileFormatMajor = 0xB123`).
    pub fn parse(bytes: &[u8]) -> Result<Self, TextureError> {
        let raw: &[u8] = match bytes.len() {
            PAL_FILE_LEN => bytes,
            COL_FILE_LEN => {
                let length = u32_le(bytes, 0);
                let format_major = u16_le(bytes, 4);
                if length as usize != COL_FILE_LEN || format_major != COL_FILE_FORMAT_MAJOR {
                    return Err(TextureError::BadColHeader {
                        length,
                        format_major,
                    });
                }
                &bytes[8..]
            }
            other => return Err(TextureError::PaletteLength { len: other }),
        };
        let mut colors = [[0u8; 3]; 256];
        for (i, color) in colors.iter_mut().enumerate() {
            let at = i * 3;
            color.copy_from_slice(&raw[at..at + 3]);
        }
        Ok(Self { colors })
    }

    /// RGB color of a palette index.
    pub fn color(&self, index: u8) -> [u8; 3] {
        self.colors[index as usize]
    }

    /// Palette indices → RGBA8: index 0 = transparent (UESP: "the first
    /// entry is considered the Transparent color (0 Alpha)"), the rest opaque.
    pub fn to_rgba(&self, pixels: &[u8]) -> Vec<u8> {
        let mut rgba = Vec::with_capacity(pixels.len() * 4);
        for &index in pixels {
            let [r, g, b] = self.colors[index as usize];
            rgba.extend_from_slice(&[r, g, b, if index == 0 { 0 } else { 255 }]);
        }
        rgba
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds a synthetic single-frame file with 2 records
    /// interleaved in 256 B chunks (same scheme as retail:
    /// [row0 rec0][row0 rec1][pad] · [row1 rec0][row1 rec1][pad] ...).
    ///
    /// 100% own fixture (0 retail bytes — INV-L99-6).
    fn synthetic_single_frame() -> (Vec<u8>, Vec<u8>, Vec<u8>) {
        // rec0: 4×3 px, rec1: 4×3 px — both fit in one 256 chunk.
        let w: usize = 4;
        let h: usize = 3;
        let count: u16 = 2;
        let header_len = TEXTURE_FILE_HEADER_LEN + count as usize * RECORD_HEADER_LEN;
        let texture_records = header_len + count as usize * TEXTURE_RECORD_LEN;
        // Data: h chunks of 256 B; rec1 starts 4 B after rec0.
        let data0 = texture_records;
        let data1 = data0 + w;
        let file_len = data0 + h * ROW_CHUNK;

        let mut file = vec![0u8; file_len];
        // TextureFileHeader.
        file[0..2].copy_from_slice(&count.to_le_bytes());
        let name = b"SYNTH M12";
        file[2..2 + name.len()].copy_from_slice(name);
        // RecordHeaders (position relative to the file).
        let pos0 = header_len as u32;
        let pos1 = (header_len + TEXTURE_RECORD_LEN) as u32;
        for (i, pos, doff) in [
            (0usize, pos0, (data0 - header_len) as u32),
            (1, pos1, (data1 - (header_len + TEXTURE_RECORD_LEN)) as u32),
        ] {
            let at = TEXTURE_FILE_HEADER_LEN + i * RECORD_HEADER_LEN;
            // Unknown1 = 0x1200 + i (different per record, no semantics).
            file[at..at + 2].copy_from_slice(&(0x1200u16 + i as u16).to_le_bytes());
            file[at + 2..at + 6].copy_from_slice(&pos.to_le_bytes());
            file[at + 6..at + 8].copy_from_slice(&(0x3400u16 + i as u16).to_le_bytes());
            file[at + 8..at + 12].copy_from_slice(&0u32.to_le_bytes());
            // NullValue = 0 (12..20) is already zero.
            let rec = pos as usize;
            // TextureRecord: ox=0 oy=0 w h comp=0 rsize=28+w*h doff.
            file[rec + 4..rec + 6].copy_from_slice(&(w as i16).to_le_bytes());
            file[rec + 6..rec + 8].copy_from_slice(&(h as i16).to_le_bytes());
            file[rec + 8..rec + 10].copy_from_slice(&0u16.to_le_bytes());
            file[rec + 10..rec + 14].copy_from_slice(&((TEXTURE_RECORD_LEN + w * h) as u32).to_le_bytes());
            file[rec + 14..rec + 18].copy_from_slice(&doff.to_le_bytes());
            file[rec + 18..rec + 20].copy_from_slice(&192u16.to_le_bytes()); // IsNormal retail-like
            file[rec + 20..rec + 22].copy_from_slice(&1u16.to_le_bytes()); // FrameCount
            file[rec + 22..rec + 24].copy_from_slice(&71u16.to_le_bytes());
        }
        // Pixels: row r of rec0 = [r, r+1, r+2, r+3] (own values);
        // row r of rec1 = [10+r, 11+r, 12+r, 13+r].
        let mut expected0 = Vec::new();
        let mut expected1 = Vec::new();
        for r in 0..h {
            let base = data0 + r * ROW_CHUNK;
            let row0: Vec<u8> = (0..w).map(|c| (r + c) as u8).collect();
            let row1: Vec<u8> = (0..w).map(|c| (10 + r + c) as u8).collect();
            file[base..base + w].copy_from_slice(&row0);
            file[base + w..base + w + w].copy_from_slice(&row1);
            expected0.extend_from_slice(&row0);
            expected1.extend_from_slice(&row1);
        }
        (file, expected0, expected1)
    }

    #[test]
    fn parses_synthetic_header_and_records() {
        let (file, _, _) = synthetic_single_frame();
        let archive = TextureArchive::parse(&file).expect("synthetic parse");
        assert_eq!(archive.record_count, 2);
        assert_eq!(archive.name, "SYNTH M12");
        assert_eq!(archive.records.len(), 2);
        let meta0 = archive.records[0].meta().expect("rec0 with image");
        assert_eq!((meta0.width, meta0.height), (4, 3));
        assert_eq!(meta0.compression, 0);
        assert_eq!(meta0.frame_count, 1);
        assert_eq!(meta0.is_normal, 192);
        assert_eq!(meta0.record_size, (TEXTURE_RECORD_LEN + 12) as u32);
        assert_eq!(archive.records[0].unknown1, 0x1200);
        assert_eq!(archive.records[1].unknown1, 0x1201);
        assert_eq!(archive.records[0].null_value, 0);
    }

    /// Golden vector 1: exact decode of the 2 interleaved records.
    #[test]
    fn golden_single_frame_decode() {
        let (file, expected0, expected1) = synthetic_single_frame();
        let archive = TextureArchive::parse(&file).unwrap();
        let d0 = decode_frame(&file, &archive, 0, 0).unwrap();
        assert_eq!((d0.width, d0.height), (4, 3));
        assert_eq!(d0.pixels, expected0, "rec0 pixels exact");
        assert_eq!(
            d0.pixels,
            vec![0, 1, 2, 3, 1, 2, 3, 4, 2, 3, 4, 5],
            "golden rec0"
        );
        let d1 = decode_frame(&file, &archive, 1, 0).unwrap();
        assert_eq!(d1.pixels, expected1);
        assert_eq!(
            d1.pixels,
            vec![10, 11, 12, 13, 11, 12, 13, 14, 12, 13, 14, 15],
            "golden rec1"
        );
    }

    #[test]
    fn deterministic_decode_same_bytes() {
        let (file, _, _) = synthetic_single_frame();
        let a = TextureArchive::parse(&file).unwrap();
        let b = TextureArchive::parse(&file).unwrap();
        assert_eq!(a, b, "same input → same parse");
        let da = decode_frame(&file, &a, 0, 0).unwrap();
        let db = decode_frame(&file, &b, 0, 0).unwrap();
        assert_eq!(da, db, "same input → same pixels");
    }

    /// Multiframe decoder: 2 frames × rows with zero/non-zero runs,
    /// including an ALL-zero row (exercises the extra closing byte).
    #[test]
    fn golden_multiframe_decode() {
        // Rows (4 px): f0r0 = [0,0,5,6] (ends in a data run),
        // f0r1 = [7,0,0,9] (data, zeros, data), f1r0 = [0,0,0,0]
        // (all zeros → extra byte), f1r1 = [1,2,3,4] (z1=0 + data).
        let frames: u16 = 2;
        let fw: u16 = 4;
        let fh: u16 = 2;
        let count: u16 = 1;
        let header_len = TEXTURE_FILE_HEADER_LEN + count as usize * RECORD_HEADER_LEN;
        let pos0 = header_len as u32;
        let base = header_len + TEXTURE_RECORD_LEN; // pos + data_offset(28)
        // Frame table (2 × i32) + FrameData.
        let frame0_rel = 8usize; // immediately after the table
        let frame0_at = base + frame0_rel;
        // FrameData0: dims + rows (runs: c announces the run length;
        // ZERO runs consume no data bytes, only data runs do):
        //   r0 = [0,0,5,6] → c=2 zeros, c=2 + 5,6
        //   r1 = [7,0,0,9] → c=0 zeros, c=1 + 7, c=2 zeros, c=1 + 9
        let frame0_data: Vec<u8> = vec![
            2, 2, 5, 6, // r0
            0, 1, 7, 2, 1, 9, // r1
        ];
        let frame0_len = 4 + frame0_data.len();
        let frame1_rel = frame0_rel + frame0_len;
        let frame1_at = base + frame1_rel;
        // FrameData1: r0 all zeros (c=4 → a row ending in zeros
        // consumes 1 extra byte per the faithful UESP pseudocode), r1 z1=0.
        let frame1_data: Vec<u8> = vec![
            4, 0xAA, // r0: c=4 zeros + extra byte consumed
            0, 4, 1, 2, 3, 4, // r1: c=0 zeros, c=4 data bytes
        ];
        let frame1_len = 4 + frame1_data.len();
        let file_len = base + frame1_rel + frame1_len;
        let mut file = vec![0u8; file_len];
        file[0..2].copy_from_slice(&count.to_le_bytes());
        file[2..2 + 4].copy_from_slice(b"MULF");
        let at = TEXTURE_FILE_HEADER_LEN;
        file[at + 2..at + 6].copy_from_slice(&pos0.to_le_bytes());
        // TextureRecord en pos0: w=4 h=2 comp=0 doff=28 frames=2.
        let rec = pos0 as usize;
        file[rec + 4..rec + 6].copy_from_slice(&fw.to_le_bytes());
        file[rec + 6..rec + 8].copy_from_slice(&fh.to_le_bytes());
        file[rec + 8..rec + 10].copy_from_slice(&0u16.to_le_bytes());
        file[rec + 14..rec + 18].copy_from_slice(&28u32.to_le_bytes());
        file[rec + 20..rec + 22].copy_from_slice(&frames.to_le_bytes());
        // Frame table (relative to base).
        file[base..base + 4].copy_from_slice(&(frame0_rel as u32).to_le_bytes());
        file[base + 4..base + 8].copy_from_slice(&(frame1_rel as u32).to_le_bytes());
        // FrameData 0.
        file[frame0_at..frame0_at + 2].copy_from_slice(&fw.to_le_bytes());
        file[frame0_at + 2..frame0_at + 4].copy_from_slice(&fh.to_le_bytes());
        file[frame0_at + 4..frame0_at + 4 + frame0_data.len()].copy_from_slice(&frame0_data);
        // FrameData 1.
        file[frame1_at..frame1_at + 2].copy_from_slice(&fw.to_le_bytes());
        file[frame1_at + 2..frame1_at + 4].copy_from_slice(&fh.to_le_bytes());
        file[frame1_at + 4..frame1_at + 4 + frame1_data.len()].copy_from_slice(&frame1_data);

        let archive = TextureArchive::parse(&file).unwrap();
        let d0 = decode_frame(&file, &archive, 0, 0).unwrap();
        assert_eq!((d0.width, d0.height), (4, 2));
        assert_eq!(
            d0.pixels,
            vec![0, 0, 5, 6, 7, 0, 0, 9],
            "golden multiframe frame 0"
        );
        let d1 = decode_frame(&file, &archive, 0, 1).unwrap();
        assert_eq!(
            d1.pixels,
            vec![0, 0, 0, 0, 1, 2, 3, 4],
            "golden multiframe frame 1 (zeros row with extra byte)"
        );
        // Frame out of range → honest error.
        assert!(matches!(
            decode_frame(&file, &archive, 0, 2),
            Err(TextureError::FrameOutOfRange { frame: 2, .. })
        ));
    }

    /// RLE decoder: 1 raw row (RowEncoding=0) + 1 RLE row (0x8000).
    #[test]
    fn golden_rle_decode() {
        let count: u16 = 1;
        let header_len = TEXTURE_FILE_HEADER_LEN + count as usize * RECORD_HEADER_LEN;
        let pos0 = header_len as u32;
        let w: i16 = 4;
        let h: i16 = 2;
        // Layout: TextureRecord (28) + SpecialFrameHeaders (2×4=8) +
        // raw row0 (4) + RLE row1 (2 + 3 + 2 + 2 = 9).
        let headers_at = header_len + TEXTURE_RECORD_LEN;
        let row0_at = headers_at + 8;
        let row1_at = row0_at + 4;
        let file_len = row1_at + 2 + 3 + 2 + 2;
        let mut file = vec![0u8; file_len];
        file[0..2].copy_from_slice(&count.to_le_bytes());
        file[2..2 + 3].copy_from_slice(b"RLE");
        let at = TEXTURE_FILE_HEADER_LEN;
        file[at + 2..at + 6].copy_from_slice(&pos0.to_le_bytes());
        let rec = pos0 as usize;
        file[rec + 4..rec + 6].copy_from_slice(&w.to_le_bytes());
        file[rec + 6..rec + 8].copy_from_slice(&h.to_le_bytes());
        file[rec + 8..rec + 10].copy_from_slice(&COMPRESSION_RECORD_RLE.to_le_bytes());
        file[rec + 14..rec + 18].copy_from_slice(&28u32.to_le_bytes());
        file[rec + 20..rec + 22].copy_from_slice(&1u16.to_le_bytes()); // frames=1
        // SpecialFrameHeader[0]: RowOffset relative to the TextureRecord.
        file[headers_at..headers_at + 2]
            .copy_from_slice(&((row0_at - header_len) as i16).to_le_bytes());
        file[headers_at + 2..headers_at + 4].copy_from_slice(&ROW_ENCODING_RAW.to_le_bytes());
        // SpecialFrameHeader[1].
        file[headers_at + 4..headers_at + 6]
            .copy_from_slice(&((row1_at - header_len) as i16).to_le_bytes());
        file[headers_at + 6..headers_at + 8].copy_from_slice(&ROW_ENCODING_RLE.to_le_bytes());
        // Raw row 0.
        file[row0_at..row0_at + 4].copy_from_slice(&[20, 21, 22, 23]);
        // RLE row 1: target=4, probes: −2 → repeats 9 twice; 2 → copies 3,4.
        file[row1_at..row1_at + 2].copy_from_slice(&4i16.to_le_bytes());
        file[row1_at + 2..row1_at + 4].copy_from_slice(&(-2i16).to_le_bytes());
        file[row1_at + 4] = 9;
        file[row1_at + 5..row1_at + 7].copy_from_slice(&2i16.to_le_bytes());
        file[row1_at + 7..row1_at + 9].copy_from_slice(&[3, 4]);

        let archive = TextureArchive::parse(&file).unwrap();
        assert_eq!(
            archive.records[0].meta().unwrap().compression,
            COMPRESSION_RECORD_RLE
        );
        let decoded = decode_frame(&file, &archive, 0, 0).unwrap();
        assert_eq!(
            decoded.pixels,
            vec![20, 21, 22, 23, 9, 9, 3, 4],
            "golden RLE (raw + modified RLE)"
        );
    }

    /// Special records: solid (`position = 0`) and no image
    /// (`position` outside the file).
    #[test]
    fn special_solid_and_no_image_records() {
        // File: 3 records — solid, no image, solid.
        let count: u16 = 3;
        let header_len = TEXTURE_FILE_HEADER_LEN + count as usize * RECORD_HEADER_LEN;
        let mut file = vec![0u8; header_len];
        file[0..2].copy_from_slice(&count.to_le_bytes());
        file[2..2 + 6].copy_from_slice(b"SOFTST");
        // rec0: solid palette_index = 0x41 (unknown1 = 0x4100).
        let at0 = TEXTURE_FILE_HEADER_LEN;
        file[at0..at0 + 2].copy_from_slice(&0x4100u16.to_le_bytes());
        // rec1: position = file length (like TEXTURE.215/217).
        let at1 = at0 + RECORD_HEADER_LEN;
        file[at1 + 2..at1 + 6].copy_from_slice(&(header_len as u32).to_le_bytes());
        // rec2: solid palette_index = 0xFF (unknown1 = 0xFF00).
        let at2 = at1 + RECORD_HEADER_LEN;
        file[at2..at2 + 2].copy_from_slice(&0xFF00u16.to_le_bytes());

        let archive = TextureArchive::parse(&file).unwrap();
        assert_eq!(archive.records[0].solid_palette_index(), Some(0x41));
        assert_eq!(archive.records[2].solid_palette_index(), Some(0xFF));
        assert!(archive.records[1].solid_palette_index().is_none());
        assert!(matches!(
            archive.records[1].kind,
            RecordKind::NoImage
        ));
        // Decoding a record without image → honest error, never a panic.
        assert!(matches!(
            decode_frame(&file, &archive, 1, 0),
            Err(TextureError::NoImageData { index: 1 })
        ));
        assert!(matches!(
            decode_frame(&file, &archive, 0, 0),
            Err(TextureError::NoImageData { index: 0 })
        ));
    }

    /// Honest errors: truncations and limits (0 panics).
    #[test]
    fn errors_are_honest_and_panic_free() {
        // Truncated header.
        assert!(matches!(
            TextureArchive::parse(&[0u8; 8]),
            Err(TextureError::TooShort { .. })
        ));
        // RecordCount does not fit.
        let mut count_only = vec![0u8; TEXTURE_FILE_HEADER_LEN];
        count_only[0..2].copy_from_slice(&10u16.to_le_bytes());
        assert!(matches!(
            TextureArchive::parse(&count_only),
            Err(TextureError::HeaderOutOfBounds { .. })
        ));
        // NullValue ≠ 0 (record 0: RecordHeader at TEXTURE_FILE_HEADER_LEN).
        let (file, _, _) = synthetic_single_frame();
        let mut corrupt = file.clone();
        corrupt[TEXTURE_FILE_HEADER_LEN + 12] = 1;
        assert!(matches!(
            TextureArchive::parse(&corrupt),
            Err(TextureError::NonZeroNullValue { index: 0, .. })
        ));
        // Row outside the file (truncated data).
        let mut truncated = file.clone();
        truncated.truncate(TEXTURE_FILE_HEADER_LEN + 2 * RECORD_HEADER_LEN + 2 * TEXTURE_RECORD_LEN + 10);
        let archive = TextureArchive::parse(&truncated).unwrap();
        assert!(matches!(
            decode_frame(&truncated, &archive, 0, 0),
            Err(TextureError::RowOutOfBounds { .. })
        ));
        // FrameOutOfRange with a nonexistent index.
        assert!(matches!(
            decode_frame(&file, &archive, 99, 0),
            Err(TextureError::FrameOutOfRange { index: 99, .. })
        ));
    }

    /// Palettes: raw PalFile + ColFile with ColHeader + errors.
    #[test]
    fn palette_parse_and_rgba() {
        // Synthetic 768 B PalFile (own colors, 0 retail).
        let mut raw = vec![0u8; PAL_FILE_LEN];
        raw[0..3].copy_from_slice(&[1, 2, 3]);
        raw[3..6].copy_from_slice(&[10, 20, 30]);
        raw[255 * 3..255 * 3 + 3].copy_from_slice(&[200, 150, 100]);
        let pal = Palette::parse(&raw).unwrap();
        assert_eq!(pal.color(0), [1, 2, 3]);
        assert_eq!(pal.color(1), [10, 20, 30]);
        assert_eq!(pal.color(255), [200, 150, 100]);

        // ColFile: ColHeader (776 · 0xB123 · 0) + same ColorList.
        let mut col = vec![0u8; COL_FILE_LEN];
        col[0..4].copy_from_slice(&(COL_FILE_LEN as u32).to_le_bytes());
        col[4..6].copy_from_slice(&COL_FILE_FORMAT_MAJOR.to_le_bytes());
        col[8..8 + PAL_FILE_LEN].copy_from_slice(&raw);
        let pal_col = Palette::parse(&col).unwrap();
        assert_eq!(pal_col, pal, "ColFile and PalFile decode the same");

        // RGBA: index 0 transparent, the rest opaque.
        let rgba = pal.to_rgba(&[0, 1, 255]);
        assert_eq!(
            rgba,
            vec![1, 2, 3, 0, 10, 20, 30, 255, 200, 150, 100, 255],
            "golden RGBA (index 0 with alpha 0)"
        );

        // Corrupt ColHeader and invalid lengths.
        let mut bad_magic = col.clone();
        bad_magic[4] = 0x00;
        assert!(matches!(
            Palette::parse(&bad_magic),
            Err(TextureError::BadColHeader { .. })
        ));
        assert!(matches!(
            Palette::parse(&[0u8; 100]),
            Err(TextureError::PaletteLength { len: 100 })
        ));
    }
}
