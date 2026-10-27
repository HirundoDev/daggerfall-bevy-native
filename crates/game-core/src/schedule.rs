//! Schedule contract (plan §3.1): what runs in what order, and what a plugin
//! may observe from it.
//!
//! # The five sets
//!
//! Every frame runs the `Update` chain
//! `Input -> Simulate -> Publish -> Drain -> Present` (total order, `chain()`),
//! applied by `GameCorePlugin`. A side places its systems in one of these sets
//! and gets the order **by contract**, not by luck:
//!
//! | Set | What belongs here | What it may touch |
//! |:---|:---|:---|
//! | [`CoreSet::Input`] | devices, queued intents, phase gates | read-only world + its own intent resource |
//! | [`CoreSet::Simulate`] | pure rules over resources (`outcome = rules(state)`), the session tick | organ state |
//! | [`CoreSet::Publish`] | organs publishing typed [`daggerfall_nervous::events::GameEvent`]s | the bus (publish only) |
//! | [`CoreSet::Drain`] | core-owned: `EventBus::drain_tick` into `EventsThisFrame`, frame counter | the bus (drain only) |
//! | [`CoreSet::Present`] | read models, UI projections, audio cues | `EventsThisFrame` (read) + organ state (read) |
//!
//! What a plugin may **observe** from the contract:
//!
//! 1. Every event published in `CoreSet::Publish` of frame N is in
//!    [`crate::EventsThisFrame`] during `CoreSet::Present` of frame N — same
//!    frame, publication order preserved (`EventBus::drain_tick` is FIFO).
//! 2. Nothing else drains the bus: a second drainer would steal the frame's
//!    events and break (1).
//! 3. The order is a `chain()`, so a side system in `Simulate` that writes a
//!    resource is visible to every `Publish`/`Present` system of that frame.
//! 4. `FrameIndex` is 1 on the first `Drain` and increments once per frame.
//!
//! **Not** in this contract (v0.1): `FixedUpdate`. The spine's tick purity
//! (`(state, dt, rng)`, contract §2) is a property of `GameSession::tick`, not
//! of a fixed-step schedule; a side that assumes a fixed rate would be reading
//! a promise nobody made.

use bevy::ecs::schedule::SystemSet;

/// The contract sets of the `Update` schedule, in their declared order.
///
/// Deriving [`SystemSet`] is what makes the order enforceable: a side writes
/// `.in_set(CoreSet::Simulate)` and bevy applies the chain from
/// [`GameCorePlugin`](crate::GameCorePlugin), so "my system runs after input
/// and before publishing" is a machine-checked statement.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CoreSet {
    /// Devices and queued intents (read-only world).
    Input,
    /// Pure rules over organ state.
    Simulate,
    /// Organs publish typed spine events.
    Publish,
    /// Core-owned: bus drain + frame counter.
    Drain,
    /// Read models, UI projections and cues.
    Present,
}

/// The contract order, as data (tests and sides compare against this).
pub const ORDER: [CoreSet; 5] = [
    CoreSet::Input,
    CoreSet::Simulate,
    CoreSet::Publish,
    CoreSet::Drain,
    CoreSet::Present,
];

/// One-line rendering of the contract order (evidence lines, docs).
pub const ORDER_LABEL: &str = "Input -> Simulate -> Publish -> Drain -> Present";
