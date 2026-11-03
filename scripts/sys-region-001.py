#!/usr/bin/env python3
"""SYS-REGION-001 — gate of M43 / W43-7 (region depth: price, legal reputation
and political climate per region, with state).

Acceptance (verbatim, roadmap §3.8): *price/legal reputation/political climate
per region with state*.

`REGION-001` is measured as its own probe: the four `REGION-*` depth rows carry
`probe=PROBE` and are read from the `--probe REGION-001` capture, never from the
`probes[0]` fallback (that fallback silently pointed them at `ECONOMY-001` until
lane A3C/A3D of CR-1b68cf36 fixed the binding). Until `PROBE-REGION-001` is
registered in the dispatch table, `--probe REGION-001` exits 2, so the harness
reports the gate as NC and the four rows as named `[MISSING]` with their probe
spelled out. The baselines the existing probes already print are unchanged:
`PROBE-ECONOMY-001` (E5..E8 regional adjustment, the two regional seeds, 28/28)
and `PROBE-BANK-001` (E29..E41, the per-region account ledger).

Contract: `docs/sintergic/validation/m43-gate-contract-CR-39d656e1-1987-4c43-aa7c-149f6f96d2cb.md`
Authoring lane: Lane A (CR-39d656e1-1987-4c43-aa7c-149f6f96d2cb). Lane B MUST
NOT edit this file. Run with `python3 scripts/sys-region-001.py`.
"""

from __future__ import annotations

import re
import subprocess
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

from m43_gate_lib import (  # noqa: E402
    Check,
    DepthRow,
    Falsifier,
    Gate,
    RowSpec,
    expect_equal,
    ids_exact,
    value_equal,
)

CR = "CR-39d656e1-1987-4c43-aa7c-149f6f96d2cb"
SPEC = "docs/sintergic/validation/m43-gate-contract-CR-39d656e1-1987-4c43-aa7c-149f6f96d2cb.md"
PROBE = "REGION-001"
ECON, BANK = "ECONOMY-001", "BANK-001"
LEDGER = "crates/behavior/src/economy.rs"
REGIONS = "61"  # MAPS.BSA / daggerfall_parser_rs `REGION_COUNT` (real regions 0..60)

PRICE_VECTOR_IDS = ["E5", "E6", "E7", "E8"]
BANK_VECTOR_IDS = [f"E{i}" for i in range(29, 42)]


def exact(label: str):
    def check(matches: list[re.Match]) -> tuple[bool, str]:
        return True, label

    return check


def vector_population(want_ids: list[str], pattern_field: str = "verdict"):
    ids = ids_exact(want_ids)
    equal = value_equal()

    def check(matches: list[re.Match]) -> tuple[bool, str]:
        ok_ids, detail_ids = ids(matches)
        if pattern_field == "verdict":
            bad = [row["id"] for row in matches if row["verdict"] != "ok"]
            return ok_ids and not bad, f"{detail_ids} not-ok={','.join(bad[:4]) or 'none'}"
        ok_values, detail_values = equal(matches)
        return ok_ids and ok_values, f"{detail_ids} {detail_values}"

    return check


def seeds(matches: list[re.Match]) -> tuple[bool, str]:
    row = matches[0]
    differ = row["s1"] != row["s2"]
    ok = differ and row["differs"] == "true" and len(row["s1"]) == 16
    return ok, f"adj_seed1={row['s1']} adj_seed2={row['s2']} seeds_differ={row['differs']} differ={differ}"


def region_census(matches: list[re.Match]) -> tuple[bool, str]:
    row = matches[0]
    members = row["census"].split(";") if row["census"] else []
    distinct = len(set(members)) == len(members)
    ok = (
        row["regions"] == REGIONS
        and len(members) == 61
        and distinct
        and all(re.fullmatch(r"\d+:.+", member) for member in members)
    )
    return ok, f"regions={row['regions']}/61 census={len(members)}/61 distinct={distinct}"


def political(matches: list[re.Match]) -> tuple[bool, str]:
    row = matches[0]
    members = row["census"].split(";") if row["census"] else []
    classes = {member.split(":", 1)[1] for member in members if ":" in member}
    ok = (
        row["regions"] == REGIONS
        and len(members) == 61
        and len(classes) == int(row["classes"])
        and int(row["classes"]) >= 2
    )
    return ok, f"regions={row['regions']}/61 classes={row['classes']} observed={len(classes)} census={len(members)}/61"


class RegionGate(Gate):
    """Adds the `PROBE-REGION-001` availability row next to the real capture.

    `PROBE` is in `GATE.probes`, so its capture is the one the four `REGION-*`
    rows read (exit 2 while the dispatch lacks it: NC, never a silent green).
    This row states the same requirement in one sentence with its citation."""

    def capture(self, binary: Path) -> dict:
        self._binary = binary
        return super().capture(binary)

    def probe_availability(self) -> Check:
        try:
            proc = subprocess.run(
                [str(self._binary), "--probe", PROBE],
                capture_output=True,
                timeout=120,
                check=False,
            )
            rc, out = proc.returncode, proc.stdout
        except (OSError, subprocess.SubprocessError) as exc:  # pragma: no cover - defensive
            return Check(
                "region.probe-registered", False, f"could not run {self._binary}: {exc}", missing=True
            )
        if rc == 0 and b"PROBE-REGION-001" in out:
            return Check(
                "region.probe-registered",
                True,
                f"exit=0 bytes={len(out)} (the region probe exists)",
                cite="crates/app/src/main.rs:655",
            )
        return Check(
            "region.probe-registered",
            False,
            f"`--probe {PROBE}` exits {rc} and prints no `PROBE-{PROBE}` banner: Lane B must register "
            "it in the dispatch table and print `PROBE-REGION-001 - ...` first",
            cite="crates/app/src/main.rs:655",
            missing=True,
        )

    def all_checks(self, captured):
        checks, unbound = super().all_checks(captured)
        return checks + [self.probe_availability()], unbound


ROWS = [
    RowSpec(
        "region.economy-vectors",
        r"^vectors=(?P<vectors>\d+) ok=(?P<ok>\d+) fail=(?P<fail>\d+)$",
        expect_equal(vectors="28", ok="28", fail="0"),
        "PROBE-ECONOMY-001 still closes 28/28 — the baseline the regional block must not break",
        cite=f"{LEDGER}:396",
        probe=ECON,
    ),
    RowSpec(
        "region.price-vectors",
        r"^(?P<id>E[5-8])\|(?P<expected>[^|\n]+)\|(?P<actual>[^|\n]+)$",
        vector_population(PRICE_VECTOR_IDS, "values"),
        "E5..E8 (the regional cost-adjustment vectors) still match their golden values",
        cite=f"{LEDGER}:396",
        probe=ECON,
    ),
    RowSpec(
        "region.price-state-seed",
        r"^adj_seed1=(?P<s1>[0-9a-f]{16}) adj_seed2=(?P<s2>[0-9a-f]{16}) seeds_differ=(?P<differs>\w+)$",
        seeds,
        "the live regional price state digests twice, two seeds differ (seeds_differ=true)",
        cite=f"{LEDGER}:414",
        probe=ECON,
    ),
    RowSpec(
        "region.economy-summary",
        r"^SUMMARY PROBE-ECONOMY-001: A=PASS B=PASS C=PASS D=PASS ⇒ PASS$",
        exact("economy summary exact"),
        "PROBE-ECONOMY-001 still closes A/B/C/D PASS",
        cite="crates/app/src/m30_probe.rs:1",
        probe=ECON,
    ),
    RowSpec(
        "region.bank-vectors",
        r"^(?P<id>E\d+)\|(?P<verdict>\w+)$",
        vector_population(BANK_VECTOR_IDS),
        "E29..E41 — the per-region account ledger vectors — all `ok`",
        cite=f"{LEDGER}:1025",
        probe=BANK,
    ),
    RowSpec(
        "region.bank-summary",
        r"^SUMMARY PROBE-BANK-001: A=PASS B=PASS ⇒ PASS$",
        exact("bank summary exact"),
        "PROBE-BANK-001 still closes A/B PASS",
        cite="crates/app/src/m30_probe.rs:1",
        probe=BANK,
    ),
]

DEPTH = [
    DepthRow(
        "REGION-PRICE-STATE",
        "print `[OK] REGION-PRICE-STATE - regions=61 priced=61 census=<region:price;...> "
        "src=<path>:<line>`: the seeded price of all 61 MAPS.BSA regions enumerated with its state",
        r"^\[OK\] REGION-PRICE-STATE - regions=(?P<regions>\d+) priced=(?P<priced>\d+) "
        r"census=(?P<census>\S+) src=(?P<src>\S+)$",
        lambda matches: (
            region_census(matches)[0] and matches[0]["priced"] == REGIONS,
            f"priced={matches[0]['priced']}/61 {region_census(matches)[1]}",
        ),
        probe=PROBE,
    ),
    DepthRow(
        "REGION-LEGAL-REPUTATION",
        "print `[OK] REGION-LEGAL-REPUTATION - regions=61 rated=61 census=<region:legal_rep;...> "
        "src=<path>:<line>`: M28's legal reputation enumerated per region (integers, G5 keeps floats "
        "off stdout), 61 regions rated",
        r"^\[OK\] REGION-LEGAL-REPUTATION - regions=(?P<regions>\d+) rated=(?P<rated>\d+) "
        r"census=(?P<census>\S+) src=(?P<src>\S+)$",
        lambda matches: (
            region_census(matches)[0] and matches[0]["rated"] == REGIONS,
            f"rated={matches[0]['rated']}/61 {region_census(matches)[1]}",
        ),
        probe=PROBE,
    ),
    DepthRow(
        "REGION-POLITICAL-CLIMATE",
        "print `[OK] REGION-POLITICAL-CLIMATE - regions=61 classes=<n> census=<region:class;...> "
        "src=<path>:<line>`: the political climate class of every region enumerated; the declared class "
        "count must equal the observed distinct classes (no region unclassified)",
        r"^\[OK\] REGION-POLITICAL-CLIMATE - regions=(?P<regions>\d+) classes=(?P<classes>\d+) "
        r"census=(?P<census>\S+) src=(?P<src>\S+)$",
        political,
        probe=PROBE,
    ),
    DepthRow(
        "REGION-STATE-DIGEST",
        "print `[OK] REGION-STATE-DIGEST - digest=<16 hex> digest2=<16 hex> identical=true "
        "src=<path>:<line>`: the three per-region states (price, legal reputation, political climate) "
        "digest twice to the same value — the `with state` clause of §3.8 is a measurement",
        r"^\[OK\] REGION-STATE-DIGEST - digest=(?P<d1>[0-9a-f]{16}) digest2=(?P<d2>[0-9a-f]{16}) "
        r"identical=(?P<identical>\w+) src=(?P<src>\S+)$",
        lambda matches: (
            matches[0]["d1"] == matches[0]["d2"] and matches[0]["identical"] == "true",
            f"digest={matches[0]['d1']} digest2={matches[0]['d2']} identical={matches[0]['identical']}",
        ),
        probe=PROBE,
    ),
]

FALSIFIERS = {
    "regional-adjustment-anchor": Falsifier(
        name="regional-adjustment-anchor",
        path=LEDGER,
        before="i64::from(shop_quality) - 10",
        after="i64::from(shop_quality) - 11",
        target="region.price-vectors",
        description="the regional adjustment's shop-quality anchor 10 -> 11 (E5..E8 leave their golden values)",
    ),
}

GATE = RegionGate(
    slug="sys-region-001",
    gate_id="SYS-REGION-001",
    cr=CR,
    probes=[ECON, BANK, PROBE],
    rows=ROWS,
    depth=DEPTH,
    falsifiers=FALSIFIERS,
    sources_hint=[LEDGER],
    spec_path=SPEC,
)

if __name__ == "__main__":
    raise SystemExit(GATE.main())
