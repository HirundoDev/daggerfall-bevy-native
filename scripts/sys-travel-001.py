#!/usr/bin/env python3
"""SYS-TRAVEL-001 — gate of M43 / W43-3 (travel depth: golden vectors of time
and cost per mode; a deterministic road encounter ×2).

Acceptance (verbatim, roadmap §3.8): *golden vectors of time/cost per mode;
deterministic road encounter ×2*.

Contract: `docs/sintergic/validation/m43-gate-contract-CR-39d656e1-1987-4c43-aa7c-149f6f96d2cb.md`
Authoring lane: Lane A (CR-39d656e1-1987-4c43-aa7c-149f6f96d2cb). Lane B MUST
NOT edit this file. Run with `python3 scripts/sys-travel-001.py`.

`PROBE-TRAVEL-001` needs ARENA2 for its census block (C) and its demo (D): it
exits 2 when the retail data is absent. Those rows are marked retail (G12) and
the verdict becomes NC — never red, never green.
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
    every,
    expect_equal,
    ids_exact,
    value_equal,
)

CR = "CR-39d656e1-1987-4c43-aa7c-149f6f96d2cb"
SPEC = "docs/sintergic/validation/m43-gate-contract-CR-39d656e1-1987-4c43-aa7c-149f6f96d2cb.md"
PROBE = "TRAVEL-001"
TRAVEL = "crates/heart/src/travel.rs"

VECTOR_IDS = (
    [f"T{i}" for i in range(1, 23)]
    + [f"C{i}" for i in range(1, 6)]
    + [f"A{i}" for i in range(1, 8)]
)
MODES = ["Foot", "Horse", "Cart", "OceanFoot", "OceanShip"]
COST_MODES = ["Foot", "Horse", "Cart", "Ship"]
CENSUS_FILES = ["climate_pak", "mapsave", "woods_wld"]


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


def spiral(matches: list[re.Match]) -> tuple[bool, str]:
    row = matches[0]
    x, y, steps = int(row["x"]), int(row["y"]), int(row["steps"])
    inside = 0 <= x < 1001 and 0 <= y < 500
    ok = steps == 2200 and inside
    return ok, f"origin=({x},{y}) steps={steps}/2200 in-map={inside}"


def spiral_census(matches: list[re.Match]) -> tuple[bool, str]:
    row = matches[0]
    pixels = int(row["pixels"])
    ocean = int(row["ocean_pixels"])
    minutes = int(row["minutes"])
    pieces = int(row["pieces"])
    days = int(row["days"])
    ok = (
        pixels == 2200
        and 0 < ocean < pixels
        and minutes >= pixels
        and days == -(-minutes // 1440)
        and 0 < pieces <= pixels
    )
    return ok, (
        f"pixels={pixels}/2200 ocean={ocean}/<pixels minutes={minutes} "
        f"days={days}/ceil(minutes/1440)={-(-minutes // 1440)} pieces={pieces}"
    )


def route_digest(matches: list[re.Match]) -> tuple[bool, str]:
    row = matches[0]
    same = row["d1"] == row["d2"]
    ok = same and row["identical"] == "true" and row["in_bounds"] == "true"
    return ok, f"route_digest={row['d1']} run2={row['d2']} identical={row['identical']} in_bounds={row['in_bounds']} same={same}"


def census_files(matches: list[re.Match]) -> tuple[bool, str]:
    bad: list[str] = []
    for row in matches:
        if not (
            row["present"] == "present"
            and row["size"] == row["expected"]
            and row["read"] == "64"
            and row["ok"] == "true"
        ):
            bad.append(f"{row['file']}={row['present']}/{row['size']}!={row['expected']}")
    present = [row["file"] for row in matches]
    ok = not bad and present == CENSUS_FILES
    return ok, f"files={present} mismatched={','.join(bad[:4]) or 'none'}"


def encounter(matches: list[re.Match]) -> tuple[bool, str]:
    row = matches[0]
    same = row["d1"] == row["d2"]
    ok = same and row["identical"] == "true" and int(row["encounters"]) >= 1
    return ok, (
        f"seed={row['seed']} encounters={row['encounters']}/>=1 digest={row['d1']} "
        f"digest2={row['d2']} identical={row['identical']} same={same}"
    )


def encounter_negative(matches: list[re.Match]) -> tuple[bool, str]:
    row = matches[0]
    differ = row["da"] != row["db"]
    ok = differ and row["differs"] == "true" and row["sa"] != row["sb"]
    return ok, (
        f"seed_a={row['sa']} seed_b={row['sb']} digest_a={row['da']} digest_b={row['db']} "
        f"differ={differ} declared={row['differs']}"
    )


def mode_census(matches: list[re.Match], want: list[str], field: str) -> tuple[bool, str]:
    row = matches[0]
    ok_census, detail = census_exact(field, want)(row)
    ok = ok_census and row["cases"] == str(len(want)) and row["mismatches"] == "0"
    return ok, f"cases={row['cases']}/{len(want)} {detail} mismatches={row['mismatches']}/0"


ROWS = [
    RowSpec(
        "travel.header",
        r"^PROBE-TRAVEL-001 - DFU travel: time per pixel, trip cost, arrival "
        r"\(M31, CR-f716455c, REQ-L99-064 · spec §1 §4\.1-§4\.3 §4\.6\)$",
        exact("header exact"),
        "the probe announces M31 §1 + §4.1-§4.6",
        cite="crates/app/src/m31_probe.rs:1",
    ),
    RowSpec(
        "travel.vector-rows",
        r"^(?P<id>[TCA]\d+)\|(?P<expected>[^|\n]+)\|(?P<actual>[^|\n]+)$",
        vector_population(VECTOR_IDS),
        "T1..T22, C1..C5, A1..A7 present in that order and expected == actual on every row (G8)",
        cite=f"{TRAVEL}:177",
    ),
    RowSpec(
        "travel.vector-count",
        r"^vectors=(?P<vectors>\d+) ok=(?P<ok>\d+) fail=(?P<fail>\d+)$",
        expect_equal(vectors="34", ok="34", fail="0"),
        "vectors=34 ok=34 fail=0",
        cite=f"{TRAVEL}:268",
    ),
    RowSpec(
        "travel.spiral",
        r"^origin=\((?P<x>-?\d+),(?P<y>-?\d+)\) steps=(?P<steps>\d+)$",
        spiral,
        "origin inside the 1001×500 CLIMATE.PAK and steps=2200",
        cite=f"{TRAVEL}:177",
        retail=True,
    ),
    RowSpec(
        "travel.spiral-census",
        r"^pixels=(?P<pixels>\d+) ocean_pixels=(?P<ocean_pixels>\d+) minutes=(?P<minutes>\d+) "
        r"pieces=(?P<pieces>\d+) days=(?P<days>\d+)$",
        spiral_census,
        "pixels=2200, 0 < ocean_pixels < pixels, days == ceil(minutes/1440), 0 < pieces <= pixels",
        cite=f"{TRAVEL}:268",
        retail=True,
    ),
    RowSpec(
        "travel.route-digest",
        r"^route_digest=(?P<d1>[0-9a-f]{16}) run2=(?P<d2>[0-9a-f]{16}) "
        r"identical=(?P<identical>\w+) in_bounds=(?P<in_bounds>\w+)$",
        route_digest,
        "route_digest == run2, identical=true, in_bounds=true (the same route twice)",
        cite=f"{TRAVEL}:281",
        retail=True,
    ),
    RowSpec(
        "travel.census-files",
        r"^(?P<file>[a-z_]+)=(?P<present>\w+) size=(?P<size>\d+) expected=(?P<expected>\d+) "
        r"bytes_read=(?P<read>\d+) ok=(?P<ok>\w+)$",
        census_files,
        "climate_pak, mapsave, woods_wld: present, size == expected, bytes_read=64, ok=true",
        cite="crates/heart/src/travel.rs:1",
        retail=True,
    ),
    RowSpec(
        "travel.demo",
        r"^PASS - D1 clock, D2 gold, D3 arrival, D4 weather$",
        exact("demo exact"),
        "the retail demo passes D1..D4 (clock, gold, arrival, weather)",
        cite=f"{TRAVEL}:365",
        retail=True,
    ),
    RowSpec(
        "travel.probe-exit",
        r"^TRAVEL-001 PASS - vectors=34 ok=34 fail=0$",
        exact("terminator exact"),
        "the probe itself closes with vectors=34 ok=34 fail=0",
        cite="crates/app/src/m31_probe.rs:1",
        retail=True,
    ),
]

DEPTH = [
    DepthRow(
        "TRAVEL-MODES",
        "print `[OK] TRAVEL-MODES - cases=5 census=Foot;Horse;Cart;OceanFoot;OceanShip "
        "mismatches=0 src=<path>:<line>`: minutes-per-pixel measured per mode against the declared "
        "modifiers (256/128/192) and the two ocean constants, 0 mismatches",
        r"^\[OK\] TRAVEL-MODES - cases=(?P<cases>\d+) census=(?P<census>\S+) "
        r"mismatches=(?P<mismatches>\d+) src=(?P<src>\S+)$",
        lambda matches: mode_census(matches, MODES, "census"),
    ),
    DepthRow(
        "TRAVEL-COST-MODES",
        "print `[OK] TRAVEL-COST-MODES - cases=4 census=Foot;Horse;Cart;Ship price_mismatches=0 "
        "src=<path>:<line>`: trip cost per mode and the vessel price measured against the declared "
        "formula, 0 mismatches",
        r"^\[OK\] TRAVEL-COST-MODES - cases=(?P<cases>\d+) census=(?P<census>\S+) "
        r"price_mismatches=(?P<mismatches>\d+) src=(?P<src>\S+)$",
        lambda matches: mode_census(matches, COST_MODES, "census"),
    ),
    DepthRow(
        "TRAVEL-ENCOUNTER",
        "print `[OK] TRAVEL-ENCOUNTER - seed=<seed> encounters=1+ digest=<16 hex> digest2=<16 hex> "
        "identical=true src=<path>:<line>`: the road encounter roll over a fixed route is deterministic "
        "(the same seed twice gives the same digest) — the §3.8 `deterministic road encounter ×2` clause; "
        "the spawn itself stays M25's (`REQ-L99-058`)",
        r"^\[OK\] TRAVEL-ENCOUNTER - seed=(?P<seed>\S+) encounters=(?P<encounters>\d+) "
        r"digest=(?P<d1>[0-9a-f]{16}) digest2=(?P<d2>[0-9a-f]{16}) identical=(?P<identical>\w+) "
        r"src=(?P<src>\S+)$",
        encounter,
    ),
    DepthRow(
        "TRAVEL-ENCOUNTER-NEGATIVE",
        "print `[OK] TRAVEL-ENCOUNTER-NEGATIVE - seed_a=<a> seed_b=<b> digest_a=<16 hex> "
        "digest_b=<16 hex> differs=true src=<path>:<line>`: two different seeds give two different "
        "digests — the counter-case that proves the row above is not constant",
        r"^\[OK\] TRAVEL-ENCOUNTER-NEGATIVE - seed_a=(?P<sa>\S+) seed_b=(?P<sb>\S+) "
        r"digest_a=(?P<da>[0-9a-f]{16}) digest_b=(?P<db>[0-9a-f]{16}) differs=(?P<differs>\w+) "
        r"src=(?P<src>\S+)$",
        encounter_negative,
    ),
]

FALSIFIERS = {
    "ocean-minutes-foot": Falsifier(
        name="ocean-minutes-foot",
        path=TRAVEL,
        before="pub const OCEAN_MINUTES_FOOT: u32 = 255;",
        after="pub const OCEAN_MINUTES_FOOT: u32 = 253;",
        target="travel.vector-rows",
        description="OCEAN_MINUTES_FOOT 255 -> 253 (T11 leaves its golden value)",
    ),
}

GATE = Gate(
    slug="sys-travel-001",
    gate_id="SYS-TRAVEL-001",
    cr=CR,
    probes=[PROBE],
    rows=ROWS,
    depth=DEPTH,
    falsifiers=FALSIFIERS,
    sources_hint=[TRAVEL],
    spec_path=SPEC,
    retail_probes=[PROBE],
)

if __name__ == "__main__":
    raise SystemExit(GATE.main())
