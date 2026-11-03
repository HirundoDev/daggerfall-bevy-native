#!/usr/bin/env python3
"""PROBE-UI-BASE-001 - gate of M37/UI-1 block F3 (axes A1, A2, A3, A4, A7).

Two separate processes, same binary, same retail bank
-----------------------------------------------------
The gate is only as strong as its determinism, so it is measured, not assumed:

1. `cargo build -p daggerfall-app` once (the runner never rebuilds between runs).
2. `PROBE-UI-BASE-001` runs in **two independent processes** with `PROBE_RUN=1`
   and `PROBE_RUN=2` (the probe never prints that value; it exists so a stray
   environment dependence would show up as a stdout difference).
3. Both stdout captures must contain `VERDICT: PASS` and exit 0, must be
   **byte-identical** (`cmp`) and must hash to the same sha256.
4. Both captures plus the 24-bit BMPs (`--out`) and a manifest are archived
   under `.sintergic/artifacts/m37-ui1/ui-base-001/` (gitignored: retail pixels
   are never committed).
5. A5 is re-measured from disk: every `BMP <name> sha256=<hex>` line printed by
   run 1 must equal the same line in run 2, and each printed digest must equal
   the `sha256sum` of the file that run wrote under its own `--out` directory
   (`bmp-digests`). A mismatch is `VERDICT: FAIL` and exit 1.

Without an accessible ARENA2 the gate is `EXIT 2` + `VERDICT: SKIP - no ARENA2`;
a missing bank is never a PASS.

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
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
PROBE = "PROBE-UI-BASE-001"
SLUG = "ui-base-001"
BINARY = ROOT / "target" / "debug" / "daggerfall-app"
ARTIFACTS = ROOT / ".sintergic" / "artifacts" / "m37-ui1" / SLUG
EXPECTED_BMPS = ("p1.bmp", "p2.bmp", "p3.bmp")
BMP_LINE = re.compile(r"^BMP (\S+) sha256=([0-9a-f]{64})$")


def arena2_dir() -> Path | None:
    """Same resolution as the probe's presence test (protocol §4.2):
    `$DF_ARENA2`, then `$DF_APP_ROOT/ARENA2`, then `$HOME/Games/Daggerfall/app/ARENA2`.
    `$DF_ARENA2` is taken as given - a bad value is the probe's IO failure, not a
    runner-side SKIP.
    """
    explicit = os.environ.get("DF_ARENA2")
    if explicit:
        return Path(explicit)
    root = os.environ.get("DF_APP_ROOT")
    if root:
        candidates = [Path(root)]
    else:
        home = os.environ.get("HOME")
        if not home:
            return None
        candidates = [Path(home) / "Games" / "Daggerfall" / "app"]
    for base in candidates:
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


def bmp_lines(capture: bytes) -> list[str]:
    """The `BMP <name> sha256=<hex>` evidence lines of one process, in order."""
    return [line for line in capture.decode("utf-8", "replace").splitlines() if line.startswith("BMP ")]


def bmp_check(captures: list[bytes], dirs: list[Path]) -> tuple[bool, str]:
    """A5 - every printed frame digest equals the bytes that process left on disk.

    Both halves of protocol §4.1 A5 are enforced: the two processes printed the
    same `BMP` lines (two banks, identical bytes) and each printed digest equals
    the sha256 of the file the same process wrote under `--out`. A missing file,
    a malformed line, an empty or renamed bank, or a run-dependent digest are all
    FAILs - this check is built to fail, not to confirm.
    """
    problems: list[str] = []
    banks = [bmp_lines(capture) for capture in captures]
    for index, (lines, out_dir) in enumerate(zip(banks, dirs), start=1):
        names: list[str] = []
        for line in lines:
            match = BMP_LINE.match(line)
            if match is None:
                problems.append(f"run{index}:malformed")
                continue
            name, printed = match.group(1), match.group(2)
            names.append(name)
            path = out_dir / name
            if not path.is_file():
                problems.append(f"run{index}:{name}:missing")
                continue
            actual = digest(path.read_bytes())
            if actual != printed:
                problems.append(f"run{index}:{name}:{printed[:8]}!={actual[:8]}")
        if names != list(EXPECTED_BMPS):
            problems.append(f"run{index}:names={names}")
    if banks[0] != banks[1]:
        problems.append("run1!=run2")
    if problems:
        return False, ",".join(problems)
    return True, f"files={len(EXPECTED_BMPS)}x2"


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", default=str(BINARY), help="prebuilt daggerfall-app")
    parser.add_argument("--skip-build", action="store_true", help="do not run cargo build")
    args = parser.parse_args()

    if arena2_dir() is None:
        print("VERDICT: SKIP - no ARENA2")
        return 2

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

    if ARTIFACTS.exists():
        shutil.rmtree(ARTIFACTS)
    (ARTIFACTS / "run1").mkdir(parents=True)
    (ARTIFACTS / "run2").mkdir(parents=True)

    captures: list[bytes] = []
    exits: list[int] = []
    for run_id in (1, 2):
        out_dir = ARTIFACTS / f"run{run_id}"
        proc = run(
            [str(binary), "--probe", PROBE, "--out", str(out_dir)],
            env={"PROBE_RUN": str(run_id)},
        )
        captures.append(proc.stdout)
        exits.append(proc.returncode)
        (ARTIFACTS / f"run{run_id}.stdout").write_bytes(proc.stdout)
        (ARTIFACTS / f"run{run_id}.stderr").write_bytes(proc.stderr)

    first, second = captures
    sha1, sha2 = digest(first), digest(second)
    stdout_lines = first.decode("utf-8", "replace").splitlines()
    pilot_lines = sum(1 for line in stdout_lines if line.startswith("PILOT "))
    axis_lines = sum(1 for line in stdout_lines if line.startswith("AXIS "))

    checks: list[tuple[str, bool, str]] = [
        ("run1-verdict", b"VERDICT: PASS" in first, f"exit={exits[0]}"),
        ("run2-verdict", b"VERDICT: PASS" in second, f"exit={exits[1]}"),
        ("run1-exit", exits[0] == 0, str(exits[0])),
        ("run2-exit", exits[1] == 0, str(exits[1])),
        ("byte-identical", first == second, f"{len(first)} B"),
        ("sha256-equal", sha1 == sha2, sha1[:16]),
        ("pilot-lines", pilot_lines == 3, str(pilot_lines)),
        ("axis-lines", axis_lines == 5, str(axis_lines)),
        ("bmp-digests", *bmp_check(captures, [ARTIFACTS / "run1", ARTIFACTS / "run2"])),
    ]

    bmp_evidence = sorted(bmp_lines(first))
    manifest = {
        "probe": PROBE,
        "runs": [
            {
                "run": index + 1,
                "exit": exits[index],
                "bytes": len(captures[index]),
                "sha256": digest(captures[index]),
            }
            for index in range(2)
        ],
        "byte_identical": first == second,
        "bmp": bmp_evidence,
        "checks": [{"name": name, "ok": ok, "detail": detail} for name, ok, detail in checks],
    }
    (ARTIFACTS / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")

    sys.stdout.write(first.decode("utf-8", "replace"))
    if first and not first.endswith(b"\n"):
        print()
    print(f"GATE {PROBE} processes=2 stdout={len(first)}B sha256={sha1}")
    print(f"GATE archive={ARTIFACTS.relative_to(ROOT)}")
    failed = [name for name, ok, _ in checks if not ok]
    if failed:
        print(f"GATE failed={','.join(failed)}")
        print("VERDICT: FAIL - process-contract")
        return 1
    print("GATE checks=" + ",".join(name for name, _, _ in checks) + " all-OK")
    print("VERDICT: PASS")
    return 0


if __name__ == "__main__":
    sys.exit(main())
