//! # daggerfall-game-core — the public in-process contract (I1 · F1)
//!
//! Owner: **main**. Medium: `path` dependency. **No network.** Trust model:
//! OD-1 (a) — native, in-process plugin (plan `I1-INFRA-1-TRUST-DUAL-SIDE-PROJECTS.md`
//! §3.1, §8.2).
//!
//! ## Why this crate exists
//!
//! Before F1 a side had to (1) copy the main's 12 `path` dependencies to read
//! any organ type, (2) invent its own `App` wiring because no `impl Plugin`
//! existed, and (3) guess the system order. All three were undocumented
//! coupling (plan §3.1, R1). This crate fixes the reading surface, the plugin
//! and the order — nothing else.
//!
//! ## Surface
//!
//! | Item | Contract role |
//! |:---|:---|
//! | [`GameCorePlugin`] | the only `impl Plugin` published (criterion (a)) |
//! | [`prelude`] | the law types (§3.1.1) re-exported, 0 duplicated definitions |
//! | [`schedule::CoreSet`] | the declared order `Input -> Simulate -> Publish -> Drain -> Present` |
//! | [`scene`] | `SceneTransition` relocated from the binary (§3.1.1 row 8) |
//! | features [`core`](#features) / `render` / `audio` | additive, opt-in |
//!
//! ## What this crate does NOT own
//!
//! - **No gameplay rules.** It registers no system beyond the bus bridge; the
//!   organs keep their rules and their state.
//! - **No data universe.** `PathChecker` mode is a user toggle: the plugin
//!   never picks Mode A or B on its own.
//! - **No UI.** `UiShell` needs a window registry supplied by the runtime.
//! - **No audio device.** The playback runtime stays in `daggerfall_app`
//!   (rodio behind the main's own `audio` feature); this crate owns no sink.
//! - **No `SaveStateV2` section.** Side state is side-owned (OD-2 (a)).
//!
//! ## Features
//!
//! | Feature | Default | Effect |
//! |:---|:---:|:---|
//! | `core` | **yes** | the contract surface (plugin + prelude + schedule) |
//! | `render` | no | window + render features of the unified bevy graph |
//! | `audio` | no | the bevy audio backend (device/decoder stay the main's) |
//!
//! Features are **additive**: enabling one never deletes or reinterprets
//! another. Follows the convention already in force in
//! `crates/app/Cargo.toml` ("additive OPT-IN", "0 deletes").
//!
//! ## Bevy: one declaration for the whole graph
//!
//! `bevy` comes from `[workspace.dependencies]` (plan §7.3). Cargo unifies
//! features, so a second declaration is a silent change of *everyone's*
//! graph: the contract forbids it. A side declares `compat.bevy` in its
//! manifest and takes the graph the main composes.
//!
//! ## MSRV
//!
//! `rust-version = "1.97.1"` = bevy 0.20.0's own MSRV. Raising it is a
//! **BREAKING** change in `0.x` (below).
//!
//! ## Semver (pre-1.0, plan §3.1)
//!
//! | Change | Version effect | Notice to sides |
//! |:---|:---|:---|
//! | add a type/function/feature | `0.MINOR+1` | none (compatible) |
//! | change observable behaviour of a system | `0.MINOR+1` + migration note | changelog required |
//! | remove/rename a public type, change a signature, change the schedule contract | `0.MINOR+1` with a **BREAKING** section | side revalidation required |
//! | reach `1.0` | frozen: breaking only in MAJOR | — |
//!
//! `compat.core = "^0.1"` in a side means `0.1.x`, **not** `0.2`: that is
//! Cargo's pre-1.0 rule and it avoids assuming compatibility where there is
//! none.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

// The composed bevy surface. A side takes the graph from HERE (plan §7.3): a
// per-side `bevy` dependency would re-open the feature-unification risk this
// contract closes, and the version must be the workspace's, not the side's.
pub use bevy;

pub mod plugin;
pub mod prelude;
pub mod scene;
pub mod schedule;

pub use plugin::{EventsThisFrame, FrameIndex, GameCoreConfig, GameCorePlugin, Session, SpineBus};
pub use scene::{SceneError, SceneTransition, TransitionState};
