#!/usr/bin/env python3
"""SYS-NPC-001 - gate of M42/W42-1: living NPCs with 24 h deterministic agendas.

Two separate processes, same binary
-----------------------------------
1. `cargo build -p daggerfall-app` once (`--skip-build` reuses the binary).
2. `PROBE-SYS-NPC-001` runs in **two independent processes** (`PROBE_RUN=1`
   and `PROBE_RUN=2`; the probe never prints that value, so an environment
   dependence shows up as a stdout difference).
3. Both captures must exit 0, contain `VEREDICTO: PASS`, be **byte-identical**
   (`cmp` + equal `sha256`), and the probe's own `DIGEST` line must equal the
   `sha256` of every byte printed before it.

Density rules the gate enforces (each one is built to fail)
-----------------------------------------------------------
* `header` -- first line declares the probe, the cycle and the CR.
* `agenda-census` -- exactly 192 `AGENDA` rows (8 NPCs x 24 h), hours 00..23
  ascending per NPC, npc0 guard / npc1..7 civilian.
* `agenda-kinds` -- civilian kinds follow the frozen day table (`home` 22-05,
  `work` 06-10/13-17, `meal` 11-12/18-19, `rest` 20-21) and the guard patrols
  24/24; every kind is one of the 5 frozen labels.
* `teleport-free` -- every `DAY` row reports `teleports=0` and `max_step<=1`,
  and the `CENSUS` total is `teleports=0` (each of the 8 days appends every
  path step and fails on Chebyshev distance > 1).
* `census-totals` -- `CENSUS npcs=8 hours=24 stops=192 teleports=0`.
* `citation-present` -- **every** `[OK]` row carries `src=` or `spec=`.
* `negative-paths` -- every `NEGATIVE` row names a `reason=` path that exists
  on disk (a NEGATIVE without its evidence path is a FAIL).
* `no-fail-rows`, `verdict-last`, `digest-integrity`, `probe-digest`.

Falsifiers (each mutates the **capture**, never the repo, requires its own rule
to go RED, then re-measures the restored captures and requires them green):
`agenda-missing`, `kind-mismatch`, `teleport-injected`, `census-mismatch`,
`citation-empty`, `negative-without-path`, `digest-tamper`.

The probe reads **0 retail bytes**: the grid is the synthetic
`NavGrid::city(1, seed)` and every NPC start is fixed, so this gate has no
ARENA2 precondition and never SKIPs for a missing retail tree.

Artifacts go to `.sintergic/artifacts/m42-sys/sys-npc-001/` (gitignored).

Exit codes: 0 PASS, 1 FAIL, 2 usage/IO (including `SKIP`).
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
GATE = "M42/W42-1"
PROBE = "PROBE-SYS-NPC-001"
SLUG = "sys-npc-001"
BINARY = ROOT / "target" / "debug" / "daggerfall-app"
ARTIFACTS = ROOT / ".sintergic" / "artifacts" / "m42-sys" / SLUG
CR = "CR-b434f896-2fe9-4750-b70e-9b748370e6c3"
HEADER_PREFIX = f"{PROBE} - living NPCs: 24h deterministic agendas (M42/W42-1, {CR})"

NPC_COUNT = 8
DAY_HOURS = 24
KINDS: tuple[str, ...] = ("home", "work", "meal", "patrol", "rest")


def expected_kind(hour: int, guard: bool) -> str:
    if guard:
        return "patrol"
    if hour <= 5 or hour >= 22:
        return "home"
    if 6 <= hour <= 10 or 13 <= hour <= 17:
        return "work"
    if 11 <= hour <= 12 or 18 <= hour <= 19:
        return "meal"
    return "rest"


AGENDA_LINE = re.compile(
    r"^AGENDA npc=(\d+) seed=(\d+) guard=([01]) hour=(\d\d) cell=\((-?\d+),(-?\d+)\) kind=(\S+)$"
)
DAY_LINE = re.compile(
    r"^DAY npc=(\d+) moves=(\d+) cells=(\d+) teleports=(\d+) max_step=(\d+) digest=([0-9a-f]{16}) agenda=([0-9a-f]{16})$"
)
CENSUS_LINE = re.compile(
    r"^CENSUS npcs=(\d+) hours=(\d+) stops=(\d+) teleports=(\d+)(?:\s|$)"
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


def agenda_rule(lines: list[str]) -> tuple[bool, str]:
    rows = [AGENDA_LINE.match(line) for line in prefixed(lines, "AGENDA ")]
    if len(rows) != NPC_COUNT * DAY_HOURS or not all(rows):
        bad = sum(1 for row in rows if row is None)
        return False, f"rows={len(rows)} malformed={bad}"
    per_npc: dict[int, list[int]] = {}
    guards: dict[int, str] = {}
    for match in rows:  # type: ignore[union-attr]
        npc, guard, hour = int(match.group(1)), match.group(3), int(match.group(4))
        if not 0 <= npc < NPC_COUNT or not 0 <= hour < DAY_HOURS:
            return False, f"npc={npc} hour={hour} out-of-range"
        per_npc.setdefault(npc, []).append(hour)
        guards.setdefault(npc, guard)
        if guards[npc] != guard:
            return False, f"npc={npc} guard-flag-flip"
    if sorted(per_npc) != list(range(NPC_COUNT)):
        return False, f"npcs={sorted(per_npc)}"
    for npc, hours in per_npc.items():
        if hours != list(range(DAY_HOURS)):
            return False, f"npc={npc} hours-not-00..23"
    if guards.get(0) != "1" or any(guards.get(i) != "0" for i in range(1, NPC_COUNT)):
        return False, f"guard-flags={sorted(guards.items())}"
    return True, f"rows={len(rows)} npcs={NPC_COUNT} guard=npc0"


def kinds_rule(lines: list[str]) -> tuple[bool, str]:
    rows = [AGENDA_LINE.match(line) for line in prefixed(lines, "AGENDA ")]
    if not rows or not all(rows):
        return False, f"rows={len(rows)}"
    bad = 0
    for match in rows:  # type: ignore[union-attr]
        npc, hour, kind = int(match.group(1)), int(match.group(4)), match.group(7)
        if kind not in KINDS or kind != expected_kind(hour, npc == 0):
            bad += 1
    return (bad == 0), f"rows={len(rows)} mismatched={bad}"


def teleport_rule(lines: list[str]) -> tuple[bool, str]:
    rows = [DAY_LINE.match(line) for line in prefixed(lines, "DAY ")]
    if len(rows) != NPC_COUNT or not all(rows):
        return False, f"day-rows={len(rows)}"
    teleports = sum(int(match.group(4)) for match in rows)  # type: ignore[union-attr]
    worst = max(int(match.group(5)) for match in rows)  # type: ignore[union-attr]
    moves = sum(int(match.group(2)) for match in rows)  # type: ignore[union-attr]
    ok = teleports == 0 and worst <= 1 and moves > 0
    return ok, f"teleports={teleports} max_step={worst} moves={moves}"


def census_rule(lines: list[str]) -> tuple[bool, str]:
    rows = [CENSUS_LINE.match(line) for line in prefixed(lines, "CENSUS ")]
    if len(rows) != 1 or rows[0] is None:
        return False, f"lines={len(rows)}"
    npcs, hours, stops, teleports = (int(value) for value in rows[0].groups())
    agenda = len(prefixed(lines, "AGENDA "))
    ok = (
        npcs == NPC_COUNT
        and hours == DAY_HOURS
        and stops == NPC_COUNT * DAY_HOURS == agenda
        and teleports == 0
    )
    return ok, f"npcs={npcs} hours={hours} stops={stops}/{agenda} teleports={teleports}"


def ids_rule(lines: list[str]) -> tuple[bool, str]:
    ids: list[str] = []
    for line in lines:
        match = OK_LINE.match(line) or FAIL_LINE.match(line)
        if match:
            ids.append(match.group(1))
    duplicates = sorted({item for item in ids if ids.count(item) > 1})
    return (not duplicates and bool(ids)), f"ids={len(ids)} duplicates={','.join(duplicates) or 'none'}"


def citation_rule(lines: list[str]) -> tuple[bool, str]:
    missing: list[str] = []
    total = 0
    for line in lines:
        match = OK_LINE.match(line)
        if not match:
            continue
        total += 1
        if CITATION.search(match.group(2)) is None:
            missing.append(match.group(1))
    return (not missing and total > 0), f"ok-rows={total} uncited={','.join(missing[:6]) or 'none'}"


def negative_rule(lines: list[str]) -> tuple[bool, str]:
    rows = [NEGATIVE_LINE.match(line) for line in prefixed(lines, "NEGATIVE ")]
    if not rows:
        return False, "no-NEGATIVE-rows"
    bad: list[str] = []
    for match in rows:
        if match is None:
            bad.append("malformed")
        elif not path_exists(match.group(2)):
            bad.append(f"{match.group(1)}={match.group(2)}")
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
        lines = raw.split(b"\n")
        while lines and lines[-1] == b"":
            lines.pop()
        if not lines:
            return False, f"run{capture.run}=empty"
        last = lines[-1].decode("utf-8", "replace")
        match = DIGEST_LINE.match(last)
        if match is None:
            return False, f"run{capture.run}=no-DIGEST"
        body = b"\n".join(lines[:-1]) + b"\n"
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
        Rule("agenda-census", *agenda_rule(lines_one)),
        Rule("agenda-kinds", *kinds_rule(lines_one)),
        Rule("teleport-free", *teleport_rule(lines_one)),
        Rule("census-totals", *census_rule(lines_one)),
        Rule("census-ids-unique", *ids_rule(lines_one)),
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
        "npcs": NPC_COUNT,
        "hours": DAY_HOURS,
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
                "agenda_rows": len(prefixed(lines_of(capture), "AGENDA ")),
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
                f"ok-rows={capture['ok_rows']} agenda={capture['agenda_rows']} "
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
            [str(binary), "--probe", "SYS-NPC-001"],
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


def mutate_agenda_missing(captures: list[Capture]) -> tuple[list[Capture], str]:
    note: list[str] = []

    def mutate(lines: list[str]) -> list[str]:
        new, removed = drop_first(lines, "AGENDA npc=3 ")
        note.append(removed.split(" ")[3])
        return new

    return replace_lines(captures, mutate), "removed one AGENDA npc=3 row"


def mutate_kind_mismatch(captures: list[Capture]) -> tuple[list[Capture], str]:
    note: list[str] = []

    def mutate(lines: list[str]) -> list[str]:
        for index, line in enumerate(lines):
            if line.startswith("AGENDA npc=1 ") and " hour=12 " in line:
                lines[index] = line.replace("kind=meal", "kind=work", 1)
                note.append("npc=1 hour=12 meal->work")
                return lines
        raise AssertionError("no AGENDA npc=1 hour=12")

    return replace_lines(captures, mutate), "npc=1 hour=12 kind meal->work"


def mutate_teleport_injected(captures: list[Capture]) -> tuple[list[Capture], str]:
    note: list[str] = []

    def mutate(lines: list[str]) -> list[str]:
        for index, line in enumerate(lines):
            if line.startswith("DAY npc=2 "):
                lines[index] = re.sub(r"teleports=\d+", "teleports=1", line, count=1)
                lines[index] = re.sub(r"max_step=\d+", "max_step=7", lines[index], count=1)
                note.append("npc=2 teleports=1 max_step=7")
                return lines
        raise AssertionError("no DAY npc=2")

    return replace_lines(captures, mutate), "npc=2 teleports=1 max_step=7"


def mutate_census_mismatch(captures: list[Capture]) -> tuple[list[Capture], str]:
    note: list[str] = []

    def mutate(lines: list[str]) -> list[str]:
        for index, line in enumerate(lines):
            match = CENSUS_LINE.match(line)
            if match:
                npcs, hours, stops, teleports = (int(value) for value in match.groups())
                lines[index] = (
                    f"CENSUS npcs={npcs} hours={hours} stops={stops - 1} "
                    f"teleports={teleports}"
                )
                note.append(f"stops {stops} -> {stops - 1}")
                return lines
        raise AssertionError("no CENSUS line")

    return replace_lines(captures, mutate), "CENSUS stops 192 -> 191"


def mutate_citation_empty(captures: list[Capture]) -> tuple[list[Capture], str]:
    note: list[str] = []

    def mutate(lines: list[str]) -> list[str]:
        for index, line in enumerate(lines):
            if line.startswith("[OK] schedule-24h "):
                lines[index] = CITATION.sub("", line).rstrip()
                note.append("schedule-24h")
                return lines
        raise AssertionError("no schedule-24h")

    return replace_lines(captures, mutate), "stripped src= from schedule-24h"


def mutate_negative_without_path(captures: list[Capture]) -> tuple[list[Capture], str]:
    note: list[str] = []

    def mutate(lines: list[str]) -> list[str]:
        for index, line in enumerate(lines):
            match = NEGATIVE_LINE.match(line)
            if match:
                lines[index] = line.replace(
                    match.group(2), "crates/does-not-exist/nope.rs", 1
                )
                note.append(match.group(1))
                return lines
        raise AssertionError("no NEGATIVE row")

    return replace_lines(captures, mutate), "reason path -> crates/does-not-exist/nope.rs"


def mutate_digest_tamper(captures: list[Capture]) -> tuple[list[Capture], str]:
    def mutate(lines: list[str]) -> list[str]:
        for index, line in enumerate(lines):
            if DIGEST_LINE.match(line):
                lines[index] = "DIGEST " + "0" * 64
                return lines
        raise AssertionError("no DIGEST line")

    return replace_lines(captures, mutate), "DIGEST -> 64 zeros"


FALSIFIERS = {
    "agenda-missing": ("agenda-census", mutate_agenda_missing),
    "kind-mismatch": ("agenda-kinds", mutate_kind_mismatch),
    "teleport-injected": ("teleport-free", mutate_teleport_injected),
    "census-mismatch": ("census-totals", mutate_census_mismatch),
    "citation-empty": ("citation-present", mutate_citation_empty),
    "negative-without-path": ("negative-paths", mutate_negative_without_path),
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
