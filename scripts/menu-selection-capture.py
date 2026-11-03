#!/usr/bin/env python3
"""Capturas 1280x720 del menu MODERNO: la barra de seleccion tiene que MOVERSE.

Retarget de `CR-4a9c1e77` (2026-10-31). El script anterior medía el plato retail
(`ui_spec::build_main_menu_panel`, canvas 320x200 a escala 2, chevron en la
columna x 790..816): ese plato ya no se embarca — `MENU-MODERN-DESIGN.md` §9.4
lo deja como fixture de `UI-001` — así que medía pantalla muerta. La pantalla que
se embarca es `menu_spec::main_menu(row, true).compose()` en su propia capa
(`MENU_LAYER`), canvas 320x180 a escala 4 = 1280x720 a sangre (0 bandas).

Geometria medida contra la maqueta sellada
`docs/sintergic/design/mockups/main-menu-1280x720.png` (sha256 `43002b7f…`,
hermana 320x180 `c50e77b1…`): fila i en `y = 220 + 68i`, alto 56, barra de
seleccion `x 108..811` con relleno RGB (44,48,49). La columna 500 y la 700 caen
fuera de todo glifo, así que la banda es un tramo contiguo limpio.

Falsificadores (todos con exit != 0):
  * tamano != 1280x720, o >=1 pixel negro puro  → sigue siendo la pantalla vieja
    (el menú retail x2 venía letterboxeado con franjas negras);
  * `paso0` != maqueta sellada pixel a pixel   → la composición derivo;
  * banda fuera de `220 + 68i` o alto != 56    → no es la fila i a escala 4;
  * capturas repetidas                         → la pantalla está congelada.

`--frozen` es el falsificador en ventana: lanza el binario con
`DF_MENU_ROW_FROZEN=1` (el sistema pinta siempre la fila 0) y exige que TODAS las
capturas salgan identicas (gate RED, exit 1).
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

from Xlib import X, XK, display as xdisplay
from Xlib.protocol import event as xevent

ROOT = Path(__file__).resolve().parent.parent
BIN = ROOT / "target" / "debug" / "daggerfall-app"
DATA = Path(os.environ.get("DF_APP_ROOT") or (Path(os.environ.get("HOME", "")) / "Games" / "Daggerfall" / "app"))
OUT = ROOT / ".sintergic" / "artifacts" / "menu-selection"
TITLE_TOKEN = "DAGGERFALL"
DISPLAY = os.environ.get("DISPLAY", ":0")

WINDOW_W, WINDOW_H = 1280, 720          # 320x180 canvas x PANEL_SCALE, a sangre
PANEL_SCALE = 4
ROW_FIRST_Y = 220                       # 55 canvas x4
ROW_STRIDE = 68                         # 17 canvas x4
ROW_HEIGHT = 56                         # 14 canvas x4
BAR_X = (108, 811)                      # tramo de tinta de la barra (704 px)
BAR_COLUMNS = (500, 700)                # fuera de todo glifo
BAR_RGB = (44, 48, 49)                  # relleno de la fila seleccionada
FIELD_RGB = (23, 23, 23)                # campo del menu (fondo a sangre)
REFERENCE = ROOT / "docs" / "sintergic" / "design" / "mockups" / "main-menu-1280x720.png"
REFERENCE_SHA256 = "43002b7f8b35b916"
# El lintel derecho ("CHAPTER II") se compone en el ancla de la maqueta:
# `glyph_right(..., 304, tracking=0)` ⇒ primera columna de tinta en canvas 262,
# igual que `render.py::lintel`. Por eso el techo es **0**: cualquier píxel de
# diferencia en el lintel es una composición que derivó de la referencia
# sellada, no un delta tolerado (CR-178183d7, lane MenuModernImpl5).
LINTEL_RIGHT_BOX = (1008, 36, 1216, 56)
# Contenido del plato `detail` (canvas 229,41,66,100 x4): cambia con la fila
# seleccionada (rótulo, retrato, cifras), así que no es zona estática.
DETAIL_CONTENT_BOX = (916, 164, 1180, 564)
# Barra de selección de cada fila: su tinta es justo lo que se mide por banda.
# La fila seleccionada ocupa el contenido del plato nav (canvas 25..203):
# relleno desde x=108 y raíl de 2 px de canvas en x=100..107.
BAR_BOX_X = (100, 812)
BAR_BOXES = tuple((BAR_BOX_X[0], ROW_FIRST_Y + ROW_STRIDE * i, BAR_BOX_X[1],
                   ROW_FIRST_Y + ROW_STRIDE * i + ROW_HEIGHT) for i in range(5))
BAR_BOX_W = BAR_BOX_X[1] - BAR_BOX_X[0]   # 712: raíl + relleno (la caja que se cuenta)
# Techo de la fila `barra`: INTERCAMBIAR la selección de fila cambia exactamente dos
# bandas enteras (la que la pierde y la que la gana) medidas sobre la caja, no sobre la
# tinta (712 px, no 704: el raíl de 2 px de canvas también cambia). Con 704 la mudanza
# legítima de fila 0→1 daba 79744 y el techo la declaraba fallo.
MAX_BAR_DIFF = 2 * ROW_HEIGHT * BAR_BOX_W
LINTEL_DELTA_BUDGET = 0                 # el ancla del lintel YA es la de la maqueta
FIELD = (23, 23, 23)
GOLD = (254, 225, 156)                  # borde del lintel en la maqueta sellada
BORDER_ALLOWED = {FIELD, GOLD}
PRESSES = 4
FROZEN = "--frozen" in sys.argv[1:]


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
        return str(win.get_wm_name() or "")
    except Exception:
        return ""


def walk(d, win, acc):
    acc.append(win)
    try:
        for child in win.query_tree().children:
            walk(d, child, acc)
    except Exception:
        pass
    return acc


def send_key(d, win, keysym: str, down: bool) -> None:
    root = d.screen().root
    code = d.keysym_to_keycode(XK.string_to_keysym(keysym))
    cls = xevent.KeyPress if down else xevent.KeyRelease
    mask = X.KeyPressMask if down else X.KeyReleaseMask
    win.send_event(
        cls(time=X.CurrentTime, root=root, window=win, same_screen=1, child=X.NONE,
            root_x=0, root_y=0, event_x=0, event_y=0, state=0, detail=code),
        event_mask=mask,
    )
    d.sync()


def press(d, win, keysym: str, hold: float = 0.12, gap: float = 0.70) -> None:
    send_key(d, win, keysym, True)
    time.sleep(hold)
    send_key(d, win, keysym, False)
    time.sleep(gap)


def capture(win, png: Path) -> None:
    if png.exists():
        png.unlink()
    rc = subprocess.run([shutil.which("import"), "-display", DISPLAY, "-window", hex(win.id), str(png)],
                        capture_output=True, text=True)
    if rc.returncode != 0 or not png.is_file() or png.stat().st_size == 0:
        raise SystemExit("import fallo rc=%d %s" % (rc.returncode, (rc.stderr or rc.stdout)[:200]))


def bar_bands(img, column: int):
    """Tramos contiguos de la tinta de la barra en una columna sin glifos."""
    bands = []
    for y in range(img.height):
        if img.getpixel((column, y)) == BAR_RGB:
            if bands and y - bands[-1][1] <= 1:
                bands[-1][1] = y
            else:
                bands.append([y, y])
    return [(a, b) for a, b in bands]


def bar_box(row: int) -> tuple:
    return BAR_BOXES[row]


def inside(x: int, y: int, box: tuple) -> bool:
    return box[0] <= x < box[2] and box[1] <= y < box[3]


def measure(png: Path, expected_row: int, reference) -> dict:
    from PIL import Image
    img = Image.open(png).convert("RGB")
    img.load()
    pixels = list(img.get_flattened_data())
    black = sum(1 for p in pixels if p == (0, 0, 0))
    bands = {column: bar_bands(img, column) for column in BAR_COLUMNS}
    reference_diff = None
    static = None
    lintel_delta = None
    bar_delta = None
    detail_delta = None
    if reference is not None:
        reference_diff = 0
        static = 0
        lintel_delta = 0
        bar_delta = 0
        detail_delta = 0
        for index, (got, want) in enumerate(zip(pixels, reference)):
            if got == want:
                continue
            reference_diff += 1
            x, y = index % WINDOW_W, index // WINDOW_W
            if inside(x, y, LINTEL_RIGHT_BOX):
                lintel_delta += 1
            elif inside(x, y, DETAIL_CONTENT_BOX):
                detail_delta += 1
            elif any(inside(x, y, box) for box in BAR_BOXES):
                bar_delta += 1
            else:
                static += 1
    border = set()
    for x in range(0, img.width, 7):
        border.add(img.getpixel((x, 0)))
        border.add(img.getpixel((x, img.height - 1)))
    for y in range(0, img.height, 7):
        border.add(img.getpixel((0, y)))
        border.add(img.getpixel((img.width - 1, y)))
    return {
        "size": img.size,
        "black": black,
        "bands": bands,
        "diff": reference_diff,
        "static": static,
        "detail": detail_delta,
        "lintel": lintel_delta,
        "bar": bar_delta,
        "border": sorted(border),
        "expected_top": ROW_FIRST_Y + ROW_STRIDE * expected_row,
        "sha256": hashlib.sha256(png.read_bytes()).hexdigest()[:16],
    }


def verdict(m: dict) -> list:
    """Motivos de fallo de UNA captura (vacío = pasa)."""
    bad = []
    if m["size"] != (WINDOW_W, WINDOW_H):
        bad.append("size=%dx%d != %dx%d" % (m["size"][0], m["size"][1], WINDOW_W, WINDOW_H))
    if m["black"]:
        bad.append("black_px=%d (franjas: pantalla vieja)" % m["black"])
    for column, bands in m["bands"].items():
        if len(bands) != 1:
            bad.append("col%d bandas=%s (se esperaba 1)" % (column, bands))
            continue
        top, bottom = bands[0]
        if top != m["expected_top"]:
            bad.append("col%d top=%d != %d" % (column, top, m["expected_top"]))
        if bottom - top + 1 != ROW_HEIGHT:
            bad.append("col%d alto=%d != %d" % (column, bottom - top + 1, ROW_HEIGHT))
    stray = [c for c in m["border"] if c not in BORDER_ALLOWED]
    if stray:
        bad.append("borde=%s fuera de la maqueta sellada %s" % (stray, sorted(BORDER_ALLOWED)))
    if m["diff"] is not None:
        if m["static"]:
            bad.append("diff_estatico=%d (cromo, lintel, footer y filas no seleccionadas deben ser idénticos)" % m["static"])
        if m["lintel"] > LINTEL_DELTA_BUDGET:
            bad.append("delta_lintel=%d > %d" % (m["lintel"], LINTEL_DELTA_BUDGET))
        if m["bar"] > MAX_BAR_DIFF:
            bad.append("delta_barra=%d > %d" % (m["bar"], MAX_BAR_DIFF))
    return bad


def main() -> int:
    from PIL import Image
    OUT.mkdir(parents=True, exist_ok=True)
    reference = None
    if REFERENCE.is_file():
        sha = hashlib.sha256(REFERENCE.read_bytes()).hexdigest()[:16]
        if sha != REFERENCE_SHA256:
            print("VERDICT: SELECTION-VISIBLE FAIL — maqueta sellada alterada sha=%s" % sha)
            return 1
        ref = Image.open(REFERENCE).convert("RGB")
        ref.load()
        if ref.size != (WINDOW_W, WINDOW_H):
            print("VERDICT: SELECTION-VISIBLE FAIL — maqueta %dx%d" % ref.size)
            return 1
        reference = list(ref.get_flattened_data())
    else:
        print("aviso: maqueta ausente %s — sin diff de referencia" % REFERENCE)

    prefix = "frozen" if FROZEN else "paso"
    log_path = OUT / ("app-%s.log" % prefix)
    d = xdisplay.Display(DISPLAY)
    state = Path(tempfile.mkdtemp(prefix="df-menu-selection-")) / "shell.state"
    state.write_text("", encoding="utf-8")
    log = open(log_path, "wb")
    env = dict(os.environ)
    env.update({"DF_APP_ROOT": str(DATA), "DF_SHELL_STATE": str(state), "DISPLAY": DISPLAY})
    if FROZEN:
        env["DF_MENU_ROW_FROZEN"] = "1"
    else:
        env.pop("DF_MENU_ROW_FROZEN", None)
    proc = subprocess.Popen([str(BIN)], cwd=str(ROOT), env=env, stdout=log, stderr=subprocess.STDOUT)
    steps = []
    try:
        deadline, win = time.monotonic() + 120.0, None
        while time.monotonic() < deadline:
            for w in walk(d, d.screen().root, []):
                try:
                    nm = name_of(d, w)
                except Exception:
                    continue
                if TITLE_TOKEN in nm.upper():
                    win = w
                    break
            if win is not None:
                break
            if proc.poll() is not None:
                print("VERDICT: SELECTION-VISIBLE FAIL — app murio antes del menu (EXIT=%s)" % proc.returncode)
                return 1
            time.sleep(0.25)
        if win is None:
            print("VERDICT: SELECTION-VISIBLE FAIL — no-window title=%r" % TITLE_TOKEN)
            return 1
        try:
            win.set_input_focus(X.RevertToParent, X.CurrentTime)
            d.sync()
        except Exception:
            pass
        # El surface aparece antes de pintar: se sondea hasta que la captura deja
        # de ser negra (el menú moderno es a sangre; negro = todavía cargando).
        ready_png = OUT / ("%s-ready.png" % prefix)
        deadline = time.monotonic() + 90.0
        while True:
            capture(win, ready_png)
            if measure(ready_png, 0, None)["black"] == 0:
                break
            if time.monotonic() >= deadline:
                print("VERDICT: SELECTION-VISIBLE FAIL — el surface sigue negro a los 90 s")
                return 1
            time.sleep(0.50)
        time.sleep(1.0)

        for n in range(PRESSES + 1):
            png = OUT / ("%s%d.png" % (prefix, n))
            if n:
                press(d, win, "Down")
                # La pintura llega en el frame siguiente a la entrada: se sondea
                # hasta que el buffer CAMBIA (no se asume un retardo fijo).
                prev = steps[-1]["sha256"]
                deadline = time.monotonic() + 6.0
                while True:
                    capture(win, png)
                    sha = hashlib.sha256(png.read_bytes()).hexdigest()[:16]
                    if sha != prev or time.monotonic() >= deadline:
                        break
                    time.sleep(0.30)
            else:
                capture(win, png)
            m = measure(png, n if not FROZEN else 0, reference)
            m["n"] = n
            steps.append(m)
            print("%s %d %s %dx%d sha=%s banda500=%s banda700=%s diff=%s estatico=%s lintel=%s detalle=%s barra=%s borde=%s"
                  % (prefix, n, png.name, m["size"][0], m["size"][1], m["sha256"],
                     m["bands"].get(500), m["bands"].get(700), m["diff"], m["static"],
                     m["lintel"], m["detail"], m["bar"], m["border"]))
            for reason in verdict(m):
                print("  ✗ %s" % reason)
    finally:
        if proc.poll() is None:
            proc.terminate()
            try:
                proc.wait(timeout=5)
            except Exception:
                proc.kill()
        log.close()

    logtext = log_path.read_text(errors="replace")
    painted = [ln for ln in logtext.splitlines() if "painted" in ln and "main-menu selection" in ln]
    reasons = [ln for ln in logtext.splitlines() if "row not painted" in ln]
    print("painted_lines=%d reasons=%s" % (len(painted), reasons or "-"))
    for ln in painted:
        print("  log: %s" % ln)

    shas = [s["sha256"] for s in steps]
    failures = [(s["n"], verdict(s)) for s in steps]
    failures = [(n, why) for n, why in failures if why]
    if FROZEN:
        identical = len(set(shas)) == 1
        froze_logged = any("FROZEN" in ln for ln in painted)
        print("shas=%s all_identical=%s frozen_logged=%s fallos=%d"
              % (shas, identical, froze_logged, len(failures)))
        if identical and froze_logged and not failures:
            print("VERDICT: FALSIFIER-BITES — frozen row ⇒ identical captures (gate RED, as required)")
            return 1
        print("VERDICT: FALSIFIER-DOES-NOT-BITE — the frozen row still changed the screen")
        return 2

    distinct = len(set(shas)) == len(shas)
    tops = [s["bands"].get(500, [[None]])[0][0] for s in steps]
    stride = all((b - a) == ROW_STRIDE for a, b in zip(tops, tops[1:]))
    # La fila 0 se embarca tal cual: tiene que coincidir con la maqueta sellada.
    reference_exact = steps[0]["diff"] == LINTEL_DELTA_BUDGET
    moved = all(0 < s["bar"] <= MAX_BAR_DIFF for s in steps[1:])
    print("tops=%s filas=%s distinct=%s stride=%s ref_fila0=%s barrido=%s fallos=%d"
          % (tops, [(t - ROW_FIRST_Y) // ROW_STRIDE if t is not None else None for t in tops],
             distinct, stride, reference_exact, moved, len(failures)))
    ok = (not failures) and distinct and stride and reference_exact and moved and len(painted) >= 2
    print("VERDICT: %s" % ("SELECTION-VISIBLE PASS" if ok else "SELECTION-VISIBLE FAIL"))
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())
