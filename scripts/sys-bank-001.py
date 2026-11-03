#!/usr/bin/env python3
"""SYS-BANK-001 — gate of M43 / W43-2 (banking depth: the 12 operations, due
dates and foreclosure measured; house and boat persisted).

Acceptance (verbatim, roadmap §3.8): *12 operations + due dates + foreclosure
measured; house/boat persisted*.

Contract: `docs/sintergic/validation/m43-gate-contract-CR-39d656e1-1987-4c43-aa7c-149f6f96d2cb.md`
Authoring lane: Lane A (CR-39d656e1-1987-4c43-aa7c-149f6f96d2cb). The lane that
implements the depth (Lane B) MUST NOT edit this file: it is the gate it has to
pass. Run with `python3 scripts/sys-bank-001.py` (builds by default, G4).

Rows `PROBE-BANK-001` prints today are asserted as they are (E29..E41, A1..A10,
B1..B3). The four depth rows §3.8 demands and the probe does not print yet come
out as named `[MISSING]` rows (G11) — never as a crash, a silent green or an
exit 2 disguised as PASS.
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
    resolve_cite,
)

CR = "CR-39d656e1-1987-4c43-aa7c-149f6f96d2cb"
SPEC = "docs/sintergic/validation/m43-gate-contract-CR-39d656e1-1987-4c43-aa7c-149f6f96d2cb.md"
PROBE = "BANK-001"
LEDGER = "crates/behavior/src/economy.rs"

VECTOR_IDS = [f"E{i}" for i in range(29, 42)]
EVIDENCE_IDS = [f"A{i}" for i in range(1, 11)]
SEQUENCE_IDS = [f"B{i}" for i in range(1, 4)]
EVENTS = "Borrow, Due, Overdue, Default, Repay"

#  The 12 operations of SPEC-M30 §5 (`Operation` table) resolved to the ledger
# methods that implement them. `account_total` is the per-region read the spec
#  closes the section with ('Accounts and houses are per region').
OPERATIONS = {
    "deposit": f"{LEDGER}:1025",
    "withdraw": f"{LEDGER}:1047",
    "deposit_loc": f"{LEDGER}:1069",
    "withdraw_loc": f"{LEDGER}:1078",
    "borrow": f"{LEDGER}:1101",
    "repay": f"{LEDGER}:1127",
    "overdue": f"{LEDGER}:1179",
    "buy_house": f"{LEDGER}:1221",
    "sell_house": f"{LEDGER}:1246",
    "buy_ship": f"{LEDGER}:1258",
    "sell_ship": f"{LEDGER}:1277",
    "account_total": f"{LEDGER}:972",
}


def exact(label: str):
    def check(matches: list[re.Match]) -> tuple[bool, str]:
        return True, label

    return check


def vector_population(want_ids: list[str]):
    ids = ids_exact(want_ids)

    def check(matches: list[re.Match]) -> tuple[bool, str]:
        ok_ids, detail_ids = ids(matches)
        bad = [m["id"] for m in matches if m["verdict"] != "ok"]
        return ok_ids and not bad, f"{detail_ids} not-ok={','.join(bad[:6]) or 'none'}"

    return check


def lifecycle(matches: list[re.Match]) -> tuple[bool, str]:
    row = matches[0]
    same = row["d1"] == row["d2"]
    ok = same and row["identical"] == "true" and row["events"] == "5"
    return ok, (
        f"state_digest={row['d1']} state_digest_2={row['d2']} identical={row['identical']} "
        f"events={row['events']}/5 same={same}"
    )


def repay(matches: list[re.Match]) -> tuple[bool, str]:
    row = matches[0]
    ok = row["loan"] == "0" and row["account"] == "0" and row["events"] == EVENTS
    return ok, f"loan={row['loan']}/0 account={row['account']}/0 events={row['events']!r} expected={EVENTS!r}"


def operations_census(matches: list[re.Match]) -> tuple[bool, str]:
    row = matches[0]
    entries = [entry for entry in row["ops"].split(";") if entry]
    bad: list[str] = []
    for entry in entries:
        _, _, cite = entry.partition(":")
        ok_cite, detail = resolve_cite(cite) if cite else (False, f"no-cite={entry}")
        if not ok_cite:
            bad.append(detail)
    declared = dict(OPERATIONS)
    got = {entry.split(":", 1)[0]: entry.split(":", 1)[1] for entry in entries}
    missing = [name for name in declared if name not in got]
    extra = [name for name in got if name not in declared]
    mismatched = [name for name in declared if name in got and got[name] != declared[name]]
    ok = (
        row["count"] == "12"
        and not missing
        and not extra
        and not mismatched
        and not bad
    )
    return ok, (
        f"count={row['count']}/12 ops={len(got)}/12 missing={','.join(missing[:6]) or 'none'} "
        f"extra={','.join(extra[:6]) or 'none'} mismatched-cite={','.join(mismatched[:4]) or 'none'} "
        f"unresolved={','.join(bad[:2]) or 'none'}"
    )


def due_dates(matches: list[re.Match]) -> tuple[bool, str]:
    row = matches[0]
    ok = int(row["cases"]) >= 3 and row["due"] == "518400" and row["mismatches"] == "0"
    return ok, f"cases={row['cases']}/>=3 due_minutes={row['due']}/518400 mismatches={row['mismatches']}/0"


def reminders(matches: list[re.Match]) -> tuple[bool, str]:
    row = matches[0]
    months = [part for part in re.split(r"[;,]", row["months"]) if part]
    ok = months == ["6", "3", "1"] and row["mismatches"] == "0"
    return ok, f"months={months} mismatches={row['mismatches']}/0"


def foreclosure(matches: list[re.Match]) -> tuple[bool, str]:
    row = matches[0]
    ok = (
        int(row["foreclosed"]) >= 1
        and row["account_after"] == "0"
        and row["owned_after"] == "false"
        and int(row["regions"]) >= 1
    )
    return ok, (
        f"regions={row['regions']}/>=1 foreclosed={row['foreclosed']}/>=1 "
        f"account_after={row['account_after']}/0 owned_after={row['owned_after']}/false"
    )


def property_persist(matches: list[re.Match]) -> tuple[bool, str]:
    row = matches[0]
    shaped = re.fullmatch(r"[a-z_]+", row["house"]) is not None and re.fullmatch(r"[a-z_]+", row["ship"]) is not None
    ok = row["roundtrip"] == "true" and shaped
    return ok, f"house={row['house']} ship={row['ship']} roundtrip={row['roundtrip']}/true shaped={shaped}"


ROWS = [
    RowSpec(
        "bank.header",
        r"^PROBE-BANK-001 - bancos: festivos, préstamo, LOC, peso, casa/barco, mora "
        r"\(M30, CR-f716455c, REQ-L99-063 · spec §5\)$",
        exact("header exact"),
        "the probe announces M30 §5 + its REQ anchor",
        cite="crates/app/src/m30_probe.rs:1",
    ),
    RowSpec(
        "bank.vector-rows",
        r"^(?P<id>E\d+)\|(?P<verdict>\w+)$",
        vector_population(VECTOR_IDS),
        "E29..E41 present and every row says `ok` (G8: the population is enumerated)",
        cite=f"{LEDGER}:688",
    ),
    RowSpec(
        "bank.vector-count",
        r"^vectors=(?P<vectors>\d+) ok=(?P<ok>\d+)$",
        expect_equal(vectors="13", ok="13"),
        "vectors=13 ok=13",
        cite=f"{LEDGER}:688",
    ),
    RowSpec(
        "bank.evidence-population",
        r"^\[OK\] (?P<id>A\d+) ",
        every("id", EVIDENCE_IDS),
        "the 10 evidence rows A1..A10 are present and none is unknown (G8)",
        cite=f"{LEDGER}:666",
    ),
    RowSpec(
        "bank.lifecycle-digest",
        r"^state_digest=(?P<d1>[0-9a-f]{16}) state_digest_2=(?P<d2>[0-9a-f]{16}) "
        r"identical=(?P<identical>\w+) events=(?P<events>\d+)$",
        lifecycle,
        "state_digest == state_digest_2, identical=true, events=5 (borrow→reminder→overdue→default→repay)",
        cite=f"{LEDGER}:1179",
    ),
    RowSpec(
        "bank.repay-outcome",
        r"^repay_outcome: loan=(?P<loan>-?\d+) account=(?P<account>-?\d+) "
        r"events=\[(?P<events>[^\]]*)\]$",
        repay,
        "loan=0 account=0 events=[Borrow, Due, Overdue, Default, Repay] (the sequence clears the debt)",
        cite=f"{LEDGER}:1127",
    ),
    RowSpec(
        "bank.sequence-population",
        r"^\[OK\] (?P<id>B\d+) ",
        every("id", SEQUENCE_IDS),
        "B1..B3 present and none unknown (G8)",
        cite=f"{LEDGER}:780",
    ),
    RowSpec(
        "bank.event-order",
        r"^\[OK\] B3 event order - expected \[Borrow, Due, Overdue, Default, Repay\] "
        r"got \[Borrow, Due, Overdue, Default, Repay\](?: src=(?P<src>\S+))?$",
        exact("event order exact"),
        "the emitted loan events are the declared order",
        cite=f"{LEDGER}:780",
    ),
    RowSpec(
        "bank.summary",
        r"^SUMMARY PROBE-BANK-001: A=PASS B=PASS ⇒ PASS$",
        exact("summary exact"),
        "the probe itself closes with A/B PASS",
        cite="crates/app/src/m30_probe.rs:1",
    ),
]

DEPTH = [
    DepthRow(
        "BANK-OPERATIONS-CENSUS",
        "print `[OK] BANK-OPERATIONS-CENSUS - count=12 ops=<op:crates/behavior/src/economy.rs:line;...> "
        "src=<path>:<line>`: the 12 SPEC-M30 §5 operations (deposit, withdraw, deposit_loc, withdraw_loc, "
        "borrow, repay, overdue, buy_house, sell_house, buy_ship, sell_ship, account_total), each resolved "
        "to the ledger method that implements it — all 12, none missing, none invented (G8)",
        r"^\[OK\] BANK-OPERATIONS-CENSUS - count=(?P<count>\d+) ops=(?P<ops>\S+) src=(?P<src>\S+)$",
        operations_census,
    ),
    DepthRow(
        "BANK-DUE-DATES",
        "print `[OK] BANK-DUE-DATES - cases=3+ due_minutes=518400 mismatches=0 src=<path>:<line>`: "
        "`due = now + LOAN_REPAY_MINUTES` (360·1440) measured over the request, the reminder and the "
        "overdue transition",
        r"^\[OK\] BANK-DUE-DATES - cases=(?P<cases>\d+) due_minutes=(?P<due>\d+) "
        r"mismatches=(?P<mismatches>\d+) src=(?P<src>\S+)$",
        due_dates,
    ),
    DepthRow(
        "BANK-REMINDERS",
        "print `[OK] BANK-REMINDERS - months=6;3;1 fired=6;3;1 mismatches=0 src=<path>:<line>`: the "
        "reminder fires exactly at the declared month crossings (`REMINDER_MONTHS`), once per crossing",
        r"^\[OK\] BANK-REMINDERS - months=(?P<months>\S+) fired=(?P<fired>\S+) "
        r"mismatches=(?P<mismatches>\d+) src=(?P<src>\S+)$",
        reminders,
    ),
    DepthRow(
        "BANK-FORECLOSURE",
        "print `[OK] BANK-FORECLOSURE - regions=1+ foreclosed=1+ account_after=0 owned_after=false "
        "src=<path>:<line>`: an unrepaid overdue loan forecloses the owned property and drains the account "
        "— measured, not asserted by prose",
        r"^\[OK\] BANK-FORECLOSURE - regions=(?P<regions>\d+) foreclosed=(?P<foreclosed>\d+) "
        r"account_after=(?P<account_after>-?\d+) owned_after=(?P<owned_after>\w+) src=(?P<src>\S+)$",
        foreclosure,
    ),
    DepthRow(
        "BANK-PROPERTY-PERSIST",
        "print `[OK] BANK-PROPERTY-PERSIST - house=<name> ship=<name> roundtrip=true src=<path>:<line>`: "
        "the bought house and boat survive a ledger round-trip (the §3.8 `house/boat persisted` clause)",
        r"^\[OK\] BANK-PROPERTY-PERSIST - house=(?P<house>[a-z_]+) ship=(?P<ship>[a-z_]+) "
        r"roundtrip=(?P<roundtrip>\w+) src=(?P<src>\S+)$",
        property_persist,
    ),
]

FALSIFIERS = {
    "house-price-multiplier": Falsifier(
        name="house-price-multiplier",
        path=LEDGER,
        before="pub const HOUSE_PRICE_MULT: f32 = 1280.0;",
        after="pub const HOUSE_PRICE_MULT: f32 = 1281.0;",
        target="bank.vector-rows",
        description="HOUSE_PRICE_MULT 1280.0 -> 1281.0 (E38/E39 house price leaves its golden value)",
    ),
    "b3-event-order-swapped": Falsifier(
        name="b3-event-order-swapped",
        path=LEDGER,
        before="            self.events.push(LoanEvent::Overdue);",
        after="            self.events.push(LoanEvent::Default);",
        target="bank.event-order",
        description="monitor_loan: the Overdue emission becomes Default -> the emitted order is no longer [Borrow, Due, Overdue, Default, Repay]",
    ),
}

GATE = Gate(
    slug="sys-bank-001",
    gate_id="SYS-BANK-001",
    cr=CR,
    probes=[PROBE],
    rows=ROWS,
    depth=DEPTH,
    falsifiers=FALSIFIERS,
    sources_hint=[LEDGER, "crates/inventory/src/commerce.rs"],
    spec_path=SPEC,
)

if __name__ == "__main__":
    raise SystemExit(GATE.main())
