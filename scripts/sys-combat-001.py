#!/usr/bin/env python3
"""SYS-COMBAT-001 - gate of M41/W41-2: complete combat (roadmap §3.6).

Two separate processes, one binary, no retail I/O
--------------------------------------------------
1. `cargo build -p daggerfall-app` once (`--skip-build` reuses the binary).
2. `--probe SYS-COMBAT-001` runs in **two independent processes**
   (`PROBE_RUN=1` and `PROBE_RUN=2`; the probe never prints that value, so an
   environment dependence shows up as a stdout difference).
3. Both captures must exit 0 and contain `VEREDICTO: PASS`, must be
   **byte-identical** (`cmp` + equal `sha256`), and the probe's own `DIGEST`
   line must be the `sha256` of every byte printed before it.

Structural checks (one rule each, all pure over the captures)
-------------------------------------------------------------
`blocks-in-order`  A..J each exactly once, in that order, non-empty, declared
                   `rows=` equal to the measured row count of the block.
`row-census`       `CENSUS rows=N ok=A fail=B blocks=10` consistent with the
                   measured rows, with the last line `VEREDICTO: PASS`.
`row-ids-unique`   no probe row id appears twice (a duplicated row is a hole).
`rows-cited`       every row carries `(src=<path>:<line>)` or
                   `(spec=<path>:<line>)` - zero uncited formula.
`src-paths-exist`  every cited path exists in the repo and the cited line is
                   within the file (a fabricated citation fails here).
`matrix-complete`  the declared domains `MATRIX skills=a armour=b materials=c
                   swings=d rows=n` are ≥3 each, `n == a·b·c·d`, and the
                   printed combinations are exactly the cartesian product,
                   each exactly once, every line cited, `hit=1 => damage>=1`,
                   `hit=0 => damage=0`.
`combat60-repeat`  `COMBAT60` and `COMBAT60-REPEAT identical` carry the same
                   sha256 (the two inner runs of the 60 s simulation).
`adversarial-present` block J is the adversarial block, machine-visible: >=2
                   rows, **every** row id carries the `ADV/` label, and the
                   refusal (`ADV/J-GATE-NO-ROLL`) plus both clamps
                   (`ADV/J-CLAMP-HIGH/LOW`) are present with an adversarial
                   claim in their text.
`death-latch-once` one death event per actor: `G-DEATH-PLAYER`,
                   `G-LATCH-SECOND`, `G-RAISE-LATCH` and `J-DEATH-LATCH` all
                   report `events=1` and a refused re-fire.
`no-approximate`   no "approximate"/"estimated"/"roughly"/"~" wording.
`no-probe-run`     the runner-only `PROBE_RUN` variable never reaches stdout.
`header-contract`  the first line is `PROBE-SYS-COMBAT-001 - <summary> (M41/W41-2, <CR>)`.

This gate needs **no retail data**: every golden calls the rule with synthetic
items (`ItemTemplate::empty` + `apply_weapon_material`), so exit 2 is reserved
for usage/IO (build failure, missing binary), never for a missing ARENA2.

Falsifiers (each mutates a TEMP COPY of the captures under its own name,
requires the named rule red, then re-checks the untouched captures green):
`header-drift`, `matrix-gap`, `matrix-duplicate`, `citation-empty`,
`adversarial-unlabelled`,
`combat60-diverges`, `death-latch-twice`, `digest-tamper`, `approx-wording`.

Exit codes: 0 PASS, 1 FAIL, 2 usage/IO.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import subprocess
import sys
from dataclasses import dataclass, replace
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
GATE = "SYS-COMBAT-001"
PROBE = "PROBE-SYS-COMBAT-001"
SLUG = "sys-combat-001"
BINARY = ROOT / "target" / "debug" / "daggerfall-app"
ARTIFACTS = ROOT / ".sintergic" / "artifacts" / "m41-sys" / SLUG
CR = "CR-da9915b0-e700-471a-80b8-0bafa9459938"
WAVE = "M41/W41-2"
CRATE = "daggerfall-app"
BLOCKS = ("A", "B", "C", "D", "E", "F", "G", "H", "I", "J")
EMPTY_SHA256 = hashlib.sha256(b"").hexdigest()

BLOCK_LINE = re.compile(r"^BLOCK (\S) rows=(\d+)$")
ROW_LINE = re.compile(r"^(?:\[OK\]|FAIL) (\S+) - .*$")
FAIL_LINE = re.compile(r"^FAIL (\S+) got=(.*?) want=(.*)$")
CITED = re.compile(r"\((src|spec)=([^():]+):(\d+)\)$")
CENSUS_LINE = re.compile(r"^CENSUS rows=(\d+) ok=(\d+) fail=(\d+) blocks=(\d+)$")
VERDICT_LINE = re.compile(r"^VEREDICTO: (PASS|FAIL|SKIP.*)$")
MATRIX_HEAD = re.compile(
    r"^MATRIX skills=(\d+) armour=(\d+) materials=(\d+) swings=(\d+) rows=(\d+)$"
)
MATRIX_ROW = re.compile(
    r"^MATRIX (-?\d+) (-?\d+) (-?\d+) (-?\d+) hit=([01]) damage=(\d+) src=(\S+):(\d+)$"
)
COMBAT60 = re.compile(r"^COMBAT60 events=(\d+) damage=(\d+) deaths=(\d+) digest=([0-9a-f]{64})$")
COMBAT60_REPEAT = re.compile(r"^COMBAT60-REPEAT (identical|differs) digest=([0-9a-f]{64})$")
DIGEST_LINE = re.compile(r"^DIGEST ([0-9a-f]{64})$")
HEADER = re.compile(r"^(PROBE-SYS-COMBAT-001) - .+ \((M41/W41-\d), (CR-[0-9a-f-]{36})\)$")
APPROX = re.compile(r"approximat|estimated|roughly|~", re.IGNORECASE)
ADV = "ADV/"
ADV_BLOCK = "J"
ADV_REQUIRED = ("ADV/J-CLAMP-HIGH", "ADV/J-CLAMP-LOW", "ADV/J-GATE-NO-ROLL")
ADV_MARKERS = ("clamp", "refus", "never", "absurd")
LATCH_FACTS: tuple[tuple[str, tuple[str, ...]], ...] = (
    ("G-DEATH-PLAYER", ("events=1",)),
    ("G-LATCH-SECOND", ("events=1", "refire=false")),
    ("G-RAISE-LATCH", ("second=false", "events=1")),
    ("ADV/J-DEATH-LATCH", ("events=1", "refire=false")),
)


# ── Process helpers ────────────────────────────────────────────────────────


def run(cmd: list[str], env: dict[str, str] | None = None) -> subprocess.CompletedProcess:
    return subprocess.run(
        cmd,
        cwd=ROOT,
        env={**os.environ, **(env or {})},
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
    )


def sha(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def head_commit() -> str | None:
    proc = run(["git", "rev-parse", "HEAD"])
    if proc.returncode != 0:
        return None
    return proc.stdout.decode("utf-8", "replace").strip() or None


def build(skip: bool) -> bool:
    if skip:
        return True
    proc = run(["cargo", "build", "-p", CRATE])
    if proc.returncode == 0:
        return True
    sys.stderr.write(proc.stderr.decode("utf-8", "replace")[-4000:])
    return False


@dataclass(frozen=True)
class Capture:
    run: int
    exit: int
    stdout: bytes
    stderr: bytes = b""


@dataclass(frozen=True)
class Rule:
    name: str
    ok: bool
    detail: str


def capture_run(binary: Path, run_id: int) -> Capture:
    proc = run([str(binary), "--probe", GATE], {"PROBE_RUN": str(run_id)})
    return Capture(run=run_id, exit=proc.returncode, stdout=proc.stdout, stderr=proc.stderr)


def capture_all(binary: Path) -> list[Capture]:
    return [capture_run(binary, 1), capture_run(binary, 2)]


def lines_of(capture: Capture) -> list[str]:
    return capture.stdout.decode("utf-8", "replace").splitlines()


def verdict_of(capture: Capture) -> str | None:
    found = [line for line in lines_of(capture) if line.startswith("VEREDICTO")]
    return found[-1] if found else None


def digest_of(capture: Capture) -> str | None:
    found = [match.group(1) for line in lines_of(capture) if (match := DIGEST_LINE.match(line))]
    return found[0] if len(found) == 1 else None


PRE_ROWS: list[str] = []


def block_rows(lines: list[str]) -> tuple[list[re.Match[str]], dict[str, list[str]]]:
    """Block headers in order + the `[OK]`/`FAIL` rows that follow each one."""
    headers: list[re.Match[str]] = []
    rows: dict[str, list[str]] = {}
    current: str | None = None
    PRE_ROWS.clear()
    for line in lines:
        header = BLOCK_LINE.match(line)
        if header:
            headers.append(header)
            current = header.group(1)
            rows.setdefault(current, [])
            continue
        if ROW_LINE.match(line):
            if current is None:
                PRE_ROWS.append(line)
            else:
                rows[current].append(line)
    return headers, rows


def all_rows(lines: list[str]) -> list[str]:
    return [line for line in lines if ROW_LINE.match(line)]


def row_id(line: str) -> str:
    match = ROW_LINE.match(line)
    assert match is not None
    return match.group(1)


# ── Pure checks: no I/O, only the captures ─────────────────────────────────


def rule_probe_verdict(captures: list[Capture]) -> Rule:
    problems: list[str] = []
    evidence: list[str] = []
    for capture in captures:
        verdict = verdict_of(capture) or "absent"
        evidence.append(f"run{capture.run}={verdict.split(' ')[0]} exit={capture.exit}")
        if verdict != "VEREDICTO: PASS" or capture.exit != 0:
            problems.append(f"run{capture.run}:{verdict}")
    if problems:
        return Rule("probe-verdict", False, " ".join(problems))
    return Rule("probe-verdict", True, " ".join(evidence))


def rule_stdout_byte_identical(captures: list[Capture]) -> Rule:
    first, second = captures
    if first.stdout != second.stdout:
        return Rule(
            "stdout-byte-identical",
            False,
            f"run1={len(first.stdout)}B run2={len(second.stdout)}B mismatch",
        )
    return Rule(
        "stdout-byte-identical",
        True,
        f"bytes={len(first.stdout)} sha256={sha(first.stdout)} x2 identical",
    )


def rule_digest_valid(captures: list[Capture]) -> Rule:
    problems: list[str] = []
    detail: list[str] = []
    for capture in captures:
        value = digest_of(capture)
        if value is None:
            problems.append(f"run{capture.run}:no-single-DIGEST")
            continue
        index = next(i for i, line in enumerate(lines_of(capture)) if line.startswith("DIGEST "))
        # the probe digests `body` = join(lines) + "\n", i.e. every byte printed
        # before the DIGEST line
        expected = sha(("\n".join(lines_of(capture)[:index]) + "\n").encode())
        if expected != value:
            problems.append(f"run{capture.run}:digest={value}!={expected}")
        if value == EMPTY_SHA256:
            problems.append(f"run{capture.run}:empty-sha")
        detail.append(f"run{capture.run}={value[:16]}…")
    if problems:
        return Rule("digest-valid", False, " ".join(problems))
    return Rule("digest-valid", True, f"prefix-sha256 verified x2 {' '.join(detail)}")


def rule_blocks_in_order(captures: list[Capture]) -> Rule:
    lines = lines_of(captures[0])
    headers, rows = block_rows(lines)
    problems: list[str] = []
    seq = [match.group(1) for match in headers]
    if seq != list(BLOCKS):
        problems.append(f"order={''.join(seq) or 'none'}")
    if PRE_ROWS:
        problems.append(f"rows-before-first-block={len(PRE_ROWS)}")
    for match in headers:
        label, declared = match.group(1), int(match.group(2))
        measured = len(rows.get(label, []))
        if declared != measured:
            problems.append(f"{label}:declared={declared} measured={measured}")
        if measured == 0:
            problems.append(f"{label}:empty")
    if problems:
        return Rule("blocks-in-order", False, " ".join(problems[:8]))
    return Rule(
        "blocks-in-order",
        True,
        f"blocks={len(seq)} rows={sum(len(v) for v in rows.values())} order={'..'.join(BLOCKS)}",
    )


def rule_row_census(captures: list[Capture]) -> Rule:
    lines = lines_of(captures[0])
    census = [match for line in lines if (match := CENSUS_LINE.match(line))]
    rows = all_rows(lines)
    fails = [line for line in lines if FAIL_LINE.match(line)]
    problems: list[str] = []
    if len(census) != 1:
        problems.append(f"census-lines={len(census)}")
        return Rule("row-census", False, " ".join(problems))
    total, ok, fail, blocks = (int(census[0].group(i)) for i in (1, 2, 3, 4))
    if total != len(rows):
        problems.append(f"rows {total}!={len(rows)}")
    if ok + fail != total:
        problems.append(f"ok+fail {ok}+{fail}!={total}")
    if ok != len(rows) - len(fails):
        problems.append(f"ok {ok}!={len(rows) - len(fails)}")
    if fail != len(fails) or fail != 0:
        problems.append(f"fail={fail}/{len(fails)}")
    if blocks != len(BLOCKS):
        problems.append(f"blocks={blocks}")
    if problems:
        return Rule("row-census", False, " ".join(problems))
    return Rule("row-census", True, f"rows={total} ok={ok} fail=0 blocks={blocks}")


def rule_row_ids_unique(captures: list[Capture]) -> Rule:
    ids = [row_id(line) for line in all_rows(lines_of(captures[0]))]
    seen: set[str] = set()
    duplicated: list[str] = []
    for name in ids:
        if name in seen and name not in duplicated:
            duplicated.append(name)
        seen.add(name)
    if duplicated:
        return Rule("row-ids-unique", False, f"duplicated={','.join(duplicated[:6])}")
    return Rule("row-ids-unique", True, f"ids={len(ids)} distinct={len(seen)}")


def rule_rows_cited(captures: list[Capture]) -> Rule:
    lines = lines_of(captures[0])
    uncited: list[str] = []
    cited = 0
    for line in all_rows(lines):
        if CITED.search(line):
            cited += 1
        else:
            uncited.append(row_id(line))
    matrix_rows = [line for line in lines if line.startswith("MATRIX ") and not MATRIX_HEAD.match(line)]
    for line in matrix_rows:
        if not MATRIX_ROW.match(line):
            uncited.append(line[:40])
    if uncited:
        return Rule("rows-cited", False, f"uncited={','.join(uncited[:8])} ({len(uncited)})")
    return Rule(
        "rows-cited",
        True,
        f"cited={cited} matrix-cited={len(matrix_rows)} uncited=0",
    )


def rule_src_paths_exist(captures: list[Capture]) -> Rule:
    lines = lines_of(captures[0])
    refs = [(m.group(2), int(m.group(3))) for line in lines if (m := CITED.search(line))]
    problems: list[str] = []
    for path, number in sorted(set(refs)):
        target = ROOT / path
        if not target.is_file():
            problems.append(f"missing:{path}")
            continue
        total = len(target.read_text(encoding="utf-8", errors="replace").splitlines())
        if number < 1 or number > total:
            problems.append(f"line:{path}:{number}>{total}")
    if problems:
        return Rule("src-paths-exist", False, " ".join(problems[:8]))
    files = len({path for path, _ in refs})
    return Rule("src-paths-exist", True, f"citations={len(refs)} files={files} all-in-range")


def rule_matrix_complete(captures: list[Capture]) -> Rule:
    lines = lines_of(captures[0])
    heads = [match for line in lines if (match := MATRIX_HEAD.match(line))]
    rows = [match for line in lines if (match := MATRIX_ROW.match(line))]
    problems: list[str] = []
    if len(heads) != 1:
        return Rule("matrix-complete", False, f"heads={len(heads)}")
    head = heads[0]
    domains = [int(head.group(i)) for i in (1, 2, 3, 4)]
    declared = int(head.group(5))
    if min(domains) < 3:
        problems.append(f"domains={domains}")
    product = domains[0] * domains[1] * domains[2] * domains[3]
    if declared != product:
        problems.append(f"declared={declared} product={product}")
    if len(rows) != declared:
        problems.append(f"printed={len(rows)} declared={declared}")
    combos = [
        (int(m.group(1)), int(m.group(2)), int(m.group(3)), int(m.group(4))) for m in rows
    ]
    if len(set(combos)) != len(combos):
        problems.append(f"duplicated={len(combos) - len(set(combos))}")
    axes = [
        sorted({combo[i] for combo in combos}) for i in range(4)
    ]
    expected = {
        (s, a, m, w) for s in axes[0] for a in axes[1] for m in axes[2] for w in axes[3]
    }
    missing = expected - set(combos)
    if missing:
        problems.append(f"gap={len(missing)}")
    if any(len(axis) != domains[i] for i, axis in enumerate(axes)):
        problems.append(f"axis-sizes={[len(a) for a in axes]}")
    for match in rows:
        hit, damage = int(match.group(5)), int(match.group(6))
        if hit == 1 and damage < 1:
            problems.append(f"hit-with-0:{match.group(1)},{match.group(2)}")
        if hit == 0 and damage != 0:
            problems.append(f"miss-with-damage:{match.group(1)},{match.group(2)}")
    if problems:
        return Rule("matrix-complete", False, " ".join(problems[:8]))
    return Rule(
        "matrix-complete",
        True,
        f"skills={domains[0]} armour={domains[1]} materials={domains[2]} swings={domains[3]} "
        f"rows={len(rows)} product={product} unique={len(set(combos))} gaps=0",
    )


def rule_combat60_repeat(captures: list[Capture]) -> Rule:
    lines = lines_of(captures[0])
    runs = [match for line in lines if (match := COMBAT60.match(line))]
    repeats = [match for line in lines if (match := COMBAT60_REPEAT.match(line))]
    problems: list[str] = []
    if len(runs) != 1:
        problems.append(f"COMBAT60={len(runs)}")
    if len(repeats) != 1:
        problems.append(f"COMBAT60-REPEAT={len(repeats)}")
    if not problems:
        word, digest = repeats[0].group(1), repeats[0].group(2)
        if word != "identical":
            problems.append(f"repeat={word}")
        if digest != runs[0].group(4):
            problems.append(f"digest {digest[:12]}!={runs[0].group(4)[:12]}")
        if int(runs[0].group(1)) == 0:
            problems.append("events=0")
    if not any(row_id(line) == "I-REPEAT" for line in all_rows(lines)):
        problems.append("no-I-REPEAT-row")
    if problems:
        return Rule("combat60-repeat", False, " ".join(problems[:6]))
    return Rule(
        "combat60-repeat",
        True,
        f"events={runs[0].group(1)} damage={runs[0].group(2)} deaths={runs[0].group(3)} "
        f"digest={runs[0].group(4)[:16]}… identical",
    )


def rule_death_latch_once(captures: list[Capture]) -> Rule:
    lines = all_rows(lines_of(captures[0]))
    problems: list[str] = []
    seen: list[str] = []
    for name, facts in LATCH_FACTS:
        matching = [line for line in lines if row_id(line) == name]
        if len(matching) != 1:
            problems.append(f"{name}:rows={len(matching)}")
            continue
        for fact in facts:
            if fact not in matching[0]:
                problems.append(f"{name}:{fact}")
        seen.append(name)
    if problems:
        return Rule("death-latch-once", False, " ".join(problems[:8]))
    return Rule("death-latch-once", True, f"actors={len(seen)} events=1 refire=false x{len(seen)}")


def rule_no_approximate(captures: list[Capture]) -> Rule:
    hits = [line[:60] for line in lines_of(captures[0]) if APPROX.search(line)]
    if hits:
        return Rule("no-approximate", False, f"hits={len(hits)} first={hits[0]!r}")
    return Rule("no-approximate", True, "0 approximate/estimated/roughly rows")


def rule_no_probe_run(captures: list[Capture]) -> Rule:
    leaks = [capture.run for capture in captures if b"PROBE_RUN" in capture.stdout]
    if leaks:
        return Rule("no-probe-run", False, f"runs={leaks}")
    return Rule("no-probe-run", True, "runner-only variable absent from stdout")


def rule_adversarial_present(captures: list[Capture]) -> Rule:
    """Block J is the adversarial block and says so: >=2 rows, every row id
    carries the `ADV/` label, and the refusal + both clamp cases are present."""
    lines = lines_of(captures[0])
    _, rows = block_rows(lines)
    labels = rows.get(ADV_BLOCK, [])
    problems: list[str] = []
    if len(labels) < 2:
        problems.append(f"adversarial-rows={len(labels)}")
    ids = [row_id(line) for line in labels]
    unlabelled = [name for name in ids if not name.startswith(ADV)]
    if unlabelled:
        problems.append(f"unlabelled={','.join(unlabelled[:6])}")
    missing = [name for name in ADV_REQUIRED if name not in ids]
    if missing:
        problems.append(f"missing={','.join(missing)}")
    for line in labels:
        if not any(marker in line.lower() for marker in ADV_MARKERS):
            problems.append(f"no-adversarial-claim:{row_id(line)}")
    if problems:
        return Rule("adversarial-present", False, " ".join(problems[:8]))
    clamp = [name for name in ids if "CLAMP" in name]
    refusals = [name for name in ids if "GATE" in name or "LATCH" in name]
    return Rule(
        "adversarial-present",
        True,
        f"block={ADV_BLOCK} rows={len(ids)} labelled={len(ids)} clamps={len(clamp)} "
        f"refusals={len(refusals)}",
    )


def rule_header_contract(captures: list[Capture]) -> Rule:
    """First line is exactly `<PROBE> - <summary> (M41/W41-2, <CR>)`."""
    first = lines_of(captures[0])[0] if lines_of(captures[0]) else ""
    suffix = f"(M41/W41-2, {CR})"
    if not first.startswith(f"{PROBE} - ") or not first.endswith(suffix):
        return Rule("header-contract", False, f"first-line={first[-60:]!r} want-suffix={suffix}")
    match = HEADER.match(first)
    if match is None:
        return Rule("header-contract", False, f"first-line-shape={first[:60]!r}")
    return Rule("header-contract", True, f"probe={match.group(1)} wave={WAVE} cr={CR}")


RULES = (
    rule_probe_verdict,
    rule_header_contract,
    rule_stdout_byte_identical,
    rule_digest_valid,
    rule_blocks_in_order,
    rule_row_census,
    rule_row_ids_unique,
    rule_rows_cited,
    rule_src_paths_exist,
    rule_matrix_complete,
    rule_combat60_repeat,
    rule_death_latch_once,
    rule_adversarial_present,
    rule_no_approximate,
    rule_no_probe_run,
)


def check(captures: list[Capture]) -> list[Rule]:
    """The fifteen rules over two captures. Pure: no disk, no clock, no build."""
    return [rule(captures) for rule in RULES]


# ── Reporting ──────────────────────────────────────────────────────────────


def failed_names(rules: list[Rule]) -> list[str]:
    return [rule.name for rule in rules if not rule.ok]


def payload(binary: Path, captures: list[Capture], rules: list[Rule], extra: dict | None = None) -> dict:
    lines = lines_of(captures[0])
    headers, rows = block_rows(lines)
    matrix = [match for line in lines if (match := MATRIX_ROW.match(line))]
    body = {
        "schema": "SYS-COMBAT-001/1",
        "gate": GATE,
        "cr": CR,
        "probe": PROBE,
        "head": head_commit(),
        "binary": str(binary),
        "binary_sha256": sha(binary.read_bytes()) if binary.is_file() else None,
        "rows": len(all_rows(lines)),
        "blocks": {match.group(1): int(match.group(2)) for match in headers},
        "rows_per_block": {label: len(value) for label, value in rows.items()},
        "matrix_rows": len(matrix),
        "adversarial_rows": len(rows.get(ADV_BLOCK, [])),
        "probe_digest": digest_of(captures[0]),
        "stdout": [
            {"run": capture.run, "exit": capture.exit, "bytes": len(capture.stdout), "sha256": sha(capture.stdout)}
            for capture in captures
        ],
        "rules": [{"name": rule.name, "ok": rule.ok, "detail": rule.detail} for rule in rules],
        "failed": failed_names(rules),
    }
    if extra:
        body.update(extra)
    return body


def emit(binary: Path, captures: list[Capture], rules: list[Rule], as_json: bool, extra: dict | None = None) -> int:
    failed = failed_names(rules)
    body = payload(binary, captures, rules, extra)
    if as_json:
        print(json.dumps(body, indent=2, sort_keys=True))
    else:
        print(
            f"gate {GATE} probe={PROBE} cr={CR} rows={body['rows']} "
            f"matrix={body['matrix_rows']} adversarial={body['adversarial_rows']}"
        )
        for capture in captures:
            print(
                f"run{capture.run} exit={capture.exit} sha256={sha(capture.stdout)} "
                f"bytes={len(capture.stdout)}"
            )
        for rule in rules:
            print(f"{'OK' if rule.ok else 'FAIL'} {rule.name} {rule.detail}")
        print(
            f"CHECKS {','.join(rule.name for rule in rules)} "
            f"{'all-OK' if not failed else 'failed=' + ','.join(failed)}"
        )
    return 0 if not failed else 1


def write_manifest(binary: Path, captures: list[Capture], rules: list[Rule], out: Path, extra: dict | None = None) -> Path:
    out.mkdir(parents=True, exist_ok=True)
    path = out / "manifest.json"
    path.write_text(json.dumps(payload(binary, captures, rules, extra), indent=2, sort_keys=True) + "\n")
    for capture in captures:
        (out / f"run{capture.run}.stdout").write_bytes(capture.stdout)
        (out / f"run{capture.run}.stderr").write_bytes(capture.stderr)
    return path


# ── Falsifiers: mutate a TEMP COPY of the captures under their own name ────


def rewrite(captures: list[Capture], index: int, line: str) -> list[Capture]:
    """Replace line `index` in every capture (they are byte-identical)."""
    return [
        replace(
            capture,
            stdout=(
                "\n".join(lines_of(capture)[:index] + [line] + lines_of(capture)[index + 1:]) + "\n"
            ).encode(),
        )
        for capture in captures
    ]


def drop(captures: list[Capture], index: int) -> list[Capture]:
    return [
        replace(
            capture,
            stdout=("\n".join(lines_of(capture)[:index] + lines_of(capture)[index + 1:]) + "\n").encode(),
        )
        for capture in captures
    ]


def index_of(lines: list[str], prefix: str) -> int:
    return next(i for i, line in enumerate(lines) if line.startswith(prefix))


def mutate_matrix_gap(captures: list[Capture]) -> tuple[list[Capture], str]:
    lines = lines_of(captures[0])
    index = index_of(lines, "MATRIX 80 -45 9 10 ")
    return drop(captures, index), f"removed {lines[index]}"


def mutate_matrix_duplicate(captures: list[Capture]) -> tuple[list[Capture], str]:
    lines = lines_of(captures[0])
    first = index_of(lines, "MATRIX 15 0 0 0 ")
    last = index_of(lines, "MATRIX 80 -45 9 10 ")
    return rewrite(captures, last, lines[first]), f"last combo replaced by {lines[first]}"


def mutate_citation_empty(captures: list[Capture]) -> tuple[list[Capture], str]:
    lines = lines_of(captures[0])
    index = index_of(lines, "[OK] B-PART-0 ")
    stripped = re.sub(r" \(src=[^)]+\)$", "", lines[index])
    return rewrite(captures, index, stripped), f"stripped citation of {row_id(lines[index])}"


def mutate_combat60_diverges(captures: list[Capture]) -> tuple[list[Capture], str]:
    lines = lines_of(captures[0])
    index = index_of(lines, "COMBAT60-REPEAT ")
    match = COMBAT60_REPEAT.match(lines[index])
    assert match is not None
    return rewrite(captures, index, f"COMBAT60-REPEAT differs digest={match.group(2)}"), "identical -> differs"


def mutate_death_latch_twice(captures: list[Capture]) -> tuple[list[Capture], str]:
    lines = lines_of(captures[0])
    index = index_of(lines, "[OK] G-LATCH-SECOND ")
    mutated = lines[index].replace("events=1 refire=false", "events=2 refire=true", 1)
    return rewrite(captures, index, mutated), "G-LATCH-SECOND events=1 -> events=2"


def mutate_digest_tamper(captures: list[Capture]) -> tuple[list[Capture], str]:
    lines = lines_of(captures[0])
    index = index_of(lines, "DIGEST ")
    value = DIGEST_LINE.match(lines[index])
    assert value is not None
    flipped = ("0" if value.group(1)[0] != "0" else "1") + value.group(1)[1:]
    return rewrite(captures, index, f"DIGEST {flipped}"), "DIGEST first hex nibble flipped"


def mutate_approx_wording(captures: list[Capture]) -> tuple[list[Capture], str]:
    lines = lines_of(captures[0])
    index = index_of(lines, "[OK] A-R00 ")
    injected = lines[index].replace(" - ", " - approximately ", 1)
    return rewrite(captures, index, injected), "injected 'approximately' into A-R00"


def mutate_header_drift(captures: list[Capture]) -> tuple[list[Capture], str]:
    lines = lines_of(captures[0])
    header = HEADER.match(lines[0])
    assert header is not None
    drifted = lines[0].replace(header.group(2), "M41/W41-9", 1)
    return rewrite(captures, 0, drifted), "wave M41/W41-2 -> M41/W41-9"


def mutate_adversarial_unlabelled(captures: list[Capture]) -> tuple[list[Capture], str]:
    lines = lines_of(captures[0])
    index = index_of(lines, f"[OK] {ADV}J-CLAMP-LOW ")
    stripped = lines[index].replace(f"{ADV}J-CLAMP-LOW", "J-CLAMP-LOW", 1)
    return rewrite(captures, index, stripped), "ADV/J-CLAMP-LOW -> J-CLAMP-LOW (label removed)"


FALSIFIERS = {
    "header-drift": ("header-contract", mutate_header_drift),
    "adversarial-unlabelled": ("adversarial-present", mutate_adversarial_unlabelled),
    "matrix-gap": ("matrix-complete", mutate_matrix_gap),
    "matrix-duplicate": ("matrix-complete", mutate_matrix_duplicate),
    "citation-empty": ("rows-cited", mutate_citation_empty),
    "combat60-diverges": ("combat60-repeat", mutate_combat60_diverges),
    "death-latch-twice": ("death-latch-once", mutate_death_latch_twice),
    "digest-tamper": ("digest-valid", mutate_digest_tamper),
    "approx-wording": ("no-approximate", mutate_approx_wording),
}


def falsify(mode: str, binary: Path, out: Path, captures: list[Capture], as_json: bool) -> int:
    """Apply one mutation, require the named rule red, then re-check the clean copies."""
    target, mutate = FALSIFIERS[mode]
    mutated, note = mutate(captures)
    rules = check(mutated)
    failed = failed_names(rules)
    red = target in failed
    restored = check(captures)
    clean_again = not failed_names(restored)
    write_manifest(
        binary,
        mutated,
        rules,
        out / f"falsify-{mode}",
        {"falsify": {"mode": mode, "target": target, "mutation": note, "red": red, "restored-clean": clean_again}},
    )
    if as_json:
        print(json.dumps({"mode": mode, "target": target, "failed": failed, "red": red, "restored_clean": clean_again}))
    else:
        print(f"FALSIFY {mode}: mutation={note}")
        print(f"FALSIFY {mode}: failed={','.join(failed) or 'none'}")
        print(f"FALSIFY {mode}: rule {target} {'red' if red else 'STAYED GREEN'}")
        print(f"FALSIFY {mode}: clean captures re-check={'PASS' if clean_again else 'FAIL'} (mutation restored)")
    return 0 if red and clean_again else 1


# ── main ───────────────────────────────────────────────────────────────────


def main() -> int:
    parser = argparse.ArgumentParser(
        description=__doc__,
        formatter_class=argparse.RawDescriptionHelpFormatter,
    )
    parser.add_argument("--binary", default=str(BINARY), help="prebuilt daggerfall-app")
    parser.add_argument("--out", default=str(ARTIFACTS), help="archive root (manifest.json + captures)")
    parser.add_argument("--skip-build", action="store_true", help="do not run cargo build")
    parser.add_argument("--json", action="store_true", help="emit one JSON object instead of the table")
    parser.add_argument("--verbose", action="store_true", help="echo the run1 probe stdout first")
    parser.add_argument("--falsify", choices=sorted(FALSIFIERS), help="mutate the capture and require a red gate")
    args = parser.parse_args()

    binary = Path(args.binary)
    out = Path(args.out)
    if not build(args.skip_build):
        print("VERDICT: FAIL - build")
        return 2
    if not binary.is_file():
        print(f"VERDICT: FAIL - missing binary {binary}")
        return 2

    captures = capture_all(binary)
    rules = check(captures)

    if args.falsify:
        return falsify(args.falsify, binary, out, captures, args.json)

    write_manifest(binary, captures, rules, out)
    if args.verbose:
        print(captures[0].stdout.decode("utf-8", "replace"))
    code = emit(binary, captures, rules, args.json)
    print("VERDICT: PASS" if code == 0 else "VERDICT: FAIL")
    return code


if __name__ == "__main__":
    sys.exit(main())
