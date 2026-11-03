#!/usr/bin/env python3
"""demo-035.py - DEMO-035 campaign driver for annex M35 (sky L0-L4 + vegetation).

Launches the windowed binary (feature `window`, **no** `--probe`: the window is
the default path), waits for the demo window **by condition** (window id whose
`_NET_WM_NAME` carries the known title prefix and whose client areas matches the
documented 1280x720), takes keyboard focus, drives the M35 bindings with core
X11 key events and captures the six frames of the campaign.

Bindings driven (emitted by the window itself, so the log dates every press):

  Enter  `M18 VE: phase MENU -> PLAYING (Enter)`  - the world surface
  M      `M35 VE: clock advanced N min to band=<b>` - day/dusk/night/dawn
  N      `M35 VE: season=<S>` + `M35 VE6: spawned=...` - summer/winter

Viewpoint: the retail spawn of this composition is an interior (no sky in
frame), so the window is launched with its own documented override
`--spawn band:land` (moves the eye only; the resolved standpoint is printed as
`M18 VE: spawn-override ...` evidence and recorded in `run.json`).

Input route measured on this workstation (2026-10-22, KWin/Wayland + XWayland):
XTEST is inert for this client, core `KeyPress`/`KeyRelease` sent straight to
the window (`send_event`) do reach the engine when the focus is verified. The
driver therefore re-checks `get_input_focus()` before every press and never
claims a key that the log did not confirm.

Outputs, all under `--out` (default `evidence/m35-demo`):

  app.log         the client's own stdout+stderr, single-writer raw capture
  demo.log        app.log plus one line per driven key/capture, spliced at the
                  position it was observed (never two writers on one file)
  run.json        binary/window/input record + per-capture sha256 and luma
  sky-day.png sky-dusk.png sky-night.png sky-dawn.png     (4 bands, `M`)
  veg-summer.png veg-winter.png                            (2 seasons, `N`)
  probe-BIOME-001.log   only with `--probe-counters` (full probe output)

A capture whose key the log did not confirm is kept on disk and recorded as
`NO-RE-MEDIDA` with its measured cause; it is never retried in a loop and never
replaced by a different frame.

Usage:
  demo-035.py --display :0 --out evidence/m35-demo --probe-counters
  demo-035.py --dry-run            # preconditions and plan, no window
"""
import argparse
import hashlib
import json
import os
import re
import signal
import subprocess
import sys
import time

from PIL import Image, ImageChops, ImageStat
from Xlib import X, XK, display
from Xlib.protocol import event as xevent

WS = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
NET_WM_NAME = "_NET_WM_NAME"
UTF8 = "UTF8_STRING"
NET_ACTIVE_WINDOW = "_NET_ACTIVE_WINDOW"
TITLE_PREFIX = "DAGGERFALL-LIKE"
WIDTH = 1280
HEIGHT = 720
CAPTURES = ("sky-day", "sky-dusk", "sky-night", "sky-dawn",
            "veg-summer", "veg-winter")
PROBE_NAME = "BIOME-001"
PROBE_LINE = "PROBE-BIOME-001:"


def now_iso():
    return time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())


# ---------------------------------------------------------------------------
# Window discovery (condition based: never a blind sleep on the launch).
# ---------------------------------------------------------------------------
def name_of(d, win):
    """`_NET_WM_NAME` (UTF-8, what winit sets) then the `WM_NAME` fallback."""
    for prop, typ in ((NET_WM_NAME, UTF8), ("WM_NAME", "STRING")):
        try:
            val = win.get_full_property(d.intern_atom(prop), d.intern_atom(typ))
        except Exception:
            val = None
        if val and val.value:
            raw = val.value
            if isinstance(raw, bytes):
                return raw.decode("utf-8", "replace")
            return str(raw)
    try:
        nm = win.get_wm_name()
        return str(nm) if nm else None
    except Exception:
        return None


def walk_named(d, win, depth=0, acc=None):
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
        walk_named(d, child, depth + 1, acc)
    return acc


def geometry(win):
    try:
        geo = win.get_geometry()
        return int(geo.width), int(geo.height)
    except Exception:
        return None


def find_window(d, proc, timeout, needle=TITLE_PREFIX):
    """Window id + known title, or `None` when the process dies first.

    The exact 1280x720 client area wins over any other candidate; the search
    keeps polling until the deadline, so the caller never sleeps blindly.
    """
    deadline = time.time() + timeout
    while time.time() < deadline:
        if proc is not None and proc.poll() is not None:
            return None
        matches = [c for c in walk_named(d, d.screen().root)
                   if needle in c[2].upper()]
        for win, wid, nm in matches:
            if geometry(win) == (WIDTH, HEIGHT):
                return win, wid, nm
        if matches:
            return matches[0]
        time.sleep(0.4)
    return None


def current_focus(d):
    try:
        focus = d.get_input_focus().focus
        return focus.id if hasattr(focus, "id") else focus
    except Exception:
        return None


def activate(d, win, timeout=6.0):
    """Real keyboard focus on the X server.

    Under a Wayland session the focus belongs to the compositor, so
    `set_input_focus` alone is a no-op: the accepted route is an EWMH
    `_NET_ACTIVE_WINDOW` client message with source indication 2 (pager/user)
    followed by a `set_input_focus` and a focus read-back loop.
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
    except Exception as exc:
        print("activate: %s" % exc)
    try:
        win.configure(stack_mode=X.Above)
    except Exception:
        pass
    try:
        win.set_input_focus(X.RevertToParent, X.CurrentTime)
        d.sync()
    except Exception:
        pass
    deadline = time.time() + timeout
    while time.time() < deadline:
        if current_focus(d) == win.id:
            return True
        time.sleep(0.2)
    return current_focus(d) == win.id


def ensure_focus(d, win):
    """Cheap focus read-back so a press is never sent to a foreign window."""
    if current_focus(d) == win.id:
        return True
    return activate(d, win)


# ---------------------------------------------------------------------------
# Input: core X11 KeyPress/KeyRelease to the window (XTEST is inert here).
# ---------------------------------------------------------------------------
def keycode(d, name):
    return d.keysym_to_keycode(XK.string_to_keysym(name))


def send_key(d, win, name, down):
    root = d.screen().root
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
            detail=keycode(d, name),
        ),
        event_mask=mask,
    )
    d.sync()


def press(d, win, name, hold=0.12):
    send_key(d, win, name, True)
    time.sleep(hold)
    send_key(d, win, name, False)
    time.sleep(0.10)


# ---------------------------------------------------------------------------
# Log tail: every line the window printed, plus condition waits on new lines.
# ---------------------------------------------------------------------------
class LogTail:
    """Incremental reader of the client's own stream (`app.log`)."""

    def __init__(self, path):
        self.path = path
        self.offset = 0
        self.partial = ""
        self.history = []

    def pump(self):
        try:
            with open(self.path, "rb") as fh:
                fh.seek(self.offset)
                chunk = fh.read()
        except FileNotFoundError:
            return []
        if not chunk:
            return []
        self.offset += len(chunk)
        self.partial += chunk.decode("utf-8", "replace")
        new = []
        while "\n" in self.partial:
            line, self.partial = self.partial.split("\n", 1)
            if line.strip():
                self.history.append(line)
                new.append(line)
        return new

    @property
    def mark(self):
        return len(self.history)

    def checkpoint(self):
        """Line count of everything printed so far, after draining the file.

        Reading before taking the mark is what keeps a press from matching a
        line the client had already printed: an unpumped backlog lands *after*
        a bare `len(history)` mark and would be credited to the new press.
        """
        self.pump()
        return len(self.history)

    def wait(self, patterns, timeout, start=None):
        """Wait until every regex in `patterns` matched a line (or the timeout).

        Returns `{pattern: line}` on success, `None` on timeout. `start` scopes
        the scan to the lines printed after a given mark, so a key can only be
        credited with the line its own press produced.
        """
        if isinstance(patterns, str):
            patterns = [patterns]
        compiled = [(p, re.compile(p)) for p in patterns]
        found = {}
        first = self.checkpoint() if start is None else start
        deadline = time.time() + timeout
        while True:
            self.pump()
            for line in self.history[first:]:
                for pattern, rx in compiled:
                    if pattern not in found and rx.search(line):
                        found[pattern] = line.strip()
            if len(found) == len(patterns):
                return found
            if time.time() >= deadline:
                return None
            time.sleep(0.12)

    def search(self, pattern):
        self.pump()
        rx = re.compile(pattern)
        for line in self.history:
            if rx.search(line):
                return line.strip()
        return None


class NoteLog:
    """Driver-side lines, kept in memory and spliced into `demo.log` at the end.

    One writer per file. The client owns `app.log` for the whole campaign and
    the live tail reads it; nothing else may append there. Two writers on one
    file were measured on 2026-10-27 to break both directions: the client's own
    lines landed behind the tail's read offset (a real `season=Autumn` line was
    never matched, so the press was recorded as not delivered) and the driver's
    own key annotations were read back as if the client had printed them (keys
    credited by their own echo). Each note keeps the tail line index it was
    observed at, so the merged log stays chronological.
    """

    def __init__(self, tail):
        self.tail = tail
        self.entries = []

    def write(self, text):
        self.tail.pump()
        self.entries.append((len(self.tail.history), text.rstrip("\n")))

    def merged(self, app_lines):
        """`app_lines` with every note inserted before the line it followed."""
        buckets = {}
        for index, text in self.entries:
            buckets.setdefault(index, []).append(text)
        out = []
        for index, line in enumerate(app_lines):
            out.extend(buckets.pop(index, []))
            out.append(line)
        for index in sorted(buckets):
            out.extend(buckets[index])
        return out


# ---------------------------------------------------------------------------
# Capture + pixel statistics.
# ---------------------------------------------------------------------------
def stats(png):
    with open(png, "rb") as fh:
        raw = fh.read()
    im = Image.open(png).convert("RGB")
    width, height = im.size
    red, green, blue = im.split()
    max_channel = ImageChops.lighter(ImageChops.lighter(red, green), blue)
    hist = max_channel.histogram()
    # `nonblack`: share of pixels whose brightest channel is > 8 (the rule the
    # M18 evidence already uses), computed from the histogram, not per pixel.
    nonblack = sum(hist[9:]) / float(width * height)
    mean_r, mean_g, mean_b = ImageStat.Stat(im).mean
    mean_luma = 0.2126 * mean_r + 0.7152 * mean_g + 0.0722 * mean_b
    # Upper band of the frame: the sky region the L0 gradient and the L1 stars
    # land in. `bright_ratio` counts it with luma > 47 (the star threshold the
    # capture report quotes), so a night frame can be checked on pixels.
    band = im.crop((0, 0, width, max(1, int(height * 0.30))))
    band_r, band_g, band_b = ImageStat.Stat(band).mean
    band_luma = 0.2126 * band_r + 0.7152 * band_g + 0.0722 * band_b
    band_gray = band.convert("L")
    gray_hist = band_gray.histogram()
    band_pixels = float(band_gray.size[0] * band_gray.size[1])
    bright_ratio = sum(gray_hist[48:]) / band_pixels if band_pixels else 0.0
    return {
        "sha256": hashlib.sha256(raw).hexdigest(),
        "bytes": len(raw),
        "width": width,
        "height": height,
        "mean_luma": round(mean_luma, 2),
        "nonblack_ratio": round(nonblack, 4),
        "unique_colors": len(im.getcolors(maxcolors=1 << 24) or []),
        "top_band_mean_luma": round(band_luma, 2),
        "top_band_bright_ratio": round(bright_ratio, 4),
    }


def capture(d, win, wid, out, label):
    """Client-area capture with the root-crop fallback for composited windows."""
    direct = os.path.join(out, label + ".png")
    subprocess.run(["import", "-window", str(wid), direct],
                   check=True, capture_output=True)
    rec = stats(direct)
    rec["method"] = "import -window %s" % hex(wid)
    if rec["nonblack_ratio"] < 0.01:
        geo = win.get_geometry()
        point = win.translate_coords(d.screen().root, 0, 0)
        crop = os.path.join(out, label + ".rootcrop.png")
        rect = "%dx%d+%d+%d" % (geo.width, geo.height,
                                point.dst_x, point.dst_y)
        try:
            subprocess.run(
                ["import", "-window", "root", "-crop", rect, "+repage", crop],
                check=True, capture_output=True,
            )
            alt = stats(crop)
            if alt["nonblack_ratio"] > rec["nonblack_ratio"]:
                alt["method"] = "import -window root -crop %s" % rect
                os.replace(crop, direct)
                rec = alt
            else:
                os.unlink(crop)
        except Exception as exc:
            rec["fallback_error"] = str(exc)
    rec["path"] = os.path.relpath(direct, WS)
    return rec


# ---------------------------------------------------------------------------
# Campaign.
# ---------------------------------------------------------------------------
def resolve_binary(explicit):
    """`--bin`, else the release build, else the debug build that exists."""
    if explicit:
        return os.path.abspath(explicit)
    for candidate in ("target/release/daggerfall-app",
                      "target/debug/daggerfall-app"):
        path = os.path.join(WS, candidate)
        if os.path.exists(path):
            return path
    return None


class Campaign:
    def __init__(self, args, binary):
        self.args = args
        self.binary = binary
        self.out = os.path.abspath(args.out)
        self.app_log_path = os.path.join(self.out, "app.log")
        self.log_path = os.path.join(self.out, "demo.log")
        self.tail = LogTail(self.app_log_path)
        self.annotate = NoteLog(self.tail)
        self.keys = []
        self.delivered = {}
        self.captures = []
        self.last_sha = None
        self.last_label = None
        self.seen_sha = {}
        self.proc = None
        self.display = None
        self.win = None
        self.wid = None
        self.focus_ok = False
        self.notes = []
        self.probe = None
        self.app_exit = None
        self.phase = None
        self.spawn_line = None
        self.app_args = self.build_app_args()

    def build_app_args(self):
        """The window path plus the documented outdoor viewpoint.

        The retail spawn of this composition is an **interior** (measured: the
        first campaign's `sky-day.png` is a closed room, no sky in frame), so a
        sky campaign has to move the eye. `--spawn band:<ocean|beach|land>` is
        the window's own documented viewpoint override (`parse_overrides` in
        `m18_demo.rs`): it moves the eye only, the composition stays, and it
        prints the resolved standpoint as evidence. `--window`/`--probe` are
        never passed: the window path is the default and the counters probe is
        a separate process.
        """
        argv = [self.binary]
        if self.args.spawn:
            argv += ["--spawn", self.args.spawn]
        if self.args.yaw is not None:
            argv += ["--yaw", "%.4f" % self.args.yaw]
        return argv

    # -- lifecycle ---------------------------------------------------------
    def prepare(self):
        os.makedirs(self.out, exist_ok=True)
        for name in CAPTURES:
            for stale in (name + ".png", name + ".rootcrop.png"):
                path = os.path.join(self.out, stale)
                if os.path.exists(path):
                    os.unlink(path)
        for stale in ("app.log", "demo.log", "run.json",
                      "probe-%s.log" % PROBE_NAME):
            path = os.path.join(self.out, stale)
            if os.path.exists(path):
                os.unlink(path)

    def launch(self):
        env = dict(os.environ)
        env["DISPLAY"] = self.args.display
        env["DF_APP_ROOT"] = self.args.data_root
        sink = open(self.app_log_path, "wb")
        try:
            self.proc = subprocess.Popen(
                self.app_args,
                stdout=sink,
                stderr=subprocess.STDOUT,
                env=env,
                cwd=WS,
                start_new_session=True,
            )
        finally:
            sink.close()

    def teardown(self):
        """SIGTERM, then SIGKILL: the client never outlives the campaign."""
        if self.proc is None:
            return
        if self.proc.poll() is None:
            try:
                os.kill(self.proc.pid, signal.SIGTERM)
            except ProcessLookupError:
                pass
            deadline = time.time() + 10.0
            while time.time() < deadline and self.proc.poll() is None:
                time.sleep(0.2)
            if self.proc.poll() is None:
                try:
                    os.kill(self.proc.pid, signal.SIGKILL)
                except ProcessLookupError:
                    pass
                try:
                    self.proc.wait(timeout=5)
                except Exception:
                    pass
        self.app_exit = self.proc.poll()
        self.annotate.write("# demo-035: client stopped pid=%d exit=%s"
                            % (self.proc.pid, self.app_exit))

    # -- input -------------------------------------------------------------
    def drive(self, key, expect, label):
        """Press `key` until `expect` shows in the log (bounded attempts)."""
        start = self.tail.checkpoint()
        rec = {
            "key": key,
            "label": label,
            "expect": expect if isinstance(expect, str) else list(expect),
            "attempts": 0,
            "delivered": False,
            "evidence": [],
            "focus": [],
            "latency_s": None,
            "ts": now_iso(),
        }
        for attempt in range(1, self.args.key_attempts + 1):
            rec["attempts"] = attempt
            focus = ensure_focus(self.display, self.win)
            rec["focus"].append(focus)
            pressed = time.monotonic()
            press(self.display, self.win, key)
            hit = self.tail.wait(expect, self.args.key_timeout, start=start)
            if hit:
                rec["delivered"] = True
                rec["latency_s"] = round(time.monotonic() - pressed, 3)
                rec["evidence"] = list(hit.values())
                self.annotate.write(
                    "# KEY %-6s attempt=%d focus=%s latency=%.3fs -> delivered: %s"
                    % (key, attempt, focus, rec["latency_s"],
                       " | ".join(rec["evidence"])))
                break
            self.annotate.write(
                "# KEY %-6s attempt=%d focus=%s -> no log effect within %.1fs"
                % (key, attempt, focus, self.args.key_timeout))
        if not rec["delivered"]:
            self.annotate.write("# KEY %-6s NOT DELIVERED after %d attempts"
                                % (key, rec["attempts"]))
        self.keys.append(rec)
        self.delivered[key] = self.delivered.get(key, True) and rec["delivered"]
        return rec

    # -- capture -----------------------------------------------------------
    def wait_paint(self, timeout=90.0, min_colors=24):
        """The composition actually painted before the first archived frame."""
        probe = "_paintprobe"
        deadline = time.time() + timeout
        while time.time() < deadline:
            try:
                rec = capture(self.display, self.win, self.wid, self.out, probe)
            except Exception:
                time.sleep(0.4)
                continue
            if rec["nonblack_ratio"] > 0.01 and rec["unique_colors"] >= min_colors:
                path = os.path.join(self.out, probe + ".png")
                if os.path.exists(path):
                    os.unlink(path)
                return True
            time.sleep(0.4)
        path = os.path.join(self.out, probe + ".png")
        if os.path.exists(path):
            os.unlink(path)
        return False

    def shot(self, label, band, season, delivered, extra=None):
        entry = {
            "label": label,
            "ts": now_iso(),
            "band": band,
            "season": season,
            "input_delivered": delivered,
        }
        entry.update(extra or {})
        try:
            rec = capture(self.display, self.win, self.wid, self.out, label)
        except Exception as exc:
            entry.update({
                "status": "NO-RE-MEDIDA",
                "cause": "capture-failed",
                "error": repr(exc),
            })
            self.captures.append(entry)
            self.annotate.write("# CAPTURE %s -> NO-RE-MEDIDA (%s)"
                                % (label, entry["error"]))
            return entry
        # A state frame must be a NEW frame: the client surface lags one frame,
        # so a grab taken right after the state change can repeat the previous
        # bytes. Retry while the sha equals the previous capture (bounded).
        attempt = 1
        while (self.last_sha and rec["sha256"] == self.last_sha
               and attempt < self.args.frame_attempts):
            time.sleep(0.35)
            try:
                rec = capture(self.display, self.win, self.wid, self.out, label)
            except Exception as exc:
                entry.update({"status": "NO-RE-MEDIDA",
                              "cause": "capture-failed", "error": repr(exc)})
                self.captures.append(entry)
                return entry
            attempt += 1
        entry.update({
            "path": rec["path"],
            "sha256": rec["sha256"],
            "width": rec["width"],
            "height": rec["height"],
            "mean_luma": rec["mean_luma"],
            "nonblack_ratio": rec["nonblack_ratio"],
            "unique_colors": rec["unique_colors"],
            "top_band_mean_luma": rec["top_band_mean_luma"],
            "top_band_bright_ratio": rec["top_band_bright_ratio"],
            "bytes": rec["bytes"],
            "method": rec["method"],
            "capture_attempts": attempt,
            "window_title": name_of(self.display, self.win),
        })
        size_ok = (rec["width"], rec["height"]) == (WIDTH, HEIGHT)
        if not delivered:
            entry["status"] = "NO-RE-MEDIDA"
            entry["cause"] = "input-not-delivered"
        elif not size_ok:
            entry["status"] = "NO-RE-MEDIDA"
            entry["cause"] = "unexpected-resolution"
        else:
            entry["status"] = "MEDIDA"
        if self.last_sha and rec["sha256"] == self.last_sha:
            entry["duplicate_of"] = self.last_label
        elif rec["sha256"] in self.seen_sha:
            entry["duplicate_of"] = self.seen_sha[rec["sha256"]]
        self.seen_sha.setdefault(rec["sha256"], label)
        self.last_sha = rec["sha256"]
        self.last_label = label
        self.captures.append(entry)
        self.annotate.write("# CAPTURE %s -> %s sha=%s luma=%.2f"
                            % (label, entry["status"], rec["sha256"][:12],
                               rec["mean_luma"]))
        print("%-12s %-11s sha=%s luma=%6.2f %dx%d"
              % (label, entry["status"], rec["sha256"][:12],
                 rec["mean_luma"], rec["width"], rec["height"]))
        return entry

    # -- the campaign itself ------------------------------------------------
    def run(self):
        self.prepare()
        try:
            self.display = display.Display(self.args.display)
        except Exception as exc:
            self.notes.append("NO-DISPLAY %s: %s" % (self.args.display, exc))
            print("NO-DISPLAY %s: %s" % (self.args.display, exc))
            return 3
        self.annotate.write("# demo-035: binary=%s display=%s out=%s"
                            % (os.path.relpath(self.binary, WS),
                               self.args.display, os.path.relpath(self.out, WS)))
        self.launch()
        try:
            found = find_window(self.display, self.proc, self.args.timeout)
            if not found:
                app_exit = self.proc.poll()
                self.notes.append("NO-WINDOW after %.0fs (app exit=%s)"
                                  % (self.args.timeout, app_exit))
                print(self.notes[-1])
                return 4
            self.win, self.wid, name = found
            self.annotate.write("# demo-035: window %s title=%r size=%s"
                                % (hex(self.wid), name,
                                   geometry(self.win)))
            print("WINDOW %s %r" % (hex(self.wid), name))
            self.focus_ok = activate(self.display, self.win)
            self.annotate.write("# demo-035: focus_acquired=%s focus=%s"
                                % (self.focus_ok,
                                   hex(current_focus(self.display) or 0)))
            print("FOCUS_ACQUIRED=%s" % self.focus_ok)
            # The band viewpoint is evidence, not a silent change: the client
            # prints where the eye stands, which band is under it and the yaw.
            if self.args.spawn:
                spawn = self.tail.wait([r"M18 VE: spawn-override band:"],
                                       min(self.args.key_timeout, 30.0),
                                       start=0)
                self.spawn_line = (spawn or {}).get(
                    r"M18 VE: spawn-override band:")
                if self.spawn_line is None:
                    self.notes.append("no `M18 VE: spawn-override` line for "
                                      "--spawn %s" % self.args.spawn)
                else:
                    print("SPAWN %s" % self.spawn_line)
            # The world surface needs `Enter`; the key handler runs in both
            # phases, so the press is also the input self-test.
            enter = self.drive("Return", r"M18 VE: phase MENU .+ PLAYING",
                              "enter-world")
            self.phase = "PLAYING" if enter["delivered"] else "MENU"
            if not enter["delivered"]:
                self.notes.append(
                    "Enter not delivered: the frame is the MENU surface, every "
                    "capture stays NO-RE-MEDIDA")
            composed = self.tail.wait([r"M35 VE: band=day season="],
                                      self.args.key_timeout, start=0)
            if composed is None:
                self.notes.append("no M35 recompose line (band=day) in the log")
            veg = self.tail.wait(
                [r"M35 VE6: (spawned=|plan=|WOODS\.WLD unavailable|"
                 r"MAPS\.BSA rects unavailable)"],
                min(self.args.key_timeout, 30.0), start=0)
            if veg is None:
                self.notes.append("no M35 VE6 line: the vegetation layer "
                                  "reported nothing at startup")
            if not self.wait_paint(self.args.paint_timeout):
                self.notes.append("no painted frame within %.0fs"
                                  % self.args.paint_timeout)
            # 1) four bands through `M` (day is the 12:00 startup clock).
            self.shot("sky-day", "day", "Summer", enter["delivered"])
            for band, label in (("dusk", "sky-dusk"),
                                ("night", "sky-night"),
                                ("dawn", "sky-dawn")):
                key = self.drive(
                    "m",
                    [r"M35 VE: clock advanced \d+ min to band=%s" % band,
                     r"M35 VE: band=%s season=" % band],
                    label)
                self.shot(label, band, "Summer", key["delivered"])
            # 2) back to `day` for the season pair: a band is the one state the
            # campaign can return to deterministically (`M` wraps dawn -> day),
            # so the pair differs by season alone.
            back = self.drive(
                "m",
                [r"M35 VE: clock advanced \d+ min to band=day",
                 r"M35 VE: band=day season="],
                "return-day")
            self.shot("veg-summer", "day", "Summer", back["delivered"])
            # The key handler is the instrument: its own `season=` line proves
            # the press arrived. The vegetation atlas line is collected as
            # extra evidence (a missing atlas is a declared gap, not a lost
            # key), so it never downgrades the capture.
            mark = self.tail.checkpoint()
            autumn = self.drive("n", [r"M35 VE: season=Autumn"], "season-autumn")
            autumn_veg = self.tail.wait([r"M35 VE6: spawned=.*season=Autumn"],
                                        15.0, start=mark)
            mark = self.tail.checkpoint()
            winter = self.drive("n", [r"M35 VE: season=Winter"], "season-winter")
            winter_veg = self.tail.wait([r"M35 VE6: spawned=.*season=Winter"],
                                        15.0, start=mark)
            self.shot("veg-winter", "day", "Winter", winter["delivered"],
                      {"veg_spawn_line": (winter_veg or {}).get(
                          r"M35 VE6: spawned=.*season=Winter")})
            if not autumn["delivered"]:
                self.notes.append("Autumn press not confirmed by the log")
            if autumn_veg is None or winter_veg is None:
                self.notes.append(
                    "vegetation atlas line missing for %s: the layer reported "
                    "no spawn for that season"
                    % ("/".join(name for name, line in
                                (("Autumn", autumn_veg), ("Winter", winter_veg))
                                if line is None)))
            return 0
        finally:
            self.teardown()
            if self.args.probe_counters:
                self.probe = self.run_probe()
            self.write_run()

    # -- counters probe (separate process, same binary) ---------------------
    def run_probe(self):
        log_path = os.path.join(self.out, "probe-%s.log" % PROBE_NAME)
        env = dict(os.environ)
        env["DF_APP_ROOT"] = self.args.data_root
        record = {"name": PROBE_NAME, "argv": [os.path.basename(self.binary),
                                               "--probe", PROBE_NAME],
                  "log": os.path.relpath(log_path, WS), "exit": None,
                  "line": None}
        try:
            with open(log_path, "wb") as sink:
                completed = subprocess.run(
                    [self.binary, "--probe", PROBE_NAME],
                    stdout=sink, stderr=subprocess.STDOUT, env=env, cwd=WS,
                    timeout=self.args.probe_timeout,
                )
            record["exit"] = completed.returncode
        except subprocess.TimeoutExpired:
            record["exit"] = None
            record["cause"] = "timeout after %.0fs" % self.args.probe_timeout
        except Exception as exc:
            record["exit"] = None
            record["cause"] = repr(exc)
        try:
            with open(log_path, "r", encoding="utf-8", errors="replace") as fh:
                lines = [ln.strip() for ln in fh if ln.strip()]
            for line in reversed(lines):
                if PROBE_LINE in line:
                    record["line"] = line
                    break
        except FileNotFoundError:
            pass
        # A declared data gap (`exit=2`) is a measurement, never a campaign
        # failure: the exit and the last probe line are appended as evidence.
        self.annotate.write(
            "# PROBE %s exit=%s line=%s"
            % (PROBE_NAME, record["exit"], record["line"]))
        print("PROBE %s exit=%s" % (PROBE_NAME, record["exit"]))
        return record

    # -- merged log --------------------------------------------------------
    def merge_log(self):
        """`demo.log` = the client stream with the driver lines spliced in.

        The client's own bytes are copied verbatim (`app.log` stays on disk as
        the raw capture), so an audit can diff the merged file against it.
        """
        try:
            with open(self.app_log_path, "r", encoding="utf-8",
                      errors="replace") as fh:
                app_lines = fh.read().splitlines()
        except FileNotFoundError:
            app_lines = []
        merged = self.annotate.merged(app_lines)
        with open(self.log_path, "w", encoding="utf-8") as fh:
            fh.write("\n".join(merged) + ("\n" if merged else ""))
        return len(merged)

    # -- run.json ----------------------------------------------------------
    def write_run(self):
        self.merge_log()
        measured = [c for c in self.captures if c.get("status") == "MEDIDA"]
        failed = [c for c in self.captures if c.get("status") != "MEDIDA"]
        missing = [name for name in CAPTURES
                   if name not in {c.get("label") for c in self.captures}]
        for name in missing:
            self.captures.append({"label": name, "status": "NO-RE-MEDIDA",
                                  "cause": "not-reached"})
        try:
            head = subprocess.run(["git", "rev-parse", "HEAD"], cwd=WS,
                                  capture_output=True, text=True).stdout.strip()
        except Exception:
            head = None
        with open(self.binary, "rb") as fh:
            binary_sha = hashlib.sha256(fh.read()).hexdigest()
        run = {
            "campaign": "DEMO-035",
            "annex": "M35",
            "cr": "CR-2fef7f8a-8af0-427c-8e38-2cf7c15a5dbb",
            "head": head,
            "ts": now_iso(),
            "binary": os.path.relpath(self.binary, WS),
            "binary_sha256": binary_sha,
            "app_args": [os.path.basename(arg) if index == 0 else arg
                         for index, arg in enumerate(self.app_args)],
            "spawn_override": {"argv": self.args.spawn,
                               "evidence": self.spawn_line},
            "display": self.args.display,
            "wayland_display": os.environ.get("WAYLAND_DISPLAY"),
            "session_type": os.environ.get("XDG_SESSION_TYPE"),
            "data_root": self.args.data_root,
            "log": os.path.relpath(self.log_path, WS),
            "app_log": os.path.relpath(self.app_log_path, WS),
            "window_id": hex(self.wid) if self.wid is not None else None,
            "window_name": (name_of(self.display, self.win)
                            if self.win is not None and self.display else None),
            "expected_resolution": [WIDTH, HEIGHT],
            "focus_acquired": self.focus_ok,
            "phase": self.phase,
            "input_route": ("core X11 KeyPress/KeyRelease via send_event with "
                            "verified focus (XTEST is inert for this client)"),
            "input_delivered": dict(self.delivered),
            "keys": self.keys,
            "captures": self.captures,
            "counters": {"probe": self.probe} if self.args.probe_counters else {},
            "measured": len(measured),
            "not_measured": len(failed),
            "notes": self.notes,
            "app_exit": self.app_exit,
            "exit": 0 if (len(measured) == len(CAPTURES)) else 1,
        }
        path = os.path.join(self.out, "run.json")
        with open(path, "w", encoding="utf-8") as fh:
            json.dump(run, fh, indent=2, sort_keys=True)
        print("RUN measured=%d not_measured=%d -> %s"
              % (len(measured), len(failed), os.path.relpath(path, WS)))
        self.exit_code = run["exit"]
        return run


# ---------------------------------------------------------------------------
# CLI.
# ---------------------------------------------------------------------------
def parse_args(argv=None):
    ap = argparse.ArgumentParser(
        description="DEMO-035 campaign driver (annex M35 sky L0-L4 + vegetation)")
    ap.add_argument("--bin", default=None,
                    help="window binary; default: release build, else debug")
    ap.add_argument("--out", default=os.path.join("evidence", "m35-demo"))
    ap.add_argument("--display", default=":0")
    ap.add_argument("--data-root",
                    default=os.path.expanduser("~/Games/Daggerfall/app"))
    ap.add_argument("--timeout", type=float, default=240.0,
                    help="window discovery deadline, seconds")
    ap.add_argument("--paint-timeout", type=float, default=90.0,
                    help="deadline for the first painted frame")
    ap.add_argument("--key-timeout", type=float, default=8.0,
                    help="deadline for the log effect of one key press")
    ap.add_argument("--key-attempts", type=int, default=3,
                    help="presses per key before the capture is NO-RE-MEDIDA")
    ap.add_argument("--frame-attempts", type=int, default=6,
                    help="re-grabs allowed while the sha repeats the previous")
    ap.add_argument("--probe-timeout", type=float, default=900.0)
    ap.add_argument("--spawn", default="band:land",
                    help="documented viewpoint override passed to the window: "
                         "origin | band:ocean | band:beach | band:land "
                         "(default `band:land`: the retail spawn of this "
                         "composition is an interior, no sky in frame)")
    ap.add_argument("--yaw", type=float, default=None,
                    help="explicit yaw in radians (wins over the yaw the band "
                         "viewpoint resolves); default: the client's own")
    ap.add_argument("--probe-counters", action="store_true",
                    help="also run `--probe BIOME-001` in a second process and "
                         "append its exit + verdict line to demo.log")
    ap.add_argument("--dry-run", action="store_true",
                    help="validate preconditions and print the plan; no window")
    return ap.parse_args(argv)


def dry_run(args, binary):
    checks = []
    checks.append(("binary", binary, os.path.isfile(binary) if binary else False))
    checks.append(("import", "ImageMagick import",
                   bool(subprocess.run(["sh", "-c", "command -v import"],
                                       capture_output=True).stdout.strip())))
    try:
        d = display.Display(args.display)
        info = "%s XTEST=%s" % (d.get_display_name(), d.has_extension("XTEST"))
        checks.append(("display", info, True))
    except Exception as exc:
        checks.append(("display", "%s: %s" % (args.display, exc), False))
    out = os.path.abspath(args.out)
    # The campaign creates `out`; a dry run checks the nearest existing parent.
    ancestor = out
    while ancestor and not os.path.exists(ancestor):
        ancestor = os.path.dirname(ancestor)
    out_ok = os.path.isdir(out) or (bool(ancestor) and os.access(ancestor, os.W_OK))
    checks.append(("out", out, out_ok))
    checks.append(("data-root", args.data_root, os.path.isdir(args.data_root)))
    print("DRY-RUN DEMO-035 (annex M35)")
    for name, detail, ok in checks:
        print("  %-10s %-4s %s" % (name, "OK" if ok else "FAIL", detail))
    print("  captures   %s" % " ".join(name + ".png" for name in CAPTURES))
    argv = [os.path.basename(binary or "daggerfall-app")]
    if args.spawn:
        argv += ["--spawn", args.spawn]
    if args.yaw is not None:
        argv += ["--yaw", "%.4f" % args.yaw]
    print("  argv       %s" % " ".join(argv))
    print("  keys       Enter, m x4 (dusk/night/dawn/day), n x2 "
          "(summer->autumn->winter)")
    if args.probe_counters:
        print("  counters   --probe %s (separate process)" % PROBE_NAME)
    return 0 if all(ok for _, _, ok in checks) else 2


def main(argv=None):
    args = parse_args(argv)
    binary = resolve_binary(args.bin)
    if args.dry_run:
        return dry_run(args, binary)
    if not binary:
        print("NO-BINARY: build it with `cargo build -p daggerfall-app "
              "--features window` (or pass --bin)")
        return 5
    campaign = Campaign(args, binary)
    try:
        code = campaign.run()
    except KeyboardInterrupt:
        campaign.teardown()
        campaign.write_run()
        return 130
    if code == 0:
        return getattr(campaign, "exit_code", 0)
    return code


if __name__ == "__main__":
    sys.exit(main())
