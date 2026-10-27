//! Nervous organ — event kinds and inter-organ messaging (F0 scaffold).
//!
//! Lanes L99 replace this placeholder with the lattice event bus integration
//! (publish/subscribe only; organs never call each other directly).
//!
//! **Spine (contract `spine-contracts-CR-f716455c.md` §6):** [`events`] types
//! the nine spine events and the scene vocabulary they carry, and [`bus`] is
//! the `EventBus` (`publish` / `drain_tick`, cap 4 096, `BusError::Full`).
//! The vocabulary lives here because §1 forbids the edge `nervous → app`:
//! the crate that types the messages cannot depend on the crate that
//! orchestrates them.
//!
//! **M34/W34-3:** [`audio`] is the device layer (positional + ambient) over an
//! already decoded PCM mixer: the DFU water placement laws, the pan/rolloff of
//! the 3D ambient source, and a device sink whose `samples_played` counter is
//! incremented by the audio callback itself. It is optional behind the
//! `audio` feature (rodio, already resolved by the workspace), so the default
//! build keeps requiring no audio system libraries.

pub mod audio;
pub mod bus;
pub mod events;

/// Canonical organ id (Sintergic organ map, ADR-FW-104).
pub const ORGAN: &str = "nervous";

/// Human-readable organ name.
pub fn organ_name() -> &'static str {
    ORGAN
}
