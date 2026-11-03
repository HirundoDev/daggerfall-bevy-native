#!/usr/bin/env python3
"""Mide que hace `NEW GAME` DESPUES de haber jugado: mundo nuevo o reanudacion.

CR-178183d7 (lane NewGameSemantics, 2026-10-31). La transicion menu -> mundo ->
menu ya esta medida (`scripts/menu-transition-capture.py`); lo que este script
decide es la SEMANTICA de la segunda entrada:

  1. arranca `target/debug/daggerfall-app` (ya construido con `--features window`);
  2. `Return` (new-game) + `Return` (confirmar el wizard) -> PLAYING #1;
  3. JUEGA: mantiene `W` unos segundos (el avatar se mueve y el reloj de juego
     avanza 1 min de juego por segundo real, `TIME_SCALE_GAME_SECONDS_PER_REAL_SECOND`);
  4. viaja rapido (`M` + `T`) para consumir minutos del reloj de SESION y mover
     el avatar sobre el mapa (estado que un mundo nuevo debe deshacer);
  5. `Escape` (pausa) -> `S` + `Return` (guarda el slot) -> `b` (menu);
  6. `Return` + `Return` -> PLAYING #2 (NEW GAME otra vez);
  7. `Escape` -> `b` -> `Down` + `Return` (CONTINUE) -> PLAYING #3.

Firma de cada entrada, leida del log (nada inferido):

  * `world_ticks` de `SHELL VE: new_game … world_ticks=N`;
  * `clock` de `M18 VE: sim ticks=N … clock M min` (reloj de juego real);
  * `sim ticks` (frames del mundo desde el arranque del proceso);
  * `pos`/`yaw` del primer `M19 VE: pos=(…)` de la entrada;
  * `character` de `SHELL VE: session … character=…`;
  * `hud` digest y `world scene=/location=` de la entrada;
  * sha256 de la captura de la ventana tras ESTABILIZAR la imagen.

ESTABILIDAD DE LA CAPTURA (CR-178183d7, lane InstrumentStability): `is_world`
solo dice que el frame PARECE un mundo; un frame a medio pintar tambien lo
parece (medido: 80f7c08c con 1614 colores frente a los 2176 del frame final,
136 923 px distintos). Por eso cada captura se acepta solo si `is_world`,
la marca de fase `SHELL VE: world scene=` ya existe y la imagen se repite igual
`SETTLE_STABLE_CAPTURES` capturas consecutivas (mismo sha256). Cada frame
medido queda en `frames.txt` y los PNG en `frames/`; cada corrida agrega una
linea a `runs.jsonl` (sha256 de las capturas, igualdad y veredictos).

Uso:  python3 scripts/new-game-semantics.py [--label pre-fix]
      (cada corrida escribe en .sintergic/artifacts/new-game-semantics/<label>/)
      python3 scripts/new-game-semantics.py --label negative --replay a.png,b.png
      (prueba negativa: reevalua capturas grabadas con la regla vieja y la nueva)

VEREDICTO:
  * `RESUMES`  — la entrada #2 conserva reloj/posicion de la sesion anterior
                 (o la captura no coincide con la primera creacion);
  * `FRESH`    — reloj, ticks, posicion y captura coinciden con la primera
                 creacion, y el slot no se reutiliza;
  * `CONTINUE-PRESERVES` / `CONTINUE-LOST` — `continue` restaura los minutos
                 guardados (`load done … ticks=N`) o no.

Uso:  python3 scripts/new-game-semantics.py [--label pre-fix]
      (cada corrida escribe en .sintergic/artifacts/new-game-semantics/<label>/)
"""
from __future__ import annotations

import argparse
import hashlib
import json
import math
import os
import re
import shutil
import subprocess
import sys
import tempfile
import time
from pathlib import Path

from PIL import Image, ImageChops
from Xlib import X, XK, display as xdisplay
from Xlib.protocol import event as xevent

ROOT = Path(__file__).resolve().parent.parent
BIN = ROOT / "target" / "debug" / "daggerfall-app"
DATA = Path(os.environ.get("DF_APP_ROOT") or (Path(os.environ.get("HOME", "")) / "Games" / "Daggerfall" / "app"))
OUT_ROOT = ROOT / ".sintergic" / "artifacts" / "new-game-semantics"
TITLE_TOKEN = "DAGGERFALL"
DISPLAY = os.environ.get("DISPLAY", ":0")
CAPTURE_TIMEOUT = 10.0

WINDOW_W, WINDOW_H = 1280, 720
FIELD_RGB = (23, 23, 23)
PAPER_RGB = (220, 220, 220)
BAR_RGB = (44, 48, 49)
BAR_COLUMNS = (500, 700)
WORLD_MIN_DISTINCT = 500
WORLD_MAX_PAPER = 0.01
WORLD_MAX_FIELD = 0.20

# ── estabilidad de la captura (CR-178183d7, lane InstrumentStability) ────────
# `is_world` solo dice que el frame PARECE un mundo: un frame a medio pintar lo
# satisface (medido: `signoff/world-first.png` = 80f7c08c, 1614 colores, campo
# 0.21%, papel 0%, sin barras). La aceptacion exige estabilidad de PIXEL:
# `SETTLE_STABLE_CAPTURES` capturas CONSECUTIVAS con el mismo sha256 separadas
# por `SETTLE_INTERVAL`. 3 capturas = la imagen aguanto >= 2 intervalos sin
# cambiar; el pincel no sobrevive a eso (ver `frames.txt` de cada corrida).
SETTLE_STABLE_CAPTURES = 3
SETTLE_INTERVAL = 0.15
SETTLE_KEEP_FRAMES = 40
SETTLE_WORLD_MARKER = "SHELL VE: world scene="

RE_NEW_GAME = re.compile(r"SHELL VE: new_game state=Playing item=new-game world_ticks=(\d+)")
RE_SIM = re.compile(r"M18 VE: sim ticks=(\d+) phase=(\w+) shell=(\w+) world=(\w+) · clock (\d+) min")
RE_POS = re.compile(
    r"M19 VE: pos=\(([-\d.]+),([-\d.]+),([-\d.]+)\) yaw=([-\d.]+) · mode=(\w+) · speed=([-\d.]+)"
)
RE_SESSION = re.compile(r"SHELL VE: session state=Started character=(\S+) save=(\S+)")
RE_CREATE_DONE = re.compile(r"SHELL VE: create done surfaces=(\d+) character=(\S+) state=Playing")
RE_WORLD = re.compile(
    r"SHELL VE: world scene=(\S+) location=\((\d+),(\d+),(\d+)\) buildings=(\d+) npcs=(\d+) panels=(\d+)"
)
RE_HUD = re.compile(r"SHELL VE: hud visible=([01]) panel=(\S+) opaque=(\d+) digest=([0-9a-f]+)")
RE_PAUSE = re.compile(r"SHELL VE: pause state=Paused ticks=(\d+)")
RE_SAVE = re.compile(r"SHELL VE: save slot=(\d+) format=(\S+) bytes=(\d+) digest=([0-9a-f]+)")
RE_LOAD_DONE = re.compile(r"SHELL VE: load done slot=(\d+) format=(\S+) state=Playing ticks=(\d+) character=(\S+)")
RE_CONTINUE = re.compile(r"SHELL VE: (continue result=\w+.*)")
RE_RESET = re.compile(
    r"M18 VE: NEW GAME reset — career=(\S+) · clock=(\d+) min · sim ticks=(\d+) · "
    r"hud digest ([0-9a-f]+)→([0-9a-f]+) · pos=\(([-\d.]+),([-\d.]+),([-\d.]+)\) yaw=([-\d.]+) · "
    r"scene=(\S+) · guards=(\d+) · stage entities despawned=(\d+) · billboards despawned=(\d+) · "
    r"npcs restored=(\d+)"
)
RE_TRAVEL = re.compile(r"SHELL VE: travel (\S+)")
# CR-7a3e9c14: lo que el SLOT declara al escribirse (generacion 4) y lo que la
# ventana aplica al reanudar. Son las dos medidas del round-trip: si el slot no
# declara mundo (fidelity=3) no hay renglon que leer y se dice `SIN LINEA`.
RE_FIDELITY = re.compile(
    r"SHELL VE: slot fidelity=(\d+) index=(\d+) pos=\((-?\d+),(-?\d+),(-?\d+)\) "
    r"yaw=(-?[\d.]+) pitch=(-?[\d.]+) world_minutes=(\d+) environment=(-?\d+) "
    r"map_id=(-?\d+)"
)
RE_RESUME = re.compile(
    r"M18 VE: SHELL resume — slot (\d+) world restored pos_mm=\((-?\d+),(-?\d+),(-?\d+)\) "
    r"yaw_mrad=(-?\d+) world_minutes=(\d+) environment=(-?\d+)"
)
RE_WARNING = re.compile(r"SHELL VE: (warning W-SAVE-\d+ .*)")


# ── ventana X ───────────────────────────────────────────────────────────────

def walk(d, win, out):
    out.append(win)
    try:
        for child in win.query_tree().children:
            walk(d, child, out)
    except Exception:
        pass
    return out


def name_of(win) -> str:
    try:
        name = win.get_wm_name()
    except Exception:
        return ""
    if isinstance(name, bytes):
        return name.decode("utf-8", "replace")
    return name or ""


def find_window(d, deadline: float, proc):
    while time.monotonic() < deadline:
        for win in walk(d, d.screen().root, []):
            if TITLE_TOKEN in name_of(win).upper():
                return win
        if proc.poll() is not None:
            return None
        time.sleep(0.25)
    return None


def focus(d, win) -> bool:
    for _ in range(20):
        try:
            win.set_input_focus(X.RevertToParent, X.CurrentTime)
            d.sync()
            got = d.get_input_focus().focus
            if getattr(got, "id", None) == win.id:
                return True
        except Exception:
            pass
        time.sleep(0.2)
    return False


CHANNEL = {"name": "xtest"}


def keycode(d, keysym: str) -> int:
    code = d.keysym_to_keycode(XK.string_to_keysym(keysym))
    if not code:
        raise SystemExit("no keycode for keysym %r" % keysym)
    return code


def xtest_key(d, keysym: str, down: bool) -> None:
    from Xlib.ext import xtest

    xtest.fake_input(d, X.KeyPress if down else X.KeyRelease, keycode(d, keysym))
    d.sync()


def send_key(d, win, keysym: str, down: bool) -> None:
    cls = xevent.KeyPress if down else xevent.KeyRelease
    win.send_event(
        cls(time=X.CurrentTime, root=d.screen().root, window=win, same_screen=1, child=X.NONE,
            root_x=0, root_y=0, event_x=0, event_y=0, state=0, detail=keycode(d, keysym)),
        event_mask=X.KeyPressMask | X.KeyReleaseMask,
    )
    d.sync()


def key_down(d, win, keysym: str) -> None:
    if CHANNEL["name"] == "xtest":
        xtest_key(d, keysym, True)
    else:
        send_key(d, win, keysym, True)


def key_up(d, win, keysym: str) -> None:
    if CHANNEL["name"] == "xtest":
        xtest_key(d, keysym, False)
    else:
        send_key(d, win, keysym, False)


def press(d, win, keysym: str, hold: float = 0.12, gap: float = 0.60) -> None:
    key_down(d, win, keysym)
    time.sleep(hold)
    key_up(d, win, keysym)
    time.sleep(gap)


# ── captura y firma de pantalla ─────────────────────────────────────────────

def capture(win, png: Path) -> None:
    if png.exists():
        png.unlink()
    try:
        rc = subprocess.run(
            [shutil.which("import"), "-display", DISPLAY, "-window", hex(win.id), str(png)],
            capture_output=True, text=True, timeout=CAPTURE_TIMEOUT,
        )
    except subprocess.TimeoutExpired:
        # La ventana desaparecio entre el `focus` y la captura: sin tope, `import`
        # se queda esperando para siempre y el plazo de fase nunca se evalua.
        raise SystemExit("import colgado >%.1fs (ventana %s perdida)" % (CAPTURE_TIMEOUT, hex(win.id)))
    if rc.returncode != 0 or not png.is_file() or png.stat().st_size == 0:
        raise SystemExit("import fallo rc=%d %s" % (rc.returncode, (rc.stderr or rc.stdout)[:200]))


def bar_bands(img, column: int):
    bands = []
    for y in range(img.height):
        if img.getpixel((column, y)) == BAR_RGB:
            if bands and y - bands[-1][1] <= 1:
                bands[-1][1] = y
            else:
                bands.append([y, y])
    return [(a, b) for a, b in bands]


def measure(png: Path) -> dict:
    img = Image.open(png).convert("RGB")
    img.load()
    pixels = list(img.get_flattened_data())
    total = len(pixels)
    return {
        "size": img.size,
        "sha256": hashlib.sha256(png.read_bytes()).hexdigest(),
        "distinct": len(set(pixels)),
        "field": sum(1 for p in pixels if p == FIELD_RGB) / total,
        "paper": sum(1 for p in pixels if p == PAPER_RGB) / total,
        "bands": {column: bar_bands(img, column) for column in BAR_COLUMNS},
    }


def is_world(m: dict) -> bool:
    return (
        m["size"] == (WINDOW_W, WINDOW_H) and m["distinct"] >= WORLD_MIN_DISTINCT
        and m["paper"] <= WORLD_MAX_PAPER and m["field"] <= WORLD_MAX_FIELD
        and not m["bands"].get(500) and not m["bands"].get(700)
    )


def settle_core(win, png: Path, deadline: float, tag: str, fatal: bool,
                frames_log: Path | None = None, frames_dir: Path | None = None,
                log_path: Path | None = None, marker_count: int | None = None) -> dict:
    """Captura la ventana hasta que la imagen sea ESTABLE, no solo «world-like».

    Defecto que corrige, medido (CR-178183d7, lane InstrumentStability): el
    criterio viejo aceptaba el PRIMER frame que pasaba `is_world`; a mitad de
    pintado el frame ya cumple distinct>=500 / paper<=1% / field<=20% / sin
    barras, pero el sha256 cambia cuando el pincel termina (80f7c08c ->
    328b363b, 136 923 px distintos). Aceptar ese frame hacia que la igualdad de
    capturas —y con ella el veredicto RESUMES/FRESH— dependiera del instante
    de la toma.

    Criterio de aceptacion (los tres obligatorios; ninguno relaja `is_world`):
      1. `is_world(m)`;
      2. la marca de fase `SHELL VE: world scene=` ya se emitio `marker_count`
         veces (el mundo existe tambien en el log);
      3. racha de `SETTLE_STABLE_CAPTURES` capturas CONSECUTIVAS con el mismo
         sha256: la imagen no cambio durante >= 2 intervalos.
    Un frame intermedio no puede cumplir (3): su captura siguiente tiene otro
    sha y reinicia la racha (ver `frames.txt` de cada corrida).
    """
    t0 = time.monotonic()
    n = 0
    streak = 0
    prev_sha = None
    rejected_world = 0
    while True:
        n += 1
        try:
            capture(win, png)
        except SystemExit as exc:
            if fatal:
                raise
            # `try_settle_world` contrato: publicar lo medido, no abortar. La
            # ventana desaparecio a mitad de fase; la captura colgada no puede
            # tumbar una corroboracion que se declara no-fatal.
            if frames_log is not None:
                with frames_log.open("a", encoding="utf-8") as fh:
                    fh.write("%s n=%d HUNG %s\n" % (tag, n, exc))
            return {
                "sha256": "-", "size": None, "distinct": 0, "field": 1.0, "paper": 1.0,
                "bands": {}, "frames": n - 1, "rejected_world": rejected_world,
                "stable": streak, "accepted": False, "hung": True,
            }
        m = measure(png)
        world = is_world(m)
        marker_ok = True
        if marker_count is not None and log_path is not None:
            marker_ok = log_text(log_path).count(SETTLE_WORLD_MARKER) >= marker_count
        if world and prev_sha is not None and m["sha256"] == prev_sha:
            streak += 1
        else:
            streak = 1 if world else 0
        prev_sha = m["sha256"] if world else None
        accepted = bool(world and marker_ok and streak >= SETTLE_STABLE_CAPTURES)
        if world and not accepted:
            rejected_world += 1
        if frames_log is not None:
            with frames_log.open("a", encoding="utf-8") as fh:
                fh.write(
                    "%s n=%d sha=%s world=%d marker=%d streak=%d accepted=%d distinct=%d "
                    "field=%.4f paper=%.4f bands=%d t=%.3f\n"
                    % (tag, n, m["sha256"], world, marker_ok, streak, accepted, m["distinct"],
                       m["field"], m["paper"],
                       int(bool(m["bands"].get(500) or m["bands"].get(700))),
                       time.monotonic() - t0))
        if frames_dir is not None and n <= SETTLE_KEEP_FRAMES:
            frames_dir.mkdir(parents=True, exist_ok=True)
            (frames_dir / ("%s-%02d-%s.png" % (tag, n, m["sha256"][:8]))).write_bytes(png.read_bytes())
        m["frames"] = n
        m["rejected_world"] = rejected_world
        m["stable"] = streak
        m["accepted"] = accepted
        if accepted:
            return m
        if time.monotonic() >= deadline:
            if fatal:
                raise SystemExit(
                    "CAPTURA no estabilizo (%s): frames=%d sha=%s size=%s colores=%d campo=%.2f%% "
                    "papel=%.2f%% racha=%d umbral=%d marca=%s"
                    % (tag, n, m["sha256"], m["size"], m["distinct"], m["field"] * 100,
                       m["paper"] * 100, streak, SETTLE_STABLE_CAPTURES, marker_ok))
            return m
        time.sleep(SETTLE_INTERVAL)


def settle_world(win, png: Path, deadline: float, tag: str = "settle",
                 frames_log: Path | None = None, frames_dir: Path | None = None,
                 log_path: Path | None = None, marker_count: int | None = None) -> dict:
    """Acepta solo una captura pixel-estable; aborta si no la consigue."""
    return settle_core(win, png, deadline, tag, True, frames_log, frames_dir, log_path, marker_count)


def try_settle_world(win, png: Path, deadline: float, tag: str = "settle",
                     frames_log: Path | None = None, frames_dir: Path | None = None,
                     log_path: Path | None = None, marker_count: int | None = None) -> dict:
    """Como `settle_world` pero devuelve la ultima medida si no estabiliza.

    La captura de CONTINUE es evidencia adicional: si no hay mundo estable (p. ej.
    sigue el menu) se publica lo medido —con `stable` < umbral—, no se aborta.
    """
    return settle_core(win, png, deadline, tag, False, frames_log, frames_dir, log_path, marker_count)


# ── replay negativo de la regla de estabilidad ──────────────────────────────

def replay(paths: list[Path], out: Path) -> int:
    """Reevalua una secuencia de capturas con la regla vieja y la nueva.

    Prueba negativa: un frame que la regla vieja aceptaba (primer `is_world`
    satisfecho, p. ej. el frame a medio pintar) debe quedar RECHAZADO por la
    nueva si no tiene `SETTLE_STABLE_CAPTURES` consecutivas identicas.
    """
    frames = []
    for p in paths:
        if not p.is_file():
            raise SystemExit("replay: falta %s" % p)
        m = measure(p)
        m["path"] = p
        frames.append(m)
    lines = [
        "═" * 118,
        "REPLAY de la regla de estabilidad · CR-178183d7 · umbral %d capturas consecutivas "
        "identicas · %d frames" % (SETTLE_STABLE_CAPTURES, len(frames)),
        "═" * 118,
    ]
    streak, prev = 0, None
    old_accept, new_accept = None, None
    for i, m in enumerate(frames, 1):
        world = is_world(m)
        if world and prev is not None and m["sha256"] == prev:
            streak += 1
        else:
            streak = 1 if world else 0
        prev = m["sha256"] if world else None
        if old_accept is None and world:
            old_accept = i
        if new_accept is None and world and streak >= SETTLE_STABLE_CAPTURES:
            new_accept = i
        lines.append("n=%-2d %-30s sha=%s world=%d racha=%d distinct=%-5d campo=%.2f%% papel=%.2f%%"
                     % (i, m["path"].name, m["sha256"][:16], world, streak, m["distinct"],
                        m["field"] * 100, m["paper"] * 100))
    lines.append("-" * 118)
    if old_accept is not None:
        lines.append("REGLA-VIEJA (primer `is_world`): ACEPTA el frame #%d (%s)"
                     % (old_accept, frames[old_accept - 1]["sha256"][:16]))
    else:
        lines.append("REGLA-VIEJA: ningun frame pasa `is_world`")
    if new_accept is not None:
        lines.append("REGLA-NUEVA (%d identicas consecutivas): ACEPTA el frame #%d (%s)"
                     % (SETTLE_STABLE_CAPTURES, new_accept, frames[new_accept - 1]["sha256"][:16]))
    else:
        lines.append("REGLA-NUEVA (%d identicas consecutivas): RECHAZA los %d frames — ninguno "
                     "tiene %d capturas consecutivas identicas"
                     % (SETTLE_STABLE_CAPTURES, len(frames), SETTLE_STABLE_CAPTURES))
    if old_accept is not None and new_accept is not None and new_accept != old_accept:
        ia = Image.open(frames[old_accept - 1]["path"]).convert("RGB")
        ib = Image.open(frames[new_accept - 1]["path"]).convert("RGB")
        if ia.size == ib.size:
            diff = ImageChops.difference(ia, ib)
            px = sum(1 for p in diff.get_flattened_data() if p != (0, 0, 0))
            lines.append("px distintos entre el frame que aceptaba la regla vieja (#%d) y el que "
                         "acepta la nueva (#%d): %d" % (old_accept, new_accept, px))
    negative_ok = old_accept is not None and (new_accept is None or new_accept > old_accept)
    lines.append("VERDICT: NEGATIVE-TEST-%s — %s"
                 % ("PASS" if negative_ok else "FAIL",
                    "el frame que la regla vieja acepta queda RECHAZADO por la regla nueva"
                    if negative_ok else "la regla nueva acepta el mismo frame que la vieja"))
    report = "\n".join(lines)
    out.mkdir(parents=True, exist_ok=True)
    (out / "report.txt").write_text(report + "\n", encoding="utf-8")
    print(report)
    return 0 if negative_ok else 1


# ── log ─────────────────────────────────────────────────────────────────────

def log_text(path: Path) -> str:
    try:
        return path.read_text(errors="replace")
    except FileNotFoundError:
        return ""


def wait_for(path: Path, predicate, deadline: float, proc):
    while time.monotonic() < deadline:
        if predicate(log_text(path)):
            return True
        if proc.poll() is not None:
            return False
        time.sleep(0.15)
    return False


def wait_line(path: Path, needle: str, deadline: float, proc) -> bool:
    return wait_for(path, lambda text: needle in text, deadline, proc)


def wait_count(path: Path, needle: str, n: int, deadline: float, proc) -> bool:
    return wait_for(path, lambda text: text.count(needle) >= n, deadline, proc)


def entry_signature(text: str, nth: int) -> dict:
    """Firma de la entrada `nth` (1-based): lineas desde su `create done`.

    El corte es `m.start()`, no `m.end()` (CR-178183d7): la propia linea
    `SHELL VE: create done surfaces=N character=<token> state=Playing` lleva el
    token del personaje, y cortar en `m.end()` la dejaba FUERA de la firma — la
    entrada #2 declaraba `character=None` por el instrumento, no por el mundo.
    """
    marks = [m.start() for m in re.finditer(r"SHELL VE: create done ", text)]
    if len(marks) < nth:
        raise SystemExit("el log no tiene la entrada #%d (create done encontrados: %d)" % (nth, len(marks)))
    start = marks[nth - 1]
    chunk = text[start:]
    sig = {"entry": nth}
    m = RE_NEW_GAME.search(chunk)
    sig["world_ticks"] = int(m.group(1)) if m else None
    m = RE_SIM.search(chunk)
    if m:
        sig["sim_ticks"] = int(m.group(1))
        sig["clock_min"] = int(m.group(5))
        sig["world"] = m.group(4)
    m = RE_POS.search(chunk)
    if m:
        sig["pos"] = [float(m.group(1)), float(m.group(2)), float(m.group(3))]
        sig["yaw"] = float(m.group(4))
    m = RE_SESSION.search(chunk)
    if m:
        sig["character"] = m.group(1)
        sig["save"] = m.group(2)
    else:
        done = RE_CREATE_DONE.search(chunk)
        if done:
            sig["character"] = done.group(2)
    m = RE_WORLD.search(chunk)
    if m:
        sig["scene"] = m.group(1)
        sig["location"] = tuple(int(m.group(i)) for i in (2, 3, 4))
    m = RE_HUD.search(chunk)
    if m:
        # `SHELL VE: hud` repite la foto de `Startup`: por si sola no mide la
        # segunda entrada. La linea del reset la SOBRESCRIBE con el digest real.
        sig["hud_digest"] = m.group(4)
    m = RE_RESET.search(chunk)
    if m:
        sig["career"] = m.group(1)
        sig["reset_clock"] = int(m.group(2))
        sig["reset_ticks"] = int(m.group(3))
        sig["hud_before"] = m.group(4)
        sig["hud_digest"] = m.group(5)
        sig["reset_pos"] = [float(m.group(k)) for k in (6, 7, 8)]
        sig["reset_yaw"] = float(m.group(9))
        sig["reset_scene"] = m.group(10)
        sig["reset_guards"] = int(m.group(11))
        sig["reset_stage"] = int(m.group(12))
        sig["reset_billboards"] = int(m.group(13))
        sig["reset_npcs"] = int(m.group(14))
        # El primer `M19 VE: pos=` periódico NO es necesariamente la entrada:
        # en la entrada #1 de la corrida post-fix cayó 1.8 m dentro del paseo
        # (412.8 con reset en 411.0) y en la #2 en el spawn. La posición de la
        # entrada es la que publica el propio reset en las DOS entradas; la
        # línea periódica se conserva como `pos_line` (nada se oculta).
        sig["pos_line"] = sig.get("pos")
        sig["pos"] = sig["reset_pos"]
        sig["yaw"] = sig["reset_yaw"]
    return sig


def last_match(pattern, text: str):
    found = None
    for found in pattern.finditer(text):
        pass
    return found


def signature_equal(a: dict, b: dict) -> tuple[bool, list]:
    keys = ("world_ticks", "clock_min", "sim_ticks", "character", "scene", "location",
            "hud_digest", "reset_pos", "reset_yaw", "reset_ticks", "reset_clock")
    diffs = []
    for key in keys:
        if a.get(key) != b.get(key):
            diffs.append("%s: %r != %r" % (key, a.get(key), b.get(key)))
    if a.get("pos") is not None and b.get("pos") is not None:
        drift = max(abs(x - y) for x, y in zip(a["pos"], b["pos"]))
        if drift > 0.05:
            diffs.append("pos: %s != %s (drift %.2f m)" % (a["pos"], b["pos"], drift))
    if a.get("yaw") is not None and b.get("yaw") is not None:
        if abs(a["yaw"] - b["yaw"]) > 1e-3:
            diffs.append("yaw: %.3f != %.3f" % (a["yaw"], b["yaw"]))
    return (not diffs), diffs


def fmt_pos(pos) -> str:
    return "(%s)" % ",".join("%.2f" % v for v in pos) if pos else "-"


def continue_signature(text: str) -> dict:
    """Firma de la sesion #3 (CONTINUE): lo que sigue al ultimo `continue result=`.

    El instrumento medía `continue` solo por `load done … ticks=N`; reloj y
    posicion tras reanudar no se publicaban, y por eso el gap
    `CONTINUE-SIN-POSICION` no tenia numeros. Aqui se leen del mismo log.
    """
    marks = [m.end() for m in re.finditer(r"SHELL VE: continue result=", text)]
    if not marks:
        return {}
    chunk = text[marks[-1]:]
    sig = {"entry": 3}
    m = RE_LOAD_DONE.search(chunk)
    if m:
        sig["load_ticks"] = int(m.group(3))
        sig["load_character"] = m.group(4)
    m = RE_SIM.search(chunk)
    if m:
        sig["sim_ticks"] = int(m.group(1))
        sig["clock_min"] = int(m.group(5))
        sig["world"] = m.group(4)
    m = RE_POS.search(chunk)
    if m:
        sig["pos"] = [float(m.group(k)) for k in (1, 2, 3)]
        sig["yaw"] = float(m.group(4))
    m = RE_SESSION.search(chunk)
    if m:
        sig["character"] = m.group(1)
        sig["save"] = m.group(2)
    m = RE_HUD.search(chunk)
    if m:
        sig["hud_digest"] = m.group(4)
    m = RE_WORLD.search(chunk)
    if m:
        sig["scene"] = m.group(1)
        sig["location"] = tuple(int(m.group(k)) for k in (2, 3, 4))
    return sig


def world_round_trip(text: str) -> dict:
    """Round-trip medido por el SLOT: lo declarado al guardar vs lo aplicado.

    `slot` es el renglon de fidelidad que la puerta de escritura publica (posicion
    en mm, yaw, minuto de mundo y sitio); `resume` es el estado que
    `m18_session_restored` aplica. Un delta de 0 mm / 0 min es la igualdad
    medida; un `slot` ausente con `W-SAVE-001` presente es el caso legado.
    """
    out = {"slot": None, "resume": None, "warning": None}
    m = last_match(RE_FIDELITY, text)
    if m:
        out["slot"] = {
            "fidelity": int(m.group(1)),
            "index": int(m.group(2)),
            "pos_mm": [int(m.group(k)) for k in (3, 4, 5)],
            "yaw": float(m.group(6)),
            "pitch": float(m.group(7)),
            "world_minutes": int(m.group(8)),
            "environment": int(m.group(9)),
            "map_id": int(m.group(10)),
        }
    m = last_match(RE_RESUME, text)
    if m:
        out["resume"] = {
            "index": int(m.group(1)),
            "pos_mm": [int(m.group(k)) for k in (2, 3, 4)],
            "yaw_mrad": int(m.group(5)),
            "world_minutes": int(m.group(6)),
            "environment": int(m.group(7)),
        }
    m = last_match(RE_WARNING, text)
    if m:
        out["warning"] = m.group(1)
    return out


def reset_compact(sig: dict) -> str:
    """Los numeros del reset medidos en ESA entrada (no inferidos)."""
    if "career" not in sig:
        return "SIN LINEA `M18 VE: NEW GAME reset` — el mundo no se midio"
    return ("career=%s clock=%d min ticks=%d hud %s→%s pos=%s yaw=%.2f scene=%s guards=%d "
            "stage=%d billboards=%d npcs=%d"
            % (sig["career"], sig["reset_clock"], sig["reset_ticks"], sig["hud_before"],
               sig["hud_digest"], fmt_pos(sig["reset_pos"]), sig["reset_yaw"], sig["reset_scene"],
               sig["reset_guards"], sig["reset_stage"], sig["reset_billboards"], sig["reset_npcs"]))


def row(label: str, sig: dict, shot: dict | None) -> str:
    return (
        "%-9s world_ticks=%-5s clock=%-12s sim_ticks=%-5s pos=%-26s yaw=%-6s character=%-10s "
        "hud=%-16s sha=%s colores=%s"
        % (
            label, sig.get("world_ticks"), sig.get("clock_min"), sig.get("sim_ticks"),
            fmt_pos(sig.get("pos")),
            "%.3f" % sig["yaw"] if "yaw" in sig else "-",
            sig.get("character"), sig.get("hud_digest") or "-",
            shot["sha256"][:16] if shot else "-",
            shot["distinct"] if shot else "-",
        )
    )


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--label", default="run", help="subcarpeta de artefactos (pre-fix, post-fix, …)")
    ap.add_argument("--replay", default=None,
                    help="prueba negativa: lista de PNG separada por comas (relativa a "
                         ".sintergic/artifacts/new-game-semantics/) reevaluada con la regla vieja y la nueva")
    args = ap.parse_args()

    if args.replay:
        paths = [Path(raw.strip()) if Path(raw.strip()).is_absolute() else OUT_ROOT / raw.strip()
                 for raw in args.replay.split(",") if raw.strip()]
        return replay(paths, OUT_ROOT / args.label)

    if not BIN.is_file():
        print("VERDICT: NEW-GAME FAIL — falta %s (cargo build -p daggerfall-app --features window)" % BIN)
        return 1
    if not DATA.is_dir():
        print("VERDICT: NEW-GAME FAIL — falta el data root %s" % DATA)
        return 1

    out = OUT_ROOT / args.label
    if out.exists():
        shutil.rmtree(out)
    out.mkdir(parents=True, exist_ok=True)

    log_path = out / "app.log"
    frames_log = out / "frames.txt"
    frames_dir = out / "frames"
    frames_log.write_text(
        "# estabilidad de captura: umbral %d consecutivas con el mismo sha256, intervalo %.2f s, "
        "marca de fase `%s`\n" % (SETTLE_STABLE_CAPTURES, SETTLE_INTERVAL, SETTLE_WORLD_MARKER.strip()),
        encoding="utf-8")
    # `shot_saved` se captura DENTRO del `try`; se inicializa aqui para que una
    # salida temprana (FAIL de captura) no rompa el reporte con `NameError`.
    shot_saved = None
    save_dir = out / "saves"
    save_dir.mkdir()
    d = xdisplay.Display(DISPLAY)
    state = out / "shell.state"
    state.write_text("", encoding="utf-8")
    env = dict(os.environ)
    env.update({
        "DF_APP_ROOT": str(DATA),
        "DF_SHELL_STATE": str(state),
        "DF_SAVE_DIR": str(save_dir),
        "DISPLAY": DISPLAY,
    })
    env.pop("DF_MENU_ROW_FROZEN", None)
    log = open(log_path, "wb")
    proc = subprocess.Popen([str(BIN)], cwd=str(ROOT), env=env, stdout=log, stderr=subprocess.STDOUT)
    try:
        win = find_window(d, time.monotonic() + 240.0, proc)
        if win is None:
            print("VERDICT: NEW-GAME FAIL — no-window title=%r" % TITLE_TOKEN)
            return 1
        focus(d, win)

        if not wait_line(log_path, "SHELL VE: menu state=Menu", time.monotonic() + 180.0, proc):
            print("VERDICT: NEW-GAME FAIL — el menu no se armo (sin `menu state=Menu`)")
            return 1

        # ── entrada #1: new-game + wizard ────────────────────────────────
        wizard = "M18 VE: SHELL create — the retail wizard opens at step 1"
        press(d, win, "Return")
        if not wait_line(log_path, wizard, time.monotonic() + 12.0, proc):
            print("aviso: XTEST no llega al cliente winit — se mide XSendEvent")
            CHANNEL["name"] = "sendevent"
            press(d, win, "Return")
            if not wait_line(log_path, wizard, time.monotonic() + 12.0, proc):
                print("VERDICT: NEW-GAME FAIL — la tecla no abrio el wizard (INJECTION-BLOCKED)")
                return 1
        print("CHANNEL: %s" % CHANNEL["name"])
        press(d, win, "Return")
        if not wait_line(log_path, "phase=PLAYING", time.monotonic() + 30.0, proc):
            print("VERDICT: NEW-GAME FAIL — la segunda `Return` no entro al mundo")
            return 1
        wait_count(log_path, "SHELL VE: new_game", 1, time.monotonic() + 15.0, proc)
        first = out / "world-first.png"
        shot1 = settle_world(win, first, time.monotonic() + 60.0, tag="entry1",
                             frames_log=frames_log, frames_dir=frames_dir,
                             log_path=log_path, marker_count=1)

        # ── jugar: el avatar anda y el reloj avanza ──────────────────────
        before_play = entry_signature(log_text(log_path), 1)
        key_down(d, win, "w")
        time.sleep(6.0)
        key_up(d, win, "w")
        time.sleep(0.5)
        clock0 = before_play.get("clock_min") or 0
        advanced = wait_for(
            log_path,
            lambda text: any(int(m.group(5)) >= clock0 + 4 for m in RE_SIM.finditer(text)),
            time.monotonic() + 20.0,
            proc,
        )

        # ── viaje rapido: consume minutos del reloj de SESION ────────────
        press(d, win, "m")
        opened = wait_line(log_path, "SHELL VE: map open=1", time.monotonic() + 10.0, proc)
        pressed_t = False
        if opened:
            press(d, win, "t", gap=1.0)
            wait_line(log_path, "SHELL VE: travel", time.monotonic() + 12.0, proc)
            pressed_t = True
            press(d, win, "m")
            wait_line(log_path, "SHELL VE: map open=0", time.monotonic() + 10.0, proc)
        time.sleep(0.6)

        # ── captura del MOMENTO DE GUARDAR (antes de la tecla de pausa) ──
        # El panel de pausa repinta la pantalla, asi que la unica forma de
        # comparar "guardado" con "reanudado" es capturar y asentar el mundo
        # ANTES de `Escape` (CR-7a3e9c14; antes esta fila salia `sha=-`).
        saved_shot_path = out / "world-saved.png"
        shot_saved = try_settle_world(win, saved_shot_path, time.monotonic() + 25.0, tag="saved",
                                      frames_log=frames_log, frames_dir=frames_dir,
                                      log_path=log_path)

        # ── pausa + guardado + vuelta al menu ────────────────────────────
        press(d, win, "Escape", gap=0.9)
        paused = wait_line(log_path, "M18 VE: SHELL pause — world frozen", time.monotonic() + 20.0, proc)
        time.sleep(0.8)
        press(d, win, "s", gap=0.9)
        opened_save = wait_line(log_path, "SHELL VE: save open=1", time.monotonic() + 10.0, proc)
        if opened_save:
            press(d, win, "Return", gap=1.2)
            wait_line(log_path, "SHELL VE: save slot=", time.monotonic() + 12.0, proc)
            press(d, win, "Escape", gap=0.9)  # cierra el browser: en panel=Save `b` no se consume
            wait_line(log_path, "M18 VE: SHELL save — browser closed", time.monotonic() + 10.0, proc)
        after_save = log_text(log_path)
        shell_ticks = last_match(RE_PAUSE, after_save)
        session_minutes_saved = int(shell_ticks.group(1)) if shell_ticks else None
        travel = last_match(RE_TRAVEL, after_save)

        press(d, win, "b", gap=1.2)
        back = wait_line(log_path, "M18 VE: SHELL menu — back from the session", time.monotonic() + 20.0, proc)

        # ── entrada #2: NEW GAME otra vez ───────────────────────────────
        press(d, win, "Return", gap=0.9)
        wizard2 = wait_count(log_path, wizard, 2, time.monotonic() + 15.0, proc)
        press(d, win, "Return", gap=0.9)
        entered2 = wait_count(log_path, "SHELL VE: new_game", 2, time.monotonic() + 30.0, proc)
        if not entered2:
            print("VERDICT: NEW-GAME FAIL — la segunda sesion no entro al mundo")
            return 1
        second = out / "world-second.png"
        shot2 = settle_world(win, second, time.monotonic() + 60.0, tag="entry2",
                             frames_log=frames_log, frames_dir=frames_dir,
                             log_path=log_path, marker_count=2)
        after_second = entry_signature(log_text(log_path), 2)

        # ── CONTINUE: debe conservar el slot ────────────────────────────
        press(d, win, "Escape", gap=0.9)
        wait_count(log_path, "M18 VE: SHELL pause — world frozen", 2, time.monotonic() + 20.0, proc)
        time.sleep(0.8)
        press(d, win, "b", gap=1.2)
        wait_count(log_path, "M18 VE: SHELL menu — back from the session", 2, time.monotonic() + 20.0, proc)
        press(d, win, "Down", gap=0.8)
        wait_line(log_path, "SHELL VE: menu selection=1", time.monotonic() + 10.0, proc)
        press(d, win, "Return", gap=0.9)
        continued = wait_for(
            log_path,
            lambda text: "continue result=" in text,
            time.monotonic() + 25.0,
            proc,
        )
        time.sleep(1.5)
        third = out / "world-continue.png"
        shot3 = try_settle_world(win, third, time.monotonic() + 20.0, tag="continue",
                                 frames_log=frames_log, frames_dir=frames_dir,
                                 log_path=log_path, marker_count=3) if continued else None

        # ── cierre ──────────────────────────────────────────────────────
        press(d, win, "Escape", gap=0.8)
        press(d, win, "q", gap=0.8)
        deadline = time.monotonic() + 25.0
        while proc.poll() is None and time.monotonic() < deadline:
            time.sleep(0.2)
        if proc.poll() is None:
            proc.terminate()
            try:
                proc.wait(timeout=10)
            except subprocess.TimeoutExpired:
                proc.kill()
    finally:
        # Un aborto (SystemExit de `capture`/`settle_world`) no puede dejar la app
        # viva en :0: la corrida siguiente engancharia su ventana por titulo y
        # mediria la instancia anterior.
        if proc.poll() is None:
            proc.terminate()
            try:
                proc.wait(timeout=10)
            except subprocess.TimeoutExpired:
                proc.kill()
        log.close()

    text = log_text(log_path)
    first_sig = entry_signature(text, 1)
    played = dict(first_sig)
    boundary = text.find("M18 VE: SHELL menu — back from the session")
    played_text = text[:boundary] if boundary >= 0 else text
    played_pos = None
    for m in RE_POS.finditer(played_text):
        played_pos = [float(m.group(1)), float(m.group(2)), float(m.group(3))]
    played["pos"] = played_pos
    sims = [m for m in RE_SIM.finditer(played_text)]
    played["clock_min"] = int(sims[-1].group(5)) if sims else None
    played["label"] = "PLAYED"

    equal, diffs = signature_equal(first_sig, after_second)
    same_pixels = shot1["sha256"] == shot2["sha256"]
    load = last_match(RE_LOAD_DONE, text)
    cont = last_match(RE_CONTINUE, text)
    # `continue` NO emite `SHELL VE: load done …ticks=N` (esa línea es del
    # browser de carga): los ticks restaurados se leen del primer
    # `SHELL VE: pause state=Paused ticks=N` POSTERIOR a `continue result=`,
    # que es la pausa de cierre del instrumento sobre la sesión reanudada.
    post_continue = text[text.rfind("SHELL VE: continue result="):] if cont else ""
    pause_after_continue = RE_PAUSE.search(post_continue)
    continue_ticks = int(load.group(3)) if load else (
        int(pause_after_continue.group(1)) if pause_after_continue else None)

    lines = []
    lines.append("═" * 118)
    lines.append("NEW GAME — semantica de la segunda entrada · CR-178183d7 · canal %s" % CHANNEL["name"])
    lines.append("═" * 118)
    lines.append(row("ENTRADA 1", first_sig, shot1))
    lines.append(row("JUGADA", played, shot_saved) + "   (ultimo pos/clock del log, "
                 "captura ANTES de la pausa)")
    lines.append(row("ENTRADA 2", after_second, shot2))
    cont_sig = continue_signature(text)
    lines.append(row("CONTINUE", cont_sig, shot3))
    lines.append("-" * 118)
    lines.append("sha256 captura entrada 1 (%s): %s" % (first.name, shot1["sha256"]))
    lines.append("sha256 captura entrada 2 (%s): %s" % (second.name, shot2["sha256"]))
    if shot3:
        lines.append("sha256 captura continue  (%s): %s" % (third.name, shot3["sha256"]))
    if shot_saved:
        lines.append("sha256 captura guardado  (%s): %s" % (saved_shot_path.name,
                                                            shot_saved["sha256"]))
    lines.append("pos de entrada (reset) vs 1ª línea periódica `M19 VE: pos=`: "
                 "entrada 1 %s / %s · entrada 2 %s / %s"
                 % (fmt_pos(first_sig.get("pos")), fmt_pos(first_sig.get("pos_line")),
                    fmt_pos(after_second.get("pos")), fmt_pos(after_second.get("pos_line"))))
    lines.append("reloj avanza jugando: %s   ·  pausa declara ticks=%s  ·  viaje: %s"
                 % (advanced, session_minutes_saved,
                    (travel.group(1) if travel else "no ejecutado")))
    lines.append("captura 1 vs 2: sha %s   %s"
                 % ("IGUAL" if same_pixels else "DISTINTO",
                    "%s (%d colores)" % (shot1["sha256"][:16], shot1["distinct"])))
    lines.append("firma entrada 1 vs 2: %s" % ("IGUAL" if equal else "DISTINTA — " + "; ".join(diffs)))
    lines.append("estabilidad de captura: umbral %d consecutivas con el mismo sha256 · intervalo %.2f s "
                 "· marca de fase `%s`" % (SETTLE_STABLE_CAPTURES, SETTLE_INTERVAL,
                                           SETTLE_WORLD_MARKER.strip()))
    for label, shot in (("entrada 1", shot1), ("entrada 2", shot2), ("continue", shot3)):
        if shot is None:
            continue
        lines.append("%-10s capturas=%d · world-like RECHAZADOS por inestables=%d · racha final=%d · %s"
                     % (label, shot.get("frames"), shot.get("rejected_world"), shot.get("stable"),
                        "ESTABLE" if shot.get("accepted") else
                        "NO ESTABLE (su sha256 no vale como evidencia de igualdad)"))
    lines.append("continue: %s · ticks restaurados=%s (guardado=%s)"
                 % (cont.group(1) if cont else "no ejecutado", continue_ticks, session_minutes_saved))
    cpos, ppos = cont_sig.get("pos"), played.get("pos")
    if cpos and ppos:
        axis = max(abs(a - b) for a, b in zip(cpos, ppos))
        euclid = math.dist(cpos, ppos)
        lines.append("CONTINUE-SIN-POSICION: pos reanudada=%s vs pos guardada=%s · delta eje=%.2f m · "
                     "euclid=%.2f m" % (fmt_pos(cpos), fmt_pos(ppos), axis, euclid))
        continue_pos_ok = axis <= 0.05
    else:
        lines.append("CONTINUE-SIN-POSICION: no medible (reanudada=%s guardada=%s)"
                     % (fmt_pos(cpos), fmt_pos(ppos)))
        continue_pos_ok = False
    lines.append("CONTINUE reloj=%s min (guardado=%s) · character=%s · hud=%s · scene=%s"
                 % (cont_sig.get("clock_min"), session_minutes_saved, cont_sig.get("character"),
                    cont_sig.get("hud_digest") or "-", cont_sig.get("scene") or "-"))
    saved_clock, cont_clock = played.get("clock_min"), cont_sig.get("clock_min")
    lines.append("CONTINUE-SIN-RELOJ-MUNDO: reloj de mundo reanudado=%s vs guardado=%s · delta=%s min"
                 % (cont_clock, saved_clock,
                    (cont_clock - saved_clock) if None not in (cont_clock, saved_clock) else "?"))
    if cont_sig.get("character") is None:
        lines.append("CONTINUE-CHARACTER: SIN LINEA — `continue` no publica "
                     "`SHELL VE: session … character=`, solo `create done` lo hace; "
                     "el personaje restaurado queda NO MEDIDO (hud digest 9020… no discrimina)")

    # La firma por `clock_min`/`sim_ticks` NO sirve para decidir FRESH/RESUMES:
    # ambas avanzan con el tiempo de juego y la captura de la entrada #1 cae en
    # un instante distinto en cada corrida (medido: postfix dio clock igual y
    # `FRESH`; postfix2, con la misma imagen 328b363b en las dos entradas, dio
    # clock 210168724 vs 210168720 y `RESUMES`). Se compara contra el estado
    # JUGADO —lo que un reanudado arrastraria— y no contra relojes en vuelo.
    resumed_signals = []
    if shot_saved is not None and shot2 is not None and shot2["sha256"] == shot_saved["sha256"]:
        resumed_signals.append("imagen == guardado")
    if (played.get("clock_min") is not None and after_second.get("clock_min") is not None
            and after_second["clock_min"] == played["clock_min"]):
        resumed_signals.append("clock == jugado")
    if (played.get("pos") is not None and after_second.get("pos") is not None
            and after_second["pos"] == played["pos"]):
        resumed_signals.append("pos == jugado")
    resumed = bool(resumed_signals)
    lines.append("señales de reanudado en la entrada #2: %s"
                 % (", ".join(resumed_signals) if resumed_signals else "ninguna"))
    continue_ok = continue_ticks is not None and session_minutes_saved is not None \
        and continue_ticks == session_minutes_saved

    # ── round-trip medido por el SLOT (CR-7a3e9c14) ────────────────────
    trip = world_round_trip(text)
    slot, resumed_world = trip["slot"], trip["resume"]
    if slot and resumed_world:
        dpos = [b - a for a, b in zip(slot["pos_mm"], resumed_world["pos_mm"])]
        dmin = resumed_world["world_minutes"] - slot["world_minutes"]
        lines.append("SLOT round-trip: fidelity=%d slot=%d pos_mm=%s yaw=%.3f world_minutes=%d "
                     "environment=%d map_id=%d"
                     % (slot["fidelity"], slot["index"], slot["pos_mm"], slot["yaw"],
                        slot["world_minutes"], slot["environment"], slot["map_id"]))
        lines.append("SLOT round-trip: reanudado=%d pos_mm=%s yaw_mrad=%d world_minutes=%d "
                     "environment=%d · delta pos_mm=%s delta mundo=%s min"
                     % (resumed_world["index"], resumed_world["pos_mm"], resumed_world["yaw_mrad"],
                        resumed_world["world_minutes"], resumed_world["environment"], dpos, dmin))
        slot_ok = dpos == [0, 0, 0] and dmin == 0
        lines.append("VERDICT: SLOT-FIDELITY-%s — pos_mm=%s min=%s (fidelidad declarada %d)"
                     % ("4" if slot["fidelity"] >= 4 else str(slot["fidelity"]),
                        "IGUAL" if dpos == [0, 0, 0] else "DISTINTA",
                        "IGUAL" if dmin == 0 else "%+d" % dmin, slot["fidelity"]))
        if not slot_ok:
            lines.append("SLOT-DRIFT: el slot declara un mundo que la reanudacion no aplica")
    else:
        lines.append("SLOT round-trip: SIN LINEA — slot=%s reanudado=%s (el slot no declara mundo)"
                     % ("medido" if slot else "ausente", "medido" if resumed_world else "ausente"))
        slot_ok = False
    if trip["warning"]:
        lines.append("SLOT aviso: %s" % trip["warning"])
    saved_vs_resume = (shot_saved is not None and shot3 is not None
                       and shot_saved["sha256"] == shot3["sha256"])
    if shot_saved and shot3:
        lines.append("captura guardado vs continue: sha %s · campo %.2f%%/%.2f%% · papel %.2f%%/%.2f%% "
                     "· colores %d/%d"
                     % ("IGUAL" if saved_vs_resume else "DISTINTO",
                        shot_saved["field"] * 100, shot3["field"] * 100,
                        shot_saved["paper"] * 100, shot3["paper"] * 100,
                        shot_saved["distinct"], shot3["distinct"]))
    else:
        lines.append("captura guardado vs continue: NO MEDIBLE (guardado=%s continue=%s)"
                     % ("si" if shot_saved else "no", "si" if shot3 else "no"))

    if resumed:
        lines.append("VERDICT: NEW-GAME RESUMES — la entrada #2 conserva estado de la sesion anterior")
    else:
        lines.append("VERDICT: NEW-GAME FRESH — la entrada #2 es una primera partida")
    lines.append("VERDICT: CONTINUE-%s — continue restaura %s min (guardados %s)"
                 % ("PRESERVES" if continue_ok else "LOST", continue_ticks, session_minutes_saved))
    lines.append("VERDICT: CONTINUE-POSICION-%s — pos %s"
                 % ("CONSERVADA" if continue_pos_ok else "PERDIDA",
                    fmt_pos(cpos) if cpos else "sin linea `M19 VE: pos=`"))
    report = "\n".join(lines)
    (out / "report.txt").write_text(report + "\n", encoding="utf-8")
    print(report)

    evidence = [
        raw.rstrip() for raw in text.splitlines()
        if any(n in raw for n in (
            "SHELL VE: new_game", "SHELL VE: create done", "SHELL VE: session", "SHELL VE: world",
            "SHELL VE: hud", "SHELL VE: pause", "SHELL VE: save", "SHELL VE: load", "SHELL VE: continue",
            "SHELL VE: travel", "SHELL VE: menu", "M18 VE: sim ticks", "M18 VE: SHELL",
            "M19 VE: pos=", "M20 VE:", "phase MENU",
        ))
    ]
    (out / "evidence.txt").write_text("\n".join(evidence) + "\n", encoding="utf-8")

    # ── ledger de corridas: una linea por ejecucion, aunque `--label` se repita ──
    run_record = {
        "ts": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
        "label": args.label,
        "channel": CHANNEL["name"],
        "report_sha256": hashlib.sha256((report + "\n").encode()).hexdigest(),
        "settle": {"stable_captures": SETTLE_STABLE_CAPTURES, "interval_s": SETTLE_INTERVAL,
                   "world_marker": SETTLE_WORLD_MARKER.strip()},
        "entry1": {"sha256": shot1["sha256"], "captures": shot1.get("frames"),
                   "rejected_unstable": shot1.get("rejected_world")},
        "entry2": {"sha256": shot2["sha256"], "captures": shot2.get("frames"),
                   "rejected_unstable": shot2.get("rejected_world")},
        "continue": None if shot3 is None else {
            "sha256": shot3["sha256"], "captures": shot3.get("frames"),
            "rejected_unstable": shot3.get("rejected_world"), "accepted": shot3.get("accepted")},
        "signature_equal": equal,
        "same_pixels": same_pixels,
        "verdicts": {"new_game": "RESUMES" if resumed else "FRESH",
                     "continue": "PRESERVES" if continue_ok else "LOST",
                     "continue_pos": "CONSERVADA" if continue_pos_ok else "PERDIDA"},
        "log_sha256": hashlib.sha256(log_path.read_bytes()).hexdigest() if log_path.is_file() else None,
    }
    with (OUT_ROOT / "runs.jsonl").open("a", encoding="utf-8") as fh:
        fh.write(json.dumps(run_record, ensure_ascii=False, sort_keys=True) + "\n")
    return 0


if __name__ == "__main__":
    sys.exit(main())
