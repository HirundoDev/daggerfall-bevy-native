#!/usr/bin/env python3
"""`m43_gate_lib` — the shared harness of the seven M43 gates (G1..G12).

Authoring lane: **Lane A** (`CR-39d656e1-1987-4c43-aa7c-149f6f96d2cb`). The
contract of the executor puts the gate and its implementation in different
lanes: this module and every `scripts/sys-*-001.py` gate of M43 are written
**before** the depth exists, and the implementing lane (Lane B) may not edit
them.

What the harness guarantees, per gate (the numbers are the pack's G-ids):

* **G2 behaviour, not literals** — every affirmation is computed over the
  bytes a *separate process* printed (`subprocess`), never over text the gate
  itself composed.
* **G3 binary↔source binding** — the probe must print
  ``SOURCES fingerprint=<64 hex> paths=<a,b>`` where the fingerprint is sha256
  over the sorted ``<path>NUL<sha256(bytes)>`` lines of the sources it baked in
  with `include_bytes!`. The gate recomputes it from disk and demands MATCH.
  The gate never computes the fingerprint on behalf of the probe: an absent
  `SOURCES` line is a named `[MISSING]` row, not a silent green.
* **G4 build by default** — `cargo build -p daggerfall-app` under one `flock`
  lock (`--no-build` is admitted only when G3 gave MATCH).
* **G5 determinism** — two independent processes, byte-identical stdout, and a
  terminal ``DIGEST <sha256>`` line recomputed by the gate over every byte
  printed before it.
* **G7 resolved citations** — every `src=<ruta>:<línea>` the gate prints, and
  every `src=` a probe row prints, is resolved against the tree (path exists,
  line <= line count). A citation valid only by shape is a FAIL.
* **G10 exit codes** — 0 all green, 1 >=1 red, 2 environment absent (NC rows
  only, never green), 3 rejection (unbound binary / bad usage).
* **G11 pre-implementation** — an absent depth row prints
  ``[MISSING] <id> - <what must be produced>``; the gate never aborts because
  a row is missing.
* **G12 zero retail in the verdict** — rows fed by a retail (ARENA2) block are
  marked retail: when the probe reports EXIT=2 (ARENA2 absent) they are NC,
  never red and never green.

The module holds no family knowledge: each gate supplies its probe names, its
row grammar, the depth rows Lane B must produce and its falsifier catalogue.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import subprocess
import sys
import time
from dataclasses import dataclass, field
from pathlib import Path
from typing import Callable, Iterable, Sequence

ROOT = Path(__file__).resolve().parent.parent
BUILD_LOCK = ROOT / ".sintergic" / "locks" / "m43-build.lock"
ARTIFACT_ROOT = ROOT / "target" / "m43-gates"
BINARY = ROOT / "target" / "debug" / "daggerfall-app"

EXIT_GREEN = 0
EXIT_RED = 1
EXIT_ENV = 2
EXIT_REJECT = 3

SOURCES_LINE = re.compile(r"^SOURCES fingerprint=([0-9a-f]{64}) paths=(\S+)$")
DIGEST_LINE = re.compile(r"^DIGEST ([0-9a-f]{64})$")
OK_LINE = re.compile(r"^\[OK\] (\S+) - (.*)$")
CITE_SUFFIX = re.compile(r"^(.*?)(?::(\d+)(?:-(\d+))?)?$")


# ── small helpers ──────────────────────────────────────────────────────────


def sha256_hex(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def file_sha256(path: Path) -> str | None:
    if not path.is_file():
        return None
    hasher = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1 << 20), b""):
            hasher.update(chunk)
    return hasher.hexdigest()


def run(cmd: Sequence[str], timeout: int = 3600) -> subprocess.CompletedProcess:
    return subprocess.run(
        list(cmd),
        cwd=str(ROOT),
        capture_output=True,
        timeout=timeout,
        check=False,
    )


def head_commit() -> str | None:
    proc = run(["git", "rev-parse", "HEAD"], timeout=60)
    if proc.returncode != 0:
        return None
    return proc.stdout.decode("utf-8", "replace").strip() or None


def git_status(paths: Sequence[str]) -> str:
    proc = run(["git", "status", "--porcelain", "--", *paths], timeout=120)
    return proc.stdout.decode("utf-8", "replace")


def build(no_build: bool) -> tuple[str, str]:
    """`(state, detail)`; state in {rebuilt, skipped, failed}."""
    if no_build:
        return "skipped", "--no-build (G4: admitted only with a G3 MATCH)"
    BUILD_LOCK.parent.mkdir(parents=True, exist_ok=True)
    proc = run(["flock", str(BUILD_LOCK), "cargo", "build", "-p", "daggerfall-app"])
    if proc.returncode != 0:
        tail = proc.stderr.decode("utf-8", "replace").strip().splitlines()[-6:]
        return "failed", " | ".join(tail)
    return "rebuilt", "cargo build -p daggerfall-app (flock .sintergic/locks/m43-build.lock)"


# ── records ────────────────────────────────────────────────────────────────


@dataclass(frozen=True)
class Capture:
    probe: str
    run: int
    exit: int
    stdout: bytes
    stderr: bytes

    @property
    def lines(self) -> list[str]:
        return self.stdout.decode("utf-8", "replace").splitlines()

    @property
    def text(self) -> str:
        return self.stdout.decode("utf-8", "replace")


@dataclass(frozen=True)
class Check:
    name: str
    ok: bool
    detail: str
    cite: str | None = None
    missing: bool = False
    retail: bool = False


@dataclass(frozen=True)
class RowSpec:
    """A row the probe already prints today (absent => FAIL)."""

    id: str
    pattern: str
    check: Callable[[list[re.Match]], tuple[bool, str]]
    expect: str
    cite: str | None = None
    probe: str | None = None
    retail: bool = False


@dataclass(frozen=True)
class DepthRow:
    """A row section 3.8 demands and Lane B must produce (absent => [MISSING])."""

    id: str
    description: str
    pattern: str
    check: Callable[[list[re.Match]], tuple[bool, str]] | None = None
    probe: str | None = None
    retail: bool = False


@dataclass(frozen=True)
class Falsifier:
    """G9: a byte-exact source mutation that must turn `target` red."""

    name: str
    path: str
    before: str
    after: str
    target: str
    description: str


# ── row-form helpers (used by every gate) ──────────────────────────────────


def expect_equal(**want: str) -> Callable[[list[re.Match]], tuple[bool, str]]:
    """Named groups must equal the declared values; exactly one match."""

    def check(matches: list[re.Match]) -> tuple[bool, str]:
        if not matches:
            return False, "absent"
        row = matches[0]
        got = " ".join(f"{key}={row[key]}" for key in want)
        bad = [f"{key}={row[key]}!={value}" for key, value in want.items() if row[key] != value]
        return (not bad), (got if not bad else f"{got} mismatch={','.join(bad)}")

    return check


def expect_same(group_a: str, group_b: str, label: str = "") -> Callable[[list[re.Match]], tuple[bool, str]]:
    """Two groups of one row must carry the same value."""

    def check(matches: list[re.Match]) -> tuple[bool, str]:
        if not matches:
            return False, "absent"
        row = matches[0]
        left, right = row[group_a], row[group_b]
        return left == right, f"{label or group_a}={left[:24]} vs {right[:24]} equal={left == right}"

    return check


def ids_exact(want: Sequence[str], group: str = "id") -> Callable[[list[re.Match]], tuple[bool, str]]:
    """G8: the population is enumerated, not only counted."""

    def check(matches: list[re.Match]) -> tuple[bool, str]:
        got = [m[group] for m in matches]
        want_list = list(want)
        if got == want_list:
            return True, f"ids={len(got)}/{len(want_list)} first={got[0] if got else '-'} last={got[-1] if got else '-'}"
        missing = [w for w in want_list if w not in got]
        extra = [g for g in got if g not in want_list]
        return False, (
            f"ids={len(got)}/{len(want_list)} missing={','.join(missing[:6]) or 'none'} "
            f"extra={','.join(extra[:6]) or 'none'}"
        )

    return check


def census_exact(group: str, want: Sequence[str], sep: str = ";") -> Callable[[re.Match], tuple[bool, str]]:
    """G8: the census population is compared member by member."""

    def check(match: re.Match) -> tuple[bool, str]:
        got = match[group].split(sep) if match[group] else []
        want_list = list(want)
        if got == want_list:
            return True, f"census={len(got)}/{len(want_list)}"
        missing = [w for w in want_list if w not in got]
        extra = [g for g in got if g not in want_list]
        return False, (
            f"census={len(got)}/{len(want_list)} missing={','.join(missing[:6]) or 'none'} "
            f"extra={','.join(extra[:6]) or 'none'}"
        )

    return check


def every(group: str, want: Sequence[str]) -> Callable[[list[re.Match]], tuple[bool, str]]:
    """Every match must carry one of `want` in `group`; all of `want` present."""

    def check(matches: list[re.Match]) -> tuple[bool, str]:
        got = [m[group] for m in matches]
        want_list = list(want)
        missing = [w for w in want_list if w not in got]
        bad = [g for g in got if g not in want_list]
        ok = not missing and not bad and len(got) == len(want_list)
        return ok, (
            f"rows={len(got)}/{len(want_list)} missing={','.join(missing[:6]) or 'none'} "
            f"unknown={','.join(bad[:6]) or 'none'}"
        )

    return check


def value_equal(group: str = "id") -> Callable[[list[re.Match]], tuple[bool, str]]:
    """`expected|actual` vector rows: both halves must agree."""

    def check(matches: list[re.Match]) -> tuple[bool, str]:
        bad = [m[group] for m in matches if m["expected"] != m["actual"]]
        return (not bad), f"rows={len(matches)} mismatch={','.join(bad[:6]) or 'none'}"

    return check


def all_vector_ok(group: str = "id") -> Callable[[list[re.Match]], tuple[bool, str]]:
    """`<id>|ok` vector rows: none may say anything but `ok`."""

    def check(matches: list[re.Match]) -> tuple[bool, str]:
        bad = [m[group] for m in matches if m["verdict"] != "ok"]
        return (not bad), f"rows={len(matches)} not-ok={','.join(bad[:6]) or 'none'}"

    return check


# ── citation resolution (G7) ───────────────────────────────────────────────


def resolve_cite(raw: str) -> tuple[bool, str]:
    """`path:line` must exist and the line must exist in that file."""
    token = raw.strip().strip("`'\"()[].,;")
    match = CITE_SUFFIX.match(token)
    if match is None:
        return False, f"unparsable={token}"
    path_text, first, last = match.group(1), match.group(2), match.group(3)
    if first is None:
        return False, f"no-line={token}"
    path = Path(path_text)
    if not path.is_absolute():
        path = ROOT / path
    if not path.is_file():
        return False, f"absent={path_text}"
    with path.open("rb") as handle:
        total = sum(1 for _ in handle)
    top = int(last or first)
    if top < 1 or top > total:
        return False, f"line>{total}={token}"
    return True, f"ok={path_text}:{first}"


def citation_check(cites: Iterable[str]) -> tuple[bool, str]:
    bad: list[str] = []
    good = 0
    for cite in cites:
        ok, detail = resolve_cite(cite)
        if ok:
            good += 1
        else:
            bad.append(detail)
    return (not bad), f"cites={good} unresolved={','.join(bad[:6]) or 'none'}"


def ok_row_id(line: str) -> str:
    """`[OK] <id> - <detail>` -> `<id>`; a clause-style label stops at ` - `."""
    match = OK_LINE.match(line)
    if match is not None:
        return match.group(1)
    return line[len("[OK] ") :].split(" - ")[0]


def probe_citations(text: str) -> tuple[bool, str]:
    """G7 over the probe's own `[OK]` rows: each needs a resolved `src=`."""
    rows = [line for line in text.splitlines() if line.startswith("[OK] ")]
    uncited: list[str] = []
    unresolved: list[str] = []
    for line in rows:
        match = OK_LINE.match(line)
        # `OK_LINE` demands a single-token id (`[OK] A1 - detail`), but this probe
        # family labels its rows with the full clause (`[OK] A9 negative: buy
        # without gold - detail`).  Widen only: rows OK_LINE cannot split are
        # searched whole and named by their id prefix.  Matched rows parse exactly
        # as before (no OK->FAIL flip); an unmatched row was always `uncited`, so
        # it becomes green only when it really carries a resolvable `src=`.
        body = match.group(2) if match else line[len("[OK] ") :]
        tokens = re.findall(r"src=(\S+)", body)
        if not tokens:
            uncited.append(match.group(1) if match else line[len("[OK] ") :].split(" - ")[0])
            continue
        for token in tokens:
            ok, detail = resolve_cite(token)
            if not ok:
                # Name the ROW whose citation failed, not only the reason: a red
                # citation must be attributable from the row itself instead of a
                # blind `absent=...` tail (WUD-158/A3b double-direction guard).
                unresolved.append(f"{ok_row_id(line)}:{detail}")
    ok = not uncited and not unresolved
    return ok, (
        f"ok-rows={len(rows)} without-src={len(uncited)} "
        f"unresolved={len(unresolved)} first={','.join(uncited[:4] + unresolved[:2]) or 'none'}"
    )


GUARD_ABSENT_SUFFIX = ".m43-citation-guard-absent"


def _cite_anchor(text: str) -> tuple[int, str, str] | None:
    """First `(line index, src token, row id)` of the stdout that resolves today."""
    for index, line in enumerate(text.split("\n")):
        if not line.startswith("[OK] "):
            continue
        for token in re.findall(r"src=(\S+)", line):
            if resolve_cite(token)[0]:
                return index, token, ok_row_id(line)
    return None


def _doctor(text: str, index: int, token: str, replacement: str) -> str:
    """Same stdout with ONE citation rewritten: the rest is byte-identical."""
    lines = text.split("\n")
    lines[index] = lines[index].replace(f"src={token}", f"src={replacement}", 1)
    return "\n".join(lines)


def probe_citations_guard(text: str) -> tuple[bool, str]:
    """G7 in BOTH directions, on the probe's own real stdout of this very run.

    (a) the real rows resolve — the same verdict `citations-<PROBE>` prints;
    (b) two doctored copies of that SAME text must go RED *and name the row*:
        (i)  `src=` naming a file that does not exist  -> `<row>:absent=...`
        (ii) `src=` naming one line past the real file -> `<row>:line><n>=...`

    Green only when all three hold, so a citation check that stopped biting can
    no longer hide behind a green `citations-<PROBE>` row: that decorative check
    is exactly what this row exists to kill.
    """
    real_ok, real_detail = probe_citations(text)
    if not real_ok:
        return False, f"real-citations=RED {real_detail}"
    anchor = _cite_anchor(text)
    if anchor is None:
        return False, (
            "no-anchor: not one `[OK] <row>` line of the probe stdout carries a "
            "resolvable `src=` token, so the citation check resolves nothing in "
            "either direction (decorative: nothing can be wrong)"
        )
    index, token, row = anchor
    parsed = CITE_SUFFIX.match(token.strip().strip("`'\"()[].,;"))
    if parsed is None or parsed.group(2) is None:
        return False, f"unparsable-anchor={token}"
    path_text, first = parsed.group(1), parsed.group(2)
    path = Path(path_text)
    if not path.is_absolute():
        path = ROOT / path_text
    total = sum(1 for _ in path.open("rb"))
    absent_text = f"{path_text}{GUARD_ABSENT_SUFFIX}"
    absent = f"{absent_text}:{first}"
    if (ROOT / absent_text).exists():
        return False, f"anchor-collision: {absent_text} exists, the absent direction cannot be built"
    invented = f"{path_text}:{total + 1}"
    problems: list[str] = []
    evidence: list[str] = []
    for label, replacement, marker in (
        ("absent", absent, f"{row}:absent="),
        (f"line>{total}", invented, f"{row}:line>{total}="),
    ):
        verdict, detail = probe_citations(_doctor(text, index, token, replacement))
        if verdict:
            problems.append(f"{label}=STAYED-GREEN(src={replacement})")
        elif marker not in detail:
            problems.append(f"{label}=RED-unattributed({detail[:72]})")
        else:
            # `without-src`/`unresolved` must NAME the row: quote the attribution.
            evidence.append(f"{label}=RED({detail[detail.index(marker):]})")
    if problems:
        return False, f"anchor={row}@{path_text}:{first} " + " ".join(problems)
    return True, f"anchor={row}@{path_text}:{first} " + " ".join(evidence) + f" real={real_detail}"


# ── the gate ───────────────────────────────────────────────────────────────


@dataclass
class Gate:
    slug: str
    gate_id: str
    cr: str
    probes: Sequence[str]
    rows: Sequence[RowSpec]
    depth: Sequence[DepthRow]
    falsifiers: dict[str, Falsifier]
    sources_hint: Sequence[str]
    spec_path: str
    retail_probes: Sequence[str] = field(default_factory=tuple)

    # ── capture ────────────────────────────────────────────────────────────

    def capture(self, binary: Path) -> dict[str, list[Capture]]:
        captured: dict[str, list[Capture]] = {}
        for probe in self.probes:
            runs: list[Capture] = []
            for run_id in (1, 2):
                proc = subprocess.run(
                    [str(binary), "--probe", probe],
                    cwd=str(ROOT),
                    capture_output=True,
                    timeout=1800,
                    check=False,
                    env={**os.environ, "PROBE_RUN": str(run_id), "PROBE_GATE": self.gate_id},
                )
                runs.append(
                    Capture(
                        probe=probe,
                        run=run_id,
                        exit=proc.returncode,
                        stdout=proc.stdout,
                        stderr=proc.stderr,
                    )
                )
            captured[probe] = runs
        return captured

    # ── checks ─────────────────────────────────────────────────────────────

    def harness_checks(self, captured: dict[str, list[Capture]]) -> tuple[list[Check], bool]:
        checks: list[Check] = []
        unbound = False
        for probe in self.probes:
            first, second = captured[probe]
            retail = probe in self.retail_probes
            checks.append(
                Check(
                    f"process-{probe}",
                    first.exit == second.exit and first.exit in (0, 2),
                    f"exit={first.exit}/{second.exit} rc-ok={first.exit in (0, 2)}",
                    retail=retail,
                )
            )
            checks.append(
                Check(
                    f"byte-identical-{probe}",
                    first.stdout == second.stdout,
                    f"bytes={len(first.stdout)}/{len(second.stdout)} equal={first.stdout == second.stdout}",
                    retail=retail,
                )
            )
            checks.append(
                Check(
                    f"stdout-sha256-{probe}",
                    sha256_hex(first.stdout) == sha256_hex(second.stdout),
                    f"sha256={sha256_hex(first.stdout)[:16]}",
                    retail=retail,
                )
            )

            digest_ok, digest_detail = self.digest_verdict(first)
            checks.append(
                Check(
                    f"DIGEST-LINE-{probe}",
                    digest_ok,
                    digest_detail,
                    missing=digest_detail.startswith("absent"),
                    retail=retail,
                )
            )

            bound, bind_detail = self.source_verdict(first)
            if bind_detail.startswith("mismatch"):
                unbound = True
            checks.append(
                Check(
                    f"SOURCES-FINGERPRINT-{probe}",
                    bound,
                    bind_detail,
                    missing=bind_detail.startswith("absent"),
                    retail=retail,
                )
            )

            cited, cite_detail = probe_citations(first.text)
            checks.append(Check(f"citations-{probe}", cited, cite_detail, retail=retail))
            guarded, guard_detail = probe_citations_guard(first.text)
            checks.append(Check(f"citations-guard-{probe}", guarded, guard_detail, retail=retail))

            floats = sorted({m.group(0) for m in re.finditer(r"\b[a-z][a-z0-9_]*=-?\d+\.\d+", first.text)})
            checks.append(
                Check(
                    f"FLOATS-{probe}",
                    not floats,
                    f"key=float tokens={len(floats)} {','.join(floats[:4]) or 'none'} (G5: no floats on stdout)",
                    retail=retail,
                )
            )

            fails = [line for line in first.lines if line.startswith("FAIL ")]
            checks.append(
                Check(
                    f"no-fail-rows-{probe}",
                    not fails,
                    f"fail-rows={len(fails)} first={(fails[0][:60] if fails else 'none')}",
                    retail=retail,
                )
            )
            checks.append(
                Check(
                    f"environment-{probe}",
                    True,
                    f"arena2={'absent' if first.exit == 2 else 'present'}",
                    retail=retail,
                )
            )
        return checks, unbound

    def digest_verdict(self, capture: Capture) -> tuple[bool, str]:
        parts = capture.stdout.split(b"\n")
        while parts and parts[-1] == b"":
            parts.pop()
        if not parts:
            return False, "absent: empty stdout"
        last = parts[-1].decode("utf-8", "replace")
        match = DIGEST_LINE.match(last)
        if match is None:
            return False, (
                "absent: probe must print a terminal line `DIGEST <sha256>` over every "
                f"byte printed before it (last line was {last[:48]!r})"
            )
        body = b"\n".join(parts[:-1]) + b"\n"
        expected = sha256_hex(body)
        if match.group(1) != expected:
            return False, f"mismatch declared={match.group(1)[:16]} recomputed={expected[:16]}"
        return True, f"sha256={expected[:16]} recomputed=match"

    def source_verdict(self, capture: Capture) -> tuple[bool, str]:
        found = [SOURCES_LINE.match(line) for line in capture.lines]
        found = [match for match in found if match is not None]
        if not found:
            return False, (
                "absent: probe must print `SOURCES fingerprint=<64 hex> paths=<a,b>` "
                f"over its `include_bytes!` set ({','.join(self.sources_hint)})"
            )
        declared, paths = found[0].group(1), found[0].group(2).split(",")
        recomputed, detail = self.recompute_fingerprint(paths)
        if recomputed is None:
            return False, f"mismatch unreadable={detail}"
        if recomputed != declared:
            return False, f"mismatch declared={declared[:16]} recomputed={recomputed[:16]} ({detail})"
        return True, f"fingerprint={recomputed[:16]} paths={len(paths)} MATCH"

    @staticmethod
    def recompute_fingerprint(paths: Sequence[str]) -> tuple[str | None, str]:
        parts: list[str] = []
        for raw in paths:
            path = Path(raw.strip())
            if not path.is_absolute():
                path = ROOT / path
            digest = file_sha256(path)
            if digest is None:
                return None, raw
            parts.append(f"{raw.strip()}\x00{digest}")
        parts.sort()
        return sha256_hex("\n".join(parts).encode()), f"{len(parts)} files"

    # ── family rows ────────────────────────────────────────────────────────

    def row_checks(self, captured: dict[str, list[Capture]]) -> list[Check]:
        checks: list[Check] = []
        for spec in self.rows:
            probe = spec.probe or self.probes[0]
            matches = [m for m in re.finditer(spec.pattern, captured[probe][0].text, re.MULTILINE)]
            if not matches:
                checks.append(
                    Check(spec.id, False, f"absent from {probe} stdout", spec.cite, retail=spec.retail)
                )
                continue
            ok, detail = spec.check(matches)
            checks.append(Check(spec.id, ok, detail, spec.cite, retail=spec.retail))
        for spec in self.depth:
            probe = spec.probe or self.probes[0]
            matches = [m for m in re.finditer(spec.pattern, captured[probe][0].text, re.MULTILINE)]
            if not matches:
                checks.append(
                    Check(
                        spec.id,
                        False,
                        f"{spec.description} ~~ measured on --probe {probe}, never on a fallback probe",
                        missing=True,
                        retail=spec.retail,
                    )
                )
                continue
            if spec.check is None:
                checks.append(Check(spec.id, True, f"rows={len(matches)}", retail=spec.retail))
                continue
            ok, detail = spec.check(matches)
            checks.append(Check(spec.id, ok, detail, retail=spec.retail))
        return checks

    def all_checks(self, captured: dict[str, list[Capture]]) -> tuple[list[Check], bool]:
        harness, unbound = self.harness_checks(captured)
        return harness + self.row_checks(captured), unbound

    # ── reporting ──────────────────────────────────────────────────────────

    def digests(self, captured: dict[str, list[Capture]]) -> dict[str, str]:
        return {probe: sha256_hex(runs[0].stdout) for probe, runs in captured.items()}

    def emit(
        self,
        checks: list[Check],
        captured: dict[str, list[Capture]],
        *,
        binary: Path,
        build_state: str,
        build_detail: str,
        as_json: bool,
        verbose: bool,
    ) -> None:
        plain_fail = [check.name for check in checks if not check.ok and not check.missing]
        missing = [check.name for check in checks if check.missing]
        red = plain_fail + missing
        digests = self.digests(captured)
        if as_json:
            print(json.dumps(
                {
                    "gate": self.gate_id,
                    "cr": self.cr,
                    "spec": self.spec_path,
                    "slug": self.slug,
                    "commit": head_commit(),
                    "build": {"state": build_state, "detail": build_detail},
                    "binary": {"path": os.path.relpath(binary, ROOT), "sha256": file_sha256(binary)},
                    "probes": {
                        probe: {
                            "runs": [
                                {
                                    "run": item.run,
                                    "exit": item.exit,
                                    "bytes": len(item.stdout),
                                    "stdout_sha256": sha256_hex(item.stdout),
                                }
                                for item in runs
                            ],
                            "byte_identical": runs[0].stdout == runs[1].stdout,
                        }
                        for probe, runs in captured.items()
                    },
                    "rows": [
                        {
                            "name": check.name,
                            "ok": check.ok,
                            "missing": check.missing,
                            "retail": check.retail,
                            "detail": check.detail,
                            "cite": check.cite,
                        }
                        for check in checks
                    ],
                    "red": red,
                    "verdict": "PASS" if not red else "FAIL",
                },
                indent=2,
                sort_keys=True,
                ensure_ascii=False,
            ))
            return
        print(
            f"GATE {self.gate_id} cr={self.cr} build={build_state} "
            f"binary={os.path.relpath(binary, ROOT)} sha256={(file_sha256(binary) or '-')[:16]}"
        )
        print(f"GATE build-detail={build_detail}")
        for probe, runs in captured.items():
            print(
                f"PROBE {probe} run1 exit={runs[0].exit} bytes={len(runs[0].stdout)} "
                f"sha256={digests[probe][:32]} run2 exit={runs[1].exit} "
                f"bytes={len(runs[1].stdout)} byte-identical={runs[0].stdout == runs[1].stdout}"
            )
        if verbose:
            for probe, runs in captured.items():
                print(f"-- run1 stdout ({probe}) --")
                sys.stdout.write(runs[0].text)
                if not runs[0].text.endswith("\n"):
                    print()
        for check in checks:
            if check.missing:
                print(f"[MISSING] {check.name} - {check.detail}")
            elif check.ok:
                suffix = f" src={check.cite}" if check.cite else ""
                print(f"OK {check.name} {check.detail}{suffix}")
            else:
                suffix = f" src={check.cite}" if check.cite else ""
                print(f"FAIL {check.name} {check.detail}{suffix}")
        print(
            f"CHECKS total={len(checks)} green={sum(1 for c in checks if c.ok)} "
            f"red={len(plain_fail)} missing={len(missing)} "
            f"failed={','.join(plain_fail[:8]) or 'none'} missing-ids={','.join(missing[:8]) or 'none'}"
        )

    def write_artifacts(
        self,
        out: Path,
        checks: list[Check],
        captured: dict[str, list[Capture]],
        *,
        binary: Path,
        extra: dict | None = None,
    ) -> None:
        out.mkdir(parents=True, exist_ok=True)
        for probe, runs in captured.items():
            for item in runs:
                (out / f"{probe}.run{item.run}.stdout").write_bytes(item.stdout)
                (out / f"{probe}.run{item.run}.stderr").write_bytes(item.stderr)
        payload = {
            "gate": self.gate_id,
            "cr": self.cr,
            "commit": head_commit(),
            "binary_sha256": file_sha256(binary),
            "digests": self.digests(captured),
            "rows": [
                {"name": c.name, "ok": c.ok, "missing": c.missing, "detail": c.detail, "cite": c.cite}
                for c in checks
            ],
            "red": [c.name for c in checks if not c.ok],
        }
        if extra:
            payload.update(extra)
        (out / "manifest.json").write_text(
            json.dumps(payload, indent=2, sort_keys=True, ensure_ascii=False) + "\n",
            encoding="utf-8",
        )

    # ── falsifier (G9) ─────────────────────────────────────────────────────

    def falsify(self, spec: Falsifier, binary: Path, out: Path, no_build: bool) -> int:
        def say(text: str) -> None:
            print(f"FALSIFY {spec.name}: {text}")

        if not spec.path.startswith("crates/"):
            say(f"REJECT path={spec.path} is outside crates/** (G9)")
            return EXIT_REJECT
        path = ROOT / spec.path
        if not path.is_file():
            say(f"REJECT path={spec.path} absent")
            return EXIT_REJECT
        status = git_status([spec.path])
        if status.strip():
            say(f"REJECT path={spec.path} is dirty before the falsifier: {status.strip()[:120]!r}")
            return EXIT_REJECT
        original = path.read_bytes()
        original_sha = sha256_hex(original)
        occurrences = original.count(spec.before.encode())
        say(f"step=1 anchor path={spec.path} occurrences={occurrences} sha256_before={original_sha[:16]}")
        if occurrences != 1:
            say("REJECT anchor is not byte-exact-unique")
            return EXIT_REJECT

        baseline = self.capture(binary)
        baseline_map = {check.name: check for check in self.all_checks(baseline)[0]}
        target = baseline_map.get(spec.target)
        baseline_digest = self.digests(baseline)[self.probes[0]]
        if target is None:
            say(f"REJECT target-row={spec.target} is not a row of {self.gate_id}")
            return EXIT_REJECT
        if not target.ok:
            say(f"REJECT target-row={spec.target} is already red before the mutation ({target.detail[:80]})")
            return EXIT_REJECT
        say(f"step=2 baseline row={spec.target} GREEN digest={baseline_digest}")

        backup = out / f"falsify-{spec.name}" / path.name
        backup.parent.mkdir(parents=True, exist_ok=True)
        backup.write_bytes(original)
        say(f"step=3 backup={os.path.relpath(backup, ROOT)} sha256={sha256_hex(backup.read_bytes())[:16]}")

        mutated = original.replace(spec.before.encode(), spec.after.encode(), 1)
        path.write_bytes(mutated)
        say(
            f"step=4 patched sha256_after={(file_sha256(path) or '-')[:16]} "
            f"bytes={len(original)}->{len(mutated)} mutation={spec.description}"
        )

        state, detail = build(no_build)
        say(f"step=5 rebuild state={state} {detail[:120]}")
        if state == "failed":
            self._restore(path, original, original_sha, spec, say)
            say("RESULT FAIL - the mutated tree does not build")
            return EXIT_RED

        red_captures = self.capture(binary)
        red_checks = self.all_checks(red_captures)[0]
        red_target = {check.name: check for check in red_checks}.get(spec.target)
        red_digest = self.digests(red_captures)[self.probes[0]]
        red_rows = [check.name for check in red_checks if not check.ok]
        say(f"step=6 mutated row={spec.target} {'RED' if not red_target.ok else 'STAYED GREEN'} digest={red_digest}")
        say(f"step=6 mutated red-rows={','.join(red_rows[:10]) or 'none'}")
        if red_target.ok:
            self._restore(path, original, original_sha, spec, say)
            say("RESULT FAIL - the mutation did not turn the named row red")
            return EXIT_RED
        self.write_artifacts(
            out / f"falsify-{spec.name}",
            red_checks,
            red_captures,
            binary=binary,
            extra={"falsify": {
                "name": spec.name,
                "phase": "mutated",
                "target": spec.target,
                "mutation": spec.description,
                "path": spec.path,
                "digest": red_digest,
            }},
        )

        restored_ok = self._restore(path, original, original_sha, spec, say)
        if not restored_ok:
            say("RESULT FAIL - restore did not return the tree to its committed bytes")
            return EXIT_RED
        state, detail = build(no_build)
        say(f"step=8 rebuild state={state} {detail[:120]}")
        if state == "failed":
            say("RESULT FAIL - the restored tree does not build")
            return EXIT_RED
        back = self.capture(binary)
        back_target = {check.name: check for check in self.all_checks(back)[0]}.get(spec.target)
        back_digest = self.digests(back)[self.probes[0]]
        say(f"step=9 restored row={spec.target} {'GREEN' if back_target.ok else 'RED'} digest={back_digest}")
        say(f"step=9 digest-baseline={baseline_digest} digest-restored={back_digest} match={baseline_digest == back_digest}")
        say(f"step=9 git-status={git_status([spec.path]).strip() or 'clean'}")
        ok = back_target.ok and baseline_digest == back_digest and restored_ok
        self.write_artifacts(
            out / f"falsify-{spec.name}" / "restored",
            self.all_checks(back)[0],
            back,
            binary=binary,
            extra={"falsify": {
                "name": spec.name,
                "phase": "restored",
                "target": spec.target,
                "mutation": spec.description,
                "path": spec.path,
                "digest_baseline": baseline_digest,
                "digest_restored": back_digest,
                "digest_match": baseline_digest == back_digest,
            }},
        )
        say(f"RESULT {'PASS' if ok else 'FAIL'} - red-under-mutation -> green-after-restore, same digest")
        return EXIT_GREEN if ok else EXIT_RED

    def _restore(
        self,
        path: Path,
        original: bytes,
        original_sha: str,
        spec: Falsifier,
        say: Callable[[str], None],
    ) -> bool:
        for attempt in (1, 2, 3):
            proc = run(["git", "checkout", "--", spec.path], timeout=120)
            if proc.returncode == 0:
                say(f"step=7 git-checkout -- {spec.path} rc=0 attempt={attempt}")
                break
            tail = proc.stderr.decode("utf-8", "replace").strip().splitlines()[-1:] or ["-"]
            say(
                f"step=7 git-checkout -- {spec.path} rc={proc.returncode} attempt={attempt} "
                f"stderr={tail[0][:120]}"
            )
            time.sleep(0.5 * attempt)
        if file_sha256(path) != original_sha:
            # The tree must NEVER stay mutated (a concurrent `git` holding index.lock
            # makes checkout exit 128). Fall back to the exact bytes captured before the
            # patch; integrity is re-proven by sha256 + `git status` right below.
            path.write_bytes(original)
            say(f"step=7 byte-fallback path={spec.path} bytes={len(original)} sha256={file_sha256(path)[:16]}")
        current = file_sha256(path)
        status = git_status([spec.path])
        say(
            f"step=7 sha256_restored={(current or '-')[:16]} matches_before={current == original_sha} "
            f"git-status={status.strip() or 'clean'}"
        )
        return proc.returncode == 0 and current == original_sha and not status.strip()

    # ── entry point ────────────────────────────────────────────────────────

    def main(self, argv: Sequence[str] | None = None) -> int:
        parser = argparse.ArgumentParser(
            description=f"{self.gate_id} - M43 gate (CR {self.cr}); contract {self.spec_path}",
        )
        parser.add_argument("--binary", default=str(BINARY), help="prebuilt daggerfall-app")
        parser.add_argument(
            "--out",
            default=str(ARTIFACT_ROOT / self.slug),
            help="artifact root (under target/, never versioned)",
        )
        parser.add_argument("--no-build", action="store_true", help="G4: skip cargo build (needs a G3 MATCH)")
        parser.add_argument("--json", action="store_true", help="emit one JSON object")
        parser.add_argument("--verbose", action="store_true", help="echo run1 stdout first")
        parser.add_argument(
            "--falsify",
            choices=sorted(self.falsifiers) or None,
            help="G9: mutate the source byte-exactly and require the named row to go red",
        )
        args = parser.parse_args(argv)

        binary = Path(args.binary)
        out = Path(args.out)
        try:
            if args.falsify:
                if not binary.is_file():
                    print(f"FALSIFY {args.falsify}: REJECT binary absent {binary}")
                    return EXIT_ENV
                return self.falsify(self.falsifiers[args.falsify], binary, out, args.no_build)

            state, detail = build(args.no_build)
            if state == "failed":
                print(f"GATE {self.gate_id} build=FAILED")
                print(f"VERDICT: NC - build failed (environment): {detail}")
                return EXIT_ENV
            if not binary.is_file():
                print(f"GATE {self.gate_id} binary={binary} absent")
                print("VERDICT: NC - environment: the runtime target is not built")
                return EXIT_ENV

            captured = self.capture(binary)
            checks, unbound = self.all_checks(captured)
            if state == "skipped" and not all(
                check.ok for check in checks if check.name.startswith("SOURCES-FINGERPRINT-")
            ):
                self.emit(checks, captured, binary=binary, build_state=state,
                          build_detail=detail, as_json=args.json, verbose=args.verbose)
                print("VERDICT: REJECT - STALE-BINARY (G4: --no-build without a G3 MATCH)")
                return EXIT_REJECT
            if unbound:
                self.emit(checks, captured, binary=binary, build_state=state,
                          build_detail=detail, as_json=args.json, verbose=args.verbose)
                print("VERDICT: REJECT - STALE-BINARY (G3: the binary is not bound to this tree)")
                return EXIT_REJECT

            self.write_artifacts(out, checks, captured, binary=binary)
            self.emit(checks, captured, binary=binary, build_state=state,
                      build_detail=detail, as_json=args.json, verbose=args.verbose)
            red = [check for check in checks if not check.ok]
            if any(check.name.startswith("environment-") and "arena2=absent" in check.detail for check in checks):
                print("VERDICT: NC - environment: a consumed probe needs ARENA2 (EXIT=2); retail rows are NC")
                return EXIT_ENV
            if red:
                print(
                    f"VERDICT: FAIL - {len(red)} red of {len(checks)} rows "
                    "(pre-implementation rows are named [MISSING])"
                )
                return EXIT_RED
            print(f"VERDICT: PASS - all {len(checks)} rows green")
            return EXIT_GREEN
        except Exception as exc:  # G10: never a traceback
            print(f"FAIL internal {type(exc).__name__}: {exc}")
            print("VERDICT: FAIL - internal")
            return EXIT_RED


def cli(gate: Gate, argv: Sequence[str] | None = None) -> int:
    return gate.main(argv)
