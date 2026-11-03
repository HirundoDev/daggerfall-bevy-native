#!/usr/bin/env python3
"""SYS-LAW-001 - gate of M42/W42-4: per-region record, legal reputation, guards, five outcomes.

Process contract (mirrors scripts/sys-faction-001.py):
  1. Two separate OS processes run `daggerfall-app --probe SYS-LAW-001`.
  2. Both stdouts must be byte-identical (determinism gate).
  3. `DIGEST <sha256>` (last line) must equal sha256 of all preceding bytes.
  4. Every `[OK]` row must carry a `src=`/`spec=` citation to a real repo path
     or the declared `rule=SR-42-4-1` (bribe: no retail source exists);
     every `NEGATIVE` row must carry a `reason=` path that exists.
  5. `--falsify <mode>` mutates the clean capture and requires the named rule
     to go RED, then re-checks the clean capture is still PASS (green restore).

Golden values (measured from the deterministic probe, stable):
  regions=62 loss=0,10,5,10,8,20,10,2,1,2,2,75,2,8,36,10
  guards: interior 64/64 2..5, countdown 5..10, quota refused, dialogues 1
  outcomes: fine 2740/187, prison clock 16156800, escape court=false,
  bribe 200 clears / 199 refuses, death None + Executed flag
  ok-rows=20 negative=3
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
GATE = "M42/W42-4"
PROBE = "PROBE-SYS-LAW-001"
SLUG = "sys-law-001"
BINARY = ROOT / "target" / "debug" / "daggerfall-app"
ARTIFACTS = ROOT / ".sintergic" / "artifacts" / "m42-sys" / SLUG
CR = "CR-b434f896-2fe9-4750-b70e-9b748370e6c3"
HEADER_PREFIX = (
    "PROBE-SYS-LAW-001 - per-region record, legal reputation, guards, "
    f"five outcomes (M42/W42-4, {CR})"
)

REGIONS = 62
REPLOSS = "0,10,5,10,8,20,10,2,1,2,2,75,2,8,36,10"
INTERIOR_HITS = "64/64"
GUARDS_RANGE = "2..5"
COUNTDOWN_RANGE = "5..10"
FINE_PAID = 2740
PRISON_LEFT = 187
PRISON_DAYS = 187
PRISON_CLOCK = 16156800
BRIBE_PRICE = "Some(200)"
OK_ROWS = 20

REGIONS_LINE = re.compile(r"^REGIONS count=(\d+) crime=(\d+) arrested=(\S+) in_prison=(\S+)$")
REGION_LINE = re.compile(r"^REGION law0_legal=(-?\d+) clamp_lo=(-?\d+) clamp_hi=(-?\d+) flags0=(\d+)$")
REPLOSS_LINE = re.compile(r"^REPLOSS ([\d,]+)$")
LOWER_LINE = re.compile(r"^LOWER crime=(\d+) legal_after=(-?\d+) people_faction=(\S+) people_delta=(-?\d+)$")
RAISE_LINE = re.compile(r"^RAISE half=(\d+) gain=(\d+) legal_after=(-?\d+) people_delta=(-?\d+)$")
GUARDS_LINE = re.compile(
    r"^GUARDS interior_hits=(\S+) guards=(\S+) countdown=(\S+) quota_refused=(\S+) dialogues=(\d+)$"
)
FINE_LINE = re.compile(
    r"^FINE outcome=(\S+) seed=(\d+) fine=(\d+) paid=(\d+) days=(\d+) left=(\d+) steps=(\d+) identity=(\S+)$"
)
PRISON_LINE = re.compile(
    r"^PRISON outcome=(\S+) days=(-?\d+) ended=(\S+) clock=(\d+) clock_ok=(\S+)$"
)
ESCAPE_LINE = re.compile(
    r"^ESCAPE outcome=(\S+) legal=(-?\d+) voluntary=(\S+) court=(\S+) health=(\d+) escape=(\S+)$"
)
BRIBE_LINE = re.compile(
    r"^BRIBE outcome=(\S+) price=(\S+) paid200=(\S.*) paid199=(\S.*) rule=(\S+)$"
)
DEATH_LINE = re.compile(
    r"^DEATH outcome=(\S+) dead_surrender_none=(\S+) executed_flag=(\S+)$"
)
NEGATIVE_LINE = re.compile(r"^NEGATIVE (\S+) reason=(\S+)")
OK_LINE = re.compile(r"^\[OK\] (\S+) - (.*)$")
CITATION = re.compile(r"\b(?:src|spec|rule)=\S+")
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


def region_record_rule(lines: list[str]) -> tuple[bool, str]:
    regs = [REGIONS_LINE.match(l) for l in prefixed(lines, "REGIONS ")]
    reg0 = [REGION_LINE.match(l) for l in prefixed(lines, "REGION ")]
    if len(regs) != 1 or regs[0] is None:
        return False, f"regions-rows={len(regs)}"
    if len(reg0) != 1 or reg0[0] is None:
        return False, f"region-rows={len(reg0)}"
    count, crime, arrested, in_prison = regs[0].groups()
    legal, lo, hi, flags = reg0[0].groups()
    ok = (
        int(count) == REGIONS
        and crime == "0"
        and arrested == "false"
        and in_prison == "false"
        and legal == "0"
        and lo == "-100"
        and hi == "100"
        and flags == "0"
    )
    return ok, f"regions={count} clamp={lo}..{hi}"


def rep_loss_rule(lines: list[str]) -> tuple[bool, str]:
    rows = [REPLOSS_LINE.match(l) for l in prefixed(lines, "REPLOSS ")]
    if len(rows) != 1 or rows[0] is None:
        return False, f"reps-rows={len(rows)}"
    ok = rows[0].group(1) == REPLOSS
    return ok, f"loss={rows[0].group(1)[:20]}.."


def rep_propagate_rule(lines: list[str]) -> tuple[bool, str]:
    lower = [LOWER_LINE.match(l) for l in prefixed(lines, "LOWER ")]
    raised = [RAISE_LINE.match(l) for l in prefixed(lines, "RAISE ")]
    if len(lower) != 1 or lower[0] is None:
        return False, "no-LOWER"
    if len(raised) != 1 or raised[0] is None:
        return False, "no-RAISE"
    crime, after, fac, delta = lower[0].groups()
    half, gain, rafter, rdelta = raised[0].groups()
    ok = (
        crime == "5"
        and after == "-20"
        and fac == "Some(77)"
        and delta == "-10"
        and half == "10"
        and gain == "9"
        and rafter == "-11"
        and rdelta == "4"
    )
    return ok, f"murder->people77 after={after} raise={rafter}"


def guards_rule(lines: list[str]) -> tuple[bool, str]:
    rows = [GUARDS_LINE.match(l) for l in prefixed(lines, "GUARDS ")]
    if len(rows) != 1 or rows[0] is None:
        return False, f"guards-rows={len(rows)}"
    hits, grange, crange, quota, dialogues = rows[0].groups()
    ok = (
        hits == INTERIOR_HITS
        and grange == GUARDS_RANGE
        and crange == COUNTDOWN_RANGE
        and quota == "true"
        and dialogues == "1"
    )
    return ok, f"interior={hits} range={grange} countdown={crange} dialogues={dialogues}"


def fine_halving_rule(lines: list[str]) -> tuple[bool, str]:
    rows = [FINE_LINE.match(l) for l in prefixed(lines, "FINE ")]
    if len(rows) != 1 or rows[0] is None:
        return False, f"fine-rows={len(rows)}"
    outcome, _seed, fine, paid, _days, left, _steps, identity = rows[0].groups()
    ok = (
        outcome == "Fine"
        and int(paid) == FINE_PAID
        and int(left) == PRISON_LEFT
        and int(paid) == int(fine) >> 1
        and identity == "true"
    )
    return ok, f"paid={paid} left={left} (halved)"


def prison_clock_rule(lines: list[str]) -> tuple[bool, str]:
    rows = [PRISON_LINE.match(l) for l in prefixed(lines, "PRISON ")]
    if len(rows) != 1 or rows[0] is None:
        return False, f"prison-rows={len(rows)}"
    outcome, days, ended, clock, clock_ok = rows[0].groups()
    ok = (
        outcome == "Prison"
        and int(days) == PRISON_DAYS
        and ended == "true"
        and int(clock) == PRISON_CLOCK
        and clock_ok == "true"
    )
    return ok, f"days={days} clock={clock}"


def escape_rule(lines: list[str]) -> tuple[bool, str]:
    rows = [ESCAPE_LINE.match(l) for l in prefixed(lines, "ESCAPE ")]
    if len(rows) != 1 or rows[0] is None:
        return False, f"escape-rows={len(rows)}"
    outcome, _legal, _vol, court, health, esc = rows[0].groups()
    ok = outcome == "Escape" and court == "false" and health == "1" and esc == "true"
    return ok, f"court={court} health={health}"


def bribe_rule(lines: list[str]) -> tuple[bool, str]:
    rows = [BRIBE_LINE.match(l) for l in prefixed(lines, "BRIBE ")]
    if len(rows) != 1 or rows[0] is None:
        return False, f"bribe-rows={len(rows)}"
    outcome, price, p200, p199, rule = rows[0].groups()
    ok = (
        outcome == "Bribe"
        and price == BRIBE_PRICE
        and p200.startswith("Some((true, 200, 0))")
        and p199.startswith("Some((false, 13))")
        and rule == "SR-42-4-1"
    )
    return ok, f"price={price} 200-clears/199-refuses declared"


def death_rule(lines: list[str]) -> tuple[bool, str]:
    rows = [DEATH_LINE.match(l) for l in prefixed(lines, "DEATH ")]
    if len(rows) != 1 or rows[0] is None:
        return False, f"death-rows={len(rows)}"
    outcome, none, flagged = rows[0].groups()
    ok = outcome == "Death" and none == "true" and flagged == "true"
    return ok, f"surrender-none={none} executed-flag={flagged}"


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
        for raw in re.findall(r"(?:src|spec)=(\S+)", match.group(2)):
            if not path_exists(raw):
                missing.append(f"{match.group(1)}:{raw}")
    total = len(rows)
    return (not missing and total > 0), f"ok-rows={total} uncited={','.join(missing[:6]) or 'none'}"


def negative_rule(lines: list[str]) -> tuple[bool, str]:
    rows = [NEGATIVE_LINE.match(line) for line in prefixed(lines, "NEGATIVE ")]
    if len(rows) < 3 or any(r is None for r in rows):
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
        Rule("region-record", *region_record_rule(lines_one)),
        Rule("rep-loss-table", *rep_loss_rule(lines_one)),
        Rule("rep-propagate-raise", *rep_propagate_rule(lines_one)),
        Rule("guard-spawn", *guards_rule(lines_one)),
        Rule("fine-halving", *fine_halving_rule(lines_one)),
        Rule("prison-clock", *prison_clock_rule(lines_one)),
        Rule("escape-refused", *escape_rule(lines_one)),
        Rule("bribe-priced", *bribe_rule(lines_one)),
        Rule("death-executed", *death_rule(lines_one)),
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
        "regions": REGIONS,
        "outcomes": 5,
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
            [str(binary), "--probe", "SYS-LAW-001"],
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


def mutate_region_mismatch(captures: list[Capture]) -> tuple[list[Capture], str]:
    def mutate(lines: list[str]) -> list[str]:
        for index, line in enumerate(lines):
            if line.startswith("REGIONS "):
                lines[index] = line.replace("count=62", "count=61", 1)
                return lines
        raise AssertionError("no REGIONS line")

    return replace_lines(captures, mutate), "REGIONS count 62 -> 61"


def mutate_reploss_mismatch(captures: list[Capture]) -> tuple[list[Capture], str]:
    def mutate(lines: list[str]) -> list[str]:
        for index, line in enumerate(lines):
            if line.startswith("REPLOSS "):
                lines[index] = line.replace(",20,10,2,", ",20,11,2,", 1)
                return lines
        raise AssertionError("no REPLOSS line")

    return replace_lines(captures, mutate), "REPLOSS Murder 20 -> 21"


def mutate_guards_mismatch(captures: list[Capture]) -> tuple[list[Capture], str]:
    def mutate(lines: list[str]) -> list[str]:
        for index, line in enumerate(lines):
            if line.startswith("GUARDS "):
                lines[index] = line.replace("interior_hits=64/64", "interior_hits=63/64", 1)
                return lines
        raise AssertionError("no GUARDS line")

    return replace_lines(captures, mutate), "interior_hits 64/64 -> 63/64"


def mutate_fine_mismatch(captures: list[Capture]) -> tuple[list[Capture], str]:
    def mutate(lines: list[str]) -> list[str]:
        for index, line in enumerate(lines):
            if line.startswith("FINE "):
                lines[index] = line.replace("paid=2740", "paid=2741", 1)
                return lines
        raise AssertionError("no FINE line")

    return replace_lines(captures, mutate), "FINE paid 2740 -> 2741"


def mutate_prison_mismatch(captures: list[Capture]) -> tuple[list[Capture], str]:
    def mutate(lines: list[str]) -> list[str]:
        for index, line in enumerate(lines):
            if line.startswith("PRISON "):
                lines[index] = line.replace("clock=16156800", "clock=16156799", 1)
                return lines
        raise AssertionError("no PRISON line")

    return replace_lines(captures, mutate), "PRISON clock 16156800 -> 16156799"


def mutate_bribe_mismatch(captures: list[Capture]) -> tuple[list[Capture], str]:
    def mutate(lines: list[str]) -> list[str]:
        for index, line in enumerate(lines):
            if line.startswith("BRIBE "):
                lines[index] = line.replace("price=Some(200)", "price=Some(199)", 1)
                return lines
        raise AssertionError("no BRIBE line")

    return replace_lines(captures, mutate), "BRIBE price 200 -> 199"


def mutate_digest_tamper(captures: list[Capture]) -> tuple[list[Capture], str]:
    def mutate(lines: list[str]) -> list[str]:
        for index, line in enumerate(lines):
            if DIGEST_LINE.match(line):
                lines[index] = "DIGEST " + "0" * 64
                return lines
        raise AssertionError("no DIGEST line")

    return replace_lines(captures, mutate), "DIGEST -> 64 zeros"


FALSIFIERS = {
    "region-mismatch": ("region-record", mutate_region_mismatch),
    "reploss-mismatch": ("rep-loss-table", mutate_reploss_mismatch),
    "guards-mismatch": ("guard-spawn", mutate_guards_mismatch),
    "fine-mismatch": ("fine-halving", mutate_fine_mismatch),
    "prison-mismatch": ("prison-clock", mutate_prison_mismatch),
    "bribe-mismatch": ("bribe-priced", mutate_bribe_mismatch),
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
