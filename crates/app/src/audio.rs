//! **Production audio** (M14, REQ-L99-045/046) — the playback path the game
//! uses, not only the probe.
//!
//! **Effects (REQ-L99-046).** [`SfxBank`] parses the retail `DAGGER.SND` at
//! runtime (Mode A custody, INV-L99-6: 0 packaged bytes) and resolves every
//! [`snd::SoundClips`] member by its **positional directory index** — the DFU
//! dispatch (`SoundReader.cs:59-101` → `SndFile.GetSound(index)`,
//! `SndFile.cs:158-176`). The entry's own `RecordID` is metadata and is never
//! used as the key. Each clip becomes a 44-byte RIFF/WAVE header + raw 8-bit
//! unsigned mono PCM at 11025 Hz (`snd::SoundClip::wave_header`).
//! [`AudioEngine`] queues [`SfxRequest`]s, resolves them on [`AudioEngine::drain`]
//! and hands the WAV to a [`SfxSink`]:
//! - [`NullSink`] — headless, accepts everything (0 fake audio);
//! - [`RecordingSink`] — headless evidence: sample count + FNV-1a64 digest;
//! - `RodioSink` — real device playback, additive under feature `audio`.
//!
//! **Music (REQ-L99-045).** [`render_song`] turns an HMI song (MIDI.BSA)
//! into a WAV rendered by a deterministic square-wave synth —
//! [`midi::parse_hmi_events`] retains the note-ons with their tick and
//! duration. [`music_context_song`] selects the track exactly like DFU
//! `SongManager.SelectCurrentSong` (playlist by context + `DFRandom.srand(gameDays)`)
//! and [`MusicBank`] resolves and renders it from the retail `MIDI.BSA`.
//!
//! **Declared delta:** this is *not* an OPL2/GM synthesiser — the timbre is a
//! square wave, so the melody/rhythm are faithful while the instrument
//! timbre is not. Playback still requires the user's retail files: with
//! `DAGGER.SND` absent `SfxBank::from_arena2` is an error and the runtime
//! degrades to `NullSink` with an explicit warning (never a panic, never a
//! fabricated clip).

use std::path::{Path, PathBuf};

use daggerfall_parser_rs::{bsa, dfrandom::DfRandom, midi, snd};

/// Retail effects bank read by [`SfxBank::from_arena2`].
pub const SOUND_BANK_FILE: &str = "DAGGER.SND";
/// Retail music bank read by [`MusicBank::from_arena2`].
pub const MUSIC_BANK_FILE: &str = "MIDI.BSA";
/// RIFF/WAVE header length produced by the synthesisers (8-bit mono PCM).
pub const WAVE_HEADER_LEN: usize = 44;
/// Volume of the music sink (DFU mixes songs below the effects).
pub const MUSIC_GAIN: f32 = 0.35;
/// Upper bound of a rendered song (a corrupt `end_tick` must not allocate
/// gigabytes: the longest retail song is far below this).
pub const MAX_RENDER_SECONDS: f64 = 600.0;
/// Peak amplitude of one rendered note around the 8-bit centre (128).
pub const NOTE_PEAK: i32 = 20;

/// Every failure of the production audio path — typed, with `Display`; the
/// runtime never panics and degrades with an explicit warning.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AudioError {
    /// The Mode A file is not resolvable (absent root/file).
    MissingFile { name: String, reason: String },
    /// I/O failure while reading a retail bank.
    Io { path: PathBuf, reason: String },
    /// `DAGGER.SND` container/record failure.
    Snd(snd::SndError),
    /// BSA container failure (`MIDI.BSA`).
    Bsa(bsa::BsaError),
    /// HMI decode failure.
    Midi(midi::HmiError),
    /// The clip carries no directory index (`SoundClips::None = -1`).
    NoClip { clip: snd::SoundClips },
    /// The song has no record in the bank (`SongFiles::song_none`, or absent).
    MissingSong { song: midi::SongFiles },
    /// The song declares a tempo the renderer cannot divide by.
    BadTempo { division: u16, bpm: u16 },
    /// The audio device/decoder failed (real playback only).
    Device { reason: String },
}

impl std::fmt::Display for AudioError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingFile { name, reason } => {
                write!(f, "{name} not available: {reason}")
            }
            Self::Io { path, reason } => write!(f, "read {path:?}: {reason}"),
            Self::Snd(err) => write!(f, "effects bank: {err}"),
            Self::Bsa(err) => write!(f, "music bank: {err}"),
            Self::Midi(err) => write!(f, "HMI decode: {err}"),
            Self::NoClip { clip } => {
                write!(f, "SoundClips::{} has no directory index", clip.name())
            }
            Self::MissingSong { song } => {
                write!(f, "song {} has no record in the bank", song.name())
            }
            Self::BadTempo { division, bpm } => {
                write!(f, "song tempo out of range: division={division} bpm={bpm}")
            }
            Self::Device { reason } => write!(f, "audio device: {reason}"),
        }
    }
}

impl std::error::Error for AudioError {}

impl From<snd::SndError> for AudioError {
    fn from(err: snd::SndError) -> Self {
        Self::Snd(err)
    }
}

impl From<bsa::BsaError> for AudioError {
    fn from(err: bsa::BsaError) -> Self {
        Self::Bsa(err)
    }
}

impl From<midi::HmiError> for AudioError {
    fn from(err: midi::HmiError) -> Self {
        Self::Midi(err)
    }
}

/// FNV-1a 64 over raw bytes (offset basis `0xcbf29ce484222325`, prime
/// `0x100000001b3`) — the clip identity of the measured goldens
/// (`daggerfall_parser_rs::snd::golden_clips`).
pub fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// Synthesizes the 44-byte RIFF/WAVE header for 8-bit unsigned mono PCM at
/// [`snd::SAMPLE_RATE`] — the same layout as [`snd::SoundClip::wave_header`].
pub fn wave_bytes(pcm: &[u8]) -> Vec<u8> {
    let data_len = pcm.len() as u32;
    let byte_rate = snd::SAMPLE_RATE * u32::from(snd::CHANNELS) * u32::from(snd::BITS_PER_SAMPLE / 8);
    let block_align = snd::CHANNELS * (snd::BITS_PER_SAMPLE / 8);
    let mut out = Vec::with_capacity(WAVE_HEADER_LEN + pcm.len());
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVE");
    out.extend_from_slice(b"fmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes()); // PCM
    out.extend_from_slice(&snd::CHANNELS.to_le_bytes());
    out.extend_from_slice(&snd::SAMPLE_RATE.to_le_bytes());
    out.extend_from_slice(&byte_rate.to_le_bytes());
    out.extend_from_slice(&block_align.to_le_bytes());
    out.extend_from_slice(&snd::BITS_PER_SAMPLE.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    out.extend_from_slice(pcm);
    out
}

/// Retail effects bank: `DAGGER.SND` kept in memory + its parsed directory.
#[derive(Debug)]
pub struct SfxBank {
    path: PathBuf,
    data: Vec<u8>,
    bank: snd::SoundBank,
}

impl SfxBank {
    /// Parses a `DAGGER.SND` file (BSA `0x0200`, positional directory).
    pub fn open(path: &Path) -> Result<Self, AudioError> {
        let data = std::fs::read(path).map_err(|err| AudioError::Io {
            path: path.to_path_buf(),
            reason: err.to_string(),
        })?;
        let bank = snd::SoundBank::parse(&data)?;
        Ok(Self {
            path: path.to_path_buf(),
            data,
            bank,
        })
    }

    /// Opens the bank from the Mode A ARENA2 (`crate::mode_a::arena2_path`).
    pub fn from_arena2() -> Result<Self, AudioError> {
        let path = crate::mode_a::arena2_path(SOUND_BANK_FILE).map_err(|reason| {
            AudioError::MissingFile {
                name: SOUND_BANK_FILE.to_string(),
                reason,
            }
        })?;
        Self::open(&path)
    }

    /// File this bank was read from (evidence).
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Number of directory records.
    pub fn len(&self) -> usize {
        self.bank.len()
    }

    /// `true` when the directory has no records.
    pub fn is_empty(&self) -> bool {
        self.bank.is_empty()
    }

    /// Decodes a clip by its **directory index** (`SoundClips` discriminant);
    /// `SoundClips::None = -1` has none and returns [`AudioError::NoClip`].
    pub fn clip(&self, clip: snd::SoundClips) -> Result<snd::SoundClip, AudioError> {
        let index = clip.index().ok_or(AudioError::NoClip { clip })?;
        self.bank.decode(&self.data, index).map_err(AudioError::Snd)
    }

    /// WAV bytes of a clip: 44-byte header + PCM.
    pub fn wave(&self, clip: snd::SoundClips) -> Result<Vec<u8>, AudioError> {
        Ok(self.clip(clip)?.wave_header())
    }

    /// FNV-1a64 of a clip's PCM (the digest of the measured goldens).
    pub fn pcm_digest(&self, clip: snd::SoundClips) -> Result<u64, AudioError> {
        Ok(fnv1a64(&self.clip(clip)?.pcm))
    }
}

/// Retail music bank: `MIDI.BSA` kept in memory + its parsed BSA directory.
#[derive(Debug)]
pub struct MusicBank {
    path: PathBuf,
    data: Vec<u8>,
    archive: bsa::BsaArchive,
}

impl MusicBank {
    /// Parses a `MIDI.BSA` file (BSA `0x0100`, named records).
    pub fn open(path: &Path) -> Result<Self, AudioError> {
        let data = std::fs::read(path).map_err(|err| AudioError::Io {
            path: path.to_path_buf(),
            reason: err.to_string(),
        })?;
        let archive = bsa::parse(&data)?;
        Ok(Self {
            path: path.to_path_buf(),
            data,
            archive,
        })
    }

    /// Opens the bank from the Mode A ARENA2 (`crate::mode_a::arena2_path`).
    pub fn from_arena2() -> Result<Self, AudioError> {
        let path = crate::mode_a::arena2_path(MUSIC_BANK_FILE).map_err(|reason| {
            AudioError::MissingFile {
                name: MUSIC_BANK_FILE.to_string(),
                reason,
            }
        })?;
        Self::open(&path)
    }

    /// File this bank was read from (evidence).
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Number of records.
    pub fn len(&self) -> usize {
        self.archive.entries.len()
    }

    /// `true` when the archive has no records.
    pub fn is_empty(&self) -> bool {
        self.archive.entries.is_empty()
    }

    /// Bytes of the HMI record of `song` (`SongFiles::record_name`, DOS 8.3,
    /// case-insensitive).
    pub fn song_bytes(&self, song: midi::SongFiles) -> Result<&[u8], AudioError> {
        let wanted = song.record_name().ok_or(AudioError::MissingSong { song })?;
        for (index, entry) in self.archive.entries.iter().enumerate() {
            if let bsa::Entry::Named { name, .. } = entry {
                if name.eq_ignore_ascii_case(wanted) {
                    return bsa::entry_data(&self.archive, &self.data, index)
                        .map_err(AudioError::Bsa);
                }
            }
        }
        Err(AudioError::MissingSong { song })
    }

    /// Renders `song` to a WAV ([`render_song`] over the retail bytes).
    pub fn render(&self, song: midi::SongFiles) -> Result<RenderedSong, AudioError> {
        render_song(self.song_bytes(song)?)
    }
}

/// A song rendered to playable audio.
#[derive(Debug, Clone)]
pub struct RenderedSong {
    /// 44-byte RIFF/WAVE header + 8-bit unsigned mono PCM at 11025 Hz.
    pub wav: Vec<u8>,
    /// PCM bytes (= samples, 8-bit mono).
    pub samples: usize,
    /// FNV-1a64 of the PCM — the determinism key.
    pub digest: u64,
    /// Length in seconds at [`snd::SAMPLE_RATE`].
    pub duration_secs: f64,
    /// Retained note-ons.
    pub note_events: usize,
    /// Last tick of the song.
    pub end_tick: u64,
    /// Ticks per quarter note.
    pub division: u16,
    /// Declared beats per minute.
    pub bpm: u16,
}

/// Renders an HMI song to a WAV with a deterministic square-wave synth.
///
/// The melody, rhythm and durations come from [`midi::parse_hmi_events`]
/// (HMI carries an explicit duration per note-on). Two renders of the same
/// bytes produce the same PCM and the same [`RenderedSong::digest`].
pub fn render_song(hmi: &[u8]) -> Result<RenderedSong, AudioError> {
    let events = midi::parse_hmi_events(hmi)?;
    if events.ticks_per_quarter == 0 || events.bpm == 0 {
        return Err(AudioError::BadTempo {
            division: events.ticks_per_quarter,
            bpm: events.bpm,
        });
    }
    let seconds_per_tick =
        60.0 / (f64::from(events.bpm) * f64::from(events.ticks_per_quarter));
    let seconds = (events.duration_secs()).min(MAX_RENDER_SECONDS);
    let sample_rate = f64::from(snd::SAMPLE_RATE);
    let total = ((seconds * sample_rate).ceil() as usize).max(1);
    let mut mix = vec![0i32; total];
    for note in &events.notes {
        let start = (note.tick as f64 * seconds_per_tick * sample_rate) as usize;
        if start >= total {
            continue;
        }
        let length = ((note.duration_ticks.max(1) as f64) * seconds_per_tick * sample_rate) as usize;
        let end = start.saturating_add(length.max(1)).min(total);
        let frequency = 440.0 * 2f64.powf((f64::from(note.note) - 69.0) / 12.0);
        let period = (sample_rate / frequency).max(2.0);
        let amplitude = NOTE_PEAK + i32::from(note.velocity) / 8;
        for (offset, slot) in mix[start..end].iter_mut().enumerate() {
            let phase = offset as f64 % period;
            let sample = if phase < period / 2.0 { amplitude } else { -amplitude };
            *slot = slot.saturating_add(sample);
        }
    }
    let mut pcm = Vec::with_capacity(total);
    for value in mix {
        pcm.push((value.clamp(-127, 127) + 128) as u8);
    }
    let digest = fnv1a64(&pcm);
    let samples = pcm.len();
    Ok(RenderedSong {
        wav: wave_bytes(&pcm),
        samples,
        digest,
        duration_secs: samples as f64 / sample_rate,
        note_events: events.total_note_ons(),
        end_tick: events.end_tick,
        division: events.ticks_per_quarter,
        bpm: events.bpm,
    })
}

/// One queued effect: the DFU clip and the per-request gain.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SfxRequest {
    /// Clip to play (`SoundClips` = directory index).
    pub clip: snd::SoundClips,
    /// Linear gain applied by the sink (`1.0` = as recorded).
    pub gain: f32,
}

/// One effect that reached a sink (headless evidence).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlayedClip {
    /// Clip that was resolved.
    pub clip: snd::SoundClips,
    /// PCM samples (= WAV bytes − 44).
    pub samples: usize,
    /// FNV-1a64 of the PCM.
    pub digest: u64,
    /// Gain requested.
    pub gain: f32,
}

/// One song that reached a sink.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlayedSong {
    /// PCM samples.
    pub samples: usize,
    /// FNV-1a64 of the PCM.
    pub digest: u64,
    /// Length in seconds.
    pub duration_secs: f64,
}

/// Destination of resolved audio. Implementations never panic: a device
/// failure is an [`AudioError`] the runtime reports and survives.
pub trait SfxSink: Send + Sync {
    /// Hands a resolved WAV (44-byte header + PCM) to the sink.
    fn submit(&mut self, req: &SfxRequest, wav: Vec<u8>) -> Result<(), AudioError>;

    /// Hands a rendered song to the music sink (sinks without a music
    /// channel ignore it).
    fn submit_music(&mut self, _wav: Vec<u8>) -> Result<(), AudioError> {
        Ok(())
    }
}

/// Headless sink: accepts everything, plays nothing (0 fake audio).
#[derive(Debug, Default)]
pub struct NullSink;

impl SfxSink for NullSink {
    fn submit(&mut self, _req: &SfxRequest, _wav: Vec<u8>) -> Result<(), AudioError> {
        Ok(())
    }
}

/// Headless sink that records what a device would have played.
#[derive(Debug, Default)]
pub struct RecordingSink {
    /// Effects submitted, in order.
    pub played: Vec<PlayedClip>,
    /// Songs submitted, in order.
    pub songs: Vec<PlayedSong>,
}

impl SfxSink for RecordingSink {
    fn submit(&mut self, req: &SfxRequest, wav: Vec<u8>) -> Result<(), AudioError> {
        let pcm = wav.get(WAVE_HEADER_LEN..).unwrap_or(&[]);
        self.played.push(PlayedClip {
            clip: req.clip,
            samples: pcm.len(),
            digest: fnv1a64(pcm),
            gain: req.gain,
        });
        Ok(())
    }

    fn submit_music(&mut self, wav: Vec<u8>) -> Result<(), AudioError> {
        let pcm = wav.get(WAVE_HEADER_LEN..).unwrap_or(&[]);
        self.songs.push(PlayedSong {
            samples: pcm.len(),
            digest: fnv1a64(pcm),
            duration_secs: pcm.len() as f64 / f64::from(snd::SAMPLE_RATE),
        });
        Ok(())
    }
}

/// Real device playback through `rodio` (feature `audio`).
///
/// cpal's stream is `!Send`/`!Sync` across platforms (it is thread-affine), so
/// the device, its stream and the two `rodio::Sink`s live on a **dedicated
/// audio thread**; this type only holds the command channel and its handle,
/// which keeps the sink `Send + Sync` and usable as a Bevy resource.
///
/// Construction fails with [`AudioError::Device`] when there is no output
/// device; the caller then degrades to [`NullSink`] with an explicit warning.
/// Music and effects use independent sinks, so the context song keeps its own
/// gain while effects overlap it.
#[cfg(feature = "audio")]
pub struct RodioSink {
    commands: Option<std::sync::mpsc::Sender<RodioCommand>>,
    worker: Option<std::thread::JoinHandle<()>>,
}

/// Commands of the audio thread (PCM = 8-bit unsigned samples, no header).
#[cfg(feature = "audio")]
enum RodioCommand {
    /// Effect: decoded PCM + linear gain.
    Effect { pcm: Vec<u8>, gain: f32 },
    /// Context song: decoded PCM, looped on the music sink.
    Music { pcm: Vec<u8> },
}

/// 8-bit unsigned mono PCM → `f32` samples in `[-1.0, 1.0]` (128 = silence).
#[cfg(feature = "audio")]
fn pcm_to_samples(pcm: &[u8]) -> Vec<f32> {
    pcm.iter()
        .map(|byte| (f32::from(*byte) - 128.0) / 128.0)
        .collect()
}

#[cfg(feature = "audio")]
impl RodioSink {
    /// Opens the default output device on the audio thread and waits for its
    /// startup verdict (device absence is reported, never hidden).
    pub fn new() -> Result<Self, AudioError> {
        use std::sync::mpsc;
        let (commands, queue) = mpsc::channel::<RodioCommand>();
        let (ready_sender, ready) = mpsc::channel::<Result<(), String>>();
        let worker = std::thread::Builder::new()
            .name("m14-rodio".to_string())
            .spawn(move || {
                use rodio::Source;
                let (stream, handle) = match rodio::OutputStream::try_default() {
                    Ok(pair) => pair,
                    Err(err) => {
                        let _ = ready_sender.send(Err(err.to_string()));
                        return;
                    }
                };
                let effects = match rodio::Sink::try_new(&handle) {
                    Ok(sink) => sink,
                    Err(err) => {
                        let _ = ready_sender.send(Err(err.to_string()));
                        return;
                    }
                };
                let music = match rodio::Sink::try_new(&handle) {
                    Ok(sink) => sink,
                    Err(err) => {
                        let _ = ready_sender.send(Err(err.to_string()));
                        return;
                    }
                };
                let _ = ready_sender.send(Ok(()));
                for command in queue {
                    match command {
                        RodioCommand::Effect { pcm, gain } => {
                            let buffer = rodio::buffer::SamplesBuffer::new(
                                snd::CHANNELS.max(1),
                                snd::SAMPLE_RATE.max(1),
                                pcm_to_samples(&pcm),
                            );
                            effects.append(buffer.amplify(gain.max(0.0)));
                        }
                        RodioCommand::Music { pcm } => {
                            let buffer = rodio::buffer::SamplesBuffer::new(
                                snd::CHANNELS.max(1),
                                snd::SAMPLE_RATE.max(1),
                                pcm_to_samples(&pcm),
                            );
                            music.append(buffer.repeat_infinite().amplify(MUSIC_GAIN));
                        }
                    }
                }
                // Dropping the stream ends playback once the queue closes.
                drop(stream);
            })
            .map_err(|err| AudioError::Device {
                reason: format!("audio thread: {err}"),
            })?;
        match ready.recv() {
            Ok(Ok(())) => Ok(Self {
                commands: Some(commands),
                worker: Some(worker),
            }),
            Ok(Err(reason)) => {
                let _ = worker.join();
                Err(AudioError::Device { reason })
            }
            Err(_) => {
                let _ = worker.join();
                Err(AudioError::Device {
                    reason: "audio thread exited during startup".to_string(),
                })
            }
        }
    }

    /// Queues a command for the audio thread.
    fn send(&self, command: RodioCommand) -> Result<(), AudioError> {
        match self.commands.as_ref() {
            Some(commands) => commands.send(command).map_err(|_| AudioError::Device {
                reason: "audio thread stopped".to_string(),
            }),
            None => Err(AudioError::Device {
                reason: "audio thread stopped".to_string(),
            }),
        }
    }
}

#[cfg(feature = "audio")]
impl SfxSink for RodioSink {
    fn submit(&mut self, req: &SfxRequest, wav: Vec<u8>) -> Result<(), AudioError> {
        let pcm = wav.get(WAVE_HEADER_LEN..).unwrap_or(&[]).to_vec();
        self.send(RodioCommand::Effect {
            pcm,
            gain: req.gain,
        })
    }

    fn submit_music(&mut self, wav: Vec<u8>) -> Result<(), AudioError> {
        let pcm = wav.get(WAVE_HEADER_LEN..).unwrap_or(&[]).to_vec();
        self.send(RodioCommand::Music { pcm })
    }
}

#[cfg(feature = "audio")]
impl Drop for RodioSink {
    fn drop(&mut self) {
        // Closing the channel ends the worker loop (and its stream).
        self.commands.take();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

/// Queue + sink + evidence of the production effects path.
pub struct AudioEngine {
    bank: SfxBank,
    sink: Box<dyn SfxSink>,
    queue: Vec<SfxRequest>,
    played: Vec<PlayedClip>,
}

impl AudioEngine {
    /// Builds an engine over an open bank and a sink.
    pub fn new(bank: SfxBank, sink: Box<dyn SfxSink>) -> Self {
        Self {
            bank,
            sink,
            queue: Vec::new(),
            played: Vec::new(),
        }
    }

    /// The bank the engine resolves against.
    pub fn bank(&self) -> &SfxBank {
        &self.bank
    }

    /// Queues a clip at gain `1.0`.
    pub fn play(&mut self, clip: snd::SoundClips) {
        self.play_with_gain(clip, 1.0);
    }

    /// Queues a clip with a linear gain.
    pub fn play_with_gain(&mut self, clip: snd::SoundClips, gain: f32) {
        self.queue.push(SfxRequest {
            clip,
            gain: gain.clamp(0.0, 4.0),
        });
    }

    /// Number of queued requests not yet drained.
    pub fn pending(&self) -> usize {
        self.queue.len()
    }

    /// Resolves and submits **every** queued request.
    ///
    /// The queue is always fully consumed (no request is retried forever).
    /// The returned count is the number of clips that reached the sink; the
    /// first failure is returned as `Err` **after** the whole queue was
    /// processed, so the caller can warn once without losing the others.
    pub fn drain(&mut self) -> Result<usize, AudioError> {
        let requests = std::mem::take(&mut self.queue);
        let mut submitted = 0usize;
        let mut first_error: Option<AudioError> = None;
        for req in requests {
            match self.bank.clip(req.clip) {
                Ok(clip) => {
                    let played = PlayedClip {
                        clip: req.clip,
                        samples: clip.samples,
                        digest: fnv1a64(&clip.pcm),
                        gain: req.gain,
                    };
                    match self.sink.submit(&req, clip.wave_header()) {
                        Ok(()) => {
                            self.played.push(played);
                            submitted += 1;
                        }
                        Err(err) => {
                            if first_error.is_none() {
                                first_error = Some(err);
                            }
                        }
                    }
                }
                Err(err) => {
                    if first_error.is_none() {
                        first_error = Some(err);
                    }
                }
            }
        }
        match first_error {
            Some(err) => Err(err),
            None => Ok(submitted),
        }
    }

    /// Every clip that reached the sink, in order.
    pub fn played(&self) -> &[PlayedClip] {
        &self.played
    }

    /// Hands a rendered song to the music channel of the sink.
    pub fn play_music(&mut self, song: RenderedSong) -> Result<(), AudioError> {
        self.sink.submit_music(song.wav)
    }
}

/// Gameplay event with a DFU sound mapping (REQ-L99-046).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GameplayEvent {
    /// Landing: `damage > 0` → `FallDamage`; a hard fall without damage →
    /// `FallHard` (`DFU:EnemyMotor.cs:1409`, `AcrobatMotor.cs:221-223`).
    Fall { damage: i32, hard: bool },
    /// Weapon swing: `high` selects the pitch. Bare-handed melee always sets
    /// `SwingWeaponSound = SwingHighPitch` (`DFU:WeaponManager.cs:756-761`,
    /// replayed on an unarmed miss at `EnemySounds.cs:153`).
    MeleeSwing { high: bool },
    /// Impact: `Hit1..=Hit5` by die (`1..=5`). Armed hits draw over the whole
    /// range (`Random.Range(0,5)`); bare-handed hits play `Hit3`/`Hit4` only
    /// (`Hit1 + Random.Range(2,4)`, `DFU:EnemySounds.cs:123,127`).
    MeleeHit { die: u8 },
    /// Bow release.
    ArrowShot,
    /// Arrow impact.
    ArrowHit,
    /// Body hitting the floor.
    BodyFall,
    /// Normal door opening.
    DoorOpen,
    /// Normal door closing.
    DoorClose,
}

/// DFU clip of a gameplay event.
pub fn gameplay_clip(event: GameplayEvent) -> Option<snd::SoundClips> {
    use snd::SoundClips;
    match event {
        GameplayEvent::Fall { damage, hard } => {
            if damage > 0 {
                Some(SoundClips::FallDamage)
            } else if hard {
                Some(SoundClips::FallHard)
            } else {
                None
            }
        }
        GameplayEvent::MeleeSwing { high } => Some(if high {
            SoundClips::SwingHighPitch
        } else {
            SoundClips::SwingLowPitch
        }),
        GameplayEvent::MeleeHit { die } => match die {
            1 => Some(SoundClips::Hit1),
            2 => Some(SoundClips::Hit2),
            3 => Some(SoundClips::Hit3),
            4 => Some(SoundClips::Hit4),
            5 => Some(SoundClips::Hit5),
            _ => None,
        },
        GameplayEvent::ArrowShot => Some(SoundClips::ArrowShoot),
        GameplayEvent::ArrowHit => Some(SoundClips::ArrowHit),
        GameplayEvent::BodyFall => Some(SoundClips::BodyFall),
        GameplayEvent::DoorOpen => Some(SoundClips::NormalDoorOpen),
        GameplayEvent::DoorClose => Some(SoundClips::NormalDoorClose),
    }
}

/// DFU playlist for city/wilderness + day + `Sunny` weather
/// (`DFU:SongManager.cs::_sunnySongs`).
pub const SUNNY_SONGS: &[midi::SongFiles] = &[
    midi::SongFiles::song_gday___d,
    midi::SongFiles::song_swimming,
    midi::SongFiles::song_gsunny2,
    midi::SongFiles::song_sunnyday,
    midi::SongFiles::song_02,
    midi::SongFiles::song_03,
    midi::SongFiles::song_22,
];

/// DFU playlist for city/wilderness + night (`_nightSongs`).
pub const NIGHT_SONGS: &[midi::SongFiles] = &[
    midi::SongFiles::song_10,
    midi::SongFiles::song_11,
    midi::SongFiles::song_gcurse,
    midi::SongFiles::song_geerie,
    midi::SongFiles::song_gruins,
    midi::SongFiles::song_18,
    midi::SongFiles::song_21,
];

/// DFU playlist of taverns (`_tavernSongs`,
/// `DFU-UP:Assets/Scripts/Game/SongManager.cs:801-808@0038a1e6`): all taverns
/// share the song of the day and advance one track per day.
pub const TAVERN_SONGS: &[midi::SongFiles] = &[
    midi::SongFiles::song_square_2,
    midi::SongFiles::song_tavern,
    midi::SongFiles::song_folk1,
    midi::SongFiles::song_folk2,
    midi::SongFiles::song_folk3,
];

/// Tavern song of the day — DFU `SongManager.SelectCurrentSong`
/// (`DFU-UP:Assets/Scripts/Game/SongManager.cs:334-345@0038a1e6`):
/// `index = gameDays % currentPlaylist.Length`, **no RNG** (unlike every other
/// context). `gameDays % len` is the literal M34 §5 block B asserts.
///
/// Single copy: the runtime, the probe and the tests consume this function.
pub fn tavern_song(game_days: u32) -> midi::SongFiles {
    let index = (game_days as usize) % TAVERN_SONGS.len();
    TAVERN_SONGS[index]
}

/// Context song — DFU `SongManager.SelectCurrentSong`: the playlist is chosen
/// by context (night vs day) and the track by `DFRandom.srand(game_days)`
/// followed by `rand() % len`.
///
/// Single copy: the runtime, the probe and the tests consume this function.
pub fn music_context_song(game_days: u32, night: bool) -> midi::SongFiles {
    let playlist = if night { NIGHT_SONGS } else { SUNNY_SONGS };
    let mut rng = DfRandom::new();
    rng.srand(game_days);
    let index = (rng.rand() as usize) % playlist.len().max(1);
    playlist.get(index).copied().unwrap_or(midi::SongFiles::song_none)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fnv1a64_matches_the_reference_vector() {
        // Canonical FNV-1a 64 vectors (offset basis/prime of the goldens).
        assert_eq!(fnv1a64(b""), 0xcbf2_9ce4_8422_2325);
        assert_eq!(fnv1a64(b"a"), 0xaf63_dc4c_8601_ec8c);
        assert_eq!(fnv1a64(b"hello"), 0xa430_d846_80aa_bd0b);
    }

    #[test]
    fn wave_header_matches_the_parser_layout() {
        let pcm = vec![128u8, 129, 130, 127];
        let clip = snd::SoundClip {
            index: 0,
            record_id: 0,
            samples: pcm.len(),
            pcm: pcm.clone(),
        };
        let wav = wave_bytes(&pcm);
        assert_eq!(wav.len(), WAVE_HEADER_LEN + pcm.len());
        assert_eq!(wav, clip.wave_header());
        assert_eq!(&wav[..4], b"RIFF");
        assert_eq!(&wav[8..12], b"WAVE");
        assert_eq!(&wav[WAVE_HEADER_LEN..], pcm.as_slice());
    }

    #[test]
    fn gameplay_events_map_to_the_dfu_clips() {
        use snd::SoundClips as Clips;
        let cases = [
            (GameplayEvent::Fall { damage: 3, hard: false }, Some(Clips::FallDamage)),
            (GameplayEvent::Fall { damage: 0, hard: true }, Some(Clips::FallHard)),
            (GameplayEvent::Fall { damage: 0, hard: false }, None),
            (GameplayEvent::MeleeSwing { high: true }, Some(Clips::SwingHighPitch)),
            (GameplayEvent::MeleeSwing { high: false }, Some(Clips::SwingLowPitch)),
            (GameplayEvent::MeleeHit { die: 1 }, Some(Clips::Hit1)),
            (GameplayEvent::MeleeHit { die: 2 }, Some(Clips::Hit2)),
            (GameplayEvent::MeleeHit { die: 3 }, Some(Clips::Hit3)),
            (GameplayEvent::MeleeHit { die: 4 }, Some(Clips::Hit4)),
            (GameplayEvent::MeleeHit { die: 5 }, Some(Clips::Hit5)),
            (GameplayEvent::MeleeHit { die: 0 }, None),
            (GameplayEvent::MeleeHit { die: 6 }, None),
            (GameplayEvent::ArrowShot, Some(Clips::ArrowShoot)),
            (GameplayEvent::ArrowHit, Some(Clips::ArrowHit)),
            (GameplayEvent::BodyFall, Some(Clips::BodyFall)),
            (GameplayEvent::DoorOpen, Some(Clips::NormalDoorOpen)),
            (GameplayEvent::DoorClose, Some(Clips::NormalDoorClose)),
        ];
        for (event, expected) in cases {
            assert_eq!(gameplay_clip(event), expected, "{event:?}");
        }
        // The discriminants required by the pack (directory indices).
        assert_eq!(Clips::FallDamage as i32, 91);
        assert_eq!(Clips::FallHard as i32, 92);
        assert_eq!(Clips::NormalDoorClose as i32, 93);
        assert_eq!(Clips::NormalDoorOpen as i32, 94);
        assert_eq!(Clips::SwingLowPitch as i32, 105);
        assert_eq!(Clips::SwingHighPitch as i32, 106);
        assert_eq!(Clips::Hit1 as i32, 108);
        assert_eq!(Clips::Hit5 as i32, 112);
        assert_eq!(Clips::ArrowShoot as i32, 3);
        assert_eq!(Clips::ArrowHit as i32, 4);
        assert_eq!(Clips::BodyFall as i32, 15);
    }

    #[test]
    fn context_song_follows_the_dfu_playlists() {
        // DFRandom: srand(0) ⇒ next = 0 ⇒ first rand() = 12345 ⇒ 0 ⇒ index 0.
        assert_eq!(music_context_song(0, false), midi::SongFiles::song_gday___d);
        assert_eq!(music_context_song(0, true), midi::SongFiles::song_10);
        // Deterministic for the same game day.
        for days in [0u32, 1, 30, 365, 12_345] {
            assert_eq!(music_context_song(days, false), music_context_song(days, false));
            assert_eq!(music_context_song(days, true), music_context_song(days, true));
            assert!(SUNNY_SONGS.contains(&music_context_song(days, false)));
            assert!(NIGHT_SONGS.contains(&music_context_song(days, true)));
        }
        // The day RNG actually walks the playlist (no constant selection).
        let picked: std::collections::HashSet<midi::SongFiles> =
            (0..64).map(|days| music_context_song(days, false)).collect();
        assert!(picked.len() > 1, "DFRandom selection is degenerate: {picked:?}");
    }

    #[test]
    fn tavern_song_is_game_days_modulo_the_playlist() {
        // DFU `SongManager.cs:342-345`: `gameDays % currentPlaylist.Length`,
        // no RNG involved.
        for days in [0u32, 1, 4, 5, 6, 29, 100, 364, 12_345] {
            let index = (days as usize) % TAVERN_SONGS.len();
            assert_eq!(tavern_song(days), TAVERN_SONGS[index], "day {days}");
        }
        assert_eq!(TAVERN_SONGS.len(), 5);
        assert_eq!(tavern_song(0), midi::SongFiles::song_square_2);
        assert_eq!(tavern_song(1), midi::SongFiles::song_tavern);
        assert_eq!(tavern_song(5), tavern_song(0));
        // The law is not constant: every playlist slot is reachable.
        let picked: std::collections::HashSet<midi::SongFiles> =
            (0..TAVERN_SONGS.len() as u32).map(tavern_song).collect();
        assert_eq!(picked.len(), TAVERN_SONGS.len());
    }

    #[test]
    fn render_rejects_a_truncated_song() {
        assert!(matches!(render_song(&[0u8; 8]), Err(AudioError::Midi(_))));
        let err = render_song(&[0u8; 8]).unwrap_err();
        assert!(err.to_string().contains("HMI decode"));
    }

    #[test]
    fn null_sink_accepts_without_playing() {
        let mut sink = NullSink;
        let req = SfxRequest {
            clip: snd::SoundClips::ArrowShoot,
            gain: 1.0,
        };
        assert!(sink.submit(&req, vec![0u8; WAVE_HEADER_LEN + 3]).is_ok());
        assert!(sink.submit_music(vec![0u8; WAVE_HEADER_LEN]).is_ok());
    }

    #[test]
    fn recording_sink_measures_the_wav_payload() {
        let mut sink = RecordingSink::default();
        let req = SfxRequest {
            clip: snd::SoundClips::FallDamage,
            gain: 0.5,
        };
        let pcm = vec![1u8, 2, 3];
        sink.submit(&req, wave_bytes(&pcm)).expect("submit");
        assert_eq!(sink.played.len(), 1);
        assert_eq!(sink.played[0].clip, snd::SoundClips::FallDamage);
        assert_eq!(sink.played[0].samples, pcm.len());
        assert_eq!(sink.played[0].digest, fnv1a64(&pcm));
        assert!((sink.played[0].gain - 0.5).abs() < f32::EPSILON);
        sink.submit_music(wave_bytes(&[7u8; 16])).expect("music");
        assert_eq!(sink.songs.len(), 1);
        assert_eq!(sink.songs[0].samples, 16);
        assert_eq!(sink.songs[0].digest, fnv1a64(&[7u8; 16]));
    }

    #[test]
    fn no_clip_is_reported_not_panicked() {
        // `SoundClips::None` has no directory index: typed error, 0 panic.
        // (The bank itself needs the retail file, so the index check is
        // exercised through the public discriminant contract.)
        assert_eq!(snd::SoundClips::None.index(), None);
        assert_eq!(snd::SoundClips::FallDamage.index(), Some(91));
    }
}
