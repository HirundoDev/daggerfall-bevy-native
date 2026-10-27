//! `GameCorePlugin`: the base resources, the schedule contract and the bridge
//! to the spine bus (plan §3.1).
//!
//! # What the plugin inserts
//!
//! | Resource | Law type it carries | Why it is core-owned |
//! |:---|:---|:---|
//! | [`SpineBus`] | `nervous::bus::EventBus` | the canonical channel is the spine's, **not** bevy's `Event` |
//! | [`Session`] | `heart::session::GameSession` | the spine's single session (contract §2) |
//! | [`EventsThisFrame`] | `Vec<nervous::events::GameEvent>` | the frame's drained events, in publication order |
//! | [`FrameIndex`] | `u64` | deterministic frame counter (0 before the first drain) |
//!
//! Resources whose lawful value depends on the *runtime* — not on the core —
//! stay out: `perception::ui_shell::UiShell` needs a window registry and
//! `inventory::path_checker::PathChecker`'s mode is a user toggle. The plugin
//! never invents a data universe nor a UI, so it cannot silently pick one.
//!
//! # Wrappers, not orphan impls
//!
//! bevy's `Resource` cannot be implemented for the organ types (foreign trait,
//! foreign type), so the core owns thin newtypes. They `Deref` to the law
//! type: a side reads `session.0` or `*session` exactly as it would read the
//! organ value, and the organ crate keeps owning the definition.

use bevy::app::{App, Plugin};
use bevy::ecs::resource::Resource;
use bevy::ecs::system::ResMut;

use daggerfall_heart::session::{CareerId, GameSession};
use daggerfall_nervous::bus::EventBus;
use daggerfall_nervous::events::GameEvent;

use crate::schedule::CoreSet;

/// The spine bus as a bevy resource.
#[derive(Resource, Debug, Default, Clone, PartialEq, Eq)]
pub struct SpineBus(pub EventBus);

impl std::ops::Deref for SpineBus {
    type Target = EventBus;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl std::ops::DerefMut for SpineBus {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

/// The spine's single session (contract §2) as a bevy resource.
#[derive(Resource, Debug, Clone, PartialEq, Eq)]
pub struct Session(pub GameSession);

impl std::ops::Deref for Session {
    type Target = GameSession;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl std::ops::DerefMut for Session {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

/// The events drained from the bus **this** frame, in publication order.
///
/// Empty until `CoreSet::Drain` runs; from `CoreSet::Present` on it is the
/// frame's read-only event window (contract, [`crate::schedule`] point 1).
#[derive(Resource, Debug, Default, Clone, PartialEq, Eq)]
pub struct EventsThisFrame(pub Vec<GameEvent>);

impl std::ops::Deref for EventsThisFrame {
    type Target = Vec<GameEvent>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

/// Number of completed frames: `0` before the first drain, `1` after the first
/// `Update`. Deterministic by construction (no wall clock).
#[derive(Resource, Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct FrameIndex(pub u64);

/// What the plugin needs to build a lawful session.
///
/// `Default` is the **neutral preview** session (`seed = 0`, `career = 0`):
/// deterministic, explicit and never presented as "the game's session". The
/// main supplies its own seed and career.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GameCoreConfig {
    /// World seed (contract §2).
    pub seed: u64,
    /// Career the session starts with (contract §2).
    pub career: CareerId,
}

impl Default for GameCoreConfig {
    fn default() -> Self {
        Self {
            seed: 0,
            career: CareerId(0),
        }
    }
}

/// The contract plugin: base resources + the `Update` chain + the bus drain.
///
/// This is the only `impl Plugin` the contract publishes (F1 criterion (a)).
/// A side composes `GameCorePlugin` and then places its own systems in
/// [`CoreSet`]s; it never registers a scheduler, never inserts the bus and
/// never drains it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct GameCorePlugin {
    config: GameCoreConfig,
}

impl GameCorePlugin {
    /// Plugin over an explicit session configuration.
    pub const fn new(config: GameCoreConfig) -> Self {
        Self { config }
    }

    /// Plugin over the neutral preview session with an explicit seed.
    pub fn with_seed(seed: u64) -> Self {
        Self::new(GameCoreConfig {
            seed,
            ..GameCoreConfig::default()
        })
    }

    /// The configuration this plugin builds with.
    pub const fn config(&self) -> GameCoreConfig {
        self.config
    }
}

impl Plugin for GameCorePlugin {
    fn build(&self, app: &mut App) {
        use bevy::prelude::IntoScheduleConfigs;

        app.insert_resource(SpineBus::default())
            .insert_resource(Session(GameSession::new(
                self.config.seed,
                self.config.career,
            )))
            .insert_resource(EventsThisFrame::default())
            .insert_resource(FrameIndex::default())
            .configure_sets(
                bevy::app::Update,
                (
                    CoreSet::Input,
                    CoreSet::Simulate,
                    CoreSet::Publish,
                    CoreSet::Drain,
                    CoreSet::Present,
                )
                    .chain(),
            )
            .add_systems(bevy::app::Update, drain_bus.in_set(CoreSet::Drain));
    }
}

/// Core-owned bridge: hands the tick's queue out and bumps the frame counter.
///
/// It is the **only** drainer (contract point 2): a second one would steal the
/// frame's events.
fn drain_bus(
    mut bus: ResMut<SpineBus>,
    mut events: ResMut<EventsThisFrame>,
    mut frame: ResMut<FrameIndex>,
) {
    events.0 = bus.0.drain_tick();
    frame.0 += 1;
}
