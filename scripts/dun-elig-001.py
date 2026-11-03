#!/usr/bin/env python3
"""DUN-ELIG-001 — gate de aceptación de M44 W44-3 (bloque C: `M44-R10`..`M44-R13`).

Contrato: `.sintergic/packs/CR-7fd7260f-m44-spec.md` §4 (interfaz fijada) y §4.3
(familias persistidas de save v3); prosa del bloque:
`docs/sintergic/m44/blocks/BC-state-eligibility-CR-7fd7260f.md`.

Qué mide (estado de hoy: **sin implementación**, luego todo `M44-R*` en rojo medido):

* `M44-R10` — token = `f(seed, container, player, epoch)` con golden vectors fijos.
  La función está fijada **bit a bit** en este gate (FNV-1a 64, el mismo primitivo
  que el `checksum=` del save, `crates/memory/src/savegame_v2.rs:18`), así que el
  golden no es «lo que haga la implementación» sino el contrato: 3 tuplas fijas ⇒
  3 tokens de 16 hex que el canal de runtime debe publicar literalmente.
* `M44-R11` — idempotencia `SEEN`: el mismo contenedor dos veces en la misma epoch
  ⇒ contenido byte-idéntico; y el `content_digest` del canal se **recomputa desde
  el artefacto real** de save (`dungeon_loot=`, §4.3), no se cree a la palabra.
* `M44-R12` — epoch nueva invalida tokens: el mismo `(location, container, player)`
  observado en dos epochs publica dos tokens distintos.
* `M44-R13` — lockouts con expiración en minutos: `Locked` mientras
  `retry_minutes < until_minutes` y `Open` en cuanto `retry_minutes >= until_minutes`;
  un lockout de más de `LOCKOUT_MAX_MINUTES` minutos es permanente y se rechaza.

Las tres formas de rojo admitidas por el pack §3 se usan así: **(a) artefacto** — el
save real del producto (`--probe SAVE-ALL-001`) y su censo de familias `dungeon_*`
(`0/4` hoy, luego `dungeon_loot` no existe y ningún `content_digest` es recomputable);
**(b) canal** — los procesos reales del producto arrancados con `DF_DUN_STATE` fijado
escriben `0` líneas `DUN VE: elig|container|lockout` (la ausencia se cuenta, no se
afirma); **(c) censo** — recuentos literales re-ejecutables sobre `crates/**`
(`DUN VE:` = 0/≥1, `dungeon_elig` = 0/≥1, `dungeon_loot` = 0/≥1).

Delta declarado respecto de `scripts/m43_gate_lib.py` (el resto se **importa**):

* **D1 — no se usa `Gate`**: su harness exige que el *probe* imprima `DIGEST` y
  `SOURCES`, y `PROBE-DUNGEON-001`/`SAVE-ALL-001` son de M33/M34 y no los imprimen;
  falsificarlos aquí sería tautología. Se reutilizan `Gate.recompute_fingerprint`,
  `sha256_hex`, `file_sha256`, `citation_check` y `probe_citations`
  sin copiar su código; la guarda bidireccional de citas es local (delta D5).
* **D2 — falsificadores sobre la evidencia de contrato, no sobre `crates/**`**: la
  lane no puede escribir `crates/**` ni correr `cargo build` (leyes del pack), así que
  los falsificadores mutan la evidencia de la **fixture de contrato** (patrón
  `scripts/shell-001.py`). Un falsificador sólo vale si su fila está VERDE antes de
  mutar; la fixture verde demuestra que el rojo de hoy es ausencia de evidencia y no
  un gate que siempre dice rojo.
* **D3 — sin `--verbose`**: `PROBE-DUNGEON-001` imprime su reloj de pared; volcarlo
  rompería la identidad byte a byte entre los dos procesos.
* **D4 — el block doc no entra en `SOURCES fingerprint`**: el documento pega este
  mismo stdout, así que hashearlo haría la pega autorreferente. Su integridad
  funcional la verifica `C-H4` (tabla golden del documento == tabla del gate) y la de
  sus citas, `C-H6`.
* **D5 — guarda de citas bidireccional local**: `probe_citations_guard` de la librería
  ancla su marcador de atribución en la forma `[OK] <id> - <detalle>` (su `OK_LINE`);
  con la gramática del pack (`[OK] <ID> <k>=<v> … src=…`) el `id` es la línea entera y
  el marcador nunca casa, lo que pondría la guarda roja por forma y no por fondo.
  `guard_citations` reproduce la misma disciplina — un fichero ausente y una línea más
  allá del final, ambos deben ir rojos **nombrando el token** — sobre el conjunto de
  citas, con el resolutor de la librería (`citation_check`) y el stdout real de la fila
  de citas (`probe_citations`).

Salidas: filas `[OK]`/`[FAIL]`/`[NC]` con la gramática del pack, `PROBE`/`ARTIFACT`/
`CANAL`/`GOLDEN`/`SOURCES`/`SCOPE`, `VERDICT: PASS|FAIL|NC` (exit 0/1/2) y
`DIGEST <sha256>` como última línea, que cubre todo lo impreso antes (filas + veredicto).
"""

from __future__ import annotations

import argparse
import json
import os
import re
import shutil
import subprocess
import sys
from dataclasses import dataclass
from pathlib import Path
from typing import Callable

sys.path.insert(0, str(Path(__file__).resolve().parent))

from m43_gate_lib import (  # noqa: E402  (import tras fijar sys.path)
    Gate,
    citation_check,
    file_sha256,
    probe_citations,
    sha256_hex,
)

ROOT = Path(__file__).resolve().parent.parent
GATE = "DUN-ELIG-001"
CR = "CR-7fd7260f-ba3d-449a-aea9-e74386a2d5a3"
PACK = ".sintergic/packs/CR-7fd7260f-m44-spec.md"
BLOCK_DOC = "docs/sintergic/m44/blocks/BC-state-eligibility-CR-7fd7260f.md"
DEFAULT_BINARY = ROOT / "target" / "debug" / "daggerfall-app"
DEFAULT_OUT = ROOT / ".sintergic" / "artifacts" / "dun-elig-001"

# ── contrato del bloque C (§4.1/§4.3 del pack) ─────────────────────────────
TOKEN_PREFIX = "dungeon-elig-v1"
TOKEN_TEMPLATE = "dungeon-elig-v1|<seed>|<location>|<container>|<player>|<epoch>"
FNV_OFFSET = 0xCBF29CE484222325
FNV_PRIME = 0x100000001B3
FNV_LIMIT = (1 << 64) - 1
LOCKOUT_MAX_MINUTES = 60
EXPECTED_DUNGEONS = 4_232  # crates/app/src/dungeon_probe.rs:35
DUN_FAMILIES = ("dungeon_state", "dungeon_elig", "dungeon_loot", "dungeon_instance")
DUN_VE_PREFIX = "DUN VE: "
CANAL_PROBES = ("PROBE-DUNGEON-001", "SAVE-ALL-001")

#: Vectores canónicos de FNV-1a 64 (los mismos que el test del árbol,
#: `crates/app/src/m34_light_probe.rs:560`): el instrumento se prueba contra ellos
#: antes de que nadie le crea un golden propio.
CANON_VECTORS: tuple[tuple[bytes, str], ...] = (
    (b"", "cbf29ce484222325"),
    (b"a", "af63dc4c8601ec8c"),
    (b"foobar", "85944171f73967e8"),
)

SOURCES_PATHS = (
    "scripts/dun-elig-001.py",
    "scripts/m43_gate_lib.py",
    PACK,
    "crates/app/src/main.rs",
    "crates/app/src/shell.rs",
    "crates/app/src/dungeon_probe.rs",
    "crates/app/src/m33_probe.rs",
    "crates/app/src/m34_light_probe.rs",
    "crates/memory/src/savegame_v2.rs",
)
CENSUS_ROOT = "crates"
IMPL_MODULE = "crates/app/src/dungeon_elig.rs"


@dataclass(frozen=True)
class Golden:
    """Una tupla fija del contrato y el token de 16 hex que DEBE producir."""

    seed: int
    location: int
    container: int
    player: int
    epoch: int
    token: str


GOLDEN: tuple[Golden, ...] = (
    Golden(12345678901234567, 26219, 0, 1, 0, "6fe1574c48403ecd"),
    Golden(12345678901234567, 26219, 0, 1, 1, "6fe1564c48403d1a"),
    Golden(42, 50015, 3, 2, 5, "a809ee2515123280"),
)

# ── gramática de las líneas que el bloque C promete en `$DF_DUN_STATE` ─────
# Grupos CON NOMBRE: cada checker consume `match.groupdict()`, así que un grupo
# posicional devuelve `{}` y la fila moriría con KeyError en vez de en rojo medido.
RE_ELIG = re.compile(
    r"^DUN VE: elig token=(?P<token>[0-9a-f]{16}) location=(?P<location>\d+) "
    r"container=(?P<container>\d+) player=(?P<player>\d+) epoch=(?P<epoch>\d+)$"
)
RE_CONTENT = re.compile(
    r"^DUN VE: container location=(?P<location>\d+) container=(?P<container>\d+) "
    r"epoch=(?P<epoch>\d+) visit=(?P<visit>[12]) content_digest=(?P<content_digest>[0-9a-f]{16})$"
)
RE_LOCKOUT = re.compile(
    r"^DUN VE: lockout location=(?P<location>\d+) container=(?P<container>\d+) "
    r"player=(?P<player>\d+) until_minutes=(?P<until_minutes>\d+) "
    r"retry_minutes=(?P<retry_minutes>\d+) state=(?P<state>Locked|Open)$"
)
RE_INDEX = re.compile(
    r"^index regions=(\d+) locations=(\d+) dungeons=(\d+) without_dungeon=(\d+) "
    r"maps=(\d+) B blocks=(\d+) B$"
)
RE_PROBE_DIGEST = re.compile(
    r"^DIGEST=([0-9a-f]{16}) \(compare across two separate processes\)$"
)
RE_SAVE_PATH = re.compile(r"^daggerfall-m33-save-all-\d+$")

CENSUS_PATTERNS = {
    "canal_elig": r"DUN VE: elig",
    "canal_container": r"DUN VE: container",
    "canal_lockout": r"DUN VE: lockout",
    "family_elig": r"\bdungeon_elig\b",
    "family_loot": r"\bdungeon_loot\b",
}


# ── utilidades ─────────────────────────────────────────────────────────────
def rel(path: Path) -> str:
    try:
        return os.path.relpath(path, ROOT)
    except ValueError:  # pragma: no cover - rutas de otra unidad
        return str(path)


def own_cite(marker: str) -> str | None:
    """Ancla `path:linea` al propio gate (fail-closed: sin marcador, sin cita)."""
    text = Path(__file__).read_text(encoding="utf-8").splitlines()
    for index, line in enumerate(text, start=1):
        if marker in line:
            return f"scripts/dun-elig-001.py:{index}"
    return None


def impl_cite(marker: str, fallback: str | None) -> str | None:
    """`src=` de una fila `M44-R*`: el módulo de implementación si ya existe, si no el checker.

    El pack (§4) exige que toda fila `[OK]` apunte a `crates/app/src/<modulo>.rs:<linea>`.
    Hoy ese módulo no existe (0 implementación), así que la fila cita el checker que la
    mide; en cuanto la implementación aterrice, la MISMA fila pasa a citarla sin tocar el
    gate. Determinista: ni reloj ni red.
    """
    path = ROOT / IMPL_MODULE
    if path.is_file():
        for index, line in enumerate(path.read_text(encoding="utf-8", errors="replace").splitlines(), start=1):
            if marker in line:
                return f"{IMPL_MODULE}:{index}"
    return fallback


GUARD_ABSENT = "crates/app/src/m44-guard-absent-001.rs"


def guard_citations(text: str, cites: Sequence[str]) -> tuple[bool, str]:
    """G7 en las dos direcciones, respetando la gramática de fila del pack.

    Dirección (a): `probe_citations` de la librería sobre el stdout real de ESTA
    corrida (cada fila `[OK]` resuelve su `src=`). Dirección (b): dos conjuntos de
    citas doctorados que DEBEN ir rojos *nombrando el token* — un fichero ausente y
    una línea más allá del final de un fichero real.

    La librería trae `probe_citations_guard`, pero su marcador de atribución asume la
    forma `[OK] <id> - <detalle>` (su `OK_LINE`): con la gramática del pack
    (`[OK] <ID> <k>=<v> … src=…`) el `id` es la línea entera y el marcador nunca casa,
    así que esa guarda se pondría roja por forma y no por fondo (delta D5). Esta
    reproduce la misma disciplina sobre el conjunto de citas con el resolutor de la
    librería (`citation_check`) y con el mismo texto real.
    """
    probe_ok, probe_detail = probe_citations(text)
    if not probe_ok:
        return False, f"real-probe=RED {probe_detail}"
    real_ok, real_detail = citation_check(cites)
    if not real_ok:
        return False, f"real-citations=RED {real_detail}"
    if (ROOT / GUARD_ABSENT).exists():
        return False, f"anchor-collision={GUARD_ABSENT} exists; the absent direction cannot be built"
    anchor = "crates/app/src/main.rs"
    anchor_path = ROOT / anchor
    if not anchor_path.is_file():
        return False, f"anchor-absent={anchor}"
    with anchor_path.open("rb") as handle:
        top = sum(1 for _ in handle)
    problems: list[str] = []
    evidence: list[str] = []
    for label, token, marker in (
        ("absent", f"{GUARD_ABSENT}:1", f"absent={GUARD_ABSENT}"),
        (f"line>{top}", f"{anchor}:{top + 1}", f"line>{top}={anchor}:{top + 1}"),
    ):
        verdict, detail = citation_check(list(cites) + [token])
        if verdict:
            problems.append(f"{label}=STAYED-GREEN(cite={token})")
        elif marker not in detail:
            problems.append(f"{label}=RED-unattributed({detail[-72:]})")
        else:
            evidence.append(f"{label}=RED({marker})")
    if problems:
        return False, " ".join(problems) + f" real={real_detail}"
    return True, " ".join(evidence) + f" real={real_detail}"


def fnv1a64(data: bytes) -> int:
    """FNV-1a 64 — mismo primitivo que el `checksum=` del save (savegame_v2.rs)."""
    value = FNV_OFFSET
    for byte in data:
        value ^= byte
        value = (value * FNV_PRIME) & FNV_LIMIT
    return value


def token_key(seed: int, location: int, container: int, player: int, epoch: int) -> str:
    return "%s|%d|%d|%d|%d|%d" % (TOKEN_PREFIX, seed, location, container, player, epoch)


def token(seed: int, location: int, container: int, player: int, epoch: int) -> str:
    """`token = "%016x" % fnv1a64(ascii("dungeon-elig-v1|<seed>|<location>|<container>|<player>|<epoch>"))`."""
    return "%016x" % fnv1a64(token_key(seed, location, container, player, epoch).encode("ascii"))


def census(pattern: str, files: list[Path]) -> tuple[int, list[str]]:
    rx = re.compile(pattern)
    hits = 0
    where: list[str] = []
    for path in files:
        text = path.read_text(encoding="utf-8", errors="replace")
        for index, line in enumerate(text.splitlines(), start=1):
            if rx.search(line):
                hits += 1
                if len(where) < 3:
                    where.append(f"{rel(path)}:{index}")
    return hits, where


def census_files() -> list[Path]:
    root = ROOT / CENSUS_ROOT
    files = sorted(root.rglob("*.rs")) + sorted(root.rglob("Cargo.toml"))
    return [path for path in files if path.is_file()]


# ── evidencia del producto ─────────────────────────────────────────────────
def run_probe(binary: Path, probe: str, tag: str, out: Path) -> dict:
    tmp = out / f"tmp-{tag}"
    tmp.mkdir(parents=True, exist_ok=True)
    # Un `TMPDIR` sucio de una corrida anterior duplicaría los artefactos del probe
    # (`files>1`) y rompería la identidad byte a byte entre los dos procesos.
    for child in sorted(tmp.iterdir()):
        if child.is_dir() and not child.is_symlink():
            shutil.rmtree(child)
        else:
            child.unlink()
    log = out / f"dun-ve-{tag}.log"
    if log.exists():
        log.unlink()
    env = {**os.environ, "TMPDIR": str(tmp), "DF_DUN_STATE": str(log)}
    proc = subprocess.run(
        [str(binary), "--probe", probe],
        cwd=str(ROOT),
        capture_output=True,
        timeout=1800,
        check=False,
        env=env,
    )
    stdout = proc.stdout.decode("utf-8", "replace")
    (out / f"{tag}-{probe}.stdout").write_text(stdout, encoding="utf-8")
    canal = (
        [line for line in log.read_text(encoding="utf-8", errors="replace").splitlines()
         if line.startswith(DUN_VE_PREFIX)]
        if log.is_file()
        else []
    )
    return {
        "probe": probe,
        "tag": tag,
        "exit": proc.returncode,
        "stdout": stdout,
        "canal": canal,
        "log_present": log.is_file(),
        "tmp": tmp,
    }


def parse_index(stdout: str) -> dict | None:
    numbers: dict | None = None
    for line in stdout.splitlines():
        match = RE_INDEX.match(line)
        if match is not None:
            numbers = {
                "regions": int(match.group(1)),
                "locations": int(match.group(2)),
                "dungeons": int(match.group(3)),
                "without_dungeon": int(match.group(4)),
            }
    return numbers


def parse_probe_digest(stdout: str) -> str | None:
    digest: str | None = None
    for line in stdout.splitlines():
        match = RE_PROBE_DIGEST.match(line)
        if match is not None:
            digest = match.group(1)
    return digest


def probe_numbers(probe: dict) -> tuple[dict, str | None]:
    """`(index, digest)` del probe: pre-parseados en la fixture, medidos en el producto."""
    index = probe.get("index")
    if index is None:
        index = parse_index(probe.get("stdout", "")) or {}
    digest = probe.get("digest") or parse_probe_digest(probe.get("stdout", ""))
    return index, digest


def save_artifact(tmp: Path, tag: str) -> dict:
    candidates: list[Path] = []
    if tmp.is_dir():
        for child in sorted(tmp.iterdir()):
            if RE_SAVE_PATH.match(child.name) and child.is_dir():
                candidates.extend(sorted(child.glob("save-all-1.dfl")))
    if not candidates:
        return {"present": False, "files": 0, "sha256": None, "text": "", "lines": 0}
    path = candidates[0]
    data = path.read_bytes()
    text = data.decode("utf-8", "replace")
    (tmp.parent / f"save-{tag}.dfl").write_bytes(data)
    return {
        "present": True,
        "files": len(candidates),
        "sha256": sha256_hex(data),
        "text": text,
        "lines": len(text.splitlines()),
    }


def family_census(save_text: str) -> tuple[list[str], dict[str, int]]:
    present: list[str] = []
    entries: dict[str, int] = {}
    for family in DUN_FAMILIES:
        if re.search(rf"^{family}_count=\d+$", save_text, re.MULTILINE) is not None:
            present.append(family)
        entries[family] = len(re.findall(rf"^{family}=", save_text, re.MULTILINE))
    return present, entries


def loot_listing(save_text: str, location: int, container: int) -> list[str]:
    """Líneas canónicas `<source>,<item_id>,<quantity>` de un contenedor (§4.3)."""
    rows: list[str] = []
    for line in save_text.splitlines():
        if not line.startswith("dungeon_loot="):
            continue
        fields = line[len("dungeon_loot="):].split(",")
        if len(fields) < 6:
            continue
        try:
            row_location = int(fields[0])
            row_container = int(fields[1])
        except ValueError:
            continue
        if row_location == location and row_container == container:
            rows.append("%s,%s,%s" % (fields[2], fields[3], fields[4]))
    return sorted(rows)


def content_digest(listing: list[str]) -> str:
    """FNV-1a 64 de las líneas del listado, ordenadas como bytes, cada una con `\\n`."""
    body = "".join(f"{line}\n" for line in listing)
    return "%016x" % fnv1a64(body.encode("utf-8"))


# ── tabla golden del documento (anti-drift documento ↔ gate) ────────────────
def doc_golden() -> dict:
    path = ROOT / BLOCK_DOC
    if not path.is_file():
        return {"rows": None, "line": 0, "present": False, "template": False, "constants": False}
    inside = False
    rows: list[Golden] = []
    first_line = 0
    for index, raw in enumerate(path.read_text(encoding="utf-8").splitlines(), start=1):
        if "<!-- table:golden -->" in raw:
            inside = True
            continue
        if "<!-- /table:golden -->" in raw:
            inside = False
            continue
        if not inside or not raw.strip().startswith("|"):
            continue
        cells = [cell.strip().strip("`") for cell in raw.strip().strip("|").split("|")]
        if len(cells) != 6 or not cells[0].isdigit():
            continue
        rows.append(Golden(int(cells[0]), int(cells[1]), int(cells[2]), int(cells[3]), int(cells[4]), cells[5]))
        if first_line == 0:
            first_line = index
    text = path.read_text(encoding="utf-8")
    return {
        "rows": rows,
        "line": first_line,
        "present": True,
        "template": TOKEN_TEMPLATE in text,
        "constants": "0xcbf29ce484222325" in text.lower() and "0x100000001b3" in text.lower(),
    }


# ── filas ──────────────────────────────────────────────────────────────────
@dataclass(frozen=True)
class Row:
    rid: str
    what: str
    check: Callable[[dict], tuple[bool, str, str, str, str]]
    cite: Callable[[], str | None]


def check_canon(ev: dict) -> tuple[bool, str, str, str, str]:
    reproduced = sum(1 for data, expected in CANON_VECTORS if "%016x" % fnv1a64(data) == expected)
    facts = (
        f"vectors={reproduced}/{len(CANON_VECTORS)} "
        + " ".join("fnv(%s)=%016x" % (data.decode("ascii") or "empty", fnv1a64(data))
                   for data, _ in CANON_VECTORS)
    )
    ok = reproduced == len(CANON_VECTORS)
    return ok, facts, "fnv-vectors", f"{len(CANON_VECTORS)}/{len(CANON_VECTORS)}", "the primitive the golden stands on does not reproduce its canonical vectors"


def check_golden(ev: dict) -> tuple[bool, str, str, str, str]:
    first = [token(g.seed, g.location, g.container, g.player, g.epoch) for g in GOLDEN]
    second = [token(g.seed, g.location, g.container, g.player, g.epoch) for g in GOLDEN]
    matches = sum(1 for value, g in zip(first, GOLDEN) if value == g.token)
    unique = len(set(first))
    shaped = sum(1 for value in first if re.fullmatch(r"[0-9a-f]{16}", value) is not None)
    facts = (
        f"tuples={len(GOLDEN)} stable={first == second} matches={matches}/{len(GOLDEN)} "
        f"unique={unique}/{len(GOLDEN)} hex16={shaped}/{len(GOLDEN)} "
        + " ".join(value for value in first)
    )
    ok = first == second and matches == len(GOLDEN) and unique == len(GOLDEN) and shaped == len(GOLDEN)
    return ok, facts, "golden-set", "3/3 stable, unique and equal to the contract", "the golden set of the gate is not stable or not the declared contract"


def check_epoch_property(ev: dict) -> tuple[bool, str, str, str, str]:
    differing = 0
    for g in GOLDEN:
        low = token(g.seed, g.location, g.container, g.player, g.epoch)
        high = token(g.seed, g.location, g.container, g.player, g.epoch + 1)
        if low != high:
            differing += 1
    facts = f"epoch_pairs={differing}/{len(GOLDEN)} basis={TOKEN_PREFIX} epoch_bump=1"
    ok = differing == len(GOLDEN)
    return ok, facts, "epoch-collision", f"{len(GOLDEN)}/{len(GOLDEN)}", "the token function does not change when the epoch advances"


def check_doc(ev: dict) -> tuple[bool, str, str, str, str]:
    doc = ev["doc"]
    coded = [(g.seed, g.location, g.container, g.player, g.epoch, g.token) for g in GOLDEN]
    if doc["rows"] is None:
        return (
            False,
            f"tuples={len(coded)} table_rows=0 table_line=0",
            "doc-golden-absent",
            str(len(coded)),
            f"{BLOCK_DOC} does not declare the golden table the gate codes",
        )
    declared = [(g.seed, g.location, g.container, g.player, g.epoch, g.token) for g in doc["rows"]]
    facts = (
        f"tuples={len(coded)} table_rows={len(declared)} table_line={doc['line']} "
        f"template={int(doc['template'])} fnv_constants={int(doc['constants'])} "
        f"missing={sum(1 for row in coded if row not in declared)} "
        f"extra={sum(1 for row in declared if row not in coded)}"
    )
    ok = declared == coded and doc["template"] and doc["constants"]
    return ok, facts, "doc-golden-drift", f"{len(coded)} identical rows + template + constants", "gate and document disagree on the token contract"


def check_canal(ev: dict) -> tuple[bool, str, str, str, str]:
    first = ev["canal"]["run1"]
    second = ev["canal"]["run2"]
    logs = [probe["log_present"] for probe in ev["probes"]["run1"].values()]
    facts = (
        f"runs=2 canal_lines={len(first)},{len(second)} equal={first == second} "
        f"mirror_logs={sum(1 for flag in logs if flag)}/2 probes={len(ev['probes']['run1'])}"
    )
    ok = first == second
    return ok, facts, "canal-nondeterministic", "byte-identical", "the two product runs do not publish the same DUN VE: lines"


def check_r10(ev: dict) -> tuple[bool, str, str, str, str]:
    rows = [match.groupdict() for match in (RE_ELIG.match(line) for line in ev["canal"]["run1"]) if match]
    matched = 0
    for g in GOLDEN:
        if any(
            int(row["location"]) == g.location
            and int(row["container"]) == g.container
            and int(row["player"]) == g.player
            and int(row["epoch"]) == g.epoch
            and row["token"] == g.token
            for row in rows
        ):
            matched += 1
    facts = f"elig_rows={len(rows)} golden_tokens={matched}/{len(GOLDEN)} basis={TOKEN_PREFIX} hex=16"
    ok = matched == len(GOLDEN)
    return ok, facts, "token-absent", f"{len(GOLDEN)}/{len(GOLDEN)} golden tokens in the canal", "the runtime canal does not publish the golden tokens of the contract"


def check_r10_census(ev: dict) -> tuple[bool, str, str, str, str]:
    hits, where = ev["census"]["canal_elig"]
    facts = f"census=DUN_VE:elig hits={hits} scope={CENSUS_ROOT}/**/*.rs files={ev['census']['files']} first={','.join(where) or 'none'}"
    ok = hits >= 1
    return ok, facts, "elig-census", ">=1", "not one source line of the tree publishes DUN VE: elig"


def check_r11(ev: dict) -> tuple[bool, str, str, str, str]:
    rows = [match.groupdict() for match in (RE_CONTENT.match(line) for line in ev["canal"]["run1"]) if match]
    grouped: dict[tuple[int, int, int], dict[int, dict]] = {}
    for row in rows:
        key = (int(row["location"]), int(row["container"]), int(row["epoch"]))
        grouped.setdefault(key, {})[int(row["visit"])] = row
    save_text = ev["save"]["run1"]["text"]
    idempotent = 0
    recomputed = 0
    for (location, container, _epoch), visits in grouped.items():
        if 1 not in visits or 2 not in visits:
            continue
        if visits[1]["content_digest"] != visits[2]["content_digest"]:
            continue
        idempotent += 1
        declared = visits[1]["content_digest"]
        if content_digest(loot_listing(save_text, location, container)) == declared:
            recomputed += 1
    facts = (
        f"container_rows={len(rows)} epoch_pairs={len(grouped)} idempotent={idempotent}/1 "
        f"digest_from_artifact={recomputed}/1 save_lines={ev['save']['run1']['lines']}"
    )
    ok = idempotent >= 1 and recomputed >= 1
    return ok, facts, "idempotence", "1 idempotent pair recomputed from the save artifact", "the same container twice in one epoch is not proved to yield the same content"


def check_r12(ev: dict) -> tuple[bool, str, str, str, str]:
    rows = [match.groupdict() for match in (RE_ELIG.match(line) for line in ev["canal"]["run1"]) if match]
    grouped: dict[tuple[int, int, int], dict[int, dict]] = {}
    for row in rows:
        key = (int(row["location"]), int(row["container"]), int(row["player"]))
        grouped.setdefault(key, {})[int(row["epoch"])] = row
    pairs = 0
    invalidated = 0
    for _key, epochs in grouped.items():
        if len(epochs) < 2:
            continue
        pairs += 1
        ordered = [epochs[epoch] for epoch in sorted(epochs)]
        if ordered[0]["token"] != ordered[1]["token"]:
            invalidated += 1
    facts = f"elig_rows={len(rows)} epoch_pairs={pairs}/1 invalidated={invalidated}/{max(pairs, 1)}"
    ok = pairs >= 1 and invalidated == pairs
    return ok, facts, "epoch-reuse", "1 pair, all tokens differ", "a new epoch does not invalidate the tokens of the previous one"


def check_r13(ev: dict) -> tuple[bool, str, str, str, str]:
    rows = [match.groupdict() for match in (RE_LOCKOUT.match(line) for line in ev["canal"]["run1"]) if match]
    locked = [
        row for row in rows
        if row["state"] == "Locked"
        and int(row["retry_minutes"]) < int(row["until_minutes"])
        and 0 < int(row["until_minutes"]) - int(row["retry_minutes"]) <= LOCKOUT_MAX_MINUTES
    ]
    expired = [
        row for row in rows
        if row["state"] == "Open" and int(row["retry_minutes"]) >= int(row["until_minutes"])
    ]
    pairs = 0
    for lock in locked:
        if any(
            other["location"] == lock["location"]
            and other["container"] == lock["container"]
            and other["player"] == lock["player"]
            for other in expired
        ):
            pairs += 1
    facts = (
        f"lockout_rows={len(rows)} locked={len(locked)}/1 expired={len(expired)}/1 "
        f"bounded_pairs={pairs}/1 max_minutes={LOCKOUT_MAX_MINUTES}"
    )
    ok = len(locked) >= 1 and len(expired) >= 1 and pairs >= 1
    return ok, facts, "lockout-permanent", "1 bounded lockout + 1 expired observation", "a lockout is not proved to expire in minutes"


def check_r13_census(ev: dict) -> tuple[bool, str, str, str, str]:
    lockout_hits, lockout_where = ev["census"]["canal_lockout"]
    container_hits, _ = ev["census"]["canal_container"]
    facts = (
        f"census=DUN_VE:lockout hits={lockout_hits} DUN_VE:container hits={container_hits} "
        f"scope={CENSUS_ROOT}/**/*.rs files={ev['census']['files']} first={','.join(lockout_where) or 'none'}"
    )
    ok = lockout_hits >= 1 and container_hits >= 1
    return ok, facts, "lockout-census", ">=1 each", "the tree publishes neither lockout nor container lines"


def check_citations(ev: dict) -> tuple[bool, str, str, str, str]:
    text = ev["rendered"]
    cites = sorted(ev["cites"])
    cited, cite_detail = citation_check(cites)
    probe_ok, probe_detail = probe_citations(text)
    guard_ok, guard_detail = guard_citations(text, cites)
    facts = (
        f"src_tokens={len(cites)} resolved={cited} probe-src={probe_ok} guard={guard_ok} "
        f"guard-detail={guard_detail.replace(' ', ',')} probe-detail=ok-rows="
        f"{probe_detail.split('ok-rows=')[1].split(' ')[0] if 'ok-rows=' in probe_detail else '0'} "
        f"unresolved={cite_detail.split('unresolved=')[1].split(' ')[0] if 'unresolved=' in cite_detail else 'na'}"
    )
    ok = cited and probe_ok and guard_ok
    return ok, facts, "citations", "all resolved + guard bites both ways", "a src= of this gate does not resolve against the tree"


ROWS: tuple[Row, ...] = (
    Row(
        "C-H1",
        "el primitivo del golden (FNV-1a 64) reproduce sus vectores canónicos",
        check_canon,
        lambda: "crates/app/src/m34_light_probe.rs:560",
    ),
    Row(
        "C-H2",
        "el golden set es estable, único y igual al contrato declarado",
        check_golden,
        lambda: own_cite("def check_golden("),
    ),
    Row(
        "C-H3",
        "la epoch nueva cambia el token (propiedad de la función declarada)",
        check_epoch_property,
        lambda: own_cite("def check_epoch_property("),
    ),
    Row(
        "C-H4",
        "la tabla golden y las constantes del documento son LAS MISMAS que codifica el gate",
        check_doc,
        lambda: f"{BLOCK_DOC}:{doc_golden()['line']}" if doc_golden()["line"] else None,
    ),
    Row(
        "C-H5",
        "el canal DUN VE: se mide en 2 corridas reales y ambas publican exactamente lo mismo",
        check_canal,
        lambda: "crates/app/src/shell.rs:210",
    ),
    Row(
        "M44-R10",
        "los 3 golden tokens del contrato se publican en el canal de runtime",
        check_r10,
        lambda: impl_cite("DUN VE: elig", own_cite("def check_r10(")),
    ),
    Row(
        "M44-R10.census",
        "el canal DUN VE: elig existe en el árbol (censo re-ejecutable)",
        check_r10_census,
        lambda: impl_cite("DUN VE:", own_cite("def check_r10_census(")),
    ),
    Row(
        "M44-R11",
        "mismo contenedor dos veces en la misma epoch = contenido idéntico, recomputado desde el save",
        check_r11,
        lambda: impl_cite("DUN VE: container", own_cite("def check_r11(")),
    ),
    Row(
        "M44-R12",
        "una epoch nueva invalida los tokens de la anterior",
        check_r12,
        lambda: impl_cite("DUN VE: elig", own_cite("def check_r12(")),
    ),
    Row(
        "M44-R13",
        "los lockouts expiran en minutos (y ninguno es permanente)",
        check_r13,
        lambda: impl_cite("DUN VE: lockout", own_cite("def check_r13(")),
    ),
    Row(
        "M44-R13.census",
        "las líneas de lockout y de contenedor existen en el árbol (censo re-ejecutable)",
        check_r13_census,
        lambda: impl_cite("DUN VE:", own_cite("def check_r13_census(")),
    ),
)

CITATION_ROW = Row(
    "C-H6",
    "toda cita src= de este gate resuelve contra el árbol y la guarda muerde en las dos direcciones",
    check_citations,
    lambda: own_cite("def check_citations("),
)

FIXTURE_INDEX = {"regions": 45, "locations": 15_251, "dungeons": EXPECTED_DUNGEONS, "without_dungeon": 11_019}
FIXTURE_DIGEST = "5573c56afce6749a"
FIXTURE_SAVE_LOCATION = 26219
FIXTURE_LOCK_LOCATION = 26219
FIXTURE_LOCK_CONTAINER = 3
FIXTURE_LOCK_PLAYER = 1
FIXTURE_LOCK_UNTIL = 40
FIXTURE_LOCK_RETRY = 10


# ── fixture de contrato (verde) y falsificadores ───────────────────────────
def fixture_save() -> str:
    lines = [
        "DFL-SESSION 3",
        "pos_x_mm=1500",
        "pos_z_mm=-2000",
        "gold=55",
        "dungeon_state_count=1",
        "dungeon_state=26219,0,6,Cleared,120,0",
        "dungeon_elig_count=2",
        "dungeon_elig=6fe1574c48403ecd,26219,0,1,0,2",
        "dungeon_elig=6fe1564c48403d1a,26219,0,1,1,0",
        "dungeon_loot_count=2",
        "dungeon_loot=26219,0,Boss,0,1,0",
        "dungeon_loot=26219,0,Barrel,7,2,0",
        "dungeon_instance_count=1",
        "dungeon_instance=0a1b2c3d4e5f6071,26219,0,1",
    ]
    body = "\n".join(lines) + "\n"
    return body + "checksum=%016x\n" % fnv1a64(body.encode("utf-8"))


def fixture_canal() -> list[str]:
    lines: list[str] = []
    for g in GOLDEN:
        lines.append(
            "DUN VE: elig token=%s location=%d container=%d player=%d epoch=%d"
            % (g.token, g.location, g.container, g.player, g.epoch)
        )
    save_text = fixture_save()
    digest = content_digest(loot_listing(save_text, FIXTURE_SAVE_LOCATION, 0))
    lines.append(
        "DUN VE: container location=%d container=0 epoch=0 visit=1 content_digest=%s"
        % (FIXTURE_SAVE_LOCATION, digest)
    )
    lines.append(
        "DUN VE: container location=%d container=0 epoch=0 visit=2 content_digest=%s"
        % (FIXTURE_SAVE_LOCATION, digest)
    )
    lines.append(
        "DUN VE: lockout location=%d container=%d player=%d until_minutes=%d retry_minutes=%d state=Locked"
        % (FIXTURE_LOCK_LOCATION, FIXTURE_LOCK_CONTAINER, FIXTURE_LOCK_PLAYER,
           FIXTURE_LOCK_UNTIL, FIXTURE_LOCK_RETRY)
    )
    lines.append(
        "DUN VE: lockout location=%d container=%d player=%d until_minutes=%d retry_minutes=%d state=Open"
        % (FIXTURE_LOCK_LOCATION, FIXTURE_LOCK_CONTAINER, FIXTURE_LOCK_PLAYER,
           FIXTURE_LOCK_UNTIL, FIXTURE_LOCK_UNTIL)
    )
    return lines


def fixture_evidence() -> dict:
    save_text = fixture_save()
    canal = fixture_canal()
    return {
        "probes": {
            "run1": {
                "dungeon": {"exit": 0, "stdout": "", "canal": [], "log_present": True,
                            "index": dict(FIXTURE_INDEX), "digest": FIXTURE_DIGEST},
                "save": {"exit": 0, "stdout": "", "canal": [], "log_present": True,
                         "index": None, "digest": None},
            },
            "run2": {
                "dungeon": {"exit": 0, "stdout": "", "canal": [], "log_present": True,
                            "index": dict(FIXTURE_INDEX), "digest": FIXTURE_DIGEST},
                "save": {"exit": 0, "stdout": "", "canal": [], "log_present": True,
                         "index": None, "digest": None},
            },
        },
        "canal": {"run1": list(canal), "run2": list(canal)},
        "save": {
            "run1": {"present": True, "files": 1, "sha256": "0" * 64, "text": save_text,
                     "lines": len(save_text.splitlines())},
            "run2": {"present": True, "files": 1, "sha256": "0" * 64, "text": save_text,
                     "lines": len(save_text.splitlines())},
        },
        "census": {
            "files": 1,
            "canal_elig": (len(GOLDEN), ["fixture:1"]),
            "canal_container": (2, ["fixture:2"]),
            "canal_lockout": (2, ["fixture:3"]),
            "family_elig": (2, ["fixture:4"]),
            "family_loot": (2, ["fixture:5"]),
        },
        "doc": {
            "rows": list(GOLDEN),
            "line": 1,
            "present": True,
            "template": True,
            "constants": True,
        },
        "index": dict(FIXTURE_INDEX),
    }


def _rewrite_save(ev: dict, text: str) -> None:
    ev["save"]["run1"]["text"] = text
    ev["save"]["run1"]["lines"] = len(text.splitlines())


def falsify_token_mismatch(ev: dict) -> str:
    lines = list(ev["canal"]["run1"])
    for index, line in enumerate(lines):
        if line.startswith("DUN VE: elig "):
            lines[index] = line.replace(GOLDEN[0].token, "0" * 16, 1)
            break
    ev["canal"]["run1"] = lines
    ev["canal"]["run2"] = list(lines)
    return f"the canal publishes token={'0' * 16} for the first golden tuple instead of {GOLDEN[0].token}"


def falsify_digest_drift(ev: dict) -> str:
    lines = []
    for line in ev["canal"]["run1"]:
        if line.startswith("DUN VE: container ") and "visit=2" in line:
            head, _, digest = line.rpartition("content_digest=")
            flipped = "%016x" % (int(digest, 16) ^ 0xFF)
            lines.append(f"{head}content_digest={flipped}")
            continue
        lines.append(line)
    ev["canal"]["run1"] = lines
    ev["canal"]["run2"] = list(lines)
    return "the second visit of the same container declares a different content_digest"


def falsify_epoch_collides(ev: dict) -> str:
    lines = []
    for line in ev["canal"]["run1"]:
        if line.startswith("DUN VE: elig ") and f"epoch={GOLDEN[1].epoch}" in line:
            lines.append(line.replace(GOLDEN[1].token, GOLDEN[0].token, 1))
            continue
        lines.append(line)
    ev["canal"]["run1"] = lines
    ev["canal"]["run2"] = list(lines)
    return f"epoch={GOLDEN[1].epoch} reuses the token of epoch={GOLDEN[0].epoch}, so the new epoch invalidates nothing"


def falsify_lockout_permanent(ev: dict) -> str:
    lines = []
    for line in ev["canal"]["run1"]:
        if line.startswith("DUN VE: lockout ") and "state=Locked" in line:
            lines.append(line.replace("until_minutes=40", "until_minutes=600", 1))
            continue
        if line.startswith("DUN VE: lockout ") and "state=Open" in line:
            continue
        lines.append(line)
    ev["canal"]["run1"] = lines
    ev["canal"]["run2"] = list(lines)
    return f"the lockout lasts 590 minutes and no observation ever sees it expire (max={LOCKOUT_MAX_MINUTES})"


def falsify_golden_corrupt(ev: dict) -> str:
    rows = list(ev["doc"]["rows"])
    first = rows[0]
    rows[0] = Golden(first.seed, first.location, first.container, first.player, first.epoch, "0" * 16)
    ev["doc"]["rows"] = rows
    return "the document golden table declares a token the gate does not code"


def falsify_family_dropped(ev: dict) -> str:
    kept = [
        line for line in ev["save"]["run1"]["text"].splitlines()
        if not line.startswith(("dungeon_loot_count=", "dungeon_loot=", "checksum="))
    ]
    body = "".join(line + "\n" for line in kept)
    text = body + "checksum=%016x\n" % fnv1a64(body.encode("utf-8"))
    _rewrite_save(ev, text)
    return "the dungeon_loot family is gone from the save artifact, so no content_digest is recomputable"


def falsify_cite_invented(ev: dict) -> str:
    ev["extra_cites"] = {"crates/app/src/does-not-exist-001.rs:1"}
    return "an invented src=crates/app/src/does-not-exist-001.rs:1 joins the citation set"


@dataclass(frozen=True)
class FalsifierSpec:
    name: str
    target: str
    mutate: Callable[[dict], str]
    description: str


FALSIFIERS: dict[str, FalsifierSpec] = {
    spec.name: spec
    for spec in (
        FalsifierSpec("r10-token-mismatch", "M44-R10", falsify_token_mismatch,
                      "publish a token that is not the golden token of its tuple"),
        FalsifierSpec("r11-digest-drift", "M44-R11", falsify_digest_drift,
                      "let the second visit of one container declare a different content digest"),
        FalsifierSpec("r12-epoch-collides", "M44-R12", falsify_epoch_collides,
                      "reuse the previous epoch's token after the epoch advances"),
        FalsifierSpec("r13-lockout-permanent", "M44-R13", falsify_lockout_permanent,
                      "make a lockout outlast the bounded window and never expire"),
        FalsifierSpec("r10-family-dropped", "M44-R11", falsify_family_dropped,
                      "drop the dungeon_loot family from the save artifact"),
        FalsifierSpec("golden-vector-corrupt", "C-H4", falsify_golden_corrupt,
                      "corrupt one token of the documented golden table"),
        FalsifierSpec("cite-invented", "C-H6", falsify_cite_invented,
                      "add a src= that does not resolve"),
    )
}


# ── evaluación y render ────────────────────────────────────────────────────
def evaluate(ev: dict) -> list[tuple[Row, tuple[bool, str, str, str, str]]]:
    results: list[tuple[Row, tuple[bool, str, str, str, str]]] = []
    rendered: list[str] = []
    cites: set[str] = set()
    for row in ROWS:
        result = row.check(ev)
        results.append((row, result))
        cite = row.cite()
        if cite:
            cites.add(cite)
        rendered.append(render_line(row, result, cite))
    ev["cites"] = cites | set(ev.get("extra_cites", ()))
    ev["rendered"] = "\n".join(rendered)
    results.append((CITATION_ROW, CITATION_ROW.check(ev)))
    return results


def render_line(row: Row, result: tuple[bool, str, str, str, str], cite: str | None) -> str:
    ok, facts, token_name, expected, symptom = result
    if ok:
        return f"[OK] {row.rid} {facts} src={cite or 'none'}"
    return f"[FAIL] {row.rid} - {token_name} {facts} expected={expected} ({symptom})"


def signature(ev: dict) -> str:
    parts: list[str] = []
    for tag in ("run1", "run2"):
        for name, probe in sorted(ev["probes"][tag].items()):
            index, digest = probe_numbers(probe)
            parts.append(
                f"{tag}:{name}:exit={probe['exit']}:dungeons={index.get('dungeons', -1)}:digest={digest or '-'}"
            )
        parts.append(f"{tag}:canal={len(ev['canal'][tag])}")
        parts.append(f"{tag}:save={ev['save'][tag]['sha256'] or '-'}")
    for g in GOLDEN:
        parts.append(f"golden:{g.location},{g.container},{g.player},{g.epoch}={g.token}")
    return sha256_hex("\n".join(parts).encode("utf-8"))


# ── evidencia real ─────────────────────────────────────────────────────────
def gather(binary: Path, out: Path) -> dict:
    out.mkdir(parents=True, exist_ok=True)
    probes: dict[str, dict] = {"run1": {}, "run2": {}}
    canal: dict[str, list[str]] = {"run1": [], "run2": []}
    save: dict[str, dict] = {}
    for tag in ("run1", "run2"):
        for slug, probe in (("dungeon", CANAL_PROBES[0]), ("save", CANAL_PROBES[1])):
            capture = run_probe(binary, probe, f"{tag}-{slug}", out)
            probes[tag][slug] = capture
            canal[tag].extend(capture["canal"])
        save[tag] = save_artifact(probes[tag]["save"]["tmp"], tag)

    files = census_files()
    census_values = {name: census(pattern, files) for name, pattern in CENSUS_PATTERNS.items()}
    fingerprint, fingerprint_detail = Gate.recompute_fingerprint(list(SOURCES_PATHS))
    scope_paths = [rel(path) for path in files]
    scope, scope_detail = Gate.recompute_fingerprint(scope_paths)
    return {
        "probes": probes,
        "canal": canal,
        "save": save,
        "census": {**census_values, "files": len(files)},
        "doc": doc_golden(),
        "sources": {"fingerprint": fingerprint, "detail": fingerprint_detail, "paths": list(SOURCES_PATHS)},
        "scope": {"fingerprint": scope, "detail": scope_detail, "paths": scope_paths},
    }


def retail_state(ev: dict) -> str:
    exits = [probe["exit"] for probe in ev["probes"]["run1"].values()]
    exits += [probe["exit"] for probe in ev["probes"]["run2"].values()]
    if any(code == 2 for code in exits):
        return "absent"
    if any(code not in (0, 2) for code in exits):
        return "failed"
    return "present"


def seal(lines: list[str]) -> str:
    body = "\n".join(lines) + "\n"
    return f"DIGEST {sha256_hex(body.encode('utf-8'))}"


def sources_line(ev: dict) -> str:
    sources = ev["sources"]
    if sources["fingerprint"] is None:
        return f"SOURCES fingerprint=none files=0 paths=unreadable:{sources['detail']}"
    return (
        f"SOURCES fingerprint={sources['fingerprint']} files={len(sources['paths'])} "
        f"paths={','.join(sources['paths'])}"
    )


def scope_line(ev: dict) -> str:
    scope = ev["scope"]
    digest = scope["fingerprint"] or "none"
    return f"SCOPE files={len(scope['paths'])} fingerprint={digest} root={CENSUS_ROOT}"


def header(ev: dict, binary: Path) -> list[str]:
    lines = [
        f"GATE {GATE} cr={CR} block=C ids=M44-R10..M44-R13 pack={PACK}",
        f"GATE build=prebuilt binary={rel(binary)} sha256={(file_sha256(binary) or '-')}",
        f"GATE doc={BLOCK_DOC} golden={len(GOLDEN)} basis={TOKEN_PREFIX} max_lockout_minutes={LOCKOUT_MAX_MINUTES}",
    ]
    for tag in ("run1", "run2"):
        for slug, probe in sorted(ev["probes"][tag].items()):
            index, digest = probe_numbers(probe)
            lines.append(
                f"PROBE {tag} {probe['probe']} exit={probe['exit']} "
                f"stdout_lines={len(probe['stdout'].splitlines())} "
                f"dungeons={index.get('dungeons', -1)} "
                f"digest={digest or '-'} "
                f"canal_lines={len(probe['canal'])} mirror_log={int(probe['log_present'])}"
            )
        save = ev["save"][tag]
        lines.append(
            f"ARTIFACT {tag} save present={int(save['present'])} files={save['files']} "
            f"lines={save['lines']} sha256={save['sha256'] or '-'}"
        )
    lines.append(f"CANAL runs=2 lines={len(ev['canal']['run1'])},{len(ev['canal']['run2'])} equal={ev['canal']['run1'] == ev['canal']['run2']}")
    lines.append(f"POPULATION dungeons={probe_numbers(ev['probes']['run1']['dungeon'])[0].get('dungeons', -1)}")
    for g in GOLDEN:
        lines.append(
            f"GOLDEN seed={g.seed} location={g.location} container={g.container} "
            f"player={g.player} epoch={g.epoch} token={g.token}"
        )
    lines.append(f"REGISTRY signature={signature(ev)}")
    return lines


# ── modos ──────────────────────────────────────────────────────────────────
def mode_list() -> int:
    lines = [f"gate={GATE} cr={CR} mode=list rows={len(ROWS) + 1} falsifiers={len(FALSIFIERS)}"]
    for row in ROWS + (CITATION_ROW,):
        cite = row.cite()
        lines.append(f"row={row.rid} cite={cite or 'none'} what={row.what}")
    for name, spec in FALSIFIERS.items():
        lines.append(f"falsifier={name} target={spec.target} mutation={spec.description}")
    lines.append(seal(lines))
    sys.stdout.write("\n".join(lines) + "\n")
    return 0


def mode_self_test() -> int:
    evidence = fixture_evidence()
    results = evaluate(evidence)
    lines = [
        f"gate={GATE} cr={CR} mode=self-test fixture=contract-lines rows={len(results)}",
    ]
    red = [row.rid for row, result in results if not result[0]]
    for row, result in results:
        lines.append(render_line(row, result, row.cite()))
    lines.append(f"SELFTEST green={len(results) - len(red)}/{len(results)} red={','.join(red) or 'none'}")
    bitten = 0
    for name, spec in FALSIFIERS.items():
        mutated = fixture_evidence()
        mutation = spec.mutate(mutated)
        after = evaluate(mutated)
        target = next(result for row, result in after if row.rid == spec.target)
        red_after = [row.rid for row, result in after if not result[0]]
        if target[0]:
            red.append(f"falsify:{name}")
            verdict = "STAYED-GREEN"
        else:
            bitten += 1
            verdict = "RED"
        lines.append(
            f"FALSIFY {name} target={spec.target} verdict={verdict} "
            f"red_rows={len(red_after)} mutation={mutation}"
        )
    lines.append(f"SELFTEST falsify-bitten={bitten}/{len(FALSIFIERS)}")
    ok = not red and bitten == len(FALSIFIERS)
    lines.append(f"VERDICT: {'PASS' if ok else 'FAIL'} - self-test over the contract fixture")
    lines.append(seal(lines))
    sys.stdout.write("\n".join(lines) + "\n")
    return 0 if ok else 1


def mode_falsify(name: str) -> int:
    spec = FALSIFIERS[name]
    evidence = fixture_evidence()
    baseline = evaluate(evidence)
    target = next(result for row, result in baseline if row.rid == spec.target)
    lines = [
        f"gate={GATE} cr={CR} mode=falsify name={name} target={spec.target}",
        f"step=1 baseline row={spec.target} {'GREEN' if target[0] else 'RED'} facts={target[1]}",
    ]
    if not target[0]:
        lines.append(f"RESULT FAIL - the target row is already red before the mutation ({target[1]})")
        lines.append(f"VERDICT: FAIL - falsifier {name} applied to an already-red row")
        lines.append(seal(lines))
        sys.stdout.write("\n".join(lines) + "\n")
        return 1
    mutated = fixture_evidence()
    mutation = spec.mutate(mutated)
    after = evaluate(mutated)
    red_after = [row.rid for row, result in after if not result[0]]
    mutated_target = next(result for row, result in after if row.rid == spec.target)
    lines.append(f"step=2 mutation={mutation}")
    lines.append(f"step=3 mutated row={spec.target} {'RED' if not mutated_target[0] else 'STAYED GREEN'} facts={mutated_target[1]}")
    lines.append(f"step=4 mutated red-rows={','.join(red_after) or 'none'}")
    bitten = spec.target in red_after
    lines.append(f"RESULT {'PASS' if bitten else 'FAIL'} - the named row flips red under its mutation")
    lines.append(f"VERDICT: {'PASS' if bitten else 'FAIL'} - falsifier {name}")
    lines.append(seal(lines))
    sys.stdout.write("\n".join(lines) + "\n")
    return 0 if bitten else 1


def mode_run(args: argparse.Namespace) -> int:
    binary = Path(args.binary)
    out = Path(args.out)
    if not binary.is_file():
        print(f"GATE {GATE} binary={rel(binary)} absent")
        print("VERDICT: NC - environment: the runtime target is not built")
        return 2
    evidence = gather(binary, out)
    state = retail_state(evidence)
    head = header(evidence, binary)
    if state == "absent":
        lines = head + [f"[NC] {GATE} - retail-absent exits={[p['exit'] for p in evidence['probes']['run1'].values()]} (G12)"]
        lines.append(sources_line(evidence))
        lines.append(scope_line(evidence))
        lines.append("VERDICT: NC - environment: ARENA2 absent, the probe returned EXIT=2 (G12)")
        lines.append(seal(lines))
        sys.stdout.write("\n".join(lines) + "\n")
        return 2
    if state == "failed":
        lines = head + [f"[FAIL] {GATE} - probe-failed exits={[p['exit'] for p in evidence['probes']['run1'].values()]} expected=0 (a consumed probe died)"]
        lines.append(sources_line(evidence))
        lines.append(scope_line(evidence))
        lines.append("VERDICT: FAIL - a consumed probe of the product path died")
        lines.append(seal(lines))
        sys.stdout.write("\n".join(lines) + "\n")
        return 1

    results = evaluate(evidence)
    red = [(row.rid, result) for row, result in results if not result[0]]
    lines = list(head)
    for row, result in results:
        lines.append(render_line(row, result, row.cite()))
    lines.append(
        f"CHECKS total={len(results)} green={len(results) - len(red)} red={len(red)} "
        f"red-ids={','.join(rid for rid, _ in red) or 'none'}"
    )
    lines.append(sources_line(evidence))
    lines.append(scope_line(evidence))
    if args.json:
        payload = {
            "gate": GATE,
            "cr": CR,
            "block": "C",
            "ids": [row.rid for row, _ in results],
            "rows": [
                {"id": row.rid, "ok": result[0], "facts": result[1], "token": result[2],
                 "expected": result[3], "symptom": result[4], "cite": row.cite()}
                for row, result in results
            ],
            "golden": [
                {"seed": g.seed, "location": g.location, "container": g.container,
                 "player": g.player, "epoch": g.epoch, "token": g.token}
                for g in GOLDEN
            ],
            "sources": {"fingerprint": evidence["sources"]["fingerprint"],
                        "files": len(evidence["sources"]["paths"]),
                        "paths": list(evidence["sources"]["paths"])},
            "scope": {"fingerprint": evidence["scope"]["fingerprint"], "files": len(evidence["scope"]["paths"])},
            "signature": signature(evidence),
            "red": [rid for rid, _ in red],
            "verdict": "PASS" if not red else "FAIL",
        }
        manifest = out / "manifest.json"
        manifest.write_text(json.dumps(payload, indent=2, sort_keys=True, ensure_ascii=False) + "\n",
                            encoding="utf-8")
        sys.stdout.write(json.dumps(payload, indent=2, sort_keys=True, ensure_ascii=False) + "\n")
        return 0 if not red else 1
    lines.append(
        f"VERDICT: {'PASS' if not red else 'FAIL'} - {len(red)} red of {len(results)} rows "
        "(this gate is written for the contract: red rows are the measured absence today)"
    )
    lines.append(seal(lines))
    sys.stdout.write("\n".join(lines) + "\n")
    return 0 if not red else 1


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--binary", default=str(DEFAULT_BINARY), help="prebuilt daggerfall-app")
    parser.add_argument("--out", default=str(DEFAULT_OUT), help="artifact root (never cited)")
    parser.add_argument("--skip-build", action="store_true",
                        help="family flag: this lane never runs cargo build (the binary is prebuilt)")
    parser.add_argument("--json", action="store_true", help="emit one JSON object (no DIGEST line)")
    parser.add_argument("--list", action="store_true", help="list rows and falsifiers")
    parser.add_argument("--self-test", action="store_true", help="evaluate the contract fixture (must be green)")
    parser.add_argument("--falsify", choices=sorted(FALSIFIERS), help="mutate fixture evidence; the named row must go red")
    args = parser.parse_args(argv)
    try:
        if args.list:
            return mode_list()
        if args.self_test:
            return mode_self_test()
        if args.falsify:
            return mode_falsify(args.falsify)
        return mode_run(args)
    except Exception as exc:  # G10: nunca un traceback
        print(f"[FAIL] {GATE} - internal {type(exc).__name__} message={exc}")
        print("VERDICT: FAIL - internal")
        return 1


if __name__ == "__main__":
    sys.exit(main())
