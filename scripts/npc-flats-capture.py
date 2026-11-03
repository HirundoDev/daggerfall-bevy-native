#!/usr/bin/env python3
"""NPC-FLATS capture driver (CR-f1b5045b) — opens the real window and shoots it.

Why this exists: the NPC flat reclassification (`resolve_flat`, commit
`ea1e463`) was verified headless (`--probe DEMO-001`); the owner asked for the
game itself: *abrir el juego* and see dressed NPCs. This driver does exactly
that and nothing else:

1. launch `target/release/daggerfall-app` (feature `window`, default = the
   integrated M18 window path — `main.rs` opens the demo when `--window` /
   `--legacy-m2` are absent),
2. find the X11 client window (`DAGGERFALL-LIKE…`), focus it,
3. `Return` to leave the MENU surface for PLAYING (the demo's own key),
4. capture ≥2 frames at different moments: `npc-start` (first painted world
   frame), then after the player *walks* (held W) and after *stepping back*
   (held S), plus an optional `npc-turn` frame after a rotation attempt
   (XTEST relative motion). Movement and rotation are read back from the
   client's own `M… VE: pos=… yaw=…` line, never assumed,
5. teardown SIGTERM→SIGKILL, report the client's exit code.

Input route is the one measured by `scripts/demo-035.py`: core X11
KeyPress/KeyRelease via `send_event` with verified focus (XTEST is inert for
this client's key path); rotation is the only thing that goes through XTEST,
and it is *verified*, not assumed — a frame is labelled `-turn` only when the
client printed a different `yaw=`.

Every capture carries sha256 + resolution + the black share; two identical
sha256 mean the render did not advance and the run says so (`duplicate_of`).
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
from Xlib.ext import xtest

WS = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
TITLE_PREFIX = "DAGGERFALL-LIKE"
# The camera/locomotion line the client prints under the M18 window path is
# `M19 VE: pos=… yaw=…` (the M19 locomotion layer owns the eye), measured
# 2026-10-26. The M2 prefix is kept as an alternative because the M2 path
# prints the same numbers.
POS_RE = r"M(?:19|2) VE: pos="
CAPTURES = ("npc-start", "npc-walk", "npc-back")


def now_iso():
    return time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())


def name_of(d, win):
    try:
        name = win.get_wm_name()
    except Exception:
        return ""
    if isinstance(name, bytes):
        return name.decode("utf-8", "replace")
    return name or ""


def walk_named(d, win, depth=0, acc=None):
    if acc is None:
        acc = []
    if depth > 4:
        return acc
    try:
        children = win.query_tree().children
    except Exception:
        return acc
    for child in children:
        acc.append(child)
        walk_named(d, child, depth + 1, acc)
    return acc


def geometry(win):
    def walk(win, depth=0):
        try:
            geo = win.get_geometry()
            if geo.width > 1 and geo.height > 1:
                return geo
        except Exception:
            pass
        if depth > 4:
            return None
        try:
            kids = win.query_tree().children
        except Exception:
            return None
        for kid in kids:
            found = walk(kid, depth + 1)
            if found:
                return found
        return None

    return walk(win)


def find_window(d, proc, timeout, needle=TITLE_PREFIX):
    root = d.screen().root
    deadline = time.time() + timeout
    while time.time() < deadline:
        for win in walk_named(d, root):
            if needle in name_of(d, win).upper():
                if geometry(win) is not None:
                    return win, win.id, name_of(d, win)
        if proc.poll() is not None:
            return None
        time.sleep(0.25)
    return None


def current_focus(d):
    try:
        focus = d.get_input_focus().focus
        return getattr(focus, "id", None)
    except Exception:
        return None


NET_ACTIVE_WINDOW = None


def activate(d, win, timeout=6.0):
    global NET_ACTIVE_WINDOW
    root = d.screen().root
    if NET_ACTIVE_WINDOW is None:
        NET_ACTIVE_WINDOW = d.intern_atom("_NET_ACTIVE_WINDOW")
    try:
        root.send_event(
            xevent.ClientMessage(
                window=win,
                client_type=NET_ACTIVE_WINDOW,
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
    if current_focus(d) == win.id:
        return True
    return activate(d, win)


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


def hold_key(d, win, name, seconds):
    send_key(d, win, name, True)
    time.sleep(seconds)
    send_key(d, win, name, False)
    time.sleep(0.25)


def turn(d, pixels, steps=12):
    """Relative-looking rotation through XTEST pointer motion.

    The client confines the cursor and reads the raw delta; the pointer is
    warped away and back so the net movement is a real relative delta, not a
    single absolute jump. Whether it reached the camera is read back from the
    client's own `yaw=` line — this function never claims the turn.
    """
    root = d.screen().root
    geo = root.get_geometry()
    cx, cy = geo.width // 2, geo.height // 2
    try:
        xtest.fake_input(d, X.MotionNotify, x=cx, y=cy)
        d.sync()
    except Exception as exc:
        return False, "xtest motion unavailable: %s" % exc
    for _ in range(steps):
        try:
            xtest.fake_input(d, X.MotionNotify, x=cx + pixels, y=cy)
            d.sync()
            time.sleep(0.03)
            xtest.fake_input(d, X.MotionNotify, x=cx, y=cy)
            d.sync()
        except Exception as exc:
            return False, "xtest motion failed mid-turn: %s" % exc
    time.sleep(0.3)
    return True, "xtest relative motion %dpx x%d" % (pixels, steps)


class LogTail:
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

    def right_now(self):
        self.pump()
        return len(self.history)

    def wait(self, patterns, timeout, start=None):
        if isinstance(patterns, str):
            patterns = [patterns]
        compiled = [(p, re.compile(p)) for p in patterns]
        found = {}
        first = self.right_now() if start is None else start
        deadline = time.time() + timeout
        while True:
            self.pump()
            for line in self.history[first:]:
                for pattern, rx in compiled:
                    if pattern not in found and rx.search(line):
                        found[pattern] = line.strip()
            if len(found) == len(compiled):
                return found
            if time.time() >= deadline:
                return found or None
            time.sleep(0.1)

    def find(self, pattern):
        self.pump()
        rx = re.compile(pattern)
        hits = []
        for line in self.history:
            match = rx.search(line)
            if match:
                hits.append(line.strip())
        return hits


def stats(png):
    with open(png, "rb") as fh:
        raw = fh.read()
    im = Image.open(png).convert("RGB")
    width, height = im.size
    red, green, blue = im.split()
    max_channel = ImageChops.lighter(ImageChops.lighter(red, green), blue)
    hist = max_channel.histogram()
    nonblack = sum(hist[9:]) / float(width * height)
    mean_r, mean_g, mean_b = ImageStat.Stat(im).mean
    return {
        "sha256": hashlib.sha256(raw).hexdigest(),
        "bytes": len(raw),
        "width": width,
        "height": height,
        "mean_luma": round(0.2126 * mean_r + 0.7152 * mean_g + 0.0722 * mean_b, 2),
        "nonblack_ratio": round(nonblack, 4),
        "unique_colors": len(im.getcolors(maxcolors=1 << 24) or []),
    }


def window_origin(win, root):
    """Absolute top-left of `win` on the root window.

    Measured 2026-10-26 on this host (python-xlib 0.33): `translate_coords`
    returns an unusable reply (`reply()` yields `None`, attributes absent), so
    the origin is summed up the parent chain instead — geometry `x`/`y` are
    parent-relative, which is exactly what the walk needs.
    """
    x = y = 0
    node = win
    for _ in range(10):
        geo = node.get_geometry()
        x += geo.x
        y += geo.y
        if node.id == root.id:
            break
        parent = node.query_tree().parent
        if parent is None:
            break
        node = parent
    return x, y


def capture(d, win, wid, out, label):
    """Two grabs, one frame of record.

    Measured 2026-10-26 on this host:
    - `import -window <id>` on this Xwayland client can return a *stale* backing
      pixmap (two grabs ten seconds and ten metres apart were byte-identical),
    - `import -window root` is broken in ImageMagick 7.1.2-34 here (`missing an
      image filename`, exit 1),
    - `spectacle -f -b -n -o <file>` captures the composited screen correctly.

    So the frame of record is the composited screen (spectacle → PIL crop to the
    client window rect); the window grab is kept beside it as
    `window_grab_sha256` for the comparison, never as the evidence frame.
    """
    direct = os.path.join(out, label + ".png")
    geo = win.get_geometry()
    left, top = window_origin(win, d.screen().root)
    rect = "%dx%d+%d+%d" % (geo.width, geo.height, left, top)
    window_grab = os.path.join(out, label + ".window-grab.png")
    subprocess.run(["import", "-window", str(wid), window_grab],
                   check=True, capture_output=True)
    window_rec = stats(window_grab)
    os.unlink(window_grab)
    screen = os.path.join(out, label + ".screen.png")
    subprocess.run(["spectacle", "-f", "-b", "-n", "-o", screen],
                   check=True, capture_output=True)
    with Image.open(screen) as shot:
        frame = shot.convert("RGB").crop(
            (left, top, left + geo.width, top + geo.height))
        frame.save(direct, "PNG")
    os.unlink(screen)
    rec = stats(direct)
    rec["method"] = ("spectacle -f -b -n (composited screen) + PIL crop %s"
                     % rect)
    rec["window_rect"] = rect
    rec["window_grab_sha256"] = window_rec["sha256"]
    rec["window_grab_nonblack_ratio"] = window_rec["nonblack_ratio"]
    rec["window_grab_matches_screen"] = window_rec["sha256"] == rec["sha256"]
    rec["label"] = label
    print("%-10s sha=%s %dx%d luma=%6.2f nonblack=%.3f window_grab_same=%s"
          % (label, rec["sha256"][:12], rec["width"], rec["height"],
             rec["mean_luma"], rec["nonblack_ratio"],
             rec["window_grab_matches_screen"]))
    return rec


def run(args):
    out = os.path.abspath(args.out)
    os.makedirs(out, exist_ok=True)
    app_log = os.path.join(out, "app.log")
    tail = LogTail(app_log)
    env = dict(os.environ)
    env["DISPLAY"] = args.display
    env["DF_APP_ROOT"] = args.data_root
    sink = open(app_log, "wb")
    started = time.time()
    try:
        proc = subprocess.Popen([args.bin], stdout=sink,
                                stderr=subprocess.STDOUT, env=env, cwd=WS,
                                start_new_session=True)
    finally:
        sink.close()
    print("LAUNCH pid=%d argv=%r DISPLAY=%s WAYLAND_DISPLAY=%s"
          % (proc.pid, [args.bin], env.get("DISPLAY"),
             env.get("WAYLAND_DISPLAY")))
    run_rec = {
        "ts": now_iso(),
        "argv": [args.bin],
        "display": env.get("DISPLAY"),
        "wayland_display": env.get("WAYLAND_DISPLAY"),
        "binary": os.path.relpath(args.bin, WS),
        "pid": proc.pid,
        "captures": [],
        "notes": [],
    }
    d = None
    exit_code = None
    try:
        try:
            d = display.Display(args.display)
        except Exception as exc:
            run_rec["notes"].append("NO-DISPLAY %s: %s" % (args.display, exc))
            print(run_rec["notes"][-1])
            return 3, run_rec
        found = find_window(d, proc, args.timeout)
        if not found:
            run_rec["notes"].append("NO-WINDOW after %.0fs (app exit=%s)"
                                    % (args.timeout, proc.poll()))
            print(run_rec["notes"][-1])
            return 4, run_rec
        win, wid, title = found
        run_rec["window_id"] = hex(wid)
        run_rec["window_title"] = title
        geo = geometry(win)
        run_rec["window_size"] = [geo.width, geo.height]
        print("WINDOW %s %r size=%s" % (hex(wid), title, run_rec["window_size"]))
        run_rec["focus_acquired"] = activate(d, win)
        print("FOCUS_ACQUIRED=%s" % run_rec["focus_acquired"])

        # The composition prints its roster before the first frame; wait for it
        # so the capture is of a world that reported its NPCs, not of a logo.
        # The names carry a space (`Tristyval Buckingwing`), hence `.+` and not
        # `\S+` (measured: the first campaign's `\S+` matched 0 of 28 lines).
        roster = tail.wait([r"M18 VE: NPC .+ \("],
                           min(args.timeout, 180.0), start=0)
        run_rec["roster_reported"] = roster is not None

        paint = tail.wait([POS_RE], min(args.timeout, 120.0), start=0)
        run_rec["painted"] = paint is not None
        if paint is None:
            run_rec["notes"].append("no `%s` line: the 3D surface never painted"
                                    % POS_RE)

        enter = tail.right_now()
        phase = None
        for _ in range(args.key_attempts):
            ensure_focus(d, win)
            press(d, win, "Return")
            hit = tail.wait([r"M18 VE: phase MENU .+ PLAYING"],
                            args.key_timeout, start=enter)
            if hit:
                phase = "PLAYING"
                break
        run_rec["phase"] = phase
        if phase != "PLAYING":
            run_rec["notes"].append("Return not delivered: the frame stayed on "
                                    "the MENU surface")
        time.sleep(args.settle)

        first = capture(d, win, wid, out, "npc-start")
        first["pos_line"] = (tail.find(POS_RE) or [None])[-1]
        run_rec["captures"].append(first)
        run_rec["painted"] = first["nonblack_ratio"] >= 0.5
        if not run_rec["painted"]:
            run_rec["notes"].append("the first frame is %.1f%% black: the 3D "
                                    "surface did not paint"
                                    % (100.0 * (1.0 - first["nonblack_ratio"])))
        pos_start = tail.find(POS_RE)
        run_rec["pos_before_walk"] = pos_start[-1] if pos_start else None

        # 1) A sequence of measured moments: each step is `key:seconds`, and
        #    `none:0` shoots the frame without touching the world. Every frame
        #    carries the client's own `pos=… yaw=…` line and the caption it had
        #    printed, so the distance of each NPC at that frame is readable.
        for index, (key, seconds) in enumerate(args.steps, start=1):
            if key != "none" and seconds > 0:
                mark = tail.right_now()
                ensure_focus(d, win)
                hold_key(d, win, key, seconds)
                tail.wait([POS_RE], 4.0, start=mark)
            label = "npc-s%02d-%s" % (index, "idle" if key == "none" else key)
            shot = capture(d, win, wid, out, label)
            shot["step"] = "%s:%.2f" % (key, seconds)
            shot["pos_line"] = (tail.find(POS_RE) or [None])[-1]
            shot["last_caption"] = (tail.find(r"M18 VE: NPC caption") or [None])[-1]
            run_rec["captures"].append(shot)
            run_rec["pos_after_steps"] = shot["pos_line"]
            run_rec["last_caption"] = shot["last_caption"]

        # 3) turn: the yaw is read back from the client; the frame is only
        #    called a measured rotation when the number changed.
        yaw_before = re.findall(r"yaw=(-?\d+\.\d+|\d+\.\d+)",
                                run_rec.get("pos_after_steps")
                                or run_rec.get("pos_before_walk") or "")
        mark = tail.right_now()
        ok, detail = turn(d, args.turn_pixels, args.turn_steps)
        run_rec["turn_route"] = detail
        turned = tail.wait([POS_RE], 4.0, start=mark)
        yaw_after = re.findall(r"yaw=(-?\d+\.\d+|\d+\.\d+)",
                               (tail.find(POS_RE) or [""])[-1])
        moved_yaw = bool(yaw_before) and bool(yaw_after) and yaw_before[-1] != yaw_after[-1]
        run_rec["yaw_before"] = yaw_before[-1] if yaw_before else None
        run_rec["yaw_after"] = yaw_after[-1] if yaw_after else None
        run_rec["turn_verified"] = moved_yaw
        if not args.turn_capture and not moved_yaw:
            run_rec["notes"].append(
                "camera yaw did not change after XTEST motion (%s, xtest_ok=%s, "
                "pos_line=%s): no rotation frame is recorded as measured"
                % (detail, ok, bool(turned)))
        if args.turn_capture:
            shot = capture(d, win, wid, out, "npc-turn")
            shot["turn_verified"] = moved_yaw
            shot["pos_line"] = (tail.find(POS_RE) or [None])[-1]
            run_rec["captures"].append(shot)

        shas = {}
        for rec in run_rec["captures"]:
            if rec["sha256"] in shas:
                rec["duplicate_of"] = shas[rec["sha256"]]
            else:
                shas[rec["sha256"]] = rec["label"]
        run_rec["distinct_frames"] = len(shas)

        # Evidence slices from the client's own stream.
        run_rec["npc_roster"] = tail.find(r"M18 VE: NPC .+ \(")
        run_rec["caption_lines"] = tail.find(r"M18 VE: NPC caption")
        run_rec["guard_lines"] = tail.find(r"M18 VE: guard billboard")
        run_rec["pos_lines"] = tail.find(POS_RE)
        run_rec["composition_lines"] = tail.find(r"M18 VE: (?!NPC)")
        run_rec["attribution_lines"] = tail.find(r"flat \d+")
    finally:
        if d is not None:
            try:
                if proc.poll() is None:
                    os.kill(proc.pid, signal.SIGTERM)
                    deadline = time.time() + 10.0
                    while time.time() < deadline and proc.poll() is None:
                        time.sleep(0.2)
                if proc.poll() is None:
                    os.kill(proc.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
        try:
            proc.wait(timeout=5)
        except Exception:
            pass
        exit_code = proc.poll()
        run_rec["app_exit"] = exit_code
        run_rec["app_exit_note"] = ("SIGTERM after the capture (the demo has no "
                                    "self-exit): -15 == stopped by driver"
                                    if exit_code in (-15, -9) else "self")
        run_rec["duration_s"] = round(time.time() - started, 1)
        tail.pump()
        run_rec["log_lines"] = len(tail.history)
        with open(os.path.join(out, "run.json"), "w", encoding="utf-8") as fh:
            json.dump(run_rec, fh, indent=2, sort_keys=True)
        with open(os.path.join(out, "app-log-slice.txt"), "w", encoding="utf-8") as fh:
            for key in ("npc_roster", "guard_lines", "caption_lines",
                        "composition_lines"):
                fh.write("## %s\n" % key)
                for line in run_rec.get(key) or []:
                    fh.write(line + "\n")
                fh.write("\n")
    return 0, run_rec


def default_binary():
    candidate = os.path.join(WS, "target/release/daggerfall-app")
    return candidate if os.path.exists(candidate) else None


def main(argv=None):
    ap = argparse.ArgumentParser(description="NPC-FLATS window capture driver "
                                            "(CR-f1b5045b)")
    ap.add_argument("--bin", default=None)
    ap.add_argument("--out", default=os.path.join(
        "docs/sintergic/l5-mission/npc-flats-reclass-CR-f1b5045b/evidence"))
    ap.add_argument("--display", default=":0")
    ap.add_argument("--data-root", default=os.path.expanduser("~/Games/Daggerfall/app"))
    ap.add_argument("--timeout", type=float, default=240.0)
    ap.add_argument("--settle", type=float, default=2.0)
    ap.add_argument("--key-timeout", type=float, default=8.0)
    ap.add_argument("--key-attempts", type=int, default=3)
    ap.add_argument("--seq", default="none:0,d:0.5,w:0.35,w:0.35,w:0.35,a:0.5",
                    help="comma list of `key:seconds` steps; a frame is shot "
                         "after each step (`none` = shoot without moving)")
    ap.add_argument("--turn-capture", action="store_true",
                    help="also shoot a frame after the XTEST turn attempt; the "
                         "frame carries turn_verified from the client's yaw")
    ap.add_argument("--turn-pixels", type=int, default=90)
    ap.add_argument("--turn-steps", type=int, default=12)
    args = ap.parse_args(argv)
    args.steps = []
    for raw in args.seq.split(","):
        raw = raw.strip()
        if not raw:
            continue
        key, _, seconds = raw.partition(":")
        args.steps.append((key.strip().lower(), float(seconds or 0)))
    args.bin = os.path.abspath(args.bin or default_binary() or "")
    if not args.bin or not os.path.isfile(args.bin):
        print("NO-BINARY: build it with `cargo build -p daggerfall-app "
              "--features window --release` (or pass --bin)")
        return 5
    code, run_rec = run(args)
    print("RUN distinct_frames=%s app_exit=%s -> %s"
          % (run_rec.get("distinct_frames"), run_rec.get("app_exit"),
             os.path.join(args.out, "run.json")))
    return code


if __name__ == "__main__":
    sys.exit(main())
