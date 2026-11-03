#!/usr/bin/env python3
"""measure-menu-screen.py — A/B driver for the menu screen of the M18 window.

Measures the product requirement of `CR-625ff0ea` (lane MenuBeforeWorld): the
main menu is **its own screen**, i.e. while the shell is in `MENU`/`CREATE` the
window paints the retail menu art and **zero pixels of the 3D world** (no scene
colours in the side strips, no scene colours in the top/bottom bands), and the
world does not simulate (0 ticks). With the player inside (`PLAYING`) the world
is back, the HUD is visible and the simulation counters are > 0.

This is measurement tooling, not a gate: it never writes under `crates/`, never
packages retail bytes (only PNG captures + a JSON of derived numbers) and never
pushes or tags. The judge of the menu surface is `scripts/menu-window-001.py`
(owned by another lane); this driver produces the BEFORE/AFTER table that
`menu-window-001.py` does not cover (per-strip world pixels + ticks).

Usage:
  python3 scripts/measure-menu-screen.py --binary target/debug/daggerfall-app \
      --out .sintergic/artifacts/menu-screen/after --label AFTER
  python3 scripts/measure-menu-screen.py --binary /var/tmp/menu-before/daggerfall-app \
      --out .sintergic/artifacts/menu-screen/before --label BEFORE --enter

Exit: 0 measured · 2 environment (no DISPLAY / binary / retail data) ·
      3 NOT-RUN (the window or the capture could not be taken — 0 PNG = 0 claim).
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import shutil
import subprocess
import sys
import threading
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent

# Regiones del defecto medido (ventana 1280x720, panel retail 320x200 a 2x).
SIDE_STRIP_PX = 320
BAND_PX = 160
CENTER = (640, 400)
CORNER_PX = 40

# Un píxel es «negro» cuando sus tres canales valen <= este umbral (ruido de
# sincronía del servidor X, no contenido).
BLACK_MAX = 8


def sha256_file(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


# ── X11 ────────────────────────────────────────────────────────────────────
def _x11():
    from Xlib import X, display  # import local: el A/B sin X no lo necesita

    return X, display


def name_of(d, win) -> str:
    for prop, typ in (("_NET_WM_NAME", "UTF8_STRING"), ("WM_NAME", "STRING")):
        try:
            val = win.get_full_property(d.intern_atom(prop), d.intern_atom(typ))
        except Exception:
            val = None
        if val and val.value:
            raw = val.value
            return raw.decode("utf-8", "replace") if isinstance(raw, bytes) else str(raw)
    try:
        nm = win.get_wm_name()
        return str(nm) if nm else ""
    except Exception:
        return ""


def walk(d, win, depth=0, acc=None):
    acc = [] if acc is None else acc
    if depth > 4:
        return acc
    try:
        children = win.query_tree().children
    except Exception:
        return acc
    for child in children:
        nm = name_of(d, child)
        if nm:
            acc.append((child, nm))
        walk(d, child, depth + 1, acc)
    return acc


def owner_pid(d, win):
    try:
        prop = win.get_full_property(d.intern_atom("_NET_WM_PID"), 0)
    except Exception:
        return None
    if prop is None or not prop.value:
        return None
    try:
        return int(prop.value[0])
    except Exception:
        return None


def activate(d, X, win):
    """Best-effort real keyboard focus (KWin/XWayland: EWMH client message)."""
    from Xlib.protocol import event as xevent

    root = d.screen().root
    try:
        root.send_event(
            xevent.ClientMessage(
                window=win,
                client_type=d.intern_atom("_NET_ACTIVE_WINDOW"),
                data=(32, [2, X.CurrentTime, 0, 0, 0]),
            ),
            event_mask=X.SubstructureRedirectMask | X.SubstructureNotifyMask,
        )
        d.sync()
    except Exception as exc:
        print("activate: %s" % exc)
    try:
        win.configure(stack_mode=X.Above)
        win.set_input_focus(X.RevertToParent, X.CurrentTime)
        d.sync()
    except Exception:
        pass


def key(d, X, win, keysym_name: str):
    """Pulsa una tecla por el camino que usan los gates de la casa: evento Core
    sintético **directo a la ventana** (`shell-001.send_key`). XTEST va al foco
    global y en la primera corrida el primer `Return` se perdió (el menú acabó
    cerrando por `quit`): aquí no hace falta foco."""
    from Xlib import XK
    from Xlib.protocol import event as xevent

    root = d.screen().root
    code = d.keysym_to_keycode(XK.string_to_keysym(keysym_name))
    for down in (True, False):
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
        time.sleep(0.08)


# ── captura + métricas ────────────────────────────────────────────────────
def capture(display_name: str, win_id: int, out: Path) -> bool:
    out.parent.mkdir(parents=True, exist_ok=True)
    done = subprocess.run(
        ["import", "-display", display_name, "-window", str(win_id), str(out)],
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
    )
    if done.returncode != 0:
        print("capture: import rc=%d %s" % (done.returncode, done.stderr.strip()[:200]))
        return False
    return out.is_file() and out.stat().st_size > 0


def region_metrics(img, box):
    crop = img.crop(box).convert("RGB")
    px = list(crop.getdata())
    total = len(px)
    nonblack = sum(1 for p in px if max(p) > BLACK_MAX)
    distinct = len(set(px))
    top = {}
    for p in px:
        top[p] = top.get(p, 0) + 1
    top_colours = sorted(top.items(), key=lambda kv: -kv[1])[:3]
    corner = (box[0], box[1], box[0] + CORNER_PX, box[1] + CORNER_PX)
    corner_px = list(crop_image(img, corner).convert("RGB").getdata()) if corner[2] <= box[2] else px
    return {
        "box": list(box),
        "px": total,
        "distinct": distinct,
        "nonblack": round(nonblack / total, 4) if total else 0.0,
        "nonblack_px": nonblack,
        "top": [
            {"rgb": list(rgb), "frac": round(count / total, 4)} for rgb, count in top_colours
        ],
        "corner_top": [list(sorted(corner_px, key=lambda p: -sum(p))[0])],
    }


def crop_image(img, box):
    return img.crop(box)


def measure_png(path: Path) -> dict:
    from PIL import Image

    img = Image.open(path)
    img.load()
    w, h = img.size
    center_w, center_h = CENTER
    cx, cy = (w - center_w) // 2, (h - center_h) // 2
    regions = {
        "window": (0, 0, w, h),
        "center": (cx, cy, cx + center_w, cy + center_h),
        "strip_left": (0, 0, SIDE_STRIP_PX, h),
        "strip_right": (w - SIDE_STRIP_PX, 0, w, h),
        "band_top": (0, 0, w, BAND_PX),
        "band_bottom": (0, h - BAND_PX, w, h),
        "corner_tl": (0, 0, CORNER_PX, CORNER_PX),
        "corner_tr": (w - CORNER_PX, 0, w, CORNER_PX),
        "corner_bl": (0, h - CORNER_PX, CORNER_PX, h),
        "corner_br": (w - CORNER_PX, h - CORNER_PX, w, h),
    }
    metrics = {"size": [w, h], "sha256": sha256_file(path), "regions": {}}
    for name, box in regions.items():
        metrics["regions"][name] = region_metrics(img, box)
    # «píxeles de mundo»: cualquier píxel no negro fuera del panel central.
    outside = 0
    outside_total = 0
    px = list(img.convert("RGB").getdata())
    for y in range(h):
        for x in range(w):
            inside_center = cx <= x < cx + center_w and cy <= y < cy + center_h
            if inside_center:
                continue
            outside_total += 1
            if max(px[y * w + x]) > BLACK_MAX:
                outside += 1
    metrics["world_pixels_outside_center"] = outside
    metrics["outside_center_px"] = outside_total
    metrics["world_pixels_pct"] = round(100.0 * outside / outside_total, 4) if outside_total else 0.0
    return metrics


# ── ejecución ─────────────────────────────────────────────────────────────
def run(args) -> int:
    display_name = os.environ.get("DISPLAY", "")
    if not display_name:
        print("MEASURE: NOT-RUN — DISPLAY is unset (no window to measure)")
        return 2
    binary = Path(args.binary)
    if not binary.is_file():
        print("MEASURE: NOT-RUN — binary %s absent" % binary)
        return 2
    out = Path(args.out)
    out.mkdir(parents=True, exist_ok=True)
    log_path = out / "app.log"
    log = log_path.open("w", encoding="utf-8")
    started = time.time()

    env = dict(os.environ)
    child = subprocess.Popen(
        [str(binary)],
        cwd=str(ROOT),
        env=env,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        text=True,
        bufsize=1,
    )
    lines: list[tuple[float, str]] = []
    lock = threading.Lock()

    def pump():
        assert child.stdout is not None
        for line in child.stdout:
            stamp = time.time() - started
            line = line.rstrip("\n")
            with lock:
                lines.append((stamp, line))
            log.write("%8.3f %s\n" % (stamp, line))
            log.flush()

    reader = threading.Thread(target=pump, daemon=True)
    reader.start()

    X, display = _x11()
    d = display.Display(display_name)
    root = d.screen().root

    def find():
        for win, name in walk(d, root):
            if "DAGGERFALL" not in name.upper() and "SHELL" not in name.upper():
                continue
            pid = owner_pid(d, win)
            if pid is None or pid == child.pid:
                return win, name
        return None, ""

    result = {
        "label": args.label,
        "binary": str(binary),
        "binary_sha256": sha256_file(binary),
        "head": subprocess.run(
            ["git", "-C", str(ROOT), "rev-parse", "HEAD"], capture_output=True, text=True
        ).stdout.strip(),
        "started_unix": started,
    }
    captures: dict[str, dict] = {}
    deadline = started + args.title_deadline
    win = None
    while time.time() < deadline:
        if child.poll() is not None:
            print("MEASURE: NOT-RUN — the process exited before any window (rc=%s)" % child.poll())
            _tail(lines)
            return 3
        win, name = find()
        if win is not None and "SHELL MENU" in name.upper():
            result["t_menu_s"] = round(time.time() - started, 3)
            result["title_menu"] = name
            break
        time.sleep(0.05)
    if win is None or "t_menu_s" not in result:
        print("MEASURE: NOT-RUN — no window titled `SHELL MENU` within %.0f s" % args.title_deadline)
        _tail(lines)
        return 3

    activate(d, X, win)
    time.sleep(args.settle)
    png = out / "menu.png"
    if not capture(display_name, win.id, png):
        print("MEASURE: NOT-RUN — `import -window %d` produced no PNG" % win.id)
        _tail(lines)
        return 3
    result["window_id"] = hex(win.id)
    captures["menu"] = measure_png(png)
    result["title_at_capture"] = name_of(d, win)

    if args.enter:
        for attempt in range(1, args.enter_attempts + 1):
            activate(d, X, win)
            key(d, X, win, "Return")
            entered_deadline = time.time() + args.enter_deadline
            while time.time() < entered_deadline:
                if "SHELL PLAYING" in name_of(d, win).upper():
                    break
                time.sleep(0.05)
            if "SHELL PLAYING" in name_of(d, win).upper():
                result["enter_attempts"] = attempt
                break
        if "SHELL PLAYING" in name_of(d, win).upper():
            result["t_playing_s"] = round(time.time() - started, 3)
            result["title_playing"] = name_of(d, win)
            time.sleep(args.playing_settle)
            png_play = out / "playing.png"
            if capture(display_name, win.id, png_play):
                captures["playing"] = measure_png(png_play)
            # teclas 1-6: cada una debe declarar su panel (evidencia del log)
            for keysym in args.panel_keys.split(","):
                activate(d, X, win)
                key(d, X, win, keysym)
                time.sleep(0.4)
            png_keys = out / "playing-keys.png"
            if capture(display_name, win.id, png_keys):
                captures["playing_keys"] = measure_png(png_keys)
            png_back = out / "menu-again.png"
        else:
            result["enter_failed"] = True

    result["captures"] = captures
    time.sleep(0.2)
    child.terminate()
    try:
        child.wait(timeout=10)
    except subprocess.TimeoutExpired:
        child.kill()
    with lock:
        result["lines"] = [(round(t, 3), line) for t, line in lines]
    result["log"] = str(log_path)

    # contadores de simulación por fase (evidencia del requisito 2)
    ticks = [
        line for _, line in result["lines"] if "M18 VE: sim ticks=" in line
    ]
    result["sim_lines"] = ticks
    result["sim_menu"] = [t for t in ticks if "phase=MENU" in t]
    result["sim_playing"] = [t for t in ticks if "phase=PLAYING" in t]
    (out / "measure.json").write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
    report(result)
    return 0


def _tail(lines, n=25):
    for stamp, line in lines[-n:]:
        print("%8.3f %s" % (stamp, line))


def report(result: dict) -> None:
    print("== %s · %s ==" % (result["label"], result["head"][:12]))
    print(
        "binary %s sha256=%s"
        % (result["binary"], (result.get("binary_sha256") or "-")[:16])
    )
    print("t_menu=%.3fs · t_playing=%ss" % (result["t_menu_s"], result.get("t_playing_s", "n/a")))
    for name, cap in result["captures"].items():
        reg = cap["regions"]
        print(
            "capture %-12s %dx%d sha256=%s world-outside-center=%d px (%.4f%%)"
            % (
                name,
                cap["size"][0],
                cap["size"][1],
                cap["sha256"][:16],
                cap["world_pixels_outside_center"],
                cap["world_pixels_pct"],
            )
        )
        for region in ("strip_left", "strip_right", "band_top", "band_bottom", "center"):
            r = reg[region]
            print(
                "   %-12s distinct=%-5d nonblack=%.4f top=%s"
                % (
                    region,
                    r["distinct"],
                    r["nonblack"],
                    ", ".join(str(tuple(t["rgb"])) for t in r["top"]),
                )
            )
    print("sim MENU lines: %d · PLAYING lines: %d" % (len(result["sim_menu"]), len(result["sim_playing"])))
    for line in (result["sim_menu"][:2] + result["sim_playing"][:2]):
        print("   " + line)


def main(argv) -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--binary", default=str(ROOT / "target" / "debug" / "daggerfall-app"))
    ap.add_argument("--out", required=True)
    ap.add_argument("--label", default="RUN")
    ap.add_argument("--title-deadline", type=float, default=120.0)
    ap.add_argument("--settle", type=float, default=2.0)
    ap.add_argument("--enter", action="store_true", help="pulsa Return y mide PLAYING")
    ap.add_argument("--enter-attempts", type=int, default=6)
    ap.add_argument("--enter-deadline", type=float, default=6.0)
    ap.add_argument("--playing-settle", type=float, default=3.0)
    ap.add_argument("--panel-keys", default="1,2,3,4,5,6")
    return run(ap.parse_args(argv))


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
