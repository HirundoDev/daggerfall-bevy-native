#!/usr/bin/env python3
"""playthrough-p1-door.py — probe P1 del recorrido: la puerta de FG-01.

Mide, en la ventana real, el recorrido minimo de la puerta: **el jugador llega a
la puerta, pulsa interact (`E`) y la escena transiciona**. Solo usa mecanismos que
ya existen en el arbol medido (`8ac5144`):

  * `M18_CS14=1` (`m18_demo.rs:2470` -> `apply_cs14_scenario`) inyecta UNA puerta
    de escenario a `CS14_DOOR_AHEAD_M = 1.4` m del ojo de spawn, y
    `m28_cs14_scenario_door` (`m18_window.rs:3507`) la mantiene ahi hasta el
    primer crimen.
  * `G` es el binding existente de `m28_door_bash` (`m18_window.rs:3546`): su
    linea `M28 VE: bash at <d> m …` es la prueba de que el jugador ESTA en una
    puerta (alcance `DOOR_REACH_M = 2.5` m, `m18_window.rs:224`).
  * `E` es `action.interact` (`m2_player.rs:135`), la tecla del recorrido.
  * El driver X11 es el aceptado en este puesto (`scripts/measure-m18-cs.py`):
    descubrimiento por `_NET_WM_NAME`, activacion por `_NET_ACTIVE_WINDOW` +
    `set_input_focus`, y eventos de teclado **nucleo** con `send_event` (XTEST es
    inerte para este cliente).

Lanzamiento: el binario se ejecuta **sin** `--window`. `main.rs:915` mapea
`--window` al camino M2 legacy (`run_windowed`, un solo bloque ARCH3D), que no
compone el demo M18: no inyecta CS-14 y no tiene `M28 VE: bash`. El demo
integrado (el que si tiene la puerta) es el camino por defecto. Prohibido
`--door-spawn`: no existe en `8ac5144` y usarlo haria tautologico el rojo.

Piernas
  * `door-interact`: spawn con la puerta a 1.4 m -> `G` (fila `reach`) -> `E` ->
    4 s -> transiciona si aparece `FG01 VE: interact → interior` o el titulo
    trae `scene=Interior`. Sin el cableado esa linea no existe nunca: ese FAIL
    es el ROJO literal.
  * `negative-no-interact`: proceso nuevo, `W` 1 s y **sin** `E`: no debe
    aparecer la linea de interior (control de que la transicion de la otra
    pierna es imputable a `E`).

Salida: filas normalizadas `row <nombre>=<valor>` (sin timestamps, sin
duraciones, sin PIDs, sin volcar el log de la app). Dos ejecuciones imprimen
bytes identicos. Los logs crudos quedan en `--out`.

Exit: 0 todas las filas PASS · 1 alguna FAIL · 2 uso/IO (sin DISPLAY, sin
ARENA2, binario ausente, build roto, ventana no encontrada).

Uso:
  scripts/playthrough-p1-door.py [--skip-build] [--out DIR] [--bin PATH]
                                 [--data-root DIR] [--timeout S]
"""
import argparse
import hashlib
import os
import re
import subprocess
import sys
import time

from Xlib import X, XK, display
from Xlib.protocol import event as xevent

WS = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
DEFAULT_BIN = os.path.join(WS, "target", "debug", "daggerfall-app")
DEFAULT_OUT = os.path.join(WS, ".sintergic", "artifacts", "fg01", "playthrough-p1-door")
DEFAULT_DATA_ROOT = os.path.expanduser("~/Games/Daggerfall/app")

NET_WM_NAME = "_NET_WM_NAME"
UTF8 = "UTF8_STRING"
NET_ACTIVE_WINDOW = "_NET_ACTIVE_WINDOW"
KEY_PRESS_MASK = X.KeyPressMask
KEY_RELEASE_MASK = X.KeyReleaseMask

# `DOOR_REACH_M` (m18_window.rs:224): alcance del binding que bate la puerta.
REACH_M = 2.5
# Puerta de escenario CS-14 (m18_demo.rs:2495, `CS14_DOOR_AHEAD_M`).
CS14_AHEAD_M = 1.4

# ── lineas de evidencia ────────────────────────────────────────────────────
RE_BASH_AT = re.compile(r"M28 VE: bash at ([0-9]+\.[0-9]) m")
RE_BASH_FAR = re.compile(
    r"M28 VE: bash — no retail door within [0-9.]+ m \(nearest at ([0-9]+\.[0-9]) m\)"
)
RE_BASH_CRIME = re.compile(r"M28 VE: bash — crime ")
RE_CS14 = re.compile(r"M18 VE: CS14 scenario")
RE_INTERIOR = re.compile(r"FG01 VE: interact → interior")
RE_PHASE_PLAYING = re.compile(r"M18 VE: phase MENU → PLAYING")


# ── descubrimiento de ventana (`measure-m18-cs.py`) ────────────────────────
def name_of(d, win):
    """`_NET_WM_NAME` (UTF-8, lo que pone winit/KWin) y `WM_NAME` de respaldo."""
    for prop, typ in ((NET_WM_NAME, UTF8), ("WM_NAME", "STRING")):
        try:
            val = win.get_full_property(d.intern_atom(prop), d.intern_atom(typ))
        except Exception:
            val = None
        if val and val.value:
            raw = val.value
            if isinstance(raw, bytes):
                try:
                    return raw.decode("utf-8", "replace")
                except Exception:
                    return raw.decode("latin-1", "replace")
            return str(raw)
    try:
        nm = win.get_wm_name()
        return str(nm) if nm else None
    except Exception:
        return None


def walk(d, win, depth=0, acc=None):
    """Recorrido en profundidad del arbol real (hijos de `win`, no del root)."""
    acc = [] if acc is None else acc
    if depth > 3:
        return acc
    try:
        children = win.query_tree().children
    except Exception:
        return acc
    for child in children:
        nm = name_of(d, child)
        if nm:
            acc.append((child, child.id, nm))
        walk(d, child, depth + 1, acc)
    return acc


def find_window(d, needle="DAGGERFALL", timeout=240.0, proc=None):
    deadline = time.time() + timeout
    while time.time() < deadline:
        if proc is not None and proc.poll() is not None:
            return None
        for cand in walk(d, d.screen().root):
            if needle in cand[2].upper():
                return cand
        time.sleep(0.5)
    return None


def activate(d, win, timeout=4.0):
    """Foco real de teclado en :0 (KWin/Wayland).

    La ruta aceptada es un ClientMessage EWMH `_NET_ACTIVE_WINDOW` con origen 2
    (pager/usuario): KWin activa entonces la superficie XWayland. `set_input_focus`
    solo es un refuerzo; los eventos de abajo van dirigidos a la ventana.
    """
    root = d.screen().root
    try:
        root.send_event(
            xevent.ClientMessage(
                window=win,
                client_type=d.intern_atom(NET_ACTIVE_WINDOW),
                data=(32, [2, X.CurrentTime, 0, 0, 0]),
            ),
            event_mask=X.SubstructureRedirectMask | X.SubstructureNotifyMask,
        )
        d.sync()
    except Exception:
        pass
    try:
        win.configure(stack_mode=X.Above)
    except Exception:
        pass
    win.set_input_focus(X.RevertToParent, X.CurrentTime)
    d.sync()
    deadline = time.time() + timeout
    while time.time() < deadline:
        try:
            focus = d.get_input_focus().focus
            fid = focus.id if hasattr(focus, "id") else focus
        except Exception:
            fid = None
        if fid == win.id:
            return True
        time.sleep(0.2)
    return False


def title(d, win):
    try:
        return name_of(d, win) or ""
    except Exception:
        return ""


# ── entrada: `send_event` nucleo (ruta aceptada; XTEST inerte) ─────────────
def kc(d, name):
    return d.keysym_to_keycode(XK.string_to_keysym(name))


def send_key(d, win, name, down):
    root = d.screen().root
    code = kc(d, name)
    cls = xevent.KeyPress if down else xevent.KeyRelease
    mask = KEY_PRESS_MASK if down else KEY_RELEASE_MASK
    win.send_event(
        cls(
            time=X.CurrentTime,
            root=root,
            window=win,
            same_screen=1,
            child=X.NONE,
            root_x=0,
            root_y=0,
            event_x=0,
            event_y=0,
            state=0,
            detail=code,
        ),
        event_mask=mask,
    )
    d.sync()


def press(d, win, name, hold=0.10):
    send_key(d, win, name, True)
    time.sleep(hold)
    send_key(d, win, name, False)
    time.sleep(0.25)


def hold(d, win, name, secs):
    send_key(d, win, name, True)
    time.sleep(secs)
    send_key(d, win, name, False)
    time.sleep(0.35)


# ── log de la app ──────────────────────────────────────────────────────────
def read_log(path):
    try:
        with open(path, "r", encoding="utf-8", errors="replace") as fh:
            return fh.read().splitlines()
    except FileNotFoundError:
        return []


def wait_lines(path, pattern, timeout, proc=None):
    """Devuelve las lineas que casan con `pattern` cuando aparecen, o []."""
    deadline = time.time() + timeout
    while time.time() < deadline:
        hits = [ln for ln in read_log(path) if pattern.search(ln)]
        if hits:
            return hits
        if proc is not None and proc.poll() is not None:
            return [ln for ln in read_log(path) if pattern.search(ln)]
        time.sleep(0.25)
    return [ln for ln in read_log(path) if pattern.search(ln)]


def wait_title(d, win, needle, timeout, proc=None):
    """Espera a que el titulo contenga `needle`; devuelve el titulo o None."""
    deadline = time.time() + timeout
    while time.time() < deadline:
        if proc is not None and proc.poll() is not None:
            return None
        current = title(d, win)
        if needle in current:
            return current
        time.sleep(0.25)
    return None


# ── una pierna: un proceso, una ventana ────────────────────────────────────
class Session:
    """Un proceso del demo con la puerta CS-14 inyectada (`M18_CS14=1`)."""

    def __init__(self, d, binary, data_root, out_dir, leg):
        self.d = d
        self.binary = binary
        self.data_root = data_root
        self.log_path = os.path.join(out_dir, "app-%s.log" % leg)
        self.leg = leg
        self.proc = None
        self.win = None
        self.sink = None

    def __enter__(self):
        env = dict(os.environ)
        # Fuente de datos (mismo custodia que `measure-m18-cs.py`).
        env["DF_APP_ROOT"] = self.data_root
        env.setdefault("DISPLAY", os.environ.get("DISPLAY", ":0"))
        # Puerta de escenario a 1.4 m del ojo de spawn (`m18_demo.rs:2470`).
        env["M18_CS14"] = "1"
        if os.path.exists(self.log_path):
            os.unlink(self.log_path)
        self.sink = open(self.log_path, "wb")
        # Sin `--window`: ese flag selecciona el camino M2 legacy (`main.rs:915`).
        self.proc = subprocess.Popen(
            [self.binary],
            stdout=self.sink,
            stderr=subprocess.STDOUT,
            env=env,
            cwd=WS,
        )
        return self

    def __exit__(self, *exc):
        if self.proc is not None:
            self.proc.terminate()
            try:
                self.proc.wait(timeout=10)
            except Exception:
                self.proc.kill()
        if self.sink is not None:
            self.sink.close()
        return False

    def window(self, timeout):
        found = find_window(self.d, timeout=timeout, proc=self.proc)
        if found is None:
            return False
        self.win = found[0]
        activate(self.d, self.win)
        return True

    def enter_playing(self, timeout=60.0):
        """`Return` hasta que el titulo diga PLAYING (MENU -> PLAYING)."""
        for _ in range(3):
            if wait_title(self.d, self.win, "PLAYING", 0.1, self.proc):
                return True
            press(self.d, self.win, "Return", hold=0.12)
            if wait_title(self.d, self.win, "PLAYING", timeout / 3.0, self.proc):
                return True
        return False


# ── filas normalizadas ─────────────────────────────────────────────────────
def row(name, value):
    return "row %s=%s" % (name, value)


def bash_distance(lines):
    """Distancia (m) que la app imprimio para el golpe de puerta, o None."""
    for ln in lines:
        hit = RE_BASH_AT.search(ln)
        if hit:
            return hit.group(1), True
        hit = RE_BASH_FAR.search(ln)
        if hit:
            return hit.group(1), False
        if RE_BASH_CRIME.search(ln):
            # La rama del crimen solo se alcanza con la puerta dentro del
            # alcance (la distancia de escenario es fija: 1.4 m).
            return "%.1f" % CS14_AHEAD_M, True
    return None, False


def leg_door_interact(d, args, rows):
    """Pierna A: `G` prueba alcance, `E` debe transicionar al interior."""
    with Session(d, args.bin, args.data_root, args.out, "door-interact") as session:
        if not session.window(args.timeout):
            return False, "NO-WINDOW (door-interact)"
        if not session.enter_playing():
            return False, "NO-PLAYING (door-interact)"
        rows.append(row("phase", "PLAYING"))

        # `G`: binding existente de `m28_door_bash`. Hasta 3 intentos solo si la
        # linea no aparece (el input de este puesto llega por `send_event`).
        bash_lines = []
        for attempt in range(3):
            press(d, session.win, "G", hold=0.10)
            bash_lines = wait_lines(
                session.log_path, re.compile(r"M28 VE: bash"), 4.0, session.proc
            )
            if bash_lines:
                break
        distance, within = bash_distance(bash_lines)
        if distance is None:
            rows.append(row("reach", "unknown"))
            rows.append(row("reach-pass", "no"))
            rows.append(row("interact-line", "unknown"))
            rows.append(row("title-interior", "unknown"))
            rows.append(row("interact-transitions", "no"))
            rows.append(row("door-interact", "FAIL"))
            return False, "NO-BASH-LINE (door-interact): el jugador no quedo ante una puerta"

        rows.append(row("reach", distance))
        rows.append(row("reach-pass", "yes" if within and float(distance) <= REACH_M else "no"))

        # `E`: `action.interact` (`m2_player.rs:135`). 4 s de margen.
        press(d, session.win, "E", hold=0.10)
        interior = wait_lines(session.log_path, RE_INTERIOR, 4.0, session.proc)
        current = title(d, session.win)
        title_interior = "scene=Interior" in current
        rows.append(row("interact-line", "present" if interior else "absent"))
        rows.append(row("title-interior", "yes" if title_interior else "no"))
        transitioned = bool(interior) or title_interior
        rows.append(row("interact-transitions", "yes" if transitioned else "no"))
        rows.append(row("door-interact", "PASS" if transitioned else "FAIL"))
        return transitioned, None


def leg_negative(d, args, rows):
    """Pierna B: control negativo — `W` 1 s sin `E` no transiciona."""
    with Session(d, args.bin, args.data_root, args.out, "negative-no-interact") as session:
        if not session.window(args.timeout):
            return False, "NO-WINDOW (negative-no-interact)"
        if not session.enter_playing():
            return False, "NO-PLAYING (negative-no-interact)"
        hold(d, session.win, "w", 1.0)
        time.sleep(1.0)
        interior = [ln for ln in read_log(session.log_path) if RE_INTERIOR.search(ln)]
        present = bool(interior)
        rows.append(row("negative-interact-line", "present" if present else "absent"))
        rows.append(row("negative-no-interact", "FAIL" if present else "PASS"))
        return not present, None


# ── corrida ────────────────────────────────────────────────────────────────
def build(args):
    """`cargo build -p daggerfall-app --features window` (default probado)."""
    if args.skip_build:
        return True, None
    proc = subprocess.run(
        ["cargo", "build", "-p", "daggerfall-app", "--features", "window"],
        cwd=WS,
        capture_output=True,
        text=True,
    )
    if proc.returncode != 0:
        return False, (proc.stderr or proc.stdout).strip().splitlines()[-1:]
    return True, None


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--bin", default=DEFAULT_BIN, help="binario (default target/debug)")
    ap.add_argument("--out", default=DEFAULT_OUT, help="dir de logs crudos")
    ap.add_argument("--data-root", default=DEFAULT_DATA_ROOT, help="root de datos retail")
    ap.add_argument("--timeout", type=float, default=240.0, help="espera de ventana (s)")
    ap.add_argument("--skip-build", action="store_true", help="no reconstruir el binario")
    args = ap.parse_args(argv)

    binary = os.path.abspath(args.bin)
    data_root = os.path.abspath(args.data_root)
    out = os.path.abspath(args.out)

    if not os.path.isdir(os.path.join(data_root, "ARENA2")):
        print("row error=no-arena2\nrow result=FAIL")
        print("IO: %s/ARENA2 no existe (--data-root)" % data_root, file=sys.stderr)
        return 2
    build_ok, build_err = build(args)
    if not build_ok:
        print("row error=build\nrow result=FAIL")
        print("IO: build roto: %s" % (build_err or ""), file=sys.stderr)
        return 2
    if not os.path.isfile(binary):
        print("row error=no-binary\nrow result=FAIL")
        print("IO: %s no existe (--bin)" % binary, file=sys.stderr)
        return 2
    try:
        d = display.Display()
    except Exception as exc:
        print("row error=no-display\nrow result=FAIL")
        print("IO: sin DISPLAY: %s" % exc, file=sys.stderr)
        return 2

    os.makedirs(out, exist_ok=True)
    with open(binary, "rb") as fh:
        sha = hashlib.sha256(fh.read()).hexdigest()

    rows = [row("binary-sha256", sha)]
    ok_door, err_door = leg_door_interact(d, args, rows)
    if err_door:
        print("\n".join(rows))
        print("SETUP: %s" % err_door, file=sys.stderr)
        return 2
    ok_negative, err_negative = leg_negative(d, args, rows)
    if err_negative:
        print("\n".join(rows))
        print("SETUP: %s" % err_negative, file=sys.stderr)
        return 2

    ok = ok_door and ok_negative
    rows.append(row("result", "PASS" if ok else "FAIL"))
    print("\n".join(rows))
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())
