//! The law types of the contract (plan §3.1.1), re-exported from their owners.
//!
//! A side writes `use daggerfall_game_core::prelude::*;` and reads the
//! vocabulary — no `path` dependency on 7 organ crates, and no second
//! definition of any type (a re-export cannot drift; a copy can).
//!
//! | Type | Owner crate | Treatment |
//! |:---|:---|:---|
//! | [`EventBus`] / [`GameEvent`] | `nervous` | **frozen**: the canonical channel (not bevy's `Event`) |
//! | [`WeatherType`] / [`HourBand`] | `nervous` | frozen for reading: the presentation vocabulary — weather and day/night band that travel in `GameEvent::WeatherChanged` / `GameEvent::DayNightChanged` |
//! | [`GameSession`] | `heart` | frozen for reading |
//! | [`SaveStateV2`] | `memory` | frozen; the 12 sections stay closed (OD-2) |
//! | [`UiShell`] | `perception` | frozen for reading; entry point of `ui-enchance` |
//! | [`PathChecker`] | `inventory` | frozen: the only door to game data |
//! | [`MeshSpec`] | `parser-bevy-bridge` | frozen: the only door to geometric content |
//! | [`Character`] / [`RpgStats`] | `skills` | frozen for reading |
//! | [`SceneTransition`] | `game-core` (relocated from `app`) | frozen (contract §4) |
//!
//! Demo resources (`DemoWorld`, `M2Grid`, …) are **not** here: they stay
//! internal to `app` or get promoted one by one with their own decision.

pub use crate::scene::{SceneError, SceneTransition, TransitionState};
pub use daggerfall_heart::session::{CareerId, GameSession};
pub use daggerfall_inventory::path_checker::PathChecker;
pub use daggerfall_memory::save_v2::SaveStateV2;
pub use daggerfall_nervous::bus::{BusError, EventBus};
pub use daggerfall_nervous::events::{GameEvent, HourBand, LocationRef, SceneId, SceneTarget, WeatherType};
pub use daggerfall_parser_bevy_bridge::mesh_spec::MeshSpec;
pub use daggerfall_perception::ui_shell::UiShell;
pub use daggerfall_skills::character::Character;
pub use daggerfall_skills::stats::RpgStats;
