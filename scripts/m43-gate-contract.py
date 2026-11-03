#!/usr/bin/env python3
"""Render the M43 gate contract (CR-39d656e1) from the gate modules themselves.

The gates are the contract; this script only projects them into Markdown so the
document can never drift from the code it describes. Run:

    python3 scripts/m43-gate-contract.py            # print
    python3 scripts/m43-gate-contract.py --write    # write the doc in place
    python3 scripts/m43-gate-contract.py --selftest # prove the `probe` column is falsifiable

The depth table (§3.8 rows) carries the probe each row is measured on, resolved
exactly like `Gate.row_checks` (`row.probe or gate.probes[0]`). `--selftest`
mutates that cell in memory and fails if the mutation is not what the document
shows — a projected column that nobody can move is decoration, not an instrument.
"""

from __future__ import annotations

import importlib.util
import re
import sys
from dataclasses import replace
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
CR = "CR-39d656e1-1987-4c43-aa7c-149f6f96d2cb"
DOC = ROOT / f"docs/sintergic/validation/m43-gate-contract-{CR}.md"
GATES = [
    "sys-econ-001.py",
    "sys-bank-001.py",
    "sys-travel-001.py",
    "sys-weather-001.py",
    "sys-calendar-001.py",
    "sys-condition-001.py",
    "sys-region-001.py",
]


def load(name: str):
    path = ROOT / "scripts" / name
    spec = importlib.util.spec_from_file_location(path.stem.replace("-", "_"), path)
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    assert spec.loader is not None
    spec.loader.exec_module(module)
    return module.GATE


def cell(text: str) -> str:
    return str(text).replace("|", "\\|").replace("\n", " ")


def spec_name(value) -> str:
    """A condition renders as its qualified name, never `<function ... at 0x...>`:
    the address is ASLR, not a spec, and made every regeneration churn 80 lines."""
    return re.sub(r" at 0x[0-9a-f]+", "", str(value))


HEADER = f"""# Contrato de los gates M43 — {CR}

> Estado de este documento: **contrato del ejecutor** (M43 SYS-3, 51 u).
> Lane autora: **Lane A**. Lane implementadora: **Lane B** (posterior).
> **Lane B NO edita `scripts/m43_gate_lib.py` ni ninguno de los `scripts/sys-*-001.py`**:
> son el gate que tiene que pasar. Si una fila parece mal especificada, se abre una
> fila C1 nueva; no se toca el gate dentro de la lane que implementa.
>
> Estado declarado de esta lane al cerrar: **`candidato`** — nunca DONE/PASS sobre M43.

## Cómo se ejecuta

```bash
python3 scripts/sys-<familia>-001.py              # construye (flock) y verifica
python3 scripts/sys-<familia>-001.py --no-build   # exige SOURCES MATCH (G4)
python3 scripts/sys-<familia>-001.py --falsify <nombre>
python3 scripts/sys-<familia>-001.py --json | --verbose
```

Códigos de salida (G10), nunca un traceback:

| exit | significado |
|---|---|
| 0 | todas las filas verdes |
| 1 | ≥1 fila roja (incluye `[MISSING]`: la fila existe y no está cumplida) |
| 2 | NC — el entorno no permite medir (runtime no disponible, probe que exige ARENA2 con `exit 2`) |
| 3 | STALE-BINARY — la huella `SOURCES` del binario no coincide con los ficheros en disco, o `--no-build` sin MATCH |

## Mecánica común (`scripts/m43_gate_lib.py`)

Un único módulo implementa la mecánica que todos los gates comparten; cada
`scripts/sys-*-001.py` es la especificación de su familia. Esto **no** viola G1:
G1 prohíbe editar los `scripts/*.py` preexistentes, no crear ficheros nuevos.

Filas de arnés que todo gate emite por probe, sin escribir gramática nueva:

| fila | qué exige |
|---|---|
| `process-<probe>` | el probe corre 2 veces, `exit` observado en {{0}} (o {{0,2}} si es retail) |
| `byte-identical-<probe>` | las dos ejecuciones son byte-idénticas (G2) |
| `stdout-sha256-<probe>` | sha256 del stdout, registrado en la evidencia |
| `DIGEST-LINE-<probe>` | línea terminal `DIGEST <sha256>` sobre todo lo impreso antes (G6) |
| `SOURCES-FINGERPRINT-<probe>` | `SOURCES fingerprint=<64 hex> paths=<...>` recomputado desde disco (G3/G4) |
| `citations-<probe>` | toda fila `[OK]` lleva `src=<path>:<line>` resoluble (G7) |
| `citations-guard-<probe>` | la comprobación de citas muerde en las DOS direcciones sobre el mismo stdout real: la cita real resuelve y dos copias doctoradas (fichero inexistente, línea > total) se ponen rojas **nombrando la fila** |
| `FLOATS-<probe>` | 0 tokens `clave=<decimal>` en stdout (G5) |
| `no-fail-rows-<probe>` | ninguna línea `FAIL ` en stdout |
| `environment-<probe>` | ARENA2 presente/ausente, para separar el bloque retail (G12) |

Política de ausencia: un `DIGEST`/`SOURCES` **ausente** es `[MISSING]` (exit 1) —
estado de pre-implementación, no binario rancio; un `SOURCES` **que no coincide**
es `exit 3` (STALE-BINARY). Un probe que exige ARENA2 y sale 2 no puede producir
verde ni rojo: el veredicto es NC.

---

"""

FOOTER = """
## Tabla de aceptación (roadmap §3.8) → filas que la cubren

| familia | aceptación literal | filas de profundidad que la miden |
|---|---|---|
| Economía | stock, 36 pares, precios por región, venta | `ECON-STOCK-CENSUS`, `ECON-SELL-MARGIN`, `ECON-HAGGLE-CENSUS`, `REGION-PRICE-STATE` |
| Banco | 12 operaciones, vencimientos, embargo; casa/barco persistidos | `BANK-OPERATIONS-CENSUS`, `BANK-DUE-DATES`, `BANK-REMINDERS`, `BANK-FORECLOSURE`, `BANK-PROPERTY-PERSIST` |
| Viaje | vectores dorados tiempo/coste por modo; encuentro determinista ×2 | `TRAVEL-MODES`, `TRAVEL-COST-MODES`, `TRAVEL-ENCOUNTER`, `TRAVEL-ENCOUNTER-NEGATIVE` |
| Clima | 7 estados × 10 climas = 70/70; efectos suelo/niebla/mojado | `WEATHER-MATRIX`, `WEATHER-EFFECTS-SUNLIGHT`, `-SHADOW`, `-FOG`, `-WET` |
| Calendario | vectores bit-exactos fases y festivos; efecto por festivo declarado | `CALENDAR-HOLIDAYS`, `CALENDAR-HOLIDAY-VECTORS`, `CALENDAR-HOLIDAY-EFFECTS`, `CALENDAR-HOLIDAY-NEGATIVE` |
| Condición | censo enfermedad/veneno con `covered`/`NEGATIVE`; transformación y cura medidos | `CONDITION-CENSUS`, `CONDITION-NEGATIVE*`, `CONDITION-TRANSFORM`, `CONDITION-CURE` |
| Región | precio/reputación legal/clima político por región con estado | `REGION-PRICE-STATE`, `REGION-LEGAL-REPUTATION`, `REGION-POLITICAL-CLIMATE`, `REGION-STATE-DIGEST` |

Los 70 del clima son medibles en este árbol: `Climate::from_climate_value`
(`crates/heart/src/weather.rs:147`) mapea los diez valores crudos 223..=232 sobre
las seis filas de `WEATHER_TABLE`; 7 estados × 10 valores crudos = 70 celdas.

Las 61 regiones son el `REGION_COUNT` del parser (`MAPS.BSA`, regiones 0..60).
El libro de banco usa 62 ranuras; el censo regional usa las 61 regiones reales.

---

## Falsificación

Cada familia declara ≥1 mutación de fuente **byte-exacta y única** que debe poner
roja una fila nombrada. El flujo (`--falsify <nombre>`) es:

1. la aguja aparece exactamente una vez en el fichero,
2. baseline GREEN del blanco,
3. copia de seguridad, parche, rebuild bajo `flock`,
4. el blanco se pone **RED**,
5. `git checkout --` y verificación de sha256 + `git status` limpio,
6. rebuild y el blanco vuelve a **GREEN** con el mismo digest.

Si en cualquier paso el blanco no se mueve, el gate falla su propia falsificación.
La evidencia cruda de la corrida vive en
`docs/sintergic/validation/m43-falsifier-evidence-` + `""" + CR + """` + `.md`.

## Frontera

- Escrituras de esta lane: `scripts/m43_gate_lib.py`, `scripts/sys-*-001.py`,
  `scripts/m43-gate-contract.py`, este documento, el documento de falsificación,
  la HH-1 y **una** fila propia en `docs/sintergic/handoffs/INDEX.jsonl`.
- No tocados: `crates/**` (salvo la mutación transitoria de falsificación, restaurada
  y verificada), `crates/promotion-gate/**`, `crates/side-contract/**`,
  `docs/sintergic/promotion-gate/promotion-ledger.jsonl`, `daggerfall-side-projects/*`,
  `docs/sintergic/validation/closure-register-CR-03390267.md`, scripts preexistentes,
  packs y registries de lanes. 0 escritura fuera del target, 0 push, 0 tag.
"""


def render(substitutions: dict[tuple[str, str], object] | None = None) -> str:
    """Project every gate into Markdown.

    `substitutions` maps `(gate_id, depth_row_id)` to a stand-in `DepthRow` that
    is read **only** by the depth-table projection. `--selftest` uses it to prove
    the `probe` cell is falsifiable: if the renderer ignored `row.probe`, the
    substituted cell could not move. Nothing else may consult it, so one
    substitution changes exactly one cell and the rest of the document stays
    byte-identical.
    """
    substitutions = substitutions or {}
    parts = [HEADER]
    for name in GATES:
        gate = load(name)
        parts.append(f"## `{gate.gate_id}` — `scripts/{name}`\n\n")
        parts.append(f"- CR: `{gate.cr}`\n")
        parts.append(f"- Probes: {', '.join(f'`{probe}`' for probe in gate.probes)}\n")
        if gate.retail_probes:
            parts.append(f"- Retail (G12): {', '.join(f'`{p}`' for p in gate.retail_probes)} — NC si salen 2\n")
        parts.append(f"- Huella de fuentes esperada: {', '.join(f'`{src}`' for src in gate.sources_hint)}\n")
        parts.append(f"- Artefactos: `target/m43-gates/{gate.slug}/` (gitignored)\n\n")
        parts.append("### Filas sobre stdout actual\n\n")
        parts.append("| fila | probe | regex (anclada) | qué exige | cita |\n|---|---|---|---|---|\n")
        for row in gate.rows:
            parts.append(
                f"| `{row.id}` | `{row.probe or gate.probes[0]}` | `{cell(row.pattern)}` | "
                f"{cell(spec_name(row.check))} · `{cell(row.pattern)}` | `{cell(row.cite or '—')}` |\n"
            )
        parts.append("\n### Filas de profundidad (§3.8) — línea que el probe debe imprimir\n\n")
        parts.append("| fila | probe | regex que la reconoce | contrato de contenido |\n|---|---|---|---|\n")
        for declared in gate.depth:
            row = substitutions.get((gate.gate_id, declared.id), declared)
            parts.append(
                f"| `{row.id}` | `{row.probe or gate.probes[0]}` | `{cell(row.pattern)}` | "
                f"{cell(row.description)} |\n"
            )
        parts.append("\n### Falsificadores\n\n")
        parts.append("| nombre | fichero | antes | después | fila que debe caer |\n|---|---|---|---|---|\n")
        for spec in gate.falsifiers.values():
            parts.append(
                f"| `{spec.name}` | `{spec.path}` | `{cell(spec.before)}` | `{cell(spec.after)}` | "
                f"`{spec.target}` |\n"
            )
        parts.append("\n---\n\n")
    parts.append(FOOTER)
    return "".join(parts)


SELFTEST_SUFFIX = "-SELFTEST"
CELL_SPLIT = re.compile(r"(?<!\\)\|")


def selftest() -> int:
    """Prove the projected `probe` cell is FALSIFIABLE, not merely present.

    For every depth row of every gate: re-render with
    `dataclasses.replace(row, probe="<distinct>")` and demand

    (a) the mutated line carries the mutated probe in its `probe` cell,
    (b) every other line of the document is byte-identical (same count, same bytes),
    (c) the mutated line DIFFERS from the original — a renderer that dropped
        `row.probe` from the projection moves zero lines and fails here.
    """
    base_lines = render().splitlines(keepends=True)
    failures: list[str] = []
    checked = 0
    total = 0
    for name in GATES:
        gate = load(name)
        for row in gate.depth:
            total += 1
            label = f"{gate.gate_id}/{row.id}"
            resolved = row.probe or gate.probes[0]
            want_after = f"`{resolved}{SELFTEST_SUFFIX}`"
            marker = f"| `{row.id}` |"
            hits = [index for index, line in enumerate(base_lines) if line.startswith(marker)]
            if len(hits) != 1:
                failures.append(f"{label}: rendered {len(hits)} depth lines (want 1)")
                continue
            index = hits[0]
            mutated_lines = render(
                {(gate.gate_id, row.id): replace(row, probe=f"{resolved}{SELFTEST_SUFFIX}")}
            ).splitlines(keepends=True)
            if len(mutated_lines) != len(base_lines):
                failures.append(f"{label}: line count {len(base_lines)} -> {len(mutated_lines)}")
                continue
            changed = [i for i, pair in enumerate(zip(base_lines, mutated_lines)) if pair[0] != pair[1]]
            before, after = base_lines[index], mutated_lines[index]
            cells_before, cells_after = CELL_SPLIT.split(before), CELL_SPLIT.split(after)
            if not changed:
                failures.append(f"{label}: probe cell did not move (renderer ignores row.probe)")
            elif len(changed) != 1 or changed[0] != index:
                failures.append(
                    f"{label}: {len(changed)} lines moved (want exactly the row's own projection line)"
                )
            elif len(cells_before) != len(cells_after):
                failures.append(f"{label}: column count changed on the mutated line")
            elif cells_before[2].strip() != f"`{resolved}`":
                failures.append(f"{label}: projected cell {cells_before[2].strip()!r} != `{resolved}`")
            elif cells_after[2].strip() != want_after:
                failures.append(f"{label}: mutated cell {cells_after[2].strip()!r} != {want_after}")
            elif cells_before[:2] + cells_before[3:] != cells_after[:2] + cells_after[3:]:
                failures.append(f"{label}: substitution changed more than the probe cell")
            else:
                checked += 1
    if failures:
        for failure in failures:
            print(f"SELFTEST FAIL - {failure}")
        return 1
    print(
        f"SELFTEST PASS - {checked}/{total} depth rows across {len(GATES)} gates: "
        "the mutated probe is what the cell shows, the rest of the document is byte-identical, "
        "and every mutated line differs from its original"
    )
    return 0


def main(argv: list[str]) -> int:
    if "--selftest" in argv:
        return selftest()
    text = render()
    if "--write" in argv:
        DOC.parent.mkdir(parents=True, exist_ok=True)
        DOC.write_text(text, encoding="utf-8")
        print(f"wrote {DOC.relative_to(ROOT)} lines={text.count(chr(10))}")
        return 0
    sys.stdout.write(text)
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
