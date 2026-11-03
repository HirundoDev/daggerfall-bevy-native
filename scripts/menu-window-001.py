#!/usr/bin/env python3
"""MENU-WINDOW-001 — the main menu IS PAINTED in the real window (CR-625ff0ea).

Why it exists
--------------
`mplay-001.py` P2 gave **PASS** by measuring `items-surface=main-menu` (shell data
structure) while the window did **not paint** the menu: `main-menu` (index 3)
was `Visible` but its sprite stayed parked at `y=PARKED_Y=-20000.0`, off
camera (defect 2026-10-30, fix `m18_apply_surface`). A probe that believes
itself cannot distinguish "the data says menu" from "the window shows the menu".

This gate measures **pixels of the real window**, captured by its X id with
`import -display <dpy> -window <id>`, and additionally requires the structural
evidence of the product log (active panel at `y=0.0` and the rest parked). A capture that
could not be taken is **never** green: the verdict is `NOT-RUN` with a named cause.

What it measures (stable rows MW1..MW8)
--------------------------------------
        MW1-bin         product binary present and built with `--features window`
                                        (if `crates/**` is newer: build under `flock`; `--skip-build` declares it stale)
        MW2-title       a window titled `SHELL MENU` appears within the deadline (>= 60 s)
        MW3-capture     `import -window <id>` wrote a PNG **of this run** (fresh, decodable,
                                        not smaller than the measured rect) — 0 PNG = 0 measurement
        MW4-pixels      the falsifiable metric over the central rect (640x400): menu art
                                        (`distinct` low + flat rows + non-black centre), NOT a 3D world
        MW5-surface     `M18 VE: surface -> index I on screen at y=0.0 . N panel(s) parked at y=-20000.0`
                                        with `I` on screen (`y == 0.0`), `N == 5` and the parking far away (> 1000 px)
        MW6-panel       `M18 VE: UI surface MENU -> panel "..." (index I, O opaque px) . HUD hidden`
                                        with `O > 0`, `HUD hidden` and the **same** `I` as MW5
        MW7-title-now   the title is still `SHELL MENU` at the instant of the capture
                                        (a later state was not captured, e.g. `SHELL PLAYING`)
        MW8-field-frame the outer frame is the menu's FIELD, not world: 8 edge strips
                                        (4 corners + 4 edge spans) `distinct == 1` and colour `PAL[221]`
                                        = `(23,23,23)` — the flat field of the full-bleed menu

MW4 metric (falsifiable, A/B measured with THIS same code)
-------------------------------------------------------
Central 640x400 rect of the 1280x720 window (exact centre, `PARKED_Y` off camera),
both columns captured by THIS file (`import -window <id>`) on 2026-10-30:

    | tree         | capture sha256 (16) | bytes  | distinct | flat_rows | nonblack | MW4   |
    |--------------|---------------------|-------:|---------:|----------:|---------:|-------|
    | PRE  52575e8 | ec9df6235a0280b4    | 726820 |     1887 |         0 |    1.000 | FAIL  |
    | POST 970bdb3 | 4d90434e17112bd8    | 600236 |      387 |       172 |    1.000 | PASS  |

The two PNGs of the A/B are **the same artifacts** the product owner measured
(PRE 726820 B / `ec9df623...`, POST 600236 B / `4d90434e...`): the capture is deterministic
byte for byte, so the A/B can be repeated.

The criterion is a **set** (all three at once): `distinct <= DISTINCT_MAX` (flat
panel art, not world texture) `and flat_rows >= FLAT_ROWS_MIN` (identical
consecutive rows: the large planes of the retail panel) `and nonblack >=
NONBLACK_MIN` (the centre is not a black screen). The first two are the ones that
**separate** PRE from POST (`1887 > 700` and `0 < 60`); `nonblack` separates nothing (both
1.000) and its role is to falsify the third failure mode: the window that exists and paints
nothing. The thresholds are declared constants and overridable (`--distinct-max`,
`--flat-rows-min`, `--nonblack-min`) so the A/B reproduces without editing the file.
fichero.

MW8 metric (outer frame = menu field) — REDEFINED with the full-bleed menu
-------------------------------------------------------------------------------
The modern menu is composed full-bleed over 320x180 scaled x4: there is no outer black
margin. The retail definition ("outside the panel there are no non-black pixels") no longer
distinguishes anything — the menu field, `PAL[221] = (23,23,23)`, IS non-black, and the
black letterbox of the previous x2 menu slipped through as green. MW8 now measures what
really separates "menu field" from "world": the 8 strips of the window edge
must be **the menu's flat field** —`distinct == 1` and colour `PAL[221]`—, which is
what the sealed art leaves in the frame (the 320/160 px retail strips included
plates and fillets, so they are no longer uniform by construction):

    region         box (1280x720 window)           what it is in the full-bleed menu
    -------------  -------------------------------  ----------------------------------
    corner_tl/tr   (0,0,40,40) . (w-40,0,w,40)      field, outside the lintel
    corner_bl/br   (0,h-40,40,h) . (w-40,h-40,w,h)  field, outside the foot
    frame_top      (w/2-40,0,w/2+40,20)             field between sign and right run
    frame_bottom   (w/2-40,h-20,w/2+40,h)           field under the foot
    frame_left     (0,h/2-20,20,h/2+20)             field left of the plate
    frame_right    (w-20,h/2-20,w,h/2+20)           field right of the card

Criterion (declared constants, overridable with `--frame-*`): each of the 8
strips `distinct <= FRAME_DISTINCT_MAX (1)` **and** modal colour `== PAL[221]`. The two
conditions are NOT redundant: a plane of a SINGLE colour that is not the field (sky,
fog, world water) satisfies `distinct == 1` and only the colour equality catches it
— falsifier `margins-flat-not-field`; a world texture falls by both paths —
falsifier `world-in-margins` (which leaves MW4 at PASS: the centre is untouched).

Declared scope: MW8 samples the frame, it does not compare it pixel by pixel against the
mock-up; TOTAL screen equality belongs to `scripts/menu-selection-capture.py`
(static diff = 0 against the sealed mock-up). MW8 is the cheap probe that needs no
reference and that MW4 did not cover.

Historical A/B (1280x720 captures of the retail defect, with the previous definition):

    | capture         | sha256 (16)  | world-outside-center  | strip_left/right                    | MW4  | MW8  |
    |-----------------|--------------|-----------------------|-------------------------------------|------|------|
    | PRE  970bdb3    | 4d90434e1711 | 665600 px (100.0000 %) | distinct 894 / 1029 . nonblack 1.0000 | PASS | FAIL |
    | POST HEAD (fix) | (from the run) | 0 px (0.0000 %)     | distinct 1 / 1 . nonblack 0.0000      | PASS | PASS |

That table is the gap: with the menu painted and the world behind, MW4 passes in both and
only MW8 separates. With the full-bleed menu, the row measures the field of the frame.

Anti-Screen (UBIQUITOUS §A-24) — what this gate does NOT see
--------------------------------------------------------
- **It does not see semantic content**: that the logo says "DaggerFall", that the plate says
    the retail title of the plate or that the buttons are ACCEPT/REJECT/CANCEL is not
    checked here (that asks for OCR/template-matching, another lane). It measures that **there is a UI
    panel painted** in the centre, not what it says.
- **It does not see interaction**: it presses no keys; it measures the startup `SHELL MENU`
    state. Menu navigation belongs to `shell-002.py` / `mplay-001.py`.
- **It does not see the compositor**: `import -window <id>` asks the X server for the
    content. An occluded window on a desktop without Composite could return stale content;
    the gate raises the window (`_NET_ACTIVE_WINDOW`, best-effort) and declares the result
    of the raise in `ENV` (`raise=1|0`), but it cannot guarantee it.
- **It does not see fps or smoothness**: a single static capture; 1 painted frame is enough.
- **It does not see who presses the keys**: if another lane (or the owner) writes to the same
    `DISPLAY`, a synthetic `Enter` can advance `MENU -> CREATE` while the measurement runs. That
    is why the window is NOT raised to focus by default (`--raise` asks for it explicitly) and
    MW7 reads the title before **and** after the capture: a transition mid-measurement
    gives `FAIL` with the offending title, never a green over another state.
- **It does not see other resolutions**: it requires the window to be >= 640x400 and centres the rect;
    a resolution change does not break the measurement, a **composition** change does
    (the thresholds are recalibrated with the documented A/B).

Usage
---
        python3 scripts/menu-window-001.py                      # real run (build + window + capture)
        python3 scripts/menu-window-001.py --skip-build         # does not build (exit 3 if the binary is stale)
        python3 scripts/menu-window-001.py --binary /path/bin --skip-build   # negative control (PRE)
        python3 scripts/menu-window-001.py --self-test          # falsifiers with synthetic PNGs (no X)
        python3 scripts/menu-window-001.py --falsify world-injected parked-active blank-screen
        python3 scripts/menu-window-001.py --falsify world-in-margins margins-flat-not-field
        python3 scripts/menu-window-001.py --list
        python3 scripts/menu-window-001.py --json --keep /tmp/menu-window-keep

Exit: 0 PASS . 1 FAIL . 2 NC (build/binary/data) . 3 NOT-RUN (no DISPLAY, no
capture possible, stale binary with `--skip-build`).

Negative control reproduced (PRE) — the defect tree, without touching this HEAD:

        git worktree add /tmp/menu-prefix 52575e8
        (cd /tmp/menu-prefix && CARGO_TARGET_DIR=/var/tmp/menu-prefix-target \
                flock .sintergic/locks/cargo-build.lock cargo build -p daggerfall-app --features window)
        python3 scripts/menu-window-001.py --binary /var/tmp/menu-prefix-target/debug/daggerfall-app \
                --skip-build --out /tmp/menu-window-pre

The product fix is already signed off by another lane: this instrument does **not** touch
`crates/**`, does not push, does not tag and does not write retail bytes.
"""

from __future__ import annotations

import argparse
import copy
import fcntl
import hashlib
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
import time
from collections import Counter
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
GATE = "MENU-WINDOW-001"
CR = "CR-625ff0ea-86e9-41b3-a56f-78f2d61fe8d7"
BIN = ROOT / "target" / "debug" / "daggerfall-app"
BUILD_LOCK = ROOT / ".sintergic" / "locks" / "cargo-build.lock"
DEFAULT_OUT = ROOT / ".sintergic" / "artifacts" / "menu-window-001"
DEFAULT_DATA_ROOT = Path(
    os.environ.get("DF_APP_ROOT")
    or Path(os.environ.get("HOME", "")) / "Games" / "Daggerfall" / "app"
)

# ── el objeto medido ──────────────────────────────────────────────────────
TITLE_TOKEN = "SHELL MENU"          # etiqueta de la fase MENU en el título de ventana
DEFAULT_RECT = (640, 400)           # rect central del A/B (ventana 1280x720)
DEFAULT_TITLE_DEADLINE = 90.0       # tope para ver `SHELL MENU` (medido: 21.4 s)
DEFAULT_SETTLE = 2.0                # margen de pintado tras el título, antes de capturar
PARKED_EXPECTED = 5                 # paneles aparcados en la composición MENU
PARK_DISTANCE_MIN = 1000.0          # «aparcado» = fuera de cámara, no `y=0`

# MW4 thresholds (A/B measured with `measure_image` of THIS file; see docstring).
DISTINCT_MAX = 700                  # POST 387 · PRE 1887
FLAT_ROWS_MIN = 60                  # POST 172/399 · PRE (ver HH)
NONBLACK_MIN = 0.50                 # POST 1.000 · pantalla negra 0.000

# -- MW8: outer frame = menu FIELD (redefined with the full-bleed menu) --
# The modern menu is composed full-bleed: the frame is NO LONGER black, it is its flat field
# `PAL[221]`. Three definitions coexisted in the retail tree; here there is only one, the
# one the sealed art satisfies in the 8 strips `_frame_metrics` measures.
MENU_FIELD_RGB = (23, 23, 23)       # PAL[221] — campo del menú a sangre (misma que render.py)
FRAME_CORNER_PX = 40                # esquina: cuadrada, fuera del lintel y del pie
FRAME_EDGE_LEN = 80                 # tramo de borde (centrado en cada lado)
FRAME_EDGE_THICK = 20               # grosor del tramo (5 px de canvas x4)
MARGIN_FLAT_RGB = (40, 32, 40)      # fixture: franja de UN color plano que NO es el campo

# MW8 thresholds (declared constants, overridable with `--frame-*`). No
# buried number. The two conditions are not redundant: `distinct == 1` catches the
# textured world, colour equality catches a uniform plane that is not the field.
FRAME_DISTINCT_MAX = 1              # un solo color por franja...
FRAME_OFF_FIELD_MAX = 0             # ...y ese color tiene que ser PAL[221] (0 franjas fuera)

FRAME_REGIONS = (
    "corner_tl", "corner_tr", "corner_bl", "corner_br",
    "frame_top", "frame_bottom", "frame_left", "frame_right",
)

# -- product evidence contract (verbatim from `m18_window.rs`) --
RE_SURFACE = re.compile(
    r"^M18 VE: surface → index (\d+) on screen at y=(-?[\d.]+) · (\d+) panel\(s\) parked at y=(-?[\d.]+)$"
)
RE_PANEL = re.compile(
    r'^M18 VE: UI surface (\w+) → panel "([^"]*)" \(index (\d+), (\d+) opaque px\) · HUD (\w+)$'
)
RE_PANEL_NO_SPRITE = re.compile(r'^M18 VE: UI surface (\w+) — panel "[^"]*" has no sprite')

ROW_IDS = (
    "MW1-bin",
    "MW2-title",
    "MW3-capture",
    "MW4-pixels",
    "MW5-surface",
    "MW6-panel",
    "MW7-title-now",
    "MW8-world-margins",
)
ROW_INTENT = {
    "MW1-bin": "binario de producto con `--features window`",
    "MW2-title": "ventana titulada `SHELL MENU` dentro del plazo",
    "MW3-capture": "PNG de ESTA corrida capturado por X id",
    "MW4-pixels": "píxeles del rect central = arte de menú (no mundo 3D, no negro)",
    "MW5-surface": "log: panel activo en y=0.0 y 5 aparcados",
    "MW6-panel": "log: panel con sprite (opaque>0) y HUD oculto",
    "MW7-title-now": "título `SHELL MENU` en el instante de la captura",
    "MW8-world-margins": "marco exterior = campo PAL[221] plano (8 franjas), no mundo",
}

ANTIPANTALLA = (
    "Anti-Pantalla: no se comprueba el contenido (logo/placa/botones por OCR) — solo que hay panel pintado",
    "Anti-Pantalla: no se pulsa ninguna tecla — mide el MENU de arranque, no la navegación",
    "Anti-Pantalla: `import -window` pide el contenido al servidor X; ventana ocluida sin Composite puede mentir",
    "Anti-Pantalla: 1 captura estática — no mide fps, fluidez ni animación",
    "Anti-Pantalla: otra lane escribiendo teclas en el mismo DISPLAY puede avanzar MENU→CREATE; MW7 rechaza esa medida",
    "Anti-Pantalla: resolución distinta de 1280x720 se acepta si es >= rect, pero no se recalibra el umbral sola",
)


class Measurement(Exception):
    """MEASUREMENT NOT POSSIBLE: there is no green verdict without this datum."""


# ── utilidades ────────────────────────────────────────────────────────────
def short(text, limit: int = 64) -> str:
    """Truncated window title: the real one carries HUD and scene and does not fit the row."""
    if text is None:
        return "-"
    return text if len(text) <= limit else text[: limit - 1] + "…"


def sha256_bytes(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def sha256_file(path: Path) -> str:
    try:
        return sha256_bytes(path.read_bytes())
    except OSError:
        return "-"


def head_commit() -> str:
    try:
        return subprocess.check_output(
            ["git", "-C", str(ROOT), "rev-parse", "HEAD"], text=True, stderr=subprocess.DEVNULL
        ).strip()
    except Exception:
        return "unknown"


def tree_dirty() -> int:
    """Foreign writes under `crates/` (provisional measurement, does not break it)."""
    try:
        out = subprocess.check_output(
            ["git", "-C", str(ROOT), "status", "--porcelain", "--", "crates"],
            text=True,
            stderr=subprocess.DEVNULL,
        )
    except Exception:
        return 0
    return len([ln for ln in out.splitlines() if ln.strip()])


def newest_source_mtime() -> float:
    newest = 0.0
    for pattern in ("crates/**/*.rs", "crates/**/Cargo.toml", "crates/*/Cargo.toml"):
        for path in ROOT.glob(pattern):
            if path.is_file():
                newest = max(newest, path.stat().st_mtime)
    return newest


def emit(lines: list[str], out_path: Path | None) -> None:
    """Prints, seals with DIGEST and archives (the DIGEST covers everything above)."""
    text = "\n".join(lines) + "\n"
    sealed = text + "DIGEST sha256:%s\n" % hashlib.sha256(text.encode("utf-8")).hexdigest()
    if out_path is not None:
        out_path.parent.mkdir(parents=True, exist_ok=True)
        out_path.write_text(sealed, encoding="utf-8")
    sys.stdout.write(sealed)


# ── build ─────────────────────────────────────────────────────────────────
def build(binary: Path) -> tuple[int, str]:
    """`cargo build -p daggerfall-app --features window` bajo `flock` (G6)."""
    BUILD_LOCK.parent.mkdir(parents=True, exist_ok=True)
    with open(BUILD_LOCK, "w", encoding="utf-8") as lock:
        fcntl.flock(lock.fileno(), fcntl.LOCK_EX)
        try:
            done = subprocess.run(
                ["cargo", "build", "-p", "daggerfall-app", "--features", "window"],
                cwd=str(ROOT),
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                text=True,
            )
        except OSError as exc:
            return 126, "cargo no ejecutable: %s" % exc
    if done.returncode != 0:
        tail = (done.stderr or done.stdout).strip().splitlines()
        return done.returncode, tail[-1] if tail else "cargo build failed"
    return 0, ""


# ── X11: ventana propia y captura ─────────────────────────────────────────
def _x11():
    from Xlib import X, display  # import local: --list/--self-test no necesitan X

    return X, display


NET_ACTIVE_WINDOW = "_NET_ACTIVE_WINDOW"
NET_WM_NAME = "_NET_WM_NAME"
UTF8 = "UTF8_STRING"


def name_of(d, win) -> str:
    for prop, typ in ((NET_WM_NAME, UTF8), ("WM_NAME", "STRING")):
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
        name = name_of(d, child)
        if name:
            acc.append((child, name))
        walk(d, child, depth + 1, acc)
    return acc


def window_owner_pid(d, win):
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


def find_window(d, proc, deadline: float):
    """Own window of the process whose title carries the requested label.

    Priority `_NET_WM_PID` == child pid; if the toolkit does not publish the PID, the
    `DAGGERFALL`/`SHELL ...` title is enough (same rule as `shell-001.py`).
    """
    fallback = None
    while True:
        for win, name in walk(d, d.screen().root):
            if TITLE_TOKEN not in name.upper():
                continue
            if window_owner_pid(d, win) == proc.pid:
                return win, name
            if "DAGGERFALL" in name.upper() and fallback is None:
                fallback = (win, name)
        if fallback is not None:
            owner = window_owner_pid(d, fallback[0])
            if owner is None or owner == proc.pid:
                return fallback
        if proc.poll() is not None:
            return None, ""
        if time.monotonic() >= deadline:
            return None, ""
        time.sleep(0.2)


def raise_window(d, win, X) -> int:
    """`_NET_ACTIVE_WINDOW` + `set_input_focus` (best-effort: se declara el resultado)."""
    from Xlib.protocol import event as xevent

    ok = 0
    try:
        d.screen().root.send_event(
            xevent.ClientMessage(
                window=win,
                client_type=d.intern_atom(NET_ACTIVE_WINDOW),
                data=(32, [2, X.CurrentTime, 0, 0, 0]),
            ),
            event_mask=X.SubstructureRedirectMask | X.SubstructureNotifyMask,
        )
        d.sync()
        ok = 1
    except Exception:
        ok = 0
    try:
        win.set_input_focus(X.RevertToParent, X.CurrentTime)
        d.sync()
    except Exception:
        pass
    return ok


def capture_window(display: str, win, png: Path) -> tuple[int, str]:
    """`import -display <dpy> -window <id>` (ImageMagick, patrón del repo)."""
    tool = shutil.which("import")
    if tool is None:
        raise Measurement("capture-tool=absent (falta ImageMagick `import`)")
    if png.exists():  # nunca medir un PNG heredado de otra corrida
        png.unlink()
    done = subprocess.run(
        [tool, "-display", display, "-window", hex(win.id), str(png)],
        capture_output=True,
        text=True,
    )
    return done.returncode, (done.stderr or done.stdout).strip()[:200]


# ── MW4: la métrica falsable ──────────────────────────────────────────────
def measure_image(path: Path, rect: tuple[int, int] = DEFAULT_RECT) -> dict:
    """Measures the central rect: distinct, flat rows, non-black centre.

    Everything comes from the PNG (not the probe JSON): a textured 3D world gives thousands of
    colours and no repeated row; retail panel art gives hundreds of colours
    and tens of identical rows. It also returns `margins` (MW8): the central rect is
    only 27.8 % of the window and the rest is measurable too.
    """
    try:
        from PIL import Image
    except Exception as exc:  # pragma: no cover - dependencia de entorno
        raise Measurement("pillow=absent (%s)" % exc)
    try:
        image = Image.open(path)
        image.load()
    except Exception as exc:
        raise Measurement("png-undecodable (bytes=%d): %s" % (path.stat().st_size, exc))
    rgb = image.convert("RGB")
    width, height = rgb.size
    cw, ch = rect
    if width < cw or height < ch:
        raise Measurement("window-too-small %dx%d < rect %dx%d" % (width, height, cw, ch))
    left = (width - cw) // 2
    top = (height - ch) // 2
    buf = rgb.crop((left, top, left + cw, top + ch)).tobytes()
    stride = cw * 3
    counts = Counter(buf[i : i + 3] for i in range(0, len(buf), 3))
    total = cw * ch
    modal_px, modal_count = counts.most_common(1)[0]
    nonblack = sum(n for pixel, n in counts.items() if pixel != b"\x00\x00\x00") / float(total)
    flat_rows = sum(
        1 for y in range(1, ch) if buf[y * stride : (y + 1) * stride] == buf[(y - 1) * stride : y * stride]
    )
    lum = {p: 0.299 * p[0] + 0.587 * p[1] + 0.114 * p[2] for p in counts}
    mean = sum(lum[p] * n for p, n in counts.items()) / total
    stdev = (sum(((lum[p] - mean) ** 2) * n for p, n in counts.items()) / total) ** 0.5
    return {
        "rect": [cw, ch],
        "image": [width, height],
        "distinct": len(counts),
        "flat_rows": flat_rows,
        "rows": ch - 1,
        "nonblack": round(nonblack, 4),
        "modal": list(modal_px),
        "modal_share": round(modal_count / float(total), 4),
        "lmean": round(mean, 2),
        "lstdev": round(stdev, 2),
        "frames": _frame_metrics(rgb),
    }


def pixel_verdict(metrics: dict, distinct_max: int, flat_rows_min: int, nonblack_min: float) -> tuple[str, str]:
    """MW4 decision — three simultaneous conditions, with the red cause named."""
    reasons = []
    if metrics["distinct"] > distinct_max:
        reasons.append("distinct=%d>%d" % (metrics["distinct"], distinct_max))
    if metrics["flat_rows"] < flat_rows_min:
        reasons.append("flat_rows=%d<%d" % (metrics["flat_rows"], flat_rows_min))
    if metrics["nonblack"] < nonblack_min:
        reasons.append("nonblack=%.3f<%.3f" % (metrics["nonblack"], nonblack_min))
    detail = "distinct=%d<=%d flat_rows=%d/%d>=%d nonblack=%.3f>=%.3f rect=%dx%d" % (
        metrics["distinct"], distinct_max, metrics["flat_rows"], metrics["rows"], flat_rows_min,
        metrics["nonblack"], nonblack_min, metrics["rect"][0], metrics["rect"][1],
    )
    if reasons:
        return "FAIL", detail + " cause=" + ",".join(reasons)
    return "PASS", detail


# -- MW8: outer frame = menu field (the SAME capture, no second launch) --
def _frame_metrics(rgb) -> dict:
    """The 8 edge strips: `distinct` and whether they are the flat field `PAL[221]`.

    The full-bleed menu turns the frame into its own field (23,23,23); the world, on the
    other hand, is texture (thousands of colours) or, at most, a plane of ANOTHER colour. It
    measures `distinct` and equality with the field, with no sealed reference in hand.
    """
    width, height = rgb.size
    mid_x, mid_y = width // 2, height // 2
    half_len = FRAME_EDGE_LEN // 2
    half_thick = FRAME_EDGE_THICK // 2
    boxes = {
        "corner_tl": (0, 0, FRAME_CORNER_PX, FRAME_CORNER_PX),
        "corner_tr": (width - FRAME_CORNER_PX, 0, width, FRAME_CORNER_PX),
        "corner_bl": (0, height - FRAME_CORNER_PX, FRAME_CORNER_PX, height),
        "corner_br": (width - FRAME_CORNER_PX, height - FRAME_CORNER_PX, width, height),
        "frame_top": (mid_x - half_len, 0, mid_x + half_len, FRAME_EDGE_THICK),
        "frame_bottom": (mid_x - half_len, height - FRAME_EDGE_THICK, mid_x + half_len, height),
        "frame_left": (0, mid_y - half_thick, FRAME_EDGE_THICK, mid_y + half_thick),
        "frame_right": (width - FRAME_EDGE_THICK, mid_y - half_thick, width, mid_y + half_thick),
    }
    regions = {}
    for name, box in boxes.items():
        crop = rgb.crop(box)
        raw = crop.tobytes()
        total = crop.size[0] * crop.size[1]
        counts = Counter(raw[at : at + 3] for at in range(0, len(raw), 3))
        pixel, count = counts.most_common(1)[0]
        color = tuple(pixel)
        regions[name] = {
            "box": list(box),
            "px": total,
            "distinct": len(counts),
            "field": color == MENU_FIELD_RGB,
            "top": {"rgb": list(color), "frac": round(count / float(total), 4) if total else 0.0},
        }
    off_field = [name for name in FRAME_REGIONS if not regions[name]["field"]]
    return {
        "field_rgb": list(MENU_FIELD_RGB),
        "regions": regions,
        "off_field": off_field,
    }


def frame_verdict(frames: dict, distinct_max: int, off_field_max: int) -> tuple[str, str]:
    """MW8 decision — the 8 strips are the menu's flat field, not world.

    Two conditions per strip, and none is redundant: `distinct <= 1` catches the
    world texture (thousands of colours) and equality with `PAL[221]` catches the uniform
    plane that is NOT the field (sky, fog, water, the x2 menu's black letterbox).
    """
    reasons = []
    for name in FRAME_REGIONS:
        region = frames["regions"][name]
        if region["distinct"] > distinct_max:
            reasons.append("%s distinct=%d>%d" % (name, region["distinct"], distinct_max))
        if not region["field"]:
            reasons.append("%s rgb=%s!=PAL221" % (name, tuple(region["top"]["rgb"])))
    off = frames["off_field"]
    if len(off) > off_field_max:
        reasons.append("off-field=%d>%d (%s)" % (len(off), off_field_max, ",".join(off)))
    strips = " ".join(
        "%s:d%d/%s" % (
            name, frames["regions"][name]["distinct"], "campo" if frames["regions"][name]["field"] else "NO-campo",
        )
        for name in FRAME_REGIONS
    )
    detail = "campo=PAL221%s franjas=%d off=%d %s" % (
        tuple(frames["field_rgb"]), len(FRAME_REGIONS), len(off), strips,
    )
    if reasons:
        return "FAIL", detail + " cause=" + ",".join(reasons)
    return "PASS", detail


# ── MW5/MW6: evidencia estructural del log ────────────────────────────────
def parse_surface(log_text: str) -> list[tuple[int, float, int, float]]:
    out = []
    for line in log_text.splitlines():
        match = RE_SURFACE.match(line.strip())
        if match:
            out.append((int(match.group(1)), float(match.group(2)), int(match.group(3)), float(match.group(4))))
    return out


def parse_panel(log_text: str) -> list[tuple[str, str, int, int, str]]:
    out = []
    for line in log_text.splitlines():
        match = RE_PANEL.match(line.strip())
        if match:
            out.append(
                (match.group(1), match.group(2), int(match.group(3)), int(match.group(4)), match.group(5))
            )
    return out


# ── el juez: puro, sin X ni disco (lo usan run, --self-test y --falsify) ───
def evaluate(obs: dict, limits: dict) -> dict[str, tuple[str, str]]:
    rows: dict[str, tuple[str, str]] = {}
    binary = obs["bin"]
    title = obs["title"]
    capture = obs["capture"]
    log_text = obs.get("log", "")

    # MW1 — binario de producto
    if not binary["exists"]:
        rows["MW1-bin"] = ("FAIL", "bin=absent path=%s" % binary["path"])
    elif binary["fresh"] == "stale":
        rows["MW1-bin"] = ("FAIL", "bin=stale (crates/** mas nuevo que el binario)")
    else:
        rows["MW1-bin"] = (
            "PASS",
            "bin=%s sha256=%s fresh=%s" % (binary["label"], binary["sha256"][:16], binary["fresh"]),
        )

    # MW2 — window with the MENU phase title
    if title["seen_at"] is None:
        rows["MW2-title"] = (
            "FAIL",
            "title=%r absent within deadline=%.0fs last=%r"
            % (TITLE_TOKEN, title["deadline"], title["last_title"] or "-"),
        )
    else:
        rows["MW2-title"] = (
            "PASS",
            'title=%r at=%.1fs deadline=%.0fs' % (TITLE_TOKEN, title["seen_at"], title["deadline"]),
        )

    # MW3 — real capture of this run (fresh: an inherited PNG measures nothing)
    if not capture["ok"]:
        rows["MW3-capture"] = ("FAIL", "capture=%s" % capture["reason"])
    elif not capture.get("fresh"):
        rows["MW3-capture"] = ("FAIL", "png-stale (heredado de otra corrida) bytes=%d" % capture["bytes"])
    else:
        rows["MW3-capture"] = (
            "PASS",
            "png=%s bytes=%d size=%dx%d fresh=%d import-rc=%d"
            % (
                capture["png_name"],
                capture["bytes"],
                capture["image"][0],
                capture["image"][1],
                int(capture["fresh"]),
                capture["rc"],
            ),
        )

    # MW4 — píxeles
    if not capture["ok"]:
        rows["MW4-pixels"] = ("FAIL", "unmeasured (0 captura: 0 píxeles)")
    elif not capture.get("metrics"):
        rows["MW4-pixels"] = ("FAIL", "unmeasured (metrics=absent cause=%s)" % capture.get("reason", "-"))
    else:
        rows["MW4-pixels"] = pixel_verdict(
            capture["metrics"], limits["distinct_max"], limits["flat_rows_min"], limits["nonblack_min"]
        )

    # MW5 — the active panel is ON screen, not parked
    surfaces = parse_surface(log_text)
    panels = parse_panel(log_text)
    if not surfaces:
        rows["MW5-surface"] = ("FAIL", "surface-line=absent (log lines=%d)" % len(log_text.splitlines()))
    else:
        index, active_y, parked, park_y = surfaces[-1]
        reasons = []
        if abs(active_y) > 0.5:
            reasons.append("active-y=%.1f!=0.0" % active_y)
        if parked != PARKED_EXPECTED:
            reasons.append("parked=%d!=%d" % (parked, PARKED_EXPECTED))
        if park_y > -PARK_DISTANCE_MIN:
            reasons.append("park-y=%.1f not-offscreen" % park_y)
        if panels and panels[-1][2] != index:
            reasons.append("index=%d!=panel-index=%d" % (index, panels[-1][2]))
        detail = "surface-lines=%d active-y=%.1f parked=%d parked-y=%.1f index=%d" % (
            len(surfaces), active_y, parked, park_y, index,
        )
        rows["MW5-surface"] = ("FAIL", detail + " cause=" + ",".join(reasons)) if reasons else ("PASS", detail)

    # MW6 — the MENU panel has a sprite (it is not data without an image) and the HUD is hidden
    if not panels:
        no_sprite = RE_PANEL_NO_SPRITE.search(log_text)
        rows["MW6-panel"] = (
            "FAIL",
            "panel-line=absent (no-sprite=%d)" % (1 if no_sprite else 0),
        )
    else:
        phase, panel_id, index, opaque, hud = panels[-1]
        reasons = []
        if phase != "MENU":
            reasons.append("phase=%s!=MENU" % phase)
        if opaque <= 0:
            reasons.append("opaque=%d<=0" % opaque)
        if hud != "hidden":
            reasons.append("hud=%s!=hidden" % hud)
        if not panel_id:
            reasons.append("panel-id=empty")
        detail = 'panel="%s" index=%d opaque=%d hud=%s' % (panel_id, index, opaque, hud)
        rows["MW6-panel"] = ("FAIL", detail + " cause=" + ",".join(reasons)) if reasons else ("PASS", detail)

    # MW7 — the capture is of the MENU state (the title did not advance before or after the `import`)
    before = title.get("at_capture")
    after = title.get("at_capture_after")
    if before is None or after is None:
        rows["MW7-title-now"] = ("FAIL", "title-at-capture=unread (before=%s after=%s)"
                                 % (short(before), short(after)))
    elif TITLE_TOKEN not in before.upper() or TITLE_TOKEN not in after.upper():
        rows["MW7-title-now"] = (
            "FAIL",
            "title-at-capture before=%r after=%r (captura de otro estado)" % (short(before), short(after)),
        )
    else:
        rows["MW7-title-now"] = ("PASS", "title-at-capture=%r before/after" % short(before))

    # MW8 — frame: with the full-bleed menu the outside is not black, it is the menu field.
    # This row measures what MW4 (central rect only) cannot see: frame = field.
    if not capture["ok"]:
        rows["MW8-world-margins"] = ("FAIL", "unmeasured (0 captura: sin marco que medir)")
    elif not capture.get("metrics"):
        rows["MW8-world-margins"] = ("FAIL", "unmeasured (metrics=absent cause=%s)" % capture.get("reason", "-"))
    elif not capture["metrics"].get("frames"):
        rows["MW8-world-margins"] = ("FAIL", "unmeasured (el medidor devolvió frames=absent)")
    else:
        rows["MW8-world-margins"] = frame_verdict(
            capture["metrics"]["frames"], limits["frame_distinct_max"],
            limits["frame_off_field_max"],
        )

    return rows


# ── corrida real ──────────────────────────────────────────────────────────
def nin(cause: str, out: Path | None) -> int:
    """NOT-RUN: without a datum there is no verdict (never green without a capture)."""
    lines = ["gate=%s cr=%s mode=run" % (GATE, CR), "CAUSE environment %s" % cause, "VERDICT: NOT-RUN"]
    emit(lines, None if out is None else out / "stdout.txt")
    sys.stderr.write("NOT-RUN: %s\n" % cause)
    return 3


def nc(cause: str, out: Path | None) -> int:
    lines = ["gate=%s cr=%s mode=run" % (GATE, CR), "CAUSE %s" % cause, "VERDICT: NC"]
    emit(lines, None if out is None else out / "stdout.txt")
    sys.stderr.write("NC: %s\n" % cause)
    return 2


def observe(args, out: Path, fresh: str = "yes") -> dict:
    """One run: real window -> title -> capture -> metrics + log."""
    display = args.display or os.environ.get("DISPLAY") or ""
    wayland = os.environ.get("WAYLAND_DISPLAY") or ""
    binary = Path(args.binary).resolve()
    data_root = Path(args.data_root).expanduser().resolve()
    started = time.monotonic()

    X, display_module = _x11()
    d = display_module.Display(display)
    tool = shutil.which("import")

    obs = {
        "env": {
            "display": display or "-",
            "wayland": wayland or "-",
            "capture_tool": tool or "-",
            "raise": 0,
            "data_root": str(data_root),
        },
        "bin": {
            "path": str(binary),
            "label": str(binary.relative_to(ROOT)) if str(binary).startswith(str(ROOT)) else str(binary),
            "exists": binary.is_file(),
            "sha256": sha256_file(binary),
            "fresh": fresh,  # yes | stale | external (binario fuera del árbol: identidad = sha256)
        },
        "title": {"token": TITLE_TOKEN, "seen_at": None, "at_capture": None, "at_capture_after": None,
                  "deadline": args.title_deadline, "last_title": ""},
        "capture": {"ok": False, "reason": "not-attempted", "png_name": "menu.png", "bytes": 0,
                    "image": [0, 0], "fresh": False, "rc": -1, "metrics": None},
        "log": "",
        "app_exit": None,
        "elapsed": 0.0,
    }

    log_path = out / "app.log"
    state_path = out / "shell.state"
    png = out / "menu.png"
    for stale in (log_path, state_path, png):
        if stale.exists():
            stale.unlink()
    state_path.write_text("", encoding="utf-8")

    env = dict(os.environ)
    env["DF_APP_ROOT"] = str(data_root)
    env["DF_SHELL_STATE"] = str(state_path)
    env["DISPLAY"] = display
    with open(log_path, "wb") as sink:
        # Sin `--window`: ese flag selecciona el camino legacy M2 (`main.rs:915`).
        proc = subprocess.Popen([str(binary)], cwd=str(ROOT), env=env, stdout=sink, stderr=subprocess.STDOUT)
        try:
            deadline = time.monotonic() + args.title_deadline
            win, name = find_window(d, proc, deadline)
            obs["title"]["last_title"] = name
            if win is None:
                obs["capture"]["reason"] = "no-window title=%r (app-alive=%d)" % (
                    TITLE_TOKEN, 0 if proc.poll() is not None else 1)
            else:
                obs["title"]["seen_at"] = time.monotonic() - started
                if args.raise_window:
                    obs["env"]["raise"] = raise_window(d, win, X)
                time.sleep(args.settle)
                # MW7 is measured BEFORE capturing: the captured frame is the one of that title.
                obs["title"]["at_capture"] = name_of(d, win)
                rc, err = capture_window(display, win, png)
                # The title is read again after the capture: MW7 requires MENU before AND after,
                # so a transition to PLAYING during the `import` does not pass as good.
                obs["title"]["at_capture_after"] = name_of(d, win)
                obs["capture"]["rc"] = rc
                if rc != 0 or not png.is_file():
                    obs["capture"]["reason"] = "import-rc=%d %s" % (rc, err or "-")
                elif png.stat().st_size == 0:
                    obs["capture"]["reason"] = "png-empty bytes=0"
                else:
                    obs["capture"]["ok"] = True
                    obs["capture"]["bytes"] = png.stat().st_size
                    obs["capture"]["fresh"] = png.stat().st_mtime >= started - 1.0
                    try:
                        obs["capture"]["metrics"] = measure_image(png, tuple(args.rect))
                        obs["capture"]["image"] = obs["capture"]["metrics"]["image"]
                    except Measurement as exc:
                        obs["capture"]["ok"] = False
                        obs["capture"]["reason"] = str(exc)
                    if obs["capture"]["ok"] and not obs["capture"]["fresh"]:
                        obs["capture"]["ok"] = False
                        obs["capture"]["reason"] = "png-stale (heredado de otra corrida)"
        finally:
            if proc.poll() is None:
                proc.terminate()
                try:
                    proc.wait(timeout=5)
                except Exception:
                    proc.kill()
                    proc.wait(timeout=5)
            obs["app_exit"] = proc.poll()

    obs["log"] = log_path.read_text(encoding="utf-8", errors="replace")
    obs["elapsed"] = time.monotonic() - started
    if obs["capture"]["ok"]:
        obs["capture"]["png_name"] = "menu.png"
    return obs


def run(args) -> int:
    display = args.display or os.environ.get("DISPLAY") or ""
    wayland = os.environ.get("WAYLAND_DISPLAY") or ""
    out = Path(args.out).resolve()
    out.mkdir(parents=True, exist_ok=True)

    # 1) environment: without X there is no capture, and without a capture there is no green verdict.
    if not display:
        return nin(
            "display=absent (DISPLAY unset; WAYLAND_DISPLAY=%s -> `import -window` sin servidor X)"
            % (wayland or "unset"),
            out,
        )
    try:
        _x11()
        from Xlib import display as display_module

        display_module.Display(display).close()
    except Exception as exc:
        return nin("display=%s unreachable (%s)" % (display, exc.__class__.__name__), out)
    if shutil.which("import") is None:
        return nin("capture-tool=absent (falta ImageMagick `import`)", out)

    # 2) datos retail
    data_root = Path(args.data_root).expanduser().resolve()
    if not (data_root / "ARENA2").is_dir():
        return nc("data=absent (sin %s/ARENA2)" % data_root, out)

    # 3) binary: build under flock if `crates/**` is newer, unless --skip-build
    binary = Path(args.binary).resolve()
    internal = str(binary).startswith(str(ROOT))
    fresh = "external"
    if not args.skip_build:
        rc, err = build(binary)
        if rc != 0:
            return nc("build=failed rc=%d tail=%r" % (rc, err), out)
        fresh = "yes"
    if not binary.is_file():
        return nc("bin=absent (%s)" % binary, out)
    if internal:
        stale = newest_source_mtime() > binary.stat().st_mtime
        fresh = "stale" if stale else "yes"
        if stale and args.skip_build:
            return nin("bin=stale (crates/** mas nuevo que %s y --skip-build)" % binary.name, out)

    obs = observe(args, out, fresh=fresh)

    lines = [
        "MENU-WINDOW 001 · %s · el menú SE PINTA en la ventana real" % CR,
        "IDENT head=%s tree=crates_%s bin=%s sha256=%s"
        % (head_commit()[:12], "clean" if tree_dirty() == 0 else "dirty",
           obs["bin"]["label"], obs["bin"]["sha256"][:16]),
        "ENV display=%s wayland=%s capture=%s raise=%d data=%s retail=%s"
        % (obs["env"]["display"], obs["env"]["wayland"], obs["env"]["capture_tool"],
           obs["env"]["raise"], obs["env"]["data_root"], "present"),
        "AB REFERENCE post=distinct:387 flat_rows:172 nonblack:1.000 (4d90434e) · "
        "pre=distinct:1887 flat_rows:0 nonblack:1.000 (ec9df623) · "
        "thresholds distinct<=%d flat_rows>=%d nonblack>=%.3f"
        % (args.distinct_max, args.flat_rows_min, args.nonblack_min),
        "AB FRAMES campo=PAL221%s · menú a sangre: el marco NO es negro, es su campo · "
        "thresholds distinct<=%d off-field<=%d franjas=%d"
        % (tuple(MENU_FIELD_RGB), args.frame_distinct_max, args.frame_off_field_max,
           len(FRAME_REGIONS)),
        "CAPTURE app-exit=%s elapsed=%.1fs log-lines=%d"
        % (obs["app_exit"], obs["elapsed"], len(obs["log"].splitlines())),
    ]

    limits = {
        "distinct_max": args.distinct_max,
        "flat_rows_min": args.flat_rows_min,
        "nonblack_min": args.nonblack_min,
        "frame_distinct_max": args.frame_distinct_max,
        "frame_off_field_max": args.frame_off_field_max,
    }
    rows = evaluate(obs, limits)
    for rid in ROW_IDS:
        verdict, detail = rows[rid]
        lines.append("CHECK %s %s · %s" % (rid, verdict, detail))
    green = sum(1 for rid in ROW_IDS if rows[rid][0] == "PASS")
    red = len(ROW_IDS) - green
    lines.append(
        "CHECKS total=%d green=%d red=%d failed=%d" % (len(ROW_IDS), green, red, 0)
    )
    lines.extend(ANTIPANTALLA)
    # An external binary is identified by its sha256: the working tree does not take part in the
    # measurement, so other lanes' writes under `crates/` do not make it provisional.
    if obs["bin"]["fresh"] == "external":
        measurement = "FINAL bin-externo sha256=%s (arbol de trabajo fuera de la medida)" % obs["bin"]["sha256"][:16]
    elif tree_dirty():
        measurement = "PROVISIONAL tree=crates_dirty (escrituras ajenas bajo crates/ durante la medida)"
    else:
        measurement = "FINAL tree=crates_clean"
    lines.append(
        "MEASUREMENT %s (captura=%s)" % (measurement, "menu.png" if obs["capture"]["ok"] else "ausente")
    )

    verdict = "PASS" if green == len(ROW_IDS) and obs["capture"]["ok"] else "FAIL"
    lines.append("VERDICT: %s" % verdict)
    emit(lines, out / "stdout.txt")

    report = {
        "gate": GATE,
        "cr": CR,
        "head": head_commit(),
        "verdict": verdict,
        "limits": limits,
        "rows": {rid: {"verdict": rows[rid][0], "detail": rows[rid][1]} for rid in ROW_IDS},
        "observation": obs,
    }
    (out / "report.json").write_text(json.dumps(report, indent=2, ensure_ascii=False), encoding="utf-8")
    if args.json:
        print("REPORT %s" % (out / "report.json"))
    if args.keep:
        print("SCRATCH kept=%s" % out)
    return 0 if verdict == "PASS" else 1


# ── fixtures sintéticas (--self-test / --falsify: sin X, sin producto) ─────
def _menu_image():
    """Full-bleed menu art: PAL[221] field + large planes and identical rows.

    The background is the menu field (not black): with the full-bleed menu the window's
    frame IS that field, and so the green fixture satisfies MW8 without cheating.
    """
    from PIL import Image, ImageDraw

    image = Image.new("RGB", (1280, 720), MENU_FIELD_RGB)
    draw = ImageDraw.Draw(image)
    left, top = 340, 160
    draw.rectangle([left, top, left + 599, top + 399], fill=(14, 12, 14))
    draw.rectangle([left + 20, top + 16, left + 579, top + 80], fill=(34, 30, 26))
    draw.rectangle([left + 40, top + 96, left + 559, top + 150], fill=(220, 220, 220))
    for row in range(6):
        y = top + 190 + row * 32
        draw.rectangle([left + 40, y, left + 240, y + 22], fill=(160, 150, 130))
        draw.rectangle([left + 300, y, left + 560, y + 22], fill=(96, 90, 80))
    return image


def _world_image():
    """Mundo 3D: textura continua, miles de colores, 0 filas repetidas."""
    from PIL import Image

    image = Image.new("RGB", (1280, 720))
    pixels = image.load()
    state = 0x2545F491
    for y in range(720):
        for x in range(1280):
            state = (1103515245 * state + 12345) & 0x7FFFFFFF
            r = (state >> 16) & 0xFF
            g = (state >> 8) & 0xFF
            b = ((state & 0x3F) + ((x * 7 + y * 3) & 0x3F)) & 0xFF
            pixels[x, y] = (r, g, b)
    return image


def _menu_like_png(path: Path) -> None:
    _menu_image().save(path)


def _world_like_png(path: Path) -> None:
    _world_image().save(path)


def _margins_png(path: Path, fill: str) -> None:
    """Full-bleed menu with the frame replaced: the case MW4 CANNOT see.

    The centre is the EXACT art of `_menu_like_png` (the art lives entirely inside the
    640x400 rect), so MW4 measures the same as in the green fixture and passes: the
    menu is painted. The frame is `fill`: "world" = world texture (what was there
    behind in 970bdb3) or "flat" = a SINGLE flat colour that is NOT the field (sky,
    fog: `distinct == 1` but not `PAL[221]`).
    """
    from PIL import Image

    width, height = 1280, 720
    cw, ch = DEFAULT_RECT
    cx, cy = (width - cw) // 2, (height - ch) // 2
    if fill == "world":
        background = _world_image()
    elif fill == "flat":
        background = Image.new("RGB", (width, height), MARGIN_FLAT_RGB)
    else:  # pragma: no cover - fixture inexistente = error de programación
        raise Measurement("fixture-fill=%r desconocido" % fill)
    background.paste(_menu_image().crop((cx, cy, cx + cw, cy + ch)), (cx, cy))
    background.save(path)


def _blank_png(path: Path) -> None:
    """Black screen: the window exists but paints nothing."""
    from PIL import Image

    Image.new("RGB", (1280, 720), (0, 0, 0)).save(path)


POST_SURFACE_LINE = "M18 VE: surface → index 3 on screen at y=0.0 · 5 panel(s) parked at y=-20000.0"
POST_PANEL_LINE = 'M18 VE: UI surface MENU → panel "main-menu" (index 3, 63878 opaque px) · HUD hidden'


def fixture_observation(tmp: Path, limits: dict) -> dict:
    """Synthetic GREEN observation: measures a panel PNG with the real meter."""
    menu = tmp / "fixture-menu.png"
    _menu_like_png(menu)
    metrics = measure_image(menu, DEFAULT_RECT)
    return {
        "env": {"display": ":0", "wayland": "-", "capture_tool": "fixture", "raise": 1, "data_root": "-"},
        "bin": {"path": "/tmp/bin", "label": "fixture-bin", "exists": True, "sha256": "0" * 64, "fresh": "yes"},
        "title": {"token": TITLE_TOKEN, "seen_at": 21.4, "at_capture": "DAGGERFALL-LIKE — SHELL MENU",
                  "at_capture_after": "DAGGERFALL-LIKE — SHELL MENU",
                  "deadline": DEFAULT_TITLE_DEADLINE, "last_title": "DAGGERFALL-LIKE — SHELL MENU"},
        "capture": {"ok": True, "reason": "-", "png_name": "fixture-menu.png",
                    "bytes": menu.stat().st_size, "image": metrics["image"], "fresh": True, "rc": 0,
                    "metrics": metrics},
        "log": "\n".join(
            [
                "daggerfall-bevy-native F0 — organs online:",
                "M18 VE: surface → index 0 on screen at y=-20000.0 · 5 panel(s) parked at y=-20000.0",
                POST_SURFACE_LINE,
                POST_PANEL_LINE,
            ]
        ),
        "app_exit": None,
        "elapsed": 24.0,
    }


# -- falsifier mutators (same mould as `mplay-001.py`) --
def _inject_png(obs: dict, tmp: Path, maker, name: str) -> None:
    png = tmp / name
    maker(png)
    metrics = measure_image(png, DEFAULT_RECT)  # medido por el MISMO medidor del gate
    obs["capture"].update(
        ok=True, reason="-", png_name=name, bytes=png.stat().st_size,
        image=metrics["image"], fresh=True, rc=0, metrics=metrics,
    )


def _mut_world_injected(obs: dict, tmp: Path, limits: dict) -> str:
    _inject_png(obs, tmp, _world_like_png, "injected-world.png")
    return "PNG de mundo 3D inyectado como si fuera el menú"


def _mut_blank_injected(obs: dict, tmp: Path, limits: dict) -> str:
    _inject_png(obs, tmp, _blank_png, "injected-blank.png")
    return "PNG de pantalla negra inyectado (ventana sin pintar)"


def _rewrite_surface(obs: dict, index: int | None = None, active_y: str | None = None,
                     parked: int | None = None) -> bool:
    lines = obs["log"].splitlines()
    for at in range(len(lines) - 1, -1, -1):
        match = RE_SURFACE.match(lines[at].strip())
        if not match:
            continue
        lines[at] = "M18 VE: surface → index %s on screen at y=%s · %s panel(s) parked at y=%s" % (
            match.group(1) if index is None else index,
            match.group(2) if active_y is None else active_y,
            match.group(3) if parked is None else parked,
            match.group(4),
        )
        obs["log"] = "\n".join(lines)
        return True
    return False


def _mut_parked_active(obs: dict, tmp: Path, limits: dict) -> str:
    _rewrite_surface(obs, active_y="-20000.0")
    return "log con el panel activo en PARKED_Y (Visible pero fuera de cámara)"


def _mut_parked_count(obs: dict, tmp: Path, limits: dict) -> str:
    _rewrite_surface(obs, parked=0)
    return "log sin los 5 paneles aparcados (composición incompleta)"


def _mut_no_surface_line(obs: dict, tmp: Path, limits: dict) -> str:
    obs["log"] = "\n".join(
        [ln for ln in obs["log"].splitlines() if not RE_SURFACE.match(ln.strip())]
    )
    return "log sin la línea `M18 VE: surface →` (sin evidencia de superficie)"


def _mut_no_capture(obs: dict, tmp: Path, limits: dict) -> str:
    obs["capture"].update(ok=False, reason="simulated import-rc=1", metrics=None, bytes=0, image=[0, 0])
    return "captura imposible: nunca verde sin PNG"


def _mut_stale_png(obs: dict, tmp: Path, limits: dict) -> str:
    obs["capture"]["fresh"] = False
    return "PNG heredado de otra corrida (fresco=0)"


def _mut_hud_visible(obs: dict, tmp: Path, limits: dict) -> str:
    obs["log"] = obs["log"].replace("· HUD hidden", "· HUD visible")
    return "log con el HUD visible sobre el menú"


def _mut_opaque_zero(obs: dict, tmp: Path, limits: dict) -> str:
    obs["log"] = re.sub(r"(index \d+, )\d+( opaque px)", r"\g<1>0\g<2>", obs["log"])
    return "log con 0 px opacos: el panel no tiene imagen"


def _mut_title_advanced(obs: dict, tmp: Path, limits: dict) -> str:
    obs["title"]["at_capture"] = "DAGGERFALL-LIKE — SHELL PLAYING"
    obs["title"]["at_capture_after"] = "DAGGERFALL-LIKE — SHELL PLAYING"
    return "título ya avanzado en el instante de la captura"


def _mut_no_title(obs: dict, tmp: Path, limits: dict) -> str:
    obs["title"].update(seen_at=None, at_capture=None, at_capture_after=None,
                        last_title="DAGGERFALL-LIKE — SHELL BOOT")
    return "sin ventana `SHELL MENU` dentro del plazo"


def _mut_bin_absent(obs: dict, tmp: Path, limits: dict) -> str:
    obs["bin"].update(exists=False, fresh="absent")
    return "binario de producto ausente"


def _mut_world_in_margins(obs: dict, tmp: Path, limits: dict) -> str:
    _inject_png(obs, tmp, lambda p: _margins_png(p, "world"), "world-in-margins.png")
    return "PNG con el arte del menú intacto y MUNDO en el marco: MW4 sigue verde, MW8 muerde"


def _mut_margins_flat_not_field(obs: dict, tmp: Path, limits: dict) -> str:
    _inject_png(obs, tmp, lambda p: _margins_png(p, "flat"), "flat-frame.png")
    return ("marco de UN color plano que NO es el campo (distinct=1 pero rgb != PAL[221]): "
            "cielo/niebla del mundo — ROJO, distinct=1 solo no basta")


def _mut_margins_black_panel_absent(obs: dict, tmp: Path, limits: dict) -> str:
    _inject_png(obs, tmp, _blank_png, "frame-black.png")
    obs["log"] = "\n".join([ln for ln in obs["log"].splitlines() if not RE_PANEL.match(ln.strip())])
    return "todo negro y sin línea de panel: el marco NO es el campo (MW8 rojo) y MW4/MW6 rojos"


# nombre -> (fila que DEBE morder, mutador, clase)
FALSIFIERS = {
    "world-injected": ("MW4-pixels", _mut_world_injected, "pixels"),
    "blank-screen": ("MW4-pixels", _mut_blank_injected, "pixels"),
    "parked-active": ("MW5-surface", _mut_parked_active, "log"),
    "parked-count": ("MW5-surface", _mut_parked_count, "log"),
    "no-surface-line": ("MW5-surface", _mut_no_surface_line, "log"),
    "no-capture": ("MW3-capture", _mut_no_capture, "capture"),
    "stale-png": ("MW3-capture", _mut_stale_png, "capture"),
    "hud-visible": ("MW6-panel", _mut_hud_visible, "log"),
    "opaque-zero": ("MW6-panel", _mut_opaque_zero, "log"),
    "title-advanced": ("MW7-title-now", _mut_title_advanced, "title"),
    "no-title": ("MW2-title", _mut_no_title, "title"),
    "bin-absent": ("MW1-bin", _mut_bin_absent, "bin"),
    "world-in-margins": ("MW8-world-margins", _mut_world_in_margins, "frame"),
    "frame-black-panel-absent": ("MW4-pixels", _mut_margins_black_panel_absent, "frame"),
    "margins-flat-not-field": ("MW8-world-margins", _mut_margins_flat_not_field, "frame"),
}

# Guards per falsifier: besides biting ITS row, the mutation must leave the others
# where it says. That is what proves (a) the case "menu painted with the world behind" is not
# a false green of MW4 and (b) MW8 does NOT replace MW4/MW6: a black window falls by
# all three paths (MW4 without art, MW6 without panel, MW8 with a frame that is not the field).
FALSIFIER_GUARDS = {
    "world-in-margins": {"must_pass": ("MW4-pixels",)},
    "frame-black-panel-absent": {"must_fail": ("MW6-panel", "MW8-world-margins")},
    "margins-flat-not-field": {"must_pass": ("MW4-pixels",)},
}


def _guard_violations(name: str, rows: dict) -> list[str]:
    """Rows where the mutation did NOT leave the verdict its guard requires."""
    guard = FALSIFIER_GUARDS.get(name)
    if not guard:
        return []
    bad = []
    for rid in guard.get("must_fail", ()):
        if rows[rid][0] != "FAIL":
            bad.append("must_fail %s=%s" % (rid, rows[rid][0]))
    for rid in guard.get("must_pass", ()):
        if rows[rid][0] != "PASS":
            bad.append("must_pass %s=%s" % (rid, rows[rid][0]))
    return bad


def _guard_text(name: str) -> str:
    guard = FALSIFIER_GUARDS.get(name)
    if not guard:
        return "-"
    parts = ["%s:FAIL" % rid for rid in guard.get("must_fail", ())]
    parts += ["%s:PASS" % rid for rid in guard.get("must_pass", ())]
    return ",".join(parts)


def _limits_of(args) -> dict:
    return {
        "distinct_max": args.distinct_max,
        "flat_rows_min": args.flat_rows_min,
        "nonblack_min": args.nonblack_min,
        "frame_distinct_max": args.frame_distinct_max,
        "frame_off_field_max": args.frame_off_field_max,
    }


# ── modos ─────────────────────────────────────────────────────────────────
def mode_list() -> int:
    limits = {"distinct_max": DISTINCT_MAX, "flat_rows_min": FLAT_ROWS_MIN, "nonblack_min": NONBLACK_MIN}
    lines = [
        "gate=%s cr=%s mode=list rows=%d" % (GATE, CR, len(ROW_IDS)),
        "MEDIDAS rect=%dx%d title=%r deadline=%.0fs parked=%d thresholds distinct<=%d flat_rows>=%d nonblack>=%.3f"
        % (DEFAULT_RECT[0], DEFAULT_RECT[1], TITLE_TOKEN, DEFAULT_TITLE_DEADLINE, PARKED_EXPECTED,
           limits["distinct_max"], limits["flat_rows_min"], limits["nonblack_min"]),
        "AB post=distinct:387 flat_rows:172 nonblack:1.000 (4d90434e 600236B) · "
        "pre=distinct:1887 flat_rows:0 nonblack:1.000 (ec9df623 726820B)",
    ]
    for rid in ROW_IDS:
        lines.append("ROW %s %s" % (rid, ROW_INTENT[rid]))
    lines.append(
        "FRAMES center=%dx%d campo=PAL221%s corners=%dpx edges=%dx%dpx distinct<=%d off-field<=%d"
        % (DEFAULT_RECT[0], DEFAULT_RECT[1], tuple(MENU_FIELD_RGB), FRAME_CORNER_PX,
           FRAME_EDGE_LEN, FRAME_EDGE_THICK, FRAME_DISTINCT_MAX, FRAME_OFF_FIELD_MAX)
    )
    lines.append(
        "AB FRAMES retail(970bdb3) strips:distinct 894..1029 (MW4 PASS/MW8 FAIL) · "
        "a-sangre post: 8 franjas campo PAL221 distinct=1 (MW8 PASS)"
    )
    for name, (row, _mut, kind) in sorted(FALSIFIERS.items()):
        lines.append("FALSIFIER %s row=%s kind=%s guards=%s" % (name, row, kind, _guard_text(name)))
    lines.extend(ANTIPANTALLA)
    emit(lines, None)
    return 0


def run_falsifiers(base_obs: dict, names: list[str], limits: dict, tmp: Path, header: str) -> int:
    """Clean -> mutated -> restored, over the base observation (real or synthetic)."""
    base_rows = evaluate(base_obs, limits)
    print(header)
    failed = 0
    for name in names:
        row, mutator, kind = FALSIFIERS[name]
        clean = base_rows[row]
        target = copy.deepcopy(base_obs)
        description = mutator(target, tmp, limits)
        after_all = evaluate(target, limits)
        after = after_all[row]
        restored = evaluate(copy.deepcopy(base_obs), limits)[row]
        # cascade = rows whose VERDICT changes (only the detail changing is not cascade)
        cascade = sorted(rid for rid in ROW_IDS if rid != row and after_all[rid][0] != base_rows[rid][0])
        guard_bad = _guard_violations(name, after_all)
        ok = after[0] == "FAIL" and restored == clean and clean[0] == "PASS" and not guard_bad
        failed += 0 if ok else 1
        print("FALSIFY %s kind=%s row=%s clean=%s mutated=%s restored=%s guards=%s cascade=%s"
              % (name, kind, row, clean[0], after[0], restored[0], _guard_text(name),
                 ",".join(cascade) or "-"))
        for violation in guard_bad:
            print("FALSIFY   guard-violation=%s" % violation)
        print("FALSIFY   mutant=%s" % description)
        print("FALSIFY   mutated-detail=%s" % after[1])
        print("FALSIFY %s VERDICT: %s" % (name, "PASS" if ok else "FAIL"))
    print("FALSIFY total=%d pass=%d fail=%d" % (len(names), len(names) - failed, failed))
    return 0 if failed == 0 else 1


def mode_self_test(args) -> int:
    """Falsifiers with synthetic PNGs: it needs neither X, nor the product, nor ARENA2."""
    limits = _limits_of(args)
    failures: list[str] = []
    print("MENU-WINDOW 001 · SELF-TEST (fixtures sintéticas; prueban la DECISIÓN, no el producto)")
    tmp = Path(tempfile.mkdtemp(prefix="menu-window-selftest-"))
    try:
        obs = fixture_observation(tmp, limits)
        base = evaluate(obs, limits)
        green = sum(1 for rid in ROW_IDS if base[rid][0] == "PASS")
        for rid in ROW_IDS:
            if base[rid][0] != "PASS":
                failures.append("fixture-clean %s=%s (%s)" % (rid, base[rid][0], base[rid][1]))
        metrics = obs["capture"]["metrics"]
        print("SELFTEST fixture-clean green=%d/%d distinct=%d flat_rows=%d nonblack=%.3f"
              % (green, len(ROW_IDS), metrics["distinct"], metrics["flat_rows"], metrics["nonblack"]))

        # The meter really separates: the same code over a 3D world and over black.
        world = tmp / "probe-world.png"
        _world_like_png(world)
        blank = tmp / "probe-blank.png"
        _blank_png(blank)
        for label, path, want in (("world", world, "FAIL"), ("blank", blank, "FAIL")):
            measured = measure_image(path, DEFAULT_RECT)
            verdict, detail = pixel_verdict(
                measured, limits["distinct_max"], limits["flat_rows_min"], limits["nonblack_min"]
            )
            print("SELFTEST meter %s distinct=%d flat_rows=%d nonblack=%.3f verdict=%s"
                  % (label, measured["distinct"], measured["flat_rows"], measured["nonblack"], verdict))
            if verdict != want:
                failures.append("meter %s verdict=%s != %s (%s)" % (label, verdict, want, detail))

        caught, signatures = 0, set()
        for name in sorted(FALSIFIERS):
            row, mutator, kind = FALSIFIERS[name]
            target = copy.deepcopy(obs)
            description = mutator(target, tmp, limits)
            after_all = evaluate(target, limits)
            # signature = **verdict and detail** of each row: two mutations that break the same
            # row for different causes are different mutations (not a copy of the same falsifier).
            signature = hashlib.sha256(
                json.dumps({rid: list(after_all[rid]) for rid in ROW_IDS}, sort_keys=True).encode()
            ).hexdigest()
            signatures.add(signature)
            guard_bad = _guard_violations(name, after_all)
            ok = (after_all[row][0] == "FAIL" and evaluate(copy.deepcopy(obs), limits) == base
                  and not guard_bad)
            caught += 1 if ok else 0
            print("SELFTEST falsify %s row=%s kind=%s clean=%s mutated=%s restored=%s guards=%s mutant=%s ⇒ %s"
                  % (name, row, kind, base[row][0], after_all[row][0],
                     evaluate(copy.deepcopy(obs), limits)[row][0], _guard_text(name), description,
                     "OK" if ok else "FALLO"))
            if not ok:
                failures.append("falsify %s mutated=%s guards=%s (detail=%s)"
                                % (name, after_all[row][0], "; ".join(guard_bad) or "-",
                                   after_all[row][1]))
        print("SELFTEST rows=%d falsifiers=%d caught=%d distinct_mutations=%d"
              % (len(ROW_IDS), len(FALSIFIERS), caught, len(signatures)))
        if caught < 3:
            failures.append("caught=%d<3 (menos de 3 falsificadores muerden)" % caught)
        if len(signatures) < len(FALSIFIERS):
            failures.append("distinct_mutations=%d<%d" % (len(signatures), len(FALSIFIERS)))
    finally:
        shutil.rmtree(tmp, ignore_errors=True)

    for failure in failures:
        print("FAIL SELFTEST: %s" % failure)
    print("SELFTEST VERDICT: %s" % ("PASS" if not failures else "FAIL"))
    return 0 if not failures else 1


def mode_falsify(args) -> int:
    """Falsifiers over a REAL run: mutates the observation already measured."""
    limits = _limits_of(args)
    display = args.display or os.environ.get("DISPLAY") or ""
    if not display:
        sys.stderr.write("NOT-RUN: display=absent (los falsificadores reales exigen una captura)\n")
        print("VERDICT: NOT-RUN")
        return 3
    binary = Path(args.binary).resolve()
    if not binary.is_file():
        sys.stderr.write("NC: bin=absent (%s)\n" % binary)
        print("VERDICT: NC")
        return 2
    out = Path(args.out).resolve()
    out.mkdir(parents=True, exist_ok=True)
    obs = observe(args, out)
    tmp = Path(tempfile.mkdtemp(prefix="menu-window-falsify-"))
    try:
        header = "MENU-WINDOW 001 · FALSIFY %s · captura=%s head=%s" % (
            ",".join(args.falsify), "menu.png" if obs["capture"]["ok"] else "ausente", head_commit()[:12])
        return run_falsifiers(obs, args.falsify, limits, tmp, header)
    finally:
        shutil.rmtree(tmp, ignore_errors=True)


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(
        description="MENU-WINDOW-001: el menú principal se PINTA en la ventana real (%s)" % CR
    )
    parser.add_argument("--binary", default=str(BIN), help="binario de producto a conducir")
    parser.add_argument("--skip-build", "--no-build", dest="skip_build", action="store_true",
                        help="no construye (exit 3 si `crates/**` es mas nuevo que el binario)")
    parser.add_argument("--out", default=str(DEFAULT_OUT), help="directorio de captura e informe")
    parser.add_argument("--data-root", default=str(DEFAULT_DATA_ROOT), help="datos retail (ARENA2)")
    parser.add_argument("--display", default=None, help="servidor X (por defecto $DISPLAY)")
    parser.add_argument("--title-deadline", type=float, default=DEFAULT_TITLE_DEADLINE,
                        help="tope en segundos para ver `SHELL MENU` (>= 60)")
    parser.add_argument("--raise", dest="raise_window", action="store_true",
                        help="eleva la ventana a foco (`_NET_ACTIVE_WINDOW`); por defecto no se toca el foco")
    parser.add_argument("--settle", type=float, default=DEFAULT_SETTLE,
                        help="segundos de pintado tras el titulo, antes de capturar")
    parser.add_argument("--rect", nargs=2, type=int, default=list(DEFAULT_RECT), metavar=("W", "H"),
                        help="rect central medido (por defecto 640 400)")
    parser.add_argument("--distinct-max", type=int, default=DISTINCT_MAX, help="MW4: colores distintos maximos")
    parser.add_argument("--flat-rows-min", type=int, default=FLAT_ROWS_MIN, help="MW4: filas planas minimas")
    parser.add_argument("--nonblack-min", type=float, default=NONBLACK_MIN, help="MW4: centro no negro minimo")
    parser.add_argument("--frame-distinct-max", type=int, default=FRAME_DISTINCT_MAX,
                        help="MW8: colores distintos maximos por franja del marco (1)")
    parser.add_argument("--frame-off-field-max", type=int, default=FRAME_OFF_FIELD_MAX,
                        help="MW8: franjas del marco permitidas fuera del campo PAL221 (0)")
    parser.add_argument("--json", action="store_true", help="imprime la ruta del report.json")
    parser.add_argument("--keep", action="store_true", help="imprime el directorio conservado")
    parser.add_argument("--list", action="store_true", help="tabla de filas y falsificadores")
    parser.add_argument("--self-test", action="store_true", help="falsificadores con fixtures sinteticas")
    parser.add_argument("--falsify", nargs="+", metavar="NAME", choices=sorted(FALSIFIERS),
                        help="control negativo limpio->rojo->restaurado sobre una corrida real")
    args = parser.parse_args(argv)

    if args.title_deadline < 60.0:
        # the statement's ceiling is >= 60 s: below that it would be declared NOT-RUN by clock, not by product
        args.title_deadline = 60.0
    if args.list:
        return mode_list()
    if args.self_test:
        return mode_self_test(args)
    if args.falsify:
        return mode_falsify(args)
    return run(args)


if __name__ == "__main__":
    sys.exit(main())
