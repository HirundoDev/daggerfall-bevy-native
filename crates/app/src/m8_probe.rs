//! PROBE-SESSION-001 — session save/load roundtrip (M8).
//!
//! Anti-screen sequence of the C3 pack (plan §2 M8, INV-L99-4 /
//! REQ-L99-041): **save → exit → reload → restored session**, with asserts
//! over the **5 VE states** (position · stats · inventory · quests · chunks)
//! + the ❤️ session time:
//!
//! 1. mutate the session (M7 script + position + clock) ⇒ `save`;
//! 2. **new app** (fresh resources) ⇒ `load` ⇒ the restored resources produce
//!    a **byte-identical** snapshot and the projected read models are the same
//!    as before exiting;
//! 3. anti-false-positives: load **without a save** fails honestly
//!    (`NotFound`), a **corrupt** save (altered byte) is detected (checksum),
//!    an unsupported **version** is rejected and two saves of the same state
//!    are **byte-identical** (format determinism).
//!
//! Runs in the **default headless** config and **does not read ARCH3D.BSA**
//! (declared synthetic session — delta documented in the M8 note); with
//! `--features window` and PASS, the M8 window (`m8_window`) opens the
//! restored session as visual evidence. Probe saves live in their own
//! temporary directory (0 artifacts in the repo); the VE save in the window
//! uses `saves/session.dfs` (project root).

use std::path::PathBuf;

use bevy::prelude::*;
use daggerfall_memory::session_store::{SaveError, SessionSnapshot};
use daggerfall_parser_bevy_bridge::ProbeCheck;
use daggerfall_perception::read_models::SessionReadModels;

use crate::m7_readmodels::M7ReadModels;
use crate::m8_session::{
    load_session, m8_session_app, save_session, snapshot_from_world,
};

/// Position offset of the M8 script (X/Z millimetres — integers).
pub const M8_SCRIPT_POS_MM: (i64, i64) = (1500, -2000);
/// Yaw of the M8 script (milliradians).
pub const M8_SCRIPT_YAW_MRAD: i64 = 314;
/// ❤️ clock advance of the M8 script (3661 s = 1 h 1 m 1 s ⇒ 12:00:00 →
/// 13:01:01 — DFU integer carry).
pub const M8_SCRIPT_CLOCK_SECS: u64 = 3661;

/// Full report of PROBE-SESSION-001.
#[derive(Debug, Clone)]
pub struct SessionProbeReport {
    /// Checks executed, in order.
    pub checks: Vec<ProbeCheck>,
    /// Global PASS = all checks ok.
    pub pass: bool,
}

/// Temporary probe save directory (hermetic: under `TMPDIR` + pid — cleaned
/// on entry and on exit).
fn probe_save_dir() -> PathBuf {
    std::env::temp_dir().join(format!("daggerfall-m8-probe-{}", std::process::id()))
}

fn probe_save_path() -> PathBuf {
    probe_save_dir().join("session.dfs")
}

/// Expected initial snapshot of the M8 session (built by hand — the probe does
/// not self-validate against its own output).
fn expected_initial_snapshot() -> SessionSnapshot {
    SessionSnapshot {
        pos_x_mm: 0,
        pos_z_mm: 0,
        yaw_mrad: 0,
        clock_year: 405,
        clock_month: 5,
        clock_day: 0,
        clock_hour: 12,
        clock_minute: 0,
        clock_second: 0,
        level: 1,
        experience: 0,
        health: 100,
        max_health: 100,
        stamina: 100,
        max_stamina: 100,
        gold: 100,
        goods: vec![("furs".into(), 1)],
        quests: vec![(
            "q-vendel-shop".into(),
            "Find Vendel's shop".into(),
        )],
        chunks: vec![(0, 0), (0, 1), (1, 0), (1, 1)],
    }
}

/// Executable core of **PROBE-SESSION-001** — the SAME code is run by the CLI
/// and the tests (anti-screen).
pub fn run_session_probe() -> SessionProbeReport {
    let mut checks: Vec<ProbeCheck> = Vec::with_capacity(8);
    let save_path = probe_save_path();
    let _ = std::fs::remove_dir_all(probe_save_dir());

    // ── Played session (M8 script) frozen into a snapshot ─────────────────
    let mut playing = m8_session_app();
    playing.update();
    let initial = snapshot_from_world(playing.world());
    crate::m8_session::apply_m8_script(&mut playing);
    playing.update();
    let before_exit = snapshot_from_world(playing.world());
    let vault_before_exit = playing.world().resource::<M7ReadModels>().0.clone();

    checks.push(ProbeCheck {
        id: "initial_session_matches_expected",
        ok: initial == expected_initial_snapshot(),
        detail: format!(
            "initial state = expected by hand (pos 0/0 · 405-05-00 12:00:00 · HP 100 Lv 1 · \
             gold 100 + furs×1 · 1 quest · 4 chunks) — snapshot {initial:?}"
        ),
    });

    let mutated = [
        before_exit.pos_x_mm != initial.pos_x_mm
            && before_exit.pos_z_mm != initial.pos_z_mm
            && before_exit.yaw_mrad != initial.yaw_mrad,
        (before_exit.clock_hour, before_exit.clock_minute, before_exit.clock_second)
            != (initial.clock_hour, initial.clock_minute, initial.clock_second),
        before_exit.health == 70 && before_exit.level == 2,
        before_exit.gold == 55 && before_exit.goods.len() == 2,
        before_exit.quests.len() == 2,
        before_exit.chunks.len() == 5,
    ];
    checks.push(ProbeCheck {
        id: "script_mutates_all_six_states",
        ok: mutated.iter().all(|changed| *changed),
        detail: format!(
            "the script mutates the 6 snapshot states (pos {}/{} mrad {} · \
             clock {:02}:{:02}:{:02} · HP {} Lv {} · gold {} goods {} · quests {} · chunks {}) \
             before saving",
            before_exit.pos_x_mm,
            before_exit.pos_z_mm,
            before_exit.yaw_mrad,
            before_exit.clock_hour,
            before_exit.clock_minute,
            before_exit.clock_second,
            before_exit.health,
            before_exit.level,
            before_exit.gold,
            before_exit.goods.len(),
            before_exit.quests.len(),
            before_exit.chunks.len(),
        ),
    });

    // ── SAVE (exit the played session) ──────────────────────────────
    let saved = save_session(playing.world_mut(), &save_path).expect("real save");
    let saved_bytes = std::fs::read(&save_path).expect("save on disk");
    checks.push(ProbeCheck {
        id: "save_freezes_session_state",
        ok: saved == before_exit && !saved_bytes.is_empty(),
        detail: format!(
            "save_session freezes the played state identical to the ECS and writes {} bytes to {}",
            saved_bytes.len(),
            save_path.display()
        ),
    });

    // ── RELOAD (NEW app, no previous state) ──────────────────────────
    let mut restored = m8_session_app();
    restored.update();
    let loaded = load_session(restored.world_mut(), &save_path).expect("load");
    restored.update();
    let after_reload = snapshot_from_world(restored.world());
    let text_bytes = loaded.to_text().into_bytes();
    checks.push(ProbeCheck {
        id: "reload_roundtrip_is_byte_equal",
        ok: loaded == before_exit
            && after_reload == before_exit
            && text_bytes == saved_bytes,
        detail: format!(
            "new app + load ⇒ snapshot identical to the saved one (6 states) and file \
             bytes == re-serialization of the loaded snapshot ({} bytes, byte-by-byte \
             roundtrip)",
            text_bytes.len()
        ),
    });

    let vault_restored = restored.world().resource::<M7ReadModels>().0.clone();
    checks.push(ProbeCheck {
        id: "restored_read_models_match_saved_session",
        ok: vault_restored == vault_before_exit,
        detail: format!(
            "read models of the restored session [{}] == those of the session on exit \
             [{}] — the restored session IS visible (title bar/M8 window)",
            vault_restored.hud.summary(),
            vault_before_exit.hud.summary()
        ),
    });

    // ── Anti-false-positives of the roundtrip ──────────────────────────────
    let missing = probe_save_dir().join("never-saved.dfs");
    let _ = std::fs::remove_file(&missing);
    let honest = load_session(restored.world_mut(), &missing).expect_err("load without save");
    checks.push(ProbeCheck {
        id: "load_without_save_fails_honestly",
        ok: SessionSnapshot::is_not_found(&honest),
        detail: format!(
            "load of a nonexistent file ⇒ honest NotFound error (no fabricated default \
             state): {honest}"
        ),
    });

    let corrupt_path = probe_save_dir().join("session-corrupt.dfs");
    let mut corrupt_bytes = saved_bytes.clone();
    // Alters ONE byte of the payload (gold area) without touching the checksum.
    let flip = corrupt_bytes
        .windows(2)
        .position(|window| window == b"55")
        .expect("gold present in the save");
    corrupt_bytes[flip] = b'6';
    std::fs::write(&corrupt_path, &corrupt_bytes).expect("write corrupt");
    let detected = load_session(restored.world_mut(), &corrupt_path).expect_err("must detect");
    checks.push(ProbeCheck {
        id: "corrupted_save_is_detected",
        ok: matches!(detected, SaveError::Corrupt(_)),
        detail: format!(
            "1 altered byte in the save (gold 55→65) ⇒ SaveError::Corrupt by FNV-1a 64 \
             checksum: {detected}"
        ),
    });

    let second_path = probe_save_dir().join("session-deterministic.dfs");
    let deterministic = save_session(playing.world_mut(), &second_path).expect("2nd save");
    let second_bytes = std::fs::read(&second_path).expect("2nd save on disk");
    checks.push(ProbeCheck {
        id: "save_is_deterministic",
        ok: deterministic == saved && second_bytes == saved_bytes,
        detail: "2 saves of the same state ⇒ identical bytes (canonical format, 0 chance)"
            .to_string(),
    });

    let version_path = probe_save_dir().join("session-version-999.dfs");
    let bumped = String::from_utf8(saved_bytes.clone())
        .expect("UTF-8 save")
        .replacen("DFL-SESSION 1", "DFL-SESSION 999", 1);
    std::fs::write(&version_path, bumped).expect("write version");
    let rejected = load_session(restored.world_mut(), &version_path).expect_err("must reject");
    checks.push(ProbeCheck {
        id: "unsupported_version_is_rejected",
        ok: matches!(rejected, SaveError::BadVersion(999)),
        detail: format!("save with version 999 ⇒ SaveError::BadVersion(999): {rejected}"),
    });

    let _ = std::fs::remove_dir_all(probe_save_dir());
    let pass = checks.iter().all(|check| check.ok);
    SessionProbeReport { checks, pass }
}

/// Full restored session for the dispatch evidence — the SAME path as the
/// probe (script → save → new app → load).
fn restored_session_for_evidence() -> (SessionSnapshot, SessionReadModels, PathBuf) {
    let dir = probe_save_dir();
    let path = dir.join("session-evidence.dfs");
    let _ = std::fs::remove_dir_all(&dir);

    let mut playing = m8_session_app();
    playing.update();
    crate::m8_session::apply_m8_script(&mut playing);
    playing.update();
    save_session(playing.world_mut(), &path).expect("evidence save");

    let mut restored = m8_session_app();
    restored.update();
    load_session(restored.world_mut(), &path).expect("evidence load");
    restored.update();
    let snapshot = snapshot_from_world(restored.world());
    let vault = restored.world().resource::<M7ReadModels>().0.clone();
    (snapshot, vault, path)
}

/// CLI dispatch of PROBE-SESSION-001 (M4-M7 scheme): core shared with the
/// tests (anti-screen) + verdict. **0 .BSA reads** (declared synthetic
/// session — delta documented in the M8 note).
pub fn run_session_001() -> i32 {
    println!(
        "PROBE-SESSION-001 - M8 session save/load roundtrip (INV-L99-4, REQ-L99-041, \
         CR-bbd5db82)"
    );
    let report = run_session_probe();
    for check in &report.checks {
        let status = if check.ok { "OK" } else { "FAIL" };
        println!("[{status}] {} - {}", check.id, check.detail);
    }

    if report.pass {
        let (snapshot, vault, _path) = restored_session_for_evidence();
        println!("-- Headless evidence (session restored from the save) --");
        println!(
            "M8 VE: position ({}, {}) mm · yaw {} mrad",
            snapshot.pos_x_mm, snapshot.pos_z_mm, snapshot.yaw_mrad
        );
        println!(
            "M8 VE: ❤️ clock {}-{:02}-{:02} {:02}:{:02}:{:02}",
            snapshot.clock_year,
            snapshot.clock_month,
            snapshot.clock_day,
            snapshot.clock_hour,
            snapshot.clock_minute,
            snapshot.clock_second
        );
        println!("M8 VE: HUD {}", vault.hud.summary());
        println!(
            "M8 VE: automap {} chunks (restored)",
            vault.automap.chunk_count()
        );
        println!("M8 VE: panel {}", vault.panel.summary());
        println!(
            "M8 VE: save → exit → reload ⇒ restored session verified (5 states + clock) · \
             0 .BSA reads (declared synthetic session)"
        );
        println!(
            "M8 VE: the VE save (M8 window) uses {} (project root) — this probe ran \
             hermetically in {}",
            crate::m8_session::default_save_path().display(),
            probe_save_dir().display()
        );
        println!(
            "VERDICT: PROBE-SESSION-001 PASS - byte-by-byte save→load roundtrip + \
             anti-false-positives (no save ⇒ NotFound · corrupt ⇒ checksum · version ⇒ \
             rejection)"
        );
        0
    } else {
        let failed: Vec<&str> = report
            .checks
            .iter()
            .filter(|check| !check.ok)
            .map(|check| check.id)
            .collect();
        println!(
            "VERDICT: PROBE-SESSION-001 FAIL - failed checks: {}",
            failed.join(", ")
        );
        1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_probe_passes() {
        let report = run_session_probe();
        for check in &report.checks {
            assert!(check.ok, "failed check: {} — {}", check.id, check.detail);
        }
        assert!(report.pass);
    }

    #[test]
    fn tampered_snapshot_is_detected_by_reparse() {
        // Complementary anti-false-positive (unit tests in the memory crate): a serialized
        // and tampered snapshot does NOT parse again.
        let snap = expected_initial_snapshot();
        let text = snap.to_text();
        let mut flipped = text.clone();
        let idx = flipped.find("gold=100").expect("gold in the text");
        flipped.replace_range(idx..idx + 8, "gold=101");
        assert!(
            SessionSnapshot::from_text(&flipped).is_err(),
            "an altered save must be rejected"
        );
        // And the original still parses (the tamper was from the mutator, not the environment).
        assert_eq!(SessionSnapshot::from_text(&text).expect("original"), snap);
    }
}
