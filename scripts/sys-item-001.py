#!/usr/bin/env python3
"""SYS-ITEM-001 - gate of M41/W41-5: items and durability in depth (§3.6).

Two separate processes, one binary, retail read-only
----------------------------------------------------
1. `cargo build -p daggerfall-app` once (`--skip-build` reuses the binary).
2. `--probe SYS-ITEM-001` runs in **two independent processes**
   (`PROBE_RUN=1` and `PROBE_RUN=2`; the probe never prints that value, so an
   environment dependence shows up as a stdout difference).
3. Both captures must exit 0 and contain `VEREDICTO: PASS`, must be
   **byte-identical** (`cmp` + equal `sha256`), and the probe's own `DIGEST`
   line must be the `sha256` of every byte printed before it.

The probe reads `FALL.EXE` at the installation root (`$DF_APP_ROOT`, default
`$HOME/Games/Daggerfall/app`). Retail missing or unreadable ⇒ both runs report
`VEREDICTO: SKIP` and exit 2 ⇒ this runner exits **2** (usage/IO), never 0 and
never 1.

Structural checks (one rule each, all pure over the captures)
-------------------------------------------------------------
`blocks-in-order`   A..F each exactly once, in order, non-empty, and the
                    declared `rows=` equal to the measured line count.
`block-line-shape`  every line inside a block is an evidence row
                    (`[OK]`/`FAIL`) or a structured row of the declared
                    families (TEMPLATE, CENSUS, CONDITION, REPAIR, WEAR,
                    MATERIAL) - a foreign line is a hole.
`row-census`        `ROWCENSUS rows=N ok=A fail=B blocks=6` consistent with
                    the measured rows, `fail=0`, last line `VEREDICTO: PASS`.
`row-ids-unique`    no evidence row id appears twice.
`rows-cited`        every evidence row carries `(src=<path>:<line>)` and every
                    structured row ends in `src=<path>:<line>` (the aggregate
                    `CENSUS` line is the only exemption) - zero uncited
                    formula.
`src-paths-exist`   every cited path exists and the cited line is within it.
`template-census`   exactly one `CENSUS templates=<n> groups=<g> materials=<m>`;
                    the TEMPLATE row count equals `<n>`, the ids are exactly
                    0..n-1 (unique and contiguous), the distinct `group=` values
                    equal `<g>`, the distinct `material_bit=` values equal
                    `<m>`, and `material_bit=0x03ff` holds exactly for the
                    Weapons (3) and Armor (2) groups.
`condition-vectors` exactly 23 CONDITION rows: 10 weapon materials with the
                    measured maxima (steel 1200 over an 800 base) and 13 armour
                    codes in order; every formula non-empty and free of
                    approximate wording.
`repair-vectors`    exactly 9 REPAIR rows; every `seconds >= 86400`; the D-M21-1
                    vector (`condition=0 max=32768`) reports `seconds=2831155`
                    (never the pre-division 2831155200); `cost=0` implies
                    `condition == max`; no negative cost.
`wear-vectors`      damage 0, 1, 2, 15, 30 present with wear 0, 0, 0, 2, 3; no
                    negative wear; wear non-decreasing in damage.
`material-table`    exactly 10 weapon MATERIAL rows (Iron..Daedric in order,
                    multipliers of spec §2.2) and 13 armour rows (the 13 codes,
                    armour values 3,6,6,7,9,9,11,13,15,15,17,19,21).
`adversarial-refusals` the nine F rows are present with their measured facts.
`no-approximate`    no "approximate"/"estimated"/"roughly"/"~" wording.
`no-probe-run`      the runner-only `PROBE_RUN` variable never reaches stdout.
`header-contract`   the first line is
                    `PROBE-SYS-ITEM-001 - <summary> (M41/W41-5, <CR>)`.

Falsifiers (each mutates a TEMP COPY of the captures under its own name,
requires the named rule red, then re-checks the untouched captures green):
`template-missing`, `template-duplicate`, `template-mask`, `condition-steel`,
`wear-sign`, `repair-overflow`, `citation-empty`, `adversarial-missing`,
`digest-tamper`, `approx-wording`.

Exit codes: 0 PASS, 1 FAIL, 2 usage/IO (build, binary, retail).
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
GATE = "SYS-ITEM-001"
PROBE = "PROBE-SYS-ITEM-001"
SLUG = "sys-item-001"
BINARY = ROOT / "target" / "debug" / "daggerfall-app"
ARTIFACTS = ROOT / ".sintergic" / "artifacts" / "m41-sys" / SLUG
CR = "CR-75eda300-4cb0-4d9f-bcc0-85cf60ac72c5"
WAVE = "M41/W41-5"
CRATE = "daggerfall-app"
BLOCKS = ("A", "B", "C", "D", "E", "F")
EMPTY_SHA256 = hashlib.sha256(b"").hexdigest()

# spec M21 §2.2 weapon table (independent transcription, the runner's own copy)
WEAPON_NAMES = ("Iron", "Steel", "Silver", "Elven", "Dwarven", "Mithril", "Adamantium", "Ebony", "Orcish", "Daedric")
WEIGHT_MUL = (4, 5, 4, 4, 3, 4, 4, 2, 4, 5)
VALUE_MUL = (1, 2, 4, 8, 16, 32, 64, 128, 256, 512)
COND_MUL = (4, 6, 6, 8, 12, 16, 20, 24, 28, 32)
TO_HIT_MOD = (-10, 0, 0, 10, 20, 30, 30, 40, 50, 60)
DAMAGE_MOD = (-1, 0, 0, 1, 2, 3, 3, 4, 5, 6)
ARMOR_CODES = ("0x0000", "0x0100", "0x0103", "0x0200", "0x0201", "0x0202", "0x0203",
               "0x0204", "0x0205", "0x0206", "0x0207", "0x0208", "0x0209")
ARMOR_VALUES = (3, 6, 6, 7, 9, 9, 11, 13, 15, 15, 17, 19, 21)
WEAPON_MAX = tuple(800 * m // 4 for m in COND_MUL)
ARMOUR_MAX = tuple(4096 * m // 4 for m in COND_MUL)
REQUIRED_WEAR = ((0, 0), (1, 0), (2, 0), (15, 2), (30, 3))
ADVERSARIAL = (
    ("F-ZERO-FLOOR", ("min=0",)),
    ("F-BREAK-ONCE", ("transitions=1",)),
    ("F-REPAIR-BROKEN", ("cost=8", "seconds=86400")),
    ("F-OVERFLOW-TIME", ("seconds=185542587100",)),
    ("F-OVERFLOW-WEAR", ("max=214748365",)),
    ("F-COST-CLAMP", ("2147483647",)),
    ("F-GATE-DAMAGE0", ("refused=true", "rolls_consumed=0")),
    ("F-REPAIR-OVER-MAX", ("cost=8",)),
    ("F-NEGATIVE-DAMAGE", ("amount=0",)),
)

BLOCK_LINE = re.compile(r"^BLOCK (\S) rows=(\d+)$")
ROW_LINE = re.compile(r"^(?:\[OK\]|FAIL) (\S+) - .*$")
FAIL_LINE = re.compile(r"^FAIL (\S+) got=(.*?) want=(.*?) \(src=[^()]+:\d+\)$")
CITED = re.compile(r"\((src|spec)=([^():]+):(\d+)\)$")
TEMPLATE = re.compile(
    r"^TEMPLATE (\d+) (.+?) group=(-?\d+) material_bit=(0x[0-9a-f]{4}) weight=(\S+) "
    r"value=(-?\d+) src=([^():]+):(\d+)$"
)
CENSUS_T = re.compile(r"^CENSUS templates=(\d+) groups=(\d+) materials=(\d+)$")
CONDITION = re.compile(
    r"^CONDITION (weapon|armour) (\S+) ([A-Za-z0-9]+) max=(-?\d+) formula=(\S+) src=([^():]+):(\d+)$"
)
REPAIR = re.compile(
    r"^REPAIR value=(-?\d+) condition=(-?\d+) max=(-?\d+) quality=(-?\d+) cost=(-?\d+) "
    r"seconds=(-?\d+) src=([^():]+):(\d+)$"
)
WEAR = re.compile(r"^WEAR damage=(-?\d+) wear=(-?\d+) src=([^():]+):(\d+)$")
MATERIAL_W = re.compile(
    r"^MATERIAL weapon (\d+) ([A-Za-z]+) value_mul=(\d+) weight_mul=(\d+) cond_mul=(\d+) "
    r"to_hit=(-?\d+) damage=(-?\d+) kg=(\S+) value=(-?\d+) max=(-?\d+) src=([^():]+):(\d+)$"
)
MATERIAL_A = re.compile(
    r"^MATERIAL armour (0x[0-9a-f]{4}) ([A-Za-z0-9]+) armour=(-?\d+) kg=(\S+) value=(-?\d+) "
    r"max=(-?\d+) src=([^():]+):(\d+)$"
)
ROWCENSUS = re.compile(r"^ROWCENSUS rows=(\d+) ok=(\d+) fail=(\d+) blocks=(\d+)$")
DIGEST_LINE = re.compile(r"^DIGEST ([0-9a-f]{64})$")
HEADER = re.compile(r"^(PROBE-SYS-ITEM-001) - .+ \((M41/W41-5), (CR-[0-9a-f-]{36})\)$")
APPROX = re.compile(r"approximat|estimated|roughly|~", re.IGNORECASE)
VERDICT_LINE = re.compile(r"^VEREDICTO: (PASS|FAIL|SKIP.*)$")

STRUCTURED = (TEMPLATE, CENSUS_T, CONDITION, REPAIR, WEAR, MATERIAL_W, MATERIAL_A)


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


SRC_TAIL = re.compile(r" src=([^():]+):(\d+)$")
TRAILER = (ROWCENSUS, VERDICT_LINE, DIGEST_LINE)
PRE_ROWS: list[str] = []
TRAILER_ROWS: list[str] = []


def block_lines(lines: list[str]) -> tuple[list[re.Match[str]], dict[str, list[str]]]:
    """Block headers in order, the lines each block contains, and the trailer.

    A block ends at the first line that is neither an evidence row nor a
    structured row: the aggregate trailer (`ROWCENSUS`, `VEREDICTO`,
    `DIGEST`) can never be mistaken for block content, so a block that
    swallowed its trailer is not silently accepted.
    """
    headers: list[re.Match[str]] = []
    body: dict[str, list[str]] = {}
    PRE_ROWS.clear()
    TRAILER_ROWS.clear()
    current: str | None = None
    closed = False
    for line in lines:
        if not line:
            continue
        if closed:
            TRAILER_ROWS.append(line)
            continue
        if any(pattern.match(line) for pattern in TRAILER):
            closed = True
            current = None
            TRAILER_ROWS.append(line)
            continue
        header = BLOCK_LINE.match(line)
        if header:
            headers.append(header)
            current = header.group(1)
            body.setdefault(current, [])
            continue
        if current is None:
            PRE_ROWS.append(line)
        else:
            body[current].append(line)
    return headers, body


def all_rows(lines: list[str]) -> list[str]:
    return [line for line in lines if ROW_LINE.match(line)]


def row_id(line: str) -> str:
    match = ROW_LINE.match(line)
    assert match is not None
    return match.group(1)


def is_evidence(line: str) -> bool:
    return line.startswith("[OK] ") or line.startswith("FAIL ")


def structured(lines: list[str], pattern: re.Pattern[str]) -> list[re.Match[str]]:
    return [match for line in lines if (match := pattern.match(line))]


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
        lines = lines_of(capture)
        index = next(i for i, line in enumerate(lines) if line.startswith("DIGEST "))
        expected = sha(("\n".join(lines[:index]) + "\n").encode())
        if expected != value:
            problems.append(f"run{capture.run}:digest={value}!={expected}")
        if value == EMPTY_SHA256:
            problems.append(f"run{capture.run}:empty-sha")
        detail.append(f"run{capture.run}={value[:16]}...")
    if problems:
        return Rule("digest-valid", False, " ".join(problems))
    return Rule("digest-valid", True, f"prefix-sha256 verified x2 {' '.join(detail)}")


def rule_blocks_in_order(captures: list[Capture]) -> Rule:
    headers, body = block_lines(lines_of(captures[0]))
    problems: list[str] = []
    seq = [match.group(1) for match in headers]
    if seq != list(BLOCKS):
        problems.append(f"order={''.join(seq) or 'none'}")
    if len(PRE_ROWS) != 1 or not PRE_ROWS[0].startswith(PROBE):
        problems.append(f"lines-before-first-block={len(PRE_ROWS)}")
    shapes = [any(pattern.match(line) for pattern in TRAILER) for line in TRAILER_ROWS]
    if len(TRAILER_ROWS) != 3 or shapes != [True, True, True]:
        problems.append(f"trailer={len(TRAILER_ROWS)}")
    for match in headers:
        label, declared = match.group(1), int(match.group(2))
        measured = len(body.get(label, []))
        if declared != measured:
            problems.append(f"{label}:declared={declared} measured={measured}")
        if measured == 0:
            problems.append(f"{label}:empty")
    if problems:
        return Rule("blocks-in-order", False, " ".join(problems[:8]))
    return Rule(
        "blocks-in-order",
        True,
        f"blocks={len(seq)} lines={sum(len(v) for v in body.values())} order={'..'.join(BLOCKS)}",
    )


def rule_block_line_shape(captures: list[Capture]) -> Rule:
    _, body = block_lines(lines_of(captures[0]))
    foreign: list[str] = []
    counts = {"evidence": 0, "structured": 0}
    for label in BLOCKS:
        for line in body.get(label, []):
            if is_evidence(line):
                counts["evidence"] += 1
            elif any(pattern.match(line) for pattern in STRUCTURED):
                counts["structured"] += 1
            else:
                foreign.append(line[:48])
    if foreign:
        return Rule("block-line-shape", False, f"foreign={len(foreign)} first={foreign[0]!r}")
    return Rule(
        "block-line-shape",
        True,
        f"evidence={counts['evidence']} structured={counts['structured']} foreign=0",
    )


def rule_row_census(captures: list[Capture]) -> Rule:
    lines = lines_of(captures[0])
    census = [match for line in lines if (match := ROWCENSUS.match(line))]
    rows = all_rows(lines)
    fails = [line for line in lines if FAIL_LINE.match(line)]
    problems: list[str] = []
    if len(census) != 1:
        return Rule("row-census", False, f"census-lines={len(census)}")
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
    aggregates = 0
    for line in lines:
        if not any(pattern.match(line) for pattern in STRUCTURED):
            continue
        if CENSUS_T.match(line):
            aggregates += 1
            continue
        if SRC_TAIL.search(line):
            cited += 1
        else:
            uncited.append(line[:40])
    if uncited:
        return Rule("rows-cited", False, f"uncited={','.join(uncited[:8])} ({len(uncited)})")
    return Rule(
        "rows-cited",
        True,
        f"cited={cited} aggregate-exempt={aggregates} uncited=0",
    )


def rule_src_paths_exist(captures: list[Capture]) -> Rule:
    lines = lines_of(captures[0])
    refs = [(m.group(2), int(m.group(3))) for line in lines if (m := CITED.search(line))]
    refs += [(m.group(1), int(m.group(2))) for line in lines if (m := SRC_TAIL.search(line))]
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


def rule_template_census(captures: list[Capture]) -> Rule:
    lines = lines_of(captures[0])
    census = [match for line in lines if (match := CENSUS_T.match(line))]
    rows = structured(lines, TEMPLATE)
    problems: list[str] = []
    if len(census) != 1:
        return Rule("template-census", False, f"census-lines={len(census)}")
    templates, groups, materials = (int(census[0].group(i)) for i in (1, 2, 3))
    if templates != len(rows):
        problems.append(f"declared={templates} rows={len(rows)}")
    ids = [int(match.group(1)) for match in rows]
    if len(set(ids)) != len(ids):
        problems.append(f"duplicated-ids={len(ids) - len(set(ids))}")
    if set(ids) != set(range(templates)):
        problems.append(f"ids-not-contiguous max={max(ids) if ids else -1}")
    group_values = {int(match.group(3)) for match in rows}
    mask_values = {match.group(4) for match in rows}
    if len(group_values) != groups:
        problems.append(f"groups declared={groups} measured={len(group_values)}")
    if len(mask_values) != materials:
        problems.append(f"materials declared={materials} measured={len(mask_values)}")
    for match in rows:
        group, mask = int(match.group(3)), match.group(4)
        if (mask == "0x03ff") != (group in (2, 3)):
            problems.append(f"mask-vs-class id={match.group(1)} group={group} mask={mask}")
            break
        if not match.group(2).strip():
            problems.append(f"empty-name id={match.group(1)}")
            break
        try:
            weight = float(match.group(5))
        except ValueError:
            problems.append(f"weight-not-numeric id={match.group(1)}")
            break
        if weight < 0:
            problems.append(f"negative-weight id={match.group(1)}")
            break
    if problems:
        return Rule("template-census", False, " ".join(problems[:8]))
    return Rule(
        "template-census",
        True,
        f"templates={templates} ids=0..{templates - 1} groups={groups} materials={materials} masks={sorted(mask_values)}",
    )


def rule_condition_vectors(captures: list[Capture]) -> Rule:
    rows = structured(lines_of(captures[0]), CONDITION)
    problems: list[str] = []
    weapons = [m for m in rows if m.group(1) == "weapon"]
    armours = [m for m in rows if m.group(1) == "armour"]
    if len(rows) != 23:
        problems.append(f"rows={len(rows)}")
    if len(weapons) != 10 or len(armours) != 13:
        problems.append(f"weapon={len(weapons)} armour={len(armours)}")
    by_material = {int(m.group(2)): int(m.group(4)) for m in weapons}
    if sorted(by_material) != list(range(10)):
        problems.append(f"weapon-materials={sorted(by_material)}")
    for material, want in enumerate(WEAPON_MAX):
        if by_material.get(material) != want:
            problems.append(f"material{material}={by_material.get(material)}!={want}")
            break
    by_code = {m.group(2): int(m.group(4)) for m in armours}
    if tuple(by_code) != ARMOR_CODES:
        problems.append(f"armour-codes={tuple(by_code)}")
    for code, want in zip(ARMOR_CODES[3:], ARMOUR_MAX):
        if by_code.get(code) != want:
            problems.append(f"{code}={by_code.get(code)}!={want}")
            break
    for match in rows:
        formula = match.group(5)
        if not formula or not any(ch.isdigit() for ch in formula):
            problems.append(f"formula-empty {match.group(2)}")
            break
        if "~" in formula or APPROX.search(formula):
            problems.append(f"formula-approximate {match.group(2)}")
            break
    if problems:
        return Rule("condition-vectors", False, " ".join(problems[:8]))
    return Rule(
        "condition-vectors",
        True,
        f"rows=23 weapon=10 (steel max={by_material[1]}) armour=13 (daedric max={by_code['0x0209']}) formulas-cited=23",
    )


def rule_repair_vectors(captures: list[Capture]) -> Rule:
    rows = structured(lines_of(captures[0]), REPAIR)
    problems: list[str] = []
    if len(rows) != 9:
        problems.append(f"rows={len(rows)}")
    overflow = [
        m for m in rows if int(m.group(2)) == 0 and int(m.group(3)) == 32768
    ]
    if len(overflow) != 1:
        problems.append(f"overflow-vectors={len(overflow)}")
    elif int(overflow[0].group(6)) != 2831155:
        problems.append(f"overflow-seconds={overflow[0].group(6)}")
    for match in rows:
        condition, maximum = int(match.group(2)), int(match.group(3))
        cost, seconds = int(match.group(5)), int(match.group(6))
        if seconds >= 2831155200:
            problems.append("pre-division-product")
        if seconds < 86400:
            problems.append(f"seconds-floor {seconds}")
        if cost < 0:
            problems.append(f"negative-cost {cost}")
        if cost == 0 and condition != maximum:
            problems.append(f"free-but-damaged {condition}/{maximum}")
    if problems:
        return Rule("repair-vectors", False, " ".join(problems[:8]))
    return Rule(
        "repair-vectors",
        True,
        f"rows=9 overflow=2831155s floor=86400 costs={[int(m.group(5)) for m in rows]}",
    )


def rule_wear_vectors(captures: list[Capture]) -> Rule:
    rows = structured(lines_of(captures[0]), WEAR)
    by_damage = {int(m.group(1)): int(m.group(2)) for m in rows}
    problems: list[str] = []
    for damage, want in REQUIRED_WEAR:
        if by_damage.get(damage) != want:
            problems.append(f"damage{damage}={by_damage.get(damage)}!={want}")
    for damage, wear in by_damage.items():
        if wear < 0:
            problems.append(f"negative-wear damage={damage}")
            break
    ordered = [by_damage[d] for d in sorted(by_damage)]
    if ordered != sorted(ordered):
        problems.append(f"not-monotone={ordered}")
    if problems:
        return Rule("wear-vectors", False, " ".join(problems[:8]))
    return Rule(
        "wear-vectors",
        True,
        f"rows={len(rows)} damages={sorted(by_damage)} wear={ordered}",
    )


def rule_material_table(captures: list[Capture]) -> Rule:
    lines = lines_of(captures[0])
    weapons = structured(lines, MATERIAL_W)
    armours = structured(lines, MATERIAL_A)
    problems: list[str] = []
    if len(weapons) != 10:
        problems.append(f"weapon-rows={len(weapons)}")
    if len(armours) != 13:
        problems.append(f"armour-rows={len(armours)}")
    for match in weapons:
        material = int(match.group(1))
        if match.group(2) != WEAPON_NAMES[material]:
            problems.append(f"name{material}={match.group(2)}")
            break
        got = tuple(int(match.group(i)) for i in (3, 4, 5, 6, 7))
        want = (VALUE_MUL[material], WEIGHT_MUL[material], COND_MUL[material],
                TO_HIT_MOD[material], DAMAGE_MOD[material])
        if got != want:
            problems.append(f"table{material}={got}!={want}")
            break
        if int(match.group(10)) != WEAPON_MAX[material]:
            problems.append(f"max{material}={match.group(10)}")
            break
    codes = tuple(m.group(1) for m in armours)
    if codes != ARMOR_CODES:
        problems.append(f"codes={codes}")
    for match, want in zip(armours, ARMOR_VALUES):
        if int(match.group(3)) != want:
            problems.append(f"armour{match.group(1)}={match.group(3)}!={want}")
            break
    if problems:
        return Rule("material-table", False, " ".join(problems[:8]))
    return Rule(
        "material-table",
        True,
        f"weapon=10 Iron..Daedric armour=13 {ARMOR_CODES[0]}..{ARMOR_CODES[-1]} armour-values={list(ARMOR_VALUES)}",
    )


def rule_adversarial_refusals(captures: list[Capture]) -> Rule:
    rows = {row_id(line): line for line in all_rows(lines_of(captures[0]))}
    problems: list[str] = []
    for name, facts in ADVERSARIAL:
        line = rows.get(name)
        if line is None:
            problems.append(f"{name}:absent")
            continue
        for fact in facts:
            if fact not in line:
                problems.append(f"{name}:{fact}")
    if problems:
        return Rule("adversarial-refusals", False, " ".join(problems[:8]))
    return Rule(
        "adversarial-refusals",
        True,
        f"rows={len(ADVERSARIAL)} present refused-or-clamped x{len(ADVERSARIAL)}",
    )


def rule_no_approximate(captures: list[Capture]) -> Rule:
    hits = [line[:60] for line in lines_of(captures[0]) if APPROX.search(line)]
    if hits:
        return Rule("no-approximate", False, f"hits={len(hits)} first={hits[0]!r}")
    return Rule("no-approximate", True, "0 approximate/estimated/roughly/~ rows")


def rule_no_probe_run(captures: list[Capture]) -> Rule:
    leaks = [capture.run for capture in captures if b"PROBE_RUN" in capture.stdout]
    if leaks:
        return Rule("no-probe-run", False, f"runs={leaks}")
    return Rule("no-probe-run", True, "runner-only variable absent from stdout")


def rule_header_contract(captures: list[Capture]) -> Rule:
    first = lines_of(captures[0])[0] if lines_of(captures[0]) else ""
    suffix = f"({WAVE}, {CR})"
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
    rule_block_line_shape,
    rule_row_census,
    rule_row_ids_unique,
    rule_rows_cited,
    rule_src_paths_exist,
    rule_template_census,
    rule_condition_vectors,
    rule_repair_vectors,
    rule_wear_vectors,
    rule_material_table,
    rule_adversarial_refusals,
    rule_no_approximate,
    rule_no_probe_run,
)


def check(captures: list[Capture]) -> list[Rule]:
    """The eighteen rules over two captures. Pure: no disk, no clock, no build."""
    return [rule(captures) for rule in RULES]


# ── Reporting ──────────────────────────────────────────────────────────────


def failed_names(rules: list[Rule]) -> list[str]:
    return [rule.name for rule in rules if not rule.ok]


def payload(binary: Path, captures: list[Capture], rules: list[Rule], extra: dict | None = None) -> dict:
    lines = lines_of(captures[0])
    headers, body = block_lines(lines)
    census = [match for line in lines if (match := CENSUS_T.match(line))]
    body_payload = {
        "schema": "SYS-ITEM-001/1",
        "gate": GATE,
        "cr": CR,
        "probe": PROBE,
        "head": head_commit(),
        "binary": str(binary),
        "binary_sha256": sha(binary.read_bytes()) if binary.is_file() else None,
        "rows": len(all_rows(lines)),
        "lines_per_block": {match.group(1): int(match.group(2)) for match in headers},
        "blocks": {label: len(value) for label, value in body.items()},
        "templates": {
            "templates": int(census[0].group(1)) if census else None,
            "groups": int(census[0].group(2)) if census else None,
            "materials": int(census[0].group(3)) if census else None,
            "rows": len(structured(lines, TEMPLATE)),
        },
        "condition_rows": len(structured(lines, CONDITION)),
        "repair_rows": len(structured(lines, REPAIR)),
        "wear_rows": len(structured(lines, WEAR)),
        "material_weapon_rows": len(structured(lines, MATERIAL_W)),
        "material_armour_rows": len(structured(lines, MATERIAL_A)),
        "probe_digest": digest_of(captures[0]),
        "stdout": [
            {"run": capture.run, "exit": capture.exit, "bytes": len(capture.stdout), "sha256": sha(capture.stdout)}
            for capture in captures
        ],
        "rules": [{"name": rule.name, "ok": rule.ok, "detail": rule.detail} for rule in rules],
        "failed": failed_names(rules),
    }
    if extra:
        body_payload.update(extra)
    return body_payload


def emit(binary: Path, captures: list[Capture], rules: list[Rule], as_json: bool, extra: dict | None = None) -> int:
    failed = failed_names(rules)
    body = payload(binary, captures, rules, extra)
    if as_json:
        print(json.dumps(body, indent=2, sort_keys=True))
    else:
        print(
            f"gate {GATE} probe={PROBE} cr={CR} rows={body['rows']} "
            f"templates={body['templates']['templates']}"
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


def reseal(captures: list[Capture]) -> list[Capture]:
    """Recompute every DIGEST after a mutation.

    Without it any mutation would trivially trip `digest-valid` and the named
    rule would never be shown to catch the defect on its own.
    """
    sealed: list[Capture] = []
    for capture in captures:
        lines = lines_of(capture)
        index = index_of(lines, "DIGEST ")
        value = sha(("\n".join(lines[:index]) + "\n").encode())
        sealed.append(replace(capture, stdout=("\n".join(lines[:index] + [f"DIGEST {value}"]) + "\n").encode()))
    return sealed


def reconcile_block_counts(captures: list[Capture]) -> list[Capture]:
    """Rewrite `BLOCK X rows=N` so a line drop surfaces only in the named rule."""
    fixed: list[Capture] = []
    for capture in captures:
        out: list[str] = []
        open_index: int | None = None
        for line in lines_of(capture):
            header = BLOCK_LINE.match(line)
            if (header is not None or any(p.match(line) for p in TRAILER)) and open_index is not None:
                label = BLOCK_LINE.match(out[open_index])
                assert label is not None
                out[open_index] = f"BLOCK {label.group(1)} rows={len(out) - open_index - 1}"
                open_index = None
            if header is not None:
                open_index = len(out)
            out.append(line)
        if open_index is not None:
            label = BLOCK_LINE.match(out[open_index])
            assert label is not None
            out[open_index] = f"BLOCK {label.group(1)} rows={len(out) - open_index - 1}"
        fixed.append(replace(capture, stdout=("\n".join(out) + "\n").encode()))
    return fixed


def mutate_template_missing(captures: list[Capture]) -> tuple[list[Capture], str]:
    lines = lines_of(captures[0])
    index = index_of(lines, "TEMPLATE 287 ")
    return drop(captures, index), f"removed {lines[index][:48]}"


def mutate_template_duplicate(captures: list[Capture]) -> tuple[list[Capture], str]:
    lines = lines_of(captures[0])
    first = index_of(lines, "TEMPLATE 0 ")
    last = index_of(lines, "TEMPLATE 287 ")
    return rewrite(captures, last, lines[first]), "template 287 replaced by template 0"


def mutate_template_mask(captures: list[Capture]) -> tuple[list[Capture], str]:
    lines = lines_of(captures[0])
    index = index_of(lines, "TEMPLATE 0 ")
    match = TEMPLATE.match(lines[index])
    assert match is not None
    mutated = lines[index].replace("material_bit=0x0000", "material_bit=0x03ff", 1)
    return rewrite(captures, index, mutated), "group 14 (Gems) claims the weapon material mask"


def mutate_condition_steel(captures: list[Capture]) -> tuple[list[Capture], str]:
    lines = lines_of(captures[0])
    index = index_of(lines, "CONDITION weapon 1 Steel ")
    return rewrite(captures, index, lines[index].replace("max=1200", "max=1199", 1)), "steel 1200 -> 1199"


def mutate_wear_sign(captures: list[Capture]) -> tuple[list[Capture], str]:
    lines = lines_of(captures[0])
    index = index_of(lines, "WEAR damage=15 ")
    return rewrite(captures, index, lines[index].replace("wear=2", "wear=-2", 1)), "wear 2 -> -2"


def mutate_repair_overflow(captures: list[Capture]) -> tuple[list[Capture], str]:
    lines = lines_of(captures[0])
    index = index_of(lines, "REPAIR value=45 condition=0 max=32768 ")
    return (
        rewrite(captures, index, lines[index].replace("seconds=2831155", "seconds=2831155200", 1)),
        "D-M21-1 seconds replaced by the pre-division product",
    )


def mutate_citation_empty(captures: list[Capture]) -> tuple[list[Capture], str]:
    lines = lines_of(captures[0])
    index = index_of(lines, "MATERIAL weapon 1 Steel ")
    stripped = re.sub(r" src=[^ ]+$", "", lines[index])
    return rewrite(captures, index, stripped), f"stripped citation of {lines[index][:32]}"


def mutate_adversarial_missing(captures: list[Capture]) -> tuple[list[Capture], str]:
    lines = lines_of(captures[0])
    index = index_of(lines, "[OK] F-COST-CLAMP ")
    return drop(captures, index), "removed the cost-clamp refusal"


def mutate_digest_tamper(captures: list[Capture]) -> tuple[list[Capture], str]:
    lines = lines_of(captures[0])
    index = index_of(lines, "DIGEST ")
    value = DIGEST_LINE.match(lines[index])
    assert value is not None
    flipped = ("0" if value.group(1)[0] != "0" else "1") + value.group(1)[1:]
    return rewrite(captures, index, f"DIGEST {flipped}"), "DIGEST first hex nibble flipped"


def mutate_approx_wording(captures: list[Capture]) -> tuple[list[Capture], str]:
    lines = lines_of(captures[0])
    index = index_of(lines, "[OK] A-COUNT ")
    injected = lines[index].replace(" - ", " - approximately ", 1)
    return rewrite(captures, index, injected), "injected 'approximately' into A-COUNT"


FALSIFIERS = {
    "template-missing": ("template-census", mutate_template_missing),
    "template-duplicate": ("template-census", mutate_template_duplicate),
    "template-mask": ("template-census", mutate_template_mask),
    "condition-steel": ("condition-vectors", mutate_condition_steel),
    "wear-sign": ("wear-vectors", mutate_wear_sign),
    "repair-overflow": ("repair-vectors", mutate_repair_overflow),
    "citation-empty": ("block-line-shape", mutate_citation_empty),
    "adversarial-missing": ("adversarial-refusals", mutate_adversarial_missing),
    "digest-tamper": ("digest-valid", mutate_digest_tamper),
    "approx-wording": ("no-approximate", mutate_approx_wording),
}


def falsify(mode: str, binary: Path, out: Path, captures: list[Capture], as_json: bool) -> int:
    """Apply one mutation, require the named rule red, then re-check the clean copies."""
    target, mutate = FALSIFIERS[mode]
    mutated, note = mutate(captures)
    if mode == "digest-tamper":
        pass  # the stale digest *is* the mutation
    else:
        mutated = reseal(reconcile_block_counts(mutated))
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
        print(f"FALSIFY {mode}: failed={','.join(failed) or 'none'} (target={target})")
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
    if all(capture.exit == 2 for capture in captures) and all(
        "VEREDICTO: SKIP" in capture.stdout.decode("utf-8", "replace") for capture in captures
    ):
        for capture in captures:
            (out).mkdir(parents=True, exist_ok=True)
            (out / f"run{capture.run}.stdout").write_bytes(capture.stdout)
        print(captures[0].stdout.decode("utf-8", "replace"))
        print("VERDICT: SKIP - FALL.EXE unavailable at the installation root (exit 2)")
        return 2

    rules = check(captures)

    if args.falsify:
        return falsify(args.falsify, binary, out, captures, args.json)

    write_manifest(binary, captures, rules, out)
    if args.verbose:
        print(captures[0].stdout.decode("utf-8", "replace"))
    code = emit(binary, captures, rules, args.json)
    if not args.json:
        print("VERDICT: PASS" if code == 0 else "VERDICT: FAIL")
    return code


if __name__ == "__main__":
    sys.exit(main())
