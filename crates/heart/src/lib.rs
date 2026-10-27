//! Heart organ — attention focus and long-running coordination (F0 scaffold).
//!
//! Placeholder until the gameplay-loop lane implements attention focus
//! (FDC) and orchestration of multi-step flows.
//!
//! M4: [`clock`] — minimal playable time system (owned by Heart
//! per PRE-3 §3: time orchestration `DaggerfallDateTime`/day-night →
//! heart); pure Rust, 0 dependencies, DFU classic start Mode A.
//!
//! M9: [`scheduler`] — deterministic phase scheduler
//! (`Input`/`Simulation`/`Presentation`) draining pending events in
//! `(priority, seq)` order over the real [`clock::GameClock`].
//!
//! Spine (contract `spine-contracts-CR-f716455c.md` §2): [`session`] — the
//! single [`session::GameSession`] (seed, mode, [`clock::GameClock`],
//! location, scene, rng) with a deterministic `tick`, the day/night band the
//! `DayNightChanged` event carries, and `snapshot`/`restore` over the v2
//! envelope owned by 🧠 memory.
//!
//! M31 (SPEC-M31, REQ-L99-064): [`calendar`] — derived dates, seasons, birth
//! signs and the bit-exact lunar phases; [`weather`] — the climate/season
//! weather table, the `GetWeather` extraction and the outdoor state machine;
//! [`travel`] — map-pixel travel time, trip cost and arrival adjustments. The
//! three modules are pure Rust (0 file I/O, 0 parser edges) and derive from
//! the M4 [`clock::GameClock`] without changing its signature.
//!
//! M35 (REQ-L99-069 (4) and (5), `PROBE-BIOME-001` blocks G and E): [`sky`] —
//! the layered 2D sky L0-L4 (hour bands, L0 palettes, seeded star field, the
//! two moons with their M31 phase, cloud/precipitation/fog descriptors);
//! [`weather_light`] — the §5 weather → light/fog table with its two declared
//! provenance rows. Both are pure arithmetic over [`clock::GameClock`] and
//! consume M31 instead of reimplementing it.

pub mod calendar;
pub mod clock;
pub mod scheduler;
pub mod session;
pub mod sky;
pub mod travel;
pub mod weather;
pub mod weather_light;


/// Canonical organ id (Sintergic organ map, ADR-FW-104).
pub const ORGAN: &str = "heart";

/// Human-readable organ name.
pub fn organ_name() -> &'static str {
    ORGAN
}
