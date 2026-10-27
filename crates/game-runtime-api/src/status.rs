//! Read-only projections of the server: `status`, `metrics` and their counters.
//!
//! Every projection is a value with `Eq`, so an embedder asserts on exact state
//! and the JSON form is produced from that same value.

use std::collections::BTreeMap;

use daggerfall_content_api::SideId;
use serde_json::{json, Value};

use crate::api::ApiVersion;
use crate::lifecycle::LifecycleState;
use crate::socket::InstanceId;

/// Monotonic counters of one server instance.
///
/// `served` counts **successful** responses only: a frame rejected for its
/// envelope, its version or its peer is not served, which is what makes
/// "unauthorized requests were never handled" observable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Counters {
    /// Frames answered with a `result`.
    pub served: u64,
    /// Connections refused before any frame was parsed.
    pub rejected_peers: u64,
    /// Frames refused because their `api` field could not be negotiated.
    pub rejected_api: u64,
    /// Commands refused by the lifecycle table.
    pub illegal_transitions: u64,
    /// Probes that actually ran (any verdict).
    pub probes_run: u64,
}

impl Counters {
    /// The JSON projection used by `status` and `metrics`.
    pub fn to_json(&self) -> Value {
        json!({
            "served": self.served,
            "rejected_peers": self.rejected_peers,
            "rejected_api": self.rejected_api,
            "illegal_transitions": self.illegal_transitions,
            "probes_run": self.probes_run,
        })
    }
}

/// Where the control socket lives and what mode it was published with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SocketStatus {
    /// Absolute or caller-relative path of the socket file.
    pub path: String,
    /// Permission bits read back from the filesystem (for example `0o600`).
    pub mode: u32,
}

/// The `status` projection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Status {
    /// The API version this server speaks.
    pub api: ApiVersion,
    /// The instance this socket controls.
    pub instance: InstanceId,
    /// Current lifecycle state.
    pub lifecycle: LifecycleState,
    /// Loaded sides in ascending id order (the host's last committed set).
    pub loaded_sides: Vec<SideId>,
    /// Socket path and mode.
    pub socket: SocketStatus,
    /// Counters snapshot taken while building this value.
    pub counters: Counters,
}

impl Status {
    /// The JSON projection returned as the `status` result.
    pub fn to_json(&self) -> Value {
        let loaded_sides: Vec<&str> = self
            .loaded_sides
            .iter()
            .map(daggerfall_content_api::SideId::as_str)
            .collect();
        json!({
            "api": self.api.label(),
            "instance": self.instance.as_str(),
            "lifecycle": self.lifecycle.as_str(),
            "loaded_sides": loaded_sides,
            "socket": {
                "path": self.socket.path,
                "mode": self.socket.mode,
            },
            "counters": self.counters.to_json(),
        })
    }
}

/// The `metrics` projection: the server counters plus host-owned counters.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Metrics {
    /// Server counters snapshot.
    pub counters: Counters,
    /// Host-owned counters, merged verbatim.
    pub host: BTreeMap<String, u64>,
}

impl Metrics {
    /// The JSON projection returned as the `metrics` result.
    pub fn to_json(&self) -> Value {
        json!({
            "counters": self.counters.to_json(),
            "host": self.host,
        })
    }
}
