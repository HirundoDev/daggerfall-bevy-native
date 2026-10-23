//! MIDI music of the original — **HMI-MIDISONG** (MIDI.BSA, REQ-L99-045, M14).
//! *Clean-room* implementation from public specs, 0 bytes copied from
//! Daggerfall Unity, 0 packaged assets (INV-L99-6).
//!
//! **Spec sources:**
//! - HMI format: WHS/Dan Wentz (Descent DevNet HMP/HMQ specs, 1999) + WildMidi
//!   `docs/formats/HmiFileFormat.txt` (CC-BY-SA 4.0, Chris Ison 2014) +
//!   VGMPF/ModdingWiki (signatures `HMI-MIDISONG061595` / `HMI-MIDITRACK`).
//! - Song catalogue: `DFU:Assets/Scripts/SongFiles.cs` (MIT, corpus pinned
//!   upstream) — `SongFiles` enum with the 132 songs of MIDI.BSA;
//!   `DFU:.../Internal/DaggerfallSongPlayer.cs` (`EnumToFilename`: enum name
//!   without the `song_` prefix + extension). Classic Daggerfall packages
//!   the `.HMI` files (not `.mid`) in MIDI.BSA — BSA type `0x0100` with names
//!   (`bsa`).
//!
//! **Empirical validation (2026-10-19, ARENA2 read-only):** the 131 songs
//! of retail `MIDI.BSA` (sha `bfb1366f…7d9997`) parse 131/131 with this
//! decoder (0 panic, EOT in every track) — PROBE-AUDIO-001 (a).
//!
//! HMI layout (offsets verified against the 131 retail files):
//! - Header: `HMI-MIDISONG061595` (18 bytes) · `division` u16 LE @0xD0 ·
//!   `bpm` u16 LE @0xD2 · `track_count` u8 @0xE4 · track offsets u32 LE
//!   @0x172 + 4·k.
//! - Track: `HMI-MIDITRACK` (13 bytes) at its offset · `track_header_size` u32
//!   LE @+0x57 (identical @+0x5B); events start at
//!   `track_start + track_header_size`.
//! - Events: delta VLQ (bit7 = continuation, MSB-first — identical to the SMF
//!   VLQ) + event. **Note-on (0x9n) carries an extra duration VLQ** after
//!   note/velocity (HMI extension). `0xFE` events are skipped with their own
//!   rules (0x10 → `data[4]+9`, 0x15 → 8, rest → 4) and **do not** alter the
//!   running status; sysex (0xF0/0xF7) does reset it; neither does 0xFF meta.
//!   End of track = meta 0xFF 0x2F (EOT).

/// HMI container signatures.
pub const HMI_SONG_MAGIC: &[u8; 18] = b"HMI-MIDISONG061595";
/// HMI container signatures (track).
pub const HMI_TRACK_MAGIC: &[u8; 13] = b"HMI-MIDITRACK";

/// Fixed offset of the song header.
mod hdr {
    /// `division` (ticks per quarter note) as declared by [`parse_hmi`]'s
    /// legacy header view — **0 in every one of the 131 retail songs**.
    pub const DIVISION: usize = 0xD0;
    /// Beats per minute as declared by [`parse_hmi`]'s legacy header view
    /// (holds the division of the retail files, see [`DIVISION_RETAIL`]).
    pub const BPM: usize = 0xD2;
    /// Ticks per quarter note **measured in retail `MIDI.BSA`** @0xD2
    /// (131/131 songs: 480). The production renderer takes the division from
    /// here because [`DIVISION`] is 0 in the retail files.
    pub const DIVISION_RETAIL: usize = 0xD2;
    /// Beats per minute **measured in retail `MIDI.BSA`** @0xD4
    /// (131/131 songs: 120).
    pub const BPM_RETAIL: usize = 0xD4;
    /// Number of tracks (u8).
    pub const TRACK_COUNT: usize = 0xE4;
    /// Base of the track offset table (u32 LE × track_count).
    pub const TRACK_OFFSETS: usize = 0x172;
}

/// Fixed offset of a track header (relative to the start of the track).
mod trk {
    /// `track_header_size` u32 LE — events start at `start + size`.
    pub const HEADER_SIZE: usize = 0x57;
}

/// HMI parse errors — every failure path returns `Result`
/// (0 panic on any input, corrupt included).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HmiError {
    /// File shorter than the song header.
    TooShort { len: usize },
    /// Song magic different from `HMI-MIDISONG061595`.
    BadSongMagic { found: Vec<u8> },
    /// `track_count == 0` or offset table outside the file.
    BadTrackTable { track_count: u8, len: usize },
    /// Track offset outside the file or missing `HMI-MIDITRACK` magic.
    BadTrackHeader { track: usize, offset: u32, len: usize },
    /// `track_header_size` pushes the events outside the file.
    EventsOutOfBounds { track: usize, start: u64, len: usize },
    /// The buffer ran out during an event (truncated/corrupt file).
    Truncated { track: usize, at: usize },
    /// Running status without a prior status (illegal event).
    RunningStatusWithoutPrior { track: usize, at: usize },
    /// Unknown status byte (neither a MIDI command nor a data VLQ).
    BadStatus { track: usize, at: usize, status: u8 },
    /// The track did not end with the EOT meta (0xFF 0x2F) inside the file.
    MissingEndOfTrack { track: usize },
}

impl std::fmt::Display for HmiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TooShort { len } => write!(f, "HMI too short for the header: {len} bytes"),
            Self::BadSongMagic { found } => write!(
                f,
                "wrong song magic: {:?} (expected HMI-MIDISONG061595)",
                String::from_utf8_lossy(found)
            ),
            Self::BadTrackTable { track_count, len } => {
                write!(f, "invalid track table: track_count={track_count} len={len}")
            }
            Self::BadTrackHeader { track, offset, len } => write!(
                f,
                "invalid track {track} header: offset {offset} > {len} or missing magic"
            ),
            Self::EventsOutOfBounds { track, start, len } => write!(
                f,
                "track {track} events out of bounds: start={start} > {len}"
            ),
            Self::Truncated { track, at } => {
                write!(f, "track {track}: truncated data at offset {at:#x}")
            }
            Self::RunningStatusWithoutPrior { track, at } => {
                write!(f, "track {track}: running status without prior status at {at:#x}")
            }
            Self::BadStatus { track, at, status } => {
                write!(f, "track {track}: illegal status byte {status:#04x} at {at:#x}")
            }
            Self::MissingEndOfTrack { track } => {
                write!(f, "track {track}: no EOT meta (0xFF 0x2F) inside the file")
            }
        }
    }
}

impl std::error::Error for HmiError {}

/// Decoded statistics of a track (full decode, events not retained).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TrackStats {
    /// Number of events consumed (excludes the final EOT).
    pub events: u64,
    /// Note-ons with velocity > 0.
    pub note_ons: u64,
}

/// Parsed HMI song: header + per-track statistics (full decode of
/// every event — any format violation is `Err`, never panic).
#[derive(Debug, Clone)]
pub struct HmiSong {
    /// MIDI division (ticks per quarter note) @0xD0.
    pub division: u16,
    /// Declared beats per minute @0xD2.
    pub bpm: u16,
    /// Number of tracks @0xE4.
    pub track_count: u8,
    /// Per-track statistics, in file order.
    pub tracks: Vec<TrackStats>,
}

impl HmiSong {
    /// Total events decoded across all tracks.
    pub fn total_events(&self) -> u64 {
        self.tracks.iter().map(|t| t.events).sum()
    }

    /// Total note-ons (velocity > 0) across all tracks.
    pub fn total_note_ons(&self) -> u64 {
        self.tracks.iter().map(|t| t.note_ons).sum()
    }
}

/// One retained note-on of an HMI song (M14 production playback).
///
/// HMI note-on events carry the note **duration** as an extra VLQ after
/// note/velocity (this format has no separate note-off), so a single event is
/// enough to schedule a rendered note.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NoteEvent {
    /// Track index in file order.
    pub track: u16,
    /// Absolute tick of the note-on (sum of the delta-times of its track).
    pub tick: u64,
    /// MIDI note number (0..=127).
    pub note: u8,
    /// Note-on velocity (> 0 — velocity 0 is never retained).
    pub velocity: u8,
    /// Duration in ticks, as carried by the HMI note-on.
    pub duration_ticks: u64,
    /// MIDI channel (`status & 0x0F`).
    pub channel: u8,
}

/// Full decode of an HMI song with the note events **retained**
/// ([`parse_hmi`] keeps only per-track statistics).
///
/// The header, track table and every event are validated with the same rules
/// as [`parse_hmi`]; a format violation is an `HmiError`, never a panic.
#[derive(Debug, Clone)]
pub struct HmiSongEvents {
    /// Ticks per quarter note **as measured in retail `MIDI.BSA`** (@0xD2;
    /// 131/131 songs: 480). The legacy `parse_hmi` field @0xD0 is 0 there.
    pub ticks_per_quarter: u16,
    /// Beats per minute **as measured in retail `MIDI.BSA`** (@0xD4;
    /// 131/131 songs: 120).
    pub bpm: u16,
    /// Number of tracks @0xE4.
    pub track_count: u8,
    /// Per-track statistics, in file order.
    pub tracks: Vec<TrackStats>,
    /// Retained note-ons, in decode order (track by track, time ascending
    /// inside each track).
    pub notes: Vec<NoteEvent>,
    /// Last tick reached by any track (its EOT delta included).
    pub end_tick: u64,
}

impl HmiSongEvents {
    /// Number of retained note-ons.
    pub fn total_note_ons(&self) -> usize {
        self.notes.len()
    }

    /// Length of the song in seconds at the retail tempo
    /// (`ticks_per_quarter` and `bpm` are validated by the renderer;
    /// `0` ⇒ `0.0`).
    pub fn duration_secs(&self) -> f64 {
        if self.ticks_per_quarter == 0 || self.bpm == 0 {
            return 0.0;
        }
        let seconds_per_tick =
            60.0 / (f64::from(self.bpm) * f64::from(self.ticks_per_quarter));
        self.end_tick as f64 * seconds_per_tick
    }
}

/// Read cursor with bounds checking (0 panic: every failure → `HmiError::Truncated`).
struct Cursor<'a> {
    data: &'a [u8],
    pos: usize,
    track: usize,
}

impl<'a> Cursor<'a> {
    fn new(data: &'a [u8], pos: usize, track: usize) -> Self {
        Self { data, pos, track }
    }

    fn u8(&mut self) -> Result<u8, HmiError> {
        let b = *self
            .data
            .get(self.pos)
            .ok_or(HmiError::Truncated { track: self.track, at: self.pos })?;
        self.pos += 1;
        Ok(b)
    }

    fn take(&mut self, n: usize) -> Result<&'a [u8], HmiError> {
        let end = self.pos.checked_add(n).ok_or(HmiError::Truncated { track: self.track, at: self.pos })?;
        let slice = self
            .data
            .get(self.pos..end)
            .ok_or(HmiError::Truncated { track: self.track, at: self.pos })?;
        self.pos = end;
        Ok(slice)
    }

    /// HMI/SMF VLQ: bit7=1 in every byte except the last; MSB-first.
    fn vlq(&mut self) -> Result<u64, HmiError> {
        let mut val: u64 = 0;
        loop {
            let b = self.u8()?;
            val = (val << 7) | u64::from(b & 0x7F);
            if b & 0x80 == 0 {
                return Ok(val);
            }
        }
    }
}

/// Number of data bytes of a channel message (0x8..0xE).
fn channel_data_len(status: u8) -> usize {
    match status >> 4 {
        0xC | 0xD => 1,
        _ => 2,
    }
}

/// Decodes the events of a track from `start` up to EOT.
/// Returns the statistics and the track's last tick; the final pointer is
/// validated against EOT (the file is not consumed beyond the track itself).
fn decode_track(song: &[u8], start: usize, track: usize) -> Result<(TrackStats, u64), HmiError> {
    decode_track_inner(song, start, track, None)
}

/// Decodes a track retaining its note-ons when `notes` is `Some` (the
/// [`parse_hmi_events`] path). Identical rules and error mapping as the
/// statistics-only decode of [`parse_hmi`].
fn decode_track_inner(
    song: &[u8],
    start: usize,
    track: usize,
    mut notes: Option<&mut Vec<NoteEvent>>,
) -> Result<(TrackStats, u64), HmiError> {
    let mut cur = Cursor::new(song, start, track);
    let mut stats = TrackStats::default();
    let mut prior_status: Option<u8> = None;
    // Absolute tick of this track: running sum of its delta-times.
    let mut tick: u64 = 0;
    loop {
        // Time delta (accumulated only to validate the VLQ; the decode is
        // enough for the probe: structure + counts).
        tick = tick.saturating_add(cur.vlq()?);
        let at = cur.pos;
        let b = cur.u8()?;
        if b == 0xFE {
            // HMI-specific event: 0x10 → total size+9 · 0x15 → total 8 ·
            // rest → total 4 (the "total" includes 0xFE and the kind byte).
            // Skipped WITHOUT altering the running status (WildMidi spec).
            let kind = cur.u8()?;
            let (skip_total, consumed) = match kind {
                0x10 => {
                    // Complete event (0xFE,0x10,d1,d2,size): the size lives
                    // in the 5th byte and the total is size+9.
                    cur.u8()?; // d1
                    cur.u8()?; // d2
                    let size = cur.u8()?;
                    (usize::from(size) + 9, 5)
                }
                0x15 => (8, 2),
                _ => (4, 2),
            };
            cur.take(skip_total.saturating_sub(consumed))?;
            stats.events += 1;
            continue;
        }
        if b < 0x80 {
            // Running status: reuses the prior status (the byte is data).
            let status = prior_status
                .ok_or(HmiError::RunningStatusWithoutPrior { track, at })?;
            cur.pos = at;
            let data_start = cur.pos;
            cur.take(channel_data_len(status))?;
            if status >> 4 == 0x9 {
                let duration_ticks = cur.vlq()?;
                if song[data_start + 1] > 0 {
                    stats.note_ons += 1;
                    if let Some(vec) = notes.as_deref_mut() {
                        vec.push(NoteEvent {
                            track: track as u16,
                            tick,
                            note: song[data_start],
                            velocity: song[data_start + 1],
                            duration_ticks,
                            channel: status & 0x0F,
                        });
                    }
                }
            }
            stats.events += 1;
            continue;
        }
        let status = b;
        match status >> 4 {
            0x8..=0xE => {
                prior_status = Some(status);
                let data_start = cur.pos;
                cur.take(channel_data_len(status))?;
                if status >> 4 == 0x9 {
                    let duration_ticks = cur.vlq()?;
                    if song[data_start + 1] > 0 {
                        stats.note_ons += 1;
                        if let Some(vec) = notes.as_deref_mut() {
                            vec.push(NoteEvent {
                                track: track as u16,
                                tick,
                                note: song[data_start],
                                velocity: song[data_start + 1],
                                duration_ticks,
                                channel: status & 0x0F,
                            });
                        }
                    }
                }
                stats.events += 1;
            }
            0xF => match status {
                0xFF => {
                    // Meta: type + VLQ len + data; 0x2F = EOT (end of track).
                    let meta_type = cur.u8()?;
                    let len = cur.vlq()?;
                    let len = usize::try_from(len).map_err(|_| HmiError::Truncated { track, at })?;
                    cur.take(len)?;
                    if meta_type == 0x2F {
                        return Ok((stats, tick));
                    }
                    stats.events += 1;
                }
                0xF0 | 0xF7 => {
                    // Sysex: VLQ len + data; resets the running status.
                    prior_status = None;
                    let len = cur.vlq()?;
                    let len = usize::try_from(len).map_err(|_| HmiError::Truncated { track, at })?;
                    cur.take(len)?;
                    stats.events += 1;
                }
                _ => return Err(HmiError::BadStatus { track, at, status }),
            },
            _ => return Err(HmiError::BadStatus { track, at, status }),
        }
    }
}

/// Validated HMI song header + the first-event offset of every track.
struct HmiHeader {
    division: u16,
    bpm: u16,
    track_count: u8,
    events_start: Vec<usize>,
}

/// Validates the song header and the track table and resolves each track's
/// first event offset. The checks (and their order) are the ones
/// [`parse_hmi`] has always enforced; both public parsers share them.
fn parse_header(song: &[u8]) -> Result<HmiHeader, HmiError> {
    if song.len() < 0x176 {
        return Err(HmiError::TooShort { len: song.len() });
    }
    if &song[..18] != HMI_SONG_MAGIC {
        return Err(HmiError::BadSongMagic { found: song[..18].to_vec() });
    }
    let division = u16::from_le_bytes([song[hdr::DIVISION], song[hdr::DIVISION + 1]]);
    let bpm = u16::from_le_bytes([song[hdr::BPM], song[hdr::BPM + 1]]);
    let track_count = song[hdr::TRACK_COUNT];
    if track_count == 0 {
        return Err(HmiError::BadTrackTable { track_count, len: song.len() });
    }
    let table_end = hdr::TRACK_OFFSETS
        .checked_add(usize::from(track_count) * 4)
        .ok_or(HmiError::BadTrackTable { track_count, len: song.len() })?;
    if table_end > song.len() {
        return Err(HmiError::BadTrackTable { track_count, len: song.len() });
    }
    let mut events_start = Vec::with_capacity(usize::from(track_count));
    for k in 0..usize::from(track_count) {
        let off = hdr::TRACK_OFFSETS + k * 4;
        let offset = u32::from_le_bytes([song[off], song[off + 1], song[off + 2], song[off + 3]]);
        let start = usize::try_from(offset)
            .ok()
            .filter(|o| *o < song.len())
            .ok_or(HmiError::BadTrackHeader { track: k, offset, len: song.len() })?;
        if &song[start..start + HMI_TRACK_MAGIC.len()] != HMI_TRACK_MAGIC {
            return Err(HmiError::BadTrackHeader { track: k, offset, len: song.len() });
        }
        let hs = start + trk::HEADER_SIZE;
        if hs + 4 > song.len() {
            return Err(HmiError::EventsOutOfBounds { track: k, start: hs as u64, len: song.len() });
        }
        let header_size = u32::from_le_bytes([song[hs], song[hs + 1], song[hs + 2], song[hs + 3]]);
        let events_start_at = start
            .checked_add(usize::try_from(header_size).map_err(|_| HmiError::EventsOutOfBounds {
                track: k,
                start: hs as u64,
                len: song.len(),
            })?)
            .ok_or(HmiError::EventsOutOfBounds { track: k, start: hs as u64, len: song.len() })?;
        if events_start_at >= song.len() {
            return Err(HmiError::EventsOutOfBounds { track: k, start: events_start_at as u64, len: song.len() });
        }
        events_start.push(events_start_at);
    }
    Ok(HmiHeader { division, bpm, track_count, events_start })
}

/// Maps a decode error of a track to `MissingEndOfTrack` when the buffer ran
/// out at the very end of the file (the historical [`parse_hmi`] mapping).
fn track_error(error: HmiError, len: usize) -> HmiError {
    match error {
        HmiError::Truncated { track, at } if at >= len => HmiError::MissingEndOfTrack { track },
        other => other,
    }
}

/// Parses a complete HMI song from its buffer (read-only, 0 copies).
/// Validates the header and track table and decodes **all** events of
/// **all** tracks up to EOT — any violation is `HmiError`.
pub fn parse_hmi(song: &[u8]) -> Result<HmiSong, HmiError> {
    let header = parse_header(song)?;
    let mut tracks = Vec::with_capacity(header.events_start.len());
    for (k, start) in header.events_start.iter().enumerate() {
        let (stats, _) =
            decode_track(song, *start, k).map_err(|e| track_error(e, song.len()))?;
        tracks.push(stats);
    }
    Ok(HmiSong {
        division: header.division,
        bpm: header.bpm,
        track_count: header.track_count,
        tracks,
    })
}

/// Additive decode of an HMI song that **retains** every note-on
/// ([`NoteEvent`]) plus the song's last tick — the read the production
/// renderer consumes. [`parse_hmi`] keeps its statistics-only contract; both
/// share [`parse_header`] and the same event rules, so a song that parses in
/// one parses in the other.
pub fn parse_hmi_events(song: &[u8]) -> Result<HmiSongEvents, HmiError> {
    let header = parse_header(song)?;
    // Retail tempo: @0xD2 ticks-per-quarter, @0xD4 bpm (measured 131/131).
    // `parse_header` already proved the header is long enough to index these,
    // but the read stays checked (0 panic on a truncated buffer).
    let tempo = song
        .get(hdr::DIVISION_RETAIL..hdr::BPM_RETAIL + 2)
        .ok_or(HmiError::TooShort { len: song.len() })?;
    let ticks_per_quarter = u16::from_le_bytes([tempo[0], tempo[1]]);
    let bpm = u16::from_le_bytes([tempo[2], tempo[3]]);
    let mut tracks = Vec::with_capacity(header.events_start.len());
    let mut notes = Vec::new();
    let mut end_tick = 0u64;
    for (k, start) in header.events_start.iter().enumerate() {
        let (stats, last_tick) = decode_track_inner(song, *start, k, Some(&mut notes))
            .map_err(|e| track_error(e, song.len()))?;
        end_tick = end_tick.max(last_tick);
        tracks.push(stats);
    }
    Ok(HmiSongEvents {
        ticks_per_quarter,
        bpm,
        track_count: header.track_count,
        tracks,
        notes,
        end_tick,
    })
}

/// DFU's `SongFiles` catalogue (`DFU:Assets/Scripts/SongFiles.cs`, MIT) — the
/// songs of the original. The discriminant is the song index in the DFU
/// enum (1:1 parity); [`SongFiles::record_name`] resolves the record name
/// inside MIDI.BSA (`EnumToFilename` of `DaggerfallSongPlayer` adapted to the
/// classic `.HMI`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[allow(non_camel_case_types, dead_code)]
pub enum SongFiles {
    /// No song (DFU: `song_none = -1`).
    song_none = -1,
    song_02 = 0,
    song_02fm = 1,
    song_03 = 2,
    song_03fm = 3,
    song_04 = 4,
    song_04fm = 5,
    song_05 = 6,
    song_05fm = 7,
    song_5strong = 8,
    song_06 = 9,
    song_06fm = 10,
    song_07 = 11,
    song_07fm = 12,
    song_08 = 13,
    song_08fm = 14,
    song_09 = 15,
    song_09fm = 16,
    song_10 = 17,
    song_11 = 18,
    song_11fm = 19,
    song_12 = 20,
    song_12fm = 21,
    song_13 = 22,
    song_13fm = 23,
    song_15 = 24,
    song_15fm = 25,
    song_16 = 26,
    song_16fm = 27,
    song_17 = 28,
    song_17fm = 29,
    song_18 = 30,
    song_18fm = 31,
    song_20 = 32,
    song_20fm = 33,
    song_21 = 34,
    song_21fm = 35,
    song_22 = 36,
    song_22fm = 37,
    song_23 = 38,
    song_23fm = 39,
    song_25 = 40,
    song_25fm = 41,
    song_28 = 42,
    song_29 = 43,
    song_29fm = 44,
    song_30 = 45,
    song_30fm = 46,
    song_d1 = 47,
    song_d10 = 48,
    song_d10fm = 49,
    song_d1fm = 50,
    song_d2 = 51,
    song_d2fm = 52,
    song_d3 = 53,
    song_d3fm = 54,
    song_d4 = 55,
    song_d4fm = 56,
    song_d5 = 57,
    song_d5fm = 58,
    song_d6 = 59,
    song_d6fm = 60,
    song_d7 = 61,
    song_d7fm = 62,
    song_d8 = 63,
    song_d8fm = 64,
    song_d9 = 65,
    song_d9fm = 66,
    song_dungeon = 67,
    song_dungeon5 = 68,
    song_dungeon6 = 69,
    song_dungeon7 = 70,
    song_dungeon8 = 71,
    song_dungeon9 = 72,
    song_fbad = 73,
    song_fcurse = 74,
    song_fday___d = 75,
    song_fdngn10 = 76,
    song_fdngn11 = 77,
    song_fdungn4 = 78,
    song_fdungn9 = 79,
    song_feerie = 80,
    song_fgood = 81,
    song_fm_dngn1 = 82,
    song_fm_dngn2 = 83,
    song_fm_dngn3 = 84,
    song_fm_dngn4 = 85,
    song_fm_dngn5 = 86,
    song_fm_nite3 = 87,
    song_fm_rain = 88,
    song_fm_sqr_2 = 89,
    song_fm_sunny = 90,
    song_fm_swim2 = 91,
    song_fmover_c = 92,
    song_fmover_s = 93,
    song_fmsneak2 = 94,
    song_fneut = 95,
    song_folk1 = 96,
    song_folk2 = 97,
    song_folk3 = 98,
    song_fpalac = 99,
    song_fruins = 100,
    song_fsneak2 = 101,
    song_fsnow__b = 102,
    song_gbad = 103,
    song_gcurse = 104,
    song_gday___d = 105,
    song_gdngn10 = 106,
    song_gdngn11 = 107,
    song_gdungn4 = 108,
    song_gdungn9 = 109,
    song_geerie = 110,
    song_ggood = 111,
    song_gmage_3 = 112,
    song_gneut = 113,
    song_gpalac = 114,
    song_gruins = 115,
    song_gshop = 116,
    song_gsneak2 = 117,
    song_gsnow__b = 118,
    song_gsunny2 = 119,
    song_magic_2 = 120,
    song_overcast = 121,
    song_overlong = 122,
    song_oversnow = 123,
    song_raining = 124,
    song_sneaking = 125,
    song_sneakng2 = 126,
    song_snowing = 127,
    song_square_2 = 128,
    song_sunnyday = 129,
    song_swimming = 130,
    song_tavern = 131,
}

impl SongFiles {
    /// Enum name in DFU (`ToString()`), e.g. `"song_02"`.
    pub const fn name(self) -> &'static str {
        match self {
            Self::song_none => "song_none",
            Self::song_02 => "song_02",
            Self::song_02fm => "song_02fm",
            Self::song_03 => "song_03",
            Self::song_03fm => "song_03fm",
            Self::song_04 => "song_04",
            Self::song_04fm => "song_04fm",
            Self::song_05 => "song_05",
            Self::song_05fm => "song_05fm",
            Self::song_5strong => "song_5strong",
            Self::song_06 => "song_06",
            Self::song_06fm => "song_06fm",
            Self::song_07 => "song_07",
            Self::song_07fm => "song_07fm",
            Self::song_08 => "song_08",
            Self::song_08fm => "song_08fm",
            Self::song_09 => "song_09",
            Self::song_09fm => "song_09fm",
            Self::song_10 => "song_10",
            Self::song_11 => "song_11",
            Self::song_11fm => "song_11fm",
            Self::song_12 => "song_12",
            Self::song_12fm => "song_12fm",
            Self::song_13 => "song_13",
            Self::song_13fm => "song_13fm",
            Self::song_15 => "song_15",
            Self::song_15fm => "song_15fm",
            Self::song_16 => "song_16",
            Self::song_16fm => "song_16fm",
            Self::song_17 => "song_17",
            Self::song_17fm => "song_17fm",
            Self::song_18 => "song_18",
            Self::song_18fm => "song_18fm",
            Self::song_20 => "song_20",
            Self::song_20fm => "song_20fm",
            Self::song_21 => "song_21",
            Self::song_21fm => "song_21fm",
            Self::song_22 => "song_22",
            Self::song_22fm => "song_22fm",
            Self::song_23 => "song_23",
            Self::song_23fm => "song_23fm",
            Self::song_25 => "song_25",
            Self::song_25fm => "song_25fm",
            Self::song_28 => "song_28",
            Self::song_29 => "song_29",
            Self::song_29fm => "song_29fm",
            Self::song_30 => "song_30",
            Self::song_30fm => "song_30fm",
            Self::song_d1 => "song_d1",
            Self::song_d10 => "song_d10",
            Self::song_d10fm => "song_d10fm",
            Self::song_d1fm => "song_d1fm",
            Self::song_d2 => "song_d2",
            Self::song_d2fm => "song_d2fm",
            Self::song_d3 => "song_d3",
            Self::song_d3fm => "song_d3fm",
            Self::song_d4 => "song_d4",
            Self::song_d4fm => "song_d4fm",
            Self::song_d5 => "song_d5",
            Self::song_d5fm => "song_d5fm",
            Self::song_d6 => "song_d6",
            Self::song_d6fm => "song_d6fm",
            Self::song_d7 => "song_d7",
            Self::song_d7fm => "song_d7fm",
            Self::song_d8 => "song_d8",
            Self::song_d8fm => "song_d8fm",
            Self::song_d9 => "song_d9",
            Self::song_d9fm => "song_d9fm",
            Self::song_dungeon => "song_dungeon",
            Self::song_dungeon5 => "song_dungeon5",
            Self::song_dungeon6 => "song_dungeon6",
            Self::song_dungeon7 => "song_dungeon7",
            Self::song_dungeon8 => "song_dungeon8",
            Self::song_dungeon9 => "song_dungeon9",
            Self::song_fbad => "song_fbad",
            Self::song_fcurse => "song_fcurse",
            Self::song_fday___d => "song_fday___d",
            Self::song_fdngn10 => "song_fdngn10",
            Self::song_fdngn11 => "song_fdngn11",
            Self::song_fdungn4 => "song_fdungn4",
            Self::song_fdungn9 => "song_fdungn9",
            Self::song_feerie => "song_feerie",
            Self::song_fgood => "song_fgood",
            Self::song_fm_dngn1 => "song_fm_dngn1",
            Self::song_fm_dngn2 => "song_fm_dngn2",
            Self::song_fm_dngn3 => "song_fm_dngn3",
            Self::song_fm_dngn4 => "song_fm_dngn4",
            Self::song_fm_dngn5 => "song_fm_dngn5",
            Self::song_fm_nite3 => "song_fm_nite3",
            Self::song_fm_rain => "song_fm_rain",
            Self::song_fm_sqr_2 => "song_fm_sqr_2",
            Self::song_fm_sunny => "song_fm_sunny",
            Self::song_fm_swim2 => "song_fm_swim2",
            Self::song_fmover_c => "song_fmover_c",
            Self::song_fmover_s => "song_fmover_s",
            Self::song_fmsneak2 => "song_fmsneak2",
            Self::song_fneut => "song_fneut",
            Self::song_folk1 => "song_folk1",
            Self::song_folk2 => "song_folk2",
            Self::song_folk3 => "song_folk3",
            Self::song_fpalac => "song_fpalac",
            Self::song_fruins => "song_fruins",
            Self::song_fsneak2 => "song_fsneak2",
            Self::song_fsnow__b => "song_fsnow__b",
            Self::song_gbad => "song_gbad",
            Self::song_gcurse => "song_gcurse",
            Self::song_gday___d => "song_gday___d",
            Self::song_gdngn10 => "song_gdngn10",
            Self::song_gdngn11 => "song_gdngn11",
            Self::song_gdungn4 => "song_gdungn4",
            Self::song_gdungn9 => "song_gdungn9",
            Self::song_geerie => "song_geerie",
            Self::song_ggood => "song_ggood",
            Self::song_gmage_3 => "song_gmage_3",
            Self::song_gneut => "song_gneut",
            Self::song_gpalac => "song_gpalac",
            Self::song_gruins => "song_gruins",
            Self::song_gshop => "song_gshop",
            Self::song_gsneak2 => "song_gsneak2",
            Self::song_gsnow__b => "song_gsnow__b",
            Self::song_gsunny2 => "song_gsunny2",
            Self::song_magic_2 => "song_magic_2",
            Self::song_overcast => "song_overcast",
            Self::song_overlong => "song_overlong",
            Self::song_oversnow => "song_oversnow",
            Self::song_raining => "song_raining",
            Self::song_sneaking => "song_sneaking",
            Self::song_sneakng2 => "song_sneakng2",
            Self::song_snowing => "song_snowing",
            Self::song_square_2 => "song_square_2",
            Self::song_sunnyday => "song_sunnyday",
            Self::song_swimming => "song_swimming",
            Self::song_tavern => "song_tavern",
        }
    }

    /// Record name inside MIDI.BSA (DOS 8.3, e.g. `"02.HMI"`,
    /// `"GDAY___D.HMI"`). Rule = DFU's `EnumToFilename` (`song_` prefix
    /// removed, uppercase) + classic `.HMI` extension. `song_none`
    /// resolves no record.
    pub const fn record_name(self) -> Option<&'static str> {
        match self {
            Self::song_none => None,
            Self::song_02 => Some("02.HMI"),
            Self::song_02fm => Some("02FM.HMI"),
            Self::song_03 => Some("03.HMI"),
            Self::song_03fm => Some("03FM.HMI"),
            Self::song_04 => Some("04.HMI"),
            Self::song_04fm => Some("04FM.HMI"),
            Self::song_05 => Some("05.HMI"),
            Self::song_05fm => Some("05FM.HMI"),
            Self::song_5strong => Some("5STRONG.HMI"),
            Self::song_06 => Some("06.HMI"),
            Self::song_06fm => Some("06FM.HMI"),
            Self::song_07 => Some("07.HMI"),
            Self::song_07fm => Some("07FM.HMI"),
            Self::song_08 => Some("08.HMI"),
            Self::song_08fm => Some("08FM.HMI"),
            Self::song_09 => Some("09.HMI"),
            Self::song_09fm => Some("09FM.HMI"),
            Self::song_10 => Some("10.HMI"),
            Self::song_11 => Some("11.HMI"),
            Self::song_11fm => Some("11FM.HMI"),
            Self::song_12 => Some("12.HMI"),
            Self::song_12fm => Some("12FM.HMI"),
            Self::song_13 => Some("13.HMI"),
            Self::song_13fm => Some("13FM.HMI"),
            Self::song_15 => Some("15.HMI"),
            Self::song_15fm => Some("15FM.HMI"),
            Self::song_16 => Some("16.HMI"),
            Self::song_16fm => Some("16FM.HMI"),
            Self::song_17 => Some("17.HMI"),
            Self::song_17fm => Some("17FM.HMI"),
            Self::song_18 => Some("18.HMI"),
            Self::song_18fm => Some("18FM.HMI"),
            Self::song_20 => Some("20.HMI"),
            Self::song_20fm => Some("20FM.HMI"),
            Self::song_21 => Some("21.HMI"),
            Self::song_21fm => Some("21FM.HMI"),
            Self::song_22 => Some("22.HMI"),
            Self::song_22fm => Some("22FM.HMI"),
            Self::song_23 => Some("23.HMI"),
            Self::song_23fm => Some("23FM.HMI"),
            Self::song_25 => Some("25.HMI"),
            Self::song_25fm => Some("25FM.HMI"),
            Self::song_28 => Some("28.HMI"),
            Self::song_29 => Some("29.HMI"),
            Self::song_29fm => Some("29FM.HMI"),
            Self::song_30 => Some("30.HMI"),
            Self::song_30fm => Some("30FM.HMI"),
            Self::song_d1 => Some("D1.HMI"),
            Self::song_d10 => Some("D10.HMI"),
            Self::song_d10fm => Some("D10FM.HMI"),
            Self::song_d1fm => Some("D1FM.HMI"),
            Self::song_d2 => Some("D2.HMI"),
            Self::song_d2fm => Some("D2FM.HMI"),
            Self::song_d3 => Some("D3.HMI"),
            Self::song_d3fm => Some("D3FM.HMI"),
            Self::song_d4 => Some("D4.HMI"),
            Self::song_d4fm => Some("D4FM.HMI"),
            Self::song_d5 => Some("D5.HMI"),
            Self::song_d5fm => Some("D5FM.HMI"),
            Self::song_d6 => Some("D6.HMI"),
            Self::song_d6fm => Some("D6FM.HMI"),
            Self::song_d7 => Some("D7.HMI"),
            Self::song_d7fm => Some("D7FM.HMI"),
            Self::song_d8 => Some("D8.HMI"),
            Self::song_d8fm => Some("D8FM.HMI"),
            Self::song_d9 => Some("D9.HMI"),
            Self::song_d9fm => Some("D9FM.HMI"),
            Self::song_dungeon => Some("DUNGEON.HMI"),
            Self::song_dungeon5 => Some("DUNGEON5.HMI"),
            Self::song_dungeon6 => Some("DUNGEON6.HMI"),
            Self::song_dungeon7 => Some("DUNGEON7.HMI"),
            Self::song_dungeon8 => Some("DUNGEON8.HMI"),
            Self::song_dungeon9 => Some("DUNGEON9.HMI"),
            Self::song_fbad => Some("FBAD.HMI"),
            Self::song_fcurse => Some("FCURSE.HMI"),
            Self::song_fday___d => Some("FDAY___D.HMI"),
            Self::song_fdngn10 => Some("FDNGN10.HMI"),
            Self::song_fdngn11 => Some("FDNGN11.HMI"),
            Self::song_fdungn4 => Some("FDUNGN4.HMI"),
            Self::song_fdungn9 => Some("FDUNGN9.HMI"),
            Self::song_feerie => Some("FEERIE.HMI"),
            Self::song_fgood => Some("FGOOD.HMI"),
            Self::song_fm_dngn1 => Some("FM_DNGN1.HMI"),
            Self::song_fm_dngn2 => Some("FM_DNGN2.HMI"),
            Self::song_fm_dngn3 => Some("FM_DNGN3.HMI"),
            Self::song_fm_dngn4 => Some("FM_DNGN4.HMI"),
            Self::song_fm_dngn5 => Some("FM_DNGN5.HMI"),
            Self::song_fm_nite3 => Some("FM_NITE3.HMI"),
            Self::song_fm_rain => Some("FM_RAIN.HMI"),
            Self::song_fm_sqr_2 => Some("FM_SQR_2.HMI"),
            Self::song_fm_sunny => Some("FM_SUNNY.HMI"),
            Self::song_fm_swim2 => Some("FM_SWIM2.HMI"),
            Self::song_fmover_c => Some("FMOVER_C.HMI"),
            Self::song_fmover_s => Some("FMOVER_S.HMI"),
            Self::song_fmsneak2 => Some("FMSNEAK2.HMI"),
            Self::song_fneut => Some("FNEUT.HMI"),
            Self::song_folk1 => Some("FOLK1.HMI"),
            Self::song_folk2 => Some("FOLK2.HMI"),
            Self::song_folk3 => Some("FOLK3.HMI"),
            Self::song_fpalac => Some("FPALAC.HMI"),
            Self::song_fruins => Some("FRUINS.HMI"),
            Self::song_fsneak2 => Some("FSNEAK2.HMI"),
            Self::song_fsnow__b => Some("FSNOW__B.HMI"),
            Self::song_gbad => Some("GBAD.HMI"),
            Self::song_gcurse => Some("GCURSE.HMI"),
            Self::song_gday___d => Some("GDAY___D.HMI"),
            Self::song_gdngn10 => Some("GDNGN10.HMI"),
            Self::song_gdngn11 => Some("GDNGN11.HMI"),
            Self::song_gdungn4 => Some("GDUNGN4.HMI"),
            Self::song_gdungn9 => Some("GDUNGN9.HMI"),
            Self::song_geerie => Some("GEERIE.HMI"),
            Self::song_ggood => Some("GGOOD.HMI"),
            Self::song_gmage_3 => Some("GMAGE_3.HMI"),
            Self::song_gneut => Some("GNEUT.HMI"),
            Self::song_gpalac => Some("GPALAC.HMI"),
            Self::song_gruins => Some("GRUINS.HMI"),
            Self::song_gshop => Some("GSHOP.HMI"),
            Self::song_gsneak2 => Some("GSNEAK2.HMI"),
            Self::song_gsnow__b => Some("GSNOW__B.HMI"),
            Self::song_gsunny2 => Some("GSUNNY2.HMI"),
            Self::song_magic_2 => Some("MAGIC_2.HMI"),
            Self::song_overcast => Some("OVERCAST.HMI"),
            Self::song_overlong => Some("OVERLONG.HMI"),
            Self::song_oversnow => Some("OVERSNOW.HMI"),
            Self::song_raining => Some("RAINING.HMI"),
            Self::song_sneaking => Some("SNEAKING.HMI"),
            Self::song_sneakng2 => Some("SNEAKNG2.HMI"),
            Self::song_snowing => Some("SNOWING.HMI"),
            Self::song_square_2 => Some("SQUARE_2.HMI"),
            Self::song_sunnyday => Some("SUNNYDAY.HMI"),
            Self::song_swimming => Some("SWIMMING.HMI"),
            Self::song_tavern => Some("TAVERN.HMI"),
        }
    }
}

/// All catalogue entries (DFU enum order, without `song_none`).
pub const SONG_CATALOG: &[SongFiles] = &[
    SongFiles::song_02,
    SongFiles::song_02fm,
    SongFiles::song_03,
    SongFiles::song_03fm,
    SongFiles::song_04,
    SongFiles::song_04fm,
    SongFiles::song_05,
    SongFiles::song_05fm,
    SongFiles::song_5strong,
    SongFiles::song_06,
    SongFiles::song_06fm,
    SongFiles::song_07,
    SongFiles::song_07fm,
    SongFiles::song_08,
    SongFiles::song_08fm,
    SongFiles::song_09,
    SongFiles::song_09fm,
    SongFiles::song_10,
    SongFiles::song_11,
    SongFiles::song_11fm,
    SongFiles::song_12,
    SongFiles::song_12fm,
    SongFiles::song_13,
    SongFiles::song_13fm,
    SongFiles::song_15,
    SongFiles::song_15fm,
    SongFiles::song_16,
    SongFiles::song_16fm,
    SongFiles::song_17,
    SongFiles::song_17fm,
    SongFiles::song_18,
    SongFiles::song_18fm,
    SongFiles::song_20,
    SongFiles::song_20fm,
    SongFiles::song_21,
    SongFiles::song_21fm,
    SongFiles::song_22,
    SongFiles::song_22fm,
    SongFiles::song_23,
    SongFiles::song_23fm,
    SongFiles::song_25,
    SongFiles::song_25fm,
    SongFiles::song_28,
    SongFiles::song_29,
    SongFiles::song_29fm,
    SongFiles::song_30,
    SongFiles::song_30fm,
    SongFiles::song_d1,
    SongFiles::song_d10,
    SongFiles::song_d10fm,
    SongFiles::song_d1fm,
    SongFiles::song_d2,
    SongFiles::song_d2fm,
    SongFiles::song_d3,
    SongFiles::song_d3fm,
    SongFiles::song_d4,
    SongFiles::song_d4fm,
    SongFiles::song_d5,
    SongFiles::song_d5fm,
    SongFiles::song_d6,
    SongFiles::song_d6fm,
    SongFiles::song_d7,
    SongFiles::song_d7fm,
    SongFiles::song_d8,
    SongFiles::song_d8fm,
    SongFiles::song_d9,
    SongFiles::song_d9fm,
    SongFiles::song_dungeon,
    SongFiles::song_dungeon5,
    SongFiles::song_dungeon6,
    SongFiles::song_dungeon7,
    SongFiles::song_dungeon8,
    SongFiles::song_dungeon9,
    SongFiles::song_fbad,
    SongFiles::song_fcurse,
    SongFiles::song_fday___d,
    SongFiles::song_fdngn10,
    SongFiles::song_fdngn11,
    SongFiles::song_fdungn4,
    SongFiles::song_fdungn9,
    SongFiles::song_feerie,
    SongFiles::song_fgood,
    SongFiles::song_fm_dngn1,
    SongFiles::song_fm_dngn2,
    SongFiles::song_fm_dngn3,
    SongFiles::song_fm_dngn4,
    SongFiles::song_fm_dngn5,
    SongFiles::song_fm_nite3,
    SongFiles::song_fm_rain,
    SongFiles::song_fm_sqr_2,
    SongFiles::song_fm_sunny,
    SongFiles::song_fm_swim2,
    SongFiles::song_fmover_c,
    SongFiles::song_fmover_s,
    SongFiles::song_fmsneak2,
    SongFiles::song_fneut,
    SongFiles::song_folk1,
    SongFiles::song_folk2,
    SongFiles::song_folk3,
    SongFiles::song_fpalac,
    SongFiles::song_fruins,
    SongFiles::song_fsneak2,
    SongFiles::song_fsnow__b,
    SongFiles::song_gbad,
    SongFiles::song_gcurse,
    SongFiles::song_gday___d,
    SongFiles::song_gdngn10,
    SongFiles::song_gdngn11,
    SongFiles::song_gdungn4,
    SongFiles::song_gdungn9,
    SongFiles::song_geerie,
    SongFiles::song_ggood,
    SongFiles::song_gmage_3,
    SongFiles::song_gneut,
    SongFiles::song_gpalac,
    SongFiles::song_gruins,
    SongFiles::song_gshop,
    SongFiles::song_gsneak2,
    SongFiles::song_gsnow__b,
    SongFiles::song_gsunny2,
    SongFiles::song_magic_2,
    SongFiles::song_overcast,
    SongFiles::song_overlong,
    SongFiles::song_oversnow,
    SongFiles::song_raining,
    SongFiles::song_sneaking,
    SongFiles::song_sneakng2,
    SongFiles::song_snowing,
    SongFiles::song_square_2,
    SongFiles::song_sunnyday,
    SongFiles::song_swimming,
    SongFiles::song_tavern,
];

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds a minimal synthetic HMI song (1 track, note-on + EOT) —
    /// 0 retail bytes (INV-L99-6).
    fn build_hmi() -> Vec<u8> {
        let track_start = 0x1C0u32;
        let header_size = 0xB7u32;
        let mut out = vec![0u8; 0x280];
        out[..18].copy_from_slice(HMI_SONG_MAGIC);
        out[hdr::DIVISION..hdr::DIVISION + 2].copy_from_slice(&480u16.to_le_bytes());
        out[hdr::BPM..hdr::BPM + 2].copy_from_slice(&120u16.to_le_bytes());
        out[hdr::TRACK_COUNT] = 1;
        out[hdr::TRACK_OFFSETS..hdr::TRACK_OFFSETS + 4].copy_from_slice(&track_start.to_le_bytes());
        let ts = track_start as usize;
        out[ts..ts + 13].copy_from_slice(HMI_TRACK_MAGIC);
        out[ts + trk::HEADER_SIZE..ts + trk::HEADER_SIZE + 4]
            .copy_from_slice(&header_size.to_le_bytes());
        let ev = ts + header_size as usize;
        // delta 0 · note-on ch0 note 60 vel 100 · duration VLQ 10.
        out[ev] = 0x00;
        out[ev + 1] = 0x90;
        out[ev + 2] = 60;
        out[ev + 3] = 100;
        out[ev + 4] = 10;
        // delta 0 · meta EOT.
        out[ev + 5] = 0x00;
        out[ev + 6] = 0xFF;
        out[ev + 7] = 0x2F;
        out[ev + 8] = 0x00;
        out
    }

    /// Same minimal song, but with the **retail layout** of the tempo fields
    /// (@0xD0 = 0, @0xD2 = 480 ticks/quarter, @0xD4 = 120 bpm), which is what
    /// the event carrier/renderer consume.
    fn build_hmi_retail() -> Vec<u8> {
        let mut song = build_hmi();
        song[hdr::DIVISION..hdr::DIVISION + 2].copy_from_slice(&0u16.to_le_bytes());
        song[hdr::DIVISION_RETAIL..hdr::DIVISION_RETAIL + 2]
            .copy_from_slice(&480u16.to_le_bytes());
        song[hdr::BPM_RETAIL..hdr::BPM_RETAIL + 2].copy_from_slice(&120u16.to_le_bytes());
        song
    }

    #[test]
    fn parses_minimal_hmi() {
        let song = build_hmi();
        let parsed = parse_hmi(&song).expect("parse");
        assert_eq!(parsed.division, 480);
        assert_eq!(parsed.bpm, 120);
        assert_eq!(parsed.track_count, 1);
        assert_eq!(parsed.total_events(), 1);
        assert_eq!(parsed.total_note_ons(), 1);
    }

    #[test]
    fn rejects_bad_magic() {
        let mut song = build_hmi();
        song[0] = b'X';
        assert!(matches!(parse_hmi(&song), Err(HmiError::BadSongMagic { .. })));
    }

    #[test]
    fn rejects_too_short() {
        assert!(matches!(parse_hmi(&[0u8; 8]), Err(HmiError::TooShort { .. })));
    }

    #[test]
    fn rejects_zero_tracks() {
        let mut song = build_hmi();
        song[hdr::TRACK_COUNT] = 0;
        assert!(matches!(parse_hmi(&song), Err(HmiError::BadTrackTable { .. })));
    }

    #[test]
    fn rejects_missing_eot() {
        let mut song = build_hmi();
        // Swaps the EOT for an extra note-on without end of track.
        let ev = 0x1C0 + 0xB7;
        song[ev + 5] = 0x00;
        song[ev + 6] = 0x90;
        song[ev + 7] = 61;
        song[ev + 8] = 90;
        // and leaves an unreadable EOT by truncating the file before the next byte.
        song.truncate(ev + 9);
        assert!(parse_hmi(&song).is_err());
    }

    #[test]
    fn running_status_with_length() {
        let mut song = build_hmi();
        let ev = 0x1C0 + 0xB7;
        song.resize(ev + 14, 0u8);
        // Replaces the original EOT: delta 0 · note-on ch0 with running status
        // (no status byte — reuses the previous 0x90) + duration VLQ 20.
        song[ev + 5] = 0x00;
        song[ev + 6] = 62;
        song[ev + 7] = 80;
        song[ev + 8] = 20;
        // delta 0 · meta EOT after the running status.
        song[ev + 9] = 0x00;
        song[ev + 10] = 0xFF;
        song[ev + 11] = 0x2F;
        song[ev + 12] = 0x00;
        let parsed = parse_hmi(&song).expect("parse");
        assert_eq!(parsed.total_events(), 2);
        assert_eq!(parsed.total_note_ons(), 2);
    }

    #[test]
    fn hmi_fe_event_skip() {
        let mut song = build_hmi();
        let ev = 0x1C0 + 0xB7;
        song.resize(ev + 14, 0u8);
        // Replaces the original EOT with: delta 0 · HMI event 0xFE (kind 0x14
        // → total 4: 0xFE + kind + 2 data bytes), WITHOUT altering running status.
        song[ev + 5] = 0x00;
        song[ev + 6] = 0xFE;
        song[ev + 7] = 0x14;
        song[ev + 8] = 0xAA;
        song[ev + 9] = 0xBB;
        // delta 0 · meta EOT after the HMI event.
        song[ev + 10] = 0x00;
        song[ev + 11] = 0xFF;
        song[ev + 12] = 0x2F;
        song[ev + 13] = 0x00;
        let parsed = parse_hmi(&song).expect("parse with HMI event 0xFE");
        assert_eq!(parsed.total_events(), 2);
        assert_eq!(parsed.total_note_ons(), 1);
    }

    #[test]
    fn retained_notes_match_statistics_decode() {
        let song = build_hmi_retail();
        let events = parse_hmi_events(&song).expect("parse events");
        assert_eq!(events.track_count, 1);
        assert_eq!(events.ticks_per_quarter, 480);
        assert_eq!(events.bpm, 120);
        assert_eq!(events.total_note_ons(), 1);
        let note = events.notes[0];
        assert_eq!(note.track, 0);
        assert_eq!(note.tick, 0);
        assert_eq!(note.note, 60);
        assert_eq!(note.velocity, 100);
        assert_eq!(note.duration_ticks, 10);
        assert_eq!(note.channel, 0);
        assert_eq!(events.end_tick, 0);
        // Same acceptance set as the statistics-only parser: a song that
        // parses here parses there, with the same note-on count.
        let stats = parse_hmi(&song).expect("parse stats");
        assert_eq!(stats.total_note_ons(), events.total_note_ons() as u64);
        assert_eq!(stats.total_events(), events.tracks[0].events);
    }

    #[test]
    fn retained_notes_accumulate_delta_ticks() {
        let mut song = build_hmi_retail();
        let ev = 0x1C0 + 0xB7;
        song[ev] = 0x60; // 96-tick delta before the note-on
        let events = parse_hmi_events(&song).expect("parse events");
        assert_eq!(events.notes[0].tick, 96);
        assert_eq!(events.end_tick, 96);
        // 96 ticks at 480 ticks/quarter and 120 bpm = 0.1 s.
        assert!((events.duration_secs() - 0.1).abs() < 1e-9);
    }

    #[test]
    fn retained_notes_follow_running_status() {
        let mut song = build_hmi();
        let ev = 0x1C0 + 0xB7;
        song.resize(ev + 14, 0u8);
        // Replaces the original EOT with a running-status note-on (no status
        // byte) + duration 20, then the EOT meta.
        song[ev + 5] = 0x00;
        song[ev + 6] = 62;
        song[ev + 7] = 80;
        song[ev + 8] = 20;
        song[ev + 9] = 0x00;
        song[ev + 10] = 0xFF;
        song[ev + 11] = 0x2F;
        song[ev + 12] = 0x00;
        let events = parse_hmi_events(&song).expect("parse events");
        assert_eq!(events.notes.len(), 2);
        assert_eq!(events.notes[1].note, 62);
        assert_eq!(events.notes[1].velocity, 80);
        assert_eq!(events.notes[1].duration_ticks, 20);
        assert_eq!(events.notes[1].tick, 0);
        assert_eq!(events.notes[1].channel, 0);
    }

    #[test]
    fn events_decode_rejects_bad_magic() {
        let mut song = build_hmi();
        song[0] = b'X';
        assert!(matches!(parse_hmi_events(&song), Err(HmiError::BadSongMagic { .. })));
        assert!(matches!(parse_hmi_events(&[0u8; 8]), Err(HmiError::TooShort { .. })));
    }

    #[test]
    fn catalog_record_names_are_unique() {
        let mut seen = std::collections::HashSet::new();
        for song in SONG_CATALOG {
            let name = song.record_name().expect("catalogue without song_none");
            assert!(seen.insert(name), "duplicate record: {name}");
        }
        assert_eq!(seen.len(), SONG_CATALOG.len());
    }

    #[test]
    fn catalog_discriminants_match_dfu_order() {
        // 1:1 parity with the DFU enum: song_02=0 … song_tavern=131 (132 songs).
        assert_eq!(SongFiles::song_02 as i32, 0);
        assert_eq!(SongFiles::song_tavern as i32, 131);
        assert_eq!(SongFiles::song_none as i32, -1);
        assert_eq!(SONG_CATALOG.len(), 132);
        assert_eq!(SONG_CATALOG[8], SongFiles::song_5strong);
        assert_eq!(SONG_CATALOG[105], SongFiles::song_gday___d);
    }
}
