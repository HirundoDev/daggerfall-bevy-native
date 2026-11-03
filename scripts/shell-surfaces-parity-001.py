#!/usr/bin/env python3
"""SHELL-SURFACES-PARITY-001 — gate de superficies de M-SHELL (16 retail + 1 divergencia = 17).

Porqué existe
-------------
`SH2-09` (`scripts/shell-002.py`) y `UI-CREATE-001` (`scripts/ui-create-001.py`)
miden **16** superficies y deben seguir midiendo 16: son las 16 páginas del
wizard **retail** (`StartNewGameWizard`, C01..C16 de
`crates/app/src/m38_surfaces.rs`). El owner decidió (2026-10-30) **añadir** una
17.ª superficie que **no es retail**: la provincia. Retirarla es revertir una
decisión del owner; contarla dentro de las 16 es romper la paridad con las 27
superficies validadas. La resolución no es mover ningún número existente, es
**separar las dos clases** en un gate propio:

    retail      = 16   paridad con el censo retail (C01..C16) — lo que miden SH2-09 y UI-CREATE-001
    divergencia =  1   superficie nueva con atribución + fecha (provincia, 0,60 u, 2026-10-30)
    total       = 17   lo que el contrato del ciclo declara

Este gate NO ablanda los dos anteriores: los dos siguen midiendo 16 y este mide
la 17.ª como una fila aparte, con su dueño y su fecha. ROJO si:

  * la divergencia aparece sin atribución, sin fecha ISO o sin ancla de decisión;
  * desaparece una superficie retail (el censo y lo declarado dejan de cuadrar);
  * el total no es 17 (ni por defecto ni por exceso: la provincia no puede
    colarse dentro del conjunto retail para "llegar" a 17).

Fuentes (los números no se escriben aquí: se leen)
--------------------------------------------------
  censo        → `crates/app/src/m38_surfaces.rs`  (`CREATE_SURFACE_COUNT` + `create_surfaces()`)
  declaración  → `docs/sintergic/playability/SHELL-SURFACES-DIVERGENCES.json`
  decisión     → `docs/sintergic/playability/SHELL-VISIBLE-LAYER.md:376-377` y `:410`
                 (cada divergencia declara sus anclas y las cadenas que deben seguir ahí)

Determinismo
------------
Dos procesos sobre el mismo árbol imprimen bytes idénticos (`cmp`): sin
timestamps, sin `pid`, sin rutas temporales y **sin `head=<commit>`** — el gate
mide documentos y censo, no el árbol git, así que su `DIGEST` no cambia al mover
HEAD. `DIGEST <sha256>` cubre todo lo impreso antes de esa línea.

Salida (canon de gates del repo)
--------------------------------
    GATE <id> cr=<CR> mode=run
    SOURCES ...
    PARITY retail=16 divergence=1 total=17 expected-total=17
    OK <check> <detalle> src=<ancla>      |  [FAIL] / [MISSING]
    CHECKS total=<n> green=<n> red=<n> missing=<n> failed=... missing-ids=...
    VERDICT: PASS|FAIL
    DIGEST <sha256>

Uso
---
    python3 scripts/shell-surfaces-parity-001.py [--out DIR]
    python3 scripts/shell-surfaces-parity-001.py --self-test

Exit: 0 PASS · 1 FAIL · 2 uso/IO.
"""
from __future__ import annotations

import argparse
import copy
import hashlib
import json
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
GATE = "SHELL-SURFACES-PARITY-001"
CR = "CR-e4b04fd9-7a3e-4136-aeeb-8b8448387396"

CENSUS = ROOT / "crates/app/src/m38_surfaces.rs"
DECLARATION = ROOT / "docs/sintergic/playability/SHELL-SURFACES-DIVERGENCES.json"
DECISION = ROOT / "docs/sintergic/playability/SHELL-VISIBLE-LAYER.md"

RETAIL_EXPECTED = 16
DIVERGENCE_EXPECTED = 1
TOTAL_EXPECTED = RETAIL_EXPECTED + DIVERGENCE_EXPECTED

RE_CENSUS_COUNT = re.compile(r"pub const CREATE_SURFACE_COUNT: usize = (\d+);")
RE_CENSUS_PUSH = re.compile(r'out\.push\(surface\(\s*"([A-Z]\d{2})",\s*"([A-Za-z0-9_]+)"')
RE_RETAIL_ID = re.compile(r"^C\d{2}$")
RE_ISO_DATE = re.compile(r"^\d{4}-\d{2}-\d{2}$")
RE_ANCHOR_REF = re.compile(r"^(?P<path>[^\s:]+\.(?:md|json)):(?P<start>\d+)(?:-(?P<end>\d+))?$")

CHECK_IDS = (
    "census-count",
    "census-ids",
    "census-names",
    "retail-declared",
    "divergence-attribution",
    "divergence-not-retail",
    "decision-anchor",
    "retail-judges",
    "total-17",
)


class CensusError(Exception):
    """El censo no se puede leer como censo (fuente ilegible o incompleta)."""


# ── censo ─────────────────────────────────────────────────────────────────
def parse_census(text: str) -> tuple[int, list[tuple[str, str]]]:
    """(CREATE_SURFACE_COUNT, [(id, name), ...]) leídos de `m38_surfaces.rs`."""
    match = RE_CENSUS_COUNT.search(text)
    if not match:
        raise CensusError("CREATE_SURFACE_COUNT no declarada")
    count = int(match.group(1))
    start = text.find("pub fn create_surfaces(")
    if start < 0:
        raise CensusError("create_surfaces() ausente")
    end = text.find("\n}\n", start)
    body = text[start:] if end < 0 else text[start:end]
    return count, RE_CENSUS_PUSH.findall(body)


# ── declaración ───────────────────────────────────────────────────────────
def normalise(text: str) -> str:
    return re.sub(r"\s+", " ", text).strip()


def load_declaration(path: Path) -> dict:
    return json.loads(path.read_text(encoding="utf-8"))


def declared_retail(declaration: dict) -> list[tuple[str, str]]:
    rows = declaration.get("retail")
    if not isinstance(rows, list):
        return []
    out = []
    for row in rows:
        if isinstance(row, dict):
            out.append((str(row.get("id", "")), str(row.get("name", ""))))
    return out


def declared_divergence(declaration: dict) -> list[dict]:
    rows = declaration.get("divergence")
    if not isinstance(rows, list):
        return []
    return [row for row in rows if isinstance(row, dict)]


def anchor_text(ref: str) -> tuple[str, str | None]:
    """(`texto de las líneas citadas`, None) o ("", motivo) si el ancla no resuelve."""
    match = RE_ANCHOR_REF.match(ref.strip())
    if not match:
        return "", f"ancla no parseable {ref!r}"
    path = ROOT / match.group("path")
    if not path.is_file():
        return "", f"ancla a file inexistente {match.group('path')}"
    lines = path.read_text(encoding="utf-8").splitlines()
    start = int(match.group("start"))
    end = int(match.group("end") or start)
    if start < 1 or end < start or end > len(lines):
        return "", f"ancla fuera de rango {ref} (file={len(lines)} líneas)"
    return "\n".join(lines[start - 1:end]), None


# ── evaluación ────────────────────────────────────────────────────────────
def evaluate(census_text: str, declaration: dict) -> list[tuple[str, bool, str, bool, str]]:
    """[(id, ok, detalle, missing, ancla)] en orden estable — nunca lanza."""
    rows: list[tuple[str, bool, str, bool, str]] = []
    census_src = "crates/app/src/m38_surfaces.rs"
    decl_src = "docs/sintergic/playability/SHELL-SURFACES-DIVERGENCES.json"

    # 1-3 · censo retail (fuente)
    count: int | None = None
    pairs: list[tuple[str, str]] = []
    try:
        count, pairs = parse_census(census_text)
        census_error = ""
    except CensusError as err:
        census_error = str(err)

    if census_error:
        missing = "censo ilegible" in census_error or "ausente" in census_error
        for cid in ("census-count", "census-ids", "census-names"):
            rows.append((cid, False, f"censo no medible: {census_error}", missing, census_src))
    else:
        ok_count = count == RETAIL_EXPECTED and len(pairs) == count
        rows.append((
            "census-count",
            ok_count,
            f"declared={count} parsed={len(pairs)} expected={RETAIL_EXPECTED}",
            False,
            f"{census_src}:CREATE_SURFACE_COUNT",
        ))
        ids = [pid for pid, _ in pairs]
        expected_ids = [f"C{n:02d}" for n in range(1, RETAIL_EXPECTED + 1)]
        rows.append((
            "census-ids",
            ids == expected_ids,
            f"ids={','.join(ids) or 'none'} expected=C01..C{RETAIL_EXPECTED:02d}",
            False,
            f"{census_src}:create_surfaces",
        ))
        names = [name for _, name in pairs]
        ok_names = bool(names) and all(names) and len(set(names)) == len(names)
        rows.append((
            "census-names",
            ok_names,
            f"names={len(names)} unique={len(set(names))}",
            False,
            f"{census_src}:create_surfaces",
        ))

    # 4 · lo declarado como retail debe ser EXACTAMENTE el censo
    retail = declared_retail(declaration)
    if census_error:
        rows.append(("retail-declared", False, "sin censo contra el que medir", False, decl_src))
    else:
        ok_retail = retail == pairs and len(retail) == RETAIL_EXPECTED
        missing_ids = [pid for pid in [p for p, _ in pairs] if pid not in [r for r, _ in retail]]
        extra_ids = [rid for rid, _ in retail if rid not in [p for p, _ in pairs]]
        detail = f"declared={len(retail)} census={len(pairs)}"
        if missing_ids:
            detail += f" retail-ausente={','.join(missing_ids)}"
        if extra_ids:
            detail += f" retail-no-censo={','.join(extra_ids)}"
        rows.append(("retail-declared", ok_retail, detail, False, decl_src))

    # 5 · divergencia: 1, con atribución, fecha ISO, unidades y ancla
    divergence = declared_divergence(declaration)
    problems: list[str] = []
    if len(divergence) != DIVERGENCE_EXPECTED:
        problems.append(f"count={len(divergence)} expected={DIVERGENCE_EXPECTED}")
    for row in divergence:
        did = str(row.get("id", "")) or "?"
        attribution = row.get("attribution")
        if not isinstance(attribution, str) or not attribution.strip():
            problems.append(f"{did}:attribution-ausente")
        date = row.get("decision_date")
        if not isinstance(date, str) or not RE_ISO_DATE.match(date):
            problems.append(f"{did}:decision_date-no-ISO({date!r})")
        units = row.get("units")
        if not isinstance(units, (int, float)) or isinstance(units, bool) or units <= 0:
            problems.append(f"{did}:units-invalidas({units!r})")
        refs = row.get("anchors")
        if not isinstance(refs, list) or not refs:
            problems.append(f"{did}:anchors-ausentes")
        if row.get("retail_parity") is not False:
            problems.append(f"{did}:retail_parity-debe-ser-false")
    attributed = ", ".join(
        f"{row.get('id', '?')}={row.get('attribution', '?')}@{row.get('decision_date', '?')}"
        for row in divergence
    )
    rows.append((
        "divergence-attribution",
        not problems,
        f"count={len(divergence)} [{attributed or 'none'}] " + (" ".join(problems) or "attributed"),
        False,
        decl_src,
    ))

    # 6 · la divergencia NO puede ser retail (ni por id ni por estar en el censo)
    retail_ids = {rid for rid, _ in retail} | {pid for pid, _ in pairs}
    smuggled = [str(row.get("id", "")) for row in divergence if str(row.get("id", "")) in retail_ids]
    rows.append((
        "divergence-not-retail",
        bool(divergence) and not smuggled,
        f"ids={','.join(str(r.get('id', '?')) for r in divergence) or 'none'} "
        f"dentro-del-retail={','.join(smuggled) or 'none'}",
        False,
        decl_src,
    ))

    # 7 · anclas de la decisión: las líneas citadas siguen diciendo lo citado
    anchor_problems: list[str] = []
    checked = 0
    for row in divergence:
        date = row.get("decision_date")
        for anchor in row.get("anchors") if isinstance(row.get("anchors"), list) else []:
            if not isinstance(anchor, dict):
                anchor_problems.append("ancla-no-objeto")
                continue
            ref = str(anchor.get("ref", ""))
            need = anchor.get("must_contain")
            need = [str(token) for token in need] if isinstance(need, list) else []
            if not need:
                anchor_problems.append(f"{ref}:must_contain-vacio")
                continue
            text, reason = anchor_text(ref)
            if reason:
                anchor_problems.append(reason)
                continue
            checked += 1
            flat = normalise(text)
            for token in need:
                if normalise(token) not in flat:
                    anchor_problems.append(f"{ref}:sin-{token!r}")
        if isinstance(date, str) and isinstance(row.get("decision_ref"), str):
            text, reason = anchor_text(str(row["decision_ref"]))
            if reason:
                anchor_problems.append(f"decision_ref:{reason}")
            elif date not in normalise(text):
                anchor_problems.append(f"decision_ref:sin-fecha-{date}")
    rows.append((
        "decision-anchor",
        bool(divergence) and bool(checked) and not anchor_problems,
        f"anclas={checked} " + (" ".join(anchor_problems) or "intactas"),
        False,
        "docs/sintergic/playability/SHELL-VISIBLE-LAYER.md:376-377,410",
    ))

    # 8 · los dos jueces del retail siguen midiendo 16 (no se ablandaron al añadir la 17.ª)
    judges = declaration.get("retail_judges")
    judges = judges if isinstance(judges, list) else []
    judge_problems: list[str] = []
    anchors = 0
    for judge in judges:
        if not isinstance(judge, dict):
            judge_problems.append("juez-no-objeto")
            continue
        gate_path = str(judge.get("gate", ""))
        need = judge.get("must_contain")
        need = [str(token) for token in need] if isinstance(need, list) else []
        if not gate_path or not need:
            judge_problems.append(f"{gate_path or '?'}:declaracion-vacia")
            continue
        target = ROOT / gate_path
        if not target.is_file():
            judge_problems.append(f"{gate_path}:ausente")
            continue
        text = target.read_text(encoding="utf-8")
        for token in need:
            anchors += 1
            if token not in text:
                judge_problems.append(f"{gate_path}:sin-{token!r}")
    rows.append((
        "retail-judges",
        bool(judges) and not judge_problems,
        f"judges={len(judges)} anchors={anchors} "
        + (" ".join(judge_problems) or "retail-16-intacto"),
        False,
        "scripts/shell-002.py:SH2-09,SH2-11 scripts/ui-create-001.py:SURFACES",
    ))

    # 8 · total = 16 + 1 = 17 (ni por defecto ni por exceso)
    total = len(retail) + len(divergence)
    declared_total = declaration.get("total_expected")
    ok_total = (
        total == TOTAL_EXPECTED
        and declared_total == TOTAL_EXPECTED
        and len(retail) == RETAIL_EXPECTED
        and len(divergence) == DIVERGENCE_EXPECTED
    )
    rows.append((
        "total-17",
        ok_total,
        f"retail={len(retail)} divergence={len(divergence)} total={total} "
        f"declared-total={declared_total!r} expected={TOTAL_EXPECTED}",
        False,
        decl_src,
    ))
    return rows


def render(rows: list[tuple[str, bool, str, bool, str]]) -> list[str]:
    lines: list[str] = []
    for cid, ok, detail, missing, src in rows:
        if missing:
            lines.append(f"[MISSING] {cid} - {detail}")
        elif ok:
            lines.append(f"OK {cid} {detail} src={src}")
        else:
            lines.append(f"[FAIL] {cid} {detail} src={src}")
    failed = [cid for cid, ok, _, missing, _ in rows if not ok and not missing]
    missing_ids = [cid for cid, ok, _, missing, _ in rows if not ok and missing]
    green = sum(1 for _, ok, _, _, _ in rows if ok)
    lines.append(
        f"CHECKS total={len(rows)} green={green} red={len(failed)} missing={len(missing_ids)} "
        f"failed={','.join(failed[:8]) or 'none'} missing-ids={','.join(missing_ids[:8]) or 'none'}"
    )
    lines.append(f"VERDICT: {'PASS' if not failed and not missing_ids else 'FAIL'}")
    return lines


def emit(lines: list[str], out: Path | None) -> None:
    text = "\n".join(lines) + "\n"
    digest = hashlib.sha256(text.encode("utf-8")).hexdigest()
    sealed = text + f"DIGEST {digest}\n"
    if out is not None:
        out.mkdir(parents=True, exist_ok=True)
        (out / "stdout.txt").write_text(sealed, encoding="utf-8")
    sys.stdout.write(sealed)


# ── modos ─────────────────────────────────────────────────────────────────
def header() -> list[str]:
    return [
        f"GATE {GATE} cr={CR} mode=run",
        "SOURCES census=crates/app/src/m38_surfaces.rs "
        "declaration=docs/sintergic/playability/SHELL-SURFACES-DIVERGENCES.json "
        "decision=docs/sintergic/playability/SHELL-VISIBLE-LAYER.md",
        f"PARITY retail={RETAIL_EXPECTED} divergence={DIVERGENCE_EXPECTED} "
        f"total={TOTAL_EXPECTED} expected-total={TOTAL_EXPECTED}",
    ]


def mode_run(out: Path | None) -> int:
    lines = header()
    problems: list[str] = []
    census_text = ""
    if CENSUS.is_file():
        census_text = CENSUS.read_text(encoding="utf-8")
    else:
        problems.append("censo ausente crates/app/src/m38_surfaces.rs")
    declaration: dict = {}
    if DECLARATION.is_file():
        try:
            declaration = load_declaration(DECLARATION)
        except (OSError, json.JSONDecodeError) as err:
            problems.append(f"declaración ilegible ({type(err).__name__})")
    else:
        problems.append("declaración ausente docs/sintergic/playability/SHELL-SURFACES-DIVERGENCES.json")
    if not DECISION.is_file():
        problems.append("decisión ausente docs/sintergic/playability/SHELL-VISIBLE-LAYER.md")

    if problems:
        for problem in problems:
            lines.append(f"[MISSING] sources - {problem}")
        lines.append(f"CHECKS total={len(problems)} green=0 red=0 missing={len(problems)} "
                     f"failed=none missing-ids=sources")
        lines.append("VERDICT: FAIL")
        emit(lines, out)
        return 1

    lines.extend(render(evaluate(census_text, declaration)))
    emit(lines, out)
    return 0 if lines[-1] == "VERDICT: PASS" else 1


def census_fixture(pairs: list[tuple[str, str]], count: int | None = None) -> str:
    body = ["pub const CREATE_SURFACE_COUNT: usize = %d;" % (len(pairs) if count is None else count), "",
            "pub fn create_surfaces() -> Result<Vec<SurfaceWindow>, UiCompatError> {",
            "    let mut out = Vec::new();"]
    for pid, name in pairs:
        body.append(f'    out.push(surface(\n        "{pid}",\n        "{name}",')
    body.append("    Ok(out)")
    body.append("}")
    body.append("")
    return "\n".join(body)


def verdict_of(rows: list[tuple[str, bool, str, bool, str]]) -> str:
    return render(rows)[-1].split(": ", 1)[1]


def red_of(rows: list[tuple[str, bool, str, bool, str]]) -> list[str]:
    return [cid for cid, ok, _, missing, _ in rows if not ok]


def mode_self_test() -> int:
    """1 control positivo (la declaración viva) + 4 negativos, cada uno ROJO por su motivo."""
    lines = [f"gate={GATE} cr={CR} mode=self-test"]
    census_text = CENSUS.read_text(encoding="utf-8") if CENSUS.is_file() else ""
    declaration = load_declaration(DECLARATION) if DECLARATION.is_file() else {}
    count, pairs = parse_census(census_text) if census_text else (0, [])
    lines.append("FIXTURE census=%d census-count=%s declared-retail=%d divergence=%d"
                 % (len(pairs), count, len(declared_retail(declaration)), len(declared_divergence(declaration))))

    controls: list[tuple[str, list[tuple[str, bool, str, bool, str]], str, tuple[str, ...]]] = []

    # positivo · la declaración viva contra el censo vivo: debe quedar VERDE
    controls.append(("positive-live", evaluate(census_text, declaration), "PASS", ()))

    # negativo 1 · divergencia sin atribución
    broken = copy.deepcopy(declaration)
    if broken.get("divergence"):
        broken["divergence"][0].pop("attribution", None)
    controls.append(("negative-missing-attribution", evaluate(census_text, broken), "FAIL",
                     ("divergence-attribution",)))

    # negativo 2 · desaparece una superficie retail del censo
    short = [pair for pair in pairs if pair[0] != "C15"]
    controls.append(("negative-missing-retail", evaluate(census_fixture(short), declaration), "FAIL",
                     ("census-count", "retail-declared")))

    # negativo 3 · total != 17 (dos divergencias atribuidas => 18)
    doubled = copy.deepcopy(declaration)
    if doubled.get("divergence"):
        extra = copy.deepcopy(doubled["divergence"][0])
        extra["id"] = str(extra.get("id", "DIVERGENCIA")) + "-2"
        doubled["divergence"] = list(doubled["divergence"]) + [extra]
    controls.append(("negative-total-not-17", evaluate(census_text, doubled), "FAIL", ("total-17",)))

    # negativo 4 · la provincia se cuela DENTRO del conjunto retail para "llegar" a 17
    smuggled = copy.deepcopy(declaration)
    if smuggled.get("divergence") and isinstance(smuggled.get("retail"), list):
        province = smuggled["divergence"][0]
        smuggled["retail"] = list(smuggled["retail"]) + [
            {"id": str(province.get("id", "PROVINCIA")), "name": "CreateCharProvince"}
        ]
    controls.append(("negative-retail-smuggling", evaluate(census_text, smuggled), "FAIL",
                     ("retail-declared", "divergence-not-retail")))

    caught = 0
    for name, rows, expected, must_red in controls:
        verdict = verdict_of(rows)
        red = red_of(rows)
        ok = verdict == expected and all(cid in red for cid in must_red)
        caught += 1 if ok else 0
        lines.append("[%s] CONTROL %s expected=%s verdict=%s red=%s"
                     % ("OK" if ok else "FAIL", name, expected, verdict, ",".join(red) or "none"))

    total_negative = len(controls) - 1
    caught_negative = sum(
        1 for name, rows, expected, must_red in controls[1:]
        if verdict_of(rows) == expected and all(cid in red_of(rows) for cid in must_red)
    )
    lines.append("SELF-TEST controls=%d negative=%d caught=%d/%d"
                 % (len(controls), total_negative, caught_negative, total_negative))
    ok_all = caught == len(controls)
    lines.append(f"VERDICT: {'PASS' if ok_all else 'FAIL'}")
    emit(lines, None)
    return 0 if ok_all else 1


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=f"{GATE} — 16 retail + 1 divergencia = 17")
    parser.add_argument("--out", help="directorio donde espejar stdout.txt (opcional)")
    parser.add_argument("--self-test", action="store_true",
                        help="control positivo + 4 negativos (atribución, retail, total, contrabando)")
    args = parser.parse_args(argv)
    try:
        if args.self_test:
            return mode_self_test()
        return mode_run(Path(args.out).resolve() if args.out else None)
    except OSError as err:
        sys.stderr.write(f"IO: {err}\n")
        return 2


if __name__ == "__main__":
    sys.exit(main())
