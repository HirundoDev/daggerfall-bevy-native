//! Library surface of `daggerfall-app`: the production audio runtime and the
//! Mode A path resolution, shared with the binary and with the integration
//! tests under `tests/` (a binary-only target cannot be linked by them).
//!
//! Everything the binary exposes as a probe/window stays in `main.rs` and its
//! `m*` modules; this crate target carries the pieces consumed from outside
//! the binary (M14: `audio` + `mode_a`; M22: `combat_runtime`; M23:
//! `m23_window`, the demo window's pure magic layer; M2: `m2_kinematics`, the
//! input→displacement math the `window`-gated action system runs, exported so
//! the default test config can execute the input path).

pub mod audio;
pub mod combat_runtime;
pub mod law_runtime;
pub mod m23_window;
pub mod m2_kinematics;
pub mod mode_a;
