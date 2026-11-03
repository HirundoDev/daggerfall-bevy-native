#!/usr/bin/env python3
"""SHELL-001 — gate de la capa aplicación de M-SHELL fase 1 (10 capacidades).

El gate lanza el binario de producto (`target/debug/daggerfall-app`, **sin**
`--window`: ese flag selecciona el camino legacy M2), con
`DF_SHELL_STATE=<ruta absoluta>` y `DF_APP_ROOT=<datos retail>`, y mide la
secuencia de shell que el contrato app↔gate fija VERBATIM
(`.sintergic/packs/CR-ca78eb76-shell-gate.md` §CONTRATO):

  * el título de la ventana lleva la etiqueta `SHELL BOOT|LOADING|MENU|PLAYING|
    PAUSED|EXIT` (se lee por `_NET_WM_NAME`, como `playthrough-p1-door.py`);
  * la app espeja sus líneas `SHELL VE: …` en `$DF_SHELL_STATE` (append + flush),
    y **ese fichero es el canal de sincronización**: cada tecla se envía cuando
    la línea que la etapa anterior promete ya está escrita, nunca por `sleep`;
  * las teclas son eventos X11 **núcleo** (`send_event`; XTEST es inerte en este
    puesto), ruta aceptada en `scripts/playthrough-p1-door.py`.

Filas medidas (capa aplicación, COVERAGE-100 §2.1 ids 1,2,3,4,6,9,10,11,12,13;
las ids 5, 7 y 8 son GAP declarado y **no** se miden aquí)::

    SH-01 arranque del ejecutable      título SHELL BOOT sin entrada, ventana mapeada
    SH-02 pantalla de carga            SHELL LOADING + step/units monótonos (N>=3)
    SH-03 menú principal navegable     SHELL MENU + Down/Up cambian `selection`
    SH-04 nueva partida                Return sobre `new-game` -> SHELL PLAYING, world_ticks=0
    SH-05 carga de mundo               world scene=Exterior buildings>0 panels>0
    SH-06 HUD                          visible=0 en MENU y visible=1 en PLAYING (misma sesión)
    SH-07 mapa                         M abre y cierra `panel=map-local`
    SH-08 viaje rápido                 T con el mapa abierto: delta_px>0 minutes>0, posición cambia
    SH-09 pausa/opciones               Escape: Paused, ticks congelados, resumed=1
    SH-10 salir                        Escape+Q: SHELL EXIT, el proceso termina con 0

El gate distingue «no hay capacidad» de «hay capacidad sin entrada» con un
tercer proceso independiente, `CTRL-no-input`: **0 teclas** durante `--timeout`,
y el título nunca debe llegar a `SHELL PLAYING`.

Contrato de proceso
-------------------
Dos procesos independientes de la secuencia completa. Cada corrida se reduce a
un registro normalizado (`window`, etiquetas de shell vistas, sha256 de la lista
exacta de líneas `SHELL VE:`, teclas, etapa de aborto, código de salida); los
dos registros deben ser byte-idénticos (`PROC-two-runs`). Todo lo impreso es
entero o token: 0 decimales, 0 timestamps, 0 PIDs. Línea terminal `DIGEST
<sha256>` sobre todo lo anterior y línea `SOURCES fingerprint=<64 hex> files=<n>`
recomputada desde disco (`path\0sha256(bytes)`, ordenadas, `\n`).

Modos
-----
    python3 scripts/shell-001.py [--binary PATH] [--skip-build] [--out DIR] [--json]
                                 [--list] [--falsify NAME] [--self-test] [--timeout S]

  run (defecto)  build (flock) o `--skip-build`, 2 procesos + CTRL, filas y digest.
  --self-test    fixture VERDE embebida (líneas del contrato): demuestra que la
                 fixture da 10/10 y exit 0 (el gate PUEDE ponerse verde) y que
                 cada uno de los 11 falsificadores la pone roja. Imprime
                 `GATE-SELFTEST rows=10 green=1 falsifiers=11 caught=11`.
  --falsify N    fixture + una mutación: exige que la fila declarada caiga en rojo.
  --list         una línea por fila con `keys=` esperadas y `failure=` nombrado.

Exit: 0 verde · 1 rojo · 2 NC (sin DISPLAY / sin ARENA2 / sin binario / build
roto) · 3 STALE (binario más viejo que `crates/**` y `--skip-build`).

Uso:
    python3 scripts/shell-001.py --skip-build --out .sintergic/artifacts/shell-001-red
    python3 scripts/shell-001.py --self-test
    python3 scripts/shell-001.py --list
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import subprocess
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
GATE = "SHELL-001"
CR = "CR-ca78eb76-a8a9-4789-a841-1b27bf428110"
DEFAULT_BINARY = ROOT / "target" / "debug" / "daggerfall-app"
DEFAULT_OUT = ROOT / ".sintergic" / "artifacts" / "shell-001"
BUILD_LOCK = ROOT / ".sintergic" / "locks" / "cargo-build.lock"
DEFAULT_DATA_ROOT = Path(
    os.environ.get("DF_APP_ROOT")
    or Path(os.environ.get("HOME", "")) / "Games" / "Daggerfall" / "app"
)

# Fuentes medidas por el gate (capa aplicación del camino de producto) y ámbito
# del control de frescura `crates/**`.
MEASURED_SOURCES = (
    "crates/app/src/main.rs",
    "crates/app/src/m18_window.rs",
    "crates/app/src/m18_phase.rs",
    "crates/app/src/m2_player.rs",
    "crates/app/Cargo.toml",
)
FRESHNESS_GLOBS = ("crates/**/*.rs", "crates/**/Cargo.toml")

SHELL_LABELS = (
    "SHELL BOOT",
    "SHELL LOADING",
    "SHELL MENU",
    "SHELL PLAYING",
    "SHELL PAUSED",
    "SHELL EXIT",
)

# ── anclas de forma (una por fila) ────────────────────────────────────────
# Cada ancla es un `path:line` real a HEAD: hoy señala DÓNDE VIVE LA AUSENCIA
# (o el primitivo parcial); cuando la capacidad exista, señala el módulo que la
# emite. El gate verifica que el path resuelve; la línea puede derivar (misma
# regla que `path_exists` de la familia `sys-*-001`).
ANCHOR_BOOT = "crates/app/src/main.rs:928"
ANCHOR_LOADING = "crates/app/src/m18_phase.rs:48"
ANCHOR_MENU = "crates/app/src/m18_window.rs:1485"
ANCHOR_NEWGAME = "crates/app/src/m18_window.rs:1491"
ANCHOR_WORLD = "crates/app/src/m18_window.rs:445"
ANCHOR_HUD = "crates/app/src/m18_window.rs:1373"
ANCHOR_MAP = "crates/app/src/m18_window.rs:546"
ANCHOR_TRAVEL = "crates/app/src/m18_window.rs:1614"
ANCHOR_PAUSE = "crates/app/src/m18_phase.rs:76"
ANCHOR_EXIT = "crates/app/src/m2_player.rs:103"
ANCHOR_GATE = "scripts/shell-001.py:1"

# ── contrato de líneas `SHELL VE:` (verbatim del pack) ────────────────────
RE_BOOT = re.compile(r"^SHELL VE: boot state=Boot$")
RE_STEP = re.compile(r"^SHELL VE: loading step=(\d+) steps=(\d+) work=(\S+) units=(\d+)$")
RE_LOADED = re.compile(r"^SHELL VE: loading done steps=(\d+) units=(\d+)$")
RE_MENU = re.compile(r"^SHELL VE: menu state=Menu items=(\d+) selection=(\d+) surface=main-menu$")
RE_SEL = re.compile(r"^SHELL VE: menu selection=(\d+)$")
RE_SEL_ONE = re.compile(r"^SHELL VE: menu selection=1$")
RE_SEL_ZERO = re.compile(r"^SHELL VE: menu selection=0$")
RE_NEW = re.compile(r"^SHELL VE: new_game state=Playing item=new-game world_ticks=(\d+)$")
RE_WORLD = re.compile(
    r"^SHELL VE: world scene=(\w+) location=\((\d+),(\d+),(\d+)\)"
    r" buildings=(\d+) npcs=(\d+) panels=(\d+)$"
)
RE_HUD = re.compile(r"^SHELL VE: hud visible=([01]) panel=hud opaque=(\d+) digest=([0-9a-f]{16})$")
RE_MAP_OPEN = re.compile(r"^SHELL VE: map open=1 panel=(\S+)$")
RE_MAP_CLOSE = re.compile(r"^SHELL VE: map open=0$")
RE_TRAVEL = re.compile(
    r"^SHELL VE: travel from=\((\d+),(\d+)\) to=\((\d+),(\d+)\)"
    r" delta_px=(\d+) minutes=(\d+) gold=(\d+) scene=(\w+)$"
)
RE_PAUSE = re.compile(r"^SHELL VE: pause state=Paused ticks=(\d+) options=(\S+)$")
RE_FROZEN = re.compile(r"^SHELL VE: pause frozen=1 ticks=(\d+) state=Paused$")
RE_RESUME = re.compile(r"^SHELL VE: pause resumed=1 state=Playing$")
RE_EXIT = re.compile(r"^SHELL VE: exit state=Exit code=(\d+)$")
RE_HUD_PLAYING = re.compile(r"^SHELL VE: hud visible=1 panel=hud ")


# ── utilidades de evidencia ───────────────────────────────────────────────
def digest(text: str) -> str:
    return hashlib.sha256(text.encode("utf-8")).hexdigest()


def label_of(title: str) -> str:
    """Etiqueta de shell del título, o `none` (el título crudo no se imprime)."""
    for label in SHELL_LABELS:
        if label in title:
            return label
    return "none"


def labels_of(titles: list[str]) -> list[str]:
    return [label for label in SHELL_LABELS if any(label in title for title in titles)]


def matching(cap: dict, pattern: re.Pattern[str]) -> list[tuple[int, re.Match[str]]]:
    return [(index, m) for index, line in enumerate(cap["lines"]) if (m := pattern.match(line))]


def first(cap: dict, pattern: re.Pattern[str]):
    hits = matching(cap, pattern)
    return hits[0] if hits else (None, None)


def anchor_check() -> tuple[int, list[str]]:
    """Cada fila declara su ancla `path:line`: el path debe resolver en disco."""
    anchors = [(row.rid, row.anchor) for row in ROWS] + [(CTRL.rid, CTRL.anchor)]
    unresolved = [
        "%s:%s" % (rid, anchor)
        for rid, anchor in anchors
        if not (ROOT / anchor.split(":")[0]).is_file()
    ]
    return len(anchors), unresolved


# ── filas: un checker por capacidad ───────────────────────────────────────
# Contrato de retorno: (ok, facts, symptom, evidence_found). `evidence_found`
# decide el token del rojo: sin la evidencia exigida es `missing_evidence`; con
# evidencia presente y fuera de contrato, `evidence_mismatch`.
def boot_title(cap: dict) -> str:
    """Primer título que ya lleva etiqueta de shell (antes de cualquier tecla)."""
    for title in cap["titles"]:
        if label_of(title) != "none":
            return title
    return ""


def check_boot(cap: dict):
    title = boot_title(cap)
    if not cap["window"]:
        return False, "window=0 title=none", "the product path opened no window", False
    if not cap.get("alive"):
        return False, "window=1 proc=dead", "the process was gone when the window was mapped", False
    if "SHELL BOOT" not in title:
        return (
            False,
            "window=1 proc=alive title=%s" % label_of(title),
            "the window title never carried SHELL BOOT before any key",
            False,
        )
    return True, "window=1 proc=alive title=SHELL BOOT keys=0", "", True


def check_loading(cap: dict):
    steps = matching(cap, RE_STEP)
    _, done = first(cap, RE_LOADED)
    if not steps and done is None:
        return (
            False,
            "steps=0 title=%s" % ("SHELL LOADING" if "SHELL LOADING" in labels_of(cap["titles"]) else "none"),
            "the loading screen reported no steps at all",
            False,
        )
    total = int(steps[0][1].group(2)) if steps else 0
    units = [int(m.group(4)) for _, m in steps]
    works = [m.group(3) for _, m in steps]
    order = [int(m.group(1)) for _, m in steps]
    facts = "steps=%d units=%d distinct_work=%d" % (len(steps), sum(units), len(set(works)))
    if total < 3:
        return False, facts, "the loading screen does not report N>=3 steps", True
    if order != list(range(1, total + 1)):
        return False, facts, "the loading steps are not monotonic 1..N", True
    if any(m.group(2) != str(total) for _, m in steps):
        return False, facts, "the step count drifted inside the loading screen", True
    if any(unit <= 0 for unit in units):
        return False, facts, "a loading step carries units=0 (a sleep disguised as work)", True
    if len(set(works)) != total:
        return False, facts, "two loading steps share the same work name", True
    if done is None:
        return False, facts, "no `loading done` line closed the screen", True
    if int(done.group(1)) != total or int(done.group(2)) != sum(units):
        return False, facts, "`loading done` disagrees with the steps (steps/units)", True
    if "SHELL LOADING" not in labels_of(cap["titles"]):
        return False, facts + " title=none", "the title never carried SHELL LOADING", True
    return True, facts + " title=SHELL LOADING", "", True


def check_menu(cap: dict):
    _, menu = first(cap, RE_MENU)
    selections = matching(cap, RE_SEL)
    if menu is None and not selections:
        return False, "menu=0 selection=0", "the main menu never reported its items", False
    if menu is None:
        return False, "menu=0 selections=%d" % len(selections), "no `menu state=Menu` line", True
    items, selection = int(menu.group(1)), int(menu.group(2))
    facts = "items=%d selections=%d" % (items, len(selections))
    if items < 2:
        return False, facts, "the main menu offers fewer than 2 items", True
    if selection != 0:
        return False, facts, "the menu did not start on selection=0", True
    if len(selections) < 2:
        return False, facts, "Down/Up printed no selection change", True
    first_sel = int(selections[0][1].group(1))
    second_sel = int(selections[1][1].group(1))
    if first_sel != 1 or second_sel != 0:
        return (
            False,
            "items=%d selection=%d,%d" % (items, first_sel, second_sel),
            "Down then Up did not move the selection 0->1->0",
            True,
        )
    if "SHELL MENU" not in labels_of(cap["titles"]):
        return False, facts + " title=none", "the title never carried SHELL MENU", True
    return True, "items=%d selection=0,1,0 title=SHELL MENU" % items, "", True


def check_new_game(cap: dict):
    _, new = first(cap, RE_NEW)
    if new is None:
        return False, "new_game=0 world_ticks=none", "Return on `new-game` never reached Playing", False
    ticks = int(new.group(1))
    if ticks != 0:
        return False, "new_game=1 world_ticks=%d" % ticks, "the world clock ran inside MENU (world_ticks>0)", True
    if "SHELL PLAYING" not in labels_of(cap["titles"]):
        return False, "new_game=1 world_ticks=0 title=none", "the title never carried SHELL PLAYING", True
    return True, "item=new-game state=Playing world_ticks=0 title=SHELL PLAYING", "", True


def check_world(cap: dict):
    _, world = first(cap, RE_WORLD)
    if world is None:
        return False, "world=0 buildings=none panels=none", "no `world scene=…` line was emitted", False
    scene = world.group(1)
    buildings, npcs, panels = int(world.group(5)), int(world.group(6)), int(world.group(7))
    location = "(%s,%s,%s)" % (world.group(2), world.group(3), world.group(4))
    facts = "scene=%s location=%s buildings=%d npcs=%d panels=%d" % (scene, location, buildings, npcs, panels)
    if scene != "Exterior":
        return False, facts, "the composed world is not scene=Exterior", True
    if buildings <= 0:
        return False, facts, "the world carries no buildings", True
    if panels <= 0:
        return False, facts, "the world carries no UI panels", True
    return True, facts, "", True


def check_hud(cap: dict):
    hud = matching(cap, RE_HUD)
    if not hud:
        return (
            False,
            "hud_hidden_in_menu=none hud_shown_in_playing=none",
            "no `hud visible=…` line was emitted",
            False,
        )
    new_index, new = first(cap, RE_NEW)
    hidden = [index for index, m in hud if m.group(1) == "0"]
    shown = [index for index, m in hud if m.group(1) == "1"]
    in_menu = [index for index in hidden if new_index is not None and index < new_index]
    in_playing = [index for index in shown if new_index is not None and index > new_index]
    facts = "hud_hidden_in_menu=%d hud_shown_in_playing=%d" % (len(in_menu), len(in_playing))
    if new is None:
        return False, facts + " session=0", "no Playing transition to compare the HUD against", True
    if not in_menu:
        return False, facts, "the HUD was never hidden (visible=0) in MENU", True
    if not in_playing:
        return False, facts, "the HUD was never shown (visible=1) in PLAYING", True
    return True, facts + " digest=%s" % hud[-1][1].group(3), "", True


def check_map(cap: dict):
    opens = matching(cap, RE_MAP_OPEN)
    closes = matching(cap, RE_MAP_CLOSE)
    if not opens and not closes:
        return False, "map_open=0 map_close=0", "no `map open=…` line was emitted", False
    facts = "map_open=%d map_close=%d" % (len(opens), len(closes))
    if not opens:
        return False, facts, "M never opened the map", True
    panel = opens[0][1].group(1)
    if panel != "map-local":
        return False, facts + " panel=%s" % panel, "the opened panel is not map-local", True
    if not any(index > opens[0][0] for index, _ in closes):
        return False, facts + " panel=%s" % panel, "M never closed the map again (no open=0 after open=1)", True
    return True, "panel=map-local open=1,0", "", True


def check_travel(cap: dict):
    index, travel = first(cap, RE_TRAVEL)
    if travel is None:
        return False, "travel=0 delta_px=none", "T with the map open did not travel", False
    delta = int(travel.group(5))
    minutes = int(travel.group(6))
    gold = int(travel.group(7))
    scene = travel.group(8)
    origin = (int(travel.group(1)), int(travel.group(2)))
    target = (int(travel.group(3)), int(travel.group(4)))
    facts = "delta_px=%d minutes=%d gold=%d from=%d,%d to=%d,%d scene=%s" % (
        delta, minutes, gold, origin[0], origin[1], target[0], target[1], scene,
    )
    opens = matching(cap, RE_MAP_OPEN)
    if not opens or opens[0][0] > index:
        return False, facts, "the travel did not happen with the map open", True
    if origin == target:
        return False, facts, "the position did not change", True
    if delta <= 0:
        return False, facts, "delta_px=0: the travel moved nothing", True
    if minutes <= 0:
        return False, facts, "minutes=0: the travel costs no game time", True
    if scene != "Exterior":
        return False, facts, "the travel did not land in scene=Exterior", True
    return True, facts, "", True


def check_pause(cap: dict):
    pauses = matching(cap, RE_PAUSE)
    _, frozen = first(cap, RE_FROZEN)
    _, resumed = first(cap, RE_RESUME)
    if not pauses and frozen is None and resumed is None:
        return False, "pause=0 frozen=none resumed=none", "Escape never paused the game", False
    facts = "pause=%d frozen=%d resumed=%d" % (len(pauses), 1 if frozen else 0, 1 if resumed else 0)
    if not pauses:
        return False, facts, "no `pause state=Paused` line", True
    if frozen is None:
        return False, facts, "the pause never reported frozen ticks", True
    ticks, frozen_ticks = int(pauses[0][1].group(1)), int(frozen.group(1))
    facts = "ticks=%d frozen_ticks=%d resumed=%d" % (ticks, frozen_ticks, 1 if resumed else 0)
    if frozen_ticks != ticks:
        return False, facts, "the world clock advanced while paused", True
    if resumed is None:
        return False, facts, "Escape never resumed the game", True
    if "SHELL PAUSED" not in labels_of(cap["titles"]):
        return False, facts + " title=none", "the title never carried SHELL PAUSED", True
    return True, "ticks=%d frozen=1 resumed=1 title=SHELL PAUSED" % ticks, "", True


def check_exit(cap: dict):
    _, exit_line = first(cap, RE_EXIT)
    code = int(exit_line.group(1)) if exit_line is not None else None
    app_exit = cap["exit"]
    facts = "code=%s app_exit=%s" % (
        code if code is not None else "none",
        app_exit if app_exit is not None else "none",
    )
    if exit_line is None:
        return False, facts, "Q in pause printed no `exit state=Exit` line", False
    if code != 0:
        return False, facts, "the app declared a non-zero exit code", True
    if app_exit != 0:
        return False, facts, "the process did not terminate with code 0", True
    if "SHELL EXIT" not in labels_of(cap["titles"]):
        return False, facts + " title=none", "the title never carried SHELL EXIT", True
    return True, "code=0 app_exit=0 title=SHELL EXIT", "", True


class Row:
    __slots__ = ("rid", "cov", "name", "keys", "expected", "failures", "anchor", "check")

    def __init__(self, rid, cov, name, keys, expected, failures, anchor, check):
        self.rid = rid
        self.cov = cov
        self.name = name
        self.keys = keys
        self.expected = expected
        self.failures = failures
        self.anchor = anchor
        self.check = check


ROWS = (
    Row("SH-01", 1, "arranque del ejecutable", (), "title=SHELL BOOT", ("boot-no-window",), ANCHOR_BOOT, check_boot),
    Row("SH-02", 2, "pantalla de carga", (), "steps>=3", ("loading-single-step", "loading-sleep-disguised"), ANCHOR_LOADING, check_loading),
    Row("SH-03", 3, "menu principal navegable", ("Down", "Up"), "selection=0,1,0", ("menu-no-nav",), ANCHOR_MENU, check_menu),
    Row("SH-04", 4, "nueva partida", ("Return",), "world_ticks=0", ("newgame-ticks-nonzero",), ANCHOR_NEWGAME, check_new_game),
    Row("SH-05", 6, "carga de mundo", (), "buildings>0", ("world-empty",), ANCHOR_WORLD, check_world),
    Row("SH-06", 9, "HUD", (), "visible=0,1", ("hud-visible-in-menu",), ANCHOR_HUD, check_hud),
    Row("SH-07", 10, "mapa", ("M", "M"), "open=1,0", ("map-no-toggle",), ANCHOR_MAP, check_map),
    Row("SH-08", 11, "viaje rapido", ("M", "T"), "delta_px>0", ("travel-zero",), ANCHOR_TRAVEL, check_travel),
    Row("SH-09", 12, "pausa/opciones", ("Escape", "Escape"), "frozen=1", ("pause-ticks-advance",), ANCHOR_PAUSE, check_pause),
    Row("SH-10", 13, "salir", ("Escape", "Q"), "code=0", ("exit-no-appexit",), ANCHOR_EXIT, check_exit),
)

CTRL = Row(
    "CTRL-no-input",
    0,
    "control negativo sin entrada",
    (),
    "playing=0",
    ("app-no-window",),
    ANCHOR_GATE,
    None,
)


def check_ctrl(cap: dict):
    """0 teclas: el título NUNCA llega a `SHELL PLAYING`. Sin ventana, es rojo."""
    playing = any("SHELL PLAYING" in title for title in cap["titles"])
    if not cap["window"]:
        return False, "window=0 playing=0", "the app opened no window: the control cannot be judged", False
    if playing:
        return False, "window=1 playing=1", "the title reached SHELL PLAYING with zero keys", True
    return True, "window=1 playing=0 keys=0", "", True


def evaluate(cap: dict) -> list[tuple[Row, tuple]]:
    return [(row, row.check(cap)) for row in ROWS]


# ── fixture VERDE embebida (las líneas del contrato) ──────────────────────
FIXTURE_TITLES = [
    "DAGGERFALL-LIKE — SHELL BOOT",
    "DAGGERFALL-LIKE — SHELL LOADING",
    "DAGGERFALL-LIKE — SHELL MENU",
    "DAGGERFALL-LIKE — SHELL PLAYING",
    "DAGGERFALL-LIKE — SHELL PAUSED",
    "DAGGERFALL-LIKE — SHELL EXIT",
]
FIXTURE_LINES = [
    "SHELL VE: boot state=Boot",
    "SHELL VE: loading step=1 steps=4 work=maps units=612",
    "SHELL VE: loading step=2 steps=4 work=blocks units=15151",
    "SHELL VE: loading step=3 steps=4 work=flats units=1253",
    "SHELL VE: loading step=4 steps=4 work=climate units=62",
    "SHELL VE: loading done steps=4 units=17078",
    "SHELL VE: menu state=Menu items=4 selection=0 surface=main-menu",
    "SHELL VE: hud visible=0 panel=hud opaque=0 digest=0000000000000000",
    "SHELL VE: menu selection=1",
    "SHELL VE: menu selection=0",
    "SHELL VE: new_game state=Playing item=new-game world_ticks=0",
    "SHELL VE: world scene=Exterior location=(105,52,14) buildings=817 npcs=42 panels=6",
    "SHELL VE: hud visible=1 panel=hud opaque=51200 digest=0123456789abcdef",
    "SHELL VE: map open=1 panel=map-local",
    "SHELL VE: map open=0",
    "SHELL VE: map open=1 panel=map-local",
    "SHELL VE: travel from=(100,50) to=(104,52) delta_px=4 minutes=45 gold=12 scene=Exterior",
    "SHELL VE: pause state=Paused ticks=120 options=pause-menu",
    "SHELL VE: pause frozen=1 ticks=120 state=Paused",
    "SHELL VE: pause resumed=1 state=Playing",
    "SHELL VE: pause state=Paused ticks=180 options=pause-menu",
    "SHELL VE: exit state=Exit code=0",
]


def fixture_capture() -> dict:
    return {
        "window": True,
        "alive": 1,
        "titles": list(FIXTURE_TITLES),
        "lines": list(FIXTURE_LINES),
        "exit": 0,
        "keys": 6,
        "abort": None,
    }


# ── falsificadores: mutan la EVIDENCIA capturada, nunca el runtime ────────
def _salient_index(cap: dict, pattern: re.Pattern[str]) -> int:
    index, match = first(cap, pattern)
    return -1 if match is None else index


def mut_boot_no_window(cap: dict) -> str:
    cap["window"] = False
    cap["alive"] = 0
    cap["titles"] = []
    return "window cleared: the product path opened no window"


def mut_loading_single_step(cap: dict) -> str:
    kept: list[str] = []
    for line in cap["lines"]:
        step = RE_STEP.match(line)
        if step is not None:
            if step.group(1) != "1":
                continue
            line = "SHELL VE: loading step=1 steps=1 work=%s units=%s" % (step.group(3), step.group(4))
        done = RE_LOADED.match(line)
        if done is not None:
            line = "SHELL VE: loading done steps=1 units=%s" % done.group(2)
        kept.append(line)
    cap["lines"] = kept
    return "one loading step declared N=1 (a screen that never loaded anything)"


def mut_loading_sleep_disguised(cap: dict) -> str:
    kept: list[str] = []
    for line in cap["lines"]:
        step = RE_STEP.match(line)
        if step is not None and step.group(1) == "2":
            line = "SHELL VE: loading step=2 steps=4 work=%s units=0" % step.group(3)
        kept.append(line)
    cap["lines"] = kept
    return "step 2 carries units=0 (a sleep disguised as work)"


def mut_loading_work_repeated(cap: dict) -> str:
    kept: list[str] = []
    repeated = ""
    for line in cap["lines"]:
        step = RE_STEP.match(line)
        if step is not None and step.group(1) == "2":
            repeated = step.group(3)
        elif step is not None and step.group(1) == "3" and repeated:
            line = "SHELL VE: loading step=3 steps=%s work=%s units=%s" % (
                step.group(2), repeated, step.group(4),
            )
        kept.append(line)
    cap["lines"] = kept
    return "step 3 repeats the work name of step 2 (the same work counted twice)"


def mut_menu_no_nav(cap: dict) -> str:
    cap["lines"] = [line for line in cap["lines"] if RE_SEL.match(line) is None]
    return "the selection-change lines are gone (Down/Up did nothing)"


def mut_newgame_ticks_nonzero(cap: dict) -> str:
    cap["lines"] = [
        re.sub(r"world_ticks=\d+", "world_ticks=3", line) if RE_NEW.match(line) else line
        for line in cap["lines"]
    ]
    return "world_ticks=3 on the new_game line (the clock ran inside MENU)"


def mut_world_empty(cap: dict) -> str:
    cap["lines"] = [
        re.sub(r"buildings=\d+", "buildings=0", line) if RE_WORLD.match(line) else line
        for line in cap["lines"]
    ]
    return "buildings=0 (an empty world composed)"


def mut_hud_visible_in_menu(cap: dict) -> str:
    new_index = _salient_index(cap, RE_NEW)
    kept: list[str] = []
    for index, line in enumerate(cap["lines"]):
        if index < new_index and line.startswith("SHELL VE: hud visible=0"):
            line = line.replace("visible=0", "visible=1", 1)
        kept.append(line)
    cap["lines"] = kept
    return "the MENU HUD reports visible=1 (the HUD leaks into the menu)"


def mut_map_no_toggle(cap: dict) -> str:
    cap["lines"] = [line for line in cap["lines"] if not RE_MAP_CLOSE.match(line)]
    return "the `map open=0` lines are gone (M never closes the map)"


def mut_travel_zero(cap: dict) -> str:
    cap["lines"] = [
        re.sub(r"delta_px=\d+", "delta_px=0", line) if RE_TRAVEL.match(line) else line
        for line in cap["lines"]
    ]
    return "delta_px=0 (the fast travel moved nothing)"


def mut_pause_ticks_advance(cap: dict) -> str:
    kept: list[str] = []
    for line in cap["lines"]:
        frozen = RE_FROZEN.match(line)
        if frozen is not None:
            line = "SHELL VE: pause frozen=1 ticks=%d state=Paused" % (int(frozen.group(1)) + 1)
        kept.append(line)
    cap["lines"] = kept
    return "the frozen ticks advanced by 1 (the clock never froze)"


def mut_exit_no_appexit(cap: dict) -> str:
    cap["exit"] = 101
    return "the process ended with 101 (the shell declared code=0 but did not exit)"


# Guardas internas del checker: facetas del pack que no tienen falsificador propio
# (el pack nombra `loading-sleep-disguised` como «units=0 O work= repetido»).
GUARDS = (("loading-work-repeated", "SH-02", mut_loading_work_repeated),)

FALSIFIERS = (
    ("boot-no-window", "SH-01", mut_boot_no_window),
    ("loading-single-step", "SH-02", mut_loading_single_step),
    ("loading-sleep-disguised", "SH-02", mut_loading_sleep_disguised),
    ("menu-no-nav", "SH-03", mut_menu_no_nav),
    ("newgame-ticks-nonzero", "SH-04", mut_newgame_ticks_nonzero),
    ("world-empty", "SH-05", mut_world_empty),
    ("hud-visible-in-menu", "SH-06", mut_hud_visible_in_menu),
    ("map-no-toggle", "SH-07", mut_map_no_toggle),
    ("travel-zero", "SH-08", mut_travel_zero),
    ("pause-ticks-advance", "SH-09", mut_pause_ticks_advance),
    ("exit-no-appexit", "SH-10", mut_exit_no_appexit),
)


# ── fuentes y frescura ────────────────────────────────────────────────────
def sources_line() -> tuple[str, list[str]]:
    parts: list[str] = []
    paths: list[str] = []
    missing: list[str] = []
    for rel_path in MEASURED_SOURCES:
        path = ROOT / rel_path
        if not path.is_file():
            missing.append(rel_path)
            continue
        parts.append("%s\0%s" % (rel_path, hashlib.sha256(path.read_bytes()).hexdigest()))
        paths.append(rel_path)
    fingerprint = hashlib.sha256("\n".join(sorted(parts)).encode("utf-8")).hexdigest()
    line = "SOURCES fingerprint=%s files=%d paths=%s" % (
        fingerprint,
        len(paths),
        ",".join(paths) or "-",
    )
    return line, missing


def newest_source() -> tuple[int, str]:
    newest_mtime, newest_path = 0, "-"
    for pattern in FRESHNESS_GLOBS:
        for path in ROOT.glob(pattern):
            if not path.is_file():
                continue
            mtime = int(path.stat().st_mtime)
            if mtime > newest_mtime:
                newest_mtime, newest_path = mtime, str(path.relative_to(ROOT))
    return newest_mtime, newest_path


def mirrored_lines(log_path: Path) -> list[str]:
    """Líneas `SHELL VE:` espejadas en stdout (cláusula 1 del contrato)."""
    try:
        text = log_path.read_text(encoding="utf-8", errors="replace")
    except FileNotFoundError:
        return []
    return [line for line in text.splitlines() if line.startswith("SHELL VE: ")]


def head_commit() -> str:
    try:
        return subprocess.check_output(
            ["git", "-C", str(ROOT), "rev-parse", "HEAD"], text=True, stderr=subprocess.DEVNULL
        ).strip()
    except Exception:
        return "unknown"


def build() -> tuple[bool, str]:
    """`cargo build -p daggerfall-app --features window` bajo flock (G6)."""
    BUILD_LOCK.parent.mkdir(parents=True, exist_ok=True)
    import fcntl

    with open(BUILD_LOCK, "w", encoding="utf-8") as lock:
        fcntl.flock(lock.fileno(), fcntl.LOCK_EX)
        completed = subprocess.run(
            ["cargo", "build", "-p", "daggerfall-app", "--features", "window"],
            cwd=str(ROOT),
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
        )
    if completed.returncode != 0:
        tail = (completed.stderr or completed.stdout).strip().splitlines()
        return False, tail[-1] if tail else "cargo build failed"
    return True, ""


# ── driver X11 (patrón aceptado: `scripts/playthrough-p1-door.py`) ────────
NET_ACTIVE_WINDOW = "_NET_ACTIVE_WINDOW"
NET_WM_NAME = "_NET_WM_NAME"
UTF8 = "UTF8_STRING"


def _x11():
    from Xlib import X, XK, display  # import local: --list/--self-test no necesitan X
    from Xlib.protocol import event as xevent

    return X, XK, display, xevent


def name_of(d, win) -> str:
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
        return str(nm) if nm else ""
    except Exception:
        return ""


def walk(d, win, depth=0, acc=None):
    acc = [] if acc is None else acc
    if depth > 3:
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


def is_shell_candidate(name: str) -> bool:
    """Ventana candidata: título con etiqueta `SHELL <ETAPA>` o el nombre del proyecto."""
    if "DAGGERFALL" in name.upper():
        return True
    return any(label in name for label in SHELL_LABELS)


def window_owner_pid(d, win):
    """`_NET_WM_PID` de la ventana, o None si el toolkit no lo publica."""
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


def find_window(d, deadline, proc):
    """Ventana propia del proceso: por `_NET_WM_PID`, o por título si falta el PID."""
    fallback = None
    while time.monotonic() < deadline:
        for win, name in walk(d, d.screen().root):
            if not is_shell_candidate(name):
                continue
            if window_owner_pid(d, win) == proc.pid:
                return win
            if fallback is None:
                fallback = win
        if fallback is not None:
            owner = window_owner_pid(d, fallback)
            if owner is None or owner == proc.pid:
                return fallback
        if proc.poll() is not None:
            return None
        time.sleep(0.25)
    return None


def activate(d, win, X, timeout=4.0) -> bool:
    """Foco real de teclado: `_NET_ACTIVE_WINDOW` (origen 2) + `set_input_focus`."""
    from Xlib.protocol import event as xevent

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
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        try:
            focus = d.get_input_focus().focus
            fid = focus.id if hasattr(focus, "id") else focus
        except Exception:
            fid = None
        if fid == win.id:
            return True
        time.sleep(0.2)
    return False


def send_key(d, win, X, XK, xevent, name, down):
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


def press(d, win, X, XK, xevent, name):
    send_key(d, win, X, XK, xevent, name, True)
    time.sleep(0.06)
    send_key(d, win, X, XK, xevent, name, False)
    time.sleep(0.12)


# Etapas de la secuencia: (etapa, tecla o None, patrón que la etapa deja escrito).
SEQUENCE = (
    ("boot", None, RE_BOOT),
    ("loading", None, RE_LOADED),
    ("menu", None, RE_MENU),
    ("nav-down", "Down", RE_SEL_ONE),
    ("nav-up", "Up", RE_SEL_ZERO),
    ("new-game", "Return", RE_NEW),
    ("world", None, RE_WORLD),
    ("hud", None, RE_HUD_PLAYING),
    ("map-open", "m", RE_MAP_OPEN),
    ("map-close", "m", RE_MAP_CLOSE),
    ("map-open-2", "m", RE_MAP_OPEN),
    ("travel", "t", RE_TRAVEL),
    ("pause", "Escape", RE_PAUSE),
    ("frozen", None, RE_FROZEN),
    ("resume", "Escape", RE_RESUME),
    ("pause-2", "Escape", RE_PAUSE),
    ("exit", "q", RE_EXIT),
)


class Session:
    """Un proceso del binario de producto y su ventana, con deadline global."""

    def __init__(self, d, X, XK, xevent, binary, data_root, state_path, app_log, timeout):
        self.d = d
        self.X, self.XK, self.xevent = X, XK, xevent
        self.binary = binary
        self.data_root = data_root
        self.state_path = state_path
        self.app_log = app_log
        self.deadline = time.monotonic() + timeout
        self.proc = None
        self.sink = None
        self.win = None
        self.cursor = 0
        self.cap = {
            "window": False,
            "alive": 0,
            "titles": [],
            "lines": [],
            "exit": None,
            "keys": 0,
            "abort": None,
        }

    # -- ciclo de vida ----------------------------------------------------
    def start(self):
        env = dict(os.environ)
        env["DF_APP_ROOT"] = str(self.data_root)
        env["DF_SHELL_STATE"] = str(self.state_path)
        env.setdefault("DISPLAY", os.environ.get("DISPLAY", ":0"))
        self.state_path.parent.mkdir(parents=True, exist_ok=True)
        self.state_path.write_text("", encoding="utf-8")
        self.sink = open(self.app_log, "wb")
        # Sin `--window`: ese flag selecciona el camino legacy M2 (`main.rs:915`).
        self.proc = subprocess.Popen(
            [str(self.binary)],
            cwd=str(ROOT),
            env=env,
            stdout=self.sink,
            stderr=subprocess.STDOUT,
        )

    def stop(self):
        if self.proc is not None and self.proc.poll() is None:
            self.proc.terminate()
            try:
                self.proc.wait(timeout=5)
            except Exception:
                self.proc.kill()
                self.proc.wait(timeout=5)
        if self.proc is not None:
            self.cap["exit"] = self.proc.poll()
        if self.sink is not None:
            self.sink.close()
            self.sink = None

    # -- evidencia --------------------------------------------------------
    def state_lines(self) -> list[str]:
        try:
            text = self.state_path.read_text(encoding="utf-8", errors="replace")
        except FileNotFoundError:
            return []
        self.cap["lines"] = [line for line in text.splitlines() if line.startswith("SHELL VE: ")]
        return self.cap["lines"]

    def sample(self):
        if self.win is None:
            return
        current = name_of(self.d, self.win)
        if current and (not self.cap["titles"] or self.cap["titles"][-1] != current):
            self.cap["titles"].append(current)

    def wait(self, pattern: re.Pattern[str]) -> bool:
        while time.monotonic() < self.deadline:
            lines = self.state_lines()
            for index in range(self.cursor, len(lines)):
                if pattern.match(lines[index]):
                    self.cursor = index + 1
                    self.sample()
                    return True
            self.sample()
            if self.proc.poll() is not None:
                lines = self.state_lines()
                for index in range(self.cursor, len(lines)):
                    if pattern.match(lines[index]):
                        self.cursor = index + 1
                        self.sample()
                        return True
                return False
            time.sleep(0.05)
        self.sample()
        return False

    # -- secuencia --------------------------------------------------------
    def open(self) -> bool:
        win = find_window(self.d, self.deadline, self.proc)
        if win is None:
            self.cap["abort"] = "window"
            return False
        self.win = win
        self.cap["window"] = True
        self.cap["alive"] = 1 if self.proc.poll() is None else 0
        activate(self.d, self.win, self.X)
        self.sample()  # SH-01: primera muestra, antes de cualquier tecla
        # La etiqueta `SHELL BOOT` puede escribirse uno o dos frames después del mapeo:
        # se espera (acotado por el deadline global, sin teclas) a la primera etiqueta.
        settle = min(self.deadline, time.monotonic() + 10.0)
        while time.monotonic() < settle and not boot_title(self.cap):
            time.sleep(0.1)
            self.sample()
            if self.proc.poll() is not None:
                break
        return True

    def run_sequence(self):
        for stage, key, pattern in SEQUENCE:
            if key is not None:
                press(self.d, self.win, self.X, self.XK, self.xevent, key)
                self.cap["keys"] += 1
            if not self.wait(pattern):
                self.cap["abort"] = stage
                return
        try:
            self.proc.wait(timeout=15)
        except Exception:
            pass

    def run_control(self):
        """CTRL-no-input: 0 teclas; el título no debe llegar a SHELL PLAYING."""
        while time.monotonic() < self.deadline:
            self.sample()
            if self.proc.poll() is not None:
                break
            time.sleep(0.1)
        self.sample()


# ── registro normalizado de una corrida (comparación entre procesos) ──────
def normalize(cap: dict) -> str:
    labels = labels_of(cap["titles"])
    body = "\n".join(cap["lines"])
    return "\n".join(
        (
            "window=%d" % (1 if cap["window"] else 0),
            "alive=%d" % (1 if cap.get("alive") else 0),
            "labels=%s" % (",".join(labels) or "-"),
            "lines=%d" % len(cap["lines"]),
            "lines_sha256=%s" % hashlib.sha256(body.encode("utf-8")).hexdigest(),
            "keys=%d" % cap["keys"],
            "abort=%s" % (cap["abort"] or "-"),
            "exit=%s" % (cap["exit"] if cap["exit"] is not None else "none"),
        )
    )


# ── render ────────────────────────────────────────────────────────────────
def render(row: Row, result: tuple, tag: str = "") -> str:
    ok, facts, symptom, found = result
    prefix = "[%s%s]" % ("OK" if ok else "FAIL", tag)
    if ok:
        return "%s %s %s - %s src=%s" % (prefix, row.rid, row.name, facts, row.anchor)
    token = "missing_evidence" if not found else "evidence_mismatch"
    return "%s %s %s - %s %s expected=%s (%s) src=%s" % (
        prefix, row.rid, row.name, token, facts, row.expected, symptom, row.anchor,
    )


def list_lines() -> list[str]:
    out = []
    for row in ROWS:
        out.append(
            "%s cov=%d %s keys=%s failure=%s expected=%s src=%s"
            % (
                row.rid,
                row.cov,
                row.name,
                ",".join(row.keys) if row.keys else "none",
                ",".join(row.failures),
                row.expected,
                row.anchor,
            )
        )
    out.append(
        "%s cov=0 %s keys=none failure=%s expected=%s src=%s"
        % (CTRL.rid, CTRL.name, ",".join(CTRL.failures), CTRL.expected, CTRL.anchor)
    )
    return out


def emit(lines: list[str], out_path: Path | None) -> int:
    """Imprime, sella con DIGEST y archiva. `DIGEST` cubre todo lo anterior."""
    text = "\n".join(lines) + "\n"
    sealed = text + "DIGEST %s\n" % digest(text)
    if out_path is not None:
        out_path.parent.mkdir(parents=True, exist_ok=True)
        out_path.write_text(sealed, encoding="utf-8")
    sys.stdout.write(sealed)
    return 0


# ── modos ─────────────────────────────────────────────────────────────────
def mode_list() -> int:
    lines = ["gate=%s cr=%s mode=list rows=%d" % (GATE, CR, len(ROWS))]
    source_line, missing = sources_line()
    lines.append(source_line)
    lines.extend(list_lines())
    anchors, unresolved = anchor_check()
    lines.append(
        "[%s] PROC-sources - declared=%d missing=%d anchors=%d unresolved=%d src=%s"
        % (
            "OK" if not missing and not unresolved else "FAIL",
            len(MEASURED_SOURCES),
            len(missing),
            anchors,
            len(unresolved),
            ANCHOR_GATE,
        )
    )
    passed = not missing and not unresolved
    # `--list` no verifica el producto: su token no puede confundirse con el veredicto del gate.
    lines.append("VEREDICTO-LIST: %s (catalogo; el veredicto del producto solo lo emite el modo run)"
                 % ("OK" if passed else "BROKEN"))
    emit(lines, None)
    return 0 if passed else 1


def mode_self_test() -> int:
    lines = ["gate=%s cr=%s mode=self-test fixture=contract-lines" % (GATE, CR)]
    source_line, _missing = sources_line()
    lines.append(source_line)

    results = evaluate(fixture_capture())
    rows_ok = len(results) == len(ROWS) and all(result[0] for _, result in results)
    for row, result in results:
        lines.append(render(row, result, "-FIXTURE"))

    caught = 0
    for name, target, mutate in FALSIFIERS:
        cap = fixture_capture()
        mutation = mutate(cap)
        red = [row.rid for row, result in evaluate(cap) if not result[0]]
        ok = target in red and bool(red)
        caught += 1 if ok else 0
        lines.append(
            "[%s] FALSIFY %s - row=%s red_rows=%s mutation=%s src=%s"
            % ("CAUGHT" if ok else "MISSED", name, target, ",".join(red) or "-", mutation, ANCHOR_GATE)
        )

    guards_caught = 0
    for name, target, mutate in GUARDS:
        cap = fixture_capture()
        mutation = mutate(cap)
        red = [row.rid for row, result in evaluate(cap) if not result[0]]
        ok = target in red and bool(red)
        guards_caught += 1 if ok else 0
        lines.append(
            "[%s] GUARD %s - row=%s red_rows=%s mutation=%s src=%s"
            % ("CAUGHT" if ok else "MISSED", name, target, ",".join(red) or "-", mutation, ANCHOR_GATE)
        )

    green_ok = 1 if rows_ok else 0
    passed = rows_ok and caught == len(FALSIFIERS) and guards_caught == len(GUARDS)
    lines.append(
        "[%s] GATE-SELFTEST - rows=%d green=%d falsifiers=%d caught=%d src=%s"
        % ("OK" if passed else "FAIL", len(ROWS), green_ok, len(FALSIFIERS), caught, ANCHOR_GATE)
    )
    lines.append(
        "GATE-SELFTEST rows=%d green=%d falsifiers=%d caught=%d"
        % (len(ROWS), green_ok, len(FALSIFIERS), caught)
    )
    lines.append("GATE-GUARDS guards=%d caught=%d" % (len(GUARDS), guards_caught))
    lines.append("VEREDICTO-GATE: %s" % ("PASS" if passed else "FAIL"))
    emit(lines, None)
    return 0 if passed else 1


def mode_falsify(name: str) -> int:
    lines = ["gate=%s cr=%s mode=falsify name=%s" % (GATE, CR, name)]
    source_line, _missing = sources_line()
    lines.append(source_line)
    known = [falsifier for falsifier, _, _ in FALSIFIERS]
    mutation = "unknown falsifier"
    target = ""
    red: list[str] = []
    ok = False
    if name in known:
        cap = fixture_capture()
        for falsifier, target, mutate in FALSIFIERS:
            if falsifier == name:
                mutation = mutate(cap)
                break
        red = [row.rid for row, result in evaluate(cap) if not result[0]]
        ok = target in red and bool(red)
        for row, result in evaluate(cap):
            lines.append(render(row, result, "-MUTATED"))
    lines.append(
        "[%s] FALSIFY %s - row=%s red_rows=%s mutation=%s src=%s"
        % ("CAUGHT" if ok else "MISSED", name, target or "-", ",".join(red) or "-", mutation, ANCHOR_GATE)
    )
    lines.append("VEREDICTO-GATE: %s" % ("PASS" if ok else "FAIL"))
    emit(lines, None)
    return 0 if ok else 1


def mode_run(args) -> int:
    binary = Path(args.binary).resolve()
    data_root = Path(args.data_root).expanduser().resolve()
    out = Path(args.out).resolve()
    out.mkdir(parents=True, exist_ok=True)
    source_line, missing = sources_line()
    stdout_path = out / "stdout.txt"

    def io_failure(reason: str, error_row: str) -> int:
        rows = ["gate=%s cr=%s mode=run" % (GATE, CR), source_line]
        rows.append("[FAIL] PROC-sources - missing=%d casualty=%s src=%s" % (len(missing), reason, ANCHOR_GATE))
        rows.append("[FAIL] PROC-run - error=%s src=%s" % (error_row, ANCHOR_GATE))
        rows.append("VEREDICTO-GATE: FAIL")
        emit(rows, stdout_path)
        sys.stderr.write("NC: %s\n" % reason)
        return 2

    if not (data_root / "ARENA2").is_dir():
        return io_failure("arena2=absent", "no-arena2")
    if not args.skip_build:
        built, build_error = build()
        if not built:
            return io_failure("build=failed", "build")
    if not binary.is_file():
        return io_failure("binary=absent", "no-binary")

    try:
        X, XK, display_module, xevent = _x11()
        d = display_module.Display()
    except Exception as exc:
        return io_failure("display=absent", "no-display")

    # Frescura: `crates/**` más nuevo que el binario ⇒ STALE (exit 3).
    binary_mtime = int(binary.stat().st_mtime)
    newest_mtime, newest_path = newest_source()
    stale = newest_mtime > binary_mtime
    anchors, unresolved = anchor_check()
    sources_ok = not missing and not stale and not unresolved
    sources_facts = "declared=%d missing=%d stale=%d anchors=%d unresolved=%d newest=%s" % (
        len(MEASURED_SOURCES),
        len(missing),
        1 if stale else 0,
        anchors,
        len(unresolved),
        newest_path if stale else "-",
    )

    captures: list[dict] = []
    for index in (1, 2):
        session = Session(
            d, X, XK, xevent, binary, data_root,
            out / ("run%d.state" % index), out / ("run%d.app.log" % index), args.timeout,
        )
        session.start()
        try:
            if session.open():
                session.run_sequence()
        finally:
            session.stop()
        captures.append(session.cap)

    control = Session(
        d, X, XK, xevent, binary, data_root,
        out / "ctrl.state", out / "ctrl.app.log", args.timeout,
    )
    control.start()
    try:
        if control.open():
            control.run_control()
    finally:
        control.stop()

    head = head_commit()
    binary_sha = hashlib.sha256(binary.read_bytes()).hexdigest()
    norm1, norm2 = normalize(captures[0]), normalize(captures[1])
    two_runs_ok = norm1 == norm2
    mirrored = mirrored_lines(out / "run1.app.log")
    mirror_ok = bool(captures[0]["lines"]) and mirrored == captures[0]["lines"]
    ctrl_result = check_ctrl(control.cap)
    rows = evaluate(captures[0])

    relative_binary = str(binary.relative_to(ROOT)) if str(binary).startswith(str(ROOT)) else str(binary)
    lines = [
        "gate=%s cr=%s head=%s" % (GATE, CR, head[:12]),
        "binary=%s sha256=%s" % (relative_binary, binary_sha),
        source_line,
    ]
    for name, cap in (("run1", captures[0]), ("run2", captures[1]), ("ctrl", control.cap)):
        lines.append(
            "NOTE capture=%s window=%d lines=%d labels=%s keys=%d abort=%s exit=%s"
            % (
                name,
                1 if cap["window"] else 0,
                len(cap["lines"]),
                ",".join(labels_of(cap["titles"])) or "-",
                cap["keys"],
                cap["abort"] or "-",
                cap["exit"] if cap["exit"] is not None else "none",
            )
        )
    for row, result in rows:
        lines.append(render(row, result))
    lines.append(render(CTRL, ctrl_result))
    lines.append(
        "[%s] PROC-sources - %s src=%s" % ("OK" if sources_ok else "FAIL", sources_facts, ANCHOR_GATE)
    )
    lines.append(
        "[%s] PROC-two-runs - identical=%d sha256=%s run1_lines=%d run2_lines=%d src=%s"
        % (
            "OK" if two_runs_ok else "FAIL",
            1 if two_runs_ok else 0,
            digest(norm1)[:16],
            len(captures[0]["lines"]),
            len(captures[1]["lines"]),
            ANCHOR_GATE,
        )
    )
    lines.append(
        "[%s] PROC-mirror - state=%d stdout=%d identical=%d src=%s"
        % (
            "OK" if mirror_ok else "FAIL",
            len(captures[0]["lines"]),
            len(mirrored),
            1 if mirrored == captures[0]["lines"] else 0,
            ANCHOR_GATE,
        )
    )
    ok = (
        all(result[0] for _, result in rows)
        and ctrl_result[0]
        and sources_ok
        and two_runs_ok
        and mirror_ok
    )
    lines.append("VEREDICTO-GATE: %s" % ("PASS" if ok else "FAIL"))

    manifest = {
        "gate": GATE,
        "cr": CR,
        "head": head,
        "binary": relative_binary,
        "binary_sha256": binary_sha,
        "sources": source_line,
        "sources_missing": missing,
        "stale": stale,
        "timeout_s": int(args.timeout),
        "captures": {"run1": norm1, "run2": norm2, "ctrl": normalize(control.cap)},
        "rows": [
            {
                "id": row.rid,
                "cov": row.cov,
                "name": row.name,
                "ok": result[0],
                "facts": result[1],
                "expected": row.expected,
                "found": result[3],
            }
            for row, result in rows
        ],
        "control": {"id": CTRL.rid, "ok": ctrl_result[0], "facts": ctrl_result[1]},
        "mirror": {"ok": mirror_ok, "state_lines": len(captures[0]["lines"]), "stdout_lines": len(mirrored)},
        "ok": ok,
    }
    (out / "manifest.json").write_text(
        json.dumps(manifest, indent=2, sort_keys=True) + "\n", encoding="utf-8"
    )

    if args.json:
        sys.stdout.write(json.dumps(manifest, indent=2, sort_keys=True) + "\n")
    emit(lines, stdout_path)
    if stale:
        sys.stderr.write("STALE: binary older than crates/** (%s)\n" % newest_path)
        return 3
    return 0 if ok else 1


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--binary", default=str(DEFAULT_BINARY), help="binario de producto")
    parser.add_argument("--skip-build", action="store_true", help="no reconstruir el binario")
    parser.add_argument("--data-root", default=str(DEFAULT_DATA_ROOT), help="datos retail (ARENA2)")
    parser.add_argument("--out", default=str(DEFAULT_OUT), help="directorio de artefactos")
    parser.add_argument("--json", action="store_true", help="resumen máquina")
    parser.add_argument("--list", action="store_true", help="una línea por fila (keys= y failure=)")
    parser.add_argument("--falsify", metavar="NAME", help="fixture + una mutación: exige rojo")
    parser.add_argument("--self-test", action="store_true", help="fixture verde + 11 falsificadores")
    parser.add_argument("--timeout", type=float, default=60.0, help="deadline global por proceso (s)")
    args = parser.parse_args(argv)

    if args.list:
        return mode_list()
    if args.self_test:
        return mode_self_test()
    if args.falsify:
        return mode_falsify(args.falsify)
    return mode_run(args)


if __name__ == "__main__":
    sys.exit(main())
