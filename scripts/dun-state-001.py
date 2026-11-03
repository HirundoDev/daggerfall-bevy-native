#!/usr/bin/env python3
"""DUN-STATE-001 — acceptance gate for M44 W44-2 (block B: `M44-R5`..`M44-R9`).

Contract: `.sintergic/packs/CR-7fd7260f-m44-spec.md` §4 (fixed interface) and §4.2
(`DUN VE:` channel); block prose: `docs/sintergic/m44/blocks/BC-state-eligibility-CR-7fd7260f.md`
(the document's transition table and the one this gate encodes are THE SAME —
row `B-H4` verifies it by reading the document, not anyone's memory).

What it measures (state today: **no implementation**, so every `M44-R*` is measured red):

* `M44-R5` — the 6 states (`Dormant/Open/Engaged/Cleared/Cooldown/Closed`) and, for
    each declared transition (origin → event → destination), one case row and one
    counter-case row. The table lives in `TRANSITIONS`; the document repeats it and `B-H4`
    compares it row by row (12 transitions ⇒ 24 evidence rows).
* `M44-R6` — `Dormant` = 0 simulation **measured with a number**: over ≥`DORMANT_TICKS_MIN`
    declared ticks in a `Dormant` dungeon, `counters=0 runs=0`.
* `M44-R7` — dead boss persists: two `state=Dead respawns=0` observations of the
    same `(location, boss, epoch)` and that location's `machine=Cleared` in the save.
* `M44-R8` — shared container: opened by one ⇒ `Open` for the other, and the
    counter-case (never opened ⇒ `Closed` for the other).
* `M44-R9` — doors/levers/traps persist: the **real** save artifact
    (`save-all-1.dfl` produced by `--probe SAVE-ALL-001`) carries the 4
    `dungeon_*` families of §4.3 and the `DUN VE: load` line declares a digest the gate
    recomputes from the file bytes (FNV-1a 64 of the `dungeon_*` lines).

The three red forms admitted by pack §3 are used like this: **(a) artifact** — the
product's real save (85 lines, §4.3 absent: `0/4` families) and the real output of
`--probe PROBE-DUNGEON-001` (4,232 dungeons, digest 5573c56afce6749a); **(b) channel** —
the product's real processes started with `DF_DUN_STATE` set write `0`
`DUN VE:` lines (absence is counted, not asserted); **(c) census** — counts that are
literal and re-executable over `crates/**` with the contract's expected value
(`DUN VE:` = 0/≥1, `Dormant` = 0/≥1, `dungeon_*` families = 0/4).

Delta declared against `scripts/m43_gate_lib.py` (the rest is **imported**):

* **D1 — `Gate` is not used**: its harness requires the *probe* to print `DIGEST` and
    `SOURCES`, and the existing probes (`PROBE-DUNGEON-001`, `SAVE-ALL-001`) are from
    M33/M34 and do not print them; faking them here would be tautology. Reused are
    `Gate.recompute_fingerprint`, `sha256_hex`, `file_sha256`, `citation_check`,
    `probe_citations` without copying their code; the two-way citation guard is local
    (D5). This gate's `DIGEST` covers
    **its own stdout** (target rule, pack §5).
* **D2 — falsifiers over contract evidence, not over `crates/**`**: the
    lane cannot write `crates/**` or run `cargo build` (pack laws L1/L6),
    so the falsifiers mutate the evidence of the **contract fixture**
    (pattern `scripts/shell-001.py`: they mutate the captured EVIDENCE, never the
    runtime). A falsifier only counts if its row is GREEN before mutating; the
    green fixture is what proves today's red is absence of evidence and
    not a gate that always says red.
* **D3 — no `--verbose`**: `PROBE-DUNGEON-001` prints its wall clock
    (`elapsed pass1=… ms`); dumping it would break byte-for-byte identity between the two
    processes. Raw probe output is archived in `--out`, never cited.
* **D4 — the block doc is not part of `SOURCES fingerprint`**: the document pastes this
    same stdout, so hashing it would make the paste self-referential (the fingerprint
    would change on paste). Its functional integrity is verified by `B-H4` (table of the
    document == table of the gate) and that of its citations, `B-H3`.
* **D5 — local two-way citation guard**: the library's `probe_citations_guard`
    anchors its attribution marker on the form `[OK] <id> - <detail>` (its `OK_LINE`);
    with the pack grammar (`[OK] <ID> <k>=<v> … src=…`) the `id` is the whole line and
    the marker never matches, which would turn the guard red by form and not by substance.
    `guard_citations` reproduces the same discipline — a missing file and a line past
    the end, both must go red **naming the token** — over the citation set,
    with the library's resolver (`citation_check`) and the real stdout of the citation
    row (`probe_citations`).

Outputs: `[OK]`/`[FAIL]`/`[NC]` rows with the pack grammar, `PROBE`/`CANAL`/
`SOURCES`/`SCOPE`, `VERDICT: PASS|FAIL|NC` (exit 0/1/2) and `DIGEST <sha256>` as
the last line, covering everything printed before (rows + verdict).
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
GATE = "DUN-STATE-001"
CR = "CR-7fd7260f-ba3d-449a-aea9-e74386a2d5a3"
PACK = ".sintergic/packs/CR-7fd7260f-m44-spec.md"
BLOCK_DOC = "docs/sintergic/m44/blocks/BC-state-eligibility-CR-7fd7260f.md"
DEFAULT_BINARY = ROOT / "target" / "debug" / "daggerfall-app"
DEFAULT_OUT = ROOT / ".sintergic" / "artifacts" / "dun-state-001"

#  ── block B contract (§4.1/§4.2/§4.3 of the pack) ──────────────────────
STATES = ("Dormant", "Open", "Engaged", "Cleared", "Cooldown", "Closed")
DUN_FAMILIES = ("dungeon_state", "dungeon_elig", "dungeon_loot", "dungeon_instance")
DORMANT_TICKS_MIN = 600
EXPECTED_DUNGEONS = 4_232  # crates/app/src/dungeon_probe.rs:35
DUN_VE_PREFIX = "DUN VE: "
FIXTURE_INDEX = {"regions": 45, "locations": 15_251, "dungeons": EXPECTED_DUNGEONS, "without_dungeon": 11_019}
FIXTURE_DIGEST = "5573c56afce6749a"

#  Probes whose stdout feeds the `DUN VE:` channel (REAL vehicles, already existing:
#  no `--probe DUN-STATE-001` is invented, which today would be an unknown token).
CANAL_PROBES = ("PROBE-DUNGEON-001", "SAVE-ALL-001")

# Fuentes cuyo sha256 entra en `SOURCES fingerprint` (se recomputan desde disco).
SOURCES_PATHS = (
    "scripts/dun-state-001.py",
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
IMPL_MODULE = "crates/app/src/dungeon_state.rs"


@dataclass(frozen=True)
class Transition:
    """A declared edge of the dungeon automaton (§4.3, `machine`)."""

    number: int
    frm: str
    event: str
    to: str
    counter_from: str
    counter_to: str


TRANSITIONS: tuple[Transition, ...] = (
    Transition(1, "Dormant", "enter", "Open", "Cleared", "Cleared"),
    Transition(2, "Dormant", "close", "Closed", "Open", "Open"),
    Transition(3, "Open", "engage", "Engaged", "Dormant", "Dormant"),
    Transition(4, "Engaged", "disengage", "Open", "Dormant", "Dormant"),
    Transition(5, "Engaged", "objective", "Cleared", "Open", "Open"),
    Transition(6, "Cleared", "leave_zone", "Cooldown", "Dormant", "Dormant"),
    Transition(7, "Cooldown", "expire", "Dormant", "Engaged", "Engaged"),
    Transition(8, "Cooldown", "reenter", "Open", "Cleared", "Cleared"),
    Transition(9, "Open", "leave_zone", "Dormant", "Closed", "Closed"),
    Transition(10, "Closed", "new_epoch", "Dormant", "Cleared", "Cleared"),
    Transition(11, "Cleared", "seal", "Closed", "Engaged", "Engaged"),
    Transition(12, "Open", "seal", "Closed", "Cooldown", "Cooldown"),
)

#  ── grammar of the lines block B promises in `$DF_DUN_STATE` ─────
#  NAMED groups: every checker consumes `match.groupdict()`, so a positional
#  group returns `{}` and the row would die with KeyError instead of measured red.
RE_STATE = re.compile(
    r"^DUN VE: state location=(?P<location>\d+) block=(?P<block>\d+) door=(?P<door>\d+) "
    r"from=(?P<from>\w+) event=(?P<event>\w+) to=(?P<to>\w+) "
    r"result=(?P<result>applied|rejected) epoch=(?P<epoch>\d+) "
    r"since_minutes=(?P<since_minutes>\d+)$"
)
RE_SIM = re.compile(
    r"^DUN VE: sim location=(?P<location>\d+) machine=(?P<machine>\w+) "
    r"ticks=(?P<ticks>\d+) counters=(?P<counters>\d+) runs=(?P<runs>\d+) "
    r"epoch=(?P<epoch>\d+)$"
)
RE_BOSS = re.compile(
    r"^DUN VE: boss location=(?P<location>\d+) boss=(?P<boss>\d+) "
    r"state=(?P<state>\w+) epoch=(?P<epoch>\d+) respawns=(?P<respawns>\d+)$"
)
RE_CONTAINER = re.compile(
    r"^DUN VE: container location=(?P<location>\d+) container=(?P<container>\d+) "
    r"opened_by=(?P<opened_by>\d+) observed_by=(?P<observed_by>\d+) "
    r"machine=(?P<machine>\w+) epoch=(?P<epoch>\d+)$"
)
RE_LOAD = re.compile(
    r"^DUN VE: load location=(?P<location>\d+) families=(?P<families>\d+) "
    r"digest=(?P<digest>[0-9a-f]{16}) epoch=(?P<epoch>\d+)$"
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
    "dun_ve": r"DUN VE:",
    "dormant": r"\bDormant\b",
    "family_state": r"\bdungeon_state\b",
    "family_elig": r"\bdungeon_elig\b",
    "family_loot": r"\bdungeon_loot\b",
    "family_instance": r"\bdungeon_instance\b",
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
            return f"scripts/dun-state-001.py:{index}"
    return None


def impl_cite(marker: str, fallback: str | None) -> str | None:
    """`src=` of an `M44-R*` row: the implementation module if it already exists, else the checker.

        Pack §4 requires every `[OK]` row to point at `crates/app/src/<module>.rs:<line>`.
        Today that module does not exist (0 implementation), so the row cites the checker that
        measures it; as soon as the implementation lands, the SAME row cites it without touching the
        gate. Deterministic: no clock, no network.
    """
    path = ROOT / IMPL_MODULE
    if path.is_file():
        for index, line in enumerate(path.read_text(encoding="utf-8", errors="replace").splitlines(), start=1):
            if marker in line:
                return f"{IMPL_MODULE}:{index}"
    return fallback


GUARD_ABSENT = "crates/app/src/m44-guard-absent-001.rs"


def guard_citations(text: str, cites: Sequence[str]) -> tuple[bool, str]:
    """G7 in both directions, respecting the pack row grammar.

        Direction (a): the library's `probe_citations` over the real stdout of THIS
        run (every `[OK]` row resolves its `src=`). Direction (b): two sets of
        doctored citations that MUST go red *naming the token* — a missing file and
        a line past the end of a real file.

        The library ships `probe_citations_guard`, but its attribution marker assumes the
        form `[OK] <id> - <detail>` (its `OK_LINE`): with the pack grammar
        (`[OK] <ID> <k>=<v> … src=…`) the `id` is the whole line and the marker never matches,
        so that guard would go red by form and not by substance (delta D5). This one
        reproduces the same discipline over the citation set with the library resolver
        (`citation_check`) and the same real text.
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
    """FNV-1a 64 — same primitive as the save `checksum=` (savegame_v2.rs)."""
    hash_value = 0xCBF29CE484222325
    for byte in data:
        hash_value ^= byte
        hash_value = (hash_value * 0x100000001B3) & 0xFFFFFFFFFFFFFFFF
    return hash_value


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
    #  A `TMPDIR` dirty from a previous run would duplicate the probe artifacts
    #  (`files>1`) and break byte-for-byte identity between the two processes.
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
    """`(index, digest)` of the probe: pre-parsed in the fixture, measured in the product."""
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
    copy = tmp.parent / f"save-{tag}.dfl"
    copy.write_bytes(data)
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


def dungeon_digest(save_text: str) -> tuple[str, int]:
    """FNV-1a 64 of the `dungeon_*` lines in file order, each with `\n`."""
    lines = [line for line in save_text.splitlines() if line.startswith(DUN_FAMILIES)]
    body = "".join(f"{line}\n" for line in lines)
    return "%016x" % fnv1a64(body.encode("utf-8")), len(lines)


def state_entries(save_text: str) -> list[tuple[int, str]]:
    """`(location, machine)` of every `dungeon_state=` in the real artifact."""
    rows: list[tuple[int, str]] = []
    for line in save_text.splitlines():
        if not line.startswith("dungeon_state="):
            continue
        fields = line[len("dungeon_state="):].split(",")
        if len(fields) < 4:
            continue
        try:
            location = int(fields[0])
        except ValueError:
            continue
        rows.append((location, fields[3]))
    return rows


# ── tabla del documento (anti-drift documento ↔ gate) ──────────────────────
def doc_table() -> dict:
    path = ROOT / BLOCK_DOC
    if not path.is_file():
        return {"rows": None, "line": 0, "present": False, "counter_to_bad": 0}
    inside = False
    rows: list[Transition] = []
    first_line = 0
    counter_to_bad = 0
    for index, raw in enumerate(path.read_text(encoding="utf-8").splitlines(), start=1):
        if "<!-- table:transitions -->" in raw:
            inside = True
            continue
        if "<!-- /table:transitions -->" in raw:
            inside = False
            continue
        if not inside or not raw.strip().startswith("|"):
            continue
        cells = [cell.strip().strip("`") for cell in raw.strip().strip("|").split("|")]
        if len(cells) != 6 or not cells[0].isdigit():
            continue
        if cells[4] != cells[5]:
            counter_to_bad += 1
        rows.append(Transition(int(cells[0]), cells[1], cells[2], cells[3], cells[4], cells[5]))
        if first_line == 0:
            first_line = index
    return {"rows": rows, "line": first_line, "present": True, "counter_to_bad": counter_to_bad}


# ── filas ──────────────────────────────────────────────────────────────────
@dataclass(frozen=True)
class Row:
    rid: str
    what: str
    check: Callable[[dict], tuple[bool, str, str, str, str]]
    cite: Callable[[], str | None]


def check_probe(ev: dict) -> tuple[bool, str, str, str, str]:
    first, second = ev["probes"]["run1"], ev["probes"]["run2"]
    dungeon1, dungeon2 = first["dungeon"], second["dungeon"]
    index1, digest1 = probe_numbers(dungeon1)
    index2, digest2 = probe_numbers(dungeon2)
    facts = (
        f"probe={CANAL_PROBES[0]} exit={dungeon1['exit']},{dungeon2['exit']} "
        f"dungeons={index1.get('dungeons', -1)},{index2.get('dungeons', -1)} "
        f"locations={index1.get('locations', -1)} "
        f"digest={digest1} digest_run2={digest2} "
        f"source_lines={len(dungeon1['stdout'].splitlines())}"
    )
    counts_ok = (
        index1.get("dungeons") == EXPECTED_DUNGEONS
        and index2.get("dungeons") == EXPECTED_DUNGEONS
        and index1 == index2
    )
    ok = dungeon1["exit"] == 0 and dungeon2["exit"] == 0 and digest1 is not None and digest1 == digest2 and counts_ok
    return ok, facts, "probe-census", str(EXPECTED_DUNGEONS), "the geometry census of the product probe is not reproducible"


def check_save_artifact(ev: dict) -> tuple[bool, str, str, str, str]:
    save1, save2 = ev["save"]["run1"], ev["save"]["run2"]
    exit1 = ev["probes"]["run1"]["save"]["exit"]
    exit2 = ev["probes"]["run2"]["save"]["exit"]
    present, entries = family_census(save1["text"]) if save1["present"] else ([], {})
    facts = (
        f"probe={CANAL_PROBES[1]} exit={exit1},{exit2} files={save1['files']},{save2['files']} "
        f"lines={save1['lines']} families_dungeon={len(present)}/4 "
        f"sha256={save1['sha256'] or 'none'} sha256_run2={save2['sha256'] or 'none'} "
        f"state_entries={entries.get('dungeon_state', 0)}"
    )
    ok = (
        exit1 == 0
        and exit2 == 0
        and save1["present"]
        and save1["sha256"] == save2["sha256"]
        and save1["files"] == 1
    )
    return ok, facts, "save-artifact", "1 byte-identical", "the real save artifact of two independent product processes differs"


def check_doc(ev: dict) -> tuple[bool, str, str, str, str]:
    doc = ev["doc"]
    coded = [(t.number, t.frm, t.event, t.to, t.counter_from, t.counter_to) for t in TRANSITIONS]
    if doc["rows"] is None:
        return (
            False,
            f"transitions={len(coded)} table_rows=0 table_line=0",
            "doc-table-absent",
            str(len(coded)),
            f"{BLOCK_DOC} does not declare the transition table the gate codes",
        )
    declared = [(t.number, t.frm, t.event, t.to, t.counter_from, t.counter_to) for t in doc["rows"]]
    missing = [row for row in coded if row not in declared]
    extra = [row for row in declared if row not in coded]
    states_declared = sorted({t.frm for t in doc["rows"]})
    facts = (
        f"transitions={len(coded)} table_rows={len(declared)} table_line={doc['line']} "
        f"states={len(states_declared)}/6 counter_to_bad={doc['counter_to_bad']} "
        f"missing={len(missing)} extra={len(extra)}"
    )
    ok = (
        declared == coded
        and doc["counter_to_bad"] == 0
        and states_declared == sorted(STATES)
    )
    return ok, facts, "doc-table-drift", f"{len(coded)} identical rows", "gate and document disagree on the transition table"


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


def check_r5(ev: dict) -> tuple[bool, str, str, str, str]:
    rows = [match.groupdict() for match in (RE_STATE.match(line) for line in ev["canal"]["run1"]) if match]
    applied = {(row["from"], row["event"], row["to"]) for row in rows if row["result"] == "applied"}
    rejected = {(row["from"], row["event"], row["to"]) for row in rows if row["result"] == "rejected"}
    want_applied = {(t.frm, t.event, t.to) for t in TRANSITIONS}
    want_rejected = {(t.counter_from, t.event, t.counter_to) for t in TRANSITIONS}
    cases = want_applied & applied
    counters = want_rejected & rejected
    origin_states = {row["from"] for row in rows}
    first_missing = next((t for t in TRANSITIONS if (t.frm, t.event, t.to) not in applied), None)
    first_missing_counter = next((t for t in TRANSITIONS if (t.counter_from, t.event, t.counter_to) not in rejected), None)
    facts = (
        f"transitions={len(cases)}/{len(TRANSITIONS)} counters={len(counters)}/{len(TRANSITIONS)} "
        f"states={len(origin_states & set(STATES))}/6 rows={len(rows)} "
        f"first-missing=#{(first_missing or TRANSITIONS[0]).number} "
        f"first-missing-counter=#{(first_missing_counter or TRANSITIONS[0]).number}"
    )
    ok = len(cases) == len(TRANSITIONS) and len(counters) == len(TRANSITIONS) and len(origin_states & set(STATES)) == 6
    return ok, facts, "transition-coverage", "12/12 casos + 12/12 contra-casos", "a declared transition has no case or no counter-case in the runtime canal"


def check_r5_census(ev: dict) -> tuple[bool, str, str, str, str]:
    hits, where = ev["census"]["dun_ve"]
    facts = f"census=DUN_VE: hits={hits} scope={CENSUS_ROOT}/**/*.rs files={ev['census']['files']} first={','.join(where) or 'none'}"
    ok = hits >= 1
    return ok, facts, "canal-census", ">=1", "not one source line of the tree publishes the DUN VE: canal"


def check_r6(ev: dict) -> tuple[bool, str, str, str, str]:
    rows = [match.groupdict() for match in (RE_SIM.match(line) for line in ev["canal"]["run1"]) if match]
    dormant = [row for row in rows if row["machine"] == "Dormant"]
    ticks_max = max((int(row["ticks"]) for row in dormant), default=0)
    counters_total = sum(int(row["counters"]) for row in dormant)
    runs_total = sum(int(row["runs"]) for row in dormant)
    locations = {row["location"] for row in dormant}
    population = probe_numbers(ev["probes"]["run1"]["dungeon"])[0].get("dungeons", -1)
    facts = (
        f"sim_rows={len(rows)} dormant_rows={len(dormant)} ticks_max={ticks_max} "
        f"counters_total={counters_total} runs_total={runs_total} "
        f"dormant_locations={len(locations)} population={population}"
    )
    ok = (
        bool(dormant)
        and ticks_max >= DORMANT_TICKS_MIN
        and counters_total == 0
        and runs_total == 0
        and len(locations) >= 1
    )
    return ok, facts, "dormant-simulation", f"0 counters over >={DORMANT_TICKS_MIN} ticks", "a Dormant dungeon is not proven to simulate nothing"


def check_r6_census(ev: dict) -> tuple[bool, str, str, str, str]:
    hits, where = ev["census"]["dormant"]
    facts = f"census=Dormant hits={hits} scope={CENSUS_ROOT}/**/*.rs files={ev['census']['files']} first={','.join(where) or 'none'} states_declared={len(STATES)}"
    ok = hits >= 1
    return ok, facts, "machine-census", ">=1", "the six state names of the machine exist in no source line"


def check_r7(ev: dict) -> tuple[bool, str, str, str, str]:
    rows = [match.groupdict() for match in (RE_BOSS.match(line) for line in ev["canal"]["run1"]) if match]
    grouped: dict[tuple[str, str, str], list[dict]] = {}
    for row in rows:
        grouped.setdefault((row["location"], row["boss"], row["epoch"]), []).append(row)
    persisted = {
        key: value
        for key, value in grouped.items()
        if len(value) >= 2 and all(item["state"] == "Dead" and item["respawns"] == "0" for item in value)
    }
    cleared = {(location, machine) for location, machine in state_entries(ev["save"]["run1"]["text"])}
    cleared_locations = {location for location, machine in cleared if machine == "Cleared"}
    matched = {key for key in persisted if int(key[0]) in cleared_locations}
    facts = (
        f"boss_rows={len(rows)} dead_rows={sum(1 for row in rows if row['state'] == 'Dead')} "
        f"persisted_pairs={len(persisted)}/1 cleared_in_save={len(matched)}/1 "
        f"save_cleared_entries={len(cleared_locations)}"
    )
    ok = len(persisted) >= 1 and len(matched) >= 1
    return ok, facts, "boss-respawn", "1 persisted pair + 1 Cleared in save", "a dead boss is not proven to stay dead inside the same epoch"


def check_r8(ev: dict) -> tuple[bool, str, str, str, str]:
    rows = [match.groupdict() for match in (RE_CONTAINER.match(line) for line in ev["canal"]["run1"]) if match]
    shared = [
        row for row in rows
        if row["opened_by"] != row["observed_by"] and row["machine"] == "Open" and row["opened_by"] != "0"
    ]
    closed = [row for row in rows if row["opened_by"] == "0" and row["machine"] == "Closed"]
    open_entries = [
        location for location, machine in state_entries(ev["save"]["run1"]["text"]) if machine == "Open"
    ]
    facts = (
        f"container_rows={len(rows)} shared_open={len(shared)}/1 never_opened_closed={len(closed)}/1 "
        f"save_open_entries={len(open_entries)}"
    )
    ok = len(shared) >= 1 and len(closed) >= 1
    return ok, facts, "shared-container", "1 shared open + 1 never-opened closed", "a container opened by one player is not proved Open for the other"


def check_r9(ev: dict) -> tuple[bool, str, str, str, str]:
    save = ev["save"]["run1"]
    present, entries = family_census(save["text"])
    declared_digest, entry_lines = dungeon_digest(save["text"])
    loads = [match.groupdict() for match in (RE_LOAD.match(line) for line in ev["canal"]["run1"]) if match]
    matched = [row for row in loads if row["digest"] == declared_digest and int(row["families"]) == len(DUN_FAMILIES)]
    facts = (
        f"save_lines={save['lines']} families={len(present)}/4 "
        f"entry_lines={entry_lines} digest_recomputed={declared_digest} "
        f"load_rows={len(loads)} load_match={len(matched)} "
        f"state_entries={entries.get('dungeon_state', 0)}"
    )
    ok = len(present) == len(DUN_FAMILIES) and entry_lines >= 1 and len(matched) >= 1
    return ok, facts, "save-roundtrip", "4/4 familias + 1 load con digest recomputado", "doors, levers and traps do not survive the save"


def check_r9_census(ev: dict) -> tuple[bool, str, str, str, str]:
    hits = {family: ev["census"][f"family_{family.split('_')[1]}"][0] for family in DUN_FAMILIES}
    covered = sum(1 for value in hits.values() if value >= 1)
    facts = (
        f"census families={covered}/4 "
        + " ".join(f"{family}={value}" for family, value in hits.items())
        + f" scope={CENSUS_ROOT}/**/*.rs files={ev['census']['files']}"
    )
    ok = covered == len(DUN_FAMILIES)
    return ok, facts, "family-census", "4/4", "a persisted dungeon family of §4.3 exists in no source line"


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
        "B-H1",
        "el probe de geometría (vehículo real del canal) es reproducible: 2 procesos, mismo censo y mismo digest",
        check_probe,
        lambda: "crates/app/src/main.rs:678",
    ),
    Row(
        "B-H2",
        "el artefacto real de save es byte-idéntico en 2 procesos independientes del producto",
        check_save_artifact,
        lambda: "crates/app/src/m33_probe.rs:810",
    ),
    Row(
        "B-H4",
        "la tabla de transiciones del documento es LA MISMA que codifica el gate",
        check_doc,
        lambda: f"{BLOCK_DOC}:{doc_table()['line']}" if doc_table()["line"] else None,
    ),
    Row(
        "B-H5",
        "el canal DUN VE: se mide en 2 corridas reales y ambas publican exactamente lo mismo",
        check_canal,
        lambda: "crates/app/src/shell.rs:210",
    ),
    Row(
        "M44-R5",
        "cada transición declarada tiene caso y contra-caso en el canal de runtime",
        check_r5,
        lambda: impl_cite("DUN VE: state", own_cite("def check_r5(")),
    ),
    Row(
        "M44-R5.census",
        "el canal DUN VE: existe en el árbol (censo re-ejecutable)",
        check_r5_census,
        lambda: impl_cite("DUN VE:", own_cite("def check_r5_census(")),
    ),
    Row(
        "M44-R6",
        "Dormant = 0 simulación, medido con ticks y contadores",
        check_r6,
        lambda: impl_cite("DUN VE: sim", own_cite("def check_r6(")),
    ),
    Row(
        "M44-R6.census",
        "los nombres de estado de la máquina existen en el árbol (censo re-ejecutable)",
        check_r6_census,
        lambda: impl_cite("Dormant", own_cite("def check_r6_census(")),
    ),
    Row(
        "M44-R7",
        "jefe muerto persiste en la misma epoch (dos observaciones + Cleared en el save)",
        check_r7,
        lambda: impl_cite("DUN VE: boss", own_cite("def check_r7(")),
    ),
    Row(
        "M44-R8",
        "contenedor compartido: abierto por uno ⇒ Open para el otro, con contra-caso",
        check_r8,
        lambda: impl_cite("DUN VE: container", own_cite("def check_r8(")),
    ),
    Row(
        "M44-R9",
        "estado de puertas/palancas/trampas persiste a través de save/load",
        check_r9,
        lambda: impl_cite("dungeon_state=", "crates/memory/src/savegame_v2.rs:18"),
    ),
    Row(
        "M44-R9.census",
        "las 4 familias persistidas de §4.3 existen en el árbol (censo re-ejecutable)",
        check_r9_census,
        lambda: impl_cite("dungeon_state", own_cite("def check_r9_census(")),
    ),
)

CITATION_ROW = Row(
    "B-H3",
    "toda cita src= de este gate resuelve contra el árbol y la guarda muerde en las dos direcciones",
    check_citations,
    lambda: own_cite("def check_citations("),
)


# ── fixture de contrato (verde) y falsificadores ───────────────────────────
def fixture_save() -> str:
    lines = [
        "DFL-SESSION 3",
        "pos_x_mm=1500",
        "pos_z_mm=-2000",
        "gold=55",
        "dungeon_state_count=3",
        "dungeon_state=26219,0,6,Cleared,120,0",
        "dungeon_state=26219,0,7,Open,5,0",
        "dungeon_state=26278,1,2,Cooldown,30,0",
        "dungeon_elig_count=1",
        "dungeon_elig=6fe1574c48403ecd,26219,0,1,0,1",
        "dungeon_loot_count=1",
        "dungeon_loot=26219,0,Boss,0,1,0",
        "dungeon_instance_count=1",
        "dungeon_instance=0a1b2c3d4e5f6071,26219,0,1",
    ]
    body = "\n".join(lines) + "\n"
    return body + "checksum=%016x\n" % fnv1a64(body.encode("utf-8"))


def fixture_canal() -> list[str]:
    lines: list[str] = []
    for transition in TRANSITIONS:
        lines.append(
            "DUN VE: state location=26219 block=0 door=%d from=%s event=%s to=%s "
            "result=applied epoch=0 since_minutes=%d"
            % (transition.number, transition.frm, transition.event, transition.to, transition.number)
        )
        lines.append(
            "DUN VE: state location=26219 block=0 door=%d from=%s event=%s to=%s "
            "result=rejected epoch=0 since_minutes=%d"
            % (100 + transition.number, transition.counter_from, transition.event,
               transition.counter_to, transition.number)
        )
    lines.append("DUN VE: sim location=26219 machine=Dormant ticks=720 counters=0 runs=0 epoch=0")
    lines.append("DUN VE: sim location=26278 machine=Dormant ticks=600 counters=0 runs=0 epoch=0")
    lines.append("DUN VE: boss location=26219 boss=3 state=Dead epoch=0 respawns=0")
    lines.append("DUN VE: boss location=26219 boss=3 state=Dead epoch=0 respawns=0")
    lines.append(
        "DUN VE: container location=26219 container=0 opened_by=1 observed_by=2 machine=Open epoch=0"
    )
    lines.append(
        "DUN VE: container location=26219 container=1 opened_by=0 observed_by=2 machine=Closed epoch=0"
    )
    digest, _ = dungeon_digest(fixture_save())
    lines.append(f"DUN VE: load location=26219 families=4 digest={digest} epoch=0")
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
            "dun_ve": (len(canal), ["fixture:1"]),
            "dormant": (len(STATES), ["fixture:2"]),
            "family_state": (1, ["fixture:3"]),
            "family_elig": (1, ["fixture:3"]),
            "family_loot": (1, ["fixture:3"]),
            "family_instance": (1, ["fixture:3"]),
        },
        "doc": {"rows": list(TRANSITIONS), "line": 1, "present": True, "counter_to_bad": 0},
        "index": dict(FIXTURE_INDEX),
    }


def falsify_drop_counter(ev: dict) -> str:
    target = next(t for t in TRANSITIONS if t.number == 7)
    prefix = f"from={target.counter_from} event={target.event} to={target.counter_to} result=rejected"
    kept = []
    dropped = 0
    for line in ev["canal"]["run1"]:
        if prefix in line and dropped == 0:
            dropped += 1
            continue
        kept.append(line)
    ev["canal"]["run1"] = kept
    ev["canal"]["run2"] = list(kept)
    return (
        f"the rejected row of transition #{target.number} "
        f"({target.counter_from} --{target.event}--> {target.counter_to}) is gone"
    )


def falsify_dormant_simulates(ev: dict) -> str:
    replaced = 0
    lines = []
    for line in ev["canal"]["run1"]:
        if line.startswith("DUN VE: sim ") and "machine=Dormant" in line and replaced == 0:
            lines.append(line.replace("counters=0", "counters=3", 1))
            replaced += 1
            continue
        lines.append(line)
    ev["canal"]["run1"] = lines
    ev["canal"]["run2"] = list(lines)
    return "a Dormant dungeon reports counters=3 over its ticks (it simulated something)"


def falsify_boss_respawns(ev: dict) -> str:
    lines = [line.replace("respawns=0", "respawns=1", 1) if line.startswith("DUN VE: boss ") else line
             for line in ev["canal"]["run1"]]
    ev["canal"]["run1"] = lines
    ev["canal"]["run2"] = list(lines)
    return "the dead boss reports respawns=1 inside the same epoch"


def falsify_container_unseen(ev: dict) -> str:
    lines = [
        line for line in ev["canal"]["run1"]
        if not (line.startswith("DUN VE: container ") and "opened_by=1" in line)
    ]
    ev["canal"]["run1"] = lines
    ev["canal"]["run2"] = list(lines)
    return "the row where the second player observes the first player's container is gone"


def falsify_family_dropped(ev: dict) -> str:
    kept = [line for line in ev["save"]["run1"]["text"].splitlines()
            if not line.startswith(("dungeon_instance_count=", "dungeon_instance=", "checksum="))]
    body = "".join(line + "\n" for line in kept)
    text = body + "checksum=%016x\n" % fnv1a64(body.encode("utf-8"))
    ev["save"]["run1"]["text"] = text
    ev["save"]["run1"]["lines"] = len(text.splitlines())
    return "the dungeon_instance family is gone from the save artifact"


def falsify_digest_tampered(ev: dict) -> str:
    lines = []
    tampered = 0
    for line in ev["save"]["run1"]["text"].splitlines():
        if line.startswith("dungeon_state=26219,0,7,Open") and tampered == 0:
            lines.append(line.replace(",Open,", ",Engaged,", 1))
            tampered += 1
            continue
        lines.append(line)
    body = "".join(line + "\n" for line in lines if not line.startswith("checksum="))
    text = body + "checksum=%016x\n" % fnv1a64(body.encode("utf-8"))
    ev["save"]["run1"]["text"] = text
    ev["save"]["run1"]["lines"] = len(text.splitlines())
    return "a persisted dungeon_state byte changed after the load line fixed its digest"


def falsify_doc_drift(ev: dict) -> str:
    rows = list(ev["doc"]["rows"])
    target = next(t for t in TRANSITIONS if t.number == 11)
    ev["doc"]["rows"] = [row for row in rows if row.number != target.number]
    return f"row #{target.number} disappeared from the document table (the gate still codes it)"


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
        FalsifierSpec("r5-drop-counter", "M44-R5", falsify_drop_counter,
                      "drop one declared counter-case row from the canal"),
        FalsifierSpec("r6-dormant-simulates", "M44-R6", falsify_dormant_simulates,
                      "let a Dormant dungeon report one simulated counter"),
        FalsifierSpec("r7-boss-respawns", "M44-R7", falsify_boss_respawns,
                      "declare respawns=1 for a boss inside its epoch"),
        FalsifierSpec("r8-container-unseen", "M44-R8", falsify_container_unseen,
                      "remove the second player's observation of an opened container"),
        FalsifierSpec("r9-family-dropped", "M44-R9", falsify_family_dropped,
                      "drop the dungeon_instance family from the save artifact"),
        FalsifierSpec("r9-digest-tampered", "M44-R9", falsify_digest_tampered,
                      "flip a persisted state byte behind the load digest"),
        FalsifierSpec("doc-table-drift", "B-H4", falsify_doc_drift,
                      "delete a row of the documented transition table"),
        FalsifierSpec("cite-invented", "B-H3", falsify_cite_invented,
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
    result = CITATION_ROW.check(ev)
    results.append((CITATION_ROW, result))
    return results


def render_line(row: Row, result: tuple[bool, str, str, str, str], cite: str | None) -> str:
    ok, facts, token, expected, symptom = result
    if ok:
        return f"[OK] {row.rid} {facts} src={cite or 'none'}"
    return f"[FAIL] {row.rid} - {token} {facts} expected={expected} ({symptom})"


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
        "doc": doc_table(),
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
        f"GATE {GATE} cr={CR} block=B ids=M44-R5..M44-R9 pack={PACK}",
        f"GATE build=prebuilt binary={rel(binary)} sha256={(file_sha256(binary) or '-')}",
        f"GATE doc={BLOCK_DOC} transitions={len(TRANSITIONS)} states={len(STATES)} families={len(DUN_FAMILIES)}",
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
            "block": "B",
            "ids": [row.rid for row, _ in results],
            "rows": [
                {"id": row.rid, "ok": result[0], "facts": result[1], "token": result[2],
                 "expected": result[3], "symptom": result[4], "cite": row.cite()}
                for row, result in results
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
