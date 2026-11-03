#!/usr/bin/env python3
"""DUN-CONTENT-001 — gate de aceptación del bloque D de M44 (contenido de mazmorra).

CR-7fd7260f-ba3d-449a-aea9-e74386a2d5a3 · bloque D (W44-4) + adversariales W44-6
spec: docs/sintergic/m44/blocks/D-content-CR-7fd7260f.md

Requisitos que este instrumento mide (M44-R14..R19):

  R14  tablas de botín por tipo de mazmorra y nivel, con 12 golden vectors
       (tabla + tirada -> digest del contenedor esperado)
  R15  cofre != drop != jefe: tres fuentes, tres tablas (digest distinto)
  R16  T_respawn exacto 20 / 40 / 30 minutos, tolerancia declarada 0 ticks
  R17  cinco reglas anti-farm, cada una con falsificador nombrado (--falsify)
  R18  contrato «nunca saqueada al entrar»: 3 mazmorras x 3 epochs = 9 celdas
       + el único camino de jugador de M44 (binario de producto -> PLAYING)
  R19  instancias: instance_id aísla estado (2 instancias de la misma location)
  ADV-01..03  adversariales de W44-6 (2 jugadores, mismo contenedor / epoch)

Canal de producto que la implementación futura debe publicar (append + flush en
$DF_DUN_STATE, espejado en stdout, una línea por hecho).  Este gate NO inventa
otro canal: lee el fichero que el binario escriba y, cuando no hay fichero, mide
el rojo con el número real (0 líneas, 0 bytes) de una sesión que sí arrancó.

  DUN VE: loot id=<key>-<level> key=<K> level=<n> rolls=<n> digest=<16hex>
  DUN VE: loot_vector id=<key>-<level>-<roll> key=<K> level=<n> roll=<n> expected=<16hex> actual=<16hex>
  DUN VE: source id=<Chest|Drop|Boss> source=<s> digest=<16hex> rolls=<n>
  DUN VE: source_distinct id=<id> sources=3 distinct=3 shared=0
  DUN VE: respawn id=<s> source=<s> minutes=<n> tolerance_ticks=0
  DUN VE: antifarm id=r<n> rule=<n> name=<slug> container=<id> epoch=<n> <métrica>=<n> verdict=ok
  DUN VE: entry id=<loc>-<epoch> location=<loc> epoch=<n> looted=0 evidence=<16hex>
  DUN VE: instance slot=<a|b> instance=<16hex> location=<loc> epoch=<n> isolated=1
  DUN VE: instance_isolation a=<16hex> b=<16hex> shared=0
  DUN VE: adversarial id=ADV-0<n> players=<n> containers=<n> instances=<n> epochs=<n> grants=<n> shared=<n>

Gramática de fila (familia shell-001 / sys-*-001):

  [OK]   <ID> - <k>=<v> ... src=<path>:<line>
  [FAIL] <ID> - <token> <hechos> expected=<v> (<síntoma>) src=<path>:<line>
  [NC]   <ID> - <token> <hechos> expected=<v> (<síntoma>) (G12 <razón>) src=<path>:<line>
  [NOT-RUN] <razón mecánica>

Cierre: imprime `VERDICT: PASS|FAIL|NC` y `DIGEST <sha256>` sobre todo lo impreso
antes de esa línea.  Exit 0/1/2.  Hoy el veredicto esperado es FAIL: el gate se
escribe contra el contrato, no contra el estado actual.
"""

from __future__ import annotations

import argparse
import importlib.util
import json
import os
import re
import subprocess
import sys
import time
from dataclasses import dataclass, replace
from pathlib import Path
from typing import Callable, Sequence

ROOT = Path(__file__).resolve().parents[1]
SCRIPTS = ROOT / "scripts"
if str(SCRIPTS) not in sys.path:
    sys.path.insert(0, str(SCRIPTS))

import m43_gate_lib as lib  # noqa: E402  — librería de gates del ciclo M43 (no se copia)

GATE = "DUN-CONTENT-001"
BLOCK = "D"
CR = "CR-7fd7260f-ba3d-449a-aea9-e74386a2d5a3"
SPEC = "docs/sintergic/m44/blocks/D-content-CR-7fd7260f.md"
OWNER = "scripts/dun-content-001.py"
DRIVER = "scripts/shell-001.py"
DEFAULT_BINARY = ROOT / "target" / "debug" / "daggerfall-app"
DEFAULT_OUT = ROOT / ".sintergic" / "artifacts" / "dun-content-001"
BUILD_LOCK = ROOT / ".sintergic" / "locks" / "cargo-build.lock"
DEFAULT_TIMEOUT = 90

PROBE = "LOOT-001"
# Medido con `./target/debug/daggerfall-app --probe LOOT-001` (seed por defecto
# 20261021, 500 contenedores key=K level=5): el vector golden de la librería de
# botín M21 que las tablas de mazmorra deben reproducir.
LOOT_LIBRARY_GOLDEN = "795e51a427c66cf21d73cd60919ed880e89391d2675ac10b813837a2182a4fb3"
LOOT_LIBRARY_CHECKS = 3

# Puerta retail alcanzable con `--door-spawn` (block 55).  Es la ÚNICA entrada
# que el producto sabe abrir hoy: entra a un interior, nunca a una mazmorra.
ENTRY_BLOCK = "N0000055.RDB"

EXIT_GREEN, EXIT_RED, EXIT_NC = 0, 1, 2

MEASURED_SOURCES = (
    "crates/inventory/src/loot.rs",
    "crates/app/src/shell.rs",
    "crates/app/src/main.rs",
    "crates/app/src/m18_doors.rs",
    "crates/app/src/dungeon_probe.rs",
    "crates/memory/src/savegame_v2.rs",
    "crates/heart/src/clock.rs",
    "crates/heart/src/session.rs",
    "scripts/m43_gate_lib.py",
    DRIVER,
)

# ── contrato declarado (constantes del bloque D) ───────────────────────────

# 19 tipos de mazmorra en DUNGEON_KEYS; tres familias elegidas para el contrato:
# 0 Crypt -> K, 5 Mine -> M, 12 Spider Nest -> L.
DUNGEON_TYPES = ((0, "K"), (5, "M"), (12, "L"))
ROLL_SPACE = 48
LOOT_TABLES = tuple((key, level) for _, key in DUNGEON_TYPES for level in (1, 10))
LOOT_VECTORS = tuple(
    (key, level, roll) for key, level in LOOT_TABLES for roll in (0, ROLL_SPACE - 1)
)
SOURCES = ("Chest", "Drop", "Boss")
RESPAWN_MINUTES = {"Chest": 20, "Drop": 40, "Boss": 30}
TOLERANCE_TICKS = 0
# (regla, slug, métrica, valor exigido).  Métrica por regla:
#   1 sin re-loot del mismo contenedor en la misma epoch        -> grants=1
#   2 el respawn no ocurre mientras el jugador está dentro      -> respawns=0
#   3 el jefe no da botín dos veces por epoch                   -> grants=1
#   4 lockout global por contenedor entre jugadores del epoch   -> grants=1
#   5 el botín no escala con el número de entradas del epoch    -> grants_per_entry=1
ANTIFARM = (
    (1, "no-reloot-same-epoch", "grants", 1),
    (2, "no-respawn-while-inside", "respawns", 0),
    (3, "boss-single-loot-per-epoch", "grants", 1),
    (4, "container-lockout-cross-player", "grants", 1),
    (5, "loot-not-scaled-by-entries", "grants_per_entry", 1),
)
ENTRY_LOCATIONS = (0, 1, 2)
ENTRY_EPOCHS = (1, 2, 3)
ADVERSARIAL = (
    ("ADV-01", {"players": 2, "containers": 1, "instances": 1, "epochs": 1, "grants": 1, "shared": 0}),
    ("ADV-02", {"players": 2, "containers": 1, "instances": 1, "epochs": 2, "grants": 2, "shared": 0}),
    ("ADV-03", {"players": 2, "containers": 1, "instances": 2, "epochs": 1, "grants": 2, "shared": 0}),
)

ADVERSARIAL_BY_ID = {adv_id: payload for adv_id, payload in ADVERSARIAL}

# Anclas: cada fila cita dónde vive HOY la ausencia o la primitiva.  Todas se
# verifican contra disco (`citation_check`) en cada corrida.
A_MATRICES = "crates/inventory/src/loot.rs:121"
A_KEYS = "crates/inventory/src/loot.rs:155"
A_KEYFN = "crates/inventory/src/loot.rs:179"
A_CONTAINER = "crates/inventory/src/loot.rs:861"
A_ENEMY = "crates/inventory/src/loot.rs:901"
A_DIGEST = "crates/inventory/src/loot.rs:926"
A_PROBE_LOOT = "crates/app/src/main.rs:585"
A_MIRROR = "crates/app/src/shell.rs:190"
A_DOOR = "crates/app/src/m18_doors.rs:661"
A_SCENE = "crates/app/src/m18_doors.rs:326"
A_DUN_PROBE = "crates/app/src/dungeon_probe.rs:35"
A_SAVE = "crates/memory/src/savegame_v2.rs:16"
A_SAVE_KEYS = "crates/memory/src/savegame_v2.rs:75"
A_CLOCK = "crates/heart/src/clock.rs:111"
A_EPOCH = "crates/heart/src/session.rs:219"

# ── regex del canal (una por familia de hecho) ─────────────────────────────

RE_LOOT = re.compile(
    r"^DUN VE: loot id=(?P<id>[A-Z]-\d+) key=(?P<key>[A-Z]) level=(?P<level>\d+) "
    r"rolls=(?P<rolls>\d+) digest=(?P<digest>[0-9a-f]{16})$",
    re.MULTILINE,
)
RE_VECTOR = re.compile(
    r"^DUN VE: loot_vector id=(?P<id>[A-Z]-\d+-\d+) key=(?P<key>[A-Z]) level=(?P<level>\d+) "
    r"roll=(?P<roll>\d+) expected=(?P<expected>[0-9a-f]{16}) actual=(?P<actual>[0-9a-f]{16})$",
    re.MULTILINE,
)
RE_SOURCE = re.compile(
    r"^DUN VE: source id=(?P<id>[A-Za-z]+) source=(?P<source>[A-Za-z]+) "
    r"digest=(?P<digest>[0-9a-f]{16}) rolls=(?P<rolls>\d+)$",
    re.MULTILINE,
)
RE_SOURCE_DISTINCT = re.compile(
    r"^DUN VE: source_distinct id=(?P<id>\S+) sources=(?P<sources>\d+) "
    r"distinct=(?P<distinct>\d+) shared=(?P<shared>\d+)$",
    re.MULTILINE,
)
RE_RESPAWN = re.compile(
    r"^DUN VE: respawn id=(?P<id>[A-Za-z]+) source=(?P<source>[A-Za-z]+) "
    r"minutes=(?P<minutes>\d+) tolerance_ticks=(?P<tolerance_ticks>\d+)$",
    re.MULTILINE,
)
RE_ANTIFARM_BASE = re.compile(
    r"^DUN VE: antifarm id=(?P<id>r\d+) rule=(?P<rule>\d+) name=(?P<name>[a-z0-9-]+) "
    r"container=(?P<container>\S+) epoch=(?P<epoch>\d+) (?P<rest>\S+) verdict=(?P<verdict>\w+)$",
    re.MULTILINE,
)
RE_ENTRY = re.compile(
    r"^DUN VE: entry id=(?P<id>\d+-\d+) location=(?P<location>\d+) epoch=(?P<epoch>\d+) "
    r"looted=(?P<looted>\d+) evidence=(?P<evidence>[0-9a-f]{16})$",
    re.MULTILINE,
)
RE_INSTANCE = re.compile(
    r"^DUN VE: instance slot=(?P<slot>[ab]) instance=(?P<instance>[0-9a-f]{16}) "
    r"location=(?P<location>\d+) epoch=(?P<epoch>\d+) isolated=(?P<isolated>\d)$",
    re.MULTILINE,
)
RE_INSTANCE_ISO = re.compile(
    r"^DUN VE: instance_isolation a=(?P<a>[0-9a-f]{16}) b=(?P<b>[0-9a-f]{16}) "
    r"shared=(?P<shared>\d)$",
    re.MULTILINE,
)
RE_ADVERSARIAL = re.compile(
    r"^DUN VE: adversarial id=(?P<id>ADV-\d+) players=(?P<players>\d+) "
    r"containers=(?P<containers>\d+) instances=(?P<instances>\d+) epochs=(?P<epochs>\d+) "
    r"grants=(?P<grants>\d+) shared=(?P<shared>\d+)$",
    re.MULTILINE,
)


def re_rule(rule: int, metric: str) -> re.Pattern[str]:
    """Regex de UNA regla anti-farm: la métrica que la regla acota es un grupo."""
    return re.compile(
        rf"^DUN VE: antifarm id=r{rule} rule={rule} name=(?P<name>[a-z0-9-]+) "
        rf"container=(?P<container>\S+) epoch=(?P<epoch>\d+) "
        rf"{metric}=(?P<{metric}>\d+) verdict=(?P<verdict>\w+)$",
        re.MULTILINE,
    )


# ── comprobadores (los de la librería + dos locales declarados) ─────────────
#
# Delta declarado frente a `m43_gate_lib`: la librería no tiene (a) «todos los
# matches de un grupo valen lo mismo» — `every` exige cardinalidad igual a la
# lista — ni (b) «N valores distintos de un grupo».  Se añaden aquí, con la
# misma forma `(matches) -> (ok, detalle)`; no se bifurca la librería.


def all_equal(group: str, value: str) -> Callable[[list[re.Match]], tuple[bool, str]]:
    def check(matches: list[re.Match]) -> tuple[bool, str]:
        if not matches:
            return False, f"{group}=absent"
        bad = [match[group] for match in matches if match[group] != value]
        return (not bad), (
            f"rows={len(matches)} {group}={value} mismatch={','.join(bad[:4]) or 'none'}"
        )

    return check


def distinct_group(group: str, want: int) -> Callable[[list[re.Match]], tuple[bool, str]]:
    def check(matches: list[re.Match]) -> tuple[bool, str]:
        values = [match[group] for match in matches]
        unique = sorted(set(values))
        ok = len(unique) == want and len(values) == want
        return ok, (
            f"{group}_distinct={len(unique)}/{want} rows={len(values)} "
            f"values={','.join(item[:8] for item in unique) or 'none'}"
        )

    return check


def stable_across(pattern: re.Pattern[str], group: str, ctx: "Ctx") -> tuple[bool, str]:
    """El mismo hecho publicado por sesiones independientes debe ser idéntico.

    Sin esto un generador aleatorio pasaría `expected==actual` publicando ambas
    mitades de la misma tirada: la referencia independiente es la OTRA sesión.
    """
    maps = [
        {match["id"]: match[group] for match in matches_of(pattern, text)}
        for text in (ctx.dun, *ctx.alts)
    ]
    same = all(item == maps[0] for item in maps)
    return same, f"sessions={len(maps)} stable={int(same)}"


def combine(*parts: tuple[bool, str]) -> tuple[bool, str]:
    ok = all(part[0] for part in parts)
    return ok, " · ".join(part[1] for part in parts)


def matches_of(pattern: re.Pattern[str], text: str) -> list[re.Match]:
    return list(pattern.finditer(text))


# ── catálogo de filas ──────────────────────────────────────────────────────


@dataclass(frozen=True)
class Row:
    rid: str
    name: str
    anchor: str
    token: str
    expected: str
    symptom: str
    kind: str  # channel | census | probe | player | harness


ROWS: tuple[Row, ...] = (
    Row("M44-R14", "tablas de botín por tipo y nivel", A_MATRICES, "missing_evidence",
        "loot id=K-1,K-10,M-1,M-10,L-1,L-10 rolls=48 digest=<16hex>",
        "ninguna línea `DUN VE: loot` en $DF_DUN_STATE tras una sesión que arrancó", "channel"),
    Row("M44-R14a", "12 golden vectors tabla+tirada -> digest", A_KEYS, "missing_evidence",
        "loot_vector id=<key>-<level>-<roll> expected==actual (12 vectores)",
        "la tabla publica rolls pero ningún vector tabla+tirada", "channel"),
    Row("M44-R14b", "golden de librería heredado (--probe LOOT-001)", A_PROBE_LOOT, "golden_drift",
        f"digest={LOOT_LIBRARY_GOLDEN[:16]}… ×2 identical · checks={LOOT_LIBRARY_CHECKS} exit=0",
        "el golden de la librería de botín no reproduce byte a byte", "probe"),
    Row("M44-R14c", "el camino de mazmorra consume la tabla", A_KEYFN, "census_short",
        "dungeon_path_callsites>=1 (generate_container|generate_enemy_loot en *dungeon*/*interior*)",
        "19 claves de mazmorra y 0 consumidores en el camino de mazmorra", "census"),
    Row("M44-R15", "cofre≠drop≠jefe: tres tablas distintas", A_CONTAINER, "missing_evidence",
        "source id=Chest|Drop|Boss · distinct=3 shared=0",
        "no hay tres fuentes separadas con digest de tabla propio", "channel"),
    Row("M44-R15a", "existe fuente de botín de jefe", A_ENEMY, "census_short", "boss_sources>=1",
        "el motor sólo tiene contenedor y enemigo; el jefe no tiene fuente propia", "census"),
    Row("M44-R16", "T_respawn 20/40/30 min exactos", A_CLOCK, "missing_evidence",
        "respawn Chest=20 Drop=40 Boss=30 tolerance_ticks=0",
        "ninguna línea `DUN VE: respawn` y 0 tokens respawn en el motor", "channel"),
    Row("M44-R16a", "existe respawn en el motor", A_CLOCK, "census_short", "respawn_tokens>=1",
        "el reloj sabe contar minutos pero nada respawnea", "census"),
    Row("M44-R17", "cinco reglas anti-farm con veredicto", A_SAVE, "missing_evidence",
        "antifarm id=r1..r5 name=<slug> verdict=ok",
        "ninguna regla anti-farm medida por el producto", "channel"),
    Row("M44-R17-r1", "regla 1 sin re-loot en la misma epoch", A_SAVE, "missing_evidence",
        "antifarm rule=1 grants=1 verdict=ok", "re-loot del mismo contenedor no acotado", "channel"),
    Row("M44-R17-r2", "regla 2 respawn no ocurre estando dentro", A_SAVE, "missing_evidence",
        "antifarm rule=2 respawns=0 verdict=ok", "el respawn no está condicionado a la presencia", "channel"),
    Row("M44-R17-r3", "regla 3 el jefe no da botín dos veces", A_SAVE, "missing_evidence",
        "antifarm rule=3 grants=1 verdict=ok", "el jefe no tiene lockout por epoch", "channel"),
    Row("M44-R17-r4", "regla 4 lockout por contenedor entre jugadores", A_SAVE, "missing_evidence",
        "antifarm rule=4 grants=1 verdict=ok", "el contenedor no está bloqueado entre jugadores", "channel"),
    Row("M44-R17-r5", "regla 5 el botín no escala con las entradas", A_SAVE, "missing_evidence",
        "antifarm rule=5 grants_per_entry=1 verdict=ok", "el botín no está desacoplado del conteo de entradas", "channel"),
    Row("M44-R17a", "las reglas existen en el motor", A_SAVE, "census_short", "antifarm_tokens>=1",
        "cero tokens de anti-farm en crates/**", "census"),
    Row("M44-R18", "nunca saqueada al entrar: 3 mazmorras × 3 epochs", A_DUN_PROBE, "missing_evidence",
        "entry id=<loc>-<epoch> looted=0 evidence=<16hex> · 9 celdas",
        "0 de 9 celdas con evidencia de entrada", "channel"),
    Row("M44-R18a", "camino de jugador: el producto no publica mazmorra", A_DOOR, "player_path_missing",
        "playing=1 interior=1 dungeon_scene>=1 dun_lines>=1",
        "la sesión llega a PLAYING y entra al interior retail, pero no hay mazmorra ni canal", "player"),
    Row("M44-R19", "instance_id aísla estado (2 instancias)", A_SAVE_KEYS, "missing_evidence",
        "instance slot=a|b instance=<16hex> isolated=1 · instance_isolation shared=0",
        "no hay instancias separadas de la misma location", "channel"),
    Row("M44-R19a", "familias de mazmorra persistidas en save v3", A_SAVE, "census_short",
        "dungeon_families>=1 (dungeon_state|dungeon_loot|dungeon_elig|dungeon_instance)",
        "el save no declara ninguna familia de mazmorra", "census"),
    Row("ADV-01", "2 jugadores mismo contenedor y epoch", A_EPOCH, "missing_evidence",
        "adversarial id=ADV-01 players=2 containers=1 epochs=1 grants=1 shared=0",
        "no existe contenedor compartido entre jugadores", "channel"),
    Row("ADV-02", "mismo contenedor en 2 epochs", A_EPOCH, "missing_evidence",
        "adversarial id=ADV-02 players=2 epochs=2 grants=2 shared=0",
        "no hay epoch que reinicie el contenedor", "channel"),
    Row("ADV-03", "2 instancias mismo contenedor", A_EPOCH, "missing_evidence",
        "adversarial id=ADV-03 players=2 instances=2 grants=2 shared=0",
        "sin instancias, dos sesiones no pueden aislarse", "channel"),
)

ROW_BY_ID = {row.rid: row for row in ROWS}

# ── hechos de sesión (camino de jugador) ───────────────────────────────────


@dataclass
class Leg:
    index: int
    window: int = 0
    playing: int = 0
    interior: int = 0
    dungeon_scene: int = 0
    door_registry: int = 0
    keys: int = 0
    scene_final: str = "-"
    abort: str = "-"
    dun_text: str = ""
    shell_text: str = ""
    app_text: str = ""
    dun_bytes: int = 0
    dun_lines: int = 0

    @property
    def record(self) -> str:
        return (
            f"window={self.window} playing={self.playing} interior={self.interior} "
            f"dungeon_scene={self.dungeon_scene} dun_lines={self.dun_lines} "
            f"dun_bytes={self.dun_bytes} scene={self.scene_final} "
            f"abort={self.abort}"
        )

    @property
    def abort_reason(self) -> str | None:
        return None if self.abort in ("-", "") else self.abort


def load_driver():
    """Importa el driver X11 ya probado (`scripts/shell-001.py`) — no se reinventa."""
    path = ROOT / DRIVER
    spec = importlib.util.spec_from_file_location("shell001_driver", path)
    if spec is None or spec.loader is None:
        raise SystemExit(f"[NOT-RUN] no se pudo cargar el driver {DRIVER} (G12)")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def play_leg(index: int, binary: Path, data_root: Path, out_dir: Path, deadline: float) -> Leg:
    """Una sesión real de jugador: arranca el binario, llega a PLAYING, entra por la puerta."""
    leg = Leg(index=index)
    legdir = out_dir / f"leg{index}"
    legdir.mkdir(parents=True, exist_ok=True)
    state = legdir / "state"
    dun = legdir / "dun"
    log = legdir / "app.log"
    state.write_text("", encoding="utf-8")
    dun.write_text("", encoding="utf-8")

    driver = load_driver()
    env = dict(os.environ)
    env["DF_APP_ROOT"] = str(data_root)
    env["DF_SHELL_STATE"] = str(state)
    env["DF_DUN_STATE"] = str(dun)
    env.setdefault("DISPLAY", ":0")

    sink = log.open("wb")
    proc = subprocess.Popen(
        [str(binary), "--door-spawn"], cwd=str(ROOT), env=env,
        stdout=sink, stderr=subprocess.STDOUT,
    )
    try:
        X, XK, display_module, xtest = driver._x11()
        display = display_module.Display()
        window = driver.find_window(display, deadline, proc)
        if window is None:
            leg.abort = "no-window"
            return leg
        leg.window = 1

        def sample() -> str:
            # sólo para detectar PLAYING: el título de ventana es una muestra
            # instantánea y no se usa como evidencia (medido: run1 Exterior /
            # run2 Interior en dos sesiones ambos interior=1).
            return driver.name_of(display, window) or ""

        # PLAYING: re-activar antes de cada pulso (una ventana sin foco se come
        # las teclas — medido: 4 Return perdidos sin re-activar).
        for attempt in range(8):
            if time.monotonic() > deadline:
                break
            title = sample()
            if "SHELL PLAYING" in title:
                leg.playing = 1
                break
            if "SHELL MENU" in title or attempt >= 2:
                driver.activate(display, window, X)
                time.sleep(0.3)
                driver.press(display, window, X, XK, xtest, "Return")
                leg.keys += 1
            wait_until = min(deadline, time.monotonic() + 6)
            while time.monotonic() < wait_until:
                if "SHELL PLAYING" in sample():
                    leg.playing = 1
                    break
                time.sleep(0.1)
            if leg.playing:
                break
        if not leg.playing:
            leg.abort = "no-playing"

        # Entrada por la única puerta retail alcanzable (interior, no mazmorra).
        if leg.playing:
            for _ in range(6):
                if time.monotonic() > deadline:
                    leg.abort = "no-interior" if not leg.interior else leg.abort
                    break
                driver.activate(display, window, X)
                time.sleep(0.2)
                driver.press(display, window, X, XK, xtest, "e")
                leg.keys += 1
                wait_until = min(deadline, time.monotonic() + 6)
                while time.monotonic() < wait_until:
                    if "interact → interior" in log.read_text(errors="replace"):
                        leg.interior = 1
                        break
                    time.sleep(0.1)
                if leg.interior:
                    break
            if not leg.interior and leg.abort == "-":
                leg.abort = "no-interior"
    except Exception as exc:  # noqa: BLE001 — se registra como aborto, nunca se silencia
        leg.abort = f"x11:{type(exc).__name__}"
    finally:
        proc.terminate()
        try:
            proc.wait(timeout=5)
        except subprocess.TimeoutExpired:
            proc.kill()
            proc.wait(timeout=5)
        sink.close()

    leg.dun_text = dun.read_text(errors="replace")
    leg.shell_text = state.read_text(errors="replace")
    leg.app_text = log.read_text(errors="replace")
    leg.dun_bytes = dun.stat().st_size
    leg.dun_lines = len([line for line in leg.dun_text.splitlines() if line.strip()])
    registry = re.search(r"door registry (\d+) door", leg.app_text)
    leg.door_registry = int(registry.group(1)) if registry else 0
    # Escena final derivada del REGISTRO (m18_doors.rs:661), no del título: el
    # título es una muestra racional del instante y cambió entre dos corridas
    # con el mismo `interior=1`.
    entered = re.search(r"FG01 VE: interact → interior (\S+)", leg.app_text)
    if entered is not None:
        leg.scene_final = f"Interior({entered.group(1)})"
    elif leg.playing:
        leg.scene_final = "Playing"
    else:
        leg.scene_final = "-"
    leg.dungeon_scene = (
        leg.app_text.count("scene=Dungeon")
        + leg.shell_text.count("scene=Dungeon")
        + leg.dun_text.count("scene=Dungeon")
    )
    return leg


def run_probe(binary: Path, timeout: int) -> dict:
    """Corre `--probe LOOT-001` dos veces: el golden de la librería debe ser estable."""
    digests: list[str] = []
    exits: list[int] = []
    checks = 0
    passed = 0
    for _ in range(2):
        try:
            done = subprocess.run(
                [str(binary), "--probe", PROBE], cwd=str(ROOT),
                capture_output=True, timeout=max(20, timeout),
            )
        except subprocess.TimeoutExpired:
            return {"runs": len(digests) + 1, "identical": 0, "exit": 1,
                    "digest": "", "checks": checks, "passed": passed}
        text = done.stdout.decode("utf-8", "replace")
        exits.append(done.returncode)
        found = re.findall(r"^digest=([0-9a-f]{64})$", text, re.MULTILINE)
        digests.append(found[0] if found else "")
        summary = re.search(r"checks=(\d+) passed=(\d+) failed=(\d+)", text)
        if summary is not None:
            checks = int(summary.group(1))
            passed = int(summary.group(2))
    identical = 1 if len(digests) == 2 and digests[0] == digests[1] and digests[0] else 0
    return {
        "runs": len(digests),
        "identical": identical,
        "exit": exits[0] if exits else 1,
        "digest": digests[0] if digests else "",
        "checks": checks,
        "passed": passed,
    }


# ── censo sobre disco (recuento re-ejecutable) ─────────────────────────────


def census(root: Path) -> dict[str, int]:
    """Recuento literal sobre `root/crates/**/*.rs` con el ámbito nombrado por fila."""
    results = {
        "matrices": 0,
        "dungeon_keys": 0,
        "dungeon_key_consumers": 0,
        "dungeon_path_callsites": 0,
        "boss_sources": 0,
        "respawn_tokens": 0,
        "antifarm_tokens": 0,
        "dungeon_families": 0,
        "save_count_keys": 0,
    }
    for path in sorted(root.glob("crates/**/*.rs")):
        try:
            text = path.read_text(errors="replace")
        except OSError:
            continue
        name = path.name
        if name == "loot.rs":
            results["matrices"] += len(re.findall(r"matrix_row\('", text))
            results["dungeon_keys"] += len(re.findall(r"^    '[A-Z]', // ", text, re.MULTILINE))
        else:
            if "dungeon_key(" in text:
                results["dungeon_key_consumers"] += 1
        results["boss_sources"] += len(re.findall(r"generate_boss_loot|boss_loot|BossLoot", text))
        results["respawn_tokens"] += len(re.findall(r"(?i)respawn", text))
        # Anti-farm por las CINCO slugs del contrato (sensibles a mayúsculas): un
        # `(?i)lockout` contaba 27 veces `LockOutcome`/`BlockOutOfBounds` de
        # crates/side-contract — cifra retirada por falsa (medido 2026-10-29).
        results["antifarm_tokens"] += sum(text.count(slug) for _, slug, _, _ in ANTIFARM)
        counted = re.search(r"COUNT_KEYS: usize = (\d+)", text)
        if counted is not None:
            results["save_count_keys"] = int(counted.group(1))
        results["dungeon_families"] += len(re.findall(r"dungeon_(?:state|loot|elig|instance)", text))
        if "dungeon" in name or "interior" in name:
            results["dungeon_path_callsites"] += len(
                re.findall(r"generate_container(?:_with_counts|_with_templates)?\(|generate_enemy_loot\(", text)
            )
    return results


# ── contexto y evaluación ──────────────────────────────────────────────────


@dataclass
class Ctx:
    dun: str
    facts: Leg
    counts: dict[str, int]
    probe: dict
    golden: str
    alts: list[str]


def check_row(row: Row, ctx: Ctx) -> tuple[bool, str]:
    rid = row.rid
    dun = ctx.dun
    if rid == "M44-R14":
        ids = [f"{key}-{level}" for key, level in LOOT_TABLES]
        matches = matches_of(RE_LOOT, dun)
        return combine(
            lib.every("id", ids)(matches),
            all_equal("rolls", str(ROLL_SPACE))(matches),
            stable_across(RE_LOOT, "digest", ctx),
        )
    if rid == "M44-R14a":
        ids = [f"{key}-{level}-{roll}" for key, level, roll in LOOT_VECTORS]
        matches = matches_of(RE_VECTOR, dun)
        return combine(
            lib.every("id", ids)(matches),
            lib.value_equal("id")(matches),
            stable_across(RE_VECTOR, "actual", ctx),
        )
    if rid == "M44-R14b":
        probe = ctx.probe
        ok = (
            probe["runs"] == 2
            and probe["identical"] == 1
            and probe["exit"] == 0
            and probe["digest"] == ctx.golden
            and probe["checks"] == LOOT_LIBRARY_CHECKS
        )
        return ok, (
            f"runs={probe['runs']} identical={probe['identical']} exit={probe['exit']} "
            f"checks={probe['checks']} digest={probe['digest'][:16] or 'absent'} "
            f"golden={ctx.golden[:16]} match={int(probe['digest'] == ctx.golden)}"
        )
    if rid == "M44-R14c":
        got = ctx.counts["dungeon_path_callsites"]
        return got >= 1, f"dungeon_path_callsites={got} dungeon_keys={ctx.counts['dungeon_keys']}"
    if rid == "M44-R15":
        sources = matches_of(RE_SOURCE, dun)
        distinct = matches_of(RE_SOURCE_DISTINCT, dun)
        return combine(
            lib.every("source", SOURCES)(sources),
            distinct_group("digest", len(SOURCES))(sources),
            lib.expect_equal(sources="3", distinct="3", shared="0")(distinct),
        )
    if rid == "M44-R15a":
        got = ctx.counts["boss_sources"]
        return got >= 1, f"boss_sources={got} sources_declared={len(SOURCES)}"
    if rid == "M44-R16":
        parts = []
        for source in SOURCES:
            rows = [m for m in matches_of(RE_RESPAWN, dun) if m["source"] == source]
            parts.append(
                lib.expect_equal(
                    source=source,
                    minutes=str(RESPAWN_MINUTES[source]),
                    tolerance_ticks=str(TOLERANCE_TICKS),
                )(rows)
            )
        parts.append(lib.every("source", SOURCES)(matches_of(RE_RESPAWN, dun)))
        return combine(*parts)
    if rid == "M44-R16a":
        got = ctx.counts["respawn_tokens"]
        return got >= 1, f"respawn_tokens={got}"
    if rid == "M44-R17":
        names = [slug for _, slug, _, _ in ANTIFARM]
        return combine(
            lib.every("name", names)(matches_of(RE_ANTIFARM_BASE, dun)),
            lib.all_vector_ok("id")(matches_of(RE_ANTIFARM_BASE, dun)),
        )
    if rid.startswith("M44-R17-r"):
        number = int(rid.rsplit("r", 1)[1])
        _, slug, metric, value = ANTIFARM[number - 1]
        matches = matches_of(re_rule(number, metric), dun)
        return lib.expect_equal(
            name=slug, verdict="ok", **{metric: str(value)}
        )(matches)
    if rid == "M44-R17a":
        got = ctx.counts["antifarm_tokens"]
        return got >= 1, f"antifarm_tokens={got} rules_declared={len(ANTIFARM)}"
    if rid == "M44-R18":
        ids = [
            f"{location}-{epoch}"
            for location in ENTRY_LOCATIONS
            for epoch in ENTRY_EPOCHS
        ]
        matches = matches_of(RE_ENTRY, dun)
        # el ancho del digest (16 hex) ya lo impone RE_ENTRY al emparejar la línea
        return combine(
            lib.every("id", ids)(matches),
            all_equal("looted", "0")(matches),
        )
    if rid == "M44-R18a":
        facts = ctx.facts
        ok = (
            facts.playing == 1
            and facts.interior == 1
            and facts.dungeon_scene >= 1
            and facts.dun_lines >= 1
        )
        return ok, (
            f"playing={facts.playing} interior={facts.interior} "
            f"dungeon_scene={facts.dungeon_scene} dun_lines={facts.dun_lines} "
            f"dun_bytes={facts.dun_bytes} door_registry={facts.door_registry} "
            f"scene={facts.scene_final}"
        )
    if rid == "M44-R19":
        instances = matches_of(RE_INSTANCE, dun)
        isolation = matches_of(RE_INSTANCE_ISO, dun)
        parts = [
            lib.every("slot", ("a", "b"))(instances),
            distinct_group("instance", 2)(instances),
            all_equal("isolated", "1")(instances),
            lib.expect_equal(shared="0")(isolation),
        ]
        if len(instances) == 2 and len(isolation) == 1:
            pair = {isolation[0]["a"], isolation[0]["b"]}
            seen = {instances[0]["instance"], instances[1]["instance"]}
            parts.append(
                (pair == seen, f"isolation_pair_matches={int(pair == seen)}")
            )
        else:
            parts.append((False, "isolation_pair_matches=0"))
        return combine(*parts)
    if rid == "M44-R19a":
        got = ctx.counts["dungeon_families"]
        return got >= 1, (
            f"dungeon_families={got} save_count_keys={ctx.counts['save_count_keys']}"
        )
    if rid.startswith("ADV-"):
        expected = dict(ADVERSARIAL_BY_ID[rid])
        rows = [m for m in matches_of(RE_ADVERSARIAL, dun) if m["id"] == rid]
        want = {key: str(value) for key, value in expected.items()}
        return lib.expect_equal(**want)(rows)
    return False, f"unknown-row={rid}"


# ── fixture (sólo para --self-test / --falsify: NUNCA evidencia de producto) ─


def digest16(*parts: object) -> str:
    return lib.sha256_hex("|".join(str(part) for part in parts).encode())[:16]


def fixture_dun() -> str:
    lines: list[str] = []
    for key, level in LOOT_TABLES:
        lines.append(
            f"DUN VE: loot id={key}-{level} key={key} level={level} rolls={ROLL_SPACE} "
            f"digest={digest16('table', key, level)}"
        )
    for key, level, roll in LOOT_VECTORS:
        value = digest16("vector", key, level, roll)
        lines.append(
            f"DUN VE: loot_vector id={key}-{level}-{roll} key={key} level={level} "
            f"roll={roll} expected={value} actual={value}"
        )
    for source in SOURCES:
        lines.append(
            f"DUN VE: source id={source} source={source} "
            f"digest={digest16('source', source)} rolls={ROLL_SPACE}"
        )
    lines.append("DUN VE: source_distinct id=sources sources=3 distinct=3 shared=0")
    for source in SOURCES:
        lines.append(
            f"DUN VE: respawn id={source} source={source} minutes={RESPAWN_MINUTES[source]} "
            f"tolerance_ticks={TOLERANCE_TICKS}"
        )
    for number, slug, metric, value in ANTIFARM:
        lines.append(
            f"DUN VE: antifarm id=r{number} rule={number} name={slug} "
            f"container=c{number} epoch=1 {metric}={value} verdict=ok"
        )
    for location in ENTRY_LOCATIONS:
        for epoch in ENTRY_EPOCHS:
            lines.append(
                f"DUN VE: entry id={location}-{epoch} location={location} epoch={epoch} "
                f"looted=0 evidence={digest16('entry', location, epoch)}"
            )
    first, second = digest16("instance", "a"), digest16("instance", "b")
    lines.append(f"DUN VE: instance slot=a instance={first} location=0 epoch=1 isolated=1")
    lines.append(f"DUN VE: instance slot=b instance={second} location=0 epoch=1 isolated=1")
    lines.append(f"DUN VE: instance_isolation a={first} b={second} shared=0")
    for adv_id, payload in ADVERSARIAL:
        body = " ".join(f"{key}={value}" for key, value in payload.items())
        lines.append(f"DUN VE: adversarial id={adv_id} {body}")
    return "\n".join(lines) + "\n"


def fixture_facts(dun: str) -> Leg:
    return Leg(
        index=0,
        window=1,
        playing=1,
        interior=1,
        dungeon_scene=1,
        door_registry=817,
        keys=4,
        scene_final=f"Interior({ENTRY_BLOCK})",
        abort="-",
        dun_text=dun,
        shell_text="SHELL VE: world scene=Exterior\n",
        app_text="FG01 VE: interact → interior N0000055.RDB · block 55\n",
        dun_bytes=len(dun.encode()),
        dun_lines=len([line for line in dun.splitlines() if line.strip()]),
    )


FIXTURE_COUNTS = {
    "matrices": len(LOOT_TABLES) + 16,
    "dungeon_keys": 19,
    "dungeon_key_consumers": 1,
    "dungeon_path_callsites": 1,
    "boss_sources": 1,
    "respawn_tokens": 3,
    "antifarm_tokens": 5,
    "dungeon_families": 4,
    "save_count_keys": 14,
}

FIXTURE_PROBE = {
    "runs": 2,
    "identical": 1,
    "exit": 0,
    "digest": LOOT_LIBRARY_GOLDEN,
    "checks": LOOT_LIBRARY_CHECKS,
    "passed": LOOT_LIBRARY_CHECKS,
}


# ── falsificadores ─────────────────────────────────────────────────────────


@dataclass(frozen=True)
class Falsifier:
    name: str
    target: str
    effect: str  # red | green
    detail: str


@dataclass(frozen=True)
class Mutation:
    name: str
    target: str
    pattern: str
    replacement: str
    detail: str
    where: str = "primary"  # primary | alt — `alt` muta la OTRA sesión (control del determinismo)


MUTATIONS: tuple[Mutation, ...] = (
    Mutation("no-reloot-same-epoch", "M44-R17-r1",
             r"(DUN VE: antifarm id=r1 rule=1 name=no-reloot-same-epoch container=\S+ epoch=\d+ )grants=1",
             r"\1grants=2", "segundo grant del mismo contenedor en la misma epoch"),
    Mutation("no-respawn-while-inside", "M44-R17-r2",
             r"(DUN VE: antifarm id=r2 rule=2 name=no-respawn-while-inside container=\S+ epoch=\d+ )respawns=0",
             r"\1respawns=1", "respawn mientras el jugador está dentro"),
    Mutation("boss-single-loot-per-epoch", "M44-R17-r3",
             r"(DUN VE: antifarm id=r3 rule=3 name=boss-single-loot-per-epoch container=\S+ epoch=\d+ )grants=1",
             r"\1grants=2", "el jefe da botín dos veces en la misma epoch"),
    Mutation("container-lockout-cross-player", "M44-R17-r4",
             r"(DUN VE: antifarm id=r4 rule=4 name=container-lockout-cross-player container=\S+ epoch=\d+ )grants=1",
             r"\1grants=2", "dos jugadores saquean el mismo contenedor en el mismo epoch"),
    Mutation("loot-not-scaled-by-entries", "M44-R17-r5",
             r"(DUN VE: antifarm id=r5 rule=5 name=loot-not-scaled-by-entries container=\S+ epoch=\d+ )grants_per_entry=1",
             r"\1grants_per_entry=3", "el botín escala con el número de entradas"),
    Mutation("table-no-rolls", "M44-R14",
             r"(DUN VE: loot id=K-1 key=K level=1 )rolls=48", r"\1rolls=0",
             "tabla publicada sin espacio de tiradas"),
    Mutation("same-table-for-boss", "M44-R15",
             r"(DUN VE: source id=Boss source=Boss digest=)[0-9a-f]{16}",
             r"\1" + digest16("source", "Chest"), "el jefe reutiliza la tabla del cofre"),
    Mutation("respawn-drift", "M44-R16",
             r"(DUN VE: respawn id=Boss source=Boss )minutes=30", r"\1minutes=31",
             "un minuto de deriva en el respawn del jefe"),
    Mutation("entry-looted", "M44-R18",
             r"(DUN VE: entry id=1-2 location=1 epoch=2 )looted=0", r"\1looted=1",
             "mazmorra ya saqueada al entrar"),
    Mutation("vector-drift", "M44-R14a",
             r"(DUN VE: loot_vector id=K-1-0 key=K level=1 roll=0 expected=[0-9a-f]{16} )actual=[0-9a-f]{16}",
             r"\1actual=0000000000000000", "una tirada del vector no reproduce su digest"),
    Mutation("antifarm-verdict-bad", "M44-R17",
             r"(DUN VE: antifarm id=r3 rule=3 name=boss-single-loot-per-epoch "
             r"container=\S+ epoch=\d+ grants=1 )verdict=ok",
             r"\1verdict=bad", "una regla anti-farm se publica sin veredicto ok"),
    Mutation("instance-shared", "M44-R19",
             r"(DUN VE: instance slot=b instance=)[0-9a-f]{16}",
             r"\1" + digest16("instance", "a"), "dos instancias de la misma location comparten id"),
    Mutation("adv-double-grant", "ADV-01",
             r"(DUN VE: adversarial id=ADV-01 [^\n]*?)grants=1", r"\1grants=2",
             "dos jugadores saquean el mismo contenedor en el mismo epoch"),
    Mutation("adv-shared-container", "ADV-02",
             r"(DUN VE: adversarial id=ADV-02 [^\n]*?)shared=0", r"\1shared=1",
             "el contenedor compartido entre jugadores no se reinicia con la epoch"),
    Mutation("adv-no-isolation", "ADV-03",
             r"(DUN VE: adversarial id=ADV-03 players=2 containers=1 )instances=2", r"\1instances=1",
             "las dos sesiones no se separan en instancias"),
    # control del determinismo: expected==actual sigue bien en la sesión 1 y sólo
    # la sesión 2 discrepa -> la fila debe caer por la referencia independiente.
    Mutation("vector-cross-session-drift", "M44-R14a",
             r"(DUN VE: loot_vector id=K-10-47 key=K level=10 roll=47 expected=[0-9a-f]{16} )actual=[0-9a-f]{16}",
             r"\1actual=ffffffffffffffff",
             "dos sesiones publican digests distintos para el mismo vector", where="alt"),
)

CENSUS_SHADOWS: tuple[tuple[str, str, str, str], ...] = (
    # el token que la fila exige es la GENERACIÓN de botín en el camino de mazmorra
    # (`dungeon_key(` sola es el precursor: su consumo se reporta como contraste).
    ("census-sees-consumer", "M44-R14c", "crates/x/src/dungeon_shadow.rs",
     "pub fn open(key: char) {\n"
     "    let _container = crate::loot::generate_container(1, 2, key, 3);\n"
     "    let _keys = crate::loot::dungeon_key(12);\n"
     "}\n"),
    ("census-sees-boss", "M44-R15a", "crates/x/src/loot_shadow.rs",
     "pub fn boss() { generate_boss_loot(1, 2); }\n"),
    ("census-sees-respawn", "M44-R16a", "crates/x/src/clock_shadow.rs",
     "pub const RESPAWN_MINUTES: u64 = 20;\n"),
    ("census-sees-antifarm", "M44-R17a", "crates/x/src/epoch_shadow.rs",
     "pub const RULE: &str = \"no-reloot-same-epoch\";\n"),
    ("census-sees-instance", "M44-R19a", "crates/x/src/save_shadow.rs",
     "pub const FAMILY: &str = \"dungeon_instance\";\n"),
)


def falsifiers() -> tuple[Falsifier, ...]:
    items = [
        Falsifier(mutation.name, mutation.target, "red", mutation.detail)
        for mutation in MUTATIONS
    ]
    items.extend(
        Falsifier(name, target, "green", f"sombra de censo en {path}")
        for name, target, path, _ in CENSUS_SHADOWS
    )
    items.append(
        Falsifier("golden-drift", "M44-R14b", "red",
                  "el golden declarado se desvía un nibble del medido")
    )
    items.append(
        Falsifier("dun-evidence-appears", "M44-R18a", "green",
                  "una línea `DUN VE: entry` real hace verde la fila de jugador")
    )
    return tuple(items)


FALSIFIER_BY_NAME = {falsifier.name: falsifier for falsifier in falsifiers()}


def fixture_ctx(dun: str | None = None, counts: dict[str, int] | None = None,
                golden: str | None = None) -> Ctx:
    text = fixture_dun() if dun is None else dun
    return Ctx(
        dun=text,
        facts=fixture_facts(text),
        counts=dict(FIXTURE_COUNTS) if counts is None else counts,
        probe=dict(FIXTURE_PROBE),
        golden=LOOT_LIBRARY_GOLDEN if golden is None else golden,
        alts=[text],  # una sesión alterna = la referencia independiente (como en la corrida real)
    )


def apply_mutation(mutation: Mutation, ctx: Ctx) -> Ctx:
    if mutation.where == "alt":
        if not ctx.alts:
            raise SystemExit(f"[NOT-RUN] la mutación {mutation.name} necesita una sesión alterna (G12)")
        alt = re.sub(mutation.pattern, mutation.replacement, ctx.alts[0], count=1,
                     flags=re.MULTILINE)
        if alt == ctx.alts[0]:
            raise SystemExit(f"[NOT-RUN] la mutación {mutation.name} no cambió el fixture (G12)")
        return replace(ctx, alts=[alt, *ctx.alts[1:]])
    mutated = re.sub(mutation.pattern, mutation.replacement, ctx.dun, count=1, flags=re.MULTILINE)
    if mutated == ctx.dun:
        raise SystemExit(f"[NOT-RUN] la mutación {mutation.name} no cambió el fixture (G12)")
    facts = replace(
        ctx.facts,
        dun_text=mutated,
        dun_bytes=len(mutated.encode()),
        dun_lines=len([line for line in mutated.splitlines() if line.strip()]),
    )
    return replace(ctx, dun=mutated, facts=facts)


def shadow_counts(name: str, out_dir: Path) -> dict[str, int]:
    """Censo sobre un árbol sombra vacío + el token del falsificador (doble dirección)."""
    entry = next(item for item in CENSUS_SHADOWS if item[0] == name)
    _, _, relative, body = entry
    empty_root = out_dir / "shadow" / f"{name}-empty"
    full_root = out_dir / "shadow" / f"{name}-full"
    for root in (empty_root, full_root):
        (root / "crates" / "x" / "src").mkdir(parents=True, exist_ok=True)
    empty = census(empty_root)
    shadow = full_root / relative
    shadow.parent.mkdir(parents=True, exist_ok=True)
    shadow.write_text(body, encoding="utf-8")
    return {"empty": empty, "full": census(full_root)}


def run_falsifier(name: str, out_dir: Path) -> int:
    falsifier = FALSIFIER_BY_NAME.get(name)
    if falsifier is None:
        print(f"[NOT-RUN] falsificador desconocido: {name} (G12)")
        print("KNOWN: " + ",".join(sorted(FALSIFIER_BY_NAME)))
        return EXIT_NC
    row = ROW_BY_ID[falsifier.target]
    if name == "golden-drift":
        drifted = ("0" if LOOT_LIBRARY_GOLDEN[0] != "0" else "1") + LOOT_LIBRARY_GOLDEN[1:]
        ok, detail = check_row(row, fixture_ctx(golden=drifted))
    elif name == "dun-evidence-appears":
        base = fixture_ctx()
        line = (
            "DUN VE: entry id=0-1 location=0 epoch=1 looted=0 "
            f"evidence={digest16('entry', 0, 1)}"
        )
        ctx = replace(base, dun=base.dun + line + "\n")
        ctx = replace(
            ctx,
            facts=replace(
                ctx.facts,
                dungeon_scene=1,
                dun_lines=ctx.facts.dun_lines + 1,
                dun_bytes=ctx.facts.dun_bytes + len(line) + 1,
                dun_text=ctx.dun,
            ),
        )
        ok, detail = check_row(row, ctx)
    elif name in {entry[0] for entry in CENSUS_SHADOWS}:
        counts = shadow_counts(name, out_dir)
        ok, detail = check_row(row, fixture_ctx(counts=counts["full"]))
        empty_ok, empty_detail = check_row(row, fixture_ctx(counts=counts["empty"]))
        caught = ok and not empty_ok
        print(
            f"FALSIFY {name} target={row.rid} effect={falsifier.effect} "
            f"shadow=green empty=red caught={int(caught)}"
        )
        print(f"  shadow: {detail}")
        print(f"  empty: {empty_detail}")
        return EXIT_GREEN if caught else EXIT_RED
    else:
        mutation = next(item for item in MUTATIONS if item.name == name)
        ok, detail = check_row(row, apply_mutation(mutation, fixture_ctx()))

    caught = (not ok) if falsifier.effect == "red" else ok
    print(
        f"FALSIFY {name} target={row.rid} effect={falsifier.effect} "
        f"row_ok={int(ok)} caught={int(caught)}"
    )
    print(f"  {detail}")
    return EXIT_GREEN if caught else EXIT_RED


def self_test(out_dir: Path) -> int:
    ctx = fixture_ctx()
    results = [(row, check_row(row, ctx)) for row in ROWS]
    red = [row.rid for row, (ok, _) in results if not ok]
    print(
        f"SELFTEST fixture rows={len(results)} green={len(results) - len(red)} red={len(red)}"
    )
    for row, (ok, detail) in results:
        if not ok:
            print(f"  FIXTURE-FAIL {row.rid} {detail}")
    caught = 0
    total = 0
    for falsifier in falsifiers():
        total += 1
        if falsifier.name == "golden-drift":
            drifted = ("0" if LOOT_LIBRARY_GOLDEN[0] != "0" else "1") + LOOT_LIBRARY_GOLDEN[1:]
            ok, detail = check_row(ROW_BY_ID[falsifier.target], fixture_ctx(golden=drifted))
            bit = (not ok) if falsifier.effect == "red" else ok
        elif falsifier.name == "dun-evidence-appears":
            base = fixture_ctx()
            line = (
                "DUN VE: entry id=0-1 location=0 epoch=1 looted=0 "
                f"evidence={digest16('entry', 0, 1)}"
            )
            ctx2 = replace(base, dun=base.dun + line + "\n")
            ctx2 = replace(
                ctx2,
                facts=replace(
                    ctx2.facts,
                    dungeon_scene=1,
                    dun_lines=ctx2.facts.dun_lines + 1,
                    dun_bytes=ctx2.facts.dun_bytes + len(line) + 1,
                    dun_text=ctx2.dun,
                ),
            )
            ok, detail = check_row(ROW_BY_ID[falsifier.target], ctx2)
            bit = ok
        elif falsifier.name in {entry[0] for entry in CENSUS_SHADOWS}:
            counts = shadow_counts(falsifier.name, out_dir)
            full_ok, detail = check_row(ROW_BY_ID[falsifier.target], fixture_ctx(counts=counts["full"]))
            empty_ok, _ = check_row(ROW_BY_ID[falsifier.target], fixture_ctx(counts=counts["empty"]))
            bit = full_ok and not empty_ok
        else:
            mutation = next(item for item in MUTATIONS if item.name == falsifier.name)
            ok, detail = check_row(ROW_BY_ID[falsifier.target], apply_mutation(mutation, fixture_ctx()))
            bit = (not ok) if falsifier.effect == "red" else ok
        caught += int(bit)
        print(
            f"  FALSIFIER {falsifier.name} target={falsifier.target} "
            f"effect={falsifier.effect} caught={int(bit)}"
        )
    print(
        f"SELFTEST falsifiers={total} caught={caught} fixture_green={int(not red)}"
    )
    good = not red and caught == total
    print("VERDICT: PASS" if good else "VERDICT: FAIL")
    return EXIT_GREEN if good else EXIT_RED


# ── emisión ────────────────────────────────────────────────────────────────


def render(row: Row, ok: bool, detail: str, nc_reason: str | None) -> str:
    if ok:
        return f"[OK] {row.rid} - {detail} src={row.anchor}"
    if nc_reason is not None:
        return (
            f"[NC] {row.rid} - {row.token} {detail} expected={row.expected} "
            f"({row.symptom}) (G12 {nc_reason}) src={row.anchor}"
        )
    return (
        f"[FAIL] {row.rid} - {row.token} {detail} expected={row.expected} "
        f"({row.symptom}) src={row.anchor}"
    )


def self_cite(marker: str) -> str:
    """`scripts/dun-content-001.py:<line>` calculado del propio fichero, nunca a mano.

    Una fila de arnés sin línea (`src=scripts/dun-content-001.py`) deja
    `PROC-citations` en rojo por `no-line`: la cita del gate también muerde.
    """
    text = Path(__file__).read_text(encoding="utf-8")
    for index, line in enumerate(text.splitlines(), 1):
        if marker in line:
            return f"{OWNER}:{index}"
    raise SystemExit(f"[NOT-RUN] marcador de autocita ausente: {marker} (G12)")


def sources_block() -> tuple[str, dict[str, int]]:
    paths = sorted(MEASURED_SOURCES)
    fingerprint, detail = lib.Gate.recompute_fingerprint(paths)
    missing = [path for path in paths if not (ROOT / path).is_file()]
    files = int(detail.split()[0]) if detail.split() else 0
    line = (
        f"SOURCES fingerprint={fingerprint or 'absent'} files={files} "
        f"paths={','.join(paths)}"
    )
    info = {"missing": len(missing), "files": files, "fingerprint": fingerprint or "absent"}
    return line, info


def catalog() -> int:
    print(f"GATE {GATE} block={BLOCK} cr={CR}")
    print(f"SPEC {SPEC}")
    print(f"REQS M44-R14..M44-R19 + ADV-01..ADV-03 · rows={len(ROWS)} · falsifiers={len(FALSIFIER_BY_NAME)}")
    for row in ROWS:
        print(f"ROW {row.rid} kind={row.kind} token={row.token} anchor={row.anchor} :: {row.name}")
    for falsifier in falsifiers():
        print(
            f"FALSIFIER {falsifier.name} target={falsifier.target} "
            f"effect={falsifier.effect} :: {falsifier.detail}"
        )
    return EXIT_GREEN


def main(argv: Sequence[str] | None = None) -> int:
    parser = argparse.ArgumentParser(
        prog=GATE,
        description="Gate de aceptación del bloque D de M44 (contenido de mazmorra).",
    )
    parser.add_argument("--binary", default=str(DEFAULT_BINARY), help="binario de producto")
    parser.add_argument("--skip-build", action="store_true",
                        help="no compila (obligatorio: `cargo build` corre bajo flock)")
    parser.add_argument("--out", default=str(DEFAULT_OUT), help="directorio de artefactos")
    parser.add_argument("--data-root", default=os.environ.get("DF_APP_ROOT", ""),
                        help="raíz retail (por defecto $DF_APP_ROOT o ~/Games/Daggerfall/app)")
    parser.add_argument("--timeout", type=int, default=DEFAULT_TIMEOUT,
                        help="deadline global en segundos para las dos sesiones de jugador")
    parser.add_argument("--json", action="store_true", help="escribe manifest.json del run")
    parser.add_argument("--list", action="store_true", help="catálogo de filas y falsificadores")
    parser.add_argument("--falsify", default=None, metavar="NAME", help="ejerce un falsificador")
    parser.add_argument("--self-test", action="store_true",
                        help="fixture verde + cada falsificador muerde")
    args = parser.parse_args(argv)

    out_dir = Path(args.out)
    if not out_dir.is_absolute():
        out_dir = ROOT / out_dir
    out_dir.mkdir(parents=True, exist_ok=True)

    if args.list:
        return catalog()
    if args.self_test:
        return self_test(out_dir)
    if args.falsify:
        return run_falsifier(args.falsify, out_dir)

    lines: list[str] = []
    if not args.skip_build:
        print(f"[NOT-RUN] compilación no permitida: `cargo build` corre bajo {BUILD_LOCK.relative_to(ROOT)} (G12)")
        print("VERDICT: NC")
        return EXIT_NC

    binary = Path(args.binary)
    if not binary.is_absolute():
        binary = ROOT / binary
    if not binary.is_file():
        print(f"[NOT-RUN] binario ausente: {os.path.relpath(binary, ROOT)} (G12 sin build)")
        print("VERDICT: NC")
        return EXIT_NC

    data_root = Path(args.data_root) if args.data_root else Path.home() / "Games" / "Daggerfall" / "app"
    started = time.monotonic()
    deadline = started + max(20, args.timeout)

    leg1 = Leg(index=1)
    leg2 = Leg(index=2)
    nc_reason: str | None = None
    if not os.environ.get("DISPLAY"):
        nc_reason = "no-display"
        leg1.abort = "no-display"
        leg2.abort = "no-display"
    elif not (data_root / "ARENA2").is_dir():
        nc_reason = "no-retail"
        leg1.abort = "no-retail"
        leg2.abort = "no-retail"
    else:
        leg1 = play_leg(1, binary, data_root, out_dir, started + args.timeout * 0.55)
        leg2 = play_leg(2, binary, data_root, out_dir, deadline)
        if leg1.abort in ("no-display", "no-retail", "no-window") and leg2.abort == leg1.abort:
            nc_reason = leg1.abort
        elif leg1.window == 0 and leg2.window == 0:
            nc_reason = "no-window"

    primary = leg2 if leg2.playing else leg1
    probe = run_probe(binary, max(20, args.timeout))
    counts = census(ROOT)
    alts = [leg.dun_text for leg in (leg1, leg2) if leg is not primary]
    ctx = Ctx(dun=primary.dun_text, facts=primary, counts=counts, probe=probe,
              golden=LOOT_LIBRARY_GOLDEN, alts=alts)

    sources_line, sources_info = sources_block()
    # `head` NO va al stdout: lo mueve cualquier lane hermana y rompería el
    # requisito de stdout byte-idéntico entre dos procesos (medido 2026-10-29:
    # 087c50f -> e493d56 entre run1 y run2, cuerpo idéntico). La identidad del
    # instrumento es SOURCES fingerprint, recomputado desde disco; el commit va
    # al manifiesto (`--json`).
    lines.append(f"gate={GATE} cr={CR} block={BLOCK}")
    lines.append(
        f"binary={os.path.relpath(binary, ROOT)} sha256={lib.file_sha256(binary) or 'absent'}"
    )
    lines.append(sources_line)
    for leg in (leg1, leg2):
        lines.append(f"NOTE capture=run{leg.index} {leg.record}")
    lines.append(
        f"NOTE census matrices={counts['matrices']} dungeon_keys={counts['dungeon_keys']} "
        f"dungeon_key_consumers={counts['dungeon_key_consumers']} "
        f"dungeon_path_callsites={counts['dungeon_path_callsites']} "
        f"boss_sources={counts['boss_sources']} respawn_tokens={counts['respawn_tokens']} "
        f"antifarm_tokens={counts['antifarm_tokens']} dungeon_families={counts['dungeon_families']}"
    )
    lines.append(
        f"NOTE probe={PROBE} runs={probe['runs']} identical={probe['identical']} "
        f"exit={probe['exit']} checks={probe['checks']} digest={probe['digest'][:16] or 'absent'}"
    )

    rendered: list[str] = []
    red = 0
    nc = 0
    for row in ROWS:
        ok, detail = check_row(row, ctx)
        reason = None
        if not ok and nc_reason is not None and row.kind in ("player", "channel"):
            reason = nc_reason
        line = render(row, ok, detail, reason)
        rendered.append(line)
        if not ok:
            if reason is not None:
                nc += 1
            else:
                red += 1

    # filas de arnés
    # (la «frescura por mtime» se retira: medía el churn de las lanes vivas —
    #  `m18_window.rs` escribiéndose mientras corre el gate —, no mi contrato;
    #  lo que sí es atribuible es que cada cita resuelva hoy sobre disco.)
    cites = sorted({row.anchor for row in ROWS})
    cite_ok, cite_detail = lib.citation_check(cites)
    targeted = {falsifier.target for falsifier in falsifiers()}
    unbound = [row.rid for row in ROWS if row.rid not in targeted]
    orphan = sorted(targeted - set(ROW_BY_ID))
    proc_rows = [
        (
            "PROC-sources",
            sources_info["missing"] == 0 and cite_ok and not orphan,
            f"declared={len(MEASURED_SOURCES)} missing={sources_info['missing']} {cite_detail}",
        ),
        (
            "PROC-catalog",
            not unbound and len(FALSIFIER_BY_NAME) == len(falsifiers()),
            f"rows={len(ROWS)} falsifiers={len(FALSIFIER_BY_NAME)} "
            f"unbound={','.join(unbound) or 'none'}",
        ),
        (
            "PROC-two-runs",
            leg1.window == 1 and leg2.window == 1 and leg1.record == leg2.record,
            f"identical={int(leg1.window == 1 and leg2.window == 1 and leg1.record == leg2.record)} "
            f"sha256={lib.sha256_hex(leg1.record.encode())[:16]} "
            f"run1_lines={leg1.dun_lines} run2_lines={leg2.dun_lines} "
            f"run1_abort={leg1.abort} run2_abort={leg2.abort}",
        ),
    ]
    for rid, ok, detail in proc_rows:
        cite = self_cite(rid)
        rendered.append(
            f"[OK] {rid} - {detail} src={cite}" if ok
            else f"[FAIL] {rid} - harness {detail} expected=window=1,identical=1 "
                 f"(arnés: dos procesos de producto deben coincidir) src={cite}"
        )
        if not ok:
            red += 1

    guard_text = "\n".join(rendered)
    guard_ok, guard_detail = lib.probe_citations_guard(guard_text)
    guard_cite = self_cite("PROC-citations")
    rendered.append(
        f"[OK] PROC-citations - {guard_detail} src={guard_cite}" if guard_ok
        else f"[FAIL] PROC-citations - guard {guard_detail} expected=real+absent+line>n "
             f"(la cita de cada fila debe morder en las dos direcciones) src={guard_cite}"
    )
    if not guard_ok:
        red += 1

    lines.extend(rendered)
    verdict = "FAIL" if red else ("NC" if nc else "PASS")
    lines.append(
        f"SUMMARY rows={len(ROWS)} harness={len(proc_rows) + 1} red={red} nc={nc} "
        f"falsifiers={len(FALSIFIER_BY_NAME)} spec={SPEC}"
    )
    lines.append(f"VERDICT: {verdict}")

    if args.json:
        manifest = {
            "gate": GATE,
            "cr": CR,
            "block": BLOCK,
            "spec": SPEC,
            "head": lib.head_commit(),
            "binary": {
                "path": os.path.relpath(binary, ROOT),
                "sha256": lib.file_sha256(binary),
            },
            "sources": {
                "fingerprint": sources_info["fingerprint"],
                "files": sources_info["files"],
                "paths": sorted(MEASURED_SOURCES),
            },
            "census": counts,
            "probe": probe,
            "legs": [
                {"run": leg.index, "record": leg.record, "abort": leg.abort,
                 "keys": leg.keys, "dun_bytes": leg.dun_bytes}
                for leg in (leg1, leg2)
            ],
            "rows": [
                {"id": row.rid, "kind": row.kind, "anchor": row.anchor,
                 "token": row.token, "expected": row.expected}
                for row in ROWS
            ],
            "verdict": verdict,
            "red": red,
            "nc": nc,
        }
        manifest_path = out_dir / "manifest.json"
        manifest_path.write_text(json.dumps(manifest, indent=2, sort_keys=True) + "\n",
                                 encoding="utf-8")
        lines.append(f"MANIFEST path={os.path.relpath(manifest_path, ROOT)} rows={len(ROWS)}")

    # Orden terminal (familia shell-001/sys-*-001): cuerpo, VERDICT, DIGEST.
    # `DIGEST` sella todo lo impreso antes de su propia línea.
    text = "\n".join(lines) + "\n"
    sys.stdout.write(text)
    sys.stdout.write(f"DIGEST {lib.sha256_hex(text.encode())}\n")
    sys.stdout.flush()
    return EXIT_RED if red else (EXIT_NC if nc else EXIT_GREEN)


if __name__ == "__main__":
    sys.exit(main())
