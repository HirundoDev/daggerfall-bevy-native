#!/usr/bin/env python3
"""CENSUS-82-M42 - mechanical census of the 82 DFU questing actions.

Source A: `reference/dfu-spec/Assets/Scripts/Game/Questing/Actions/*.cs`
(82 classes, all `: ActionTemplate`).
Source B: `crates/behavior/src/quests.rs` (`pub enum OpcodeEffect`,
`QuestRequest::from_effect`, `Quest::run_action`).

One row per DFU class, with the variant(s) covering it, the verdict
`executed` (the variant dispatches via `QuestRequest::from_effect`, that is,
it crosses the organ boundary) or `NEGATIVE-proven` (`from_effect` returns
`None` and the effect is applied inside `run_action`, cited by line), and a
citation `src=crates/behavior/src/quests.rs:<line>` on every row.

Determinism: two child processes emit the canonical body; the parent requires
byte-identical stdout and closes with `DIGEST <sha256>` over the preceding bytes.
Fails (exit != 0) if the row count != 82, if any row lacks `src=`,
if a DFU class ends up with no variant and no documented proof, or if the variant/
verdict goldens drift.

Usage:
    python3 scripts/quest-82-census.py             # census + 2 processes + DIGEST
    python3 scripts/quest-82-census.py --emit      # canonical body (child)
    python3 scripts/quest-82-census.py --markdown  # doc table (same source)
  python3 scripts/quest-82-census.py --falsify row-count|src-strip|class-double
"""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import shutil
import subprocess
import sys
from collections import OrderedDict
from dataclasses import dataclass
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
ACTIONS = ROOT / "reference" / "dfu-spec" / "Assets" / "Scripts" / "Game" / "Questing" / "Actions"
QUESTS = ROOT / "crates" / "behavior" / "src" / "quests.rs"
SRC = "crates/behavior/src/quests.rs"
SLUG = "quest-82-census"
ARTIFACTS = ROOT / ".sintergic" / "artifacts" / "m42-sys" / SLUG
CR = "CR-a69444c1-adc6-475f-aea4-f8b1fed8d74d"
GATE = "M42/W42-5"
PROBE = "CENSUS-82-M42"

#  Goldens measured at the starting HEAD (065f3a9): 89 OpcodeEffect variants,
#  10 of them with `from_effect` -> None (internal effect), 82 DFU classes.
EXPECTED_ROWS = 82
EXPECTED_EXECUTED = 73
EXPECTED_NEGATIVE = 9
EXPECTED_VARIANTS = 89
EXPECTED_NONE = 10

ANCHOR = re.compile(r"Actions/([A-Za-z0-9_]+)\.cs")
VARIANT_DECL = re.compile(r"([A-Z][A-Za-z0-9]*),$")
ARM = re.compile(r"OpcodeEffect::([A-Z][A-Za-z0-9]*) =>")
EFFECT_USE = re.compile(r"OpcodeEffect::([A-Z][A-Za-z0-9]*)")
ROW_LINE = re.compile(r"^ROW (\d{2}) class=(\S+) variants=(\S+) verdict=(\S+) src=(\S+)(?: .*)?$")
CENSUS_LINE = re.compile(
    r"^CENSUS classes=(\d+) rows=(\d+) executed=(\d+) negative=(\d+) "
    r"variants=(\d+) from-effect-none=(\d+) .*src=(\S+)$"
)

#  Note per internal variant: where the effect is applied and why it does not cross
#  the organ boundary. `{arm}` is filled by the parser with the real arm line.
NEGATIVE_NOTE = {
    "LogMessage": "`run_action` anota el step en el journal (`{arm}`)",
    "JournalNote": "`run_action` anota la nota libre en el journal (`{arm}`)",
    "DisplayMessage": "`run_action` encola el popup y rompe el tick (`{arm}`)",
    "RemoveLogMessage": "`run_action` retiene el journal por step/message (`{arm}`)",
    "EndQuest": "`run_action` rompe el tick y termina la quest (`{arm}`)",
    "ClearTask": "`run_action` limpia la tarea objetivo (`{arm}`)",
    "StartTask": "`run_action` marca la tarea objetivo como disparada (`{arm}`)",
    "UnsetTask": "`run_action` limpia la tarea objetivo (`{arm}`)",
    "StartTimer": "`run_action` arranca el reloj de la quest (`{arm}`)",
    "StopTimer": "`run_action` para el reloj de la quest (`{arm}`)",
}

#  DFU class with no variant of its own: it can only enter here with the external effect
#  DOCUMENTED (criterion 2: zero silently ignored actions). Empty = the 82
#  are covered by a variant; if a new class appears with no variant and no
#  note, the census FAILS.
EXTERNAL_EFFECT: dict[str, str] = {}


@dataclass(frozen=True)
class Variant:
    name: str
    line: int
    anchors: tuple[str, ...]


@dataclass(frozen=True)
class Row:
    class_name: str
    variants: tuple[str, ...]
    verdict: str
    src_line: int
    none_line: int | None
    arm_line: int | None
    note: str | None


@dataclass(frozen=True)
class Rule:
    name: str
    ok: bool
    detail: str


def digest(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def read_lines(path: Path) -> list[str]:
    return path.read_text(encoding="utf-8").splitlines()


def enum_variants(lines: list[str]) -> list[Variant]:
    start = next(
        (i for i, line in enumerate(lines) if line.startswith("pub enum OpcodeEffect {")),
        None,
    )
    if start is None:
        raise SystemExit("FAIL: no `pub enum OpcodeEffect {` en quests.rs")
    variants: list[Variant] = []
    doc: list[str] = []
    for index in range(start + 1, len(lines)):
        line = lines[index]
        if line == "}":
            break
        stripped = line.strip()
        if line.startswith("    ///"):
            doc.append(line)
            continue
        if not stripped:
            continue
        match = VARIANT_DECL.match(stripped)
        if match:
            anchors = tuple(sorted(set(ANCHOR.findall("\n".join(doc)))))
            variants.append(Variant(match.group(1), index + 1, anchors))
        doc = []
    return variants


def from_effect(lines: list[str]) -> tuple[set[str], int]:
    start = next(
        (i for i, line in enumerate(lines) if "pub fn from_effect(" in line),
        None,
    )
    if start is None:
        raise SystemExit("FAIL: no `pub fn from_effect(` en quests.rs")
    none_at = next(
        (i for i in range(start, len(lines)) if "_ => return None," in lines[i]),
        None,
    )
    if none_at is None:
        raise SystemExit("FAIL: from_effect sin rama `_ => return None`")
    close = next(i for i in range(none_at, len(lines)) if lines[i].strip() == "}")
    body = "\n".join(lines[start:close])
    return set(EFFECT_USE.findall(body)), none_at + 1


def run_action_arms(lines: list[str]) -> dict[str, int]:
    start = next(
        (i for i, line in enumerate(lines) if line.strip() == "fn run_action("),
        None,
    )
    if start is None:
        raise SystemExit("FAIL: no `fn run_action(` en quests.rs")
    end = next(
        (i for i in range(start + 1, len(lines)) if lines[i].startswith("    fn ")),
        len(lines),
    )
    arms: dict[str, int] = {}
    for index in range(start, end):
        match = ARM.search(lines[index])
        if match:
            arms.setdefault(match.group(1), index + 1)
    return arms


def dfu_classes() -> list[str]:
    if not ACTIONS.is_dir():
        raise SystemExit(f"FAIL: no existe {ACTIONS}")
    return sorted(path.stem for path in ACTIONS.glob("*.cs"))


def build_rows(lines: list[str], classes: list[str]) -> tuple[list[Row], list[Rule]]:
    variants = enum_variants(lines)
    executed, none_line = from_effect(lines)
    arms = run_action_arms(lines)
    rules: list[Rule] = []

    rules.append(
        Rule(
            "variant-count",
            len(variants) == EXPECTED_VARIANTS,
            f"variants={len(variants)} want={EXPECTED_VARIANTS}",
        )
    )
    rules.append(
        Rule(
            "from-effect-none",
            len(variants) - len(executed) == EXPECTED_NONE,
            f"none={len(variants) - len(executed)} want={EXPECTED_NONE}",
        )
    )

    unknown = sorted({name for variant in variants for name in variant.anchors} - set(classes))
    rules.append(
        Rule(
            "anchor-resolves",
            not unknown,
            f"anchors fuera de Actions/={','.join(unknown) or 'none'}",
        )
    )

    covering: dict[str, list[Variant]] = {}
    for variant in variants:
        for name in variant.anchors:
            covering.setdefault(name, []).append(variant)

    rows: list[Row] = []
    for name in classes:
        candidates = covering.get(name, [])
        if not candidates:
            note = EXTERNAL_EFFECT.get(name)
            rules.append(
                Rule(
                    "silently-ignored",
                    note is not None,
                    f"class={name} sin variante y sin efecto externo documentado",
                )
            )
            rows.append(
                Row(name, (), "NEGATIVE-proven", none_line, none_line, None, note or "SIN PRUEBA")
            )
            continue
        hits = [variant for variant in candidates if variant.name in executed]
        if hits:
            first = hits[0]
            rows.append(
                Row(
                    name,
                    tuple(variant.name for variant in candidates),
                    "executed",
                    first.line,
                    None,
                    None,
                    None,
                )
            )
            continue
        first = candidates[0]
        arm = arms.get(first.name)
        missing = [variant.name for variant in candidates if arms.get(variant.name) is None or variant.name not in NEGATIVE_NOTE]
        rules.append(
            Rule(
                "negative-proof",
                not missing,
                f"class={name} variants={','.join(v.name for v in candidates)} "
                f"arm={arm or 'absent'} unproven={','.join(missing) or 'none'}",
            )
        )
        note = "efecto interno: " + " · ".join(
            NEGATIVE_NOTE.get(variant.name, "SIN NOTA").format(arm=arms.get(variant.name) or 0)
            for variant in candidates
        )
        rows.append(
            Row(
                name,
                tuple(variant.name for variant in candidates),
                "NEGATIVE-proven",
                arm or none_line,
                none_line,
                arm,
                note,
            )
        )
    return rows, rules


def body_text(rows: list[Row], rules: list[Rule], quests_sha: str) -> str:
    executed = sum(1 for row in rows if row.verdict == "executed")
    negative = sum(1 for row in rows if row.verdict == "NEGATIVE-proven")
    variants = len(enum_variants(read_lines(QUESTS)))
    none = variants - len(from_effect(read_lines(QUESTS))[0])
    out: list[str] = [
        f"CENSUS classes={len(dfu_classes())} rows={len(rows)} executed={executed} "
        f"negative={negative} variants={variants} from-effect-none={none} "
        f"quests-sha256={quests_sha} src={SRC}"
    ]
    for index, row in enumerate(rows, 1):
        line = (
            f"ROW {index:02d} class={row.class_name} variants={','.join(row.variants) or '-'} "
            f"verdict={row.verdict} src={SRC}:{row.src_line}"
        )
        if row.verdict == "NEGATIVE-proven":
            line += f" from-effect=none@{row.none_line}"
            if row.arm_line:
                line += f" internal=run_action@{row.arm_line}"
        out.append(line)
    failed = [rule for rule in rules if not rule.ok]
    for rule in failed:
        out.append(f"[FAIL] {rule.name} - {rule.detail}")
    out.append("VERDICT: PASS" if not failed else "VERDICT: FAIL")
    return "\n".join(out) + "\n"


def markdown_text(rows: list[Row]) -> str:
    out = [
        "| # | Clase DFU | Variante(s) `OpcodeEffect` | Veredicto | Evidencia |",
        "|---:|---|---|---|---|",
    ]
    for index, row in enumerate(rows, 1):
        variants = ", ".join(f"`{name}`" for name in row.variants) or "—"
        evidence = f"`src={SRC}:{row.src_line}`"
        if row.verdict == "NEGATIVE-proven":
            evidence += f" · `from_effect`→`None` (`{SRC}:{row.none_line}`) · {row.note}"
        out.append(f"| {index} | `{row.class_name}` | {variants} | `{row.verdict}` | {evidence} |")
    return "\n".join(out) + "\n"


def parse_body(text: str) -> tuple[dict[str, int], list[Rule]]:
    lines = text.splitlines()
    header = next((line for line in lines if line.startswith("CENSUS ")), None)
    rows = [line for line in lines if line.startswith("ROW ")]
    rules: list[Rule] = []

    counts: dict[str, int] = {
        "rows": len(rows),
        "executed": sum(1 for line in rows if " verdict=executed " in line),
        "negative": sum(1 for line in rows if " verdict=NEGATIVE-proven " in line),
        "variants": 0,
        "none": 0,
    }
    match = CENSUS_LINE.match(header or "")
    if match:
        counts["variants"] = int(match.group(5))
        counts["none"] = int(match.group(6))

    rules.append(
        Rule("row-count", len(rows) == EXPECTED_ROWS, f"rows={len(rows)} want={EXPECTED_ROWS}")
    )
    uncited = [line.split(" ")[2] for line in rows if " src=" not in line]
    rules.append(
        Rule(
            "src-present",
            not uncited and bool(rows),
            f"rows={len(rows)} uncited={','.join(uncited[:6]) or 'none'}",
        )
    )
    shaped = [line for line in rows if ROW_LINE.match(line)]
    rules.append(
        Rule(
            "row-shape",
            len(shaped) == len(rows) and bool(rows),
            f"shaped={len(shaped)} rows={len(rows)}",
        )
    )
    names = {ROW_LINE.match(line).group(2) for line in shaped}
    rules.append(
        Rule(
            "class-unique",
            len(names) == EXPECTED_ROWS and len(names) == len(shaped),
            f"classes={len(names)} rows={len(shaped)}",
        )
    )
    rules.append(
        Rule(
            "verdict-golden",
            counts["executed"] == EXPECTED_EXECUTED and counts["negative"] == EXPECTED_NEGATIVE,
            f"executed={counts['executed']} negative={counts['negative']}",
        )
    )
    rules.append(
        Rule(
            "variant-golden",
            counts["variants"] == EXPECTED_VARIANTS and counts["none"] == EXPECTED_NONE,
            f"variants={counts['variants']} none={counts['none']}",
        )
    )
    body = [line for line in lines if not line.startswith("DIGEST ")]
    rules.append(
        Rule(
            "verdict-last",
            bool(body) and body[-1] == "VERDICT: PASS",
            f"last={(body[-1] if body else '')[:40]!r}",
        )
    )
    return counts, rules


def child_body() -> str:
    """Cuerpo canonico de un proceso hijo (sin gate lines ni DIGEST)."""
    lines = read_lines(QUESTS)
    classes = dfu_classes()
    rules: list[Rule] = [
        Rule(
            "class-count",
            len(classes) == EXPECTED_ROWS,
            f"classes={len(classes)} want={EXPECTED_ROWS}",
        )
    ]
    shape = [
        name
        for name in classes
        if ": ActionTemplate" not in (ACTIONS / f"{name}.cs").read_text(encoding="utf-8", errors="replace")
    ]
    rules.append(
        Rule("class-shape", not shape, f"not-ActionTemplate={','.join(shape[:6]) or 'none'}")
    )
    rows, structural = build_rows(lines, classes)
    rules.extend(structural)
    sha = digest(QUESTS.read_bytes())
    provisional = body_text(rows, rules, sha)
    _, body_rules = parse_body(provisional)
    rules.extend(body_rules)
    return body_text(rows, rules, sha)


def run_child() -> subprocess.CompletedProcess:
    return subprocess.run(
        [sys.executable, str(Path(__file__).resolve()), "--emit"],
        cwd=ROOT,
        capture_output=True,
    )


def payload(
    bodies: list[bytes],
    rules: list[Rule],
    counts: dict[str, int],
    extra: dict | None = None,
) -> dict:
    body = {
        "gate": GATE,
        "probe": PROBE,
        "slug": SLUG,
        "cr": CR,
        "commit": head_commit(),
        "source": {
            "path": SRC,
            "sha256": digest(QUESTS.read_bytes()),
            "variants": counts["variants"],
            "from_effect_none": counts["none"],
        },
        "classes": len(dfu_classes()),
        "rows": counts["rows"],
        "executed": counts["executed"],
        "negative": counts["negative"],
        "byte_identical": len(bodies) == 2 and bodies[0] == bodies[1],
        "checks": [{"name": rule.name, "ok": rule.ok, "detail": rule.detail} for rule in rules],
        "failed": [rule.name for rule in rules if not rule.ok],
        "runs": [
            {"run": index + 1, "bytes": len(data), "stdout_sha256": digest(data)}
            for index, data in enumerate(bodies)
        ],
        "digest": digest(bodies[0]) if bodies else None,
        "verdict": "PASS" if bodies and all(rule.ok for rule in rules) else "FAIL",
    }
    if extra:
        body.update(extra)
    return body


def head_commit() -> str | None:
    proc = subprocess.run(["git", "rev-parse", "HEAD"], cwd=ROOT, capture_output=True)
    return proc.stdout.decode("utf-8", "replace").strip() or None


def write_artifacts(bodies: list[bytes], rules: list[Rule], counts: dict[str, int],
                    out: Path, extra: dict | None = None) -> Path:
    out.mkdir(parents=True, exist_ok=True)
    for index, data in enumerate(bodies, 1):
        (out / f"run{index}.stdout").write_bytes(data)
    path = out / "manifest.json"
    path.write_text(json.dumps(payload(bodies, rules, counts, extra), indent=2, sort_keys=True) + "\n")
    return path


#  ── falsifiers: they mutate the canonical body under its own rule ─────────


def mutate_row_drop(body: str) -> tuple[str, str]:
    lines = body.splitlines()
    for index, line in enumerate(lines):
        if line.startswith("ROW ") and " verdict=executed " in line:
            del lines[index]
            return "\n".join(lines) + "\n", "drop one executed ROW (82 -> 81)"
    raise AssertionError("no ROW executed row")


def mutate_src_strip(body: str) -> tuple[str, str]:
    lines = body.splitlines()
    for index, line in enumerate(lines):
        if line.startswith("ROW ") and " src=" in line:
            lines[index] = re.sub(r" src=\S+", "", line, count=1)
            return "\n".join(lines) + "\n", "strip src= from the first ROW"
    raise AssertionError("no ROW with src=")


def mutate_class_double(body: str) -> tuple[str, str]:
    lines = body.splitlines()
    first = next((line for line in lines if line.startswith("ROW ")), None)
    if first is None:
        raise AssertionError("no ROW")
    for index, line in enumerate(lines):
        if line.startswith("ROW ") and line != first:
            lines[index] = re.sub(r"(ROW \d{2}) class=\S+", r"\1 class=" + first.split(" class=")[1].split(" ")[0], line, count=1)
            return "\n".join(lines) + "\n", "duplicate the first class into a second ROW"
    raise AssertionError("no second ROW")


FALSIFIERS = {
    "row-count": ("row-count", mutate_row_drop),
    "src-strip": ("src-present", mutate_src_strip),
    "class-double": ("class-unique", mutate_class_double),
}


def falsify(mode: str, clean: bytes) -> int:
    target, mutate = FALSIFIERS[mode]
    mutated, note = mutate(clean.decode("utf-8", "replace"))
    _, rules = parse_body(mutated)
    failed = [rule.name for rule in rules if not rule.ok]
    red = target in failed
    _, restored = parse_body(clean.decode("utf-8", "replace"))
    clean_again = not [rule.name for rule in restored if not rule.ok]
    out = ARTIFACTS / f"falsify-{mode}"
    if out.exists():
        shutil.rmtree(out)
    out.mkdir(parents=True, exist_ok=True)
    (out / "mutated.stdout").write_text(mutated)
    print(f"FALSIFY {mode}: mutation={note}")
    print(f"FALSIFY {mode}: failed={','.join(failed) or 'none'}")
    print(f"FALSIFY {mode}: rule {target} {'RED' if red else 'STAYED GREEN'}")
    print(
        f"FALSIFY {mode}: clean body re-check="
        f"{'PASS' if clean_again else 'FAIL'} (mutation restored)"
    )
    return 0 if red and clean_again else 1


def main() -> int:
    parser = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter
    )
    parser.add_argument("--emit", action="store_true", help="emit the canonical body only")
    parser.add_argument("--markdown", action="store_true", help="emit the census table (doc)")
    parser.add_argument("--out", default=str(ARTIFACTS), help="archive root")
    parser.add_argument("--no-archive", action="store_true", help="do not write artifacts")
    parser.add_argument("--falsify", choices=sorted(FALSIFIERS), help="mutate the body")
    args = parser.parse_args()

    if args.emit:
        body = child_body()
        sys.stdout.write(body)
        return 0 if body.rstrip("\n").endswith("VERDICT: PASS") else 1

    if args.markdown:
        lines = read_lines(QUESTS)
        rows, rules = build_rows(lines, dfu_classes())
        failed = [rule.name for rule in rules if not rule.ok]
        sys.stdout.write(markdown_text(rows))
        return 1 if failed else 0

    captures = [run_child() for _ in range(2)]
    bodies = [capture.stdout for capture in captures]
    if args.falsify:
        return falsify(args.falsify, bodies[0])

    counts, rules = parse_body(bodies[0].decode("utf-8", "replace"))
    identical = bodies[0] == bodies[1]
    rules.extend(
        [
            Rule(
                "child-exit",
                all(capture.returncode == 0 for capture in captures),
                f"exit={','.join(str(capture.returncode) for capture in captures)}",
            ),
            Rule(
                "byte-identical",
                identical,
                f"{len(bodies[0])}B vs {len(bodies[1])}B",
            ),
            Rule(
                "sha256-equal",
                digest(bodies[0]) == digest(bodies[1]),
                digest(bodies[0])[:16],
            ),
        ]
    )
    header = (
        f"GATE {PROBE} processes={len(captures)} stdout={len(bodies[0])}B "
        f"sha256={digest(bodies[0])}\n"
    )
    archive = None
    if not args.no_archive:
        archive = write_artifacts(bodies, rules, counts, Path(args.out))
    failed = [rule for rule in rules if not rule.ok]
    printed = (
        header
        + bodies[0].decode("utf-8", "replace")
        + f"GATE archive={archive.relative_to(ROOT) if archive else 'disabled'}\n"
        + "".join(f"[FAIL] {rule.name} - {rule.detail}\n" for rule in failed)
    )
    sys.stdout.write(printed)
    sys.stdout.write(f"DIGEST {digest(printed.encode())}\n")
    return 0 if not failed else 1


if __name__ == "__main__":
    sys.exit(main())
