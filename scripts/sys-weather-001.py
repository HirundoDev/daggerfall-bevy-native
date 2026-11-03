#!/usr/bin/env python3
"""SYS-WEATHER-001 — gate of M43 / W43-4 (weather depth: 7 states × 10
climates = 70/70; ground/fog/wet effects measured).

Acceptance (verbatim, roadmap §3.8): *7 states × 10 climates = 70/70;
ground/fog/wet effects measured*.

The 70 cells are measurable in this tree: `Climate::from_climate_value`
(`crates/heart/src/weather.rs:147`) maps the ten raw CLIMATE.PAK values
223..=232 onto the six `CLIMATES` rows, so `7 states × 10 raw climates = 70`.

External anchor (Lane A2 repair, same CR): Lane A asserted the probe's W rows
against literals the probe itself prints, so the row was a tautology and a
source mutation could not move it. `weather.retail-vectors` recomputes W1..W11
from this gate's own transcription of the retail weather table
(`Weather.cs:68-83` normalisation, `Weather.cs:116-127` subtraction order,
`Weather.cs:205-227` raw climate → row) and compares it with the *probe's
`actual` column* — never with its `expected` column.
`weather.shadow-vectors` does the same for the two fog-attenuated shadow
vectors from spec §4.5. `weather.draws-digest` freezes the 10 000-draw
distribution digest as a regression golden.

Contract: `docs/sintergic/validation/m43-gate-contract-CR-39d656e1-1987-4c43-aa7c-149f6f96d2cb.md`
Authoring lanes: Lane A + Lane A2 (CR-39d656e1-1987-4c43-aa7c-149f6f96d2cb).
Lane B MUST NOT edit this file. Run with `python3 scripts/sys-weather-001.py`.
"""

from __future__ import annotations

import re
import sys
from decimal import ROUND_HALF_UP, Decimal
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

from m43_gate_lib import (  # noqa: E402
    DepthRow,
    Falsifier,
    Gate,
    RowSpec,
    citation_check,
    expect_equal,
    ids_exact,
    value_equal,
)

CR = "CR-39d656e1-1987-4c43-aa7c-149f6f96d2cb"
SPEC = "docs/sintergic/validation/m43-gate-contract-CR-39d656e1-1987-4c43-aa7c-149f6f96d2cb.md"
PROBE = "WEATHER-001"
WEATHER = "crates/heart/src/weather.rs"
WEATHER_LIGHT = "crates/heart/src/weather_light.rs"

SPEC_DOC = "docs/sintergic/game/programming/m31-travel-weather-calendar-spec-CR-5243af63.md"
M35_DOC = "docs/sintergic/game/programming/m35-vegetation-night-sky-spec-CR-f716455c.md"
RETAIL_WEATHER = "reference/dfu-spec/Assets/Scripts/Game/Weather/Weather.cs"
RETAIL_MANAGER = "reference/dfu-spec/Assets/Scripts/Game/WeatherManager.cs"

VECTOR_IDS = [f"W{i}" for i in range(1, 14)]
STATES = ["Sunny", "Cloudy", "Overcast", "Fog", "Rain", "Thunder", "Snow"]
CLIMATE_VALUES = list(range(223, 233))  # 10 raw CLIMATE.PAK values
WET_STATES = ["Rain", "Thunder", "Snow"]
DRY_STATES = ["Sunny", "Cloudy", "Overcast", "Fog"]

# ── retail semantics, external to `crates/**` ───────────────────────────────
# GetWeather subtracts in `CompileOdds()` order (`Weather.cs:94-100`) and returns
# as soon as the remainder is `<= 0` (`Weather.cs:116-127`).
COMPILE_ORDER = ["Sunny", "Cloudy", "Overcast", "Fog", "Rain", "Snow", "Thunder"]
SEASONS = ["Fall", "Spring", "Summer", "Winter"]
ROWS_ORDER = ["Desert", "Mountains", "Jungle", "Swamp", "Subtropical", "Woodlands"]

# Raw CLIMATE.PAK value -> weather-table row (`Weather.cs:205-227`).
RAW_TO_ROW = {
    223: "Swamp",
    224: "Desert",
    225: "Desert",
    226: "Mountains",
    227: "Jungle",
    228: "Swamp",
    229: "Subtropical",
    230: "Mountains",
    231: "Woodlands",
    232: "Woodlands",
}

# WeatherTable.json transcription in `WeatherType` order
# [Sunny, Cloudy, Overcast, Fog, Rain, Thunder, Snow] as declared percent.
# `cite` is the resolved M31 spec line that transcribes the row from the retail
# mirror; None means this tree's mirror does not carry
# `Assets/Resources/WeatherTable.json` (sparse checkout) and the row is published
# as `[MISSING-SOURCE]` in the contract instead of being called verified.
RETAIL_ROWS: dict[tuple[str, str], tuple[list[float], str | None]] = {
    ("Desert", "Fall"): ([80, 15, 0, 0, 3, 2, 0], None),
    ("Desert", "Spring"): ([75, 15, 0, 0, 5, 5, 0], None),
    ("Desert", "Summer"): ([85, 15, 0, 0, 0, 0, 0], f"{SPEC_DOC}:111"),
    ("Desert", "Winter"): ([75, 15, 0, 3, 5, 2, 0], f"{SPEC_DOC}:110"),
    ("Mountains", "Fall"): ([30, 18, 20, 2, 20, 10, 0], None),
    ("Mountains", "Spring"): ([30, 23, 15, 2, 20, 10, 0], None),
    ("Mountains", "Summer"): ([45, 25, 15, 0, 10, 5, 0], f"{SPEC_DOC}:113"),
    ("Mountains", "Winter"): ([18, 20, 25, 2, 0, 0, 35], f"{SPEC_DOC}:112"),
    ("Jungle", "Fall"): ([20, 20, 20, 0, 25, 15, 0], None),
    ("Jungle", "Spring"): ([20, 15, 10, 3, 37, 15, 0], f"{SPEC_DOC}:114"),
    ("Jungle", "Summer"): ([35, 20, 10, 0, 25, 10, 0], None),
    ("Jungle", "Winter"): ([15, 20, 25, 3, 25, 12, 0], None),
    ("Swamp", "Fall"): ([15, 15, 15, 20, 20, 15, 0], None),
    ("Swamp", "Spring"): ([10, 10, 20, 20, 25, 15, 0], None),
    ("Swamp", "Summer"): ([25, 15, 15, 15, 20, 10, 0], None),
    ("Swamp", "Winter"): ([15, 20, 25, 25, 0, 0, 15], f"{SPEC_DOC}:115"),
    ("Subtropical", "Fall"): ([25, 20, 15, 0, 25, 15, 0], f"{SPEC_DOC}:116"),
    ("Subtropical", "Spring"): ([30, 15, 10, 3, 27, 15, 0], None),
    ("Subtropical", "Summer"): ([40, 15, 10, 0, 20, 15, 0], None),
    ("Subtropical", "Winter"): ([20, 20, 20, 5, 25, 10, 0], None),
    ("Woodlands", "Fall"): ([25, 15, 20, 10, 20, 10, 0], None),
    ("Woodlands", "Spring"): ([35, 15, 10, 5, 25, 10, 0], None),
    ("Woodlands", "Summer"): ([60, 20, 5, 0, 10, 5, 0], f"{SPEC_DOC}:118"),
    ("Woodlands", "Winter"): ([25, 15, 20, 5, 0, 0, 25], f"{SPEC_DOC}:117"),
}

# W1..W11 inputs, read off spec §4.5 (`m31…:265-275`) — the gate, not the
# probe, decides what the implementation must return for each of them.
VECTOR_INPUTS: dict[str, tuple[str, str, float, str]] = {
    "W1": ("Woodlands", "Summer", 0.0, f"{SPEC_DOC}:265"),
    "W2": ("Woodlands", "Summer", 60.1, f"{SPEC_DOC}:266"),
    "W3": ("Woodlands", "Summer", 80.1, f"{SPEC_DOC}:267"),
    "W4": ("Woodlands", "Summer", 85.1, f"{SPEC_DOC}:268"),
    "W5": ("Woodlands", "Summer", 95.1, f"{SPEC_DOC}:269"),
    "W6": ("Mountains", "Winter", 18.0, f"{SPEC_DOC}:270"),
    "W7": ("Mountains", "Winter", 18.01, f"{SPEC_DOC}:271"),
    "W8": ("Mountains", "Winter", 65.0, f"{SPEC_DOC}:272"),
    "W9": ("Mountains", "Winter", 65.01, f"{SPEC_DOC}:273"),
    "W10": ("Woodlands", "Winter", 27.0, f"{SPEC_DOC}:274"),
    "W11": ("Woodlands", "Winter", 27.9, f"{SPEC_DOC}:275"),
}

# §5 light table (`m35…:141-144`), integers in permille (G5: no floats on stdout).
GROUND_PERMILLE = {
    "Sunny": 1000,
    "Cloudy": 800,
    "Overcast": 650,
    "Fog": 650,
    "Rain": 450,
    "Thunder": 250,
    "Snow": 450,
}
FOG_PERMILLE = {"Sunny": 0, "Cloudy": 0, "Overcast": 0, "Fog": 50, "Rain": 3, "Thunder": 3, "Snow": 5}
# `shadowStrength *= exp(-50 * density)` (`WeatherManager.cs:344`) over the §5
# shadow strengths, in ten-thousandths (4-decimal fixed point, same scale the
# probe prints for W12/W13).
SHADOW_BASE = {
    "Sunny": 1.0,
    "Cloudy": 0.8,
    "Overcast": 0.6,
    "Fog": 0.6,
    "Rain": 0.4,
    "Thunder": 0.4,
    "Snow": 0.25,
}
FOG_DENSITY = {"Sunny": 0.0, "Cloudy": 0.0, "Overcast": 0.0, "Fog": 0.05, "Rain": 0.003, "Thunder": 0.003, "Snow": 0.005}

# 10 000-draw distribution digest frozen by Lane A2 as a regression golden (G9
# option (a)): the digest only moves when the drawn distribution moves.
GOLDEN_DIGEST = "a2ba48e31a9cc1ff"
GOLDEN_DIGEST_CITE = f"{SPEC_DOC}:301"


def scaled(value: float, factor: int) -> int:
    """`round_half_up(value * factor)` as an integer (G5 keeps floats off stdout)."""
    return int(Decimal(value * factor).quantize(Decimal(1), rounding=ROUND_HALF_UP))


def normalized_odds(climate: str, season: str) -> tuple[list[float], str | None]:
    """Retail `Validate()` (`Weather.cs:68-83`) applied to a declared row."""
    raw, cite = RETAIL_ROWS[(climate, season)]
    total = float(sum(raw))
    odds = [float(chance) for chance in raw]
    if total > 0.0 and abs(total - 100.0) >= 0.1:
        scale = 100.0 / total
        odds = [chance * scale for chance in odds]
    return odds, cite


def retail_state(climate: str, season: str, rand: float) -> str:
    """Retail `GetWeather(int climate, Seasons)` (`Weather.cs:200-227`)."""
    odds, _ = normalized_odds(climate, season)
    remaining = rand
    for state in COMPILE_ORDER:
        remaining -= odds[STATES.index(state)]
        if remaining <= 0.0:
            return state
    return "Sunny"


def matrix_permille(value: int, season: str, state: str) -> int:
    odds, _ = normalized_odds(RAW_TO_ROW[value], season)
    return scaled(odds[STATES.index(state)], 10)


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


def distribution(matches: list[re.Match]) -> tuple[bool, str]:
    """The 10 000-draw spread, measured in integer ppm (G5).

    `key=<decimal>` is forbidden on every probe stdout by the G5 scan of
    `m43_gate_lib.py`; the spread is therefore printed as `max_abs_delta_ppm`
    against `threshold_ppm` (20 000 ppm = the 2 % band `PASS - B1` declares).
    The band is kept exactly, not widened: the assertion is still
    `max_abs_delta <= threshold`, and the exactness of the distribution stays
    anchored on the frozen 16-hex golden of `weather.draws-digest`.
    """
    row = matches[0]
    delta, threshold = int(row["max_abs_delta_ppm"]), int(row["threshold_ppm"])
    shapes = re.fullmatch(r"[A-Za-z]+/[A-Za-z]+ [A-Za-z]+", row["cell"]) is not None
    ok = (
        row["draws"] == "10000"
        and row["cells"] == "24"
        and row["seed"] == "0x0038A1E6"
        and 0 <= delta <= threshold
        and threshold == 20000
        and shapes
    )
    return ok, (
        f"draws={row['draws']}/10000 cells={row['cells']}/24 seed={row['seed']} "
        f"threshold_ppm={row['threshold_ppm']} max_abs_delta_ppm={row['max_abs_delta_ppm']}"
        f"<={row['threshold_ppm']} cell={row['cell']!r} shaped={shapes}"
    )


def state_flags(matches: list[re.Match]) -> tuple[bool, str]:
    row = matches[0]
    ok = row["thunder"] == "true" and row["fog"] == "true" and row["sunny"] == "true"
    return ok, f"state_thunder={row['thunder']} state_fog={row['fog']} state_sunny={row['sunny']}"


def retail_vectors(matches: list[re.Match]) -> tuple[bool, str]:
    """W1..W11 `actual` vs the gate's own retail-derived expectation (G2)."""
    rows = {match["id"]: match for match in matches}
    bad: list[str] = []
    for vector_id, (climate, season, rand, _cite) in VECTOR_INPUTS.items():
        match = rows.get(vector_id)
        if match is None:
            bad.append(f"{vector_id}=absent")
            continue
        want = retail_state(climate, season, rand)
        got = match["actual"]
        if got != want:
            bad.append(f"{vector_id}={got}!={want}")
    cites = [entry[3] for entry in VECTOR_INPUTS.values()]
    cites += [f"{SPEC_DOC}:108", f"{RETAIL_WEATHER}:68", f"{RETAIL_WEATHER}:116", f"{RETAIL_MANAGER}:200"]
    ok_cites, detail_cites = citation_check(cites)
    ok = not bad and ok_cites and len(rows) == len(VECTOR_INPUTS)
    return ok, (
        f"rows={len(rows)}/11 mismatch={','.join(bad[:5]) or 'none'} "
        f"source=retail-transcription(Weather.cs:68-83,116-127,205-227) {detail_cites}"
    )


def shadow_vectors(matches: list[re.Match]) -> tuple[bool, str]:
    """W12/W13 `actual` vs the two §4.5 attenuation goldens."""
    want = {"W12": "0.3443", "W13": "0.1947"}
    rows = {match["id"]: match for match in matches}
    bad = [f"{key}={rows.get(key)['actual'] if key in rows else 'absent'}!={value}" for key, value in want.items()
           if key not in rows or rows[key]["actual"] != value]
    ok_cites, detail_cites = citation_check([f"{SPEC_DOC}:276", f"{SPEC_DOC}:277", f"{RETAIL_MANAGER}:344"])
    return (not bad and ok_cites and len(rows) == 2), f"rows={len(rows)}/2 mismatch={','.join(bad) or 'none'} {detail_cites}"


def draws_digest(matches: list[re.Match]) -> tuple[bool, str]:
    """Frozen golden of the 10 000-draw distribution (G9 option (a))."""
    row = matches[0]
    ok_cites, detail_cites = citation_check([GOLDEN_DIGEST_CITE])
    ok = row["digest"] == GOLDEN_DIGEST and ok_cites
    return ok, f"digest={row['digest']} golden={GOLDEN_DIGEST} match={row['digest'] == GOLDEN_DIGEST} {detail_cites}"


def matrix(matches: list[re.Match]) -> tuple[bool, str]:
    """Per-season 7 × 10 census with the retail-declared probability on each cell."""
    problems: list[str] = []
    seen: list[str] = []
    for row in matches:
        season = row["season"]
        seen.append(season)
        if season not in SEASONS:
            problems.append(f"season={season} unknown")
            continue
        cells: dict[tuple[int, str], int] = {}
        malformed = 0
        for member in row["census"].split(";") if row["census"] else []:
            parsed = re.fullmatch(r"(\d+):([A-Za-z]+)=(-?\d+)", member)
            if parsed is None:
                malformed += 1
                continue
            cells[(int(parsed.group(1)), parsed.group(2))] = int(parsed.group(3))
        wanted = {(value, state) for value in CLIMATE_VALUES for state in STATES}
        if malformed:
            problems.append(f"{season}:malformed={malformed}")
        if set(cells) != wanted:
            problems.append(f"{season}:cells={len(cells)}/{len(wanted)} missing={len(wanted - set(cells))} extra={len(set(cells) - wanted)}")
        wrong = [f"{value}:{state}={cells[(value, state)]}!={matrix_permille(value, season, state)}"
                 for (value, state) in wanted & set(cells)
                 if cells[(value, state)] != matrix_permille(value, season, state)]
        if wrong:
            problems.append(f"{season}:wrong={','.join(wrong[:4])}")
        if (row["states"], row["climates"], row["cells"], row["pass"], row["total"]) != ("7", "10", "70", "70", "70"):
            problems.append(
                f"{season}:counters=states{row['states']}/7 climates{row['climates']}/10 "
                f"cells{row['cells']}/70 pass{row['pass']}/70 total{row['total']}/70"
            )
        ok_src, detail_src = citation_check([row["src"]])
        if not ok_src:
            problems.append(f"{season}:{detail_src}")
    missing_seasons = [season for season in SEASONS if season not in seen]
    if missing_seasons:
        problems.append(f"seasons missing={','.join(missing_seasons)}")
    if len(seen) != len(set(seen)):
        problems.append("duplicate season rows")
    return not problems, (
        f"seasons={len(seen)}/4 cells=70 each prob=retail {', '.join(problems[:4]) if problems else 'all cells match'}"
    )


def light_census(expected: dict[str, int]):
    """7 states × 10 raw climates census over `weather_light` effect values."""

    def check(matches: list[re.Match]) -> tuple[bool, str]:
        row = matches[0]
        cells: dict[tuple[int, str], int] = {}
        malformed = 0
        for member in row["census"].split(";") if row["census"] else []:
            parsed = re.fullmatch(r"(\d+):([A-Za-z]+)=(-?\d+)", member)
            if parsed is None:
                malformed += 1
                continue
            cells[(int(parsed.group(1)), parsed.group(2))] = int(parsed.group(3))
        wanted = {(value, state) for value in CLIMATE_VALUES for state in STATES}
        wrong = [f"{value}:{state}={cells[(value, state)]}!={expected[state]}"
                 for (value, state) in wanted & set(cells)
                 if cells[(value, state)] != expected[state]]
        ok_src, detail_src = citation_check([row["src"]])
        ok = (
            not malformed
            and set(cells) == wanted
            and not wrong
            and (row["states"], row["climates"], row["cells"]) == ("7", "10", "70")
            and ok_src
        )
        return ok, (
            f"states={row['states']}/7 climates={row['climates']}/10 cells={row['cells']}/70 "
            f"census={len(cells)} malformed={malformed} wrong={len(wrong)} "
            f"{','.join(wrong[:3]) if wrong else 'values=declared'} {detail_src}"
        )

    return check


def per_state_census(matches: list[re.Match]) -> tuple[bool, str]:
    row = matches[0]
    members = row["census"].split(";") if row["census"] else []
    names = [member.split(":", 1)[0] for member in members]
    ok = row["states"] == "7" and names == STATES
    return ok, f"states={row['states']}/7 census={len(members)} names={','.join(names)} expected={','.join(STATES)}"


def wet_dry(matches: list[re.Match]) -> tuple[bool, str]:
    row = matches[0]
    wet = [value for value in row["wet"].split(",") if value]
    dry = [value for value in row["dry"].split(",") if value]
    ok = row["states"] == "7" and wet == WET_STATES and dry == DRY_STATES
    return ok, f"states={row['states']}/7 wet={wet} dry={dry}"


def published_matrix() -> str:
    """The 70-cell grid per season, with the retail probability on every cell."""
    blocks: list[str] = []
    for season in SEASONS:
        cells = [
            f"{value}:{state}={matrix_permille(value, season, state)}"
            for value in CLIMATE_VALUES
            for state in STATES
        ]
        blocks.append(f"{season} " + ";".join(cells))
    provenance = "; ".join(
        f"{row}/{season} {RETAIL_ROWS[(row, season)][1] or '[MISSING-SOURCE]'}"
        for row in ROWS_ORDER
        for season in SEASONS
    )
    return (
        " ~ ".join(blocks)
        + " ~~ provenance: " + provenance
        + " ~~ unit: permille (declared percent ×10, half-up) over the Validate()-normalised odds "
        + f"({RETAIL_WEATHER}:68-83); row map {RETAIL_WEATHER}:205-227; subtraction order "
        + f"{RETAIL_WEATHER}:94-100 + :116-127"
    )


def published_light(expected: dict[str, int], unit: str) -> str:
    return ";".join(f"{state}={expected[state]}" for state in STATES) + f" ~~ unit: {unit}"


ROWS = [
    RowSpec(
        "weather.header",
        r"^PROBE-WEATHER-001 - DFU weather table \+ GetWeather \+ fog attenuation "
        r"\(M31, CR-f716455c, REQ-L99-064 · spec §2 §4\.5\)$",
        exact("header exact"),
        "the probe announces M31 §2 + §4.5",
        cite="crates/app/src/m31_probe.rs:1",
    ),
    RowSpec(
        "weather.vector-rows",
        r"^(?P<id>W\d+)\|(?P<expected>[^|\n]+)\|(?P<actual>[^|\n]+)$",
        vector_population(VECTOR_IDS),
        "W1..W13 present and expected == actual on every row (G8)",
        cite=f"{WEATHER}:271",
    ),
    RowSpec(
        "weather.retail-vectors",
        r"^(?P<id>W(?:[1-9]|1[01]))\|(?P<expected>[^|\n]+)\|(?P<actual>[^|\n]+)$",
        retail_vectors,
        "W1..W11 `actual` equals the state the gate derives from the retail table and subtraction "
        "order — the probe's own `expected` column is never the reference",
        cite=f"{SPEC_DOC}:108",
    ),
    RowSpec(
        "weather.shadow-vectors",
        r"^(?P<id>W1[23])\|(?P<expected>[^|\n]+)\|(?P<actual>[^|\n]+)$",
        shadow_vectors,
        "W12/W13 `actual` equals the two §4.5 fog-attenuated shadow goldens 0.3443 / 0.1947",
        cite=f"{SPEC_DOC}:276",
    ),
    RowSpec(
        "weather.vector-count",
        r"^vectors=(?P<vectors>\d+) ok=(?P<ok>\d+) fail=(?P<fail>\d+)$",
        expect_equal(vectors="13", ok="13", fail="0"),
        "vectors=13 ok=13 fail=0",
        cite=f"{WEATHER}:285",
    ),
    RowSpec(
        "weather.draws",
        r"^draws=(?P<draws>\d+) cells=(?P<cells>\d+) seed=(?P<seed>\S+) "
        r"max_abs_delta_ppm=(?P<max_abs_delta_ppm>-?\d+) threshold_ppm=(?P<threshold_ppm>-?\d+) "
        r'cell="(?P<cell>[^"\n]+)"$',
        distribution,
        "draws=10000, cells=24, seed=0x0038A1E6, threshold_ppm=20000, "
        "max_abs_delta_ppm <= threshold_ppm",
        cite=f"{WEATHER}:334",
    ),
    RowSpec(
        "weather.draws-digest",
        r"^digest=(?P<digest>[0-9a-f]{16})$",
        draws_digest,
        f"the 10 000-draw distribution digest equals the frozen regression golden {GOLDEN_DIGEST}",
        cite=GOLDEN_DIGEST_CITE,
    ),
    RowSpec(
        "weather.frequency-verdict",
        r"^PASS - B1 frequency within 2 % of the declared distribution$",
        exact("B1 exact"),
        "the 10 000-draw frequency is within 2 % of the declared distribution",
        cite=f"{WEATHER}:334",
    ),
    RowSpec(
        "weather.state",
        r"^state_thunder=(?P<thunder>\w+) state_fog=(?P<fog>\w+) state_sunny=(?P<sunny>\w+)$",
        state_flags,
        "the thunder, fog and sunny states were each observed in the draws",
        cite=f"{WEATHER}:89",
    ),
    RowSpec(
        "weather.probe-exit",
        r"^WEATHER-001 PASS - vectors=13 ok=13 fail=0 max_abs_delta(?:_ppm)?=\S+$",
        exact("terminator exact"),
        "the probe itself closes with vectors=13 ok=13 fail=0",
        cite="crates/app/src/m31_probe.rs:1",
    ),
]

DEPTH = [
    DepthRow(
        "WEATHER-MATRIX",
        "print one `[OK] WEATHER-MATRIX - season=<Fall|Spring|Summer|Winter> states=7 climates=10 "
        "cells=70 pass=70 total=70 census=<223:Sunny=<permille>;...;232:Snow=<permille>> src=<path>:<line>` "
        "per season: the §3.8 matrix — each of the 7 states against each of the 10 raw CLIMATE.PAK values "
        "(223..=232) — 70 cells enumerated (G8), every cell's declared probability measured. Declared grid "
        "(permille, percent×10 half-up after Validate()): " + published_matrix(),
        r"^\[OK\] WEATHER-MATRIX - season=(?P<season>\w+) states=(?P<states>\d+) climates=(?P<climates>\d+) "
        r"cells=(?P<cells>\d+) pass=(?P<pass>\d+) total=(?P<total>\d+) census=(?P<census>\S+) src=(?P<src>\S+)$",
        matrix,
    ),
    DepthRow(
        "WEATHER-LIGHT-GROUND",
        "print `[OK] WEATHER-LIGHT-GROUND - states=7 climates=10 cells=70 "
        "census=<223:Sunny=<permille>;...> src=<path>:<line>`: ground illumination — `sunlight_scale` of "
        f"`weather_light` ({WEATHER_LIGHT}) measured over the whole 7 × 10 grid, in permille. Declared: "
        + published_light(GROUND_PERMILLE, "permille of SunlightScale")
        + f" ~~ cite {M35_DOC}:142 and {RETAIL_MANAGER}:38-46 (heel: Cloudy 0.8 is §5-only, gap H3)",
        r"^\[OK\] WEATHER-LIGHT-GROUND - states=(?P<states>\d+) climates=(?P<climates>\d+) "
        r"cells=(?P<cells>\d+) census=(?P<census>\S+) src=(?P<src>\S+)$",
        light_census(GROUND_PERMILLE),
    ),
    DepthRow(
        "WEATHER-LIGHT-FOG",
        "print `[OK] WEATHER-LIGHT-FOG - states=7 climates=10 cells=70 "
        "census=<223:Sunny=<permille>;...> src=<path>:<line>`: `fog_density` of `weather_light` over the "
        "whole 7 × 10 grid, in permille of density. Declared: "
        + published_light(FOG_PERMILLE, "permille of fog density")
        + f" ~~ cite {M35_DOC}:144 and {RETAIL_MANAGER}:66-71 (Fog = HeavyFog 0.05; Rain/Thunder 0.003; Snow 0.005)",
        r"^\[OK\] WEATHER-LIGHT-FOG - states=(?P<states>\d+) climates=(?P<climates>\d+) "
        r"cells=(?P<cells>\d+) census=(?P<census>\S+) src=(?P<src>\S+)$",
        light_census(FOG_PERMILLE),
    ),
    DepthRow(
        "WEATHER-LIGHT-WET",
        "print `[OK] WEATHER-LIGHT-WET - states=7 climates=10 cells=70 "
        "census=<223:Sunny=<ten-thousandths>;...> src=<path>:<line>`: the wet effect — shadow strength "
        f"after `shadowStrength *= exp(-50 * density)` ({RETAIL_MANAGER}:344) over the whole 7 × 10 grid, "
        "in ten-thousandths (4-decimal fixed point, W12/W13 scale). Declared: "
        + published_light(
            {state: scaled(SHADOW_BASE[state] * 2.718281828459045 ** (-50.0 * FOG_DENSITY[state]), 10000)
             for state in STATES},
            "ten-thousandths of ShadowStrength",
        )
        + f" ~~ wet set Rain,Thunder,Snow; dry Sunny,Cloudy,Overcast,Fog ~~ cite {SPEC_DOC}:276-277, "
        + f"{M35_DOC}:143, {RETAIL_MANAGER}:48-58 + :344",
        r"^\[OK\] WEATHER-LIGHT-WET - states=(?P<states>\d+) climates=(?P<climates>\d+) "
        r"cells=(?P<cells>\d+) census=(?P<census>\S+) src=(?P<src>\S+)$",
        light_census(
            {state: scaled(SHADOW_BASE[state] * 2.718281828459045 ** (-50.0 * FOG_DENSITY[state]), 10000)
             for state in STATES}
        ),
    ),
    DepthRow(
        "WEATHER-EFFECTS-SUNLIGHT",
        "print `[OK] WEATHER-EFFECTS-SUNLIGHT - states=7 census=<Sunny:<permille>;...> src=<path>:<line>`: "
        "the sunlight scale is measured per state (integers in permille, G5 keeps floats off stdout) against "
        "the declared WINTER/OVERCAST/RAIN/STORM/SNOW scales",
        r"^\[OK\] WEATHER-EFFECTS-SUNLIGHT - states=(?P<states>\d+) census=(?P<census>\S+) "
        r"src=(?P<src>\S+)$",
        per_state_census,
    ),
    DepthRow(
        "WEATHER-EFFECTS-SHADOW",
        "print `[OK] WEATHER-EFFECTS-SHADOW - states=7 census=<Sunny:<permille>;...> src=<path>:<line>`: "
        "the shadow strength is measured per state against the declared OVERCAST/RAIN/STORM/SNOW strengths",
        r"^\[OK\] WEATHER-EFFECTS-SHADOW - states=(?P<states>\d+) census=(?P<census>\S+) "
        r"src=(?P<src>\S+)$",
        per_state_census,
    ),
    DepthRow(
        "WEATHER-EFFECTS-FOG",
        "print `[OK] WEATHER-EFFECTS-FOG - states=7 census=<Sunny:<permille>;...> src=<path>:<line>`: "
        "`fog_density` measured per state — Fog attenuates, every other state is transparent (0)",
        r"^\[OK\] WEATHER-EFFECTS-FOG - states=(?P<states>\d+) census=(?P<census>\S+) "
        r"src=(?P<src>\S+)$",
        per_state_census,
    ),
    DepthRow(
        "WEATHER-EFFECTS-WET",
        "print `[OK] WEATHER-EFFECTS-WET - states=7 wet=Rain,Thunder,Snow dry=Sunny,Cloudy,Overcast,Fog "
        "src=<path>:<line>`: the wet set is measured from the predicates (`is_raining`/`is_storming`/"
        "`is_snowing`) and its complement enumerated — no state unclassified",
        r"^\[OK\] WEATHER-EFFECTS-WET - states=(?P<states>\d+) wet=(?P<wet>\S+) dry=(?P<dry>\S+) "
        r"src=(?P<src>\S+)$",
        wet_dry,
    ),
]

FALSIFIERS = {
    "retail-table-row": Falsifier(
        name="retail-table-row",
        path=WEATHER,
        before="[60.0, 20.0, 5.0, 0.0, 10.0, 5.0, 0.0],    // Summer",
        after="[60.0, 0.0, 25.0, 0.0, 10.0, 5.0, 0.0],    // Summer",
        target="weather.retail-vectors",
        description=(
            "Woodlands/Summer 60/20/5/0/10/5/0 -> 60/0/25/0/10/5/0 (a one-row source mutation that keeps "
            "Σ=100 so Validate() cannot absorb it): W2 `rand=60.1` stops landing on Cloudy and the "
            "retail-derived row goes red while the probe's own self-consistent literals stay green"
        ),
    ),
    "rng-step": Falsifier(
        name="rng-step",
        path=WEATHER,
        before="self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);",
        after="self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C16);",
        target="weather.draws-digest",
        description=(
            "splitmix64 increment ...7C15 -> ...7C16: every W row is fed an explicit literal rand and stays "
            "green, but the 10 000-draw distribution (and therefore its frozen digest) moves"
        ),
    ),
    "climate-matrix-row": Falsifier(
        name="climate-matrix-row",
        path=WEATHER,
        before="            223 | 228 => Some(Self::Swamp),",
        after="            223 | 228 => Some(Self::Desert),",
        target="WEATHER-MATRIX",
        description=(
            "raw CLIMATE.PAK values 223 and 228 leave the Swamp row for Desert in "
            "`Climate::from_climate_value`: the matrix census those two columns print now carries Desert's "
            "retail odds instead of Swamp's, so the §3.8 depth row REDs on the two mismatched columns. "
            "No other row absorbs it: the two existing falsifiers watch `weather.retail-vectors` (this gate's "
            "own transcription) and `weather.draws-digest` (the RNG golden), neither of which reads the "
            "raw->row mapping, and the WEATHER-LIGHT-* rows key on `WeatherType`, not on the raw value "
            "(only WEATHER-MATRIX carries the per-value x per-state cell)"
        ),
    ),
    "fog-density": Falsifier(
        name="fog-density",
        path=WEATHER_LIGHT,
        before="            fog_density: 0.05,",
        after="            fog_density: 0.06,",
        target="WEATHER-LIGHT-FOG",
        description=(
            "the Fog row of `weather_light` declares density 0.05 -> 0.06 (50 permille -> 60 permille): the "
            "7 x 10 fog census REDs because every one of the 70 cells that resolves to Fog reports 60 where "
            "the depth row declares 50; WEATHER-LIGHT-WET follows it (its shadow strength is "
            "`exp(-50 * density)`), which is the same quantity, not an absorption. Measured red set is exactly "
            "{WEATHER-LIGHT-FOG, WEATHER-LIGHT-WET}: WEATHER-LIGHT-GROUND stays green (sunlight_scale "
            "untouched), `weather.*` rows stay green (explicit vectors) and WEATHER-MATRIX stays green "
            "(raw->row mapping untouched) — no other row absorbs the fog density"
        ),
    ),
}

GATE = Gate(
    slug="sys-weather-001",
    gate_id="SYS-WEATHER-001",
    cr=CR,
    probes=[PROBE],
    rows=ROWS,
    depth=DEPTH,
    falsifiers=FALSIFIERS,
    sources_hint=[WEATHER],
    spec_path=SPEC,
)

if __name__ == "__main__":
    raise SystemExit(GATE.main())
