#!/usr/bin/env python3
"""SYS-CALENDAR-001 — gate of M43 / W43-5 (calendar depth: bit-exact golden
vectors for phases and holidays; the effect of every holiday declared).

Acceptance (verbatim, roadmap §3.8): *bit-exact golden vectors for phases and
holidays; effect per holiday declared*.

Contract: `docs/sintergic/validation/m43-gate-contract-CR-39d656e1-1987-4c43-aa7c-149f6f96d2cb.md`
Authoring lane: Lane A (CR-39d656e1-1987-4c43-aa7c-149f6f96d2cb). Lane B MUST
NOT edit this file. Run with `python3 scripts/sys-calendar-001.py`.

The 53-holiday population is the same table `PROBE-BANK-001` A1 names
(`53-row table, dayOfYear <= 355`): the two gates must agree on the count.
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
    census_exact,
    expect_equal,
    ids_exact,
    value_equal,
)

CR = "CR-39d656e1-1987-4c43-aa7c-149f6f96d2cb"
SPEC = "docs/sintergic/validation/m43-gate-contract-CR-39d656e1-1987-4c43-aa7c-149f6f96d2cb.md"
PROBE = "CALENDAR-001"
CALENDAR = "crates/heart/src/calendar.rs"

VECTOR_IDS = [f"L{i}" for i in range(1, 13)] + ["SEASON", "SIGN"]
PHASES = ["New", "OneWax", "HalfWax", "ThreeWax", "Full", "ThreeWane", "HalfWane", "OneWane"]
PHASE_GROUPS = ["new", "onewax", "halfwax", "threewax", "full", "threewane", "halfwane", "onewane"]
HOLIDAY_COUNT = "53"


def exact(label: str):
    def check(matches: list[re.Match]) -> tuple[bool, str]:
        return True, label

    return check


def vector_population(want_ids: list[str]):
    ids = ids_exact(want_ids)
    equal = value_equal()

    def check(matches: list[re.Match]) -> tuple[bool, str]:
        ok_ids, detail_ids = ids(matches)
        ok_values, detail_values = equal(matches)
        return ok_ids and ok_values, f"{detail_ids} {detail_values}"

    return check


def moon_census(matches: list[re.Match]) -> tuple[bool, str]:
    row = matches[0]
    counts = {phase: int(row[group]) for phase, group in zip(PHASES, PHASE_GROUPS)}
    declared = int(row["sum"])
    total = sum(counts.values())
    ok = declared == 360 and total == 360 and all(value > 0 for value in counts.values())
    return ok, (
        f"sum={declared}/360 recount={total}/360 "
        f"all-positive={all(value > 0 for value in counts.values())} "
        f"counts={','.join(f'{k}={v}' for k, v in counts.items())}"
    )


def full_new(matches: list[re.Match]) -> tuple[bool, str]:
    row = matches[0]
    full, new = int(row["full"]), int(row["new"])
    bounds = re.fullmatch(r"(\d+)\.\.(\d+)", row["range"])
    inside = bounds is not None and int(bounds.group(1)) <= full <= int(bounds.group(2)) and int(bounds.group(1)) <= new <= int(bounds.group(2))
    ok = full >= 1 and new >= 1 and inside
    return ok, f"masser_full={full} masser_new={new} range={row['range']} inside={inside}"


def replay(matches: list[re.Match]) -> tuple[bool, str]:
    row = matches[0]
    same = row["c1"] == row["c2"]
    ok = same and row["identical"] == "true" and len(row["d"]) == 16
    return ok, f"count_digest={row['c1']} replay={row['c2']} identical={row['identical']} same={same}"


def holiday_census(matches: list[re.Match]) -> tuple[bool, str]:
    row = matches[0]
    members = row["census"].split(";") if row["census"] else []
    distinct = len(set(members)) == len(members)
    shaped = all(re.fullmatch(r"[A-Za-z_]+:\d{1,3}", member) for member in members)
    days = [int(member.split(":")[1]) for member in members if ":" in member]
    ok = (
        row["count"] == HOLIDAY_COUNT
        and len(members) == 53
        and distinct
        and shaped
        and all(0 <= day <= 355 for day in days)
    )
    return ok, (
        f"count={row['count']}/53 census={len(members)} distinct={distinct} shaped={shaped} "
        f"day<=355={all(0 <= day <= 355 for day in days)}"
    )


def holiday_vectors(matches: list[re.Match]) -> tuple[bool, str]:
    row = matches[0]
    ok = row["vectors"] == HOLIDAY_COUNT and row["ok"] == HOLIDAY_COUNT and row["fail"] == "0"
    return ok, f"vectors={row['vectors']}/53 ok={row['ok']}/53 fail={row['fail']}/0"


def holiday_effects(matches: list[re.Match]) -> tuple[bool, str]:
    row = matches[0]
    ok = (
        row["declared"] == row["holidays"]
        and row["undeclared"] == "0"
        and row["holidays"] == HOLIDAY_COUNT
    )
    return ok, f"holidays={row['holidays']}/53 declared={row['declared']} undeclared={row['undeclared']}/0"


def holiday_negative(matches: list[re.Match]) -> tuple[bool, str]:
    row = matches[0]
    ok = row["checked"] == "360" and row["false_positives"] == "0"
    return ok, f"checked={row['checked']}/360 false_positives={row['false_positives']}/0"


ROWS = [
    RowSpec(
        "calendar.header",
        r"^PROBE-CALENDAR-001 - DFU calendar: derived dates, seasons, signs, lunar phases "
        r"\(M31, CR-f716455c, REQ-L99-064 · spec §3 §4\.4\)$",
        exact("header exact"),
        "the probe announces M31 §3 + §4.4",
        cite="crates/app/src/m31_probe.rs:1",
    ),
    RowSpec(
        "calendar.vector-rows",
        r"^(?P<id>[A-Z]+\d*)\|(?P<expected>[^|\n]+)\|(?P<actual>[^|\n]+)$",
        vector_population(VECTOR_IDS),
        "L1..L12, SEASON, SIGN present in that order and expected == actual on every row (G8)",
        cite=f"{CALENDAR}:208",
    ),
    RowSpec(
        "calendar.vector-count",
        r"^vectors=(?P<vectors>\d+) ok=(?P<ok>\d+) fail=(?P<fail>\d+)$",
        expect_equal(vectors="14", ok="14", fail="0"),
        "vectors=14 ok=14 fail=0",
        cite=f"{CALENDAR}:208",
    ),
    RowSpec(
        "calendar.census-masser",
        r"^masser: New=(?P<new>\d+) OneWax=(?P<onewax>\d+) HalfWax=(?P<halfwax>\d+) "
        r"ThreeWax=(?P<threewax>\d+) Full=(?P<full>\d+) ThreeWane=(?P<threewane>\d+) "
        r"HalfWane=(?P<halfwane>\d+) OneWane=(?P<onewane>\d+) sum=(?P<sum>\d+)$",
        moon_census,
        "the 8 Masser phases are enumerated and recount to sum=360, every phase reachable",
        cite=f"{CALENDAR}:35",
    ),
    RowSpec(
        "calendar.census-secunda",
        r"^secunda: New=(?P<new>\d+) OneWax=(?P<onewax>\d+) HalfWax=(?P<halfwax>\d+) "
        r"ThreeWax=(?P<threewax>\d+) Full=(?P<full>\d+) ThreeWane=(?P<threewane>\d+) "
        r"HalfWane=(?P<halfwane>\d+) OneWane=(?P<onewane>\d+) sum=(?P<sum>\d+)$",
        moon_census,
        "the 8 Secunda phases are enumerated and recount to sum=360, every phase reachable",
        cite=f"{CALENDAR}:35",
    ),
    RowSpec(
        "calendar.full-new",
        r"^masser_full=(?P<full>\d+) masser_new=(?P<new>\d+) full_new_range=(?P<range>\S+) "
        r"PASS - B1 360 days, sum=360 per moon$",
        full_new,
        "masser_full and masser_new are both inside the declared range and B1 closes PASS",
        cite=f"{CALENDAR}:208",
    ),
    RowSpec(
        "calendar.replay",
        r"^digest=(?P<d>[0-9a-f]{16}) count_digest=(?P<c1>[0-9a-f]{16}) "
        r"replay=(?P<c2>[0-9a-f]{16}) identical=(?P<identical>\w+)$",
        replay,
        "count_digest == replay (the phase census is deterministic), identical=true",
        cite=f"{CALENDAR}:208",
    ),
    RowSpec(
        "calendar.probe-exit",
        r"^CALENDAR-001 PASS - vectors=14 ok=14 fail=0 masser_sum=360 secunda_sum=360$",
        exact("terminator exact"),
        "the probe itself closes with both moons summing 360",
        cite="crates/app/src/m31_probe.rs:1",
    ),
]

DEPTH = [
    DepthRow(
        "CALENDAR-HOLIDAYS",
        "print `[OK] CALENDAR-HOLIDAYS - count=53 census=<Name:dayOfYear;...> src=<path>:<line>`: the "
        "holiday table is enumerated in full — 53 distinct entries, every `dayOfYear` within 355 "
        "(the same population `PROBE-BANK-001` A1 consumes)",
        r"^\[OK\] CALENDAR-HOLIDAYS - count=(?P<count>\d+) census=(?P<census>\S+) src=(?P<src>\S+)$",
        holiday_census,
    ),
    DepthRow(
        "CALENDAR-HOLIDAY-VECTORS",
        "print `[OK] CALENDAR-HOLIDAY-VECTORS - vectors=53 ok=53 fail=0 src=<path>:<line>`: one bit-exact "
        "golden vector per holiday (its month/day), 0 failures",
        r"^\[OK\] CALENDAR-HOLIDAY-VECTORS - vectors=(?P<vectors>\d+) ok=(?P<ok>\d+) "
        r"fail=(?P<fail>\d+) src=(?P<src>\S+)$",
        holiday_vectors,
    ),
    DepthRow(
        "CALENDAR-HOLIDAY-EFFECTS",
        "print `[OK] CALENDAR-HOLIDAY-EFFECTS - holidays=53 declared=53 undeclared=0 src=<path>:<line>`: "
        "every holiday declares its effect (free room, shop closed, ...) — none left undeclared "
        "(the §3.8 `effect per holiday declared` clause)",
        r"^\[OK\] CALENDAR-HOLIDAY-EFFECTS - holidays=(?P<holidays>\d+) declared=(?P<declared>\d+) "
        r"undeclared=(?P<undeclared>\d+) src=(?P<src>\S+)$",
        holiday_effects,
    ),
    DepthRow(
        "CALENDAR-HOLIDAY-NEGATIVE",
        "print `[OK] CALENDAR-HOLIDAY-NEGATIVE - checked=360 false_positives=0 src=<path>:<line>`: the "
        "counter-case — none of the 360 days is flagged as a holiday unless the table declares it",
        r"^\[OK\] CALENDAR-HOLIDAY-NEGATIVE - checked=(?P<checked>\d+) "
        r"false_positives=(?P<false_positives>\d+) src=(?P<src>\S+)$",
        holiday_negative,
    ),
]

FALSIFIERS = {
    "lunar-cycle-days": Falsifier(
        name="lunar-cycle-days",
        path=CALENDAR,
        before="pub const LUNAR_CYCLE_DAYS: i64 = 32;",
        after="pub const LUNAR_CYCLE_DAYS: i64 = 33;",
        target="calendar.census-masser",
        description="LUNAR_CYCLE_DAYS 32 -> 33 (the 360-day Masser census stops summing to 360)",
    ),
}

GATE = Gate(
    slug="sys-calendar-001",
    gate_id="SYS-CALENDAR-001",
    cr=CR,
    probes=[PROBE],
    rows=ROWS,
    depth=DEPTH,
    falsifiers=FALSIFIERS,
    sources_hint=[CALENDAR],
    spec_path=SPEC,
)

if __name__ == "__main__":
    raise SystemExit(GATE.main())
