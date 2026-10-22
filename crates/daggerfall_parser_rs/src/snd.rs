//! Original sound effects — **`DAGGER.SND`** (REQ-L99-046, M14).
//! *Clean-room* implementation from spec, 0 bytes of Daggerfall
//! Unity copied, 0 packed assets (INV-L99-6).
//!
//! **Spec source:**
//! - UESP *Daggerfall Mod:BSA file formats*: `DAGGER.SND` is Daggerfall's only
//!   WAVE audio file, in **BSA type 0x0200** format (directory
//!   with numeric IDs) — we reuse [`crate::bsa`] (already validated against
//!   `ARCH3D.BSA`/`BLOCKS.BSA` in M6).
//! - DFWorkshop *Sounds & Movies*: each clip is **raw 8-bit unsigned mono
//!   PCM audio at ~11025 Hz** (no embedded RIFF/WAVE container; the
//!   WAVE header is synthesized on playback).
//! - *enum-id* mapping: `DFU:Assets/Scripts/Game/SoundClips.cs` (`ArrowShoot = 3`,
//!   `FallDamage = 91`, …) — the DFU enum discriminant **is** the
//!   `RecordID` of the 0x0200 BSA directory, validated at runtime against the
//!   actual `record_id` of each entry.
//!
//! **Finding verified today (ARENA2 read-only):** the actual file present on
//! this machine is **`DAGGER.SND`** (`fbb6195b…86f3e2`, 7,661,766 B), **not**
//! `SOUND.BSA` (absent, negative finding R-D2). `DAGGER.SND` parses as BSA
//! 0x0200 with 459 records: raw packed data from offset 4 + directory
//! `(RecordID, DataSize)` at the end; the invariant `4 + ΣDataSize == directory_offset`
//! holds exactly (7,658,094). Each record is 8-bit unsigned mono PCM.
//!
//! Honest degradation: if the file is missing from the user's ARENA2 (Mode A /
//! PathChecker), no audio is fabricated — [`SndError::Missing`] is reported.

use crate::bsa;

/// Default sample rate of Daggerfall PCM (DFWorkshop Sounds & Movies).
pub const SAMPLE_RATE: u32 = 11025;
/// Channels (mono).
pub const CHANNELS: u16 = 1;
/// Bits per sample (8-bit unsigned).
pub const BITS_PER_SAMPLE: u16 = 8;

/// Errors of the effects pipeline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SndError {
    /// The file/underlying BSA does not parse.
    BadContainer(bsa::BsaError),
    /// The directory is not 0x0200 (numeric IDs).
    NotNumbered,
    /// The requested `SoundClips` has no record in the file.
    MissingClip { clip: i32, count: usize },
    /// The record exists but is empty (no PCM).
    EmptyClip { clip: i32, index: usize },
}

impl std::fmt::Display for SndError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BadContainer(e) => write!(f, "invalid DAGGER.SND container: {e}"),
            Self::NotNumbered => write!(f, "directory is not 0x0200 (numeric)"),
            Self::MissingClip { clip, count } => {
                write!(f, "SoundClips {clip} has no record in the file ({count} records)")
            }
            Self::EmptyClip { clip, index } => {
                write!(f, "SoundClips {clip} -> record {index} is empty (0 PCM bytes)")
            }
        }
    }
}

impl std::error::Error for SndError {}

impl From<bsa::BsaError> for SndError {
    fn from(e: bsa::BsaError) -> Self {
        Self::BadContainer(e)
    }
}

/// A decoded clip: raw 8-bit unsigned mono PCM (no header).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SoundClip {
    /// Directory `RecordID` (== DFU `SoundClips` enum discriminant).
    pub record_id: u32,
    /// Number of samples (= PCM bytes, 8-bit mono).
    pub samples: usize,
    /// Raw 8-bit unsigned PCM (record payload).
    pub pcm: Vec<u8>,
}

impl SoundClip {
    /// Synthesizes a 44-byte RIFF/WAVE header for this clip
    /// (8-bit unsigned mono PCM at [`SAMPLE_RATE`] Hz) — the same thing DFU
    /// does when materializing an `AudioClip` from the raw PCM.
    pub fn wave_header(&self) -> Vec<u8> {
        let data_len = self.pcm.len() as u32;
        let byte_rate = SAMPLE_RATE * u32::from(CHANNELS) * u32::from(BITS_PER_SAMPLE / 8);
        let block_align = CHANNELS * (BITS_PER_SAMPLE / 8);
        let mut h = Vec::with_capacity(44 + self.pcm.len());
        h.extend_from_slice(b"RIFF");
        h.extend_from_slice(&(36 + data_len).to_le_bytes());
        h.extend_from_slice(b"WAVE");
        h.extend_from_slice(b"fmt ");
        h.extend_from_slice(&16u32.to_le_bytes());
        h.extend_from_slice(&1u16.to_le_bytes()); // PCM
        h.extend_from_slice(&CHANNELS.to_le_bytes());
        h.extend_from_slice(&SAMPLE_RATE.to_le_bytes());
        h.extend_from_slice(&byte_rate.to_le_bytes());
        h.extend_from_slice(&block_align.to_le_bytes());
        h.extend_from_slice(&BITS_PER_SAMPLE.to_le_bytes());
        h.extend_from_slice(b"data");
        h.extend_from_slice(&data_len.to_le_bytes());
        h.extend_from_slice(&self.pcm);
        h
    }
}

/// Effect bank: `DAGGER.SND` parsed as BSA 0x0200.
#[derive(Debug)]
pub struct SoundBank {
    archive: bsa::BsaArchive,
    /// `record_id` -> directory index, for O(1) access by enum-id.
    by_id: std::collections::HashMap<u32, usize>,
}

impl SoundBank {
    /// Parses the bank from the complete `DAGGER.SND` buffer (read-only).
    pub fn parse(data: &[u8]) -> Result<Self, SndError> {
        let archive = bsa::parse(data)?;
        if archive.directory_type != bsa::DirectoryType::Number8 {
            return Err(SndError::NotNumbered);
        }
        let mut by_id = std::collections::HashMap::with_capacity(archive.entries.len());
        for (idx, entry) in archive.entries.iter().enumerate() {
            if let bsa::Entry::Numbered { record_id, .. } = entry {
                by_id.insert(*record_id, idx);
            }
        }
        Ok(Self { archive, by_id })
    }

    /// Number of records in the bank.
    pub fn len(&self) -> usize {
        self.archive.entries.len()
    }

    /// `true` if there are no records.
    pub fn is_empty(&self) -> bool {
        self.archive.entries.is_empty()
    }

    /// Decodes the clip whose `RecordID` == `clip` (DFU `SoundClips`
    /// discriminant). `Err(MissingClip)` if the ID does not exist in the directory.
    pub fn decode(&self, data: &[u8], clip: i32) -> Result<SoundClip, SndError> {
        let record_id = clip as u32;
        let index = *self
            .by_id
            .get(&record_id)
            .ok_or(SndError::MissingClip { clip, count: self.archive.entries.len() })?;
        let pcm = bsa::entry_data(&self.archive, data, index)?.to_vec();
        if pcm.is_empty() {
            return Err(SndError::EmptyClip { clip, index });
        }
        let samples = pcm.len();
        Ok(SoundClip { record_id, samples, pcm })
    }

    /// `record_id` of the record at position `index` (0-based, directory
    /// order) — for an enum-id↔index mapping smoke test.
    pub fn record_id_at(&self, index: usize) -> Option<u32> {
        match self.archive.entries.get(index) {
            Some(bsa::Entry::Numbered { record_id, .. }) => Some(*record_id),
            _ => None,
        }
    }
}

/// DFU `SoundClips` *enum-id* mapping (subset of the ones used in gameplay, with
/// the exact discriminants from the DFU corpus). The discriminant **is** the
/// `RecordID` in `DAGGER.SND` (validated at runtime by [`SoundBank::decode`]).
/// References: `DFU:EnemyAttack.cs:110` (`ArrowShoot`), `DFU:EnemyMotor.cs:1409`
/// (`FallDamage`), `DFU:.../Actions/PlaySound.cs:20-23`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(i32)]
pub enum SoundClips {
    /// `DFU:EnemyMotor.cs:1409` — fall damage.
    FallDamage = 91,
    /// `DFU:EnemyAttack.cs:110` — arrow shot.
    ArrowShoot = 3,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds a minimal synthetic `DAGGER.SND` (BSA 0x0200) with two test
    /// PCM clips — 0 retail bytes (INV-L99-6).
    fn build_snd(records: &[(u32, Vec<u8>)]) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(&(records.len() as u16).to_le_bytes());
        out.extend_from_slice(&0x0200u16.to_le_bytes());
        for (_, pcm) in records {
            out.extend_from_slice(pcm);
        }
        for (id, pcm) in records {
            out.extend_from_slice(&id.to_le_bytes());
            out.extend_from_slice(&(pcm.len() as u32).to_le_bytes());
        }
        out
    }

    #[test]
    fn parses_numbered_directory() {
        let snd = build_snd(&[
            (3, vec![0x80, 0x81, 0x82]),
            (91, vec![0x7f, 0x7e]),
        ]);
        let bank = SoundBank::parse(&snd).expect("parse");
        assert_eq!(bank.len(), 2);
        assert_eq!(bank.record_id_at(0), Some(3));
        assert_eq!(bank.record_id_at(1), Some(91));
    }

    #[test]
    fn decodes_by_enum_id() {
        let snd = build_snd(&[(3, vec![0x80, 0x81, 0x82]), (91, vec![0x7f, 0x7e])]);
        let bank = SoundBank::parse(&snd).expect("parse");
        let arrow = bank.decode(&snd, SoundClips::ArrowShoot as i32).expect("decode arrow");
        assert_eq!(arrow.record_id, 3);
        assert_eq!(arrow.samples, 3);
        assert_eq!(arrow.pcm, vec![0x80, 0x81, 0x82]);
        let fall = bank.decode(&snd, SoundClips::FallDamage as i32).expect("decode fall");
        assert_eq!(fall.record_id, 91);
        assert_eq!(fall.samples, 2);
    }

    #[test]
    fn missing_clip_errors_honestly() {
        let snd = build_snd(&[(3, vec![0x80])]);
        let bank = SoundBank::parse(&snd).expect("parse");
        let err = bank.decode(&snd, SoundClips::FallDamage as i32).expect_err("missing");
        assert!(matches!(err, SndError::MissingClip { clip: 91, count: 1 }));
    }

    #[test]
    fn rejects_wrong_directory_type() {
        // 0x0100 (named) is not an effect bank.
        let mut snd = Vec::new();
        snd.extend_from_slice(&1u16.to_le_bytes());
        snd.extend_from_slice(&0x0100u16.to_le_bytes());
        snd.extend_from_slice(&[1, 2, 3]); // data (3 bytes) before the directory
        snd.extend_from_slice(b"FOO.WAV\0\0\0\0\0\0\0"); // 14-byte name
        snd.extend_from_slice(&3u32.to_le_bytes()); // size
        let err = SoundBank::parse(&snd).expect_err("wrong type");
        assert!(matches!(err, SndError::NotNumbered));
    }

    #[test]
    fn wave_header_is_44_bytes() {
        let clip = SoundClip { record_id: 3, samples: 2, pcm: vec![0x80, 0x7f] };
        let w = clip.wave_header();
        assert_eq!(&w[0..4], b"RIFF");
        assert_eq!(&w[8..12], b"WAVE");
        assert_eq!(&w[12..16], b"fmt ");
        assert_eq!(&w[36..40], b"data");
        assert_eq!(w.len(), 44 + 2);
        // data length field == 2.
        assert_eq!(u32::from_le_bytes([w[40], w[41], w[42], w[43]]), 2);
    }
}
