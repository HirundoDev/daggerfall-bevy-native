#!/usr/bin/env python3
"""artifact-prune.py — poda de directorios de corrida con dry-run por defecto (CR-4f8a1c93).

`shell-002.py` crea un directorio por invocacion (`.sintergic/artifacts/shell-002/
<UTC>-p<pid>`, commit 67a585a) y nada los poda: ~250 KB por corrida, para siempre.
Esta herramienta aplica una politica explicita y auditable, sin heuristicas:

  · Solo se consideran entradas de `--root` cuyo NOMBRE encaje EXACTO con
    `^\\d{8}T\\d{6}Z-p\\d+$` **y** que ademas sean directorios. Todo lo demas
    (`stdout.txt`, `t1`..`t3`, `manifest.json`, ficheros sueltos, directorios con
    otro nombre, o un fichero que imite un nombre valido) queda FUERA de
    consideracion: se lista como SKIP y NUNCA se borra.
  · Se conservan los `--keep N` (10 por defecto) mas recientes por orden ESTRICTO
    del nombre: (timestamp UTC, pid NUMERICO). El pid no se compara como texto:
    `p1000000` es mas nuevo que `p999999`.
  · Un directorio cuyo pid sigue vivo (`/proc/<pid>` existe) se marca ALIVE y NO
    se borra, este dentro o fuera de la ventana: una corrida en vuelo es evidencia
    en construccion.
  · Sin `--apply` no se toca NADA: solo se imprime la tabla. Con `--apply` se
    borran unicamente los PRUNE.

Salida (tabla estable): `dir  mtime  size  pid  estado`, estado ∈
{KEEP, PRUNE, ALIVE, SKIP}.

Uso:
    python3 scripts/artifact-prune.py                       # dry-run, keep=10
    python3 scripts/artifact-prune.py --keep 5 --apply       # poda real
    python3 scripts/artifact-prune.py --self-test            # 3 falsificadores

Exit: 0 dry-run / 0 apply correcto · 1 fallo al borrar o self-test rojo ·
2 root inexistente o uso invalido.
"""
from __future__ import annotations

import argparse
import os
import re
import shutil
import sys
import tempfile
import time
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
DEFAULT_ROOT = REPO / ".sintergic" / "artifacts" / "shell-002"
NAME_RE = re.compile(r"^\d{8}T\d{6}Z-p\d+$")
ORDER_RE = re.compile(r"^(\d{8}T\d{6}Z)-p(\d+)$")

KEEP, PRUNE, ALIVE, SKIP = "KEEP", "PRUNE", "ALIVE", "SKIP"


def pid_alive(pid: int) -> bool:
    """El pid existe en /proc: la corrida puede seguir escribiendo ahi."""
    return pid > 1 and os.path.isdir("/proc/%d" % pid)


def tree_size(path: Path) -> int:
    """Bytes de los ficheros regulares bajo `path` (symlinks no se siguen)."""
    if path.is_file() and not path.is_symlink():
        return path.stat().st_size
    total = 0
    for base, dirs, files in os.walk(path, followlinks=False):
        for name in files:
            candidate = Path(base) / name
            try:
                total += candidate.lstat().st_size
            except OSError:
                continue
    return total


def order_key(name: str):
    """Orden estricto: timestamp UTC y pid como entero (no como texto)."""
    match = ORDER_RE.match(name)
    return (match.group(1), int(match.group(2)))


def plan(root: Path, keep: int) -> list[dict]:
    """Clasifica cada entrada del root; no toca el disco."""
    considered, skipped = [], []
    for entry in sorted(root.iterdir(), key=lambda p: p.name):
        if NAME_RE.match(entry.name) and entry.is_dir() and not entry.is_symlink():
            considered.append(entry)
        else:
            skipped.append(entry)

    considered.sort(key=lambda p: order_key(p.name))
    window = len(considered) - keep
    rows = []
    for index, entry in enumerate(considered):
        pid = int(ORDER_RE.match(entry.name).group(2))
        if pid_alive(pid):
            state = ALIVE
        elif index < window:
            state = PRUNE
        else:
            state = KEEP
        rows.append({"path": entry, "name": entry.name, "pid": pid, "estado": state})
    for entry in skipped:
        rows.append({"path": entry, "name": entry.name, "pid": None, "estado": SKIP})
    rows.sort(key=lambda row: row["name"])
    for row in rows:
        stat = row["path"].lstat()
        row["mtime"] = time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime(stat.st_mtime))
        row["size"] = tree_size(row["path"]) if row["path"].is_dir() else stat.st_size
    return rows


def render(rows, root: Path, keep: int, apply: bool) -> list[str]:
    mode = "apply" if apply else "dry-run (nada se borra)"
    lines = [
        "root: %s" % root,
        "keep: %d · modo: %s" % (keep, mode),
        "%-34s %-20s %10s %8s %s" % ("dir", "mtime", "size", "pid", "estado"),
    ]
    for row in rows:
        lines.append("%-34s %-20s %10d %8s %s" % (
            row["name"], row["mtime"], row["size"],
            "-" if row["pid"] is None else row["pid"], row["estado"]))
    counts = {state: 0 for state in (KEEP, PRUNE, ALIVE, SKIP)}
    for row in rows:
        counts[row["estado"]] += 1
    lines.append("resumen: considerados=%d · KEEP=%d · PRUNE=%d · ALIVE=%d · SKIP=%d" % (
        counts[KEEP] + counts[PRUNE] + counts[ALIVE],
        counts[KEEP], counts[PRUNE], counts[ALIVE], counts[SKIP]))
    return lines


def apply_plan(rows) -> tuple[int, int]:
    """Borra solo los PRUNE; devuelve (borrados, fallos). Nunca cuenta lo no borrado."""
    deleted = failures = 0
    for row in rows:
        if row["estado"] != PRUNE:
            continue
        try:
            shutil.rmtree(row["path"])
            deleted += 1
            print("BORRADO %s (%d bytes)" % (row["name"], row["size"]))
        except OSError as exc:
            failures += 1
            print("ERROR al borrar %s: %s" % (row["name"], exc), file=sys.stderr)
    return deleted, failures


# ── self-test: tres falsificadores propios sobre arboles temporales ──────────

def _dead_pids(count: int) -> list[int]:
    """Pids que hoy NO existen en /proc (para que ALIVE no ensucie el control)."""
    found = []
    for candidate in range(990000, 980000, -1):
        if not pid_alive(candidate):
            found.append(candidate)
            if len(found) == count:
                return found
    raise SystemExit("self-test: no hay %d pids muertos disponibles" % count)


def _make_dirs(root: Path, names: list[str]) -> None:
    for name in names:
        target = root / name
        (target / "receipts").mkdir(parents=True)
        (target / "receipts" / "inv-1.json").write_text('{"gate": "x"}\n', encoding="utf-8")


def _valid_names(pids: list[int], start: int = 0) -> list[str]:
    return ["20260101T0000%02dZ-p%d" % (start + i, pid) for i, pid in enumerate(pids)]


def _run(args: list[str]):
    import subprocess
    return subprocess.run([sys.executable, str(Path(__file__).resolve())] + args,
                          capture_output=True, text=True)


def _states(stdout: str) -> dict:
    """Lee la tabla del CLI: nombre -> estado."""
    out = {}
    for line in stdout.splitlines():
        parts = line.split()
        if len(parts) == 5 and parts[4] in (KEEP, PRUNE, ALIVE, SKIP):
            out[parts[0]] = parts[4]
    return out


def _checker(failures: list[str], tag: str):
    def check(condition: bool, message: str) -> None:
        print("    %s %s" % ("ok  " if condition else "FALLO", message))
        if not condition:
            failures.append("%s: %s" % (tag, message))
    return check


def _reconcile(failures: list[str], tag: str, root: Path, states: dict,
               expect: dict) -> None:
    """Cuenta `path exists` por veredicto y compara con lo declarado."""
    check = _checker(failures, tag)
    for name, state in expect.items():
        got = states.get(name)
        check(got == state, "%s estado %s (esperado %s)" % (name, got, state))
    tally = {state: [0, 0] for state in (KEEP, PRUNE, ALIVE, SKIP)}
    for name, state in states.items():
        exists = (root / name).exists()
        tally[state][1] += 1
        if exists == (state != PRUNE):
            tally[state][0] += 1
        else:
            failures.append("%s: %s declarado %s pero exists=%s" % (tag, name, state, exists))
    print("    existencia: " + " · ".join(
        "%s %d/%d" % (state, tally[state][0], tally[state][1])
        for state in (KEEP, PRUNE, ALIVE, SKIP)))


def _case_a(failures: list[str], base: Path) -> None:
    print("(a) 12 validos · keep=10 -> los 2 mas viejos PRUNE, los 10 recientes KEEP")
    root = base / "a"
    root.mkdir()
    dead = _dead_pids(12)
    # Dos directorios comparten el timestamp mas viejo y difieren en el pid: el
    # orden estricto (timestamp, pid NUMERICO) debe distinguirlos.
    names = _valid_names(dead)
    _make_dirs(root, names)
    result = _run(["--root", str(root), "--keep", "10"])
    check = _checker(failures, "a")
    check(result.returncode == 0, "dry-run exit 0 (got %d)" % result.returncode)
    states = _states(result.stdout)
    expect = {name: (PRUNE if i < 2 else KEEP) for i, name in enumerate(names)}
    check(states == expect, "veredictos %s" % ("coinciden" if states == expect else states))
    check(all((root / name).exists() for name in names), "dry-run no borro nada (12/12)")
    applied = _run(["--root", str(root), "--keep", "10", "--apply"])
    check(applied.returncode == 0, "--apply exit 0 (got %d)" % applied.returncode)
    _reconcile(failures, "a", root, _states(applied.stdout) or states, expect)

    # Mismo timestamp, pids 999999 vs 1000000: con keep=11 solo el pid 999999
    # cae fuera; un orden lexicografico podaria el 1000000.
    tie = base / "a-tie"
    tie.mkdir()
    tie_names = ["20260101T000000Z-p999999", "20260101T000000Z-p1000000"] + \
                _valid_names([pid for pid in _dead_pids(12) if pid not in (999999, 1000000)][:10], start=1)
    _make_dirs(tie, tie_names)
    tie_result = _run(["--root", str(tie), "--keep", "11"])
    tie_states = _states(tie_result.stdout)
    check(tie_states.get("20260101T000000Z-p999999") == PRUNE,
          "orden estricto: p999999 PRUNE con keep=11")
    check(tie_states.get("20260101T000000Z-p1000000") == KEEP,
          "orden estricto: p1000000 KEEP con keep=11 (no lexicografico)")


def _case_b(failures: list[str], base: Path) -> None:
    print("(b) stdout.txt, dir fuera de patron y fichero-con-nombre-valido sobreviven a --apply")
    root = base / "b"
    root.mkdir()
    names = _valid_names(_dead_pids(12))
    _make_dirs(root, names)
    (root / "stdout.txt").write_text("evidencia de otra epoca\n", encoding="utf-8")
    (root / "t1").mkdir()
    (root / "t1" / "nota.txt").write_text("x\n", encoding="utf-8")
    (root / "20260101T000023Z-p555555").write_text("soy un fichero\n", encoding="utf-8")

    check = _checker(failures, "b")
    dry = _run(["--root", str(root)])
    check(dry.returncode == 0, "dry-run exit 0")
    check((root / "stdout.txt").exists() and (root / "t1").exists(),
          "dry-run no toco las entradas fuera de consideracion")
    states = _states(dry.stdout)
    check(states.get("stdout.txt") == SKIP, "stdout.txt SKIP")
    check(states.get("t1") == SKIP, "dir fuera de patron SKIP")
    check(states.get("20260101T000023Z-p555555") == SKIP, "fichero con nombre valido SKIP")

    applied = _run(["--root", str(root), "--apply"])
    check(applied.returncode == 0, "--apply exit 0 (got %d)" % applied.returncode)
    check((root / "stdout.txt").is_file(), "stdout.txt sobrevive a --apply")
    check((root / "t1" / "nota.txt").is_file(), "t1/ sobrevive a --apply")
    check((root / "20260101T000023Z-p555555").is_file(),
          "el fichero con nombre valido sobrevive a --apply")
    expect = {name: (PRUNE if i < 2 else KEEP) for i, name in enumerate(names)}
    expect.update({"stdout.txt": SKIP, "t1": SKIP,
                   "20260101T000023Z-p555555": SKIP})
    _reconcile(failures, "b", root, _states(applied.stdout), expect)


def _case_c(failures: list[str], base: Path) -> None:
    print("(c) pid vivo fuera de ventana -> ALIVE y no se borra (pid real: propio proceso)")
    root = base / "c"
    root.mkdir()
    live = os.getpid()
    check = _checker(failures, "c")
    if not pid_alive(live):
        check(False, "el propio pid %d no aparece en /proc: imposible montar el control" % live)
        return
    oldest = "20260101T000000Z-p%d" % live
    dead = _dead_pids(11)
    names = [oldest] + _valid_names(dead, start=1)
    _make_dirs(root, names)

    dry = _run(["--root", str(root), "--keep", "10"])
    check(dry.returncode == 0, "dry-run exit 0")
    states = _states(dry.stdout)
    check(states.get(oldest) == ALIVE, "%s ALIVE (fuera de la ventana)" % oldest)
    check(states.get(names[1]) == PRUNE, "%s PRUNE (muerto y fuera de ventana)" % names[1])
    check((root / oldest).exists(), "dry-run conserva el dir con pid vivo")

    applied = _run(["--root", str(root), "--keep", "10", "--apply"])
    check(applied.returncode == 0, "--apply exit 0")
    check((root / oldest).exists(), "el dir con pid vivo SOBREVIVE a --apply")
    check(not (root / names[1]).exists(), "el PRUNE vecino si se borro")
    expect = {name: (KEEP if i >= 2 else (ALIVE if i == 0 else PRUNE))
              for i, name in enumerate(names)}
    _reconcile(failures, "c", root, _states(applied.stdout), expect)


def self_test() -> int:
    failures: list[str] = []
    print("SELF-TEST artifact-prune · politica: patron exacto + dir, keep estricto, pid vivo intocable")
    with tempfile.TemporaryDirectory(prefix="artifact-prune-selftest-") as tmp:
        base = Path(tmp)
        for case in (_case_a, _case_b, _case_c):
            before = len(failures)
            case(failures, base)
            print("    -> %s" % ("OK" if len(failures) == before else "FALLO"))
    if failures:
        print("SELF-TEST: %d fallo(s)" % len(failures))
        for item in failures:
            print("  - %s" % item)
        return 1
    print("SELF-TEST: 3/3 falsificadores cazados (a orden+ventana · b exclusion · c pid vivo)")
    return 0


def main(argv) -> int:
    parser = argparse.ArgumentParser(
        description="Poda .sintergic/artifacts/shell-002 con dry-run por defecto (CR-4f8a1c93).")
    parser.add_argument("--root", default=str(DEFAULT_ROOT),
                        help="directorio de artefactos (default: .sintergic/artifacts/shell-002)")
    parser.add_argument("--keep", type=int, default=10,
                        help="directorios validos mas recientes a conservar (default: 10)")
    parser.add_argument("--apply", action="store_true",
                        help="borra los PRUNE (sin este flag no se toca nada)")
    parser.add_argument("--self-test", action="store_true",
                        help="ejecuta los 3 falsificadores sobre arboles temporales")
    args = parser.parse_args(argv)

    if args.self_test:
        return self_test()
    if args.keep < 0:
        print("artifact-prune: --keep debe ser >= 0", file=sys.stderr)
        return 2
    root = Path(args.root).expanduser().resolve()
    if not root.is_dir():
        print("artifact-prune: root inexistente: %s" % root, file=sys.stderr)
        return 2

    rows = plan(root, args.keep)
    for line in render(rows, root, args.keep, args.apply):
        print(line)
    planned = sum(1 for row in rows if row["estado"] == PRUNE)
    if not args.apply:
        print("borrados: 0 de %d planificados (dry-run)" % planned)
        return 0
    deleted, failures = apply_plan(rows)
    print("borrados: %d de %d planificados" % (deleted, planned))
    if failures or deleted != planned:
        print("artifact-prune: %d borrado(s) no completado(s)" % (planned - deleted),
              file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
