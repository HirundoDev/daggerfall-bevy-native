//! # daggerfall-game-runtime-api - the runtime control contract (I1 - F3)
//!
//! Owner: **main**. Medium: **Unix domain socket** (no network). Trust model:
//! kernel peer credentials plus filesystem permissions (plan
//! `I1-INFRA-1-TRUST-DUAL-SIDE-PROJECTS.md` 3.3, 3.5; decision OD-6 = (a)
//! partial: BRP's request/response *form*, this crate's own contract).
//!
//! API version: **1.0** ([`API_LABEL`]). The normative published form lives in
//! `docs/sintergic/game-runtime-api/GAME-RUNTIME-API-v1.md`;
//! `tests/surface_published.rs` fails if it drifts from this crate.
//!
//! ## Why this crate exists
//!
//! Tooling, previews and QA agents need to observe and control a *running*
//! instance: ask which lifecycle state it is in, start/pause/stop it, load or
//! unload a side, and run the deterministic probes the main already ships.
//! TCP would expose process control to the network, so the transport is a Unix
//! domain socket whose filesystem permissions *are* the capability: a `0600`
//! socket inside a workspace directory is reachable only by the user that owns
//! it, and `SO_PEERCRED` lets the server re-check the peer uid **in the kernel**
//! before it parses a single byte of the request.
//!
//! Bevy's BRP (`bevy_remote`) covers observation/reflection only: HTTP on a
//! fixed port, no authentication, no lifecycle. This crate keeps BRP's *form*
//! (a JSON-RPC 2.0 request/response envelope) but not its transport or its
//! authority: the contract is ours, it lives on UDS, and it owns control and
//! lifecycle (OD-6 = (a) partial).
//!
//! ## Surface
//!
//! | Item | Contract role |
//! |:---|:---|
//! | [`Server`] / [`RuntimeServer`] | bind the socket, dispatch frames, own the accept loop |
//! | [`ServerConfig`] | socket path, instance id, expected uid, host, probe catalogue |
//! | [`protocol`] | newline-delimited JSON-RPC 2.0 envelope, [`MAX_FRAME`], method table |
//! | [`LifecycleState`] / [`ControlCommand`] / [`transition`] | the transition table |
//! | [`probe::ProbeRegistry`] | name-only diagnostic catalogue (no paths, no commands) |
//! | [`PeerPolicy`] / [`peer::authorize`] | kernel-attested client identity |
//! | [`socket::RuntimeSocket`] / [`SocketPath`] / [`InstanceId`] | one `0600` socket per instance |
//! | [`RuntimeHost`] | the embedding game: apply control, capture frames, publish metrics |
//! | [`Status`] / [`Metrics`] / [`Counters`] | observation projections |
//! | [`ApiVersion`] / [`ApiVersion::LABEL`] | wire negotiation (`1.0`) |
//! | [`RpcError`] / [`ERROR_CODES`] | the closed wire failure set |
//! | [`RuntimeError`] | setup and transport failures (never a wire variant) |
//!
//! ## Invariants (enforced by types, constructors or the transition table)
//!
//! 1. **The peer uid is kernel-attested.** It arrives through `SO_PEERCRED`; no
//!    request field can carry or override it.
//! 2. **One socket per instance.** A second bind of the same
//!    `(directory, instance)` pair fails with [`RuntimeError::SocketInUse`]
//!    instead of silently sharing control of an instance.
//! 3. **The socket is user-only.** `RuntimeSocket::bind` publishes exactly
//!    `0600` and the [`RuntimeServer`] never widens it.
//! 4. **`stop` is total and idempotent.** It succeeds from every state and
//!    leaves the state `Stopped`, so a failed stop cannot leave a zombie.
//! 5. **Probes are named, never executed by path.** `probes.run` accepts a name
//!    from the registered catalogue; the wire type system has no `command`,
//!    `path` or `argv` parameter.
//! 6. **Version negotiation precedes all work.** A frame that declares an
//!    unsupported `api` receives a typed [`RpcError::UnsupportedApi`] before any
//!    method is looked up, so an old client never gets different behaviour.
//! 7. **Nothing is served before authorization.** A rejected peer receives
//!    `unauthorized_peer` and the request is never parsed or counted as served.
//!
//! ## What this crate does NOT own
//!
//! - **No game and no engine.** 0 bevy, 0 ECS, 0 windowing. Control is applied
//!   through the [`RuntimeHost`] trait, which the embedding binary implements.
//! - **No async runtime.** 0 tokio: one nonblocking accept loop on a plain
//!   thread plus one thread per accepted connection.
//! - **No network.** 0 TCP, 0 UDP, 0 HTTP: a local Unix domain socket is the
//!   only transport, and opening it requires a filesystem path the caller
//!   already owns.
//! - **No frame capture.** [`RuntimeHost::screenshot`] is host-owned; this crate
//!   only projects the metadata it is given.
//! - **No server push.** `events` is a pull-with-cursor log, not a stream.
//! - **No plugin loading by path.** `load-side` takes a validated
//!   [`daggerfall_content_api::SideId`], never a filesystem path.
//! - **No promotion gate and no ledger.** Those belong to F4.
//!
//! ## Conformance
//!
//! `tests/f3_runtime_acceptance.rs` covers every criterion of plan 9 F3 plus the
//! framing, lifecycle and probe invariants above; `tests/surface_published.rs`
//! is the anti-drift guard for the normative document and for the "0 TCP"
//! structural claim.
//!
//! ## MSRV and semver
//!
//! `rust-version = "1.97.1"` (workspace MSRV). Below 1.0, any change to a
//! method name, to a published [`ERROR_CODES`] entry, to a numeric JSON-RPC
//! code, to the frame limit or to the lifecycle table is a **breaking** release,
//! because clients are external processes.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod api;
pub mod error;
pub mod host;
pub mod lifecycle;
pub mod peer;
pub mod probe;
pub mod protocol;
pub mod server;
pub mod socket;
pub mod status;

pub use api::{ApiVersion, API_LABEL};
pub use error::{RpcError, RuntimeError, ERROR_CODES};
pub use host::{RuntimeHost, Screenshot};
pub use lifecycle::{transition, ControlCommand, LifecycleState};
pub use peer::{authorize, credentials, PeerInfo, PeerPolicy};
pub use probe::{ProbeFn, ProbeName, ProbeOutcome, ProbeRegistry, ProbeSpec, ProbeStatus};
pub use protocol::{methods, Request, RejectedRequest, JSONRPC_VERSION, MAX_FRAME};
pub use server::{RuntimeServer, Server, ServerConfig};
pub use socket::{InstanceId, RuntimeSocket, SocketPath, MAX_SOCKET_PATH_BYTES};
pub use status::{Counters, Metrics, SocketStatus, Status};
