#!/usr/bin/env python3
"""fg01-world-doors.py — WORLD probe of the retail doors (`GAP-FG01-02`).

Measures in the real window what the byte census says exists:

    * on finishing composing the city the app prints
        `FG01-02 VE: world doors composed=<n> retail=<n> scenario=<n> ...`
        (`m18_demo.rs`, FG-01 instrumentation);
    * `--door-spawn` (`m18_window.rs:363`) places the player at
        `DOOR_STANDOFF_M = 1.0` m from the FIRST door of the composition;
    * `E` is `action.interact` -> `m18_door_interact` (`m18_doors.rs`), which resolves
        reach (`DOOR_REACH_M = 2.5`) against `DoorWorld` and transitions to the interior
        `N0000055.RDB` reusing `DoorLoader`;
    * the X11 driver is the one accepted on this workstation (`scripts/playthrough-p1-door.py`):
        `_NET_WM_NAME` + `_NET_ACTIVE_WINDOW` + `set_input_focus`, CORE keyboard via
        `send_event` (XTEST is inert for this client).

Legs
    * `retail-interact`: `--door-spawn`, **without** `M18_CS14` -> the door that opens
        is data, not scenario. Row `retail-door-interact` PASS only if the line
        `FG01 VE: interact -> interior N0000055.RDB ... digest <16 hex>` appears.
    * `negative-no-interact`: same setup, `W` for 1 s and **no** `E` -> it does not
        transition (the other leg's transition is attributable to `E`).
    * `scenario-compose`: `M18_CS14=1 --door-spawn` -> the scenario door still
        enters the composition (`scenario=1`) and the count adds up
        (`composed = retail + scenario`).

Literal RED: without data doors the `retail-interact` leg prints
`door-spawn=refused` (the app refuses to open the world elsewhere, exit 2) and
`retail-door-interact=FAIL` -> this probe exits **1**.

Output: normalised rows `row <name>=<value>` (no timestamps, durations or
PIDs). Two runs print identical bytes. Raw logs stay in `--out`.

Exit: 0 all rows PASS · 1 some FAIL · 2 usage/IO (no DISPLAY, no ARENA2,
binary absent, broken build, instrument line absent).

Usage:
    scripts/fg01-world-doors.py [--skip-build] [--out DIR] [--bin PATH]
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
DEFAULT_OUT = os.path.join(WS, ".sintergic", "artifacts", "fg01-02", "world-doors")
DEFAULT_DATA_ROOT = os.path.expanduser("~/Games/Daggerfall/app")

NET_WM_NAME = "_NET_WM_NAME"
UTF8 = "UTF8_STRING"
NET_ACTIVE_WINDOW = "_NET_ACTIVE_WINDOW"

# `m18_doors.rs` — constants of the binding this probe exercises.
REACH_M = 2.5
STANDOFF_M = 1.0
INTERIOR_BLOCK_NAME = "N0000055.RDB"

# ── lineas de evidencia ────────────────────────────────────────────────────
RE_WORLD = re.compile(
    r"FG01-02 VE: world doors composed=(\d+) retail=(\d+) scenario=(\d+) "
    r"buildings=(\d+) buildings_with_doors=(\d+)"
)
RE_SPAWN = re.compile(
    r"FG01 VE: door-spawn eye=\((-?[0-9.]+),(-?[0-9.]+),(-?[0-9.]+)\) yaw=(-?[0-9.]+) "
    r"\((-?[0-9.]+)°\) · door at \((-?[0-9.]+),(-?[0-9.]+)\) (-?[0-9.]+) m away · "
    r"doors in composition (\d+) · interior (\S+)"
)
RE_SPAWN_REFUSED = re.compile(r"FG01 VE: door-spawn refused — (.+)$")
RE_REGISTRY = re.compile(
    r"FG01 VE: door registry (\d+) door\(s\) · interior (\S+) \(block (\d+)\) · reach "
    r"([0-9.]+) m · exterior digest ([0-9a-f]{16})"
)
RE_NO_DOORS = re.compile(r"FG01 VE: 0 doors in the composition")
RE_CS14 = re.compile(r"M18 VE: CS14 scenario")
RE_BASH_AT = re.compile(r"M28 VE: bash at ([0-9]+\.[0-9]) m")
RE_BASH_FAR = re.compile(
    r"M28 VE: bash — no retail door within [0-9.]+ m \(nearest at ([0-9]+\.[0-9]) m\)"
)
RE_BASH_CRIME = re.compile(r"M28 VE: bash — crime ")
RE_INTERIOR = re.compile(
    r"FG01 VE: interact → interior (\S+) · block (\d+) · digest ([0-9a-f]{16}) · scene=(\S+)"
)
RE_INTERIOR_REJECTED = re.compile(r"FG01 VE: interact rejected — (.+)$")


# ── descubrimiento de ventana (`playthrough-p1-door.py`) ───────────────────
def name_of(d, win):
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
def send_key(d, win, name, down):
    root = d.screen().root
    code = d.keysym_to_keycode(XK.string_to_keysym(name))
    cls = xevent.KeyPress if down else xevent.KeyRelease
    mask = X.KeyPressMask if down else X.KeyReleaseMask
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


# ── app log ──────────────────────────────────────────────────────────
def read_log(path):
    try:
        with open(path, "r", encoding="utf-8", errors="replace") as fh:
            return fh.read().splitlines()
    except FileNotFoundError:
        return []


def wait_lines(path, pattern, timeout, proc=None):
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
    deadline = time.time() + timeout
    while time.time() < deadline:
        if proc is not None and proc.poll() is not None:
            return None
        current = title(d, win)
        if needle in current:
            return current
        time.sleep(0.25)
    return None


# ── one leg: one process, one window ────────────────────────────────────
class Session:
    """One demo process; `cs14` decides whether the scenario door is injected."""

    def __init__(self, d, binary, data_root, out_dir, leg, cs14, door_spawn=True):
        self.d = d
        self.binary = binary
        self.data_root = data_root
        self.log_path = os.path.join(out_dir, "app-%s.log" % leg)
        self.leg = leg
        self.cs14 = cs14
        self.door_spawn = door_spawn
        self.proc = None
        self.win = None
        self.sink = None

    def __enter__(self):
        env = dict(os.environ)
        env["DF_APP_ROOT"] = self.data_root
        env.setdefault("DISPLAY", os.environ.get("DISPLAY", ":0"))
        if self.cs14:
            env["M18_CS14"] = "1"
        else:
            env.pop("M18_CS14", None)
        if os.path.exists(self.log_path):
            os.unlink(self.log_path)
        self.sink = open(self.log_path, "wb")
        argv = [self.binary]
        if self.door_spawn:
            argv.append("--door-spawn")
        self.proc = subprocess.Popen(
            argv, stdout=self.sink, stderr=subprocess.STDOUT, env=env, cwd=WS
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

    def lines(self, pattern):
        return [ln for ln in read_log(self.log_path) if pattern.search(ln)]

    def window(self, timeout):
        found = find_window(self.d, timeout=timeout, proc=self.proc)
        if found is None:
            return False
        self.win = found[0]
        activate(self.d, self.win)
        return True

    def enter_playing(self, timeout=60.0):
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


def world_counts(lines):
    """(composed, retail, scenario, buildings, with_doors) del instrumento."""
    for ln in lines:
        hit = RE_WORLD.search(ln)
        if hit:
            return tuple(int(g) for g in hit.groups())
    return None


def emit_world(rows, counts):
    if counts is None:
        rows.append(row("world-line", "absent"))
        return False
    rows.append(row("world-line", "present"))
    rows.append(row("world-composed", counts[0]))
    rows.append(row("world-retail", counts[1]))
    rows.append(row("world-scenario", counts[2]))
    rows.append(row("world-buildings", counts[3]))
    rows.append(row("world-buildings-with-doors", counts[4]))
    return True


def bash_distance(lines):
    for ln in lines:
        hit = RE_BASH_AT.search(ln)
        if hit:
            return hit.group(1), True
        hit = RE_BASH_FAR.search(ln)
        if hit:
            return hit.group(1), False
        if RE_BASH_CRIME.search(ln):
            return None, True
    return None, False


# ── piernas ────────────────────────────────────────────────────────────────
def leg_retail_interact(d, args, rows):
    """Leg A: the DATA door opens with `E` (no scenario door)."""
    with Session(d, args.bin, args.data_root, args.out, "retail-interact", cs14=False) as s:
        world = wait_lines(s.log_path, RE_WORLD, args.timeout, s.proc)
        counts = world_counts(world)
        if counts is None:
            return False, "NO-WORLD-LINE (retail-interact): instrumento ausente"
        emit_world(rows, counts)
        rows.append(row("leg-cs14", "off"))
        rows.append(row("leg-spawn", "%.1f m" % STANDOFF_M))

        refused = wait_lines(s.log_path, RE_SPAWN_REFUSED, 30.0, s.proc)
        spawned = wait_lines(s.log_path, RE_SPAWN, 30.0, s.proc)
        if refused or not spawned:
            rows.append(row("door-spawn", "refused"))
            reason = refused[0] if refused else "no door-spawn line"
            rows.append(row("door-spawn-reason", reason.split("— ")[-1][:60].replace(" ", "_")))
            rows.append(row("door-sites", 0))
            rows.append(row("phase", "not-opened"))
            rows.append(row("reach", "unknown"))
            rows.append(row("reach-pass", "no"))
            rows.append(row("interact-line", "absent"))
            rows.append(row("interior-block", "absent"))
            rows.append(row("interior-digest", "absent"))
            rows.append(row("title-interior", "no"))
            rows.append(row("retail-door-interact", "FAIL"))
            return False, None

        spawn = RE_SPAWN.search(spawned[0])
        rows.append(row("door-spawn", "spawned"))
        rows.append(row("door-standoff-m", spawn.group(8)))
        rows.append(row("door-sites", spawn.group(9)))
        rows.append(row("door-spawn-interior", spawn.group(10)))
        if not s.window(args.timeout):
            rows.append(row("phase", "no-window"))
            rows.append(row("retail-door-interact", "FAIL"))
            return False, None
        if not s.enter_playing():
            rows.append(row("phase", "no-playing"))
            rows.append(row("retail-door-interact", "FAIL"))
            return False, None
        rows.append(row("phase", "PLAYING"))

        # `G` (`m28_door_bash`): independent proof that the player is at
        # a DATA retail door (the crime branch is only reachable within
        # reach).
        bash = []
        for _ in range(3):
            press(d, s.win, "G", hold=0.10)
            bash = wait_lines(s.log_path, re.compile(r"M28 VE: bash"), 4.0, s.proc)
            if bash:
                break
        distance, within = bash_distance(bash)
        rows.append(row("reach", distance if distance else "crime"))
        rows.append(row("reach-pass", "yes" if within else "no"))

        # `E`: `action.interact` -> `m18_door_interact`.
        press(d, s.win, "E", hold=0.10)
        interior = wait_lines(s.log_path, RE_INTERIOR, 6.0, s.proc)
        current = title(d, s.win)
        title_interior = "scene=Interior" in current
        if interior:
            hit = RE_INTERIOR.search(interior[0])
            rows.append(row("interact-line", "present"))
            rows.append(row("interior-block", hit.group(1)))
            rows.append(row("interior-digest", hit.group(3)))
            rows.append(row("interior-scene", hit.group(4)))
        else:
            rows.append(row("interact-line", "absent"))
            rows.append(row("interior-block", "absent"))
            rows.append(row("interior-digest", "absent"))
            rows.append(row("interior-scene", "absent"))
            rejected = s.lines(RE_INTERIOR_REJECTED)
            if rejected:
                rows.append(row("interact-rejected", "yes"))
        rows.append(row("title-interior", "yes" if title_interior else "no"))
        ok = bool(interior) and title_interior
        rows.append(row("retail-door-interact", "PASS" if ok else "FAIL"))
        return ok, None


def leg_negative(d, args, rows):
    """Pierna B: mismo setup, `W` 1 s y sin `E` -> no transiciona."""
    with Session(d, args.bin, args.data_root, args.out, "negative-no-interact", cs14=False) as s:
        wait_lines(s.log_path, RE_WORLD, args.timeout, s.proc)
        refused = wait_lines(s.log_path, RE_SPAWN_REFUSED, 20.0, s.proc)
        spawned = wait_lines(s.log_path, RE_SPAWN, 20.0, s.proc)
        if refused or not spawned:
            rows.append(row("negative-door-spawn", "refused"))
            rows.append(row("negative-interact-line", "absent"))
            rows.append(row("negative-no-interact", "PASS"))
            return True, None
        if not s.window(args.timeout):
            return False, "NO-WINDOW (negative-no-interact)"
        if not s.enter_playing():
            return False, "NO-PLAYING (negative-no-interact)"
        hold(d, s.win, "w", 1.0)
        time.sleep(1.0)
        present = bool(s.lines(RE_INTERIOR))
        rows.append(row("negative-door-spawn", "spawned"))
        rows.append(row("negative-interact-line", "present" if present else "absent"))
        rows.append(row("negative-no-interact", "FAIL" if present else "PASS"))
        return not present, None


def leg_scenario_compose(d, args, rows):
    """Pierna C: `M18_CS14=1` sigue componiendo su puerta junto al dato."""
    with Session(d, args.bin, args.data_root, args.out, "scenario-compose", cs14=True) as s:
        world = wait_lines(s.log_path, RE_WORLD, args.timeout, s.proc)
        counts = world_counts(world)
        if counts is None:
            return False, "NO-WORLD-LINE (scenario-compose)"
        composed, retail, scenario = counts[0], counts[1], counts[2]
        cs14 = wait_lines(s.log_path, RE_CS14, 30.0, s.proc)
        rows.append(row("scenario-line", "present" if cs14 else "absent"))
        rows.append(row("scenario-door", scenario))
        rows.append(row("scenario-retail", retail))
        consistent = scenario == 1 and composed == retail + scenario
        rows.append(row("scenario-counts", "consistent" if consistent else "inconsistent"))
        ok = bool(cs14) and consistent
        rows.append(row("scenario-compose", "PASS" if ok else "FAIL"))
        return ok, None


# ── corrida ────────────────────────────────────────────────────────────────
def build(args):
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

    args.bin = os.path.abspath(args.bin)
    args.data_root = os.path.abspath(args.data_root)
    args.out = os.path.abspath(args.out)

    if not os.path.isdir(os.path.join(args.data_root, "ARENA2")):
        print("row error=no-arena2\nrow result=FAIL")
        print("IO: %s/ARENA2 no existe (--data-root)" % args.data_root, file=sys.stderr)
        return 2
    build_ok, build_err = build(args)
    if not build_ok:
        print("row error=build\nrow result=FAIL")
        print("IO: build roto: %s" % (build_err or ""), file=sys.stderr)
        return 2
    if not os.path.isfile(args.bin):
        print("row error=no-binary\nrow result=FAIL")
        print("IO: %s no existe (--bin)" % args.bin, file=sys.stderr)
        return 2
    try:
        d = display.Display()
    except Exception as exc:
        print("row error=no-display\nrow result=FAIL")
        print("IO: sin DISPLAY: %s" % exc, file=sys.stderr)
        return 2

    os.makedirs(args.out, exist_ok=True)
    with open(args.bin, "rb") as fh:
        sha = hashlib.sha256(fh.read()).hexdigest()

    rows = [row("binary-sha256", sha)]
    ok_retail, err_retail = leg_retail_interact(d, args, rows)
    if err_retail:
        print("\n".join(rows))
        print("SETUP: %s" % err_retail, file=sys.stderr)
        return 2
    ok_negative, err_negative = leg_negative(d, args, rows)
    if err_negative:
        print("\n".join(rows))
        print("SETUP: %s" % err_negative, file=sys.stderr)
        return 2
    ok_scenario, err_scenario = leg_scenario_compose(d, args, rows)
    if err_scenario:
        print("\n".join(rows))
        print("SETUP: %s" % err_scenario, file=sys.stderr)
        return 2

    ok = ok_retail and ok_negative and ok_scenario
    rows.append(row("result", "PASS" if ok else "FAIL"))
    print("\n".join(rows))
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())
