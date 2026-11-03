#!/usr/bin/env python3
"""SHELL-002 — gate de M-SHELL **fase 2** (menú completo, creación, sesión y slots v3).

Escrito **antes** de la implementación (regla `gate primero` del pack
`CR-75e7c81f`): hoy está en ROJO contra el binario de producto porque la fase 2
no existe; la lane de implementación escribe contra esta tabla y **no** edita
este fichero ni `scripts/shell-001.py`.

Qué añade sobre SHELL-001
------------------------
`SHELL-001` mide la capa aplicación de la fase 1 (boot, loading, menú de 2
ítems, nueva partida, mundo exterior, HUD, mapa, viaje, pausa, salida). Este
gate mide lo que la fase 1 declaró como GAP (`shell.rs:43`, `GAP-SHELL-07/08/09`)
y lo que `COVERAGE-100.md` deja abierto tras la fase 1: filas 5 (creación), 7
(sesión), 8 (slots) y las opciones navegables de la 12, sobre el formato
**save v3** que escribe `M44 W44-5`. NO mide el automap vivo de la fila 10 ni el
sitio inicial interior de la 6: eso queda para `SHELL-003` (declarado en
`docs/sintergic/playability/SHELL-002.md` §8).

Contrato de evidencia (verbatim; la app espeja `SHELL VE: …` en
`$DF_SHELL_STATE` con append+flush, y ese fichero es el canal de
sincronización — nunca `sleep`):

    SHELL VE: menu state=Menu items=5 selection=N surface=main-menu
    SHELL VE: menu order=new-game,continue,load,options,quit
    SHELL VE: menu selection=N                        (N=1..4, y 0 al cerrar el ciclo)
    SHELL VE: continue result=none saves=0 notice=no-save
    SHELL VE: continue result=loaded slot=I format=v3
    SHELL VE: load open=1 slots=N format=v3 panel=save-browser
    SHELL VE: load open=0
    SHELL VE: slot index=I name=TOKEN level=L minutes=M format=v3
    SHELL VE: load done slot=I format=v3 state=Playing ticks=T character=TOKEN
    SHELL VE: options open=1 page=ID pages=11 panel=pause-menu
    SHELL VE: options set key=ID value=V pages=11
    SHELL VE: options close=1
    SHELL VE: create state=Create surfaces=16 host=DaggerfallStartNewGameWizard face=F
    SHELL VE: create step=N steps=10 field=ID
    SHELL VE: create done surfaces=16 character=TOKEN state=Playing
    SHELL VE: session state=Started character=TOKEN save=<none|slot=I>
    SHELL VE: save open=1 slots=6 format=v3 panel=save-browser
    SHELL VE: save slot=I format=v3 bytes=B digest=D
    SHELL VE: session state=Menu from=Playing

### Arranque (readiness) — defecto medido y corregido

Bajo carga (3 lanes concurrentes) la corrida 1 capturaba **0 líneas** y abortaba en `menu`: el deadline global
arrancaba en `__init__` y el arranque de la app se lo comía, así que «la app no arrancó» y «arrancó tarde» eran
indistinguibles — la misma clase de defecto que el control de `SHELL-001`. Aquí se espera la **condición**
(`menu state=Menu` en el canal), no un `sleep` mayor, con presupuesto de arranque propio (`READY_BUDGET`) separado
del de secuencia (`SEQUENCE_BUDGET`), y el fallo se reporta con motivo tipado:

    ready=1 reason=menu             -> lista; la secuencia empieza con presupuesto nuevo
    reason=menu-too-late            -> el canal escribio (boot/loading) y el menu no llego: timing
    reason=channel-empty            -> el canal no escribio nada con el proceso vivo: arnes/canal
    reason=exited|exited-no-channel -> el proceso murio antes del menu: producto

La espera **no** avanza `cursor`: la secuencia vuelve a validar boot -> loading -> menu desde el principio. En el
NOTE va `waited=<cubo>` (`lt30|lt60|ge60`) y **no** los segundos: los segundos varian con la carga y romperian el
digest. El `DIGEST` sella una **espina estable** (ver `DIGEST-SCOPE`): identidad (gate/CR/`head`/binario/huella
de fuentes) + los eventos del contrato en orden (estados, selecciones, órdenes, contadores) + el veredicto de cada
fila + los flags `PROC-*`. Quedan FUERA los valores de render (`hud … opaque=`, digests compuestos) y los de
timing (cubo de espera, conteos de líneas capturadas, títulos muestreados): el informe los imprime para
diagnóstico pero no los sella — tres corridas al mismo binario daban tres digests y el sello no probaba nada.
Invariantes: **dos invocaciones EXTERNAS al mismo árbol ⇒ 1 solo `DIGEST`**, y `PROC-idempotent` lo comprueba entre
invocaciones (el sello de una sola invocación lo prueba `PROC-digest-stable`)
perturbando únicamente esos campos volátiles.

Ninguna línea lleva decimales, timestamps ni PIDs. `TOKEN` es
`^[a-z0-9-]{1,32}$` (derivado de las elecciones de creación, nunca de la hora).
Los 16 `surfaces=` son `m38_surfaces::CREATE_SURFACE_COUNT`; los 11 `pages=`
son `m38_surfaces::CONFIG_SURFACE_COUNT`; los 6 slots son `SAVE0..SAVE5`.

Mapa de teclas (fase 2) — las de la fase 1 no cambian (`Return`, `Up`, `Down`,
`M`, `T`, `Escape`, `Q`):

    menú        Up/Down navegan, Return activa (orden: new-game, continue, load, options, quit)
    creación    Right/Left cambian la cara, Down/Up el campo, Return confirma, Escape cancela
    pausa       Escape (fase 1);  `o` abre opciones;  Right/Left cambian el valor; Escape cierra
    partida     `s` abre el browser de guardado;  Down/Up eligen slot;  Return escribe;
                Escape cierra
    pausa       `b` vuelve al menú (`session state=Menu from=Playing`)

Escenario. El perfil es **por proceso**: el gate exporta `DF_SAVE_DIR` con un
directorio vacío por corrida, así las dos corridas ven 0 partidas al arrancar y
`PROC-two-runs` puede exigir stdout byte-idéntico. La secuencia es:
boot → loading → menú → orden → 1..4 y 0 → `continue` sin partida (aviso, sigue
en MENU) → `load` (0 slots) → cierra → `options` (cambia un valor) → `new-game`
→ creación (2 pasos + confirma) → `session state=Started` → pausa → opciones
desde la pausa (cambia el mismo valor otra vez) → `s` guarda en slot 0 → `Escape` cierra el
browser modal (sin línea propia; hueco del canal) → `b` vuelve al menú → `load` (1 slot) →
carga slot 0 → `load done` → pausa → `q` sale con 0.

Control negativo FUERTE (`CTRL-non-shell-key`). El de `SHELL-001` captura 0
líneas y muere por deadline (`exit=-15`) porque su `run_control` sólo muestrea
títulos: no juzga el canal. Este control **lee el canal** y exige las tres cosas
a la vez: (i) ≥3 líneas de evidencia escritas (el canal funciona), (ii) una
tecla que no es de shell (`z`) y **cero** transición (el título nunca sale de
BOOT/LOADING/MENU), (iii) cero líneas de fase 2 y `selection` sin mover.

### Aislamiento e idempotencia (defecto medido y corregido — CR-cf456e65)

Hasta esta lane el gate medía sobre `--out` heredado: `run1-saves/` conservaba
el `SAVE0.dfl` de la corrida anterior, así que la MISMA orden sobre el MISMO
binario daba `PASS` en frío y `FAIL` en caliente (`continue` veía 1 partida y
`load open slots=0` veía 1 slot). El experimento del capitán (A limpio `PASS`,
B verbatim `FAIL`, C tras `rm -rf …/{run1,run2,ctrl}-saves` `PASS`) mide
exactamente eso: **single-shot**, no una regresión del canal.

Ahora `wipe_footprint()` vacía **toda la huella** que el gate y el binario escriben
—no sólo `--out`— al principio de CADA invocación, y la fila `PROC-idempotent`
compara **dos invocaciones EXTERNAS completas** (subprocesos reales del MISMO
comando: el driver lanza `sys.executable scripts/shell-002.py <argv>` y sólo se
distinguen por entorno, `DF_SHELL002_CHILD`/`DF_SHELL002_INVOCATION`). La invocación
2 hereda lo que escribió la 1 y el barrido tiene que anularlo. **El bucle interno
no vale**: dentro de un mismo proceso el residuo nunca se reproduce, así que la
fila rechaza dos recibos con el mismo `pid` (`basis=single-process`).

Huella (enumerada leyendo gate y producto; línea exacta por elemento):

    run{1,2}.state, ctrl.state          canal `$DF_SHELL_STATE`    shell-001.py:934
    run{1,2}.app.log, ctrl.app.log      stdout/stderr de la app    shell-001.py:941-943
    run{1,2}-saves/, ctrl-saves/        perfil `DF_SAVE_DIR`       shell-002.py:run_scenario
      └─ SAVE0..SAVE5.dfl               slots del producto         shell_slots.rs:180-192
    stdout.txt, manifest.json           informes del gate          shell-002.py:emit/driver
    receipts/inv-<n>.json               recibo por invocación      shell-002.py:write_receipt
    *.dfl, *.dfs en la raíz de `--out`  residuo suelto de producto (patrón, no nombre)

El barrido conserva los `receipts/inv-<m>.json` de las OTRAS invocaciones externas
(`wipe_footprint(keep=<n>)`): están indexados por número, el hijo sólo lee el suyo
—que el driver vacía justo antes de lanzarlo— así que no hay herencia posible, y
borrarlos dejaría sin `head` la mitad del diagnóstico de un drift.

`saves/` del repositorio NO entra en el barrido, y no por comodidad: el camino del
shell lee y escribe **sólo** `DF_SAVE_DIR` (`crates/app/src/shell_slots.rs:96-98`;
sin la variable no hay slot, `crates/app/src/m18_window.rs:2173`), y
`saves/session.dfs` lo escribe `crates/app/src/m8_window.rs:45` en el camino
`--probe SESSION-001 --window` (`crates/app/src/main.rs:890-909`), que este gate no
lanza: arranca el binario pelado (`scripts/shell-001.py:941`), que va a
`run_windowed_m18` (`crates/app/src/main.rs:986-990`).

Si alguien vuelve a medir sobre restos, las invocaciones divergen (veredicto o
sello) y la fila se pone ROJA. Una sola invocación no compara nada: se reporta
roja por vacuidad, nunca verde. Control negativo (también de la fila):
`DF_SHELL002_NO_ISOLATION=1` salta el barrido y `PROC-idempotent` se pone ROJO con
`first_drift=inv2`.

### Causa del drift: `head-drift` no es una carrera de emisión (CR-7a2e9c41)

`PROC-idempotent` sella `head=<sha>` en la identidad de cada invocación, así que un
commit ajeno que aterriza ENTRE la invocación 1 y la 2 cambia el sello **sin que el
producto tenga defecto**. Medido dos veces: `head` de `e463dab` a `2f17eab`
(`seals=1b2e8c08,7ea8de77 distinct=2 first_drift=inv3`) y de `0b68111` a `79441ad`
25 s después de arrancar (`distinct=2 first_drift=inv2`); repetida con el árbol
quieto, `distinct=1 first_drift=-`. Diagnosticarlo exigía comparar a mano marcas de
tiempo de commits con el arranque de la corrida, y el informe lo leía como si fuera
una carrera de emisión.

Ahora el drift **se nombra**, y sigue siendo `FAIL` — nombrar no indulta:

    cause=head-drift heads=0b68111->79441ad — otro proceso commiteó durante la corrida; repite con el árbol quieto

Condición dura: `head-drift` sólo si el `head` es la **única** diferencia — mismo
`binary_sha256` y misma espina con el `head` enmascarado (identidad, eventos del
contrato, filas, flags y veredicto). Cualquier otra diferencia manda y se reporta
`cause=emission-drift why=…`: un `head-drift` que tapara una carrera real de emisión
sería peor que el falso rojo que esto arregla. Cada recibo (`receipts/inv-<n>.json`)
registra el `head` y el `binary_sha256` con los que corrió esa invocación, y el NOTE
`idempotence` los imprime (`heads=`) junto al sello.

### `--out` por defecto: uno por proceso (defecto medido — CR-d3f0a5b8)

Dos invocaciones concurrentes resolvían el MISMO `--out`
(`.sintergic/artifacts/shell-002`): la segunda pisaba `manifest.json` —y con él
`binary_sha256`, la identidad con la que se detecta un binario movido a mitad de
corrida— además de `stdout.txt`, `receipts/inv-<n>.json` y `run{1,2}-saves/`.
Dos mediciones simultáneas se corrompían la evidencia mutuamente, y el informe
que quedaba en disco no era el de ninguna de las dos.

El default es ahora **único por proceso**
(`.sintergic/artifacts/shell-002/<UTC>-p<pid>`). Elegido sobre serializar con
`flock` porque el directorio propio elimina la colisión por construcción —sin
esperas y sin cerrojo compartido entre el driver y sus propios hijos, que
escriben en el mismo `--out` por diseño— y deja cada evidencia atribuida a su
`pid`; con `flock` las dos corridas seguirían siendo indistinguibles dentro del
mismo directorio. Con `--out` explícito el comportamiento es el de siempre, y el
driver pasa siempre a sus hijos el `--out` que él resolvió: si el operador no lo
dio, lo **añade** al argv del hijo (`_rewrite_argv` sólo reescribe flags ya
presentes), porque un hijo sin `--out` estrenaría su propio directorio y
`PROC-idempotent` compararía dos objetos de sitios distintos. La ruta efectiva se
imprime en el informe (`NOTE out=… source=default-per-process`).

Intacto a propósito: `normalize()`, `two_runs_ok`, el orden de `stable_events` /
`spine` y la sensibilidad del `DIGEST` al orden de emisión. Canonicalizar la
espina convertiría una carrera real en un verde permanente.

### Verdad de la evidencia histórica

Los 13 `FAIL` que reportó el capitán **no** son una regresión de `9b642f6`
(14:56): los restos culpables tenían `mtime` 14:29, ANTERIOR al wiring, y las
corridas `PASS` anteriores se midieron sobre `--out` frescos. La causa es la
heredabilidad descrita arriba — el diagnóstico está en `ab-dirty-stdout.txt`
frente a `ab-clean-stdout.txt`. Este gate no cambia lo que miden sus 15 filas:
cambia que ahora mide siempre sobre el mismo estado inicial.

Filas medidas (14 + control):

    SH2-01 menú completo              items=5 y orden declarado
    SH2-02 navegación completa        1,2,3,4 y vuelta a 0
    SH2-03 continuar sin partida      aviso no-save, sigue en MENU, 0 sesión
    SH2-04 browser de carga           load open slots=0 y slots=1, panel=save-browser
    SH2-05 manifiesto de slots        slot index=… format=v3, level/minutes enteros
    SH2-06 cargar restaura            load done slot=0 ticks>0, mismo character
    SH2-07 opciones alcanzables       desde el menú y desde la pausa, pages=11
    SH2-08 opciones aplican           mismo key, valor creciente (1 → 2)
    SH2-09 creación enganchada        create state=Create, surfaces=16, host del wizard
    SH2-10 creación navegable         ≥2 create step=, steps=10, campos distintos
    SH2-11 creación confirma          create done character=TOKEN → título PLAYING
    SH2-12 sesión con personaje       session state=Started, mismo TOKEN, save=none
    SH2-13 guardar slot v3            save slot=0 bytes>0 digest, tras abrir el browser
    SH2-14 volver al menú             session state=Menu from=Playing → menu state

Modos
-----
    python3 scripts/shell-002.py [--binary PATH] [--skip-build] [--out DIR] [--json]
                                 [--data-root DIR] [--list] [--falsify NAME]
                                 [--self-test] [--pairs N] [--timeout S]

Sin `--out`, cada proceso estrena su propio directorio bajo
`.sintergic/artifacts/shell-002/` (`<UTC>-p<pid>`, ver §`--out` por defecto) y lo
imprime al terminar; con `--out DIR` mide sobre ese directorio, como siempre.

Exit: 0 verde · 1 rojo · 2 NC (sin DISPLAY / sin ARENA2 / sin binario / build
roto) · 3 STALE (binario más viejo que `crates/**` con `--skip-build`).

El driver X11 y los helpers de evidencia se **importan** de `scripts/shell-001.py`
(por ruta; ese fichero no se edita). Lo propio de esta fase es el contrato, las
filas, la secuencia y el control fuerte.
"""

from __future__ import annotations

import argparse
import hashlib
import importlib.util
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
GATE = "SHELL-002"
CR = "CR-75e7c81f-782a-487b-8e8d-96313ae0e13d"
DEFAULT_BINARY = ROOT / "target" / "debug" / "daggerfall-app"
DEFAULT_OUT = ROOT / ".sintergic" / "artifacts" / "shell-002"
DEFAULT_DATA_ROOT = Path(
    os.environ.get("DF_APP_ROOT")
    or Path(os.environ.get("HOME", "")) / "Games" / "Daggerfall" / "app"
)


def default_out_dir() -> Path:
    """`--out` por defecto: UN directorio por proceso, bajo [`DEFAULT_OUT`].

    El defecto medido (CR-d3f0a5b8) era el directorio compartido a secas: dos
    invocaciones concurrentes escribían `manifest.json`/`stdout.txt`/`receipts/`/
    `run{1,2}-saves/` en el mismo sitio, así que la segunda pisaba la evidencia de
    la primera —empezando por `manifest.binary_sha256`, la identidad con la que se
    detecta un binario cambiado a mitad de corrida— y el informe resultante no era
    el de ninguna de las dos.

    El instante UTC de arranque ordena los directorios al leerlos; el `pid`
    desempata dos procesos que arrancan en el mismo segundo.
    """
    stamp = time.strftime("%Y%m%dT%H%M%SZ", time.gmtime())
    return DEFAULT_OUT / ("%s-p%d" % (stamp, os.getpid()))


# ── huella: TODO lo que el gate y el binario escriben ─────────────────────
# Defecto medido (CR-cf456e65): el `--out` se creaba con `mkdir(exist_ok=True)`
# y NADIE vaciaba `run1-saves/`. La segunda corrida arrancaba con el `SAVE0.dfl`
# que dejó la primera, `continue` veía 1 partida donde el contrato exige 0 y el
# gate pasaba de PASS a FAIL **sin que cambiara el binario, el árbol ni la
# orden**. Un juez que sólo acierta sobre un `--out` virgen es SINGLE-SHOT.
#
# `--out` no ES la huella: es sólo el sitio donde cae. La huella se enumeró
# leyendo gate y producto (tabla en la cabecera §Aislamiento y en el HH-1).
RUN_ARTIFACTS = (
    # canal `$DF_SHELL_STATE` (shell-001.py:934) y stdout/stderr de la app
    # (shell-001.py:941-943)
    "run1.state",
    "run2.state",
    "ctrl.state",
    "run1.app.log",
    "run2.app.log",
    "ctrl.app.log",
    # perfil de guardado `DF_SAVE_DIR` (run_scenario) con los slots SAVE0..SAVE5.dfl
    "run1-saves",
    "run2-saves",
    "ctrl-saves",
)
# Informes del propio gate: si sobreviven, el informe de la invocación anterior
# se lee como si fuera el de ésta.
GATE_REPORTS = ("stdout.txt", "manifest.json")
# Un recibo por invocación externa (`receipts/inv-<n>.json`).
GATE_RECEIPTS = "receipts"
# Residuo de producto que puede caer en la RAÍZ de `--out` (un perfil a medio
# camino escribe slots `.dfl` y sesiones `.dfs`): se barre por PATRÓN, porque el
# nombre depende del slot y de la sesión.
PRODUCT_RESIDUE = ("*.dfl", "*.dfs")
# Invocaciones EXTERNAS por par. Una sola no compara nada, y el defecto medido
# vive ENTRE invocaciones, nunca dentro del bucle de un mismo proceso.
IDEMPOTENCE_INVOCATIONS = 2
IDEMPOTENCE_PAIRS = 1
# Línea de identidad de la espina. El `head` se enmascara para comparar dos
# invocaciones: si el resto coincide, el sello se movió SÓLO por el árbol.
IDENTITY_HEAD = re.compile(r"^(gate=\S+ cr=\S+ head=)(\S+)$")
# Interruptor del barrido: SÓLO para el control negativo (demostrar que la fila
# se pone ROJA cuando el aislamiento no cubre el residuo). Jamás en una medición.
ISOLATION_OFF_ENV = "DF_SHELL002_NO_ISOLATION"
# El driver lanza el MISMO comando; estas variables distinguen al hijo (si no,
# recursión infinita) y le dicen qué recibo escribir.
CHILD_ENV = "DF_SHELL002_CHILD"
INVOCATION_ENV = "DF_SHELL002_INVOCATION"


def _wipe(path: Path) -> None:
    """Borra un artefacto: fichero de estado, log, informe o árbol de saves."""
    if path.is_dir() and not path.is_symlink():
        shutil.rmtree(path)
    elif path.exists() or path.is_symlink():
        path.unlink()


def footprint(out: Path, keep: int | None = None) -> list[Path]:
    """Toda la huella bajo `--out`, en orden de barrido (nombres, luego patrón).

    Con `keep` el directorio `receipts/` NO se borra entero: se barre sólo el
    recibo de ESA invocación. Los `inv-<m>.json` de las otras se conservan porque
    están indexados por número (el hijo sólo lee el suyo, que el driver vacía
    antes de lanzarlo, así que no hay herencia posible) y porque un drift necesita
    el `head` de las DOS invocaciones en disco, no sólo del último.
    """
    targets = [out / name for name in RUN_ARTIFACTS + GATE_REPORTS]
    receipts = out / GATE_RECEIPTS
    targets.append(receipts if keep is None else receipts / ("inv-%d.json" % keep))
    for pattern in PRODUCT_RESIDUE:
        targets.extend(sorted(out.glob(pattern)))
    return targets


def isolation_off() -> bool:
    """El control negativo pide saltarse el barrido (`DF_SHELL002_NO_ISOLATION`)."""
    return os.environ.get(ISOLATION_OFF_ENV, "") not in ("", "0")


def wipe_footprint(out: Path, keep: int | None = None) -> list[str]:
    """Vacía la huella ENTERA de una invocación anterior, no sólo su `--out`.

    Devuelve los nombres vaciados; el informe los imprime como `NOTE` (fuera del
    DIGEST) para que se vea qué residuo se descartó antes de medir. Con
    `ISOLATION_OFF_ENV` no se barre nada y la invocación lo declara
    (`isolation=off`): es el control negativo de `PROC-idempotent`. `keep` es la
    invocación en curso: su recibo se barre, los de las otras se conservan.
    """
    out.mkdir(parents=True, exist_ok=True)
    if isolation_off():
        return []
    wiped: list[str] = []
    for target in footprint(out, keep):
        if target.exists() or target.is_symlink():
            _wipe(target)
            wiped.append(target.name)
    return wiped


def _footprint_selftest() -> bool:
    """Guarda mecánica del barrido: cubre la huella entera y no toca lo ajeno.

    Lo ajeno es real: los `--out` de otras mediciones viven bajo el MISMO
    directorio (`c1/`, `CR-*/`), así que un `rmtree(--out)` los borraría.
    """
    keep = os.environ.pop(ISOLATION_OFF_ENV, None)
    try:
        with tempfile.TemporaryDirectory(prefix="shell002-footprint-") as tmp:
            out = Path(tmp)
            owned = [
                out / "run1.state",
                out / "run2.state",
                out / "ctrl.state",
                out / "run1.app.log",
                out / "run2.app.log",
                out / "ctrl.app.log",
                out / "run1-saves" / "SAVE0.dfl",
                out / "run2-saves" / "SAVE5.dfl",
                out / "stdout.txt",
                out / "manifest.json",
                out / "receipts" / "inv-1.json",
                out / "SAVE0.dfl",
                out / "session.dfs",
            ]
            for path in owned:
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text("x", encoding="utf-8")
            trees = [out / "ctrl-saves"]
            for tree in trees:
                tree.mkdir(parents=True, exist_ok=True)
            foreign = [out / "CR-a602cafc" / "stdout.txt", out / "c1" / "note.md", out / "keep.txt"]
            for path in foreign:
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text("x", encoding="utf-8")
            wiped = wipe_footprint(out)
            return (
                all(not path.exists() for path in owned)
                and all(not tree.exists() for tree in trees)
                and all(path.exists() for path in foreign)
                and bool(wiped)
            )
    finally:
        if keep is not None:
            os.environ[ISOLATION_OFF_ENV] = keep


def _footprint_keep_selftest() -> bool:
    """Con `keep`, el barrido NO borra el recibo de la OTRA invocación.

    Un drift necesita el `head` de las DOS invocaciones en disco, y el recibo está
    indexado por número: conservarlo no hereda estado (el hijo sólo lee el suyo, y
    el driver lo vacía justo antes de lanzarlo).
    """
    keep = os.environ.pop(ISOLATION_OFF_ENV, None)
    try:
        with tempfile.TemporaryDirectory(prefix="shell002-keep-") as tmp:
            out = Path(tmp)
            sibling = out / GATE_RECEIPTS / "inv-1.json"
            own = out / GATE_RECEIPTS / "inv-2.json"
            for path in (sibling, own):
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text("x", encoding="utf-8")
            wiped = wipe_footprint(out, keep=2)
            return sibling.exists() and not own.exists() and "inv-2.json" in wiped
    finally:
        if keep is not None:
            os.environ[ISOLATION_OFF_ENV] = keep


# Ficheros de los que depende la fase 2 (fingerprint recomputado desde disco).
MEASURED_SOURCES = (
    "crates/app/src/shell.rs",
    "crates/app/src/m18_window.rs",
    "crates/app/src/m38_surfaces.rs",
    "crates/app/src/m39_block_a.rs",
    "crates/heart/src/session.rs",
    "crates/memory/src/savegame_v2.rs",
    "crates/app/Cargo.toml",
)


def _load_shell1():
    """Importa `scripts/shell-001.py` por ruta (genérico para Python 3.12)."""
    spec = importlib.util.spec_from_file_location("shell_001", ROOT / "scripts" / "shell-001.py")
    module = importlib.util.module_from_spec(spec)
    assert spec.loader is not None
    spec.loader.exec_module(module)
    return module


s1 = _load_shell1()

LABELS = s1.SHELL_LABELS + ("SHELL CREATE",)

# ── anclas `src=` (el path debe resolver; la línea puede derivar) ─────────
ANCHOR_MENU_ORDER = "crates/app/src/shell.rs:44"
ANCHOR_NAV = "crates/app/src/shell.rs:451"
ANCHOR_CONTINUE = "crates/app/src/m39_block_a.rs:75"
ANCHOR_LOAD = "crates/app/src/m39_block_a.rs:54"
ANCHOR_SLOTS = "crates/memory/src/lib.rs:27"
ANCHOR_LOAD_DONE = "crates/heart/src/session.rs:204"
ANCHOR_OPTIONS = "crates/app/src/m38_surfaces.rs:531"
ANCHOR_OPTIONS_SET = "crates/app/src/shell.rs:56"
ANCHOR_CREATE = "crates/app/src/m38_surfaces.rs:346"
ANCHOR_CREATE_STEP = "crates/app/src/m38_surfaces.rs:240"
ANCHOR_CREATE_DONE = "crates/app/src/m38_surfaces.rs:35"
ANCHOR_SESSION = "crates/heart/src/session.rs:261"
ANCHOR_SAVE = "crates/memory/src/savegame_v2.rs:1616"
ANCHOR_BACK_MENU = "crates/app/src/shell.rs:89"
ANCHOR_GATE = "scripts/shell-002.py:1"

TOKEN = r"[a-z0-9-]{1,32}"
ID = r"[a-z0-9-]{1,24}"
HEX16 = r"[0-9a-f]{16}"

# ── contrato (una expresión por línea del bloque de arriba) ───────────────
# ── readiness: el arranque NO consume el presupuesto de la secuencia ──────
# Defecto medido (2026-10-29, bajo 3 lanes concurrentes): la corrida 1 capturaba
# 0 líneas y abortaba en `menu` porque el deadline global arrancaba en `__init__`
# y el arranque de la app se lo comía. Se espera la CONDICIÓN (label de menú en el
# canal) con presupuesto propio y se distingue el motivo del fallo:
#   menu            → lista (la secuencia empieza con presupuesto nuevo)
#   menu-too-late   → el canal escribió (boot/loading) pero el menú no llegó: timing
#   channel-empty   → el canal no escribió NADA con el proceso vivo: arnés/canal
#   exited          → el proceso murió antes del menú: producto
FOREIGN_TITLES = ("SHELL PLAYING", "SHELL PAUSED", "SHELL EXIT")
READY_BUDGET = 60.0
SEQUENCE_BUDGET = 90.0

RE_MENU = re.compile(r"^SHELL VE: menu state=Menu items=(\d+) selection=(\d+) surface=main-menu$")
RE_MENU_ORDER = re.compile(
    r"^SHELL VE: menu order=([a-z0-9-]+(?:,[a-z0-9-]+){4})$"
)
RE_SEL = re.compile(r"^SHELL VE: menu selection=(\d+)$")
RE_CONTINUE_NONE = re.compile(r"^SHELL VE: continue result=none saves=(\d+) notice=no-save$")
RE_CONTINUE_LOADED = re.compile(
    r"^SHELL VE: continue result=loaded slot=(\d+) format=v3$"
)
RE_LOAD_OPEN = re.compile(
    r"^SHELL VE: load open=1 slots=(\d+) format=v3 panel=save-browser$"
)
RE_LOAD_CLOSE = re.compile(r"^SHELL VE: load open=0$")
RE_SLOT = re.compile(
    r"^SHELL VE: slot index=(\d+) name=" + TOKEN + r" level=(\d+) minutes=(\d+) format=v3$"
)
RE_LOAD_DONE = re.compile(
    r"^SHELL VE: load done slot=(\d+) format=v3 state=Playing ticks=(\d+) character=(" + TOKEN + r")$"
)
RE_OPTIONS_OPEN = re.compile(
    r"^SHELL VE: options open=1 page=(" + ID + r") pages=(\d+) panel=pause-menu$"
)
RE_OPTIONS_SET = re.compile(r"^SHELL VE: options set key=(" + ID + r") value=(\d+) pages=(\d+)$")
RE_OPTIONS_CLOSE = re.compile(r"^SHELL VE: options close=1$")
RE_CREATE_STATE = re.compile(
    r"^SHELL VE: create state=Create surfaces=(\d+) host=(\S+) face=(\d+)$"
)
RE_CREATE_STEP = re.compile(r"^SHELL VE: create step=(\d+) steps=(\d+) field=(" + ID + r")$")
RE_CREATE_DONE = re.compile(
    r"^SHELL VE: create done surfaces=(\d+) character=(" + TOKEN + r") state=Playing$"
)
RE_SESSION = re.compile(
    r"^SHELL VE: session state=Started character=(" + TOKEN + r") save=(none|slot=\d+)$"
)
RE_SAVE_OPEN = re.compile(
    r"^SHELL VE: save open=1 slots=(\d+) format=v3 panel=save-browser$"
)
RE_SAVE = re.compile(r"^SHELL VE: save slot=(\d+) format=v3 bytes=(\d+) digest=(" + HEX16 + r")$")
RE_BACK_MENU = re.compile(r"^SHELL VE: session state=Menu from=Playing$")
# Líneas de fase 1 que el escenario necesita para situarse (no se re-miden aquí).
RE_PAUSE = s1.RE_PAUSE
RE_RESUME = s1.RE_RESUME

PHASE1_LINES = (
    s1.RE_BOOT,
    s1.RE_STEP,
    s1.RE_LOADED,
    s1.RE_MENU,
    s1.RE_NEW,
    s1.RE_WORLD,
    s1.RE_HUD,
    s1.RE_MAP_OPEN,
    s1.RE_MAP_CLOSE,
    s1.RE_TRAVEL,
    s1.RE_PAUSE,
    s1.RE_FROZEN,
    s1.RE_RESUME,
    s1.RE_EXIT,
    # `menu order=…` se declara al ENTRAR al menú, sin tecla: es fase 1 y `SH2-01`
    # la exige. Sin esta entrada `check_ctrl` la contaba como fuga de fase 2, así
    # que el control negativo y la fila del menú completo eran excluyentes.
    RE_MENU_ORDER,
    RE_SEL,
)


# ── DIGEST: solo campos estables ─────────────────────────────────────────
# Defecto medido (QA de `CR-a34396f5`): el sello se calculaba sobre el informe
# entero, que arrastra valores de pantalla (`hud … opaque=14720`) y de timing
# (cubo de espera, conteo de líneas capturadas, títulos muestreados). Tres
# corridas al MISMO binario daban sellos distintos (`41d4bc6d…` frente a
# `87f2ec65…`), así que el DIGEST no servía como prueba de reproducibilidad.
# El sello deriva ahora de una espina canónica: identidad (gate/CR/head/binario/
# huella de fuentes) + los eventos del CONTRATO en orden con sus campos estables
# (estados, selecciones, órdenes, contadores deterministas) + el veredicto de
# cada fila + los flags `PROC-*`. El informe sigue imprimiendo los valores
# volátiles para diagnóstico; simplemente no los sella.
RE_HUD_VISIBLE = re.compile(r"^SHELL VE: hud visible=([01]) panel=(\S+) ")

CONTRACT_EVENTS = (
    ("boot", s1.RE_BOOT),
    ("loading", s1.RE_STEP),
    ("loaded", s1.RE_LOADED),
    ("menu", s1.RE_MENU),
    ("menu-order", RE_MENU_ORDER),
    ("selection", RE_SEL),
    ("continue-none", RE_CONTINUE_NONE),
    ("continue-loaded", RE_CONTINUE_LOADED),
    ("load-open", RE_LOAD_OPEN),
    ("load-close", RE_LOAD_CLOSE),
    ("slot", RE_SLOT),
    ("load-done", RE_LOAD_DONE),
    ("options-open", RE_OPTIONS_OPEN),
    ("options-set", RE_OPTIONS_SET),
    ("options-close", RE_OPTIONS_CLOSE),
    ("create", RE_CREATE_STATE),
    ("create-step", RE_CREATE_STEP),
    ("create-done", RE_CREATE_DONE),
    ("session", RE_SESSION),
    ("save-open", RE_SAVE_OPEN),
    ("save", RE_SAVE),
    ("back-to-menu", RE_BACK_MENU),
    ("new-game", s1.RE_NEW),
    ("world", s1.RE_WORLD),
    # El HUD entra por estado y panel; píxeles opacos y digest compuesto son render.
    ("hud", RE_HUD_VISIBLE),
    ("map-open", s1.RE_MAP_OPEN),
    ("map-close", s1.RE_MAP_CLOSE),
    ("travel", s1.RE_TRAVEL),
    ("pause", RE_PAUSE),
    ("frozen", s1.RE_FROZEN),
    ("resumed", s1.RE_RESUME),
    ("exit", s1.RE_EXIT),
)

VOLATILE_OPAQUE = re.compile(r"\bopaque=\d+")
VOLATILE_HUD_DIGEST = re.compile(r"^(SHELL VE: hud .*?)digest=[0-9a-f]{16}$")


def stable_events(cap: dict) -> list[str]:
    """Eventos del contrato en orden, solo con los campos que no son render/timing.

    Una línea que no casa con el contrato no entra: el DIGEST no puede depender
    de ruido de pantalla, y las filas ya juzgan el canal crudo por su cuenta.
    """
    events: list[str] = []
    for line in cap["lines"]:
        for name, pattern in CONTRACT_EVENTS:
            match = pattern.match(line)
            if match is not None:
                groups = " ".join(group for group in match.groups() if group is not None)
                events.append("%s %s" % (name, groups) if groups else name)
                break
    return events


def perturb_volatile(cap: dict) -> dict:
    """Copia con SOLO los campos volátiles cambiados (control positivo del sello).

    Simula otra corrida con distinto render y distinto muestreo de títulos: si el
    sello cambia, el DIGEST depende de algo que no es contrato.
    """
    lines = []
    for line in cap["lines"]:
        line = VOLATILE_OPAQUE.sub(lambda m: "opaque=%d" % (int(m.group(0).split("=")[1]) + 7), line)
        match = VOLATILE_HUD_DIGEST.match(line)
        if match is not None:
            line = match.group(1) + "digest=0123456789abcdef"
        lines.append(line)
    perturbed = dict(cap)
    perturbed["lines"] = lines
    perturbed["titles"] = list(cap["titles"])[:1]
    wait = cap.get("ready_wait_s")
    perturbed["ready_wait_s"] = None if wait is None else wait + 300.0
    return perturbed


def volatile_fields(cap: dict) -> int:
    """Cuántos campos volátiles cambia la perturbación (0 = nada volátil capturado)."""
    other = perturb_volatile(cap)
    return sum(1 for left, right in zip(cap["lines"], other["lines"]) if left != right)


def stable_spine(
    identity: list[str],
    captures: list[tuple[str, dict]],
    rows: list[tuple],
    ctrl_result: tuple,
    flags: list[str],
    verdict: str,
) -> list[str]:
    """Espina canónica que sella el DIGEST: identidad + contrato + veredictos + flags."""
    spine = list(identity)
    for name, cap in captures:
        spine.append(
            "capture=%s window=%d ready=%d reason=%s keys=%d abort=%s exit=%s"
            % (
                name,
                1 if cap["window"] else 0,
                cap.get("ready", 0),
                cap.get("ready_reason") or "-",
                cap["keys"],
                cap["abort"] or "-",
                cap["exit"] if cap["exit"] is not None else "none",
            )
        )
        spine.extend("event %s" % event for event in stable_events(cap))
    for row, result in rows:
        spine.append("row %s ok=%d" % (row.rid, 1 if result[0] else 0))
    spine.append("control %s ok=%d" % (CTRL.rid, 1 if ctrl_result[0] else 0))
    spine.extend(flags)
    spine.append(verdict)
    return spine


def label_of(title: str) -> str:
    for label in LABELS:
        if label in title:
            return label
    return "none"


def labels_of(titles: list[str]) -> list[str]:
    return [label for label in LABELS if any(label in title for title in titles)]


def matching(cap: dict, pattern: re.Pattern[str]) -> list[tuple[int, re.Match[str]]]:
    return [(index, m) for index, line in enumerate(cap["lines"]) if (m := pattern.match(line))]


def first(cap: dict, pattern: re.Pattern[str]):
    hits = matching(cap, pattern)
    return hits[0] if hits else (None, None)


def index_of(cap: dict, pattern: re.Pattern[str]) -> int:
    index, match = first(cap, pattern)
    return -1 if match is None else index


# ── filas: un checker por capacidad ───────────────────────────────────────
# Contrato de retorno: (ok, facts, symptom, evidence_found).
MENU_ORDER = ["new-game", "continue", "load", "options", "quit"]


def check_menu_order(cap: dict):
    _, menu = first(cap, RE_MENU)
    _, order = first(cap, RE_MENU_ORDER)
    if menu is None and order is None:
        return False, "items=none order=none", "no full-menu line was emitted", False
    items = int(menu.group(1)) if menu is not None else 0
    facts = "items=%d order=%s" % (items, order.group(1) if order is not None else "none")
    if items != 5:
        return False, facts, "the menu does not declare 5 items", True
    if order is None:
        return False, facts, "no `menu order=` line: the item set is not declared", True
    if order.group(1) != ",".join(MENU_ORDER):
        return False, facts, "the declared order is not %s" % ",".join(MENU_ORDER), True
    return True, "items=5 order=%s" % ",".join(MENU_ORDER), "", True


def check_nav_full(cap: dict):
    seen = [int(m.group(1)) for _, m in matching(cap, RE_SEL)]
    if not seen:
        return False, "selections=0", "no `menu selection=` line was emitted", False
    needed = [1, 2, 3, 4, 0]
    position = 0
    for value in seen:
        if position < len(needed) and value == needed[position]:
            position += 1
    facts = "selections=%s" % ",".join(str(v) for v in seen)
    if position != len(needed):
        return False, facts, "the selection never visited 1,2,3,4 and back to 0", True
    return True, facts, "", True


def check_continue_no_save(cap: dict):
    _, line = first(cap, RE_CONTINUE_NONE)
    if line is None:
        found = bool(matching(cap, RE_CONTINUE_LOADED))
        return False, "continue_none=0", "continue on an empty profile did not report no-save", found
    index = index_of(cap, RE_CONTINUE_NONE)
    started = index_of(cap, RE_SESSION)
    loaded = index_of(cap, RE_LOAD_DONE)
    facts = "saves=%s session_before=%d load_before=%d" % (
        line.group(1),
        1 if 0 <= started < index else 0,
        1 if 0 <= loaded < index else 0,
    )
    if line.group(1) != "0":
        return False, facts, "continue on an empty profile claims saves>0", True
    if 0 <= started < index or 0 <= loaded < index:
        return False, facts, "continue started a session without a save", True
    return True, "saves=0 notice=no-save session_before=0", "", True


def check_load_browser(cap: dict):
    opens = matching(cap, RE_LOAD_OPEN)
    if not opens:
        return False, "load_open=0", "no save browser was opened", False
    slots = [int(m.group(1)) for _, m in opens]
    facts = "load_open=%d slots=%s" % (len(opens), ",".join(str(s) for s in slots))
    if 0 not in slots:
        return False, facts, "the browser never showed an empty profile (slots=0)", True
    if max(slots) < 1:
        return False, facts, "the browser never showed the written save (slots>=1)", True
    return True, facts + " panel=save-browser format=v3", "", True


def check_slot_manifest(cap: dict):
    slots = matching(cap, RE_SLOT)
    if not slots:
        return False, "slot_lines=0", "no slot manifest line was emitted", False
    index, line = slots[0]
    facts = "slot_lines=%d index=%s level=%s minutes=%s" % (
        len(slots),
        line.group(1),
        line.group(2),
        line.group(3),
    )
    if line.group(1) != "0":
        return False, facts, "the manifest does not start at slot 0", True
    if int(line.group(2)) < 1:
        return False, facts, "slot 0 declares level<1", True
    return True, facts + " format=v3", "", True


def check_load_restores(cap: dict):
    _, done = first(cap, RE_LOAD_DONE)
    _, created = first(cap, RE_CREATE_DONE)
    if done is None:
        return False, "load_done=0", "choosing a slot never restored the session", False
    facts = "slot=%s ticks=%s character=%s" % (done.group(1), done.group(2), done.group(3))
    if int(done.group(2)) < 1:
        return False, facts, "the restored session carries ticks=0 (nothing was saved)", True
    if created is not None and done.group(3) != created.group(2):
        return False, facts + " created=%s" % created.group(2), "the restored character is not the created one", True
    return True, facts + " state=Playing format=v3", "", True


def check_options_reachable(cap: dict):
    opens = matching(cap, RE_OPTIONS_OPEN)
    if not opens:
        return False, "options_open=0", "the options page was never opened", False
    pause = index_of(cap, RE_PAUSE)
    from_pause = [i for i, _ in opens if 0 <= pause < i]
    pages = {int(m.group(2)) for _, m in opens}
    facts = "options_open=%d pages=%s from_pause=%d" % (
        len(opens),
        ",".join(str(p) for p in sorted(pages)),
        len(from_pause),
    )
    if len(opens) < 2:
        return False, facts, "options is not reachable from both the menu and the pause", True
    if not from_pause:
        return False, facts, "options was never opened from the pause", True
    if pages != {11}:
        return False, facts, "the options surface does not declare 11 pages", True
    return True, facts + " pages=11", "", True


def check_options_apply(cap: dict):
    sets = matching(cap, RE_OPTIONS_SET)
    if not sets:
        return False, "options_set=0", "no option value was ever applied", False
    keys = {m.group(1) for _, m in sets}
    values = [int(m.group(2)) for _, m in sets]
    facts = "options_set=%d key=%s values=%s" % (
        len(sets),
        ",".join(sorted(keys)),
        ",".join(str(v) for v in values),
    )
    if len(keys) != 1:
        return False, facts, "the scenario did not exercise one single option key", True
    if len(sets) < 2 or values[-1] <= values[0]:
        return False, facts, "the option value did not advance between the two paths", True
    if any(int(m.group(3)) != 11 for _, m in sets):
        return False, facts, "the applied setting does not report pages=11", True
    return True, facts + " pages=11", "", True


def check_create_hooked(cap: dict):
    _, state = first(cap, RE_CREATE_STATE)
    if state is None:
        return False, "create_state=0", "new game never entered character creation", False
    facts = "surfaces=%s host=%s face=%s" % (state.group(1), state.group(2), state.group(3))
    if int(state.group(1)) != 16:
        return False, facts, "creation does not declare the 16 gated surfaces", True
    if state.group(2) != "DaggerfallStartNewGameWizard":
        return False, facts, "creation is not hosted by the retail wizard", True
    if "SHELL CREATE" not in labels_of(cap["titles"]):
        return False, facts + " title=none", "the title never carried SHELL CREATE", True
    return True, facts + " title=SHELL CREATE", "", True


def check_create_navigable(cap: dict):
    steps = matching(cap, RE_CREATE_STEP)
    if not steps:
        return False, "create_steps=0", "creation emitted no step", False
    numbers = [int(m.group(1)) for _, m in steps]
    fields = [m.group(3) for _, m in steps]
    facts = "create_steps=%d steps=%s fields=%s" % (
        len(steps),
        ",".join(str(n) for n in numbers),
        ",".join(fields),
    )
    if any(int(m.group(2)) != 10 for _, m in steps):
        return False, facts, "the creation flow does not declare 10 steps", True
    if len(steps) < 2 or len(set(fields)) < 2:
        return False, facts, "the flow did not advance through distinct fields", True
    if numbers != sorted(numbers) or numbers[0] != 1:
        return False, facts, "the step index is not monotone from 1", True
    return True, facts + " steps=10", "", True


def check_create_confirm(cap: dict):
    _, done = first(cap, RE_CREATE_DONE)
    if done is None:
        return False, "create_done=0", "creation was never confirmed", False
    facts = "surfaces=%s character=%s" % (done.group(1), done.group(2))
    if done.group(2) == "none":
        return False, facts, "creation confirmed the placeholder character `none`", True
    if int(done.group(1)) != 16:
        return False, facts, "the confirmed character does not report 16 surfaces", True
    if "SHELL PLAYING" not in labels_of(cap["titles"]):
        return False, facts + " title=none", "the title never reached SHELL PLAYING after creation", True
    return True, facts + " state=Playing title=SHELL PLAYING", "", True


def check_session_character(cap: dict):
    index, session = first(cap, RE_SESSION)
    _, done = first(cap, RE_CREATE_DONE)
    if session is None:
        return False, "session=0", "no session was started from the application", False
    facts = "character=%s save=%s" % (session.group(1), session.group(2))
    if done is None:
        return False, facts, "a session started without a confirmed character", True
    if session.group(1) != done.group(2):
        return False, facts + " created=%s" % done.group(2), "the session character is not the created one", True
    created_index = index_of(cap, RE_CREATE_DONE)
    if index < created_index:
        return False, facts, "the session started before creation was confirmed", True
    return True, facts, "", True


def check_save_slot(cap: dict):
    opens = matching(cap, RE_SAVE_OPEN)
    saves = matching(cap, RE_SAVE)
    if not saves:
        return False, "save=0", "no slot was written from the application", False
    index, line = saves[0]
    facts = "slot=%s bytes=%s digest=%s" % (line.group(1), line.group(2), line.group(3))
    if not opens:
        return False, facts, "the slot was written without opening the save browser", True
    if int(line.group(2)) == 0:
        return False, facts, "the written save carries bytes=0", True
    if line.group(1) != "0":
        return False, facts, "the scenario wrote slot 0 but another slot was reported", True
    if int(opens[0][1].group(1)) != 6:
        return False, facts + " slots=%s" % opens[0][1].group(1), "the browser does not declare 6 slots", True
    return True, facts + " slots=6 format=v3", "", True


def check_back_to_menu(cap: dict):
    index = index_of(cap, RE_BACK_MENU)
    if index < 0:
        return False, "back_to_menu=0", "the session never returned to the menu", False
    later_menu = [i for i, _ in matching(cap, RE_MENU) if i > index]
    facts = "back_to_menu=1 menu_after=%d" % len(later_menu)
    if not later_menu:
        return False, facts, "the menu was not re-entered after leaving the session", True
    return True, facts, "", True


ROWS = (
    s1.Row("SH2-01", 3, "menu completo", (), "items=5,order", ("menu-incomplete",), ANCHOR_MENU_ORDER, check_menu_order),
    s1.Row("SH2-02", 3, "navegacion completa", ("Down",), "selection=1,2,3,4,0", ("nav-missing",), ANCHOR_NAV, check_nav_full),
    s1.Row("SH2-03", 3, "continuar sin partida", ("Down", "Return"), "notice=no-save", ("continue-starts",), ANCHOR_CONTINUE, check_continue_no_save),
    s1.Row("SH2-04", 8, "browser de carga", ("Return",), "slots=0,1", ("load-no-browser",), ANCHOR_LOAD, check_load_browser),
    s1.Row("SH2-05", 8, "manifiesto de slots", (), "format=v3", ("slot-v2",), ANCHOR_SLOTS, check_slot_manifest),
    s1.Row("SH2-06", 7, "cargar restaura", ("Return",), "ticks>0,character", ("load-ticks-zero",), ANCHOR_LOAD_DONE, check_load_restores),
    s1.Row("SH2-07", 12, "opciones alcanzables", ("o", "Return"), "pages=11,from_pause", ("options-single-path",), ANCHOR_OPTIONS, check_options_reachable),
    s1.Row("SH2-08", 12, "opciones aplican", ("Right",), "values=1,2", ("options-stuck",), ANCHOR_OPTIONS_SET, check_options_apply),
    s1.Row("SH2-09", 5, "creacion enganchada", ("Return",), "surfaces=16,host", ("create-no-hook",), ANCHOR_CREATE, check_create_hooked),
    s1.Row("SH2-10", 5, "creacion navegable", ("Right", "Down"), "steps=10", ("create-single-step",), ANCHOR_CREATE_STEP, check_create_navigable),
    s1.Row("SH2-11", 5, "creacion confirma", ("Return",), "character=TOKEN", ("create-no-character",), ANCHOR_CREATE_DONE, check_create_confirm),
    s1.Row("SH2-12", 7, "sesion con personaje", (), "session=Started", ("session-token-mismatch",), ANCHOR_SESSION, check_session_character),
    s1.Row("SH2-13", 8, "guardar slot v3", ("s", "Return"), "bytes>0", ("save-v2",), ANCHOR_SAVE, check_save_slot),
    s1.Row("SH2-14", 7, "volver al menu", ("b",), "from=Playing", ("menu-no-return",), ANCHOR_BACK_MENU, check_back_to_menu),
)

CTRL = s1.Row(
    "CTRL-non-shell-key",
    0,
    "control negativo con tecla no-shell",
    ("z",),
    "playing=0 phase2=0 lines>=3",
    ("ctrl-no-channel",),
    ANCHOR_GATE,
    None,
)


def check_ctrl(cap: dict):
    """Control fuerte: canal vivo + tecla no-shell + cero transición + cero fase 2."""
    phase2 = [line for line in cap["lines"] if line.startswith("SHELL VE: ") and not any(
        p.match(line) for p in PHASE1_LINES
    )]
    labels = labels_of(cap["titles"])
    transition = [label for label in labels if label in ("SHELL PLAYING", "SHELL PAUSED", "SHELL EXIT", "SHELL CREATE")]
    facts = "window=%d lines=%d labels=%s phase2=%d keys=%d" % (
        1 if cap["window"] else 0,
        len(cap["lines"]),
        ",".join(labels) or "-",
        len(phase2),
        cap["keys"],
    )
    if not cap["window"]:
        return False, facts, "the app opened no window: the control cannot be judged", False
    if len(cap["lines"]) < 3:
        return False, facts, "the evidence channel is mute: a control over it proves nothing", False
    if transition:
        return False, facts + " transition=%s" % ",".join(transition), "a non-shell key moved the state machine", True
    if phase2:
        return False, facts, "a non-shell key emitted phase-2 evidence", True
    return True, facts, "", True


def evaluate(cap: dict) -> list[tuple]:
    return [(row, row.check(cap)) for row in ROWS]


# ── fixture VERDE embebida + falsificadores ───────────────────────────────
FIXTURE_TITLES = [
    "DAGGERFALL-LIKE — SHELL BOOT",
    "DAGGERFALL-LIKE — SHELL LOADING",
    "DAGGERFALL-LIKE — SHELL MENU",
    "DAGGERFALL-LIKE — SHELL CREATE",
    "DAGGERFALL-LIKE — SHELL PLAYING",
    "DAGGERFALL-LIKE — SHELL PAUSED",
    "DAGGERFALL-LIKE — SHELL EXIT",
]
FIXTURE_LINES = [
    "SHELL VE: boot state=Boot",
    "SHELL VE: loading step=1 steps=3 work=maps units=61",
    "SHELL VE: loading step=2 steps=3 work=blocks units=15151",
    "SHELL VE: loading step=3 steps=3 work=flats units=1253",
    "SHELL VE: loading done steps=3 units=16465",
    "SHELL VE: menu state=Menu items=5 selection=0 surface=main-menu",
    "SHELL VE: menu order=new-game,continue,load,options,quit",
    "SHELL VE: hud visible=0 panel=hud opaque=0 digest=0000000000000000",
    "SHELL VE: menu selection=1",
    "SHELL VE: menu selection=2",
    "SHELL VE: menu selection=3",
    "SHELL VE: menu selection=4",
    "SHELL VE: menu selection=0",
    "SHELL VE: menu selection=1",
    "SHELL VE: continue result=none saves=0 notice=no-save",
    "SHELL VE: menu selection=2",
    "SHELL VE: load open=1 slots=0 format=v3 panel=save-browser",
    "SHELL VE: load open=0",
    "SHELL VE: menu selection=3",
    "SHELL VE: options open=1 page=general pages=11 panel=pause-menu",
    "SHELL VE: options set key=detail value=1 pages=11",
    "SHELL VE: options close=1",
    "SHELL VE: menu selection=4",
    "SHELL VE: menu selection=0",
    "SHELL VE: create state=Create surfaces=16 host=DaggerfallStartNewGameWizard face=0",
    "SHELL VE: create step=1 steps=10 field=face",
    "SHELL VE: create step=2 steps=10 field=name",
    "SHELL VE: create done surfaces=16 character=breton-m state=Playing",
    "SHELL VE: session state=Started character=breton-m save=none",
    "SHELL VE: world scene=Exterior location=(17,50026,0) buildings=647 npcs=25 panels=6",
    "SHELL VE: hud visible=1 panel=hud opaque=51200 digest=0123456789abcdef",
    "SHELL VE: pause state=Paused ticks=12 options=pause-menu",
    "SHELL VE: options open=1 page=general pages=11 panel=pause-menu",
    "SHELL VE: options set key=detail value=2 pages=11",
    "SHELL VE: options close=1",
    "SHELL VE: pause resumed=1 state=Playing",
    "SHELL VE: save open=1 slots=6 format=v3 panel=save-browser",
    "SHELL VE: save slot=0 format=v3 bytes=4096 digest=0123456789abcdef",
    "SHELL VE: session state=Menu from=Playing",
    "SHELL VE: menu state=Menu items=5 selection=0 surface=main-menu",
    "SHELL VE: menu selection=1",
    "SHELL VE: menu selection=2",
    "SHELL VE: load open=1 slots=1 format=v3 panel=save-browser",
    "SHELL VE: slot index=0 name=breton-m level=1 minutes=0 format=v3",
    "SHELL VE: load done slot=0 format=v3 state=Playing ticks=12 character=breton-m",
    "SHELL VE: pause state=Paused ticks=12 options=pause-menu",
    "SHELL VE: exit state=Exit code=0",
]


CONTROL_FIXTURE_TITLES = [
    "DAGGERFALL-LIKE — SHELL BOOT",
    "DAGGERFALL-LIKE — SHELL LOADING",
    "DAGGERFALL-LIKE — SHELL MENU",
]
CONTROL_FIXTURE_LINES = [
    "SHELL VE: boot state=Boot",
    "SHELL VE: loading step=1 steps=3 work=maps units=61",
    "SHELL VE: loading step=2 steps=3 work=blocks units=15151",
    "SHELL VE: loading step=3 steps=3 work=flats units=1253",
    "SHELL VE: loading done steps=3 units=16465",
    "SHELL VE: menu state=Menu items=5 selection=0 surface=main-menu",
    "SHELL VE: hud visible=0 panel=hud opaque=0 digest=0000000000000000",
]


def fixture_control_capture() -> dict:
    return {
        "window": True,
        "alive": 1,
        "titles": list(CONTROL_FIXTURE_TITLES),
        "lines": list(CONTROL_FIXTURE_LINES),
        "exit": None,
        "keys": 1,
        "abort": None,
    }


def fixture_capture() -> dict:
    return {
        "window": True,
        "alive": 1,
        "titles": list(FIXTURE_TITLES),
        "lines": list(FIXTURE_LINES),
        "exit": 0,
        "keys": 5,
        "abort": None,
    }


def _drop(cap: dict, pattern: re.Pattern[str]) -> None:
    cap["lines"] = [line for line in cap["lines"] if not pattern.match(line)]


def _replace(cap: dict, pattern: re.Pattern[str], new: str) -> None:
    kept = []
    done = False
    for line in cap["lines"]:
        match = pattern.match(line)
        if match is not None and not done:
            kept.append(new)
            done = True
            continue
        kept.append(line)
    cap["lines"] = kept


def mut_menu_incomplete(cap: dict) -> str:
    _replace(cap, RE_MENU, "SHELL VE: menu state=Menu items=2 selection=0 surface=main-menu")
    _drop(cap, RE_MENU_ORDER)
    return "menu declares 2 items and no order line"


def mut_nav_missing(cap: dict) -> str:
    cap["lines"] = [
        line for line in cap["lines"] if not re.match(r"^SHELL VE: menu selection=4$", line)
    ]
    return "selection=4 never happened: only the first two items are reachable"


def mut_continue_starts(cap: dict) -> str:
    _replace(
        cap,
        RE_CONTINUE_NONE,
        "SHELL VE: continue result=none saves=0 notice=no-save\nSHELL VE: session state=Started character=breton-m save=none",
    )
    return "continue on an empty profile starts a session"


def mut_load_no_browser(cap: dict) -> str:
    cap["lines"] = [
        line
        for line in cap["lines"]
        if not re.match(r"^SHELL VE: load open=1 slots=0 format=v3 panel=save-browser$", line)
    ]
    return "the browser is never opened on the empty profile (no slots=0 open)"


def mut_slot_v2(cap: dict) -> str:
    _replace(
        cap,
        RE_SLOT,
        "SHELL VE: slot index=0 name=breton-m level=1 minutes=0 format=v2",
    )
    return "the slot manifest reports save format v2"


def mut_load_ticks_zero(cap: dict) -> str:
    _replace(
        cap,
        RE_LOAD_DONE,
        "SHELL VE: load done slot=0 format=v3 state=Playing ticks=0 character=breton-m",
    )
    return "the restored session carries ticks=0"


def mut_options_single_path(cap: dict) -> str:
    seen = 0
    kept = []
    for line in cap["lines"]:
        if RE_OPTIONS_OPEN.match(line):
            seen += 1
            if seen == 2:
                continue
        kept.append(line)
    cap["lines"] = kept
    return "options is only reachable from one path (the pause one is missing)"


def mut_options_stuck(cap: dict) -> str:
    seen = 0
    kept = []
    for line in cap["lines"]:
        if RE_OPTIONS_SET.match(line):
            seen += 1
            if seen == 2:
                kept.append("SHELL VE: options set key=detail value=1 pages=11")
                continue
        kept.append(line)
    cap["lines"] = kept
    return "the option value does not advance on the second path"


def mut_create_no_hook(cap: dict) -> str:
    _drop(cap, RE_CREATE_STATE)
    return "new game never enters creation (no create state line)"


def mut_create_single_step(cap: dict) -> str:
    seen = 0
    kept = []
    for line in cap["lines"]:
        if RE_CREATE_STEP.match(line):
            seen += 1
            if seen == 2:
                continue
        kept.append(line)
    cap["lines"] = kept
    return "creation declares a single step (nothing to navigate)"


def mut_create_no_character(cap: dict) -> str:
    _replace(cap, RE_CREATE_DONE, "SHELL VE: create done surfaces=16 character=none state=Playing")
    return "creation confirms the placeholder character `none`"


def mut_session_token_mismatch(cap: dict) -> str:
    _replace(cap, RE_SESSION, "SHELL VE: session state=Started character=redguard-f save=none")
    return "the session character is not the created one"


def mut_save_v2(cap: dict) -> str:
    _replace(cap, RE_SAVE, "SHELL VE: save slot=0 format=v2 bytes=4096 digest=0123456789abcdef")
    return "the slot is written in save v2, not v3"


def mut_menu_no_return(cap: dict) -> str:
    _drop(cap, RE_BACK_MENU)
    return "the session never returns to the menu"


MUTATORS = {
    "menu-incomplete": (0, mut_menu_incomplete),
    "nav-missing": (1, mut_nav_missing),
    "continue-starts": (2, mut_continue_starts),
    "load-no-browser": (3, mut_load_no_browser),
    "slot-v2": (4, mut_slot_v2),
    "load-ticks-zero": (5, mut_load_ticks_zero),
    "options-single-path": (6, mut_options_single_path),
    "options-stuck": (7, mut_options_stuck),
    "create-no-hook": (8, mut_create_no_hook),
    "create-single-step": (9, mut_create_single_step),
    "create-no-character": (10, mut_create_no_character),
    "session-token-mismatch": (11, mut_session_token_mismatch),
    "save-v2": (12, mut_save_v2),
    "menu-no-return": (13, mut_menu_no_return),
}


# ── fingerprint de fuentes y frescura ─────────────────────────────────────
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
    return (
        "SOURCES fingerprint=%s files=%d paths=%s" % (fingerprint, len(paths), ",".join(paths) or "-"),
        missing,
    )


# ── proceso: la secuencia de fase 2 ───────────────────────────────────────
PHASE2_SEQUENCE = (
    ("menu", None, RE_MENU),
    ("menu-order", None, RE_MENU_ORDER),
    ("sel-1", "Down", re.compile(r"^SHELL VE: menu selection=1$")),
    ("continue", "Return", RE_CONTINUE_NONE),
    ("sel-2", "Down", re.compile(r"^SHELL VE: menu selection=2$")),
    ("load-open", "Return", RE_LOAD_OPEN),
    ("load-close", "Escape", RE_LOAD_CLOSE),
    ("sel-3", "Down", re.compile(r"^SHELL VE: menu selection=3$")),
    ("options-menu", "Return", RE_OPTIONS_OPEN),
    ("options-menu-set", "Right", RE_OPTIONS_SET),
    ("options-menu-close", "Escape", RE_OPTIONS_CLOSE),
    # Desde `options` (3) el camino retail-natural es 3 -> 4 -> 0 con envoltura
    # (`(selection + 1) % MENU_ITEMS.len()`, shell.rs:734): DOS pasos, cada uno
    # declarado por su nombre. Un solo `Down` era aritmética de un menú de 4
    # ítems; el juez abortaba en `sel-0` y dejaba sin medir el resto de la fase 2.
    ("sel-4", "Down", re.compile(r"^SHELL VE: menu selection=4$")),
    ("sel-0", "Down", re.compile(r"^SHELL VE: menu selection=0$")),
    ("create", "Return", RE_CREATE_STATE),
    ("create-step-1", "Right", RE_CREATE_STEP),
    ("create-step-2", "Down", RE_CREATE_STEP),
    ("create-confirm", "Return", RE_CREATE_DONE),
    ("session", None, RE_SESSION),
    ("pause", "Escape", RE_PAUSE),
    ("options-pause", "o", RE_OPTIONS_OPEN),
    ("options-pause-set", "Right", RE_OPTIONS_SET),
    ("options-pause-close", "Escape", RE_OPTIONS_CLOSE),
    ("save-open", "s", RE_SAVE_OPEN),
    ("save-write", "Return", RE_SAVE),
    # El browser de guardado NO se cierra al escribir: el panel es modal y en
    # `ShellPanel::Save` la pausa sólo atiende Down/Up/Escape (`m18_window.rs:1844-1853`,
    # `return` antes de la rama de `b`), así que `b` se pierde con el panel abierto.
    # Se cierra con `Escape` —el mismo idioma que el browser de carga y el panel de
    # opciones— y el estado sigue en `Paused`, que es donde `b` significa «volver al
    # menú». Etapa sin línea propia: la pausa no emite evidencia de cierre de este
    # panel (hueco del canal, se reporta; el juez no lo tapa con un `sleep`).
    ("save-close", "Escape", None),
    ("back-to-menu", "b", RE_BACK_MENU),
    ("sel-1", "Down", re.compile(r"^SHELL VE: menu selection=1$")),
    ("sel-2", "Down", re.compile(r"^SHELL VE: menu selection=2$")),
    ("load-open-2", "Return", RE_LOAD_OPEN),
    ("load-slot", "Return", RE_LOAD_DONE),
    ("pause-2", "Escape", RE_PAUSE),
    ("exit", "q", s1.RE_EXIT),
)


class Shell2Session(s1.Session):
    """Un proceso de producto con perfil de guardado propio (`DF_SAVE_DIR`)."""

    def __init__(self, *args, save_dir: Path, **kwargs):
        super().__init__(*args, **kwargs)
        self.save_dir = save_dir
        self.ready_budget = READY_BUDGET
        self.sequence_budget = SEQUENCE_BUDGET
        self.cap["ready"] = 0
        self.cap["ready_reason"] = "not-started"
        self.cap["ready_wait_s"] = None

    def set_budgets(self, timeout: float) -> None:
        """Los presupuestos nunca son menores que el `--timeout` pedido."""
        self.ready_budget = max(float(timeout), READY_BUDGET)
        self.sequence_budget = max(float(timeout), SEQUENCE_BUDGET)

    def start(self):
        self.save_dir.mkdir(parents=True, exist_ok=True)
        os.environ["DF_SAVE_DIR"] = str(self.save_dir)
        try:
            super().start()
        finally:
            os.environ.pop("DF_SAVE_DIR", None)
        # El descubrimiento de ventana también es arranque: no debe comer la secuencia.
        self.deadline = max(self.deadline, time.monotonic() + self.ready_budget)

    def wait_ready(self) -> bool:
        """Espera la CONDICIÓN de arranque (label de menú), no un `sleep` mayor.

        No avanza `self.cursor`: la secuencia vuelve a validar boot → loading → menú
        desde el principio, así que la espera no esconde una etapa perdida.
        """
        started = time.monotonic()
        ready_deadline = started + self.ready_budget
        while True:
            lines = self.state_lines()
            if any(RE_MENU.match(line) for line in lines):
                self.cap["ready"] = 1
                self.cap["ready_reason"] = "menu"
                self.cap["ready_wait_s"] = round(time.monotonic() - started, 1)
                self.sample()
                self.deadline = time.monotonic() + self.sequence_budget
                return True
            self.sample()
            if self.proc.poll() is not None:
                self.cap["ready_reason"] = "exited" if lines else "exited-no-channel"
                self.cap["abort"] = "ready:%s" % self.cap["ready_reason"]
                return False
            if time.monotonic() >= ready_deadline:
                self.cap["ready_reason"] = "channel-empty" if not lines else "menu-too-late"
                self.cap["abort"] = "ready:%s" % self.cap["ready_reason"]
                self.state_lines()
                return False
            time.sleep(0.05)

    def run_sequence(self):
        if not self.wait_ready():
            return
        for stage, key, pattern in PHASE2_SEQUENCE:
            if key is not None:
                s1.press(self.d, self.win, self.X, self.XK, self.xevent, key)
                self.cap["keys"] += 1
            # Etapa de navegación sin evidencia propia (`pattern=None`): la tecla se
            # pulsa para dejar la app en el estado que la SIGUIENTE etapa exige.
            if pattern is not None and not self.wait(pattern):
                self.cap["abort"] = stage
                return
        try:
            self.proc.wait(timeout=15)
        except Exception:
            pass

    def check_isolation(self) -> None:
        """Aísla la ventana: si la app avanzó de fase sin tecla mía, la corrida es INVÁLIDA.

        Contención de pantalla (otra sonda pulsando sobre la misma ventana) no es carga:
        produce líneas que mi secuencia no pidió y envenenaría el digest sin avisar.
        """
        stages = [stage for stage, _, _ in PHASE2_SEQUENCE]
        advance = stages.index("create")  # primera etapa que puede llegar a `Playing`
        aborted = self.cap["abort"]
        position = stages.index(aborted) if aborted in stages else len(stages)
        foreign = [title for title in self.cap["titles"] if title in FOREIGN_TITLES]
        if foreign and position < advance:
            self.cap["foreign"] = ",".join(foreign)
        else:
            self.cap["foreign"] = ""

    def run_control(self):
        """CTRL fuerte: espera MENU leyendo el canal, pulsa `z`, exige cero transición."""
        if not self.wait_ready():
            self.cap["abort"] = "ctrl-menu"
            self.state_lines()
            return
        s1.press(self.d, self.win, self.X, self.XK, self.xevent, "z")
        self.cap["keys"] = 1
        settle = min(self.deadline, time.monotonic() + 6.0)
        while time.monotonic() < settle:
            self.state_lines()
            self.sample()
            if self.proc.poll() is not None:
                break
            time.sleep(0.1)
        self.state_lines()
        self.sample()


def _wait_bucket(seconds) -> str:
    """Cubo de espera de readiness: estable entre corridas, legible en el NOTE."""
    if seconds is None:
        return "-"
    if seconds < 30:
        return "lt30"
    if seconds < 60:
        return "lt60"
    return "ge60"


def normalize(cap: dict) -> str:
    labels = labels_of(cap["titles"])
    body = "\n".join(cap["lines"])
    return "\n".join(
        (
            "window=%d" % (1 if cap["window"] else 0),
            "alive=%d" % (1 if cap.get("alive") else 0),
            "labels=%s" % (",".join(labels) or "-"),
            "ready=%d" % cap.get("ready", 0),
            "ready_reason=%s" % (cap.get("ready_reason") or "-"),
            "lines=%d" % len(cap["lines"]),
            "lines_sha256=%s" % hashlib.sha256(body.encode("utf-8")).hexdigest(),
            "keys=%d" % cap["keys"],
            "abort=%s" % (cap["abort"] or "-"),
            "exit=%s" % (cap["exit"] if cap["exit"] is not None else "none"),
        )
    )


def render(row, result: tuple) -> str:
    ok, facts, symptom, found = result
    prefix = "[%s]" % ("OK" if ok else "FAIL")
    if ok:
        return "%s %s %s - %s src=%s" % (prefix, row.rid, row.name, facts, row.anchor)
    token = "missing_evidence" if not found else "evidence_mismatch"
    return "%s %s %s - %s %s expected=%s (%s) src=%s" % (
        prefix,
        row.rid,
        row.name,
        token,
        facts,
        row.expected,
        symptom,
        row.anchor,
    )


def emit(lines: list[str], out_path: Path | None, spine: list[str] | None = None) -> None:
    """Imprime y sella. `spine` es la proyección estable; sin ella, el informe tal cual."""
    text = "\n".join(lines) + "\n"
    sealed = text + "DIGEST %s\n" % s1.digest("\n".join(lines if spine is None else spine))
    if out_path is not None:
        out_path.parent.mkdir(parents=True, exist_ok=True)
        out_path.write_text(sealed, encoding="utf-8")
    sys.stdout.write(sealed)


def list_lines() -> list[str]:
    lines = ["gate=%s cr=%s mode=list rows=%d" % (GATE, CR, len(ROWS))]
    for row in ROWS:
        lines.append(
            "%s cov=%d name=%s keys=%s expected=%s failure=%s src=%s"
            % (row.rid, row.cov, row.name, ",".join(row.keys) or "-", row.expected, ",".join(row.failures), row.anchor)
        )
    lines.append(
        "%s cov=0 name=%s keys=%s expected=%s failure=%s src=%s"
        % (CTRL.rid, CTRL.name, ",".join(CTRL.keys), CTRL.expected, ",".join(CTRL.failures), CTRL.anchor)
    )
    lines.append(
        "PROC-idempotent cov=0 name=invocaciones externas keys=- "
        "expected='%d invocaciones externas ⇒ 1 veredicto y 1 DIGEST' "
        "failure=digest-drift,verdict-drift,head-drift,vacuous,single-process src=%s"
        % (IDEMPOTENCE_INVOCATIONS, ANCHOR_GATE)
    )
    lines.append("VEREDICTO-LIST: OK")
    return lines


def mode_list() -> int:
    emit(list_lines(), None)
    return 0


def _order_digest(lines: list[str]) -> str:
    """Sello de una captura con esas líneas: fixture de la sensibilidad al orden."""
    cap = fixture_capture()
    cap["lines"] = list(lines)
    return s1.digest("\n".join(stable_spine(["order-probe"], [("run1", cap)], [], (True, "", "", True), [], "verdict")))


def mode_self_test() -> int:
    lines = ["gate=%s cr=%s mode=self-test" % (GATE, CR)]
    cap = fixture_capture()
    rows = evaluate(cap)
    green = sum(1 for _, result in rows if result[0])
    lines.append("FIXTURE rows=%d green=%d" % (len(rows), green))
    for row, result in rows:
        lines.append(render(row, result))
    control_fixture = fixture_control_capture()
    ctrl_ok = check_ctrl(control_fixture)[0]
    lines.append("[%s] %s fixture-control - %s" % ("OK" if ctrl_ok else "FAIL", CTRL.rid, check_ctrl(control_fixture)[1]))
    caught = 0
    for name, (index, mutator) in MUTATORS.items():
        mutated = fixture_capture()
        symptom = mutator(mutated)
        row, result = evaluate(mutated)[index]
        red = not result[0]
        caught += 1 if red else 0
        lines.append(
            "[%s] FALSIFY %s row=%s mutant=%s" % ("OK" if red else "FAIL", name, row.rid, symptom)
        )
    # Guardas internas del control fuerte.
    guards = []
    mute = fixture_control_capture()
    mute["lines"] = []
    guards.append(("ctrl-no-channel", not check_ctrl(mute)[0]))
    moving = fixture_control_capture()
    moving["titles"] = list(CONTROL_FIXTURE_TITLES) + ["DAGGERFALL-LIKE — SHELL PLAYING"]
    guards.append(("ctrl-transition", not check_ctrl(moving)[0]))
    leaky = fixture_control_capture()
    leaky["lines"] = list(leaky["lines"]) + ["SHELL VE: options open=1 page=general pages=11 panel=pause-menu"]
    guards.append(("ctrl-phase2-leak", not check_ctrl(leaky)[0]))
    # Guarda del sello: perturbar SOLO render/timing no puede mover la espina.
    fixture = fixture_capture()
    spine_probe = "selftest"
    frozen = stable_spine([spine_probe], [("run1", fixture)], evaluate(fixture),
                          check_ctrl(fixture_control_capture()), [], "verdict")
    thawed = stable_spine([spine_probe], [("run1", perturb_volatile(fixture))], evaluate(fixture),
                          check_ctrl(fixture_control_capture()), [], "verdict")
    guards.append(("digest-volatile", frozen == thawed))
    # El sello DEBE seguir siendo sensible al ORDEN de emisión: invertir dos
    # eventos del contrato mueve el DIGEST. Canonicalizar la espina convertiría
    # una carrera real en un verde permanente.
    probe = [line for line in FIXTURE_LINES if any(pattern.match(line) for _, pattern in CONTRACT_EVENTS)]
    distinct_events: list[str] = []
    for line in probe:
        if not distinct_events or stable_events({"lines": [line]}) != stable_events({"lines": [distinct_events[-1]]}):
            distinct_events.append(line)
    swapped = list(distinct_events)
    if len(swapped) >= 2:
        swapped[0], swapped[1] = swapped[1], swapped[0]
    guards.append(
        ("digest-order-sensitive", len(distinct_events) >= 2 and _order_digest(distinct_events) != _order_digest(swapped))
    )
    # Fila `PROC-idempotent`: el comparador tiene que MOVERSE, no puede ser vacuo
    # y no puede aceptar dos recibos del MISMO proceso (bucle interno disfrazado).
    guards.append(
        (
            "idempotent-identical",
            check_idempotent([_receipt_stub(1, 4001, "PASS", "a"), _receipt_stub(2, 4002, "PASS", "a")])[0],
        )
    )
    guards.append(("idempotent-drift", not check_idempotent(IDEMPOTENCE_MUTANTS["idempotent-drift"]())[0]))
    guards.append(("idempotent-vacuous", not check_idempotent(IDEMPOTENCE_MUTANTS["idempotent-vacuous"]())[0]))
    # Control negativo de la fila: residuo que el barrido NO cubrió ⇒ ROJO.
    guards.append(("idempotent-residue", not check_idempotent(IDEMPOTENCE_MUTANTS["idempotent-residue"]())[0]))
    guards.append(
        ("idempotent-single-process", not check_idempotent(IDEMPOTENCE_MUTANTS["idempotent-single-process"]())[0])
    )
    # Causa del drift: nombrar NO indulta. `head-drift` sólo si el `head` es la
    # ÚNICA diferencia; mismo `head` o una diferencia MÁS ⇒ causa de emisión.
    head_drift_red, head_drift_facts = check_idempotent(IDEMPOTENCE_MUTANTS["idempotent-head-drift"]())
    guards.append(("idempotent-head-drift", not head_drift_red and "cause=head-drift" in head_drift_facts))
    emission_red, emission_facts = check_idempotent(IDEMPOTENCE_MUTANTS["idempotent-emission-drift"]())
    guards.append(
        (
            "idempotent-emission-drift",
            not emission_red and "cause=head-drift" not in emission_facts and "cause=emission-drift" in emission_facts,
        )
    )
    mixed_red, mixed_facts = check_idempotent(IDEMPOTENCE_MUTANTS["idempotent-head-plus-emission"]())
    guards.append(
        (
            "idempotent-head-plus-emission",
            not mixed_red and "cause=head-drift" not in mixed_facts and "cause=emission-drift" in mixed_facts,
        )
    )
    # Huella: el barrido cubre todo lo que escriben gate y producto, y no lo ajeno.
    guards.append(("footprint-complete", _footprint_selftest()))
    # El recibo de la OTRA invocación sobrevive al barrido: sin él, un drift se
    # diagnostica con medio `head`.
    guards.append(("footprint-keeps-sibling-receipt", _footprint_keep_selftest()))
    sealed_sample = "a=1\nb=2\nVEREDICTO-GATE: PASS\nINVOCATION-SEAL n=1 pid=9 verdict=PASS digest=d\nDIGEST d\n"
    unsealed_sample = "a=1\nb=2\nVEREDICTO-GATE: FAIL\nDIGEST d\n"
    guards.append(
        (
            "report-body-extract",
            _report_body(sealed_sample) == ["a=1", "b=2", "VEREDICTO-GATE: PASS"]
            and _report_body(unsealed_sample) == ["a=1", "b=2", "VEREDICTO-GATE: FAIL"],
        )
    )
    for name, ok in guards:
        caught += 1 if ok else 0
        lines.append("[%s] GUARD %s" % ("OK" if ok else "FAIL", name))
    all_green = green == len(rows) and ctrl_ok
    ok = all_green and caught == len(MUTATORS) + len(guards)
    lines.append(
        "GATE-SELFTEST rows=%d green=%d falsifiers=%d caught=%d"
        % (len(rows), 1 if all_green else 0, len(MUTATORS) + len(guards), caught)
    )
    fixture_spine = stable_spine(
        ["gate=%s cr=%s mode=self-test" % (GATE, CR)],
        [("run1", fixture_capture()), ("ctrl", fixture_control_capture())],
        evaluate(fixture_capture()),
        check_ctrl(fixture_control_capture()),
        ["selftest"],
        "GATE-SELFTEST",
    )
    emit(lines, None, fixture_spine)
    return 0 if ok else 1


def mode_falsify(name: str) -> int:
    if name in IDEMPOTENCE_MUTANTS:
        identical, facts = check_idempotent(IDEMPOTENCE_MUTANTS[name]())
        ok = not identical
        lines = [
            "gate=%s cr=%s mode=falsify mutant=%s row=PROC-idempotent" % (GATE, CR, name),
            "mutant=%s" % facts,
            "[%s] PROC-idempotent - %s src=%s" % ("FAIL" if identical else "OK", facts, ANCHOR_GATE),
            "VEREDICTO-FALSIFY: %s" % ("PASS" if ok else "FAIL"),
        ]
        emit(lines, None)
        return 0 if ok else 1
    if name not in MUTATORS:
        sys.stderr.write("unknown falsifier: %s\n" % name)
        return 2
    index, mutator = MUTATORS[name]
    cap = fixture_capture()
    symptom = mutator(cap)
    row, result = evaluate(cap)[index]
    lines = [
        "gate=%s cr=%s mode=falsify mutant=%s row=%s" % (GATE, CR, name, row.rid),
        "mutant=%s" % symptom,
        render(row, result),
    ]
    ok = not result[0]
    lines.append("VEREDICTO-FALSIFY: %s" % ("PASS" if ok else "FAIL"))
    emit(lines, None)
    return 0 if ok else 1


def run_scenario(d, X, XK, xevent, binary: Path, data_root: Path, out: Path, timeout: float) -> dict:
    """Una corrida del escenario (run1/run2/ctrl). El barrido NO es cosa suya.

    Antes barría aquí (`isolate(out)` por repetición); ahora lo hace
    `wipe_footprint(out)` UNA vez por invocación externa, que es la unidad que
    `PROC-idempotent` compara. Barrer por repetición medía el bucle interno y
    dejaba fuera justo lo que sólo aparece ENTRE invocaciones.
    """
    captures: list[dict] = []
    for index in (1, 2):
        session = Shell2Session(
            d,
            X,
            XK,
            xevent,
            binary,
            data_root,
            out / ("run%d.state" % index),
            out / ("run%d.app.log" % index),
            timeout,
            save_dir=out / ("run%d-saves" % index),
        )
        session.set_budgets(timeout)
        session.start()
        try:
            if session.open():
                session.run_sequence()
            session.sample()
            session.check_isolation()
        finally:
            session.stop()
        captures.append(session.cap)

    control = Shell2Session(
        d,
        X,
        XK,
        xevent,
        binary,
        data_root,
        out / "ctrl.state",
        out / "ctrl.app.log",
        timeout,
        save_dir=out / "ctrl-saves",
    )
    control.set_budgets(timeout)
    control.start()
    try:
        if control.open():
            control.run_control()
        control.check_isolation()
    finally:
        control.stop()

    norm1, norm2 = normalize(captures[0]), normalize(captures[1])
    mirrored = s1.mirrored_lines(out / "run1.app.log")
    return {
        "captures": captures,
        "control": control.cap,
        "norm1": norm1,
        "norm2": norm2,
        "two_runs_ok": norm1 == norm2,
        "mirrored": mirrored,
        "mirror_ok": bool(captures[0]["lines"]) and mirrored == captures[0]["lines"],
        "ctrl_result": check_ctrl(control.cap),
        "rows": evaluate(captures[0]),
        "contaminated": [
            (name, cap.get("foreign"))
            for name, cap in (("run1", captures[0]), ("run2", captures[1]), ("ctrl", control.cap))
            if cap.get("foreign")
        ],
    }


def seal_invocation(scenario: dict, identity: list[str], sources_ok: bool) -> dict:
    """Veredicto y sello (DIGEST) de UNA invocación del escenario.

    La espina es la MISMA que sella el informe, y el flag de esta misma fila
    (`PROC-idempotent`) queda fuera a propósito: así `seal_invocation` devuelve
    literalmente el `DIGEST` que la invocación imprime, y el driver compara dos
    objetos comparables (mismo comando, mismo `--out`, mismos args).
    """
    captured = [
        ("run1", scenario["captures"][0]),
        ("run2", scenario["captures"][1]),
        ("ctrl", scenario["control"]),
    ]
    rows = scenario["rows"]
    ctrl_result = scenario["ctrl_result"]
    flags = [
        "proc sources ok=%d" % (1 if sources_ok else 0),
        "proc two-runs identical=%d" % (1 if scenario["two_runs_ok"] else 0),
        "proc isolation contaminated=%d" % (1 if scenario["contaminated"] else 0),
        "proc mirror identical=%d" % (1 if scenario["mirror_ok"] else 0),
    ]
    # El sello se comprueba perturbando SOLO render/timing: si el DIGEST se
    # mueve, sella algo que no es contrato.
    seal_probe = "VEREDICTO-GATE: ?"
    digest_stable = stable_spine(identity, captured, rows, ctrl_result, flags, seal_probe) == stable_spine(
        identity,
        [(name, perturb_volatile(cap)) for name, cap in captured],
        rows,
        ctrl_result,
        flags,
        seal_probe,
    )
    flags.append("proc digest-stable identical=%d" % (1 if digest_stable else 0))
    ok = (
        all(result[0] for _, result in rows)
        and ctrl_result[0]
        and sources_ok
        and scenario["two_runs_ok"]
        and scenario["mirror_ok"]
        and not scenario["contaminated"]
        and digest_stable
    )
    verdict = "VEREDICTO-GATE: %s" % ("PASS" if ok else "FAIL")
    spine = stable_spine(identity, captured, rows, ctrl_result, flags, verdict)
    return {
        "scenario": scenario,
        "captured": captured,
        "rows": rows,
        "ctrl_result": ctrl_result,
        "flags": flags,
        "digest_stable": digest_stable,
        "ok": ok,
        "verdict": verdict,
        "digest": s1.digest("\n".join(spine)),
        "spine": spine,
    }


def _receipt_stub(
    invocation: int,
    pid: int,
    verdict: str,
    digest: str,
    residue: str = "",
    head: str = "0" * 40,
    binary_sha256: str = "f" * 64,
    events: tuple[str, ...] = ("menu state=Menu", "menu order=new-game,continue"),
) -> dict:
    """Invocación sintética: sólo para el self-test y los falsificadores.

    Trae `head`, `binary_sha256` y una `spine` de verdad (identidad + eventos +
    veredicto) porque la fila nombra la causa comparando la espina SIN el `head`:
    sin espina no hay diagnóstico, sólo el sello.
    """
    spine = [
        "gate=%s cr=%s head=%s" % (GATE, CR, head),
        "binary=daggerfall-app sha256=%s" % binary_sha256,
        "capture=run1 window=1 ready=1 reason=menu keys=5 abort=- exit=0",
    ]
    spine.extend("event %s" % event for event in events)
    spine.append("VEREDICTO-GATE: %s" % verdict)
    return {
        "invocation": invocation,
        "pid": pid,
        "head": head,
        "binary_sha256": binary_sha256,
        "verdict": "VEREDICTO-GATE: %s" % verdict,
        "digest": digest * 64,
        "residue": residue,
        "spine": spine,
    }


IDEMPOTENCE_MUTANTS = {
    # La invocación 2 diverge: exactamente lo que producía el `--out` heredado
    # (la 2.ª invocación leyendo el `SAVE0.dfl` de la 1.ª).
    "idempotent-drift": lambda: [
        _receipt_stub(1, 4001, "PASS", "a"),
        _receipt_stub(2, 4002, "FAIL", "b"),
    ],
    # Una sola invocación no compara nada: jamás puede leerse como idempotencia.
    "idempotent-vacuous": lambda: [_receipt_stub(1, 4001, "PASS", "a")],
    # Control negativo de la fila: la 2.ª invocación hereda un residuo que el
    # barrido NO cubrió (`run1-saves/SAVE0.dfl`), así que veredicto y sello
    # divergen. Es el control que hoy falta: el comparador viejo pasaba con el
    # defecto vivo porque sólo repetía dentro del proceso.
    "idempotent-residue": lambda: [
        _receipt_stub(1, 4001, "PASS", "a"),
        _receipt_stub(2, 4002, "FAIL", "b", residue="run1-saves/SAVE0.dfl"),
    ],
    # Dos "recibos" del MISMO proceso son el bucle interno disfrazado: la fila
    # mide invocaciones externas y tiene que rechazarlo.
    "idempotent-single-process": lambda: [
        _receipt_stub(1, 4001, "PASS", "a"),
        _receipt_stub(2, 4001, "PASS", "a"),
    ],
    # Un commit ajeno entre invocaciones mueve SÓLO el `head`: el sello cambia y
    # el producto no. La fila sigue ROJA, pero NOMBRA `head-drift` en vez de
    # dejar que el operador lo confunda con una carrera de emisión.
    "idempotent-head-drift": lambda: [
        _receipt_stub(1, 4001, "PASS", "a", head="0b68111" + "0" * 33),
        _receipt_stub(2, 4002, "PASS", "b", head="79441ad" + "1" * 33),
    ],
    # Mismo `head` y secuencia distinta: es el producto emitiendo distinto. Aquí
    # `head-drift` NUNCA puede aparecer.
    "idempotent-emission-drift": lambda: [
        _receipt_stub(1, 4001, "PASS", "a"),
        _receipt_stub(
            2,
            4002,
            "PASS",
            "b",
            events=("menu state=Menu", "load open=1 slots=1 format=v3"),
        ),
    ],
    # `head` movido Y además otra diferencia: manda la otra. Un `head-drift` que
    # tape una carrera real sería peor que el falso rojo que esto arregla.
    "idempotent-head-plus-emission": lambda: [
        _receipt_stub(1, 4001, "PASS", "a", head="0b68111" + "0" * 33),
        _receipt_stub(
            2,
            4002,
            "FAIL",
            "b",
            head="79441ad" + "1" * 33,
            events=("menu state=Menu", "load open=1 slots=1 format=v3"),
        ),
    ],
}


def _head_of(receipt: dict) -> str:
    """`head` con el que corrió la invocación: campo propio, su espina o su manifiesto."""
    head = receipt.get("head")
    if head:
        return str(head)
    for line in receipt.get("spine") or ():
        match = IDENTITY_HEAD.match(str(line))
        if match is not None:
            return match.group(2)
    return str((receipt.get("manifest") or {}).get("head") or "")


def _binary_of(receipt: dict) -> str:
    """`sha256` del binario de la invocación: campo propio o su manifiesto."""
    return str(receipt.get("binary_sha256") or (receipt.get("manifest") or {}).get("binary_sha256") or "")


def _sealed_body(receipt: dict) -> tuple[str, ...] | None:
    """Espina del recibo con el `head` enmascarado: el resto de la identidad, tal cual.

    Es lo que el `DIGEST` sella, así que compararla dice si el sello se movió SÓLO
    por el `head` o por algo más. No se compara `normalize()` completo a propósito:
    lleva render y timing que el sello excluye por contrato, y exigirlos aquí
    convertiría un `head-drift` real en `emission-drift` (el error contrario).
    """
    spine = receipt.get("spine")
    if not spine:
        return None
    return tuple(IDENTITY_HEAD.sub(r"\1<head>", str(line)) for line in spine)


def _heads_fact(reference: str, other: str) -> str:
    """`heads=aaaaaaa->bbbbbbb` para el informe (o `same(…)`/`-`)."""
    if not reference or not other:
        return "heads=-"
    if reference == other:
        return "heads=same(%s)" % reference[:7]
    return "heads=%s->%s" % (reference[:7], other[:7])


def _cause_text(receipts: list[dict], index: int) -> str:
    """Causa del drift, en el informe. Sigue siendo FAIL: nombrar no indulta.

    `head-drift` sólo si el `head` es la ÚNICA diferencia entre la invocación 1 y
    la que drifta: mismo `binary_sha256` y misma espina con el `head` enmascarado
    (identidad, eventos del contrato, filas, flags y veredicto). Cualquier otra
    diferencia manda — un `head-drift` que tape una carrera real de emisión sería
    peor que el falso rojo que esto arregla.
    """
    reference, drifting = receipts[0], receipts[index]
    reference_head, drifting_head = _head_of(reference), _head_of(drifting)
    heads = _heads_fact(reference_head, drifting_head)
    reference_body, drifting_body = _sealed_body(reference), _sealed_body(drifting)
    if reference_body is None or drifting_body is None:
        return (
            "cause=emission-drift %s why=no-spine — una invocación no dejó espina comparable "
            "(ver NOTE drift)" % heads
        )
    if reference_body != drifting_body or _binary_of(reference) != _binary_of(drifting):
        return (
            "cause=emission-drift %s why=sealed-body — difiere algo más que el head "
            "(eventos, filas o binario); el producto emitió distinto (ver NOTE drift)" % heads
        )
    if not (reference_head and drifting_head):
        return (
            "cause=emission-drift %s why=head-unknown — el head no se pudo leer; el sello "
            "no se atribuye al árbol (ver NOTE drift)" % heads
        )
    if reference_head != drifting_head:
        return "cause=head-drift %s — otro proceso commiteó durante la corrida; repite con el árbol quieto" % heads
    return (
        "cause=emission-drift %s why=same-head — mismo head con sellos distintos: "
        "el producto emitió distinto (ver NOTE drift)" % heads
    )


def _drift_index(receipts: list[dict]) -> int | None:
    """Primera invocación cuyo (veredicto, sello) se sale del de la invocación 1."""
    signatures = [(receipt["verdict"], receipt["digest"]) for receipt in receipts]
    return next((index for index, sig in enumerate(signatures) if sig != signatures[0]), None)


def drift_cause(receipts: list[dict]) -> str:
    """Causa del primer drift (`""` si no hay drift): la usa el driver para el NOTE."""
    index = _drift_index(receipts)
    return "" if index is None else _cause_text(receipts, index)


def check_idempotent(receipts: list[dict]) -> tuple[bool, str]:
    """Dos (o más) invocaciones EXTERNAS del gate ⇒ mismo VEREDICTO y mismo DIGEST.

    El bucle interno no vale: repetir el escenario dentro del mismo proceso
    nunca reproduce el defecto medido (CR-cf456e65), que vive ENTRE invocaciones
    — la segunda hereda lo que escribió la primera. Por eso la fila exige además
    `pid` distinto en cada recibo: dos recibos del mismo proceso son el bucle
    interno disfrazado y se declaran ROJOS (`basis=single-process`).
    `first_drift` nombra la primera invocación que se sale de la invocación 1, y
    `cause=` dice QUÉ se movió: `head-drift` (un commit ajeno, no el producto) o
    `emission-drift` (el producto emitió distinto). Nombrar la causa NO indulta:
    el veredicto sigue siendo rojo.
    """
    pids = [str(receipt.get("pid", "-")) for receipt in receipts]
    verdicts = [receipt["verdict"].replace("VEREDICTO-GATE: ", "") for receipt in receipts]
    seals = [str(receipt.get("digest", "")) for receipt in receipts]

    def facts(basis: str, drift: str, cause: str = "") -> str:
        return (
            "basis=%s invocations=%d pids=%s verdicts=%s seals=%s distinct=%d pairs=%d stable=%d first_drift=%s%s"
            % (
                basis,
                len(receipts),
                ",".join(pids) or "-",
                ",".join(verdicts) or "-",
                ",".join(dict.fromkeys(seal[:16] for seal in seals)) or "-",
                len(set(zip(verdicts, seals))),
                len(receipts) // IDEMPOTENCE_INVOCATIONS,
                sum(
                    1
                    for index in range(1, len(receipts), IDEMPOTENCE_INVOCATIONS)
                    if receipts[index]["verdict"] == receipts[index - 1]["verdict"]
                    and receipts[index]["digest"] == receipts[index - 1]["digest"]
                ),
                drift,
                " " + cause if cause else "",
            )
        )

    if len(receipts) < IDEMPOTENCE_INVOCATIONS:
        return False, facts("external-subprocess", "vacuous")
    if any(not receipt.get("pid") for receipt in receipts) or len(set(pids)) != len(pids):
        return False, facts("single-process", "same-pid")
    drift = _drift_index(receipts)
    if drift is None:
        return True, facts("external-subprocess", "-")
    return False, facts("external-subprocess", "inv%d" % (drift + 1), _cause_text(receipts, drift))


def _binary_sha(binary: Path) -> str:
    """`sha256` del binario medido; `-` si aún no existe (una invocación abortada)."""
    try:
        return hashlib.sha256(binary.read_bytes()).hexdigest()
    except OSError:
        return "-"


def receipt_of(invocation: int, head: str = "-", binary_sha256: str = "-", **fields) -> dict:
    """Recibo de UNA invocación externa: veredicto, sello, espina y huella barrida.

    `head` y `binary_sha256` van al recibo de PRIMER nivel —no sólo dentro del
    manifiesto— porque son la identidad con la que se nombra la causa de un
    drift: sin ellos, diagnosticar exige comparar marcas de tiempo de commits
    con el arranque de la corrida, que es justo el trabajo que la fila evita.
    """
    receipt = {
        "gate": GATE,
        "cr": CR,
        "head": head,
        "binary_sha256": binary_sha256,
        "invocation": invocation,
        "pid": os.getpid(),
        "argv": sys.argv,
        "argv_sha256": hashlib.sha256("\0".join(sys.argv).encode("utf-8")).hexdigest(),
        "cwd": str(Path.cwd()),
        "isolation": "off" if isolation_off() else "on",
    }
    receipt.update(fields)
    return receipt


def write_receipt(out: Path, receipt: dict) -> Path:
    """Persiste el recibo de esta invocación (`receipts/inv-<n>.json`)."""
    directory = out / GATE_RECEIPTS
    directory.mkdir(parents=True, exist_ok=True)
    path = directory / ("inv-%d.json" % receipt["invocation"])
    path.write_text(json.dumps(receipt, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    return path


def _write_receipt_safe(out: Path, receipt: dict) -> None:
    """Un fallo al escribir el recibo NO puede tumbar la invocación medida."""
    try:
        write_receipt(out, receipt)
    except OSError as error:
        sys.stderr.write("receipt not written: %s\n" % error)


def mode_invocation(args, invocation: int) -> int:
    """UNA invocación completa del gate: NC, barrido de la huella, escenario, sello.

    La huella se barre AQUÍ y sólo aquí (`wipe_footprint`), antes de medir: es la
    unidad que `PROC-idempotent` compara. El hijo NUNCA escribe `stdout.txt` ni
    `manifest.json` — eso los escribe el driver; si no, la invocación 2 borraría
    el informe de la 1 y la comparación sería de un objeto consigo mismo.
    """
    binary = Path(args.binary).resolve()
    data_root = Path(args.data_root).expanduser().resolve()
    out = Path(args.out).resolve()
    out.mkdir(parents=True, exist_ok=True)
    source_line, missing = sources_line()

    def bail(reason: str, error_row: str) -> int:
        spine = [
            "gate=%s cr=%s mode=run" % (GATE, CR),
            source_line,
            "invocation=%d" % invocation,
            "VEREDICTO-GATE: FAIL",
        ]
        seal = s1.digest("\n".join(spine))
        lines = [
            "gate=%s cr=%s mode=run" % (GATE, CR),
            source_line,
            "[FAIL] PROC-sources - missing=%d casualty=%s src=%s" % (len(missing), reason, ANCHOR_GATE),
            "[FAIL] PROC-run - error=%s src=%s" % (error_row, ANCHOR_GATE),
            "VEREDICTO-GATE: FAIL",
            "INVOCATION-SEAL n=%d pid=%d verdict=VEREDICTO-GATE: FAIL digest=%s"
            % (invocation, os.getpid(), seal),
        ]
        _write_receipt_safe(
            out,
            receipt_of(
                invocation,
                s1.head_commit(),
                _binary_sha(binary),
                verdict="VEREDICTO-GATE: FAIL",
                digest=seal,
                spine=spine,
                ok=False,
                wiped=[],
                invalid=reason,
                manifest={"gate": GATE, "cr": CR, "ok": False, "invalid": reason},
            ),
        )
        emit(lines, None, spine)
        sys.stderr.write("NC: %s\n" % reason)
        return 2

    if not (data_root / "ARENA2").is_dir():
        return bail("arena2=absent", "no-arena2")
    if not args.skip_build:
        built, build_error = s1.build()
        if not built:
            return bail("build=failed", "build")
    if not binary.is_file():
        return bail("binary=absent", "no-binary")

    try:
        X, XK, display_module, xevent = s1._x11()
        d = display_module.Display()
    except Exception:
        return bail("display=absent", "no-display")

    binary_mtime = int(binary.stat().st_mtime)
    newest_mtime, newest_path = s1.newest_source()
    stale = newest_mtime > binary_mtime
    anchors = [(row.rid, row.anchor) for row in ROWS] + [(CTRL.rid, CTRL.anchor)]
    unresolved = [rid for rid, anchor in anchors if not (ROOT / anchor.split(":")[0]).is_file()]
    sources_ok = not missing and not stale and not unresolved
    sources_facts = "declared=%d missing=%d stale=%d anchors=%d unresolved=%d newest=%s" % (
        len(MEASURED_SOURCES),
        len(missing),
        1 if stale else 0,
        len(anchors),
        len(unresolved),
        newest_path if stale else "-",
    )

    head = s1.head_commit()
    binary_sha = _binary_sha(binary)
    relative_binary = str(binary.relative_to(ROOT)) if str(binary).startswith(str(ROOT)) else str(binary)
    identity = [
        "gate=%s cr=%s head=%s" % (GATE, CR, head),
        "binary=%s sha256=%s" % (relative_binary, binary_sha),
        source_line,
    ]

    # Aislamiento: se vacía la huella ENTERA antes de medir (no sólo `--out`).
    # `keep` conserva el recibo de las OTRAS invocaciones: un drift necesita el
    # `head` de las dos en disco, y ese recibo está indexado por número.
    wiped = wipe_footprint(out, keep=invocation)
    scenario = run_scenario(d, X, XK, xevent, binary, data_root, out, args.timeout)
    scenario["wiped"] = wiped
    sealed = seal_invocation(scenario, identity, sources_ok)

    rows = sealed["rows"]
    ctrl_result = sealed["ctrl_result"]
    captures = scenario["captures"]
    digest_stable = sealed["digest_stable"]
    verdict = sealed["verdict"]
    lines = [
        "gate=%s cr=%s head=%s" % (GATE, CR, head[:12]),
        "binary=%s sha256=%s" % (relative_binary, binary_sha),
        source_line,
    ]
    for name, cap in sealed["captured"]:
        lines.append(
            "NOTE capture=%s window=%d lines=%d labels=%s keys=%d abort=%s exit=%s ready=%d reason=%s waited=%s"
            % (
                name,
                1 if cap["window"] else 0,
                len(cap["lines"]),
                ",".join(labels_of(cap["titles"])) or "-",
                cap["keys"],
                cap["abort"] or "-",
                cap["exit"] if cap["exit"] is not None else "none",
                cap.get("ready", 0),
                cap.get("ready_reason") or "-",
                # BUCKET, no segundos: los segundos varían con la carga y romperían el digest.
                _wait_bucket(cap.get("ready_wait_s")),
            )
        )
    lines.append(
        "NOTE isolation invocations=1 location=%s isolation=%s wiped=%s"
        % (out, "off" if isolation_off() else "on", ",".join(wiped) or "-")
    )
    lines.append(
        "NOTE invocation index=%d pid=%d argv_sha256=%s cwd=%s"
        % (
            invocation,
            os.getpid(),
            hashlib.sha256("\0".join(sys.argv).encode("utf-8")).hexdigest()[:16],
            Path.cwd(),
        )
    )
    for row, result in rows:
        lines.append(render(row, result))
    lines.append(render(CTRL, ctrl_result))
    lines.append(
        "[%s] PROC-digest-stable - volatile_fields=%d identical=%d scope=stable excluded=render,timing src=%s"
        % ("OK" if digest_stable else "FAIL", volatile_fields(captures[0]), 1 if digest_stable else 0, ANCHOR_GATE)
    )
    lines.append("[%s] PROC-sources - %s src=%s" % ("OK" if sources_ok else "FAIL", sources_facts, ANCHOR_GATE))
    lines.append(
        "[%s] PROC-two-runs - identical=%d sha256=%s run1_lines=%d run2_lines=%d src=%s"
        % (
            "OK" if scenario["two_runs_ok"] else "FAIL",
            1 if scenario["two_runs_ok"] else 0,
            s1.digest(scenario["norm1"])[:16],
            len(captures[0]["lines"]),
            len(captures[1]["lines"]),
            ANCHOR_GATE,
        )
    )
    lines.append(
        "[%s] PROC-isolation - contaminated=%d detail=%s src=%s"
        % (
            "FAIL" if scenario["contaminated"] else "OK",
            len(scenario["contaminated"]),
            ";".join("%s:%s" % (name, titles) for name, titles in scenario["contaminated"]) or "-",
            ANCHOR_GATE,
        )
    )
    if scenario["contaminated"]:
        lines.append("NOTE invalid-run: input ajeno sobre la ventana (contencion de pantalla, no carga); digest NO congelable")
    lines.append(
        "[%s] PROC-mirror - state=%d stdout=%d identical=%d src=%s"
        % (
            "OK" if scenario["mirror_ok"] else "FAIL",
            len(captures[0]["lines"]),
            len(scenario["mirrored"]),
            1 if scenario["mirrored"] == captures[0]["lines"] else 0,
            ANCHOR_GATE,
        )
    )
    lines.append(verdict)
    lines.append(
        "INVOCATION-SEAL n=%d pid=%d verdict=%s digest=%s rows=%d/%d sources_ok=%d isolation=%s"
        % (
            invocation,
            os.getpid(),
            verdict,
            sealed["digest"],
            sum(1 for _row, result in rows if result[0]) + (1 if ctrl_result[0] else 0),
            len(rows) + 1,
            1 if sources_ok else 0,
            "off" if isolation_off() else "on",
        )
    )

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
        "captures": {
            "run1": scenario["norm1"],
            "run2": scenario["norm2"],
            "ctrl": normalize(scenario["control"]),
        },
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
        "mirror": {
            "ok": scenario["mirror_ok"],
            "state_lines": len(captures[0]["lines"]),
            "stdout_lines": len(scenario["mirrored"]),
        },
        "digest": sealed["digest"],
        "digest_scope": "stable",
        "digest_stable": digest_stable,
        "invocation": {
            "index": invocation,
            "pid": os.getpid(),
            "isolation": "off" if isolation_off() else "on",
            "wiped": wiped,
        },
        "ok": sealed["ok"],
    }
    _write_receipt_safe(
        out,
        receipt_of(
            invocation,
            head,
            binary_sha,
            verdict=verdict,
            digest=sealed["digest"],
            spine=sealed["spine"],
            ok=sealed["ok"],
            wiped=wiped,
            manifest=manifest,
        ),
    )
    emit(lines, None, sealed["spine"])
    if stale:
        sys.stderr.write("STALE: binary older than crates/** (%s)\n" % newest_path)
        return 3
    return 0 if sealed["ok"] else 1


def _report_body(stdout_text: str) -> list[str]:
    """Cuerpo del informe de una invocación, cortado en su línea de sello.

    Si el sello falta (crash a mitad), se devuelve TODO el stdout en vez de
    nada: un informe vacío se leería como «no pasó nada» y es lo contrario.
    """
    head, separator, _tail = stdout_text.rpartition("INVOCATION-SEAL ")
    if not separator:
        head = stdout_text
    return [
        line
        for line in head.splitlines()
        if line.strip() and not line.startswith("DIGEST ")
    ]


def _capture_field(capture, key: str) -> str:
    """Campo de una captura normalizada (`abort=`, `lines=`…): diagnóstico, no contrato."""
    for line in (capture or "").splitlines():
        if line.startswith(key + "="):
            return line.split("=", 1)[1]
    return "-"


def receipt_diagnosis(receipt: dict) -> dict:
    """Qué falló en una invocación: filas rojas y etapa de aborto por corrida.

    Sin esto un drift dice `first_drift=inv2` y nada más: el informe no permite
    distinguir herencia de residuo de un fallo del propio arnés (tecla perdida).
    """
    manifest = receipt.get("manifest") or {}
    captures = manifest.get("captures") or {}
    return {
        "rows_failed": [row.get("id") for row in (manifest.get("rows") or []) if not row.get("ok")],
        "abort": {
            name: "%s/%s" % (_capture_field(capture, "abort"), _capture_field(capture, "lines"))
            for name, capture in captures.items()
        },
        "invalid": receipt.get("invalid"),
        "crashed": receipt.get("crashed"),
    }


def _crashed_receipt(invocation: int, rc: int, body: list[str]) -> dict:
    """Recibo sintético cuando el subproceso no dejó el suyo (crash/timeout)."""
    digest = s1.digest(
        "invocation=%d rc=%d lines=%d" % (invocation, rc, len(body))
    )
    return {
        "invocation": invocation,
        "pid": -invocation,
        # Un crash no dejó identidad: `head`/`binary_sha256` desconocidos, para que
        # la causa del drift NUNCA se atribuya al árbol sin prueba.
        "head": None,
        "binary_sha256": None,
        "verdict": "VEREDICTO-GATE: FAIL",
        "digest": digest,
        "ok": False,
        "spine": [
            "gate=%s cr=%s mode=run" % (GATE, CR),
            "invocation=%d crashed rc=%d" % (invocation, rc),
            "VEREDICTO-GATE: FAIL",
        ],
        "manifest": {"gate": GATE, "cr": CR, "ok": False, "crashed": rc},
        "wiped": [],
        "crashed": rc,
    }


def _rewrite_argv(raw_argv: list[str], overrides: dict) -> list[str]:
    """Absolutiza en el argv del hijo las rutas que el driver ya resolvió.

    El hijo corre con `cwd=ROOT`; una ruta relativa del usuario resolvería a otro
    sitio. Los NOMBRES de los flags no cambian: el comando sigue siendo el mismo.
    """
    argv = list(raw_argv)
    for index, token in enumerate(argv):
        for flag, value in overrides.items():
            if token == flag and index + 1 < len(argv):
                argv[index + 1] = value
            elif token.startswith(flag + "="):
                argv[index] = flag + "=" + value
    return argv


def _has_flag(argv: list[str], flag: str) -> bool:
    """¿El argv trae `flag` en cualquiera de sus dos formas (`flag V` / `flag=V`)?"""
    return any(token == flag or token.startswith(flag + "=") for token in argv)


def _spawn_invocation(args, raw_argv: list[str], invocation: int, out: Path) -> tuple[int, list[str], dict]:
    """Lanza el MISMO comando en un subproceso real y recoge cuerpo + recibo."""
    env = dict(os.environ)
    env[CHILD_ENV] = "1"
    env[INVOCATION_ENV] = str(invocation)
    receipt_path = out / GATE_RECEIPTS / ("inv-%d.json" % invocation)
    _wipe(receipt_path)
    budget = max(float(args.timeout), READY_BUDGET) + max(float(args.timeout), SEQUENCE_BUDGET)
    timeout = 3.0 * budget + 120.0
    proc = subprocess.Popen(
        [sys.executable, str(Path(__file__).resolve()), *list(raw_argv)],
        cwd=str(ROOT),
        env=env,
        stdin=subprocess.DEVNULL,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
    )
    try:
        stdout_text, stderr_text = proc.communicate(timeout=timeout)
    except subprocess.TimeoutExpired:
        proc.kill()
        stdout_text, stderr_text = proc.communicate()
        stderr_text = (stderr_text or "") + "\ninvocation %d killed after %.0fs\n" % (invocation, timeout)
    rc = proc.returncode if proc.returncode is not None else -9
    lines = _report_body(stdout_text)
    if stderr_text.strip() and rc != 0:
        sys.stderr.write("invocation %d stderr: %s\n" % (invocation, stderr_text.strip()[:2000]))
    receipt = None
    if receipt_path.is_file():
        try:
            receipt = json.loads(receipt_path.read_text(encoding="utf-8"))
        except (OSError, json.JSONDecodeError):
            receipt = None
    return rc, lines, receipt or _crashed_receipt(invocation, rc, lines)


def mode_run(args, raw_argv: list[str]) -> int:
    """Driver: N pares de invocaciones EXTERNAS del mismo comando, y su comparación.

    El driver NO mide escenarios (cero filas propias): lanza el comando N veces
    como subprocesos reales sobre el MISMO `--out` y compara veredicto + sello.
    Es la única forma de ver el defecto medido (CR-cf456e65), que vive ENTRE
    invocaciones: el bucle interno del proceso nunca lo reproduce.
    """
    out = Path(args.out).resolve()
    out.mkdir(parents=True, exist_ok=True)
    invocations = max(1, int(args.pairs)) * IDEMPOTENCE_INVOCATIONS
    child_argv = _rewrite_argv(
        raw_argv,
        {
            "--binary": str(Path(args.binary).resolve()),
            "--data-root": str(Path(args.data_root).expanduser().resolve()),
            "--out": str(out),
        },
    )
    if not _has_flag(child_argv, "--out"):
        # `_rewrite_argv` sólo reescribe flags ya presentes: si el operador no dio
        # `--out`, el hijo estrenaría SU propio directorio y `PROC-idempotent`
        # compararía dos objetos de sitios distintos. El hijo hereda el resuelto.
        child_argv += ["--out", str(out)]
    bodies: list[list[str]] = []
    receipts: list[dict] = []
    codes: list[int] = []
    for invocation in range(1, invocations + 1):
        rc, body, receipt = _spawn_invocation(args, child_argv, invocation, out)
        bodies.append(body)
        receipts.append(receipt)
        codes.append(rc)

    first = receipts[0]
    idempotent_ok, idempotent_facts = check_idempotent(receipts)
    cause = drift_cause(receipts)
    ok = bool(first.get("ok")) and idempotent_ok
    verdict = "VEREDICTO-GATE: %s" % ("PASS" if ok else "FAIL")
    spine = [line for line in (first.get("spine") or []) if not line.startswith("VEREDICTO-GATE:")]
    if not spine:
        spine = ["gate=%s cr=%s mode=run unavailable" % (GATE, CR)]
    spine.append(verdict)

    lines = [
        line
        for line in (bodies[0] or ["gate=%s cr=%s mode=run" % (GATE, CR)])
        if not line.startswith("VEREDICTO-GATE:") and not line.startswith("DIGEST-SCOPE ")
    ]
    if getattr(args, "out_defaulted", False):
        # La ruta efectiva, para el operador: el default ya no es un directorio fijo.
        lines.append("NOTE out=%s source=default-per-process" % out)
    lines.append(
        "NOTE idempotence basis=external-subprocess invocations=%d pairs=%d pids=%s heads=%s digests=%s isolation=%s wiped=%s"
        % (
            len(receipts),
            len(receipts) // IDEMPOTENCE_INVOCATIONS,
            ",".join(str(receipt.get("pid")) for receipt in receipts),
            # El `head` de CADA invocación: un drift por commit ajeno se lee aquí
            # sin comparar marcas de tiempo de commits con el arranque.
            ",".join(str(receipt.get("head") or "-")[:7] for receipt in receipts),
            ",".join((receipt.get("digest") or "-")[:16] for receipt in receipts),
            ",".join(receipt.get("isolation") or "-" for receipt in receipts),
            ";".join(
                "inv%d:%s" % (receipt.get("invocation"), ",".join(receipt.get("wiped") or []) or "-")
                for receipt in receipts
            ),
        )
    )
    if not idempotent_ok:
        # Un drift sin diagnóstico no sirve: `first_drift=inv2` no dice si la
        # invocación 2 heredó residuo o si el arnés perdió una tecla. Se nombra
        # CADA invocación (filas rojas + etapa de aborto por corrida).
        for index, receipt in enumerate(receipts):
            caught = receipt_diagnosis(receipt)
            lines.append(
                "NOTE drift inv%d pid=%s head=%s verdict=%s digest=%s rows=%s abort=%s invalid=%s crashed=%s"
                % (
                    index + 1,
                    receipt.get("pid"),
                    str(receipt.get("head") or "-")[:7],
                    str(receipt.get("verdict") or "-").replace("VEREDICTO-GATE: ", ""),
                    str(receipt.get("digest") or "-")[:16],
                    ",".join(caught["rows_failed"]) or "-",
                    ",".join("%s:%s" % (name, value) for name, value in caught["abort"].items()) or "-",
                    caught["invalid"] or "-",
                    caught["crashed"] if caught["crashed"] is not None else "-",
                )
            )
    lines.append("[%s] PROC-idempotent - %s src=%s" % ("OK" if idempotent_ok else "FAIL", idempotent_facts, ANCHOR_GATE))
    lines.append(verdict)
    lines.append(
        "DIGEST-SCOPE stable=%d fields=identity,events,rows,flags excluded=render,timing,idempotence-note"
        % len(spine)
    )

    manifest = dict(first.get("manifest") or {})
    manifest.pop("invocation", None)
    manifest.pop("crashed", None)
    manifest["idempotence"] = {
        "basis": "external-subprocess",
        "invocations": len(receipts),
        "invocations_expected": invocations,
        "pairs": len(receipts) // IDEMPOTENCE_INVOCATIONS,
        "pids": [receipt.get("pid") for receipt in receipts],
        "heads": [receipt.get("head") for receipt in receipts],
        "verdicts": [receipt.get("verdict") for receipt in receipts],
        "seals": [receipt.get("digest") for receipt in receipts],
        "distinct": len({(receipt.get("verdict"), receipt.get("digest")) for receipt in receipts}),
        "identical": idempotent_ok,
        # La causa nombrada del primer drift (`None` si no hay drift): un
        # `head-drift` es un commit ajeno, no una carrera de emisión del producto.
        "cause": cause or None,
        "facts": idempotent_facts,
        "receipts": [
            {
                "invocation": receipt.get("invocation"),
                "pid": receipt.get("pid"),
                "head": receipt.get("head"),
                "binary_sha256": receipt.get("binary_sha256"),
                "verdict": receipt.get("verdict"),
                "digest": receipt.get("digest"),
                "ok": receipt.get("ok"),
                "isolation": receipt.get("isolation"),
                "wiped": receipt.get("wiped"),
                "argv_sha256": receipt.get("argv_sha256"),
                "crashed": receipt.get("crashed"),
                "diagnosis": receipt_diagnosis(receipt),
            }
            for receipt in receipts
        ],
    }
    manifest["child_exit"] = codes
    manifest["digest"] = s1.digest("\n".join(spine))
    manifest["digest_scope"] = "stable"
    manifest["ok"] = ok
    (out / "manifest.json").write_text(json.dumps(manifest, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    emit(lines, out / "stdout.txt", spine)
    if args.json:
        sys.stdout.write(json.dumps(manifest, indent=2, sort_keys=True) + "\n")
    for code in (2, 3, 1):
        if code in codes:
            return code
    return 0 if ok else 1


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--binary", default=str(DEFAULT_BINARY), help="binario de producto")
    parser.add_argument("--skip-build", action="store_true", help="no reconstruir el binario")
    parser.add_argument("--data-root", default=str(DEFAULT_DATA_ROOT), help="datos retail (ARENA2)")
    parser.add_argument(
        "--out",
        default=None,
        help="directorio de artefactos (default: uno propio por proceso bajo %s)" % DEFAULT_OUT,
    )
    parser.add_argument("--json", action="store_true", help="resumen máquina")
    parser.add_argument("--list", action="store_true", help="una línea por fila")
    parser.add_argument("--falsify", metavar="NAME", help="fixture + una mutación: exige rojo")
    parser.add_argument("--self-test", action="store_true", help="fixture verde + falsificadores + guardas")
    parser.add_argument(
        "--pairs",
        type=int,
        default=IDEMPOTENCE_PAIRS,
        help="pares de invocaciones EXTERNAS consecutivas sobre el mismo --out (min 1; "
        "cada par = 2 ejecuciones; default %d)" % IDEMPOTENCE_PAIRS,
    )
    parser.add_argument("--timeout", type=float, default=60.0, help="deadline global por proceso (s)")
    args = parser.parse_args(argv)
    raw_argv = list(argv) if argv is not None else sys.argv[1:]

    # `--out` por defecto: uno por proceso (dos invocaciones concurrentes no
    # comparten artefactos; ver §`--out` por defecto en la cabecera). Con `--out`
    # explícito no se toca nada: el camino es el de siempre.
    args.out_defaulted = args.out is None
    if args.out_defaulted:
        args.out = str(default_out_dir())

    # Hijo: lo lanzó el driver. Ejecuta UNA invocación completa y NO vuelve a
    # dirigir (si no, recursión infinita). El flag de argv es idéntico al del
    # driver: lo único que cambia es el entorno.
    if os.environ.get(CHILD_ENV, "") not in ("", "0"):
        index = os.environ.get(INVOCATION_ENV, "")
        if not index.isdigit() or int(index) < 1:
            sys.stderr.write("child invocation without %s: refusing to drive\n" % INVOCATION_ENV)
            return 2
        return mode_invocation(args, int(index))

    if args.list:
        return mode_list()
    if args.self_test:
        return mode_self_test()
    if args.falsify:
        return mode_falsify(args.falsify)
    return mode_run(args, raw_argv)


if __name__ == "__main__":
    sys.exit(main())
