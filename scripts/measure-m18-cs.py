#!/usr/bin/env python3
"""measure-m18-cs.py - CS-01..CS-21 window evidence driver for the M18 owner pack.

Launches the release binary on the real X11/XWayland display, drives it with the
bindings declared by the M18 acceptance protocol (XTEST: WASD/Shift, 1-4 panels,
R/L/E/G, left click, Space) and records for every step: the literal window name,
an `import` capture of the client area, its sha256, pixel statistics and the
`M* VE` lines the engine printed while that step ran.

Nothing under crates/** is touched. Measurement tooling only (0 code change).

Usage:
  measure-m18-cs.py --bin <path> --out <dir> [--smoke]
"""
import argparse
import hashlib
import json
import os
import subprocess
import sys
import time

from PIL import Image, ImageChops, ImageStat
from Xlib import X, XK, display
from Xlib.ext import xtest
from Xlib.protocol import event as xevent

WS = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
NET_WM_NAME = "_NET_WM_NAME"
UTF8 = "UTF8_STRING"
NET_ACTIVE_WINDOW = "_NET_ACTIVE_WINDOW"


# ── window discovery ───────────────────────────────────────────────────────
def name_of(d, win):
    """_NET_WM_NAME (UTF-8, what KWin/winit set) then the WM_NAME fallback."""
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
    """Depth-first walk of the real tree (children of `win`, not of the root)."""
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


def find_window(d, needle="DAGGERFALL", timeout=180.0, proc=None):
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
    """Real keyboard focus on :0 (KWin/Wayland session).

    XTEST delivers key events to the X input focus, and under a Wayland session
    that focus is owned by the compositor: `set_input_focus` alone is a no-op.
    The accepted route is an EWMH `_NET_ACTIVE_WINDOW` client message with
    source indication 2 (pager/user) - KWin then activates the XWayland surface.
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
    except Exception as exc:  # pragma: no cover - WM dependent
        print("activate: %s" % exc)
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
            break
        time.sleep(0.2)
    # Pointer inside the client area (mouse-look needs the cursor on the window).
    try:
        geo = win.get_geometry()
        c = win.translate_coords(root, geo.width // 2, geo.height // 2)
        root.warp_pointer(c.dst_x, c.dst_y)
        d.sync()
    except Exception:
        pass
    try:
        focus = d.get_input_focus().focus
        return (focus.id if hasattr(focus, "id") else focus) == win.id
    except Exception:
        return False


# ── input ──────────────────────────────────────────────────────────────────
# Route measured on this workstation (2026-10-22, KWin/Wayland + XWayland):
#   * XTEST (`xtest.fake_input`) is INERT for this client: focus is correct
#     (`get_input_focus()` returns the game window) and the events are accepted,
#     yet the engine never sees them - measured with three different key codes
#     and with the window both explicitly focused and EWMH-activated.
#   * Core key events delivered straight to the window (`send_event`) DO reach
#     the engine: the M19 locomotion log advances (pos 411.0 -> 415.7).
#   * Mouse-look cannot be driven at all: the demo reads raw device motion
#     (`DeviceEvent::MouseMotion`), which neither `send_event` nor
#     `warp_pointer` produces. Every run therefore keeps the spawn yaw (3.93)
#     and the sweep steps below move the player instead of turning the camera.
KEY_PRESS_MASK = X.KeyPressMask
KEY_RELEASE_MASK = X.KeyReleaseMask


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


def hold(d, win, name, secs, shift=False):
    if shift:
        send_key(d, win, "Shift_L", True)
    send_key(d, win, name, True)
    time.sleep(secs)
    send_key(d, win, name, False)
    if shift:
        send_key(d, win, "Shift_L", False)
    time.sleep(0.35)


def click(d, win, button=1):
    """Core ButtonPress/Release to the window (kept as evidence, see run.json)."""
    root = d.screen().root
    for cls, mask in ((xevent.ButtonPress, X.ButtonPressMask), (xevent.ButtonRelease, X.ButtonReleaseMask)):
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
                detail=button,
            ),
            event_mask=mask,
        )
        d.sync()
        time.sleep(0.12)
    time.sleep(0.30)


# ── capture ────────────────────────────────────────────────────────────────
def stats(png):
    with open(png, "rb") as fh:
        raw = fh.read()
    im = Image.open(png).convert("RGB")
    w, h = im.size
    n = float(w * h)
    red, green, blue = im.split()
    max_channel = ImageChops.lighter(ImageChops.lighter(red, green), blue)
    hist = max_channel.histogram()
    # `nonblack` = share of pixels whose brightest channel is > 8 (same rule the
    # earlier M18 runs used), computed from the histogram instead of per pixel.
    nonblack = sum(hist[9:]) / n
    bright = sum(ImageStat.Stat(im).mean) / 3.0
    return {
        "png": png,
        "sha256": hashlib.sha256(raw).hexdigest(),
        "bytes": len(raw),
        "width": w,
        "height": h,
        "unique_colors": len(im.getcolors(maxcolors=1 << 24) or []),
        "nonblack_ratio": round(nonblack, 4),
        "mean_brightness": round(bright, 2),
    }


def capture(d, win, wid, out, label):
    """Client-area capture, with a root-crop fallback for composited windows.

    Interpreted as a screen proof only when something actually painted: a
    black canvas is recorded as such, never as evidence of content.
    """
    direct = os.path.join(out, "%s.png" % label)
    subprocess.run(
        ["import", "-window", str(wid), direct], check=True, capture_output=True
    )
    best = stats(direct)
    best["method"] = "import -window %s" % hex(wid)
    if best["nonblack_ratio"] < 0.01:
        # Fallback: what the compositor shows in the window rectangle.
        geo = win.get_geometry()
        pt = win.translate_coords(d.screen().root, 0, 0)
        crop = os.path.join(out, "%s.rootcrop.png" % label)
        try:
            subprocess.run(
                [
                    "import", "-window", "root",
                    "-crop", "%dx%d+%d+%d" % (geo.width, geo.height, pt.dst_x, pt.dst_y),
                    "+repage", crop,
                ],
                check=True, capture_output=True,
            )
            alt = stats(crop)
            if alt["nonblack_ratio"] > best["nonblack_ratio"]:
                alt["method"] = "import -window root -crop %dx%d+%d+%d" % (
                    geo.width, geo.height, pt.dst_x, pt.dst_y
                )
                os.replace(crop, direct)
                best = alt
            else:
                os.unlink(crop)
        except Exception as exc:
            best["fallback_error"] = str(exc)
    for key in ("png",):
        best[key] = os.path.relpath(best[key], WS)
    return best


# ── step bookkeeping ───────────────────────────────────────────────────────
def log_json(rec, path):
    with open(path, "a") as fh:
        fh.write(json.dumps(rec, sort_keys=True) + "\n")


def new_log_slice(logfile, offset):
    with open(logfile, "rb") as fh:
        fh.seek(offset)
        chunk = fh.read().decode("utf-8", "replace")
    return [ln for ln in chunk.splitlines() if ln.strip()], os.path.getsize(logfile)


def last_pos(lines):
    for ln in reversed(lines):
        if ln.startswith("M19 VE: pos="):
            return ln
    return None


class Driver:
    def __init__(self, d, win, wid, out, logfile, steps_path, steps):
        self.d, self.win, self.wid = d, win, wid
        self.out, self.logfile = out, logfile
        self.steps_path, self.steps = steps_path, steps
        self.offset = os.path.getsize(logfile)

    def step(self, label, note):
        lines, self.offset = new_log_slice(self.logfile, self.offset)
        try:
            title = name_of(self.d, self.win)
        except Exception:
            title = None
        rec = {
            "label": label,
            "note": note,
            "ts": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
            "window_name": title,
            "ve_lines": [ln for ln in lines if " VE:" in ln][:40],
            "audio_lines": [ln for ln in lines if "SND" in ln or "sound" in ln.lower()][:10],
            "pos": last_pos(lines),
            "log_lines": len(lines),
        }
        rec.update(capture(self.d, self.win, self.wid, self.out, label))
        self.steps.append(rec)
        log_json(rec, self.steps_path)
        print(
            "%-28s sha=%s colors=%d nonblack=%.3f pos=%s"
            % (label, rec["sha256"][:12], rec["unique_colors"], rec["nonblack_ratio"], rec["pos"])
        )
        return rec


def campaign(drv, d):
    """Fixed order: locomotion first (the demo's own M22 retaliation drains the
    character while the window stays open), then the 1996 panels, then skills."""
    win = drv.win
    drv.step("00-launch", "primer frame tras activar la ventana (CS-20: no hay menu principal)")
    time.sleep(6)
    drv.step("01-spawn-cs01", "spawn: calle retail, map-pixel del titulo (CS-01/CS-04/CS-13)")

    # Input self-test: without a position delta every later step is meaningless.
    before = drv.step("02-pre-walk", "pos de referencia antes de inyectar W")["pos"]
    hold(d, win, "w", 1.5)
    after = drv.step("03-post-walk-cs01", "W 1.5 s: delta de pos (auto-test de inyeccion)")["pos"]
    input_ok = bool(before and after and before != after)

    # Traversal: the camera yaw is fixed (see the input note), so the city is
    # sampled by moving, not by turning.
    for label, key, secs, shift, note in (
        ("04-run-forward", "w", 4.0, True, "Shift+W 4 s: carrera calle adentro (CS-01/CS-04/CS-13)"),
        ("05-strafe-right", "d", 3.0, False, "D 3 s: desplazamiento lateral (fachadas/calzada)"),
        ("06-backward", "s", 3.0, False, "S 3 s: marcha atras (misma calle, otro angulo de props)"),
        ("07-strafe-left", "a", 3.0, False, "A 3 s: otro lateral (materiales y vegetacion de borde)"),
    ):
        hold(d, win, key, secs, shift=shift)
        drv.step(label, note)

    for key, name in (("1", "cs17-inventario"), ("2", "cs16-quests"), ("3", "cs-menu"), ("4", "cs-hud")):
        press(d, win, key)
        time.sleep(0.8)
        drv.step("08-panel-%s" % name, "panel 1996 con la tecla %s (IMG/CIF/FNT de ARENA2)" % key)
    press(d, win, "1")
    time.sleep(0.8)
    drv.step("09-panel-back", "tecla 1: vuelta al panel por defecto sobre la capa de mundo")

    press(d, win, "space")
    time.sleep(0.6)
    drv.step("10-jump", "Space: salto (modo del motor en el titulo)")
    press(d, win, "c")
    time.sleep(0.8)
    drv.step("11-sneak-c", "C: sigilo/agachado (altura de ojo distinta)")
    press(d, win, "c")
    time.sleep(0.6)
    press(d, win, "r")
    time.sleep(1.0)
    drv.step("12-running-r", "R: uso real de la skill Running (M20)")
    press(d, win, "l")
    time.sleep(1.5)
    drv.step("13-levitate-l", "L: Levitate (M23) - vista elevada del terreno")
    press(d, win, "e")
    time.sleep(1.0)
    drv.step("14-interact-e", "E: interactuar con NPC (CS-15 dialogo)")
    press(d, win, "g")
    time.sleep(1.0)
    drv.step("15-bash-g", "G: bash de puerta (CS-02/03 interior)")
    for _ in range(3):
        click(d, win, 1)
        time.sleep(0.3)
    drv.step("16-attack-cs21", "3 clicks al area cliente: golpe (CS-21 / CS-07 SoundClip)")
    drv.step("17-final", "estado final de la sesion medida")
    return input_ok


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--bin", required=True)
    ap.add_argument("--out", required=True)
    ap.add_argument("--data-root", default=os.path.expanduser("~/Games/Daggerfall/app"))
    ap.add_argument("--timeout", type=float, default=240.0)
    ap.add_argument("--smoke", action="store_true", help="solo ventana + auto-test de input")
    args = ap.parse_args()

    binary = os.path.abspath(args.bin)
    out = os.path.abspath(args.out)
    os.makedirs(out, exist_ok=True)
    logfile = os.path.join(out, "app.log")
    steps_path = os.path.join(out, "steps.jsonl")
    for stale in (logfile, steps_path):
        if os.path.exists(stale):
            os.unlink(stale)

    try:
        d = display.Display()
    except Exception as exc:
        print("NO-DISPLAY: %s" % exc)
        return 3
    xtest_available = d.has_extension("XTEST")

    env = dict(os.environ)
    env["DF_APP_ROOT"] = args.data_root
    env.setdefault("DISPLAY", os.environ.get("DISPLAY", ":0"))
    with open(binary, "rb") as fh:
        sha = hashlib.sha256(fh.read()).hexdigest()
    head = subprocess.run(
        ["git", "rev-parse", "HEAD"], cwd=WS, capture_output=True, text=True
    ).stdout.strip()

    steps, ok, focus_ok, wid, nm = [], False, False, None, None
    with open(logfile, "wb") as sink:
        proc = subprocess.Popen([binary], stdout=sink, stderr=subprocess.STDOUT, env=env, cwd=WS)

    try:
        found = find_window(d, timeout=args.timeout, proc=proc)
        if not found:
            rc = proc.poll()
            print("NO-WINDOW after %ss (app rc=%s)" % (args.timeout, rc))
            return 4
        win, wid, nm = found
        print("WINDOW %s %s" % (hex(wid), nm))
        focus_ok = activate(d, win)
        print("FOCUS_ACQUIRED=%s" % focus_ok)
        time.sleep(1.5)
        # The title is rewritten by the demo every frame: re-read it here as the
        # canonical machine-readable state (mode/band/HP), not from a stale copy.
        nm = name_of(d, win) or nm
        drv = Driver(d, win, wid, out, logfile, steps_path, steps)
        if args.smoke:
            drv.step("00-smoke-launch", "smoke")
            before = drv.step("01-smoke-pre", "pos antes de W")["pos"]
            hold(d, win, "w", 1.5)
            after = drv.step("02-smoke-post", "pos tras W 1.5 s")["pos"]
            print("SMOKE input_ok=%s before=%s after=%s" % (before != after, before, after))
            ok = before != after
        else:
            ok = campaign(drv, d)
    finally:
        proc.terminate()
        try:
            proc.wait(timeout=10)
        except Exception:
            proc.kill()

    caps = [s for s in steps]
    bands, audio, modes = set(), set(), set()
    for s in caps:
        for ln in s.get("ve_lines", []):
            if "terrain band" in ln:
                bands.add(ln.split("terrain band ")[1].split(" ")[0])
            if "mode=" in ln:
                modes.add(ln.split("mode=")[1].split(" ")[0])
        for ln in s.get("audio_lines", []):
            audio.add(ln.strip())
    run = {
        "head": head,
        "binary": os.path.relpath(binary, WS),
        "binary_sha256": sha,
        "data_root": args.data_root,
        "window_id_hex": hex(wid),
        "window_name": nm,
        "display": os.environ.get("DISPLAY"),
        "wayland_display": os.environ.get("WAYLAND_DISPLAY"),
        "session_type": os.environ.get("XDG_SESSION_TYPE"),
        "focus_acquired": focus_ok,
        "xtest_available": xtest_available,
        "input_route": "core KeyPress/KeyRelease via send_event (XTEST inert here; mouse-look needs raw motion)",
        "input_delivered": ok,
        "steps": len(caps),
        "captures": caps,
        "bands_seen": sorted(bands),
        "modes_seen": sorted(modes),
        "audio_lines_seen": sorted(audio)[:20],
        "ts": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
    }
    with open(os.path.join(out, "run.json"), "w") as fh:
        json.dump(run, fh, indent=2, sort_keys=True)
    print("RUN OK captures=%d input_delivered=%s head=%s" % (len(caps), ok, head[:12]))
    return 0


if __name__ == "__main__":
    sys.exit(main())
