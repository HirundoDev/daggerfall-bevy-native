#!/usr/bin/env python3
"""SYS-DIALOGUE-001 - gate of M42/W42-2: dialogue topics per faction and region.

Two separate processes, same binary
-----------------------------------
1. `cargo build -p daggerfall-app` once (`--skip-build` reuses the binary).
2. `PROBE-SYS-DIALOGUE-001` runs in **two independent processes** (`PROBE_RUN=1`
   and `PROBE_RUN=2`; the probe never prints that value, so an environment
   dependence shows up as a stdout difference).
3. Both captures must exit 0, contain `VEREDICTO: PASS`, be **byte-identical**
   (`cmp` + equal `sha256`), and the probe's own `DIGEST` line must equal the
   `sha256` of every byte printed before it.

Density rules the gate enforces (each one is built to fail)
-----------------------------------------------------------
* `header` -- first line declares the probe, the cycle and the CR.
* `topics-faction` -- exactly 3 `TOPIC` rows (npc 0..2, factions 10/11/12,
  region 17), pairwise distinct digests (topics differ per faction).
* `region-differs` -- `REGION` pins 17 vs 45 disagree (region pin refuses
  out-of-region quest topics).
* `rumor-retail` -- `RUMOR records=31 mill=5 spoken_with_text=5`, first text
  bytes > 0 (every served answer is retail text: RUMOR.DAT bytes or a
  TEXT.RSC id that resolves in the user's file).
* `context-closed` -- `CONTEXT keys>=1` (every emitted key is a member;
  invented keys refused, 0 out-of-context answers).
* `citation-present` -- **every** `[OK]` row carries `src=` or `spec=`.
* `negative-paths` -- every `NEGATIVE` row names a `reason=` path that exists
  on disk (a NEGATIVE without its evidence path is a FAIL).
* `no-fail-rows`, `verdict-last`, `digest-integrity`, `probe-digest`.

Falsifiers (each mutates the **capture**, never the repo, requires its own rule
to go RED, then re-measures the restored captures and requires them green):
`topic-missing`, `region-clash`, `rumor-zero`, `context-empty`,
`citation-empty`, `negative-without-path`, `digest-tamper`.

The probe reads **retail bytes**: `TEXT.RSC` + `RUMOR.DAT` via Mode A
(`$DF_APP_ROOT` or `$HOME/Games/Daggerfall/app`); unreadable file exits 2
with `VERDICT: PROBE-SYS-DIALOGUE-001 NO DATA` (same custody as M26).

Artifacts go to `.sintergic/artifacts/m42-sys/sys-dialogue-001/` (gitignored).

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
GATE = "M42/W42-2"
PROBE = "PROBE-SYS-DIALOGUE-001"
SLUG = "sys-dialogue-001"
BINARY = ROOT / "target" / "debug" / "daggerfall-app"
ARTIFACTS = ROOT / ".sintergic" / "artifacts" / "m42-sys" / SLUG
CR = "CR-b434f896-2fe9-4750-b70e-9b748370e6c3"
HEADER_PREFIX = (
    f"{PROBE} - dialogue topics per faction and region, "
    f"retail rumors (M42/W42-2, {CR})"
)

FACTIONS: tuple[int, ...] = (10, 11, 12)
REGION_A = 17
REGION_B = 45


TOPIC_LINE = re.compile(
    r"^TOPIC npc=(\d+) faction=(\d+) region=(\d+) topics=(\d+) digest=([0-9a-f]{16})$"
)
REGION_LINE = re.compile(
    r"^REGION pin=(\d+) topics=(\d+) digest=([0-9a-f]{16})$"
)
RUMOR_LINE = re.compile(
    r"^RUMOR records=(\d+) mill=(\d+) spoken_with_text=(\d+) regions=(\d+)$"
)
RUMOR_FIRST = re.compile(r"^RUMOR first_text_bytes=(\d+)$")
CONTEXT_LINE = re.compile(r"^CONTEXT keys=(\d+) digest=([0-9a-f]{16})$")
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


def topics_rule(lines: list[str]) -> tuple[bool, str]:
    rows = [TOPIC_LINE.match(line) for line in prefixed(lines, "TOPIC ")]
    if len(rows) != len(FACTIONS) or not all(rows):
        bad = sum(1 for row in rows if row is None)
        return False, f"rows={len(rows)} malformed={bad}"
    seen: dict[str, None] = {}
    for match in rows:  # type: ignore[union-attr]
        npc, faction, region = (
            int(match.group(1)),
            int(match.group(2)),
            int(match.group(3)),
        )
        topics = int(match.group(4))
        if not 0 <= npc < len(FACTIONS):
            return False, f"npc={npc} out-of-range"
        if faction != FACTIONS[npc]:
            return False, f"npc={npc} faction={faction} want={FACTIONS[npc]}"
        if region != REGION_A:
            return False, f"npc={npc} region={region} want={REGION_A}"
        if topics < 1:
            return False, f"npc={npc} empty-topics"
        seen[match.group(5)] = None
    if len(seen) != len(rows):
        return False, "digests-not-distinct"
    return True, f"rows={len(rows)} factions={','.join(map(str, FACTIONS))} distinct={len(seen)}"


def region_rule(lines: list[str]) -> tuple[bool, str]:
    rows = [REGION_LINE.match(line) for line in prefixed(lines, "REGION ")]
    if len(rows) != 2 or not all(rows):
        return False, f"lines={len(rows)}"
    pins = [int(m.group(1)) for m in rows]  # type: ignore[union-attr]
    digests = [m.group(3) for m in rows]  # type: ignore[union-attr]
    if sorted(pins) != [REGION_A, REGION_B]:
        return False, f"pins={pins}"
    if digests[0] == digests[1]:
        return False, "pins-agree"
    return True, f"pin{REGION_A}-vs-pin{REGION_B} differ"


def rumor_rule(lines: list[str]) -> tuple[bool, str]:
    rows = [RUMOR_LINE.match(line) for line in prefixed(lines, "RUMOR records=")]
    if len(rows) != 1 or rows[0] is None:
        return False, f"lines={len(rows)}"
    records, mill, spoken, regions = (int(v) for v in rows[0].groups())
    first = [RUMOR_FIRST.match(line) for line in prefixed(lines, "RUMOR first_text_bytes=")]
    if len(first) != 1 or first[0] is None:
        return False, "no-first-text"
    first_bytes = int(first[0].group(1))
    ok = records > 0 and mill > 0 and spoken > 0 and regions > 0 and first_bytes > 0
    return ok, f"records={records} mill={mill} spoken={spoken} regions={regions} bytes={first_bytes}"


def context_rule(lines: list[str]) -> tuple[bool, str]:
    rows = [CONTEXT_LINE.match(line) for line in prefixed(lines, "CONTEXT ")]
    if len(rows) != 1 or rows[0] is None:
        return False, f"lines={len(rows)}"
    keys = int(rows[0].group(1))
    if keys < 1:
        return False, "empty-context"
    return True, f"keys={keys} closed"


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
        Rule("topics-faction", *topics_rule(lines_one)),
        Rule("region-differs", *region_rule(lines_one)),
        Rule("rumor-retail", *rumor_rule(lines_one)),
        Rule("context-closed", *context_rule(lines_one)),
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
        "factions": list(FACTIONS),
        "regions": [REGION_A, REGION_B],
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
                "topic_rows": len(prefixed(lines_of(capture), "TOPIC ")),
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
                f"ok-rows={capture['ok_rows']} topics={capture['topic_rows']} "
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
            [str(binary), "--probe", "SYS-DIALOGUE-001"],
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


def mutate_topic_missing(captures: list[Capture]) -> tuple[list[Capture], str]:
    note: list[str] = []

    def mutate(lines: list[str]) -> list[str]:
        new, removed = drop_first(lines, "TOPIC npc=1 ")
        note.append(removed.split(" ")[2])
        return new

    return replace_lines(captures, mutate), "removed one TOPIC npc=1 row"


def mutate_region_clash(captures: list[Capture]) -> tuple[list[Capture], str]:
    note: list[str] = []

    def mutate(lines: list[str]) -> list[str]:
        pins: dict[str, str] = {}
        for line in lines:
            match = REGION_LINE.match(line)
            if match:
                pins[match.group(1)] = match.group(3)
        assert str(REGION_A) in pins and str(REGION_B) in pins, "no REGION pair"
        for index, line in enumerate(lines):
            if line.startswith(f"REGION pin={REGION_B} "):
                lines[index] = re.sub(
                    r"digest=[0-9a-f]{16}",
                    f"digest={pins[str(REGION_A)]}",
                    line,
                    count=1,
                )
                note.append(f"pin {REGION_B} digest := pin {REGION_A} digest")
                return lines
        raise AssertionError("no REGION pin=45")

    return replace_lines(captures, mutate), f"pin {REGION_B} digest := pin {REGION_A} digest"


def mutate_rumor_zero(captures: list[Capture]) -> tuple[list[Capture], str]:
    note: list[str] = []

    def mutate(lines: list[str]) -> list[str]:
        for index, line in enumerate(lines):
            if line.startswith("RUMOR records="):
                lines[index] = re.sub(r"spoken_with_text=\d+", "spoken_with_text=0", line, count=1)
                note.append("spoken_with_text -> 0")
                return lines
        raise AssertionError("no RUMOR records line")

    return replace_lines(captures, mutate), "RUMOR spoken_with_text -> 0"


def mutate_context_empty(captures: list[Capture]) -> tuple[list[Capture], str]:
    note: list[str] = []

    def mutate(lines: list[str]) -> list[str]:
        for index, line in enumerate(lines):
            if line.startswith("CONTEXT "):
                lines[index] = re.sub(r"keys=\d+", "keys=0", line, count=1)
                note.append("CONTEXT keys -> 0")
                return lines
        raise AssertionError("no CONTEXT line")

    return replace_lines(captures, mutate), "CONTEXT keys -> 0"


def mutate_citation_empty(captures: list[Capture]) -> tuple[list[Capture], str]:
    note: list[str] = []

    def mutate(lines: list[str]) -> list[str]:
        for index, line in enumerate(lines):
            if line.startswith("[OK] topics-per-faction "):
                lines[index] = CITATION.sub("", line).rstrip()
                note.append("topics-per-faction")
                return lines
        raise AssertionError("no topics-per-faction")

    return replace_lines(captures, mutate), "stripped src= from topics-per-faction"


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
    "topic-missing": ("topics-faction", mutate_topic_missing),
    "region-clash": ("region-differs", mutate_region_clash),
    "rumor-zero": ("rumor-retail", mutate_rumor_zero),
    "context-empty": ("context-closed", mutate_context_empty),
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
