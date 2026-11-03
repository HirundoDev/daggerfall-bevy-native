#!/usr/bin/env python3
"""PROBE-UI-CREATE-001 - gate of M38/UI-2 (16 createchar surfaces, axes A1..A7).

Deterministic process contract
------------------------------
Both M38 probes print every line as a function of the retail bytes and of the
declarations, so a second process must print the same bytes. The runner
enforces exactly that:

1. `cargo build -p daggerfall-app` once.
2. `PROBE-UI-CREATE-001` in **two independent processes** (`PROBE_RUN=1|2`; the
   probe never prints that value).
3. `VERDICT: PASS` and exit 0 in both, byte-identical stdout (`cmp`) and equal
   sha256, the exact surface id set in ascending order, `AXIS A1`..`AXIS A7`
   present in order and all OK, and one `DIGEST` line equal in both runs.
4. Both captures, the frame banks (`--out run1/`, `--out run2/`) and a manifest
   are archived under `.sintergic/artifacts/m38-ui2/ui-create-001/` (gitignored:
   retail pixels are never committed).
5. A5 is re-measured from disk: every `BMP <name> sha256=<hex>` line printed by
   a run must equal the `sha256` of the file that same run wrote under its own
   `--out` directory, the printed bank must be exactly one frame per surface,
   and both runs must print the same bank (`bmp-digests`). A4 is re-measured
   from its own text: `empty=0`, `resolved=declarations`, and the published
   `textmap=rsc=..,literal=..,unmapped=..` must equal the `rsc`/`literal`/
   `unmapped` of the same line (`axis-a4-resolved`). Any mismatch is
   `VERDICT: FAIL` and exit 1.

Without ARENA2: `EXIT 2` + `VERDICT: SKIP - no ARENA2` - never a PASS.

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
GATE = "M38/UI-2"
PROBE = "PROBE-UI-CREATE-001"
SLUG = "ui-create-001"
BINARY = ROOT / "target" / "debug" / "daggerfall-app"
ARTIFACTS = ROOT / ".sintergic" / "artifacts" / "m38-ui2" / SLUG
CR = "CR-71f2b7bf-cb0d-43b3-bfaa-eb66de77988f"
SURFACES = tuple(f"C{index:02d}" for index in range(1, 17))
FRAMES = tuple(f"{surface.lower()}.bmp" for surface in SURFACES)
HEADER = f"{PROBE} - {len(SURFACES)} surfaces (M38/UI-2, {CR})"
BMP_LINE = re.compile(r"^BMP (\S+) sha256=([0-9a-f]{64})$")
SURFACE_LINE = re.compile(
    r"^SURFACE (\S+) (\S+) host=(\S+) assets=(\S+) nodes=(\d+) glyphs=(\d+) "
    r"focus=(\d+) opaque=(\d+) digest=([0-9a-f]{16})$"
)
AXIS_LINE = re.compile(r"^AXIS (A[1-7]) (OK|FAIL) (.*)$")
DIGEST_LINE = re.compile(r"^DIGEST ([0-9a-f]{64})$")
A4_FIELD = re.compile(r"(?:^|\s)(declarations|resolved|literal|readmodel|rsc|unmapped|empty)=(\d+)")
A4_TEXTMAP = re.compile(r"textmap=rsc=(\d+),literal=(\d+),unmapped=(\d+)")


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


def lines_of(capture: bytes) -> list[str]:
    return capture.decode("utf-8", "replace").splitlines()


def bmp_lines(capture: bytes) -> list[str]:
    """The `BMP <name> sha256=<hex>` evidence lines of one process, in order."""
    return [line for line in lines_of(capture) if line.startswith("BMP ")]


def header_check(lines: list[str]) -> tuple[bool, str]:
    """The first line declares this gate, this surface count and this cycle."""
    if not lines:
        return False, "empty-stdout"
    if lines[0] != HEADER:
        return False, f"header={lines[0][:72]!r}"
    return True, f"surfaces={len(SURFACES)}"


def surface_check(lines: list[str]) -> tuple[bool, str]:
    """A1 - every declared surface prints one `SURFACE` line, ids in order.

    Missing, extra, duplicated or reordered ids are FAILs: the set is the
    declaration of `m38_surfaces` (16 createchar surfaces), not a sample.
    """
    problems: list[str] = []
    ids: list[str] = []
    for line in lines:
        if not line.startswith("SURFACE "):
            continue
        match = SURFACE_LINE.match(line)
        if match is None:
            problems.append("malformed")
            continue
        ids.append(match.group(1))
    if ids != list(SURFACES):
        problems.append(f"ids={ids}")
    if problems:
        return False, ",".join(problems)
    return True, f"ids={len(ids)}"


def axis_check(lines: list[str]) -> tuple[bool, str]:
    """The seven §4.2 axes, present in order and all OK."""
    problems: list[str] = []
    axes: list[tuple[str, str]] = []
    for line in lines:
        if not line.startswith("AXIS "):
            continue
        match = AXIS_LINE.match(line)
        if match is None:
            problems.append("malformed")
            continue
        axes.append((match.group(1), match.group(2)))
        if match.group(2) != "OK":
            problems.append(f"{match.group(1)}=FAIL")
    expected = ["A1", "A2", "A3", "A4", "A5", "A6", "A7"]
    if [axis for axis, _ in axes] != expected:
        problems.append("order=" + ",".join(axis for axis, _ in axes))
    if problems:
        return False, ",".join(problems)
    return True, "axes=7"


def a4_check(lines: list[str]) -> tuple[bool, str]:
    """A4 - no empty declaration, all resolved, and the string map agrees.

    The published `textmap=rsc=..,literal=..,unmapped=..` is a projection of the
    same counters that precede it; a line whose two halves disagree is a FAIL.
    """
    a4 = [line for line in lines if line.startswith("AXIS A4 ")]
    if len(a4) != 1:
        return False, f"lines={len(a4)}"
    match = AXIS_LINE.match(a4[0])
    if match is None:
        return False, "malformed"
    detail = match.group(3)
    fields = {name: int(value) for name, value in A4_FIELD.findall(detail)}
    problems: list[str] = []
    missing = [name for name in ("declarations", "resolved", "empty") if name not in fields]
    if missing:
        problems.append("missing=" + ",".join(missing))
    if fields.get("empty") != 0:
        problems.append(f"empty={fields.get('empty')}")
    if fields.get("resolved") != fields.get("declarations"):
        problems.append(f"resolved={fields.get('resolved')}/{fields.get('declarations')}")
    textmap = A4_TEXTMAP.search(detail)
    if textmap is None:
        problems.append("no-textmap")
    else:
        published = tuple(int(value) for value in textmap.groups())
        counted = (fields.get("rsc"), fields.get("literal"), fields.get("unmapped"))
        if published != counted:
            problems.append(f"textmap={published}!={counted}")
    if problems:
        return False, ",".join(problems)
    return True, f"declarations={fields.get('declarations')} textmap={textmap.group(0)[8:]}"


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
            actual = file_digest(path)
            if actual is None:
                problems.append(f"run{index}:{name}:missing")
                continue
            if actual != printed:
                problems.append(f"run{index}:{name}:{printed[:8]}!={actual[:8]}")
        if names != list(FRAMES):
            problems.append(f"run{index}:names={names}")
    if banks[0] != banks[1]:
        problems.append("run1!=run2")
    if problems:
        return False, ",".join(problems)
    return True, f"files={len(FRAMES)}x2"


def digest_of(capture: bytes) -> str | None:
    """The probe's own `DIGEST` line (one per process), or None."""
    found = [match.group(1) for line in lines_of(capture) if (match := DIGEST_LINE.match(line))]
    return found[0] if len(found) == 1 else None


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
    lines = lines_of(first)
    probe_digests = [digest_of(first), digest_of(second)]

    checks: list[tuple[str, bool, str]] = [
        ("run1-verdict", "VERDICT: PASS" in first.decode("utf-8", "replace"), f"exit={exits[0]}"),
        ("run2-verdict", "VERDICT: PASS" in second.decode("utf-8", "replace"), f"exit={exits[1]}"),
        ("run1-exit", exits[0] == 0, str(exits[0])),
        ("run2-exit", exits[1] == 0, str(exits[1])),
        ("byte-identical", first == second, f"{len(first)} B"),
        ("sha256-equal", sha1 == sha2, sha1[:16]),
        ("header", *header_check(lines)),
        ("surface-set", *surface_check(lines)),
        ("axis-a1..a7", *axis_check(lines)),
        ("axis-a4-resolved", *a4_check(lines)),
        ("bmp-digests", *bmp_check(captures, [ARTIFACTS / "run1", ARTIFACTS / "run2"])),
        (
            "probe-digest",
            probe_digests[0] is not None and probe_digests[0] == probe_digests[1],
            probe_digests[0][:16] if probe_digests[0] else "absent",
        ),
    ]

    failed = [name for name, ok, _ in checks if not ok]
    verdict = "FAIL" if failed else "PASS"
    manifest = {
        "gate": GATE,
        "probe": PROBE,
        "cr": CR,
        "slug": SLUG,
        "commit": head_commit(),
        "binary": {
            "path": os.path.relpath(binary, ROOT),
            "sha256": file_digest(binary),
        },
        "surfaces": len(SURFACES),
        "runs": [
            {
                "run": index + 1,
                "exit": exits[index],
                "bytes": len(captures[index]),
                "stdout_sha256": digest(captures[index]),
                "probe_digest": probe_digests[index],
                "surface_ids": [
                    match.group(1)
                    for line in lines_of(captures[index])
                    if (match := SURFACE_LINE.match(line))
                ],
                "bmp": bmp_lines(captures[index]),
            }
            for index in range(2)
        ],
        "byte_identical": first == second,
        "checks": [{"name": name, "ok": ok, "detail": detail} for name, ok, detail in checks],
        "failed": failed,
        "verdict": verdict,
        "exit": 1 if failed else 0,
    }
    (ARTIFACTS / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")

    sys.stdout.write(first.decode("utf-8", "replace"))
    if first and not first.endswith(b"\n"):
        print()
    print(f"GATE {PROBE} processes=2 stdout={len(first)}B sha256={sha1}")
    print(f"GATE archive={ARTIFACTS.relative_to(ROOT)} manifest={manifest['commit'] or 'no-HEAD'}")
    for name, ok, detail in checks:
        print(f"GATE check {name}: {'OK' if ok else 'FAIL'} {detail}".rstrip())
    if failed:
        print(f"GATE failed={','.join(failed)}")
        print(f"VERDICT: FAIL - process-contract ({','.join(failed)})")
        return 1
    print("GATE checks=" + ",".join(name for name, _, _ in checks) + " all-OK")
    print("VERDICT: PASS")
    return 0


if __name__ == "__main__":
    sys.exit(main())
