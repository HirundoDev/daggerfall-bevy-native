//! Cinematic video of Daggerfall — **`.VID`** (interleaved audio + video) and
//! **`.FLC`** (Autodesk FLIC) — M34 §6 (`m34-e2e-integration-presentation-spec
//! -CR-5243af63.md:257-268`, GV-07/GV-08 `:289-313`).
//!
//! *Clean-room* from the format itself + the DFU readers, which are used here
//! only as an **executable spec** (0 bytes of DFU source copied, 0 packed
//! assets — INV-L99-6): every field below is cited to its line.
//!
//! ## `.VID` container (`DFU@0038a1e6:Assets/Scripts/API/VidFile.cs`)
//!
//! Header at 0 (`:204-221`): `"VID"` · `u16 unknown1` (512) · `u16 frames` ·
//! `u16 width` · `u16 height` · `u16 globalDelay` · `u16 unknown2` (14), then a
//! **mandatory** palette block (`:223-240`): `u8 0x02` + 768 bytes of 6-bit RGB
//! scaled by 4 on display (`paletteMultiplier = 4`, `:33,:235-237`).
//! After that the file is a stream of blocks, each prefixed by a 1-byte type
//! (`VidBlockTypes`, `:351-361`), where video and audio frames **interleave**:
//!
//! | id | block | payload |
//! |:--:|:---|:---|
//! | `0` | `Null` | none — rejected by the player (`:130-137`) |
//! | `1` | `Video_IncrementalFrame` | `u16 delay` + partial RLE from pixel 0 |
//! | `2` | `Palette` | 768 bytes (skipped: `:145-148`) |
//! | `3` | `Video_StartFrame` | `u16 delay` + **full** RLE over `width*height` |
//! | `4` | `Video_IncrementalRowOffsetFrame` | `u16 delay` + `u16 row` + partial RLE from `row*width` |
//! | `20` | `EndOfFile` | none |
//! | `124` | `Audio_StartFrame` | `u16 unk` + `u8 playbackRate` (166 ⇒ 11025 Hz) + `u16 len` + samples |
//! | `125` | `Audio_IncrementalFrame` | `u16 len` + samples |
//!
//! RLE (`:287-333`): opcode `>= 0x80` ⇒ `count = opcode & 0x7F`; a *full* frame
//! then reads **one** index byte and writes it `count` times, while a *partial*
//! frame **skips** `count` pixels (they keep the previous canvas). Opcode
//! `1..=0x7F` ⇒ that many literal index bytes. Opcode `0` ends the block early
//! (a partial frame may stop before the buffer is full: `:325-328`). A full
//! frame must fill the buffer exactly (`:300`).
//!
//! **Measured on this machine (2026-10-26, `$ARENA2`, 17 files, 9 244 blocks):**
//!
//! ```text
//! VID block histogram: {0:84 null, 1:2324 incr, 3:17 start, 4:2320 rowoffset,
//!                       20:17 eof, 124:19 audio_start, 125:4463 audio_incr}
//! 17/17 files: header frames == decoded video blocks (4 661 frames total);
//! 17/17 full frames fill width*height exactly; 17/17 playbackRate == 166;
//! 17/17 end of file block present at byte == file length; palette raw max 63.
//! dimensions: 320x200 (16 files) and 256x200 (DAG2.VID).
//! ```
//!
//! ## `.FLC` container (`DFU@0038a1e6:Assets/Scripts/API/FlcFile.cs`)
//!
//! 128-byte header (`:172-206`): `u32 size` · `u16 id` (`0xAF12` = FLIC,
//! `0xAF11` = FLI, `:580-584`) · `u16 frames` · `u16 width` · `u16 height` ·
//! `u16 depth` · `u16 flags` · `u32 frameDelayMs` … `u32 frame1Offset` (`@80`).
//! Frame blocks start at `frame1Offset`: `u32 size` · `u16 type`
//! (`FRAME_TYPE = -3590`, `:622`) · `u16 subchunks` · 8 reserved bytes, then the
//! sub-chunks `u32 size` · `u16 type` (`ChunkType`, `:592-618`).
//!
//! The **first** frame block is the *ring* frame (`:109-141`,
//! `BufferNextFrame(0)`) and is not displayed; it paints the base image and the
//! palette. Displayed frames are the `frames` blocks after it, each a delta over
//! the previous canvas — exactly the loop of `:208-297`.
//!
//! **Measured (16 files, 256 sub-chunks, 240 frame blocks, 224 displayed frames):**
//!
//! ```text
//! FLC chunk histogram: {4 COLOR_256:16, 7 DELTA_FLC:224, 15 BYTE_RUN:16,
//!                       18 PSTAMP:16}   (16 ring frames: PSTAMP+COLOR_256+BYTE_RUN)
//! 16/16 files: header frames == displayed frames (14 each), 320x200, depth 8,
//! COLOR_256 = 1 packet / skip 0 / 256 colours, palette max 255,
//! 0 DELTA_FLC line-skip opcodes, 0 zero-length BYTE_RUN packets,
//! last frame ends exactly at file length; 1 trailing pad byte inside
//! BYTE_RUN on 6 files (DFU never validates chunk ends: `:225-296`).
//! ```
//!
//! `COLOR_64 (11)`, `DELTA_FLI (12)`, `BLACK (13)` and `FLI_COPY (16)` are
//! **not implemented**: they do not appear in a single one of the 4 885 frames
//! measured, so decoding them would be untested invention. They return
//! [`VideoError::UnsupportedChunk`], never a silent skip.
//!
//! ## Clock (GV-07/GV-08)
//!
//! [`DIVISOR`] is the 8-bit → float sample scale of the DFU player
//! (`DFU@0038a1e6:Assets/Scripts/Game/UserInterface/DaggerfallVideo.cs:106,110`:
//! `data[pos++] = (AudioBuffer[i] - 128) * divisor`): it is **dimensionless**,
//! `1/128 = 0.0078125`, and it is *not* a time quantum — §8 GV-08 labels it
//! `s`, which the source does not support. [`CLIP_QUEUE_LENGTH`] is the audio
//! clip queue (`:28`). A `.VID` frame lasts `audioBytes / VID_SAMPLE_RATE`
//! seconds with a floor of [`MIN_FRAME_DELAY_SECONDS`] (`VidFile.cs:184-193`).

use std::collections::BTreeMap;
use std::fmt;

/// 8-bit → float sample scale of the `.VID` player: `(sample - 128) * DIVISOR`.
///
/// `DFU@0038a1e6:Assets/Scripts/Game/UserInterface/DaggerfallVideo.cs:106,110`.
/// Numeric value of GV-08 (`1/128 = 0.0078125`); **dimensionless**, not seconds.
pub const DIVISOR: f64 = 1.0 / 128.0;
/// Audio clips the player keeps queued (GV-07).
///
/// `DFU@0038a1e6:Assets/Scripts/Game/UserInterface/DaggerfallVideo.cs:28,34-35`.
pub const CLIP_QUEUE_LENGTH: usize = 2;
/// Sample rate of `.VID` audio (`DFU@0038a1e6:.../API/VidFile.cs:29`).
pub const VID_SAMPLE_RATE: u32 = 11_025;
/// Playback-rate byte of `Audio_StartFrame` that means [`VID_SAMPLE_RATE`]
/// (`DFU@0038a1e6:.../API/VidFile.cs:242-250` rejects any other value).
pub const VID_PLAYBACK_RATE: u8 = 166;
/// `.VID` palettes are stored 6-bit and scaled by this factor on display
/// (`DFU@0038a1e6:.../API/VidFile.cs:33,235-237`).
pub const VID_PALETTE_MULTIPLIER: u8 = 4;
/// Floor of a `.VID` frame delay: 740 samples at [`VID_SAMPLE_RATE`]
/// (`DFU@0038a1e6:.../API/VidFile.cs:34,184-193`).
pub const MIN_FRAME_DELAY_SECONDS: f64 = 740.0 / VID_SAMPLE_RATE as f64;
/// Bytes of one palette: 256 entries × 3 channels.
pub const PALETTE_BYTES: usize = 768;

/// Container actually decoded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VideoFormat {
    /// Daggerfall `.VID`: interleaved audio + video blocks.
    Vid,
    /// Autodesk FLIC (`0xAF12`).
    Flc,
    /// Autodesk FLI (`0xAF11`) — same frame layout as [`VideoFormat::Flc`].
    Fli,
}

impl VideoFormat {
    /// Three-letter label used by the probes (`VID`/`FLC`/`FLI`).
    pub fn label(self) -> &'static str {
        match self {
            VideoFormat::Vid => "VID",
            VideoFormat::Flc => "FLC",
            VideoFormat::Fli => "FLI",
        }
    }
}

/// One decoded frame: palette indices + the palette in force.
///
/// `indices.len() == width * height`, row-major, **row 0 = top**. DFU stores the
/// same buffer bottom-up because it uploads it to a Unity texture whose origin
/// is bottom-left (`DFU@0038a1e6:.../API/FlcFile.cs:462-475`); the flip is a
/// renderer concern and is deliberately not baked in here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    /// `width * height` palette indices.
    pub indices: Vec<u8>,
    /// 256 RGB triplets, 8-bit per channel (`.VID` values already ×4).
    pub palette: [u8; PALETTE_BYTES],
}

impl Frame {
    /// Number of palette indices of this frame.
    pub fn len(&self) -> usize {
        self.indices.len()
    }

    /// `true` if the frame has no pixels (only possible for a 0×0 header, which
    /// the decoders reject).
    pub fn is_empty(&self) -> bool {
        self.indices.is_empty()
    }

    /// Distinct palette indices actually used by this frame.
    pub fn distinct_colors(&self) -> usize {
        let mut seen = [false; 256];
        for index in &self.indices {
            seen[usize::from(*index)] = true;
        }
        seen.iter().filter(|used| **used).count()
    }
}

/// `.VID` header (`DFU@0038a1e6:.../API/VidFile.cs:204-221`, `:363-374`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VidHeader {
    /// `u16` at 3 — always 512 in retail.
    pub unknown1: u16,
    /// Displayed video frames declared by the file.
    pub frames: u16,
    /// Frame width (256 or 320 in retail).
    pub width: u16,
    /// Frame height (200 in retail).
    pub height: u16,
    /// Global delay field (`u16` at 11) — not the per-frame clock, see the
    /// module docs.
    pub global_delay: u16,
    /// `u16` at 13 — always 14 in retail.
    pub unknown2: u16,
}

/// `.FLC`/`.FLI` header (`DFU@0038a1e6:.../API/FlcFile.cs:172-206`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FlicHeader {
    /// Declared file size (`u32` at 0).
    pub file_size: u32,
    /// `0xAF12` (FLIC) or `0xAF11` (FLI), as read (`i16` at 4).
    pub file_id: i16,
    /// Displayed frames — the ring frame is **not** counted (`:566`).
    pub frames: u16,
    /// Frame width.
    pub width: u16,
    /// Frame height.
    pub height: u16,
    /// Bits per pixel (8 in retail).
    pub depth: u16,
    /// Flag word (`u16` at 14).
    pub flags: u16,
    /// Frame delay in `1/1000 s` (`u32` at 16; FLI uses `1/70 s`, `:586-590`).
    pub frame_delay: u32,
    /// Absolute offset of the ring frame (`u32` at 80).
    pub frame1_offset: u32,
}

/// Audio frames seen in a `.VID` and the rate they declare.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AudioSummary {
    /// `Audio_StartFrame` (124) blocks.
    pub start_blocks: usize,
    /// `Audio_IncrementalFrame` (125) blocks.
    pub incremental_blocks: usize,
    /// Sample bytes carried by all audio blocks.
    pub bytes: usize,
    /// Playback-rate byte of the first `Audio_StartFrame`, if any.
    pub playback_rate: Option<u8>,
}

impl AudioSummary {
    /// Total audio blocks (`start + incremental`).
    pub fn blocks(&self) -> usize {
        self.start_blocks + self.incremental_blocks
    }
}

/// Everything one decode produced: the frames plus the **measured** structure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecodedVideo {
    /// Container that was decoded.
    pub format: VideoFormat,
    /// Frame width from the header.
    pub width: u16,
    /// Frame height from the header.
    pub height: u16,
    /// Displayed frames, in playback order.
    pub frames: Vec<Frame>,
    /// Block/chunk type → occurrences (`key` is the raw id of the container).
    pub chunks: BTreeMap<u16, usize>,
    /// Audio blocks of a `.VID` (empty for FLIC).
    pub audio: AudioSummary,
    /// Bytes declared by a chunk but not consumed by its decoder (repair-free
    /// slack measured, never skipped silently: it is reported).
    pub padding_bytes: usize,
}

impl DecodedVideo {
    /// Pixels of one frame (`width * height`).
    pub fn pixels_per_frame(&self) -> usize {
        usize::from(self.width) * usize::from(self.height)
    }

    /// Total palette indices decoded (`frames * width * height`).
    pub fn total_indices(&self) -> usize {
        self.pixels_per_frame() * self.frames.len()
    }
}

/// Typed failures — no silent skip, no partial frame reported as success.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VideoError {
    /// The buffer is shorter than the structure being read.
    TooShort {
        /// Offset that could not be read.
        offset: usize,
        /// Bytes needed from `offset`.
        need: usize,
        /// Bytes actually available.
        len: usize,
    },
    /// Neither `VID` nor `AF11`/`AF12`: not a container this module decodes.
    UnknownFormat([u8; 4]),
    /// `.VID` magic is not `VID` (`VidFile.cs:204-208`).
    BadVidMagic([u8; 3]),
    /// FLIC file id is neither FLI nor FLIC (`FlcFile.cs:580-584`).
    BadFlicId(i16),
    /// Width or height is 0 — no frame could ever be complete.
    ZeroDimensions,
    /// The mandatory palette is absent (`.VID` block 2 after the header,
    /// `VidFile.cs:223-227`; FLIC `COLOR_256` chunk, `FlcFile.cs:307-339`).
    PaletteMissing,
    /// Container block/chunk type with no measured occurrence and no
    /// implementation. `(kind, offset)`. Never a silent skip.
    UnsupportedChunk(u16, usize),
    /// FLIC frame header type is not `FRAME_TYPE = -3590` (`FlcFile.cs:622`).
    BadFrameType {
        /// Type read.
        kind: i16,
        /// Offset of the frame header.
        offset: usize,
    },
    /// A length field is inconsistent with what the decoder needs.
    BadLength {
        /// Offset of the length field (or of the chunk that declared it).
        offset: usize,
        /// Bytes declared.
        declared: usize,
        /// Bytes the decoder consumed.
        consumed: usize,
    },
    /// A pixel run left the frame buffer.
    PixelOverflow {
        /// Stream offset of the offending opcode.
        offset: usize,
        /// Pixel index the run would reach.
        pixel: usize,
        /// Pixels in the frame.
        total: usize,
    },
    /// A **full** `.VID` frame ended before filling `width*height` pixels
    /// (`VidFile.cs:300`).
    IncompleteFrame {
        /// Stream offset where the block ended.
        offset: usize,
        /// Pixels written.
        pixel: usize,
        /// Pixels in the frame.
        total: usize,
    },
    /// `.VID` header `frames` differs from the decoded video blocks.
    FrameCountMismatch {
        /// `u16` of the header.
        declared: usize,
        /// Video blocks actually decoded.
        decoded: usize,
    },
    /// `.VID` stream ended without the `EndOfFile` (20) block.
    MissingEndOfFile,
    /// Bytes left after the container's last declared structure.
    TrailingBytes {
        /// First unreferenced byte.
        offset: usize,
        /// How many bytes are left.
        len: usize,
    },
    /// `Audio_StartFrame` declares a rate other than 166 (`VidFile.cs:249-250`).
    UnknownPlaybackRate {
        /// Rate read.
        rate: u8,
        /// Offset of the rate byte.
        offset: usize,
    },
    /// `BYTE_RUN` packet of length 0 — DFU would spin on it (`FlcFile.cs:341-368`).
    ZeroRunLength {
        /// Stream offset of the packet.
        offset: usize,
        /// Row being decoded.
        row: usize,
    },
    /// `DELTA_FLC` opcode outside the three defined forms (`FlcFile.cs:370-419`).
    BadDeltaOpcode {
        /// Opcode read.
        opcode: i16,
        /// Stream offset of the opcode.
        offset: usize,
    },
}

impl fmt::Display for VideoError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            VideoError::TooShort { offset, need, len } => {
                write!(f, "truncated at {offset}: need {need} bytes, have {len}")
            }
            VideoError::UnknownFormat(magic) => {
                write!(f, "not a VID/FLC container: magic {magic:02X?}")
            }
            VideoError::BadVidMagic(magic) => write!(f, "bad VID magic {magic:02X?}"),
            VideoError::BadFlicId(id) => write!(f, "bad FLIC file id {id}"),
            VideoError::ZeroDimensions => write!(f, "zero width/height"),
            VideoError::PaletteMissing => write!(f, "palette missing"),
            VideoError::UnsupportedChunk(kind, offset) => {
                write!(f, "unsupported chunk type {kind} at {offset}")
            }
            VideoError::BadFrameType { kind, offset } => {
                write!(f, "frame header type {kind} at {offset} (expected -3590)")
            }
            VideoError::BadLength {
                offset,
                declared,
                consumed,
            } => write!(
                f,
                "length mismatch at {offset}: declared {declared}, consumed {consumed}"
            ),
            VideoError::PixelOverflow {
                offset,
                pixel,
                total,
            } => write!(f, "run at {offset} reaches pixel {pixel} > {total}"),
            VideoError::IncompleteFrame {
                offset,
                pixel,
                total,
            } => write!(f, "full frame at {offset} stopped at {pixel}/{total} pixels"),
            VideoError::FrameCountMismatch { declared, decoded } => {
                write!(f, "header frames {declared} != decoded {decoded}")
            }
            VideoError::MissingEndOfFile => write!(f, "VID stream without EndOfFile block"),
            VideoError::TrailingBytes { offset, len } => {
                write!(f, "{len} trailing bytes from {offset}")
            }
            VideoError::UnknownPlaybackRate { rate, offset } => {
                write!(f, "audio playback rate {rate} at {offset} (expected 166)")
            }
            VideoError::ZeroRunLength { offset, row } => {
                write!(f, "BYTE_RUN zero-length packet at {offset} row {row}")
            }
            VideoError::BadDeltaOpcode { opcode, offset } => {
                write!(f, "DELTA_FLC opcode {opcode:#06x} at {offset}")
            }
        }
    }
}

impl std::error::Error for VideoError {}

/// Decodes **every** frame of a `.VID`/`.FLC`/`.FLI` buffer.
///
/// The container is detected from the magic. `frames.len()` always equals the
/// frame count declared by the header (`VideoError::FrameCountMismatch`
/// otherwise) and every frame carries `width * height` indices plus a 768-byte
/// palette — the invariants the M34 probe asserts per file.
pub fn decode_frames(bytes: &[u8]) -> Result<Vec<Frame>, VideoError> {
    decode(bytes).map(|decoded| decoded.frames)
}

/// Decodes a container and returns the frames **plus** the measured structure
/// (chunk histogram, audio summary, chunk padding).
pub fn decode(bytes: &[u8]) -> Result<DecodedVideo, VideoError> {
    if bytes.len() < 4 {
        return Err(VideoError::TooShort {
            offset: 0,
            need: 4,
            len: bytes.len(),
        });
    }
    if &bytes[0..3] == b"VID" {
        return decode_vid(bytes);
    }
    // `i16 file id` at offset 4: 0xAF12 = FLIC, 0xAF11 = FLI (FlcFile.cs:580-584).
    if bytes.len() >= 6 {
        match u16::from_le_bytes([bytes[4], bytes[5]]) {
            0xAF12 => return decode_flic(bytes, VideoFormat::Flc),
            0xAF11 => return decode_flic(bytes, VideoFormat::Fli),
            _ => {}
        }
    }
    let mut magic = [0u8; 4];
    magic.copy_from_slice(&bytes[0..4]);
    Err(VideoError::UnknownFormat(magic))
}

// ── cursor ─────────────────────────────────────────────────────────────────

/// Bounds-checked little-endian reader: every access either succeeds or returns
/// [`VideoError::TooShort`]. 0 `unwrap()` on parse paths.
struct Cursor<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Cursor<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Cursor { bytes, pos: 0 }
    }

    fn at(bytes: &'a [u8], pos: usize) -> Self {
        Cursor { bytes, pos }
    }

    fn remaining(&self) -> usize {
        self.bytes.len().saturating_sub(self.pos)
    }

    fn take(&mut self, need: usize) -> Result<&'a [u8], VideoError> {
        let end = self.pos.checked_add(need).ok_or(VideoError::TooShort {
            offset: self.pos,
            need,
            len: self.bytes.len(),
        })?;
        if end > self.bytes.len() {
            return Err(VideoError::TooShort {
                offset: self.pos,
                need,
                len: self.bytes.len(),
            });
        }
        let slice = &self.bytes[self.pos..end];
        self.pos = end;
        Ok(slice)
    }

    fn u8(&mut self) -> Result<u8, VideoError> {
        Ok(self.take(1)?[0])
    }

    fn i8(&mut self) -> Result<i8, VideoError> {
        Ok(self.u8()? as i8)
    }

    fn u16(&mut self) -> Result<u16, VideoError> {
        let slice = self.take(2)?;
        Ok(u16::from_le_bytes([slice[0], slice[1]]))
    }

    fn i16(&mut self) -> Result<i16, VideoError> {
        Ok(self.u16()? as i16)
    }

    fn u32(&mut self) -> Result<u32, VideoError> {
        let slice = self.take(4)?;
        Ok(u32::from_le_bytes([slice[0], slice[1], slice[2], slice[3]]))
    }
}

// ── .VID ───────────────────────────────────────────────────────────────────

/// `.VID` block types (`DFU@0038a1e6:.../API/VidFile.cs:351-361`).
pub const VID_NULL: u8 = 0;
/// `Video_IncrementalFrame`: delay + RLE continuing the canvas from pixel 0.
pub const VID_VIDEO_INCREMENTAL: u8 = 1;
/// Full 768-byte palette block.
pub const VID_PALETTE: u8 = 2;
/// `Video_StartFrame`: delay + RLE that repaints the whole canvas.
pub const VID_VIDEO_START: u8 = 3;
/// `Video_RowOffsetFrame`: delay + first row + RLE from `row * width`.
pub const VID_VIDEO_ROW_OFFSET: u8 = 4;
/// End of the container; bytes after it are rejected (`TrailingBytes`).
pub const VID_END_OF_FILE: u8 = 20;
/// `Audio_StartFrame`: unknown + playback rate + payload.
pub const VID_AUDIO_START: u8 = 124;
/// `Audio_IncrementalFrame`: payload continuing the previous audio block.
pub const VID_AUDIO_INCREMENTAL: u8 = 125;

fn decode_vid(bytes: &[u8]) -> Result<DecodedVideo, VideoError> {
    let mut magic = [0u8; 3];
    magic.copy_from_slice(&bytes[0..3]);
    if magic != *b"VID" {
        return Err(VideoError::BadVidMagic(magic));
    }
    let mut cursor = Cursor::at(bytes, 3);
    let header = VidHeader {
        unknown1: cursor.u16()?,
        frames: cursor.u16()?,
        width: cursor.u16()?,
        height: cursor.u16()?,
        global_delay: cursor.u16()?,
        unknown2: cursor.u16()?,
    };
    if header.width == 0 || header.height == 0 {
        return Err(VideoError::ZeroDimensions);
    }
    let total = usize::from(header.width) * usize::from(header.height);

    // Mandatory palette block right after the header (VidFile.cs:223-240).
    if cursor.u8()? != VID_PALETTE {
        return Err(VideoError::PaletteMissing);
    }
    let raw = cursor.take(PALETTE_BYTES)?;
    let palette = vid_palette(raw);

    let mut canvas = vec![0u8; total];
    let mut frames: Vec<Frame> = Vec::with_capacity(usize::from(header.frames));
    let mut chunks: BTreeMap<u16, usize> = BTreeMap::new();
    let mut audio = AudioSummary::default();
    let mut seen_end = false;

    loop {
        if cursor.remaining() == 0 {
            break;
        }
        let block_offset = cursor.pos;
        let block = cursor.u8()?;
        *chunks.entry(u16::from(block)).or_default() += 1;
        match block {
            // Null block: the player discards it (VidFile.cs:130-137).
            VID_NULL => {}
            // Extra palette mid-stream: the player skips 768 bytes (VidFile.cs:145-148).
            VID_PALETTE => {
                cursor.take(PALETTE_BYTES)?;
            }
            VID_VIDEO_START | VID_VIDEO_INCREMENTAL | VID_VIDEO_ROW_OFFSET => {
                let _delay = cursor.u16()?;
                let start = if block == VID_VIDEO_ROW_OFFSET {
                    let row = usize::from(cursor.u16()?);
                    if row >= usize::from(header.height) {
                        return Err(VideoError::PixelOverflow {
                            offset: block_offset,
                            pixel: row * usize::from(header.width),
                            total,
                        });
                    }
                    row * usize::from(header.width)
                } else {
                    0
                };
                let full = block == VID_VIDEO_START;
                let end = decode_vid_rle(&mut cursor, &mut canvas, start, total, full)?;
                if full && end != total {
                    return Err(VideoError::IncompleteFrame {
                        offset: cursor.pos,
                        pixel: end,
                        total,
                    });
                }
                frames.push(Frame {
                    indices: canvas.clone(),
                    palette,
                });
            }
            VID_AUDIO_START => {
                let _unknown = cursor.u16()?;
                let rate_offset = cursor.pos;
                let rate = cursor.u8()?;
                if rate != VID_PLAYBACK_RATE {
                    return Err(VideoError::UnknownPlaybackRate {
                        rate,
                        offset: rate_offset,
                    });
                }
                let len = usize::from(cursor.u16()?);
                cursor.take(len)?;
                audio.start_blocks += 1;
                audio.bytes += len;
                audio.playback_rate.get_or_insert(rate);
            }
            VID_AUDIO_INCREMENTAL => {
                let len = usize::from(cursor.u16()?);
                cursor.take(len)?;
                audio.incremental_blocks += 1;
                audio.bytes += len;
            }
            VID_END_OF_FILE => {
                seen_end = true;
                break;
            }
            other => return Err(VideoError::UnsupportedChunk(u16::from(other), block_offset)),
        }
    }

    if !seen_end {
        return Err(VideoError::MissingEndOfFile);
    }
    if cursor.pos != bytes.len() {
        return Err(VideoError::TrailingBytes {
            offset: cursor.pos,
            len: bytes.len() - cursor.pos,
        });
    }
    if frames.len() != usize::from(header.frames) {
        return Err(VideoError::FrameCountMismatch {
            declared: usize::from(header.frames),
            decoded: frames.len(),
        });
    }

    Ok(DecodedVideo {
        format: VideoFormat::Vid,
        width: header.width,
        height: header.height,
        frames,
        chunks,
        audio,
        padding_bytes: 0,
    })
}

/// `.VID` palette: 6-bit entries scaled by [`VID_PALETTE_MULTIPLIER`]
/// (`VidFile.cs:33,235-237`).
fn vid_palette(raw: &[u8]) -> [u8; PALETTE_BYTES] {
    let mut palette = [0u8; PALETTE_BYTES];
    for (out, byte) in palette.iter_mut().zip(raw.iter()) {
        *out = byte.saturating_mul(VID_PALETTE_MULTIPLIER);
    }
    palette
}

/// Video RLE of a `.VID` block (`VidFile.cs:287-333`).
///
/// Returns the pixel index the block reached. A **full** frame writes indices;
/// a **partial** frame only writes literal runs and *skips* the run-encoded
/// ones, so the previous canvas survives (that is the inter-frame delta).
fn decode_vid_rle(
    cursor: &mut Cursor<'_>,
    canvas: &mut [u8],
    start: usize,
    total: usize,
    full: bool,
) -> Result<usize, VideoError> {
    let mut pixel = start;
    while pixel < total {
        let opcode_offset = cursor.pos;
        let opcode = cursor.u8()?;
        match opcode {
            // Premature end of the block (VidFile.cs:311-314 / :331-334).
            0 => break,
            1..=0x7f => {
                let count = usize::from(opcode);
                if pixel + count > total {
                    return Err(VideoError::PixelOverflow {
                        offset: opcode_offset,
                        pixel: pixel + count,
                        total,
                    });
                }
                // Literal run: indexed bytes are written over the canvas in
                // both frame kinds (VidFile.cs:307-310 / :326-330).
                let indices = cursor.take(count)?;
                canvas[pixel..pixel + count].copy_from_slice(indices);
                pixel += count;
            }
            _ => {
                let count = usize::from(opcode & 0x7f);
                if pixel + count > total {
                    return Err(VideoError::PixelOverflow {
                        offset: opcode_offset,
                        pixel: pixel + count,
                        total,
                    });
                }
                if full {
                    let index = cursor.u8()?;
                    canvas[pixel..pixel + count].fill(index);
                }
                // Partial frames: a run header carries no data byte and the
                // pixels keep the previous canvas (VidFile.cs:321-325).
                pixel += count;
            }
        }
    }
    Ok(pixel)
}

// ── .FLC / .FLI ────────────────────────────────────────────────────────────

/// FLIC sub-chunk types (`DFU@0038a1e6:.../API/FlcFile.cs:592-618`).
const FLC_COLOR_256: u16 = 4;
const FLC_DELTA_FLC: u16 = 7;
const FLC_BYTE_RUN: u16 = 15;
const FLC_PSTAMP: u16 = 18;
/// Frame header type (`FlcFile.cs:622`).
const FLC_FRAME_TYPE: i16 = -3590;
/// `size i32` + `type i16` + `subchunks u16` + 8 reserved (`FlcFile.cs:26,193-206`).
const FLC_FRAME_HEADER_SIZE: usize = 16;
/// `size i32` + `type i16` (`FlcFile.cs:27`).
const FLC_CHUNK_HEADER_SIZE: usize = 6;
/// Slack a chunk may declare beyond what its decoder consumes and still be
/// accepted: measured 0 or 1 byte, only inside `BYTE_RUN` (see module docs).
const FLC_MAX_PADDING: usize = 8;

fn decode_flic(bytes: &[u8], format: VideoFormat) -> Result<DecodedVideo, VideoError> {
    let mut header_cursor = Cursor::new(bytes);
    let file_size = header_cursor.u32()?;
    let file_id = header_cursor.i16()?;
    if file_id != 0xAF12u16 as i16 && file_id != 0xAF11u16 as i16 {
        return Err(VideoError::BadFlicId(file_id));
    }
    let frames_declared = header_cursor.u16()?;
    let header = FlicHeader {
        file_size,
        file_id,
        frames: frames_declared,
        width: header_cursor.u16()?,
        height: header_cursor.u16()?,
        depth: header_cursor.u16()?,
        flags: header_cursor.u16()?,
        frame_delay: header_cursor.u32()?,
        frame1_offset: {
            let mut cursor = Cursor::at(bytes, 80);
            cursor.u32()?
        },
    };
    if header.width == 0 || header.height == 0 {
        return Err(VideoError::ZeroDimensions);
    }
    let total = usize::from(header.width) * usize::from(header.height);
    let width = usize::from(header.width);
    let height = usize::from(header.height);

    let mut canvas = vec![0u8; total];
    let mut palette = [0u8; PALETTE_BYTES];
    let mut palette_seen = false;
    let mut chunks: BTreeMap<u16, usize> = BTreeMap::new();
    let mut frames: Vec<Frame> = Vec::with_capacity(usize::from(header.frames));
    let mut padding_bytes = 0usize;
    let mut pos = header.frame1_offset as usize;
    let mut block_index = 0usize;

    while frames.len() < usize::from(header.frames) {
        let frame_offset = pos;
        let mut cursor = Cursor::at(bytes, pos);
        let frame_size = cursor.u32()? as usize;
        let frame_type = cursor.i16()?;
        let subchunks = cursor.u16()?;
        cursor.take(8)?;
        if frame_type != FLC_FRAME_TYPE {
            return Err(VideoError::BadFrameType {
                kind: frame_type,
                offset: frame_offset,
            });
        }
        if frame_size < FLC_FRAME_HEADER_SIZE {
            return Err(VideoError::BadLength {
                offset: frame_offset,
                declared: frame_size,
                consumed: FLC_FRAME_HEADER_SIZE,
            });
        }
        for _ in 0..subchunks {
            let chunk_offset = cursor.pos;
            let chunk_size = cursor.u32()? as usize;
            let chunk_type = cursor.u16()?;
            *chunks.entry(chunk_type).or_default() += 1;
            if chunk_size < FLC_CHUNK_HEADER_SIZE {
                return Err(VideoError::BadLength {
                    offset: chunk_offset,
                    declared: chunk_size,
                    consumed: FLC_CHUNK_HEADER_SIZE,
                });
            }
            match chunk_type {
                FLC_PSTAMP => {
                    // Thumbnail: the player skips it whole (FlcFile.cs:249-252).
                    cursor.take(chunk_size - FLC_CHUNK_HEADER_SIZE)?;
                }
                FLC_COLOR_256 => {
                    palette_seen = true;
                    decode_flc_color(&mut cursor, &mut palette)?;
                }
                FLC_BYTE_RUN => decode_flc_byte_run(&mut cursor, &mut canvas, width, height)?,
                FLC_DELTA_FLC => {
                    decode_flc_delta(&mut cursor, &mut canvas, width, height)?
                }
                other => return Err(VideoError::UnsupportedChunk(other, chunk_offset)),
            }
            let consumed = cursor.pos - chunk_offset;
            if consumed > chunk_size {
                return Err(VideoError::BadLength {
                    offset: chunk_offset,
                    declared: chunk_size,
                    consumed,
                });
            }
            let slack = chunk_size - consumed;
            if slack > FLC_MAX_PADDING {
                return Err(VideoError::BadLength {
                    offset: chunk_offset,
                    declared: chunk_size,
                    consumed,
                });
            }
            padding_bytes += slack;
            // Chunk size is the authority for where the next chunk starts.
            cursor.pos = chunk_offset + chunk_size;
        }
        // The ring frame (block 0) paints the base canvas and is never played
        // (FlcFile.cs:109-141, BufferNextFrame(0)).
        if block_index > 0 {
            frames.push(Frame {
                indices: canvas.clone(),
                palette,
            });
        }
        pos = frame_offset
            .checked_add(frame_size)
            .ok_or(VideoError::BadLength {
                offset: frame_offset,
                declared: frame_size,
                consumed: 0,
            })?;
        block_index += 1;
    }

    if !palette_seen {
        return Err(VideoError::PaletteMissing);
    }
    if pos > bytes.len() {
        return Err(VideoError::BadLength {
            offset: pos,
            declared: pos,
            consumed: bytes.len(),
        });
    }
    if frames.len() != usize::from(header.frames) {
        return Err(VideoError::FrameCountMismatch {
            declared: usize::from(header.frames),
            decoded: frames.len(),
        });
    }

    Ok(DecodedVideo {
        format,
        width: header.width,
        height: header.height,
        frames,
        chunks,
        audio: AudioSummary::default(),
        padding_bytes,
    })
}

/// `COLOR_256` (4): `u16 packets`, then per packet `u8 skip` (colours) +
/// `u8 count` (0 ⇒ 256) + RGB triplets (`FlcFile.cs:307-339`).
fn decode_flc_color(
    cursor: &mut Cursor<'_>,
    palette: &mut [u8; PALETTE_BYTES],
) -> Result<(), VideoError> {
    let packets = cursor.u16()?;
    let mut color_index = 0usize;
    for _ in 0..packets {
        let skip = usize::from(cursor.u8()?);
        // DFU skips `skip * 3` bytes of the stream but does **not** advance its
        // colour index (`FlcFile.cs:318-319`); retail files carry skip = 0 in
        // all 16 palettes, so both readings coincide. The faithful one wins.
        cursor.take(skip * 3)?;
        let count = cursor.u8()?;
        let colors = if count == 0 { 256 } else { usize::from(count) };
        if color_index + colors > 256 {
            return Err(VideoError::BadLength {
                offset: cursor.pos,
                declared: color_index + colors,
                consumed: 256,
            });
        }
        let data = cursor.take(colors * 3)?;
        for (n, rgb) in data.chunks_exact(3).enumerate() {
            let base = (color_index + n) * 3;
            palette[base] = rgb[0];
            palette[base + 1] = rgb[1];
            palette[base + 2] = rgb[2];
        }
        color_index += colors;
    }
    Ok(())
}

/// `BYTE_RUN` (15): per row a leading byte, then packets until the row is full;
/// negative count = literal segment, positive = one replicated pixel
/// (`FlcFile.cs:341-368`).
fn decode_flc_byte_run(
    cursor: &mut Cursor<'_>,
    canvas: &mut [u8],
    width: usize,
    height: usize,
) -> Result<(), VideoError> {
    for row in 0..height {
        let _row_packet_hint = cursor.u8()?;
        let mut x = 0usize;
        while x < width {
            let offset = cursor.pos;
            let packet = cursor.i8()?;
            let count = packet.unsigned_abs() as usize;
            if count == 0 {
                return Err(VideoError::ZeroRunLength { offset, row });
            }
            if x + count > width {
                return Err(VideoError::PixelOverflow {
                    offset,
                    pixel: row * width + x + count,
                    total: width * height,
                });
            }
            let base = row * width + x;
            if packet < 0 {
                let indices = cursor.take(count)?;
                canvas[base..base + count].copy_from_slice(indices);
            } else {
                let index = cursor.u8()?;
                canvas[base..base + count].fill(index);
            }
            x += count;
        }
    }
    Ok(())
}

/// `DELTA_FLC` (7): `u16 lines`, then per line opcodes (packet count, line skip
/// `0xC000|n`, end of line `0x8000`) followed by 2-pixel-wide packets
/// (`FlcFile.cs:370-419`).
fn decode_flc_delta(
    cursor: &mut Cursor<'_>,
    canvas: &mut [u8],
    width: usize,
    height: usize,
) -> Result<(), VideoError> {
    let lines = cursor.u16()?;
    let mut y = 0usize;
    for _ in 0..lines {
        let packets = loop {
            let offset = cursor.pos;
            let opcode = cursor.i16()?;
            if opcode & 0x8000u16 as i16 != 0 {
                if opcode & 0x4000u16 as i16 != 0 {
                    // Line skip: the low 14 bits are the row count. DFU uses
                    // `Math.Abs(opcode)` (`FlcFile.cs:376`) which is wrong for
                    // this form; retail files contain 0 of them (measured), so
                    // the spec form is the one that can be trusted.
                    y += usize::from((opcode as u16) & 0x3fff);
                    continue;
                }
                // 0x8000: end of the packets of this line.
                break 0usize;
            }
            if opcode & 0x4000u16 as i16 == 0 {
                break usize::from(opcode as u16);
            }
            return Err(VideoError::BadDeltaOpcode { opcode, offset });
        };
        if packets > 0 {
            let mut x = 0usize;
            for _ in 0..packets {
                let offset = cursor.pos;
                let column_skip = usize::from(cursor.u8()?);
                x += column_skip;
                let size = cursor.i8()?;
                let count = size.unsigned_abs() as usize;
                if count == 0 {
                    continue;
                }
                let needed = count * 2;
                if y >= height || x + needed > width {
                    return Err(VideoError::PixelOverflow {
                        offset,
                        pixel: y * width + x + needed,
                        total: width * height,
                    });
                }
                let base = y * width + x;
                if size > 0 {
                    // Copy segment: 2 literal indices per repetition.
                    let data = cursor.take(needed)?;
                    canvas[base..base + needed].copy_from_slice(data);
                } else {
                    // Repeat two pixels `count` times.
                    let first = cursor.u8()?;
                    let second = cursor.u8()?;
                    for n in 0..count {
                        canvas[base + n * 2] = first;
                        canvas[base + n * 2 + 1] = second;
                    }
                }
                x += needed;
            }
        }
        y += 1;
    }
    Ok(())
}

#[cfg(test)]
impl VideoError {
    /// Drops the stream offset of [`VideoError::IncompleteFrame`] so a test can
    /// assert the shape without pinning a byte position.
    fn into_offset_agnostic(self) -> VideoError {
        match self {
            VideoError::IncompleteFrame { pixel, total, .. } => VideoError::IncompleteFrame {
                offset: 0,
                pixel,
                total,
            },
            other => other,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn palette_block(fill: u8) -> Vec<u8> {
        let mut block = vec![VID_PALETTE];
        block.extend(std::iter::repeat_n(fill, PALETTE_BYTES));
        block
    }

    fn vid_header(frames: u16, width: u16, height: u16) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"VID");
        for value in [512u16, frames, width, height, 4, 14] {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        bytes
    }

    /// One `.VID` with a full start frame and one incremental frame.
    fn synthetic_vid() -> Vec<u8> {
        let mut bytes = vid_header(2, 2, 2);
        bytes.extend(palette_block(1));
        // Video_StartFrame: delay 20, literal run of 4 indices.
        bytes.push(VID_VIDEO_START);
        bytes.extend_from_slice(&20u16.to_le_bytes());
        bytes.push(4);
        bytes.extend_from_slice(&[10, 11, 12, 13]);
        // Audio_StartFrame: unknown 0, rate 166, 4 samples.
        bytes.push(VID_AUDIO_START);
        bytes.extend_from_slice(&0u16.to_le_bytes());
        bytes.push(VID_PLAYBACK_RATE);
        bytes.extend_from_slice(&4u16.to_le_bytes());
        bytes.extend_from_slice(&[1, 2, 3, 4]);
        // Video_IncrementalFrame: delay 20, skip 2, then 2 literal indices.
        bytes.push(VID_VIDEO_INCREMENTAL);
        bytes.extend_from_slice(&20u16.to_le_bytes());
        bytes.push(0x80 | 2);
        bytes.push(2);
        bytes.extend_from_slice(&[20, 21]);
        // Audio_IncrementalFrame with 2 samples.
        bytes.push(VID_AUDIO_INCREMENTAL);
        bytes.extend_from_slice(&2u16.to_le_bytes());
        bytes.extend_from_slice(&[9, 9]);
        bytes.push(VID_END_OF_FILE);
        bytes
    }

    #[test]
    fn vid_decodes_full_and_incremental_frames() {
        let decoded = decode(&synthetic_vid()).expect("synthetic VID decodes");
        assert_eq!(decoded.format, VideoFormat::Vid);
        assert_eq!((decoded.width, decoded.height), (2, 2));
        assert_eq!(decoded.frames.len(), 2);
        assert_eq!(decoded.frames[0].indices, vec![10, 11, 12, 13]);
        // The skip run keeps pixels 0 and 1 of the previous canvas.
        assert_eq!(decoded.frames[1].indices, vec![10, 11, 20, 21]);
        // 6-bit palette scaled by 4.
        assert_eq!(decoded.frames[0].palette[0], 4);
        assert_eq!(decoded.frames[0].palette[PALETTE_BYTES - 1], 4);
        assert_eq!(decoded.audio.start_blocks, 1);
        assert_eq!(decoded.audio.incremental_blocks, 1);
        assert_eq!(decoded.audio.bytes, 6);
        assert_eq!(decoded.audio.playback_rate, Some(VID_PLAYBACK_RATE));
        assert_eq!(decoded.chunks.get(&u16::from(VID_VIDEO_START)), Some(&1));
        assert_eq!(decoded.chunks.get(&u16::from(VID_NULL)), None);
        assert_eq!(decoded.padding_bytes, 0);
        assert_eq!(decoded.total_indices(), 8);
    }

    #[test]
    fn vid_row_offset_frame_starts_at_its_row() {
        let mut bytes = vid_header(2, 2, 2);
        bytes.extend(palette_block(0));
        bytes.push(VID_VIDEO_START);
        bytes.extend_from_slice(&20u16.to_le_bytes());
        bytes.push(0x80 | 4);
        bytes.push(5);
        // Row 1 only: skip nothing, write 2 literals.
        bytes.push(VID_VIDEO_ROW_OFFSET);
        bytes.extend_from_slice(&20u16.to_le_bytes());
        bytes.extend_from_slice(&1u16.to_le_bytes());
        bytes.push(2);
        bytes.extend_from_slice(&[7, 8]);
        bytes.push(VID_END_OF_FILE);
        let decoded = decode(&bytes).expect("row offset VID decodes");
        assert_eq!(decoded.frames[0].indices, vec![5, 5, 5, 5]);
        assert_eq!(decoded.frames[1].indices, vec![5, 5, 7, 8]);
    }

    #[test]
    fn vid_rejects_unknown_block_type() {
        let mut bytes = vid_header(1, 2, 2);
        bytes.extend(palette_block(0));
        bytes.push(VID_VIDEO_START);
        bytes.extend_from_slice(&20u16.to_le_bytes());
        bytes.push(4);
        bytes.extend_from_slice(&[1, 2, 3, 4]);
        let offset = bytes.len();
        bytes.push(0x42);
        bytes.push(VID_END_OF_FILE);
        let error = decode(&bytes).expect_err("unknown block must fail");
        assert_eq!(error, VideoError::UnsupportedChunk(0x42, offset));
    }

    #[test]
    fn vid_rejects_frame_count_mismatch() {
        let mut bytes = vid_header(3, 2, 2);
        bytes.extend(palette_block(0));
        bytes.push(VID_VIDEO_START);
        bytes.extend_from_slice(&20u16.to_le_bytes());
        bytes.push(4);
        bytes.extend_from_slice(&[1, 2, 3, 4]);
        bytes.push(VID_END_OF_FILE);
        assert_eq!(
            decode(&bytes).expect_err("declared 3, decoded 1"),
            VideoError::FrameCountMismatch {
                declared: 3,
                decoded: 1
            }
        );
    }

    #[test]
    fn vid_rejects_incomplete_full_frame() {
        let mut bytes = vid_header(1, 2, 2);
        bytes.extend(palette_block(0));
        bytes.push(VID_VIDEO_START);
        bytes.extend_from_slice(&20u16.to_le_bytes());
        bytes.push(2);
        bytes.extend_from_slice(&[1, 2]);
        bytes.push(0);
        bytes.push(VID_END_OF_FILE);
        assert_eq!(
            decode(&bytes)
                .expect_err("2 of 4 pixels")
                .into_offset_agnostic(),
            VideoError::IncompleteFrame {
                offset: 0,
                pixel: 2,
                total: 4
            }
        );
    }

    #[test]
    fn vid_rejects_bad_playback_rate() {
        let mut bytes = vid_header(1, 2, 2);
        bytes.extend(palette_block(0));
        bytes.push(VID_AUDIO_START);
        bytes.extend_from_slice(&0u16.to_le_bytes());
        bytes.push(165);
        bytes.extend_from_slice(&0u16.to_le_bytes());
        bytes.push(VID_END_OF_FILE);
        assert!(matches!(
            decode(&bytes).expect_err("rate 166 only"),
            VideoError::UnknownPlaybackRate { rate: 165, .. }
        ));
    }

    #[test]
    fn vid_rejects_missing_palette() {
        let mut bytes = vid_header(1, 2, 2);
        bytes.push(VID_END_OF_FILE);
        assert_eq!(
            decode(&bytes).expect_err("no palette block"),
            VideoError::PaletteMissing
        );
    }

    #[test]
    fn unknown_container_is_rejected() {
        let error = decode(b"XXXX").expect_err("not a container");
        assert_eq!(error, VideoError::UnknownFormat(*b"XXXX"));
    }

    // ── FLIC fixtures ──────────────────────────────────────────────────────

    /// One sub-chunk: `size i32` + `type u16` + payload (`FlcFile.cs:27`).
    fn chunk(kind: u16, payload: &[u8]) -> Vec<u8> {
        let size = (FLC_CHUNK_HEADER_SIZE + payload.len()) as u32;
        let mut bytes = Vec::with_capacity(size as usize);
        bytes.extend_from_slice(&size.to_le_bytes());
        bytes.extend_from_slice(&kind.to_le_bytes());
        bytes.extend_from_slice(payload);
        bytes
    }

    /// Counts the `size i32 + type i16` chunk headers of a chunk blob.
    fn subchunks_count(chunks: &[u8]) -> u16 {
        let mut count = 0u16;
        let mut pos = 0usize;
        while pos + FLC_CHUNK_HEADER_SIZE <= chunks.len() {
            let size = u32::from_le_bytes([
                chunks[pos],
                chunks[pos + 1],
                chunks[pos + 2],
                chunks[pos + 3],
            ]) as usize;
            if size < FLC_CHUNK_HEADER_SIZE {
                break;
            }
            count += 1;
            pos += size;
        }
        count
    }

    /// Wraps a chunk blob into a frame block (size + type + count + 8 reserved).
    fn wrap_frame(chunks: &[u8]) -> Vec<u8> {
        let mut frame = Vec::with_capacity(FLC_FRAME_HEADER_SIZE + chunks.len());
        let size = (FLC_FRAME_HEADER_SIZE + chunks.len()) as u32;
        frame.extend_from_slice(&size.to_le_bytes());
        frame.extend_from_slice(&FLC_FRAME_TYPE.to_le_bytes());
        frame.extend_from_slice(&subchunks_count(chunks).to_le_bytes());
        frame.extend_from_slice(&[0u8; 8]);
        frame.extend_from_slice(chunks);
        frame
    }

    /// A whole `.FLC` file: 128-byte header (`FlcFile.cs:172-206`) + frames.
    fn flc_file(width: u16, height: u16, frames: u16, body: &[u8]) -> Vec<u8> {
        let mut header = vec![0u8; 128];
        let file_size = (128 + body.len()) as u32;
        header[0..4].copy_from_slice(&file_size.to_le_bytes());
        header[4..6].copy_from_slice(&0xAF12u16.to_le_bytes());
        header[6..8].copy_from_slice(&frames.to_le_bytes());
        header[8..10].copy_from_slice(&width.to_le_bytes());
        header[10..12].copy_from_slice(&height.to_le_bytes());
        header[12..14].copy_from_slice(&8u16.to_le_bytes());
        header[14..16].copy_from_slice(&3u16.to_le_bytes());
        header[16..20].copy_from_slice(&100u32.to_le_bytes());
        header[80..84].copy_from_slice(&128u32.to_le_bytes());
        header.extend_from_slice(body);
        header
    }

    fn color_chunk_payload() -> Vec<u8> {
        // 1 packet, skip 0, 256 colours: the palette is fully rewritten.
        let mut payload = Vec::new();
        payload.extend_from_slice(&1u16.to_le_bytes());
        payload.push(0);
        payload.push(0); // 0 ⇒ 256 colours
        for index in 0..256u16 {
            let value = index as u8;
            payload.extend_from_slice(&[value, value / 2, 255 - value]);
        }
        payload
    }

    /// Ring frame (PSTAMP + COLOR_256 + BYTE_RUN) + one DELTA_FLC frame, 4×2.
    fn synthetic_flc() -> Vec<u8> {
        let mut ring = chunk(FLC_PSTAMP, &[0xAA, 0xBB, 0xCC]);
        ring.extend(chunk(FLC_COLOR_256, &color_chunk_payload()));
        ring.extend(chunk(
            FLC_BYTE_RUN,
            &[
                0, // row 0 hint
                2, 0x11, // replicate 0x11 ×2
                0xFE, 0x22, 0x33, // i8 -2 ⇒ literal run of 2
                0, // row 1 hint
                0xFC, 0x44, 0x55, 0x66, 0x77, // i8 -4 ⇒ literal run of 4
            ],
        ));
        let ring = wrap_frame(&ring);

        // One delta: 2 lines, 1 packet each, 2-pixel-wide segments. Retail
        // DELTA_FLC streams carry no 0x8000 end-of-line marker after the last
        // packet of a line (measured: 0 occurrences, chunk size consumed
        // exactly), so the fixture does not invent one.
        let mut delta = Vec::new();
        delta.extend_from_slice(&2u16.to_le_bytes()); // lines
        for row in 0..2u8 {
            delta.extend_from_slice(&1i16.to_le_bytes()); // packet count
            delta.push(1); // column skip
            delta.push(1); // size 1 ⇒ 2 literal pixels
            delta.extend_from_slice(&[0x90 + row * 2, 0x91 + row * 2]);
        }
        let delta = wrap_frame(&chunk(FLC_DELTA_FLC, &delta));

        let mut body = ring;
        body.extend_from_slice(&delta);
        flc_file(4, 2, 1, &body)
    }

    #[test]
    fn flc_decodes_ring_plus_delta() {
        let decoded = decode(&synthetic_flc()).expect("synthetic FLC decodes");
        assert_eq!(decoded.format, VideoFormat::Flc);
        assert_eq!((decoded.width, decoded.height), (4, 2));
        assert_eq!(decoded.frames.len(), 1);
        // Ring frame → delta overwrote x=1..2 of each row (2-pixel segments).
        assert_eq!(decoded.frames[0].indices, vec![0x11, 0x90, 0x91, 0x33, 0x44, 0x92, 0x93, 0x77]);
        // COLOR_256: entry n = (n, n/2, 255-n), so entry 0 = (0, 0, 255).
        assert_eq!(&decoded.frames[0].palette[0..6], &[0, 0, 255, 1, 0, 254]);
        assert_eq!(&decoded.frames[0].palette[765..768], &[255, 127, 0]);
        assert_eq!(decoded.chunks.get(&FLC_PSTAMP), Some(&1));
        assert_eq!(decoded.chunks.get(&FLC_COLOR_256), Some(&1));
        assert_eq!(decoded.chunks.get(&FLC_BYTE_RUN), Some(&1));
        assert_eq!(decoded.chunks.get(&FLC_DELTA_FLC), Some(&1));
        assert_eq!(decoded.padding_bytes, 0);
        assert_eq!(decoded.total_indices(), 8);
    }

    #[test]
    fn flc_rejects_unknown_chunk_type() {
        let mut blob = chunk(FLC_COLOR_256, &color_chunk_payload());
        blob.extend(chunk(12, &[0, 0, 0, 0])); // DELTA_FLI: 0 retail, not implemented
        let bytes = flc_file(4, 2, 1, &wrap_frame(&blob));
        let error = decode(&bytes).expect_err("DELTA_FLI is not implemented");
        assert!(
            matches!(error, VideoError::UnsupportedChunk(12, _)),
            "got {error:?}"
        );
    }

    #[test]
    fn flc_rejects_bad_frame_type() {
        let mut bytes = synthetic_flc();
        // Corrupt the ring frame type (offset 128 + 4).
        bytes[132..134].copy_from_slice(&0i16.to_le_bytes());
        assert!(matches!(
            decode(&bytes).expect_err("frame type must be -3590"),
            VideoError::BadFrameType { kind: 0, .. }
        ));
    }

    #[test]
    fn flc_rejects_zero_byte_run_packet() {
        let mut blob = chunk(FLC_COLOR_256, &color_chunk_payload());
        blob.extend(chunk(FLC_BYTE_RUN, &[0, 0, 0xFF, 0xFF, 0xFF, 0xFF]));
        let bytes = flc_file(4, 2, 1, &wrap_frame(&blob));
        assert!(matches!(
            decode(&bytes).expect_err("zero-length run"),
            VideoError::ZeroRunLength { row: 0, .. }
        ));
    }

    #[test]
    fn clock_constants_match_the_golden_vectors() {
        // GV-08 numeric value (the unit in §8 is wrong: see module docs).
        assert_eq!(DIVISOR, 0.007_812_5);
        assert_eq!(1.0 / 128.0, DIVISOR);
        // GV-07 audio clip queue.
        assert_eq!(CLIP_QUEUE_LENGTH, 2);
        // VidFile.cs:29,34.
        assert_eq!(VID_SAMPLE_RATE, 11_025);
        assert!((MIN_FRAME_DELAY_SECONDS - 740.0 / 11_025.0).abs() < f64::EPSILON);
        // VidFile.cs:242-250, :33.
        assert_eq!(VID_PLAYBACK_RATE, 166);
        assert_eq!(VID_PALETTE_MULTIPLIER, 4);
    }
}
