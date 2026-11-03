#!/usr/bin/env python3
"""PROBE-SYS-MATRIX-001 - gate of M41/W41-0 (49-system matrix, gap G-4).

What it is
----------
The documentation instrument of W41-0. It re-measures the 49-system matrix page
row by row against the repository instead of trusting its prose: 49 rows exactly
(S01..S49, ascending, once each), a valid state per row, a non-empty evidence
cell and a non-empty "what is missing" cell, every repo path cited in the
evidence column present on disk, every `covered` row actually RUN green — each
probe named in its gate column is invoked as `<binary> --probe <TOKEN>` (exit 0
and no `FAIL` row) and each cited `scripts/*-001.py` is invoked with
`--binary <binary>` (exit 0) — a TOTALS line that equals the actual counts and
sums to 49, and the declared measurement date.

Why it is a Python gate and not a Rust probe
--------------------------------------------
Every other `PROBE-*-001` of this repo runs inside `daggerfall-app`; this one
measures a *document*, so its own process needs no retail data, no cargo build,
no GPU and no LLM. What it measures is nevertheless **behaviour, not prose**:
rule 6 executes the already-built `daggerfall-app` (`--binary`, default
`target/debug/daggerfall-app`) for every instrument a `covered` row names, so a
real regression in the engine turns this gate RED even when every file still
exists. The script is **one** program that
*is* the two-process runner: the parent process re-execs itself twice with
`PROBE_RUN=1` and `PROBE_RUN=2` (the value is never printed), captures both
stdouts, requires exit 0 + `VEREDICTO: PASS` + byte-identical output + equal
sha256 in both, and reprints run 1 verbatim so the exit-0 stdout is itself the
probe output the contract describes. The two-process evidence (sizes, both
sha256, the DIGEST) goes to stderr and into `manifest.json`; stdout carries only
the probe rows and the `DIGEST` line.

Deterministic process contract
------------------------------
1. Two independent processes (`PROBE_RUN=1|2`), byte-identical stdout (`cmp`
   equivalent) and equal sha256.
2. `VEREDICTO: PASS` and exit 0 in both; any FAIL row means exit 1.
3. The last line is `DIGEST <sha256>` equal to the sha256 of every byte printed
   before it.
4. Captures + `manifest.json` (both sha256 + DIGEST) are archived under
   `.sintergic/artifacts/m41-sys/sys-matrix-001/` (gitignored).

Falsifiers (`--falsify <name>`)
-------------------------------
Each mutates a TEMP COPY of the document (the repo copy is never touched),
requires its named rule to go red, then re-checks the clean document and
requires it green again:

    row-missing      -> rows-count
    row-duplicate    -> rows-count
    state-invalid    -> state-valid
    evidence-empty   -> evidence-nonempty
    path-not-found   -> paths-exist
    totals-mismatch  -> totals
    covered-probe-gone -> covered-probe

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
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
PROBE = "PROBE-SYS-MATRIX-001"
GATE = "SYS-MATRIX-001"
SLUG = "sys-matrix-001"
CR = "CR-98f1836e-0bf0-4017-9ba8-898c0037ff4f"
DEFAULT_DOC = ROOT / "docs" / "sintergic" / "l5-mission" / "m41-remeasure-49-systems-CR-98f1836e.md"
ARTIFACTS = ROOT / ".sintergic" / "artifacts" / "m41-sys" / SLUG
DISPATCH = ROOT / "crates" / "app" / "src" / "main.rs"
HEADER = f"{PROBE} - 49 systems re-measured (M41/W41-0, {CR})"

EXPECTED_SCHEMA = "M41-SYSTEM-MATRIX-v1"
EXPECTED_DATE = "2026-10-27"
EXPECTED_IDS = tuple(f"S{index:02d}" for index in range(1, 50))
STATES = ("covered", "partial", "missing")
FRONTMATTER_FIELDS = ("schema", "cr", "date", "target_head", "method")

DIGEST_LINE = re.compile(r"^DIGEST ([0-9a-f]{64})$")
ROW_LINE = re.compile(r"^\|\s*\*{0,2}(S\d{2})\*{0,2}\s*\|")
TOTALS_LINE = re.compile(
    r"^TOTALS: covered=(\d+) partial=(\d+) missing=(\d+) total=(\d+)$", re.MULTILINE
)
PATH_TOKEN = re.compile(r"(?:crates|scripts|docs|reference)/[A-Za-z0-9_./+-]+")
PROBE_TOKEN = re.compile(r"\b[A-Z][A-Z0-9]*(?:-[A-Z0-9]+)*-\d{3}\b")
SCRIPT_TOKEN = re.compile(r"scripts/[A-Za-z0-9_-]+-001\.py")
DISPATCH_ARM = re.compile(r'Some\("([^"]+)"\)')


# ── doc parsing ────────────────────────────────────────────────────────────


def split_frontmatter(doc: str) -> tuple[dict[str, str], str]:
    """Return (frontmatter, body). Missing/odd frontmatter yields {}."""
    lines = doc.splitlines()
    if not lines or lines[0].strip() != "---":
        return {}, doc
    for index in range(1, len(lines)):
        if lines[index].strip() == "---":
            fields: dict[str, str] = {}
            for line in lines[1:index]:
                if ":" in line:
                    key, value = line.split(":", 1)
                    fields[key.strip()] = value.strip()
            return fields, "\n".join(lines[index + 1 :])
    return {}, doc


def matrix_block(body: str) -> list[str]:
    """The §1 matrix only (§2 re-lists the same ids for the delta and must not
    be counted as data rows)."""
    lines = body.splitlines()
    start = None
    for index, line in enumerate(lines):
        if line.startswith("## §1"):
            start = index
            break
    if start is None:
        return []
    block: list[str] = []
    for line in lines[start + 1 :]:
        if line.startswith("## §2"):
            break
        block.append(line)
    return block


SENTINEL = "\x00PIPE\x00"


def split_cells(line: str) -> list[str]:
    """Split one markdown table row on unescaped pipes; `\\|` stays literal."""
    text = line.strip()
    if text.startswith("|"):
        text = text[1:]
    if text.endswith("|"):
        text = text[:-1]
    parts = text.replace("\\|", SENTINEL).split("|")
    return [part.replace(SENTINEL, "|").strip() for part in parts]


def data_rows(body: str) -> list[list[str]]:
    rows: list[list[str]] = []
    for line in matrix_block(body):
        if not ROW_LINE.match(line):
            continue
        rows.append(split_cells(line))
    return rows


def strip_inline_code(text: str) -> str:
    return text.replace("`", "")


def cited_paths(cell: str) -> list[str]:
    found: list[str] = []
    for raw in PATH_TOKEN.findall(strip_inline_code(cell)):
        token = raw.rstrip(".,;:)*+")
        # drop a trailing `:line` / `:line-line` selector
        token = re.sub(r":\d+(?:-\d+)?$", "", token)
        if token:
            found.append(token)
    return found


def dispatch_names() -> set[str]:
    if not DISPATCH.is_file():
        return set()
    return set(DISPATCH_ARM.findall(DISPATCH.read_text(encoding="utf-8", errors="replace")))


def default_binary(root: Path = ROOT) -> Path:
    return root / "target" / "debug" / "daggerfall-app"


PROBE_CACHE: dict[tuple[str, str], tuple[int, int, str]] = {}


def probe_run(binary: Path, token: str) -> tuple[int, int, str]:
    """Invoke one `--probe <token>` arm of the binary: coverage is measured by
    behaviour (exit 0, no FAIL row), never by textual existence."""
    key = (str(binary), token)
    if key in PROBE_CACHE:
        return PROBE_CACHE[key]
    try:
        proc = subprocess.run(
            [str(binary), "--probe", token], capture_output=True, text=True, timeout=900
        )
        fails = [line for line in proc.stdout.splitlines() if line.startswith("FAIL")]
        result = (proc.returncode, len(fails), fails[0][:70] if fails else "")
    except (OSError, subprocess.SubprocessError) as error:
        result = (127, 0, type(error).__name__)
    PROBE_CACHE[key] = result
    return result


def script_run(binary: Path, name: str) -> int:
    """Invoke one cited `scripts/*-001.py` gate against the same binary."""
    key = (str(binary), f"script:{name}")
    if key in PROBE_CACHE:
        return PROBE_CACHE[key][0]
    script = ROOT / "scripts" / name
    if not script.is_file():
        PROBE_CACHE[key] = (127, 0, "absent")
        return 127
    try:
        proc = subprocess.run(
            [sys.executable, str(script), "--binary", str(binary)],
            capture_output=True,
            text=True,
            timeout=1800,
        )
        code = proc.returncode
    except (OSError, subprocess.SubprocessError):
        code = 127
    PROBE_CACHE[key] = (code, 0, "")
    return code


# ── rules ──────────────────────────────────────────────────────────────────


class Rule:
    __slots__ = ("name", "ok", "detail")

    def __init__(self, name: str, ok: bool, detail: str) -> None:
        self.name = name
        self.ok = ok
        self.detail = detail


def evaluate(doc: str, root: Path = ROOT, binary_path: Path | None = None) -> list[Rule]:
    fields, body = split_frontmatter(doc)
    rows = data_rows(body)
    rules: list[Rule] = []

    # 1. frontmatter
    missing = [field for field in FRONTMATTER_FIELDS if not fields.get(field)]
    bad = [k for k, v in fields.items() if k not in FRONTMATTER_FIELDS]
    schema_ok = fields.get("schema") == EXPECTED_SCHEMA
    rules.append(
        Rule(
            "frontmatter",
            not missing and not bad and schema_ok,
            f"schema={fields.get('schema', '-')} fields={len(fields)}"
            f" missing={','.join(missing) or 'none'} extra={','.join(bad) or 'none'}",
        )
    )

    # 2. rows: exactly 49, S01..S49 ascending, once each
    ids = [strip_inline_code(cell[0]).strip("* ") if cell else "" for cell in rows]
    ok_rows = len(rows) == len(EXPECTED_IDS) and tuple(ids) == EXPECTED_IDS
    # the detail carries the document digest so the emitted bytes are bound to
    # the exact document measured (two different documents never emit the same
    # probe output)
    doc_sha = hashlib.sha256(doc.encode("utf-8")).hexdigest()[:32]
    rules.append(
        Rule(
            "rows-count",
            ok_rows,
            f"rows={len(rows)} ids={'ok' if tuple(ids) == EXPECTED_IDS else 'bad'} doc_sha256={doc_sha}",
        )
    )

    # 3. state in {covered,partial,missing}
    bad_states = [
        f"{ids[i] or '?'}={rows[i][2] if len(rows[i]) > 2 else '<none>'}"
        for i in range(len(rows))
        if len(rows[i]) < 3 or rows[i][2] not in STATES
    ]
    counts = {state: 0 for state in STATES}
    for row in rows:
        if len(row) > 2 and row[2] in counts:
            counts[row[2]] += 1
    rules.append(
        Rule(
            "state-valid",
            not bad_states,
            f"invalid={len(bad_states)}" + (f" [{' '.join(bad_states)}]" if bad_states else ""),
        )
    )

    # 4. evidence and "what is missing" cells non-empty (7 columns)
    empty: list[str] = []
    for index, row in enumerate(rows):
        if len(row) != 6:
            empty.append(f"{ids[index] or '?'}:columns={len(row)}")
            continue
        if not row[4].strip():
            empty.append(f"{ids[index]}:evidence")
        if not row[5].strip():
            empty.append(f"{ids[index]}:missing")
    rules.append(Rule("evidence-nonempty", not empty, f"empty={len(empty)}" + (f" [{' '.join(empty)}]" if empty else "")))

    # 5. every cited repo path exists on disk
    ghosts: list[str] = []
    cited = 0
    for row in rows:
        if len(row) < 5:
            continue
        for path in cited_paths(row[4]):
            cited += 1
            if not (root / path).exists():
                ghosts.append(path)
    rules.append(
        Rule(
            "paths-exist",
            not ghosts,
            f"cited={cited} missing={len(ghosts)}" + (f" [{' '.join(sorted(set(ghosts)))}]" if ghosts else ""),
        )
    )

    # 6. covered rows must RUN green, not merely exist: every probe named in the
    # gate column is invoked as `--probe <TOKEN>` and must exit 0 with no FAIL
    # row, and every cited `scripts/*-001.py` gate must exit 0 against the same
    # binary. Existence in a dispatch table is no longer accepted as proof.
    binary = Path(binary_path) if binary_path is not None else default_binary(root)
    failures: list[str] = []
    covered_rows = 0
    probe_tokens: set[str] = set()
    script_names: set[str] = set()
    if not binary.is_file():
        failures.append(f"binary-absent:{binary}")
        rules.append(
            Rule("covered-probe", False, f"covered=? probes=? scripts=? failed=1 [{failures[0]}]")
        )
    else:
        for index, row in enumerate(rows):
            if len(row) < 5 or row[2] != "covered":
                continue
            covered_rows += 1
            column = strip_inline_code(row[3])
            named = PROBE_TOKEN.findall(column)
            cited = [Path(hit).name for hit in SCRIPT_TOKEN.findall(column)]
            if not named and not cited:
                # a `covered` row must NAME the instrument that measures it:
                # a module that merely exists is not coverage
                failures.append(f"{ids[index]}:no-probe-named")
            probe_tokens.update(named)
            script_names.update(cited)
        for token in sorted(probe_tokens):
            code, fails, first = probe_run(binary, token)
            if code != 0 or fails:
                failures.append(f"{token}:exit={code},fails={fails}" + (f",{first}" if first else ""))
        for name in sorted(script_names):
            code = script_run(binary, name)
            if code != 0:
                failures.append(f"{name}:exit={code}")
        rules.append(
            Rule(
                "covered-probe",
                not failures,
                f"covered={covered_rows} probes={len(probe_tokens)} scripts={len(script_names)}"
                f" failed={len(failures)}" + (f" [{' '.join(failures)}]" if failures else ""),
            )
        )

    # 7. TOTALS line
    match = TOTALS_LINE.search(doc)
    if match is None:
        rules.append(Rule("totals", False, "line=absent got=none want='TOTALS: covered=.. partial=.. missing=.. total=49'"))
    else:
        declared = tuple(int(value) for value in match.groups())
        actual = (counts["covered"], counts["partial"], counts["missing"], len(rows))
        rules.append(Rule("totals", declared == actual and actual[3] == 49, f"declared={declared} actual={actual}"))

    # 8. date
    date = fields.get("date", "-")
    rules.append(Rule("date", date == EXPECTED_DATE, f"got={date} want={EXPECTED_DATE}"))

    return rules


def failed_names(rules: list[Rule]) -> list[str]:
    return [rule.name for rule in rules if not rule.ok]


# ── probe output ───────────────────────────────────────────────────────────


def render(doc: str, binary: Path | None = None) -> tuple[str, list[Rule]]:
    rules = evaluate(doc, binary_path=binary)
    lines = [HEADER]
    for rule in rules:
        if rule.ok:
            lines.append(f"[OK] {rule.name} - {rule.detail}")
        else:
            lines.append(f"FAIL {rule.name} got={rule.detail} want=green")
    verdict = "PASS" if not failed_names(rules) else "FAIL"
    lines.append(f"VEREDICTO: {verdict}")
    payload = "\n".join(lines) + "\n"
    digest = hashlib.sha256(payload.encode("utf-8")).hexdigest()
    return payload + f"DIGEST {digest}\n", rules


def digest_of(payload: str) -> str:
    body, _, last = payload.rpartition("DIGEST ")
    return last.strip() if body else ""


def child(doc_path: Path, binary: Path | None = None) -> int:
    try:
        doc = doc_path.read_text(encoding="utf-8")
    except OSError as error:
        print(f"FAIL doc - {error}")
        print("VEREDICTO: FAIL")
        return 2
    payload, rules = render(doc, binary)
    sys.stdout.write(payload)
    return 0 if not failed_names(rules) else 1


# ── two-process runner ─────────────────────────────────────────────────────


def run_twice(doc_path: Path, out: Path, verbose: bool, binary: Path | None = None) -> int:
    captures = []
    for which in (1, 2):
        env = dict(os.environ)
        env["PROBE_RUN"] = str(which)
        argv = [sys.executable, str(Path(__file__).resolve()), "--doc", str(doc_path)]
        if binary is not None:
            argv += ["--binary", str(binary)]
        completed = subprocess.run(
            argv,
            capture_output=True,
            env=env,
            cwd=str(ROOT),
        )
        captures.append(
            (
                completed.returncode,
                completed.stdout,
                completed.stderr,
            )
        )
        if completed.returncode == 2:
            sys.stderr.write(completed.stderr.decode("utf-8", "replace"))
            print("VEREDICTO: FAIL - capture process IO")
            return 2

    (code1, out1, err1), (code2, out2, err2) = captures
    text1 = out1.decode("utf-8")
    sha1 = hashlib.sha256(out1).hexdigest()
    sha2 = hashlib.sha256(out2).hexdigest()
    identical = out1 == out2
    pass1 = re.search(r"^VEREDICTO: PASS$", text1, re.MULTILINE) is not None
    pass2 = re.search(r"^VEREDICTO: PASS$", out2.decode("utf-8", "replace"), re.MULTILINE) is not None
    digest_ok = True
    digest_value = ""
    match = DIGEST_LINE.search(text1.splitlines()[-1]) if text1.splitlines() else None
    if match:
        digest_value = match.group(1)
        prefix = out1[: out1.rfind(b"DIGEST ")]
        digest_ok = hashlib.sha256(prefix).hexdigest() == digest_value
    else:
        digest_ok = False

    out.mkdir(parents=True, exist_ok=True)
    (out / "run1.txt").write_bytes(out1)
    (out / "run2.txt").write_bytes(out2)
    manifest = {
        "schema": "SYS-MATRIX-001-MANIFEST-v1",
        "gate": GATE,
        "probe": PROBE,
        "cr": CR,
        "doc": str(doc_path.relative_to(ROOT)) if doc_path.is_relative_to(ROOT) else str(doc_path),
        "doc_sha256": hashlib.sha256(doc_path.read_bytes()).hexdigest(),
        "runs": 2,
        "exit_codes": [code1, code2],
        "stdout_bytes": [len(out1), len(out2)],
        "stdout_sha256": [sha1, sha2],
        "identical": identical,
        "verdict_pass": [pass1, pass2],
        "digest": digest_value,
        "digest_self_consistent": digest_ok,
    }
    (out / "manifest.json").write_text(json.dumps(manifest, indent=2, sort_keys=True) + "\n", encoding="utf-8")

    sys.stderr.write(f"{PROBE}: run1 bytes={len(out1)} sha256={sha1}\n")
    sys.stderr.write(f"{PROBE}: run2 bytes={len(out2)} sha256={sha2}\n")
    sys.stderr.write(f"{PROBE}: identical={identical} digest={digest_value} self-consistent={digest_ok}\n")
    if err1.strip():
        sys.stderr.write(err1.decode("utf-8", "replace"))
    if verbose:
        for line in text1.splitlines():
            sys.stderr.write(f"run1| {line}\n")

    sys.stdout.write(text1)
    green = code1 == 0 and code2 == 0 and pass1 and pass2 and identical and sha1 == sha2 and digest_ok
    if not green:
        # the emitted stdout already carries the failing rows; mark the verdict
        # only when both runs agreed, so determinism is never masked
        return 1 if identical else 1
    return 0


# ── falsifiers ─────────────────────────────────────────────────────────────


def _replace_row(doc: str, needle_prefix: str, new_line: str | None) -> str:
    lines = doc.splitlines(keepends=True)
    for index, line in enumerate(lines):
        if line.startswith(needle_prefix):
            if new_line is None:
                del lines[index]
            else:
                lines[index] = new_line
            return "".join(lines)
    raise KeyError(needle_prefix)


def _cells(line: str) -> list[str]:
    return line.strip().strip("|").split("|")


def _rebuild(cells: list[str]) -> str:
    return "|" + "|".join(cells) + "|\n"


def mutate_row_missing(doc: str) -> tuple[str, str]:
    return _replace_row(doc, "| S25 |", None), "deleted the S25 data row"


def mutate_row_duplicate(doc: str) -> tuple[str, str]:
    lines = doc.splitlines(keepends=True)
    for index, line in enumerate(lines):
        if line.startswith("| S10 |"):
            lines.insert(index + 1, line)
            return "".join(lines), "duplicated the S10 data row"
    raise KeyError("S10")


def mutate_state_invalid(doc: str) -> tuple[str, str]:
    lines = doc.splitlines(keepends=True)
    for index, line in enumerate(lines):
        if line.startswith("| S12 |"):
            cells = _cells(line)
            cells[2] = " done "
            lines[index] = _rebuild(cells)
            return "".join(lines), "S12 state 'partial' -> 'done'"
    raise KeyError("S12")


def mutate_evidence_empty(doc: str) -> tuple[str, str]:
    lines = doc.splitlines(keepends=True)
    for index, line in enumerate(lines):
        if line.startswith("| S13 |"):
            cells = _cells(line)
            cells[4] = " "
            lines[index] = _rebuild(cells)
            return "".join(lines), "S13 evidence cell emptied"
    raise KeyError("S13")


def mutate_path_not_found(doc: str) -> tuple[str, str]:
    ghost = "crates/does-not-exist/ghost_system.rs:7"
    lines = doc.splitlines(keepends=True)
    for index, line in enumerate(lines):
        if line.startswith("| S15 |"):
            cells = _cells(line)
            cells[4] = f" `{ghost}` "
            lines[index] = _rebuild(cells)
            return "".join(lines), f"S15 evidence cites absent path {ghost}"
    raise KeyError("S15")


def mutate_totals(doc: str) -> tuple[str, str]:
    """Shift one row from `partial` to `covered` in the declared TOTALS.

    The mutated numbers are derived from the line actually declared, so the
    falsifier never carries a stale literal (the previous one kept claiming
    "covered 43 -> 44" long after the measured totals moved).
    """
    match = TOTALS_LINE.search(doc)
    if match is None:
        raise KeyError("TOTALS")
    covered, partial, missing, total = (int(value) for value in match.groups())
    mutated = TOTALS_LINE.sub(
        f"TOTALS: covered={covered + 1} partial={max(partial - 1, 0)}"
        f" missing={missing} total={total}",
        doc,
        count=1,
    )
    return mutated, (
        f"TOTALS covered {covered} -> {covered + 1}"
        f" (partial {partial} -> {max(partial - 1, 0)})"
    )


def mutate_covered_probe_gone(doc: str) -> tuple[str, str]:
    lines = doc.splitlines(keepends=True)
    for index, line in enumerate(lines):
        if line.startswith("| S18 |"):
            cells = _cells(line)
            cells[3] = " NOPE-999 "
            lines[index] = _rebuild(cells)
            return "".join(lines), "S18 probe/gate -> NOPE-999 (absent from dispatch)"
    raise KeyError("S18")


FALSIFIERS: dict[str, tuple[str, callable]] = {
    "row-missing": ("rows-count", mutate_row_missing),
    "row-duplicate": ("rows-count", mutate_row_duplicate),
    "state-invalid": ("state-valid", mutate_state_invalid),
    "evidence-empty": ("evidence-nonempty", mutate_evidence_empty),
    "path-not-found": ("paths-exist", mutate_path_not_found),
    "totals-mismatch": ("totals", mutate_totals),
    "covered-probe-gone": ("covered-probe", mutate_covered_probe_gone),
}


def falsify(mode: str, doc: str, out: Path, binary: Path | None = None) -> int:
    target, mutate = FALSIFIERS[mode]
    mutated, note = mutate(doc)
    red = target in failed_names(evaluate(mutated, binary_path=binary))
    clean_again = not failed_names(evaluate(doc, binary_path=binary))
    out.mkdir(parents=True, exist_ok=True)
    (out / f"falsify-{mode}.md").write_text(mutated, encoding="utf-8")
    (out / f"falsify-{mode}.json").write_text(
        json.dumps(
            {"mode": mode, "target": target, "mutation": note, "red": red, "restored_clean": clean_again},
            indent=2,
            sort_keys=True,
        )
        + "\n",
        encoding="utf-8",
    )
    print(f"FALSIFY {mode}: mutation={note}")
    print(f"FALSIFY {mode}: rule {target} {'red' if red else 'STAYED GREEN'}")
    print(f"FALSIFY {mode}: clean document re-check={'PASS' if clean_again else 'FAIL'} (copy only, repo untouched)")
    return 0 if red and clean_again else 1


# ── main ───────────────────────────────────────────────────────────────────


def main() -> int:
    parser = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter
    )
    parser.add_argument("--doc", default=str(DEFAULT_DOC), help="matrix document to measure")
    parser.add_argument(
        "--binary",
        default=str(default_binary()),
        help="built daggerfall-app used to RUN every probe the matrix accredits",
    )
    parser.add_argument("--out", default=str(ARTIFACTS), help="artifact root (captures + manifest.json)")
    parser.add_argument("--falsify", choices=sorted(FALSIFIERS), help="mutate a temp copy and require a red rule")
    parser.add_argument("--verbose", action="store_true", help="echo run 1 to stderr")
    args = parser.parse_args()

    binary = Path(args.binary).expanduser()
    if not binary.is_absolute():
        binary = ROOT / binary

    doc_path = Path(args.doc)
    if not doc_path.is_absolute():
        doc_path = ROOT / doc_path

    if not doc_path.is_file():
        print(f"FAIL doc - absent {doc_path}")
        print("VEREDICTO: FAIL")
        return 2
    try:
        doc = doc_path.read_text(encoding="utf-8")
    except OSError as error:
        print(f"FAIL doc - {error}")
        print("VEREDICTO: FAIL")
        return 2

    if args.falsify:
        with tempfile.TemporaryDirectory(prefix="sys-matrix-falsify-") as scratch:
            target_out = Path(args.out) if args.out != str(ARTIFACTS) else Path(scratch)
            return falsify(args.falsify, doc, target_out, binary)

    # child mode: PROBE_RUN is set by the parent and is never printed
    if os.environ.get("PROBE_RUN"):
        return child(doc_path, binary)

    return run_twice(doc_path, Path(args.out), args.verbose, binary)


if __name__ == "__main__":
    sys.exit(main())
