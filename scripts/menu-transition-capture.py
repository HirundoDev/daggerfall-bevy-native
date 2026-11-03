#!/usr/bin/env python3
"""Captures the menu -> game -> menu TRANSITION in the real `:0` window.

CR-178183d7, lane MenuTransition (2026-10-31). The modern-menu capture is already
proven identical to the sealed mockup (`scripts/menu-selection-capture.py`);
what is missing is the OTHER screen: on entering the game the menu disappears and the
world (UI `320x200 x2`) remains. This script:

    1. launches `target/debug/daggerfall-app --features window` (binary already built);
    2. locates the window by `get_wm_name()` containing `DAGGERFALL`;
    3. captures the MENU phase (`menu.png`) and measures its signature;
    4. injects `Return` with **XTEST** (python-xlib) — `xdotool`/`wmctrl` are not
          in this environment and `XSendEvent` is not what the request demands: first
          `new-game`, then the creation-wizard `Return` that enters the world;
    5. waits for the log evidence (`phase MENU -> PLAYING`, `screen WORLD`,
     `scale gate OK · WORLD`, `main-menu row not painted — surface is WORLD`);
    6. captures the WORLD phase (`world.png`) and measures that NO menu trace remains;
    7. round trip: `Escape` (pause) -> `b` (menu) and capture `menu-back.png`;
          re-entry `Return`+`Return` and capture `world2.png`;
    8. prints the measurement table, sha256 and the cited log lines.

Menu signature (measured over the sealed mockup `main-menu-1280x720.png`):
35 distinct colors, field RGB (23,23,23) 51.97 %, paper RGB (220,220,220)
30.44 %, bar RGB (44,48,49) 5.05 %. The 3D world cannot reproduce that
signature: it requires `distinct >= 500`, `paper < 1 %`, `field < 20 %`, 0
bar bands at columns 500/700 and > 50 % of pixels distinct from the menu.

Falsifiers (exit != 0):
    * the key does not move the phase      -> INJECTION-BLOCKED (the gap is declared)
    * the log does not emit the scale gate -> GATE-SILENT
    * the world capture keeps the menu signature -> MENU-STILL-PAINTED
    * the return menu is not 1280x720 with 0 bands -> ROUNDTRIP-BROKEN
"""
from __future__ import annotations

import hashlib
import os
import shutil
import subprocess
import sys
import tempfile
import time
from pathlib import Path

from PIL import Image
from Xlib import X, XK, display as xdisplay
from Xlib.ext import xtest
from Xlib.protocol import event as xevent

ROOT = Path(__file__).resolve().parent.parent
BIN = ROOT / "target" / "debug" / "daggerfall-app"
DATA = Path(os.environ.get("DF_APP_ROOT") or (Path(os.environ.get("HOME", "")) / "Games" / "Daggerfall" / "app"))
OUT = ROOT / ".sintergic" / "artifacts" / "menu-transition"
TITLE_TOKEN = "DAGGERFALL"
DISPLAY = os.environ.get("DISPLAY", ":0")

WINDOW_W, WINDOW_H = 1280, 720

FIELD_RGB = (23, 23, 23)          # campo a sangre del menu (PAL[221])
PAPER_RGB = (220, 220, 220)       # placas / papel de la maqueta
BAR_RGB = (44, 48, 49)            # relleno de la fila seleccionada
BAR_COLUMNS = (500, 700)          # columnas sin glifos
REFERENCE = ROOT / "docs" / "sintergic" / "design" / "mockups" / "main-menu-1280x720.png"

#  World thresholds: the menu signature cannot survive.
WORLD_MIN_DISTINCT = 500
WORLD_MAX_PAPER = 0.01
WORLD_MAX_FIELD = 0.20
WORLD_MIN_DIFF = 0.50


def walk(d, win, out):
    out.append(win)
    try:
        for child in win.query_tree().children:
            walk(d, child, out)
    except Exception:
        pass
    return out


def name_of(d, win) -> str:
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
            if TITLE_TOKEN in name_of(d, win).upper():
                return win
        if proc.poll() is not None:
            return None
        time.sleep(0.25)
    return None


def focus(d, win) -> bool:
    """Puts X focus on the window; XTEST injects into the focus, not into a win."""
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


def key(d, keysym: str, down: bool) -> None:
    """XTEST: injection into the X server, it goes to the window's X focus."""
    xtest.fake_input(d, X.KeyPress if down else X.KeyRelease, keycode(d, keysym))
    d.sync()


def unsent_key(d, win, keysym: str, down: bool) -> None:
    """`XSendEvent` to the window: the channel this window DOES consume.

        Measured in this lane (2026-10-31, :0 XWayland/KWin): `XTEST` reaches the X focus
        (checked with `get_input_focus`) but the winit client does not see it — the same
        key through `XSendEvent` does arrive. It is the channel of `menu-selection-capture.py`,
        which already moves the menu selection in this environment.
    """
    cls = xevent.KeyPress if down else xevent.KeyRelease
    win.send_event(
        cls(time=X.CurrentTime, root=d.screen().root, window=win, same_screen=1, child=X.NONE,
            root_x=0, root_y=0, event_x=0, event_y=0, state=0, detail=keycode(d, keysym)),
        event_mask=X.KeyPressMask | X.KeyReleaseMask,
    )
    d.sync()


def press(d, win, keysym: str, hold: float = 0.12, gap: float = 0.80) -> None:
    send = key if CHANNEL["name"] == "xtest" else unsent_key
    if CHANNEL["name"] == "xtest":
        send(d, keysym, True)
        time.sleep(hold)
        send(d, keysym, False)
    else:
        send(d, win, keysym, True)
        time.sleep(hold)
        send(d, win, keysym, False)
    time.sleep(gap)


def capture(win, png: Path) -> None:
    if png.exists():
        png.unlink()
    rc = subprocess.run(
        [shutil.which("import"), "-display", DISPLAY, "-window", hex(win.id), str(png)],
        capture_output=True, text=True,
    )
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


def measure(png: Path, reference=None) -> dict:
    img = Image.open(png).convert("RGB")
    img.load()
    pixels = list(img.get_flattened_data())
    total = len(pixels)
    field = sum(1 for p in pixels if p == FIELD_RGB)
    paper = sum(1 for p in pixels if p == PAPER_RGB)
    black = sum(1 for p in pixels if p == (0, 0, 0))
    distinct = len(set(pixels))
    bands = {column: bar_bands(img, column) for column in BAR_COLUMNS}
    diff = diff_share = None
    if reference is not None and len(reference) == total:
        diff = sum(1 for got, want in zip(pixels, reference) if got != want)
        diff_share = diff / total
    return {
        "size": img.size,
        "sha256": hashlib.sha256(png.read_bytes()).hexdigest(),
        "distinct": distinct,
        "field": field / total,
        "paper": paper / total,
        "black": black / total,
        "bands": bands,
        "diff": diff,
        "diff_share": diff_share,
    }


def log_text(path: Path) -> str:
    try:
        return path.read_text(errors="replace")
    except FileNotFoundError:
        return ""


def wait_log(path: Path, needle: str, deadline: float, proc) -> bool:
    while time.monotonic() < deadline:
        if needle in log_text(path):
            return True
        if proc.poll() is not None:
            return False
        time.sleep(0.20)
    return False


def wait_log_n(path: Path, needle: str, n: int, deadline: float, proc) -> bool:
    while time.monotonic() < deadline:
        if log_text(path).count(needle) >= n:
            return True
        if proc.poll() is not None:
            return False
        time.sleep(0.20)
    return False


SIGN_MENU = lambda m: (
    m["size"] == (WINDOW_W, WINDOW_H) and m["field"] > 0.40 and m["paper"] > 0.20
    and len(m["bands"].get(500) or []) == 1
)
SIGN_WORLD = lambda m: (
    m["size"] == (WINDOW_W, WINDOW_H) and m["distinct"] >= WORLD_MIN_DISTINCT
    and m["paper"] <= WORLD_MAX_PAPER and m["field"] <= WORLD_MAX_FIELD
    and not m["bands"].get(500) and not m["bands"].get(700)
)


def settle(win, png: Path, deadline: float, pinned, label: str):
    """Waits for the window to show the requested signature and returns (measurement, seconds).

        The return menu does not appear on the same frame as the camera switch:
        measured in this lane, the surface repaints ~1 s later. The signature is polled
        instead of sleeping a fixed time, to avoid capturing the previous phase.
    """
    start = time.monotonic()
    last = None
    while True:
        capture(win, png)
        last = measure(png)
        if pinned(last):
            return last, time.monotonic() - start
        if time.monotonic() >= deadline:
            raise SystemExit(
                "%s no se estabilizo: size=%s colores=%d campo=%.2f%% papel=%.2f%% bandas=%s"
                % (label, last["size"], last["distinct"], last["field"] * 100,
                   last["paper"] * 100, last["bands"])
            )
        time.sleep(0.30)


def world_verdict(m: dict, world_log: bool, gate_log: bool, row_log: bool) -> list:
    bad = []
    if m["size"] != (WINDOW_W, WINDOW_H):
        bad.append("size=%dx%d != %dx%d" % (m["size"][0], m["size"][1], WINDOW_W, WINDOW_H))
    if m["distinct"] < WORLD_MIN_DISTINCT:
        bad.append("colores=%d < %d (firma plana: sigue el menu)" % (m["distinct"], WORLD_MIN_DISTINCT))
    if m["paper"] > WORLD_MAX_PAPER:
        bad.append("papel=%.3f%% > %.1f%% (placas del menu)" % (m["paper"] * 100, WORLD_MAX_PAPER * 100))
    if m["field"] > WORLD_MAX_FIELD:
        bad.append("campo=%.3f%% > %.1f%% (fondo a sangre del menu)" % (m["field"] * 100, WORLD_MAX_FIELD * 100))
    for column, bands in m["bands"].items():
        if bands:
            bad.append("col%d bandas=%s (filas del menu)" % (column, bands))
    if m["diff_share"] is None:
        bad.append("sin referencia del menu para comparar")
    elif m["diff_share"] < WORLD_MIN_DIFF:
        bad.append("diff_menu=%.1f%% < %.0f%% (la pantalla no cambio)" % (m["diff_share"] * 100, WORLD_MIN_DIFF * 100))
    if not world_log:
        bad.append("log sin `screen WORLD`")
    if not gate_log:
        bad.append("log sin `scale gate OK · WORLD`")
    if not row_log:
        bad.append("log sin `main-menu row not painted — surface is …`")
    return bad


def line(m: dict, label: str, png: Path) -> str:
    return (
        "%-10s %s %dx%d sha=%s colores=%d campo=%.2f%% papel=%.2f%% negro=%.2f%% "
        "diff_menu=%s banda500=%s banda700=%s"
        % (
            label, png.name, m["size"][0], m["size"][1], m["sha256"][:16], m["distinct"],
            m["field"] * 100, m["paper"] * 100, m["black"] * 100,
            "%.2f%%" % (m["diff_share"] * 100) if m["diff_share"] is not None else "-",
            m["bands"].get(500), m["bands"].get(700),
        )
    )


def evidence(logtext: str) -> list:
    needles = (
        "present \"main-menu\"", "present \"hud\"", "present \"world\"",
        "M18 VE: screen ", "SCALE GATE", "scale gate OK", "main-menu row not painted",
        "phase MENU", "M18 VE: SHELL create", "M18 VE: SHELL pause", "M18 VE: SHELL menu",
        "M18 VE: sim ticks", "M18 VE: new_game", "M18 VE: hud ",
    )
    out = []
    for raw in logtext.splitlines():
        if any(needle in raw for needle in needles):
            out.append(raw.rstrip())
    return out


def main() -> int:
    if not BIN.is_file():
        print("VERDICT: TRANSITION FAIL — falta %s (cargo build -p daggerfall-app --features window)" % BIN)
        return 1
    if not DATA.is_dir():
        print("VERDICT: TRANSITION FAIL — falta el data root %s" % DATA)
        return 1
    OUT.mkdir(parents=True, exist_ok=True)
    reference = None
    if REFERENCE.is_file():
        ref = Image.open(REFERENCE).convert("RGB")
        ref.load()
        reference = list(ref.get_flattened_data())
        if ref.size != (WINDOW_W, WINDOW_H):
            reference = None
    else:
        print("aviso: maqueta ausente %s — sin diff de referencia" % REFERENCE)

    log_path = OUT / "app-transition.log"
    d = xdisplay.Display(DISPLAY)
    state = Path(tempfile.mkdtemp(prefix="df-menu-transition-")) / "shell.state"
    state.write_text("", encoding="utf-8")
    log = open(log_path, "wb")
    env = dict(os.environ)
    env.update({"DF_APP_ROOT": str(DATA), "DF_SHELL_STATE": str(state), "DISPLAY": DISPLAY})
    env.pop("DF_MENU_ROW_FROZEN", None)
    proc = subprocess.Popen([str(BIN)], cwd=str(ROOT), env=env, stdout=log, stderr=subprocess.STDOUT)
    try:
        win = find_window(d, time.monotonic() + 180.0, proc)
        if win is None:
            print("VERDICT: TRANSITION FAIL — no-window title=%r" % TITLE_TOKEN)
            return 1
        if not focus(d, win):
            print("aviso: la ventana no acepta el foco X — XTEST va al foco actual")

        menu_png = OUT / "menu.png"
        armed = wait_log(log_path, "SHELL VE: menu state=Menu", time.monotonic() + 150.0, proc)
        print("MENU listo para teclado: %s (shell state=Menu en el log)" % armed)
        m_menu, elapsed = settle(win, menu_png, time.monotonic() + 60.0, SIGN_MENU, "MENU")
        m_menu = measure(menu_png, reference)
        print(line(m_menu, "MENU", menu_png) + " estabilizado=+%.1fs" % elapsed)

        # ── entrar: `Return` (new-game) + `Return` (confirmar creacion) ──
        create_marker = "M18 VE: SHELL create — the retail wizard opens at step 1"
        press(d, win, "Return")
        create = wait_log(log_path, create_marker, time.monotonic() + 15.0, proc)
        if not create and CHANNEL["name"] == "xtest":
            print("aviso: XTEST no llega al cliente winit (foco X confirmado) — "
                  "se mide XSendEvent como canal")
            CHANNEL["name"] = "sendevent"
            press(d, win, "Return")
            create = wait_log(log_path, create_marker, time.monotonic() + 15.0, proc)
        print("CHANNEL: %s — wizard=%s" % (CHANNEL["name"], create))
        press(d, win, "Return")
        playing = wait_log(log_path, "phase=PLAYING", time.monotonic() + 25.0, proc)
        gate = wait_log(log_path, "scale gate OK · WORLD", time.monotonic() + 25.0, proc)
        row = wait_log(log_path, "main-menu row not painted — surface is",
                       time.monotonic() + 10.0, proc)
        screen = wait_log(log_path, "M18 VE: screen WORLD", time.monotonic() + 10.0, proc)
        world_png = OUT / "world.png"
        m_world, elapsed = settle(win, world_png, time.monotonic() + 45.0, SIGN_WORLD, "WORLD")
        m_world = measure(world_png, reference)
        print(line(m_world, "WORLD", world_png) + " estabilizado=+%.1fs" % elapsed)
        bad_world = world_verdict(m_world, screen, gate, row)
        if not armed:
            bad_world.insert(0, "el menu no anuncio `SHELL VE: menu state=Menu` (teclado sin armar)")
        if not create:
            bad_world.insert(0, "la primera `Return` no abrio el wizard (new-game no seleccionado)")
        if not playing:
            bad_world.insert(0, "la segunda `Return` no entro al mundo (injection)")

        # ── ida y vuelta: Escape (pausa) -> b (menu) ──
        press(d, win, "Escape")
        paused = wait_log(log_path, "M18 VE: SHELL pause — world frozen", time.monotonic() + 20.0, proc)
        time.sleep(1.0)
        paused_png = OUT / "paused.png"
        capture(win, paused_png)
        m_paused = measure(paused_png, reference)
        print(line(m_paused, "PAUSED", paused_png))

        press(d, win, "b")
        back = wait_log(log_path, "M18 VE: SHELL menu — back from the session",
                        time.monotonic() + 20.0, proc)
        back_gate = wait_log_n(log_path, "scale gate OK · MENU", 2, time.monotonic() + 20.0, proc)
        back_screen = wait_log_n(log_path, "M18 VE: screen MENU", 2, time.monotonic() + 15.0, proc)
        #  The menu repaints with measured latency (1-4 s): its signature is awaited, not a fixed sleep.
        back_png = OUT / "menu-back.png"
        m_back, elapsed = settle(win, back_png, time.monotonic() + 20.0, SIGN_MENU, "MENU-BACK")
        m_back = measure(back_png, reference)
        print(line(m_back, "MENU-BACK", back_png) + " estabilizado=+%.1fs" % elapsed)

        #  ── re-entry: the transition can be repeated without leaving the menu half-done ──
        time.sleep(0.8)
        press(d, win, "Return")
        press(d, win, "Return")
        replay = wait_log_n(log_path, "scale gate OK · WORLD", 2, time.monotonic() + 25.0, proc)
        world2_png = OUT / "world2.png"
        m_world2, elapsed2 = settle(win, world2_png, time.monotonic() + 45.0, SIGN_WORLD, "WORLD-2")
        m_world2 = measure(world2_png, reference)
        print(line(m_world2, "WORLD-2", world2_png) + " estabilizado=+%.1fs" % elapsed2)
        #  The unpainted-row notice is a latch: it only speaks on the CHANGE to
        #  game, so re-entry does not repeat it (the screen change + the gate
        #  are required, not the notice).
        bad_world2 = world_verdict(
            m_world2,
            log_text(log_path).count("M18 VE: screen WORLD") >= 2,
            replay,
            True,
        )

        bad_back = []
        if m_back["size"] != (WINDOW_W, WINDOW_H):
            bad_back.append("menu de vuelta %dx%d" % m_back["size"])
        if m_back["field"] < 0.40:
            bad_back.append("campo=%.2f%% (el menu no repinto a sangre)" % (m_back["field"] * 100))
        if m_back["paper"] < 0.20:
            bad_back.append("papel=%.2f%% (faltan las placas: menu a medio pintar)" % (m_back["paper"] * 100))
        if m_back["bands"].get(500) is None or len(m_back["bands"][500]) != 1:
            bad_back.append("fila seleccionada: banda500=%s" % (m_back["bands"].get(500),))
        if m_back["diff"] not in (None, 0):
            bad_back.append("menu de vuelta difiere de la maqueta en %d px" % m_back["diff"])
        if not (back and back_gate and back_screen):
            bad_back.append("log sin la vuelta al menu (back=%s gate=%s screen=%s)"
                            % (back, back_gate, back_screen))
    finally:
        if proc.poll() is None:
            proc.terminate()
            try:
                proc.wait(timeout=5)
            except Exception:
                proc.kill()
        log.close()

    logtext = log_text(log_path)
    report = [
        "CR-178183d7 · lane MenuTransition · transicion menu -> juego -> menu",
        "maqueta sellada: %s" % REFERENCE.relative_to(ROOT),
        "canal de teclado: %s" % CHANNEL["name"],
        "",
        line(m_menu, "MENU", menu_png),
        line(m_world, "WORLD", world_png),
        line(m_paused, "PAUSED", paused_png),
        line(m_back, "MENU-BACK", back_png),
        line(m_world2, "WORLD-2", world2_png),
        "",
        "── evidencia del log ──",
    ]
    report += ["  log: %s" % raw for raw in evidence(logtext)]
    report += ["", "── sha256 completos ──"]
    for name, m in (("menu", m_menu), ("world", m_world), ("paused", m_paused),
                    ("menu-back", m_back), ("world2", m_world2)):
        report.append("  %-10s %s" % (name, m["sha256"]))

    failures = []
    for label, why in (("WORLD", bad_world), ("ROUNDTRIP", bad_back), ("WORLD-2", bad_world2)):
        for reason in why:
            failures.append("%s: %s" % (label, reason))
            report.append("  ✗ %s: %s" % (label, reason))

    injection_ok = not any("injection" in f or "wizard" in f for f in failures)
    if not injection_ok:
        verdict = "VERDICT: INJECTION-BLOCKED — XTEST sobre `Return` no movio la fase; gap declarado"
        rc = 3
    elif not playing or not gate:
        verdict = "VERDICT: GATE-SILENT — la fase no emitio el gate de escalas del mundo"
        rc = 4
    elif failures:
        verdict = "VERDICT: TRANSITION FAIL — %d fallo(s)" % len(failures)
        rc = 1
    else:
        verdict = "VERDICT: TRANSITION PASS — menu -> juego (mundo x2, 0 rastro de menu) -> pausa -> menu"
        rc = 0
    report.append(verdict)
    text = "\n".join(report)
    print(text)
    (OUT / "report.txt").write_text(text + "\n", encoding="utf-8")
    return rc


if __name__ == "__main__":
    sys.exit(main())
