//! The UDS server: bind, authenticate, frame, dispatch.
//!
//! [`Server`] is the pure half (bind + `handle_authenticated_request`), which an
//! embedder that already authenticated its peer can drive in-process.
//! [`RuntimeServer`] adds the accept loop: one nonblocking listener polled every
//! ~2 ms and one thread per accepted connection.
//!
//! The accept order is fixed and observable:
//!
//! 1. `accept`,
//! 2. [`peer::authorize`] against the configured uid - on mismatch the server
//!    writes the `unauthorized_peer` error (id `null`) and closes **without
//!    reading or parsing the request**, incrementing `rejected_peers`,
//! 3. only then read frames and dispatch.
//!
//! Mutable core state (lifecycle, loaded sides, event log) is behind one
//! `Mutex`; counters are atomics. Host hooks run while that lock is held, so a
//! [`RuntimeHost`] implementation must be reentrancy-free and fast.

use std::collections::{BTreeMap, VecDeque};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use daggerfall_content_api::SideId;
use serde_json::{json, Map, Value};

use crate::error::{RpcError, RuntimeError};
use crate::host::RuntimeHost;
use crate::lifecycle::{self, ControlCommand, LifecycleState};
use crate::peer::{self, PeerPolicy};
use crate::probe::{ProbeName, ProbeRegistry};
use crate::protocol::{self, methods, Request, MAX_FRAME};
use crate::socket::{InstanceId, RuntimeSocket, SocketPath};
use crate::status::{Counters, Metrics, SocketStatus, Status};

/// Maximum number of events kept in memory by the `events` log.
pub const EVENT_CAPACITY: usize = 256;

/// How long the accept loop sleeps between polls of the nonblocking listener.
const ACCEPT_POLL: Duration = Duration::from_millis(2);

/// Everything [`Server::bind`] needs.
///
/// The host and the probe catalogue are shared (`Arc`) because the embedding
/// binary keeps using them after the server starts; the expected uid is the
/// single acceptance rule of v1.
pub struct ServerConfig {
    /// The validated socket path to bind.
    pub socket: SocketPath,
    /// The instance this server controls.
    pub instance: InstanceId,
    /// The only uid allowed to connect (normally `getuid()` of the game).
    pub expected_uid: u32,
    /// The embedding game.
    pub host: Arc<dyn RuntimeHost>,
    /// The diagnostic catalogue exposed by `probes.list` / `probes.run`.
    pub probes: Arc<ProbeRegistry>,
}

/// The bound server, before the accept loop starts.
pub struct Server {
    socket: RuntimeSocket,
    instance: InstanceId,
    expected_uid: u32,
    host: Arc<dyn RuntimeHost>,
    probes: Arc<ProbeRegistry>,
    core: Mutex<CoreState>,
    counters: AtomicCounters,
}

impl Server {
    /// Binds the socket (mode `0600`) and creates a `Stopped` instance with no
    /// loaded sides and an empty event log.
    pub fn bind(config: ServerConfig) -> Result<Server, RuntimeError> {
        let socket = RuntimeSocket::bind(config.socket)?;
        Ok(Server {
            socket,
            instance: config.instance,
            expected_uid: config.expected_uid,
            host: config.host,
            probes: config.probes,
            core: Mutex::new(CoreState::new()),
            counters: AtomicCounters::default(),
        })
    }

    /// The socket path this server owns.
    pub fn path(&self) -> &Path {
        self.socket.path()
    }

    /// A `status` projection of the current state.
    pub fn status(&self) -> Status {
        let core = self.core();
        Status {
            api: crate::api::ApiVersion::CURRENT,
            instance: self.instance.clone(),
            lifecycle: core.lifecycle,
            loaded_sides: core.loaded.values().cloned().collect(),
            socket: SocketStatus {
                path: self.socket.path().display().to_string(),
                mode: self.socket.mode(),
            },
            counters: self.counters.snapshot(),
        }
    }

    /// Handles one frame for a peer the caller has already authenticated.
    ///
    /// This is the pure, testable core of the wire contract: it never touches
    /// the socket and never inspects credentials, so it is only for embedders
    /// that authenticated out of band. The returned string is one response
    /// object without a trailing newline.
    pub fn handle_authenticated_request(&self, line: &str) -> String {
        if line.len() > MAX_FRAME {
            return protocol::encode_error(
                None,
                &RpcError::FrameTooLarge {
                    len: line.len(),
                    max: MAX_FRAME,
                },
            );
        }
        match protocol::parse_request(line) {
            Err(rejected) => {
                self.note_error(&rejected.error);
                protocol::encode_error(rejected.id.as_ref(), &rejected.error)
            }
            Ok(request) => {
                let id = request.id.clone();
                match self.dispatch(request) {
                    Ok(result) => {
                        self.counters.served.fetch_add(1, Ordering::Relaxed);
                        protocol::encode_success(&id, &result)
                    }
                    Err(error) => {
                        self.note_error(&error);
                        protocol::encode_error(Some(&id), &error)
                    }
                }
            }
        }
    }

    /// Starts the accept loop on a background thread.
    ///
    /// The listener is already nonblocking and is polled every 2 ms.
    /// Spawning the accept thread is the one operation that cannot be returned
    /// as an error without changing the ownership contract, so a spawn failure
    /// aborts: it means the host cannot create threads at all.
    pub fn start(self) -> RuntimeServer {
        let path = self.socket.path().to_path_buf();
        let stop = Arc::new(AtomicBool::new(false));
        let server = Arc::new(self);
        let accept_server = Arc::clone(&server);
        let accept_stop = Arc::clone(&stop);
        let handle = thread::Builder::new()
            .name("dfr-runtime-accept".to_string())
            .spawn(move || accept_server.accept_loop(accept_stop))
            .expect("the accept thread must be spawnable");
        RuntimeServer {
            server,
            stop,
            handle: Some(handle),
            path,
        }
    }

    fn core(&self) -> MutexGuard<'_, CoreState> {
        self.core
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn note_error(&self, error: &RpcError) {
        match error {
            RpcError::UnsupportedApi { .. } => {
                self.counters.rejected_api.fetch_add(1, Ordering::Relaxed);
            }
            RpcError::IllegalTransition { .. } => {
                self.counters
                    .illegal_transitions
                    .fetch_add(1, Ordering::Relaxed);
            }
            _ => {}
        }
    }

    fn record_event(&self, kind: &'static str) {
        self.core().events.push(kind);
    }

    fn dispatch(&self, request: Request) -> Result<Value, RpcError> {
        let params = &request.params;
        match request.method.as_str() {
            methods::STATUS => {
                no_params(params)?;
                Ok(self.status().to_json())
            }
            methods::METRICS => {
                no_params(params)?;
                let host = self
                    .host
                    .metrics()
                    .map_err(|reason| RpcError::HostError { detail: reason })?;
                Ok(Metrics {
                    counters: self.counters.snapshot(),
                    host,
                }
                .to_json())
            }
            methods::START => self.lifecycle_command(ControlCommand::Start),
            methods::STOP => self.lifecycle_command(ControlCommand::Stop),
            methods::PAUSE => self.lifecycle_command(ControlCommand::Pause),
            methods::RESUME => self.lifecycle_command(ControlCommand::Resume),
            methods::LOAD_SIDE => {
                let side = parse_side(params)?;
                self.side_command(ControlCommand::LoadSide(side))
            }
            methods::UNLOAD_SIDE => {
                let side = parse_side(params)?;
                self.side_command(ControlCommand::UnloadSide(side))
            }
            methods::PROBES_LIST => {
                no_params(params)?;
                let specs = self.probes.list();
                let probes: Vec<Value> = specs
                    .iter()
                    .map(|spec| {
                        json!({
                            "name": spec.name.as_str(),
                            "description": spec.description,
                        })
                    })
                    .collect();
                Ok(json!({"probes": probes, "count": specs.len()}))
            }
            methods::PROBES_RUN => self.run_probe(params),
            methods::EVENTS => self.events(params),
            methods::SCREENSHOT => {
                no_params(params)?;
                self.screenshot()
            }
            other => Err(RpcError::MethodNotFound {
                method: other.to_string(),
            }),
        }
    }

    fn lifecycle_command(&self, command: ControlCommand) -> Result<Value, RpcError> {
        let state = self.apply_control(&command)?;
        Ok(json!({"lifecycle": state.as_str()}))
    }

    fn side_command(&self, command: ControlCommand) -> Result<Value, RpcError> {
        {
            let core = self.core();
            // State legality wins over membership: `load-side` from `stopped`
            // is an illegal transition, not a host question.
            lifecycle::transition(core.lifecycle, &command)?;
            if let ControlCommand::UnloadSide(side) = &command {
                if !core.loaded.contains_key(side.as_str()) {
                    return Err(RpcError::UnknownSide {
                        side: side.as_str().to_string(),
                    });
                }
            }
        }
        self.apply_control(&command)?;
        let core = self.core();
        let loaded: Vec<&str> = core.loaded.values().map(SideId::as_str).collect();
        Ok(json!({"loaded_sides": loaded}))
    }

    /// Validates the transition, applies it on the host and commits the host's
    /// answer. On a host error nothing is committed.
    fn apply_control(&self, command: &ControlCommand) -> Result<LifecycleState, RpcError> {
        let mut core = self.core();
        let next = lifecycle::transition(core.lifecycle, command)?;
        let sides = self
            .host
            .apply(command)
            .map_err(|reason| RpcError::HostError { detail: reason })?;
        core.loaded = sides
            .into_iter()
            .map(|side| (side.as_str().to_string(), side))
            .collect();
        core.lifecycle = next;
        let kind = match command {
            ControlCommand::LoadSide(_) | ControlCommand::UnloadSide(_) => "control",
            ControlCommand::Start
            | ControlCommand::Stop
            | ControlCommand::Pause
            | ControlCommand::Resume => "lifecycle",
        };
        core.events.push(kind);
        Ok(next)
    }

    fn run_probe(&self, params: &Map<String, Value>) -> Result<Value, RpcError> {
        let value = single_param(params, "name")?;
        let raw = value.as_str().ok_or_else(|| RpcError::InvalidParams {
            detail: "`name` must be a string".to_string(),
        })?;
        let name = ProbeName::parse(raw)?;
        let outcome = self.probes.run(name.as_str())?;
        self.counters.probes_run.fetch_add(1, Ordering::Relaxed);
        self.record_event("probe");
        Ok(json!({
            "name": name.as_str(),
            "outcome": outcome.status.as_str(),
            "detail": outcome.detail,
        }))
    }

    fn events(&self, params: &Map<String, Value>) -> Result<Value, RpcError> {
        if let Some(unknown) = params.keys().find(|key| key.as_str() != "after") {
            return Err(RpcError::InvalidParams {
                detail: format!("unknown parameter `{unknown}`"),
            });
        }
        let after = match params.get("after") {
            None => 0,
            Some(value) => value.as_u64().ok_or_else(|| RpcError::InvalidParams {
                detail: "`after` must be a non-negative integer".to_string(),
            })?,
        };
        let core = self.core();
        Ok(core.events.project(after))
    }

    fn screenshot(&self) -> Result<Value, RpcError> {
        let shot = self
            .host
            .screenshot()
            .map_err(|reason| RpcError::HostError { detail: reason })?;
        match shot {
            Some(shot) => Ok(json!({
                "screenshot": {
                    "width": shot.width,
                    "height": shot.height,
                    "format": shot.format,
                    "sha256": shot.sha256,
                }
            })),
            None => Ok(json!({"screenshot": null})),
        }
    }

    fn accept_loop(self: Arc<Self>, stop: Arc<AtomicBool>) {
        let listener: &UnixListener = self.socket.listener();
        while !stop.load(Ordering::Acquire) {
            match listener.accept() {
                Ok((stream, _address)) => {
                    let server = Arc::clone(&self);
                    // One thread per connection: the frames are independent and
                    // no connection may stall the accept loop.
                    let _ = thread::Builder::new()
                        .name("dfr-runtime-conn".to_string())
                        .spawn(move || server.serve_connection(stream));
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(ACCEPT_POLL);
                }
                Err(_) => break,
            }
        }
    }

    fn serve_connection(self: &Arc<Self>, stream: UnixStream) {
        // Fixed order: authenticate before reading a single byte.
        if peer::authorize(&stream, &PeerPolicy::OnlyUid(self.expected_uid)).is_err() {
            self.counters.rejected_peers.fetch_add(1, Ordering::Relaxed);
            let error = RpcError::UnauthorizedPeer {
                detail: format!(
                    "peer uid is not authorized (expected {})",
                    self.expected_uid
                ),
            };
            let _ = write_line(&stream, &protocol::encode_error(None, &error));
            return;
        }

        let read_stream = match stream.try_clone() {
            Ok(clone) => clone,
            Err(_) => return,
        };
        let mut reader = BufReader::new(read_stream);
        loop {
            match read_frame(&mut reader) {
                Ok(Some(line)) => {
                    let response = self.handle_authenticated_request(&line);
                    if write_line(&stream, &response).is_err() {
                        break;
                    }
                }
                Ok(None) => break,
                Err(FrameFailure::TooLarge(len)) => {
                    let error = RpcError::FrameTooLarge {
                        len,
                        max: MAX_FRAME,
                    };
                    let _ = write_line(&stream, &protocol::encode_error(None, &error));
                    break;
                }
                Err(FrameFailure::Io) => break,
            }
        }
    }
}

/// The running server: accept thread handle plus a shutdown flag.
///
/// `drop` stops and joins the accept thread, and because the socket guard lives
/// in the [`Server`] that the thread owns, the filesystem entry is unlinked when
/// the last reference drops. Calling [`RuntimeServer::shutdown`] followed by
/// [`RuntimeServer::join`] is the explicit form of the same thing.
pub struct RuntimeServer {
    server: Arc<Server>,
    stop: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
    path: PathBuf,
}

impl RuntimeServer {
    /// The socket path this server owns.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// A `status` projection, including the live counters.
    pub fn status(&self) -> Status {
        self.server.status()
    }

    /// Asks the accept loop to stop. Idempotent and non-blocking.
    pub fn shutdown(&self) {
        self.stop.store(true, Ordering::Release);
    }

    /// Stops and joins the accept thread.
    pub fn join(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

impl Drop for RuntimeServer {
    fn drop(&mut self) {
        self.join();
    }
}

enum FrameFailure {
    TooLarge(usize),
    Io,
}

/// Reads one newline-terminated frame, bounded by [`MAX_FRAME`].
///
/// A frame longer than the limit is reported as soon as the crossing is
/// observed; the caller answers `frame_too_large` and closes the connection, so
/// no later byte can be reinterpreted as a new frame (no desync).
fn read_frame(reader: &mut BufReader<UnixStream>) -> Result<Option<String>, FrameFailure> {
    let mut buffer: Vec<u8> = Vec::new();
    loop {
        let chunk_len;
        let mut newline_at = None;
        {
            let available = reader.fill_buf().map_err(|_| FrameFailure::Io)?;
            if available.is_empty() {
                return if buffer.is_empty() {
                    Ok(None)
                } else {
                    Ok(Some(String::from_utf8_lossy(&buffer).into_owned()))
                };
            }
            chunk_len = available.len();
            match available.iter().position(|&byte| byte == b'\n') {
                Some(position) => {
                    newline_at = Some(position);
                    buffer.extend_from_slice(&available[..position]);
                }
                None => buffer.extend_from_slice(available),
            }
        }
        match newline_at {
            Some(position) => {
                reader.consume(position + 1);
                if buffer.len() > MAX_FRAME {
                    return Err(FrameFailure::TooLarge(buffer.len()));
                }
                return Ok(Some(String::from_utf8_lossy(&buffer).into_owned()));
            }
            None => {
                reader.consume(chunk_len);
                if buffer.len() > MAX_FRAME {
                    return Err(FrameFailure::TooLarge(buffer.len()));
                }
            }
        }
    }
}

fn write_line(stream: &UnixStream, line: &str) -> std::io::Result<()> {
    let mut writer = stream;
    writer.write_all(line.as_bytes())?;
    writer.write_all(b"\n")?;
    writer.flush()
}

fn no_params(params: &Map<String, Value>) -> Result<(), RpcError> {
    if let Some(unknown) = params.keys().next() {
        return Err(RpcError::InvalidParams {
            detail: format!("this method takes no parameters, got `{unknown}`"),
        });
    }
    Ok(())
}

fn single_param<'a>(
    params: &'a Map<String, Value>,
    key: &str,
) -> Result<&'a Value, RpcError> {
    if let Some(unknown) = params.keys().find(|candidate| candidate.as_str() != key) {
        return Err(RpcError::InvalidParams {
            detail: format!("unknown parameter `{unknown}`"),
        });
    }
    params.get(key).ok_or_else(|| RpcError::InvalidParams {
        detail: format!("missing parameter `{key}`"),
    })
}

fn parse_side(params: &Map<String, Value>) -> Result<SideId, RpcError> {
    let value = single_param(params, "side")?;
    let raw = value.as_str().ok_or_else(|| RpcError::InvalidParams {
        detail: "`side` must be a string".to_string(),
    })?;
    SideId::parse(raw).map_err(|error| RpcError::InvalidParams {
        detail: error.to_string(),
    })
}

#[derive(Default)]
struct AtomicCounters {
    served: AtomicU64,
    rejected_peers: AtomicU64,
    rejected_api: AtomicU64,
    illegal_transitions: AtomicU64,
    probes_run: AtomicU64,
}

impl AtomicCounters {
    fn snapshot(&self) -> Counters {
        Counters {
            served: self.served.load(Ordering::Relaxed),
            rejected_peers: self.rejected_peers.load(Ordering::Relaxed),
            rejected_api: self.rejected_api.load(Ordering::Relaxed),
            illegal_transitions: self.illegal_transitions.load(Ordering::Relaxed),
            probes_run: self.probes_run.load(Ordering::Relaxed),
        }
    }
}

struct CoreState {
    lifecycle: LifecycleState,
    loaded: BTreeMap<String, SideId>,
    events: EventLog,
}

impl CoreState {
    fn new() -> Self {
        CoreState {
            lifecycle: LifecycleState::Stopped,
            loaded: BTreeMap::new(),
            events: EventLog::new(),
        }
    }
}

struct Event {
    seq: u64,
    kind: &'static str,
}

struct EventLog {
    next_seq: u64,
    entries: VecDeque<Event>,
    dropped: u64,
}

impl EventLog {
    fn new() -> Self {
        EventLog {
            next_seq: 1,
            entries: VecDeque::new(),
            dropped: 0,
        }
    }

    fn push(&mut self, kind: &'static str) {
        let seq = self.next_seq;
        self.next_seq += 1;
        if self.entries.len() == EVENT_CAPACITY {
            self.entries.pop_front();
            self.dropped += 1;
        }
        self.entries.push_back(Event { seq, kind });
    }

    fn project(&self, after: u64) -> Value {
        let events: Vec<Value> = self
            .entries
            .iter()
            .filter(|event| event.seq > after)
            .map(|event| json!({"seq": event.seq, "kind": event.kind}))
            .collect();
        json!({
            "events": events,
            "next": self.next_seq,
            "dropped": self.dropped,
        })
    }
}
