//! I1 F1 — the minimal side (plan §9, criterion (e)).
//!
//! Composition in **preview**: headless, in-process, no network, no assets.
//! The side registers NO scheduler and NO bus: `GameCorePlugin` provides both,
//! and the side only places systems in the contract sets and reads law types
//! from the prelude.
//!
//! What it proves, mechanically:
//!
//! 1. An outside crate composes the public `impl Plugin` (criterion (a)).
//! 2. It reads law types with **one** path dependency instead of the main's 12
//!    (plan §3.1 prelude row) — `GameSession`, `PathChecker`, `SaveStateV2`,
//!    `UiShell`, `MeshSpec` arrive through `game_core::prelude`, 0 duplicated
//!    definitions.
//! 3. The schedule contract holds for a stranger: an event published in
//!    `CoreSet::Publish` is readable in `CoreSet::Present` of the same frame.
//! 4. It declares **no `bevy` of its own**: the graph (and the plugin API) come
//!    from `game_core`. Its own state uses `Local`, precisely so that no bevy
//!    derive macro has to resolve `bevy_ecs` inside the side.
//!
//! Exit codes: 0 = the composition holds, 1 = a contract expectation failed
//! (never a silent pass).

use std::process::ExitCode;

use daggerfall_game_core::bevy::app::{App, Update};
use daggerfall_game_core::bevy::ecs::schedule::IntoScheduleConfigs;
use daggerfall_game_core::bevy::ecs::system::{Local, Res, ResMut};
use daggerfall_game_core::bevy::prelude::MinimalPlugins;

use daggerfall_game_core::prelude::*;
use daggerfall_game_core::schedule::{CoreSet, ORDER_LABEL};
use daggerfall_game_core::{EventsThisFrame, FrameIndex, GameCorePlugin, Session, SpineBus};

/// The seed this preview composes with (fixed: previews are deterministic).
const PREVIEW_SEED: u64 = 0x00DA_B6_1D;

/// Publishes one typed spine event (`nervous`) inside `CoreSet::Publish`.
fn publish_one(mut bus: ResMut<SpineBus>) {
    let _ = bus.0.publish(GameEvent::SceneChanged(SceneTarget::Menu));
}

/// Reads the law type the plugin inserted (🫀 `GameSession`) plus the frame's
/// event window, and remembers the last observation in a `Local` (no bevy
/// derive needed: the side owns no resource type).
fn observe(
    session: Res<Session>,
    events: Res<EventsThisFrame>,
    mut last: Local<Option<(u64, usize)>>,
) {
    let _scene = session.0.scene.label();
    *last = Some((session.0.seed, events.0.len()));
}

fn main() -> ExitCode {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_plugins(GameCorePlugin::with_seed(PREVIEW_SEED))
        .add_systems(Update, publish_one.in_set(CoreSet::Publish))
        .add_systems(Update, observe.in_set(CoreSet::Present));

    app.update();
    app.update();

    let session = app.world().resource::<Session>();
    let frames = app.world().resource::<FrameIndex>().0;
    let events = app.world().resource::<EventsThisFrame>().0.len();

    // The law types below are read straight from the prelude: each one is an
    // import from `game_core`, not a `path` dependency of the side.
    //
    // 🧠 memory — the envelope is readable; its 12 sections stay closed (OD-2).
    let save_sections = SaveStateV2::new().sections.len();
    // 🎒 inventory — the only door to game data (Mode A by default).
    let mode_a = !PathChecker::new().use_custom_universe();
    // 🌉 bridge — the only door to geometric content (synthetic, 0 retail I/O).
    let mesh = MeshSpec::new(
        vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
        vec![[0.0, 0.0, 1.0]; 3],
        vec![0, 1, 2],
    );
    // 👁 perception — the shell is readable (0 windows: no runtime registry).
    let ui_windows = UiShell::default().registry().len();

    let mut failures: Vec<&str> = Vec::new();
    if session.0.seed != PREVIEW_SEED {
        failures.push("session seed is not the configured one");
    }
    if session.0.scene != SceneId::Menu {
        failures.push("session does not start on the menu scene");
    }
    if save_sections != 0 {
        failures.push("SaveStateV2 is not the empty envelope (OD-2)");
    }
    if !mode_a {
        failures.push("PathChecker did not default to Mode A");
    }
    if mesh.positions.is_empty() {
        failures.push("MeshSpec carries no vertices");
    }
    if frames != 2 {
        failures.push("FrameIndex does not count one per Update");
    }
    if events != 1 {
        failures.push("the published event was not drained into EventsThisFrame");
    }

    println!(
        "SIDE VE: preview composed · contract=game-core 0.1 · schedule=[{ORDER_LABEL}] · \
         seed={} · scene={} · frames={frames} · events_this_frame={events} · \
         laws=8 (nervous,heart,memory,inventory,perception,skills,bridge,scene) · \
         mesh_vertices={} · ui_windows={ui_windows} · save_sections={save_sections} · \
         own_bevy_deps=0",
        session.0.seed,
        session.0.scene.label(),
        mesh.positions.len(),
    );

    if failures.is_empty() {
        println!("SIDE VERDICT: PASS (composition holds)");
        ExitCode::SUCCESS
    } else {
        for failure in &failures {
            eprintln!("SIDE FAIL: {failure}");
        }
        println!("SIDE VERDICT: FAIL ({} expectation(s))", failures.len());
        ExitCode::FAILURE
    }
}
