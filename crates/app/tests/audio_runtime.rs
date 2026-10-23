//! M14 production audio over the retail data — headless integration test.
//!
//! It drives the SAME code the game runs (`daggerfall_app::audio`): effects
//! resolved by their `DAGGER.SND` **directory index** through `AudioEngine` +
//! `RecordingSink`, verified against the measured goldens embedded in
//! `daggerfall_parser_rs::snd::golden_clips()` (C3-L1: 375 rows measured from
//! the retail file), and the city day/night context song rendered from
//! `MIDI.BSA`.
//!
//! Custody (INV-L99-6): 0 retail bytes in the repository, 0 packaged assets —
//! the files are read from the user's Mode A ARENA2 through
//! `daggerfall_app::mode_a::arena2_path` (the resolver the binary uses).
//! A missing file is an explicit SKIP with its reason; no data is fabricated.

use std::path::PathBuf;

use daggerfall_app::audio::{
    self, AudioEngine, GameplayEvent, MusicBank, RecordingSink, SfxBank, WAVE_HEADER_LEN,
};
use daggerfall_parser_rs::{midi, snd};

/// Resolves a Mode A retail file, or reports the explicit SKIP reason.
fn arena2_path(name: &str) -> Option<PathBuf> {
    match daggerfall_app::mode_a::arena2_path(name) {
        Ok(path) => Some(path),
        Err(reason) => {
            eprintln!("SKIP audio_runtime: {name} unavailable ({reason}) — 0 fake audio");
            None
        }
    }
}

/// Gameplay events exercised end-to-end with the DFU member each must play.
const EVENTS: &[(GameplayEvent, snd::SoundClips)] = &[
    (
        GameplayEvent::Fall {
            damage: 5,
            hard: false,
        },
        snd::SoundClips::FallDamage,
    ),
    (
        GameplayEvent::Fall {
            damage: 0,
            hard: true,
        },
        snd::SoundClips::FallHard,
    ),
    (
        GameplayEvent::MeleeSwing { high: true },
        snd::SoundClips::SwingHighPitch,
    ),
    (
        GameplayEvent::MeleeSwing { high: false },
        snd::SoundClips::SwingLowPitch,
    ),
    (GameplayEvent::MeleeHit { die: 1 }, snd::SoundClips::Hit1),
    (GameplayEvent::MeleeHit { die: 5 }, snd::SoundClips::Hit5),
    (GameplayEvent::ArrowShot, snd::SoundClips::ArrowShoot),
    (GameplayEvent::ArrowHit, snd::SoundClips::ArrowHit),
    (GameplayEvent::BodyFall, snd::SoundClips::BodyFall),
    (GameplayEvent::DoorOpen, snd::SoundClips::NormalDoorOpen),
    (GameplayEvent::DoorClose, snd::SoundClips::NormalDoorClose),
];

/// Row of the measured golden set for a clip (`SoundClips` = directory index).
fn golden_for(goldens: &[snd::GoldenClip], clip: snd::SoundClips) -> &snd::GoldenClip {
    goldens
        .iter()
        .find(|row| row.discriminant == clip as i32)
        .unwrap_or_else(|| panic!("no golden row for SoundClips::{}", clip.name()))
}

#[test]
fn gameplay_events_play_the_measured_golden_clips() {
    let Some(path) = arena2_path(audio::SOUND_BANK_FILE) else {
        return;
    };
    let bank = match SfxBank::open(&path) {
        Ok(bank) => bank,
        Err(err) => panic!("retail DAGGER.SND at {path:?} must parse: {err}"),
    };
    let goldens = match snd::golden_clips() {
        Ok(rows) => rows,
        Err(err) => panic!("embedded goldens must parse: {err}"),
    };
    assert_eq!(bank.len(), 459, "retail DAGGER.SND directory records");
    assert_eq!(goldens.len(), snd::SOUND_CLIP_GOLDEN_ROWS);

    for (event, expected) in EVENTS {
        // 1 · mapping: the event selects the DFU member the pack requires.
        let clip = audio::gameplay_clip(*event)
            .unwrap_or_else(|| panic!("{event:?} must map to a DFU clip"));
        assert_eq!(clip, *expected, "{event:?}");
        let golden = golden_for(&goldens, clip);
        assert_eq!(golden.name, clip.name(), "{event:?}");
        // 2 · production resolution: by DIRECTORY INDEX, WAV = 44 + PCM.
        let wav = match bank.wave(clip) {
            Ok(wav) => wav,
            Err(err) => panic!("wave of {}: {err}", clip.name()),
        };
        assert_eq!(
            wav.len(),
            WAVE_HEADER_LEN + golden.samples,
            "{} WAV length",
            clip.name()
        );
        assert_eq!(&wav[..4], b"RIFF", "{} header", clip.name());
        assert_eq!(
            audio::fnv1a64(&wav[WAVE_HEADER_LEN..]),
            golden
                .fnv1a64
                .unwrap_or_else(|| panic!("{} golden without digest", clip.name())),
            "{} PCM digest",
            clip.name()
        );
        assert_eq!(
            bank.pcm_digest(clip)
                .unwrap_or_else(|err| panic!("digest of {}: {err}", clip.name())),
            golden
                .fnv1a64
                .unwrap_or_else(|| panic!("{} golden without digest", clip.name())),
            "{} pcm_digest()",
            clip.name()
        );
    }

    // 3 · engine path: enqueue → drain through a sink → same measurements.
    let mut engine = AudioEngine::new(bank, Box::new(RecordingSink::default()));
    for (event, _) in EVENTS {
        let clip = audio::gameplay_clip(*event)
            .unwrap_or_else(|| panic!("{event:?} must map to a DFU clip"));
        engine.play(clip);
    }
    engine.play_with_gain(snd::SoundClips::Hit3, 0.5);
    assert_eq!(engine.pending(), EVENTS.len() + 1);
    let drained = match engine.drain() {
        Ok(count) => count,
        Err(err) => panic!("every mapped clip must resolve: {err}"),
    };
    assert_eq!(drained, EVENTS.len() + 1);
    assert_eq!(engine.pending(), 0);
    assert_eq!(engine.played().len(), drained);
    for played in engine.played() {
        let golden = golden_for(&goldens, played.clip);
        assert_eq!(played.samples, golden.samples, "{} samples", played.clip.name());
        assert_eq!(Some(played.digest), golden.fnv1a64, "{} digest", played.clip.name());
    }
    let hit3 = engine.played()[EVENTS.len()];
    assert_eq!(hit3.clip, snd::SoundClips::Hit3);
    assert!((hit3.gain - 0.5).abs() < f32::EPSILON);
}

#[test]
fn context_song_renders_deterministically_from_retail() {
    let Some(path) = arena2_path(audio::MUSIC_BANK_FILE) else {
        return;
    };
    let bank = match MusicBank::open(&path) {
        Ok(bank) => bank,
        Err(err) => panic!("retail MIDI.BSA at {path:?} must parse: {err}"),
    };
    // DFU `SongManager`: city + day, DFRandom seeded with the game day 0.
    let song = audio::music_context_song(0, false);
    assert_eq!(song, midi::SongFiles::song_gday___d);
    // The statistics-only parser still accepts the same retail record.
    let bytes = bank
        .song_bytes(song)
        .unwrap_or_else(|err| panic!("record of {}: {err}", song.name()));
    assert!(
        midi::parse_hmi(bytes).is_ok(),
        "parse_hmi must accept the retail record of {}",
        song.name()
    );

    let first = bank
        .render(song)
        .unwrap_or_else(|err| panic!("render of {}: {err}", song.name()));
    let second = bank
        .render(song)
        .unwrap_or_else(|err| panic!("second render of {}: {err}", song.name()));
    assert_eq!(first.digest, second.digest, "render must be deterministic");
    assert_eq!(first.samples, second.samples);
    assert!(first.samples > 0, "rendered song must carry PCM");
    assert!(first.duration_secs > 0.0, "rendered song must have a duration");
    assert_eq!(first.wav.len(), WAVE_HEADER_LEN + first.samples);
    assert!(first.note_events > 0, "retail song must retain note events");
    // Non-silent: not every sample is the 8-bit silence value (128).
    let pcm = &first.wav[WAVE_HEADER_LEN..];
    assert!(
        pcm.iter().any(|byte| *byte != 128),
        "rendered song is silent ({} samples)",
        first.samples
    );
    eprintln!(
        "M14 audio_runtime: {} ({} notes, {} ticks @ {}/{} division/bpm) -> {:.1} s · {} PCM · FNV-1a64 0x{:016x}",
        song.name(),
        first.note_events,
        first.end_tick,
        first.division,
        first.bpm,
        first.duration_secs,
        first.samples,
        first.digest,
    );
}

#[test]
fn missing_sound_bank_is_a_typed_error_not_a_panic() {
    // A path that cannot exist: the bank reports a typed error and the engine
    // never fabricates audio (the runtime degrades to `NullSink`).
    let err = SfxBank::open(std::path::Path::new("/nonexistent/DAGGER.SND"))
        .unwrap_err();
    assert!(err.to_string().contains("read"), "unexpected error: {err}");
}
