#!/usr/bin/env python3
"""PROBE-UI-AUDIT-001 - gate of milestone M36 (UI-0), blocks A + B + C.

Read-only over the tree. No GPU, no `ARENA2`, no retail decode: every input is
source text already committed (`ui_census.rs`, `ui_playable_g*.rs`, `ui_spec.rs`,
`ui_shell.rs`, the W34 TSV) plus the two M36 documents.

What it gates
-------------
BLOCK 0  the M36..M48 + E8 transcript exists with 14 milestone rows verbatim.
BLOCK A  the 85-window inventory (UI-AUDIT-001 A): 85/85 rows, census order,
         canonical category, declared retail surface, implementation pointer and
         derived state.
BLOCK B  the patch/rewrite rule (UI-AUDIT-001 B): 85/85 verdicts, each one
         recomputed here from the inventory inputs by the declared rule.
BLOCK C  the compatibility contract (UI-AUDIT-001 C: 7 axes, each with what the
         M37 layer guarantees, what it does not cover, and the interface M38/M39
         consume) plus the E1 re-estimation arithmetic.

The rule is recomputed, never read back as an opinion: the document must agree
with the derivation or the gate fails. Two separate processes must print the
same bytes; `--emit` regenerates the marked table regions so the document can
never drift from the measurement.

Exit codes: 0 PASS, 1 FAIL (block reported), 2 usage/IO error.
"""

from __future__ import annotations

import argparse
import re
import sys
from dataclasses import dataclass
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent

CENSUS = "crates/perception/src/ui_census.rs"
UI_SPEC = "crates/parser-bevy-bridge/src/ui_spec.rs"
UI_SHELL = "crates/perception/src/ui_shell.rs"
PLAYABLE = [
    "crates/parser-bevy-bridge/src/ui_playable_g1.rs",
    "crates/parser-bevy-bridge/src/ui_playable_g2.rs",
    "crates/parser-bevy-bridge/src/ui_playable_g3.rs",
]
TSV = "docs/sintergic/l5-mission/m34-window-assets-CR-2d5c8f41.tsv"
CENSUS_DOC = "docs/sintergic/game/programming/w34-1-ui-census-CR-2d5c8f41.md"
M17_SPEC = "docs/sintergic/game/programming/m17-ui-1996-spec-CR-1a889186.md"

AUDIT_DOC = "docs/sintergic/l5-mission/m36-ui0-audit-CR-d5d17a3a-c7cb-45b5-b4c4-42d48e9ee56f.md"
ROADMAP_DOC = (
    "docs/sintergic/l5-mission/ROADMAP-M36-M48-CR-d5d17a3a-c7cb-45b5-b4c4-42d48e9ee56f.md"
)

CR = "CR-d5d17a3a-c7cb-45b5-b4c4-42d48e9ee56f"

#: Published digests (W34-1 census §3 and §4, GV-02 / GV-19). External oracles:
#: the probe must reproduce them from the current source, not trust the prose.
GV_02 = "1da6a516914e33db"
GV_19 = "c85e0bb5ae867373"

#: `WindowId` arm -> DFU window name, read from `ui_shell::canonical_id`. The
#: four contract panels are the only windows with a dedicated M17 builder.
CONTRACT_PANELS = {
    "DaggerfallHUD": "build_hud_panel",
    "DaggerfallInventoryWindow": "build_inventory_panel",
    "DaggerfallQuestJournalWindow": "build_quests_panel",
    "DaggerfallStartWindow": "build_main_menu_panel",
}

#: Retail screen labels measured in `m17-ui-1996-spec` §3 (UESP pairing,
#: [INFERENCE] by size/name per NEGATIVE-5). Everything else has no measured
#: on-screen title in the tree and stays `NO-VERIFICADO`.
RETAIL_TITLE = {
    "DaggerfallHUD": "HUD",
    "DaggerfallInventoryWindow": "Inventario",
    "DaggerfallQuestJournalWindow": "Diario / quests",
    "DaggerfallStartWindow": "Menú principal",
}

RUBRIC_IMPLEMENTADA = "IMPLEMENTADA"
RUBRIC_PARCIAL = "PARCIAL"
RUBRIC_AUSENTE = "AUSENTE"

VERDICT_PATCH = "PATCH"
VERDICT_REWRITE = "REWRITE"


class Fail(Exception):
    """A gate block did not hold."""


@dataclass(frozen=True)
class CensusRow:
    index: int
    name: str
    category: str
    line: int


@dataclass(frozen=True)
class Spec:
    name: str
    asset: str | None
    path: str
    line: int


@dataclass(frozen=True)
class AssetRow:
    group: str
    name: str
    asset: str
    kind: str
    decodes: str
    rect: tuple[int, int, int, int]
    anchor: str
    font: str


@dataclass(frozen=True)
class Builder:
    fn: str
    line: int
    nodes: int
    assets: tuple[str, ...]


@dataclass
class Window:
    index: int
    name: str
    category: str
    census_line: int
    spec: Spec | None
    asset: AssetRow | None
    builder: Builder | None
    estado: str
    verdict: str
    rule: str


# --------------------------------------------------------------------------- #
# derivation
# --------------------------------------------------------------------------- #
def fnv1a64(names: list[str]) -> str:
    """Digest used by W34-1: FNV-1a 64 over the ascending names joined by `\\n`."""
    digest = 0xCBF29CE484222325
    for byte in "\n".join(sorted(names)).encode("utf-8"):
        digest ^= byte
        digest = (digest * 0x100000001B3) & 0xFFFFFFFFFFFFFFFF
    return format(digest, "x")


def read_text(rel: str) -> str:
    path = ROOT / rel
    if not path.is_file():
        raise Fail(f"missing input: {rel}")
    return path.read_text(encoding="utf-8")


def parse_census() -> list[CensusRow]:
    rows: list[CensusRow] = []
    for number, line in enumerate(read_text(CENSUS).splitlines(), start=1):
        match = re.search(r'entry\(\s*"(\w+)",\s*WindowCategory::(\w+)', line)
        if match:
            rows.append(CensusRow(len(rows), match.group(1), match.group(2), number))
    return rows


def parse_specs() -> list[Spec]:
    specs: list[Spec] = []
    pattern = re.compile(
        r'WindowSpec\s*\{\s*id:\s*"(\w+)",\s*asset_retail:\s*'
        r'(?:Some\("([^"]+)"\)|None)',
        re.S,
    )
    for rel in PLAYABLE:
        for match in pattern.finditer(read_text(rel)):
            line = read_text(rel)[: match.start()].count("\n") + 1
            specs.append(Spec(match.group(1), match.group(2), rel, line))
    return specs


def parse_tsv() -> list[AssetRow]:
    lines = read_text(TSV).splitlines()
    if not lines:
        raise Fail(f"empty TSV: {TSV}")
    rows: list[AssetRow] = []
    for line in lines[1:]:
        if not line.strip():
            continue
        cells = line.split("\t")
        if len(cells) < 10:
            raise Fail(f"TSV row with {len(cells)} cells: {line[:60]}")
        rows.append(
            AssetRow(
                group=cells[0],
                name=cells[1],
                asset=cells[2],
                kind=cells[3],
                decodes=cells[4],
                rect=(
                    int(cells[5]),
                    int(cells[6]),
                    int(cells[7]),
                    int(cells[8]),
                ),
                anchor=cells[9],
                font=cells[10] if len(cells) > 10 else "",
            )
        )
    return rows


def parse_builders() -> dict[str, Builder]:
    text = read_text(UI_SPEC)
    lines = text.splitlines()
    # Every top-level `pub fn` bounds a body: a builder does NOT own the
    # literals of the function that follows it (ui_spec.rs has helpers such as
    # `build_all_panels`/`build_map_panels` between and after the four panels).
    boundaries = [
        number
        for number, line in enumerate(lines, start=1)
        if re.match(r"pub (?:const )?fn \w+", line)
    ]
    starts: list[tuple[str, int]] = []
    for number, line in enumerate(lines, start=1):
        match = re.match(r"pub fn (build_\w+_panel)\b", line)
        if match:
            starts.append((match.group(1), number))
    builders: dict[str, Builder] = {}
    for fn, number in starts:
        following = [line for line in boundaries if line > number]
        end = (following[0] - 1) if following else len(lines)
        body = "\n".join(lines[number - 1 : end])
        nodes = len(
            re.findall(r"\b(?:header_sprite|declared_sprite|glyph_node)\(", body)
        )
        assets = tuple(
            sorted(set(re.findall(r'"([A-Z0-9]+\.(?:IMG|RCI|CIF|FNT))"', body)))
        )
        builders[fn] = Builder(fn, number, nodes, assets)
    return builders


def derive(
    census: list[CensusRow],
    specs: list[Spec],
    tsv: list[AssetRow],
    builders: dict[str, Builder],
) -> list[Window]:
    spec_by_name = {spec.name: spec for spec in specs}
    asset_by_name = {row.name: row for row in tsv}
    windows: list[Window] = []
    for row in census:
        spec = spec_by_name.get(row.name)
        asset = asset_by_name.get(row.name)
        builder = None
        if row.name in CONTRACT_PANELS:
            builder = builders.get(CONTRACT_PANELS[row.name])

        if spec is None:
            estado = RUBRIC_AUSENTE
        elif builder is not None and builder.nodes > 2:
            estado = RUBRIC_IMPLEMENTADA
        else:
            estado = RUBRIC_PARCIAL

        verdict, rule = decide(estado, asset, builder, asset_by_name, tsv)
        windows.append(
            Window(
                index=row.index,
                name=row.name,
                category=row.category,
                census_line=row.line,
                spec=spec,
                asset=asset,
                builder=builder,
                estado=estado,
                verdict=verdict,
                rule=rule,
            )
        )
    return windows


def anchor_is_invalid(asset: AssetRow | None) -> str | None:
    """P2 - the declared anchor cannot carry a window composition."""
    if asset is None:
        return None
    if asset.kind == "FNT":
        return "P2a-fuente-no-es-superficie"
    if asset.kind == "none":
        return "P2b-sin-raster"
    if asset.kind == "RCI" and asset.rect[2:] == (22, 22):
        return "P2c-registro-contenedor-22x22"
    return None


def decide(
    estado: str,
    asset: AssetRow | None,
    builder: Builder | None,
    asset_by_name: dict[str, AssetRow],
    tsv: list[AssetRow],
) -> tuple[str, str]:
    """UI-AUDIT-001 B rule. Ordered, first match wins, no free parameters.

    P1  no declared artefact            -> REWRITE  (nothing to extend)
    P2  invalid surface anchor          -> REWRITE  (the patch would land on a
                                       node that does not describe a window)
    P3  builder disagrees with the census surface:
          P3a it composes the declared asset of another window (a nested
              sub-surface)             -> PATCH    (add the container node)
          P3b it composes another full screen -> REWRITE (re-declare)
        otherwise                       -> PATCH    (extend in place)
    """
    if estado == RUBRIC_AUSENTE:
        return VERDICT_REWRITE, "P1-sin-artefacto"

    invalid = anchor_is_invalid(asset)
    if invalid is not None:
        return VERDICT_REWRITE, invalid

    if builder is not None and asset is not None:
        declared = {asset.asset}
        composed = set(builder.assets)
        if composed and not declared <= composed and asset.asset not in composed:
            foreign = {
                row.asset
                for row in tsv
                if row.name != asset.name and row.asset in composed
            }
            if foreign:
                return VERDICT_PATCH, "P3a-contenedor-anidado"
            if any(row.rect[2:] == (320, 200) for row in tsv if row.asset in composed):
                return VERDICT_REWRITE, "P3b-pantalla-ajena"
            return VERDICT_REWRITE, "P3b-superficie-sin-coincidencia"
    return VERDICT_PATCH, "P4-superficie-propia-extender"


# --------------------------------------------------------------------------- #
# document tables
# --------------------------------------------------------------------------- #
MARK_A = ("<!-- UI-AUDIT-A:BEGIN -->", "<!-- UI-AUDIT-A:END -->")
MARK_B = ("<!-- UI-AUDIT-B:BEGIN -->", "<!-- UI-AUDIT-B:END -->")

HEAD_A = (
    "| # | id | título retail | categoría | superficie retail | implementación "
    "| estado | evidencia |"
)
SEP_A = "|---:|:---|:---|:---|:---|---:|:---|:---|"
HEAD_B = "| # | id | P1 spec | P2 ancla | nodos | P3 coincidencia | veredicto |"
SEP_B = "|---:|:---|:---:|:---:|---:|:---|:---|"


def render_a(windows: list[Window]) -> list[str]:
    out = [HEAD_A, SEP_A]
    for window in windows:
        title = RETAIL_TITLE.get(window.name, "NO-VERIFICADO")
        surface = window.asset.asset if window.asset else "—"
        if window.estado == RUBRIC_IMPLEMENTADA and window.builder is not None:
            impl = f"`{Path(UI_SPEC).name}:{window.builder.line}`"
        elif window.spec is not None:
            impl = f"`{Path(window.spec.path).name}:{window.spec.line}`"
        else:
            impl = "AUSENTE"
        evidence = {
            RUBRIC_IMPLEMENTADA: "EV-A,EV-B,EV-C",
            RUBRIC_PARCIAL: "EV-A,EV-B",
            RUBRIC_AUSENTE: "EV-A",
        }[window.estado]
        out.append(
            f"| {window.index + 1:03d} | `{window.name}` | {title} | "
            f"{window.category} | {surface} | {impl} | {window.estado} | {evidence} |"
        )
    return out


def render_b(windows: list[Window]) -> list[str]:
    out = [HEAD_B, SEP_B]
    for window in windows:
        p1 = "sí" if window.spec is not None else "no"
        if window.asset is None:
            p2 = "—"
        elif window.asset.kind == "RCI" and window.asset.rect[2:] == (22, 22):
            p2 = "RCI 22×22"
        else:
            p2 = window.asset.kind
        nodes = str(window.builder.nodes) if window.builder is not None else "—"
        if window.verdict == VERDICT_REWRITE:
            p3 = "—"
        elif window.rule == "P3a-contenedor-anidado":
            p3 = "contenedor"
        elif window.builder is not None:
            p3 = "coincide"
        else:
            p3 = "—"
        out.append(
            f"| {window.index + 1:03d} | `{window.name}` | {p1} | {p2} | {nodes} | "
            f"{p3} | {window.verdict} |"
        )
    return out


def splice(text: str, markers: tuple[str, str], rows: list[str], label: str) -> str:
    begin, end = markers
    if begin not in text or end not in text:
        raise Fail(f"document lacks the {label} markers")
    head, rest = text.split(begin, 1)
    _, tail = rest.split(end, 1)
    return f"{head}{begin}\n" + "\n".join(rows) + f"\n{end}{tail}"


def table_rows(text: str, markers: tuple[str, str], label: str) -> list[list[str]]:
    begin, end = markers
    if begin not in text or end not in text:
        raise Fail(f"document lacks the {label} markers")
    body = text.split(begin, 1)[1].split(end, 1)[0]
    rows: list[list[str]] = []
    for line in body.splitlines():
        line = line.strip()
        if not line.startswith("|") or set(line) <= set("|-: "):
            continue
        cells = [cell.strip() for cell in line.strip("|").split("|")]
        if cells[0] in ("#", "---") or not cells[0]:
            continue
        rows.append(cells)
    return rows


# --------------------------------------------------------------------------- #
# blocks
# --------------------------------------------------------------------------- #
REPORT: list[str] = []


def say(line: str = "") -> None:
    REPORT.append(line)


def block0(roadmap: str) -> None:
    say("BLOCK 0 - roadmap M36..M48 + E8 transcription")
    rows: list[tuple[str, list[str]]] = []
    for line in roadmap.splitlines():
        stripped = line.strip()
        if not stripped.startswith("|"):
            continue
        cells = [cell.strip() for cell in stripped.strip("|").split("|")]
        if len(cells) != 6:
            continue
        if re.fullmatch(r"M\d{2}", cells[0]) or cells[0] == "E8":
            rows.append((cells[0], cells))
    milestones = [cells[0] for _, cells in rows]
    expect = [f"M{number}" for number in range(36, 49)] + ["E8"]
    if milestones != expect:
        raise Fail(f"roadmap milestones {milestones} != {expect}")
    say(f"  milestone rows = {len(rows)} (13 M rows + E8)")
    for _, cells in rows:
        _, epic, scope, state, u, gate = cells
        if epic == "" or scope == "" or gate == "":
            raise Fail(f"{cells[0]}: empty epic/scope/gate cell")
        if cells[0] == "E8":
            if "no estimable" not in state:
                raise Fail("E8 must be declared not estimable")
        else:
            if not re.fullmatch(r"\d+\.\d{2} \(\d+\.\d{2}–\d+\.\d{2}\)", u):
                raise Fail(f"{cells[0]}: u nominal not in 'N.NN (N.NN–N.NN)' form: {u}")
            if "NOT STARTED" not in state:
                raise Fail(f"{cells[0]}: state is not NOT STARTED: {state}")
    head = "  " + " · ".join(f"{cells[0]}={cells[4]}" for _, cells in rows[:5])
    say(head)


def block_a(
    windows: list[Window],
    census: list[CensusRow],
    specs: list[Spec],
    tsv: list[AssetRow],
    audit: str,
) -> None:
    say("BLOCK A - inventory 85/85 (UI-AUDIT-001 A)")
    categories = {}
    for row in census:
        categories[row.category] = categories.get(row.category, 0) + 1
    say(
        "  census = {} rows · CreateChar={} Config={} Infra={} Playable={}".format(
            len(census),
            categories.get("CreateChar", 0),
            categories.get("Config", 0),
            categories.get("Infra", 0),
            categories.get("Playable", 0),
        )
    )
    if len(census) != 85:
        raise Fail(f"census has {len(census)} rows, expected 85")
    if categories != {"CreateChar": 16, "Config": 11, "Infra": 7, "Playable": 51}:
        raise Fail(f"census partition is not 16+11+7+51: {categories}")

    digest02 = fnv1a64([row.name for row in census])
    digest19 = fnv1a64([row.name for row in tsv])
    say(f"  digest GV-02 (85 names) = {digest02} expect {GV_02}")
    say(f"  digest GV-19 (51 names) = {digest19} expect {GV_19}")
    if digest02 != GV_02:
        raise Fail("GV-02 digest does not reproduce")
    if digest19 != GV_19:
        raise Fail("GV-19 digest does not reproduce")

    say(f"  WindowSpec = {len(specs)} · TSV rows = {len(tsv)}")
    if len(specs) != 51 or len(tsv) != 51:
        raise Fail(f"spec/TSV count {len(specs)}/{len(tsv)} != 51/51")
    playable = {row.name for row in census if row.category == "Playable"}
    if not {spec.name for spec in specs} <= playable:
        raise Fail("a WindowSpec names a window outside the Playable census")
    if {spec.name for spec in specs} != {row.name for row in tsv}:
        raise Fail("WindowSpec set and TSV set disagree")
    duplicates = len(specs) - len({spec.name for spec in specs})
    if duplicates:
        raise Fail(f"{duplicates} duplicated WindowSpec ids")

    decodes = {}
    for row in tsv:
        decodes[row.decodes] = decodes.get(row.decodes, 0) + 1
    say(
        "  decodes = "
        + " ".join(f"{key}:{value}" for key, value in sorted(decodes.items()))
    )
    if decodes.get("no", 0) != 0:
        raise Fail("a TSV row declares decodes=no")

    states: dict[str, int] = {}
    for window in windows:
        states[window.estado] = states.get(window.estado, 0) + 1
    say(
        "  states = "
        + " · ".join(f"{key}={value}" for key, value in sorted(states.items()))
    )
    if states.get(RUBRIC_IMPLEMENTADA, 0) + states.get(RUBRIC_PARCIAL, 0) != 51:
        raise Fail(f"IMPLEMENTADA+PARCIAL != 51: {states}")
    if states.get(RUBRIC_AUSENTE, 0) != 34:
        raise Fail(f"AUSENTE != 34: {states}")

    rows = table_rows(audit, MARK_A, "A")
    say(f"  document rows = {len(rows)}")
    if len(rows) != 85:
        raise Fail(f"document has {len(rows)} inventory rows, expected 85")
    seen = set()
    for window, cells in zip(windows, rows):
        if len(cells) != 8:
            raise Fail(f"row {window.name}: {len(cells)} cells, expected 8")
        number, name, title, category, surface, impl, state, evidence = cells
        if int(number) != window.index + 1:
            raise Fail(f"row order out of sequence at {number}")
        if name != f"`{window.name}`":
            raise Fail(f"row {number}: id {name} != {window.name}")
        if name in seen:
            raise Fail(f"duplicated row {name}")
        seen.add(name)
        if category != window.category:
            raise Fail(f"{window.name}: category {category} != {window.category}")
        if state != window.estado:
            raise Fail(f"{window.name}: state {state} != {window.estado}")
        expected_surface = window.asset.asset if window.asset else "—"
        if surface != expected_surface:
            raise Fail(f"{window.name}: surface {surface} != {expected_surface}")
        if impl == "AUSENTE" and window.estado != RUBRIC_AUSENTE:
            raise Fail(f"{window.name}: AUSENTE pointer on a declared window")
        if impl != "AUSENTE" and window.estado == RUBRIC_AUSENTE:
            raise Fail(f"{window.name}: census-only window claims an implementation")
        if title != RETAIL_TITLE.get(window.name, "NO-VERIFICADO"):
            raise Fail(f"{window.name}: title {title} is not the declared label")
        if not evidence.startswith("EV-A"):
            raise Fail(f"{window.name}: evidence {evidence} lacks the census anchor")
    say(f"  digest UI-AUDIT-A1 (85 rows) = {fnv1a64([line for line in render_a(windows)])}")
    say("  verdict: A OK (85/85 ids, categories, surfaces, states, titles)")


def block_b(windows: list[Window], audit: str) -> None:
    say("BLOCK B - patch/rewrite rule 85/85 (UI-AUDIT-001 B)")
    verdicts = {}
    for window in windows:
        verdicts[window.verdict] = verdicts.get(window.verdict, 0) + 1
    say(
        "  rule verdicts = "
        + " · ".join(f"{key}={value}" for key, value in sorted(verdicts.items()))
    )
    if sum(verdicts.values()) != 85:
        raise Fail(f"rule covers {sum(verdicts.values())}/85 windows")
    rewrite_reasons = {}
    for window in windows:
        if window.verdict == VERDICT_REWRITE:
            key = window.rule.split("-")[0]
            rewrite_reasons[key] = rewrite_reasons.get(key, 0) + 1
    say(
        "  rewrite by rule = "
        + " · ".join(f"{key}:{value}" for key, value in sorted(rewrite_reasons.items()))
    )
    rows = table_rows(audit, MARK_B, "B")
    say(f"  document rows = {len(rows)}")
    if len(rows) != 85:
        raise Fail(f"document has {len(rows)} verdict rows, expected 85")
    seen = set()
    for window, cells in zip(windows, rows):
        if len(cells) != 7:
            raise Fail(f"{window.name}: {len(cells)} cells, expected 7")
        number, name, p1, p2, nodes, p3, verdict = cells
        if int(number) != window.index + 1:
            raise Fail(f"row order out of sequence at {number}")
        if name != f"`{window.name}`":
            raise Fail(f"row {number}: id {name} != {window.name}")
        if name in seen:
            raise Fail(f"duplicated verdict row {name}")
        seen.add(name)
        if verdict != window.verdict:
            raise Fail(
                f"{window.name}: document verdict {verdict} != rule {window.verdict} "
                f"({window.rule})"
            )
        expected_p1 = "sí" if window.spec is not None else "no"
        if p1 != expected_p1:
            raise Fail(f"{window.name}: P1 {p1} != {expected_p1}")
        if window.builder is not None and nodes != str(window.builder.nodes):
            raise Fail(f"{window.name}: nodes {nodes} != {window.builder.nodes}")
    say(f"  digest UI-AUDIT-B1 (85 rows) = {fnv1a64([line for line in render_b(windows)])}")
    say("  verdict: B OK (85/85 verdicts recomputed, document agrees)")


AXES = [
    "A1 superficie",
    "A2 paleta",
    "A3 tipografía",
    "A4 texto",
    "A5 navegación",
    "A6 entrada",
    "A7 presentación",
]


def block_c(audit: str, roadmap: str) -> None:
    say("BLOCK C - compatibility contract + E1 re-estimation (UI-AUDIT-001 C)")
    for axis in AXES:
        for field in ("garantiza", "no cubre", "interfaz M38/M39"):
            if f"{axis} · {field}" not in audit:
                raise Fail(f"contract axis '{axis}' lacks the '{field}' line")
    say(f"  axes = {len(AXES)} × 3 fields present")

    published = published_e1(roadmap)
    say(
        "  published M36..M40 = {:.2f} ({:.2f}–{:.2f}) u".format(*published)
    )
    block = e1_block(audit)
    components = sorted(key for key in block if re.fullmatch(r"m\d{2}", key))
    if len(components) != 5:
        raise Fail(f"E1 block declares {len(components)} components, expected 5")
    low = sum(block[key][0] for key in components)
    high = sum(block[key][1] for key in components)
    total = block["total"]
    if abs(low - total[0]) > 0.005 or abs(high - total[1]) > 0.005:
        raise Fail(
            f"E1 total {total} != sum of components ({low:.2f}, {high:.2f})"
        )
    for key in components:
        if block[key][0] > block[key][1]:
            raise Fail(f"E1 component {key} has low > high")
    nominal, p_low, p_high = published
    if not (p_low <= nominal <= p_high):
        raise Fail("published nominal outside its own range")
    delta_low = low - p_low
    delta_high = high - p_high
    say(f"  E1 re-estimate = {total[0]:.2f}–{total[1]:.2f} u (sum recomputed OK)")
    say(f"  delta vs published = {delta_low:+.2f} / {delta_high:+.2f} u")
    spent = block.get("gastado")
    if spent is None:
        raise Fail("E1 block lacks the 'gastado' (M36 already spent) row")
    say(f"  M36 spent (measured) = {spent[0]:.2f}–{spent[1]:.2f} u of {nominal:.2f} published")
    say("  verdict: C OK (7 axes × 3 fields, E1 arithmetic closes)")


def e1_block(audit: str) -> dict[str, tuple[float, float]]:
    match = re.search(r"<!-- E1-RANGE(.*?)-->", audit, re.S)
    if not match:
        raise Fail("document lacks the E1-RANGE block")
    out: dict[str, tuple[float, float]] = {}
    for line in match.group(1).strip().splitlines():
        cells = line.split()
        if len(cells) != 3:
            raise Fail(f"E1-RANGE line malformed: {line!r}")
        out[cells[0]] = (float(cells[1]), float(cells[2]))
    return out


def published_e1(roadmap: str) -> tuple[float, float, float]:
    values: dict[str, tuple[float, float, float]] = {}
    for line in roadmap.splitlines():
        stripped = line.strip()
        if not stripped.startswith("|"):
            continue
        cells = [cell.strip() for cell in stripped.strip("|").split("|")]
        if len(cells) != 6 or not re.fullmatch(r"M\d{2}", cells[0]):
            continue
        match = re.fullmatch(r"(\d+\.\d{2}) \((\d+\.\d{2})–(\d+\.\d{2})\)", cells[4])
        if not match:
            continue
        values[cells[0]] = (
            float(match.group(1)),
            float(match.group(2)),
            float(match.group(3)),
        )
    wanted = [f"M{number}" for number in range(36, 41)]
    if any(key not in values for key in wanted):
        raise Fail("roadmap lacks one of M36..M40 with a parseable u nominal")
    return (
        sum(values[key][0] for key in wanted),
        sum(values[key][1] for key in wanted),
        sum(values[key][2] for key in wanted),
    )


# --------------------------------------------------------------------------- #
# main
# --------------------------------------------------------------------------- #
def derive_all():
    census = parse_census()
    specs = parse_specs()
    tsv = parse_tsv()
    builders = parse_builders()
    windows = derive(census, specs, tsv, builders)
    return census, specs, tsv, builders, windows


def run(args: argparse.Namespace) -> int:
    census, specs, tsv, builders, windows = derive_all()
    if args.emit:
        text = read_text(args.doc)
        text = splice(text, MARK_A, render_a(windows), "A")
        text = splice(text, MARK_B, render_b(windows), "B")
        target = ROOT / args.doc
        target.write_text(text, encoding="utf-8")
        print(f"UI-AUDIT-001: regenerated {args.doc} ({len(windows)} rows × 2 tables)")
        return 0

    audit = read_text(args.doc)
    roadmap = read_text(args.roadmap)

    say(f"PROBE-UI-AUDIT-001 - M36 UI-0 gate (UI-AUDIT-001 A+B+C, {CR})")
    say(
        "  sources: {} · {} · ui_playable_g1..g3 · ui_spec.rs · ui_shell.rs · TSV W34-2"
        .format(Path(CENSUS).name, Path(TSV).name)
    )
    say(f"  builders parsed = {len(builders)} ({', '.join(sorted(builders))})")
    say()
    try:
        block0(roadmap)
        say()
        block_a(windows, census, specs, tsv, audit)
        say()
        block_b(windows, audit)
        say()
        block_c(audit, roadmap)
    except Fail as error:
        say()
        say(f"VERDICT: FAIL - {error}")
        print("\n".join(REPORT))
        return 1
    say()
    say("VERDICT: PASS")
    print("\n".join(REPORT))
    return 0


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--doc", default=AUDIT_DOC, help="audit document to gate")
    parser.add_argument("--roadmap", default=ROADMAP_DOC, help="roadmap transcript")
    parser.add_argument(
        "--emit",
        action="store_true",
        help="regenerate the marked table regions of --doc from the tree",
    )
    args = parser.parse_args(argv)
    try:
        return run(args)
    except Fail as error:
        print(f"PROBE-UI-AUDIT-001 - cannot run: {error}")
        return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
