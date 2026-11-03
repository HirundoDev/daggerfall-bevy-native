#!/usr/bin/env python3
"""SYS-STATS-001 - gate of M41/W41-1: stats, resistances and birthsigns in depth.

Two separate processes, same binary
-----------------------------------
1. `cargo build -p daggerfall-app` once (`--skip-build` reuses the binary).
2. `PROBE-SYS-STATS-001` runs in **two independent processes** (`PROBE_RUN=1`
   and `PROBE_RUN=2`; the probe never prints that value, so an environment
   dependence shows up as a stdout difference).
3. Both captures must exit 0, contain `VEREDICTO: PASS`, be **byte-identical**
   (`cmp` + equal `sha256`), and the probe's own `DIGEST` line must equal the
   `sha256` of every byte printed before it.

Density rules the gate enforces (each one is built to fail)
-----------------------------------------------------------
* `header` -- first line declares the probe, the cycle and the CR.
* `attribute-set` / `attribute-count` -- exactly the 8 `DFCareer.Stats` in
  canonical order, and the count the probe itself declares.
* `derived-census` -- one `DERIVED` row per derived value, no duplicate names,
  and no golden vector naming a value that was never derived.
* `resistance-census` -- one `RESISTANCE` row for every type of the pinned
  model (`DFCareer.EffectFlags`, `DFCareer.cs:398-408`) with a tolerance of
  the pinned vocabulary.
* `birthsign-census` -- 12 `BIRTHSIGN` rows, ordinals 0..11 ascending, names
  equal to the frozen table transcribed from `DaggerfallDateTime.cs:256-270`.
* `mod-census` / `census-totals` / `census-ids-unique` -- the `PlayerMod`
  census: 6 unique codes, ordinals 0..5, unique evidence ids, and the
  `CENSUS advantages=.. disadvantages=.. total=..` line equal to the ids the
  probe actually emitted.
* `golden-coverage` -- every derived value and every resistance type has at
  least one golden vector (`GV-STAT`/`GV-RES`), and every golden vector names
  a value that exists.
* `citation-present` -- **every** `[OK]` row carries `src=` or `spec=`;
  a golden row without a citation is a FAIL.
* `negative-paths` -- every `NEGATIVE` row names a `reason=` path that exists
  on disk (a NEGATIVE without its evidence path is a FAIL).
* `no-fail-rows`, `verdict-last`, `digest-integrity`.

Falsifiers (each mutates the **capture**, never the repo, requires its own rule
to go RED, then re-measures the restored captures and requires them green):
`attribute-missing`, `resistance-missing`, `citation-empty`, `census-mismatch`,
`derived-without-golden`, `negative-without-path`, `digest-tamper`.

The probe reads **0 retail bytes**: the career fixture is a synthetic 74-byte
record validated by `Career::parse`, so this gate has no ARENA2 precondition
and never SKIPs for a missing retail tree.

Artifacts go to `.sintergic/artifacts/m41-sys/sys-stats-001/` (gitignored).

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
GATE = "M41/W41-1"
PROBE = "PROBE-SYS-STATS-001"
SLUG = "sys-stats-001"
BINARY = ROOT / "target" / "debug" / "daggerfall-app"
ARTIFACTS = ROOT / ".sintergic" / "artifacts" / "m41-sys" / SLUG
CR = "CR-8b5f2d61-4c7a-4e93-9f18-6d0c2ae74155"
HEADER_PREFIX = f"{PROBE} - stats/resistances/birthsigns in depth (M41/W41-1, {CR})"

# Frozen tables: an **independent** transcription of the pinned source, so a
# drift between the probe, the crate and the corpus cannot pass silently.
ATTRIBUTES: tuple[str, ...] = (
    "Strength",
    "Intelligence",
    "Willpower",
    "Agility",
    "Endurance",
    "Personality",
    "Speed",
    "Luck",
)
BIRTHSIGNS: tuple[str, ...] = (
    "TheRitual",
    "TheLover",
    "TheLord",
    "TheMage",
    "TheShadow",
    "TheSteed",
    "TheApprentice",
    "TheWarrior",
    "TheLady",
    "TheTower",
    "TheAtronach",
    "TheThief",
)
RESISTANCE_TYPES: tuple[str, ...] = (
    "Paralysis",
    "Magic",
    "Poison",
    "Fire",
    "Frost",
    "Shock",
    "Disease",
)
TOLERANCES: frozenset[str] = frozenset(
    {"Normal", "Immune", "Resistant", "LowTolerance", "CriticalWeakness"}
)
MOD_CODES: tuple[str, ...] = ("RP", "RD", "MR", "TH", "FT", "RR")

ATTRIBUTE_LINE = re.compile(r"^ATTRIBUTE (\S+) (\d+) short=(\S+) (.*)$")
DERIVED_LINE = re.compile(r"^DERIVED (\S+) (-?\d+) (.*)$")
RESISTANCE_LINE = re.compile(r"^RESISTANCE (\S+) (\S+)(.*)$")
BIRTHSIGN_LINE = re.compile(r"^BIRTHSIGN (\d+) (\S+) (\S+) (.*)$")
MOD_LINE = re.compile(r"^MOD (\S+) (\d+) (\S+) field=(\S+) (.*)$")
CENSUS_LINE = re.compile(
    r"^CENSUS advantages=(\d+) disadvantages=(\d+) total=(\d+)(?:\s|$)"
)
NEGATIVE_LINE = re.compile(r"^NEGATIVE (\S+) reason=(\S+)")
OK_LINE = re.compile(r"^\[OK\] (\S+) - (.*)$")
FAIL_LINE = re.compile(r"^FAIL (\S+) (.*)$")
GV_STAT_NAME = re.compile(r"\bname=(\S+)")
GV_RES_TYPE = re.compile(r"\btype=(\S+)")
CITATION = re.compile(r"\b(src|spec)=\S+")
CITATION_RESOLVE = re.compile(r"\b(?:src|spec|carry)=(\S+)")
#  Three derived values carry a snake_case project name but the
#  cited authority uses its own identifier: the row declares the alias.
SYMBOL_ALIASES = {
    "max_magicka": ("SpellPoints", "max_magicka"),
    "max_fatigue": ("Fatiga máxima", "max_fatigue"),
    "max_breath": ("Aire máximo", "max_breath"),
}

#  ── citation resolved against the tree (CR-4171fb19 · GAP-M41-02) ───────────
#  `CITATION` only proved presence and shape: today the file, the range,
#  the cited text (with a declaration: not blank, not a comment, not a bare
#  brace) and the symbol the row declares are resolved, following the block the citation
#  opens when it points at the opener line.
CITE_SPEC = re.compile(
    r"^(?P<path>[\w./+-]+\.(?:rs|md|py|json|toml|cs)):(?P<start>\d+)(?:-(?P<end>\d+))?$"
)
CITE_MAX_SPAN = 400
CITE_OPEN_BLOCK = 80
DECORATION_LINE = re.compile(r"^\s*(?:$|//|/\*|\*|\}|\)|\]|use\s|#\[|#!|import\s|from\s)")
_FILE_LINES: dict[str, list[str]] = {}


def file_lines(path: str) -> list[str] | None:
    """The cited file's lines, or `None` when it is not a file of this repo."""
    if path not in _FILE_LINES:
        target = (ROOT / path).resolve()
        try:
            target.relative_to(ROOT.resolve())
        except ValueError:
            _FILE_LINES[path] = []
        else:
            _FILE_LINES[path] = (
                target.read_text(encoding="utf-8", errors="replace").splitlines()
                if target.is_file()
                else []
            )
    return _FILE_LINES[path] or None


def _declaration(lines: list[str], start: int, end: int) -> list[str]:
    """The cited lines plus the block they open, so a citation of the opener
    (`EffectProperties::new(`, `public enum Stats`) resolves to its body."""
    depth, index = 0, start - 1
    limit = min(len(lines), end + CITE_OPEN_BLOCK)
    while index < limit:
        code = re.sub(r'"(?:\\.|[^"\\])*"', '""', lines[index]).split("//")[0]
        depth += code.count("(") + code.count("{") + code.count("[")
        depth -= code.count(")") + code.count("}") + code.count("]")
        index += 1
        if index >= end and depth <= 0:
            break
    return lines[start - 1:index]


def symbol_in(symbol: str, body: str) -> bool:
    lowered = body.lower()
    squeezed = re.sub(r"[\s_\-]", "", lowered)
    words = [word for word in re.split(r"[_\-]", symbol.lower()) if word]
    return re.sub(r"[\s_\-]", "", symbol.lower()) in squeezed or all(
        word in lowered for word in words
    )


def resolve_citation(raw: str, *alternatives: str) -> str | None:
    """`None` when the citation resolves against the tree, else the reason."""
    match = CITE_SPEC.match(raw)
    if match is None:
        return f"shape:{raw}"
    path, start = match.group("path"), int(match.group("start"))
    end = int(match.group("end") or start)
    lines = file_lines(path)
    if lines is None:
        return f"missing-file:{path}"
    if not 1 <= start <= end <= len(lines) or end - start > CITE_MAX_SPAN:
        return f"line-out-of-range:{path}:{start}-{end}"
    body = "\n".join(
        line
        for line in _declaration(lines, start, end)
        if not DECORATION_LINE.match(line)
    )
    if not body.strip():
        return f"no-declaration:{path}:{start}"
    if alternatives and not any(symbol_in(symbol, body) for symbol in alternatives):
        return f"symbol-absent:{'|'.join(alternatives)}@{path}:{start}"
    return None


def row_symbols(line: str) -> tuple[str, ...]:
    """The symbol the row declares in source terms, when it declares one."""
    named = GV_STAT_NAME.search(line)
    if named is not None:
        return SYMBOL_ALIASES.get(named.group(1), (named.group(1),))
    parts = line.split(" ")
    if parts[0] in ("ATTRIBUTE", "RESISTANCE", "DERIVED", "MOD") and len(parts) > 1:
        return SYMBOL_ALIASES.get(parts[1], (parts[1],))
    if parts[0] == "BIRTHSIGN" and len(parts) > 2:
        return SYMBOL_ALIASES.get(parts[2], (parts[2],))
    return ()
DECLARED_GOT = re.compile(r"\bgot=(\d+)\b")
DECLARED_TYPES = re.compile(r"\btypes=(\d+)\b")
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


def attribute_rules(lines: list[str]) -> tuple[tuple[bool, str], tuple[bool, str]]:
    rows = [ATTRIBUTE_LINE.match(line) for line in prefixed(lines, "ATTRIBUTE ")]
    names = [match.group(1) for match in rows if match]
    ordinals = [int(match.group(2)) for match in rows if match]
    set_ok = (
        len(rows) == len(ATTRIBUTES)
        and all(match is not None for match in rows)
        and tuple(names) == ATTRIBUTES
        and ordinals == list(range(len(ATTRIBUTES)))
    )
    set_detail = f"names={','.join(names)}"

    declared: int | None = None
    for line in prefixed(lines, "[OK] att-count "):
        match = DECLARED_GOT.search(line)
        if match:
            declared = int(match.group(1))
            break
    count_ok = declared is not None and declared == len(ATTRIBUTES) and len(rows) == declared
    return (set_ok, set_detail), (count_ok, f"emitted={len(rows)} declared={declared}")


def derived_rule(lines: list[str], stat_vectors: list[str]) -> tuple[bool, str]:
    """One `DERIVED` row per derived value, no repeats, and enough vectors.

    The per-name coverage itself (`every derived value has a golden vector`)
    belongs to `golden-coverage`; here only the declared census is checked.
    """
    rows = [DERIVED_LINE.match(line) for line in prefixed(lines, "DERIVED ")]
    names = [match.group(1) for match in rows if match]
    duplicates = sorted({name for name in names if names.count(name) > 1})
    ok = (
        bool(names)
        and all(match is not None for match in rows)
        and not duplicates
        and len(stat_vectors) >= len(names)
    )
    detail = (
        f"derived={len(names)} vectors={len(stat_vectors)} "
        f"duplicates={','.join(duplicates) or 'none'}"
    )
    return ok, detail


def resistance_rule(lines: list[str]) -> tuple[bool, str]:
    rows = [RESISTANCE_LINE.match(line) for line in prefixed(lines, "RESISTANCE ")]
    types = [match.group(1) for match in rows if match]
    tvalues = [match.group(2) for match in rows if match]
    declared: int | None = None
    for line in prefixed(lines, "[OK] res-census "):
        match = DECLARED_TYPES.search(line)
        if match:
            declared = int(match.group(1))
            break
    ok = (
        all(match is not None for match in rows)
        and tuple(types) == RESISTANCE_TYPES
        and all(value in TOLERANCES for value in tvalues)
        and declared == len(RESISTANCE_TYPES)
        and len(rows) == declared
    )
    return ok, f"types={len(types)} declared={declared} values={','.join(tvalues)}"


def birthsign_rule(lines: list[str]) -> tuple[bool, str]:
    rows = [BIRTHSIGN_LINE.match(line) for line in prefixed(lines, "BIRTHSIGN ")]
    ordinals = [int(match.group(1)) for match in rows if match]
    names = [match.group(2) for match in rows if match]
    effects = [match.group(3) for match in rows if match]
    ok = (
        all(match is not None for match in rows)
        and ordinals == list(range(len(BIRTHSIGNS)))
        and tuple(names) == BIRTHSIGNS
        and all(effect == "NEGATIVE" for effect in effects)
    )
    return ok, f"months={len(rows)} names={len(set(names))} effects={len(set(effects))}"


def mod_rule(lines: list[str]) -> tuple[bool, str]:
    rows = [MOD_LINE.match(line) for line in prefixed(lines, "MOD ")]
    codes = [match.group(1) for match in rows if match]
    ordinals = [int(match.group(2)) for match in rows if match]
    names = [match.group(3) for match in rows if match]
    ok = (
        all(match is not None for match in rows)
        and tuple(codes) == MOD_CODES
        and ordinals == list(range(len(MOD_CODES)))
        and len(set(names)) == len(MOD_CODES)
    )
    return ok, f"mods={len(rows)} codes={','.join(codes)}"


def census_rule(lines: list[str]) -> tuple[bool, str]:
    rows = [CENSUS_LINE.match(line) for line in prefixed(lines, "CENSUS ")]
    if len(rows) != 1 or rows[0] is None:
        return False, f"lines={len(rows)}"
    advantages, disadvantages, total = (int(value) for value in rows[0].groups())
    emitted_adv = sorted(
        {OK_LINE.match(line).group(1) for line in lines
         if OK_LINE.match(line) and OK_LINE.match(line).group(1).startswith("MOD-ADV-")}
    )
    emitted_dis = sorted(
        {OK_LINE.match(line).group(1) for line in lines
         if OK_LINE.match(line) and OK_LINE.match(line).group(1).startswith("MOD-DIS-")}
    )
    ok = (
        advantages == len(emitted_adv)
        and disadvantages == len(emitted_dis)
        and total == advantages + disadvantages
        and advantages == len(MOD_CODES)
        and disadvantages == len(MOD_CODES)
    )
    detail = (
        f"advantages={advantages}/{len(emitted_adv)} "
        f"disadvantages={disadvantages}/{len(emitted_dis)} total={total}"
    )
    return ok, detail


def ids_rule(lines: list[str]) -> tuple[bool, str]:
    ids: list[str] = []
    for line in lines:
        match = OK_LINE.match(line) or FAIL_LINE.match(line)
        if match:
            ids.append(match.group(1))
    duplicates = sorted({item for item in ids if ids.count(item) > 1})
    return (not duplicates and bool(ids)), f"ids={len(ids)} duplicates={','.join(duplicates) or 'none'}"


def golden_coverage_rule(lines: list[str]) -> tuple[bool, str]:
    derived = [DERIVED_LINE.match(line).group(1) for line in prefixed(lines, "DERIVED ")]
    stat_names = [
        match.group(1)
        for line in lines
        if (ok := OK_LINE.match(line)) and ok.group(1).startswith("GV-STAT-")
        and (match := GV_STAT_NAME.search(ok.group(2)))
    ]
    res_types = [
        match.group(1)
        for line in lines
        if (ok := OK_LINE.match(line)) and ok.group(1).startswith("GV-RES-")
        and (match := GV_RES_TYPE.search(ok.group(2)))
    ]
    unnamed = sorted({name for name in derived if name not in set(stat_names)})
    orphan = sorted({name for name in stat_names if name not in set(derived)})
    missing_types = sorted(set(RESISTANCE_TYPES) - set(res_types))
    sign_vectors = len(prefixed(lines, "[OK] GV-SIGN-"))
    ok = not unnamed and not orphan and not missing_types and sign_vectors == len(BIRTHSIGNS)
    detail = (
        f"derived={len(set(derived))} covered={len(set(derived) & set(stat_names))} "
        f"unnamed={','.join(unnamed) or 'none'} orphan={','.join(orphan) or 'none'} "
        f"res-covered={len(set(res_types))} missing={','.join(missing_types) or 'none'} "
        f"sign-vectors={sign_vectors}"
    )
    return ok, detail


def citation_rule(lines: list[str]) -> tuple[bool, str]:
    problems: list[str] = []
    total = 0
    resolved = 0
    for line in lines:
        for raw in CITATION_RESOLVE.findall(line):
            total += 1
            reason = resolve_citation(raw, *row_symbols(line))
            if reason is None:
                resolved += 1
            else:
                problems.append(reason)
    if total == 0:
        return False, "no-citations"
    ok = not problems
    return ok, (
        f"citations={total} resolved={resolved} unresolved={len(problems)} "
        f"{' '.join(problems[:4]) or 'none'}"
    )


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
    stat_vectors = prefixed(lines_one, "[OK] GV-STAT-")
    (set_ok, set_detail), (count_ok, count_detail) = attribute_rules(lines_one)
    derived_ok, derived_detail = derived_rule(lines_one, stat_vectors)
    rules = [
        Rule("run1-verdict", "VEREDICTO: PASS" in first.stdout.decode("utf-8", "replace")
             and first.exit == 0, f"exit={first.exit}"),
        Rule("run2-verdict", "VEREDICTO: PASS" in second.stdout.decode("utf-8", "replace")
             and second.exit == 0, f"exit={second.exit}"),
        Rule("byte-identical", first.stdout == second.stdout, f"{len(first.stdout)} B"),
        Rule("sha256-equal", sha_one == sha_two, sha_one[:16]),
        Rule("header", *header_rule(lines_one)),
        Rule("attribute-set", set_ok, set_detail),
        Rule("attribute-count", count_ok, count_detail),
        Rule("derived-census", derived_ok, derived_detail),
        Rule("resistance-census", *resistance_rule(lines_one)),
        Rule("birthsign-census", *birthsign_rule(lines_one)),
        Rule("mod-census", *mod_rule(lines_one)),
        Rule("census-totals", *census_rule(lines_one)),
        Rule("census-ids-unique", *ids_rule(lines_one)),
        Rule("golden-coverage", *golden_coverage_rule(lines_one)),
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
        "attributes": len(ATTRIBUTES),
        "birthsigns": len(BIRTHSIGNS),
        "resistance_types": len(RESISTANCE_TYPES),
        "player_mods": len(MOD_CODES),
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
                f"ok-rows={capture['ok_rows']} negative={capture['negative_rows']}"
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
            [str(binary), "--probe", "SYS-STATS-001"],
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


def mutate_attribute_missing(captures: list[Capture]) -> tuple[list[Capture], str]:
    note: list[str] = []

    def mutate(lines: list[str]) -> list[str]:
        new, removed = drop_first(lines, "ATTRIBUTE Willpower")
        note.append(removed.split(" ")[1])
        return new

    return replace_lines(captures, mutate), "removed ATTRIBUTE Willpower"


def mutate_resistance_missing(captures: list[Capture]) -> tuple[list[Capture], str]:
    def mutate(lines: list[str]) -> list[str]:
        new, _removed = drop_first(lines, "RESISTANCE Disease")
        return new

    return replace_lines(captures, mutate), "removed RESISTANCE Disease"


def mutate_citation_empty(captures: list[Capture]) -> tuple[list[Capture], str]:
    note: list[str] = []

    def mutate(lines: list[str]) -> list[str]:
        for index, line in enumerate(lines):
            if line.startswith("[OK] GV-STAT-01 "):
                lines[index] = CITATION.sub("", line).rstrip()
                note.append("GV-STAT-01")
                return lines
        raise AssertionError("no GV-STAT-01")

    return replace_lines(captures, mutate), "stripped src= from GV-STAT-01"


def mutate_census_mismatch(captures: list[Capture]) -> tuple[list[Capture], str]:
    note: list[str] = []

    def mutate(lines: list[str]) -> list[str]:
        for index, line in enumerate(lines):
            match = CENSUS_LINE.match(line)
            if match:
                advantages, disadvantages, total = (int(value) for value in match.groups())
                lines[index] = (
                    f"CENSUS advantages={advantages - 1} disadvantages={disadvantages} "
                    f"total={total}"
                )
                note.append(f"advantages {advantages} -> {advantages - 1}")
                return lines
        raise AssertionError("no CENSUS line")

    return replace_lines(captures, mutate), "CENSUS advantages 6 -> 5"


def mutate_derived_without_golden(captures: list[Capture]) -> tuple[list[Capture], str]:
    note: list[str] = []

    def mutate(lines: list[str]) -> list[str]:
        changed = []
        for line in lines:
            if line.startswith("[OK] GV-STAT-") and "name=player_level " in line:
                changed.append(line.replace("name=player_level ", "name=player_level_typo ", 1))
            else:
                changed.append(line)
        note.append("player_level vectors renamed")
        return changed

    return replace_lines(captures, mutate), "renamed every name=player_level vector"


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


def mutate_citation_wrong_line(captures: list[Capture]) -> tuple[list[Capture], str]:
    """A citation whose line exists but does not declare the row's symbol."""

    def mutate(lines: list[str]) -> list[str]:
        for index, line in enumerate(lines):
            if line.startswith("DERIVED max_fatigue "):
                lines[index] = re.sub(
                    r"src=\S+$",
                    "src=reference/dfu-spec/Assets/Scripts/Game/Formulas/FormulaHelper.cs:8",
                    line,
                )
                return lines
        raise AssertionError("no DERIVED max_fatigue")

    return replace_lines(captures, mutate), "max_fatigue -> FormulaHelper.cs:8 (no declaration)"


def mutate_citation_missing_file(captures: list[Capture]) -> tuple[list[Capture], str]:
    """A citation to a file that is not in the tree."""

    def mutate(lines: list[str]) -> list[str]:
        for index, line in enumerate(lines):
            if line.startswith("ATTRIBUTE Strength "):
                lines[index] = re.sub(r"src=\S+$", "src=crates/skills/src/no_such_stats.rs:12", line)
                return lines
        raise AssertionError("no ATTRIBUTE Strength")

    return replace_lines(captures, mutate), "Strength -> missing stats file"


FALSIFIERS = {
    "attribute-missing": ("attribute-set", mutate_attribute_missing),
    "resistance-missing": ("resistance-census", mutate_resistance_missing),
    "citation-empty": ("citation-present", mutate_citation_empty),
    "citation-wrong-line": ("citation-present", mutate_citation_wrong_line),
    "citation-missing-file": ("citation-present", mutate_citation_missing_file),
    "census-mismatch": ("census-totals", mutate_census_mismatch),
    "derived-without-golden": ("golden-coverage", mutate_derived_without_golden),
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
