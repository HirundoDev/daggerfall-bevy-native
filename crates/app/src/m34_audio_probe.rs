//! `AUDIO-RT-001` — **PROBE-AUDIO-RT-001** (M34 / W34-3), spec
//! `docs/sintergic/game/programming/m34-e2e-integration-presentation-spec-CR-5243af63.md`
//! §5 `:239-257`, CR-2d5c8f41.
//!
//! Block by block, what is **measured** (nothing is estimated):
//!
//! - **A — decode:** the **132** entries of `midi::SONG_CATALOG` are resolved
//!   from the retail `MIDI.BSA` (Mode A) by the production `MusicBank` and
//!   parsed with `midi::parse_hmi` under `catch_unwind` — the panic counter is
//!   real, not a promise; and `DAGGER.SND` exposes **459** records with
//!   `dirType = 0x0200`: the **header** is asserted (u16 at `0` = 459, u16 at
//!   `2` = `0x0200`), plus `SoundBank::parse` (0x0200) and the decoding of the
//!   459 directory entries.
//!
//!   **Two measurements of this retail copy contradict the §5 literal, and they
//!   are declared instead of dressed up** (M34 contract §5: the measurement is
//!   printed and the deviation reported):
//!   1. `MIDI.BSA` has **131** named records: `5STRONG.HMI`
//!      (`SongFiles::song_5strong`) **does not exist** in the retail copy, so the
//!      real ceiling is `131/132` (already declared by the M14 probe `AUDIO-001`);
//!      the assert is `parsed + missing == 132` and the `[DESVÍO]` line names
//!      the missing one.
//!   2. `DAGGER.SND` carries **one empty record** (index `5`, 0 PCM bytes): gate A
//!      counts *directory entries* (459), not payload, so the empty one is
//!      reported (`vacíos [5]`) and does not become a decode failure.
//! - **B — trigger:** **60** resolutions (30 days × 2 contexts: city/day and
//!   city/night) through the single production selector
//!   (`audio::music_context_song`) plus the DFU tavern law
//!   (`SongManager.SelectCurrentSong`, `:334-345@0038a1e6`):
//!   `index = gameDays % len` (5 themes), no RNG.
//! - **C — device:** opens the **default** output device and plays 3 real clips
//!   (1 s of rendered music, 0.5 s of a retail `SoundClips`, the full positional
//!   water clip) measuring `samples_played`, the counter bumped by the source the
//!   audio callback pulls from. With no device, or built without the `audio`
//!   feature, the `[SKIP-DEVICE]` line is **mandatory** with its reason and the
//!   verdict turns `PASS(degraded)`.
//! - **D — positional:** water source at `y = -blockWaterLevel · 0.025`
//!   (`MeshReader.GlobalScale`) with `± 3 m` jitter
//!   (`AmbientEffectsPlayer.cs:165-177@0038a1e6`); `pan != 0` whenever
//!   `|x − listener.x| > 0`. The real `blockWaterLevel` comes from W34-D
//!   (dungeon), so here the input is **synthetic and documented**: `-128`, the
//!   GV-13 literal (`waterLevel = −128`), with the player at the origin.
//! - **ambience:** the `[4, 35] s` clock (GV-06) runs 600 s at `dt = 1/60` and
//!   prints how many times it fires (mean 19.5 s ⇒ ≈ 30).
//!
//! **Output semantics (literal §5):** `EXIT=0` ⟺ A ∧ B ∧ (C PASS ∨ C
//! SKIP-DEVICE declared) ∧ digest ×2 identical; `EXIT=2` = retail source
//! missing; `EXIT=1` = any other measurement outside the criterion (never a
//! synthetic PASS, never a SKIP without a reason). The FNV-1a 64 digest covers
//! **only** the measurement lines (paths are printed but not hashed): two
//! separate processes must print the same value.
//!
//! **Declared deviations** (nobody tunes a number to make it fit):
//! - the §5 example block `:253-258` prints no D and no ambience line; the §5
//!   table does require them, so they are printed here;
//! - `samples_played` counts **interleaved samples** the device asked for
//!   (frames × 2 channels) after rodio's channel/rate conversion, not the source
//!   PCM frames; the example literal (`48000/22050/8000`) is a **shape**, not an
//!   obligation;
//! - the audible duration is set by the device (rodio converts to the stream
//!   rate); what this probe certifies is that the device **pulls** the audio,
//!   not the playback tempo.

use std::panic::AssertUnwindSafe;
#[cfg(feature = "audio")]
use std::time::{Duration, Instant};

use daggerfall_app::audio::{self, fnv1a64, AudioError, MusicBank, SfxBank, WAVE_HEADER_LEN};
use daggerfall_nervous::audio::{
    spatial_mix, water_source_position, water_source_y, AmbientClock, DeviceClip, SpatialMix,
    AMBIENT_MEAN_WAIT_SECS, WATER_JITTER_METERS,
};
use daggerfall_parser_rs::{midi, snd};

/// Songs the `MIDI.BSA` catalog must resolve (`assert_eq!(SONG_CATALOG.len(), 132)`).
const EXPECTED_SONGS: usize = 132;
/// Records of `DAGGER.SND` (GV-05, `DISK`).
const EXPECTED_SND_RECORDS: u16 = 459;
/// BSA directory type of a `DAGGER.SND` (`NumberRecord`, GV-05).
const SND_DIRECTORY_TYPE: u16 = 0x0200;
/// Days walked by the trigger block (M34 §5 block B).
const SMOKE_DAYS: u32 = 30;
/// Contexts per day (city/day, city/night).
const CONTEXTS: usize = 2;
/// Music excerpt handed to the device, in seconds (§5 block C: 1 s).
const DEVICE_MUSIC_SECS: f32 = 1.0;
/// `SoundClips` excerpt handed to the device, in seconds (§5 block C: 0.5 s).
const DEVICE_SFX_SECS: f32 = 0.5;
/// **Documented synthetic input** for `blockWaterLevel` (GV-13: `−128`); the
/// real block is supplied by W34-D, which is not in this lane.
const WATER_BLOCK_LEVEL: i32 = -128;
/// Listener position (origin: deterministic, with the player's `x` at 0).
const LISTENER: [f32; 3] = [0.0, 0.0, 0.0];
/// Jitter seeds measured in block D.
const POSITIONAL_SEEDS: u64 = 30;
/// Ambience measurement window (GV-06: `600 s`).
const AMBIENT_WINDOW_SECS: f32 = 600.0;
/// Fixed simulation step (spec §0: `dt = 1/60`).
const DT: f32 = 1.0 / 60.0;

/// What block C was able to measure.
///
/// Without the `audio` feature the binary has no backend, so `measure_device`
/// can only build `Skip`: the other two variants **do not exist** in that
/// configuration (`#[cfg(feature = "audio")]` per variant, not an `allow`), and
/// that is why their `match` arms do not exist either.
enum DeviceOutcome {
/// The device asked for audio: backend name + samples per clip.
    #[cfg(feature = "audio")]
    Pass {
        name: String,
        counts: [u64; 3],
    },
/// There is no device (or the backend was not compiled): explicit reason.
    Skip { reason: String },
/// The device opened but never pulled audio: that is a failure, not a skip.
    #[cfg(feature = "audio")]
    Fail { reason: String },
}

/// Transcript of **measurement** lines (the only ones entering the digest).
struct Transcript {
    lines: Vec<String>,
}

impl Transcript {
    fn new() -> Self {
        Self { lines: Vec::new() }
    }

/// Prints the line and folds it into the digest.
    fn record(&mut self, text: String) {
        println!("{text}");
        self.lines.push(text);
    }

/// FNV-1a 64 over the measurement lines joined by `\n`.
    fn digest(&self) -> u64 {
        fnv1a64(self.lines.join("\n").as_bytes())
    }
}

/// Runs `PROBE-AUDIO-RT-001`.
pub fn run_audio_rt_001() -> i32 {
    println!(
        "PROBE-AUDIO-RT-001 - audio a dispositivo (M34/W34-3, REQ-L99-067, CR-2d5c8f41)"
    );
    let mut transcript = Transcript::new();
    let mut all_ok = true;

// ---------- sources (Mode A: EXIT=2 when missing) ----------
    let music = match MusicBank::from_arena2() {
        Ok(bank) => bank,
        Err(err) => return missing_source("MIDI.BSA", &err),
    };
    let sfx = match SfxBank::from_arena2() {
        Ok(bank) => bank,
        Err(err) => return missing_source(audio::SOUND_BANK_FILE, &err),
    };
    println!("     mode_a MIDI.BSA: {}", music.path().display());
    println!("     mode_a DAGGER.SND: {}", sfx.path().display());

// ---------- A — decode ----------
    let mut songs_ok = 0usize;
    let mut song_panics = 0usize;
// Catalog songs the retail copy does **not** carry (declared by name).
    let mut absent_from_retail: Vec<&'static str> = Vec::new();
    let catalog_total = midi::SONG_CATALOG.len();
    for song in midi::SONG_CATALOG {
        let record = match song.record_name() {
            Some(record) => record,
            None => {
                println!("[FAIL] A: {} has no retail record name", song.name());
                all_ok = false;
                continue;
            }
        };
        let bytes = match music.song_bytes(*song) {
            Ok(bytes) => bytes,
// Retail data, not a parser failure: the name does not exist in the BSA
// measured. It is declared by name and counted as missing (the sum
// `parsed + missing == catalog` remains the assert).
            Err(AudioError::MissingSong { .. }) => {
                absent_from_retail.push(record);
                println!("[DESVÍO] A: {record} ausente del retail MIDI.BSA (medido)");
                continue;
            }
            Err(err) => {
                println!("[FAIL] A: {record}: {err}");
                all_ok = false;
                continue;
            }
        };
        match std::panic::catch_unwind(AssertUnwindSafe(|| midi::parse_hmi(bytes))) {
            Ok(Ok(hmi)) if hmi.total_events() > 0 => songs_ok += 1,
            Ok(Ok(_)) => {
                println!("[FAIL] A: {record}: parsed with 0 events");
                all_ok = false;
            }
            Ok(Err(err)) => {
                println!("[FAIL] A: {record}: HMI parse: {err}");
                all_ok = false;
            }
            Err(_) => {
                song_panics += 1;
                println!("[FAIL] A: {record}: parser panicked");
                all_ok = false;
            }
        }
    }

// `DAGGER.SND` header: 459 records, `dirType = 0x0200` (GV-05).
    let snd_bytes = match std::fs::read(sfx.path()) {
        Ok(bytes) => bytes,
        Err(err) => {
            println!("[FAIL] A: reading {}: {err}", sfx.path().display());
            return 1;
        }
    };
    let header_records = read_u16(&snd_bytes, 0);
    let header_directory = read_u16(&snd_bytes, 2);
    let header_ok = header_records == Some(EXPECTED_SND_RECORDS)
        && header_directory == Some(SND_DIRECTORY_TYPE);
    if !header_ok {
        println!(
            "[FAIL] A: DAGGER.SND header records={:?} dirType={:?} (expected {} / 0x{:04x})",
            header_records, header_directory, EXPECTED_SND_RECORDS, SND_DIRECTORY_TYPE
        );
        all_ok = false;
    }
    let bank = match snd::SoundBank::parse(&snd_bytes) {
        Ok(bank) => bank,
        Err(err) => {
            println!("[FAIL] A: DAGGER.SND parse: {err}");
            return 1;
        }
    };
    let mut decoded = 0usize;
    let mut empty_records: Vec<usize> = Vec::new();
    let mut decode_failures = 0usize;
    for index in 0..bank.len() {
        match std::panic::catch_unwind(AssertUnwindSafe(|| bank.decode(&snd_bytes, index))) {
            Ok(Ok(_)) => decoded += 1,
// Empty record = retail data (measured), not a decode failure:
// gate A counts **directory entries** (459), not payload.
            Ok(Err(snd::SndError::EmptyClip { index })) => empty_records.push(index),
            Ok(Err(err)) => {
                decode_failures += 1;
                println!("[FAIL] A: DAGGER.SND record {index}: {err}");
            }
            Err(_) => {
                decode_failures += 1;
                println!("[FAIL] A: DAGGER.SND record {index}: decode panicked");
            }
        }
    }
    let snd_total = bank.len();
    let absent_total = absent_from_retail.len();
    let a_ok = songs_ok + absent_total == EXPECTED_SONGS
        && catalog_total == EXPECTED_SONGS
        && song_panics == 0
        && header_ok
        && snd_total == usize::from(EXPECTED_SND_RECORDS)
        && decoded + empty_records.len() == snd_total
        && decode_failures == 0;
    if !a_ok {
        all_ok = false;
    }
    transcript.record(format!(
        "[{}] A decode: {songs_ok}/{catalog_total} canciones · {snd_total} SND records · {song_panics} panic",
        if a_ok { "OK" } else { "FAIL" }
    ));
    transcript.record(format!(
        "     A: dirType=0x{:04x} (cabecera {} registros) · {decoded}/{snd_total} con PCM · {} vacíos {:?} · fallos={decode_failures}",
        header_directory.unwrap_or(0),
        header_records.unwrap_or(0),
        empty_records.len(),
        empty_records
    ));
    if absent_total > 0 {
        transcript.record(format!(
            "[DESVÍO] A: {songs_ok}+{absent_total}={catalog_total} (catálogo completo) · ausente(s) del retail: {} — dato medido, el literal §5 «132/132» no es alcanzable con esta copia",
            absent_from_retail.join(", ")
        ));
    }

// ---------- B — trigger ----------
    let contexts: [(&str, bool); CONTEXTS] = [("city/day", false), ("city/night", true)];
    let mut triggers_ok = 0usize;
    let mut triggers_total = 0usize;
    for (label, night) in contexts {
        for day in 0..SMOKE_DAYS {
            triggers_total += 1;
            let song = audio::music_context_song(day, night);
            match music
                .song_bytes(song)
                .and_then(|bytes| midi::parse_hmi(bytes).map_err(AudioError::Midi))
            {
                Ok(_) => triggers_ok += 1,
                Err(err) => {
                    println!("[FAIL] B: {label} day {day} ({}): {err}", song.name());
                    all_ok = false;
                }
            }
        }
    }

// DFU tavern law: `index = gameDays % len`, no RNG.
    let mut tavern_ok = 0usize;
    let tavern_len = audio::TAVERN_SONGS.len();
    for day in 0..SMOKE_DAYS {
        let song = audio::tavern_song(day);
        let expected_index = (day as usize) % tavern_len;
        let law_ok = song == audio::TAVERN_SONGS[expected_index];
        let retail_ok = music
            .song_bytes(song)
            .and_then(|bytes| midi::parse_hmi(bytes).map_err(AudioError::Midi))
            .is_ok();
        if law_ok && retail_ok {
            tavern_ok += 1;
        } else {
            println!(
                "[FAIL] B: tavern day {day}: law={law_ok} ({} != {}) retail={retail_ok}",
                song.name(),
                audio::TAVERN_SONGS[expected_index].name()
            );
            all_ok = false;
        }
    }
    let b_ok = triggers_ok == triggers_total
        && triggers_total == SMOKE_DAYS as usize * CONTEXTS
        && tavern_ok == SMOKE_DAYS as usize;
    if !b_ok {
        all_ok = false;
    }
    transcript.record(format!(
        "[{}] B triggers: {triggers_ok}/{triggers_total} ({} días × {CONTEXTS} contextos)",
        if b_ok { "OK" } else { "FAIL" },
        SMOKE_DAYS
    ));
    transcript.record(format!(
        "     B: taberna gameDays%{tavern_len} {tavern_ok}/{SMOKE_DAYS} · ley DFU SongManager.cs:342-345"
    ));

// ---------- D — positional (needs no device) ----------
    let mut positional_ok = 0usize;
    let mut dx_nonzero = 0usize;
    let mut dx_zero = 0usize;
    let mut max_jitter = 0.0f32;
    let expected_y = water_source_y(WATER_BLOCK_LEVEL);
    for seed in 0..POSITIONAL_SEEDS {
        let source = water_source_position(WATER_BLOCK_LEVEL, LISTENER, seed);
        let mix = spatial_mix(source, LISTENER);
        let dx = source[0] - LISTENER[0];
        max_jitter = max_jitter.max(dx.abs());
        let y_ok = (source[1] - expected_y).abs() < 1e-6;
        let pan_ok = if dx == 0.0 {
            dx_zero += 1;
            mix.pan == 0.0
        } else {
            dx_nonzero += 1;
            mix.pan != 0.0
        };
        if y_ok && pan_ok {
            positional_ok += 1;
        } else {
            println!(
                "[FAIL] D: seed {seed}: source={source:?} y_ok={y_ok} pan={} dx={dx}",
                mix.pan
            );
            all_ok = false;
        }
    }
    let d_ok = positional_ok == POSITIONAL_SEEDS as usize
        && max_jitter <= WATER_JITTER_METERS + 1e-5
        && dx_nonzero + dx_zero == POSITIONAL_SEEDS as usize;
    if !d_ok {
        all_ok = false;
    }
    transcript.record(format!(
        "[{}] D positional: agua y={expected_y:.3} (blockWaterLevel={WATER_BLOCK_LEVEL} sintético, GV-13) · jitter_max={max_jitter:.3} m · pan≠0 en {dx_nonzero}/{POSITIONAL_SEEDS} (dx≠0: {dx_nonzero}, dx=0: {dx_zero})",
        if d_ok { "OK" } else { "FAIL" }
    ));

// ---------- ambience (GV-06) ----------
    let mut clock = AmbientClock::general(0x5EED_0026);
    let mut fires = 0usize;
    let steps = (AMBIENT_WINDOW_SECS / DT) as usize;
    for _ in 0..steps {
        if clock.tick(DT) {
            fires += 1;
        }
    }
    let expected_fires = AMBIENT_WINDOW_SECS / AMBIENT_MEAN_WAIT_SECS;
    let ambient_ok = fires > 0;
    if !ambient_ok {
        all_ok = false;
    }
    transcript.record(format!(
        "[{}] ambiente: {AMBIENT_WINDOW_SECS:.0} s ⇒ {fires} disparos (media {AMBIENT_MEAN_WAIT_SECS:.1} s ⇒ ≈{expected_fires:.0}, rango [4,35] s, GV-06)",
        if ambient_ok { "OK" } else { "FAIL" }
    ));

// ---------- C — device ----------
    let clips = match prepare_clips(&music, &sfx) {
        Ok(clips) => clips,
        Err(reason) => {
            println!("[FAIL] C: no se pudieron preparar los 3 clips: {reason}");
            return 1;
        }
    };
    transcript.record(format!(
        "     C clips: music={DEVICE_MUSIC_SECS:.1}s/{}f · sfx={DEVICE_SFX_SECS:.1}s/{}f · ambient={}f pan={:.3} gain={:.3}",
        clips.music_frames, clips.sfx_frames, clips.ambient_frames, clips.mix.pan, clips.mix.gain
    ));
// The degraded verdict **is derived** from the outcome (not pre-initialised):
// without the `audio` feature the only possible outcome is `Skip`, so a
// `let mut degraded = false` would be a dead assignment in that build.
    let degraded = match measure_device(&clips) {
        #[cfg(feature = "audio")]
        DeviceOutcome::Pass { name, counts } => {
            transcript.record(format!(
                "[OK] C device: {name} · samples_played={}/{}/{}",
                counts[0], counts[1], counts[2]
            ));
            false
        }
        DeviceOutcome::Skip { reason } => {
            transcript.record(format!(
                "[SKIP-DEVICE] C: sin dispositivo de salida (PASS degradado declarado) — {reason}"
            ));
            true
        }
        #[cfg(feature = "audio")]
        DeviceOutcome::Fail { reason } => {
            all_ok = false;
            transcript.record(format!("[FAIL] C device: {reason}"));
            false
        }
    };

// ---------- verdict + digest ----------
    let digest = transcript.digest();
    let status = if !all_ok {
        "FAIL".to_string()
    } else if degraded {
        "PASS(degraded)".to_string()
    } else {
        "PASS".to_string()
    };
    println!("VEREDICTO: PROBE-AUDIO-RT-001 {status} · digest={digest:016x} ×2");
    if all_ok {
        0
    } else {
        1
    }
}

/// `4`-byte little-endian header field, or `None` when the buffer is short.
fn read_u16(bytes: &[u8], offset: usize) -> Option<u16> {
    let slice = bytes.get(offset..offset + 2)?;
    Some(u16::from_le_bytes([slice[0], slice[1]]))
}

/// Retail source missing ⇒ `EXIT=2` (spec §5: "sin `MIDI.BSA`/`DAGGER.SND`").
fn missing_source(name: &str, err: &AudioError) -> i32 {
    match err {
        AudioError::MissingFile { .. } => {
            println!("[FAIL] A: fuente ausente {name} (EXIT=2): {err}");
            2
        }
        other => {
            println!("[FAIL] A: {name} ilegible (EXIT=2): {other}");
            2
        }
    }
}

/// The three clips of measure C, already trimmed (deterministic) and placed.
///
/// The clips only exist for the device: without the `audio` feature there is no
/// backend to consume them, so the struct keeps only the measured frame census
/// (what the `C clips` line reports).
struct PreparedClips {
    #[cfg(feature = "audio")]
    music: DeviceClip,
    #[cfg(feature = "audio")]
    sfx: DeviceClip,
    #[cfg(feature = "audio")]
    ambient: DeviceClip,
/// Frames handed to the device per clip (measured by the `C clips` line).
    music_frames: usize,
    sfx_frames: usize,
    ambient_frames: usize,
/// Positional mix of the ambience clip.
    mix: SpatialMix,
}

/// Prepares the 3 clips **before** opening the device: a decode failure is an
/// A/C `[FAIL]`, never a `[SKIP-DEVICE]` that would hide a broken decode.
fn prepare_clips(music: &MusicBank, sound_bank: &SfxBank) -> Result<PreparedClips, String> {
// 1 — music: the city/day context song, trimmed to 1 s.
    let song = audio::music_context_song(0, false);
    let rendered = music
        .render(song)
        .map_err(|err| format!("music {}: {err}", song.name()))?;
    let music_pcm = rendered
        .wav
        .get(WAVE_HEADER_LEN..)
        .ok_or_else(|| "rendered song without PCM".to_string())?
        .to_vec();
    let music_clip = DeviceClip::new("music", music_pcm, snd::SAMPLE_RATE);
    let music_take = music_clip.take_secs(DEVICE_MUSIC_SECS);
    if music_take.is_empty() {
        return Err(format!("music {}: render produced no samples", song.name()));
    }
    let music = DeviceClip {
        pcm: music_take.clone(),
        ..music_clip
    };

// 2 — effects: a retail `SoundClips`, trimmed to 0.5 s.
    let door_clip = sound_bank
        .clip(snd::SoundClips::NormalDoorOpen)
        .map_err(|err| format!("SoundClips::NormalDoorOpen: {err}"))?;
    let door = DeviceClip::new("sfx", door_clip.pcm, snd::SAMPLE_RATE);
    let sfx_take = door.take_secs(DEVICE_SFX_SECS);
    if sfx_take.is_empty() {
        return Err("SoundClips::NormalDoorOpen has no PCM".to_string());
    }
    let sfx = DeviceClip {
        pcm: sfx_take.clone(),
        ..door
    };

// 3 — positional ambience: surface water, with DFU jitter and pan.
    let source = water_source_position(WATER_BLOCK_LEVEL, LISTENER, 0x5EED_0026);
    let mix = spatial_mix(source, LISTENER);
    let water = sound_bank
        .clip(snd::SoundClips::WaterGentle)
        .map_err(|err| format!("SoundClips::WaterGentle: {err}"))?;
    let ambient = DeviceClip::new("ambient", water.pcm, snd::SAMPLE_RATE).placed(mix);
    if ambient.pcm.is_empty() {
        return Err("SoundClips::WaterGentle has no PCM".to_string());
    }

    Ok(PreparedClips {
        ambient_frames: ambient.pcm.len(),
        music_frames: music.pcm.len(),
        sfx_frames: sfx.pcm.len(),
        #[cfg(feature = "audio")]
        music,
        #[cfg(feature = "audio")]
        sfx,
        #[cfg(feature = "audio")]
        ambient,
        mix,
    })
}

/// Measures block C according to the build: with the `audio` feature it opens
/// the real device; without it, it **declares** the skip reason.
#[cfg(feature = "audio")]
fn measure_device(clips: &PreparedClips) -> DeviceOutcome {
    use daggerfall_nervous::audio::{AudioDevice, RodioDevice};

    let mut device = match RodioDevice::open() {
        Ok(device) => device,
        Err(err) => {
            return DeviceOutcome::Skip {
                reason: err.to_string(),
            }
        }
    };
    let name = device.name().to_string();
    let mut counts = [0u64; 3];
    let entries = [&clips.music, &clips.sfx, &clips.ambient];
    for (slot, clip) in entries.into_iter().enumerate() {
        let before = device.samples_played();
        if let Err(err) = device.play(clip.clone()) {
            return DeviceOutcome::Fail {
                reason: format!("play {}: {err}", clip.label),
            };
        }
// The clip is finite: the counter converges to the total the device asked for.
// Wait for it to stabilise (2 equal reads) with a ceiling of 2× the duration
// + 2 s.
        let timeout = Duration::from_secs_f64(clip.duration_secs() * 2.0 + 2.0);
        let after = wait_until_stable(&device, timeout);
        counts[slot] = after.saturating_sub(before);
        device.reset();
        if counts[slot] == 0 {
            return DeviceOutcome::Fail {
                reason: format!(
                    "clip {} played 0 samples on {name} (the device never pulled audio)",
                    clip.label
                ),
            };
        }
    }
    DeviceOutcome::Pass { name, counts }
}

/// Without the `audio` feature there is no device backend in the binary: the
/// skip is by **compilation**, and it says so.
#[cfg(not(feature = "audio"))]
fn measure_device(_clips: &PreparedClips) -> DeviceOutcome {
    DeviceOutcome::Skip {
        reason: "el binario se compiló sin la feature `audio` (0 backend de dispositivo); \
                 recompila con `--features audio` para medir C"
            .to_string(),
    }
}

/// Waits until the sample counter stops growing.
#[cfg(feature = "audio")]
fn wait_until_stable<D: daggerfall_nervous::audio::AudioDevice>(
    device: &D,
    timeout: Duration,
) -> u64 {
    let start = Instant::now();
    let mut last = device.samples_played();
    let mut stable = 0u32;
    while start.elapsed() < timeout {
        std::thread::sleep(Duration::from_millis(40));
        let now = device.samples_played();
        if now == last {
            stable += 1;
            if stable >= 2 {
                return now;
            }
        } else {
            stable = 0;
            last = now;
        }
    }
    last
}
