#!/usr/bin/env python3
"""SYS-ECON-001 — gate of M43 / W43-1 (economy depth: retail stock census,
haggling as a formula, displayed price == charged price).

Contract: `docs/sintergic/validation/m43-gate-contract-CR-39d656e1-1987-4c43-aa7c-149f6f96d2cb.md`
Authoring lane: Lane A (CR-39d656e1-1987-4c43-aa7c-149f6f96d2cb). The lane that
implements the depth (Lane B) MUST NOT edit this file: it is the gate it has to
pass. Run with `python3 scripts/sys-econ-001.py` (builds by default, G4).

Rows the probe already prints today are asserted as they are (A/E/B blocks,
census of the retail faction). The depth rows §3.8 demands and the probe does
not print yet come out as named `[MISSING]` rows (G11) — never as a crash, a
silent green or an exit 2 disguised as PASS.
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
    value_equal,
)

CR = "CR-39d656e1-1987-4c43-aa7c-149f6f96d2cb"
SPEC = "docs/sintergic/validation/m43-gate-contract-CR-39d656e1-1987-4c43-aa7c-149f6f96d2cb.md"
PROBE = "ECONOMY-001"

VECTOR_IDS = [f"E{i}" for i in range(1, 29)]
EVIDENCE_IDS = [f"A{i}" for i in range(1, 12)]
BUILDINGS = [
    "Alchemist",
    "Armorer",
    "Bookseller",
    "ClothingStore",
    "FurnitureStore",
    "GemStore",
    "GeneralStore",
    "PawnShop",
    "WeaponSmith",
]


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


def determinism(matches: list[re.Match]) -> tuple[bool, str]:
    row = matches[0]
    same = row["d1"] == row["d2"]
    ok = same and row["runs"] == "2" and row["identical"] == "true" and len(row["d1"]) == 16
    return ok, f"digest_seed1={row['d1']} digest_seed2={row['d2']} runs={row['runs']} identical={row['identical']} same={same}"


def regional_seeds(matches: list[re.Match]) -> tuple[bool, str]:
    row = matches[0]
    differ = row["a1"] != row["a2"]
    ok = differ and row["differ"] == "true"
    return ok, f"adj_seed1={row['a1']} adj_seed2={row['a2']} differ={differ}"


def stock_census(matches: list[re.Match]) -> tuple[bool, str]:
    row = matches[0]
    members = row["census"].split(";") if row["census"] else []
    distinct = len(set(members)) == len(members)
    shaped = all(re.fullmatch(r"[A-Za-z]+:[A-Za-z0-9+]+", member) for member in members)
    buildings = {member.split(":", 1)[0] for member in members if ":" in member}
    groups = {member.split(":", 1)[1] for member in members if ":" in member}
    ok = (
        row["buildings"] == "9"
        and row["pairs"] == "36"
        and row["groups"] == str(len(groups))
        and len(groups) >= 10
        and len(members) == 36
        and distinct
        and shaped
        and buildings == set(BUILDINGS)
        and not re.search(r"\bsrc\b", row["census"])
    )
    return ok, (
        f"buildings={row['buildings']}/9 pairs={row['pairs']}/36 groups={row['groups']} "
        f"census={len(members)} distinct={distinct} shaped={shaped} building-set-match={buildings == set(BUILDINGS)}"
    )


def haggling(matches: list[re.Match]) -> tuple[bool, str]:
    row = matches[0]
    cases = int(row["cases"])
    ok = cases >= 8 and row["formula"] == "calculate_trade_price" and row["mismatches"] == "0"
    return ok, f"cases={cases}/>=8 formula={row['formula']} mismatches={row['mismatches']}/0"


def display_charged(matches: list[re.Match]) -> tuple[bool, str]:
    row = matches[0]
    ok = row["shops"] == "9" and int(row["items"]) >= 36 and row["mismatches"] == "0"
    return ok, f"shops={row['shops']}/9 items={row['items']}/>=36 mismatches={row['mismatches']}/0"


def stock_negative(matches: list[re.Match]) -> tuple[bool, str]:
    row = matches[0]
    refused, total = int(row["refused"]), int(row["of"])
    ok = 0 < refused and total == 36
    return ok, f"refused={refused}/of={total} non-empty={refused > 0}"


ROWS = [
    RowSpec(
        "econ.header",
        r"^PROBE-ECONOMY-001 - DFU economy: prices, holidays, inn, bank, transport "
        r"\(M30, CR-f716455c, REQ-L99-063 · spec §7-§8\)$",
        exact("header exact"),
        "the probe announces M30 + its REQ anchor",
        cite="crates/app/src/m30_probe.rs:1",
    ),
    RowSpec(
        "econ.vector-rows",
        r"^(?P<id>E\d+)\|(?P<expected>-?\d+)\|(?P<actual>-?\d+)$",
        vector_population(VECTOR_IDS),
        "E1..E28 present, expected == actual on every row (G8: the population is enumerated)",
        cite="crates/behavior/src/economy.rs:406",
    ),
    RowSpec(
        "econ.vector-count",
        r"^vectors=(?P<vectors>\d+) ok=(?P<ok>\d+) fail=(?P<fail>\d+)$",
        expect_equal(vectors="28", ok="28", fail="0"),
        "vectors=28 ok=28 fail=0",
        cite="crates/app/src/m30_probe.rs:1",
    ),
    RowSpec(
        "econ.evidence-population",
        r"^\[OK\] (?P<id>A\d+) ",
        every("id", EVIDENCE_IDS),
        "the 11 evidence rows A1..A11 are present and none is unknown (G8)",
        cite="crates/behavior/src/economy.rs:129",
    ),
    RowSpec(
        "econ.negative-tampered-vector",
        r"^\[OK\] A8 negative: tampered GV detected - altering E1's expected value "
        r"must break exactly that row(?: src=(?P<src>\S+))?$",
        exact("A8 exact"),
        "a tampered golden vector is detected (negative control of the vector block)",
        cite="crates/app/src/m30_probe.rs:1",
    ),
    RowSpec(
        "econ.negative-buy-without-gold",
        r"^\[OK\] A9 negative: buy without gold - `commerce\.rs:184-217`: "
        r"atomic transition, 0 mutation on Err(?: src=(?P<src>\S+))?$",
        exact("A9 exact"),
        "a buy without gold mutates nothing",
        cite="crates/inventory/src/commerce.rs:184",
    ),
    RowSpec(
        "econ.negative-region-isolation",
        r"^\[OK\] A10 negative: region isolation - operating region 0 must not touch "
        r"region 1 \(§5 per-region accounts\)(?: src=(?P<src>\S+))?$",
        exact("A10 exact"),
        "regions are isolated (region 0 cannot touch region 1)",
        cite="crates/behavior/src/economy.rs:697",
    ),
    RowSpec(
        "econ.determinism",
        r"^digest_seed1=(?P<d1>[0-9a-f]{16}) digest_seed2=(?P<d2>[0-9a-f]{16}) "
        r"runs=(?P<runs>\d+) identical=(?P<identical>\w+)$",
        determinism,
        "digest_seed1 == digest_seed2, runs=2, identical=true",
        cite="crates/app/src/m30_probe.rs:1",
    ),
    RowSpec(
        "econ.regional-seed-live",
        r"^adj_seed1=(?P<a1>[0-9a-f]{16}) adj_seed2=(?P<a2>[0-9a-f]{16}) "
        r"seeds_differ=(?P<differ>\w+)$",
        regional_seeds,
        "adj_seed1 != adj_seed2 (the seeded regional prices are live, B2 is not tautological)",
        cite="crates/behavior/src/economy.rs:414",
    ),
    RowSpec(
        "econ.retail-faction-census",
        r"^faction_bytes=(?P<bytes>\d+)$",
        expect_equal(bytes="74221"),
        "faction_bytes=74221 (retail FACTION.TXT census)",
        cite="crates/behavior/src/economy.rs:129",
        retail=True,
    ),
    RowSpec(
        "econ.summary",
        r"^SUMMARY PROBE-ECONOMY-001: A=PASS B=PASS C=PASS D=PASS ⇒ PASS$",
        exact("summary exact"),
        "the probe itself closes with A/B/C/D PASS",
        cite="crates/app/src/m30_probe.rs:1",
        retail=True,
    ),
]

DEPTH = [
    DepthRow(
        "ECON-STOCK-CENSUS",
        "print `[OK] ECON-STOCK-CENSUS - buildings=9 groups=<distinct groups> pairs=36 "
        "census=<Building:Group+Group;...> src=<path>:<line>`: every (building, group) pair "
        "the retail `storeBuysItemType` accepts, enumerated — 9 buildings × 36 pairs, no pair "
        "missing and no pair invented (G8)",
        r"^\[OK\] ECON-STOCK-CENSUS - buildings=(?P<buildings>\d+) groups=(?P<groups>\d+) "
        r"pairs=(?P<pairs>\d+) census=(?P<census>\S+) src=(?P<src>\S+)$",
        stock_census,
    ),
    DepthRow(
        "ECON-HAGGLE-FORMULA",
        "print `[OK] ECON-HAGGLE-FORMULA - cases=8 formula=calculate_trade_price mismatches=0 "
        "src=<path>:<line>`: haggling is the declared formula (`<<6` on the buying branch only), "
        "exercised over both branches with 0 mismatches",
        r"^\[OK\] ECON-HAGGLE-FORMULA - cases=(?P<cases>\d+) formula=(?P<formula>\S+) "
        r"mismatches=(?P<mismatches>\d+) src=(?P<src>\S+)$",
        haggling,
    ),
    DepthRow(
        "ECON-DISPLAY-CHARGED",
        "print `[OK] ECON-DISPLAY-CHARGED - shops=9 items=0..36 mismatches=0 src=<path>:<line>`: "
        "the price the shop displays is the price it charges, over every shop and every item it "
        "stocks",
        r"^\[OK\] ECON-DISPLAY-CHARGED - shops=(?P<shops>\d+) items=(?P<items>\d+) "
        r"mismatches=(?P<mismatches>\d+) src=(?P<src>\S+)$",
        display_charged,
    ),
    DepthRow(
        "ECON-STOCK-NEGATIVE",
        "print `[OK] ECON-STOCK-NEGATIVE - refused=1..36 of=36 src=<path>:<line>`: a shop refuses "
        "the groups it does not accept (the counter-case of the stock census)",
        r"^\[OK\] ECON-STOCK-NEGATIVE - refused=(?P<refused>\d+) of=(?P<of>\d+) src=(?P<src>\S+)$",
        stock_negative,
    ),
]

FALSIFIERS = {
    "cost-chain-multiplier": Falsifier(
        name="cost-chain-multiplier",
        path="crates/behavior/src/economy.rs",
        before="    2 * (cost * (i64::from(shop_quality) - 10) / 100 + cost)",
        after="    3 * (cost * (i64::from(shop_quality) - 10) / 100 + cost)",
        target="econ.vector-rows",
        description="calculate_cost: `2 *` -> `3 *` (E1 200 -> 300)",
    ),
    "a8-tampered-vector-undetected": Falsifier(
        name="a8-tampered-vector-undetected",
        path="crates/behavior/src/economy.rs",
        before="    2 * (cost * (i64::from(shop_quality) - 10) / 100 + cost)",
        after="    2 * (cost * (i64::from(shop_quality) - 10) / 100 + cost) + i64::from(cost == 100 && shop_quality == 10)",
        target="econ.negative-tampered-vector",
        description="calculate_cost: +1 on the E1 slot (cost=100, q=10) -> the tampered golden vector is no longer detected (mismatches 1->0)",
    ),
    "a9-err-mutates-state": Falsifier(
        name="a9-err-mutates-state",
        path="crates/inventory/src/commerce.rs",
        before="            return Err(TradeError::NotEnoughGold);",
        after="            self.inventory.gold = self.inventory.gold.saturating_sub(price);\n            return Err(TradeError::NotEnoughGold);",
        target="econ.negative-buy-without-gold",
        description="buy: the NotEnoughGold Err path debits gold -> the refusal is no longer 0-mutation (gold 5 -> -40)",
    ),
    "a10-region-not-isolated": Falsifier(
        name="a10-region-not-isolated",
        path="crates/behavior/src/economy.rs",
        before="        self.accounts.get(region).ok_or(BankError::UnknownRegion)",
        after="        self.accounts.get(region * 0).ok_or(BankError::UnknownRegion)",
        target="econ.negative-region-isolation",
        description="BankLedger::account: `region * 0` -> every region reads region 0 (r1=500: the per-region accounts stop being isolated)",
    ),
}

GATE = Gate(
    slug="sys-econ-001",
    gate_id="SYS-ECON-001",
    cr=CR,
    probes=[PROBE],
    rows=ROWS,
    depth=DEPTH,
    falsifiers=FALSIFIERS,
    sources_hint=["crates/behavior/src/economy.rs", "crates/skills/src/formula.rs"],
    spec_path=SPEC,
    retail_probes=[PROBE],
)

if __name__ == "__main__":
    raise SystemExit(GATE.main())
