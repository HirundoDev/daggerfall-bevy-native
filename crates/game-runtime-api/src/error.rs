//! Stable errors of the runtime contract.
//!
//! Two layers, one vocabulary:
//!
//! - [`RuntimeError`] - setup and transport failures that happen before a frame
//!   exists (bind, peer credentials, host plumbing) or outside it (I/O).
//! - [`RpcError`] - the **closed wire error set**. [`ERROR_CODES`] enumerates it
//!   and [`RpcError::code`] is the only mapping from a variant to its stable
//!   lowercase code. The code is the first token of `Display`, so a client
//!   branches on values instead of parsing prose.
//!
//! Neither type ever carries a client-supplied value into an executable
//! position: the payloads are data for the caller, not for a shell.

use std::fmt;

use serde_json::{json, Value};

/// Every stable error code a runtime client can receive on the wire.
///
/// The list is closed and ordered; a client may treat anything else as
/// `internal_error`. Published in `GAME-RUNTIME-API-v1.md` with its numeric
/// JSON-RPC code.
pub const ERROR_CODES: &[&str] = &[
    "invalid_request",
    "method_not_found",
    "invalid_params",
    "internal_error",
    "unauthorized_peer",
    "unsupported_api",
    "illegal_transition",
    "probe_unknown",
    "invalid_probe_name",
    "host_error",
    "unknown_side",
    "frame_too_large",
];

/// Setup and transport failures of the runtime contract.
///
/// These never appear as a wire variant: when a client can observe the
/// consequence, the server projects them onto [`RpcError`] (for example a
/// failed host call becomes [`RpcError::HostError`]). They *are* the values an
/// embedder asserts on when it binds a server in-process.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuntimeError {
    /// The socket path already exists; one socket per instance (plan 3.3).
    SocketInUse {
        /// The path that was refused.
        path: String,
    },
    /// The resolved path does not fit `sun_path` (108 bytes including the NUL).
    SocketPathTooLong {
        /// Byte length of the resolved path.
        len: usize,
    },
    /// Raw text did not match the instance-id grammar.
    InvalidInstanceId {
        /// The offending raw text.
        raw: String,
        /// Static, stable reason.
        reason: &'static str,
    },
    /// The resolved socket path is unusable (for example abstract-socket syntax).
    InvalidSocketPath {
        /// The offending path.
        path: String,
        /// Static, stable reason.
        reason: &'static str,
    },
    /// The peer could not be identified or does not satisfy the policy.
    PeerCredentials {
        /// Human-readable reason; never used for control flow.
        reason: String,
    },
    /// Filesystem or socket I/O failed.
    Io {
        /// Human-readable reason; never used for control flow.
        reason: String,
    },
    /// A frame exceeded [`crate::MAX_FRAME`].
    FrameTooLarge {
        /// Observed byte length.
        len: usize,
        /// Largest accepted frame.
        max: usize,
    },
    /// The embedding host rejected a command or failed to answer.
    HostFailure {
        /// Human-readable reason; never used for control flow.
        reason: String,
    },
    /// The probe catalogue already has an entry with that name.
    DuplicateProbe {
        /// The repeated probe name.
        name: String,
    },
    /// A probe registration name violated the catalogue grammar.
    InvalidProbeName {
        /// The offending raw text.
        raw: String,
        /// Static, stable reason.
        reason: &'static str,
    },
}

impl fmt::Display for RuntimeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RuntimeError::SocketInUse { path } => write!(f, "socket_in_use: {path}"),
            RuntimeError::SocketPathTooLong { len } => {
                write!(f, "socket_path_too_long: {len} bytes")
            }
            RuntimeError::InvalidInstanceId { raw, reason } => {
                write!(f, "invalid_instance_id: {raw} ({reason})")
            }
            RuntimeError::InvalidSocketPath { path, reason } => {
                write!(f, "invalid_socket_path: {path} ({reason})")
            }
            RuntimeError::PeerCredentials { reason } => write!(f, "peer_credentials: {reason}"),
            RuntimeError::Io { reason } => write!(f, "io: {reason}"),
            RuntimeError::FrameTooLarge { len, max } => {
                write!(f, "frame_too_large: {len} bytes exceeds {max}")
            }
            RuntimeError::HostFailure { reason } => write!(f, "host_failure: {reason}"),
            RuntimeError::DuplicateProbe { name } => write!(f, "duplicate_probe: {name}"),
            RuntimeError::InvalidProbeName { raw, reason } => {
                write!(f, "invalid_probe_name: {raw} ({reason})")
            }
        }
    }
}

impl std::error::Error for RuntimeError {}

impl From<std::io::Error> for RuntimeError {
    fn from(error: std::io::Error) -> Self {
        RuntimeError::Io {
            reason: error.to_string(),
        }
    }
}

/// Every failure that can be serialized into a JSON-RPC error object.
///
/// The type is `Eq` so callers assert on exact values; the strings exist for
/// humans and for the JSON projection only.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RpcError {
    /// The envelope itself is malformed (not JSON, not an object, `jsonrpc`
    /// other than `"2.0"`, missing or non-scalar `id`, unknown top-level field).
    InvalidRequest {
        /// Human-readable reason; never used for control flow.
        detail: String,
    },
    /// The method name is not in the v1 table.
    MethodNotFound {
        /// The method as declared by the client.
        method: String,
    },
    /// A method's parameters are missing, extra or of the wrong type.
    InvalidParams {
        /// Human-readable reason; never used for control flow.
        detail: String,
    },
    /// A failure the contract cannot attribute to the client.
    InternalError {
        /// Human-readable reason; never used for control flow.
        detail: String,
    },
    /// The kernel-attested peer uid does not satisfy the server policy.
    UnauthorizedPeer {
        /// Human-readable reason; never used for control flow.
        detail: String,
    },
    /// The declared API version cannot be served by this build.
    UnsupportedApi {
        /// What the client declared (empty when the field is missing).
        declared: String,
        /// What this build publishes.
        supported: String,
    },
    /// The command is not legal from the current lifecycle state.
    IllegalTransition {
        /// The current state, lowercase.
        from: &'static str,
        /// The refused command, as published on the wire.
        command: &'static str,
    },
    /// The probe name is well-formed but not registered.
    ProbeUnknown {
        /// The requested probe name.
        name: String,
    },
    /// The probe name violates the catalogue grammar.
    InvalidProbeName {
        /// The offending raw text.
        raw: String,
        /// Static, stable reason.
        reason: &'static str,
    },
    /// The embedding host rejected a command or could not answer.
    HostError {
        /// Human-readable reason; never used for control flow.
        detail: String,
    },
    /// The side is not currently loaded, so it cannot be unloaded.
    UnknownSide {
        /// The requested side id.
        side: String,
    },
    /// The frame exceeded [`crate::MAX_FRAME`]; the connection is closed.
    FrameTooLarge {
        /// Observed byte length.
        len: usize,
        /// Largest accepted frame.
        max: usize,
    },
}

impl RpcError {
    /// The stable lowercase code of this failure (first token of `Display`).
    pub fn code(&self) -> &'static str {
        match self {
            RpcError::InvalidRequest { .. } => "invalid_request",
            RpcError::MethodNotFound { .. } => "method_not_found",
            RpcError::InvalidParams { .. } => "invalid_params",
            RpcError::InternalError { .. } => "internal_error",
            RpcError::UnauthorizedPeer { .. } => "unauthorized_peer",
            RpcError::UnsupportedApi { .. } => "unsupported_api",
            RpcError::IllegalTransition { .. } => "illegal_transition",
            RpcError::ProbeUnknown { .. } => "probe_unknown",
            RpcError::InvalidProbeName { .. } => "invalid_probe_name",
            RpcError::HostError { .. } => "host_error",
            RpcError::UnknownSide { .. } => "unknown_side",
            RpcError::FrameTooLarge { .. } => "frame_too_large",
        }
    }

    /// The JSON-RPC numeric code published for this failure.
    pub fn numeric(&self) -> i64 {
        match self {
            RpcError::InvalidRequest { .. } => -32600,
            RpcError::MethodNotFound { .. } => -32601,
            RpcError::InvalidParams { .. } => -32602,
            RpcError::InternalError { .. } => -32603,
            RpcError::UnauthorizedPeer { .. } => -32010,
            RpcError::UnsupportedApi { .. } => -32011,
            RpcError::IllegalTransition { .. } => -32012,
            RpcError::ProbeUnknown { .. } => -32013,
            RpcError::InvalidProbeName { .. } => -32014,
            RpcError::HostError { .. } => -32015,
            RpcError::UnknownSide { .. } => -32016,
            RpcError::FrameTooLarge { .. } => -32017,
        }
    }

    /// The `error.data` object: the stable code plus the variant's fields.
    ///
    /// `data.code` always repeats the stable code so a client can branch on
    /// `data` alone, without matching the numeric JSON-RPC code.
    pub fn data(&self) -> Value {
        let code = self.code();
        match self {
            RpcError::UnsupportedApi {
                declared,
                supported,
            } => json!({
                "code": code,
                "declared": declared,
                "supported": supported,
            }),
            RpcError::IllegalTransition { from, command } => json!({
                "code": code,
                "from": from,
                "command": command,
            }),
            RpcError::ProbeUnknown { name } => json!({"code": code, "name": name}),
            RpcError::InvalidProbeName { raw, reason } => {
                json!({"code": code, "raw": raw, "reason": reason})
            }
            RpcError::UnknownSide { side } => json!({"code": code, "side": side}),
            RpcError::FrameTooLarge { len, max } => {
                json!({"code": code, "len": len, "max": max})
            }
            RpcError::MethodNotFound { method } => json!({"code": code, "method": method}),
            RpcError::InvalidRequest { .. }
            | RpcError::InvalidParams { .. }
            | RpcError::InternalError { .. }
            | RpcError::UnauthorizedPeer { .. }
            | RpcError::HostError { .. } => json!({"code": code}),
        }
    }
}

impl fmt::Display for RpcError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RpcError::InvalidRequest { detail } => write!(f, "invalid_request: {detail}"),
            RpcError::MethodNotFound { method } => write!(f, "method_not_found: {method}"),
            RpcError::InvalidParams { detail } => write!(f, "invalid_params: {detail}"),
            RpcError::InternalError { detail } => write!(f, "internal_error: {detail}"),
            RpcError::UnauthorizedPeer { detail } => write!(f, "unauthorized_peer: {detail}"),
            RpcError::UnsupportedApi {
                declared,
                supported,
            } => {
                let declared = if declared.is_empty() {
                    "(missing)"
                } else {
                    declared.as_str()
                };
                write!(
                    f,
                    "unsupported_api: declared {declared}, supported {supported}"
                )
            }
            RpcError::IllegalTransition { from, command } => {
                write!(f, "illegal_transition: {command} from {from}")
            }
            RpcError::ProbeUnknown { name } => write!(f, "probe_unknown: {name}"),
            RpcError::InvalidProbeName { raw, reason } => {
                write!(f, "invalid_probe_name: {raw} ({reason})")
            }
            RpcError::HostError { detail } => write!(f, "host_error: {detail}"),
            RpcError::UnknownSide { side } => write!(f, "unknown_side: {side}"),
            RpcError::FrameTooLarge { len, max } => {
                write!(f, "frame_too_large: {len} bytes exceeds {max}")
            }
        }
    }
}

impl std::error::Error for RpcError {}
