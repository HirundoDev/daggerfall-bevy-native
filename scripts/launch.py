#!/usr/bin/env python3
"""launch.py — Python wrapper around the release launcher (CR-b8a547ad).

Single source of truth = scripts/launch.sh: this wrapper only re-executes the
same argv with bash and propagates its exit code (0 ok · 1 build · 2 usage ·
3 environment · 124 timeout · >124 probe code). 0 duplicated logic, 0
dependencies outside the stdlib.

Usage:
  python3 scripts/launch.py --mode preview --headless
  python3 scripts/launch.py --probe ALL-001
  python3 scripts/launch.py --help
"""

from __future__ import annotations

import os
import sys

SCRIPT_DIR = os.path.dirname(os.path.abspath(__file__))
LAUNCH_SH = os.path.join(SCRIPT_DIR, "launch.sh")


def main(argv: list[str]) -> int:
    if not os.path.isfile(LAUNCH_SH):
        sys.stderr.write(f"launch.py: ERROR: missing {LAUNCH_SH}\n")
        return 1
    os.execvp("bash", ["bash", LAUNCH_SH, *argv])
    # os.execvp only returns if launching bash failed.
    sys.stderr.write("launch.py: ERROR: could not execute bash\n")
    return 1


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
