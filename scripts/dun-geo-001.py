#!/usr/bin/env python3
"""DUN-GEO-001 - M44 / block A gate (W44-1): the M34 geometry is what the content path consumes.

Contract: `.sintergic/packs/CR-7fd7260f-m44-spec.md` §4.1 (IDs `M44-R1..R4`);
golden vectors and postconditions: `docs/sintergic/m44/blocks/A-geometry-CR-7fd7260f.md`.

What it measures, and why like this
-----------------------
* `M44-R1` **delegates** to `PROBE-DUNGEON-001` (`crates/app/src/main.rs:685`): two
    separate processes of the binary, and every geometry golden already measured by M34
    (`crates/app/src/dungeon_probe.rs:35-52`) is compared against what the probe
    prints. The gate does NOT reimplement the census: it parses it. If the probe prints the
    number, the gate's number is that number.
* `M44-R2` measures with a re-executable census (R-negative) whether the content path
    (`crates/app/src/m18_window.rs`) calls the geometry assembler. The expected
    value is fixed by the contract (`assemble_calls>=1` and `geometry_links>=1`), not by
    today's measurement.
* `M44-R3` requires the probe `DIGEST=` to be equal in both processes **and** the
    geometry block to be byte-identical. The only excluded line is the wall clock
    `elapsed pass1=N ms pass2=M ms`: it is declared, bounded to ONE line per capture and
    its shape is verified (`PROC-volatile`). Without that exclusion byte-for-byte
    identity is unreachable for a reason foreign to the contract, and an instrument that
    cannot go green measures nothing.
* `M44-R4` requires 0 overlaps at 1.400 mm over the set the content path
    consumes. The figure is published by the product on the runtime channel
    (`DUN VE: geo overlaps ...`); if the channel does not carry that row the row is red
    `missing_evidence` (or `blocked_by:M44-R2` when the census measures 0 calls to the
    assembler): the gate does not publish an overlap count nobody has measured.

Runtime channel: the owner is the gate, not the operator
--------------------------------------------------
An instrument cannot depend on the operator's recipe: a PASS that only
appears when someone exports `$DF_DUN_STATE` by hand does not measure the product, it measures the
memory of whoever launches it. That is why the gate **resolves its own channel**:

* default `<out>/dun_canal.txt` (inside its own artifacts) => `source=gate`
* `--channel <path>` (absolute, or relative to the repo root) => `source=flag`
* `--channel off`: the probe runs **without** `$DF_DUN_STATE` => `source=off`, and
    `M44-R4` stays red for lack of evidence (permanent negative control)

Before each of the two probe runs the channel is truncated to 0 bytes and
`$DF_DUN_STATE=<its path>` is exported to the child process. Leftovers from a previous
run cannot contaminate the measurement. Any environment variable the
operator brings is ignored: the gate imposes its own on the product. The resolved channel is
published in the run header as `NOTE channel=<path> source=gate|flag|off`.

Preconditions: retail ARENA2 (`$HOME/Games/Daggerfall/app/ARENA2/{MAPS,BLOCKS}.BSA`)
and a built binary. Without retail => `VERDICT: NC`, exit 2 (never a false PASS).

Exit codes: 0 PASS, 1 FAIL (contract red), 2 environment/usage (NC).
"""

from __future__ import annotations

import argparse
import json
import os
import re
import subprocess
import sys
from dataclasses import dataclass
from pathlib import Path
from typing import Callable

ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT / "scripts"))

from m43_gate_lib import (  # noqa: E402  (la lib vive al lado; se importa, no se copia)
    Gate as _Gate,
    build as lib_build,
    citation_check,
    file_sha256,
    head_commit,
    resolve_cite,
    sha256_hex,
)

GATE = "DUN-GEO-001"
CR = "CR-7fd7260f-ba3d-449a-aea9-e74386a2d5a3"
PROBE = "PROBE-DUNGEON-001"
CONTENT_PATH = "crates/app/src/m18_window.rs"
DEFAULT_BINARY = "target/debug/daggerfall-app"
DEFAULT_OUT = ".sintergic/artifacts/dun-geo-001"
PROBE_TIMEOUT = 1800
DUN_CHANNEL_ENV = "DF_DUN_STATE"
DEFAULT_CHANNEL = "dun_canal.txt"
CHANNEL_OFF = "off"

#  Sources this gate measures: the probe, its dispatch, the content path
#  censused, the interior-path geometry entry and the assembler.
MEASURED_SOURCES = (
    "crates/app/src/dungeon_probe.rs",
    "crates/app/src/main.rs",
    "crates/app/src/m18_window.rs",
    "crates/app/src/interior_retail.rs",
    "crates/parser-bevy-bridge/src/dungeon_grid.rs",
)

ANCHOR_GATE = "scripts/dun-geo-001.py:1"
EXTRA_CITES = (
    "crates/app/src/main.rs:685",
    "crates/app/src/interior_retail.rs:162",
    "crates/parser-bevy-bridge/src/dungeon_grid.rs:304",
)

ANCHORS = {
    "M44-R1": "crates/app/src/dungeon_probe.rs:35",
    "M44-R2": "crates/app/src/m18_window.rs:4072",
    "M44-R3": "crates/app/src/dungeon_probe.rs:313",
    "M44-R4": "crates/app/src/m18_window.rs:518",
    "PROC-sources": ANCHOR_GATE,
    "PROC-two-runs": ANCHOR_GATE,
    "PROC-volatile": ANCHOR_GATE,
    "PROC-citations": ANCHOR_GATE,
    "PROC-fresh": ANCHOR_GATE,
}

# Gramatica del probe (M34) - se parsea, no se reimplementa
RE_CONTRACT = re.compile(r"^contract=(\S+) .*cells=([0-9.]+) m .*door tolerance=([0-9.]+) m$")
RE_INDEX = re.compile(
    r"^index regions=(\d+) locations=(\d+) dungeons=(\d+) without_dungeon=(\d+)"
    r" maps=(\d+) B blocks=(\d+) B$"
)
RE_GRID = re.compile(
    r"^grid blocks=(\d+) doors=(\d+) removed=(\d+) with_dup=(\d+) start_markers=(\d+)"
    r" enter_markers=(\d+) missing_blocks=(\d+) unparsed_blocks=(\d+)$"
)
RE_SUMMARY = re.compile(r"^SUMMARY PROBE-DUNGEON-001: passed=(\d+)/(\d+) .* EXIT=(\d+)$")
RE_INPROC = re.compile(r"^digest1=([0-9a-f]{16}) digest2=([0-9a-f]{16}) identical=(true|false)$")
RE_DIGEST = re.compile(r"^DIGEST=([0-9a-f]{16}) ")
RE_PASS_ROW = re.compile(r"^PASS (\S+)$")
RE_FAIL_ROW = re.compile(r"^FAIL (\S+)$")

#  The only declared volatile line: the probe's wall clock.
VOLATILE_RE = re.compile(r"^elapsed pass1=(?:\d+|<ms>) ms pass2=(?:\d+|<ms>) ms$")
VOLATILE_MASK = "elapsed pass1=<ms> ms pass2=<ms> ms"

#  Content-path censuses (R-negative, re-executable over the file).
RE_ASSEMBLE_CALL = re.compile(r"assemble\(")
RE_GEOMETRY_LINK = re.compile(r"dungeon_grid|assemble_dungeon|DungeonGrid|interior_retail|RetailDungeon")
RE_DUNGEON_MENTION = re.compile(r"dungeon", re.IGNORECASE)

# Canal runtime del contrato (§4.2): lineas `DUN VE: ...` en `$DF_DUN_STATE`.
RE_DUN_GEO = re.compile(
    r"^DUN VE: geo dungeons=(\d+) blocks=(\d+) cells_mm=51200 tolerance_mm=1400 digest=([0-9a-f]{16})$"
)
RE_DUN_OVERLAP = re.compile(r"^DUN VE: geo overlaps dungeons=(\d+) overlaps=(\d+) tolerance_mm=(\d+)$")

# Golden vectors heredados de M34 (`crates/app/src/dungeon_probe.rs:35-52`).
GOLDEN = (
    ("regions", 45),
    ("locations", 15251),
    ("dungeons", 4232),
    ("without_dungeon", 11019),
    ("blocks", 40263),
    ("doors", 376547),
    ("removed", 4322),
    ("with_dup", 1303),
    ("start_markers", 4232),
    ("enter_markers", 775),
    ("missing_blocks", 0),
    ("unparsed_blocks", 0),
    ("cells_mm", 51200),
    ("tolerance_mm", 1400),
)


def rel(path: Path) -> str:
    try:
        return str(path.resolve().relative_to(ROOT))
    except ValueError:
        return str(path)


def milli(declared: str) -> int:
    """`51.2` -> 51200 (mm enteros, sin coma flotante en la cuenta)."""
    whole, _, frac = declared.partition(".")
    return int(whole) * 1000 + int((frac + "000")[:3])


def normalize(text: str) -> tuple[str, int]:
    """ Masks the only declared volatile line; returns (text, lines with that shape)."""
    out: list[str] = []
    shaped = 0
    for line in text.splitlines():
        if VOLATILE_RE.match(line):
            shaped += 1
            out.append(VOLATILE_MASK)
        else:
            out.append(line)
    return "\n".join(out) + "\n", shaped


def parse_probe(text: str) -> dict:
    """ Values the probe prints. Missing key = the row does not exist."""
    out: dict = {}
    rows: list[str] = []
    for line in text.splitlines():
        if not line or line.startswith("--"):
            continue
        if (m := RE_CONTRACT.match(line)):
            out["cells_mm"] = milli(m.group(2))
            out["tolerance_mm"] = milli(m.group(3))
        elif (m := RE_INDEX.match(line)):
            out["regions"] = int(m.group(1))
            out["locations"] = int(m.group(2))
            out["dungeons"] = int(m.group(3))
            out["without_dungeon"] = int(m.group(4))
            out["maps_bytes"] = int(m.group(5))
            out["blocks_bytes"] = int(m.group(6))
        elif (m := RE_GRID.match(line)):
            out["blocks"] = int(m.group(1))
            out["doors"] = int(m.group(2))
            out["removed"] = int(m.group(3))
            out["with_dup"] = int(m.group(4))
            out["start_markers"] = int(m.group(5))
            out["enter_markers"] = int(m.group(6))
            out["missing_blocks"] = int(m.group(7))
            out["unparsed_blocks"] = int(m.group(8))
        elif (m := RE_SUMMARY.match(line)):
            out["passed"] = int(m.group(1))
            out["checks"] = int(m.group(2))
            out["probe_exit"] = int(m.group(3))
        elif (m := RE_INPROC.match(line)):
            out["digest1"] = m.group(1)
            out["digest2"] = m.group(2)
            out["in_process"] = 1 if m.group(3) == "true" else 0
        elif (m := RE_DIGEST.match(line)):
            out["digest"] = m.group(1)
        elif (m := RE_PASS_ROW.match(line)):
            rows.append(m.group(1))
        elif (m := RE_FAIL_ROW.match(line)):
            rows.append("FAIL:" + m.group(1))
    out["pass_rows"] = rows
    return out


def census_of(text: str) -> dict:
    calls: list[int] = []
    links = 0
    mentions: list[int] = []
    lines = text.splitlines()
    for number, line in enumerate(lines, 1):
        for _ in RE_ASSEMBLE_CALL.finditer(line):
            calls.append(number)
        links += len(RE_GEOMETRY_LINK.findall(line))
        if RE_DUNGEON_MENTION.search(line):
            mentions.append(number)
    return {
        "lines": len(lines),
        "calls": len(calls),
        "call_lines": calls,
        "links": links,
        "mention_lines": mentions,
    }


def read_text(path: Path) -> str | None:
    if not path.is_file():
        return None
    return path.read_text(encoding="utf-8", errors="replace")


def resolve_channel(spec: str | None, out: Path) -> tuple[Path | None, str]:
    """`(path, source)`; `path is None` = canal apagado. source in {gate, flag, off}."""
    if spec is not None and spec.strip().lower() == CHANNEL_OFF:
        return None, "off"
    if spec:
        path = Path(spec).expanduser()
        if not path.is_absolute():
            path = ROOT / path
        return path, "flag"
    return out / DEFAULT_CHANNEL, "gate"


def read_dun_state(path: Path | None, source: str) -> tuple[list[str], str]:
    """ `DUN VE: ` lines of the channel the GATE resolved (the operator's variable does not rule)."""
    if path is None or not path.is_file():
        return [], source
    try:
        text = path.read_text(encoding="utf-8", errors="replace")
    except OSError:
        return [], source
    lines = [line for line in text.splitlines() if line.startswith("DUN VE: ")]
    return lines, source


def source_snapshot() -> dict:
    fingerprint, detail = _Gate.recompute_fingerprint(list(MEASURED_SOURCES))
    present = [p for p in MEASURED_SOURCES if (ROOT / p).is_file()]
    missing = [p for p in MEASURED_SOURCES if not (ROOT / p).is_file()]
    return {
        "fingerprint": fingerprint,
        "detail": detail,
        "present": present,
        "missing": missing,
        "paths": ",".join(present),
    }


def retail_root() -> Path:
    base = os.environ.get("DF_APP_ROOT", "")
    if base:
        return Path(base).expanduser()
    return Path.home() / "Games" / "Daggerfall" / "app"


def retail_present() -> tuple[bool, str]:
    root = retail_root() / "ARENA2"
    maps = root / "MAPS.BSA"
    blocks = root / "BLOCKS.BSA"
    if maps.is_file() and blocks.is_file():
        return True, "maps=%d blocks=%d" % (maps.stat().st_size, blocks.stat().st_size)
    return False, "maps=%d blocks=%d" % (int(maps.is_file()), int(blocks.is_file()))


def truncate(path: Path) -> None:
    """ Empties the channel before each run: yesterday's leftovers do not measure today."""
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(b"")


def run_probe(binary: Path, channel: Path | None) -> subprocess.CompletedProcess:
    """ Probe with the gate channel exported, or WITHOUT the variable when the channel is `off`.

         `m43_gate_lib.run()` does not accept `env`, and the lib is not edited: the runner lives here.
    """
    env = dict(os.environ)
    if channel is None:
        env.pop(DUN_CHANNEL_ENV, None)
    else:
        env[DUN_CHANNEL_ENV] = str(channel)
    return subprocess.run(
        [str(binary), "--probe", PROBE],
        cwd=str(ROOT),
        capture_output=True,
        timeout=PROBE_TIMEOUT,
        check=False,
        env=env,
    )


def capture(binary: Path, channel: Path | None, source: str) -> dict:
    raw: list[str] = []
    exits: list[int] = []
    for _ in (1, 2):
        if channel is not None:
            truncate(channel)
        proc = run_probe(binary, channel)
        raw.append(proc.stdout.decode("utf-8", "replace"))
        exits.append(proc.returncode)
    norm: list[str] = []
    shaped: list[int] = []
    for text in raw:
        masked, count = normalize(text)
        norm.append(masked)
        shaped.append(count)
    text = read_text(ROOT / CONTENT_PATH)
    dun_lines, dun_source = read_dun_state(channel, source)
    return {
        "probe": raw,
        "norm": norm,
        "volatile": shaped,
        "exit": exits,
        "census": {CONTENT_PATH: text},
        "dun_lines": dun_lines,
        "dun_source": dun_source,
        "channel": "off" if channel is None else rel(channel),
        "sources": source_snapshot(),
        "binary": binary,
    }


@dataclass(frozen=True)
class Result:
    ok: bool
    facts: str
    token: str = "evidence_mismatch"
    symptom: str = ""
    found: bool = True


@dataclass(frozen=True)
class Row:
    rid: str
    name: str
    expected: str
    check: Callable[[dict], Result]
    falsifiers: tuple[str, ...] = ()

    @property
    def anchor(self) -> str:
        return ANCHORS[self.rid]


def check_geometry(cap: dict) -> Result:
    text = cap["norm"][0]
    if not text.strip():
        return Result(False, "stdout_bytes=0", found=False)
    got = parse_probe(text)
    absent = [key for key, _ in GOLDEN if key not in got]
    if absent:
        return Result(
            False,
            "absent=%s" % ",".join(absent),
            token="missing_evidence",
            symptom="the probe no longer prints the geometry rows the contract cites",
            found=False,
        )
    bad = [(key, got[key], want) for key, want in GOLDEN if got[key] != want]
    failed_rows = [row for row in got["pass_rows"] if row.startswith("FAIL:")]
    summary_ok = (
        got.get("passed") == got.get("checks")
        and got.get("probe_exit") == 0
        and not failed_rows
    )
    facts = "dungeons=%d/4232 blocks=%d doors=%d removed=%d cells_mm=%d tolerance_mm=%d passed=%d/%d checks_failed=%d" % (
        got["dungeons"],
        got["blocks"],
        got["doors"],
        got["removed"],
        got["cells_mm"],
        got["tolerance_mm"],
        got.get("passed", 0),
        got.get("checks", 0),
        len(failed_rows),
    )
    if bad or not summary_ok:
        detail = ",".join("%s=%d_want_%d" % (key, value, want) for key, value, want in bad[:4])
        if not detail:
            detail = "summary=%d/%d_exit=%d" % (
                got.get("passed", 0),
                got.get("checks", 0),
                got.get("probe_exit", -1),
            )
        return Result(
            False,
            facts + " divergences=" + detail,
            symptom="the geometry the probe measures no longer matches the M34 goldens",
        )
    return Result(True, facts)


def check_content_path(cap: dict) -> Result:
    text = cap["census"].get(CONTENT_PATH)
    if text is None:
        return Result(False, "file=%s lines=0" % CONTENT_PATH, found=False)
    census = census_of(text)
    facts = "assemble_calls=%d geometry_links=%d mentions=%d mention_lines=%s lines=%d" % (
        census["calls"],
        census["links"],
        len(census["mention_lines"]),
        ",".join(str(n) for n in census["mention_lines"]) or "-",
        census["lines"],
    )
    if census["calls"] >= 1 and census["links"] >= 1:
        return Result(True, facts)
    return Result(
        False,
        facts,
        symptom=(
            "the content path never calls the geometry assembler: its only `dungeon` "
            "mention is a string literal of an M28 event, not a load"
        ),
    )


def check_digest(cap: dict) -> Result:
    text1, text2 = cap["norm"]
    got1 = parse_probe(text1)
    got2 = parse_probe(text2)
    if "digest" not in got1 or "digest" not in got2:
        return Result(
            False,
            "digest_lines=%d,%d" % (int("digest" in got1), int("digest" in got2)),
            token="missing_evidence",
            symptom="the probe prints no DIGEST= line: the cross-process comparison has no subject",
            found=False,
        )
    digest1 = got1["digest"]
    digest2 = got2["digest"]
    block_equal = text1 == text2
    in_process = (
        got1.get("digest1") == got1.get("digest2")
        and got1.get("in_process") == 1
        and got2.get("in_process") == 1
    )
    facts = "digest=%s run1=%s run2=%s in_process=%d block_identical=%d block_digest=%s" % (
        digest1,
        digest1,
        digest2,
        1 if in_process else 0,
        1 if block_equal else 0,
        sha256_hex(text1.encode("utf-8"))[:16],
    )
    if digest1 != digest2:
        return Result(
            False,
            facts,
            token="digest_mismatch",
            symptom="the two independent processes print different geometry digests",
        )
    if not block_equal:
        return Result(
            False,
            facts,
            token="block_mismatch",
            symptom="the two independent processes print different geometry blocks",
        )
    if not in_process:
        return Result(
            False,
            facts,
            token="digest_mismatch",
            symptom="the probe's own in-process digest pair does not match",
        )
    return Result(True, facts)


def check_overlaps(cap: dict) -> Result:
    text = cap["census"].get(CONTENT_PATH) or ""
    census = census_of(text) if text else {"calls": 0}
    dun_lines = cap["dun_lines"]
    assemblies = [m for m in (RE_DUN_GEO.match(line) for line in dun_lines) if m]
    overlaps = [m for m in (RE_DUN_OVERLAP.match(line) for line in dun_lines) if m]

    if census["calls"] == 0:
        return Result(
            False,
            "consumed_dungeons=0 assembler_calls=0 overlap_rows=%d dun_state=%s"
            % (len(overlaps), cap["dun_source"]),
            token="blocked_by:M44-R2",
            symptom=(
                "the content path assembles no dungeon: the consumed set is empty, so "
                "no zero-overlap claim is measured"
            ),
        )
    if not assemblies:
        return Result(
            False,
            "consumed_dungeons=unknown assembled_rows=0 overlap_rows=%d dun_state=%s"
            % (len(overlaps), cap["dun_source"]),
            token="missing_evidence",
            symptom="the content path calls the assembler but publishes no `DUN VE: geo` row for the consumed set",
        )
    if not overlaps:
        return Result(
            False,
            "consumed_dungeons=%d assembled_rows=%d overlap_rows=0 dun_state=%s"
            % (len(assemblies), len(assemblies), cap["dun_source"]),
            token="missing_evidence",
            symptom="no `DUN VE: geo overlaps` row: the 0-overlap claim over the consumed set is unmeasured",
        )
    supplied = sum(int(m.group(2)) for m in overlaps)
    ok = supplied == 0 and all(m.group(3) == "1400" for m in overlaps)
    facts = "dungeons=%s overlaps=%d tolerance_mm=%s rows=%d dun_state=%s" % (
        assemblies[0].group(1),
        supplied,
        overlaps[0].group(3),
        len(overlaps),
        cap["dun_source"],
    )
    return Result(ok, facts, symptom="the consumed set declares overlapping doors at 1400 mm")


def check_sources(cap: dict) -> Result:
    snap = cap["sources"]
    if snap["fingerprint"] is None or snap["missing"]:
        return Result(
            False,
            "files=%d missing=%d detail=%s"
            % (len(snap["present"]), len(snap["missing"]), snap["detail"][:120]),
            token="missing_evidence",
            symptom="a measured source is absent from disk",
            found=bool(snap["present"]),
        )
    return Result(
        True,
        "fingerprint=%s files=%d missing=0" % (snap["fingerprint"], len(snap["present"])),
    )


def check_fresh(cap: dict) -> Result:
    binary: Path = cap["binary"]
    if not binary.is_file():
        return Result(False, "binary=absent path=%s" % rel(binary), found=False)
    binary_mtime = binary.stat().st_mtime_ns
    newest_mtime = 0
    newest_path = "-"
    for pattern in ("crates/**/*.rs", "crates/**/Cargo.toml"):
        for path in ROOT.glob(pattern):
            if not path.is_file():
                continue
            mtime = path.stat().st_mtime_ns
            if mtime > newest_mtime:
                newest_mtime = mtime
                newest_path = rel(path)
    stale = newest_mtime > binary_mtime
    return Result(
        not stale,
        "binary=%s sources_newest=%s stale=%d" % (rel(binary), newest_path, int(stale)),
        symptom="the binary predates %s: the capture may not come from this tree" % newest_path,
    )


def check_two_runs(cap: dict) -> Result:
    text1, text2 = cap["norm"]
    if not text1.strip():
        return Result(False, "block_bytes=0", found=False)
    if text1 != text2:
        return Result(
            False,
            "run1_bytes=%d run2_bytes=%d run1_digest=%s run2_digest=%s"
            % (
                len(text1.encode("utf-8")),
                len(text2.encode("utf-8")),
                sha256_hex(text1.encode("utf-8"))[:16],
                sha256_hex(text2.encode("utf-8"))[:16],
            ),
            token="capture_mismatch",
            symptom="the two independent processes printed different geometry blocks",
        )
    return Result(
        True,
        "identical=1 block_bytes=%d block_digest=%s"
        % (len(text1.encode("utf-8")), sha256_hex(text1.encode("utf-8"))[:16]),
    )


def check_volatile(cap: dict) -> Result:
    shaped = cap["volatile"]
    ok = shaped == [1, 1]
    return Result(
        ok,
        "excluded_lines=%s pattern=elapsed_pass1_ms_pass2_ms"
        % ("/".join(str(n) for n in shaped)),
        symptom="the declared exclusion is not bounded to exactly one chronometer line per capture",
    )


def check_citations(_cap: dict) -> Result:
    cites = [row.anchor for row in ROWS] + list(EXTRA_CITES)
    ok, detail = citation_check(cites)
    if not ok:
        return Result(False, detail, token="citation_unresolved", symptom="a row cites a file:line that does not resolve")
    absent_ok, absent_detail = resolve_cite("crates/app/src/.dun-geo-absent.rs:1")
    line_ok, line_detail = resolve_cite("crates/app/src/main.rs:999999")
    guards = (not absent_ok) and absent_detail.startswith("absent=") and (not line_ok) and line_detail.startswith("line>")
    return Result(
        guards,
        detail + " reject_absent=1 reject_line=1",
        token="citation_guard",
        symptom="the citation resolver accepts an absent path or a line past EOF",
    )


ROWS: tuple[Row, ...] = (
    Row(
        "M44-R1",
        "geometria M34 medida por el probe, reutilizada",
        "dungeons=4232 removed=4322 cells_mm=51200 tolerance_mm=1400 passed=12/12",
        check_geometry,
        ("geometry-dungeons", "geometry-cells", "geometry-removed"),
    ),
    Row(
        "M44-R2",
        "el camino de contenido consume esa geometria",
        "assemble_calls>=1 geometry_links>=1",
        check_content_path,
        ("content-assemble",),
    ),
    Row(
        "M44-R3",
        "digest x2 sobre dos procesos independientes",
        "digest_run1==digest_run2 block_identical=1",
        check_digest,
        ("digest-mismatch", "digest-block-mismatch"),
    ),
    Row(
        "M44-R4",
        "0 puertas solapadas a 1400 mm en el conjunto consumido",
        "overlaps=0 tolerance_mm=1400",
        check_overlaps,
        ("overlap-witness",),
    ),
    Row("PROC-sources", "huella de fuentes recomputada desde disco", "files=5 missing=0", check_sources),
    Row("PROC-two-runs", "dos procesos, bloque de geometria identico", "block_identical=1", check_two_runs),
    Row("PROC-volatile", "exclusion declarada y acotada", "excluded_lines=1/1", check_volatile),
    Row("PROC-citations", "cada fila cita file:line resoluble", "cites_resolved=all", check_citations),
    Row("PROC-fresh", "el binario no es anterior a las fuentes", "stale=0", check_fresh),
)

ROW_BY_ID = {row.rid: row for row in ROWS}


def copy_cap(cap: dict) -> dict:
    return {
        "probe": list(cap["probe"]),
        "norm": list(cap["norm"]),
        "volatile": list(cap["volatile"]),
        "exit": list(cap["exit"]),
        "census": dict(cap["census"]),
        "dun_lines": list(cap["dun_lines"]),
        "dun_source": cap["dun_source"],
        "channel": cap["channel"],
        "sources": cap["sources"],
        "binary": cap["binary"],
    }


def sub_once(text: str, old: str, new: str) -> str:
    if old not in text:
        raise ValueError("token ausente en la captura: %r" % old)
    return text.replace(old, new, 1)


def mut_dungeons(cap: dict) -> dict:
    out = copy_cap(cap)
    out["norm"] = [sub_once(text, "dungeons=4232", "dungeons=4231") for text in out["norm"]]
    return out


def mut_cells(cap: dict) -> dict:
    out = copy_cap(cap)
    out["norm"] = [sub_once(text, "cells=51.2 m", "cells=51.3 m") for text in out["norm"]]
    return out


def mut_removed(cap: dict) -> dict:
    out = copy_cap(cap)
    out["norm"] = [sub_once(text, "removed=4322", "removed=4325") for text in out["norm"]]
    return out


def mut_digest(cap: dict) -> dict:
    out = copy_cap(cap)
    out["norm"][1] = sub_once(out["norm"][1], "DIGEST=5573c56afce6749a", "DIGEST=0123456789abcdef")
    return out


def mut_block(cap: dict) -> dict:
    out = copy_cap(cap)
    out["norm"][1] = sub_once(out["norm"][1], "blocks=40263 doors=", "blocks=40264 doors=")
    return out


def _with_consumer(cap: dict) -> dict:
    out = copy_cap(cap)
    text = out["census"].get(CONTENT_PATH) or ""
    text += (
        "\nuse daggerfall_parser_bevy_bridge::dungeon_grid::assemble_dungeon;\n"
        "fn assemble_interior(retail: &RetailInterior) {\n"
        "    let grid: DungeonGrid = retail.assemble(layout);\n"
        "}\n"
    )
    out["census"][CONTENT_PATH] = text
    return out


def mut_content_consumer(cap: dict) -> dict:
    return _with_consumer(cap)


def mut_mention_only(cap: dict) -> dict:
    out = copy_cap(cap)
    text = out["census"].get(CONTENT_PATH) or ""
    text += "\n// the dungeon is dark, the dungeon keeps a dungeon door\n"
    out["census"][CONTENT_PATH] = text
    return out


def mut_overlap_zero(cap: dict) -> dict:
    out = _with_consumer(cap)
    out["dun_lines"] = [
        "DUN VE: geo dungeons=1 blocks=12 cells_mm=51200 tolerance_mm=1400 digest=0000000000000000",
        "DUN VE: geo overlaps dungeons=1 overlaps=0 tolerance_mm=1400",
    ]
    out["dun_source"] = "gate"
    return out


def mut_overlap_two(cap: dict) -> dict:
    out = _with_consumer(cap)
    out["dun_lines"] = [
        "DUN VE: geo dungeons=1 blocks=12 cells_mm=51200 tolerance_mm=1400 digest=0000000000000000",
        "DUN VE: geo overlaps dungeons=1 overlaps=2 tolerance_mm=1400",
    ]
    out["dun_source"] = "gate"
    return out


MENTION_ONLY_CENSUS = (
    "// the dungeon is dark, the dungeon keeps a dungeon door\n"
    "fn place(grid: &Grid) {\n"
    "    consume(grid);\n"
    "}\n"
)


def mention_only_capture(binary: Path) -> dict:
    """ Capture WITHOUT a consumer: the census only MENTIONS `dungeon`, in a comment.

         The `content-mention-only` guard asserts a property of the instrument (a
         mention does not satisfy the census), not of today's tree: that is why its base is this
         synthetic census and not the real file, which may gain the consumer at
         any moment without the guard ceasing to be true.
    """
    out = fixture_capture(binary)
    out["census"] = {CONTENT_PATH: MENTION_ONLY_CENSUS}
    return out


@dataclass(frozen=True)
class Falsifier:
    name: str
    target: str
    kind: str  # mutate | witness | guard
    how: str
    expect: str  # token esperado tras la mutacion
    apply: Callable[[dict], dict]
    base: Callable[[Path], dict] | None = None  # solo guards: captura de partida


FALSIFIERS: tuple[Falsifier, ...] = (
    Falsifier(
        "geometry-dungeons",
        "M44-R1",
        "mutate",
        "dungeons=4232 -> dungeons=4231 en las dos capturas",
        "evidence_mismatch",
        mut_dungeons,
    ),
    Falsifier(
        "geometry-cells",
        "M44-R1",
        "mutate",
        "celdas declaradas 51.2 m -> 51.3 m (51200 mm -> 51300 mm)",
        "evidence_mismatch",
        mut_cells,
    ),
    Falsifier(
        "geometry-removed",
        "M44-R1",
        "mutate",
        "removed=4322 -> removed=4325",
        "evidence_mismatch",
        mut_removed,
    ),
    Falsifier(
        "content-assemble",
        "M44-R2",
        "witness",
        "el camino de contenido gana el import del ensamblador y una llamada assemble(",
        "ok",
        mut_content_consumer,
    ),
    Falsifier(
        "digest-mismatch",
        "M44-R3",
        "mutate",
        "DIGEST= de la captura 2 -> otro digest de 16 hex",
        "digest_mismatch",
        mut_digest,
    ),
    Falsifier(
        "digest-block-mismatch",
        "M44-R3",
        "mutate",
        "blocks=40263 -> 40264 solo en la captura 2 (el DIGEST= sigue igual)",
        "block_mismatch",
        mut_block,
    ),
    Falsifier(
        "overlap-witness",
        "M44-R4",
        "witness",
        "consumidor + `DUN VE: geo` + `DUN VE: geo overlaps ... overlaps=0`",
        "ok",
        mut_overlap_zero,
    ),
    Falsifier(
        "content-mention-only",
        "M44-R2",
        "guard",
        "una mencion de `dungeon` en un comentario NO satisface el censo",
        "evidence_mismatch",
        mut_mention_only,
        base=mention_only_capture,
    ),
    Falsifier(
        "overlap-nonzero",
        "M44-R4",
        "guard",
        "la misma fila declarando overlaps=2 tiene que seguir roja",
        "evidence_mismatch",
        mut_overlap_two,
    ),
)

FALSIFIER_BY_NAME = {spec.name: spec for spec in FALSIFIERS}
GUARD_NAMES = tuple(spec.name for spec in FALSIFIERS if spec.kind == "guard")

FIXTURE_PROBE = (
    "PROBE-DUNGEON-001 - MAPDITEM dungeon grid (M34-D / G-N1), geometry only\n"
    "contract=docs/sintergic/game/programming/m34d-dungeon-grid-spec-CR-2d5c8f41.md * cells=51.2 m * door tolerance=1.4 m\n"
    "index regions=45 locations=15251 dungeons=4232 without_dungeon=11019 maps=22129312 B blocks=32884997 B\n"
    "grid blocks=40263 doors=376547 removed=4322 with_dup=1303 start_markers=4232 enter_markers=775 missing_blocks=0 unparsed_blocks=0\n"
    "sample[0] region=000 idx=2 location_id=26219 blocks=12 extent=(-3,1,-1,2) start=N0000006.RDB enter=false doors=129 removed=0 PASS\n"
    "\n"
    "-- determinism (full index x2, same process) --\n"
    "digest1=5573c56afce6749a digest2=5573c56afce6749a identical=true\n"
    "DIGEST=5573c56afce6749a (compare across two separate processes)\n"
    "elapsed pass1=<ms> ms pass2=<ms> ms\n"
    "PASS dungeons\n"
    "PASS without_dungeon\n"
    "PASS locations\n"
    "PASS blocks_resolved\n"
    "PASS blocks_parsed\n"
    "PASS doors_removed\n"
    "PASS dungeons_with_dup\n"
    "PASS start_markers\n"
    "PASS enter_markers\n"
    "PASS sample\n"
    "PASS orsinium\n"
    "PASS determinism\n"
    "\n"
    "SUMMARY PROBE-DUNGEON-001: passed=12/12 * PASS EXIT=0\n"
)

FIXTURE_CENSUS = (
    "use daggerfall_parser_bevy_bridge::dungeon_grid::assemble_dungeon;\n"
    "use daggerfall_parser_bevy_bridge::dungeon_grid::DungeonGrid;\n"
    "\n"
    "fn assemble_interior(retail: &RetailInterior, layout: &Layout) {\n"
    "    let grid: DungeonGrid = retail.assemble(layout);\n"
    "    place(grid);\n"
    "}\n"
)

FIXTURE_DUN = [
    "DUN VE: geo dungeons=1 blocks=12 cells_mm=51200 tolerance_mm=1400 digest=0000000000000000",
    "DUN VE: geo overlaps dungeons=1 overlaps=0 tolerance_mm=1400",
]


def fixture_capture(binary: Path) -> dict:
    masked, count = normalize(FIXTURE_PROBE)
    return {
        "probe": [masked, masked],
        "norm": [masked, masked],
        "volatile": [count, count],
        "exit": [0, 0],
        "census": {CONTENT_PATH: FIXTURE_CENSUS},
        "dun_lines": list(FIXTURE_DUN),
        "dun_source": "gate",
        "channel": "fixture",
        "sources": source_snapshot(),
        "binary": binary,
    }


def base_capture(binary: Path) -> dict:
    """ Capture with the fixture geometry and the REAL content-path census."""
    masked, count = normalize(FIXTURE_PROBE)
    return {
        "probe": [masked, masked],
        "norm": [masked, masked],
        "volatile": [count, count],
        "exit": [0, 0],
        "census": {CONTENT_PATH: read_text(ROOT / CONTENT_PATH)},
        "dun_lines": [],
        "dun_source": "off",
        "channel": "off",
        "sources": source_snapshot(),
        "binary": binary,
    }


def render(row: Row, result: Result) -> str:
    if result.ok:
        return "[OK] %s %s - %s src=%s" % (row.rid, row.name, result.facts, row.anchor)
    if not result.found:
        return (
            "[FAIL] %s %s - missing_evidence %s expected=%s "
            "(the evidence this row needs does not exist on disk) src=%s"
            % (row.rid, row.name, result.facts, row.expected, row.anchor)
        )
    return "[FAIL] %s %s - %s %s expected=%s (%s) src=%s" % (
        row.rid,
        row.name,
        result.token,
        result.facts,
        row.expected,
        result.symptom,
        row.anchor,
    )


FLOAT_TOKEN = re.compile(r"\b[a-z][a-z0-9_]*=-?\d+\.\d+")


def floats_in(lines: list[str]) -> list[str]:
    found: list[str] = []
    for line in lines:
        for match in FLOAT_TOKEN.finditer(line):
            found.append(match.group(0))
    return found


def header_lines() -> list[str]:
    commit = head_commit() or "unknown"
    return [
        "GATE %s - M44 bloque A (W44-1) cr=%s head=%s" % (GATE, CR, commit[:12]),
        "PROBE %s - dos procesos independientes del binario" % PROBE,
    ]


def sources_line(cap: dict) -> str:
    snap = cap["sources"]
    fingerprint = snap["fingerprint"] or ("0" * 64)
    return "SOURCES fingerprint=%s files=%d paths=%s" % (
        fingerprint,
        len(snap["present"]),
        snap["paths"],
    )


def note_lines(cap: dict) -> list[str]:
    text1 = cap["norm"][0]
    text2 = cap["norm"][1]
    got1 = parse_probe(text1)
    got2 = parse_probe(text2)
    census = census_of(cap["census"].get(CONTENT_PATH) or "")
    return [
        "NOTE channel=%s source=%s" % (cap["channel"], cap["dun_source"]),
        "NOTE probe_run1 exit=%d lines=%d block_bytes=%d digest=%s"
        % (cap["exit"][0], len(text1.splitlines()), len(text1.encode("utf-8")), got1.get("digest", "-")),
        "NOTE probe_run2 exit=%d lines=%d block_bytes=%d digest=%s"
        % (cap["exit"][1], len(text2.splitlines()), len(text2.encode("utf-8")), got2.get("digest", "-")),
        "NOTE capture=census file=%s lines=%d assemble_calls=%d geometry_links=%d"
        % (CONTENT_PATH, census["lines"], census["calls"], census["links"]),
        "NOTE capture=dun_state lines=%d source=%s" % (len(cap["dun_lines"]), cap["dun_source"]),
    ]


def seal(body: list[str], out: Path | None, rc: int) -> int:
    lines = list(body)
    offenders = floats_in(lines)
    lines.append(
        "[%s] PROC-no-floats decimales_en_stdout - tokens=%d detail=%s"
        % ("OK" if not offenders else "FAIL", len(offenders), ",".join(offenders[:4]) or "none")
    )
    text = "\n".join(lines) + "\n"
    digest = sha256_hex(text.encode("utf-8"))
    final = text + "DIGEST " + digest + "\n"
    sys.stdout.write(final)
    sys.stdout.flush()
    if out is not None:
        out.mkdir(parents=True, exist_ok=True)
        (out / "gate.stdout").write_text(final, encoding="utf-8")
    if offenders:
        return 1
    return rc


def verdict_line(results: list[tuple[Row, Result]]) -> tuple[str, int]:
    red = [row for row, result in results if not result.ok]
    tokens = ",".join(sorted({result.token for _, result in results if not result.ok}))
    if not red:
        return "VERDICT: PASS - rows=%d red=0" % len(results), 0
    return (
        "VERDICT: FAIL - red=%d/%d rows=%s tokens=%s"
        % (len(red), len(results), ",".join(row.rid for row in red), tokens or "-"),
        1,
    )


def mode_list(args: argparse.Namespace) -> int:
    lines = header_lines()
    snap = source_snapshot()
    lines.append(
        "SOURCES fingerprint=%s files=%d paths=%s"
        % (snap["fingerprint"] or ("0" * 64), len(snap["present"]), snap["paths"])
    )
    lines.append(
        "channel default=%s source=gate env=%s override=--channel=<path> off=--channel %s"
        % (DEFAULT_CHANNEL, DUN_CHANNEL_ENV, CHANNEL_OFF)
    )
    for row in ROWS:
        lines.append(
            "row %s name=%s expected=%s falsifier=%s src=%s"
            % (
                row.rid,
                row.name,
                row.expected.replace(" ", ";"),
                ",".join(row.falsifiers) or "-",
                row.anchor,
            )
        )
    for spec in FALSIFIERS:
        lines.append(
            "falsifier %s target=%s kind=%s how=%s" % (spec.name, spec.target, spec.kind, spec.how)
        )
    ok, detail = citation_check([row.anchor for row in ROWS] + list(EXTRA_CITES))
    lines.append("[%s] PROC-citations catalogo - %s src=%s" % ("OK" if ok else "FAIL", detail, ANCHOR_GATE))
    lines.append(
        "VEREDICTO-LIST: %s - rows=%d falsifiers=%d guards=%d"
        % ("PASS" if ok else "FAIL", len(ROWS), len(FALSIFIERS), len(GUARD_NAMES))
    )
    text = "\n".join(lines) + "\n"
    sys.stdout.write(text + "DIGEST " + sha256_hex(text.encode("utf-8")) + "\n")
    return 0 if ok else 1


def mode_selftest(args: argparse.Namespace) -> int:
    binary = Path(args.binary).expanduser()
    if not binary.is_absolute():
        binary = ROOT / binary
    lines = header_lines() + ["SELFTEST fixture=contract-lines (verde) + captura base (rojo de hoy)"]
    failures: list[str] = []

    green = fixture_capture(binary)
    lines.append("SELFTEST block=green-fixture")
    for row in ROWS:
        result = row.check(green)
        lines.append(render(row, result))
        if not result.ok:
            failures.append("green:" + row.rid)

    base = base_capture(binary)
    lines.append("SELFTEST block=base-capture")
    for rid in ("M44-R2", "M44-R4"):
        result = ROW_BY_ID[rid].check(base)
        lines.append(
            "SELFTEST base %s state=%s token=%s"
            % (rid, "OK" if result.ok else "RED", "ok" if result.ok else result.token)
        )

    lines.append("SELFTEST block=falsifiers")
    for spec in FALSIFIERS:
        row = ROW_BY_ID[spec.target]
        if spec.kind == "mutate":
            source = green
        elif spec.base is not None:
            source = spec.base(binary)
        else:
            source = base
        before = row.check(source)
        try:
            after = row.check(spec.apply(source))
        except ValueError as exc:
            lines.append(
                "FALSIFY %s - target=%s detail=error:%s => MISSED"
                % (spec.name, spec.target, str(exc)[:80])
            )
            failures.append("falsify:" + spec.name)
            continue
        if spec.kind == "mutate":
            caught = before.ok and not after.ok and after.token == spec.expect
        elif spec.kind == "witness":
            caught = after.ok or (before.ok and after.ok)
        else:
            caught = (not before.ok) and (not after.ok) and after.token == spec.expect
        lines.append(
            "FALSIFY %s - target=%s kind=%s before=%s after=%s expected=%s => %s"
            % (
                spec.name,
                spec.target,
                spec.kind,
                "OK" if before.ok else "RED:" + before.token,
                "OK" if after.ok else "RED:" + after.token,
                spec.expect,
                "CAUGHT" if caught else "MISSED",
            )
        )
        if not caught:
            failures.append("falsify:" + spec.name)

    if failures:
        lines.append("VEREDICTO-GATE: FAIL - failures=%s" % ",".join(failures[:6]))
    else:
        lines.append(
            "VEREDICTO-GATE: PASS - rows=%d green=all falsifiers=%d caught guards=%d"
            % (len(ROWS), len(FALSIFIERS), len(GUARD_NAMES))
        )
    return seal(lines, None, 0 if not failures else 1)


def mode_falsify(args: argparse.Namespace) -> int:
    spec = FALSIFIER_BY_NAME.get(args.falsify or "")
    if spec is None:
        sys.stderr.write(
            "falsify desconocido: %r (validos: %s)\n" % (args.falsify, ",".join(FALSIFIER_BY_NAME))
        )
        return 2
    binary = Path(args.binary).expanduser()
    if not binary.is_absolute():
        binary = ROOT / binary
    lines = header_lines() + [
        "FALSIFY %s - target=%s kind=%s how=%s" % (spec.name, spec.target, spec.kind, spec.how)
    ]
    row = ROW_BY_ID[spec.target]
    if spec.kind == "mutate":
        source = fixture_capture(binary)
    elif spec.base is not None:
        source = spec.base(binary)
    else:
        source = base_capture(binary)
    before = row.check(source)
    try:
        after = row.check(spec.apply(source))
    except ValueError as exc:
        lines.append("FALSIFY %s - detail=error:%s => MISSED" % (spec.name, str(exc)[:100]))
        lines.append("VEREDICTO-FALSIFY: FAIL - %s" % spec.name)
        return seal(lines, None, 1)
    if spec.kind == "mutate":
        caught = before.ok and not after.ok and after.token == spec.expect
    elif spec.kind == "witness":
        caught = after.ok or (before.ok and after.ok)
    else:
        caught = (not before.ok) and (not after.ok) and after.token == spec.expect
    lines.append("FALSIFY %s - before=%s" % (spec.name, render(row, before)))
    lines.append("FALSIFY %s - after=%s" % (spec.name, render(row, after)))
    lines.append(
        "FALSIFY %s - verdict before_state=%s after_state=%s expected=%s => %s"
        % (
            spec.name,
            "OK" if before.ok else "RED:" + before.token,
            "OK" if after.ok else "RED:" + after.token,
            spec.expect,
            "CAUGHT" if caught else "MISSED",
        )
    )
    lines.append("VEREDICTO-FALSIFY: %s - %s" % ("PASS" if caught else "FAIL", spec.name))
    return seal(lines, None, 0 if caught else 1)


def mode_run(args: argparse.Namespace) -> int:
    binary = Path(args.binary).expanduser()
    if not binary.is_absolute():
        binary = ROOT / binary
    out = Path(args.out).expanduser()
    if not out.is_absolute():
        out = ROOT / out
    channel, channel_source = resolve_channel(args.channel, out)
    channel_note = "NOTE channel=%s source=%s" % (
        "off" if channel is None else rel(channel),
        channel_source,
    )

    present, detail = retail_present()
    if not present:
        lines = header_lines() + [channel_note, "[NC] PROC-retail retail_disponible - %s (G12)" % detail]
        for row in ROWS:
            lines.append("[NC] %s - not_measured (G12 retail absent) src=%s" % (row.rid, row.anchor))
        lines.append("VERDICT: NC - environment: retail ARENA2 absent (exit 2, never a false PASS)")
        return seal(lines, out, 2)

    if not args.skip_build:
        state, build_detail = lib_build(False)
        if state == "failed":
            lines = header_lines() + [
                channel_note,
                "[NC] PROC-build build - state=%s detail=%s (G12)" % (state, build_detail[:120]),
            ]
            lines.append("VERDICT: NC - environment: build failed (exit 2)")
            return seal(lines, out, 2)

    if not binary.is_file():
        lines = header_lines() + [
            channel_note,
            "[NC] PROC-binary binario_presente - path=%s (G12)" % rel(binary),
        ]
        lines.append("VERDICT: NC - environment: binary absent (exit 2)")
        return seal(lines, out, 2)

    cap = capture(binary, channel, channel_source)
    if any(code == 2 for code in cap["exit"]):
        lines = header_lines() + [
            channel_note,
            "[NC] PROC-probe probe_retail - exit=2 run1=%d run2=%d (G12)"
            % (cap["exit"][0], cap["exit"][1]),
        ]
        lines.append("VERDICT: NC - environment: the probe cannot reach ARENA2 (exit 2)")
        return seal(lines, out, 2)

    body = header_lines() + [sources_line(cap)] + note_lines(cap)
    results: list[tuple[Row, Result]] = []
    for row in ROWS:
        result = row.check(cap)
        results.append((row, result))
        body.append(render(row, result))
    verdict, rc = verdict_line(results)
    body.append(verdict)

    out.mkdir(parents=True, exist_ok=True)
    (out / "run1.probe.txt").write_text(cap["probe"][0], encoding="utf-8")
    (out / "run2.probe.txt").write_text(cap["probe"][1], encoding="utf-8")
    manifest = {
        "gate": GATE,
        "cr": CR,
        "head": head_commit(),
        "probe": PROBE,
        "binary": rel(binary),
        "binary_digest": file_sha256(binary),
        "sources_fingerprint": cap["sources"]["fingerprint"],
        "sources": list(cap["sources"]["present"]),
        "channel": {"path": cap["channel"], "source": cap["dun_source"]},
        "probe_exit": cap["exit"],
        "block_digest": sha256_hex(cap["norm"][0].encode("utf-8"))[:16],
        "verdict": verdict,
        "rows": [
            {
                "id": row.rid,
                "ok": result.ok,
                "token": "ok" if result.ok else result.token,
                "facts": result.facts,
                "anchor": row.anchor,
            }
            for row, result in results
        ],
    }
    (out / "manifest.json").write_text(
        json.dumps(manifest, indent=2, sort_keys=True) + "\n", encoding="utf-8"
    )
    if args.json:
        body = json.dumps(manifest, indent=2, sort_keys=True).splitlines() + body
    return seal(body, out, rc)


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(
        prog="dun-geo-001.py",
        description="%s - M44 bloque A: la geometria M34 es la que consume el camino de contenido" % GATE,
    )
    parser.add_argument("--binary", default=DEFAULT_BINARY, help="binario a medir (default: %(default)s)")
    parser.add_argument("--out", default=DEFAULT_OUT, help="directorio de artefactos (gitignored)")
    parser.add_argument("--skip-build", action="store_true", help="reutiliza el binario ya construido")
    parser.add_argument("--json", action="store_true", help="imprime el manifiesto JSON antes del bloque")
    parser.add_argument(
        "--channel",
        default=None,
        metavar="PATH|off",
        help="canal runtime del producto resuelto por el gate (default: <out>/%s; "
        "'%s' corre el probe SIN %s)" % (DEFAULT_CHANNEL, CHANNEL_OFF, DUN_CHANNEL_ENV),
    )
    parser.add_argument("--list", action="store_true", help="catalogo de filas y falsificadores")
    parser.add_argument("--falsify", metavar="NAME", help="aplica un falsificador a la captura y exige la reaccion")
    parser.add_argument("--self-test", action="store_true", help="verifica instrumento, fixture verde y falsificadores")
    args = parser.parse_args(argv)

    if args.list:
        return mode_list(args)
    if args.self_test:
        return mode_selftest(args)
    if args.falsify:
        return mode_falsify(args)
    return mode_run(args)


if __name__ == "__main__":
    sys.exit(main())
