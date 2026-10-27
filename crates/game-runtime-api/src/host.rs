//! The embedding side of the contract.
//!
//! This crate never touches the game and never captures a frame. [`RuntimeHost`]
//! is the seam the embedding binary implements: the server enforces the
//! lifecycle table and the peer policy, then delegates the actual effect to the
//! host and treats its answer as truth.
//!
//! Host hooks are called while the server holds its state lock, so an
//! implementation must not re-enter the server (no `handle_authenticated_request`
//! from inside `apply`) and must return promptly.

use std::collections::BTreeMap;

use crate::lifecycle::ControlCommand;

/// Canonical format tag of an RGBA8 frame.
pub const SCREENSHOT_FORMAT_RGBA8: &str = "rgba8";

/// Metadata of a frame the host already captured.
///
/// The crate never owns pixels: it publishes width, height, format and the
/// SHA-256 of the bytes so a QA agent can diff two observations without
/// streaming a framebuffer through the control socket.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Screenshot {
    /// Frame width in pixels.
    pub width: u32,
    /// Frame height in pixels.
    pub height: u32,
    /// Canonical format tag; v1 hosts report [`SCREENSHOT_FORMAT_RGBA8`].
    pub format: &'static str,
    /// Lowercase hex SHA-256 of the captured bytes.
    pub sha256: String,
}

impl Screenshot {
    /// Builds an RGBA8 screenshot descriptor.
    pub fn new(width: u32, height: u32, sha256: impl Into<String>) -> Self {
        Screenshot {
            width,
            height,
            format: SCREENSHOT_FORMAT_RGBA8,
            sha256: sha256.into(),
        }
    }
}

/// The game side of the runtime contract.
///
/// Every method is synchronous: the server calls it from the connection thread
/// while holding the state lock, so the embedding binary stays in control of
/// its own threading.
pub trait RuntimeHost: Send + Sync {
    /// Applies `command` and returns the host's **full** observable loaded-side
    /// set afterwards.
    ///
    /// The returned set replaces the server's view (the host is truth): the
    /// server commits it only when this call succeeds. On `Err` the server
    /// reports `host_error` and commits nothing - in particular the lifecycle
    /// state is unchanged, so a failed `stop` is visible as a still-running
    /// instance instead of a silent zombie.
    fn apply(&self, command: &ControlCommand) -> Result<Vec<daggerfall_content_api::SideId>, String>;

    /// Returns the current frame metadata, or `None` when no frame is
    /// available (headless run, window not yet created).
    fn screenshot(&self) -> Result<Option<Screenshot>, String>;

    /// Returns host-owned counters to be merged into `metrics.host`.
    fn metrics(&self) -> Result<BTreeMap<String, u64>, String>;
}
