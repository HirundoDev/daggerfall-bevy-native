#!/usr/bin/env python3
"""SYS-FACTION-001 - gate of M42/W42-3: faction tree with census, rank/service tables, versioned reputation.

Process contract (mirrors scripts/sys-npc-001.py):
  1. Two separate OS processes run `daggerfall-app --probe SYS-FACTION-001`.
  2. Both stdouts must be byte-identical (determinism gate).
  3. `DIGEST <sha256>` (last line) must equal sha256 of all preceding bytes.
  4. Every `[OK]` row must carry a `src=`/`spec=` citation to a real repo path;
     every `NEGATIVE` row must carry a `reason=` path that exists.
  5. `--falsify <mode>` mutates the clean capture and requires the named rule
     to go RED, then re-checks the clean capture is still PASS (green restore).

Golden values (measured from retail ARENA2/faction.txt, stable):
  blocks=366 bytes=74221 dup=1 roots=102 leaves=270 depth=4 unknown=0
  type[7]=46 type[15]=44 provinces=46 ggroup[11]=5 ggroup[17]=51
  rank rows=10 row0=(0,22,4) row9=(90,87,37)
  promotion: join rank 0 +28d => rank 3, services 5->6, library false->true
  mages 5,5,5,6,7,8,9,9,10,10 fighters x7 knights x3 temples x8
  Arkay soulgems@3=false soulgems@4=true
  ledger v1 id=10 amount=10 deltas=26 digest 7529b7fa088c4d59->d118a6190496d3ed
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import shutil
import subprocess
import sys
from dataclasses import dataclass, replace
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
GATE = "M42/W42-3"
PROBE = "PROBE-SYS-FACTION-001"
SLUG = "sys-faction-001"
BINARY = ROOT / "target" / "debug" / "daggerfall-app"
ARTIFACTS = ROOT / ".sintergic" / "artifacts" / "m42-sys" / SLUG
CR = "CR-b434f896-2fe9-4750-b70e-9b748370e6c3"
HEADER_PREFIX = (
    "PROBE-SYS-FACTION-001 - faction tree with census, rank/service tables, "
    f"versioned reputation (M42/W42-3, {CR})"
)

BLOCKS = 366
BYTES = 74221
DUP = 1
ROOTS = 102
LEAVES = 270
DEPTH = 4
T7 = 46
T15 = 44
PROVINCES = 46
G11 = 5
G17 = 51
RANK_ROWS = 10
ROW0 = "0,22,4"
ROW9 = "90,87,37"
PROMO_RANK = 3
LEDGER_DELTAS = 26
OK_ROWS = 13

DISK_LINE = re.compile(r"^DISK:faction\.txt bytes=(\d+) blocks=(\d+) digest=([0-9a-f]{16})$")
CENSUS_LINE = re.compile(
    r"^CENSUS total=(\d+) dup=(\d+) roots=(\d+) leaves=(\d+) depth=(\d+) unknown=(\d+)$"
)
TYPES_LINE = re.compile(r"^TYPES (.*)$")
TYPE_CELL = re.compile(r"t\[(\d+)\]=(\d+)")
RETAIL_LINE = re.compile(r"^RETAIL provinces=(\d+) ggroup\[11\]=(\d+) ggroup\[17\]=(\d+)$")
RANKS_LINE = re.compile(r"^RANKS rows=(\d+) row0=\(([^)]+)\) row9=\(([^)]+)\)$")
PROMO_LINE = re.compile(
    r"^PROMOTION join_rank=(\d+) event=(\S+) rank=(\d+) services=(\d+)->(\d+) library=(\S+)->(\S+)$"
)
MAGES_LINE = re.compile(r"^MAGES counts=([\d,]+)$")
FIGHTERS_LINE = re.compile(r"^FIGHTERS counts=([\d,]+) KNIGHTS counts=([\d,]+)$")
ARKAY_LINE = re.compile(
    r"^ARKAY soulgems@3=(\S+) soulgems@4=(\S+) rank0_has_magic=(\S+)$"
)
TEMPLE_LINE = re.compile(r"^TEMPLE/(\S+) counts=([\d,]+)$")
LEDGER_LINE = re.compile(
    r"^LEDGER v1 id=(\d+) amount=(\d+) propagate=(\S+) deltas=(\d+) digest=([0-9a-f]+)->([0-9a-f]+)$"
)
NEGATIVE_LINE = re.compile(r"^NEGATIVE (\S+) reason=(\S+)")
OK_LINE = re.compile(r"^\[OK\] (\S+) - (.*)$")
FAIL_LINE = re.compile(r"^FAIL (\S+) (.*)$")
CITATION = re.compile(r"\b(src|spec)=\S+")
DIGEST_LINE = re.compile(r"^DIGEST ([0-9a-f]{64})$")
PATH_SUFFIX = re.compile(r"^(.*?)(?::\d+(?:-\d+)?)?$")


@dataclass(frozen=True)
class Capture:
    run: int
    exit: int
    stdout: bytes
    stderr: bytes


@dataclass(frozen=True)
class Rule:
    name: str
    ok: bool
    detail: str


def run(cmd: list[str], env: dict[str, str] | None = None) -> subprocess.CompletedProcess:
    return subprocess.run(
        cmd,
        cwd=ROOT,
        env={**os.environ, **(env or {})},
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
    )


def digest(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def file_digest(path: Path) -> str | None:
    if not path.is_file():
        return None
    hasher = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1 << 20), b""):
            hasher.update(chunk)
    return hasher.hexdigest()


def head_commit() -> str | None:
    proc = run(["git", "rev-parse", "HEAD"])
    if proc.returncode != 0:
        return None
    return proc.stdout.decode("utf-8", "replace").strip() or None


def lines_of(capture: Capture) -> list[str]:
    return capture.stdout.decode("utf-8", "replace").splitlines()


def prefixed(lines: list[str], prefix: str) -> list[str]:
    return [line for line in lines if line.startswith(prefix)]


def path_exists(raw: str) -> bool:
    """`reason=<path>[:line[-line]]` -> does the file exist in the repo?"""
    match = PATH_SUFFIX.match(raw)
    candidate = (match.group(1) if match else raw).strip()
    if not candidate:
        return False
    path = Path(candidate)
    return (path if path.is_absolute() else ROOT / path).exists()


# ── rules ──────────────────────────────────────────────────────────────────


def header_rule(lines: list[str]) -> tuple[bool, str]:
    if not lines:
        return False, "empty-stdout"
    if lines[0] != HEADER_PREFIX:
        return False, f"header={lines[0][:72]!r}"
    return True, f"cr={CR}"


def disk_census_rule(lines: list[str]) -> tuple[bool, str]:
    disks = [DISK_LINE.match(l) for l in prefixed(lines, "DISK:")]
    census = [CENSUS_LINE.match(l) for l in prefixed(lines, "CENSUS ")]
    if len(disks) != 1 or disks[0] is None:
        return False, f"disk-rows={len(disks)}"
    if len(census) != 1 or census[0] is None:
        return False, f"census-rows={len(census)}"
    b, bl, _ = disks[0].groups()
    t, d, _, _, _, _ = census[0].groups()
    ok = int(b) == BYTES and int(bl) == BLOCKS and int(t) == BLOCKS and int(d) == DUP
    return ok, f"bytes={b} blocks={bl} total={t} dup={d}"


def census_shape_rule(lines: list[str]) -> tuple[bool, str]:
    rows = [CENSUS_LINE.match(l) for l in prefixed(lines, "CENSUS ")]
    if len(rows) != 1 or rows[0] is None:
        return False, "no-CENSUS"
    _, _, r, lv, dp, un = (int(v) for v in rows[0].groups())
    ok = r == ROOTS and lv == LEAVES and dp == DEPTH and un == 0
    return ok, f"roots={r} leaves={lv} depth={dp} unknown={un}"


def census_types_rule(lines: list[str]) -> tuple[bool, str]:
    rows = prefixed(lines, "TYPES ")
    if len(rows) != 1:
        return False, f"types-rows={len(rows)}"
    match = TYPES_LINE.match(rows[0])
    if match is None:
        return False, "no-TYPES"
    cells = dict((int(a), int(b)) for a, b in TYPE_CELL.findall(match.group(1)))
    if len(cells) != 16:
        return False, f"buckets={len(cells)}"
    ok = cells.get(7) == T7 and cells.get(15) == T15 and sum(cells.values()) == BLOCKS
    return ok, f"t7={cells.get(7)} t15={cells.get(15)} sum={sum(cells.values())}"


def census_ggroups_rule(lines: list[str]) -> tuple[bool, str]:
    rows = [RETAIL_LINE.match(l) for l in prefixed(lines, "RETAIL ")]
    if len(rows) != 1 or rows[0] is None:
        return False, f"retail-rows={len(rows)}"
    p, g11, g17 = (int(v) for v in rows[0].groups())
    ok = p == PROVINCES and g11 == G11 and g17 == G17
    return ok, f"provinces={p} g11={g11} g17={g17}"


def rank_table_rule(lines: list[str]) -> tuple[bool, str]:
    rows = [RANKS_LINE.match(l) for l in prefixed(lines, "RANKS ")]
    if len(rows) != 1 or rows[0] is None:
        return False, f"ranks-rows={len(rows)}"
    n, r0, r9 = rows[0].groups()
    ok = int(n) == RANK_ROWS and r0 == ROW0 and r9 == ROW9
    return ok, f"rows={n} row0=({r0}) row9=({r9})"


def promotion_rule(lines: list[str]) -> tuple[bool, str]:
    rows = [PROMO_LINE.match(l) for l in prefixed(lines, "PROMOTION ")]
    if len(rows) != 1 or rows[0] is None:
        return False, f"promo-rows={len(rows)}"
    jr, _ev, rank, s0, s1, lib0, lib1 = rows[0].groups()
    ok = (
        int(jr) == 0
        and int(rank) == PROMO_RANK
        and int(s0) == 5
        and int(s1) == 6
        and lib0 == "false"
        and lib1 == "true"
    )
    return ok, f"rank={rank} services={s0}->{s1} library={lib0}->{lib1}"


def service_tables_rule(lines: list[str]) -> tuple[bool, str]:
    mages = [MAGES_LINE.match(l) for l in prefixed(lines, "MAGES ")]
    ff = [FIGHTERS_LINE.match(l) for l in prefixed(lines, "FIGHTERS ")]
    temples = [TEMPLE_LINE.match(l) for l in prefixed(lines, "TEMPLE/")]
    if len(mages) != 1 or mages[0] is None:
        return False, "no-MAGES"
    if len(ff) != 1 or ff[0] is None:
        return False, "no-FIGHTERS"
    if len(temples) != 8:
        return False, f"temples={len(temples)}"
    mc = mages[0].group(1).split(",")
    ok = mc[0] == "5" and mc[-2:] == ["10", "10"] and len(mc) == 10
    fc, kc = ff[0].groups()
    ok = ok and all(v == "7" for v in fc.split(",")) and all(v == "3" for v in kc.split(","))
    ok = ok and all(len(t.group(2).split(",")) == 10 for t in temples if t is not None)
    return ok, f"mages={mages[0].group(1)[:13]}.. temples={len(temples)}"


def service_thresholds_rule(lines: list[str]) -> tuple[bool, str]:
    rows = [ARKAY_LINE.match(l) for l in prefixed(lines, "ARKAY ")]
    if len(rows) != 1 or rows[0] is None:
        return False, f"arkay-rows={len(rows)}"
    s3, s4, m0 = rows[0].groups()
    ok = s3 == "false" and s4 == "true" and m0 == "true"
    return ok, f"soulgems@3={s3} @4={s4}"


def ledger_rule(lines: list[str]) -> tuple[bool, str]:
    rows = [LEDGER_LINE.match(l) for l in prefixed(lines, "LEDGER ")]
    if len(rows) != 1 or rows[0] is None:
        return False, f"ledger-rows={len(rows)}"
    fid, amt, prop, deltas, d0, d1 = rows[0].groups()
    ok = (
        int(fid) == 10
        and int(amt) == 10
        and prop == "true"
        and int(deltas) == LEDGER_DELTAS
        and len(d0) == 16
        and len(d1) == 16
        and d0 != d1
    )
    return ok, f"deltas={deltas} linked={d0[:8]}->{d1[:8]}"


def ok_rows_rule(lines: list[str]) -> tuple[bool, str]:
    rows = prefixed(lines, "[OK] ")
    parsed = [OK_LINE.match(l) for l in rows]
    ok = len(rows) == OK_ROWS and all(p is not None for p in parsed)
    return ok, f"ok-rows={len(rows)}"


def citation_rule(lines: list[str]) -> tuple[bool, str]:
    rows = prefixed(lines, "[OK] ")
    missing: list[str] = []
    for line in rows:
        match = OK_LINE.match(line)
        if match is None or not CITATION.search(match.group(2)):
            missing.append(match.group(1) if match else line[:24])
            continue
        for path in CITATION.findall(match.group(2)):
            _ = path
        for raw in re.findall(r"(?:src|spec)=(\S+)", match.group(2)):
            if not path_exists(raw):
                missing.append(f"{match.group(1)}:{raw}")
    total = len(rows)
    return (not missing and total > 0), f"ok-rows={total} uncited={','.join(missing[:6]) or 'none'}"


def negative_rule(lines: list[str]) -> tuple[bool, str]:
    rows = [NEGATIVE_LINE.match(line) for line in prefixed(lines, "NEGATIVE ")]
    if len(rows) < 2 or any(r is None for r in rows):
        return False, f"negative={len(rows)}"
    bad = [r.group(2) for r in rows if r is not None and not path_exists(r.group(2))]
    return (not bad), f"negative={len(rows)} missing-path={','.join(bad[:6]) or 'none'}"


def no_fail_rule(lines: list[str]) -> tuple[bool, str]:
    fails = prefixed(lines, "FAIL ")
    return (not fails), f"fail-rows={len(fails)}"


def verdict_rule(lines: list[str]) -> tuple[bool, str]:
    body = [line for line in lines if not line.startswith("DIGEST ")]
    if not body:
        return False, "empty-stdout"
    return (body[-1] == "VEREDICTO: PASS"), f"last={body[-1][:40]!r}"


def digest_rule(captures: list[Capture]) -> tuple[bool, str]:
    details: list[str] = []
    ok = True
    for capture in captures:
        raw = capture.stdout
        parts = raw.split(b"\n")
        while parts and parts[-1] == b"":
            parts.pop()
        if not parts:
            return False, f"run{capture.run}=empty"
        last = parts[-1].decode("utf-8", "replace")
        match = DIGEST_LINE.match(last)
        if match is None:
            return False, f"run{capture.run}=no-DIGEST"
        body = b"\n".join(parts[:-1]) + b"\n"
        expected = digest(body)
        if match.group(1) != expected:
            ok = False
            details.append(f"run{capture.run}:{match.group(1)[:8]}!={expected[:8]}")
        else:
            details.append(f"run{capture.run}:{match.group(1)[:8]}")
    return ok, ",".join(details)


def check(captures: list[Capture]) -> list[Rule]:
    first, second = captures
    lines_one = lines_of(first)
    sha_one, sha_two = digest(first.stdout), digest(second.stdout)
    rules = [
        Rule("run1-verdict", "VEREDICTO: PASS" in first.stdout.decode("utf-8", "replace")
             and first.exit == 0, f"exit={first.exit}"),
        Rule("run2-verdict", "VEREDICTO: PASS" in second.stdout.decode("utf-8", "replace")
             and second.exit == 0, f"exit={second.exit}"),
        Rule("byte-identical", first.stdout == second.stdout, f"{len(first.stdout)} B"),
        Rule("sha256-equal", sha_one == sha_two, sha_one[:16]),
        Rule("header", *header_rule(lines_one)),
        Rule("disk-census", *disk_census_rule(lines_one)),
        Rule("census-shape", *census_shape_rule(lines_one)),
        Rule("census-types", *census_types_rule(lines_one)),
        Rule("census-ggroups", *census_ggroups_rule(lines_one)),
        Rule("rank-table", *rank_table_rule(lines_one)),
        Rule("promotion-path", *promotion_rule(lines_one)),
        Rule("service-tables", *service_tables_rule(lines_one)),
        Rule("service-thresholds", *service_thresholds_rule(lines_one)),
        Rule("ledger-versioned", *ledger_rule(lines_one)),
        Rule("ok-rows", *ok_rows_rule(lines_one)),
        Rule("citation-present", *citation_rule(lines_one)),
        Rule("negative-paths", *negative_rule(lines_one)),
        Rule("no-fail-rows", *no_fail_rule(lines_one)),
        Rule("verdict-last", *verdict_rule(lines_one)),
        Rule("digest-integrity", *digest_rule(captures)),
        Rule("probe-digest", *probe_digest_rule(captures)),
    ]
    return rules


def probe_digest_rule(captures: list[Capture]) -> tuple[bool, str]:
    values: list[str | None] = []
    for capture in captures:
        found = [
            match.group(1)
            for line in lines_of(capture)
            if (match := DIGEST_LINE.match(line))
        ]
        values.append(found[0] if len(found) == 1 else None)
    ok = values[0] is not None and values[0] == values[1]
    return ok, (values[0][:16] if values[0] else "absent")


def failed_names(rules: list[Rule]) -> list[str]:
    return [rule.name for rule in rules if not rule.ok]


def payload(binary: Path, captures: list[Capture], rules: list[Rule], extra: dict | None = None) -> dict:
    failed = failed_names(rules)
    body = {
        "gate": GATE,
        "probe": PROBE,
        "cr": CR,
        "slug": SLUG,
        "commit": head_commit(),
        "binary": {
            "path": os.path.relpath(binary, ROOT),
            "sha256": file_digest(binary),
        },
        "blocks": BLOCKS,
        "ranks": RANK_ROWS,
        "runs": [
            {
                "run": capture.run,
                "exit": capture.exit,
                "bytes": len(capture.stdout),
                "stdout_sha256": digest(capture.stdout),
                "probe_digest": [
                    match.group(1)
                    for line in lines_of(capture)
                    if (match := DIGEST_LINE.match(line))
                ],
                "ok_rows": len(prefixed(lines_of(capture), "[OK] ")),
                "negative_rows": len(prefixed(lines_of(capture), "NEGATIVE ")),
            }
            for capture in captures
        ],
        "byte_identical": captures[0].stdout == captures[1].stdout,
        "checks": [{"name": rule.name, "ok": rule.ok, "detail": rule.detail} for rule in rules],
        "failed": failed,
        "verdict": "FAIL" if failed else "PASS",
        "exit": 1 if failed else 0,
    }
    if extra:
        body.update(extra)
    return body


def emit(rules: list[Rule], as_json: bool, body: dict) -> int:
    failed = failed_names(rules)
    if as_json:
        print(json.dumps(body, indent=2, sort_keys=True))
    else:
        for capture in body["runs"]:
            print(
                f"run{capture['run']} exit={capture['exit']} "
                f"bytes={capture['bytes']} sha256={capture['stdout_sha256']} "
                f"ok-rows={capture['ok_rows']} "
                f"negative={capture['negative_rows']}"
            )
        for rule in rules:
            print(f"{'OK' if rule.ok else 'FAIL'} {rule.name} {rule.detail}")
        print(
            f"CHECKS {','.join(rule.name for rule in rules)} "
            f"{'all-OK' if not failed else 'failed=' + ','.join(failed)}"
        )
    return 0 if not failed else 1


def run_probe(binary: Path) -> list[Capture]:
    captures: list[Capture] = []
    for run_id in (1, 2):
        proc = run(
            [str(binary), "--probe", "SYS-FACTION-001"],
            env={"PROBE_RUN": str(run_id)},
        )
        captures.append(
            Capture(
                run=run_id,
                exit=proc.returncode,
                stdout=proc.stdout,
                stderr=proc.stderr,
            )
        )
    return captures


def write_artifacts(binary: Path, captures: list[Capture], rules: list[Rule], out: Path,
                    extra: dict | None = None) -> Path:
    out.mkdir(parents=True, exist_ok=True)
    for capture in captures:
        (out / f"run{capture.run}.stdout").write_bytes(capture.stdout)
        (out / f"run{capture.run}.stderr").write_bytes(capture.stderr)
    path = out / "manifest.json"
    path.write_text(json.dumps(payload(binary, captures, rules, extra), indent=2, sort_keys=True) + "\n")
    return path


# ── falsifiers: mutate the capture under its own rule name ─────────────────


def replace_lines(captures: list[Capture], mutate) -> list[Capture]:
    """Apply `mutate(lines)` to both captures (they are byte-identical)."""
    mutated: list[Capture] = []
    for capture in captures:
        lines = lines_of(capture)
        new = mutate(list(lines))
        mutated.append(replace(capture, stdout=("\n".join(new) + "\n").encode()))
    return mutated


def drop_first(lines: list[str], prefix: str) -> tuple[list[str], str]:
    for index, line in enumerate(lines):
        if line.startswith(prefix):
            return lines[:index] + lines[index + 1:], line
    raise AssertionError(f"no line with prefix {prefix}")


def mutate_census_mismatch(captures: list[Capture]) -> tuple[list[Capture], str]:
    def mutate(lines: list[str]) -> list[str]:
        for index, line in enumerate(lines):
            match = CENSUS_LINE.match(line)
            if match:
                t, d, r, lv, dp, un = (int(v) for v in match.groups())
                lines[index] = (
                    f"CENSUS total={t - 1} dup={d} roots={r} leaves={lv} depth={dp} unknown={un}"
                )
                return lines
        raise AssertionError("no CENSUS line")

    return replace_lines(captures, mutate), "CENSUS total 366 -> 365"


def mutate_shape_mismatch(captures: list[Capture]) -> tuple[list[Capture], str]:
    def mutate(lines: list[str]) -> list[str]:
        for index, line in enumerate(lines):
            match = CENSUS_LINE.match(line)
            if match:
                t, d, r, lv, dp, un = (int(v) for v in match.groups())
                lines[index] = (
                    f"CENSUS total={t} dup={d} roots={r - 1} leaves={lv} depth={dp} unknown={un}"
                )
                return lines
        raise AssertionError("no CENSUS line")

    return replace_lines(captures, mutate), "roots 102 -> 101"


def mutate_types_mismatch(captures: list[Capture]) -> tuple[list[Capture], str]:
    def mutate(lines: list[str]) -> list[str]:
        for index, line in enumerate(lines):
            if line.startswith("TYPES "):
                lines[index] = line.replace("t[7]=46", "t[7]=45", 1)
                return lines
        raise AssertionError("no TYPES line")

    return replace_lines(captures, mutate), "t[7]=46 -> 45"


def mutate_rank_mismatch(captures: list[Capture]) -> tuple[list[Capture], str]:
    def mutate(lines: list[str]) -> list[str]:
        for index, line in enumerate(lines):
            if line.startswith("RANKS "):
                lines[index] = line.replace("row9=(90,87,37)", "row9=(91,87,37)", 1)
                return lines
        raise AssertionError("no RANKS line")

    return replace_lines(captures, mutate), "row9 90 -> 91"


def mutate_promotion_mismatch(captures: list[Capture]) -> tuple[list[Capture], str]:
    def mutate(lines: list[str]) -> list[str]:
        for index, line in enumerate(lines):
            if line.startswith("PROMOTION "):
                lines[index] = line.replace("rank=3 services=5->6", "rank=4 services=5->6", 1)
                return lines
        raise AssertionError("no PROMOTION line")

    return replace_lines(captures, mutate), "promotion rank 3 -> 4"


def mutate_ledger_mismatch(captures: list[Capture]) -> tuple[list[Capture], str]:
    def mutate(lines: list[str]) -> list[str]:
        for index, line in enumerate(lines):
            match = LEDGER_LINE.match(line)
            if match:
                fid, amt, prop, _d, d0, d1 = match.groups()
                lines[index] = (
                    f"LEDGER v1 id={fid} amount={amt} propagate={prop} deltas=25 digest={d0}->{d1}"
                )
                return lines
        raise AssertionError("no LEDGER line")

    return replace_lines(captures, mutate), "LEDGER deltas 26 -> 25"


def mutate_digest_tamper(captures: list[Capture]) -> tuple[list[Capture], str]:
    def mutate(lines: list[str]) -> list[str]:
        for index, line in enumerate(lines):
            if DIGEST_LINE.match(line):
                lines[index] = "DIGEST " + "0" * 64
                return lines
        raise AssertionError("no DIGEST line")

    return replace_lines(captures, mutate), "DIGEST -> 64 zeros"


FALSIFIERS = {
    "census-mismatch": ("disk-census", mutate_census_mismatch),
    "shape-mismatch": ("census-shape", mutate_shape_mismatch),
    "types-mismatch": ("census-types", mutate_types_mismatch),
    "rank-mismatch": ("rank-table", mutate_rank_mismatch),
    "promotion-mismatch": ("promotion-path", mutate_promotion_mismatch),
    "ledger-mismatch": ("ledger-versioned", mutate_ledger_mismatch),
    "digest-tamper": ("digest-integrity", mutate_digest_tamper),
}


def falsify(mode: str, binary: Path, out: Path, captures: list[Capture],
            as_json: bool) -> int:
    target, mutate = FALSIFIERS[mode]
    mutated, note = mutate(captures)
    if (out / f"falsify-{mode}").exists():
        shutil.rmtree(out / f"falsify-{mode}")
    rules = check(mutated)
    failed = failed_names(rules)
    red = target in failed
    restored = check(captures)
    clean_again = not failed_names(restored)
    write_artifacts(
        binary,
        mutated,
        rules,
        out / f"falsify-{mode}",
        {
            "falsify": {
                "mode": mode,
                "target": target,
                "mutation": note,
                "red": red,
                "failed": failed,
                "restored-clean": clean_again,
            }
        },
    )
    if as_json:
        print(json.dumps({
            "mode": mode,
            "target": target,
            "mutation": note,
            "failed": failed,
            "red": red,
            "restored_clean": clean_again,
        }))
    else:
        print(f"FALSIFY {mode}: mutation={note}")
        print(f"FALSIFY {mode}: failed={','.join(failed) or 'none'}")
        print(f"FALSIFY {mode}: rule {target} {'RED' if red else 'STAYED GREEN'}")
        print(
            f"FALSIFY {mode}: clean captures re-check="
            f"{'PASS' if clean_again else 'FAIL'} (mutation restored)"
        )
    return 0 if red and clean_again else 1


def main() -> int:
    parser = argparse.ArgumentParser(
        description=__doc__,
        formatter_class=argparse.RawDescriptionHelpFormatter,
    )
    parser.add_argument("--binary", default=str(BINARY), help="prebuilt daggerfall-app")
    parser.add_argument("--out", default=str(ARTIFACTS), help="archive root")
    parser.add_argument("--skip-build", action="store_true", help="do not run cargo build")
    parser.add_argument("--json", action="store_true", help="emit one JSON object")
    parser.add_argument("--verbose", action="store_true", help="echo run1 stdout first")
    parser.add_argument(
        "--falsify",
        choices=sorted(FALSIFIERS),
        help="mutate the capture and require the named rule to go red",
    )
    args = parser.parse_args()

    binary = Path(args.binary)
    if not args.skip_build:
        build = run(["cargo", "build", "-p", "daggerfall-app"])
        if build.returncode != 0:
            sys.stderr.write(build.stderr.decode("utf-8", "replace")[-4000:])
            print("VERDICT: FAIL - build")
            return 2
    if not binary.is_file():
        print(f"VERDICT: FAIL - missing binary {binary}")
        return 2

    # A falsifier archives under its own subdirectory: it must never destroy
    # the clean run it was derived from.
    out = Path(args.out)
    if args.falsify:
        if not out.exists():
            out.mkdir(parents=True, exist_ok=True)
    else:
        if out.exists():
            shutil.rmtree(out)
        out.mkdir(parents=True, exist_ok=True)

    captures = run_probe(binary)
    rules = check(captures)

    if args.falsify:
        return falsify(args.falsify, binary, out, captures, args.json)

    write_artifacts(binary, captures, rules, out)
    if args.verbose:
        sys.stdout.write(captures[0].stdout.decode("utf-8", "replace"))
    print(f"GATE {PROBE} processes=2 stdout={len(captures[0].stdout)}B "
          f"sha256={digest(captures[0].stdout)}")
    print(f"GATE archive={os.path.relpath(out, ROOT)}")
    code = emit(rules, args.json, payload(binary, captures, rules))
    if code == 0:
        print("VERDICT: PASS")
    else:
        print("VERDICT: FAIL - process-contract")
    return code


if __name__ == "__main__":
    sys.exit(main())
