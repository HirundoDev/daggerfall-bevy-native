//! F3 acceptance suite - plan `I1` 9 F3 criteria (a)-(g), plus the lifecycle,
//! probe and framing invariants the contract publishes.
//!
//! Every test is deterministic: temp directories are unique per test
//! (`temp_dir()` + pid + counter), there are no sleeps beyond the server's own
//! 2 ms accept poll, and no assertion depends on a wall clock, randomness or an
//! environment variable.

use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::fs::{FileTypeExt, PermissionsExt};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use daggerfall_content_api::SideId;
use daggerfall_game_runtime_api::{
    ApiVersion, ControlCommand, InstanceId, LifecycleState, ProbeOutcome, ProbeRegistry,
    RuntimeError, RuntimeHost, RuntimeServer, RuntimeSocket, Screenshot, Server, ServerConfig,
    SocketPath, MAX_FRAME,
};
use serde_json::{json, Value};

// ---------------------------------------------------------------- fixtures

static COUNTER: AtomicU64 = AtomicU64::new(0);

struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new(tag: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "dfr-f3-{}-{}-{tag}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&path).expect("create temp dir");
        TempDir { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

fn current_uid() -> u32 {
    nix::sys::socket::UnixCredentials::new().uid()
}

#[derive(Default)]
struct MockHost {
    applied: Mutex<Vec<String>>,
    loaded: Mutex<Vec<SideId>>,
    screenshot: Mutex<Option<Screenshot>>,
    fail_next: AtomicBool,
}

impl MockHost {
    fn new() -> Self {
        MockHost::default()
    }

    fn applied_commands(&self) -> Vec<String> {
        self.applied.lock().expect("applied lock").clone()
    }

    fn fail_next_command(&self) {
        self.fail_next.store(true, Ordering::SeqCst);
    }

    fn set_screenshot(&self, screenshot: Option<Screenshot>) {
        *self.screenshot.lock().expect("screenshot lock") = screenshot;
    }
}

impl RuntimeHost for MockHost {
    fn apply(&self, command: &ControlCommand) -> Result<Vec<SideId>, String> {
        if self.fail_next.swap(false, Ordering::SeqCst) {
            return Err("host refused the command".to_string());
        }
        self.applied
            .lock()
            .expect("applied lock")
            .push(command.as_str().to_string());
        let mut loaded = self.loaded.lock().expect("loaded lock");
        match command {
            ControlCommand::LoadSide(side) => {
                if !loaded.contains(side) {
                    loaded.push(side.clone());
                }
            }
            ControlCommand::UnloadSide(side) => loaded.retain(|candidate| candidate != side),
            ControlCommand::Start
            | ControlCommand::Stop
            | ControlCommand::Pause
            | ControlCommand::Resume => {}
        }
        Ok(loaded.clone())
    }

    fn screenshot(&self) -> Result<Option<Screenshot>, String> {
        Ok(self.screenshot.lock().expect("screenshot lock").clone())
    }

    fn metrics(&self) -> Result<BTreeMap<String, u64>, String> {
        let mut metrics = BTreeMap::new();
        metrics.insert("frames".to_string(), 42);
        metrics.insert("dropped_frames".to_string(), 0);
        Ok(metrics)
    }
}

struct Fixture {
    dir: TempDir,
    host: Arc<MockHost>,
    probes: Arc<ProbeRegistry>,
}

impl Fixture {
    fn new(tag: &str) -> Self {
        Fixture::with_probes(tag, Arc::new(ProbeRegistry::new()))
    }

    fn with_probes(tag: &str, probes: Arc<ProbeRegistry>) -> Self {
        Fixture {
            dir: TempDir::new(tag),
            host: Arc::new(MockHost::new()),
            probes,
        }
    }

    fn config(&self, instance: &str, uid: u32) -> ServerConfig {
        let instance = InstanceId::parse(instance).expect("valid instance id");
        let socket =
            SocketPath::for_instance(self.dir.path(), &instance).expect("valid socket path");
        ServerConfig {
            socket,
            instance,
            expected_uid: uid,
            host: self.host.clone(),
            probes: Arc::clone(&self.probes),
        }
    }

    fn server(&self, instance: &str, uid: u32) -> Server {
        Server::bind(self.config(instance, uid)).expect("bind runtime server")
    }

    fn started(&self, instance: &str, uid: u32) -> RuntimeServer {
        self.server(instance, uid).start()
    }
}

struct Client {
    reader: BufReader<UnixStream>,
    writer: UnixStream,
}

impl Client {
    fn connect(path: &Path) -> Self {
        let stream = UnixStream::connect(path).expect("connect to runtime socket");
        stream
            .set_read_timeout(Some(Duration::from_secs(10)))
            .expect("set read timeout");
        let writer = stream.try_clone().expect("clone stream");
        Client {
            reader: BufReader::new(stream),
            writer,
        }
    }

    fn send(&mut self, line: &str) {
        self.writer
            .write_all(line.as_bytes())
            .expect("write request");
        self.writer.write_all(b"\n").expect("write newline");
        self.writer.flush().expect("flush request");
    }

    fn recv(&mut self) -> Value {
        let mut line = String::new();
        let read = self.reader.read_line(&mut line).expect("read response");
        assert!(read > 0, "the server closed without a response");
        serde_json::from_str(line.trim()).expect("response is valid JSON")
    }

    fn at_eof(&mut self) -> bool {
        let mut byte = [0u8; 1];
        matches!(self.reader.read(&mut byte), Ok(0))
    }
}

fn request(id: u64, method: &str, params: Value) -> String {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "api": "1.0",
        "method": method,
        "params": params,
    })
    .to_string()
}

fn plain(id: u64, method: &str) -> String {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "api": "1.0",
        "method": method,
    })
    .to_string()
}

fn status_request(id: u64) -> String {
    plain(id, "status")
}

fn side_request(id: u64, method: &str, side: &str) -> String {
    request(id, method, json!({"side": side}))
}

fn probe_request(id: u64, name: &str) -> String {
    request(id, "probes.run", json!({"name": name}))
}

fn handle(server: &Server, line: &str) -> Value {
    serde_json::from_str(&server.handle_authenticated_request(line)).expect("response is JSON")
}

fn error_code(response: &Value) -> String {
    response
        .pointer("/error/data/code")
        .and_then(Value::as_str)
        .unwrap_or_else(|| panic!("no error code in {response}"))
        .to_string()
}

fn lifecycle_of(response: &Value) -> String {
    response
        .pointer("/result/lifecycle")
        .and_then(Value::as_str)
        .unwrap_or_else(|| panic!("no lifecycle in {response}"))
        .to_string()
}

fn side(raw: &str) -> SideId {
    SideId::parse(raw).expect("valid side id")
}

// -------------------------------------------------- (a) socket, permissions

#[test]
fn a_runtime_socket_is_a_user_only_unix_socket_under_the_requested_dir_and_disappears_on_drop() {
    let dir = TempDir::new("socket-mode");
    let instance = InstanceId::parse("preview-one").expect("valid instance");
    let path = SocketPath::for_instance(dir.path(), &instance).expect("valid path");

    assert!(
        path.as_path().starts_with(dir.path()),
        "the socket must live under the requested dir: {}",
        path
    );
    assert_eq!(
        path.as_path().file_name().and_then(|name| name.to_str()),
        Some("dfr-preview-one.sock")
    );

    let socket = RuntimeSocket::bind(path.clone()).expect("bind socket");
    let metadata = std::fs::metadata(path.as_path()).expect("socket metadata");
    assert!(
        metadata.file_type().is_socket(),
        "the path must be a unix socket, got {:?}",
        metadata.file_type()
    );
    assert_eq!(
        metadata.permissions().mode() & 0o777,
        0o600,
        "a runtime socket is user-only"
    );
    assert_eq!(socket.mode(), 0o600);

    drop(socket);
    assert!(
        !path.as_path().exists(),
        "dropping the socket guard must unlink the path"
    );
}

#[test]
fn a_server_bind_publishes_a_0600_socket_and_unlinks_it_on_drop() {
    let fixture = Fixture::new("server-mode");
    let server = fixture.server("preview", current_uid());
    let path = server.path().to_path_buf();

    let status = server.status();
    assert_eq!(status.socket.mode, 0o600);
    assert_eq!(status.socket.path, path.display().to_string());
    let metadata = std::fs::metadata(&path).expect("socket metadata");
    assert!(metadata.file_type().is_socket());
    assert_eq!(metadata.permissions().mode() & 0o777, 0o600);

    drop(server);
    assert!(!path.exists(), "the server must unlink its socket on drop");
}

#[test]
fn a_instance_and_socket_path_grammars_are_enforced() {
    assert!(InstanceId::parse("preview-1").is_ok());
    assert!(InstanceId::parse("a").is_ok());
    assert!(InstanceId::parse("0.preview_2-x").is_ok());
    assert!(InstanceId::parse("").is_err());
    assert!(InstanceId::parse("Preview").is_err());
    assert!(InstanceId::parse("-preview").is_err());
    assert!(InstanceId::parse("@preview").is_err());
    assert!(InstanceId::parse("pre/view").is_err());
    assert!(InstanceId::parse(&"a".repeat(65)).is_err());

    let instance = InstanceId::parse("preview").expect("valid instance");
    let deep = PathBuf::from(format!("/tmp/{}", "x".repeat(120)));
    match SocketPath::for_instance(&deep, &instance) {
        Err(RuntimeError::SocketPathTooLong { len }) => {
            assert!(len > 100, "reported length must exceed sun_path budget");
        }
        other => panic!("deep path must be refused, got {other:?}"),
    }
    match SocketPath::for_instance(Path::new("/tmp/@abstract"), &instance) {
        Err(RuntimeError::InvalidSocketPath { .. }) => {}
        other => panic!("abstract-socket syntax must be refused, got {other:?}"),
    }
}

// ------------------------------------------------- (b) peer authentication

#[test]
fn b_unauthorized_peer_uid_is_rejected_before_the_request_is_read() {
    let fixture = Fixture::new("peer-rejected");
    let unauthorized_uid = current_uid().wrapping_add(1);
    let mut server = fixture.started("preview", unauthorized_uid);
    let path = server.path().to_path_buf();

    let mut client = Client::connect(&path);
    client.send(&status_request(1));
    let response = client.recv();
    assert_eq!(
        error_code(&response),
        "unauthorized_peer",
        "a peer with the wrong uid must be refused: {response}"
    );
    assert_eq!(response["error"]["code"], json!(-32010));
    assert_eq!(
        response["id"],
        Value::Null,
        "a pre-parse refusal cannot echo an id"
    );

    // The request was never parsed: a non-JSON frame gets the same refusal.
    let mut attacker = Client::connect(&path);
    attacker.send("this is not json and must never be parsed");
    assert_eq!(error_code(&attacker.recv()), "unauthorized_peer");

    let status = server.status();
    assert_eq!(
        status.counters.served, 0,
        "no unauthorized request may be served"
    );
    assert_eq!(status.counters.rejected_peers, 2);

    server.shutdown();
    server.join();
}

#[test]
fn b_authorized_peer_uid_is_served_status() {
    let fixture = Fixture::new("peer-authorized");
    let mut server = fixture.started("preview", current_uid());
    let path = server.path().to_path_buf();

    let mut client = Client::connect(&path);
    client.send(&status_request(1));
    let response = client.recv();
    assert_eq!(response["id"], json!(1));
    assert_eq!(response["result"]["api"], json!("1.0"));
    assert_eq!(response["result"]["instance"], json!("preview"));
    assert_eq!(response["result"]["lifecycle"], json!("stopped"));
    assert_eq!(response["result"]["loaded_sides"], json!([]));

    assert_eq!(server.status().counters.served, 1);
    server.shutdown();
    server.join();
}

#[test]
fn b_client_cannot_supply_a_uid_or_peer_identity() {
    let fixture = Fixture::new("uid-spoof");
    let server = fixture.server("preview", current_uid());

    let with_uid = r#"{"jsonrpc":"2.0","id":1,"api":"1.0","method":"status","uid":0}"#;
    assert_eq!(
        error_code(&handle(&server, with_uid)),
        "invalid_request",
        "a top-level uid must be rejected as an unknown field"
    );

    let with_peer = r#"{"jsonrpc":"2.0","id":2,"api":"1.0","method":"status","peer":{"uid":0}}"#;
    assert_eq!(error_code(&handle(&server, with_peer)), "invalid_request");

    let in_params = r#"{"jsonrpc":"2.0","id":3,"api":"1.0","method":"status","params":{"uid":0}}"#;
    assert_eq!(
        error_code(&handle(&server, in_params)),
        "invalid_params",
        "status takes no parameters, so a uid in params is refused"
    );

    let legit = handle(&server, &status_request(4));
    assert_eq!(
        legit["result"]["lifecycle"],
        json!("stopped"),
        "the kernel-attested path still works"
    );
}

// ------------------------------------------------------------- (c) probes

#[test]
fn c_probes_run_accepts_only_catalogue_names() {
    let probes = Arc::new(ProbeRegistry::new());
    probes
        .register(
            "ALL-001",
            "baseline main probe",
            Arc::new(|| ProbeOutcome::pass("37/37")),
        )
        .expect("register ALL-001");
    probes
        .register(
            "SKILLS-001",
            "skills catalogue probe",
            Arc::new(|| ProbeOutcome::skip("no skills dir")),
        )
        .expect("register SKILLS-001");

    let listed: Vec<String> = probes.list().iter().map(|spec| spec.name.to_string()).collect();
    assert_eq!(listed, vec!["ALL-001", "SKILLS-001"], "list is sorted by name");
    assert_eq!(
        probes.register("ALL-001", "again", Arc::new(|| ProbeOutcome::pass("x"))),
        Err(RuntimeError::DuplicateProbe {
            name: "ALL-001".to_string()
        }),
        "a duplicate registration must not silently replace an entry"
    );
    assert!(matches!(
        probes.register("bad-name", "x", Arc::new(|| ProbeOutcome::pass("x"))),
        Err(RuntimeError::InvalidProbeName { .. })
    ));

    let fixture = Fixture::with_probes("probes", Arc::clone(&probes));
    let server = fixture.server("preview", current_uid());

    let ok = handle(&server, &probe_request(1, "ALL-001"));
    assert_eq!(ok["result"]["name"], json!("ALL-001"));
    assert_eq!(ok["result"]["outcome"], json!("pass"));
    assert_eq!(ok["result"]["detail"], json!("37/37"));

    let skipped = handle(&server, &probe_request(2, "SKILLS-001"));
    assert_eq!(skipped["result"]["outcome"], json!("skip"));

    for bad in ["../bin/sh", "/bin/sh", "bash -c id", "ALL-001 ", "all-001"] {
        let response = handle(&server, &probe_request(3, bad));
        assert_eq!(
            error_code(&response),
            "invalid_probe_name",
            "`{bad}` must be refused by the name grammar"
        );
    }

    let unknown = handle(&server, &probe_request(4, "ALL-999"));
    assert_eq!(
        error_code(&unknown),
        "probe_unknown",
        "a well-formed unregistered name is a lookup miss"
    );

    for extra in ["command", "path"] {
        let mut params = serde_json::Map::new();
        params.insert("name".to_string(), json!("ALL-001"));
        params.insert(extra.to_string(), json!("id"));
        let line = request(5, "probes.run", Value::Object(params));
        assert_eq!(
            error_code(&handle(&server, &line)),
            "invalid_params",
            "`{extra}` must not be accepted as a probe parameter"
        );
    }

    let catalogue = handle(&server, &plain(6, "probes.list"));
    assert_eq!(catalogue["result"]["count"], json!(2));
    assert_eq!(catalogue["result"]["probes"][0]["name"], json!("ALL-001"));
    assert_eq!(server.status().counters.probes_run, 2);
}

// ------------------------------------------------------------ (d) versioning

#[test]
fn d_api_version_is_negotiated_before_any_method_work() {
    let fixture = Fixture::new("api-version");
    let server = fixture.server("preview", current_uid());

    for declared in ["2.0", "1.1", "1", "01.0", "1.0.0"] {
        let line = format!(
            r#"{{"jsonrpc":"2.0","id":7,"api":"{declared}","method":"no-such-method"}}"#
        );
        let response = handle(&server, &line);
        assert_eq!(
            error_code(&response),
            "unsupported_api",
            "`{declared}` must be refused with a typed version error"
        );
        assert_eq!(response["error"]["data"]["declared"], json!(declared));
        assert_eq!(response["error"]["data"]["supported"], json!("1.0"));
        assert_eq!(
            response["id"],
            json!(7),
            "a post-id refusal echoes the id"
        );
    }

    let missing = r#"{"jsonrpc":"2.0","id":8,"method":"status"}"#;
    assert_eq!(error_code(&handle(&server, missing)), "unsupported_api");

    let numeric = r#"{"jsonrpc":"2.0","id":9,"api":1.0,"method":"status"}"#;
    assert_eq!(error_code(&handle(&server, numeric)), "unsupported_api");

    let supported = handle(&server, &status_request(10));
    assert!(supported["result"].is_object(), "1.0 must be served");

    assert_eq!(ApiVersion::parse("1.0"), Ok(ApiVersion::CURRENT));
    assert!(ApiVersion::CURRENT.accepts(ApiVersion::CURRENT));
    assert!(!ApiVersion::CURRENT.accepts(ApiVersion::parse("1.1").expect("parses")));
    assert!(!ApiVersion::CURRENT.accepts(ApiVersion::parse("2.0").expect("parses")));
    for malformed in ["", "1", "1.", ".1", "01.0", "1.0.0", "1.a"] {
        assert!(
            ApiVersion::parse(malformed).is_err(),
            "`{malformed}` is not canonical M.m"
        );
    }

    assert_eq!(ApiVersion::CURRENT.label(), "1.0");
    assert_eq!(ApiVersion::LABEL, "1.0");
    assert!(
        server.status().counters.rejected_api >= 7,
        "every unsupported declaration is counted"
    );
}

// --------------------------------------------------------- (e) one socket

#[test]
fn e_one_socket_per_instance_and_rebind_after_drop() {
    let fixture = Fixture::new("one-socket");
    let instance = InstanceId::parse("preview").expect("valid instance");
    let path = SocketPath::for_instance(fixture.dir.path(), &instance).expect("valid path");

    let first = RuntimeSocket::bind(path.clone()).expect("first bind");
    match RuntimeSocket::bind(path.clone()) {
        Err(RuntimeError::SocketInUse { path: refused }) => {
            assert_eq!(
                refused,
                path.to_string(),
                "the refusal must name the socket it did not create"
            );
        }
        Err(other) => panic!("expected socket_in_use, got {other}"),
        Ok(_) => panic!("a second bind of the same instance must be refused"),
    }

    let other_instance = InstanceId::parse("preview-two").expect("valid instance");
    let other_path =
        SocketPath::for_instance(fixture.dir.path(), &other_instance).expect("valid path");
    let other = RuntimeSocket::bind(other_path.clone()).expect("second instance binds");
    assert_ne!(path, other_path);
    assert!(path.as_path().exists() && other_path.as_path().exists());

    drop(first);
    assert!(!path.as_path().exists());
    let reborn = RuntimeSocket::bind(path.clone()).expect("rebind after drop");

    let server = fixture.server("preview-three", current_uid());
    match Server::bind(fixture.config("preview-three", current_uid())) {
        Err(RuntimeError::SocketInUse { .. }) => {}
        Err(other) => panic!("expected socket_in_use, got {other}"),
        Ok(_) => panic!("a bound server must refuse a second bind"),
    }

    drop(server);
    drop(reborn);
    drop(other);
}

// ----------------------------------------------------------- (f) lifecycle

#[test]
fn f_stop_is_idempotent_total_and_never_leaves_a_zombie() {
    let fixture = Fixture::new("lifecycle");
    let server = fixture.server("preview", current_uid());
    let host = Arc::clone(&fixture.host);

    assert_eq!(
        lifecycle_of(&handle(&server, &plain(1, "start"))),
        "running"
    );
    assert_eq!(lifecycle_of(&handle(&server, &plain(2, "stop"))), "stopped");
    assert_eq!(
        lifecycle_of(&handle(&server, &plain(3, "stop"))),
        "stopped",
        "stop is idempotent and still Ok"
    );
    assert_eq!(
        lifecycle_of(&handle(&server, &plain(4, "start"))),
        "running"
    );
    assert_eq!(lifecycle_of(&handle(&server, &plain(5, "pause"))), "paused");
    assert_eq!(
        lifecycle_of(&handle(&server, &plain(6, "stop"))),
        "stopped",
        "stop from paused is legal"
    );
    assert_eq!(server.status().lifecycle, LifecycleState::Stopped);
    assert_eq!(
        host.applied_commands(),
        vec!["start", "stop", "stop", "start", "pause", "stop"],
        "the host sees exactly the legal commands, in order"
    );

    let applied_before = host.applied_commands().len();
    let pause_from_stopped = handle(&server, &plain(7, "pause"));
    assert_eq!(error_code(&pause_from_stopped), "illegal_transition");
    assert_eq!(pause_from_stopped["error"]["data"]["from"], json!("stopped"));
    assert_eq!(
        pause_from_stopped["error"]["data"]["command"],
        json!("pause")
    );

    let resume_from_stopped = handle(&server, &plain(8, "resume"));
    assert_eq!(error_code(&resume_from_stopped), "illegal_transition");

    let load_from_stopped = handle(&server, &side_request(9, "load-side", "alpha"));
    assert_eq!(error_code(&load_from_stopped), "illegal_transition");

    assert_eq!(
        host.applied_commands().len(),
        applied_before,
        "illegal commands must never reach the host"
    );
    assert_eq!(server.status().counters.illegal_transitions, 3);
    assert_eq!(server.status().lifecycle, LifecycleState::Stopped);
}

#[test]
fn f_loaded_side_set_follows_the_host_and_unknown_side_is_refused_before_the_host() {
    let fixture = Fixture::new("sides");
    let server = fixture.server("preview", current_uid());
    let host = Arc::clone(&fixture.host);

    assert_eq!(
        lifecycle_of(&handle(&server, &plain(1, "start"))),
        "running"
    );

    let loaded_one = handle(&server, &side_request(2, "load-side", "alpha"));
    assert_eq!(loaded_one["result"]["loaded_sides"], json!(["alpha"]));

    let loaded_two = handle(&server, &side_request(3, "load-side", "beta"));
    assert_eq!(
        loaded_two["result"]["loaded_sides"],
        json!(["alpha", "beta"]),
        "the host's returned set is committed and sorted"
    );
    assert_eq!(
        server.status().loaded_sides,
        vec![side("alpha"), side("beta")]
    );

    let applied_before = host.applied_commands().len();
    let unknown = handle(&server, &side_request(4, "unload-side", "gamma"));
    assert_eq!(error_code(&unknown), "unknown_side");
    assert_eq!(
        host.applied_commands().len(),
        applied_before,
        "an unknown side is refused before the host is called"
    );

    let unloaded = handle(&server, &side_request(5, "unload-side", "beta"));
    assert_eq!(unloaded["result"]["loaded_sides"], json!(["alpha"]));

    let bad_grammar = handle(&server, &side_request(6, "load-side", "Bad!"));
    assert_eq!(error_code(&bad_grammar), "invalid_params");
    assert!(
        bad_grammar["error"]["message"]
            .as_str()
            .expect("message")
            .contains("invalid_side"),
        "the content-api detail must survive: {bad_grammar}"
    );

    let missing = handle(&server, &request(7, "load-side", json!({})));
    assert_eq!(error_code(&missing), "invalid_params");

    let extra = handle(
        &server,
        &request(8, "load-side", json!({"side": "alpha", "path": "/bin/sh"})),
    );
    assert_eq!(
        error_code(&extra),
        "invalid_params",
        "no path parameter may ever be accepted"
    );
}

#[test]
fn f_host_failure_commits_nothing_and_reports_host_error() {
    let fixture = Fixture::new("host-failure");
    let server = fixture.server("preview", current_uid());

    let start = handle(&server, &plain(1, "start"));
    assert_eq!(lifecycle_of(&start), "running");

    fixture.host.fail_next_command();
    let failed = handle(&server, &side_request(2, "load-side", "alpha"));
    assert_eq!(error_code(&failed), "host_error");
    assert_eq!(
        server.status().lifecycle,
        LifecycleState::Running,
        "a failed command leaves the committed state untouched"
    );
    assert!(server.status().loaded_sides.is_empty());

    fixture.host.fail_next_command();
    let failed_stop = handle(&server, &plain(3, "stop"));
    assert_eq!(error_code(&failed_stop), "host_error");
    assert_eq!(
        server.status().lifecycle,
        LifecycleState::Running,
        "a stop the host refuses is visible instead of a silent zombie"
    );
}

// ---------------------------------------------- (g) framing and robustness

#[test]
fn g_malformed_frames_produce_exact_stable_codes_and_the_server_recovers() {
    let fixture = Fixture::new("framing");
    let server = fixture.server("preview", current_uid());

    let cases: [(&str, &str); 11] = [
        ("this is not json", "invalid_request"),
        ("[1,2,3]", "invalid_request"),
        (r#"{"jsonrpc":"1.0","id":1,"api":"1.0","method":"status"}"#, "invalid_request"),
        (r#"{"jsonrpc":"2.0","api":"1.0","method":"status"}"#, "invalid_request"),
        (r#"{"jsonrpc":"2.0","id":1,"api":"1.0","method":"nope"}"#, "method_not_found"),
        (r#"{"jsonrpc":"2.0","id":1,"api":"1.0","method":"status","params":[1]}"#, "invalid_params"),
        ("   ", "invalid_request"),
        (r#"{"jsonrpc":"2.0","id":1,"api":"1.0","method":"events","params":{"after":1.5}}"#, "invalid_params"),
        (r#"{"jsonrpc":"2.0","id":1,"api":"1.0","method":"events","params":{"after":-1}}"#, "invalid_params"),
        (r#"{"jsonrpc":"2.0","id":1,"api":"1.0","method":"events","params":{"after":"1"}}"#, "invalid_params"),
        (r#"{"jsonrpc":"2.0","id":1,"api":"1.0","method":"status","params":{"nope":1}}"#, "invalid_params"),
    ];
    for (line, expected) in cases {
        assert_eq!(
            error_code(&handle(&server, line)),
            expected,
            "frame `{line}` must map to `{expected}`"
        );
    }

    let oversized = "x".repeat(MAX_FRAME + 1);
    assert_eq!(
        error_code(&handle(&server, &oversized)),
        "frame_too_large",
        "the pure entry point enforces the same frame bound"
    );

    let recovered = handle(&server, &status_request(99));
    assert!(recovered["result"].is_object());

    let mut wire = fixture.started("framing-wire", current_uid());
    let path = wire.path().to_path_buf();
    let mut client = Client::connect(&path);
    client.send("not json");
    assert_eq!(error_code(&client.recv()), "invalid_request");
    client.send(&status_request(1));
    assert_eq!(
        client.recv()["result"]["lifecycle"],
        json!("stopped"),
        "a bad frame must not desync the next one on the same connection"
    );
    wire.shutdown();
    wire.join();
}

#[test]
fn g_oversized_frame_closes_only_that_connection() {
    let fixture = Fixture::new("oversized");
    let mut server = fixture.started("preview", current_uid());
    let path = server.path().to_path_buf();

    let mut client = Client::connect(&path);
    client.send(&"x".repeat(MAX_FRAME + 1024));
    let response = client.recv();
    assert_eq!(error_code(&response), "frame_too_large");
    assert_eq!(response["error"]["data"]["max"], json!(65536));
    assert_eq!(response["id"], Value::Null);
    assert!(
        client.at_eof(),
        "the offending connection must be closed after frame_too_large"
    );

    let mut fresh = Client::connect(&path);
    fresh.send(&status_request(1));
    assert_eq!(fresh.recv()["result"]["instance"], json!("preview"));

    server.shutdown();
    server.join();
}

// ------------------------------------------------------- observability

#[test]
fn f_events_is_a_bounded_pull_with_cursor_and_metrics_and_screenshot_are_host_owned() {
    let probes = Arc::new(ProbeRegistry::new());
    probes
        .register("ALL-001", "baseline", Arc::new(|| ProbeOutcome::pass("ok")))
        .expect("register");
    let fixture = Fixture::with_probes("observability", probes);
    let server = fixture.server("preview", current_uid());

    let empty = handle(&server, &request(1, "events", json!({})));
    assert_eq!(empty["result"]["events"], json!([]));
    assert_eq!(empty["result"]["next"], json!(1));
    assert_eq!(empty["result"]["dropped"], json!(0));

    handle(&server, &plain(2, "start"));
    handle(&server, &plain(3, "pause"));
    handle(&server, &probe_request(4, "ALL-001"));

    let all = handle(&server, &request(5, "events", json!({"after": 0})));
    assert_eq!(
        all["result"]["events"],
        json!([
            {"seq": 1, "kind": "lifecycle"},
            {"seq": 2, "kind": "lifecycle"},
            {"seq": 3, "kind": "probe"},
        ])
    );
    assert_eq!(all["result"]["next"], json!(4));

    let tail = handle(&server, &request(6, "events", json!({"after": 2})));
    assert_eq!(tail["result"]["events"], json!([{"seq": 3, "kind": "probe"}]));

    let metrics = handle(&server, &plain(7, "metrics"));
    assert_eq!(metrics["result"]["host"]["frames"], json!(42));
    assert_eq!(metrics["result"]["counters"]["probes_run"], json!(1));
    // The snapshot is taken while building the `metrics` result, before that
    // response itself is counted: the six prior successes are visible.
    assert_eq!(metrics["result"]["counters"]["served"], json!(6));

    let empty_shot = handle(&server, &plain(8, "screenshot"));
    assert_eq!(empty_shot["result"]["screenshot"], Value::Null);

    fixture
        .host
        .set_screenshot(Some(Screenshot::new(640, 480, "ab".repeat(32))));
    let shot = handle(&server, &plain(9, "screenshot"));
    assert_eq!(shot["result"]["screenshot"]["width"], json!(640));
    assert_eq!(shot["result"]["screenshot"]["height"], json!(480));
    assert_eq!(shot["result"]["screenshot"]["format"], json!("rgba8"));
    assert_eq!(
        shot["result"]["screenshot"]["sha256"],
        json!("ab".repeat(32))
    );
}

// --------------------- duplicate object names are never ambiguous ----------

/// RFC 8259 §4 leaves the effective name of a repeated key undefined, and
/// `serde_json` silently keeps the last one. A control boundary cannot let the
/// client and the server disagree about which value was read, so every object
/// that names a key twice - at any depth - is `invalid_request` (-32600).
///
/// Each case is chosen so that last-wins would have produced a *different*
/// code, which is what the pre-fix server did.
#[test]
fn duplicate_json_keys_are_rejected_at_any_depth() {
    let fixture = Fixture::new("duplicates");
    let server = fixture.server("preview", current_uid());

    let cases: [(&str, &str); 6] = [
        // `method` twice: last-wins would answer `method_not_found`.
        (
            r#"{"jsonrpc":"2.0","id":1,"api":"1.0","method":"status","method":"nope"}"#,
            "invalid_request",
        ),
        // `api` twice, benign then hostile: last-wins would be `unsupported_api`.
        (
            r#"{"jsonrpc":"2.0","id":1,"api":"1.0","api":"9.9","method":"status"}"#,
            "invalid_request",
        ),
        // `params` twice: last-wins would serve the request.
        (
            r#"{"jsonrpc":"2.0","id":1,"api":"1.0","method":"events","params":{"after":0},"params":{}}"#,
            "invalid_request",
        ),
        // The same key twice inside `params`: last-wins would be `probe_unknown`.
        (
            r#"{"jsonrpc":"2.0","id":1,"api":"1.0","method":"probes.run","params":{"name":"ALL-001","name":"ALL-002"}}"#,
            "invalid_request",
        ),
        // Two levels down, inside a value `serde_json` would collapse.
        (
            r#"{"jsonrpc":"2.0","id":1,"api":"1.0","method":"events","params":{"after":0,"filter":{"k":1,"k":2}}}"#,
            "invalid_request",
        ),
        // The same name spelled with a unicode escape: `"\u006eame"` is `"name"`.
        (
            r#"{"jsonrpc":"2.0","id":1,"api":"1.0","method":"probes.run","params":{"name":"ALL-001","\u006eame":"ALL-001"}}"#,
            "invalid_request",
        ),
    ];
    for (line, expected) in cases {
        let response = handle(&server, line);
        assert_eq!(
            error_code(&response),
            expected,
            "frame `{line}` must map to `{expected}`"
        );
        assert_eq!(
            response["error"]["code"],
            json!(-32600),
            "duplicate keys reuse the existing `invalid_request` code"
        );
        assert_eq!(response["error"]["data"]["code"], json!("invalid_request"));
    }

    // The rule lives in the pure entry point, so it holds on the wire too.
    let mut wire = fixture.started("duplicates-wire", current_uid());
    let path = wire.path().to_path_buf();
    let mut client = Client::connect(&path);
    client.send(r#"{"jsonrpc":"2.0","id":7,"api":"1.0","method":"status","method":"nope"}"#);
    let response = client.recv();
    assert_eq!(error_code(&response), "invalid_request");
    assert_eq!(response["error"]["code"], json!(-32600));
    wire.shutdown();
    wire.join();
}

/// The duplicate-key scan must not reject valid JSON: escaped quotes and
/// backslashes, `\uXXXX` (including a surrogate pair), arrays of objects, empty
/// objects and empty arrays all still parse, and frames the dispatcher refuses
/// for their *content* keep their own codes instead of turning into
/// `invalid_request`.
#[test]
fn legitimate_frames_with_escapes_and_nesting_are_not_refused() {
    let probes = Arc::new(ProbeRegistry::new());
    probes
        .register("ALL-001", "baseline", Arc::new(|| ProbeOutcome::pass("ok")))
        .expect("register");
    let fixture = Fixture::with_probes("legitimate", probes);
    let server = fixture.server("preview", current_uid());

    // Served normally.
    let probe = handle(&server, &probe_request(1, "ALL-001"));
    assert_eq!(probe["result"]["outcome"], json!("pass"));
    let events = handle(&server, &request(2, "events", json!({"after": 0})));
    assert_eq!(events["result"]["events"][0]["kind"], json!("probe"));
    let status = handle(&server, &request(3, "status", json!({})));
    assert_eq!(status["result"]["lifecycle"], json!("stopped"));

    // Accepted by the protocol: no repeated key anywhere, so the scan stays out
    // of the way. The dispatcher's own refusal is `invalid_params` (unknown
    // parameter), which proves the frame reached dispatch.
    let accepted = [
        r#"{"jsonrpc":"2.0","id":4,"api":"1.0","method":"events","params":{"after":0,"note":"he said \"quoted\" and \\ backslash"}}"#,
        r#"{"jsonrpc":"2.0","id":5,"api":"1.0","method":"events","params":{"after":0,"note":"caf\u00e9 \u2603 \ud83d\ude00"}}"#,
        r#"{"jsonrpc":"2.0","id":6,"api":"1.0","method":"events","params":{"after":0,"nested":[{"a":1},{"a":2}],"empty":{},"none":[],"deep":{"a":{"a":true,"b":null}}}}"#,
    ];
    for line in accepted {
        assert!(
            daggerfall_game_runtime_api::protocol::parse_request(line).is_ok(),
            "a duplicate-free frame must still parse: {line}"
        );
        assert_eq!(
            error_code(&handle(&server, line)),
            "invalid_params",
            "frame `{line}` must reach dispatch instead of being refused as malformed"
        );
    }
}

// ------------- the published socket path is never briefly world-visible ----

/// `bind` must publish the final path only once it is already `0600`.
///
/// Binding at the final name and calling `chmod` afterwards leaves the socket
/// connectable, for a moment, with the umask-derived mode. The fix binds at an
/// unpredictable sibling, narrows it, and publishes it atomically, so after
/// `bind` returns the directory holds exactly one entry - the final name, a
/// socket, mode `0600` - and no temporary name survives.
#[test]
fn final_socket_path_is_never_observable_with_wider_permissions() {
    let dir = TempDir::new("socket-publish");
    let instance = InstanceId::parse("preview").expect("valid instance");
    let path = SocketPath::for_instance(dir.path(), &instance).expect("valid path");

    let socket = RuntimeSocket::bind(path.clone()).expect("bind socket");
    let metadata = std::fs::symlink_metadata(path.as_path()).expect("the final path exists");
    assert!(
        !metadata.file_type().is_symlink(),
        "the final path is a real filesystem entry"
    );
    assert!(
        metadata.file_type().is_socket(),
        "the final path is a unix socket"
    );
    assert_eq!(
        metadata.permissions().mode() & 0o777,
        0o600,
        "the published path is exactly 0600, never the umask-derived mode"
    );
    assert_eq!(socket.mode(), 0o600);

    let mut names = directory_names(dir.path());
    names.sort();
    assert_eq!(
        names,
        vec!["dfr-preview.sock".to_string()],
        "binding must leave exactly the final entry behind, no temporary sibling"
    );

    drop(socket);
    assert!(
        !path.as_path().exists(),
        "dropping the socket guard unlinks the final path"
    );
}

/// Publishing by rename/link must not break the binding: a client that resolves
/// the *final* name is served by the listener that was bound at the temporary
/// name (same inode, and `sun_path` is only resolved by the client).
#[test]
fn a_client_connecting_through_the_published_path_is_served() {
    let fixture = Fixture::new("socket-publish-connect");
    let mut server = fixture.started("preview", current_uid());
    let path = server.path().to_path_buf();

    let metadata = std::fs::symlink_metadata(&path).expect("the final path exists");
    assert!(metadata.file_type().is_socket());
    assert_eq!(metadata.permissions().mode() & 0o777, 0o600);

    let mut client = Client::connect(&path);
    client.send(&status_request(1));
    let response = client.recv();
    assert_eq!(
        response["result"]["instance"],
        json!("preview"),
        "the final name must reach the listener that was bound as a temporary sibling"
    );

    server.shutdown();
    server.join();
}

/// A refused bind leaves the directory exactly as it found it: no temporary
/// sibling, whether the final name is held by a live socket or by a stale file.
#[test]
fn a_refused_bind_leaves_no_temporary_sibling_behind() {
    let dir = TempDir::new("socket-occupied");
    let instance = InstanceId::parse("preview").expect("valid instance");
    let path = SocketPath::for_instance(dir.path(), &instance).expect("valid path");

    let held = RuntimeSocket::bind(path.clone()).expect("first bind");
    match RuntimeSocket::bind(path.clone()) {
        Err(RuntimeError::SocketInUse { path: refused }) => {
            assert_eq!(refused, path.to_string(), "the refusal names the final path");
        }
        other => panic!("a second bind of a live path must be SocketInUse, got {other:?}"),
    }
    assert_eq!(
        directory_names(dir.path()),
        vec!["dfr-preview.sock".to_string()],
        "a refused bind must not leave a temporary sibling"
    );
    drop(held);

    // A stale entry is refused the same way, and again leaves nothing behind.
    std::fs::write(path.as_path(), b"stale").expect("occupy the final path with a regular file");
    match RuntimeSocket::bind(path.clone()) {
        Err(RuntimeError::SocketInUse { .. }) => {}
        other => panic!("a stale entry must be refused as SocketInUse, got {other:?}"),
    }
    assert_eq!(
        directory_names(dir.path()),
        vec!["dfr-preview.sock".to_string()],
        "a refused bind must not disturb the existing entry or add a temporary one"
    );
    std::fs::remove_file(path.as_path()).expect("clear the stale entry");

    // A dangling symlink is an existing filesystem entry too. Following it
    // (`Path::exists`) would report nothing and bind would fail with a raw
    // `io` error instead of the contract's `SocketInUse`.
    let target = dir.path().join("elsewhere.sock");
    std::os::unix::fs::symlink(&target, path.as_path()).expect("occupy the final path with a link");
    match RuntimeSocket::bind(path.clone()) {
        Err(RuntimeError::SocketInUse { .. }) => {}
        other => panic!("an existing entry must be SocketInUse, got {other:?}"),
    }
    assert_eq!(
        directory_names(dir.path()),
        vec!["dfr-preview.sock".to_string()],
        "the refusal must not add a temporary sibling next to the existing entry"
    );
}

fn directory_names(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .expect("read directory")
        .map(|entry| {
            entry
                .expect("directory entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    names.sort();
    names
}
