#!/usr/bin/env python3
"""SYS-CONDITION-001 — gate of M43 / W43-6 (condition depth: disease/poison
census with `covered` and `NEGATIVE`; transformation and cure measured).

Acceptance (verbatim, roadmap §3.8): *disease/poison census with
`covered`/`NEGATIVE`; transformation and cure measured*.

This family is the M32 quaternity (SPEC-M32 §8): `CONDITION-001`,
`VAMPIRE-001`, `REST-001` and `DEATH-STATE-001`. All four are consumed; the
retail-fed rows of `REST-001` (block D and the `mode_a` BSA census) are marked
retail (G12) because that probe exits 2 without ARENA2.

Contract: `docs/sintergic/validation/m43-gate-contract-CR-39d656e1-1987-4c43-aa7c-149f6f96d2cb.md`
Authoring lane: Lane A (CR-39d656e1-1987-4c43-aa7c-149f6f96d2cb). Lane B MUST
NOT edit this file. Run with `python3 scripts/sys-condition-001.py`.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

from m43_gate_lib import (  # noqa: E402
    DepthRow,
    Falsifier,
    Gate,
    RowSpec,
    every,
    expect_equal,
    ids_exact,
)

CR = "CR-39d656e1-1987-4c43-aa7c-149f6f96d2cb"
SPEC = "docs/sintergic/validation/m43-gate-contract-CR-39d656e1-1987-4c43-aa7c-149f6f96d2cb.md"
CONDITIONS = "crates/skills/src/conditions.rs"
VAMPIRISM = "crates/skills/src/vampirism.rs"
REST = "crates/skills/src/rest.rs"
DEATH = "crates/skills/src/death.rs"

P1, P2, P3, P4 = "CONDITION-001", "VAMPIRE-001", "REST-001", "DEATH-STATE-001"

DISEASE_IDS = [f"D{i}" for i in range(1, 14)] + [f"P{i}" for i in range(1, 12)]
VAMPIRE_IDS = [f"V{i}" for i in range(1, 9)] + [f"Y{i}" for i in range(1, 6)] + [f"K{i}" for i in range(1, 4)]
REST_IDS = [f"R{i}" for i in range(1, 11)]
DEATH_IDS = ["X1", "X2"] + [f"B{i}" for i in range(1, 5)]
RETAIL_FILES = ["MAPS.BSA", "BLOCKS.BSA", "ARCH3D.BSA"]


def exact(label: str):
    def check(matches: list[re.Match]) -> tuple[bool, str]:
        return True, label

    return check


def digest_pair(matches: list[re.Match]) -> tuple[bool, str]:
    row = matches[0]
    same = row["d1"] == row["d2"]
    return same, f"digest={row['d1']} digest2={row['d2']} same={same}"


def cured(matches: list[re.Match]) -> tuple[bool, str]:
    row = matches[0]
    ok = row["cured"] == "1" and row["after"] == "0" and row["ended"] == "true"
    return ok, f"cured={row['cured']}/1 active_after_cure={row['after']}/0 ended={row['ended']}/true"


def census(matches: list[re.Match]) -> tuple[bool, str]:
    row = matches[0]
    ok = row["classes"] == "21" and row["infections"] == "4" and row["poisons"] == "12"
    return ok, f"disease_classes={row['classes']}/21 infections={row['infections']}/4 poisons={row['poisons']}/12"


def sun(matches: list[re.Match]) -> tuple[bool, str]:
    row = matches[0]
    ok = row["hp"] == "40" and row["dmg"] == "60"
    return ok, f"hp_after_sun={row['hp']}/40 sun_damage={row['dmg']}/60"


def rest_interrupt(matches: list[re.Match]) -> tuple[bool, str]:
    row = matches[0]
    ok = (
        row["hours"] == "3"
        and row["minutes"] == "180"
        and row["interrupted"] == "true"
        and row["msg"] == "354"
        and int(row["room"]) > int(row["hours"]) and int(row["hp"]) > 0
    )
    return ok, (
        f"hours={row['hours']}/3 minutes={row['minutes']}/180 interrupted={row['interrupted']} "
        f"msg={row['msg']}/354 hp={row['hp']} room_hours={row['room']}"
    )


def retail_files(matches: list[re.Match]) -> tuple[bool, str]:
    got = [row["file"] for row in matches]
    empty = [row["file"] for row in matches if int(row["bytes"]) <= 0]
    ok = got == RETAIL_FILES and not empty
    return ok, f"files={got} empty={','.join(empty) or 'none'}"


def negative_census(matches: list[re.Match]) -> tuple[bool, str]:
    row = matches[0]
    symbols, scanned, mentions = int(row["symbols"]), int(row["files"]), int(row["docs"])
    ok = symbols == 0 and scanned >= 350 and mentions >= 0
    return ok, f"resurrect_symbols={symbols}/0 scanned_files={scanned}/>=350 doc_mentions={mentions}"


def death_latch(matches: list[re.Match]) -> tuple[bool, str]:
    row = matches[0]
    ok = row["death"] == "true" and row["paused"] == "true" and row["reset"] == "3"
    return ok, f"death={row['death']} input_paused={row['paused']} reset_after_s={row['reset']}/3"


def condition_census(matches: list[re.Match]) -> tuple[bool, str]:
    row = matches[0]
    members = row["census"].split(";") if row["census"] else []
    distinct = len(set(members)) == len(members)
    names = [member.split(":", 1)[0] for member in members]
    ok = (
        row["diseases"] == "21"
        and row["covered"] == "21"
        and row["poisons"] == "12"
        and row["poisons_covered"] == "12"
        and len(members) == 33
        and distinct
        and len(set(names)) == 33
    )
    return ok, (
        f"diseases={row['diseases']}/21 covered={row['covered']}/21 poisons={row['poisons']}/12 "
        f"poisons_covered={row['poisons_covered']}/12 census={len(members)}/33 distinct={distinct}"
    )


def negative_rows(matches: list[re.Match]) -> tuple[bool, str]:
    ids = [row["id"] for row in matches]
    reasons = [row["reason"] for row in matches]
    ok = len(matches) >= 4 and len(set(ids)) == len(ids) and all(re.fullmatch(r"[a-z_]+", reason) for reason in reasons)
    return ok, f"rows={len(ids)}/>=4 unique={len(set(ids)) == len(ids)} ids={','.join(ids[:6]) or 'none'}"


def transform(matches: list[re.Match]) -> tuple[bool, str]:
    row = matches[0]
    ok = row["infections"] == "4" and row["transformed"] == "4" and row["mismatches"] == "0"
    return ok, f"infections={row['infections']}/4 transformed={row['transformed']}/4 mismatches={row['mismatches']}/0"


def cure(matches: list[re.Match]) -> tuple[bool, str]:
    row = matches[0]
    ok = int(row["cured"]) >= 1 and row["after"] == "0" and int(row["attempts"]) >= 2
    return ok, f"cured={row['cured']}/>=1 active_after={row['after']}/0 attempts={row['attempts']}/>=2"


ROWS = [
    RowSpec(
        "condition.header",
        r"^PROBE-CONDITION-001 - DFU conditions: diseases, contagion, poisons, cure "
        r"\(M32, CR-f716455c, REQ-L99-065 · spec §1-§2, §7\.1-§7\.2, §8\)$",
        exact("header exact"),
        "the probe announces M32 §1-§2, §7.1-§7.2, §8",
        cite="crates/app/src/m32_probe.rs:1",
        probe=P1,
    ),
    RowSpec(
        "condition.vector-rows",
        r"^\[OK\] (?P<id>[DP]\d+) - ",
        ids_exact(DISEASE_IDS),
        "D1..D13 + P1..P11 present in that order (G8: the population is enumerated)",
        cite=f"{CONDITIONS}:341",
        probe=P1,
    ),
    RowSpec(
        "condition.vector-count",
        r"^vectors=(?P<vectors>\d+) ok=(?P<ok>\d+) fail=(?P<fail>\d+)$",
        expect_equal(vectors="24", ok="24", fail="0"),
        "vectors=24 ok=24 fail=0",
        cite=f"{CONDITIONS}:341",
        probe=P1,
    ),
    RowSpec(
        "condition.lifecycle-digest",
        r"^\[B\] digest=(?P<d1>[0-9a-f]{16}) digest2=(?P<d2>[0-9a-f]{16})$",
        digest_pair,
        "the seeded disease lifecycle digests twice to the same value",
        cite=f"{CONDITIONS}:341",
        probe=P1,
    ),
    RowSpec(
        "condition.cured",
        r"^\[B\] cured=(?P<cured>\d+) active_after_cure=(?P<after>\d+) ended=(?P<ended>\w+)$",
        cured,
        "cured=1, active_after_cure=0, ended=true (the cure really ends the condition)",
        cite=f"{CONDITIONS}:494",
        probe=P1,
    ),
    RowSpec(
        "condition.census",
        r"^disease_classes=(?P<classes>\d+) infections=(?P<infections>\d+) poisons=(?P<poisons>\d+)$",
        census,
        "disease_classes=21, infections=4, poisons=12 (SPEC-M32 §8 C)",
        cite=f"{CONDITIONS}:118",
        probe=P1,
    ),
    RowSpec(
        "condition.summary",
        r"^SUMMARY PROBE-CONDITION-001: A=PASS B=PASS C=PASS ⇒ PASS$",
        exact("summary exact"),
        "the probe itself closes with A/B/C PASS",
        cite="crates/app/src/m32_probe.rs:1",
        probe=P1,
    ),
    RowSpec(
        "vampire.header",
        r"^PROBE-VAMPIRE-001 - DFU vampirism/lycanthropy/death/cure "
        r"\(M32, CR-f716455c, REQ-L99-065 · spec §4-§6, §7\.4, §8\)$",
        exact("header exact"),
        "the probe announces M32 §4-§6, §7.4, §8",
        cite="crates/app/src/m32_probe.rs:1",
        probe=P2,
    ),
    RowSpec(
        "vampire.vector-rows",
        r"^\[OK\] (?P<id>[VYK]\d+) - ",
        ids_exact(VAMPIRE_IDS),
        "V1..V8 + Y1..Y5 + K1..K3 present in that order (G8)",
        cite=f"{VAMPIRISM}:26",
        probe=P2,
    ),
    RowSpec(
        "vampire.vector-count",
        r"^vectors=(?P<vectors>\d+) ok=(?P<ok>\d+) fail=(?P<fail>\d+)$",
        expect_equal(vectors="16", ok="16", fail="0"),
        "vectors=16 ok=16 fail=0",
        cite=f"{VAMPIRISM}:26",
        probe=P2,
    ),
    RowSpec(
        "vampire.lifecycle-digest",
        r"^\[B\] digest=(?P<d1>[0-9a-f]{16}) digest2=(?P<d2>[0-9a-f]{16})$",
        digest_pair,
        "stage/morph/sun digests twice to the same value",
        cite=f"{VAMPIRISM}:26",
        probe=P2,
    ),
    RowSpec(
        "vampire.sun",
        r"^\[B\] hp_after_sun=(?P<hp>\d+) sun_damage=(?P<dmg>\d+)$",
        sun,
        "hp_after_sun=40, sun_damage=60 (20 min in sunlight = 5 ticks × 12 HP)",
        cite=f"{VAMPIRISM}:39",
        probe=P2,
    ),
    RowSpec(
        "vampire.summary",
        r"^SUMMARY PROBE-VAMPIRE-001: A=PASS B=PASS ⇒ PASS$",
        exact("summary exact"),
        "the probe itself closes with A/B PASS",
        cite="crates/app/src/m32_probe.rs:1",
        probe=P2,
    ),
    RowSpec(
        "rest.header",
        r"^PROBE-REST-001 - DFU rest: hourly recovery, interruption, rent "
        r"\(M32, CR-f716455c, REQ-L99-065 · spec §3, §7\.3, §8\)$",
        exact("header exact"),
        "the probe announces M32 §3, §7.3, §8",
        cite="crates/app/src/m32_probe.rs:1",
        probe=P3,
        retail=True,
    ),
    RowSpec(
        "rest.vector-rows",
        r"^\[OK\] (?P<id>R\d+) - ",
        ids_exact(REST_IDS),
        "R1..R10 present in that order (G8)",
        cite=f"{REST}:128",
        probe=P3,
        retail=True,
    ),
    RowSpec(
        "rest.vector-count",
        r"^vectors=(?P<vectors>\d+) ok=(?P<ok>\d+) fail=(?P<fail>\d+)$",
        expect_equal(vectors="10", ok="10", fail="0"),
        "vectors=10 ok=10 fail=0",
        cite=f"{REST}:128",
        probe=P3,
        retail=True,
    ),
    RowSpec(
        "rest.interruption",
        r"^\[B\] hours=(?P<hours>\d+) minutes=(?P<minutes>\d+) interrupted=(?P<interrupted>\w+) "
        r"msg=(?P<msg>\d+) hp=(?P<hp>\d+) room_hours=(?P<room>\d+)$",
        rest_interrupt,
        "a 12 h rest with an enemy on hour 3 stops at hours=3 minutes=180 msg=354",
        cite=f"{REST}:28",
        probe=P3,
        retail=True,
    ),
    RowSpec(
        "rest.lifecycle-digest",
        r"^\[B\] digest=(?P<d1>[0-9a-f]{16}) digest2=(?P<d2>[0-9a-f]{16})$",
        digest_pair,
        "the interrupted rest digests twice to the same value",
        cite=f"{REST}:19",
        probe=P3,
        retail=True,
    ),
    RowSpec(
        "rest.retail-files",
        r"^\[OK\] mode_a (?P<file>[A-Z0-9]+\.BSA): (?P<path>\S+) \((?P<bytes>\d+) bytes\)(?: src=\S+)?$",
        retail_files,
        "MAPS.BSA, BLOCKS.BSA, ARCH3D.BSA are readable and non-empty (ARENA2)",
        cite=f"{REST}:1",
        probe=P3,
        retail=True,
    ),
    RowSpec(
        "rest.summary",
        r"^SUMMARY PROBE-REST-001: A=PASS B=PASS D=PASS ⇒ PASS$",
        exact("summary exact"),
        "the probe itself closes with A/B/D PASS",
        cite="crates/app/src/m32_probe.rs:1",
        probe=P3,
        retail=True,
    ),
    RowSpec(
        "death.header",
        r"^PROBE-DEATH-STATE-001 - DFU player death: HP<=0, guild benefit, no revival "
        r"\(M32, CR-f716455c, REQ-L99-065 · spec §6, §8\)$",
        exact("header exact"),
        "the probe announces M32 §6 + §8",
        cite="crates/app/src/m32_probe.rs:1",
        probe=P4,
    ),
    RowSpec(
        "death.vector-rows",
        r"^\[OK\] (?P<id>[XB]\d+) ",
        ids_exact(DEATH_IDS),
        "X1, X2 + B1..B4 present in that order (G8)",
        cite=f"{DEATH}:83",
        probe=P4,
    ),
    RowSpec(
        "death.negative-census",
        r"^\[OK\] NEGATIVE - resurrect_symbols=(?P<symbols>\d+) "
        r"\(scanned_files=(?P<files>\d+), doc_mentions=(?P<docs>\d+);",
        negative_census,
        "resurrect_symbols=0 over a scanned source tree — the NEGATIVE census of SPEC-M32 §6",
        cite=f"{DEATH}:83",
        probe=P4,
    ),
    RowSpec(
        "death.latch",
        r"^\[OK\] B2 death state - death=(?P<death>\w+) input_paused=(?P<paused>\w+) "
        r"reset_after_s=(?P<reset>\d+)(?: src=\S+)?$",
        death_latch,
        "death=true, input_paused=true, reset_after_s=3",
        cite=f"{DEATH}:20",
        probe=P4,
    ),
    RowSpec(
        "death.reset",
        r"^\[OK\] B3 reset - TimeBeforeReset = 3 s ⇒ title menu(?: src=\S+)?$",
        exact("reset exact"),
        "TimeBeforeReset = 3 s returns to the title menu",
        cite=f"{DEATH}:20",
        probe=P4,
    ),
    RowSpec(
        "death.hp-restored",
        r"^\[OK\] B4 hp restored - AvoidDeath leaves 10 % of MaxHealth(?: src=\S+)?$",
        exact("avoid-death exact"),
        "the guild AvoidDeath benefit leaves 10 % of MaxHealth",
        cite=f"{DEATH}:23",
        probe=P4,
    ),
    RowSpec(
        "death.summary",
        r"^SUMMARY PROBE-DEATH-STATE-001: A=PASS B=PASS ⇒ PASS$",
        exact("summary exact"),
        "the probe itself closes with A/B PASS",
        cite="crates/app/src/m32_probe.rs:1",
        probe=P4,
    ),
]

DEPTH = [
    DepthRow(
        "CONDITION-CENSUS",
        "print `[OK] CONDITION-CENSUS - diseases=21 covered=21 poisons=12 poisons_covered=12 "
        "census=<Name:covered;...> src=<path>:<line>`: the 21 disease classes and the 12 poisons are each "
        "measured (contagion + damage path exercised) — `covered` is a measurement, not a declaration, and "
        "the census enumerates all 33 (G8)",
        r"^\[OK\] CONDITION-CENSUS - diseases=(?P<diseases>\d+) covered=(?P<covered>\d+) "
        r"poisons=(?P<poisons>\d+) poisons_covered=(?P<poisons_covered>\d+) census=(?P<census>\S+) "
        r"src=(?P<src>\S+)$",
        condition_census,
    ),
    DepthRow(
        "CONDITION-NEGATIVE",
        "print one `NEGATIVE COND-<id> reason=<reason> src=<path>:<line>` line per counter-case (>=4) and "
        "`[OK] CONDITION-NEGATIVE - count=<n> src=<path>:<line>`: the §3.8 `NEGATIVE` clause — behaviours "
        "the source does NOT have (e.g. no resurrection, no unconditional infection) are measured as absent",
        r"^NEGATIVE (?P<id>COND-[A-Za-z0-9-]+) reason=(?P<reason>[a-z_]+) src=(?P<src>\S+)$",
        negative_rows,
    ),
    DepthRow(
        "CONDITION-NEGATIVE-COUNT",
        "print `[OK] CONDITION-NEGATIVE - count=4+ src=<path>:<line>`: the count of the `NEGATIVE` rows "
        "above, so the census cannot shrink silently",
        r"^\[OK\] CONDITION-NEGATIVE - count=(?P<count>\d+) src=(?P<src>\S+)$",
        lambda matches: (int(matches[0]["count"]) >= 4, f"count={matches[0]['count']}/>=4"),
    ),
    DepthRow(
        "CONDITION-TRANSFORM",
        "print `[OK] CONDITION-TRANSFORM - infections=4 transformed=4 mismatches=0 src=<path>:<line>`: "
        "each of the 4 infection classes (Lycanthropy, Vampirism, Werewolf, Wereboar) reaches its "
        "transformed state — `transformation measured`",
        r"^\[OK\] CONDITION-TRANSFORM - infections=(?P<infections>\d+) transformed=(?P<transformed>\d+) "
        r"mismatches=(?P<mismatches>\d+) src=(?P<src>\S+)$",
        transform,
    ),
    DepthRow(
        "CONDITION-CURE",
        "print `[OK] CONDITION-CURE - cured=1+ active_after=0 attempts=2+ src=<path>:<line>`: the cure is "
        "exercised at least twice (cure and cure-again) and leaves 0 active conditions — `cure measured`",
        r"^\[OK\] CONDITION-CURE - cured=(?P<cured>\d+) active_after=(?P<after>\d+) "
        r"attempts=(?P<attempts>\d+) src=(?P<src>\S+)$",
        cure,
    ),
]

FALSIFIERS = {
    "plague-int-mask": Falsifier(
        name="plague-int-mask",
        path=CONDITIONS,
        before="stats: [1, 0, 1, 1, 1, 1, 1, 1, 1, 1, 1], min_damage: 3, max_damage: 30",
        after="stats: [1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1], min_damage: 3, max_damage: 30",
        target="condition.vector-rows",
        description="DISEASE_DATA Plague INT bit 0 -> 1 (D1 asserts INT stays untouched)",
    ),
    "condition-count": Falsifier(
        name="condition-count",
        path=CONDITIONS,
        before="pub const INFECTION_COUNT: usize = INFECTION_CLASSES.len();",
        after="pub const INFECTION_COUNT: usize = INFECTION_CLASSES.len() + 1;",
        target="condition.census",
        description=(
            "INFECTION_COUNT 4 -> 5 (disease_classes 21 -> 22, infections 4 -> 5: the census row reads "
            "the declared counts). Re-pointed 2026-10-29 (CR-1b68cf36): the old anchor "
            "`DISEASE_COUNT: usize = 21` no longer exists (Lane B2: DISEASE_COUNT=17 + INFECTION_COUNT=4), "
            "and DISEASE_COUNT is an array length (`DISEASE_DATA: [DiseaseData; DISEASE_COUNT]`), so a "
            "+1 there does not compile; INFECTION_COUNT is a derived `len()` with no array bound"
        ),
    ),
}

GATE = Gate(
    slug="sys-condition-001",
    gate_id="SYS-CONDITION-001",
    cr=CR,
    probes=[P1, P2, P3, P4],
    rows=ROWS,
    depth=DEPTH,
    falsifiers=FALSIFIERS,
    sources_hint=[CONDITIONS, VAMPIRISM, REST, DEATH],
    spec_path=SPEC,
    retail_probes=[P3],
)

if __name__ == "__main__":
    raise SystemExit(GATE.main())
