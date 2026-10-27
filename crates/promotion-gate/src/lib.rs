//! # daggerfall-promotion-gate — the promotion gate (I1 · F4-B)
//!
//! Owner: **main**. Medium: a command plus an append-only ledger. **No network.**
//! Plan `I1-INFRA-1-TRUST-DUAL-SIDE-PROJECTS.md` §4.3 (gate), §6.1-6.2
//! (allowlist), §8.2 OD-8 (b) (probe contract), §9 F4.
//!
//! ## Why this crate exists
//!
//! F1-F3 shipped the surfaces a side can use; nothing yet decided whether a
//! side may *compose* into the main. §4.3 defines that decision as seven
//! mechanical checks whose first failure stops, and §4.4 says what does **not**
//! count as evidence. This crate turns the first five checks plus the verdict
//! row into one command: a side cannot talk its way into `promoted`.
//!
//! ## Scope: every §4.3 step is executable (F4-B closed step 6; F6-D closed step 0)
//!
//! | §4.3 step | state | Typed cause on failure |
//! |:---:|:---|:---|
//! | 0 contract.lock vs releases del main (DESIGN-v1 §12.1) | implemented (F6-D) | `contract_lock_missing`, `contract_stale`, `contract_unknown`, `contract_tampered` |
//! | 1 manifest + `readiness.state=ready` | implemented | `manifest_invalid`, `side_not_ready` |
//! | 2 `compat.core` / `compat.bevy` | implemented | `core_incompatible`, `bevy_mismatch` |
//! | 3 report sections + OD-8 machine block | implemented | `readiness_section_missing`, `readiness_machine_block_missing` |
//! | 4 probes exist, run twice, same hash | implemented | `probe_*` (7 causes) |
//! | 5 0 allowlist violations | implemented | `allowlist_violation` |
//! | 6a **preview composes** (side process + runtime `status`) | implemented (F4-B) | `preview_failed`, `preview_timeout`, `runtime_status_unavailable` |
//! | 6b **main baseline** (`main-baseline.toml` vs `HEAD`) | implemented (F4-B) | `main_baseline_failed` |
//! | 7 verdict row | implemented | — |
//!
//! `PASS` requires steps 1-6: [`verdict::Verdict::pass`] cannot be built
//! without both step-6 evidence blocks, so no code path can promote a side that
//! nobody has previewed.
//!
//! ## Surfaces
//!
//! | Item | Contract role |
//! |:---|:---|
//! | [`gate::run_gate`] | one execution: baseline → steps 1-6 → one ledger row |
//! | [`gate::GateConfig`] | side root, main root, ledger path, fleet directory |
//! | [`taxonomy::Cause`] | closed, versioned cause list (23 ids, `TAXONOMY_VERSION`) |
//! | [`verdict::Verdict`] | the row: stdout JSON **and** ledger entry, same object |
//! | [`manifest`] | `side-project.toml` schema 1.3 + §2 validation table |
//! | [`report`] | §4.2 sections and the OD-8 (b) machine block |
//! | [`probes`] | argv-without-shell execution, `{seed}`, timeout, double-run hash |
//! | [`allowlist`] | C1 ledger audit, fail-closed |
//! | [`preview`] | step 6a: spawn the declared preview (argv, no shell), wait for its socket, validate `status` |
//! | [`baseline`] | step 6b: `main-baseline.toml` surface vs `git show HEAD:<path>` |
//!
//! ## Exit codes
//!
//! `0` PASS · `1` usage error (no row) · `2` FAIL · `4` INCOMPLETE
//! (`gate_internal_error`). Exit `3` is retired (it meant
//! `preview_not_implemented` in taxonomy v1 and is deliberately not reused).
//!
//! The normative contract lives in
//! `docs/sintergic/promotion-gate/PROMOTION-GATE-v3.md` (v1 and v2 are history).

pub mod allowlist;
pub mod baseline;
pub mod contract;
pub mod gate;
pub mod main_identity;
pub mod manifest;
pub mod preview;
pub mod probes;
pub mod report;
pub mod taxonomy;
pub mod verdict;

pub use contract::{ContractEvidence, ContractFailure, ContractOk};
pub use gate::{run_gate, GateConfig, RunOutcome, DEFAULT_LEDGER};
pub use taxonomy::{Cause, TAXONOMY_VERSION, VERDICT_SCHEMA_VERSION};
pub use verdict::{BaselineEvidence, EvidenceHashes, PreviewEvidence, StepSixEvidence, Verdict};
