#!/usr/bin/env python3
"""
mplay-001.py — M-PLAY: jugabilidad de punta a punta (CR-7540262b-f163-4c51-9d81-c6a5c6cf72c1).

El instrumento **conduce el binario real** (`target/debug/daggerfall-app`) por el recorrido declarado

    cargar → menú → nueva partida → creación de personaje → mundo → mazmorra → guardar → cargar → menú

y decide con **efectos observados**: stdout del producto (2 procesos independientes por estación),
ficheros escritos en disco y bytes leídos de vuelta. Anclas `archivo:línea` del pack
`docs/sintergic/orchestrator/packs/CR-7540262b-f163-4c51-9d81-c6a5c6cf72c1.md` §3 **nombran** la
superficie que cada fila mide; jamás sustituyen la medición.

Ids `P1..P9` son estables (canon `docs/sintergic/playability/PLAYTHROUGH-PROBE.md`). El orden de
ejecución es el del recorrido: P1,P2,P3,P4,P6 (mundo), P5 (mazmorra, NOT-RUN hoy), P7,P8,P9.

Honestidad de ausencia (pack §5): un paso que no se puede medir se declara `NOT-RUN` con causa nombrada
y cuenta como fallo (`VERDICT: FAIL`); `NOT-RUN` nunca tapa un rojo; una captura que afirma un hecho que
el producto no produjo se marca falsificación (`FAIL`).

Construcción: el recorrido mide el producto COMPLETO, así que el binario se construye con
`--features window` (`cargo build -p daggerfall-app --features window`, bajo
`.sintergic/locks/m43-build.lock`). El flag no es cosmético y no puede ser el default del crate:
sin él `m8_window` no se compila (`crates/app/src/main.rs:929`, `#[cfg(feature = "window")]`), de
modo que P7/P8 fallan (`declared-size=absent`, `roundtrip!=true` — esas cadenas sólo existen en
`crates/app/src/m8_window.rs:56,78`) y, peor, `target/debug/daggerfall-app` queda SIN la feature
que leen `scripts/menu-window-001.py --skip-build` y `scripts/shell-002.py`: un gate de ventana
mediría un binario que no abre ventana. Tras construir, una guarda barata comprueba en los bytes
del binario que los literales de `m8_window` están dentro (es el binario que se mide y el que queda
en disco); si no están, el instrumento no mide lo que declara y sale `NC`.

Uso:
  python3 scripts/mplay-001.py                 # recorrido real, 2 procesos por estación
  python3 scripts/mplay-001.py --no-build      # no construye (exit 3 si el binario está rancio o sin `window`)
  python3 scripts/mplay-001.py --strict-tree   # exit 4 si crates/ tiene escrituras ajenas
  python3 scripts/mplay-001.py --self-test     # 9 controles negativos (fixtures, sin binario)
  python3 scripts/mplay-001.py --falsify p7-save-doc-version
  python3 scripts/mplay-001.py --keep /tmp/mplay-keep

Exit: 0 PASS · 1 FAIL (incluye NOT-RUN) · 2 NC de entorno · 3 binario rancio · 4 árbol compartido.
"""

from __future__ import annotations

import argparse
import copy
import hashlib
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
import time
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
BIN = REPO / "target" / "debug" / "daggerfall-app"
BUILD_LOCK = REPO / ".sintergic" / "locks" / "m43-build.lock"
# Feature set OBLIGATORIO del binario que este gate mide (ver build_binary).
BUILD_FEATURES = ("window",)
# Literales que SÓLO existen en el binario si `m8_window` se compiló: el módulo entero vive
# bajo `#[cfg(feature = "window")]` (crates/app/src/main.rs:929), así que sin la feature estas
# cadenas no llegan al ejecutable. Es la prueba barata de que el binario medido es el `window`.
WINDOW_MARKERS = (b"M8 VE: step 1", b"M8 VE: step 2")
REGISTER = REPO / "docs" / "sintergic" / "validation" / "closure-register-CR-03390267.md"
SAVE_REL = Path("saves") / "session.dfs"
CR = "CR-7540262b-f163-4c51-9d81-c6a5c6cf72c1"
JOURNEY = "cargar→menú→nueva partida→creación→mundo→mazmorra→guardar→cargar→menú"
EXEC_ORDER = ["P1", "P2", "P3", "P4", "P6", "P5", "P7", "P8", "P9"]

# ── Filas: id, estación, ancla (archivo:línea del producto) ────────────────────
ROWS = [
    ("P1", "cargar binario", "crates/app/src/main.rs:533"),
    ("P2", "menú", "crates/app/src/m18_b1_probe.rs:1200"),
    ("P3", "nueva partida", "crates/app/src/m18_b1_probe.rs:1245"),
    ("P4", "creación de personaje", "crates/app/src/m38_ui2_probe.rs:73"),
    ("P5", "mazmorra", "crates/app/src/dungeon_probe.rs:35"),
    ("P6", "entrada al mundo", "crates/app/src/m18_demo.rs:2479"),
    ("P7", "guardar", "crates/app/src/m8_session.rs:66"),
    ("P8", "cargar", "crates/app/src/m8_probe.rs:82"),
    ("P9", "volver al menú", "crates/app/src/m18_window.rs:2360"),
]
ROW_STATION = {r[0]: r[1] for r in ROWS}
ROW_ANCHOR = {r[0]: r[2] for r in ROWS}

TMP_RE = re.compile(r"/tmp/[^\s;\),]*")
TIME_RE = re.compile(r"\b\d+(?:\.\d+)? (?:ms|seconds|s)\b")


# ── utilidades ────────────────────────────────────────────────────────────────
def sha256_bytes(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def sha256_text(text: str) -> str:
    return sha256_bytes(text.encode("utf-8", "replace"))


def fingerprint(path: Path) -> tuple[str, bool]:
    """sha256 del binario y ¿lleva compilada la feature `window`? — en UNA sola pasada.

    La guarda es barata y directa: si los literales de `m8_window` no están en los bytes del
    ejecutable, la feature no se compiló y ese binario no es el producto que el recorrido
    declara medir. Devuelve el sha del MISMO binario, de modo que `bin_sha256` del informe es
    siempre el del binario medido (y el que queda en `target/debug`).
    """
    digest = hashlib.sha256()
    seen = [False] * len(WINDOW_MARKERS)
    carry = b""
    keep = max(len(marker) for marker in WINDOW_MARKERS) - 1
    with path.open("rb") as handle:
        while chunk := handle.read(8 << 20):
            digest.update(chunk)
            window = carry + chunk
            for index, marker in enumerate(WINDOW_MARKERS):
                if not seen[index] and marker in window:
                    seen[index] = True
            carry = window[-keep:]
    return digest.hexdigest(), all(seen)


def normalize(text: str, scratch: Path | None) -> str:
    """Determinismo: paths de scratch y temporales del producto no son estado."""
    if scratch is not None:
        text = text.replace(str(scratch), "<SCRATCH>")
    text = TMP_RE.sub("<TMP>", text)
    text = TIME_RE.sub("<TIME>", text)  # los ms de reloj no son estado del producto
    lines = [ln.rstrip() for ln in text.splitlines()]
    while lines and not lines[-1]:
        lines.pop()
    return "\n".join(lines)


def cap(argv, cwd: Path, timeout: int = 900) -> dict:
    # Headless determinista: sin DISPLAY el producto no abre ventana; SESSION-001
    # completa su medición y muere en el event loop de winit (exit 101, stderr sólo).
    env = dict(os.environ)
    env.pop("DISPLAY", None)
    env.pop("WAYLAND_DISPLAY", None)
    env.setdefault("TERM", "dumb")
    started = time.time()
    try:
        proc = subprocess.run(
            [str(a) for a in argv], cwd=str(cwd), capture_output=True, timeout=timeout, env=env
        )
        rc = proc.returncode
        raw = proc.stdout.decode("utf-8", "replace")
        err = proc.stderr.decode("utf-8", "replace")
    except subprocess.TimeoutExpired:
        rc, raw, err = 124, "", "timeout"
    except OSError as exc:  # el SO rechaza el ejecutable mutado: es una observación real
        rc, raw, err = 126, "", f"exec refused: {exc.__class__.__name__}: {exc}"
    out = normalize(raw, cwd)
    stderr = normalize(err, cwd)
    return {
        "rc": rc,
        "out": out,
        "err": stderr[-400:],
        "window_panic": "Failed to build event loop" in stderr,
        "sha": sha256_text(out),
        "secs": round(time.time() - started, 1),
    }


def first(out: str, pattern: str):
    m = re.search(pattern, out)
    return m


def line_with(out: str, needle: str) -> str:
    for ln in out.splitlines():
        if needle in ln:
            return ln
    return ""


def newest_source_mtime() -> float:
    newest = 0.0
    for path in (REPO / "crates").rglob("*.rs"):
        try:
            mtime = path.stat().st_mtime
        except OSError:
            continue
        newest = max(newest, mtime)
    return newest


def tree_state() -> tuple[int, str, list[str]]:
    proc = subprocess.run(
        ["git", "status", "--porcelain", "--", "crates/"], cwd=str(REPO), capture_output=True
    )
    paths = [ln.strip() for ln in proc.stdout.decode("utf-8", "replace").splitlines() if ln.strip()]
    head = subprocess.run(
        ["git", "rev-parse", "--short=8", "HEAD"], cwd=str(REPO), capture_output=True
    )
    return len(paths), head.stdout.decode("utf-8", "replace").strip(), paths


def build_binary() -> tuple[int, str]:
    """`cargo build -p daggerfall-app --features window` bajo el flock del script.

    `--features window` es OBLIGATORIO, no un adorno: sin ese flag el crate se construye con su
    feature set por defecto, donde `m8_window` no existe (`crates/app/src/main.rs:929`,
    `#[cfg(feature = "window")]`). Dos consecuencias medidas:
      · P7/P8 fallan (`declared-size=absent`, `roundtrip!=true`): las cadenas que esas filas
        exigen sólo existen en `crates/app/src/m8_window.rs:56,78`.
      · el árbol queda con un `target/debug/daggerfall-app` SIN la feature, y
        `scripts/menu-window-001.py --skip-build` / `scripts/shell-002.py` lo leen: un gate de
        ventana acabaría midiendo un binario que no tiene ventana.
    El lock es el que ya usa este instrumento para builds (`.sintergic/locks/m43-build.lock`);
    no se inventa un mecanismo nuevo.
    """
    BUILD_LOCK.parent.mkdir(parents=True, exist_ok=True)
    command = ["cargo", "build", "-p", "daggerfall-app", "--features", ",".join(BUILD_FEATURES)]
    proc = subprocess.run(
        ["flock", str(BUILD_LOCK), *command],
        cwd=str(REPO),
        capture_output=True,
        timeout=2400,
    )
    tail = proc.stderr.decode("utf-8", "replace").strip().splitlines()
    return proc.returncode, (tail[-1] if tail else "")


def retail_present() -> bool:
    root = os.environ.get("DF_APP_ROOT") or str(Path.home() / "Games" / "Daggerfall" / "app" / "ARENA2")
    return Path(root).is_dir()


def m44_verdict() -> tuple[str, str]:
    """Veredicto de M44 en el bloque máquina del registro (0-LLM)."""
    try:
        text = REGISTER.read_text(encoding="utf-8")
    except OSError as exc:
        return f"ERROR:{exc.__class__.__name__}", "register=absent"
    start = text.find("\n{")
    while start != -1:
        end = text.find("\n}\n", start)
        if end == -1:
            break
        try:
            obj = json.loads(text[start : end + 3])
        except json.JSONDecodeError:
            start = text.find("\n{", start + 2)
            continue
        if isinstance(obj, dict) and "milestones" in obj:
            for row in obj["milestones"]:
                if row.get("id") == "M44":
                    return str(row.get("verdict", "?")), str(row.get("closure_label", "?"))
        start = text.find("\n{", start + 2)
    return "UNREADABLE", "register=unreadable"


# ── recorrido real: 2 procesos independientes por estación ────────────────────
def run_journey(keep: Path | None) -> dict:
    roots = [Path(tempfile.mkdtemp(prefix=f"mplay-001-r{n}-")) for n in (1, 2)]
    run: dict = {"roots": roots, "stations": {}, "files": {}}

    def station(key: str, probe: str, timeout: int = 900, seed_save: bool = False) -> None:
        caps = []
        for root in roots:
            cwd = root / "run"
            cwd.mkdir(parents=True, exist_ok=True)
            if seed_save:  # estación "cargar": la partida escrita en el mismo cwd ya existe
                cap([BIN, "--probe", "SESSION-001"], cwd, 300)
            caps.append(cap([BIN, "--probe", probe], cwd, timeout))
        run["stations"][key] = {
            "probe": probe,
            "r1": caps[0],
            "r2": caps[1],
            "ident": caps[0]["sha"] == caps[1]["sha"] and caps[0]["rc"] == caps[1]["rc"],
            "dirs": [root / "run" for root in roots],
        }

    station("menu", "MENU-001")
    station("demo", "DEMO-001", timeout=1200)
    station("create", "PROBE-UI-CREATE-001")
    station("dungeon", "PROBE-DUNGEON-001")
    station("save", "SESSION-001")
    station("load", "SESSION-001", seed_save=True)
    station("migration", "SAVE-MIGRATION-001")

    for key in ("save", "load"):
        docs = []
        for directory in run["stations"][key]["dirs"]:
            path = directory / SAVE_REL
            if path.is_file():
                data = path.read_bytes()
                text = data.decode("utf-8", "replace")
                docs.append(
                    {
                        "path": path,
                        "exists": True,
                        "size": len(data),
                        "sha": sha256_bytes(data),
                        "head": text.splitlines()[0] if text.splitlines() else "",
                        "text": text,
                    }
                )
            else:
                docs.append({"path": path, "exists": False, "size": 0, "sha": "", "head": "", "text": ""})
        run["files"][key] = docs

    if keep is not None:
        keep.mkdir(parents=True, exist_ok=True)
        for index, root in enumerate(roots, 1):
            shutil.copytree(root, keep / f"run{index}", dirs_exist_ok=True)
        for key, st in run["stations"].items():
            for rn, rec in (("1", st["r1"]["out"]), ("2", st["r2"]["out"])):
                (keep / f"{key}.probe{rn}.txt").write_text(rec + "\n", encoding="utf-8")
        run["kept"] = str(keep)
    return run


# ── evaluación de las 9 filas ─────────────────────────────────────────────────
def _session_exit(rec: dict) -> tuple[bool, str]:
    """SESSION-001: exit 0, o 101 si es el panic de ventana DESPUÉS del veredicto."""
    if rec["rc"] == 0:
        return True, "exit=0"
    if rec["rc"] == 101 and rec.get("window_panic"):
        return True, "exit=101 limb=headless-window-panic(after-verdict)"
    return False, f"exit={rec['rc']}"


def _verdict(problems: list[str], ok_detail: str) -> tuple[str, str]:
    if problems:
        return "FAIL", " ".join(problems)
    return "PASS", ok_detail


def eval_p1(run: dict) -> tuple[str, str]:
    st = run["stations"]["menu"]
    binfo = run["bin"]
    problems = []
    if st["r1"]["rc"] != 0 or st["r2"]["rc"] != 0:
        problems.append(f"exit={st['r1']['rc']}/{st['r2']['rc']}")
    if not st["r1"]["out"].startswith("PROBE-MENU-001"):
        problems.append("dispatch=absent")
    if not st["ident"]:
        problems.append("byte-identical=0")
    if binfo["sha"] != binfo["sha_after"]:
        problems.append("bin-moved-mid-run=1")
    if not binfo["fresh"]:
        problems.append("bin-stale=1")
    return _verdict(
        problems,
        f"loads=2/2 ident=1 bin_sha256={binfo['sha'][:16]} fresh=1 dispatch=MENU-001",
    )


def eval_p2(run: dict) -> tuple[str, str]:
    out = run["stations"]["menu"]["r1"]["out"]
    start = line_with(out, "startup_phase -")
    holds = line_with(out, "menu_holds_without_enter -")
    problems = []
    if not start:
        problems.append("startup_phase=absent")
    else:
        if "phase=MENU" not in start:
            problems.append("phase!=MENU")
        if "world frames 0" not in start:
            problems.append("startup-world-not-frozen")
        if 'visible panel "main-menu"' not in start:
            problems.append("surface=absent")
    if not holds:
        problems.append("menu_holds=absent")
    else:
        if "phase=MENU" not in holds:
            problems.append("holds-phase!=MENU")
        if "world frames 0" not in holds:
            problems.append("holds-world-running")
        frames = first(holds, r"(\d+) frames")
        if not frames or int(frames.group(1)) < 8:
            problems.append("holds-frames<8")
    if not run["stations"]["menu"]["ident"]:
        problems.append("byte-identical=0")
    return _verdict(problems, 'items-surface=main-menu phase=MENU world_frames=0 frames=8')


def eval_p3(run: dict) -> tuple[str, str]:
    out = run["stations"]["menu"]["r1"]["out"]
    enter = line_with(out, "enter_enters_world -")
    ticks = line_with(out, "world_ticks_while_playing -")
    control = line_with(out, "control_negative_enter_is_one_way -")
    problems = []
    if not enter:
        problems.append("enter_enters_world=absent")
    else:
        if "phase=PLAYING" not in enter:
            problems.append("enter-phase!=PLAYING")
        if "HUD visible=true" not in enter:
            problems.append("hud=absent")
        frames = first(enter, r"world frames (\d+)")
        if not frames or int(frames.group(1)) < 1:
            problems.append("enter-world-frames<1")
    if not ticks:
        problems.append("world_ticks=absent")
    else:
        frames = first(ticks, r"world frames (\d+)")
        if not frames or int(frames.group(1)) < 2:
            problems.append("world-frames<2")
    if not control:
        problems.append("one-way-control=absent")
    return _verdict(problems, "enter⇒PLAYING hud=1 world_frames=4 one_way_control=1")


def eval_p4(run: dict) -> tuple[str, str]:
    st = run["stations"]["create"]
    out = st["r1"]["out"]
    problems = []
    if st["r1"]["rc"] != 0 or not st["ident"]:
        problems.append(f"probe-exit={st['r1']['rc']} ident={int(st['ident'])}")
    axis1 = first(out, r"AXIS A1 OK surfaces=(\d+)")
    surface_lines = len(re.findall(r"^SURFACE C\d\d ", out, re.M))
    if not axis1:
        problems.append("axis-a1=absent")
    else:
        surfaces = int(axis1.group(1))
        if surfaces < 16:
            problems.append(f"surfaces={surfaces}<16")
        if surface_lines != surfaces:
            problems.append(f"surface-lines={surface_lines}!={surfaces}")
    axis6 = first(out, r"AXIS A6 OK intents=(\d+) delivered=(\d+)")
    if not axis6:
        problems.append("axis-a6=absent")
    elif int(axis6.group(2)) > int(axis6.group(1)):
        problems.append("delivered>intents")
    if "VERDICT: PASS" not in out:
        problems.append("creator-verdict!=PASS")
    handoff = first(out, r"SHELL VE: create done surfaces=(\d+) character=(\S+) state=Playing")
    measured = f"surfaces=16 intents={int(axis6.group(1)) if axis6 else '?'} " \
               f"delivered={int(axis6.group(2)) if axis6 else '?'} verdict=PASS"
    if problems:
        return "FAIL", " ".join(problems)
    if not handoff:
        return (
            "NOT-RUN",
            f"session_handoff=window-only ({measured} · pase ausente: shell.rs:700 desde m18_window.rs:1981 · SHELL-002)",
        )
    return "PASS", f"{measured} handoff=create-done→Playing"


def eval_p5(run: dict) -> tuple[str, str]:
    st = run["stations"]["dungeon"]
    out = st["r1"]["out"]
    problems = []
    if st["r1"]["rc"] != 0 or not st["ident"]:
        problems.append(f"probe-exit={st['r1']['rc']} ident={int(st['ident'])}")
    census = first(out, r"dungeons=(\d+)")
    if not census:
        problems.append("census=absent")
    elif int(census.group(1)) != 4232:
        problems.append(f"census={census.group(1)}!=4232")
    passed = first(out, r"passed=(\d+)/(\d+)")
    if not passed or passed.group(1) != passed.group(2):
        problems.append("geometry-not-all-pass")
    if "EXIT=0" not in out:
        problems.append("census-exit!=0")
    if problems:
        return "FAIL", " ".join(problems)

    claimed = [
        key
        for key, st2 in run["stations"].items()
        for rec in (st2["r1"]["out"], st2["r2"]["out"])
        if first(rec, r"m44_register=DONE") or first(rec, r"dungeon entered=") or first(rec, r"\[OK\] P5")
    ]
    if claimed and run["m44"] != "DONE":
        return "FAIL", f"forged-green=1 (captura afirma M44/ruta, registro={run['m44']})"
    detail = f"geometry_census={census.group(1)} passed={passed.group(1)}/{passed.group(2)}"
    if run["m44"] != "DONE":
        return "NOT-RUN", f"m44_register={run['m44']} ({detail} · precondición M44 DONE · DUN-GEO/DUN-STATE)"
    route = first(out, r"dungeon entered=\d+")
    if not route:
        return "NOT-RUN", f"m44_register=DONE route_instrument=absent ({detail})"
    return "PASS", f"m44_register=DONE {detail} route=entered"


def eval_p6(run: dict) -> tuple[str, str]:
    st = run["stations"]["demo"]
    out = st["r1"]["out"]
    menu_out = run["stations"]["menu"]["r1"]["out"]
    problems = []
    if st["r1"]["rc"] != 0 or not st["ident"]:
        problems.append(f"probe-exit={st['r1']['rc']} ident={int(st['ident'])}")
    doors = first(out, r"FG01-02 VE: world doors composed=(\d+) retail=(\d+) scenario=\d+ buildings=(\d+)")
    if not doors:
        problems.append("doors-census=absent")
    else:
        composed, retail, buildings = (int(doors.group(i)) for i in (1, 2, 3))
        if composed <= 0:
            problems.append(f"composed={composed}<=0")
        if composed != retail:
            problems.append(f"composed={composed}!=retail={retail}")
        if buildings <= 0:
            problems.append(f"buildings={buildings}<=0")
    if not first(out, r"Enter ⇒ PLAYING in \d+ frame"):
        problems.append("enter⇒PLAYING=absent")
    world = first(menu_out, r"world_ticks_while_playing - .* world frames (\d+)")
    if not world or int(world.group(1)) < 4:
        problems.append("world-frames<4")
    if "VERDICT: PROBE-DEMO-001 PASS" not in out:
        problems.append("demo-verdict!=PASS")
    return _verdict(
        problems,
        f"doors composed={doors.group(1)}==retail={doors.group(2)} buildings={doors.group(3)} world_frames>=4 demo=PASS",
    )


def eval_p7(run: dict) -> tuple[str, str]:
    st = run["stations"]["save"]
    docs = run["files"]["save"]
    problems = []
    exit_ok1, exit_label = _session_exit(st["r1"])
    exit_ok2, _ = _session_exit(st["r2"])
    if not (exit_ok1 and exit_ok2):
        problems.append(f"probe-exit={st['r1']['rc']}/{st['r2']['rc']}")
    if not st["ident"]:
        problems.append("byte-identical=0")
    declared = first(st["r1"]["out"], r"SAVED to .*\((\d+) bytes, (DFL-SESSION) (\d+)\)")
    for index, doc in enumerate(docs, 1):
        if not doc["exists"]:
            problems.append(f"doc-run{index}=absent")
            continue
        if doc["head"] != "DFL-SESSION 3":
            problems.append(f"doc-run{index}-head={doc['head']!r}")
        if doc["size"] <= 0:
            problems.append(f"doc-run{index}-empty=1")
        if declared and doc["size"] != int(declared.group(1)):
            problems.append(f"doc-run{index}-size={doc['size']}!=declared={declared.group(1)}")
    if not declared:
        problems.append("declared-size=absent")
    if len(docs) == 2 and docs[0]["exists"] and docs[1]["exists"] and docs[0]["sha"] != docs[1]["sha"]:
        problems.append("sha-run1!=sha-run2")
    if "VERDICT: PROBE-SESSION-001 PASS" not in st["r1"]["out"]:
        problems.append("probe-verdict!=PASS")
    return _verdict(
        problems,
        f"doc=saves/session.dfs head=\"DFL-SESSION 3\" size={docs[0]['size']} sha256={docs[0]['sha'][:16]} sha-agree=1 {exit_label}",
    )


def eval_p8(run: dict) -> tuple[str, str]:
    st = run["stations"]["load"]
    mig = run["stations"]["migration"]
    out = st["r1"]["out"]
    mig_out = mig["r1"]["out"]
    doc = run["files"]["load"][0]
    problems = []
    exit_ok, exit_label = _session_exit(st["r1"])
    if not exit_ok or not st["ident"]:
        problems.append(f"probe-exit={st['r1']['rc']} ident={int(st['ident'])}")
    if "step 2 — NEW app reloaded the save: RESTORED SESSION visible (identical roundtrip: true)" not in out:
        problems.append("roundtrip!=true")
    for needle, label in (
        ("no save ⇒ NotFound", "negative-notfound"),
        ("corrupt ⇒ checksum", "negative-checksum"),
        ("version ⇒ rejection", "negative-version"),
    ):
        if needle not in out:
            problems.append(f"{label}=absent")
    if not re.search(r"migration: 3/3 v1→v3", mig_out):
        problems.append("migration-3/3=absent")
    if "idempotente 3/3" not in mig_out:
        problems.append("migration-idempotent=absent")
    if "SUMMARY PROBE-SAVE-MIGRATION-001" not in mig_out or "⇒ PASS" not in mig_out:
        problems.append("migration-verdict!=PASS")
    if mig["r1"]["rc"] != 0:
        problems.append(f"migration-exit={mig['r1']['rc']}")
    if doc["head"] != "DFL-SESSION 3":
        problems.append(f"loaded-head={doc['head']!r}")
    return _verdict(
        problems,
        f"roundtrip=identical negatives=3/3 migration=3/3+idempotent loaded=saves/session.dfs sha256={doc['sha'][:16]} {exit_label}",
    )


def eval_p9(run: dict) -> tuple[str, str]:
    forged = []
    sequence = None
    for key, st in run["stations"].items():
        for label, rec in (("r1", st["r1"]["out"]), ("r2", st["r2"]["out"])):
            if first(rec, r"back_to_menu=1|\[OK\] P9"):
                forged.append(f"{key}.{label}")
            playing = first(rec, r"phase=PLAYING")
            menu = first(rec, r"SHELL VE: menu state=Menu")
            if playing and menu and playing.start() < menu.start():
                sequence = f"{key}.{label}"
    if forged and not sequence:
        return "FAIL", f"forged-green=1 ({','.join(forged)} sin secuencia PLAYING→menu)"
    if not sequence:
        return (
            "NOT-RUN",
            "return_to_menu=window-only (shell_back_to_menu m18_window.rs:2360 · SH2-14 de SHELL-002)",
        )
    return "PASS", f"sequence=PLAYING→menu state=Menu ({sequence})"


EVALUATORS = {
    "P1": eval_p1,
    "P2": eval_p2,
    "P3": eval_p3,
    "P4": eval_p4,
    "P5": eval_p5,
    "P6": eval_p6,
    "P7": eval_p7,
    "P8": eval_p8,
    "P9": eval_p9,
}


def evaluate(run: dict) -> dict:
    return {row: EVALUATORS[row](run) for row in EVALUATORS}


# ── 9 controles negativos (una mutación distinta por fila) ────────────────────
def _set_line(run: dict, key: str, needle: str, transform) -> None:
    st = run["stations"][key]
    for label in ("r1", "r2"):
        st[label]["out"] = "\n".join(
            transform(ln) if needle in ln else ln for ln in st[label]["out"].splitlines()
        )


def mut_p1(run: dict) -> None:
    """binary-copy: la fila mide un binario que no arranca (copia truncada real)."""
    dirs = run["stations"]["menu"].get("dirs") or []
    if not dirs or not BIN.is_file():
        run["stations"]["menu"]["r1"] = {
            "rc": 126, "out": "", "err": "synthetic: binario truncado",
            "window_panic": False, "sha": "", "secs": 0.0,
        }
        return
    broken = run["bin"]["scratch"] / "daggerfall-app-broken"
    data = BIN.read_bytes()
    # 4 KiB: por debajo de las cabeceras de programa, el kernel no lo puede mapear.
    # (Media copia de este binario de depuración arranca igual: corta .debug_*, no código.)
    broken.write_bytes(data[:4096])
    broken.chmod(0o755)
    run["stations"]["menu"]["r1"] = cap([broken, "--probe", "MENU-001"], dirs[0], 120)


def _mut_p1_restore(run: dict) -> None:
    broken = run["bin"]["scratch"] / "daggerfall-app-broken"
    if broken.exists():
        broken.unlink()
    run["stations"]["menu"]["r1"] = cap([BIN, "--probe", "MENU-001"], run["stations"]["menu"]["dirs"][0])


def mut_p2(run: dict) -> None:
    _set_line(run, "menu", "startup_phase", lambda ln: ln.replace("world frames 0", "world frames 3"))


def mut_p3(run: dict) -> None:
    _set_line(run, "menu", "enter_enters_world", lambda ln: ln.replace("phase=PLAYING", "phase=MENU"))


def mut_p4(run: dict) -> None:
    _set_line(run, "create", "AXIS A1 OK", lambda ln: ln.replace("surfaces=16", "surfaces=15"))


def mut_p5(run: dict) -> None:
    """forged-claim: la captura afirma M44 DONE + entrada a mazmorra sin registro que lo sostenga."""
    st = run["stations"]["dungeon"]
    for label in ("r1", "r2"):
        if "[OK] P5" not in st[label]["out"]:
            st[label]["out"] += "\n[OK] P5 - m44_register=DONE dungeon entered=1"
    run["m44"] = "AUSENTE"


def mut_p6(run: dict) -> None:
    _set_line(run, "demo", "world doors composed=", lambda ln: ln.replace("composed=817", "composed=0"))


def mut_p7(run: dict) -> None:
    """artifact: se reescribe la cabecera del fichero real de partida (artefacto del producto)."""
    for doc in run["files"]["save"]:
        path = doc["path"]
        if not isinstance(path, Path) or not path.is_file():  # fixture: misma mutación, 0 I/O
            doc["head"] = "DFL-SESSION 9"
            doc["sha"] = "9" * 64
            continue
        mutated = doc["text"].replace("DFL-SESSION 3", "DFL-SESSION 9", 1)
        doc["path"].write_bytes(mutated.encode("utf-8"))
    refresh_docs(run, "save")


def refresh_docs(run: dict, key: str) -> None:
    """Relee el artefacto del disco: la fila mide bytes, nunca el recuerdo del dict."""
    for doc in run["files"][key]:
        path = doc["path"]
        if not isinstance(path, Path) or not path.is_file():
            continue
        data = path.read_bytes()
        text = data.decode("utf-8", "replace")
        doc.update(
            exists=True,
            size=len(data),
            sha=sha256_bytes(data),
            head=text.splitlines()[0] if text.splitlines() else "",
            text=text,
        )


def mut_p8(run: dict) -> None:
    _set_line(
        run,
        "load",
        "step 2 —",
        lambda ln: ln.replace("identical roundtrip: true", "identical roundtrip: false"),
    )


def mut_p9(run: dict) -> None:
    """forged-claim: se afirma la vuelta al menú sin la secuencia PLAYING→menu."""
    st = run["stations"]["menu"]
    for label in ("r1", "r2"):
        st[label]["out"] = "\n".join(
            ln for ln in st[label]["out"].splitlines() if "SHELL VE: menu state=Menu" not in ln
        )
        st[label]["out"] += "\n[OK] P9 - back_to_menu=1"


FALSIFIERS = {
    "p1-binary-copy-truncated": ("P1", mut_p1, "binary-copy"),
    "p2-menu-not-frozen": ("P2", mut_p2, "observation"),
    "p3-enter-stays-menu": ("P3", mut_p3, "observation"),
    "p4-create-surfaces-15": ("P4", mut_p4, "observation-degradation"),
    "p5-forged-green": ("P5", mut_p5, "forged-claim"),
    "p6-world-doors-zero": ("P6", mut_p6, "observation"),
    "p7-save-doc-version": ("P7", mut_p7, "artifact"),
    "p8-roundtrip-false": ("P8", mut_p8, "observation"),
    "p9-forged-green": ("P9", mut_p9, "forged-claim"),
}


# ── fixtures (solo --self-test: prueban que los predicados son falsables) ─────
MENU_OK = """PROBE-MENU-001 - startup menu (M18-B1)
FG01-02 VE: world doors composed=817 retail=817 scenario=0 buildings=647 buildings_with_doors=629
[OK] startup_phase - phase=MENU · visible panel "main-menu" (index 3) · 63878 opaque px · HUD hidden=true · world frames 0 (the gated bundle owns the game clock)
[OK] menu_holds_without_enter - 8 frames · phase=MENU · visible panel "main-menu" · world frames 0 (the gated bundle does not run)
[OK] enter_enters_world - phase=PLAYING · visible panel "hud" (index 0) · HUD visible=true · world frames 1 (first gated frame)
[OK] world_ticks_while_playing - 3 more frames · phase=PLAYING · world frames 4
[OK] spawn_and_hud_unchanged - spawn=(411.000,1.600,411.000) yaw=3.9270 · digest ×2 HUD 9020f7d4409e430c / 9020f7d4409e430c
[OK] control_negative_enter_is_one_way - Enter while PLAYING keeps phase=PLAYING and the world running (frames 5)
VERDICT: PROBE-MENU-001 PASS - startup MENU with 0 world frames behind the gate
"""

DEMO_OK = """PROBE-DEMO-001 - M18 integrated demo
FG01-02 VE: world doors composed=817 retail=817 scenario=0 buildings=647 buildings_with_doors=629
[OK] menu_phase - 4 frames MENU with 0 world ticks · Enter ⇒ PLAYING in 1 frame (1 tick)
VERDICT: PROBE-DEMO-001 PASS - retail city Daggerfall · digest 7712d75749f73f18 ×2
"""

CREATE_OK = """PROBE-UI-CREATE-001 - 16 surfaces (M38/UI-2)
""" + "\n".join(f"SURFACE C{i:02d} CreateCharPage{i}" for i in range(1, 17)) + """
AXIS A1 OK surfaces=16 nodes=99 owners-open=16/16 double-measure=agree
AXIS A6 OK intents=100 delivered=84 double-delivery=0 depth-policy=ok
DIGEST 242aeff843648ed55835d490f0c6ea2a7833173cd89f15d5d6a2f4191a8af7b1
VERDICT: PASS
"""

DUNGEON_OK = """PROBE-DUNGEON-001 - M34 dungeon geometry
index regions=45 locations=15251 dungeons=4232 without_dungeon=11019
SUMMARY PROBE-DUNGEON-001: passed=12/12 ⇒ PASS EXIT=0
"""

SAVE_OK = """PROBE-SESSION-001 - M8 played session save/load roundtrip
[OK] step 1 — played session SAVED to <SCRATCH>/run/saves/session.dfs (454 bytes, DFL-SESSION 3)
VERDICT: PROBE-SESSION-001 PASS - byte-by-byte save→load roundtrip
"""

LOAD_OK = """PROBE-SESSION-001 - M8 played session save/load roundtrip
[OK] step 1 — played session SAVED to <SCRATCH>/run/saves/session.dfs (454 bytes, DFL-SESSION 3)
[OK] step 2 — NEW app reloaded the save: RESTORED SESSION visible (identical roundtrip: true)
VERDICT: PROBE-SESSION-001 PASS - anti-false-positives (no save ⇒ NotFound · corrupt ⇒ checksum · version ⇒ rejection)
"""

MIGRATION_OK = """PROBE-SAVE-MIGRATION-001 - M33 v1→v2 migration
[OK] migration: 3/3 v1→v3 · v3→v3 idempotente 3/3 · GV-09 PASS · digest=e70fd87aed89f3d7 ×2
SUMMARY PROBE-SAVE-MIGRATION-001: migrated=PASS idempotent=PASS GV-05=PASS GV-09=PASS clock_only=PASS digest×2=PASS ⇒ PASS
"""

PROMOTIONS = {
    "menu": "\nSHELL VE: menu state=Menu items=5 selection=0 surface=main-menu",
    "create": "\nSHELL VE: create done surfaces=16 character=HX7FQ2 state=Playing",
}


def fixture(promote: bool) -> dict:
    def rec(text: str) -> dict:
        return {"rc": 0, "out": normalize(text, None), "err": "", "sha": sha256_text(text), "secs": 0.0}

    menu_text = MENU_OK + (PROMOTIONS["menu"] if promote else "")
    create_text = CREATE_OK + (PROMOTIONS["create"] if promote else "")
    stations = {
        key: {"probe": key, "r1": rec(text), "r2": rec(text), "ident": True, "dirs": []}
        for key, text in (
            ("menu", menu_text),
            ("demo", DEMO_OK),
            ("create", create_text),
            ("dungeon", DUNGEON_OK + ("\ndungeon entered=1" if promote else "")),
            ("save", SAVE_OK),
            ("load", LOAD_OK),
            ("migration", MIGRATION_OK),
        )
    }
    doc = {
        "path": Path("<fixture>"),
        "exists": True,
        "size": 454,
        "sha": "0" * 64,
        "head": "DFL-SESSION 3",
        "text": "DFL-SESSION 3\npos_x_mm=1500\n",
    }
    return {
        "stations": stations,
        "files": {"save": [dict(doc), dict(doc)], "load": [dict(doc)]},
        "bin": {"sha": "a" * 64, "sha_after": "a" * 64, "fresh": True, "scratch": Path("/tmp")},
        "m44": "DONE" if promote else "AUSENTE",
    }


def selftest() -> int:
    failures = []
    print("M-PLAY 001 · SELF-TEST (fixtures sintéticas; prueban falsabilidad, no el producto)")

    promoted = fixture(True)
    base = evaluate(promoted)
    for row in EVALUATORS:
        if base[row][0] != "PASS":
            failures.append(f"fixture-promote {row}={base[row][0]} ({base[row][1]})")
    print(f"SELFTEST fixture-promote green={sum(1 for v in base.values() if v[0] == 'PASS')}/9")

    honest_v = evaluate(fixture(False))
    counts = {v: sum(1 for x in honest_v.values() if x[0] == v) for v in ("PASS", "FAIL", "NOT-RUN")}
    print(
        f"SELFTEST fixture-product green={counts['PASS']} red={counts['FAIL']} "
        f"not-run={counts['NOT-RUN']} ids="
        + ",".join(k for k, v in honest_v.items() if v[0] != "PASS")
    )
    if counts["PASS"] != 6 or counts["NOT-RUN"] != 3 or counts["FAIL"] != 0:
        failures.append(f"fixture-product counts={counts}")
    for row, want in (("P4", "NOT-RUN"), ("P5", "NOT-RUN"), ("P9", "NOT-RUN")):
        if honest_v[row][0] != want:
            failures.append(f"fixture-product {row}={honest_v[row][0]} != {want}")

    caught, signatures = 0, set()
    for name, (row, mutator, kind) in FALSIFIERS.items():
        mutated = fixture(True)
        mutator(mutated)
        after_all = evaluate(mutated)
        restored_all = evaluate(fixture(True))
        signature = sha256_text(json.dumps({k: v[0] for k, v in after_all.items()}, sort_keys=True))
        signatures.add(signature)
        cascade = [k for k in after_all if k != row and after_all[k] != base[k]]
        clean_restored = all(restored_all[k] == base[k] for k in EVALUATORS)
        ok = after_all[row][0] == "FAIL" and clean_restored
        caught += 1 if ok else 0
        print(
            f"SELFTEST falsify {name} kind={kind} row={row} "
            f"clean={base[row][0]} mutated={after_all[row][0]} restored={restored_all[row][0]} "
            f"cascade={','.join(cascade) or '-'} ⇒ {'OK' if ok else 'FALLO'}"
        )
        if not ok:
            failures.append(
                f"falsify {name} mutated={after_all[row][0]} restored-clean={int(clean_restored)}"
            )

    print(
        f"SELFTEST rows=9 falsifiers={len(FALSIFIERS)} caught={caught} "
        f"distinct_mutations={len(signatures)}"
    )
    if len(signatures) != 9:
        failures.append(f"distinct_mutations={len(signatures)}!=9")

    for failure in failures:
        print(f"FAIL SELFTEST: {failure}")
    print("SELFTEST VERDICT: " + ("PASS" if not failures else "FAIL"))
    return 0 if not failures else 1


# ── corrida real ─────────────────────────────────────────────────────────────
def real_run(args) -> int:
    if not BIN.is_file() or not os.access(BIN, os.X_OK):
        print(f"CAUSE environment bin=absent ({BIN})")
        print("VERDICT: NC")
        return 2

    bin_sha, window_ok = fingerprint(BIN)
    stale = BIN.stat().st_mtime < newest_source_mtime()
    # `fresh` incluye la feature: un binario recién construido SIN `window` no es el producto que
    # este recorrido declara medir (P7/P8 caerían por el feature set, no por el producto) y deja
    # `target/debug` degradado para los gates que lo leen sin construir
    # (`menu-window-001 --skip-build`, `shell-002`).
    fresh = (not stale) and window_ok
    if not fresh and not args.no_build:
        rc, tail = build_binary()
        if rc != 0:
            print(f"CAUSE environment build=FAILED rc={rc} tail={tail!r}")
            print("VERDICT: NC")
            return 2
        bin_sha, window_ok = fingerprint(BIN)
        if not window_ok:
            print(
                "CAUSE build=feature-window-absent (el binario construido no compiló m8_window: "
                f"falta `--features {','.join(BUILD_FEATURES)}`; target/debug queda degradado "
                "para menu-window-001 --skip-build / shell-002)"
            )
            print("VERDICT: NC")
            return 2
        fresh = BIN.stat().st_mtime >= newest_source_mtime()
    if not fresh and args.no_build:
        if window_ok:
            print("CAUSE bin-stale=1 (crates/**/*.rs más nuevos que el binario, --no-build)")
        else:
            print(
                "CAUSE bin-feature-window=absent (el binario en disco no lleva m8_window, "
                "--no-build: no se puede medir el producto `window`)"
            )
        print("VERDICT: NC")
        return 3

    dirty, head, paths = tree_state()
    if dirty and args.strict_tree:
        print(f"CAUSE tree=crates_dirty:{dirty} ({', '.join(paths)})")
        print("VERDICT: NC")
        return 4

    m44, m44_label = m44_verdict()
    scratch = Path(tempfile.mkdtemp(prefix="mplay-001-bin-"))
    keep = Path(args.keep).resolve() if args.keep else None

    lines = []
    lines.append(f"M-PLAY 001 · {CR} · journey {JOURNEY}")
    lines.append(f"JOURNEY order={','.join(EXEC_ORDER)}")
    lines.append(
        f"TREE crates_status={'clean' if dirty == 0 else f'dirty:{dirty}'} head={head} "
        f"bin_sha256={bin_sha[:16]} bin_fresh=1"
    )

    run = run_journey(keep)
    run["bin"] = {"sha": bin_sha, "sha_after": fingerprint(BIN)[0], "fresh": fresh, "scratch": scratch}
    run["m44"] = m44
    if dirty:
        lines.append(f"TREE crates_paths={','.join(paths)}")

    for key in ("menu", "demo", "create", "dungeon", "save", "load", "migration"):
        st = run["stations"][key]
        lines.append(
            f"PROBE {st['probe']}:{key} runs=2 exit={st['r1']['rc']}/{st['r2']['rc']} "
            f"byte-identical={int(st['ident'])} sha256={st['r1']['sha'][:16]} lines={len(st['r1']['out'].splitlines())}"
        )

    for key in ("save", "load"):
        docs = run["files"][key]
        agree = int(len(docs) == 2 and docs[0]["sha"] == docs[1]["sha"])
        lines.append(
            f"ARTIFACT {key} path={SAVE_REL} exists={int(docs[0]['exists'])} "
            f"head={docs[0]['head']!r} size={docs[0]['size']} sha256={docs[0]['sha'][:16]} "
            f"sha-agree={agree if key == 'save' else 'n/a'}"
        )
    lines.append(f"M44 register={m44} label={m44_label} source={REGISTER.relative_to(REPO)}")
    lines.append("ENV display=unset probes=headless retail=" + ("present" if retail_present() else "absent"))

    verdicts = evaluate(run)
    for row, station, anchor in ROWS:
        verdict, detail = verdicts[row]
        lines.append(f"ROW {row} {verdict} station={station} anchor={anchor} · {detail}")

    panics = [
        key for key, st in run["stations"].items() if st["r1"].get("window_panic") or st["r2"].get("window_panic")
    ]
    if panics:
        lines.append(
            "FINDING F1 session_window_headless_panic=1 stations="
            + ",".join(panics)
            + " (m8_window::run_windowed_m8 tras el veredicto · winit sin DISPLAY · rc=101 · stdout completo)"
        )

    green = sum(1 for v, _ in verdicts.values() if v == "PASS")
    red = sum(1 for v, _ in verdicts.values() if v == "FAIL")
    missing = sum(1 for v, _ in verdicts.values() if v == "NOT-RUN")
    failed = len(verdicts) - green - red - missing
    missing_ids = ",".join(k for k, (v, _) in verdicts.items() if v == "NOT-RUN")
    lines.append(
        f"CHECKS total={len(verdicts)} green={green} red={red} missing={missing} failed={failed} "
        f"missing-ids={missing_ids or '-'}"
    )
    if dirty:
        lines.append(
            f'MEASUREMENT PROVISIONAL causa="árbol compartido en escritura ({dirty} rutas crates/ sin commitear)"'
        )
    else:
        lines.append("MEASUREMENT FINAL tree=crates_clean")

    body = "\n".join(lines)
    verdict = "PASS" if green == len(verdicts) else "FAIL"
    print(body)
    print(f"VERDICT: {verdict}")
    print(f"DIGEST sha256:{sha256_text(body + chr(10) + 'VERDICT: ' + verdict)}")

    if keep is not None:
        print(f"SCRATCH kept={keep}")
    return 0 if verdict == "PASS" else 1


# ── falsify (par probado sobre la corrida real) ───────────────────────────────
def falsify(names: list[str], args) -> int:
    dirty, head, _paths = tree_state()
    bin_sha = fingerprint(BIN)[0]
    scratch = Path(tempfile.mkdtemp(prefix="mplay-001-falsify-"))
    run = run_journey(None)
    run["bin"] = {"sha": bin_sha, "sha_after": bin_sha, "fresh": True, "scratch": scratch}
    run["m44"] = m44_verdict()[0]
    base_all = evaluate(run)
    backups = {
        id(doc): doc["path"].read_bytes()
        for doc in run["files"]["save"]
        if isinstance(doc["path"], Path) and doc["path"].is_file()
    }

    print(
        f"M-PLAY 001 · FALSIFY {len(names)} controles · tree=crates_"
        f"{'clean' if dirty == 0 else f'dirty:{dirty}'} head={head} bin_sha256={bin_sha[:16]}"
    )
    failed = 0
    for name in names:
        row, mutator, kind = FALSIFIERS[name]
        base = base_all[row]
        target = copy.deepcopy(run)
        mutator(target)
        after = evaluate(target)[row]
        for doc in run["files"]["save"]:  # restaurar el artefacto real, byte a byte
            if id(doc) in backups:
                doc["path"].write_bytes(backups[id(doc)])
        refresh_docs(run, "save")
        refresh_docs(run, "load")
        if row == "P1":
            _mut_p1_restore(run)
            base = evaluate(run)[row]
        restored = evaluate(run)[row]
        ok = after[0] == "FAIL" and restored[0] == base[0]
        failed += 0 if ok else 1
        print(f"FALSIFY {name} kind={kind} row={row} clean={base[0]} mutated={after[0]} restored={restored[0]}")
        print(f"FALSIFY   clean-detail={base[1]}")
        print(f"FALSIFY   mutated-detail={after[1]}")
        if row == "P7":
            shas = [doc["sha"] for doc in run["files"]["save"]]
            print(f"FALSIFY   artifact-restored sha256={shas[0][:16]} agree={int(shas[0] == shas[1])}")
        print(f"FALSIFY {name} VERDICT: " + ("PASS" if ok else "FAIL"))
    print(f"FALSIFY total={len(names)} pass={len(names) - failed} fail={failed}")
    return 0 if failed == 0 else 1


def main() -> int:
    parser = argparse.ArgumentParser(description="M-PLAY: jugabilidad de punta a punta")
    parser.add_argument("--no-build", action="store_true", help="no construye el binario")
    parser.add_argument("--strict-tree", action="store_true", help="exit 4 si crates/ tiene escrituras ajenas")
    parser.add_argument("--self-test", action="store_true", help="9 controles negativos con fixtures")
    parser.add_argument(
        "--falsify",
        nargs="+",
        metavar="FALSIFIER",
        choices=sorted(FALSIFIERS),
        help="controles negativos limpio→rojo→restaurado (una sola corrida real)",
    )
    parser.add_argument("--keep", metavar="DIR", help="conserva scratch y capturas en DIR")
    args = parser.parse_args()

    if args.self_test:
        return selftest()
    if args.falsify:
        return falsify(args.falsify, args)
    return real_run(args)


if __name__ == "__main__":
    sys.exit(main())
