//! daggerfall_parser_rs — PHASE 1 parity-first.
//!
//! Clean-room implementation (envelope rec-9) from Dave Humphrey's UESP
//! specification `dfbsa.txt` (2002):
//! <https://en.uesp.net/dagger/dfbsa.txt>
//! sha256 pinned in `docs/sintergic/inputs/2026-10-18-fase1-input-manifest.md`.
//! No byte of Daggerfall Unity (MIT) was copied; DFRandom.cs is used only
//! as an executable spec for PROBE-FORMULA-001 (lanes after PHASE 1).
//!
//! PARITY-FIRST: this crate is additive — 0 modification of the engine core
//! until Mode A parity is verified.

pub mod bsa;
pub mod climate;
pub mod dfrandom;
pub mod image;
pub mod maps;
pub mod mesh;
pub mod midi;
pub mod monster;
pub mod namegen;
pub mod quest;
pub mod rdb;
pub mod rmb;
pub mod rumor;
pub mod snd;
pub mod spells_std;
pub mod text_rsc;
pub mod texture;
pub mod woods;
