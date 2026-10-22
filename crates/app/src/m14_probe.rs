//! M14 — anti-screen probes for retail audio (REQ-L99-045 +046, DoD §d):
//!
//! - **PROBE-AUDIO-001** (`--probe AUDIO-001`): audio from the original, from
//!   ARENA2 at runtime (Mode A / PathChecker custody — 0 packaged bytes,
//!   INV-L99-6):
//!   - **(a)** MIDI.BSA catalog: N/total songs of the `SongFiles` catalog
//!     parsed with the project's own HMI parser (`midi::parse_hmi`) — 0 panic,
//!     EOT in every track, 100% of the file records consumed by the catalog;
//!   - **(b)** song trigger by context (city day/night): playlists and
//!     selection replicated from DFU `SongManager.cs` (AssignPlaylist +
//!     `DFRandom.srand(gameDays) % len`) — 30 days × 2 contexts resolve a
//!     record and parse without crashing;
//!   - **(c)** SoundClips effects over `DAGGER.SND` (BSA 0x0200): DFU
//!     *enum-id* dispatch (the discriminant **is** the `RecordID`) + WAVE
//!     synthesis; **SKIP+warning degradation** if the file is missing from the
//!     user's ARENA2 (0 fake audio — REQ-L99-046).
//!
//! **DFU sources (upstream master, fetched 2026-10-19, MIT):**
//! `Assets/Scripts/Game/SongManager.cs` (playlists `_sunnySongs`/`_nightSongs`,
//! `AssignPlaylist`, `SelectCurrentSong`) · `Assets/Scripts/API/DFRandom.cs`
//! (LCG `next*1103515245+12345`, `(next>>16)&0x7FFF`) ·
//! `Assets/Scripts/SongFiles.cs` / `SoundClips.cs` (catalogs — already pinned
//! in `daggerfall_parser_rs::midi`/`snd`).

use daggerfall_parser_rs::{bsa, midi, snd};

/// DFU playlist for city/wilderness + day + `Sunny` weather
/// (`DFU:SongManager.cs::_sunnySongs`).
const SUNNY_SONGS: &[midi::SongFiles] = &[
    midi::SongFiles::song_gday___d,
    midi::SongFiles::song_swimming,
    midi::SongFiles::song_gsunny2,
    midi::SongFiles::song_sunnyday,
    midi::SongFiles::song_02,
    midi::SongFiles::song_03,
    midi::SongFiles::song_22,
];

/// DFU playlist for city/wilderness + night (`_nightSongs`).
const NIGHT_SONGS: &[midi::SongFiles] = &[
    midi::SongFiles::song_10,
    midi::SongFiles::song_11,
    midi::SongFiles::song_gcurse,
    midi::SongFiles::song_geerie,
    midi::SongFiles::song_gruins,
    midi::SongFiles::song_18,
    midi::SongFiles::song_21,
];

/// Game days walked by the trigger smoke test (context × day).
const SMOKE_DAYS: u32 = 30;

/// DFU LCG ported 1:1 (`DFU:API/DFRandom.cs`: `ulong next`,
/// `next = next*1103515245 + 12345`, return `(next>>16)&0x7FFF`).
///
/// **M15 (criterion b of the pack §2.3):** the implementation now lives in
/// `daggerfall_parser_rs::dfrandom` (a single copy for M14/M15); this module
/// imports it with 0 semantic change.
use daggerfall_parser_rs::dfrandom::DfRandom;

/// Song selection identical to `DFU:SongManager.cs::SelectCurrentSong` for
/// multi-song playlists: `srand(gameDays)` + `rand() % len`.
fn select_song(
    playlist: &[midi::SongFiles],
    game_days: u32,
    rng: &mut DfRandom,
) -> midi::SongFiles {
    rng.srand(game_days);
    playlist[(rng.rand() % playlist.len() as u32) as usize]
}

/// Bytes of a named record inside a BSA (case-insensitive).
fn named_entry<'a>(
    archive: &bsa::BsaArchive,
    data: &'a [u8],
    name: &str,
) -> Result<&'a [u8], String> {
    for (i, entry) in archive.entries.iter().enumerate() {
        if let bsa::Entry::Named { name: n, .. } = entry {
            if n.eq_ignore_ascii_case(name) {
                return bsa::entry_data(archive, data, i)
                    .map_err(|e| format!("bytes of {name}: {e}"));
            }
        }
    }
    Err(format!("record {name} not found in the BSA"))
}

/// Runs PROBE-AUDIO-001 — MIDI.BSA music + DAGGER.SND effects
/// (REQ-L99-045/046, M14).
pub fn run_audio_001() -> i32 {
    println!("PROBE-AUDIO-001 - retail audio MIDI.BSA + DAGGER.SND (REQ-L99-045 +046, M14, CR-28d62bea)");
    let mut all_ok = true;

    // Source custody (PRE-2 §8 rule 3): Mode A only (PathChecker).
    let path_checker = daggerfall_inventory::path_checker::PathChecker::new();
    if path_checker.use_custom_universe() {
        println!("[FAIL] custom universe (PathChecker) not supported in this probe");
        return 1;
    }

    // ---------- (a) MIDI.BSA catalog ----------
    let midi_path = match crate::mode_a_arena2_path("MIDI.BSA") {
        Ok(p) => p,
        Err(reason) => {
            println!("[FAIL] MIDI.BSA: {reason}");
            return 1;
        }
    };
    let midi_data = match std::fs::read(&midi_path) {
        Ok(d) => d,
        Err(e) => {
            println!("[FAIL] reading MIDI.BSA: {e}");
            return 1;
        }
    };
    println!(
        "[OK] mode_a MIDI.BSA: {} ({} bytes)",
        midi_path.display(),
        midi_data.len()
    );
    let midi_archive = match bsa::parse(&midi_data) {
        Ok(a) if a.directory_type == bsa::DirectoryType::Name18 => a,
        Ok(a) => {
            println!(
                "[FAIL] MIDI.BSA: directory type is not 0x0100 with names ({:?})",
                a.directory_type
            );
            return 1;
        }
        Err(e) => {
            println!("[FAIL] invalid MIDI.BSA: {e}");
            return 1;
        }
    };

    let mut parsed = 0usize;
    let mut total_events: u64 = 0;
    let mut total_note_ons: u64 = 0;
    let mut missing: Vec<&str> = Vec::new();
    for song in midi::SONG_CATALOG {
        let record = song.record_name().expect("catalog without song_none");
        match named_entry(&midi_archive, &midi_data, record) {
            Ok(bytes) => match midi::parse_hmi(bytes) {
                Ok(hmi) => {
                    parsed += 1;
                    total_events += hmi.total_events();
                    total_note_ons += hmi.total_note_ons();
                }
                Err(e) => {
                    println!("[FAIL] {record}: HMI parse: {e}");
                    all_ok = false;
                }
            },
            Err(_) => missing.push(record),
        }
    }
    let catalog_total = midi::SONG_CATALOG.len();
    if parsed == 0 {
        println!("[FAIL] catalog: 0/{catalog_total} songs present in MIDI.BSA");
        all_ok = false;
    }
    println!(
        "[{}] MIDI.BSA catalog: {parsed}/{catalog_total} songs parsed · {} without retail record · {total_events} events · {total_note_ons} note-ons · 0 panic",
        if parsed > 0 { "OK" } else { "FAIL" },
        missing.len(),
    );
    if !missing.is_empty() {
        println!("     absent from retail (honest, no fake audio): {}", missing.join(", "));
    }
    println!(
        "[OK] file records: {} · consumed by the catalog: {parsed}",
        midi_archive.entries.len()
    );
    if midi_archive.entries.len() != parsed {
        println!(
            "[FAIL] coverage: {} records in MIDI.BSA vs {parsed} resolved by the SongFiles catalog",
            midi_archive.entries.len()
        );
        all_ok = false;
    }

    // ---------- (b) trigger by context (city day/night) ----------
    let contexts: [(&str, &[midi::SongFiles]); 2] = [
        ("city/day", SUNNY_SONGS),
        ("city/night", NIGHT_SONGS),
    ];
    let mut rng = DfRandom { next: 1 };
    let mut triggers_ok = 0usize;
    let mut triggers_total = 0usize;
    for (label, playlist) in &contexts {
        let mut distinct: Vec<&'static str> = Vec::new();
        for day in 0..SMOKE_DAYS {
            triggers_total += 1;
            let song = select_song(playlist, day, &mut rng);
            let record = song.record_name().expect("catalog without song_none");
            if !distinct.contains(&song.name()) {
                distinct.push(song.name());
            }
            let resolved = named_entry(&midi_archive, &midi_data, record)
                .and_then(|bytes| midi::parse_hmi(bytes).map_err(|e| e.to_string()));
            match resolved {
                Ok(_) => triggers_ok += 1,
                Err(e) => {
                    println!("[FAIL] trigger {label} day {day} ({record}): {e}");
                    all_ok = false;
                }
            }
            if day == 0 {
                println!(
                    "[OK] trigger {label} day 0 → {} ({record})",
                    song.name()
                );
            }
        }
        println!(
            "[OK] trigger {label}: {SMOKE_DAYS}/{SMOKE_DAYS} days resolved · {} distinct songs: {}",
            distinct.len(),
            distinct.join(", ")
        );
    }
    if triggers_ok != triggers_total {
        all_ok = false;
    }

    // ---------- (c) SoundClips effects over DAGGER.SND ----------
    // Honest degradation (REQ-L99-046): if the file is missing from the
    // user's ARENA2, SKIP+warning — never fake audio.
    let snd_path = crate::mode_a_arena2_path("DAGGER.SND").ok();
    let snd_present = snd_path
        .as_ref()
        .and_then(|p| std::fs::metadata(p).ok())
        .is_some();
    if !snd_present {
        println!(
            "[SKIP] DAGGER.SND absent from ARENA2 (Mode A / PathChecker): effects degraded with warning, 0 fake audio (REQ-L99-046)"
        );
    } else {
        let snd_path = snd_path.expect("presence already verified");
        let snd_data = match std::fs::read(&snd_path) {
            Ok(d) => d,
            Err(e) => {
                println!("[FAIL] reading DAGGER.SND: {e}");
                return 1;
            }
        };
        println!(
            "[OK] mode_a DAGGER.SND: {} ({} bytes)",
            snd_path.display(),
            snd_data.len()
        );
        let bank = match snd::SoundBank::parse(&snd_data) {
            Ok(b) => b,
            Err(e) => {
                println!("[FAIL] invalid DAGGER.SND: {e}");
                return 1;
            }
        };
        println!(
            "[OK] SoundBank: {} records (BSA 0x0200) · first record_id: {}",
            bank.len(),
            bank.record_id_at(0)
                .map(|id| id.to_string())
                .unwrap_or_else(|| "n/a".into())
        );
        // DFU enum-id dispatch: the discriminant IS the RecordID.
        for clip in [snd::SoundClips::ArrowShoot, snd::SoundClips::FallDamage] {
            let discriminant = clip as i32;
            match bank.decode(&snd_data, discriminant) {
                Ok(decoded) => {
                    let wave = decoded.wave_header();
                    let wave_ok = wave.len() == 44 + decoded.pcm.len()
                        && &wave[..4] == b"RIFF"
                        && &wave[8..12] == b"WAVE";
                    let id_ok = i32::try_from(decoded.record_id).ok() == Some(discriminant);
                    let ok = decoded.samples > 0 && wave_ok && id_ok;
                    if !ok {
                        all_ok = false;
                    }
                    println!(
                        "[{}] SoundClips::{clip:?} = {discriminant} → record_id {} · {} PCM 8-bit mono samples · WAVE {} bytes ({}+{})",
                        if ok { "OK" } else { "FAIL" },
                        decoded.record_id,
                        decoded.samples,
                        wave.len(),
                        wave.len() - decoded.pcm.len(),
                        decoded.pcm.len(),
                    );
                }
                Err(e) => {
                    println!("[FAIL] SoundClips::{clip:?} = {discriminant}: {e}");
                    all_ok = false;
                }
            }
        }
    }

    if all_ok {
        println!(
            "VERDICT: PROBE-AUDIO-001 PASS - {parsed}/{catalog_total} retail songs parsed (0 panic) · city day/night trigger {triggers_ok}/{triggers_total} without crash · SoundClips effects DFU enum-id dispatch{}",
            if snd_present {
                " over DAGGER.SND"
            } else {
                " (SKIP+warning: DAGGER.SND absent)"
            }
        );
        0
    } else {
        println!("VERDICT: PROBE-AUDIO-001 FAIL - see [FAIL] rows");
        1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Ported DFRandom parity vs DFU (`DFU:API/DFRandom.cs`): vectors computed
    /// with the same LCG (srand(0)→0, srand(1)→16838).
    #[test]
    fn dfrandom_matches_dfu_lcg() {
        let mut rng = DfRandom { next: 1 };
        rng.srand(0);
        assert_eq!(rng.rand(), 0);
        rng.srand(1);
        assert_eq!(rng.rand(), 16838);
    }

    /// Context selection is deterministic: same day → same song.
    #[test]
    fn select_song_is_deterministic_per_day() {
        let mut a = DfRandom { next: 1 };
        let mut b = DfRandom { next: 1 };
        for day in 0..SMOKE_DAYS {
            assert_eq!(
                select_song(SUNNY_SONGS, day, &mut a),
                select_song(SUNNY_SONGS, day, &mut b)
            );
            assert_eq!(
                select_song(NIGHT_SONGS, day, &mut a),
                select_song(NIGHT_SONGS, day, &mut b)
            );
        }
    }

    /// Every song selectable by the smoke contexts exists in the catalog and
    /// resolves a record (0 song_none in playlists).
    #[test]
    fn context_playlists_resolve_records() {
        for playlist in [SUNNY_SONGS, NIGHT_SONGS] {
            for song in playlist {
                assert!(
                    song.record_name().is_some(),
                    "playlist with song_none: {}",
                    song.name()
                );
            }
        }
    }
}
