//! `side-contract` — deterministic side-contract bundle tool (DESIGN-v1 §10).
//!
//! Five evaluators (`--generate`, `--verify`, `--check`, `--sync`, `--diff`)
//! each print **exactly one** canonical `side-contract-verdict` 1.0 row on
//! stdout and a `phase:` trace on stderr; `--help` and `--explain` print text
//! without a row (the `command` enum only admits the evaluators). Exits: `0`
//! OK · `1` usage · `2` FAIL|DRIFT · `3` reserved and never used · `4`
//! INCOMPLETE (`internal_error`).
//!
//! The crate is deliberately small: `serde`/`serde_json`/`toml`/`sha2` only,
//! no network, no spawned processes, no clock, no `HashMap` — byte-exact
//! determinism (R1-R10) is the product.

pub mod bundle;
pub mod contract;
pub mod explain;
pub mod facts;
pub mod fsutil;
pub mod globmatch;
pub mod lockfile;
pub mod run;
pub mod semver;
pub mod sideio;
pub mod taxonomy;
pub mod verdict;

pub use taxonomy::Cause;

/// A typed stop: `internal_error` aborts at its origin and never combines
/// with any other cause.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Stop {
    /// Deterministic, path-free detail for the `INCOMPLETE` row.
    Internal(String),
}
