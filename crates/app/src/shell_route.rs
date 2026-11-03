//! **M-SHELL · windowless session routes** (`CR-a602cafc`): the
//! `menu → creation → world → save → menu` stretch of the shell as functions that
//! do **not** depend on the window, so the gate can measure it.
//!
//! ## Why this module exists
//!
//! Phase 2 left these transitions inside `m18_window` (the Bevy systems that
//! receive the keys and the composed world's resources). The measured result
//! was `P4 NOT-RUN` and `P9 NOT-RUN` in `scripts/mplay-001.py`: the instrument
//! drives the real binary **without a window**, so the two rows had nobody to
//! produce them — the route existed and was not measurable.
//!
//! Here lives the route, with the same evidence text as the window
//! (`M18 VE:`) and the same contract lines (`SHELL VE:`), without ECS: the
//! functions receive `&mut Shell` (+ the phase, the scene and the slot directory)
//! and return the facts the consumer prints. `m18_window` **calls** these
//! functions (a single owner of the route) and the creation probe exercises them
//! in a headless pass ([`headless_session_pass`]).
//!
//! ## Boundaries (what this module does NOT do)
//!
//! - **Missing scene ⇒ 0 world/hud.** [`enter_playing`] receives
//!   `Option<&SessionScene>`: without a composed world (the headless probe has
//!   none) `world` and `hud` are NOT published — an invented HUD would be a
//!   capture claiming a fact the product did not produce.
//! - **No paths on stdout.** [`headless_session_pass`] works in
//!   `temp/dfl-session-pass-<pid>` and deletes the directory at the end: the
//!   temporary name never enters the evidence (and the gate normalizes `/tmp`).
//! - **Zero new decisions.** The texts, the character token and the order of
//!   the lines are `shell`/`shell_slots`'; this module only composes.

use std::path::{Path, PathBuf};

use crate::m18_phase::AppPhase;
use crate::shell::{self, HudFacts, Shell, ShellState};
use crate::shell_slots::{self, SaveStore};

/// Retail wizard surfaces that `create done` declares (16).
pub const CREATE_SURFACES: usize = daggerfall_app::m38_surfaces::CREATE_SURFACE_COUNT;

/// Facts of the composed world that the `world` line publishes.
///
/// The three counters are the real `DemoScene`'s (`city.buildings`, `npcs`,
///   `panels`) and the label is `DoorWorld::scene_label`'s: the same source
///   the window title uses, so the line cannot diverge from what is on screen.
///   The retail location does NOT travel here: it belongs to [`Shell`]
///   (`shell.location`), which is where `Startup` measured it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionScene {
    /// Label of the composed scene (`Exterior`, `Interior(<block>)`).
    pub label: String,
    /// Edificios compuestos del asentamiento.
    pub buildings: usize,
    /// NPCs del censo compuesto.
    pub npcs: usize,
    /// Panels of the composed 2D layer.
    pub panels: usize,
}

// ── character creation (canon wizard) ──────────────────────────────────────

/// `Right`/`Left` over the wizard: next/previous face of the ring
/// (`shell::CREATE_FACES`) and repaint of the step in progress.
///
/// Returns the resulting face. Outside `Create` it does nothing and returns the
/// face stored in the shell: the key does not invent a state.
pub fn create_face(shell: &mut Shell, right: bool) -> usize {
    let ShellState::Create { step, face } = shell.state else {
        return shell.face;
    };
    let next_face = if right {
        (face + 1) % shell::CREATE_FACES
    } else {
        (face + shell::CREATE_FACES - 1) % shell::CREATE_FACES
    };
    shell.state = ShellState::Create {
        step,
        face: next_face,
    };
    shell.face = next_face;
    // A face change repaints the step in progress: the line is a causal
    // consequence of the key, not input evidence.
    shell.say(
        0.0,
        shell::create_step_line(step, shell::CREATE_STEPS, shell::CREATE_FIELDS[step - 1]),
    );
    next_face
}

/// `Down` over the wizard: next canon step (it stops at the last one).
///
/// Returns the step in progress. Outside `Create` it returns 0 and touches nothing.
pub fn create_next_step(shell: &mut Shell) -> usize {
    let ShellState::Create { step, face } = shell.state else {
        return 0;
    };
    let next = if step < shell::CREATE_STEPS {
        step + 1
    } else {
        shell::CREATE_STEPS
    };
    shell.state = ShellState::Create { step: next, face };
    shell.face = face;
    shell.say(
        0.0,
        shell::create_step_line(next, shell::CREATE_STEPS, shell::CREATE_FIELDS[next - 1]),
    );
    next
}

/// Opens the wizard at step 1 with face 0 (the `Return` over `new-game`).
pub fn create_begin(shell: &mut Shell) {
    shell.state = ShellState::Create { step: 1, face: 0 };
    shell.face = 0;
    shell.panel = shell::ShellPanel::None;
    shell.dwell = 0.0;
    shell.say(
        0.0,
        shell::create_state_line(
            CREATE_SURFACES,
            daggerfall_app::m38_surfaces::CREATE_HOST,
            0,
        ),
    );
}

/// `Return` over the wizard: confirms the character and enters the world.
///
/// Publishes `create done`, the `session` line of the first session (no slots in
/// the profile) and delegates to [`enter_playing`] with `new_game = true`, so
/// phrase and state cannot diverge. Returns the character token; `None`
/// if the key arrives outside `Create` (nothing to confirm).
///
/// `slots_present` is the number of occupied slots in the profile: the
/// consumer counts it (`shell_slots::scan`), not this module — the directory
/// belongs to the process and is not guessed here.
pub fn confirm_create(
    shell: &mut Shell,
    phase: &mut AppPhase,
    scene: Option<&SessionScene>,
    slots_present: usize,
) -> Option<String> {
    let ShellState::Create { face, .. } = shell.state else {
        return None;
    };
    let character = shell::character_token(face);
    shell.character = Some(character.clone());
    shell.face = face;
    shell.panel = shell::ShellPanel::None;
    // M-QST: a new game is a new session — the quest returns to
    // `Unstarted` so that `enter_playing`'s trigger can offer it.
    // (Load/continue does NOT go through here: it rehydrates from the document.)
    shell.mainquest = crate::mainquest_route::MainQuest::new();
    // M-NEW (CR-178183d7): a new game starts from zero. The session
    // clock returns to the seed (`SESSION_CLOCK_SEED`; only fast travel and
    // load move it), the pause carries no minutes over and the loaded slot
    // stops being tied to the session: if play continues, the next save
    // chooses the first free slot instead of rewriting the one that was loaded.
    // `continue`/`load` do NOT go through here — [`apply_load`] is the only route
    // that preserves, and that is the difference between the two menu entries.
    shell.clock_minutes = shell::SESSION_CLOCK_SEED;
    shell.paused_ticks = shell::SESSION_CLOCK_SEED;
    shell.session_slot = None;
    shell.say(
        0.0,
        shell::create_done_line(CREATE_SURFACES, &character),
    );
    let first_session = shell.session_slot.is_none();
    if first_session && slots_present == 0 {
        shell.say(0.0, shell::session_line(&character, None));
    }
    enter_playing(shell, phase, scene, true);
    Some(character)
}

// ── entrada al mundo ────────────────────────────────────────────────────────

/// Enters `Playing` (creation, `continue` or load): phase, world and HUD at
/// once, so phrase and state cannot diverge.
///
/// **Without a scene `world` and `hud` are NOT published** — the headless
/// consumer has no composed world and the HUD is a measured fact, not filler.
/// The `new_game` line IS published: entering the world is a shell fact.
///
/// The phase line includes `phase=<LABEL>` (`phase=PLAYING`) and is printed
/// **after** draining the queue: the evidence of the state that caused it
/// (`create done`, `session`) comes before the phrase, and the return to the menu
/// (`back_to_menu`) always publishes `menu state=Menu` after that phrase.
pub fn enter_playing(
    shell: &mut Shell,
    phase: &mut AppPhase,
    scene: Option<&SessionScene>,
    new_game: bool,
) {
    let next = crate::m18_phase::phase_after_enter(*phase);
    if next != *phase {
        shell.drain();
        println!(
            "M18 VE: phase {} → {} (phase={}) — world systems live, the clock starts at the \
             menu's 0 minutes",
            phase.label(),
            next.label(),
            next.label()
        );
        *phase = next;
    }
    shell.state = ShellState::Playing;
    shell.panel = shell::ShellPanel::None;
    shell.map_open = false;
    shell.dwell = 0.0;
    let ticks = shell.clock_minutes;
    if new_game {
        shell.say(0.0, shell::new_game_line(ticks));
    }
    // M-QST: the trigger enters AFTER `state = Playing` and BEFORE the
    // missing-scene early return — with `scene = None` the headless pass
    // (and therefore the measurement) sees the offer too. `offer` only emits from
    // `Unstarted`, so `continue`/load do not re-offer what is already decided.
    crate::mainquest_route::offer(shell);
    let Some(scene) = scene else {
        return;
    };
    if let Some(location) = shell.location {
        let line = shell::world_line(
            &scene.label,
            location,
            scene.buildings,
            scene.npcs,
            scene.panels,
        );
        shell.say(0.0, line);
    }
    let hud = shell.hud.clone();
    shell.say(0.0, shell::hud_line(true, &hud));
}

// ── slots: abrir el browser y escribir ──────────────────────────────────────

/// Opens the save browser over the store already swept by the consumer.
///
/// Returns the occupied slots (the `slots.len()` the window publishes in its
/// `M18 VE:` line), so the caller does not recompute the census.
pub fn open_save(shell: &mut Shell, store: SaveStore) -> usize {
    let occupied = store.count();
    shell.slots = store.slots;
    shell.slots_rejected = store.rejected;
    shell.browser_slot = 0;
    shell.panel = shell::ShellPanel::Save;
    shell.say(
        0.0,
        shell::save_open_line(shell_slots::SLOT_COUNT, shell_slots::FORMAT),
    );
    occupied
}

/// Writes the chosen slot (the first free one, or the cursor's if none is
/// left) with the real session clock and refreshes the browser census.
///
/// `dir` is the slot directory the consumer resolves
/// (`shell_slots::save_dir()`): an implicit directory is not guessed here.
/// Returns `(slot, bytes, digest)` of the written v3 document.
pub fn write_slot(shell: &mut Shell, dir: &Path) -> Result<(usize, usize, u64), String> {
    let Some(character) = shell.character.clone() else {
        return Err("no session character: nothing to write".to_string());
    };
    let occupied: Vec<usize> = shell.slots.iter().map(|slot| slot.index).collect();
    let index = (0..shell_slots::SLOT_COUNT)
        .find(|at| !occupied.contains(at))
        .unwrap_or(shell.browser_slot);
    // `CR-7a3e9c14`: if the session measured a world, the slot declares it
    // (generation 4, same v3 grammar); if it did not, the slot stays the
    // generation 3 one and the evidence says `fidelity=3` with its reason — the
    // door invents neither position nor world clock.
    let written = match shell.snapshot {
        Some(world) => shell_slots::write_in_with_world(
            dir,
            index,
            &character,
            shell.face,
            shell.clock_minutes,
            shell.mainquest.state(),
            &world,
        ),
        None => shell_slots::write_in(
            dir,
            index,
            &character,
            shell.face,
            shell.clock_minutes,
            shell.mainquest.state(),
        ),
    };
    match written {
        Ok((bytes, digest)) => {
            shell.session_slot = Some(index);
            match shell.snapshot {
                Some(world) => shell.say(0.0, shell::slot_fidelity_line(index, &world)),
                None => shell.say(0.0, shell::slot_fidelity_absent_line(index)),
            }
            shell.say(
                0.0,
                shell::save_line(index, shell_slots::FORMAT, bytes, digest),
            );
            let store = shell_slots::scan_in(dir);
            shell.slots = store.slots;
            shell.slots_rejected = store.rejected;
            Ok((index, bytes, digest))
        }
        Err(err) => Err(format!("slot {index} not written: {err}")),
    }
}

// ── regreso al menú ─────────────────────────────────────────────────────────

/// `b` while paused: the session returns to the menu with the selection at 0
/// (the `new-game` item), without redeclaring the set of items (already declared on entry).
///
/// Publishes `session state=Menu from=Playing` and the menu state: the phase
/// phrase [`enter_playing`] printed comes **before** these two lines by
/// construction (the queue only moves forward), so the
/// `PLAYING → menu` sequence is the same one the window shows.
pub fn back_to_menu(shell: &mut Shell) {
    shell.state = ShellState::Menu {
        selection: shell::MENU_NEW_GAME,
    };
    shell.panel = shell::ShellPanel::None;
    shell.map_open = false;
    shell.dwell = 0.0;
    shell.say(0.0, shell::session_menu_line());
    shell.say(
        0.0,
        shell::menu_line(shell::MENU_ITEMS.len(), shell::MENU_NEW_GAME),
    );
}

// ── headless pass of the session walkthrough ───────────────────────────────

/// Pass directory: `temp/dfl-session-pass-<pid>` (deleted at the end).
fn pass_dir() -> PathBuf {
    std::env::temp_dir().join(format!("dfl-session-pass-{}", std::process::id()))
}

/// Walks `menu → creation → PLAYING → save → menu` without a window and publishes
/// the **same** evidence the window produces with the same keys.
///
/// It is the pass that makes `P4` (`create done … state=Playing`) and `P9`
/// (`phase=PLAYING` before `menu state=Menu`) measurable in a binary without a
/// graphics server: the creation probe calls it at the end of its measurement
/// and the `scripts/mplay-001.py` gate reads the stdout lines.
///
/// Honesty: the pass does **not** invent a scene (0 `world`/`hud` — see
/// [`enter_playing`]) nor writes paths; the slot directory is temporary and is
/// deleted at the end even if a step fails.
pub fn headless_session_pass() -> Result<(), String> {
    let dir = pass_dir();
    if dir.exists() {
        std::fs::remove_dir_all(&dir)
            .map_err(|err| format!("pass dir {}: {err}", dir.display()))?;
    }
    std::fs::create_dir_all(&dir)
        .map_err(|err| format!("pass dir {}: {err}", dir.display()))?;
    let result = session_pass(&dir);
    let _ = std::fs::remove_dir_all(&dir);
    result
}

/// Body of the pass (separated so the `remove_dir_all` is unconditional).
fn session_pass(dir: &Path) -> Result<(), String> {
    // No world facts: the headless shell composes no scene. The fields that
    // are NOT published (hud/location/settlements) stay declared absent, never
    // filled with zeros that look measured.
    let mut shell = Shell::new(
        Vec::new(),
        HudFacts {
            panel: crate::m18_phase::HUD_PANEL_ID.to_string(),
            opaque: 0,
            digest: 0,
        },
        None,
        [0, 0],
        Vec::new(),
        None,
    );
    let mut phase = AppPhase::Menu;
    shell.state = ShellState::Menu {
        selection: shell::MENU_NEW_GAME,
    };
    // `Return` over `new-game`: opens the wizard.
    create_begin(&mut shell);
    // The canon advances (the gate driver confirms at step 2).
    create_next_step(&mut shell);
    let slots_present = shell_slots::scan_in(dir).count();
    let character = confirm_create(&mut shell, &mut phase, None, slots_present)
        .ok_or_else(|| "create: the wizard was not open".to_string())?;
    if shell.state != ShellState::Playing {
        return Err(format!("create: `{character}` did not enter Playing"));
    }
    // `Escape` (pausa) → `s` (browser) → `Return` (escribe el slot v3).
    shell.state = ShellState::Paused;
    shell.dwell = 0.0;
    shell.paused_ticks = shell.clock_minutes;
    open_save(&mut shell, shell_slots::scan_in(dir));
    let (slot, _bytes, _digest) = write_slot(&mut shell, dir)?;
    if shell.session_slot != Some(slot) {
        return Err(format!("save: slot {slot} not recorded in the session"));
    }
    // `b`: the session returns to the menu.
    back_to_menu(&mut shell);
    shell.drain();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shell() -> Shell {
        Shell::new(
            Vec::new(),
            HudFacts {
                panel: crate::m18_phase::HUD_PANEL_ID.to_string(),
                opaque: 0,
                digest: 0,
            },
            None,
            [0, 0],
            Vec::new(),
            None,
        )
    }

    #[test]
    fn drain_publishes_every_ready_line_and_stops_at_a_live_delay() {
        let mut shell = shell();
        shell.say(0.0, "SHELL VE: a".to_string());
        shell.say(0.0, "SHELL VE: b".to_string());
        shell.say(5.0, "SHELL VE: late".to_string());
        shell.say(0.0, "SHELL VE: blocked-behind-late".to_string());
        assert_eq!(shell.drain(), 2);
        assert_eq!(shell.drain(), 0, "the live delay is still live");
        // One line per `pump`: the strict order does not change.
        shell.pump(5.0);
        assert_eq!(shell.drain(), 1, "the late line became ready");
    }

    #[test]
    fn create_route_advances_the_canon_and_confirms_the_token() {
        let mut shell = shell();
        let mut phase = AppPhase::Menu;
        create_begin(&mut shell);
        assert_eq!(
            shell.state,
            ShellState::Create { step: 1, face: 0 },
            "the wizard opens at step 1 with face 0"
        );
        assert_eq!(create_face(&mut shell, true), 1);
        assert_eq!(create_next_step(&mut shell), 2);
        assert_eq!(
            shell.state,
            ShellState::Create { step: 2, face: 1 },
            "the canon step and the face are independent"
        );
        let character = confirm_create(&mut shell, &mut phase, None, 0)
            .expect("the wizard was open");
        assert_eq!(character, shell::character_token(1));
        assert_eq!(phase, AppPhase::Playing);
        assert_eq!(shell.state, ShellState::Playing);
        assert_eq!(shell.character.as_deref(), Some(character.as_str()));
    }

    #[test]
    fn enter_playing_without_a_scene_publishes_no_world_or_hud() {
        let mut shell = shell();
        let mut phase = AppPhase::Menu;
        enter_playing(&mut shell, &mut phase, None, true);
        assert_eq!(phase, AppPhase::Playing);
        assert_eq!(shell.state, ShellState::Playing);
        // Only `new_game` + the quest offer (M-QST): no invented `world` or
        // `hud`. The offer comes out of here precisely because the
        // missing-scene early return goes AFTER the trigger.
        assert_eq!(shell.mainquest.state(), daggerfall_behavior::mainquest::State::Offered);
        assert_eq!(shell.drain(), 2);
    }

    #[test]
    fn enter_playing_with_a_scene_publishes_world_and_hud() {
        let mut shell = shell();
        let mut phase = AppPhase::Menu;
        let scene = SessionScene {
            label: "Exterior".to_string(),
            buildings: 2,
            npcs: 3,
            panels: 4,
        };
        enter_playing(&mut shell, &mut phase, Some(&scene), false);
        // `world` requires a retail location: without it only the HUD is published,
        // plus the first entry's quest offer (M-QST).
        assert_eq!(shell.drain(), 2);
        shell.location = Some((17, 50026, 0));
        let mut phase = AppPhase::Playing;
        enter_playing(&mut shell, &mut phase, Some(&scene), false);
        // The offer does not repeat (the state is no longer `Unstarted`).
        assert_eq!(shell.mainquest.offers(), 1);
        assert_eq!(shell.drain(), 2, "world + hud");
    }

    #[test]
    fn back_to_menu_returns_to_new_game_and_publishes_the_two_lines() {
        let mut shell = shell();
        shell.state = ShellState::Paused;
        back_to_menu(&mut shell);
        assert_eq!(
            shell.state,
            ShellState::Menu {
                selection: shell::MENU_NEW_GAME
            }
        );
        assert_eq!(shell.drain(), 2, "session state=Menu + menu state=Menu");
    }

    #[test]
    fn the_headless_pass_runs_and_leaves_no_directory() {
        let dir = pass_dir();
        let _ = std::fs::remove_dir_all(&dir);
        assert!(headless_session_pass().is_ok());
        assert!(!dir.exists(), "the pass removes its own directory");
    }
}
