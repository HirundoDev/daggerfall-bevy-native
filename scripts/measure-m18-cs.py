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
import math
import os
import re
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
# Evidence keeps the binary only under this size (CS captures are small; a
# larger one is archived as sha256 + stats, never silently dropped).
MAX_PNG_BYTES = 1_500_000


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


GUARD_PROJ_RE = re.compile(
    r"M18 VE: CS14 guard #(\d+) \(session enemy (\d+)\) screen=\((-?\d+),(-?\d+)\) "
    r"size=\((\d+)x(\d+)\) depth=([0-9.]+) m in_frame=(true|false)"
)


def guard_projections(lines):
    """Projected screen rectangles the window prints for every arriving guard
    (`m18_cs14_face_guards`). The title alone says a guard EXISTS; these lines
    say WHERE it landed, so the capture can be checked on pixels."""
    out = []
    for line in lines or []:
        hit = GUARD_PROJ_RE.search(line)
        if not hit:
            continue
        out.append({
            "guard": int(hit.group(1)),
            "session_enemy": int(hit.group(2)),
            "x": int(hit.group(3)),
            "y": int(hit.group(4)),
            "w": int(hit.group(5)),
            "h": int(hit.group(6)),
            "depth_m": float(hit.group(7)),
            "in_frame": hit.group(8) == "true",
        })
    return out


CS13_PROJ_RE = re.compile(
    r"M18 VE: CS13 npc #(\d+) screen=\((\d+),(\d+)\) size=\((\d+)x(\d+)\) "
    r"depth=([0-9.]+) m at \((-?[0-9.]+),(-?[0-9.]+)\)"
)


def cs13_projections(lines):
    """`m18_cs13_face_crowd` prints the projected rectangle of the three nearest
    NPC billboards in the aimed direction; the capture is checked on those
    pixels, not on the window title."""
    out = []
    for line in lines or []:
        hit = CS13_PROJ_RE.search(line)
        if not hit:
            continue
        out.append({
            "npc": int(hit.group(1)),
            "x": int(hit.group(2)),
            "y": int(hit.group(3)),
            "w": int(hit.group(4)),
            "h": int(hit.group(5)),
            "depth_m": float(hit.group(6)),
        })
    return out


def sprite_contrast(png, rect):
    """Mean |difference| between the projected guard rectangle and the equally
    sized patch one width to its side. A retail knight sprite does not differ
    from the wall by ~0; an empty rect does. `None` when the rect leaves the
    image (black padding would fake a difference)."""
    if not png or not os.path.exists(png):
        return None
    if rect["w"] < 8 or rect["h"] < 8:
        return None
    im = Image.open(png).convert("RGB")
    width, height = im.size
    x0 = rect["x"] - rect["w"] // 2
    y0 = rect["y"] - rect["h"] // 2
    bx0 = x0 + rect["w"]
    if x0 < 0 or y0 < 0 or x0 + rect["w"] > width or bx0 + rect["w"] > width \
            or y0 + rect["h"] > height:
        return None
    guard = im.crop((x0, y0, x0 + rect["w"], y0 + rect["h"]))
    neighbour = im.crop((bx0, y0, bx0 + rect["w"], y0 + rect["h"]))
    diff = ImageChops.difference(guard, neighbour)
    return {
        "mean_abs_diff": round(sum(ImageStat.Stat(diff).mean) / 3.0, 2),
        "guard_colors": len(guard.getcolors(maxcolors=1 << 24) or []),
        "neighbour_colors": len(neighbour.getcolors(maxcolors=1 << 24) or []),
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
    # A capture bigger than ~1.5 MB is archived as sha256 + stats only (the
    # binary is dropped and the record says so): the repo keeps the evidence,
    # not the ballast.
    if best["bytes"] > MAX_PNG_BYTES:
        try:
            os.unlink(os.path.join(out, "%s.png" % label))
        except OSError:
            pass
        best["omitted"] = "png > %d B: sha256 + stats only" % MAX_PNG_BYTES
        best["png"] = None
    for key in ("png",):
        if best.get(key):
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
        # Offset 0: the startup evidence (M18 spawn / origin / yaw lines and the
        # M14 music line) is printed before the first capture and must not be lost.
        self.offset = 0
        self.startup_lines = []
        # Last `M19 VE: pos=` line seen. The engine prints it every 2 s, so a
        # step taken inside that window has no new line of its own; carrying the
        # last one keeps the before/after deltas measurable (flagged `carried`).
        self.last_pos_line = None

    def wait_paint(self, timeout=90.0, min_colors=100):
        """Wait until the client area actually painted the composition (a black
        or single-colour first surface is never archived as the MENU evidence)."""
        deadline = time.time() + timeout
        while time.time() < deadline:
            rec = capture(self.d, self.win, self.wid, self.out, "_paintprobe")
            if rec["nonblack_ratio"] > 0.01 and rec["unique_colors"] >= min_colors:
                probe = os.path.join(self.out, "_paintprobe.png")
                if os.path.exists(probe):
                    os.unlink(probe)
                return True
            time.sleep(0.5)
        return False

    def seen_shas(self):
        """Every sha256 already archived by this campaign run."""
        return {s.get("sha256") for s in self.steps if s.get("sha256")}

    def step(self, label, note, shot=True, forbid=(), tries=1, wait=0.6):
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
            "ve_lines": [ln for ln in lines if " VE:" in ln][:80],
            "audio_lines": [ln for ln in lines if "SND" in ln or "sound" in ln.lower()][:10],
            "pos": last_pos(lines),
            "log_lines": len(lines),
        }
        if rec["pos"]:
            self.last_pos_line = rec["pos"]
        elif self.last_pos_line:
            rec["pos"] = self.last_pos_line
            rec["pos_carried"] = True
        if shot:
            rec.update(capture(self.d, self.win, self.wid, self.out, label))
            # A state screenshot must be a NEW frame: the client surface lags a
            # frame, so the grab right after a state change can repeat the
            # previous bytes (measured 2026-10-24: three pairs of captures came
            # out byte-identical and the second one proved nothing).
            attempt = 1
            while attempt < tries and rec.get("sha256") in forbid:
                time.sleep(wait)
                rec.update(capture(self.d, self.win, self.wid, self.out, label))
                attempt += 1
            rec["capture_attempts"] = attempt
            if rec.get("sha256") in forbid:
                rec["stale"] = True
        else:
            # Repeated interactions inside one check do not each deserve a PNG:
            # 60 bash attempts produced 60 near-identical frames (measured
            # 2026-10-24). The check keeps ONE frame, taken after it settles.
            rec.update({"sha256": None, "unique_colors": 0, "nonblack_ratio": 0.0})
        self.steps.append(rec)
        log_json(rec, self.steps_path)
        print(
            "%-28s sha=%s colors=%d nonblack=%.3f pos=%s"
            % (label, (rec["sha256"] or "-")[:12], rec["unique_colors"],
               rec["nonblack_ratio"], rec["pos"])
        )
        return rec


# ── B1-d / B1-c: phase, spawn and combat parsing ───────────────────────────
DOC_SPAWN_POS = (411.0, 1.6, 411.0)
DOC_SPAWN_YAW = 3.927  # 225 deg; the engine prints 2 decimals (3.93)


def parse_pos(line):
    """`M19 VE: pos=(411.0,1.6,411.0) yaw=3.93 · mode=...` -> tuple or None."""
    if not line or "pos=(" not in line:
        return None
    try:
        body = line.split("pos=(", 1)[1]
        xyz, rest = body.split(")", 1)
        x, y, z = (float(v) for v in xyz.split(","))
        yaw = None
        if "yaw=" in rest:
            yaw = float(rest.split("yaw=", 1)[1].split(" ")[0])
        return x, y, z, yaw
    except Exception:
        return None


TARGET_RE = re.compile(r"TARGET\s+(.+?)\s+HP\s+(\d+)\s*/\s*(\d+)")
# `M18 VE: yaw 230.0° (4.0143 rad) · , / . step 5°` — printed by the key system
# itself, so it dates the press (unlike the 2 s M19 telemetry).
YAW_KEY_RE = re.compile(r"M18 VE: yaw\s+(-?\d+(?:\.\d+)?)°\s+\((-?\d+(?:\.\d+)?) rad\)")


def yaw_key_of(lines):
    value = None
    for ln in lines or []:
        m = YAW_KEY_RE.search(ln)
        if m:
            value = float(m.group(2))
    return value


def parse_target(title):
    """`... - TARGET EnemyClass 9001 HP 43/100` -> dict or None."""
    if not title:
        return None
    m = TARGET_RE.search(title)
    if not m:
        return None
    return {"label": m.group(1), "hp": int(m.group(2)), "max": int(m.group(3))}


def phase_of(title):
    up = (title or "").upper()
    if "PLAYING" in up:
        return "PLAYING"
    if "MENU" in up:
        return "MENU"
    return None


def spawn_check(pos, title):
    """Verify the documented spawn (411.0,1.6,411.0) and yaw 3.93 (225 deg)."""
    out = {"documented": {"pos": list(DOC_SPAWN_POS), "yaw": DOC_SPAWN_YAW}}
    if pos:
        x, y, z, yaw = pos
        out["observed"] = {"pos": [x, y, z], "yaw": yaw}
        out["pos_delta_m"] = round(
            ((x - DOC_SPAWN_POS[0]) ** 2 + (y - DOC_SPAWN_POS[1]) ** 2 + (z - DOC_SPAWN_POS[2]) ** 2) ** 0.5, 3
        )
        out["yaw_delta_rad"] = None if yaw is None else round(abs(yaw - DOC_SPAWN_YAW), 4)
        out["ok"] = out["pos_delta_m"] <= 0.05 and (yaw is None or out["yaw_delta_rad"] <= 0.01)
    else:
        out["ok"] = False
    return out


def yaw_of(lines):
    for ln in reversed(lines):
        got = parse_pos(ln)
        if got and got[3] is not None:
            return got[3]
    return None


# ── band spawn (CS-08/CS-09/CS-10) ─────────────────────────────────────────
# `--spawn band:<ocean|beach|land>` moves the EYE onto the band: the city hull,
# the collision grid, the NPC placement and the session state are the default
# composition's (the engine prints the resolved viewpoint as evidence).
SPAWN_OVERRIDE_RE = re.compile(
    r"M18 VE: spawn-override band:(\w+) · eye=\(([-\d.]+),([-\d.]+),([-\d.]+)\) · "
    r"at \(([-\d.]+),([-\d.]+)\) m = ([-\d.]+) m outside the city hull"
)
CAMPAIGN_SPAWN = {
    "band-ocean": "band:ocean",
    "band-beach": "band:beach",
    "band-mountain": "band:land",
}


def parse_spawn_override(line):
    """`M18 VE: spawn-override band:ocean · eye=(…) · at (…) = N m outside the city hull`."""
    if not line:
        return None
    m = SPAWN_OVERRIDE_RE.search(line)
    if not m:
        return None
    return {
        "band": m.group(1),
        "eye": [float(m.group(2)), float(m.group(3)), float(m.group(4))],
        "at": [float(m.group(5)), float(m.group(6))],
        "outside_hull_m": float(m.group(7)),
    }


def band_of(line):
    """`M19 VE: … · terrain band ocean · blocked=false` -> ('ocean', False)."""
    if not line or "terrain band " not in line:
        return None, None
    band = line.split("terrain band ", 1)[1].split(" ")[0]
    blocked = None
    if "blocked=" in line:
        token = line.split("blocked=", 1)[1].split(" ")[0]
        blocked = token.startswith("true")
    return band, blocked


def spawn_check_band(pos, override_line, wanted):
    """Band spawn: the engine's evidence line, where the window shows the eye,
    outside the hull and on the requested band."""
    parsed = parse_spawn_override(override_line)
    out = {"documented": {"spawn": wanted}, "override_line": override_line, "parsed": parsed}
    if not parsed:
        out["ok"] = False
        out["why"] = "no spawn-override line from the engine"
        return out
    wanted_band = wanted.split(":", 1)[1]
    out["band"] = parsed["band"]
    out["outside_hull_m"] = parsed["outside_hull_m"]
    if not pos:
        out["ok"] = False
        out["why"] = "no M19 position line"
        return out
    x, y, z, yaw = pos
    out["observed"] = {"pos": [x, y, z], "yaw": yaw}
    eye = parsed["eye"]
    out["pos_delta_m"] = round(
        ((x - eye[0]) ** 2 + (y - eye[1]) ** 2 + (z - eye[2]) ** 2) ** 0.5, 3
    )
    out["ok"] = (
        parsed["band"] == wanted_band
        and parsed["outside_hull_m"] >= 24.0
        # The engine prints the eye with 2 decimals and M19 the position with 1.
        and out["pos_delta_m"] <= 0.5
    )
    return out


def campaign(drv, d, kind, scenario=False):
    """B1-d/B1-c aware campaign.

    Prologue (every campaign): MENU frame (CS-20) -> `Enter` -> PLAYING with the
    documented spawn -> audio startup lines (CS-06) -> input self-test -> `,`/`.`
    yaw keys. Then the campaign body:
      * capital  — locomotion + the attack (clicks, then `F`) with the HUD target
                   before/after (CS-21 / CS-07);
      * coastal  — 4 heading captures around the coastal origin + the `T`
                   toggle band counts (CS-08 ocean / CS-09 beach);
      * mountain — 4 heading captures around the inland ridge + Levitate view
                   (CS-10 silhouette).
    """
    win = drv.win
    report = {"campaign": kind, "checks": {}}
    # CS-08/09/10: a band campaign measures a `--spawn band:<…>` viewpoint, so
    # the documented retail spawn is NOT the expected state (and the input
    # self-test cannot be a forward walk when the eye stands on water).
    spawn_wanted = CAMPAIGN_SPAWN.get(kind)

    # ── 00 · MENU (world frozen) — CS-20 ──────────────────────────────
    painted = drv.wait_paint()
    time.sleep(1.0)
    menu = drv.step(
        "00-menu-cs20",
        "primer frame: MENU (mundo congelado, 'Enter enters the world') — CS-20",
    )
    menu_title = menu.get("window_name") or ""
    menu_phase = phase_of(menu_title)
    time.sleep(2.0)
    idle = drv.step("00b-menu-idle", "2 s en MENU: 0 lineas M19 VE = el mundo no avanza (CS-20)")
    idle_m19 = [ln for ln in idle.get("ve_lines", []) if ln.startswith("M19 VE:")]
    music = [ln for ln in menu.get("ve_lines", []) if "M14 VE:" in ln and "music" in ln]
    report["checks"]["cs20_menu"] = {
        "first_paint": painted,
        "title": menu_title,
        "phase": menu_phase,
        "ok": menu_phase == "MENU" and "enter enters the world" in menu_title.lower(),
        "world_ticks_while_menu": len(idle_m19),
    }
    report["checks"]["cs06_music"] = {
        "lines": music,
        "ok": any("music context" in ln for ln in music),
    }
    print("MENU title=%r phase=%s m19_lines_in_menu=%d music=%d"
          % (menu_title, menu_phase, len(idle_m19), len(music)))

    # ── 01 · Enter -> PLAYING with the documented spawn — B1-d ────────
    press(d, win, "Return", hold=0.12)
    time.sleep(3.2)
    play = drv.step(
        "01-enter-playing",
        "Enter: MENU -> PLAYING; spawn documentado (411.0,1.6,411.0) yaw 3.927",
    )
    play_title = play.get("window_name") or ""
    override_line = next((ln for ln in drv.startup_lines if "spawn-override" in ln), None)
    if spawn_wanted:
        spawn = spawn_check_band(play.get("pos") and parse_pos(play["pos"]),
                                 override_line, spawn_wanted)
    else:
        spawn = spawn_check(play.get("pos") and parse_pos(play["pos"]), play_title)
    report["checks"]["cs20_menu"]["enter_playing"] = {
        "title": play_title,
        "phase": phase_of(play_title),
        "ok": phase_of(play_title) == "PLAYING",
    }
    report["spawn"] = spawn
    print("PLAYING title=%r spawn_ok=%s observed=%s override=%r"
          % (play_title, spawn["ok"], spawn.get("observed"), override_line))

    # ── input self-test (without a position delta nothing below is valid) ──
    if spawn_wanted == "band:ocean":
        # CS-08: every translation from the ocean viewpoint is water (blocked by
        # design), so the input path is proven with the yaw keys instead: the
        # engine prints `M18 VE: yaw …` from its own key system.
        yb = drv.step("02-pre-walk", "yaw de referencia antes de las teclas , (auto-test)")
        y0 = yaw_key_of(yb.get("ve_lines", [])) or yaw_of(yb.get("ve_lines", []))
        for _ in range(6):
            press(d, win, "comma")
        time.sleep(0.6)
        ya = drv.step("03-post-walk-cs01", "6x ',': +30 grados (auto-test de inyeccion)")
        y1 = yaw_key_of(ya.get("ve_lines", [])) or yaw_of(ya.get("ve_lines", []))
        d_yaw = None if (y0 is None or y1 is None) else round(y1 - y0, 4)
        input_ok = d_yaw is not None and abs(d_yaw - 6 * math.radians(5.0)) < 0.05
        report["input_self_test"] = {"route": "yaw keys (translation blocked by water)",
                                     "before": y0, "after": y1, "delta_rad": d_yaw,
                                     "expected_rad": round(6 * math.radians(5.0), 4),
                                     "ok": input_ok}
    elif spawn_wanted:
        # Beach/land viewpoint: backwards (`S`) walks away from the water and
        # downhill, so the same production input path must advance.
        before = drv.step("02-pre-walk", "pos de referencia antes de inyectar S")
        hold(d, win, "s", 1.5)
        after = drv.step("03-post-walk-cs01", "S 1.5 s: delta de pos (auto-test de inyeccion)")
        p0, p1 = parse_pos(before.get("pos")), parse_pos(after.get("pos"))
        delta = None
        if p0 and p1:
            delta = round(((p1[0] - p0[0]) ** 2 + (p1[2] - p0[2]) ** 2) ** 0.5, 3)
        input_ok = bool(delta and delta > 0.2)
        report["input_self_test"] = {"route": "S (backwards)", "before": before.get("pos"),
                                     "after": after.get("pos"), "delta_m": delta, "ok": input_ok}
    else:
        before = drv.step("02-pre-walk", "pos de referencia antes de inyectar W")
        hold(d, win, "w", 1.5)
        after = drv.step("03-post-walk-cs01", "W 1.5 s: delta de pos (auto-test de inyeccion)")
        p0, p1 = parse_pos(before.get("pos")), parse_pos(after.get("pos"))
        delta = None
        if p0 and p1:
            delta = round(((p1[0] - p0[0]) ** 2 + (p1[2] - p0[2]) ** 2) ** 0.5, 3)
        input_ok = bool(delta and delta > 0.2)
        report["input_self_test"] = {"before": before.get("pos"), "after": after.get("pos"),
                                     "delta_m": delta, "ok": input_ok}

    # ── campaign body ─────────────────────────────────────────────────
    if kind == "capital":
        # CS-21 / CS-07 · attack FIRST: the demo's own M22 retaliation drains
        # the character in ~30 s, so the strike burst runs before the long
        # locomotion beat. First the click route (evidence), then `F`.
        pre = drv.step("11-attack-pre-cs21", "titulo antes de golpear (sin TARGET)")
        for _ in range(2):
            click(d, win, 1)
            time.sleep(0.4)
        drv.step("12-attack-click", "2 clicks core via send_event (ruta de raton; CS-21)")
        hits, bursts = [], []
        for burst in (4, 8, 12, 16):
            while len(bursts) < burst:
                press(d, win, "f", hold=0.06)
                time.sleep(0.22)
                bursts.append(1)
            step = drv.step("13-attack-key-f-%02d" % burst,
                            "F x%d: ataque por tecla (CS-21 HUD target / CS-07 clip)" % burst)
            hits += [ln for ln in step.get("ve_lines", []) if "melee swing" in ln and "hit=true" in ln]
            tgt = parse_target(step.get("window_name"))
            if tgt:
                # A landed strike: four more swings show the HP moving.
                for _ in range(4):
                    press(d, win, "f", hold=0.06)
                    time.sleep(0.22)
                after = drv.step("14-attack-after-hit",
                                 "4 golpes mas con TARGET en el HUD (HP n/100)")
                hits += [ln for ln in after.get("ve_lines", [])
                         if "melee swing" in ln and "hit=true" in ln]
                break
        # B1-a: `,` / `.` yaw keys, measured after the strike (the retaliation
        # clock runs; the keys cost ~4 s).
        yb = drv.step("04-yaw-before", "yaw antes de las teclas , / . (B1-a)")
        y0 = yaw_of(yb.get("ve_lines", []))
        if y0 is None and yb.get("pos") and parse_pos(yb["pos"]):
            y0 = parse_pos(yb["pos"])[3]
        for _ in range(6):
            press(d, win, "comma")
        time.sleep(0.6)
        yc = drv.step("05-yaw-after-comma", "6x ',' = +30 grados (M18 VE: yaw por tecla)")
        y1 = yaw_key_of(yc.get("ve_lines", []))
        if y1 is None:
            y1 = yaw_of(yc.get("ve_lines", []))
        for _ in range(6):
            press(d, win, "period")
        time.sleep(0.6)
        yp = drv.step("06-yaw-after-period", "6x '.' = vuelta al yaw original")
        y2 = yaw_key_of(yp.get("ve_lines", []))
        if y2 is None:
            y2 = yaw_of(yp.get("ve_lines", []))
        report["yaw_keys"] = {
            "before": y0, "after_comma": y1, "after_period": y2,
            "delta_comma_rad": None if (y0 is None or y1 is None) else round(y1 - y0, 4),
            "delta_period_rad": None if (y1 is None or y2 is None) else round(y2 - y1, 4),
            "expected_step_rad": round(6 * math.radians(5.0), 4),
            "ok": None if (y0 is None or y1 is None or y2 is None)
            else abs((y1 - y0) - 6 * math.radians(5.0)) < 0.05 and abs(y2 - y0) < 0.05,
        }
        print("YAW before=%s comma=%s period=%s" % (y0, y1, y2))


        # Locomotion evidence after the strike (CS-01/CS-04 stay measured).
        hold(d, win, "w", 2.0, shift=True)
        drv.step("15-run-forward", "Shift+W 2 s: carrera calle adentro (CS-01/CS-04/CS-13)")
        drv.step("19-final", "estado final de la campana capital")
        tgt = parse_target(drv.steps[-1].get("window_name"))
        swings = [ln for st in drv.steps for ln in st.get("ve_lines", []) if "melee swing" in ln]
        hud = [ln for st in drv.steps for ln in st.get("ve_lines", []) if "HUD reflects" in ln]
        report["checks"]["cs21_attack"] = {
            "target_before": parse_target(pre.get("window_name")),
            "target_after": tgt,
            "swings": len(swings),
            "hits": len([ln for ln in swings if "hit=true" in ln]),
            "ok": tgt is not None,
        }
        report["checks"]["cs07_hit"] = {
            "swings": len(swings),
            "hit_lines": [ln for ln in swings if "hit=true" in ln][:6],
            "hud_lines": hud[:6],
            "ok": any("hit=true" in ln for ln in swings),
        }
    elif kind == "coastal":
        # CS-08 ocean / CS-09 beach: 4 headings 90 deg apart from the coastal
        # origin (`--origin 207,215`, the pixel ORIGIN-001 found).
        for idx in range(4):
            drv.step("20-heading-%03d" % (idx * 90),
                     "vista %d grados desde el origen costero (CS-08 oceano / CS-09 playa)" % (idx * 90))
            if idx < 3:
                for _ in range(18):
                    press(d, win, "comma")
                time.sleep(0.5)
        # `T` toggles the terrain origin: capital (0/0 bands) -> coastal
        # (ocean/beach/land band counts printed by the engine).
        press(d, win, "t")
        time.sleep(1.8)
        drv.step("21-toggle-t-capital", "T: origen -> capital (bandas 0/0 esperadas)")
        press(d, win, "t")
        time.sleep(1.8)
        drv.step("22-toggle-t-coastal", "T: origen -> costero (conteos de banda del engine)")
        drv.step("29-final", "estado final de la campana costera")
    elif kind == "mountain":
        for idx in range(4):
            drv.step("30-heading-%03d" % (idx * 90),
                     "vista %d grados desde el origen de montana (CS-10 silueta)" % (idx * 90))
            if idx < 3:
                for _ in range(18):
                    press(d, win, "comma")
                time.sleep(0.5)
        press(d, win, "l")
        time.sleep(2.5)
        drv.step("31-levitate-view", "L: Levitate -> vista elevada de la silueta (CS-10)")
        press(d, win, "l")
        time.sleep(1.0)
        drv.step("39-final", "estado final de la campana montana")
    elif kind == "band-ocean":
        # CS-08 · ocean: the sea surface and its level under an eye that stands
        # ON the band, then the not-walkable attempt (position before/after).
        drv.step("40-ocean-view",
                 "vista al mar desde el punto de banda oceano (CS-08 nivel de mar)")
        before = drv.step("41-advance-pre", "pos antes de avanzar hacia el agua (CS-08)")
        hold(d, win, "w", 3.0)
        after = drv.step("42-advance-into-water",
                         "W 3 s hacia el agua: la posicion no cambia (CS-08 no caminable)")
        p0, p1 = parse_pos(before.get("pos")), parse_pos(after.get("pos"))
        delta = None
        if p0 and p1:
            delta = round(((p1[0] - p0[0]) ** 2 + (p1[2] - p0[2]) ** 2) ** 0.5, 3)
        band_after, blocked_after = band_of(after.get("pos"))
        carried = bool(after.get("pos_carried"))
        ok = ((not carried) and delta is not None and delta <= 0.05
              and band_after == "ocean" and blocked_after is True)
        report["checks"]["cs08_water_not_walkable"] = {
            "before": before.get("pos"), "after": after.get("pos"), "delta_m": delta,
            "band": band_after, "blocked": blocked_after, "pos_carried": carried, "ok": ok,
        }
        print("CS-08 water: delta=%s band=%s blocked=%s carried=%s ok=%s"
              % (delta, band_after, blocked_after, carried, ok))
        for _ in range(18):
            press(d, win, "comma")
        time.sleep(0.5)
        drv.step("43-ocean-heading-090", "yaw +90 grados: la costa lateral (CS-08)")
        drv.step("49-final", "estado final de la campana banda oceano")
    elif kind == "band-beach":
        # CS-09 · beach: sand underfoot, open sea in front, then the strip along
        # the coast and the slope inland (4 headings, 90 deg apart).
        drv.step("50-beach-strip",
                 "franja de playa al pie de la camara y mar al frente (CS-09)")
        for idx in (1, 2, 3):
            for _ in range(18):
                press(d, win, "comma")
            time.sleep(0.5)
            drv.step("5%d-beach-heading-%03d" % (idx, idx * 90),
                     "yaw +%d grados: franja y pendiente de la playa (CS-09)" % (idx * 90))
        drv.step("59-final", "estado final de la campana banda playa")
    elif kind == "band-mountain":
        # CS-10 · mountain: the relief silhouette from a vantage point on the
        # land band (the peak is what the yaw faces), 4 headings + a walk.
        drv.step("60-mountain-silhouette",
                 "vista al pico de relieve desde el punto de banda tierra (CS-10)")
        for idx in (1, 2, 3):
            for _ in range(18):
                press(d, win, "comma")
            time.sleep(0.5)
            drv.step("6%d-mountain-heading-%03d" % (idx, idx * 90),
                     "yaw +%d grados: silueta/relieve (CS-10)" % (idx * 90))
        before = drv.step("64-walk-pre", "pos antes de S 1.5 s (tierra caminable)")
        hold(d, win, "s", 1.5)
        after = drv.step("65-walk-post", "S 1.5 s: la tierra avanza (control de CS-08)")
        p0, p1 = parse_pos(before.get("pos")), parse_pos(after.get("pos"))
        delta = None
        if p0 and p1:
            delta = round(((p1[0] - p0[0]) ** 2 + (p1[2] - p0[2]) ** 2) ** 0.5, 3)
        band_after, _ = band_of(after.get("pos"))
        report["checks"]["cs10_land_walk"] = {
            "before": before.get("pos"), "after": after.get("pos"), "delta_m": delta,
            "band": band_after, "ok": bool(delta and delta > 0.2),
        }
        drv.step("69-final", "estado final de la campana banda tierra/montana")
    elif kind == "parity":
        # CS-12..CS-19: the B3 tanda visible in the window (weather, population,
        # guards, retail overlays, map panels) at the documented spawn.
        parity_checks(report, drv, d, win, scenario=scenario)
    return input_ok, report


def parity_checks(report, drv, d, win, scenario=False):
    """CS-12..CS-19 of the B3 tanda, measured in the running window.

    Every capture is one `Driver.step` (PNG + sha256 + the ` VE:` lines of its
    log slice). `ok` is only true when the engine's OWN evidence line proves the
    state; the visual verdict stays the owner's (class C, never auto-signed).
    """
    checks = report["checks"]

    def ve(step, needle):
        return [ln for ln in step.get("ve_lines", []) if needle in ln]

    def drops_of(lines):
        best = 0
        for ln in lines:
            if "drops=" not in ln:
                continue
            frag = ln.split("drops=", 1)[1].split()[0].strip(",")
            try:
                best = max(best, int(frag))
            except ValueError:
                continue
        return best

    def settle():
        """Discards one capture: the client surface lags a frame.

        Measured 2026-10-23: the first capture right after a key shows the
        PREVIOUS state (journal/dialogue/panel switches arrived one step late),
        so every state screenshot is taken after one throwaway grab.
        """
        capture(d, drv.win, drv.wid, drv.out, "_settle")
        probe = os.path.join(drv.out, "_settle.png")
        if os.path.exists(probe):
            os.unlink(probe)
        time.sleep(0.4)

    def rec_of(step, ok, **extra):
        out = {"ok": ok, "label": step.get("label"), "window_name": step.get("window_name"),
               "png": step.get("png"), "sha256": step.get("sha256"),
               "nonblack_ratio": step.get("nonblack_ratio")}
        out.update(extra)
        return out

    # ── CS-13 · street population (24 NPCs) ─────────────────────────────
    street = drv.step("70-street-population-cs13",
                      "calle del demo: 24 NPCs con caption (CS-13)",
                      forbid=drv.seen_shas(), tries=8, wait=0.7)
    title = street.get("window_name") or ""
    # The aim runs on the first `Playing` frames and then re-prints its
    # rectangles as the campaign walks, so the numbers live in earlier records,
    # not in the CS-13 one: the last set printed wins.
    cs13_lines = [ln for s in drv.steps for ln in (s.get("ve_lines") or [])
                  if "CS13 npc #" in ln]
    latest = {}
    for rec in cs13_projections(cs13_lines):
        latest[rec["npc"]] = rec
    npc_rects = [latest[key] for key in sorted(latest)]
    npc_contrasts = [c for c in (sprite_contrast(street.get("png"), r) for r in npc_rects) if c]
    npc_contrast = max((c["mean_abs_diff"] for c in npc_contrasts), default=0.0)
    checks["cs13_population"] = rec_of(
        street, "POP 24" in title and street.get("sha256") not in
        {s.get("sha256") for s in drv.steps[:-1]}
        and bool(npc_rects) and npc_contrast >= 6.0,
        title=title, capture_attempts=street.get("capture_attempts"),
        npc_projections=npc_rects, sprite_contrast=npc_contrasts,
        aimed=[ln for s in drv.steps for ln in (s.get("ve_lines") or [])
               if "CS13 scenario" in ln][-1:])
    print("CS-13 pop_ok=%s npc_rects=%d contrast=%.2f title=%r"
          % (checks["cs13_population"]["ok"], len(npc_rects), npc_contrast, title))

    # ── CS-16 · retail journal overlay (measured alone, before any walking) ─
    press(d, win, "i")
    settle()
    jou = drv.step("75-journal-overlay-cs16",
                   "overlay de journal retail QBN/QRC (CS-16)")
    jou_on = ve(jou, "overlay journal ON")
    checks["cs16_journal"] = rec_of(
        jou, bool(jou_on) and jou.get("sha256") != street.get("sha256"),
        lines=(jou_on or ve(jou, "overlay journal"))[-1:])
    print("CS-16 ok=%s" % checks["cs16_journal"]["ok"])
    press(d, win, "i")
    time.sleep(0.6)

    # ── CS-15 · retail dialogue overlay ─────────────────────────────────
    press(d, win, "j")
    settle()
    dia = drv.step("74-dialogue-overlay-cs15",
                   "overlay de dialogo retail TEXT.RSC (CS-15)")
    dia_on = ve(dia, "overlay dialogue ON")
    checks["cs15_dialogue"] = rec_of(
        dia,
        bool(dia_on) and dia.get("sha256") != jou.get("sha256"),
        lines=(dia_on or ve(dia, "overlay dialogue"))[-1:])
    print("CS-15 ok=%s" % checks["cs15_dialogue"]["ok"])
    press(d, win, "j")
    time.sleep(0.6)

    # ── CS-12 · precipitation from the retail climate index ─────────────
    cs12 = {}
    for attempt in range(6):
        step = drv.step("71-precipitation-cs12",
                        "lluvia/nieve del indice CLIMATE.PAK (CS-12)")
        lines = ve(step, "weather climate=")
        drops = drops_of(lines)
        cs12 = rec_of(step, drops > 0, attempt=attempt, drops=drops, lines=lines[-2:])
        if drops > 0:
            break
        # `O` advances to the next retail climate that actually precipitates.
        press(d, win, "o")
        settle()
    checks["cs12_precipitation"] = cs12
    print("CS-12 drops=%s ok=%s" % (cs12.get("drops"), cs12.get("ok")))

    # ── CS-14 · city watch: crime -> guardsArriveCountdown -> billboards ─
    bash_lines = []
    crime = False
    # Door search guided by the window's own number: every bash prints the
    # distance to the nearest retail door when none is in reach, so the capture
    # walks the facade down (greedy, rotate on no progress) instead of guessing.
    # A crime needs a failed lock roll, so bashing continues inside reach.
    distance_re = re.compile(r"nearest at ([0-9.]+) m")

    def bash_once(shot=True):
        press(d, win, "g")
        settle()
        step = drv.step("72-city-watch-%02d" % (len(bash_lines) + 1),
                        "G (bash): distancia a la puerta retail mas cercana (CS-14)",
                        shot=shot)
        lines = ve(step, "M28 VE: bash")
        bash_lines.extend(lines)
        # A stored crime is the `CrimeTrigger` line ("bash — crime <Crimes>")
        # or the law's request line; the dice lines say "no crime" and must not
        # be read as success (measured 2026-10-24: "no crime" matched a naive
        # `"crime" in line` test and ended the loop on a non-event).
        if any(ln.startswith("M28 VE: bash — crime") or "→ request" in ln for ln in lines):
            return 0.0, True
        for ln in lines:
            hit = distance_re.search(ln)
            if hit:
                return float(hit.group(1)), False
        return None, False

    best, crime = bash_once()
    if scenario:
        # Scenario door (`M18_CS14=1`, `m18_demo::apply_cs14_scenario`): 1.4 m
        # ahead of the retail spawn, so the binding reaches it without walking.
        # The interaction is repeated (the dice advance) until the lock fails
        # and the crime is stored — the production chain, no shortcut.
        for _ in range(60):
            if crime:
                break
            best, crime = bash_once(shot=False)
    else:
        directions = ("w", "d", "s", "a")
        turn = 0
        for _ in range(30):
            if crime or best is None:
                break
            hold(d, win, directions[turn % len(directions)], 1.0, shift=True)
            now, crime = bash_once(shot=False)
            if crime:
                break
            if now is None:
                break
            if best is not None and now >= best - 0.1:
                turn += 1  # no progress: rotate to the next direction
            best = now
    # `guardsArriveCountdown` elapses in game time (~12 s, declared).
    # `guardsArriveCountdown` elapses in game time (~12 s, declared). The title
    # carries the law's census, so wait for the count instead of trusting one
    # sleep, and only then grab a frame that is not a repeat.
    deadline = time.time() + 45.0
    while time.time() < deadline:
        probe = name_of(d, win) or ""
        if "guards visible" in probe \
                and not probe.split("guards visible", 1)[1].strip().startswith("0"):
            break
        time.sleep(0.5)
    settle()
    guards = drv.step("73-city-watch-cs14",
                      "guardia de la ciudad visible tras el countdown (CS-14)",
                      forbid=drv.seen_shas(), tries=8, wait=0.8)
    gtitle = guards.get("window_name") or ""
    placed = [ln for ln in bash_lines if "guards placed" in ln] + ve(guards, "guards placed")
    visible = 0
    if "guards visible" in gtitle:
        try:
            visible = int(gtitle.split("guards visible", 1)[1].split()[0])
        except ValueError:
            visible = 0
    # The guard must be IN the pixels, not only in the title: the scenario
    # camera is parked with a clear line of sight (`m18_cs14_face_guards`) and
    # prints each guard's projected rectangle, which is cropped here and
    # compared against the neighbouring wall.
    # The guards are placed (and the camera aimed) while the crime's own step is
    # still open, so the projection lines land in the LAST few records, not only
    # in the CS-14 one (measured 2026-10-24: `ve_lines` of step `72-16` carried
    # them and the CS-14 record had a single `pos=` line).
    recent_ve = [ln for s in drv.steps[-4:] for ln in (s.get("ve_lines") or [])]
    projections = guard_projections(recent_ve)
    in_frame = [p for p in projections if p["in_frame"]]
    contrasts = [c for c in (sprite_contrast(guards.get("png"), p) for p in in_frame) if c]
    contrast = max((c["mean_abs_diff"] for c in contrasts), default=0.0)
    checks["cs14_guards"] = rec_of(
        guards,
        bool(crime and visible > 0 and in_frame and contrast >= 6.0
             and not guards.get("stale")),
        crime=crime, visible=visible, nearest_m=best,
        projections=projections, sprite_contrast=contrasts,
        capture_attempts=guards.get("capture_attempts"), stale=guards.get("stale"),
        bash_lines=bash_lines[-4:], placed_lines=placed[-4:], title=gtitle)
    print("CS-14 crime=%s visible=%d in_frame=%d contrast=%.2f ok=%s"
          % (crime, visible, len(in_frame), contrast, checks["cs14_guards"]["ok"]))

    # ── CS-18 / CS-19 · the two map panels (B3-c) ───────────────────────
    # The panel switch lands on the next frame: a single capture right after
    # the key press can photograph the PREVIOUS panel (measured 2026-10-24:
    # CS-18 and CS-19 came out with the same sha256). Each panel gets a retry
    # and the frame is only taken once the panel state has settled.
    def panel(key, label, note, forbidden):
        shot = None
        for _attempt in range(5):
            press(d, win, key)
            # The panel overlay reaches the window about a second after the key
            # (measured 2026-10-24: a 0.45 s shot still carried the previous
            # frame, byte-identical to the CS-14 capture).
            time.sleep(1.2)
            shot = drv.step(label, note)
            if shot.get("sha256") and shot["sha256"] not in forbidden:
                return shot
            time.sleep(0.8)
        return shot

    forbidden = {guards.get("sha256")}
    local = panel("5", "76-map-local-cs18", "panel map-local visible (CS-18)", forbidden)
    world = panel("6", "77-map-world-cs19", "panel map-world visible (CS-19)",
                  forbidden | {local.get("sha256")})
    # The two panels must differ from each other and from the pre-panel frame:
    # an identical sha256 is a key that did NOT switch (measured 2026-10-23).
    distinct = local.get("sha256") != world.get("sha256")
    checks["cs18_map_local"] = rec_of(
        local,
        (local.get("nonblack_ratio") or 0) > 0.01
        and local.get("sha256") != dia.get("sha256"))
    checks["cs19_map_world"] = rec_of(
        world,
        (world.get("nonblack_ratio") or 0) > 0.01 and distinct)
    checks["cs18_cs19_distinct"] = distinct
    print("CS-18 ok=%s CS-19 ok=%s distinct=%s"
          % (checks["cs18_map_local"]["ok"], checks["cs19_map_world"]["ok"], distinct))

    # Close the world panel first: the final frame was byte-identical to CS-19
    # (measured 2026-10-24), a capture with no state change between them.
    press(d, win, "6")
    settle()
    drv.step("79-final", "estado final de la campana parity",
             forbid=drv.seen_shas(), tries=6, wait=0.6)


def rebuild_manifest(ev_dir):
    """Regenerates `<ev_dir>/manifest.json` (schema M18-CS-REMEASURE-EVIDENCE-v1):
    sha256 + bytes of every archived file and the campaign tuples read from each
    `<campaign>/run.json`. The `sweep` block of an existing manifest is kept."""
    path = os.path.join(ev_dir, "manifest.json")
    previous = {}
    if os.path.exists(path):
        try:
            with open(path) as fh:
                previous = json.load(fh)
        except Exception:
            previous = {}
    campaigns, files, heads = {}, [], []
    for root, dirs, names in os.walk(ev_dir):
        dirs.sort()
        for name in sorted(names):
            full = os.path.join(root, name)
            rel = os.path.relpath(full, WS)
            if os.path.abspath(full) == os.path.abspath(path):
                continue
            with open(full, "rb") as fh:
                data = fh.read()
            files.append({"path": rel, "bytes": len(data),
                          "sha256": hashlib.sha256(data).hexdigest()})
            if name == "run.json":
                with open(full) as fh:
                    run = json.load(fh)
                key = os.path.basename(root)
                campaigns[key] = {
                    "campaign": run.get("campaign"),
                    "app_args": run.get("app_args"),
                    "binary_sha256": run.get("binary_sha256"),
                    "captures": run.get("steps"),
                    "head": run.get("head"),
                    "focus_acquired": run.get("focus_acquired"),
                    "input_delivered": run.get("input_delivered"),
                    "input_self_test": run.get("input_self_test"),
                    "resolution": run.get("resolution"),
                    "bands_seen": run.get("bands_seen"),
                    "checks": run.get("checks"),
                    "spawn": run.get("spawn"),
                    "spawn_line": run.get("spawn_line"),
                    "spawn_override_line": run.get("spawn_override_line"),
                    "spawn_arg": run.get("spawn_arg"),
                    "origin_override_line": run.get("origin_override_line"),
                    "yaw_keys": run.get("yaw_keys"),
                }
                head = run.get("head")
                if head and head not in heads:
                    heads.append(head)
    binaries = sorted({c.get("binary_sha256") for c in campaigns.values() if c.get("binary_sha256")})
    manifest = {
        "schema": previous.get("schema", "M18-CS-REMEASURE-EVIDENCE-v1"),
        "cr": previous.get("cr", "CR-f716455c"),
        "binary": previous.get("binary", "target/release/daggerfall-app (window,audio)"),
        "binary_sha256": binaries or previous.get("binary_sha256", []),
        "resolution": previous.get("resolution", [1280, 720]),
        "measured_head": heads or previous.get("measured_head", []),
        "campaigns": campaigns,
        "sweep": previous.get("sweep"),
        "files": files,
    }
    with open(path, "w") as fh:
        json.dump(manifest, fh, indent=2, sort_keys=True)
    print("MANIFEST %s · %d files · campaigns %s" % (path, len(files), sorted(campaigns)))
    return 0


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--bin", default=None,
                    help="binario release (obligatorio salvo con --manifest)")
    ap.add_argument("--out", default=None)
    ap.add_argument("--campaign",
                    choices=("capital", "coastal", "mountain",
                             "band-ocean", "band-beach", "band-mountain", "parity"),
                    default="capital")
    ap.add_argument("--spawn", default=None,
                    help="--spawn band:ocean|band:beach|band:land (CS-08/09/10); "
                         "las campanas band-* lo fijan por defecto")
    ap.add_argument("--manifest", default=None,
                    help="regenera <dir>/manifest.json (sha256 por fichero) y sale")
    ap.add_argument("--app-arg", action="append", default=[],
                    help="argumento extra para el binario, repetible (p.ej. --app-arg=--origin --app-arg=207,215)")
    ap.add_argument("--origin", default=None, help="atajo: --origin x,y -> --app-arg --origin --app-arg x,y")
    ap.add_argument("--yaw", default=None, help="atajo: --yaw rad -> --app-arg --yaw --app-arg rad")
    ap.add_argument("--data-root", default=os.path.expanduser("~/Games/Daggerfall/app"))
    ap.add_argument("--timeout", type=float, default=240.0)
    ap.add_argument("--smoke", action="store_true", help="solo ventana + menu + auto-test de input")
    ap.add_argument("--cs14-scenario", action="store_true",
                    help="lanza con M18_CS14=1 (puerta de escenario a 1.4 m) y bate la puerta "
                         "en el sitio hasta que salta el crimen (CS-14)")
    args = ap.parse_args()

    if args.manifest:
        return rebuild_manifest(os.path.abspath(args.manifest))
    if not args.bin or not args.out:
        ap.error("--bin y --out son obligatorios (salvo --manifest)")
    binary = os.path.abspath(args.bin)
    out = os.path.abspath(args.out)
    os.makedirs(out, exist_ok=True)
    logfile = os.path.join(out, "app.log")
    steps_path = os.path.join(out, "steps.jsonl")
    for stale in (logfile, steps_path):
        if os.path.exists(stale):
            os.unlink(stale)

    app_args = list(args.app_arg)
    if args.origin:
        app_args += ["--origin", args.origin]
    if args.yaw:
        app_args += ["--yaw", args.yaw]
    spawn_arg = args.spawn or CAMPAIGN_SPAWN.get(args.campaign)
    if spawn_arg:
        app_args += ["--spawn", spawn_arg]

    try:
        d = display.Display()
    except Exception as exc:
        print("NO-DISPLAY: %s" % exc)
        return 3
    xtest_available = d.has_extension("XTEST")

    env = dict(os.environ)
    env["DF_APP_ROOT"] = args.data_root
    # CS-13 (`m18_cs13_face_crowd`): the walk leaves the camera against a
    # facade, so the scenario turns the yaw to the busiest direction of the
    # retail crowd. Only the yaw changes; no NPC is moved or hidden.
    env["M18_CS13"] = "1"
    if args.cs14_scenario:
        # Scenario door of `m18_demo::apply_cs14_scenario` (CS-14): the retail
        # sub-records carry 0 doors in this data path, so the bash interaction
        # has no target unless the scenario injects one.
        env["M18_CS14"] = "1"
    env.setdefault("DISPLAY", os.environ.get("DISPLAY", ":0"))
    with open(binary, "rb") as fh:
        sha = hashlib.sha256(fh.read()).hexdigest()
    head = subprocess.run(
        ["git", "rev-parse", "HEAD"], cwd=WS, capture_output=True, text=True
    ).stdout.strip()

    steps, ok, focus_ok, wid, nm = [], False, False, None, None
    report = {}
    with open(logfile, "wb") as sink:
        proc = subprocess.Popen(
            [binary] + app_args, stdout=sink, stderr=subprocess.STDOUT, env=env, cwd=WS
        )

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
        nm = name_of(d, win) or nm
        drv = Driver(d, win, wid, out, logfile, steps_path, steps)
        launch_lines, _ = new_log_slice(logfile, 0)
        drv.startup_lines = launch_lines
        if args.smoke:
            drv.step("00-smoke-menu", "smoke: MENU")
            press(d, win, "Return", hold=0.12)
            time.sleep(2.5)
            drv.step("01-smoke-playing", "smoke: PLAYING")
            before = drv.step("02-smoke-pre", "pos antes de W")["pos"]
            hold(d, win, "w", 1.5)
            after = drv.step("03-smoke-post", "pos tras W 1.5 s")["pos"]
            print("SMOKE input_ok=%s before=%s after=%s" % (before != after, before, after))
            ok = before != after
        else:
            ok, report = campaign(drv, d, args.campaign, args.cs14_scenario)
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
            if "terrain band" in ln and "→" in ln:
                bands.add(ln.split("terrain band ")[1].split(" ")[0])
            if "mode=" in ln:
                modes.add(ln.split("mode=")[1].split(" ")[0])
        for ln in s.get("audio_lines", []):
            audio.add(ln.strip())
    startup = getattr(drv, "startup_lines", []) if not args.smoke else []
    startup_ve = [ln for ln in startup if " VE:" in ln]
    origin_line = next((ln for ln in startup if "origin-override" in ln), None)
    spawn_override_line = next((ln for ln in startup if "spawn-override" in ln), None)
    spawn_line = next((ln for ln in startup if ln.startswith("M18 VE:") and "spawn=(" in ln), None)
    music_lines = [ln for ln in startup if ln.startswith("M14 VE:") and "music" in ln]
    resolution = None
    for s in caps:
        if s.get("width"):
            resolution = [s["width"], s["height"]]
            break
    run = {
        "head": head,
        "campaign": args.campaign,
        "binary": os.path.relpath(binary, WS),
        "binary_sha256": sha,
        "app_args": app_args,
        "origin_arg": args.origin,
        "yaw_arg": args.yaw,
        "data_root": args.data_root,
        "resolution": resolution,
        "window_id_hex": hex(wid),
        "window_name": nm,
        "display": os.environ.get("DISPLAY"),
        "wayland_display": os.environ.get("WAYLAND_DISPLAY"),
        "session_type": os.environ.get("XDG_SESSION_TYPE"),
        "focus_acquired": focus_ok,
        "xtest_available": xtest_available,
        "input_route": "core KeyPress/KeyRelease via send_event (XTEST inert here; mouse-look needs raw motion)",
        "input_delivered": ok,
        "spawn_line": spawn_line,
        "origin_override_line": origin_line,
        "spawn_arg": spawn_arg,
        "spawn_override_line": spawn_override_line,
        "music_lines": music_lines,
        "startup_ve_lines": startup_ve[:60],
        "checks": report.get("checks", {}),
        "spawn": report.get("spawn"),
        "input_self_test": report.get("input_self_test"),
        "yaw_keys": report.get("yaw_keys"),
        "steps": len(caps),
        "captures": caps,
        "bands_seen": sorted(bands),
        "modes_seen": sorted(modes),
        "audio_lines_seen": sorted(audio)[:20],
        "ts": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
    }
    with open(os.path.join(out, "run.json"), "w") as fh:
        json.dump(run, fh, indent=2, sort_keys=True)
    print("RUN OK campaign=%s captures=%d input_delivered=%s head=%s"
          % (args.campaign, len(caps), ok, head[:12]))
    return 0


if __name__ == "__main__":
    sys.exit(main())
