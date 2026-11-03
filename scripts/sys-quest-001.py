#!/usr/bin/env python3
"""Gate for `PROBE-SYS-QUEST-001` (M42 / W42-5) — quest dispatch in depth.

**No golden count is written here.** The audit `DESIGN-AUDIT-276c7d94` measured
this gate blessing its own literals (`VARIANTS=89`, `EXECUTED=79`,
`NEGATIVE_A=10`, `BLOCK_A=90`, `OK_ROWS=116`): the probe and the gate agreed
because the gate had written down the probe's numbers. Every count below is now
read from the probe's real stdout and closed by an identity, so a probe that
stops measuring cannot keep the gate green (GAP-M42-04)::

    ENUM variants=89 walked=89 unique=89 cycle=0      # exhaustive match walk
    ORACLE variants=89 internal=10 request=78 success=1
    GAP triggers=9 verbs=7
    COVERAGE variants=89 executed=79 negative=10
    BLOCK a=90 b=10 c=7 d=4 e=5

    variants == walked == unique                        (GAP-M42-03)
    internal + request + success == variants            (oracle census)
    executed == request + success                       (engine observed split)
    negative == internal
    executed + negative == variants
    counted rows of block A == variants + 1             (the total row)
    counted rows of block B == triggers + 1
    counted rows of block C == verbs
    sum(counted rows) == declared BLOCK sum

`--skip-build` can no longer bless a pristine binary over a regressed tree
(GAP-M42-06): the probe bakes the measured sources into the binary and prints
their `sha256` fingerprint; here it is **recomputed from disk** and compared, so
source regressed + pristine binary + `--skip-build` is red.

Usage:
    python3 scripts/sys-quest-001.py               # build, run twice, verify
    python3 scripts/sys-quest-001.py --skip-build  # reuse ROOT/target/debug
    python3 scripts/sys-quest-001.py --binary P    # explicit binary
    python3 scripts/sys-quest-001.py --json        # machine summary
    python3 scripts/sys-quest-001.py --falsify M   # prove one rule can fail

Exit 0 = every rule OK; 1 = a rule failed; 2 = probe write failure.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
GATE = "sys-quest-001"
PROBE = "SYS-QUEST-001"
SLUG = "sys-quest-001"
ARTIFACTS = Path(os.environ.get("PROBE_ARTIFACTS", ROOT / "target" / "probe-artifacts"))
CR = "CR-765daa10-5084-4197-883d-dd626301e59e"

BIN_NAME = "daggerfall-app"
DEFAULT_BINARY = ROOT / "target" / "debug" / BIN_NAME

# ── the contract of the printed format ───────────────────────────────────
HEADER = re.compile(
    r"^PROBE-SYS-QUEST-001 - (\d+) quest variants via from_effect, triggers, verbs, "
    r"single_tick \(M42/W42-5, (CR-[0-9a-z-]+)\)$"
)
ROADMAP_LINE = "roadmap=docs/acknowledgments/roadmap-m18-m48.md:320 src=crates/behavior/src/quests.rs"
SINGLE_TICK = "crates/app/src/m29_probe.rs:1722"
ENGINE_SRC = "crates/behavior/src/quests.rs"

SOURCES_LINE = re.compile(
    r"^SOURCES sha256=([0-9a-f]{64}) files=(\d+) build=baked paths=(\S+) src=(\S+)$"
)
ENUM_LINE = re.compile(
    r"^ENUM variants=(\d+) walked=(\d+) unique=(\d+) cycle=([01]) src=(\S+)$"
)
ORACLE_LINE = re.compile(
    r"^ORACLE variants=(\d+) internal=(\d+) request=(\d+) success=(\d+) src=(\S+)$"
)
GAP_LINE = re.compile(r"^GAP triggers=(\d+) verbs=(\d+) src=(\S+)$")
COVERAGE_LINE = re.compile(
    r"^COVERAGE variants=(\d+) executed=(\d+) negative=(\d+) src=(\S+)$"
)
TOTAL_LINE = re.compile(
    r"^\[OK\] Q-A-TOTAL - want executed=(\d+) negative=(\d+) oracle=internal (\d+)/"
    r"request (\d+)/success (\d+) pos=(\w+) src=(\S+)$"
)
BLOCK_LINE = re.compile(r"^BLOCK a=(\d+) b=(\d+) c=(\d+) d=(\d+) e=(\d+)$")
INSTRUMENT_LINE = re.compile(
    r"^INSTRUMENT_ONLY delta_variants=(\d+) delta_requests=(\d+) delta_functions=(\d+) "
    r"productive_consumers=(\d+) functions=(\S+) consumers_measured=(\S+) src=(\S+)$"
)
NEW_CLASSES_LINE = re.compile(
    r"^NEW_CLASSES reached=(\d+)/(\d+) scanned=(\d+) rows=(\d+) measured=(\S+) src=(\S+)$"
)
NEW_CLASS_ROW = re.compile(
    r"^NOTE NEWCLASS (\w+) reachable=(\d+) type_ids=(\S+) role=(trigger|verb) src=(\S+)$"
)
OK_LINE = re.compile(r"^\[OK\] (\S+) - (\S.*)$")
FAIL_LINE = re.compile(r"^\[FAIL\] (\S+) got=(\S+) want=(\S+) - (\S.*)$")
VERDICT_LINE = re.compile(r"^VEREDICTO: (PASS|FAIL)$")
DIGEST_LINE = re.compile(r"^DIGEST ([0-9a-f]{64})$")
NEGATIVE_ROW = re.compile(r"^\[OK\] (\S*-NEGATIVE) - (\S.*)$")
CITATION = re.compile(r"src=(\S+)")
# Provenance of an inherited measurement (any lane CR, not necessarily this one).
MEASURED_CR = re.compile(r"^CR-[0-9a-z-]+$")


def rel(path: Path) -> str:
    """Path relative to ROOT when inside it (evidence may live outside)."""
    try:
        return str(path.relative_to(ROOT))
    except ValueError:
        return str(path)


def path_exists(src: str) -> bool:
    """`path:line` resolves when the path exists (line numbers may drift)."""
    return (ROOT / src.split(":")[0]).is_file()


def prefixed(lines: list[str], prefix: str) -> list[str]:
    return [line for line in lines if line.startswith(prefix)]


def _one(lines: list[str], pattern: re.Pattern[str], prefix: str):
    hits = [line for line in lines if line.startswith(prefix)]
    matched = [(index, m) for index, line in enumerate(hits) if (m := pattern.match(line))]
    if len(matched) != 1:
        return None, f"{prefix.strip()} lines={len(hits)} matched={len(matched)}"
    return matched[0][1], "ok"


def read_census(lines: list[str]) -> dict:
    census = {}
    census["sources"] = _one(lines, SOURCES_LINE, "SOURCES ")
    census["enum"] = _one(lines, ENUM_LINE, "ENUM ")
    census["oracle"] = _one(lines, ORACLE_LINE, "ORACLE ")
    census["gap"] = _one(lines, GAP_LINE, "GAP ")
    census["coverage"] = _one(lines, COVERAGE_LINE, "COVERAGE ")
    census["block"] = _one(lines, BLOCK_LINE, "BLOCK ")
    census["instrument"] = _one(lines, INSTRUMENT_LINE, "INSTRUMENT_ONLY ")
    census["new_classes"] = _one(lines, NEW_CLASSES_LINE, "NEW_CLASSES ")
    return census


def _fail_unreadable(detail: str) -> tuple[bool, str]:
    return False, f"unreadable census ({detail})"


# ── rules ────────────────────────────────────────────────────────────────
def rule_header(ctx: dict) -> tuple[bool, str]:
    m = HEADER.match(ctx["lines"][0]) if ctx["lines"] else None
    if m is None:
        return False, "header absent or malformed"
    count, cr = int(m.group(1)), m.group(2)
    enum_m, detail = ctx["census"]["enum"]
    if enum_m is None:
        return False, f"header binds no census ({detail})"
    variants = int(enum_m.group(1))
    return (
        count == variants and cr == CR,
        f"header variants={count} enum variants={variants} cr={cr}",
    )


def rule_roadmap(ctx: dict) -> tuple[bool, str]:
    present = ROADMAP_LINE in ctx["lines"]
    return present, "roadmap citation present" if present else "roadmap citation missing"


def rule_sources(ctx: dict) -> tuple[bool, str]:
    m, detail = ctx["census"]["sources"]
    if m is None:
        return _fail_unreadable(detail)
    printed, files, paths, src = m.group(1), int(m.group(2)), m.group(3), m.group(4)
    path_list = [p for p in paths.split(",") if p]
    if not path_exists(src):
        return False, f"src not on disk: {src}"
    if ENGINE_SRC not in path_list:
        return False, f"measured engine absent from fingerprint paths={path_list}"
    if files != len(path_list) or not path_list:
        return False, f"files={files} paths={len(path_list)}"
    parts = []
    for raw in path_list:
        target = ROOT / raw
        if not target.is_file():
            return False, f"fingerprinted source absent on disk: {raw}"
        parts.append(f"{raw}\u0000{hashlib.sha256(target.read_bytes()).hexdigest()}")
    recomputed = hashlib.sha256("\n".join(sorted(parts)).encode()).hexdigest()
    return (
        recomputed == printed,
        f"binary={printed[:16]} disk={recomputed[:16]} files={files}"
        + ("" if recomputed == printed else " → binary built from other bytes"),
    )


def rule_enum(ctx: dict) -> tuple[bool, str]:
    m, detail = ctx["census"]["enum"]
    if m is None:
        return _fail_unreadable(detail)
    variants, walked, unique, cycle, src = (
        int(m.group(1)),
        int(m.group(2)),
        int(m.group(3)),
        int(m.group(4)),
        m.group(5),
    )
    ok = (
        variants > 0
        and variants == walked == unique
        and cycle == 0
        and path_exists(src)
    )
    return ok, f"variants={variants} walked={walked} unique={unique} cycle={cycle}"


def rule_oracle(ctx: dict) -> tuple[bool, str]:
    oracle_m, oracle_detail = ctx["census"]["oracle"]
    enum_m, enum_detail = ctx["census"]["enum"]
    if oracle_m is None or enum_m is None:
        return _fail_unreadable(oracle_detail if oracle_m is None else enum_detail)
    variants, internal, request, success = (
        int(oracle_m.group(1)),
        int(oracle_m.group(2)),
        int(oracle_m.group(3)),
        int(oracle_m.group(4)),
    )
    src = oracle_m.group(5)
    ok = (
        variants == int(enum_m.group(1))
        and internal + request + success == variants
        and path_exists(src)
    )
    return ok, f"variants={variants} internal={internal} request={request} success={success}"


def rule_gap(ctx: dict) -> tuple[bool, str]:
    m, detail = ctx["census"]["gap"]
    if m is None:
        return _fail_unreadable(detail)
    triggers, verbs, src = int(m.group(1)), int(m.group(2)), m.group(3)
    enum_m, _ = ctx["census"]["enum"]
    cap = int(enum_m.group(1)) if enum_m else 0
    ok = triggers > 0 and verbs > 0 and triggers + verbs <= cap and path_exists(src)
    return ok, f"triggers={triggers} verbs={verbs} cap={cap}"


def rule_coverage(ctx: dict) -> tuple[bool, str]:
    """GAP-M42-04: the engine's observed split must be the oracle's split."""
    m, detail = ctx["census"]["coverage"]
    oracle_m, oracle_detail = ctx["census"]["oracle"]
    enum_m, enum_detail = ctx["census"]["enum"]
    if m is None or oracle_m is None or enum_m is None:
        return _fail_unreadable(detail if m is None else f"oracle {oracle_detail}")
    variants, executed, negative = int(m.group(1)), int(m.group(2)), int(m.group(3))
    src = m.group(4)
    oracle_request, oracle_success, oracle_internal = (
        int(oracle_m.group(3)),
        int(oracle_m.group(4)),
        int(oracle_m.group(2)),
    )
    ok = (
        variants == int(enum_m.group(1))
        and executed == oracle_request + oracle_success
        and negative == oracle_internal
        and executed + negative == variants
        and path_exists(src)
    )
    return ok, (
        f"variants={variants} executed={executed} negative={negative} "
        f"want executed={oracle_request + oracle_success} negative={oracle_internal}"
    )


def _block_count(lines: list[str], letter: str) -> int:
    return len(prefixed(lines, f"[OK] Q-{letter}-"))


def rule_block_a(ctx: dict) -> tuple[bool, str]:
    enum_m, detail = ctx["census"]["enum"]
    if enum_m is None:
        return _fail_unreadable(detail)
    want = int(enum_m.group(1)) + 1  # + the Q-A-TOTAL row
    got = _block_count(ctx["lines"], "A")
    return got == want, f"rows={got} want={want} (variants+total)"


def rule_block_b(ctx: dict) -> tuple[bool, str]:
    m, detail = ctx["census"]["gap"]
    if m is None:
        return _fail_unreadable(detail)
    want = int(m.group(1)) + 1  # + the pre-gap FactionReputation row
    got = _block_count(ctx["lines"], "B")
    return got == want, f"rows={got} want={want} (triggers+pre-gap)"


def rule_block_c(ctx: dict) -> tuple[bool, str]:
    m, detail = ctx["census"]["gap"]
    if m is None:
        return _fail_unreadable(detail)
    want = int(m.group(2))
    got = _block_count(ctx["lines"], "C")
    return got == want, f"rows={got} want={want} (verbs)"


def rule_block_d(ctx: dict) -> tuple[bool, str]:
    m, detail = ctx["census"]["block"]
    if m is None:
        return _fail_unreadable(detail)
    want = int(m.group(4))
    got = _block_count(ctx["lines"], "D")
    return got == want, f"rows={got} declared={want}"


def rule_block_e(ctx: dict) -> tuple[bool, str]:
    m, detail = ctx["census"]["block"]
    if m is None:
        return _fail_unreadable(detail)
    want = int(m.group(5))
    got = _block_count(ctx["lines"], "E")
    return got == want, f"rows={got} declared={want}"


def rule_block_total(ctx: dict) -> tuple[bool, str]:
    m, detail = ctx["census"]["block"]
    if m is None:
        return _fail_unreadable(detail)
    declared = sum(int(m.group(index)) for index in range(1, 6))
    counted = sum(_block_count(ctx["lines"], letter) for letter in "ABCDE")
    return counted == declared, f"counted={counted} declared={declared}"


def rule_total_row(ctx: dict) -> tuple[bool, str]:
    m, detail = _one(ctx["lines"], TOTAL_LINE, "[OK] Q-A-TOTAL")
    if m is None:
        return False, f"total row absent ({detail})"
    coverage_m, coverage_detail = ctx["census"]["coverage"]
    if coverage_m is None:
        return _fail_unreadable(coverage_detail)
    executed, negative = int(m.group(1)), int(m.group(2))
    oracle_internal, oracle_request, oracle_success = (
        int(m.group(3)),
        int(m.group(4)),
        int(m.group(5)),
    )
    src = m.group(7)
    ok = (
        executed == int(coverage_m.group(2))
        and negative == int(coverage_m.group(3))
        and executed == oracle_request + oracle_success
        and negative == oracle_internal
        and path_exists(src)
    )
    return ok, f"row executed={executed} negative={negative} coverage=§{int(coverage_m.group(2))}/{int(coverage_m.group(3))}"


def rule_single_tick(ctx: dict) -> tuple[bool, str]:
    seen = any(SINGLE_TICK in line for line in ctx["lines"])
    exists = path_exists(SINGLE_TICK)
    return seen and exists, f"single_tick citation present={seen} resolves={exists}"


def rule_citation_present(ctx: dict) -> tuple[bool, str]:
    rows = [OK_LINE.match(line) for line in prefixed(ctx["lines"], "[OK] ")]
    rows = [row for row in rows if row]
    if not rows:
        return False, "no evidence rows"
    missing = [row.group(1) for row in rows if not CITATION.search(row.group(2))]
    unresolved = [
        (row.group(1), CITATION.search(row.group(2)).group(1))
        for row in rows
        if CITATION.search(row.group(2)) and not path_exists(CITATION.search(row.group(2)).group(1))
    ]
    ok = not missing and not unresolved
    return ok, (
        f"rows={len(rows)} missing_src={len(missing)} unresolved_src={len(unresolved)}"
        + (f" first_unresolved={unresolved[0]}" if unresolved else "")
    )


def rule_negative_proven(ctx: dict) -> tuple[bool, str]:
    coverage_m, detail = ctx["census"]["coverage"]
    if coverage_m is None:
        return _fail_unreadable(detail)
    negative_all = [NEGATIVE_ROW.match(line) for line in ctx["lines"]]
    negative_all = [row for row in negative_all if row]
    engine_negative = int(coverage_m.group(3))
    qa_negative = [row for row in negative_all if row.group(1).startswith("Q-A-")]
    block_rows = [row for row in negative_all if not row.group(1).startswith("Q-A-")]
    cited = all(CITATION.search(row.group(2)) for row in negative_all)
    ok = len(qa_negative) == engine_negative and cited
    return ok, (
        f"rows={len(negative_all)} block_a={len(qa_negative)} coverage.negative="
        f"{engine_negative} non_a={len(block_rows)} cited={cited}"
    )


def rule_no_fail_rows(ctx: dict) -> tuple[bool, str]:
    fails = [line for line in ctx["lines"] if FAIL_LINE.match(line)]
    bad = [line for line in ctx["lines"] if line.startswith("[FAIL]") and not FAIL_LINE.match(line)]
    return not fails and not bad, f"[FAIL] rows={len(fails)} malformed={len(bad)}"


def rule_verdict_last(ctx: dict) -> tuple[bool, str]:
    """The verdict is the last line before the trailing `DIGEST`."""
    lines = [line for line in ctx["lines"] if line.strip()]
    m = VERDICT_LINE.match(lines[-2]) if len(lines) >= 2 else None
    return (
        m is not None and m.group(1) == "PASS",
        f"verdict line: {lines[-2][:48] if len(lines) >= 2 else 'absent'}",
    )


def rule_digest(ctx: dict) -> tuple[bool, str]:
    # Not filtered: the digest covers the printed bytes verbatim.
    lines = ctx["lines"]
    body = lines[:-1]
    expected = None
    for line in body:
        match = DIGEST_LINE.match(line)
        if match:
            expected = match.group(1)
    if expected is not None:
        return False, "a DIGEST line appears before the last line"
    digest = DIGEST_LINE.match(lines[-1]) if lines else None
    if digest is None:
        return False, "no trailing DIGEST"
    recomputed = hashlib.sha256(("\n".join(body) + "\n").encode()).hexdigest()
    return (
        recomputed == digest.group(1),
        f"recomputed={recomputed[:16]} printed={digest.group(1)[:16]}",
    )


def rule_probe_digest(ctx: dict) -> tuple[bool, str]:
    return (
        ctx["digest1"] == ctx["digest2"] and ctx["run1"] == ctx["run2"],
        f"probe={ctx['digest1'][:16]} run1={ctx['digest2'][:16]}",
    )


def rule_instrument_only(ctx: dict) -> tuple[bool, str]:
    """GAP-M42-08: the delta declares itself instrument-only, with 0 consumers."""
    m, detail = ctx["census"]["instrument"]
    if m is None:
        return False, f"declaration absent ({detail})"
    delta_variants, delta_requests, delta_functions = (
        int(m.group(1)),
        int(m.group(2)),
        int(m.group(3)),
    )
    consumers = int(m.group(4))
    functions = [f for f in m.group(5).split(",") if f]
    measured, src = m.group(6), m.group(7)
    gap_m, gap_detail = ctx["census"]["gap"]
    if gap_m is None:
        return _fail_unreadable(gap_detail)
    block_c = _block_count(ctx["lines"], "C")
    quests_bytes = (ROOT / ENGINE_SRC).read_bytes() if (ROOT / ENGINE_SRC).is_file() else b""
    declared = all(
        f"fn {name.split('::')[-1]}(".encode() in quests_bytes for name in functions
    )
    ok = (
        consumers == 0
        and delta_requests == block_c == int(gap_m.group(2))
        and delta_variants == int(gap_m.group(1)) + int(gap_m.group(2))
        and delta_functions == len(functions) > 0
        and declared
        and MEASURED_CR.match(measured) is not None
        and path_exists(src)
    )
    return ok, (
        f"delta_variants={delta_variants} delta_requests={delta_requests} block_c={block_c} "
        f"functions={delta_functions} declared_in_source={declared} consumers={consumers}"
    )


def rule_new_classes(ctx: dict) -> tuple[bool, str]:
    """GAP-M42-02 retired: 0/16 declared, measured per class, never claimed."""
    m, detail = ctx["census"]["new_classes"]
    if m is None:
        return False, f"declaration absent ({detail})"
    reached, total, scanned, rows = (int(m.group(i)) for i in range(1, 5))
    src = m.group(6)
    note_rows = [NEW_CLASS_ROW.match(line) for line in ctx["lines"]]
    note_rows = [row for row in note_rows if row]
    instrument_m, instrument_detail = ctx["census"]["instrument"]
    if instrument_m is None:
        return _fail_unreadable(instrument_detail)
    delta_variants = int(instrument_m.group(1))
    ok = (
        reached == 0
        and total == delta_variants
        and len(note_rows) == total
        and all(int(row.group(2)) == 0 for row in note_rows)
        and all(path_exists(row.group(5)) for row in note_rows)
        and scanned >= rows > 0
        and path_exists(src)
    )
    return ok, (
        f"reached={reached}/{total} declared={delta_variants} note_rows={len(note_rows)} "
        f"scanned={scanned} rows={rows}"
    )


RULES = (
    ("header-cr", rule_header),
    ("roadmap-src", rule_roadmap),
    ("sources-fingerprint", rule_sources),
    ("enum-census", rule_enum),
    ("oracle-census", rule_oracle),
    ("gap-census", rule_gap),
    ("coverage-identity", rule_coverage),
    ("block-a-engine", rule_block_a),
    ("block-b-triggers", rule_block_b),
    ("block-c-verbs", rule_block_c),
    ("block-d-single-tick", rule_block_d),
    ("block-e-negative", rule_block_e),
    ("block-total-identity", rule_block_total),
    ("total-row-identity", rule_total_row),
    ("single-tick-cited", rule_single_tick),
    ("citation-present", rule_citation_present),
    ("negative-proven", rule_negative_proven),
    ("no-fail-rows", rule_no_fail_rows),
    ("instrument-only-delta", rule_instrument_only),
    ("new-classes-retired", rule_new_classes),
    ("verdict-last", rule_verdict_last),
    ("digest-integrity", rule_digest),
    ("probe-digest", rule_probe_digest),
)


# ── execution ────────────────────────────────────────────────────────────
def head_commit() -> str:
    try:
        return subprocess.check_output(
            ["git", "-C", str(ROOT), "rev-parse", "HEAD"], text=True
        ).strip()
    except subprocess.CalledProcessError:
        return "unknown"


def build(binary: Path) -> None:
    subprocess.check_call(["cargo", "build", "-p", BIN_NAME], cwd=ROOT)
    if not binary.is_file():
        raise SystemExit(f"build produced no binary at {binary}")


def run_probe(binary: Path) -> tuple[str, int]:
    completed = subprocess.run(
        [str(binary), "--probe", PROBE],
        cwd=ROOT,
        capture_output=True,
        text=True,
    )
    return completed.stdout, completed.returncode


def verify(stdout: str, digest_run1: str, digest_run2: str, run1: str, run2: str):
    lines = stdout.splitlines()
    ctx = {
        "lines": lines,
        "run1": run1,
        "run2": run2,
        "digest1": digest_run1,
        "digest2": digest_run2,
        "census": read_census(lines),
    }
    return [(rule_id, rule(ctx)) for rule_id, rule in RULES]


def falsify(text: str, mode: str) -> tuple[str, str]:
    """Mutate the captured stdout to prove one rule can fail.

    Proves the *rule*, not the probe: the source-level control lives in the
    handoff (mutate the probe's own count → red).
    """
    lines = text.splitlines()

    def bump(line: str, pattern: str, group: int = 1) -> str:
        return re.sub(pattern, lambda m: m.group(0).replace(m.group(group), str(int(m.group(group)) + 1), 1), line, count=1)

    if mode == "coverage-mismatch":
        for index, line in enumerate(lines):
            if line.startswith("COVERAGE "):
                lines[index] = bump(line, r"executed=(\d+)")
                return "\n".join(lines) + "\n", "coverage executed +1 (identity must break)"
        return text, "no coverage line"
    if mode == "block-b-drop":
        for index, line in enumerate(lines):
            if line.startswith("[OK] Q-B-"):
                del lines[index]
                return "\n".join(lines) + "\n", "dropped one trigger row"
        return text, "no trigger row"
    if mode == "block-c-drop":
        for index, line in enumerate(lines):
            if line.startswith("[OK] Q-C-"):
                del lines[index]
                return "\n".join(lines) + "\n", "dropped one verb row"
        return text, "no verb row"
    if mode == "block-d-drop":
        for index, line in enumerate(lines):
            if line.startswith("[OK] Q-D-"):
                del lines[index]
                return "\n".join(lines) + "\n", "dropped one single_tick row"
        return text, "no single_tick row"
    if mode == "block-e-drop":
        for index, line in enumerate(lines):
            if line.startswith("[OK] Q-E-"):
                del lines[index]
                return "\n".join(lines) + "\n", "dropped one adversarial row"
        return text, "no adversarial row"
    if mode == "single-tick-mismatch":
        return text.replace(SINGLE_TICK, "crates/app/src/m29_probe.rs:9999"), "single_tick citation re-pointed"
    if mode == "header-mismatch":
        for index, line in enumerate(lines):
            if line.startswith("PROBE-SYS-QUEST-001 - "):
                lines[index] = bump(line, r"PROBE-SYS-QUEST-001 - (\d+)")
                return "\n".join(lines) + "\n", "header count +1 (census must disagree)"
        return text, "no header"
    if mode == "citation-strip":
        for index, line in enumerate(lines):
            if line.startswith("[OK] ") and "src=" in line:
                lines[index] = re.sub(r"src=\S+", "", line, count=1)
                return "\n".join(lines) + "\n", "stripped one citation"
        return text, "no cited row"
    if mode == "digest-tamper":
        for index, line in enumerate(lines):
            if line.startswith("[OK] "):
                lines[index] = line.replace("engine", "enginX", 1)
                return "\n".join(lines) + "\n", "tampered a row after the digest"
        return text, "no row"
    if mode == "sources-tamper":
        return re.sub(r"sha256=[0-9a-f]{64}", "sha256=" + "0" * 64, text, count=1), "sources sha zeroed"
    if mode == "instrument-clear":
        return "\n".join(l for l in lines if not l.startswith("INSTRUMENT_ONLY ")) + "\n", "INSTRUMENT_ONLY dropped"
    if mode == "noop":
        return text, "no mutation (harness self-test: must stay green)"
    if mode == "enum-drop":
        return "\n".join(l for l in lines if not l.startswith("ENUM ")) + "\n", "ENUM census dropped"
    return text, f"unknown mode {mode}"


FALSIFY_MODES = (
    "coverage-mismatch",
    "block-b-drop",
    "block-c-drop",
    "block-d-drop",
    "block-e-drop",
    "single-tick-mismatch",
    "header-mismatch",
    "citation-strip",
    "digest-tamper",
    "sources-tamper",
    "instrument-clear",
    "enum-drop",
    "noop",
)


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--skip-build", action="store_true", help="reuse the built binary")
    parser.add_argument("--binary", type=Path, default=DEFAULT_BINARY, help="probe binary")
    parser.add_argument("--json", action="store_true", help="machine summary")
    parser.add_argument("--falsify", choices=FALSIFY_MODES, help="mutate a captured run to prove a rule fails")
    args = parser.parse_args(argv)

    binary = args.binary.resolve()
    if not args.skip_build:
        build(binary)
    if not binary.is_file():
        raise SystemExit(f"binary not found: {binary} (drop --skip-build or pass --binary)")

    run1, exit1 = run_probe(binary)
    run2, exit2 = run_probe(binary)
    digest1 = hashlib.sha256(run1.encode()).hexdigest()
    digest2 = hashlib.sha256(run2.encode()).hexdigest()
    byte_identical = digest1 == digest2
    run_verdicts_ok = exit1 == 0 and exit2 == 0
    verdict = run1.splitlines()[-1] if run1 else ""

    rules = verify(run1, digest1, digest2, run1, run2)
    pre = (
        ("run1-verdict", (run_verdicts_ok, f"run1={exit1} run2={exit2}")),
        ("byte-identical", (byte_identical, f"run1={digest1[:16]} run2={digest2[:16]}")),
    )
    result = list(pre) + rules
    pre_ok = all(ok for _, (ok, _) in pre)
    rules_ok = all(ok for _, (ok, _) in rules)
    ok = pre_ok and rules_ok
    failures = [rule_id for rule_id, (rule_ok, _) in result if not rule_ok]

    ARTIFACTS.mkdir(parents=True, exist_ok=True)
    (ARTIFACTS / f"{SLUG}.stdout").write_text(run1)
    (ARTIFACTS / f"{SLUG}.run2.stdout").write_text(run2)
    (ARTIFACTS / f"{SLUG}.gate.json").write_text(
        json.dumps(
            {
                "gate": GATE,
                "probe": PROBE,
                "cr": CR,
                "binary": str(binary),
                "head": head_commit(),
                "verdict": verdict,
                "exit1": exit1,
                "exit2": exit2,
                "digest_run1": digest1,
                "digest_run2": digest2,
                "byte_identical": byte_identical,
                "ok": ok,
                "failures": failures,
                "rules": [
                    {"rule": rule_id, "ok": rule_ok, "detail": detail}
                    for rule_id, (rule_ok, detail) in result
                ],
            },
            indent=2,
            sort_keys=True,
        )
    )
    (ARTIFACTS / f"{SLUG}.rules.tsv").write_text(
        "\n".join(f"{'OK' if r else 'RED'}\t{rule_id}\t{detail}" for rule_id, (r, detail) in result)
        + "\n"
    )

    falsify_result = None
    coupled = {"digest-integrity", "probe-digest"}
    if args.falsify:
        mutated, detail = falsify(run1, args.falsify)
        mutated_rules = verify(mutated, digest1, hashlib.sha256(mutated.encode()).hexdigest(), run1, mutated)
        red = [rule_id for rule_id, (rule_ok, _) in mutated_rules if not rule_ok]
        # `digest-tamper` targets the digest rules themselves; every other
        # mutation is judged on the rules the digest does not couple.
        falsifier_ok = (
            coupled <= set(red)
            if args.falsify == "digest-tamper"
            else bool(set(red) - coupled)
        )
        falsify_result = {
            "mode": args.falsify,
            "mutation": detail,
            "verdict": "RED" if falsifier_ok else "STILL-GREEN",
            "red_rules": red,
            "still_green": [
                rule_id for rule_id, (rule_ok, _) in mutated_rules if rule_ok
            ],
        }
        # The self-test passes only when the mutation really turned a rule red.
        ok = ok and falsifier_ok
        (ARTIFACTS / f"{SLUG}.falsify.json").write_text(
            json.dumps(falsify_result, indent=2, sort_keys=True)
        )

    if args.json:
        census = read_census(run1.splitlines())
        for key, (match, _) in census.items():
            census[key] = list(match.groups()) if match else None
        print(
            json.dumps(
                {
                    "gate": GATE,
                    "probe": PROBE,
                    "cr": CR,
                    "head": head_commit(),
                    "ok": ok,
                    "exit": 0 if ok else 1,
                    "verdict": verdict,
                    "digest_run1": digest1,
                    "digest_run2": digest2,
                    "byte_identical": byte_identical,
                    "census": census,
                    "failures": failures,
                    "falsify": falsify_result,
                    "rules": [
                        {"rule": rule_id, "ok": rule_ok, "detail": detail}
                        for rule_id, (rule_ok, detail) in result
                    ],
                    "evidence": sorted(rel(path) for path in ARTIFACTS.glob(f"{SLUG}.*")),
                    "head_commit": head_commit(),
                },
                indent=2,
                sort_keys=True,
            )
        )
    else:
        print(f"gate={GATE} probe={PROBE} cr={CR} head={head_commit()[:12]}")
        for rule_id, (rule_ok, detail) in result:
            print(f"[{'OK' if rule_ok else 'RED'}] {rule_id} - {detail}")
        if falsify_result:
            print(f"[FALSIFY {falsify_result['verdict']}] {falsify_result['mode']} - {falsify_result['mutation']}")
            print(f"  red={','.join(falsify_result['red_rules']) or '-'} still_green={','.join(falsify_result['still_green']) or '-'}")
        print(f"digest run1={digest1} run2={digest2}")
        print(f"evidence {rel(ARTIFACTS)}/{SLUG}.*")
        print(f"VEREDICTO-GATE: {'PASS' if ok else 'FAIL'}")
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())
