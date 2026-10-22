#!/usr/bin/env python3
"""launch.py — wrapper Python del launcher de release (CR-b8a547ad).

Fuente única de verdad = scripts/launch.sh: este wrapper solo re-ejecuta el
mismo argv con bash y propaga su exit code (0 ok · 1 build · 2 uso · 3 entorno
· 124 timeout · >124 código del probe). 0 lógica duplicada, 0 deps fuera de la
stdlib.

Uso:
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
        sys.stderr.write(f"launch.py: ERROR: no existe {LAUNCH_SH}\n")
        return 1
    os.execvp("bash", ["bash", LAUNCH_SH, *argv])
    # os.execvp solo retorna si falla al lanzar bash.
    sys.stderr.write("launch.py: ERROR: no se pudo ejecutar bash\n")
    return 1


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
