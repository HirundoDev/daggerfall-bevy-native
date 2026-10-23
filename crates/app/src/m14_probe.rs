//! M14 — anti-screen probes for retail audio (REQ-L99-045 +046, DoD §d):
//!
//! - **PROBE-AUDIO-001** (`--probe AUDIO-001`): audio from the original, read
//!   from ARENA2 at runtime (Mode A / PathChecker custody — 0 packaged bytes,
//!   INV-L99-6):
//!   - **(a)** MIDI.BSA catalog: N/total songs of the `SongFiles` catalog
//!     parsed with the project's own HMI parser (`midi::parse_hmi`) — 0 panic,
//!     EOT in every track, 100% of the file records consumed by the catalog;
//!   - **(b)** song trigger by context (city day/night) through the single
//!     production selector `audio::music_context_song` — 30 days × 2 contexts
//!     resolve a record and parse without crashing;
//!   - **(c)** SoundClips effects over `DAGGER.SND` (BSA 0x0200): all
//!     **375/375** DFU members of `snd::SOUND_CLIP_CATALOG` joined 1:1 with
//!     the measured goldens of `snd::golden_clips()` (373 `pcm` + 1 `empty`
//!     (`Invalid = 5`, 0 PCM bytes) + 1 `none` (`None = -1`, no clip) + the
//!     flagged alias `EnemyOrcShamanAttack = 138`). Every `pcm` row is
//!     resolved through the **production** `audio::SfxBank` (DFU
//!     **directory-index** dispatch: `SoundClips` discriminants are positions,
//!     not `RecordID`s — `SoundReader.cs:59-101`, `:126-132` →
//!     `SndFile.cs:158-176`) and compared against the measured PCM digest
//!     (FNV-1a64), sample count, first/last byte, `RecordID` and the 44-byte
//!     WAVE header. The old record-id dispatch is kept as an **aggregated
//!     negative control** (missing/differs/same) that fails when a coincidence
//!     lands on a *different* index with the same digest;
//!   - **(d)** production playback path: `audio::AudioEngine` over the retail
//!     bank with a `RecordingSink`, fed with every mapped `GameplayEvent`,
//!     drained, and compared per event against the same golden rows (order,
//!     samples, digest, WAVE size);
//!   - **(e)** music: `audio::MusicBank` + `audio::music_context_song` for the
//!     city day/night contexts, rendered twice with an identical digest and
//!     non-silent PCM.
//!
//! A missing or unreadable `DAGGER.SND` is a `[FAIL]` row and **exit 2**: the
//! effects block never degrades to a passing verdict, and no row is skipped.
//!
//! **DFU sources (upstream master, fetched 2026-10-19, MIT):**
//! `Assets/Scripts/Game/SongManager.cs` (playlists `_sunnySongs`/`_nightSongs`,
//! `AssignPlaylist`, `SelectCurrentSong`) · `Assets/Scripts/API/DFRandom.cs`
//! (LCG `next*1103515245+12345`, `(next>>16)&0x7FFF`) ·
//! `Assets/Scripts/SongFiles.cs` / `SoundClips.cs` (catalogs — already pinned
//! in `daggerfall_parser_rs::midi`/`snd`).

use daggerfall_app::audio::{
    self, AudioEngine, AudioError, GameplayEvent, MusicBank, RecordingSink, SfxBank, WAVE_HEADER_LEN,
};
use daggerfall_parser_rs::{bsa, midi, snd};

/// Game days walked by the trigger smoke test (context × day).
const SMOKE_DAYS: u32 = 30;

/// `pcm` rows of the measured golden set (contract of `sound-clips-375.tsv`).
const EXPECTED_PCM_ROWS: usize = 373;
/// `empty` rows (`SoundClips::Invalid = 5`, 0 PCM bytes).
const EXPECTED_EMPTY_ROWS: usize = 1;
/// `none` rows (`SoundClips::None = -1`, the DFU "no clip" sentinel).
const EXPECTED_NONE_ROWS: usize = 1;
/// Catalogue rows flagged as an alias of a duplicated discriminant.
const EXPECTED_ALIAS_ROWS: usize = 1;

/// Coverage of the effects block: one counter per row class plus the
/// aggregated record-id negative control. [`Self::line`] is the exact
/// `coverage:` line the probe must print.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
struct EffectsCoverage {
    /// Rows whose catalogue entry and golden row are aligned and dispatched.
    catalog: usize,
    /// Rows the catalogue declares (375).
    total: usize,
    pcm: usize,
    empty: usize,
    none: usize,
    alias: usize,
    id_missing: usize,
    id_differs: usize,
    id_same: usize,
}

impl EffectsCoverage {
    /// Counts one dispatched row.
    fn count_row(&mut self, expect: snd::GoldenExpect, alias: bool) {
        match expect {
            snd::GoldenExpect::Pcm => self.pcm += 1,
            snd::GoldenExpect::Empty => self.empty += 1,
            snd::GoldenExpect::None => self.none += 1,
        }
        if alias {
            self.alias += 1;
        }
    }

    /// Counts one record-id negative-control outcome.
    fn count_id_dispatch(&mut self, outcome: IdDispatch) {
        match outcome {
            IdDispatch::Missing => self.id_missing += 1,
            IdDispatch::Differs { .. } => self.id_differs += 1,
            IdDispatch::Same { .. } => self.id_same += 1,
        }
    }

    /// The `coverage:` line (format fixed by the C3-L3 pack).
    fn line(&self) -> String {
        format!(
            "coverage: catalog={}/{} · pcm={} · empty={} · none={} · alias={} · id_dispatch missing={} differs={} same={}",
            self.catalog,
            self.total,
            self.pcm,
            self.empty,
            self.none,
            self.alias,
            self.id_missing,
            self.id_differs,
            self.id_same,
        )
    }
}

/// What the *wrong* (record-id) dispatch selects for a `SoundClips`
/// discriminant: the RFU lookups compare by **index**, never by digest alone
/// (retail reuses identical PCM across directory positions).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum IdDispatch {
    /// No directory entry carries this `RecordID` (DFU `GetRecordIndex` = -1).
    Missing,
    /// The id resolves to the same positional index (`id == index`).
    Same { index: usize },
    /// The id resolves to a different positional index.
    Differs { index: usize },
}

/// Classifies a record-id lookup against the positional index the DFU
/// dispatch would use.
fn classify_id_dispatch(expected: usize, resolved: Option<usize>) -> IdDispatch {
    match resolved {
        None => IdDispatch::Missing,
        Some(index) if index == expected => IdDispatch::Same { index },
        Some(index) => IdDispatch::Differs { index },
    }
}

/// `true` when the record-id path landed on a **different** index that carries
/// the same PCM digest as the golden of the positional index — the only
/// coincidence that falsifies the negative control.
fn id_dispatch_coincidence(
    expected_index: usize,
    resolved_index: usize,
    resolved_digest: u64,
    golden_digest: Option<u64>,
) -> bool {
    resolved_index != expected_index && golden_digest == Some(resolved_digest)
}

/// Checks that a catalogue row and its golden row describe the same DFU
/// member. `None` = aligned; `Some(reason)` = drift that must be printed as a
/// `[FAIL]` row (the golden is never adjusted).
fn row_alignment(def: &snd::SoundClipDef, golden: &snd::GoldenClip) -> Option<String> {
    if def.name != golden.name {
        return Some(format!(
            "name drift: catalogue {:?} vs golden {:?}",
            def.name, golden.name
        ));
    }
    if def.clip as i32 != golden.discriminant {
        return Some(format!(
            "discriminant drift: catalogue {} vs golden {}",
            def.clip as i32, golden.discriminant
        ));
    }
    None
}

/// The golden row of a clip (`SoundClips` = directory index); the alias shares
/// its twin's discriminant, so the first row of that discriminant is used.
fn golden_for(
    goldens: &[snd::GoldenClip],
    clip: snd::SoundClips,
) -> Option<&snd::GoldenClip> {
    goldens
        .iter()
        .find(|row| row.discriminant == clip as i32)
}

/// Verdict line of the probe. `status` is PASS or FAIL — never a degraded pass.
fn verdict_line(status: &str, detail: &str) -> String {
    format!("VERDICT: PROBE-AUDIO-001 {status} - {detail}")
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
    println!("PROBE-AUDIO-001 - retail audio MIDI.BSA + DAGGER.SND (REQ-L99-045 +046, M14, CR-4634de38)");
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
    // Single copy of the DFU selection (`SongManager.SelectCurrentSong`):
    // `audio::music_context_song` — the runtime consumes the same function.
    let contexts: [(&str, bool); 2] = [("city/day", false), ("city/night", true)];
    let mut triggers_ok = 0usize;
    let mut triggers_total = 0usize;
    for (label, night) in contexts {
        let mut distinct: Vec<&'static str> = Vec::new();
        for day in 0..SMOKE_DAYS {
            triggers_total += 1;
            let song = audio::music_context_song(day, night);
            let record = match song.record_name() {
                Some(record) => record,
                None => {
                    println!("[FAIL] trigger {label} day {day}: selection is song_none");
                    all_ok = false;
                    continue;
                }
            };
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

    // ---------- (c) SoundClips effects over DAGGER.SND (375/375) ----------
    // No degraded pass (audit F-02): the retail bank is required. Missing or
    // unreadable file -> [FAIL] and exit 2, never a passing verdict.
    let bank = match SfxBank::from_arena2() {
        Ok(bank) => bank,
        Err(err @ (AudioError::MissingFile { .. } | AudioError::Io { .. })) => {
            println!("[FAIL] DAGGER.SND unavailable (Mode A / PathChecker): {err}");
            println!(
                "[FAIL] effects coverage: 0/{} SoundClips rows measured - the retail bank is required",
                snd::SOUND_CLIP_COUNT
            );
            println!(
                "{}",
                verdict_line(
                    "FAIL",
                    "DAGGER.SND unavailable: 0 of 375 SoundClips rows measurable - the retail file is required (exit 2)"
                )
            );
            return 2;
        }
        Err(err) => {
            println!("[FAIL] invalid DAGGER.SND: {err}");
            println!(
                "{}",
                verdict_line("FAIL", "invalid DAGGER.SND container - 0 rows measurable")
            );
            return 1;
        }
    };
    let snd_path = bank.path().to_path_buf();
    // Second read of the same resolved file: the negative control drives
    // `snd::SoundBank` (record-id lookups + directory metadata), while every
    // measured row goes through the production `SfxBank`.
    let snd_data = match std::fs::read(&snd_path) {
        Ok(data) => data,
        Err(err) => {
            println!(
                "[FAIL] unreadable DAGGER.SND at {}: {err}",
                snd_path.display()
            );
            println!(
                "{}",
                verdict_line(
                    "FAIL",
                    "DAGGER.SND unreadable - the retail file is required (exit 2)"
                )
            );
            return 2;
        }
    };
    println!(
        "[OK] mode_a DAGGER.SND: {} ({} bytes) · production SfxBank ({} records)",
        snd_path.display(),
        snd_data.len(),
        bank.len()
    );
    let snd_bank = match snd::SoundBank::parse(&snd_data) {
        Ok(bank) => bank,
        Err(err) => {
            println!("[FAIL] invalid DAGGER.SND (reference parse): {err}");
            println!(
                "{}",
                verdict_line("FAIL", "DAGGER.SND does not parse as BSA 0x0200")
            );
            return 1;
        }
    };
    println!(
        "[OK] SoundBank: {} records (BSA 0x0200) · first record_id: {}",
        snd_bank.len(),
        snd_bank
            .record_id_at(0)
            .map(|id| id.to_string())
            .unwrap_or_else(|| "n/a".into())
    );

    let goldens = match snd::golden_clips() {
        Ok(rows) => rows,
        Err(err) => {
            println!("[FAIL] embedded goldens (sound-clips-375.tsv): {err}");
            println!(
                "{}",
                verdict_line("FAIL", "the embedded golden set does not parse")
            );
            return 1;
        }
    };
    let catalog = snd::SOUND_CLIP_CATALOG;
    if catalog.len() != snd::SOUND_CLIP_COUNT || goldens.len() != snd::SOUND_CLIP_GOLDEN_ROWS {
        println!(
            "[FAIL] catalogue {} rows vs goldens {} rows (contract: {} / {})",
            catalog.len(),
            goldens.len(),
            snd::SOUND_CLIP_COUNT,
            snd::SOUND_CLIP_GOLDEN_ROWS
        );
        all_ok = false;
    }
    let mut coverage = EffectsCoverage {
        total: catalog.len(),
        ..Default::default()
    };
    for (row_index, (def, golden)) in catalog.iter().zip(goldens.iter()).enumerate() {
        if let Some(problem) = row_alignment(def, golden) {
            println!(
                "[FAIL] row {row_index}: catalogue SoundClips::{} ({}) vs golden {} ({}) - {problem}",
                def.name, def.clip as i32, golden.name, golden.discriminant
            );
            all_ok = false;
            continue;
        }
        coverage.catalog += 1;
        coverage.count_row(golden.expect, def.alias);
        let discriminant = golden.discriminant;
        let alias_note = if def.alias {
            format!(
                " · alias (shares SoundClips::{} = {discriminant})",
                def.clip.name()
            )
        } else {
            String::new()
        };

        match golden.expect {
            // --- pcm: production resolution by directory index --------------
            snd::GoldenExpect::Pcm => {
                let mut problems: Vec<String> = Vec::new();
                match bank.clip(def.clip) {
                    Ok(clip) => {
                        if clip.index != discriminant as usize
                            || def.clip.index() != Some(clip.index)
                        {
                            problems.push(format!(
                                "index {} (discriminant {discriminant})",
                                clip.index
                            ));
                        }
                        if golden.record_id != Some(clip.record_id) {
                            problems.push(format!(
                                "record_id {} (golden {:?})",
                                clip.record_id, golden.record_id
                            ));
                        }
                        if snd_bank.record_id_at(clip.index) != Some(clip.record_id) {
                            problems.push(format!(
                                "directory record_id {:?} at index {}",
                                snd_bank.record_id_at(clip.index),
                                clip.index
                            ));
                        }
                        if clip.samples != clip.pcm.len() {
                            problems.push(format!(
                                "samples {} vs {} PCM bytes",
                                clip.samples,
                                clip.pcm.len()
                            ));
                        }
                        if clip.samples != golden.samples {
                            problems.push(format!(
                                "samples {} (golden {})",
                                clip.samples, golden.samples
                            ));
                        }
                        if clip.samples == 0 {
                            problems.push("0 PCM samples".to_string());
                        }
                        let digest = audio::fnv1a64(&clip.pcm);
                        if golden.fnv1a64 != Some(digest) {
                            problems.push(format!(
                                "FNV-1a64 0x{digest:016x} (golden {:?})",
                                golden.fnv1a64
                            ));
                        }
                        if bank.pcm_digest(def.clip) != Ok(digest) {
                            problems.push("SfxBank::pcm_digest disagrees with the clip".to_string());
                        }
                        if golden.first != clip.pcm.first().copied() {
                            problems.push(format!(
                                "first {:?} (golden {:?})",
                                clip.pcm.first(),
                                golden.first
                            ));
                        }
                        if golden.last != clip.pcm.last().copied() {
                            problems.push(format!(
                                "last {:?} (golden {:?})",
                                clip.pcm.last(),
                                golden.last
                            ));
                        }
                        match bank.wave(def.clip) {
                            Ok(wave) => {
                                if wave.len() != WAVE_HEADER_LEN + clip.samples {
                                    problems.push(format!(
                                        "WAVE {} B (expected {})",
                                        wave.len(),
                                        WAVE_HEADER_LEN + clip.samples
                                    ));
                                } else {
                                    if &wave[..4] != b"RIFF" || &wave[8..12] != b"WAVE" {
                                        problems.push("WAVE header is not RIFF/WAVE".to_string());
                                    }
                                    if &wave[WAVE_HEADER_LEN..] != clip.pcm.as_slice() {
                                        problems.push(
                                            "WAVE payload differs from the decoded PCM".to_string(),
                                        );
                                    }
                                }
                            }
                            Err(err) => problems.push(format!("SfxBank::wave: {err}")),
                        }
                        if problems.is_empty() {
                            println!(
                                "[OK] SoundClips::{} = {discriminant} → index {} · record_id {} · {} PCM samples · FNV-1a64 0x{digest:016x} · WAVE {} B{alias_note}",
                                def.name,
                                clip.index,
                                clip.record_id,
                                clip.samples,
                                WAVE_HEADER_LEN + clip.samples
                            );
                        } else {
                            all_ok = false;
                            println!(
                                "[FAIL] SoundClips::{} = {discriminant} → index {} · record_id {} · {} PCM samples · FNV-1a64 0x{digest:016x} · golden record_id {:?} / {} samples / 0x{:016x} · {}",
                                def.name,
                                clip.index,
                                clip.record_id,
                                clip.samples,
                                golden.record_id,
                                golden.samples,
                                golden.fnv1a64.unwrap_or(0),
                                problems.join(" · ")
                            );
                        }
                    }
                    Err(err) => {
                        all_ok = false;
                        println!(
                            "[FAIL] SoundClips::{} = {discriminant} → index {discriminant}: production SfxBank refused a `pcm` row: {err}",
                            def.name
                        );
                    }
                }
            }
            // --- empty: typed EmptyClip, 0 PCM bytes, record metadata -------
            snd::GoldenExpect::Empty => {
                let index = discriminant as usize;
                let mut problems: Vec<String> = Vec::new();
                match bank.clip(def.clip) {
                    Err(AudioError::Snd(snd::SndError::EmptyClip { index: got })) => {
                        if got != index {
                            problems.push(format!("EmptyClip index {got} (expected {index})"));
                        }
                    }
                    Err(err) => problems.push(format!("expected EmptyClip, got {err}")),
                    Ok(clip) => problems.push(format!(
                        "expected EmptyClip, decoded {} PCM bytes",
                        clip.samples
                    )),
                }
                match bank.wave(def.clip) {
                    Err(AudioError::Snd(snd::SndError::EmptyClip { .. })) => {}
                    Err(err) => problems.push(format!("WAVE expected EmptyClip, got {err}")),
                    Ok(wave) => problems.push(format!("WAVE has {} bytes", wave.len())),
                }
                match bank.pcm_digest(def.clip) {
                    Err(AudioError::Snd(snd::SndError::EmptyClip { .. })) => {}
                    Err(err) => problems.push(format!("digest expected EmptyClip, got {err}")),
                    Ok(digest) => problems.push(format!("digest 0x{digest:016x} for an empty row")),
                }
                let record_id = snd_bank.record_id_at(index);
                if golden.record_id != record_id {
                    problems.push(format!(
                        "record_id {:?} (golden {:?})",
                        record_id, golden.record_id
                    ));
                }
                if golden.samples != 0 {
                    problems.push(format!("golden samples {} (expected 0)", golden.samples));
                }
                if problems.is_empty() {
                    println!(
                        "[OK] SoundClips::{} = {discriminant} → EmptyClip (0 PCM bytes; DFU \"No valid sound data\") · index {} · record_id {}{alias_note}",
                        def.name,
                        index,
                        record_id.map(|id| id.to_string()).unwrap_or_else(|| "n/a".into())
                    );
                } else {
                    all_ok = false;
                    println!(
                        "[FAIL] SoundClips::{} = {discriminant} → empty row: {}",
                        def.name,
                        problems.join(" · ")
                    );
                }
            }
            // --- none: the DFU sentinel, no directory entry, no request -----
            snd::GoldenExpect::None => {
                let mut problems: Vec<String> = Vec::new();
                if def.clip.index().is_some() {
                    problems.push(format!("index() = {:?} (expected None)", def.clip.index()));
                }
                if golden.record_id.is_some() {
                    problems.push(format!("golden record_id {:?} (expected none)", golden.record_id));
                }
                match bank.clip(def.clip) {
                    Err(AudioError::NoClip { clip }) if clip == def.clip => {}
                    Err(err) => problems.push(format!("expected NoClip, got {err}")),
                    Ok(clip) => problems.push(format!(
                        "expected NoClip, decoded index {} ({} samples)",
                        clip.index, clip.samples
                    )),
                }
                if bank.wave(def.clip).is_ok() {
                    problems.push("SfxBank::wave produced a clip for the sentinel".to_string());
                }
                if problems.is_empty() {
                    println!(
                        "[OK] SoundClips::{} = {discriminant} → no clip (DFU: None) · index() is None · production SfxBank refuses with NoClip (no clip requested from the directory)",
                        def.name
                    );
                } else {
                    all_ok = false;
                    println!(
                        "[FAIL] SoundClips::{} = {discriminant} → none row: {}",
                        def.name,
                        problems.join(" · ")
                    );
                }
            }
        }

        // Negative control (aggregated): what the former, wrong record-id
        // dispatch would select for this discriminant. Compared by INDEX.
        if discriminant >= 0 {
            let expected_index = discriminant as usize;
            let resolved = snd_bank.index_of_record_id(discriminant as u32);
            let outcome = classify_id_dispatch(expected_index, resolved);
            coverage.count_id_dispatch(outcome);
            if let IdDispatch::Differs { index } = outcome {
                match snd_bank.decode_by_record_id(&snd_data, discriminant as u32) {
                    Ok(by_id) => {
                        let by_id_digest = audio::fnv1a64(&by_id.pcm);
                        if by_id.index != index {
                            all_ok = false;
                            println!(
                                "[FAIL] id_dispatch SoundClips::{} = {discriminant}: lookup reported index {index} but decoded index {}",
                                def.name, by_id.index
                            );
                        } else if id_dispatch_coincidence(
                            expected_index,
                            by_id.index,
                            by_id_digest,
                            golden.fnv1a64,
                        ) {
                            all_ok = false;
                            println!(
                                "[FAIL] id_dispatch SoundClips::{} = {discriminant}: RecordID dispatch lands on index {} (record_id {}) carrying the SAME PCM digest 0x{by_id_digest:016x} as the golden of index {expected_index} - digest alone would confuse them",
                                def.name, by_id.index, by_id.record_id
                            );
                        }
                    }
                    Err(snd::SndError::EmptyClip { index: got }) => {
                        if got != index {
                            all_ok = false;
                            println!(
                                "[FAIL] id_dispatch SoundClips::{} = {discriminant}: lookup reported index {index} but the record is empty at index {got}",
                                def.name
                            );
                        }
                    }
                    Err(err) => {
                        all_ok = false;
                        println!(
                            "[FAIL] id_dispatch SoundClips::{} = {discriminant}: RecordID lookup failed in the bank: {err}",
                            def.name
                        );
                    }
                }
            }
        }
    }

    let non_sentinel = coverage.id_missing + coverage.id_differs + coverage.id_same;
    println!(
        "[OK] id_dispatch: {non_sentinel}/{} non-sentinel rows looked up by RecordID (SoundClips::None = -1 carries no id)",
        snd::SOUND_CLIP_COUNT.saturating_sub(EXPECTED_NONE_ROWS)
    );
    println!("{}", coverage.line());
    if coverage.catalog != coverage.total
        || coverage.pcm != EXPECTED_PCM_ROWS
        || coverage.empty != EXPECTED_EMPTY_ROWS
        || coverage.none != EXPECTED_NONE_ROWS
        || coverage.alias != EXPECTED_ALIAS_ROWS
    {
        all_ok = false;
        println!(
            "[FAIL] coverage counts diverge from the measured contract (375 rows = 373 pcm + 1 empty + 1 none, 1 alias)"
        );
    }

    // ---------- (d) production playback path ----------
    // Every `GameplayEvent` variant, through the real engine (queue -> sink),
    // measured against the same golden rows.
    let events = [
        GameplayEvent::Fall { damage: 3, hard: false },
        GameplayEvent::Fall { damage: 0, hard: true },
        GameplayEvent::Fall { damage: 0, hard: false },
        GameplayEvent::MeleeSwing { high: false },
        GameplayEvent::MeleeSwing { high: true },
        GameplayEvent::MeleeHit { die: 1 },
        GameplayEvent::MeleeHit { die: 2 },
        GameplayEvent::MeleeHit { die: 3 },
        GameplayEvent::MeleeHit { die: 4 },
        GameplayEvent::MeleeHit { die: 5 },
        GameplayEvent::MeleeHit { die: 0 },
        GameplayEvent::MeleeHit { die: 6 },
        GameplayEvent::ArrowShot,
        GameplayEvent::ArrowHit,
        GameplayEvent::BodyFall,
        GameplayEvent::DoorOpen,
        GameplayEvent::DoorClose,
    ];
    let mut engine = AudioEngine::new(bank, Box::new(RecordingSink::default()));
    let mut requested: Vec<(GameplayEvent, snd::SoundClips)> = Vec::new();
    let mut events_ok = true;
    for event in events {
        match audio::gameplay_clip(event) {
            Some(clip) => {
                engine.play(clip);
                requested.push((event, clip));
            }
            None => println!("[OK] gameplay {event:?} → no clip (0 requests queued)"),
        }
        if engine.pending() != requested.len() {
            events_ok = false;
            println!(
                "[FAIL] gameplay {event:?}: pending {} vs {} mapped events",
                engine.pending(),
                requested.len()
            );
        }
    }
    match engine.drain() {
        Ok(count) => {
            if count != requested.len() {
                events_ok = false;
                println!(
                    "[FAIL] gameplay drain: {count} clips reached the sink vs {} mapped events",
                    requested.len()
                );
            }
        }
        Err(err) => {
            events_ok = false;
            println!("[FAIL] gameplay drain: {err}");
        }
    }
    if engine.pending() != 0 {
        events_ok = false;
        println!("[FAIL] gameplay queue not empty after drain: {}", engine.pending());
    }
    if engine.played().len() != requested.len() {
        events_ok = false;
        println!(
            "[FAIL] played().len() = {} vs {} mapped events",
            engine.played().len(),
            requested.len()
        );
    }
    for (position, ((event, expected), played)) in requested
        .iter()
        .zip(engine.played().iter())
        .enumerate()
    {
        let mut problems: Vec<String> = Vec::new();
        if played.clip != *expected {
            problems.push(format!(
                "played SoundClips::{} vs expected SoundClips::{}",
                played.clip.name(),
                expected.name()
            ));
        }
        match golden_for(&goldens, *expected) {
            Some(golden) => {
                if played.samples != golden.samples {
                    problems.push(format!(
                        "samples {} (golden {})",
                        played.samples, golden.samples
                    ));
                }
                if Some(played.digest) != golden.fnv1a64 {
                    problems.push(format!(
                        "FNV-1a64 0x{:016x} (golden {:?})",
                        played.digest, golden.fnv1a64
                    ));
                }
                match engine.bank().wave(*expected) {
                    Ok(wave) => {
                        if wave.len() != WAVE_HEADER_LEN + played.samples {
                            problems.push(format!(
                                "WAVE {} B (expected {})",
                                wave.len(),
                                WAVE_HEADER_LEN + played.samples
                            ));
                        }
                        if &wave[..4] != b"RIFF" || &wave[8..12] != b"WAVE" {
                            problems.push("WAVE header is not RIFF/WAVE".to_string());
                        }
                    }
                    Err(err) => problems.push(format!("production wave: {err}")),
                }
            }
            None => problems.push("no golden row for the played clip".to_string()),
        }
        if (played.gain - 1.0).abs() > f32::EPSILON {
            problems.push(format!("gain {}", played.gain));
        }
        if problems.is_empty() {
            println!(
                "[OK] gameplay {event:?} → SoundClips::{} = {} · order {position} · {} PCM samples · FNV-1a64 0x{:016x} · WAVE {} B",
                expected.name(),
                *expected as i32,
                played.samples,
                played.digest,
                WAVE_HEADER_LEN + played.samples
            );
        } else {
            events_ok = false;
            println!(
                "[FAIL] gameplay {event:?} → SoundClips::{} = {} · order {position}: {}",
                expected.name(),
                *expected as i32,
                problems.join(" · ")
            );
        }
    }
    let mapped_events = requested.len();
    if !events_ok {
        all_ok = false;
    }

    // ---------- (e) music: deterministic context render from MIDI.BSA ----------
    let music = match MusicBank::from_arena2() {
        Ok(bank) => Some(bank),
        Err(err) => {
            println!(
                "[FAIL] MIDI.BSA unavailable for the music context render (Mode A / PathChecker): {err}"
            );
            all_ok = false;
            None
        }
    };
    let mut music_ok = 0usize;
    let mut music_total = 0usize;
    if let Some(music) = music.as_ref() {
        for (label, night) in [("city/day", false), ("city/night", true)] {
            music_total += 1;
            let day = 0u32;
            let song = audio::music_context_song(day, night);
            let playlist = if night {
                audio::NIGHT_SONGS
            } else {
                audio::SUNNY_SONGS
            };
            let mut problems: Vec<String> = Vec::new();
            if !playlist.contains(&song) {
                problems.push("selection outside the context playlist".to_string());
            }
            if song.record_name().is_none() {
                problems.push("selection is song_none".to_string());
            }
            let renders = music
                .render(song)
                .and_then(|first| music.render(song).map(|second| (first, second)));
            match renders {
                Ok((first, second)) => {
                    if first.digest != second.digest {
                        problems.push("renders are not deterministic".to_string());
                    }
                    if first.samples == 0 {
                        problems.push("0 PCM samples".to_string());
                    }
                    if first.duration_secs <= 0.0 {
                        problems.push("duration is not positive".to_string());
                    }
                    if first.note_events == 0 {
                        problems.push("0 note events".to_string());
                    }
                    if first.wav.len() != WAVE_HEADER_LEN + first.samples {
                        problems.push(format!(
                            "WAVE {} B (expected {})",
                            first.wav.len(),
                            WAVE_HEADER_LEN + first.samples
                        ));
                    }
                    let pcm = first.wav.get(WAVE_HEADER_LEN..).unwrap_or(&[]);
                    if pcm.is_empty() || pcm.iter().all(|byte| *byte == 128) {
                        problems.push("rendered PCM is silent".to_string());
                    }
                    if problems.is_empty() {
                        music_ok += 1;
                        println!(
                            "[OK] music {label} day {day} → {} · 2 renders identical · FNV-1a64 0x{:016x} · {} PCM samples · {:.2} s · {} note events · WAVE {} B",
                            song.name(),
                            first.digest,
                            first.samples,
                            first.duration_secs,
                            first.note_events,
                            first.wav.len()
                        );
                    } else {
                        all_ok = false;
                        println!(
                            "[FAIL] music {label} day {day} → {}: {}",
                            song.name(),
                            problems.join(" · ")
                        );
                    }
                }
                Err(err) => {
                    all_ok = false;
                    println!("[FAIL] music {label} day {day} → {}: {err}", song.name());
                }
            }
        }
    }
    if music_ok != music_total {
        all_ok = false;
    }

    let status = if all_ok { "PASS" } else { "FAIL" };
    let detail = format!(
        "{}/{} SoundClips rows against the measured goldens (pcm={} · empty={} · none={} · alias={}) · id_dispatch missing={} differs={} same={} · production playback {mapped_events} gameplay events through AudioEngine+RecordingSink · music {music_ok}/{music_total} deterministic context renders · {parsed}/{catalog_total} retail songs parsed (0 panic) · city day/night trigger {triggers_ok}/{triggers_total} without crash · 0 degraded passes",
        coverage.catalog,
        coverage.total,
        coverage.pcm,
        coverage.empty,
        coverage.none,
        coverage.alias,
        coverage.id_missing,
        coverage.id_differs,
        coverage.id_same,
    );
    println!("{}", verdict_line(status, &detail));
    if all_ok {
        0
    } else {
        1
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use daggerfall_parser_rs::dfrandom::DfRandom;

    /// One synthetic golden row (`pcm` shape, one byte of payload).
    fn pcm_row(name: &str, discriminant: i32) -> snd::GoldenClip {
        snd::GoldenClip {
            name: name.to_string(),
            discriminant,
            expect: snd::GoldenExpect::Pcm,
            record_id: Some(discriminant as u32),
            samples: 1,
            fnv1a64: Some(audio::fnv1a64(&[128])),
            first: Some(128),
            last: Some(128),
        }
    }

    /// FNV-1a 64 canonical vectors (offset basis for empty input, and the
    /// published `"a"` value) — the digest used by the clip goldens.
    #[test]
    fn fnv1a64_matches_reference_vectors() {
        assert_eq!(audio::fnv1a64(&[]), 0xcbf2_9ce4_8422_2325);
        assert_eq!(audio::fnv1a64(b"a"), 0xaf63_dc4c_8601_ec8c);
    }

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

    /// Context selection is deterministic: same day → same song, always inside
    /// the context playlist (single production selector, 0 probe-local copies).
    #[test]
    fn music_context_song_is_deterministic_per_day() {
        for night in [false, true] {
            let playlist = if night {
                audio::NIGHT_SONGS
            } else {
                audio::SUNNY_SONGS
            };
            for day in 0..SMOKE_DAYS {
                let first = audio::music_context_song(day, night);
                let second = audio::music_context_song(day, night);
                assert_eq!(first, second, "day {day} night={night}");
                assert!(
                    playlist.contains(&first),
                    "day {day} night={night}: {} outside the playlist",
                    first.name()
                );
            }
        }
        // DFU anchor: day 0 + city/day selects `song_gday___d`.
        assert_eq!(
            audio::music_context_song(0, false),
            daggerfall_parser_rs::midi::SongFiles::song_gday___d
        );
    }

    /// Every song selectable by the smoke contexts exists in the catalog and
    /// resolves a record (0 song_none in playlists).
    #[test]
    fn context_playlists_resolve_records() {
        for playlist in [audio::SUNNY_SONGS, audio::NIGHT_SONGS] {
            for song in playlist {
                assert!(
                    song.record_name().is_some(),
                    "playlist with song_none: {}",
                    song.name()
                );
            }
        }
    }

    /// The embedded catalogue and the embedded golden set are row-aligned and
    /// carry the measured contract counts (hermetic: golden text, no retail
    /// file). Anchors are measured values — never adjusted to fit the code.
    #[test]
    fn catalog_and_goldens_join_all_375_rows() {
        let goldens = snd::golden_clips().expect("embedded goldens must parse");
        assert_eq!(snd::SOUND_CLIP_COUNT, 375);
        assert_eq!(snd::SOUND_CLIP_CATALOG.len(), 375);
        assert_eq!(goldens.len(), snd::SOUND_CLIP_GOLDEN_ROWS);
        let mut coverage = EffectsCoverage {
            total: snd::SOUND_CLIP_CATALOG.len(),
            ..Default::default()
        };
        for (row, (def, golden)) in snd::SOUND_CLIP_CATALOG
            .iter()
            .zip(goldens.iter())
            .enumerate()
        {
            assert_eq!(
                row_alignment(def, golden),
                None,
                "row {row}: {} vs {}",
                def.name,
                golden.name
            );
            coverage.catalog += 1;
            coverage.count_row(golden.expect, def.alias);
        }
        assert_eq!(coverage.catalog, 375);
        assert_eq!(coverage.pcm, EXPECTED_PCM_ROWS);
        assert_eq!(coverage.empty, EXPECTED_EMPTY_ROWS);
        assert_eq!(coverage.none, EXPECTED_NONE_ROWS);
        assert_eq!(coverage.alias, EXPECTED_ALIAS_ROWS);
        assert_eq!(
            coverage.line(),
            "coverage: catalog=375/375 · pcm=373 · empty=1 · none=1 · alias=1 · id_dispatch missing=0 differs=0 same=0"
        );

        let arrow = golden_for(&goldens, snd::SoundClips::ArrowShoot).expect("ArrowShoot row");
        assert_eq!(arrow.discriminant, 3);
        assert_eq!(arrow.record_id, Some(6));
        assert_eq!(arrow.samples, 2259);
        assert_eq!(arrow.fnv1a64, Some(0xaa27_2f37_2d2a_a6fc));
        let fall = golden_for(&goldens, snd::SoundClips::FallDamage).expect("FallDamage row");
        assert_eq!(fall.discriminant, 91);
        assert_eq!(fall.record_id, Some(359));
        assert_eq!(fall.samples, 2226);
        assert_eq!(fall.fnv1a64, Some(0xbb9c_ce9c_0f2e_d458));
        let invalid = golden_for(&goldens, snd::SoundClips::Invalid).expect("Invalid row");
        assert_eq!(invalid.expect, snd::GoldenExpect::Empty);
        assert_eq!(invalid.record_id, Some(8));
        assert_eq!(invalid.samples, 0);
        let none = golden_for(&goldens, snd::SoundClips::None).expect("None row");
        assert_eq!(none.expect, snd::GoldenExpect::None);
        assert_eq!(none.record_id, None);
        // The alias shares its twin's measured row.
        let alias = snd::sound_clip_by_name("EnemyOrcShamanAttack").expect("alias entry");
        assert!(alias.alias);
        let twin = golden_for(&goldens, alias.clip).expect("alias golden");
        assert_eq!(twin.discriminant, 138);
        assert_eq!(twin.samples, 9477);
        assert_eq!(twin.fnv1a64, Some(0x3cf5_0cae_27d6_a5b0));
    }

    /// Row drift is detected (name or discriminant) so it can never pass.
    #[test]
    fn row_alignment_detects_catalog_drift() {
        let def = snd::SoundClipDef {
            name: "ArrowShoot",
            clip: snd::SoundClips::ArrowShoot,
            alias: false,
        };
        assert_eq!(row_alignment(&def, &pcm_row("ArrowShoot", 3)), None);
        let renamed = pcm_row("ArrowShot", 3);
        assert_eq!(
            row_alignment(&def, &renamed),
            Some("name drift: catalogue \"ArrowShoot\" vs golden \"ArrowShot\"".to_string())
        );
        let shifted = pcm_row("ArrowShoot", 4);
        assert_eq!(
            row_alignment(&def, &shifted),
            Some("discriminant drift: catalogue 3 vs golden 4".to_string())
        );
    }

    /// The `coverage:` line is the exact contract format.
    #[test]
    fn coverage_line_matches_the_contract_format() {
        let mut coverage = EffectsCoverage {
            total: 375,
            catalog: 375,
            ..Default::default()
        };
        // 373 pcm rows (one of them the flagged alias) + Invalid + None.
        for _ in 0..(EXPECTED_PCM_ROWS - EXPECTED_ALIAS_ROWS) {
            coverage.count_row(snd::GoldenExpect::Pcm, false);
        }
        coverage.count_row(snd::GoldenExpect::Pcm, true);
        coverage.count_row(snd::GoldenExpect::Empty, false);
        coverage.count_row(snd::GoldenExpect::None, false);
        coverage.count_id_dispatch(classify_id_dispatch(3, None));
        coverage.count_id_dispatch(classify_id_dispatch(3, Some(9)));
        coverage.count_id_dispatch(classify_id_dispatch(3, Some(3)));
        coverage.count_id_dispatch(classify_id_dispatch(4, Some(5)));
        assert_eq!(
            coverage.line(),
            "coverage: catalog=375/375 · pcm=373 · empty=1 · none=1 · alias=1 · id_dispatch missing=1 differs=2 same=1"
        );
    }

    /// The verdict never carries a degraded-pass wording: PASS/FAIL only.
    #[test]
    fn verdict_text_never_says_skip() {
        let pass = verdict_line("PASS", "375/375 rows");
        assert!(pass.starts_with("VERDICT: PROBE-AUDIO-001 PASS - "));
        assert!(!pass.contains("SKIP"));
        let fail = verdict_line("FAIL", "DAGGER.SND unavailable");
        assert!(fail.starts_with("VERDICT: PROBE-AUDIO-001 FAIL - "));
        assert!(!fail.contains("SKIP"));
    }

    /// Record-id dispatch is classified by INDEX and only a coincidence on a
    /// different index falsifies the negative control.
    #[test]
    fn id_dispatch_is_classified_by_index() {
        assert_eq!(classify_id_dispatch(3, None), IdDispatch::Missing);
        assert_eq!(
            classify_id_dispatch(3, Some(3)),
            IdDispatch::Same { index: 3 }
        );
        assert_eq!(
            classify_id_dispatch(3, Some(9)),
            IdDispatch::Differs { index: 9 }
        );
        // Same digest at a different index -> the coincidence that must FAIL.
        assert!(id_dispatch_coincidence(3, 9, 0xaa27_2f37_2d2a_a6fc, Some(0xaa27_2f37_2d2a_a6fc)));
        // Different digest at a different index -> expected, legitimate.
        assert!(!id_dispatch_coincidence(3, 9, 0xdead_beef_dead_beef, Some(0xaa27_2f37_2d2a_a6fc)));
        // Same index (id == index) is not a coincidence.
        assert!(!id_dispatch_coincidence(3, 3, 0xaa27_2f37_2d2a_a6fc, Some(0xaa27_2f37_2d2a_a6fc)));
        // Rows without a measured digest (empty/none) can never coincide.
        assert!(!id_dispatch_coincidence(5, 2, 0x379f_358f_f9aa_90ee, None));
    }
}
