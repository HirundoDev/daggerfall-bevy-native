//! `PROBE-VIDEO-001` — M34 §6 «vídeo»: decode **all** `.VID`/`.FLC` cinematics
//! of the retail install (spec `m34-e2e-integration-presentation-spec
//! -CR-5243af63.md:257-268`, GV-07/GV-08 `:289-313`; W34-4, `CR-2d5c8f41`).
//!
//! ## What is measured (never read from a table)
//!
//! - the 33 files of `ARENA2` (17 `.VID` + 16 `.FLC`, GV-07) are decoded
//!   **frame by frame** through [`daggerfall_parser_rs::video::decode`]; every
//!   frame of every file contributes to the digest, not only the first;
//! - the block/chunk histogram is **counted from the data**, per container
//!   (the ids overlap between `.VID` blocks and FLIC chunks, so merging them
//!   would be a measurement bug) and printed next to the documented baseline;
//! - per file: header `frames` == decoded frames (the decoder refuses the file
//!   otherwise), every frame contributes exactly `width*height` indices, the
//!   768-byte palette is present, and `width`/`height` are > 0 and constant;
//! - `0 panic`: every decode runs inside `catch_unwind`; a caught panic is a
//!   FAIL carrying the file name, never a silent skip.
//!
//! ## Digest transcript (so an independent decode can be compared)
//!
//! Per file, chained from `FNV_OFFSET`:
//! `NAME` (ASCII upper) · `VID`|`FLC` · `w` LE·u16 · `h` LE·u16 · `frames`
//! LE·u64 · for each frame `palette` (768 B) then `indices` (`w*h` B).
//! The corpus digest chains each per-file digest (LE·u64) in file order
//! (`.VID` sorted, then `.FLC` sorted). The printed per-file digests were
//! reproduced byte-for-byte by an independent Python decoder of the same
//! retail bytes (`ANIM0011.VID`, `AZURA.FLC`) — see the W34-4 handoff.
//!
//! ## Deviation reported, never silently absorbed
//!
//! §8 GV-08 prints `divisor = 1/128 = 0.0078125 s`. In the source the divisor
//! is the **8-bit sample scale** of the audio player
//! (`DFU@0038a1e6:…/DaggerfallVideo.cs:106,110`: `(AudioBuffer[i] - 128) *
//! divisor`), i.e. dimensionless; a `.VID` frame lasts `audio_bytes / 11 025 s`
//! with a 740-sample floor. The numeric value is reproduced bit-for-bit, the
//! unit `s` is **not**.
//!
//! A file that cannot be decoded is printed with its [`VideoError`] (which
//! carries the stream offset) and counted as a FAIL: the exit code reports how
//! many files decoded, it never rounds up.
//!
//! ## Determinism
//!
//! The corpus digest is computed **twice** over two independent decode passes
//! in one process, and the whole binary was run twice in separate processes:
//! all three values must agree. `EXIT=0` PASS · `EXIT=1` assertion FAIL ·
//! `EXIT=2` no ARENA2.

use daggerfall_app::mode_a;
use daggerfall_parser_rs::video::{self, VideoError};
use std::collections::{BTreeMap, BTreeSet};
use std::panic::{self, AssertUnwindSafe};
use std::path::{Path, PathBuf};

/// `.VID` files in the retail install (GV-07; measured 2026-10-26).
const EXPECTED_VID: usize = 17;
/// `.FLC` files in the retail install (GV-07; measured 2026-10-26).
const EXPECTED_FLC: usize = 16;

/// Documented baseline: `.VID` block type → count, over all 17 files.
/// Measured with this probe (W34-4, 2026-10-26); printed beside the live count.
const BASELINE_VID_BLOCKS: &[(u16, usize)] = &[
    (0, 84),
    (1, 2_324),
    (3, 17),
    (4, 2_320),
    (20, 17),
    (124, 19),
    (125, 4_463),
];

/// Documented baseline: FLIC chunk type → count, over all 16 files.
const BASELINE_FLC_CHUNKS: &[(u16, usize)] = &[
    (4, 16),
    (7, 224),
    (15, 16),
    (18, 16),
];

/// Everything one measurement pass accumulates.
#[derive(Default)]
struct Census {
    vid_files: usize,
    flc_files: usize,
    vid_frames: usize,
    flc_frames: usize,
    /// `w*h` summed per frame, from each frame's own `indices.len()`.
    frame_pixels: usize,
    /// `w*h*frames` summed per file, from the container header.
    header_pixels: usize,
    vid_blocks: BTreeMap<u16, usize>,
    flc_chunks: BTreeMap<u16, usize>,
    audio_blocks: usize,
    audio_start_blocks: usize,
    audio_incremental_blocks: usize,
    audio_bytes: usize,
    playback_rates: BTreeSet<u8>,
    padding_bytes: usize,
    dims: BTreeSet<(u16, u16)>,
    per_file_digests: Vec<u64>,
    panics: usize,
    failures: Vec<String>,
}

impl Census {
    fn merge(&mut self, decoded: &video::DecodedVideo, digest: u64) {
        self.per_file_digests.push(digest);
        self.header_pixels += decoded.total_indices();
        self.padding_bytes += decoded.padding_bytes;
        self.dims.insert((decoded.width, decoded.height));
        match decoded.format {
            video::VideoFormat::Vid => {
                self.vid_files += 1;
                self.vid_frames += decoded.frames.len();
                self.audio_blocks += decoded.audio.blocks();
                self.audio_start_blocks += decoded.audio.start_blocks;
                self.audio_incremental_blocks += decoded.audio.incremental_blocks;
                self.audio_bytes += decoded.audio.bytes;
                if let Some(rate) = decoded.audio.playback_rate {
                    self.playback_rates.insert(rate);
                }
                for (kind, count) in &decoded.chunks {
                    *self.vid_blocks.entry(*kind).or_default() += count;
                }
            }
            video::VideoFormat::Flc | video::VideoFormat::Fli => {
                self.flc_files += 1;
                self.flc_frames += decoded.frames.len();
                for (kind, count) in &decoded.chunks {
                    *self.flc_chunks.entry(*kind).or_default() += count;
                }
            }
        }
    }

    fn frames(&self) -> usize {
        self.vid_frames + self.flc_frames
    }

    /// Count of the given `.VID` block types.
    fn vid_block(&self, kinds: &[u8]) -> usize {
        kinds
            .iter()
            .map(|kind| self.vid_blocks.get(&u16::from(*kind)).copied().unwrap_or(0))
            .sum()
    }

    /// Corpus digest: each per-file digest chained in file order.
    fn corpus_digest(&self) -> u64 {
        self.per_file_digests
            .iter()
            .fold(FNV_OFFSET, |hash, digest| {
                fnv1a64(&digest.to_le_bytes(), hash)
            })
    }
}

/// `PROBE-VIDEO-001` entry point: prints, returns the process exit code.
pub fn run_video_001() -> i32 {
    println!("== PROBE-VIDEO-001 — M34 §6 .VID/.FLC decoding (W34-4) ==");
    let dir = match mode_a::arena2_dir() {
        Ok(dir) => dir,
        Err(err) => {
            eprintln!("[FAIL] ARENA2 - {err}");
            println!("VEREDICTO: NO-PASS (no retail data root)");
            println!("EXIT=2");
            return 2;
        }
    };
    println!("arena2: {}", dir.display());

    let vids = match collect(&dir, "VID") {
        Ok(files) => files,
        Err(err) => {
            eprintln!("[FAIL] VID listing - {err}");
            println!("EXIT=2");
            return 2;
        }
    };
    let flcs = match collect(&dir, "FLC") {
        Ok(files) => files,
        Err(err) => {
            eprintln!("[FAIL] FLC listing - {err}");
            println!("EXIT=2");
            return 2;
        }
    };

    let mut passed = true;
    check(
        &mut passed,
        vids.len() == EXPECTED_VID,
        "GV-07 VID file count",
        format!("{} (expected {EXPECTED_VID})", vids.len()),
    );
    check(
        &mut passed,
        flcs.len() == EXPECTED_FLC,
        "GV-07 FLC file count",
        format!("{} (expected {EXPECTED_FLC})", flcs.len()),
    );
    check(
        &mut passed,
        vids.len() + flcs.len() == EXPECTED_VID + EXPECTED_FLC,
        "GV-07 corpus 33",
        format!("{}/33", vids.len() + flcs.len()),
    );

    // Pass 1 prints the per-file evidence; pass 2 only recomputes the digest.
    let first = measure(&vids, &flcs, true);
    let second = measure(&vids, &flcs, false);
    let digest_a = first.corpus_digest();
    let digest_b = second.corpus_digest();

    check(
        &mut passed,
        first.failures.is_empty(),
        "every file decoded",
        format!(
            "{}/{} files ({})",
            vids.len() + flcs.len() - first.failures.len(),
            vids.len() + flcs.len(),
            if first.failures.is_empty() {
                "no failures".to_string()
            } else {
                first.failures.join("; ")
            }
        ),
    );
    check(
        &mut passed,
        first.panics == 0,
        "zero panics",
        format!("{} caught panic(s)", first.panics),
    );
    check(
        &mut passed,
        first.vid_frames >= 1 && first.flc_frames >= 1,
        ">= 1 decoded frame per container",
        format!(
            "VID {} frames, FLC {} frames",
            first.vid_frames, first.flc_frames
        ),
    );
    check(
        &mut passed,
        first.frame_pixels == first.header_pixels,
        "per-frame pixel budget",
        format!(
            "{} indices measured per frame == {} from headers, dims {:?}",
            first.frame_pixels, first.header_pixels, first.dims
        ),
    );
    check(
        &mut passed,
        first.playback_rates == BTreeSet::from([video::VID_PLAYBACK_RATE]),
        "audio playback rate",
        format!("{:?} (166 ⇒ 11 025 Hz)", first.playback_rates),
    );
    check(
        &mut passed,
        (video::DIVISOR - 1.0 / 128.0).abs() < f64::EPSILON,
        "GV-08 DIVISOR value",
        format!(
            "{:.10} (1/128 = 0.0078125; §8 prints unit `s`, source is a sample scale)",
            video::DIVISOR
        ),
    );
    check(
        &mut passed,
        video::CLIP_QUEUE_LENGTH == 2,
        "GV-07 CLIP_QUEUE_LENGTH",
        format!("{}", video::CLIP_QUEUE_LENGTH),
    );
    check(
        &mut passed,
        digest_a == digest_b && first.frames() == second.frames(),
        "digest x2 (independent passes)",
        format!("{digest_a:#018x} == {digest_b:#018x}"),
    );
    let frame_blocks = first.vid_block(&[
        video::VID_VIDEO_START,
        video::VID_VIDEO_INCREMENTAL,
        video::VID_VIDEO_ROW_OFFSET,
    ]);
    check(
        &mut passed,
        frame_blocks == first.vid_frames,
        "VID video blocks vs frames",
        format!(
            "{frame_blocks} frame blocks (types 3/1/4) == {} decoded frames",
            first.vid_frames
        ),
    );
    check(
        &mut passed,
        first.vid_block(&[video::VID_AUDIO_START]) == first.audio_start_blocks
            && first.vid_block(&[video::VID_AUDIO_INCREMENTAL]) == first.audio_incremental_blocks,
        "VID audio blocks vs sample bytes",
        format!(
            "124={} start, 125={} incremental (+{} B of sample data)",
            first.vid_block(&[video::VID_AUDIO_START]),
            first.vid_block(&[video::VID_AUDIO_INCREMENTAL]),
            first.audio_bytes
        ),
    );
    check(
        &mut passed,
        first.vid_block(&[video::VID_END_OF_FILE]) == first.vid_files,
        "VID end-of-file blocks",
        format!(
            "{} EOF blocks for {} files",
            first.vid_block(&[video::VID_END_OF_FILE]),
            first.vid_files
        ),
    );

    println!();
    print_histogram(
        "VID blocks",
        &first.vid_blocks,
        BASELINE_VID_BLOCKS,
        |kind| match kind {
            0 => "null",
            1 => "video_incremental",
            2 => "palette",
            3 => "video_start",
            4 => "video_row_offset",
            20 => "end_of_file",
            124 => "audio_start",
            125 => "audio_incremental",
            _ => "?",
        },
    );
    print_histogram(
        "FLC chunks",
        &first.flc_chunks,
        BASELINE_FLC_CHUNKS,
        |kind| match kind {
            4 => "color_256",
            7 => "delta_flc",
            15 => "byte_run",
            18 => "pstamp",
            _ => "?",
        },
    );
    println!(
        "audio: {} blocks ({} start + {} incremental), {} sample bytes, rate {:?}",
        first.audio_blocks,
        first.audio_start_blocks,
        first.audio_incremental_blocks,
        first.audio_bytes,
        first.playback_rates
    );
    println!(
        "chunk slack: {} bytes declared-but-unconsumed (FLC BYTE_RUN pad)",
        first.padding_bytes
    );
    println!(
        "frames: VID {} + FLC {} = {}; indices {}; dims {:?}",
        first.vid_frames,
        first.flc_frames,
        first.frames(),
        first.frame_pixels,
        first.dims
    );
    println!(
        "clock: DIVISOR={:.10} (dimensionless) CLIP_QUEUE_LENGTH={} \
         MIN_FRAME_DELAY={:.6}s SAMPLE_RATE={}Hz",
        video::DIVISOR,
        video::CLIP_QUEUE_LENGTH,
        video::MIN_FRAME_DELAY_SECONDS,
        video::VID_SAMPLE_RATE
    );
    println!("digest FNV-1a 64 (corpus): {digest_a:#018x}");
    println!();

    if passed {
        println!("VEREDICTO: PASS");
        println!("EXIT=0");
        0
    } else {
        println!("VEREDICTO: FAIL");
        println!("EXIT=1");
        1
    }
}

/// One full pass over the corpus. Returns the census with per-file digests.
fn measure(vids: &[PathBuf], flcs: &[PathBuf], verbose: bool) -> Census {
    let mut census = Census::default();
    for path in vids.iter().chain(flcs.iter()) {
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("<unnamed>")
            .to_ascii_uppercase();
        let bytes = match std::fs::read(path) {
            Ok(bytes) => bytes,
            Err(err) => {
                census.failures.push(format!("{name}: read {err}"));
                continue;
            }
        };
        let decoded = match panic::catch_unwind(AssertUnwindSafe(|| video::decode(&bytes))) {
            Ok(Ok(decoded)) => decoded,
            Ok(Err(err)) => {
                census.failures.push(format!("{name}: {err}"));
                if verbose {
                    println!("[FAIL] {name} - {}", describe(&err));
                }
                continue;
            }
            Err(_) => {
                census.panics += 1;
                census.failures.push(format!("{name}: PANIC"));
                if verbose {
                    println!("[FAIL] {name} - PANIC while decoding");
                }
                continue;
            }
        };

        let pixels = decoded.pixels_per_frame();
        let mut file_ok = decoded.width > 0 && decoded.height > 0 && !decoded.frames.is_empty();
        for (index, frame) in decoded.frames.iter().enumerate() {
            if frame.indices.len() != pixels {
                census.failures.push(format!(
                    "{name}: frame {index} has {} indices, expected {pixels}",
                    frame.indices.len()
                ));
                file_ok = false;
                break;
            }
            if frame.palette.len() != video::PALETTE_BYTES {
                census.failures.push(format!(
                    "{name}: frame {index} palette is {} B, expected {}",
                    frame.palette.len(),
                    video::PALETTE_BYTES
                ));
                file_ok = false;
                break;
            }
            census.frame_pixels += frame.indices.len();
        }
        if !file_ok {
            continue;
        }

        let digest = transcript_digest(&name, &decoded);
        if verbose {
            println!(
                "[OK] {name} - {} {}B frames={} {}x{} chunk_instances={} audio={}B slack={}B digest={digest:#018x}",
                decoded.format.label(),
                bytes.len(),
                decoded.frames.len(),
                decoded.width,
                decoded.height,
                decoded.chunks.values().sum::<usize>(),
                decoded.audio.bytes,
                decoded.padding_bytes
            );
        }
        census.merge(&decoded, digest);
    }
    census
}

/// Per-file digest, exactly the transcript documented in the module header.
fn transcript_digest(name: &str, decoded: &video::DecodedVideo) -> u64 {
    let mut hash = fnv1a64(name.as_bytes(), FNV_OFFSET);
    hash = fnv1a64(decoded.format.label().as_bytes(), hash);
    hash = fnv1a64(&decoded.width.to_le_bytes(), hash);
    hash = fnv1a64(&decoded.height.to_le_bytes(), hash);
    hash = fnv1a64(&(decoded.frames.len() as u64).to_le_bytes(), hash);
    for frame in &decoded.frames {
        hash = fnv1a64(&frame.palette, hash);
        hash = fnv1a64(&frame.indices, hash);
    }
    hash
}

/// Prints the live histogram beside its documented baseline, flagging drift.
fn print_histogram(
    title: &str,
    live: &BTreeMap<u16, usize>,
    baseline: &[(u16, usize)],
    label: impl Fn(u16) -> &'static str,
) {
    let rows: Vec<String> = live
        .iter()
        .map(|(kind, count)| format!("{kind}({})={count}", label(*kind)))
        .collect();
    println!("{title} (type -> count): {}", rows.join(" "));
    let drift: Vec<String> = baseline
        .iter()
        .filter(|(kind, expected)| live.get(kind).copied().unwrap_or(0) != *expected)
        .map(|(kind, expected)| {
            format!(
                "{kind}: baseline {expected} vs measured {}",
                live.get(kind).copied().unwrap_or(0)
            )
        })
        .chain(
            live.iter()
                .filter(|(kind, _)| !baseline.iter().any(|(known, _)| known == *kind))
                .map(|(kind, count)| format!("{kind}: {count} not in baseline")),
        )
        .collect();
    if drift.is_empty() {
        println!("  baseline: identical");
    } else {
        println!("  [WARN] baseline drift - {}", drift.join("; "));
    }
}

fn describe(err: &VideoError) -> String {
    match err {
        VideoError::UnsupportedChunk(kind, offset) => {
            format!("unsupported chunk type {kind} at offset {offset}")
        }
        VideoError::BadLength {
            offset,
            declared,
            consumed,
        } => format!("length mismatch at {offset}: declared {declared}, consumed {consumed}"),
        VideoError::PixelOverflow {
            offset,
            pixel,
            total,
        } => format!("pixel overflow at {offset}: to {pixel} > {total}"),
        other => other.to_string(),
    }
}

fn check(passed: &mut bool, condition: bool, label: &str, detail: String) {
    if !condition {
        *passed = false;
    }
    println!(
        "[{}] {label} - {detail}",
        if condition { "OK" } else { "FAIL" }
    );
}

/// Extension match (case-insensitive), sorted: deterministic corpus order.
fn collect(dir: &Path, extension: &str) -> Result<Vec<PathBuf>, String> {
    let entries = std::fs::read_dir(dir).map_err(|err| format!("reading {dir:?}: {err}"))?;
    let mut files = Vec::new();
    for entry in entries {
        let path = entry
            .map_err(|err| format!("entry of {dir:?}: {err}"))?
            .path();
        let matches = path
            .extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| ext.eq_ignore_ascii_case(extension));
        if matches {
            files.push(path);
        }
    }
    files.sort();
    Ok(files)
}

/// FNV-1a 64 offset basis.
const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
/// FNV-1a 64 prime.
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

/// FNV-1a 64 continued from `state`.
fn fnv1a64(bytes: &[u8], state: u64) -> u64 {
    let mut hash = state;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fnv1a64_matches_the_reference_vectors() {
        assert_eq!(fnv1a64(b"", FNV_OFFSET), 0xcbf2_9ce4_8422_2325);
        assert_eq!(fnv1a64(b"a", FNV_OFFSET), 0xaf63_dc4c_8601_ec8c);
        assert_eq!(fnv1a64(b"foobar", FNV_OFFSET), 0x8594_4171_f739_67e8);
    }
}
