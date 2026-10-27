//! F1 criterion (a)/(b): the plugin exists, it is composable by an outside
//! crate, and the schedule contract it declares is the order the machine
//! actually runs.

use std::sync::{Arc, Mutex};

use bevy::app::{App, Update};
use bevy::ecs::resource::Resource;
use bevy::ecs::schedule::IntoScheduleConfigs;
use bevy::ecs::system::{Res, ResMut};
use bevy::prelude::MinimalPlugins;

use daggerfall_game_core::prelude::*;
use daggerfall_game_core::schedule::{CoreSet, ORDER, ORDER_LABEL};
use daggerfall_game_core::{EventsThisFrame, FrameIndex, GameCorePlugin, Session, SpineBus};

/// Order markers written by one system per set.
#[derive(Resource, Default, Clone)]
struct Trace(Arc<Mutex<Vec<&'static str>>>);

fn input_mark(trace: Res<Trace>) {
    trace.0.lock().expect("trace lock").push("input");
}

fn simulate_mark(trace: Res<Trace>) {
    trace.0.lock().expect("trace lock").push("simulate");
}

/// Which set produced the marker — declared once, used by the order test.
fn present_mark(trace: Res<Trace>) {
    trace.0.lock().expect("trace lock").push("present");
}

/// Publishes exactly one spine event in `CoreSet::Publish`.
fn publish_one(mut bus: ResMut<SpineBus>) {
    bus.0
        .publish(GameEvent::SceneChanged(SceneTarget::Menu))
        .expect("publish");
}

/// How many events the `Present` set saw this frame.
#[derive(Resource, Default, Clone)]
struct Observed(Arc<Mutex<usize>>);

fn observe(events: Res<EventsThisFrame>, observed: Res<Observed>) {
    *observed.0.lock().expect("observed lock") = events.0.len();
}

#[test]
fn plugin_inserts_the_base_resources() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_plugins(GameCorePlugin::with_seed(4242));

    assert!(app.world().get_resource::<SpineBus>().is_some());
    assert!(app.world().get_resource::<EventsThisFrame>().is_some());
    assert_eq!(
        app.world().resource::<Session>().seed,
        4242,
        "the session carries the configured seed"
    );
    assert_eq!(app.world().resource::<FrameIndex>().0, 0, "0 before drain");

    app.update();
    assert_eq!(app.world().resource::<FrameIndex>().0, 1, "1 after drain");
    assert!(app.world().resource::<EventsThisFrame>().0.is_empty());
}

#[test]
fn schedule_order_is_the_declared_one() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_plugins(GameCorePlugin::default());

    let trace = Trace::default();
    let handle = trace.clone();
    app.insert_resource(trace)
        .add_systems(Update, input_mark.in_set(CoreSet::Input))
        .add_systems(Update, simulate_mark.in_set(CoreSet::Simulate))
        .add_systems(Update, present_mark.in_set(CoreSet::Present));

    app.update();

    let seen = handle.0.lock().expect("trace lock").clone();
    assert_eq!(seen, vec!["input", "simulate", "present"]);
    assert_eq!(
        ORDER_LABEL,
        "Input -> Simulate -> Publish -> Drain -> Present"
    );
    assert_eq!(
        ORDER,
        [
            CoreSet::Input,
            CoreSet::Simulate,
            CoreSet::Publish,
            CoreSet::Drain,
            CoreSet::Present
        ]
    );
}

#[test]
fn events_published_in_publish_are_readable_in_present_same_frame() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_plugins(GameCorePlugin::default());

    let observed = Observed::default();
    let handle = observed.clone();
    app.insert_resource(observed)
        .add_systems(Update, publish_one.in_set(CoreSet::Publish))
        .add_systems(Update, observe.in_set(CoreSet::Present));

    app.update();
    assert_eq!(
        *handle.0.lock().expect("observed lock"),
        1,
        "same frame: Publish -> Drain -> Present"
    );
    assert!(
        app.world().resource::<SpineBus>().0.is_empty(),
        "exactly one drainer: the bus is empty after the frame"
    );
}
