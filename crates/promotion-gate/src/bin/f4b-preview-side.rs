//! `f4b-preview-side` — the F4-B **preview harness** (a fixture, never a product).
//!
//! The promotion gate's step 6 has to observe a real composition: a separate
//! process that publishes a `0600` Unix domain socket and answers the v1
//! `status` projection of [`daggerfall_game_runtime_api`] for one instance with
//! a side loaded. This binary *is* that process for the gate's fixtures.
//!
//! It is deliberately a harness and not `crates/app`: the gate must observe a
//! composition it did not build into the game's default path (F4-B constraint:
//! the preview is a separate harness, `crates/app` is untouched).
//!
//! ## Modes (one per typed step-6 cause)
//!
//! | Flag | Behaviour | Gate cause |
//! |:---|:---|:---|
//! | *(none)* | bind → `start` → `load-side` → serve | — (`PASS`) |
//! | `--no-side` | bind → `start` (no side loaded) | `runtime_status_unavailable` |
//! | `--no-socket` | sleep without binding | `preview_timeout` |
//! | `--exit <code>` | print and exit immediately | `preview_failed` |
//! | `--slow-start <ms>` | bind → *wait* → `start` → `load-side` → serve | — (`PASS`) |
//! | `--slow-load <ms>` | bind → `start` → *wait* → `load-side` → serve | — (`PASS`) |
//!
//! The two `--slow-*` modes do not introduce a cause: they exist so the gate's
//! step 6a is tested against the **legal** window between publishing the socket
//! and composing. `bind` may precede `start`/`load-side`, so during the wait the
//! `status` projection honestly answers `lifecycle="stopped"` (slow start) or
//! `loaded_sides=[]` (slow load). A gate that treats that first answer as
//! terminal rejects a valid side; the gate must re-read inside
//! `preview.timeout_secs` and only then decide.
//!
//! ## Why it drives itself over the socket
//!
//! `RuntimeServer` owns its `Server` behind an `Arc`, so the embedder cannot
//! call `handle_authenticated_request` on a started instance. Doing the
//! `start`/`load-side` calls **as a client** is the honest thing anyway: it
//! exercises exactly the wire the gate will read, including the framing and the
//! peer-credential check, and it leaves no in-process shortcut the gate could
//! accidentally trust.

use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::fs::MetadataExt;
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Duration;

use daggerfall_content_api::SideId;
use daggerfall_game_runtime_api::{
    ControlCommand, InstanceId, ProbeRegistry, RuntimeHost, RuntimeServer, Screenshot,
    ServerConfig, SocketPath, API_LABEL,
};

const USAGE: &str = "\
f4b-preview-side — harness de preview del gate F4-B (fixture)

Uso:
  f4b-preview-side --socket <path> --instance <id> [--side <side-id>]
                   [--no-side] [--no-socket] [--exit <code>]
                   [--slow-start <ms>] [--slow-load <ms>]

  --socket <path>   socket exacto que el gate espera publicar
  --instance <id>   instancia a servir (misma gramática que el contrato)
  --side <side-id>  side a cargar tras `start` (default: ninguno)
  --no-side         bind + start, pero no carga side (fallo de composición)
  --no-socket       no publica socket (fixture de timeout)
  --exit <code>     sale de inmediato con ese código (fixture de preview_failed)
  --slow-start <ms> espera tras publicar el socket y antes de `start`
                    (ventana legal bind → start; NO es una causa nueva)
  --slow-load <ms>  espera tras `start` y antes de `load-side`
                    (ventana legal start → load-side; NO es una causa nueva)
";

fn main() {
    let mut socket: Option<PathBuf> = None;
    let mut instance: Option<String> = None;
    let mut side: Option<String> = None;
    let mut no_side = false;
    let mut no_socket = false;
    let mut exit: Option<i32> = None;
    let mut slow_start = Duration::ZERO;
    let mut slow_load = Duration::ZERO;

    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-h" | "--help" => {
                print!("{USAGE}");
                return;
            }
            "--socket" | "--instance" | "--side" | "--exit" | "--slow-start" | "--slow-load" => {
                let Some(value) = args.next() else {
                    fail(&format!("{arg} requiere un valor"));
                };
                match arg.as_str() {
                    "--socket" => socket = Some(PathBuf::from(value)),
                    "--instance" => instance = Some(value),
                    "--side" => side = Some(value),
                    "--exit" => match value.parse::<i32>() {
                        Ok(code) => exit = Some(code),
                        Err(e) => fail(&format!("--exit {}: {e}", value)),
                    },
                    "--slow-start" => slow_start = millis(&arg, &value),
                    "--slow-load" => slow_load = millis(&arg, &value),
                    _ => unreachable!(),
                }
            }
            "--no-side" => no_side = true,
            "--no-socket" => no_socket = true,
            other => fail(&format!("argumento desconocido {other:?}")),
        }
    }

    if let Some(code) = exit {
        println!("f4b-preview-side: salida inmediata {code} (fixture preview_failed)");
        std::process::exit(code);
    }

    if no_socket {
        println!("f4b-preview-side: sin socket, durmiendo (fixture preview_timeout)");
        park_forever();
    }

    let Some(socket) = socket else {
        fail("falta --socket <path>");
    };
    let Some(instance) = instance else {
        fail("falta --instance <id>");
    };
    if no_side && side.is_some() {
        fail("--no-side y --side son excluyentes");
    }
    let instance = match InstanceId::parse(&instance) {
        Ok(instance) => instance,
        Err(e) => fail(&format!("--instance inválida: {e}")),
    };
    let dir = match socket.parent() {
        Some(dir) if !dir.as_os_str().is_empty() => dir.to_path_buf(),
        _ => fail("--socket necesita un directorio padre"),
    };
    let published = match SocketPath::for_instance(&dir, &instance) {
        Ok(path) => path,
        Err(e) => fail(&format!("socket inválido: {e}")),
    };
    if published.as_path() != socket.as_path() {
        fail(&format!(
            "el gate pidió {:?} pero el contrato publica {:?}",
            socket.display(),
            published.as_path().display()
        ));
    }
    let side = match side {
        Some(raw) => match SideId::parse(&raw) {
            Ok(side) => Some(side),
            Err(e) => fail(&format!("--side inválido: {e}")),
        },
        None => None,
    };

    let host = std::sync::Arc::new(FixtureHost::default());
    let config = ServerConfig {
        socket: published,
        instance: instance.clone(),
        expected_uid: own_uid(),
        host,
        probes: std::sync::Arc::new(ProbeRegistry::new()),
    };
    // Owns the accept loop for the life of the process; the gate kills us.
    let _server: RuntimeServer = match daggerfall_game_runtime_api::Server::bind(config) {
        Ok(server) => server.start(),
        Err(e) => fail(&format!("no se pudo publicar el socket: {e}")),
    };

    // The socket is already published and answering `status` truthfully with
    // the pre-composition projection. A real preview is allowed to publish
    // before it composes, so the gate must not read this window as a verdict.
    if !slow_start.is_zero() {
        println!(
            "f4b-preview-side: socket publicado, `start` en {} ms (ventana bind → start)",
            slow_start.as_millis()
        );
        std::thread::sleep(slow_start);
    }

    // Compose: `start`, then (unless --no-side) `load-side`. Both are the
    // published wire methods, so the state the gate later reads is the state a
    // real client would have produced.
    if let Err(e) = call(&socket, "start", None) {
        fail(&format!("start falló: {e}"));
    }
    if !slow_load.is_zero() {
        println!(
            "f4b-preview-side: `start` hecho, `load-side` en {} ms (ventana start → load-side)",
            slow_load.as_millis()
        );
        std::thread::sleep(slow_load);
    }
    if let Some(side) = &side {
        let params = format!("{{\"side\":\"{}\"}}", side.as_str());
        if let Err(e) = call(&socket, "load-side", Some(&params)) {
            fail(&format!("load-side falló: {e}"));
        }
    }

    println!(
        "f4b-preview-side: sirviendo {} · instance {} · api {} · side {}",
        socket.display(),
        instance.as_str(),
        API_LABEL,
        side.as_ref().map(SideId::as_str).unwrap_or("(ninguno)")
    );

    park_forever()
}

/// The fixture host: "loads" a side by recording its id (no ECS behind it).
#[derive(Default)]
struct FixtureHost {
    loaded: Mutex<BTreeMap<String, SideId>>,
}

impl RuntimeHost for FixtureHost {
    fn apply(&self, command: &ControlCommand) -> Result<Vec<SideId>, String> {
        let mut loaded = self
            .loaded
            .lock()
            .map_err(|_| "fixture host poisoned".to_string())?;
        match command {
            ControlCommand::LoadSide(side) => {
                loaded.insert(side.as_str().to_string(), side.clone());
            }
            ControlCommand::UnloadSide(side) => {
                loaded.remove(side.as_str());
            }
            ControlCommand::Start
            | ControlCommand::Stop
            | ControlCommand::Pause
            | ControlCommand::Resume => {}
        }
        Ok(loaded.values().cloned().collect())
    }

    fn screenshot(&self) -> Result<Option<Screenshot>, String> {
        // Headless harness: no frame to project.
        Ok(None)
    }

    fn metrics(&self) -> Result<BTreeMap<String, u64>, String> {
        Ok(BTreeMap::new())
    }
}

/// One JSON-RPC frame over the control socket, newline-delimited like the gate.
fn call(socket: &PathBuf, method: &str, params: Option<&str>) -> Result<String, String> {
    let mut stream =
        UnixStream::connect(socket).map_err(|e| format!("connect {}: {e}", socket.display()))?;
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .map_err(|e| e.to_string())?;
    let params = params.map(|p| format!(",\"params\":{p}")).unwrap_or_default();
    let frame = format!(
        "{{\"jsonrpc\":\"2.0\",\"id\":1,\"api\":\"{API_LABEL}\",\"method\":\"{method}\"{params}}}\n"
    );
    stream
        .write_all(frame.as_bytes())
        .and_then(|_| stream.flush())
        .map_err(|e| format!("write: {e}"))?;
    let mut line = String::new();
    BufReader::new(stream)
        .read_line(&mut line)
        .map_err(|e| format!("read: {e}"))?;
    if line.contains("\"error\"") {
        return Err(line.trim().to_string());
    }
    Ok(line)
}

/// The process uid, from the owner of `/proc/self` (no extra dependency).
fn own_uid() -> u32 {
    std::fs::metadata("/proc/self")
        .map(|m| m.uid())
        .unwrap_or_else(|_| {
            std::fs::metadata("/")
                .map(|m| m.uid())
                .expect("the harness needs to know its own uid")
        })
}

/// `--slow-start`/`--slow-load` value: whole milliseconds.
fn millis(flag: &str, value: &str) -> Duration {
    match value.parse::<u64>() {
        Ok(ms) => Duration::from_millis(ms),
        Err(e) => fail(&format!("{flag} {value}: {e}")),
    }
}

fn park_forever() -> ! {
    loop {
        std::thread::sleep(Duration::from_secs(3600));
    }
}

fn fail(message: &str) -> ! {
    eprintln!("f4b-preview-side: {message}\n\n{USAGE}");
    std::process::exit(2);
}
