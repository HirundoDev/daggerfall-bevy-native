#!/usr/bin/env python3
"""SYS-MAGIC-001 - gate of M41/W41-3: complete magic.

Two separate processes, same binary, same evidence
--------------------------------------------------
1. `cargo build -p daggerfall-app` once (`--skip-build` reuses the binary).
2. `--probe SYS-MAGIC-001` runs in **two independent processes** (`PROBE_RUN=1`
   and `PROBE_RUN=2`; the probe never prints that value, so an environment
   dependence shows up as a stdout difference).
3. Both captures must exit 0, contain `VEREDICTO: PASS`, be **byte-identical**
   (`cmp` + equal `sha256`), and the probe's own `DIGEST` line must be the
   `sha256` of every byte printed before it.
4. The probe's censuses are checked one by one against the frozen tables below:

   - `school-census`: the seven `SCHOOL` lines, each school's `rows=` equal to
     its `EFFECT` row count, `effects=` derived as `rows - sum(n-1)` over the
     declared variant families, totals `rows=165`, `effects=143`, `variants=22`
     and the declared registry length (`TEMPLATE_COUNT`) equal to 165.
   - `effect-rows`: exactly 165 `EFFECT` rows with unique keys, known schools,
     numeric classic keys and a `covered`/`NEGATIVE` verdict.
   - `row-citations`: every `EFFECT` and every `[OK]` row cites a real
     `path:line` (or, for `NEGATIVE`, a `spec=...:line` plus a `reason=`).
   - `rounds-formula`: every `ROUND` row recomputed as
     `base + mod * max(1, level // per_level)` (`per_level <= 0` contributes no
     step), with the classic per-level and maximum-level vectors present.
   - `cost-census`: `COST` rows for both classic formulas, `got == want`,
     >= 6 distinct vectors per formula, and a non-empty derivation per row.
   - `range-census`: the five classic `rangeType` values and their
     cost/behaviour modifiers, recomputed from the frozen multipliers.
   - `spellmaker`: the composed spell (>= 3 effects) and
     `SPELLPERSIST roundtrip=identical` with a 64-hex digest.
   - `no-hedge-words`: zero occurrences of `approximate`/`estimated`/`approx`.
   - `adversarial`: the six negative rows F1..F6 are present and green.

Falsifiers (each mutates a TEMP COPY of the captures under its own name,
requires the named rule red, then re-measures the clean captures):
`census-gap`, `negative-without-path`, `approximate-word`, `persist-diverges`,
`digest-tamper`, `duplicate-key`, `round-formula`.

Exit codes: 0 PASS, 1 FAIL, 2 usage/IO (including `SKIP`).
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
GATE = "SYS-MAGIC-001"
PROBE = "PROBE-SYS-MAGIC-001"
SLUG = "sys-magic-001"
BINARY = ROOT / "target" / "debug" / "daggerfall-app"
ARTIFACTS = ROOT / ".sintergic" / "artifacts" / "m41-sys" / SLUG
CR = "CR-a86cec9d-3ec1-406f-aac9-82d2265bbb28"
CRATE = "daggerfall-app"

DECLARED_ROWS = 165
DECLARED_CLASSES = 143
DECLARED_VARIANTS = 22
DECLARED_SOURCE_FILES = 153

# Frozen census: (school, effect classes, registry rows). `rows` is measured by
# the gate from the EFFECT rows; `effects` is re-derived below from `rows` and
# the declared variant families, so this table is the nominal target, never a
# substitute for the measurement.
SCHOOLS: tuple[tuple[str, int, int], ...] = (
    ("Alteration", 7, 11),
    ("Destruction", 25, 25),
    ("Illusion", 9, 13),
    ("Mysticism", 10, 10),
    ("None", 58, 69),
    ("Restoration", 24, 24),
    ("Thaumaturgy", 10, 13),
)

# Typed variant families (contract §1/§4): `variant_count > 1` collapses the
# family to one class, so `effects = rows - (n - 1)` per family.
VARIANT_FAMILIES: tuple[tuple[str, int, str], ...] = (
    ("ElementalResistance", 5, "Alteration"),
    ("MageLight", 5, "Illusion"),
    ("Pacify", 4, "Thaumaturgy"),
    ("Poison", 12, "None"),
)

# Classic rangeType (spec §3.3: 0..4, cost multiplier percent).
RANGES: tuple[tuple[int, str, int], ...] = (
    (0, "CasterOnly", 100),
    (1, "ByTouch", 100),
    (2, "SingleTargetAtRange", 150),
    (3, "AreaAroundCaster", 200),
    (4, "AreaAtRange", 250),
)
RANGE_MODIFIERS = (2, 2, 3, 4, 5)
# The D8 bundle is the reference cost the range table is measured on.
RANGE_BASE_GOLD = 208
RANGE_BASE_POINTS = 31

# Spec §2.1 B1-B5 + the per-level maximum (B7) and the `per_level = 0` guard
# (B8); the gate recomputes `rounds` from the printed inputs.
ROUND_VECTORS: tuple[tuple[str, int, int, int, int], ...] = (
    ("Climbing", 1, 1, 1, 1),
    ("Levitate", 1, 1, 25, 1),
    ("Shield", 1, 1, 1, 5),
    ("Pacify-Animal", 5, 2, 3, 9),
    ("WaterWalking", 2, 3, 2, 7),
    ("Cure-Disease", 1, 1, 1, 100),
    ("Slowfall", 5, 1, 0, 7),
)

CITE = re.compile(r"^[\w./+-]+\.(?:rs|md|py|json|toml):\d+$")

# ── cita resuelta contra el árbol (CR-4171fb19 · GAP-M41-02) ────────────────
# `CITE` sólo probaba la FORMA `path:line`: hoy la cita se resuelve contra el
# árbol — el fichero existe, el rango citado existe, el texto citado lleva
# declaración (ni blanco, ni comentario, ni llave suelta) y el símbolo que la
# fila declara aparece en la declaración resuelta. La cita puede apuntar a la
# línea que ABRE el bloque (`EffectProperties::new(`) y se resuelve el bloque.
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
    (`EffectProperties::new(`) resolves to the declaration it contains."""
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

SCHOOL_LINE = re.compile(r"^SCHOOL (\S+) effects=(\d+) rows=(\d+)$")
EFFECT_LINE = re.compile(
    r"^EFFECT (\S+) school=(\S+) classic_key=(-?\d+) verdict=(covered|NEGATIVE) (.*)$"
)
EFFECT_APPLIED = re.compile(r"^applied=(\d+) src=(\S+)$")
EFFECT_NEGATIVE = re.compile(r"^spec=(\S+) reason=(.+)$")
OK_LINE = re.compile(r"^\[OK\] (\S+) - .* \(src=(\S+)\)$")
FAIL_LINE = re.compile(r"^FAIL (\S+) got=(.*) want=(.*)$")
ROUND_LINE = re.compile(
    r"^ROUND (\S+) base=(-?\d+) mod=(-?\d+) per_level=(-?\d+) level=(-?\d+) "
    r"rounds=(-?\d+) src=(\S+)$"
)
COST_LINE = re.compile(
    r"^COST formula=([BC]) vector=(\S+) field=(\w+) got=(-?\d+) want=(-?\d+) "
    r"derivation=(\S+) src=(\S+)$"
)
RANGE_LINE = re.compile(r"^RANGE (\S+) classic=(\d+) multiplier=(\d+) gold=(\d+) points=(\d+) src=(\S+)$")
RANGE_MOD_LINE = re.compile(r"^RANGE-MOD classic=(\d+) modifier=(\d+) cost=(\d+) src=(\S+)$")
SPELL_LINE = re.compile(
    r"^SPELL name=(\S+) effects=(\d+) target=(\S+) gold=(\d+) points=(\d+) "
    r"school_split=(\S+) src=(\S+)$"
)
SPELLPERSIST_LINE = re.compile(
    r"^SPELLPERSIST roundtrip=(identical|diverges) digest=([0-9a-f]{64}) bytes=(\d+) "
    r"fields=(\S+) src=(\S+)$"
)
DIGEST_LINE = re.compile(r"^DIGEST ([0-9a-f]{64})$")
HEDGE = re.compile(r"approximate|estimated|approx", re.IGNORECASE)


# -- process helpers --------------------------------------------------------


def retail_dir() -> Path | None:
    """Retail install (read-only): `$DF_APP_ROOT` or `$HOME/Games/Daggerfall/app`."""
    root = os.environ.get("DF_APP_ROOT")
    base = Path(root) if root else Path(os.environ.get("HOME", "")) / "Games" / "Daggerfall" / "app"
    if (base / "FALL.EXE").is_file() and (base / "ARENA2").is_dir():
        return base
    return None


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


def matches(lines: list[str], pattern: re.Pattern[str]) -> list[re.Match[str]]:
    return [match for line in lines if (match := pattern.match(line))]


def effect_rows(lines: list[str]) -> list[re.Match[str]]:
    return matches(lines, EFFECT_LINE)


def verdict_of(capture: Capture) -> str | None:
    found = [line for line in lines_of(capture) if line.startswith("VEREDICTO")]
    return found[-1] if found else None


def digest_of(capture: Capture) -> str | None:
    found = [match.group(1) for line in lines_of(capture) if (match := DIGEST_LINE.match(line))]
    return found[0] if len(found) == 1 else None


# -- pure checks ------------------------------------------------------------


def rule_probe_verdict(captures: list[Capture]) -> Rule:
    problems: list[str] = []
    evidence: list[str] = []
    for capture in captures:
        verdict = verdict_of(capture) or "absent"
        status = verdict.replace("VEREDICTO:", "").strip() or "absent"
        evidence.append(f"run{capture.run}={status} exit={capture.exit}")
        if " PASS" not in verdict or capture.exit != 0:
            problems.append(f"run{capture.run}:{verdict}")
    for capture in captures:
        failures = matches(lines_of(capture), FAIL_LINE)
        if failures:
            problems.append(f"run{capture.run}:{len(failures)} FAIL rows")
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
        f"stdout={len(first.stdout)}B sha256={digest(first.stdout)}",
    )


def rule_digest_valid(captures: list[Capture]) -> Rule:
    capture = captures[0]
    value = digest_of(capture)
    if value is None:
        return Rule("digest-valid", False, "no single DIGEST line")
    lines = lines_of(capture)
    index = next((i for i, line in enumerate(lines) if line.startswith("DIGEST ")), None)
    if index is None:
        return Rule("digest-valid", False, "DIGEST line not found")
    if index != len(lines) - 1:
        return Rule("digest-valid", False, f"DIGEST at line {index + 1} of {len(lines)} (not last)")
    expected = digest("\n".join(lines[:index]).encode())
    if expected != value:
        return Rule("digest-valid", False, f"digest={value} != sha256(prefix)={expected}")
    if capture.exit != 0:
        return Rule("digest-valid", False, f"exit={capture.exit}")
    return Rule("digest-valid", True, f"digest={value} prefix=verified last-line=yes")


def rule_school_census(captures: list[Capture]) -> Rule:
    lines = lines_of(captures[0])
    declared = {
        int(match.group(1))
        for match in matches(lines, re.compile(r"^\[OK\] A1 - registry rows=(\d+) "))
    }
    schools = matches(lines, SCHOOL_LINE)
    rows = effect_rows(lines)
    problems: list[str] = []
    observed: dict[str, int] = {}
    for match in rows:
        observed[match.group(2)] = observed.get(match.group(2), 0) + 1
    pairs = [(match.group(1), int(match.group(2)), int(match.group(3))) for match in schools]
    if pairs != list(SCHOOLS):
        problems.append(f"schools={pairs}")
    for school, effects, n in SCHOOLS:
        if observed.get(school) != n:
            problems.append(f"{school}:rows={observed.get(school)}")
    if set(observed) != {school for school, _, _ in SCHOOLS}:
        problems.append(f"school-set={sorted(observed)}")
    total_rows = sum(n for _, _, n in pairs)
    total_effects = sum(e for _, e, _ in pairs)
    if total_rows != DECLARED_ROWS:
        problems.append(f"rows={total_rows}")
    if total_effects != DECLARED_CLASSES:
        problems.append(f"effects={total_effects}")
    if total_rows - total_effects != DECLARED_VARIANTS:
        problems.append(f"variants={total_rows - total_effects}")
    # Independent derivation of the `effects` column: rows minus the collapse of
    # each declared variant family, verified against the EFFECT keys.
    keys = [match.group(1) for match in rows]
    derived: dict[str, int] = {}
    for school, _, n in SCHOOLS:
        derived[school] = n
    for prefix, n, school in VARIANT_FAMILIES:
        family = [key for key in keys if key.split("-")[0] == prefix]
        if len(family) != n:
            problems.append(f"{prefix}:keys={len(family)} want={n}")
        if any(row.group(2) != school for row in rows if row.group(1) in family):
            problems.append(f"{prefix}:school")
        derived[school] -= n - 1
    for school, effects, _ in SCHOOLS:
        if derived[school] != effects:
            problems.append(f"{school}:derived={derived[school]} want={effects}")
    if declared != {DECLARED_ROWS}:
        problems.append(f"declared={sorted(declared) or 'absent'}")
    if DECLARED_SOURCE_FILES - DECLARED_CLASSES != 10:
        problems.append("source-files-reconciliation")
    if problems:
        return Rule("school-census", False, " ".join(problems[:8]))
    return Rule(
        "school-census",
        True,
        f"schools={len(pairs)} rows={total_rows} effects={total_effects} "
        f"variants={total_rows - total_effects} declared={DECLARED_ROWS} files={DECLARED_SOURCE_FILES}",
    )


def rule_effect_rows(captures: list[Capture]) -> Rule:
    rows = effect_rows(lines_of(captures[0]))
    problems: list[str] = []
    keys = [match.group(1) for match in rows]
    if len(rows) != DECLARED_ROWS:
        problems.append(f"rows={len(rows)} want={DECLARED_ROWS}")
    duplicated = sorted({key for key in keys if keys.count(key) > 1})
    if duplicated:
        problems.append(f"duplicated={','.join(duplicated[:4])}")
    verdicts = {match.group(4) for match in rows}
    if not verdicts <= {"covered", "NEGATIVE"}:
        problems.append(f"verdicts={sorted(verdicts)}")
    covered = sum(1 for match in rows if match.group(4) == "covered")
    negative = len(rows) - covered
    schools = {school for school, _, _ in SCHOOLS}
    for match in rows:
        key, school, classic = match.group(1), match.group(2), match.group(3)
        if school not in schools:
            problems.append(f"{key}:school={school}")
        try:
            int(classic)
        except ValueError:
            problems.append(f"{key}:classic={classic}")
    if problems:
        return Rule("effect-rows", False, " ".join(problems[:8]))
    return Rule(
        "effect-rows",
        True,
        f"rows={len(rows)} unique={len(set(keys))} covered={covered} NEGATIVE={negative}",
    )


def rule_row_citations(captures: list[Capture]) -> Rule:
    lines = lines_of(captures[0])
    problems: list[str] = []
    rows = effect_rows(lines)
    negative_rows = 0
    resolved = 0
    for match in rows:
        key, verdict, tail = match.group(1), match.group(4), match.group(5)
        if verdict == "covered":
            applied = EFFECT_APPLIED.match(tail)
            if applied is None:
                problems.append(f"{key}:covered-payload")
            elif (reason := resolve_citation(applied.group(2), key)) is not None:
                problems.append(f"{key}:{reason}")
            else:
                resolved += 1
        else:
            negative_rows += 1
            spec = EFFECT_NEGATIVE.match(tail)
            if spec is None:
                problems.append(f"{key}:NEGATIVE-without-path")
            elif (reason := resolve_citation(spec.group(1))) is not None:
                problems.append(f"{key}:{reason}")
            elif not spec.group(2).strip():
                problems.append(f"{key}:NEGATIVE-without-reason")
            else:
                resolved += 1
    ok_rows = matches(lines, OK_LINE)
    for match in ok_rows:
        if (reason := resolve_citation(match.group(2))) is not None:
            problems.append(f"{match.group(1)}:{reason}")
        else:
            resolved += 1
    evidence_rows = (
        matches(lines, ROUND_LINE)
        + matches(lines, COST_LINE)
        + matches(lines, RANGE_LINE)
        + matches(lines, RANGE_MOD_LINE)
        + matches(lines, SPELL_LINE)
        + matches(lines, SPELLPERSIST_LINE)
    )
    for match in evidence_rows:
        source = match.group(match.lastindex or 1)
        if (reason := resolve_citation(source)) is not None:
            problems.append(f"evidence:{reason}")
        else:
            resolved += 1
    if problems:
        return Rule("row-citations", False, " ".join(problems[:8]))
    return Rule(
        "row-citations",
        True,
        f"effect-rows={len(rows)} ok-rows={len(ok_rows)} evidence-rows={len(evidence_rows)} "
        f"NEGATIVE={negative_rows} resolved={resolved} unresolved=0",
    )


def rounds_expected(base: int, plus: int, per_level: int, level: int) -> int:
    multiplier = level // per_level if per_level > 0 else 0
    multiplier = max(multiplier, 1)
    return base + plus * multiplier


def rule_rounds_formula(captures: list[Capture]) -> Rule:
    rows = matches(lines_of(captures[0]), ROUND_LINE)
    problems: list[str] = []
    seen: list[tuple[str, int, int, int, int]] = []
    for match in rows:
        effect = match.group(1)
        base, plus, per_level, level, got = (int(match.group(i)) for i in (2, 3, 4, 5, 6))
        # B6 is the `SupportDuration = false` control: the formula is bypassed.
        want = 0 if effect == "B6" else rounds_expected(base, plus, per_level, level)
        if got != want:
            problems.append(f"{effect}:rounds={got} want={want}")
        seen.append((effect, base, plus, per_level, level))
    for vector in ROUND_VECTORS:
        if vector not in seen:
            problems.append(f"missing={vector[0]}")
    without_duration = next((m for m in rows if m.group(1) == "B6"), None)
    if without_duration is None or int(without_duration.group(6)) != 0:
        problems.append("SupportDuration=false")
    if problems:
        return Rule("rounds-formula", False, " ".join(problems[:8]))
    return Rule("rounds-formula", True, f"vectors={len(rows)} recomputed={len(rows)} zero-duration=ok")


def rule_cost_census(captures: list[Capture]) -> Rule:
    rows = matches(lines_of(captures[0]), COST_LINE)
    problems: list[str] = []
    by_formula: dict[str, dict[str, int]] = {"B": {}, "C": {}}
    for match in rows:
        formula, vector, field = match.group(1), match.group(2), match.group(3)
        got, want, derivation = int(match.group(4)), int(match.group(5)), match.group(6)
        if got != want:
            problems.append(f"{formula}/{vector}/{field}:got={got} want={want}")
        if not derivation or HEDGE.search(derivation):
            problems.append(f"{formula}/{vector}/{field}:derivation")
        by_formula[formula].setdefault(vector, 0)
        by_formula[formula][vector] += 1
    for formula in ("B", "C"):
        if len(by_formula[formula]) < 6:
            problems.append(f"{formula}:vectors={len(by_formula[formula])}")
    fields = {match.group(3) for match in rows}
    if not {"cost", "gold", "points"} <= fields:
        problems.append(f"fields={sorted(fields)}")
    floor = next((m for m in rows if m.group(2) == "FLOOR"), None)
    if floor is None or int(floor.group(4)) != 5:
        problems.append("floor")
    # The explicit "computed, not tabulated" assertion lives in its own row.
    lines = lines_of(captures[0])
    if not any(line.startswith("[OK] C-NOT-A-TABLE ") for line in lines):
        problems.append("formula-not-table row absent")
    if problems:
        return Rule("cost-census", False, " ".join(problems[:8]))
    return Rule(
        "cost-census",
        True,
        f"rows={len(rows)} formula-B-vectors={len(by_formula['B'])} "
        f"formula-C-vectors={len(by_formula['C'])} floor=5 formula-not-table=ok",
    )


def rule_range_census(captures: list[Capture]) -> Rule:
    lines = lines_of(captures[0])
    rows = matches(lines, RANGE_LINE)
    mods = matches(lines, RANGE_MOD_LINE)
    problems: list[str] = []
    observed = [(m.group(2), m.group(1), int(m.group(3)), int(m.group(4)), int(m.group(5))) for m in rows]
    expected = [(str(index), name, percent) for index, name, percent in RANGES]
    if [(o[0], o[1], o[2]) for o in observed] != expected:
        problems.append(f"ranges={[(o[0], o[1], o[2]) for o in observed]}")
    for _, name, percent, gold, points in observed:
        if gold != RANGE_BASE_GOLD * percent // 100:
            problems.append(f"{name}:gold={gold}")
        if points != RANGE_BASE_POINTS * percent // 100:
            problems.append(f"{name}:points={points}")
    observed_mods = [(int(m.group(1)), int(m.group(2)), int(m.group(3))) for m in mods]
    expected_mods = [(index, modifier, (28 * modifier) >> 1) for index, modifier in enumerate(RANGE_MODIFIERS)]
    if observed_mods != expected_mods:
        problems.append(f"modifiers={observed_mods}")
    unknown = [line for line in lines if line.startswith("[OK] D-unknown-range ")]
    if len(unknown) != 1:
        problems.append("unknown-range")
    if problems:
        return Rule("range-census", False, " ".join(problems[:8]))
    return Rule(
        "range-census",
        True,
        f"ranges={len(rows)} modifiers={','.join(str(m) for m in RANGE_MODIFIERS)} unknown-refused=1",
    )


def rule_spellmaker(captures: list[Capture]) -> Rule:
    lines = lines_of(captures[0])
    spell = matches(lines, SPELL_LINE)
    persist = matches(lines, SPELLPERSIST_LINE)
    problems: list[str] = []
    if len(spell) != 1:
        problems.append(f"SPELL-rows={len(spell)}")
    else:
        match = spell[0]
        effects, target, gold, points = int(match.group(2)), match.group(3), int(match.group(4)), int(match.group(5))
        split = {}
        for part in match.group(6).split(","):
            school, _, value = part.partition(":")
            split[school] = int(value)
        if effects < 3:
            problems.append(f"effects={effects}")
        if sum(split.values()) != gold:
            problems.append(f"split={sum(split.values())} gold={gold}")
        if points <= 0 or gold <= 0:
            problems.append("non-positive")
        if not match.group(1):
            problems.append("name")
        if target not in {name for _, name, _ in RANGES}:
            problems.append(f"target={target}")
    if len(persist) != 1:
        problems.append(f"SPELLPERSIST-rows={len(persist)}")
    elif persist[0].group(1) != "identical":
        problems.append(f"roundtrip={persist[0].group(1)}")
    elif int(persist[0].group(3)) != 89:
        problems.append(f"bytes={persist[0].group(3)}")
    if problems:
        return Rule("spellmaker", False, " ".join(problems[:8]))
    match = spell[0]
    return Rule(
        "spellmaker",
        True,
        f"effects={match.group(2)} gold={match.group(4)} points={match.group(5)} "
        f"split={match.group(6)} persist=identical digest={persist[0].group(2)[:16]}",
    )


def rule_no_hedge_words(captures: list[Capture]) -> Rule:
    hits = HEDGE.findall(captures[0].stdout.decode("utf-8", "replace"))
    if hits:
        return Rule("no-hedge-words", False, f"hits={len(hits)} words={sorted(set(hits))}")
    return Rule("no-hedge-words", True, "approximate=0 estimated=0 approx=0")


def rule_adversarial(captures: list[Capture]) -> Rule:
    lines = lines_of(captures[0])
    ok_ids = {match.group(1) for match in matches(lines, OK_LINE)}
    wanted = [f"F{i}" for i in range(1, 7)]
    missing = [row for row in wanted if row not in ok_ids]
    if missing:
        return Rule("adversarial", False, f"missing={','.join(missing)}")
    return Rule("adversarial", True, f"rows={len(wanted)} green={len(wanted)}")


RULES = (
    rule_probe_verdict,
    rule_stdout_byte_identical,
    rule_digest_valid,
    rule_school_census,
    rule_effect_rows,
    rule_row_citations,
    rule_rounds_formula,
    rule_cost_census,
    rule_range_census,
    rule_spellmaker,
    rule_no_hedge_words,
    rule_adversarial,
)


def check(captures: list[Capture]) -> list[Rule]:
    """The twelve rules over two captures. Pure: no disk, no clock, no build."""
    return [rule(captures) for rule in RULES]


# -- reporting --------------------------------------------------------------


def failed_names(rules: list[Rule]) -> list[str]:
    return [rule.name for rule in rules if not rule.ok]


def payload(binary: Path, captures: list[Capture], rules: list[Rule], extra: dict | None = None) -> dict:
    lines = lines_of(captures[0])
    rows = effect_rows(lines)
    covered = sum(1 for match in rows if match.group(4) == "covered")
    schools = {
        match.group(1): {"effects": int(match.group(2)), "rows": int(match.group(3))}
        for match in matches(lines, SCHOOL_LINE)
    }
    ok_rows = matches(lines, OK_LINE)
    body = {
        "schema": "SYS-MAGIC-001/1",
        "gate": GATE,
        "cr": CR,
        "probe": PROBE,
        "head": head_commit(),
        "binary": str(binary),
        "binary_sha256": digest(binary.read_bytes()) if binary.is_file() else None,
        "rows": len(rows),
        "covered": covered,
        "negative": len(rows) - covered,
        "declared_rows": DECLARED_ROWS,
        "declared_classes": DECLARED_CLASSES,
        "declared_variants": DECLARED_VARIANTS,
        "schools": schools,
        "ok_rows": len(ok_rows),
        "rounds": len(matches(lines, ROUND_LINE)),
        "cost_rows": len(matches(lines, COST_LINE)),
        "range_rows": len(matches(lines, RANGE_LINE)),
        "range_modifier_rows": len(matches(lines, RANGE_MOD_LINE)),
        "spellpersist": [match.group(1) for match in matches(lines, SPELLPERSIST_LINE)],
        "probe_digest": digest_of(captures[0]),
        "stdout": [
            {"run": capture.run, "exit": capture.exit, "bytes": len(capture.stdout), "sha256": digest(capture.stdout)}
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
            f"covered={body['covered']} negative={body['negative']} schools={len(body['schools'])}"
        )
        for capture in captures:
            print(
                f"run{capture.run} exit={capture.exit} sha256={digest(capture.stdout)} "
                f"bytes={len(capture.stdout)}"
            )
        for rule in rules:
            print(f"{'OK' if rule.ok else 'FAIL'} {rule.name} {rule.detail}")
        print(
            f"CHECKS {','.join(rule.name for rule in rules)} "
            f"{'all-OK' if not failed else 'failed=' + ','.join(failed)}"
        )
    return 0 if not failed else 1


def write_manifest(
    binary: Path, captures: list[Capture], rules: list[Rule], out: Path, extra: dict | None = None
) -> Path:
    out.mkdir(parents=True, exist_ok=True)
    path = out / "manifest.json"
    path.write_text(json.dumps(payload(binary, captures, rules, extra), indent=2, sort_keys=True) + "\n")
    for capture in captures:
        (out / f"run{capture.run}.stdout").write_bytes(capture.stdout)
        (out / f"run{capture.run}.stderr").write_bytes(capture.stderr)
    return path


# -- falsifiers: mutate a TEMP COPY of the captures -------------------------


def rewrite(captures: list[Capture], index: int, line: str) -> list[Capture]:
    """Replace line `index` in both captures (they are byte-identical)."""
    return [
        replace(
            capture,
            stdout=("\n".join(lines_of(capture)[:index] + [line] + lines_of(capture)[index + 1:]) + "\n").encode(),
        )
        for capture in captures
    ]


def mutate_census_gap(captures: list[Capture]) -> tuple[list[Capture], str]:
    lines = lines_of(captures[0])
    index = next(i for i, line in enumerate(lines) if line.startswith("EFFECT "))
    removed = lines[index]
    mutated = [
        replace(capture, stdout=("\n".join(lines_of(capture)[:index] + lines_of(capture)[index + 1:]) + "\n").encode())
        for capture in captures
    ]
    return mutated, f"removed {removed.split(' ')[1]}"


def mutate_negative_without_path(captures: list[Capture]) -> tuple[list[Capture], str]:
    lines = lines_of(captures[0])
    index = next(i for i, line in enumerate(lines) if line.startswith("EFFECT "))
    match = EFFECT_LINE.match(lines[index])
    assert match is not None
    line = (
        f"EFFECT {match.group(1)} school={match.group(2)} classic_key={match.group(3)} "
        f"verdict=NEGATIVE applied=0 reason=dead src=-"
    )
    return rewrite(captures, index, line), f"{match.group(1)} -> NEGATIVE without spec path"


def mutate_approximate_word(captures: list[Capture]) -> tuple[list[Capture], str]:
    lines = lines_of(captures[0])
    index = next(i for i, line in enumerate(lines) if line.startswith("[OK] A6 "))
    return rewrite(captures, index, lines[index] + " approximate"), "A6 hedged with 'approximate'"


def mutate_persist_diverges(captures: list[Capture]) -> tuple[list[Capture], str]:
    lines = lines_of(captures[0])
    index = next(i for i, line in enumerate(lines) if line.startswith("SPELLPERSIST "))
    match = SPELLPERSIST_LINE.match(lines[index])
    assert match is not None
    line = lines[index].replace("roundtrip=identical", "roundtrip=diverges", 1)
    return rewrite(captures, index, line), f"roundtrip {match.group(1)} -> diverges"


def mutate_digest_tamper(captures: list[Capture]) -> tuple[list[Capture], str]:
    lines = lines_of(captures[0])
    index = next(i for i, line in enumerate(lines) if line.startswith("DIGEST "))
    value = DIGEST_LINE.match(lines[index])
    assert value is not None
    tampered = value.group(1)[:-1] + ("0" if value.group(1)[-1] != "0" else "1")
    return rewrite(captures, index, f"DIGEST {tampered}"), "DIGEST last nibble flipped"


def mutate_duplicate_key(captures: list[Capture]) -> tuple[list[Capture], str]:
    lines = lines_of(captures[0])
    indices = [i for i, line in enumerate(lines) if line.startswith("EFFECT ")]
    first = EFFECT_LINE.match(lines[indices[0]])
    second = EFFECT_LINE.match(lines[indices[1]])
    assert first is not None and second is not None
    line = lines[indices[1]].replace(f"EFFECT {second.group(1)} ", f"EFFECT {first.group(1)} ", 1)
    return rewrite(captures, indices[1], line), f"{second.group(1)} -> duplicate of {first.group(1)}"


def mutate_round_formula(captures: list[Capture]) -> tuple[list[Capture], str]:
    lines = lines_of(captures[0])
    index = next(i for i, line in enumerate(lines) if line.startswith("ROUND "))
    match = ROUND_LINE.match(lines[index])
    assert match is not None
    rounds = int(match.group(6))
    line = lines[index].replace(f"rounds={rounds} ", f"rounds={rounds + 1} ", 1)
    return rewrite(captures, index, line), f"{match.group(1)} rounds {rounds} -> {rounds + 1}"


def mutate_citation_wrong_line(captures: list[Capture]) -> tuple[list[Capture], str]:
    """A citation whose line exists but does not declare the effect key."""
    lines = lines_of(captures[0])
    index = next(i for i, line in enumerate(lines) if line.startswith("EFFECT "))
    match = EFFECT_LINE.match(lines[index])
    assert match is not None
    line = re.sub(r"src=\S+$", "src=crates/behavior/src/magic.rs:6", lines[index])
    return rewrite(captures, index, line), f"{match.group(1)} -> magic.rs:6 (no declaration)"


def mutate_citation_missing_file(captures: list[Capture]) -> tuple[list[Capture], str]:
    """A citation to a file that is not in the tree."""
    lines = lines_of(captures[0])
    index = next(i for i, line in enumerate(lines) if line.startswith("EFFECT "))
    match = EFFECT_LINE.match(lines[index])
    assert match is not None
    line = re.sub(
        r"src=\S+$", "src=crates/behavior/src/magic/effects/no_such_effect.rs:12", lines[index]
    )
    return rewrite(captures, index, line), f"{match.group(1)} -> missing effect file"


FALSIFIERS = {
    "census-gap": ("effect-rows", mutate_census_gap),
    "negative-without-path": ("row-citations", mutate_negative_without_path),
    "approximate-word": ("no-hedge-words", mutate_approximate_word),
    "persist-diverges": ("spellmaker", mutate_persist_diverges),
    "digest-tamper": ("digest-valid", mutate_digest_tamper),
    "duplicate-key": ("effect-rows", mutate_duplicate_key),
    "round-formula": ("rounds-formula", mutate_round_formula),
    "citation-wrong-line": ("row-citations", mutate_citation_wrong_line),
    "citation-missing-file": ("row-citations", mutate_citation_missing_file),
}


def falsify(mode: str, binary: Path, out: Path, captures: list[Capture], clean: list[Rule], as_json: bool) -> int:
    """Apply one mutation, require the named rule red, then restore and re-check."""
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
    if not as_json:
        print(f"FALSIFY {mode}: mutation={note}")
        print(f"FALSIFY {mode}: failed={','.join(failed) or 'none'}")
        print(f"FALSIFY {mode}: rule {target} {'red' if red else 'STAYED GREEN'}")
        print(f"FALSIFY {mode}: clean captures re-check={'PASS' if clean_again else 'FAIL'} (mutation restored)")
    else:
        print(json.dumps({"mode": mode, "target": target, "failed": failed, "red": red, "restored_clean": clean_again}))
    return 0 if red and clean_again else 1


# -- main -------------------------------------------------------------------


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

    if retail_dir() is None:
        print("VERDICT: SKIP - no retail install")
        return 2

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
        return falsify(args.falsify, binary, out, captures, rules, args.json)

    write_manifest(binary, captures, rules, out)
    if args.verbose:
        print(captures[0].stdout.decode("utf-8", "replace"))
    code = emit(binary, captures, rules, args.json)
    print("VERDICT: PASS" if code == 0 else "VERDICT: FAIL")
    return code


if __name__ == "__main__":
    sys.exit(main())
