#!/usr/bin/env python3
"""SYS-CRAFT-001 gate - M41/W41-4 (CR-0f2a75d6-4d6e-4a1b-8ac2-6c6f3ef0b9d1).

Runs the `SYS-CRAFT-001` probe twice in independent processes and gates the
output against the retail artifacts it claims to measure, then mutates a TEMP
COPY of the captures to prove each named falsifier can turn the gate red.

Contract (W41-4):
  1. two processes (`PROBE_RUN=1|2`), both exit 0, both `VEREDICTO: PASS`,
     byte-identical stdout, equal sha256;
  2. the last line is `DIGEST <sha256>` = sha256 of every byte printed before
     it (lines joined with "\\n", no trailing newline);
  3. structural checks derived from `$HOME/Games/Daggerfall/app`
     (`FALL.EXE` item table, `ARENA2/MAGIC.DEF`) and from the crate sources:
       - the POINTS census covers every template with `enchantment_points > 0`
         (an independent 288x48 parse of `FALL.EXE`, never the probe's claim);
       - every `MAGIC.DEF` record has exactly one `ARTEFACT` row whose name,
         class and `(type, param)` pairs match the record bytes;
       - the recipe count equals the declared count and the source's
         `display_name_key` lines, every key reproduces its hash and every
         ingredient carries the `is_ingredient` bit of its template;
       - every row cites `src=<file:line>` and the cited line exists;
       - the adversarial refusals are present with their named rules;
       - the material floor tables recompute from the frozen multiplier arrays.
  4. `--falsify <name>` mutates the capture copy (recomputing the digest so the
     digest check cannot mask it) and requires the named check to go RED; the
     clean capture must analyse green again.

Exit: 0 PASS, 1 FAIL, 2 usage/IO/SKIP (retail missing).

Usage:
  scripts/sys-craft-001.py [--skip-build] [--app-root DIR] [--falsify NAME|all]
"""

from __future__ import annotations

import argparse
import hashlib
import json
import math
import os
import re
import shutil
import struct
import subprocess
import sys
import tempfile
from datetime import datetime, timezone
from functools import lru_cache
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
PROBE = "SYS-CRAFT-001"
CR = "CR-0f2a75d6-4d6e-4a1b-8ac2-6c6f3ef0b9d1"
BIN = ROOT / "target" / "debug" / "daggerfall-app"
SLUG = "craft"
ARTIFACT_ROOT = ROOT / ".sintergic" / "artifacts" / "m41-sys" / SLUG
ALCHEMY_SRC = ROOT / "crates" / "crafting" / "src" / "alchemy.rs"

PASS, FAIL, SKIP = 0, 1, 2

# ── retail layout (crates/inventory/src/items.rs:50-56, items.rs:197-209) ────
DEFAULT_ITEMS_OFFSET = 1_776_954
RUBY_SENTINEL = b"Ruby"
ITEM_NAME_LENGTH = 24
ITEM_RECORD_SIZE = 48
TOTAL_ITEMS = 288
POINTS_OFFSET = 38
PROPERTIES_OFFSET = 43
MAGIC_RECORD_SIZE = 62
MAGIC_NAME_LENGTH = 32

# ── frozen engine tables (cited in the probe, recomputed here) ───────────────
WEAPON_MATERIALS = [
    "Iron", "Steel", "Silver", "Elven", "Dwarven",
    "Mithril", "Adamantium", "Ebony", "Orcish", "Daedric",
]
WEAPON_MULT = [-0.25, 0.0, 0.75, 0.25, 0.5, 0.25, 0.75, 1.0, 1.5, 2.0]
ARMOR_MULT = {0x000: 0.0, **{0x200 + i: mult for i, mult in enumerate(WEAPON_MULT)}}
ARMOR_LEATHER = "Leather"

KIND_COST = {
    "FeatherWeight": 100,
    "StrengthensArmor": 700,
    "EnhancesSkill": 900,
    "RepairsObjects": 900,
    "AbsorbsSpells": 1500,
    "ExtraWeight": -100,
    "WeakensArmor": -700,
    "ExtraSpellPts.Spring": 500,
    "ExtraSpellPts.Summer": 500,
    "ExtraSpellPts.Fall": 500,
    "ExtraSpellPts.Winter": 500,
    "ExtraSpellPts.Waxing": 200,
    "ExtraSpellPts.Waning": 200,
    "ExtraSpellPts.New": 200,
    "ExtraSpellPts.Undead": 700,
    "ExtraSpellPts.Daedra": 800,
    "ExtraSpellPts.Humanoids": 900,
    "ExtraSpellPts.Animals": 1000,
}

SLOT_TEMPLATES = ["120/steel", "120/daedric", "108/0x200", "102/0x209"]

CLASS_NAMES = {0: "RegularMagicItem", 1: "ArtifactClass1", 2: "ArtifactClass2"}

EFFECT_LABELS = {
    "ExtraSpellPts", "RepairsObjects", "AbsorbsSpells", "EnhancesSkill",
    "FeatherWeight", "StrengthensArmor", "ExtraWeight", "WeakensArmor",
    "SpecialArtifactEffect",
}

REQUIRED_REFUSALS = [
    "beyondLimit",
    "unknown-recipe",
    "duplicate-key",
    "non-ingredient",
    "noRoomInItem",
    "notEnoughGold",
    "unknown-enchantment",
]

RE_HEADER = re.compile(r"^PROBE-SYS-CRAFT-001 - .+ \(M41/W41-4, CR-[0-9a-f-]{36}\)$")
RE_DIGEST = re.compile(r"^DIGEST ([0-9a-f]{64})$")
RE_ROWS = re.compile(r"^ROWS ((?:[\w-]+=\d+ ?)+)$")
RE_CHECKS = re.compile(r"^CHECKS passed=(\d+) failed=(\d+)$")
RE_OK = re.compile(r"^\[OK\] (\S+) - (.*) \(src=(\S+:\d+)\)$")
RE_SRC_SUFFIX = re.compile(r" src=(\S+:\d+)$")
RE_POINTS = re.compile(r"^POINTS (\d+) base=(-?\d+) mult=(-?\d\.\d{2}) budget=(-?\d+) src=(\S+:\d+)$")
RE_POINTS_MAT = re.compile(
    r"^POINTS-MAT (\d+) material=(\w+) mult=(-?\d\.\d{2}) base=(-?\d+) budget=(-?\d+) src=(\S+:\d+)$"
)
RE_POINTS_ARMOR = re.compile(
    r"^POINTS-ARMOR (\d+) native=0x([0-9a-f]{3}) material=(\w+) mult=(-?\d\.\d{2}) "
    r"base=(-?\d+) budget=(-?\d+) src=(\S+:\d+)$"
)
RE_ACCOUNT = re.compile(
    r"^ACCOUNT (\S+) budget=(-?\d+) sel=(\S+) used=(-?\d+) remaining=(-?\d+) verdict=(\S+) src=(\S+:\d+)$"
)
RE_SLOT = re.compile(
    r"^SLOT (\S+) (\S+) cost=(-?\d+) budget=(-?\d+) allowed=([01]) reason=(\S+) src=(\S+:\d+)$"
)
RE_ARTEFACT = re.compile(
    r"^ARTEFACT (\d+) (.+) kind=(\d) class=(\w+) points=(-?\d+) effects=(\S+) raw=(\S+) src=(\S+:\d+)$"
)
RE_RECIPE = re.compile(
    r"^RECIPE (\w+) key=(-?\d+) hash=0x([0-9a-f]{8}) price=(\d+) effect=(\S+) variant=(-?\d+) "
    r"ingredients=([\d,]+) texture=(-?\d+) src=(\S+:\d+)$"
)
RE_MIX = re.compile(
    r"^MIX (.+?) => (\S+) key=(-?\d+) price=(\d+) variant=(-?\d+) magnitude=(-?\d+) src=(\S+:\d+)$"
)
RE_REFUSE = re.compile(r"^REFUSE (\w+) rule=(\S+) case=(\S+) src=(\S+:\d+)$")
RE_SECTION = re.compile(r"^-- .+ --$")


class Skip(Exception):
    """Missing retail / IO: the gate must SKIP (exit 2), never PASS."""


# ══════════════════════════════════════════════════════════════════════════
# retail readers (independent of the Rust parse)
# ══════════════════════════════════════════════════════════════════════════

def app_root_from(arg: str | None) -> Path:
    if arg:
        return Path(arg).expanduser()
    env = os.environ.get("DF_APP_ROOT")
    if env:
        return Path(env).expanduser()
    return Path.home() / "Games" / "Daggerfall" / "app"


def load_templates(root: Path) -> list[dict]:
    """288 item templates of `FALL.EXE` (items.rs:34-56, 250-260)."""
    fall = root / "FALL.EXE"
    if not fall.is_file():
        raise Skip(f"{fall} not found (retail installation required)")
    data = fall.read_bytes()
    end = DEFAULT_ITEMS_OFFSET + len(RUBY_SENTINEL)
    if data[DEFAULT_ITEMS_OFFSET:end] == RUBY_SENTINEL:
        offset = DEFAULT_ITEMS_OFFSET
    else:
        offset = data.find(RUBY_SENTINEL)
    if offset < 0:
        raise Skip("FALL.EXE: \"Ruby\" sentinel not found")
    needed = offset + TOTAL_ITEMS * ITEM_RECORD_SIZE
    if len(data) < needed:
        raise Skip(f"FALL.EXE truncated at {offset}: need {needed} B, have {len(data)} B")
    templates = []
    for index in range(TOTAL_ITEMS):
        base = offset + index * ITEM_RECORD_SIZE
        record = data[base:base + ITEM_RECORD_SIZE]
        raw_name = record[:ITEM_NAME_LENGTH].split(b"\0")[0]
        templates.append(
            {
                "index": index,
                "name": raw_name.decode("latin-1"),
                "points": struct.unpack_from("<h", record, POINTS_OFFSET)[0],
                "properties": record[PROPERTIES_OFFSET],
                "ingredient": record[PROPERTIES_OFFSET] & 1 == 1,
            }
        )
    return templates


def load_magic(root: Path) -> tuple[bytes, list[dict]]:
    """`ARENA2/MAGIC.DEF`: `recordCount` + 62-byte records (magic_def.rs:121-160)."""
    path = root / "ARENA2" / "MAGIC.DEF"
    if not path.is_file():
        raise Skip(f"{path} not found (retail installation required)")
    data = path.read_bytes()
    if len(data) < 4:
        raise Skip(f"MAGIC.DEF: {len(data)} B < 4 B header")
    count = struct.unpack_from("<I", data, 0)[0]
    implied = 4 + count * MAGIC_RECORD_SIZE
    if implied != len(data):
        raise Skip(f"MAGIC.DEF: recordCount={count} implies {implied} B, file is {len(data)} B")
    records = []
    for index in range(count):
        base = 4 + index * MAGIC_RECORD_SIZE
        record = data[base:base + MAGIC_RECORD_SIZE]
        pairs = []
        for slot in range(10):
            off = 35 + slot * 2
            kind = struct.unpack_from("<b", record, off)[0]
            param = struct.unpack_from("<b", record, off + 1)[0]
            if kind != -1:
                pairs.append((kind, param))
        records.append(
            {
                "index": index,
                "name": record[:MAGIC_NAME_LENGTH].split(b"\0")[0].decode("latin-1"),
                "kind": record[32],
                "pairs": pairs,
            }
        )
    return data, records


def recipe_source_count() -> int:
    """`display_name_key` lines in the classic table (independent of the probe)."""
    if not ALCHEMY_SRC.is_file():
        raise Skip(f"{ALCHEMY_SRC} not found")
    text = ALCHEMY_SRC.read_text(errors="replace")
    return sum(1 for line in text.splitlines() if 'display_name_key: "' in line)


# ══════════════════════════════════════════════════════════════════════════
# arithmetic shared with the engine
# ══════════════════════════════════════════════════════════════════════════

def enchantment_power(base: int, mult: float) -> int:
    """`crafting.rs:71`: `base + floor(base * mult)`."""
    return base + math.floor(base * mult)


def recipe_hash(ids: list[int]) -> int:
    """`crafting.rs:90-105`: `0` for an empty list, else `h = 23*h + id` over the sorted ids, int32."""
    if not ids:
        return 0
    value = 17
    for item in sorted(ids):
        value = (value * 23 + item) & 0xFFFFFFFF
    return value - 0x100000000 if value >= 0x80000000 else value


@lru_cache(maxsize=64)
def _source_lines(path: str) -> tuple[str, ...]:
    return tuple((ROOT / path).read_text(errors="replace").splitlines())


def cited_line(ref: str) -> str | None:
    """The text of a `path:line` citation, or None when it does not resolve."""
    path, _, line = ref.rpartition(":")
    if not path or not line.isdigit():
        return None
    if not (ROOT / path).is_file():
        return None
    lines = _source_lines(path)
    number = int(line)
    if not 1 <= number <= len(lines):
        return None
    return lines[number - 1]


RESISTANCE_VARIANTS = ["Fire", "Frost", "Poison", "Shock", "Magicka"]


@lru_cache(maxsize=1)
def default_texture_record() -> int | None:
    """`RECIPE_TEXTURE_DEFAULT` (`alchemy.rs:28`), read from the source."""
    text = ALCHEMY_SRC.read_text(errors="replace")
    match = re.search(r"pub const RECIPE_TEXTURE_DEFAULT: i32 = (\d+);", text)
    return int(match.group(1)) if match else None


def source_recipe_block(ref: str) -> dict | None:
    """The `ClassicRecipe { ... }` entry a RECIPE row cites, read from the source.

    Returns the literal the table row must mirror: name, price, effect,
    variant ordinal, ingredient ids in source order and the texture record.
    """
    path, _, line = ref.rpartition(":")
    if not line.isdigit() or not (ROOT / path).is_file():
        return None
    lines = _source_lines(path)
    start = int(line) - 1
    if not 0 <= start < len(lines):
        return None
    block: list[str] = []
    for text in lines[start:start + 16]:
        block.append(text)
        if text.strip() == "},":
            break
    body = "\n".join(block)
    name = re.search(r'display_name_key: "([^"]+)"', body)
    price = re.search(r"price: (\d+)", body)
    effect = re.search(r'effect_key: "([^"]+)"', body)
    variant = re.search(r'variant: "([^"]*)"', body)
    ingredients = re.search(r"ingredients: &\[([\d, ]*)\]", body)
    texture = re.search(r"texture_record: ([A-Za-z0-9_]+)", body)
    if not all((name, price, effect, variant, ingredients, texture)):
        return None
    texture_value = (
        int(texture.group(1)) if texture.group(1).isdigit() else default_texture_record()
    )
    if texture_value is None:
        return None
    ids = [int(part) for part in ingredients.group(1).replace(" ", "").split(",") if part]
    variant_name = variant.group(1)
    return {
        "name": name.group(1),
        "price": int(price.group(1)),
        "effect": effect.group(1),
        "variant": RESISTANCE_VARIANTS.index(variant_name) if variant_name in RESISTANCE_VARIANTS else -1,
        "ids": ids,
        "texture": texture_value,
    }


# ══════════════════════════════════════════════════════════════════════════
# analysis
# ══════════════════════════════════════════════════════════════════════════

class Report:
    """Collects named structural checks: each is `(id, ok, detail)`."""

    def __init__(self) -> None:
        self.checks: list[tuple[str, bool, str]] = []

    def add(self, check_id: str, ok: bool, detail: str = "") -> None:
        self.checks.append((check_id, bool(ok), detail))

    @property
    def failed(self) -> list[tuple[str, bool, str]]:
        return [check for check in self.checks if not check[1]]


def analyze(
    text: str,
    templates: list[dict],
    magic_bytes: bytes,
    magic_records: list[dict],
    source_recipes: int,
) -> tuple[Report, dict]:
    report = Report()
    info: dict = {}

    raw = text.encode()
    if not raw.endswith(b"\n"):
        report.add("contract/layout", False, "stdout does not end with a newline")
    lines = text[:-1].split("\n") if text.endswith("\n") else text.split("\n")
    if not lines:
        report.add("contract/layout", False, "empty stdout")
        return report, info

    # ── contract: header, probe-run silence, wording, digest, verdict ───────
    report.add("contract/header", bool(RE_HEADER.match(lines[0])), lines[0][:100])
    report.add(
        "contract/probe-run",
        "PROBE_RUN" not in text,
        "PROBE_RUN never appears in the output",
    )
    banned = re.search(r"approximate|estimated|approx\.", text, re.IGNORECASE)
    report.add("contract/wording", banned is None, f"no approximation wording ({banned.group(0) if banned else '-'})")

    digest_ok = False
    if len(lines) >= 2 and RE_DIGEST.match(lines[-1]):
        covered = "\n".join(lines[:-1]).encode()
        digest_ok = hashlib.sha256(covered).hexdigest() == RE_DIGEST.match(lines[-1]).group(1)
        info["digest"] = RE_DIGEST.match(lines[-1]).group(1)
        report.add("contract/digest", digest_ok, "DIGEST = sha256 of the preceding bytes")
    else:
        report.add("contract/digest", False, "last line is not `DIGEST <sha256>`")
    report.add(
        "contract/verdict",
        len(lines) >= 2 and lines[-2] == "VEREDICTO: PASS",
        "penultimate line is `VEREDICTO: PASS`",
    )

    # ── split meta rows from evidence rows ──────────────────────────────────
    body = lines[1:]
    meta = {"VEREDICTO", "DIGEST", "ROWS", "CHECKS"}
    ok_rows: list[tuple[str, str, str]] = []
    fail_rows: list[str] = []
    points_rows, points_mat_rows, points_armor_rows = [], [], []
    account_rows, slot_rows, artefact_rows = [], [], []
    recipe_rows, mix_rows, refuse_rows = [], [], []
    unidentified: list[str] = []

    for line in body:
        if not line or line in meta or line.startswith(("VEREDICTO", "DIGEST", "ROWS ", "CHECKS ")):
            continue
        if RE_SECTION.match(line):
            continue
        match = RE_OK.match(line)
        if match:
            ok_rows.append((match.group(1), match.group(2), match.group(3)))
            continue
        if line.startswith("FAIL "):
            fail_rows.append(line)
            continue
        for regex, bucket in (
            (RE_POINTS, points_rows),
            (RE_POINTS_MAT, points_mat_rows),
            (RE_POINTS_ARMOR, points_armor_rows),
            (RE_ACCOUNT, account_rows),
            (RE_SLOT, slot_rows),
            (RE_ARTEFACT, artefact_rows),
            (RE_RECIPE, recipe_rows),
            (RE_MIX, mix_rows),
            (RE_REFUSE, refuse_rows),
        ):
            row = regex.match(line)
            if row:
                bucket.append(row)
                break
        else:
            unidentified.append(line)

    report.add("no-fail", not fail_rows, f"{len(fail_rows)} FAIL rows")
    report.add(
        "contract/rows",
        not unidentified,
        f"{len(unidentified)} unidentified lines" + (f": {unidentified[:3]}" if unidentified else ""),
    )

    # ── citations: every evidence row ends with src=file:line ───────────────
    evidence = ok_rows + refuse_rows + points_rows + points_mat_rows + points_armor_rows
    evidence += account_rows + slot_rows + artefact_rows + recipe_rows + mix_rows
    cited = 0
    unresolved: list[str] = []
    for row in evidence:
        ref = row[-1] if isinstance(row, tuple) else None
        if ref is None:
            ref = row.group(len(row.groups()))
        text_line = cited_line(ref)
        if text_line is None:
            unresolved.append(ref)
        else:
            cited += 1
    report.add(
        "citations/resolve",
        not unresolved,
        f"{cited}/{len(evidence)} citations resolve to an existing source line"
        + (f"; unresolved: {sorted(set(unresolved))[:3]}" if unresolved else ""),
    )
    missing_src = [
        line for line in unidentified if not RE_SRC_SUFFIX.search(line)
    ]
    report.add(
        "citations/present",
        not missing_src,
        "every table row carries src=<file:line>",
    )

    # ── ROWS / CHECKS cross-check ───────────────────────────────────────────
    rows_match = None
    for line in body:
        if RE_ROWS.match(line):
            rows_match = RE_ROWS.match(line)
            break
    counts = {
        "points": len(points_rows),
        "points-mat": len(points_mat_rows),
        "points-armor": len(points_armor_rows),
        "account": len(account_rows),
        "slots": len(slot_rows),
        "artefacts": len(artefact_rows),
        "recipes": len(recipe_rows),
        "mixes": len(mix_rows),
        "refusals": len(refuse_rows),
    }
    counts["total"] = sum(counts.values())
    declared: dict[str, int] = {}
    if rows_match is None:
        report.add("rows/counts", False, "no ROWS line")
    else:
        declared = {key: int(value) for key, value in (pair.split("=") for pair in rows_match.group(1).split())}
        mismatched = {
            key: (declared.get(key), value) for key, value in counts.items() if declared.get(key) != value
        }
        report.add(
            "rows/counts",
            not mismatched,
            f"ROWS declares {declared}; parsed {counts}"
            + (f"; mismatched {mismatched}" if mismatched else ""),
        )
    info["rows"] = declared or counts

    checks_match = None
    for line in body:
        match = RE_CHECKS.match(line)
        if match:
            checks_match = match
            break
    expected_passed = len(ok_rows) + len(refuse_rows)
    if checks_match is None:
        report.add("rows/checks", False, "no CHECKS line")
    else:
        passed, failed = int(checks_match.group(1)), int(checks_match.group(2))
        report.add(
            "rows/checks",
            passed == expected_passed and failed == 0 and failed == len(fail_rows),
            f"CHECKS passed={passed} (rows {expected_passed}) failed={failed}",
        )
        info["checks"] = {"passed": passed, "failed": failed}

    # ── A · points census against the retail item table ─────────────────────
    with_points = [template for template in templates if template["points"] > 0]
    expected_ids = [template["index"] for template in with_points]
    got_ids = [int(row.group(1)) for row in points_rows]
    report.add(
        "points/census",
        got_ids == expected_ids,
        f"{len(got_ids)} POINTS rows vs {len(expected_ids)} retail templates with points > 0 "
        f"(ids {'match' if got_ids == expected_ids else 'differ'})",
    )
    points_ok = True
    for row in points_rows:
        index, base, mult, budget = (int(row.group(1)), int(row.group(2)), row.group(3), int(row.group(4)))
        template = templates[index] if index < len(templates) else None
        if (
            template is None
            or base != template["points"]
            or mult != "0.00"
            or budget != enchantment_power(base, 0.0)
        ):
            points_ok = False
            break
    report.add(
        "points/arithmetic",
        points_ok,
        "each POINTS budget = base + floor(base·0.00) at the identity material and equals the retail base",
    )

    mat_ok = len(points_mat_rows) == len(WEAPON_MATERIALS)
    longsword = templates[120]["points"]
    for position, row in enumerate(points_mat_rows):
        index, name, mult, base, budget = (
            int(row.group(1)), row.group(2), row.group(3), int(row.group(4)), int(row.group(5)),
        )
        if (
            index != 120
            or base != longsword
            or name != WEAPON_MATERIALS[position]
            or abs(float(mult) - WEAPON_MULT[position]) > 1e-9
            or budget != enchantment_power(base, WEAPON_MULT[position])
        ):
            mat_ok = False
    report.add(
        "points/mat",
        mat_ok,
        f"{len(points_mat_rows)} weapon materials × Longsword base {longsword}: budgets recompute from the frozen "
        f"multiplier array",
    )

    armor_ok = len(points_armor_rows) == len(ARMOR_MULT)
    boots = templates[108]["points"]
    for row in points_armor_rows:
        index, native, name, mult, base, budget = (
            int(row.group(1)), int(row.group(2), 16), row.group(3), row.group(4),
            int(row.group(5)), int(row.group(6)),
        )
        expected_name = ARMOR_LEATHER if native == 0x000 else WEAPON_MATERIALS[native - 0x200]
        if (
            index != 108
            or base != boots
            or native not in ARMOR_MULT
            or name != expected_name
            or abs(float(mult) - ARMOR_MULT[native]) > 1e-9
            or budget != enchantment_power(base, ARMOR_MULT[native])
        ):
            armor_ok = False
    report.add(
        "points/armor",
        armor_ok,
        f"{len(points_armor_rows)} armor materials × Boots base {boots}: Leather + 0x200..0x209 recompute",
    )

    # ── A · accounting ─────────────────────────────────────────────────────
    account_used_ok = True
    account_verdict_ok = True
    for row in account_rows:
        label, budget, sel, used, remaining, verdict = (
            row.group(1), int(row.group(2)), row.group(3), int(row.group(4)),
            int(row.group(5)), row.group(6),
        )
        costs = []
        for kind in sel.split("+"):
            costs.append(KIND_COST.get(kind))
        if any(cost is None for cost in costs):
            account_used_ok = False
            continue
        if used != sum(costs) or remaining != budget - used:
            account_used_ok = False
        rule = verdict.split(":", 1)[1] if verdict.startswith("overrun:") else verdict
        if verdict == "accept":
            if remaining < 0:
                account_verdict_ok = False
        elif verdict.startswith("overrun:"):
            if rule not in {"beyondLimit", "notEnoughGold", "noRoomInItem"}:
                account_verdict_ok = False
            if rule in {"beyondLimit", "notEnoughGold"} and remaining >= 0:
                account_verdict_ok = False
        else:
            account_verdict_ok = False
    report.add(
        "account/arithmetic",
        account_used_ok and bool(account_rows),
        f"{len(account_rows)} ACCOUNT rows: used = Σ costs, remaining = budget − used",
    )
    report.add(
        "account/verdicts",
        account_verdict_ok and any(row.group(6) == "overrun:beyondLimit" for row in account_rows),
        "accept ⇒ remaining ≥ 0; overrun:beyondLimit/notEnoughGold ⇒ remaining < 0; slots named separately",
    )

    # ── B · slots ──────────────────────────────────────────────────────────
    grid = {}
    slot_cost_ok = True
    extras = {"no-slot": 0, "artifact": 0}
    for row in slot_rows:
        template, kind, cost, budget, allowed, reason = (
            row.group(1), row.group(2), int(row.group(3)), int(row.group(4)),
            int(row.group(5)), row.group(6),
        )
        if KIND_COST.get(kind) != cost:
            slot_cost_ok = False
        if reason == "no-slot":
            extras["no-slot"] += 1
            slot_cost_ok &= allowed == 0
        elif reason == "artifact-no-budget":
            extras["artifact"] += 1
            slot_cost_ok &= allowed == 0 and budget == 0
        else:
            expected_allowed = 1 if cost < budget else 0
            expected_reason = ("side-effect" if cost < 0 else "fits") if expected_allowed else "over-budget"
            slot_cost_ok &= allowed == expected_allowed and reason == expected_reason
            grid[(template, kind)] = allowed
    expected_grid = {(template, kind) for template in SLOT_TEMPLATES for kind in KIND_COST}
    report.add(
        "slot/grid",
        set(grid) == expected_grid,
        f"{len(grid)}/{len(expected_grid)} (template, kind) verdicts for {len(SLOT_TEMPLATES)} templates × "
        f"{len(KIND_COST)} kinds",
    )
    report.add(
        "slot/rules",
        slot_cost_ok and extras == {"no-slot": 2, "artifact": 1},
        f"cost matches the frozen registry, allowed = [cost < budget], 2 no-slot + 1 artifact row ({extras})",
    )

    # ── C · artefacts ─────────────────────────────────────────────────────
    got_ids = [int(row.group(1)) for row in artefact_rows]
    expected_ids = [record["index"] for record in magic_records]
    census_ok = got_ids == expected_ids
    detail = (
        f"{len(got_ids)} ARTEFACT rows vs {len(magic_records)} MAGIC.DEF records "
        f"(ids {'0..' + str(len(expected_ids) - 1) if census_ok else 'differ'})"
    )
    name_ok = True
    budget_ok = True
    for row, record in zip(artefact_rows, magic_records):
        index, name, kind, class_name, points, effects, raw = (
            int(row.group(1)), row.group(2), int(row.group(3)), row.group(4),
            int(row.group(5)), row.group(6), row.group(7),
        )
        expected_pairs = ",".join(f"({t},{p})" for t, p in record["pairs"])
        if (
            index != record["index"]
            or name != record["name"]
            or kind != record["kind"]
            or class_name != CLASS_NAMES.get(record["kind"], "Unknown")
            or raw != expected_pairs
        ):
            name_ok = False
        if points != 0:
            budget_ok = False
        labels = effects.split("+") if effects else []
        if not labels or not all(
            label in EFFECT_LABELS or re.fullmatch(r"t-?\d+", label) for label in labels
        ):
            name_ok = False
    report.add("artefact/census", census_ok and name_ok, detail + "; name, class and raw pairs match the record bytes")
    report.add(
        "artefact/budget",
        budget_ok and bool(artefact_rows),
        "every record reports points=0 (the 62 B layout has no budget field)",
    )

    # ── D · recipes ────────────────────────────────────────────────────────
    recipe_by_key = {}
    count_ok = len(recipe_rows) == source_recipes and counts["recipes"] == source_recipes
    report.add(
        "recipe/count",
        count_ok,
        f"{len(recipe_rows)} RECIPE rows vs {source_recipes} `display_name_key` lines vs ROWS.recipes="
        f"{counts['recipes']}",
    )
    hash_ok = True
    ingredient_ok = True
    source_ok = True
    keys: set[int] = set()
    names: set[str] = set()
    for row in recipe_rows:
        name, key, hash_hex, price, effect, variant, ingredients, texture = (
            row.group(1), int(row.group(2)), row.group(3), int(row.group(4)), row.group(5),
            int(row.group(6)), row.group(7), int(row.group(8)),
        )
        ids = [int(part) for part in ingredients.split(",")]
        recomputed = recipe_hash(ids)
        if key != int(hash_hex, 16) or recomputed != key:
            hash_ok = False
        if key in keys or name in names:
            hash_ok = False
        keys.add(key)
        names.add(name)
        block = source_recipe_block(row.group(9))
        if block is None:
            source_ok = False
        else:
            if (
                block["name"] != name
                or block["price"] != price
                or block["effect"] != effect
                or block["variant"] != variant
                or block["ids"] != ids
                or block["texture"] != texture
            ):
                source_ok = False
            if recipe_hash(block["ids"]) != key:
                hash_ok = False
        for item_id in ids:
            template = templates[item_id] if 0 <= item_id < len(templates) else None
            if template is None or not template["ingredient"]:
                ingredient_ok = False
        recipe_by_key[key] = {"name": name, "price": price, "effect": effect, "variant": variant}
    report.add(
        "recipe/hash",
        hash_ok and bool(recipe_rows),
        "every key = its 0x hash = `recipe_hash(sorted ids)` (row and source ids, int32 wrap); keys and names unique",
    )
    report.add(
        "recipe/ingredients",
        ingredient_ok and bool(recipe_rows),
        "every ingredient id exists and carries the `is_ingredient` bit of its FALL.EXE template",
    )
    report.add(
        "recipe/citations",
        source_ok and bool(recipe_rows),
        "every RECIPE row cites its own `ClassicRecipe` entry and mirrors its price, effect, variant, ids and texture",
    )

    # ── E · mixing ─────────────────────────────────────────────────────────
    mix_key_ok = True
    cross_ok = True
    negatives = 0
    positives = 0
    cauldron_rows = 0
    magnitudes: dict[int, int] = {}
    for row in mix_rows:
        ids_text, effect, key, price, variant, magnitude, src = (
            row.group(1), row.group(2), int(row.group(3)), int(row.group(4)),
            int(row.group(5)), int(row.group(6)), row.group(7),
        )
        if ids_text == "(none)":
            ids: list[int] = []
        else:
            ids = [int(part) for part in re.split(r"\s*\+\s*", ids_text)]
        effective = [item for item in ids if 0 <= item < len(templates) and templates[item]["ingredient"]]
        if len(effective) != len(ids):
            cauldron_rows += 1
        if recipe_hash(effective) != key:
            mix_key_ok = False
        if effect == "NEGATIVE":
            negatives += 1
            if key in recipe_by_key or price != 0 or magnitude != 0:
                cross_ok = False
        else:
            positives += 1
            recipe = recipe_by_key.get(key)
            if recipe is None or recipe["price"] != price or recipe["effect"] != effect or magnitude < 0:
                cross_ok = False
            if key in magnitudes and magnitudes[key] != magnitude:
                cross_ok = False
            magnitudes[key] = magnitude
        if not src.endswith(("alchemy.rs:355", "alchemy.rs:369")):
            mix_key_ok = False
    report.add(
        "mix/keys",
        mix_key_ok and bool(mix_rows),
        "every key = `recipe_hash` of the ingredient ids it prints (non-ingredients filtered first)",
    )
    report.add(
        "mix/cross",
        cross_ok and positives >= 4 and negatives >= 2,
        f"{positives} recipe hits (price/effect equal to the RECIPE row, magnitude stable) and {negatives} NEGATIVE "
        f"keys absent from the table",
    )
    report.add(
        "mix/cauldron",
        cauldron_rows >= 1,
        f"{cauldron_rows} cauldron row brews through `brew_cauldron` with a non-ingredient in the pot",
    )

    # ── F · adversarial ────────────────────────────────────────────────────
    rules = {row.group(2) for row in refuse_rows}
    report.add(
        "refusal/rules",
        all(rule in rules for rule in REQUIRED_REFUSALS) and len(refuse_rows) >= 5,
        f"refusals present: {sorted(rules)}",
    )

    info["ok_rows"] = len(ok_rows)
    info["refusals"] = len(refuse_rows)
    info["negatives"] = negatives
    return report, info


# ══════════════════════════════════════════════════════════════════════════
# falsifiers: mutate a TEMP COPY of the capture, keep the digest valid
# ══════════════════════════════════════════════════════════════════════════

def redigest(lines: list[str]) -> str:
    """Replace/append the DIGEST line so it matches the preceding bytes."""
    body = [line for line in lines if not line.startswith("DIGEST ")]
    covered = "\n".join(body).encode()
    return "\n".join(body + [f"DIGEST {hashlib.sha256(covered).hexdigest()}"]) + "\n"


def _lines(text: str) -> list[str]:
    return text[:-1].split("\n") if text.endswith("\n") else text.split("\n")


def _adjust_rows(text: str, **deltas: int) -> str:
    lines = _lines(text)
    for position, line in enumerate(lines):
        match = RE_ROWS.match(line)
        if not match:
            continue
        pairs = dict(pair.split("=") for pair in match.group(1).split())
        for key, delta in deltas.items():
            pairs[key] = str(int(pairs[key]) + delta)
        lines[position] = "ROWS " + " ".join(f"{key}={value}" for key, value in pairs.items())
        break
    return "\n".join(lines)


def mut_recipe_invented(text: str) -> str:
    """Give one RECIPE row a key its ingredients cannot produce."""
    lines = _lines(text)
    for position, line in enumerate(lines):
        if line.startswith("RECIPE "):
            lines[position] = re.sub(
                r"key=-?\d+ hash=0x[0-9a-f]{8}", "key=9018 hash=0x0000233a", line, count=1
            )
            return redigest(lines)
    raise RuntimeError("no RECIPE row to mutate")


def mut_artefact_gap(text: str) -> str:
    """Drop one ARTEFACT row and keep ROWS consistent, so only the census catches it."""
    lines = _lines(text)
    for position, line in enumerate(lines):
        if line.startswith("ARTEFACT 30 "):
            del lines[position]
            return redigest(_lines(_adjust_rows("\n".join(lines), artefacts=-1, total=-1)))
    raise RuntimeError("ARTEFACT 30 not found")


def mut_points_overrun_accepted(text: str) -> str:
    """Declare an overrun as accepted."""
    lines = _lines(text)
    for position, line in enumerate(lines):
        match = RE_ACCOUNT.match(line)
        if match and int(match.group(5)) < 0 and match.group(6) == "overrun:beyondLimit":
            lines[position] = line.replace("verdict=overrun:beyondLimit", "verdict=accept")
            return redigest(lines)
    raise RuntimeError("no over-budget ACCOUNT row")


def mut_citation_empty(text: str) -> str:
    """Strip the citation from one table row."""
    lines = _lines(text)
    for position, line in enumerate(lines):
        if line.startswith("POINTS-MAT "):
            lines[position] = RE_SRC_SUFFIX.sub("", line)
            return redigest(lines)
    raise RuntimeError("no POINTS-MAT row")


def mut_slot_verdict_tamper(text: str) -> str:
    """Accept a kind whose cost exceeds the budget."""
    lines = _lines(text)
    for position, line in enumerate(lines):
        match = RE_SLOT.match(line)
        if match and match.group(6) == "over-budget":
            lines[position] = line.replace("allowed=0 reason=over-budget", "allowed=1 reason=fits")
            return redigest(lines)
    raise RuntimeError("no over-budget SLOT row")


def mut_slot_row_removed(text: str) -> str:
    """Delete one verdict from the grid."""
    lines = _lines(text)
    for position, line in enumerate(lines):
        if line.startswith("SLOT 120/steel FeatherWeight "):
            del lines[position]
            return redigest(_lines(_adjust_rows("\n".join(lines), slots=-1, total=-1)))
    raise RuntimeError("SLOT 120/steel FeatherWeight not found")


def mut_points_multiplier_tamper(text: str) -> str:
    """Change one material budget by one point."""
    lines = _lines(text)
    for position, line in enumerate(lines):
        match = RE_POINTS_MAT.match(line)
        if match and match.group(2) == "Iron":
            budget = int(match.group(5)) + 1
            lines[position] = (
                f"POINTS-MAT 120 material=Iron mult={match.group(3)} base={match.group(4)} "
                f"budget={budget} src={match.group(6)}"
            )
            return redigest(lines)
    raise RuntimeError("POINTS-MAT Iron not found")


def mut_mix_negative_hidden(text: str) -> str:
    """Hide the NEGATIVE mixing cases."""
    lines = _lines(text)
    kept = []
    removed = 0
    for line in lines:
        match = RE_MIX.match(line)
        if match and match.group(2) == "NEGATIVE":
            removed += 1
            continue
        kept.append(line)
    if not removed:
        raise RuntimeError("no NEGATIVE MIX row")
    adjusted = _adjust_rows("\n".join(kept), mixes=-removed, total=-removed)
    return redigest(_lines(adjusted))


def mut_refusal_rule_erased(text: str) -> str:
    """Erase the budget-overrun refusal."""
    lines = _lines(text)
    kept = []
    removed = 0
    for line in lines:
        match = RE_REFUSE.match(line)
        if match and match.group(2) == "beyondLimit":
            removed += 1
            continue
        kept.append(line)
    if not removed:
        raise RuntimeError("no beyondLimit REFUSE row")
    return redigest(_lines(_adjust_rows("\n".join(kept), refusals=-removed, total=-removed)))


def mut_recipe_non_ingredient(text: str) -> str:
    """Point a recipe at a template without the `is_ingredient` bit (key kept consistent)."""
    lines = _lines(text)
    for position, line in enumerate(lines):
        match = RE_RECIPE.match(line)
        if not match or match.group(1) != "stamina":
            continue
        ids = [int(part) for part in match.group(7).split(",")]
        ids[0] = 120  # Longsword: a weapon, not an ingredient
        key = recipe_hash(ids)
        lines[position] = (
            f"RECIPE {match.group(1)} key={key} hash=0x{key & 0xFFFFFFFF:08x} price={match.group(4)} "
            f"effect={match.group(5)} variant={match.group(6)} ingredients={','.join(str(i) for i in ids)} "
            f"texture={match.group(8)} src={match.group(9)}"
        )
        return redigest(lines)
    raise RuntimeError("RECIPE stamina not found")


def mut_refusal_non_ingredient_erased(text: str) -> str:
    """Erase the non-ingredient cauldron refusal."""
    lines = _lines(text)
    kept = []
    removed = 0
    for line in lines:
        match = RE_REFUSE.match(line)
        if match and match.group(2) == "non-ingredient":
            removed += 1
            continue
        kept.append(line)
    if not removed:
        raise RuntimeError("no non-ingredient REFUSE row")
    return redigest(_lines(_adjust_rows("\n".join(kept), refusals=-removed, total=-removed)))


def mut_rows_count_tamper(text: str) -> str:
    """Break the declared row census."""
    return redigest(_lines(_adjust_rows(text, total=1)))


def mut_digest_tamper(text: str) -> str:
    """Corrupt the DIGEST value without touching the bytes."""
    lines = _lines(text)
    for position, line in enumerate(lines):
        match = RE_DIGEST.match(line)
        if match:
            flipped = "0" if match.group(1)[0] != "0" else "1"
            lines[position] = f"DIGEST {flipped}{match.group(1)[1:]}"
            return "\n".join(lines) + "\n"
    raise RuntimeError("no DIGEST line")


# name -> (mutator, expected red check id prefix)
FALSIFIERS = {
    "recipe-invented": (mut_recipe_invented, "recipe/hash"),
    "artefact-gap": (mut_artefact_gap, "artefact/census"),
    "points-overrun-accepted": (mut_points_overrun_accepted, "account/verdicts"),
    "citation-empty": (mut_citation_empty, "citations/present"),
    "slot-verdict-tamper": (mut_slot_verdict_tamper, "slot/rules"),
    "slot-row-removed": (mut_slot_row_removed, "slot/grid"),
    "points-multiplier-tamper": (mut_points_multiplier_tamper, "points/mat"),
    "mix-negative-hidden": (mut_mix_negative_hidden, "mix/cross"),
    "refusal-rule-erased": (mut_refusal_rule_erased, "refusal/rules"),
    "recipe-non-ingredient": (mut_recipe_non_ingredient, "recipe/ingredients"),
    "refusal-non-ingredient-erased": (mut_refusal_non_ingredient_erased, "refusal/rules"),
    "rows-count-tamper": (mut_rows_count_tamper, "rows/counts"),
    "digest-tamper": (mut_digest_tamper, "contract/digest"),
}


# ══════════════════════════════════════════════════════════════════════════
# process handling
# ══════════════════════════════════════════════════════════════════════════

def build() -> None:
    env = os.environ.copy()
    env.setdefault("TMPDIR", str(Path.home() / ".t"))
    proc = subprocess.run(
        ["cargo", "build", "-p", "daggerfall-app"],
        cwd=ROOT,
        env=env,
        capture_output=True,
        text=True,
    )
    if proc.returncode != 0:
        tail = "\n".join((proc.stderr or proc.stdout).splitlines()[-20:])
        raise Skip(f"cargo build failed:\n{tail}")


def run_probe(run: int) -> tuple[int, bytes, bytes]:
    env = os.environ.copy()
    env["PROBE_RUN"] = str(run)
    env.setdefault("TMPDIR", str(Path.home() / ".t"))
    proc = subprocess.run(
        [str(BIN), "--probe", PROBE],
        cwd=ROOT,
        env=env,
        capture_output=True,
    )
    return proc.returncode, proc.stdout, proc.stderr


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description="SYS-CRAFT-001 gate (M41/W41-4)")
    parser.add_argument("--skip-build", action="store_true", help="use the existing target/debug/daggerfall-app")
    parser.add_argument("--app-root", default=None, help="retail install root (default $DF_APP_ROOT or ~/Games/Daggerfall/app)")
    parser.add_argument(
        "--falsify",
        default="all",
        help="falsifier name to prove (default: all); `--falsify list` prints the names",
    )
    args = parser.parse_args(argv)

    if args.falsify == "list":
        for name in FALSIFIERS:
            print(name)
        return PASS
    selected = list(FALSIFIERS) if args.falsify == "all" else [args.falsify]
    unknown = [name for name in selected if name not in FALSIFIERS]
    if unknown:
        print(f"usage: unknown falsifier(s) {unknown}; known: {sorted(FALSIFIERS)}", file=sys.stderr)
        return SKIP

    root = app_root_from(args.app_root)
    try:
        templates = load_templates(root)
        magic_bytes, magic_records = load_magic(root)
        source_recipes = recipe_source_count()
        if not BIN.is_file():
            build()
        elif not args.skip_build:
            build()
    except Skip as exc:
        print(f"SYS-CRAFT-001: SKIP - {exc}")
        return SKIP

    runs = []
    for run in (1, 2):
        code, out, err = run_probe(run)
        runs.append((code, out, err))

    problems: list[str] = []
    for index, (code, out, err) in enumerate(runs, start=1):
        if code != 0:
            problems.append(f"run{index}: exit={code} (want 0)")
        if b"VEREDICTO: SKIP" in out:
            reason = [line for line in out.decode("utf-8", "replace").splitlines() if line.startswith("reason=")]
            print(f"SYS-CRAFT-001: SKIP - probe skipped: {reason[-1] if reason else 'unknown'}")
            return SKIP
        if err.strip():
            problems.append(f"run{index}: stderr not empty: {err[:200]!r}")

    sha1 = hashlib.sha256(runs[0][1]).hexdigest()
    sha2 = hashlib.sha256(runs[1][1]).hexdigest()
    if runs[0][1] != runs[1][1]:
        problems.append("stdout is not byte-identical between the two processes")
    if sha1 != sha2:
        problems.append(f"sha256 differs between processes ({sha1} vs {sha2})")

    text = runs[0][1].decode("utf-8", "strict")
    report, info = analyze(text, templates, magic_bytes, magic_records, source_recipes)
    problems.extend(f"structure: {check_id} - {detail}" for check_id, _, detail in report.failed)

    # ── falsifiers on a temp copy ──────────────────────────────────────────
    falsifier_results = []
    falsifier_failures = 0
    tempdir = Path(tempfile.mkdtemp(prefix="sys-craft-001-"))
    try:
        clean_copy = tempdir / "run1.stdout.txt"
        clean_copy.write_bytes(runs[0][1])
        for name in selected:
            mutator, expected = FALSIFIERS[name]
            mutated = mutator(text)
            path = tempdir / f"{name}.stdout.txt"
            path.write_text(mutated)
            mutated_report, _ = analyze(mutated, templates, magic_bytes, magic_records, source_recipes)
            red = [check_id for check_id, ok, _ in mutated_report.checks if not ok]
            caught = expected in red
            # the clean capture must analyse green again
            clean_report, _ = analyze(text, templates, magic_bytes, magic_records, source_recipes)
            recovered = not clean_report.failed
            ok = caught and recovered
            falsifier_failures += 0 if ok else 1
            falsifier_results.append(
                {
                    "name": name,
                    "expected_red": expected,
                    "red": red,
                    "caught": caught,
                    "clean_green_after": recovered,
                    "ok": ok,
                }
            )
            print(
                f"  falsifier {name:26s} {'RED' if caught else 'STILL GREEN':12s} "
                f"expected={expected} red={len(red)} clean_green={recovered} -> "
                f"{'ok' if ok else 'FAIL'}"
            )
    finally:
        shutil.rmtree(tempdir, ignore_errors=True)

    # ── archive ────────────────────────────────────────────────────────────
    stamp = datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%SZ")
    archive = ARTIFACT_ROOT / stamp
    archive.mkdir(parents=True, exist_ok=True)
    (archive / "run1.stdout.txt").write_bytes(runs[0][1])
    (archive / "run2.stdout.txt").write_bytes(runs[1][1])
    (archive / "run1.stderr.txt").write_bytes(runs[0][2])
    manifest = {
        "gate": "SYS-CRAFT-001",
        "probe": PROBE,
        "cr": CR,
        "timestamp": stamp,
        "retail_root": str(root),
        "retail": {
            "fall_exe_bytes": (root / "FALL.EXE").stat().st_size,
            "magic_def_bytes": len(magic_bytes),
            "magic_records": len(magic_records),
            "templates": len(templates),
            "templates_with_points": sum(1 for template in templates if template["points"] > 0),
            "ingredients": sum(1 for template in templates if template["ingredient"]),
            "classic_recipes_in_source": source_recipes,
        },
        "runs": [
            {"run": index + 1, "exit": code, "stdout_bytes": len(out), "sha256": hashlib.sha256(out).hexdigest()}
            for index, (code, out, _) in enumerate(runs)
        ],
        "digest": info.get("digest"),
        "rows": info.get("rows"),
        "checks": info.get("checks"),
        "structural": [
            {"check": check_id, "ok": ok, "detail": detail} for check_id, ok, detail in report.checks
        ],
        "falsifiers": falsifier_results,
        "problems": problems,
    }
    (archive / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")
    # Stable pointer so the gate report stays byte-identical across processes;
    # the wall-clock stamp lives in latest.txt and in the manifest.
    (ARTIFACT_ROOT / "latest.txt").write_text(stamp + "\n")

    # ── report ─────────────────────────────────────────────────────────────
    print(f"SYS-CRAFT-001 — gate evidence (M41/W41-4, {CR})")
    print(f"  retail        {root}")
    print(
        f"  retail census FALL.EXE templates={len(templates)} "
        f"points>0={manifest['retail']['templates_with_points']} ingredients={manifest['retail']['ingredients']} · "
        f"MAGIC.DEF {len(magic_bytes)} B / {len(magic_records)} records"
    )
    print(
        f"  run1          exit={runs[0][0]} stdout={len(runs[0][1])} B sha256={sha1}"
    )
    print(
        f"  run2          exit={runs[1][0]} stdout={len(runs[1][1])} B sha256={sha2} "
        f"(identical={runs[0][1] == runs[1][1]})"
    )
    print(f"  DIGEST        {info.get('digest')} (valid)")
    rows = info.get("rows") or {}
    print("  ROWS          " + " ".join(f"{key}={value}" for key, value in rows.items()))
    checks = info.get("checks") or {}
    print(f"  CHECKS        passed={checks.get('passed')} failed={checks.get('failed')}")
    failed = report.failed
    print(f"  structure     {len(report.checks) - len(failed)}/{len(report.checks)} checks green")
    for check_id, ok, detail in report.checks:
        if not ok:
            print(f"    RED {check_id}: {detail}")
    print(f"  falsifiers    {len(selected) - falsifier_failures}/{len(selected)} red under their name")
    archive_rel = archive.relative_to(ROOT).as_posix()
    print(
        f"  captures      {archive_rel.rsplit('/', 1)[0]}/ "
        f"(run dir named by manifest.json timestamp; latest.txt)"
    )

    if problems:
        for problem in problems:
            print(f"  PROBLEM {problem}")
        print("VEREDICTO: FAIL")
        return FAIL
    print("VEREDICTO: PASS")
    return PASS


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
