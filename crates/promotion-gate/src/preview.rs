//! Gate step 6a — the preview actually composes (plan
//! `I1-INFRA-1-TRUST-DUAL-SIDE-PROJECTS.md` §4.3 step 6, F4-B).
//!
//! What this module proves, and what it does not:
//!
//! * It proves that the side's **declared preview program** can be started as a
//!   separate process (argv, never a shell), that it publishes a Unix domain
//!   socket, and that the process holding that socket answers the **v1 `status`**
//!   projection of [`daggerfall_game_runtime_api`] for *this* instance with the
//!   side loaded and a `0600` socket.
//! * It does **not** prove the side's ECS composition: whether the plugin adds
//!   systems to `GameCorePlugin` is step-1/§4.3 (e) evidence, owned by the side's
//!   report. The gate never links `game-core`, so it cannot be fooled into
//!   "verifying" composition with the same code it is checking.
//!
//! The wire form is validated here as an **external client**: the gate does not
//! depend on the runtime crate's server types, so a regression inside the
//! runtime cannot validate itself.
//!
//! Cleanup is unconditional: the preview is killed and its socket directory is
//! removed on every path (success, typed failure, panic in the middle), so a run
//! cannot leave a zombie instance or a stale socket behind.
//!
//! ## The handshake is a bounded poll, not a single shot
//!
//! Publishing the socket and composing the side happen at **different
//! moments**: the contract lets a preview `bind` first and call
//! `start`/`load-side` afterwards, so a `status` read inside that window
//! answers truthfully with `lifecycle="stopped"` and `loaded_sides=[]`. That
//! projection is *progress*, not a verdict — reading it once and failing turns
//! a valid composition into a false `FAIL` and, worse, masks step 6b's
//! attribution (`main_baseline_failed` reported as
//! `runtime_status_unavailable`). This module therefore re-reads a composing
//! projection inside the `preview.timeout_secs` budget and only then decides:
//! a preview that answered but never composed is `runtime_status_unavailable`,
//! a preview that never answered is `preview_timeout`. A projection that
//! violates the **static** contract (`api`, `instance`, socket path/mode,
//! framing) is still rejected at once: waiting cannot change those fields, and
//! pretending otherwise would only delay a definite failure.

use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use serde_json::Value;

use crate::manifest::Preview;
use crate::probes::sha256_hex;
use crate::taxonomy::Cause;
use crate::verdict::PreviewEvidence;

/// API label the gate negotiates with the preview (`status.api`).
pub const STATUS_API: &str = "1.0";

/// Lifecycle the preview must report once composed.
pub const COMPOSED_LIFECYCLE: &str = "running";

/// Mode the socket must be published with (`0o600`, user only).
pub const REQUIRED_SOCKET_MODE: u32 = 0o600;

/// `MAX_SOCKET_PATH_BYTES` of the control contract.
const MAX_SOCKET_PATH_BYTES: usize = 100;

/// Poll interval while waiting for the socket.
const POLL: Duration = Duration::from_millis(5);

/// Spacing between two reads of a **composing** `status` (see [`Handshake`]).
///
/// Publishing the socket and composing the side are two different moments —
/// `bind` is legal before `start`/`load-side` — so a not-yet-composed
/// projection is progress, and it is re-read at this pace instead of hammering
/// the socket with a busy-loop.
const COMPOSING_POLL: Duration = Duration::from_millis(20);

/// Ceiling for one handshake attempt (connect + write + read).
///
/// A stalled answer is a `Retry`, never a verdict: the verdict belongs to the
/// `preview.timeout_secs` deadline, so one slow read cannot consume it whole.
const HANDSHAKE_READ: Duration = Duration::from_millis(500);

/// How much of the preview's stdout/stderr is kept for diagnostics.
const PIPE_CAP: usize = 64 * 1024;

static SEQ: AtomicU64 = AtomicU64::new(0);

/// A typed step-6a failure: the cause plus the one-line detail for the row.
#[derive(Debug, Clone)]
pub struct PreviewFailure {
    pub cause: Cause,
    pub detail: String,
}

impl PreviewFailure {
    fn new(cause: Cause, detail: impl Into<String>) -> Self {
        PreviewFailure {
            cause,
            detail: detail.into(),
        }
    }
}

/// Launch the side's preview and validate its `status` within `preview.timeout_secs`.
///
/// `side_id` is the id the manifest claims; the preview must report it as
/// loaded. Nothing else about the side is trusted.
pub fn run(
    preview: &Preview,
    side_root: &Path,
    side_id: &str,
) -> Result<PreviewEvidence, PreviewFailure> {
    let started = Instant::now();
    let dir = socket_dir()?;
    let socket = dir.join(format!("dfr-{}.sock", preview.instance));
    let socket_text = socket.display().to_string();
    if socket_text.len() > MAX_SOCKET_PATH_BYTES || socket_text.contains('@') {
        return Err(PreviewFailure::new(
            Cause::PreviewFailed,
            format!("socket del preview no es válido para el contrato ({socket_text:?})"),
        ));
    }

    let argv: Vec<String> = preview
        .argv
        .iter()
        .map(|arg| {
            arg.replace("{side}", &side_root.display().to_string())
                .replace("{socket}", &socket_text)
                .replace("{instance}", &preview.instance)
        })
        .collect();
    let program = resolve_program(&argv[0], side_root);
    let mut spawned = Spawned::spawn(&program, &argv[1..], side_root, &dir)?;

    let deadline = started + Duration::from_secs(preview.timeout_secs);
    // Last `status` that answered for *this* instance without showing
    // composition. Kept for the terminal detail: at the deadline, a preview
    // that answered but never composed is `runtime_status_unavailable` (it
    // failed to compose), not `preview_timeout` (it never spoke).
    let mut composing: Option<String> = None;
    loop {
        if let Some(status) = spawned.exited() {
            let tail = spawned.stderr_tail();
            return Err(PreviewFailure::new(
                Cause::PreviewFailed,
                format!(
                    "el preview terminó antes de publicar un status válido ({status}){tail}"
                ),
            ));
        }
        if socket.exists() {
            match handshake(&socket, &preview.instance, side_id, deadline) {
                Err(Handshake::Retry) => {}
                Err(Handshake::Composing(detail)) => {
                    // The socket answered for this instance: the composition is
                    // still in flight. Re-read (paced) until the deadline.
                    composing = Some(detail);
                    std::thread::sleep(COMPOSING_POLL);
                    continue;
                }
                Err(Handshake::Invalid(detail)) => {
                    let tail = spawned.stderr_tail();
                    return Err(PreviewFailure::new(
                        Cause::RuntimeStatusUnavailable,
                        format!("{detail}{tail}"),
                    ));
                }
                Ok((api, lifecycle, mode, loaded, raw)) => {
                    // Best effort: ask for a clean stop, then kill regardless.
                    let _ = request_stop(&socket);
                    let stdout = spawned.terminate();
                    return Ok(PreviewEvidence {
                        instance: preview.instance.clone(),
                        api,
                        lifecycle,
                        socket_mode: mode,
                        loaded_sides: loaded,
                        status_sha256: sha256_hex(raw.as_bytes()),
                        stdout_sha256: sha256_hex(&stdout),
                        startup_ms: started.elapsed().as_millis() as u64,
                    });
                }
            }
        }
        if Instant::now() >= deadline {
            let tail = spawned.stderr_tail();
            return Err(match composing {
                // Answered, never composed: a composition failure with a cause.
                Some(detail) => PreviewFailure::new(
                    Cause::RuntimeStatusUnavailable,
                    format!(
                        "{detail} · el preview no compuso dentro de preview.timeout_secs={}s{tail}",
                        preview.timeout_secs
                    ),
                ),
                // Never answered: there is no projection to attribute.
                None => PreviewFailure::new(
                    Cause::PreviewTimeout,
                    format!(
                        "el preview no publicó un status válido en {}s (socket {socket_text}){tail}",
                        preview.timeout_secs
                    ),
                ),
            });
        }
        std::thread::sleep(POLL);
    }
}

/// One private socket directory per run: `$TMPDIR/dfr-preview-<pid>-<n>`.
///
/// The gate owns this directory (not the side), which is what lets it delete the
/// socket afterwards and why the path stays inside the 100-byte limit even when
/// the side lives deep in a workspace tree.
fn socket_dir() -> Result<PathBuf, PreviewFailure> {
    let dir = std::env::temp_dir().join(format!(
        "dfr-preview-{}-{}",
        std::process::id(),
        SEQ.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&dir).map_err(|e| {
        PreviewFailure::new(
            Cause::PreviewFailed,
            format!("no se pudo crear el directorio del socket {}: {e}", dir.display()),
        )
    })?;
    Ok(dir)
}

/// Resolve `argv[0]`: absolute as-is; a name with a slash relative to the side
/// root (the process cwd); a bare name against the side root, then beside the
/// gate binary, then `PATH`.
pub fn resolve_program(arg0: &str, side_root: &Path) -> PathBuf {
    let path = Path::new(arg0);
    if path.is_absolute() || arg0.contains('/') {
        return path.to_path_buf();
    }
    let in_side = side_root.join(arg0);
    if in_side.is_file() {
        return in_side;
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let beside = dir.join(arg0);
            if beside.is_file() {
                return beside;
            }
        }
    }
    path.to_path_buf()
}

/// Result of one attempt to read `status`.
enum Handshake {
    /// The listener is not accepting yet (or the answer is still in flight).
    Retry,
    /// A well-formed `status` of *this* instance whose composition has not
    /// landed yet (`lifecycle` not yet `running`, or the declared side not yet
    /// in `loaded_sides`). Legitimate while the preview composes — `bind` is
    /// legal before `start`/`load-side` — so it is re-read until the deadline
    /// and only then becomes terminal (`runtime_status_unavailable`).
    Composing(String),
    /// The socket answered, but not with an acceptable v1 `status` in a way
    /// that waiting cannot fix (framing, `api`, `instance`, socket path/mode,
    /// JSON-RPC error). Terminal at once.
    Invalid(String),
}

/// Connect, send one `status` request frame, validate the response.
fn handshake(
    socket: &Path,
    instance: &str,
    side_id: &str,
    deadline: Instant,
) -> Result<(String, String, u32, Vec<String>, String), Handshake> {
    let remaining = deadline.saturating_duration_since(Instant::now());
    if remaining.is_zero() {
        return Err(Handshake::Retry);
    }
    let mut stream = match UnixStream::connect(socket) {
        Ok(stream) => stream,
        Err(_) => return Err(Handshake::Retry),
    };
    let io_budget = remaining.min(HANDSHAKE_READ);
    let _ = stream.set_read_timeout(Some(io_budget));
    let _ = stream.set_write_timeout(Some(io_budget));
    let request = format!(
        "{{\"jsonrpc\":\"2.0\",\"id\":1,\"api\":\"{STATUS_API}\",\"method\":\"status\"}}\n"
    );
    if stream.write_all(request.as_bytes()).and_then(|_| stream.flush()).is_err() {
        return Err(Handshake::Retry);
    }
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    match reader.read_line(&mut line) {
        Ok(0) => Err(Handshake::Retry),
        Ok(_) => validate_status(&line, instance, socket, side_id),
        Err(e)
            if e.kind() == std::io::ErrorKind::WouldBlock
                || e.kind() == std::io::ErrorKind::TimedOut =>
        {
            Err(Handshake::Retry)
        }
        Err(_) => Err(Handshake::Retry),
    }
}

/// Validate the exact v1 `status` projection of *this* instance.
fn validate_status(
    line: &str,
    instance: &str,
    socket: &Path,
    side_id: &str,
) -> Result<(String, String, u32, Vec<String>, String), Handshake> {
    let raw = line.trim().to_string();
    let value: Value = serde_json::from_str(&raw)
        .map_err(|e| Handshake::Invalid(format!("status no es JSON: {e}")))?;
    if let Some(error) = value.get("error") {
        return Err(Handshake::Invalid(format!(
            "status respondió error JSON-RPC: {}",
            error
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("sin mensaje")
        )));
    }
    let result = value
        .get("result")
        .and_then(Value::as_object)
        .ok_or_else(|| Handshake::Invalid("status sin objeto `result`".into()))?;
    let api = result
        .get("api")
        .and_then(Value::as_str)
        .ok_or_else(|| Handshake::Invalid("status sin `result.api`".into()))?;
    if api != STATUS_API {
        return Err(Handshake::Invalid(format!(
            "status.api={api:?} no es {STATUS_API:?}"
        )));
    }
    let got_instance = result
        .get("instance")
        .and_then(Value::as_str)
        .ok_or_else(|| Handshake::Invalid("status sin `result.instance`".into()))?;
    if got_instance != instance {
        return Err(Handshake::Invalid(format!(
            "status.instance={got_instance:?} no es la instancia pedida {instance:?}"
        )));
    }
    let lifecycle = result
        .get("lifecycle")
        .and_then(Value::as_str)
        .ok_or_else(|| Handshake::Invalid("status sin `result.lifecycle`".into()))?;
    if lifecycle != COMPOSED_LIFECYCLE {
        return Err(Handshake::Composing(format!(
            "status.lifecycle={lifecycle:?} no es {COMPOSED_LIFECYCLE:?}: el preview aún no compuso"
        )));
    }
    let socket_obj = result
        .get("socket")
        .and_then(Value::as_object)
        .ok_or_else(|| Handshake::Invalid("status sin `result.socket`".into()))?;
    let path = socket_obj
        .get("path")
        .and_then(Value::as_str)
        .ok_or_else(|| Handshake::Invalid("status sin `result.socket.path`".into()))?;
    if Path::new(path) != socket {
        return Err(Handshake::Invalid(format!(
            "status.socket.path={path:?} no es el socket del gate {:?}",
            socket.display().to_string()
        )));
    }
    let mode = socket_obj
        .get("mode")
        .and_then(Value::as_u64)
        .ok_or_else(|| Handshake::Invalid("status sin `result.socket.mode`".into()))?;
    if mode != u64::from(REQUIRED_SOCKET_MODE) {
        return Err(Handshake::Invalid(format!(
            "status.socket.mode={mode:#o} no es {REQUIRED_SOCKET_MODE:#o}"
        )));
    }
    let loaded: Vec<String> = result
        .get("loaded_sides")
        .and_then(Value::as_array)
        .ok_or_else(|| Handshake::Invalid("status sin `result.loaded_sides`".into()))?
        .iter()
        .map(|v| {
            v.as_str()
                .ok_or_else(|| Handshake::Invalid("status.loaded_sides con entrada no textual".into()))
                .map(str::to_string)
        })
        .collect::<Result<_, _>>()?;
    if !loaded.iter().any(|s| s == side_id) {
        return Err(Handshake::Composing(format!(
            "status.loaded_sides={loaded:?} no contiene el side declarado {side_id:?}: el preview aún no compuso el side"
        )));
    }
    Ok((
        api.to_string(),
        lifecycle.to_string(),
        mode as u32,
        loaded,
        raw,
    ))
}

/// Best-effort `stop` so the preview can unwind before we kill it.
fn request_stop(socket: &Path) -> std::io::Result<()> {
    let mut stream = UnixStream::connect(socket)?;
    stream.set_read_timeout(Some(Duration::from_millis(200)))?;
    stream.write_all(
        format!("{{\"jsonrpc\":\"2.0\",\"id\":2,\"api\":\"{STATUS_API}\",\"method\":\"stop\"}}\n")
            .as_bytes(),
    )?;
    stream.flush()
}

/// A live preview process plus its captured output and socket directory.
struct Spawned {
    child: Option<Child>,
    child_dir: PathBuf,
    stdout: Option<JoinHandle<Vec<u8>>>,
    stderr: Option<JoinHandle<Vec<u8>>>,
}

impl Spawned {
    fn spawn(
        program: &Path,
        args: &[String],
        side_root: &Path,
        socket_dir: &Path,
    ) -> Result<Self, PreviewFailure> {
        let mut child = Command::new(program)
            .args(args)
            .current_dir(side_root)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| {
                PreviewFailure::new(
                    Cause::PreviewFailed,
                    format!(
                        "no se pudo lanzar el preview {:?} (argv sin shell): {e}",
                        program.display()
                    ),
                )
            })?;
        let stdout = child.stdout.take().map(drain);
        let stderr = child.stderr.take().map(drain);
        Ok(Spawned {
            child: Some(child),
            child_dir: socket_dir.to_path_buf(),
            stdout,
            stderr,
        })
    }

    /// `Some(status)` when the preview process already exited.
    fn exited(&mut self) -> Option<std::process::ExitStatus> {
        let child = self.child.as_mut()?;
        match child.try_wait() {
            Ok(Some(status)) => Some(status),
            _ => None,
        }
    }

    /// Kill (if still alive), reap, join the readers and return stdout.
    fn terminate(&mut self) -> Vec<u8> {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        self.stdout
            .take()
            .and_then(|h| h.join().ok())
            .unwrap_or_default()
    }

    /// Kill, reap and return the captured stderr tail (for the failure detail).
    fn stderr_tail(&mut self) -> String {
        let _ = self.terminate();
        let bytes = self
            .stderr
            .take()
            .and_then(|h| h.join().ok())
            .unwrap_or_default();
        let text = String::from_utf8_lossy(&bytes);
        let tail: String = text.chars().rev().take(400).collect::<Vec<_>>().into_iter().rev().collect();
        if tail.trim().is_empty() {
            String::new()
        } else {
            format!(" · stderr: {}", tail.trim())
        }
    }
}

impl Drop for Spawned {
    fn drop(&mut self) {
        let _ = self.terminate();
        if let Some(h) = self.stderr.take() {
            let _ = h.join();
        }
        let _ = fs::remove_dir_all(&self.child_dir);
    }
}

fn drain<R: Read + Send + 'static>(mut reader: R) -> JoinHandle<Vec<u8>> {
    std::thread::spawn(move || {
        let mut kept = Vec::new();
        let mut buf = [0u8; 4096];
        loop {
            match reader.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    if kept.len() < PIPE_CAP {
                        let room = PIPE_CAP - kept.len();
                        kept.extend_from_slice(&buf[..n.min(room)]);
                    }
                }
                Err(_) => break,
            }
        }
        kept
    })
}

/// The socket directory of a run is private to the gate; this helper exists so
/// tests can assert the mode the preview published is not world-readable.
pub fn socket_mode(socket: &Path) -> Option<u32> {
    fs::metadata(socket)
        .ok()
        .map(|m| m.permissions().mode() & 0o777)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn program_resolution_order() {
        let side = std::env::temp_dir().join(format!("f4b-prog-{}", std::process::id()));
        let _ = fs::create_dir_all(&side);
        let local = side.join("preview-bin");
        fs::write(&local, b"#!/bin/sh\n").unwrap();
        // bare name found in the side root
        assert_eq!(resolve_program("preview-bin", &side), local);
        // bare name absent everywhere stays bare (PATH lookup by exec)
        assert_eq!(resolve_program("definitely-not-here-xyz", &side), PathBuf::from("definitely-not-here-xyz"));
        // slash keeps the caller's relative path (cwd = side root)
        assert_eq!(resolve_program("./bin/x", &side), PathBuf::from("./bin/x"));
        // absolute is untouched
        assert_eq!(resolve_program("/bin/sh", &side), PathBuf::from("/bin/sh"));
        let _ = fs::remove_dir_all(&side);
    }

    #[test]
    fn status_validation_accepts_the_composed_projection() {
        let socket = PathBuf::from("/tmp/dfr-preview-1-0/dfr-preview.sock");
        let line = format!(
            r#"{{"jsonrpc":"2.0","id":1,"result":{{"api":"1.0","instance":"preview","lifecycle":"running","loaded_sides":["f4b-preview-pass"],"socket":{{"path":"{}","mode":384}},"counters":{{}}}}}}"#,
            socket.display()
        );
        let (api, lifecycle, mode, loaded, raw) =
            match validate_status(&line, "preview", &socket, "f4b-preview-pass") {
                Ok(ok) => ok,
                Err(Handshake::Invalid(detail)) => panic!("{detail}"),
                Err(Handshake::Composing(detail)) => panic!("{detail}"),
                Err(Handshake::Retry) => panic!("unexpected retry"),
            };
        assert_eq!(api, "1.0");
        assert_eq!(lifecycle, "running");
        assert_eq!(mode, 0o600);
        assert_eq!(loaded, vec!["f4b-preview-pass".to_string()]);
        assert_eq!(raw, line.trim());
    }

    /// The classification is the whole fix: a projection that is merely *not
    /// composed yet* (the legal `bind` → `start`/`load-side` window) is
    /// `Composing` and gets re-read; a projection that violates a static field
    /// is `Invalid` and is terminal at once, because no amount of waiting can
    /// change `api`, `instance`, the socket path or its mode.
    #[test]
    fn status_validation_classifies_progress_and_violation() {
        let socket = PathBuf::from("/tmp/dfr-preview-1-0/dfr-preview.sock");
        let path = socket.display().to_string();
        let good = |api: &str, instance: &str, lifecycle: &str, mode: u32, sides: &str, path: &str| {
            format!(
                r#"{{"jsonrpc":"2.0","id":1,"result":{{"api":"{api}","instance":"{instance}","lifecycle":"{lifecycle}","loaded_sides":{sides},"socket":{{"path":"{path}","mode":{mode}}}}}}}"#
            )
        };
        let kind = |line: &str| match validate_status(line, "preview", &socket, "side-x") {
            Ok(_) => "ok",
            Err(Handshake::Retry) => "retry",
            Err(Handshake::Composing(_)) => "composing",
            Err(Handshake::Invalid(_)) => "invalid",
        };
        // Static violations: waiting cannot fix them -> terminal at once.
        assert_eq!(kind(&good("9.9", "preview", "running", 384, r#"["side-x"]"#, &path)), "invalid");
        assert_eq!(kind(&good("1.0", "other", "running", 384, r#"["side-x"]"#, &path)), "invalid");
        assert_eq!(kind(&good("1.0", "preview", "running", 0o644, r#"["side-x"]"#, &path)), "invalid");
        assert_eq!(kind(&good("1.0", "preview", "running", 384, r#"["side-x"]"#, "/tmp/other.sock")), "invalid");
        assert_eq!(kind(r#"{"jsonrpc":"2.0","id":1,"error":{"code":-32001,"message":"host_error"}}"#), "invalid");
        assert_eq!(kind("not json"), "invalid");
        assert_eq!(kind(r#"{"jsonrpc":"2.0","id":1,"result":{}}"#), "invalid");
        // Composition progress: legal before `start`/`load-side` -> re-read.
        assert_eq!(kind(&good("1.0", "preview", "stopped", 384, r#"["side-x"]"#, &path)), "composing");
        assert_eq!(kind(&good("1.0", "preview", "running", 384, "[]", &path)), "composing");
        assert_eq!(kind(&good("1.0", "preview", "running", 384, r#"["other"]"#, &path)), "composing");
        // The composed projection.
        assert_eq!(kind(&good("1.0", "preview", "running", 384, r#"["side-x"]"#, &path)), "ok");
    }

    #[test]
    fn socket_dir_is_private_and_short() {
        let dir = socket_dir().unwrap();
        let socket = dir.join("dfr-preview.sock");
        assert!(socket.display().to_string().len() <= MAX_SOCKET_PATH_BYTES);
        assert!(!socket.display().to_string().contains('@'));
        let _ = fs::remove_dir_all(&dir);
    }
}
