#!/usr/bin/env python3
"""UI-JUGABLE-001 - gate of M39/UI-3 (51 `Playable` surfaces, blocks A..H, axes A1..A7).

Deterministic process contract
------------------------------
`PROBE-UI-JUGABLE-001` prints every line as a function of the retail bytes and
of the declarations (`crates/app/src/m39_surfaces.rs`), so a second process must
print the same bytes. The runner enforces exactly that, in two **independent
processes** (`PROBE_RUN=1|2`; the probe never prints that value):

1. `cargo build -p daggerfall-app` once (skip with `--skip-build`).
2. Both processes into `--out/run1` and `--out/run2` (default
   `.sintergic/artifacts/m39-ui3/ui-jugable-001`, gitignored: retail pixels are
   never committed).
3. Nine named rules, each with its own evidence line:

   * `probe-verdict` — both runs print `VERDICT: PASS` and exit 0.
   * `stdout-byte-identical` — stdout of run 1 equals stdout of run 2 byte by
     byte, equal `sha256`, and one equal `DIGEST` line per run.
   * `census-surface-set` — exactly the 51 `SURFACE` lines of the frozen plan
     §1/§2 census: unique ascending ordinals, 0 orphans, 0 duplicates,
     0 missing, and each line's `block=` equal to the frozen partition.
   * `block-counts` — eight `BLOCK <L> OK` lines, `windows=n/n` equal to the
     frozen partition (A16 B1 C2 D7 E7 F3 G6 H9), and coherent counters
     (`intents > 0`, `flows > 0`, `delivered <= intents`).
   * `axis-set` — `AXIS A1`..`AXIS A7` present in order and all `OK`; `A6`
     must report `intents == delivered` and `intents > 0`.
   * `focus-rows` — 51 `FOCUS` lines, one per surface in census order; `AXIS A5`
     declares `focus-rows > 0`, `surfaces-with-focus=48` and
     `not-applicable=031,071,074`; every interactive surface declares
     `rows >= 1` with `next == prev == rows-1`, `sel=0`, `enter=ok`, `space=ok`
     and one `targets=` label per row; every read-only surface declares
     `rows=0` plus a citable `not-applicable=`; and each `SURFACE ... focus=N`
     equals its `FOCUS` row count. Failures are named `focus-missing`,
     `focus-zero`, `focus-nav`, `focus-activate`, `focus-na` and
     `focus-surface-mismatch`.
   * `interact-delivery` — 51 `INTERACT` lines, one per surface in census
     order, `intents > 0`, `delivered >= 1`, `delivered <= intents`; the three
     read-only surfaces (031, 071, 074) with a non-empty `read-only=` and no
     delta; every other surface with `read-only=-`.
   * `bmp-digests` — 51 `BMP` lines per run and every printed digest equals the
     `sha256` of the file that same run left in `--out/runN/`.
   * `no-empty-digest` — `DIGEST` present and never the `sha256` of the empty
     string.

Any failing rule is `VERDICT: FAIL - <rule>` and exit 1. Without ARENA2:
`VERDICT: SKIP - no ARENA2` and exit 2 - never a PASS.

Falsifiers (`--falsify <mode>`)
-------------------------------
Each mode mutates the **capture** (or the artifact the check reads) after the
clean runs, never the verdict, and must turn the gate red under its own name:

* `wrong-bmp-digest` — flips one byte of a real `.bmp` on disk (both runs of
  stdout untouched) => `bmp-digests` names `runN:<file>:<disk>=<printed>`.
* `block-count-mismatch` — rewrites one `BLOCK` line (`windows=16/16` ->
  `windows=15/16`) in **both** captures => `block-counts`.
* `double-delivery` — rewrites one `INTERACT` line to `delivered = intents + 1`
  in **both** captures => `interact-delivery`.
* `focus-zero` — rewrites one interactive `FOCUS` line to
  `rows=0 next=0 prev=0 sel=- enter=- space=-` in **both** captures =>
  `focus-rows`.

The corrupt artifact is restored byte for byte (and re-hashed) before exit; the
mutated captures only ever live in memory. Exit 1 with `FALSIFY <mode>: gate
red - <rule>`; a falsifier that leaves the gate green is itself a FAIL.

E2E (`--e2e-revert`)
--------------------
Proves the independent census bites even when the probe derives its own
denominators: one row is deleted from `crates/app/src/m39_surfaces.rs` (plus the
array arity), the crate is rebuilt, and the gate must go red with
`census-surface-set` among the failed rules. A `finally` restores the file
(`git checkout --`), rebuilds and requires a green gate plus an untouched tree:
zero `git status --porcelain` entries for `crates/` and for the patched file.
The whole-tree porcelain is printed as well; entries belonging to other lanes
sharing this worktree are reported as foreign dirt, never silently hidden.

Exit codes: 0 PASS, 1 FAIL, 2 SKIP/usage (including a failed build).
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
GATE = "UI-JUGABLE-001"
PROBE = "PROBE-UI-JUGABLE-001"
SLUG = "ui-jugable-001"
BINARY = ROOT / "target" / "debug" / "daggerfall-app"
ARTIFACTS = ROOT / ".sintergic" / "artifacts" / "m39-ui3" / SLUG
CR = "CR-aecd1cef-275c-4dd1-a361-f2d0e9d28eb2"
CRATE = "daggerfall-app"
SURFACES_FILE = "crates/app/src/m39_surfaces.rs"

# Plan §1 (51 `Playable` ordinals, ascending) x plan §2 (block partition).
CENSUS: tuple[tuple[str, str], ...] = (
    ("022", "A"),
    ("023", "G"),
    ("024", "D"),
    ("025", "D"),
    ("027", "F"),
    ("028", "C"),
    ("029", "A"),
    ("030", "H"),
    ("031", "H"),
    ("032", "E"),
    ("033", "G"),
    ("034", "H"),
    ("035", "H"),
    ("036", "H"),
    ("037", "H"),
    ("038", "B"),
    ("039", "A"),
    ("040", "C"),
    ("041", "E"),
    ("042", "A"),
    ("043", "A"),
    ("044", "A"),
    ("045", "D"),
    ("046", "D"),
    ("047", "A"),
    ("048", "A"),
    ("049", "F"),
    ("051", "E"),
    ("052", "F"),
    ("053", "H"),
    ("055", "D"),
    ("056", "E"),
    ("057", "E"),
    ("058", "A"),
    ("059", "A"),
    ("060", "H"),
    ("061", "D"),
    ("062", "G"),
    ("063", "D"),
    ("064", "G"),
    ("065", "G"),
    ("066", "G"),
    ("067", "A"),
    ("068", "A"),
    ("069", "A"),
    ("070", "E"),
    ("071", "A"),
    ("072", "H"),
    ("073", "A"),
    ("074", "A"),
    ("082", "E"),
)
CENSUS_IDS: tuple[str, ...] = tuple(ordinal for ordinal, _ in CENSUS)
CENSUS_BLOCK: dict[str, str] = dict(CENSUS)
BLOCKS: tuple[str, ...] = ("A", "B", "C", "D", "E", "F", "G", "H")
BLOCK_SIZES: dict[str, int] = {"A": 16, "B": 1, "C": 2, "D": 7, "E": 7, "F": 3, "G": 6, "H": 9}
AXES: tuple[str, ...] = ("A1", "A2", "A3", "A4", "A5", "A6", "A7")
# Plan §3.4: the only surfaces without raster in the tree.
READ_ONLY: tuple[str, ...] = ("031", "071", "074")
# The one legitimate 64-hex literal of this file: `sha256(b"")`, rule 8.
EMPTY_SHA256 = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"

SURFACE_LINE = re.compile(
    r"^SURFACE (\d{3}) id=(\S+) block=(\S+) host=(\S+) assets=(\S+) nodes=(\d+) "
    r"glyphs=(\d+) focus=(\d+) opaque=(\d+) digest=([0-9a-f]{16})$"
)
INTERACT_LINE = re.compile(
    r"^INTERACT (\d{3}) id=(\S+) block=(\S+) intents=(\d+) delivered=(\d+) "
    r"outcomes=(\d+) delta=(\S+) read-only=(.*) digest=([0-9a-f]{16})$"
)
BLOCK_LINE = re.compile(
    r"^BLOCK (\S) OK windows=(\d+)/(\d+) flows=(\d+) intents=(\d+) "
    r"delivered=(\d+) state_delta=(\d+) read-only=(\d+)$"
)
BLOCK_FAIL_LINE = re.compile(r"^BLOCK (\S) FAIL reason=(.*)$")
FOCUS_LINE = re.compile(
    r"^FOCUS (\d{3}) id=(\S+) block=(\S) rows=(\d+) next=(\d+) prev=(\d+) "
    r"sel=(-|\d+) enter=(ok|no|-) space=(ok|no|-) targets=(\S+)"
    r"(?: not-applicable=(\S.*))?$"
)
# The walked half of a `FOCUS` line: what the `focus-zero` falsifier erases.
FOCUS_WALK = re.compile(
    r"rows=\d+ next=\d+ prev=\d+ sel=(?:-|\d+) enter=(?:ok|no|-) space=(?:ok|no|-)"
)
AXIS_LINE = re.compile(r"^AXIS (A\d) (OK|FAIL) (.*)$")
AXIS_A6 = re.compile(r"(?:^|\s)intents=(\d+) delivered=(\d+)")
AXIS_A5_ROWS = re.compile(r"(?:^|\s)focus-rows=(\d+)")
AXIS_A5_WITH = re.compile(r"(?:^|\s)surfaces-with-focus=(\d+)")
AXIS_A5_NA = re.compile(r"(?:^|\s)not-applicable=(\S+)")
BMP_LINE = re.compile(r"^BMP (\d{3})\.bmp sha256=([0-9a-f]{64})$")
DIGEST_LINE = re.compile(r"^DIGEST ([0-9a-f]{64})$")


# ── Process helpers ────────────────────────────────────────────────────────


def arena2_dir() -> Path | None:
    """Same resolution as the probe's presence test (plan §4.1):

    `$DAGGERFALL_ARENA2` (the M39 override), then `$DF_ARENA2`, then
    `$DF_APP_ROOT/ARENA2`, then `$HOME/Games/Daggerfall/app/ARENA2`. An explicit
    value is taken as given - a bad one is the probe's IO failure, not a
    runner-side SKIP.
    """
    for name in ("DAGGERFALL_ARENA2", "DF_ARENA2"):
        explicit = os.environ.get(name)
        if explicit and explicit.strip():
            return Path(explicit)
    root = os.environ.get("DF_APP_ROOT")
    base = Path(root) if root else Path(os.environ.get("HOME", "")) / "Games" / "Daggerfall" / "app"
    for name in ("ARENA2", "arena2"):
        candidate = base / name
        if candidate.is_dir():
            return candidate
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


def shown(path: Path) -> str:
    """Repo-relative when inside the tree, absolute otherwise (`--out /tmp/...`)."""
    try:
        return str(path.resolve().relative_to(ROOT))
    except ValueError:
        return str(path.resolve())


def porcelain(*pathspec: str) -> str:
    """`git status --porcelain`, optionally narrowed to a pathspec."""
    cmd = ["git", "status", "--porcelain"]
    if pathspec:
        cmd += ["--", *pathspec]
    return run(cmd).stdout.decode("utf-8", "replace")


def build(skip: bool) -> bool:
    """`cargo build -p daggerfall-app`; the stderr tail is printed on failure."""
    if skip:
        return True
    proc = run(["cargo", "build", "-p", CRATE])
    if proc.returncode == 0:
        return True
    sys.stderr.write(proc.stderr.decode("utf-8", "replace")[-4000:])
    return False


# ── (a) Capture: the two processes and the files they leave ────────────────


@dataclass(frozen=True)
class Capture:
    """One probe process: what it printed, what it exited with, what it wrote."""

    run: int
    exit: int
    stdout: bytes
    out_dir: Path
    disk: dict[str, str | None]
    stderr: bytes = b""


@dataclass(frozen=True)
class Rule:
    name: str
    ok: bool
    detail: str


@dataclass(frozen=True)
class FocusRow:
    """One parsed `FOCUS` line: the declared rows and the walked outcome."""

    ordinal: str
    block: str
    rows: int
    next: int
    prev: int
    sel: int | None
    enter: str
    space: str
    targets: str
    not_applicable: str | None


def lines_of(capture: Capture) -> list[str]:
    return capture.stdout.decode("utf-8", "replace").splitlines()


def bmp_pairs(capture: Capture) -> list[tuple[str, str]]:
    """The `BMP <id>.bmp sha256=<hex>` pairs of one process, in order."""
    pairs: list[tuple[str, str]] = []
    for line in lines_of(capture):
        if not line.startswith("BMP "):
            continue
        match = BMP_LINE.match(line)
        if match is not None:
            pairs.append((f"{match.group(1)}.bmp", match.group(2)))
    return pairs


def hash_bank(out_dir: Path, pairs: list[tuple[str, str]]) -> dict[str, str | None]:
    """`sha256` of every frame the process printed, read back from disk."""
    return {name: file_digest(out_dir / name) for name, _ in pairs}


def refresh_disk(capture: Capture) -> Capture:
    """Re-hash the frame bank of one capture (used after an artifact mutation)."""
    return replace(capture, disk=hash_bank(capture.out_dir, bmp_pairs(capture)))


def capture_run(binary: Path, out_dir: Path, run_id: int) -> Capture:
    proc = run(
        [str(binary), "--probe", PROBE, "--out", str(out_dir)],
        env={"PROBE_RUN": str(run_id)},
    )
    (out_dir.parent / f"run{run_id}.stdout").write_bytes(proc.stdout)
    (out_dir.parent / f"run{run_id}.stderr").write_bytes(proc.stderr)
    capture = Capture(
        run=run_id,
        exit=proc.returncode,
        stdout=proc.stdout,
        out_dir=out_dir,
        disk={},
        stderr=proc.stderr,
    )
    return refresh_disk(capture)


def capture_all(binary: Path, out: Path) -> list[Capture]:
    """Both independent processes into `out/run1` and `out/run2`."""
    if out.exists():
        shutil.rmtree(out)
    captures: list[Capture] = []
    for run_id in (1, 2):
        out_dir = out / f"run{run_id}"
        out_dir.mkdir(parents=True)
        captures.append(capture_run(binary, out_dir, run_id))
    return captures


def gate(binary: Path, out: Path) -> tuple[list[Capture], list[Rule]]:
    """(a) capture then (b) check - the whole gate for one archive root."""
    captures = capture_all(binary, out)
    return captures, check(captures)


# ── (b) Pure checks: no I/O, only the captures ─────────────────────────────


def verdict_of(capture: Capture) -> str | None:
    """The probe's closing `VERDICT ...` line, or None."""
    found = [line for line in lines_of(capture) if line.startswith("VERDICT")]
    return found[-1] if found else None


def digest_of(capture: Capture) -> str | None:
    """The probe's own `DIGEST` line (exactly one per process), or None."""
    found = [match.group(1) for line in lines_of(capture) if (match := DIGEST_LINE.match(line))]
    return found[0] if len(found) == 1 else None


def rule_probe_verdict(captures: list[Capture]) -> Rule:
    problems: list[str] = []
    evidence: list[str] = []
    for capture in captures:
        verdict = verdict_of(capture)
        evidence.append(f"run{capture.run}={verdict or 'absent'} exit={capture.exit}")
        if verdict != "VERDICT: PASS":
            problems.append(f"run{capture.run}={verdict or 'absent'}")
        if capture.exit != 0:
            problems.append(f"run{capture.run}:exit={capture.exit}")
    if problems:
        return Rule("probe-verdict", False, " ".join(problems))
    return Rule("probe-verdict", True, " ".join(evidence))


def rule_stdout_byte_identical(captures: list[Capture]) -> Rule:
    first, second = captures
    sha1, sha2 = digest(first.stdout), digest(second.stdout)
    digest1, digest2 = digest_of(first), digest_of(second)
    problems: list[str] = []
    if first.stdout != second.stdout:
        problems.append(f"stdout={len(first.stdout)}B!={len(second.stdout)}B")
    if sha1 != sha2:
        problems.append(f"sha256={sha1}!={sha2}")
    if digest1 is None or digest2 is None or digest1 != digest2:
        problems.append(f"digest={digest1 or 'absent'}!={digest2 or 'absent'}")
    if problems:
        return Rule("stdout-byte-identical", False, " ".join(problems))
    return Rule(
        "stdout-byte-identical",
        True,
        f"stdout={len(first.stdout)}B sha256={sha1} digest={digest1}",
    )


def rule_census_surface_set(captures: list[Capture]) -> Rule:
    rows: list[tuple[str, str]] = []
    problems: list[str] = []
    for line in lines_of(captures[0]):
        if not line.startswith("SURFACE "):
            continue
        match = SURFACE_LINE.match(line)
        if match is None:
            problems.append(f"malformed:{line[:64]}")
            continue
        rows.append((match.group(1), match.group(3)))
    ids = [ordinal for ordinal, _ in rows]
    missing = [ordinal for ordinal in CENSUS_IDS if ordinal not in ids]
    orphans = [ordinal for ordinal in ids if ordinal not in CENSUS_BLOCK]
    duplicated = sorted({ordinal for ordinal in ids if ids.count(ordinal) > 1})
    drifted = [
        f"{ordinal}:block={block}!={CENSUS_BLOCK[ordinal]}"
        for ordinal, block in rows
        if ordinal in CENSUS_BLOCK and block != CENSUS_BLOCK[ordinal]
    ]
    if len(ids) != len(CENSUS_IDS):
        problems.append(f"{len(ids)}!={len(CENSUS_IDS)}")
    if ids != list(CENSUS_IDS):
        broke = next(
            (index for index, pair in enumerate(zip(ids, CENSUS_IDS)) if pair[0] != pair[1]),
            min(len(ids), len(CENSUS_IDS)),
        )
        problems.append(f"order-broke-at={ids[broke] if broke < len(ids) else 'end'}")
    if missing:
        problems.append("missing=" + ",".join(missing))
    if orphans:
        problems.append("orphans=" + ",".join(orphans))
    if duplicated:
        problems.append("duplicated=" + ",".join(duplicated))
    problems.extend(drifted)
    if problems:
        return Rule("census-surface-set", False, " ".join(problems))
    partition = ",".join(f"{block}{BLOCK_SIZES[block]}" for block in BLOCKS)
    return Rule(
        "census-surface-set",
        True,
        f"surfaces={len(ids)} partition={partition} missing=0 orphans=0 duplicated=0",
    )


def rule_block_counts(captures: list[Capture]) -> Rule:
    problems: list[str] = []
    evidence: list[str] = []
    seen: list[str] = []
    for line in lines_of(captures[0]):
        if not line.startswith("BLOCK "):
            continue
        match = BLOCK_LINE.match(line)
        if match is None:
            failed = BLOCK_FAIL_LINE.match(line)
            if failed is not None:
                problems.append(f"{failed.group(1)}:FAIL reason={failed.group(2)}")
                seen.append(failed.group(1))
            else:
                problems.append(f"malformed:{line[:64]}")
            continue
        block = match.group(1)
        windows, want = int(match.group(2)), int(match.group(3))
        flows, intents = int(match.group(4)), int(match.group(5))
        delivered = int(match.group(6))
        seen.append(block)
        evidence.append(f"{block}={windows}/{want}")
        frozen = BLOCK_SIZES.get(block)
        if frozen is None:
            problems.append(f"{block}:not-a-block")
            continue
        if windows != frozen or want != frozen:
            problems.append(f"{block}:windows={windows}/{want}!={frozen}/{frozen}")
        elif windows != want:
            problems.append(f"{block}:windows={windows}!={want}")
        if flows <= 0:
            problems.append(f"{block}:flows={flows}")
        if intents <= 0:
            problems.append(f"{block}:intents={intents}")
        if delivered > intents:
            problems.append(f"{block}:delivered={delivered}>intents={intents}")
        if delivered < 1:
            problems.append(f"{block}:delivered={delivered}")
    if seen != list(BLOCKS):
        problems.append("blocks=" + ",".join(seen))
    if problems:
        return Rule("block-counts", False, " ".join(problems))
    return Rule("block-counts", True, " ".join(evidence))


def rule_axis_set(captures: list[Capture]) -> Rule:
    problems: list[str] = []
    axes: list[tuple[str, str, str]] = []
    for line in lines_of(captures[0]):
        if not line.startswith("AXIS "):
            continue
        match = AXIS_LINE.match(line)
        if match is None:
            problems.append(f"malformed:{line[:64]}")
            continue
        axes.append((match.group(1), match.group(2), match.group(3)))
        if match.group(2) != "OK":
            problems.append(f"{match.group(1)}=FAIL")
    found = [axis for axis, _, _ in axes]
    if found != list(AXES):
        missing = [axis for axis in AXES if axis not in found]
        problems.append("axes=" + ",".join(found) + (f" missing={','.join(missing)}" if missing else ""))
    a6 = [detail for axis, _, detail in axes if axis == "A6"]
    if len(a6) != 1:
        problems.append(f"A6-lines={len(a6)}")
        a6_pair = None
    else:
        match = AXIS_A6.search(a6[0])
        a6_pair = (int(match.group(1)), int(match.group(2))) if match else None
        if a6_pair is None:
            problems.append("A6-no-intents")
        elif a6_pair[0] != a6_pair[1]:
            problems.append(f"A6:intents={a6_pair[0]}!=delivered={a6_pair[1]}")
        elif a6_pair[0] <= 0:
            problems.append(f"A6:intents={a6_pair[0]}")
    if problems:
        return Rule("axis-set", False, " ".join(problems))
    return Rule("axis-set", True, f"axes=7 A6=intents={a6_pair[0]} delivered={a6_pair[1]}")


def rule_focus_rows(captures: list[Capture]) -> Rule:
    """Focus is measured, walked and activatable - never a silent zero.

    Names the failing dimension: `focus-missing` (a surface without `FOCUS`, out
    of census order, or a row whose accounting does not add up), `focus-zero`
    (`AXIS A5` silent about the rows, a zero total, or an interactive surface
    declaring none), `focus-nav` (`next`/`prev`/`sel` disagree with the declared
    rows), `focus-activate` (`Enter`/`Space` not delivered to the surface's own
    window), `focus-na` (a read-only surface without a citable reason, or an
    interactive one carrying one) and `focus-surface-mismatch` (`SURFACE ...
    focus=` or `block=` disagree with their `FOCUS` line).
    """
    problems: list[str] = []
    seen: list[str] = []
    rows_of: dict[str, FocusRow] = {}
    for line in lines_of(captures[0]):
        if not line.startswith("FOCUS "):
            continue
        match = FOCUS_LINE.match(line)
        if match is None:
            problems.append(f"focus-missing:malformed:{line[:64]}")
            continue
        ordinal = match.group(1)
        if ordinal in rows_of:
            problems.append(f"focus-missing:duplicate:{ordinal}")
            continue
        seen.append(ordinal)
        rows_of[ordinal] = FocusRow(
            ordinal=ordinal,
            block=match.group(3),
            rows=int(match.group(4)),
            next=int(match.group(5)),
            prev=int(match.group(6)),
            sel=None if match.group(7) == "-" else int(match.group(7)),
            enter=match.group(8),
            space=match.group(9),
            targets=match.group(10),
            not_applicable=match.group(11).strip() if match.group(11) is not None else None,
        )
    missing = [ordinal for ordinal in CENSUS_IDS if ordinal not in rows_of]
    foreign = [ordinal for ordinal in rows_of if ordinal not in CENSUS_IDS]
    if missing:
        problems.append(f"focus-missing:absent={','.join(missing)}")
    if foreign:
        problems.append(f"focus-missing:foreign={','.join(foreign)}")
    if not missing and not foreign and seen != list(CENSUS_IDS):
        problems.append("focus-missing:order")

    declared: dict[str, int] = {}
    for line in lines_of(captures[0]):
        if not line.startswith("SURFACE "):
            continue
        match = SURFACE_LINE.match(line)
        if match is not None:
            declared[match.group(1)] = int(match.group(8))

    a5_detail: str | None = None
    for line in lines_of(captures[0]):
        match = AXIS_LINE.match(line)
        if match is not None and match.group(1) == "A5":
            a5_detail = match.group(3)
    axis_total: int | None = None
    axis_with: int | None = None
    axis_na: list[str] = []
    if a5_detail is None:
        problems.append("focus-zero:axis-A5=absent")
    else:
        total_match = AXIS_A5_ROWS.search(a5_detail)
        with_match = AXIS_A5_WITH.search(a5_detail)
        na_match = AXIS_A5_NA.search(a5_detail)
        if total_match is None:
            problems.append("focus-zero:axis-A5=no-focus-rows")
        else:
            axis_total = int(total_match.group(1))
            if axis_total == 0:
                problems.append("focus-zero:axis-A5:focus-rows=0")
        if with_match is None:
            problems.append("focus-na:axis-A5=no-surfaces-with-focus")
        else:
            axis_with = int(with_match.group(1))
        axis_na = (
            []
            if na_match is None
            else [item for item in na_match.group(1).split(",") if item != "-"]
        )
        if sorted(axis_na) != sorted(READ_ONLY):
            problems.append("focus-na:axis-A5:not-applicable=" + (",".join(axis_na) or "-"))

    total = 0
    with_focus = 0
    for ordinal in CENSUS_IDS:
        row = rows_of.get(ordinal)
        if row is None:
            continue
        total += row.rows
        if row.block != CENSUS_BLOCK[ordinal]:
            problems.append(f"focus-surface-mismatch:{ordinal}:block={row.block}")
        if ordinal in declared and declared[ordinal] != row.rows:
            problems.append(
                f"focus-surface-mismatch:{ordinal}:surface={declared[ordinal]}!=rows={row.rows}"
            )
        if ordinal in READ_ONLY:
            if row.rows != 0:
                problems.append(f"focus-na:{ordinal}:rows={row.rows}")
            if not row.not_applicable:
                problems.append(f"focus-na:{ordinal}:no-reason")
            if row.sel is not None or row.enter != "-" or row.space != "-":
                problems.append(f"focus-na:{ordinal}:walked")
            continue
        if row.rows == 0:
            problems.append(f"focus-zero:{ordinal}:rows=0")
            continue
        with_focus += 1
        if row.not_applicable is not None:
            problems.append(f"focus-na:{ordinal}:not-applicable={row.not_applicable}")
        labels = [item for item in row.targets.split(",") if item]
        if len(labels) != row.rows:
            problems.append(f"focus-missing:{ordinal}:targets={len(labels)}!=rows={row.rows}")
        if row.next != row.rows - 1 or row.prev != row.rows - 1:
            problems.append(
                f"focus-nav:{ordinal}:next={row.next} prev={row.prev} rows={row.rows}"
            )
        if row.sel != 0:
            problems.append(f"focus-nav:{ordinal}:sel={row.sel}")
        if row.enter != "ok" or row.space != "ok":
            problems.append(f"focus-activate:{ordinal}:enter={row.enter} space={row.space}")
    if declared and set(declared) != set(CENSUS_IDS):
        problems.append("focus-surface-mismatch:surface-set")
    if axis_total is not None and axis_total != total:
        problems.append(f"focus-missing:axis-focus-rows={axis_total}!=sum={total}")
    if axis_with is not None and axis_with != with_focus:
        problems.append(f"focus-na:axis-surfaces-with-focus={axis_with}!=measured={with_focus}")
    if problems:
        return Rule("focus-rows", False, " ".join(problems))
    return Rule(
        "focus-rows",
        True,
        f"surfaces={len(rows_of)} focus-rows={total} surfaces-with-focus={with_focus} "
        f"not-applicable={','.join(READ_ONLY)}",
    )


def rule_interact_delivery(captures: list[Capture]) -> Rule:
    problems: list[str] = []
    rows: list[tuple[str, int, int, str, str]] = []
    for line in lines_of(captures[0]):
        if not line.startswith("INTERACT "):
            continue
        match = INTERACT_LINE.match(line)
        if match is None:
            problems.append(f"malformed:{line[:64]}")
            continue
        ordinal = match.group(1)
        intents, delivered = int(match.group(4)), int(match.group(5))
        delta, read_only = match.group(7), match.group(8).strip()
        rows.append((ordinal, intents, delivered, delta, read_only))
        if ordinal not in CENSUS_BLOCK:
            problems.append(f"{ordinal}:not-in-census")
        if intents <= 0:
            problems.append(f"{ordinal}:intents={intents}")
        if delivered < 1:
            problems.append(f"{ordinal}:delivered={delivered}")
        if delivered > intents:
            problems.append(f"{ordinal}:delivered={delivered}>intents={intents}")
        if ordinal in READ_ONLY:
            if read_only in ("", "-"):
                problems.append(f"{ordinal}:read-only={read_only or 'empty'}")
            if delta != "-":
                problems.append(f"{ordinal}:read-only-with-delta={delta}")
        elif read_only != "-":
            problems.append(f"{ordinal}:read-only={read_only[:32]}")
    ids = [ordinal for ordinal, _, _, _, _ in rows]
    if ids != list(CENSUS_IDS):
        broke = next(
            (index for index, pair in enumerate(zip(ids, CENSUS_IDS)) if pair[0] != pair[1]),
            min(len(ids), len(CENSUS_IDS)),
        )
        missing = [ordinal for ordinal in CENSUS_IDS if ordinal not in ids]
        problems.append(
            f"surfaces={len(ids)}"
            + (f" missing={','.join(missing)}" if missing else "")
            + f" order-broke-at={ids[broke] if broke < len(ids) else 'end'}"
        )
    if problems:
        return Rule("interact-delivery", False, " ".join(problems))
    intents = sum(row[1] for row in rows)
    delivered = sum(row[2] for row in rows)
    return Rule(
        "interact-delivery",
        True,
        f"surfaces={len(rows)} intents={intents} delivered={delivered} "
        f"read-only={','.join(READ_ONLY)}",
    )


def rule_bmp_digests(captures: list[Capture]) -> Rule:
    problems: list[str] = []
    banks: list[list[tuple[str, str]]] = []
    expected = [f"{ordinal}.bmp" for ordinal in CENSUS_IDS]
    for capture in captures:
        pairs = bmp_pairs(capture)
        banks.append(pairs)
        names: list[str] = []
        for name, printed in pairs:
            names.append(name)
            actual = capture.disk.get(name)
            if actual is None:
                problems.append(f"run{capture.run}:{name}:missing")
            elif actual != printed:
                problems.append(f"run{capture.run}:{name}:{actual}!={printed}")
        if names != expected:
            problems.append(f"run{capture.run}:names={len(names)}!={len(expected)}")
    if banks[0] != banks[1]:
        problems.append("run1!=run2")
    if problems:
        return Rule("bmp-digests", False, " ".join(problems))
    return Rule("bmp-digests", True, f"files={len(expected)}x2 rehashed-from-disk")


def rule_no_empty_digest(captures: list[Capture]) -> Rule:
    problems: list[str] = []
    for capture in captures:
        value = digest_of(capture)
        if value is None:
            problems.append(f"run{capture.run}=absent")
        elif value == EMPTY_SHA256:
            problems.append(f"run{capture.run}={value}")
    if problems:
        return Rule("no-empty-digest", False, " ".join(problems))
    return Rule("no-empty-digest", True, f"digest={digest_of(captures[0])}")


RULES = (
    rule_probe_verdict,
    rule_stdout_byte_identical,
    rule_census_surface_set,
    rule_block_counts,
    rule_axis_set,
    rule_focus_rows,
    rule_interact_delivery,
    rule_bmp_digests,
    rule_no_empty_digest,
)


def check(captures: list[Capture]) -> list[Rule]:
    """The nine rules over two captures. Pure: no disk, no process, no clock."""
    return [rule(captures) for rule in RULES]


# ── Reporting ──────────────────────────────────────────────────────────────


def failed_names(rules: list[Rule]) -> list[str]:
    return [rule.name for rule in rules if not rule.ok]


def payload(
    binary: Path,
    captures: list[Capture],
    rules: list[Rule],
    extra: dict[str, object] | None = None,
) -> dict[str, object]:
    failed = failed_names(rules)
    body: dict[str, object] = {
        "gate": GATE,
        "probe": PROBE,
        "cr": CR,
        "commit": head_commit(),
        "binary": {"path": shown(binary), "sha256": file_digest(binary)},
        "surfaces": len(CENSUS_IDS),
        "partition": {block: BLOCK_SIZES[block] for block in BLOCKS},
        "runs": [
            {
                "run": capture.run,
                "exit": capture.exit,
                "bytes": len(capture.stdout),
                "stdout_sha256": digest(capture.stdout),
                "probe_digest": digest_of(capture),
                "probe_verdict": verdict_of(capture),
                "out": shown(capture.out_dir),
                "bmp": [f"{name} {printed}" for name, printed in bmp_pairs(capture)],
            }
            for capture in captures
        ],
        "rules": [{"name": rule.name, "ok": rule.ok, "detail": rule.detail} for rule in rules],
        "failed": failed,
        "verdict": "FAIL" if failed else "PASS",
        "exit": 1 if failed else 0,
    }
    if extra:
        body.update(extra)
    return body


def emit(
    binary: Path,
    captures: list[Capture],
    rules: list[Rule],
    as_json: bool,
    extra: dict[str, object] | None = None,
) -> int:
    """Print the rule table (or the JSON object) and return the exit code."""
    failed = failed_names(rules)
    if as_json:
        print(json.dumps(payload(binary, captures, rules, extra), indent=2))
        return 1 if failed else 0
    first = captures[0]
    print(
        f"gate {GATE} probe={PROBE} cr={CR} surfaces={len(CENSUS_IDS)} "
        f"partition={'/'.join(str(BLOCK_SIZES[block]) for block in BLOCKS)}"
    )
    print(
        f"run1={shown(first.out_dir)} sha256={digest(first.stdout)} "
        f"run2={shown(captures[1].out_dir)} sha256={digest(captures[1].stdout)}"
    )
    for rule in rules:
        print(f"{'OK' if rule.ok else 'FAIL'} {rule.name} {rule.detail}".rstrip())
    if failed:
        print(f"FAILED {','.join(failed)}")
        print(f"VERDICT: FAIL - {failed[0]}")
        return 1
    print(f"CHECKS {','.join(rule.name for rule in rules)} all-OK")
    print("VERDICT: PASS")
    return 0


def write_manifest(
    out: Path,
    binary: Path,
    captures: list[Capture],
    rules: list[Rule],
    extra: dict[str, object] | None = None,
) -> Path:
    path = out / "manifest.json"
    path.write_text(json.dumps(payload(binary, captures, rules, extra), indent=2) + "\n")
    return path


def echo_probe(capture: Capture) -> None:
    text = capture.stdout.decode("utf-8", "replace")
    sys.stdout.write(text)
    if text and not text.endswith("\n"):
        print()


# ── Falsifiers: mutate the capture (or the artifact), never the verdict ────


def mutate_bmp_digest(captures: list[Capture], undo: list[tuple[Path, bytes, str]]) -> tuple[list[Capture], str]:
    """Flip one byte of a real frame in run 1; the undo list restores it."""
    capture = captures[0]
    for name, printed in bmp_pairs(capture):
        path = capture.out_dir / name
        if not path.is_file():
            continue
        original = path.read_bytes()
        corrupted = bytearray(original)
        corrupted[len(corrupted) // 2] ^= 0xFF
        path.write_bytes(bytes(corrupted))
        undo.append((path, original, printed))
        return [refresh_disk(capture), captures[1]], f"run{capture.run}:{name} one flipped byte"
    raise RuntimeError("no frame of run1 exists on disk to corrupt")


def mutate_block_line(captures: list[Capture], undo: list[tuple[Path, bytes, str]]) -> tuple[list[Capture], str]:
    """Rewrite one `BLOCK` line's `windows=n/n` numerator in both captures."""
    mutated: list[Capture] = []
    note = ""
    for capture in captures:
        lines = lines_of(capture)
        replaced = False
        for index, line in enumerate(lines):
            match = BLOCK_LINE.match(line)
            if match is None or replaced:
                continue
            block = match.group(1)
            windows, want = int(match.group(2)), int(match.group(3))
            if windows < 2:
                continue
            lines[index] = line.replace(
                f"windows={match.group(2)}/{match.group(3)}",
                f"windows={windows - 1}/{want}",
                1,
            )
            note = f"block {block}: windows={windows}/{want} -> windows={windows - 1}/{want}"
            replaced = True
        if not replaced:
            raise RuntimeError("no BLOCK line with windows>=2 in the capture")
        mutated.append(replace(capture, stdout=("\n".join(lines) + "\n").encode()))
    return mutated, note


def mutate_interact_line(
    captures: list[Capture], undo: list[tuple[Path, bytes, str]]
) -> tuple[list[Capture], str]:
    """Rewrite one `INTERACT` line to `delivered = intents + 1` in both captures."""
    mutated: list[Capture] = []
    note = ""
    planned: tuple[str, int, int, int] | None = None
    for capture in captures:
        lines = lines_of(capture)
        for index, line in enumerate(lines):
            match = INTERACT_LINE.match(line)
            if match is None:
                continue
            intents, delivered = int(match.group(4)), int(match.group(5))
            if planned is None:
                planned = (match.group(1), intents, delivered, intents + 1)
            ordinal, intents, delivered, bumped = planned
            lines[index] = line.replace(f"delivered={delivered}", f"delivered={bumped}", 1)
            break
        if planned is None:
            raise RuntimeError("no INTERACT line in the capture")
        note = (
            f"surface {planned[0]}: intents={planned[1]} delivered={planned[2]}"
            f" -> delivered={planned[3]}"
        )
        mutated.append(replace(capture, stdout=("\n".join(lines) + "\n").encode()))
    return mutated, note


def mutate_focus_zero(
    captures: list[Capture], undo: list[tuple[Path, bytes, str]]
) -> tuple[list[Capture], str]:
    """Erase the walk of one interactive `FOCUS` line in both captures."""
    planned: tuple[str, int] | None = None
    for line in lines_of(captures[0]):
        match = FOCUS_LINE.match(line)
        if match is None or match.group(11) is not None or match.group(1) in READ_ONLY:
            continue
        planned = (match.group(1), int(match.group(4)))
        break
    if planned is None:
        raise RuntimeError("no interactive FOCUS line in the capture")
    ordinal, rows = planned
    mutated: list[Capture] = []
    for capture in captures:
        lines = lines_of(capture)
        replaced = False
        for index, line in enumerate(lines):
            if not line.startswith(f"FOCUS {ordinal} "):
                continue
            new_line = FOCUS_WALK.sub(
                "rows=0 next=0 prev=0 sel=- enter=- space=-", line, count=1
            )
            if new_line == line:
                continue
            lines[index] = new_line
            replaced = True
            break
        if not replaced:
            raise RuntimeError(f"FOCUS {ordinal} not rewritable in run{capture.run}")
        mutated.append(replace(capture, stdout=("\n".join(lines) + "\n").encode()))
    return mutated, f"surface {ordinal}: rows={rows} walked -> rows=0 (walk erased)"


FALSIFIERS = {
    "wrong-bmp-digest": ("bmp-digests", mutate_bmp_digest),
    "block-count-mismatch": ("block-counts", mutate_block_line),
    "double-delivery": ("interact-delivery", mutate_interact_line),
    "focus-zero": ("focus-rows", mutate_focus_zero),
}


def falsify(mode: str, binary: Path, out: Path, captures: list[Capture], clean: list[Rule], as_json: bool) -> int:
    """Run one named falsifier: the gate must go red under that rule's name."""
    target, mutate = FALSIFIERS[mode]
    write_manifest(out, binary, captures, clean, {"falsify": {"mode": mode, "target": target}})
    undo: list[tuple[Path, bytes, str]] = []
    try:
        mutated, note = mutate(captures, undo)
        rules = check(mutated)
    finally:
        restored: list[str] = []
        for path, original, printed in undo:
            path.write_bytes(original)
            actual = file_digest(path)
            restored.append(f"{path.name}:sha256={actual}{'' if actual == printed else '!=printed'}")
        if restored:
            print(f"FALSIFY {mode}: artifact restored {' '.join(restored)}")
    failed = failed_names(rules)
    emit(binary, mutated, rules, as_json, {"falsify": {"mode": mode, "target": target, "mutation": note}})
    print(f"FALSIFY {mode}: mutated {note}")
    if target not in failed:
        print(f"FALSIFY {mode}: gate stayed green on {target} (failed={','.join(failed) or 'none'})")
        return 1
    print(f"FALSIFY {mode}: gate red - {target}")
    return 1


# ── E2E: delete one census row, then revert ────────────────────────────────

ARITY_51 = b"static ROWS: [SurfaceRow; 51] = ["
ARITY_50 = b"static ROWS: [SurfaceRow; 50] = ["
ROW_022 = (
    b'    no_panel(22, \'A\', "DaggerfallAdvancedSettingsWindow", FONT_DEFAULT, '
    b'(0, 0, 5, 7), "m39.settings.scale"),\n'
)


def patch_census(source: bytes) -> bytes | None:
    """Delete the 022 row and its array arity; None when the literals drifted."""
    if source.count(ARITY_51) != 1 or source.count(ROW_022) != 1:
        return None
    patched = source.replace(ARITY_51, ARITY_50, 1).replace(ROW_022, b"", 1)
    if patched.count(ROW_022) or patched.count(ARITY_50) != 1:
        return None
    return patched


def e2e(binary: Path, out: Path, skip_build: bool) -> int:
    """Delete one census row, measure the red gate, restore and measure green."""
    path = ROOT / SURFACES_FILE
    run(["git", "checkout", "--", SURFACES_FILE])
    if not path.is_file():
        print(f"E2E: FAIL - {SURFACES_FILE} is not a file")
        return 2
    backup = path.read_bytes()
    patched = patch_census(backup)
    if patched is None:
        print("E2E: FAIL - census patch literals drifted (arity or row 022)")
        return 2

    red_rule: str | None = None
    red_failed: list[str] = []
    probe_red = "absent"
    green = False
    try:
        path.write_bytes(patched)
        if not build(skip_build):
            print("E2E: FAIL - build of the patched crate")
            return 2
        red_captures, red_rules = gate(binary, out / "e2e-red")
        probe_red = verdict_of(red_captures[0]) or "absent"
        red_failed = failed_names(red_rules)
        red_rule = red_failed[0] if red_failed else None
        write_manifest(out / "e2e-red", binary, red_captures, red_rules, {"e2e": "patched-50"})
        print(
            f"E2E step 2 (022 removed): probe={probe_red} runner="
            f"{'FAIL' if red_failed else 'PASS'} failed={','.join(red_failed) or 'none'}"
        )
        for rule in red_rules:
            if not rule.ok:
                print(f"E2E step 2 FAIL {rule.name} {rule.detail}")
    finally:
        run(["git", "checkout", "--", SURFACES_FILE])
        if path.read_bytes() != backup:
            path.write_bytes(backup)
        restored = path.read_bytes() == backup
        built = build(skip_build)
        if restored and built:
            green_captures, green_rules = gate(binary, out / "e2e-green")
            green = not failed_names(green_rules)
            write_manifest(out / "e2e-green", binary, green_captures, green_rules, {"e2e": "restored-51"})
            print(
                f"E2E step 3 (restored): bytes-identical={restored} "
                f"{verdict_of(green_captures[0]) or 'absent'} rules-failed="
                f"{','.join(failed_names(green_rules)) or 'none'}"
            )
        else:
            print(f"E2E step 3 (restored): bytes-identical={restored} build={built}")

    tree = porcelain()
    crate_tree = porcelain("crates")
    patched_entry = porcelain(SURFACES_FILE)
    clean = crate_tree.strip() == "" and patched_entry.strip() == ""
    census_bit = "census-surface-set" in red_failed
    print(f"E2E: red rule={red_rule or 'none'} census-bit={census_bit} green={green}")
    print(f"E2E: git status --porcelain -> {tree!r}")
    print(f"E2E: git status --porcelain -- crates -> {crate_tree!r}")
    print(f"E2E: git status --porcelain -- {SURFACES_FILE} -> {patched_entry!r}")
    foreign = [line for line in tree.splitlines() if not line.endswith(SURFACES_FILE)]
    if clean:
        print("E2E: tree clean")
        if foreign:
            print(f"E2E: foreign-dirt (other lanes, not this E2E) -> {foreign!r}")
    else:
        print("E2E: tree dirty")
    ok = red_rule is not None and census_bit and green and clean
    print("VERDICT: PASS" if ok else "VERDICT: FAIL - e2e")
    return 0 if ok else 1


# ── (c) main ───────────────────────────────────────────────────────────────


def main() -> int:
    parser = argparse.ArgumentParser(
        description=__doc__,
        formatter_class=argparse.RawDescriptionHelpFormatter,
    )
    parser.add_argument("--binary", default=str(BINARY), help="prebuilt daggerfall-app")
    parser.add_argument("--out", default=str(ARTIFACTS), help="archive root (run1/, run2/, manifest.json)")
    parser.add_argument("--skip-build", action="store_true", help="do not run cargo build")
    parser.add_argument("--json", action="store_true", help="emit one JSON object instead of the table")
    parser.add_argument("--verbose", action="store_true", help="echo the run1 probe stdout first")
    parser.add_argument("--falsify", choices=sorted(FALSIFIERS), help="mutate the capture and require a red gate")
    parser.add_argument("--e2e-revert", action="store_true", help="delete one census row, measure red, revert")
    args = parser.parse_args()

    if arena2_dir() is None:
        print("VERDICT: SKIP - no ARENA2")
        return 2

    binary = Path(args.binary)
    out = Path(args.out)

    if args.e2e_revert:
        return e2e(binary, out, args.skip_build)

    if not build(args.skip_build):
        print("VERDICT: FAIL - build")
        return 2
    if not binary.is_file():
        print(f"VERDICT: FAIL - missing binary {binary}")
        return 2

    captures = capture_all(binary, out)
    rules = check(captures)

    if args.falsify:
        return falsify(args.falsify, binary, out, captures, rules, args.json)

    write_manifest(out, binary, captures, rules)
    if args.verbose:
        echo_probe(captures[0])
    return emit(binary, captures, rules, args.json)


if __name__ == "__main__":
    sys.exit(main())
