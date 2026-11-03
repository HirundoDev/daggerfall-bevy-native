#!/usr/bin/env python3
"""closure-gate.py - mechanical gate for the closure rule (CR-03390267).

Reads the machine-readable block of the closure register, derives the program
state from the per-milestone rows, and fails when public prose contradicts it.
The rule itself lives in docs/sintergic/validation/CLOSURE-RULE.md.

Deterministic: two processes over the same tree print byte-identical stdout,
including REGISTER-DIGEST. No timestamps, no randomness, no network.

Usage:
    python3 scripts/closure-gate.py [--register PATH] [--handoff-index PATH] [--self-test]
Exit: 0 PASS, 1 FAIL, 2 usage.
"""
from __future__ import annotations

import argparse
import copy
import hashlib
import json
import re
import subprocess
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
DEFAULT_REGISTER = REPO / "docs/sintergic/validation/closure-register-CR-03390267.md"
FIXTURE_DIR = REPO / "scripts/ci/fixtures/closure-gate"

BEGIN = "<!-- closure-register:begin -->"
END = "<!-- closure-register:end -->"

VERDICTS = {"DONE", "PARTIAL", "PARTIAL-TAG-PENDING", "PARTIAL-TECHNICAL-CLOSURE",
            "AUSENTE", "NOT-COMPARABLE-DECLARED"}
STATUSES = {"PASS", "PARTIAL", "FAIL", "NC", "NOT-RUN", "AUSENTE"}
REPROS = {"two-process", "legacy-tagged", "legacy-single", "nc-signed"}
ACCEPTANCE = {"ACCEPTED", "TAGGED", "PENDING", "N-A"}
PUBLIC_STATES = {"DONE", "DONE-technical", "PARTIAL", "IN-PROGRESS", "NOT-STARTED"}

# ── handoff registry (INDEX.jsonl): `DONE` requires a signature by another author ──
# CR-2c301a5a: the state of a lane stops being a word its own author pronounces.
# The registry reads `DONE` only with `signed_by` + `signed_at` + `qa_evidence`;
# the signer cannot be the author of the HH-1, and the evidence must exist on disk
# and speak about the same CR. `candidato`/`PARTIAL`/`BLOCKED` require nothing.
# Closures that predate the schema are frozen in a legacy file whose digest lives
# in the register: the exemption cannot be extended without editing that digest,
# and no id dated after the cutoff may enter it (two locks, not one).
# CR-5ca7f44e: the signer must also exist. `signed_by: "REGISTRO"` (or `yo`,
# `firma-externa`, `N/A`) names no author, so a lane could seal its own DONE by
# inventing one; now the base must be `og-*` or the row is not signed.
HANDOFF_INDEX = REPO / "docs/sintergic/handoffs/INDEX.jsonl"
HANDOFF_REGISTRY_SCHEMA = "HH-1-INDEX/2"
CLOSURE_STATUSES = ("candidato", "PARTIAL", "BLOCKED", "DONE")
SIGNATURE_FIELDS = ("signed_by", "signed_at", "qa_evidence")
REGISTRY_ROW_REQUIRED = ("id", "cr", "date", "agent", "status", "path", "sha256", "schema")
ISO_DATE_RE = re.compile(r"^\d{4}-\d{2}-\d{2}$")
ISO_DATETIME_RE = re.compile(
    r"^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d+)?(?:Z|[+-]\d{2}:\d{2})$")
# CR-9f3c17a2: the lane lives in the FIRST parenthetical of the agent string, and
# prose may follow it (`og-orquestador (lane SaveV3Cierre) — cierre del ciclo`). The
# old suffix regex only stripped a parenthesis that CLOSED the string, so a tail of
# prose turned `og-l1-checklist (lane SaveV3Cierre)` into its own identity and hid a
# lane signing itself.
AGENT_PAREN_RE = re.compile(r"\(([^()]*)\)")
LANE_WORD_RE = re.compile(r"^lane\s+", re.IGNORECASE)
# A signature must name a sovereign author: an `og-*` base (`og-qa`, and
# `og-qa (lane X)` once the lane suffix is gone). `REGISTRO`, `yo`, `N/A` name
# nobody, so they cannot seal anything.
SOVEREIGN_SIGNER_RE = re.compile(r"^og-[a-z0-9][a-z0-9._-]*")
# A QA artefact may cite a change request in full or in this repository's short form.
CR_UUID_RE = re.compile(r"^(CR-[0-9a-fA-F]{8})-")

# CR-359b5d8e: M-SHELL and M-PLAY are machine-block rows, in their sequence position
# (M43 -> M44 -> M-SHELL -> M-PLAY -> M45..M48). 53 ids.
EXPECTED_IDS = ([f"M{i}" for i in range(0, 14)] + ["M13b"] + [f"M{i}" for i in range(14, 45)]
                + ["M-SHELL", "M-PLAY"] + [f"M{i}" for i in range(45, 49)] + ["E8"])

PROBE_ROW_RE = re.compile(r"^\|\s*\*\*M\d{1,2}b?\*\*\s*\|")
ROW_START_RE = re.compile(r"^\|\s*\*\*M\d")
STATE_IN_ONE_RE = re.compile(r"^\s*State in one line:", re.I)
BOLD_STATE_RE = re.compile(r"^\s*\*\*M\d")
ID_RE = re.compile(r"\bM(\d{1,2})(b)?\b")
RANGE_RE = re.compile(r"\bM(\d{1,2})\s*[-\u2013\u2014]\s*M?(\d{1,2})\b")
DONE_RE = re.compile(r"\b(DONE|closed)\b")
ABSENT_RE = re.compile(r"\b(NOT STARTED|not started|frozen|spec-only|spec only|"
                       r"not implemented|never implemented|unimplemented)\b")
TECH_RE = re.compile(r"technic", re.I)
NEG_RE = re.compile(r"\b(not|no|never|none|pending|without|does not|do not|"
                    r"has not|have not|isn't|aren't|yet)\b", re.I)
AGGREGATE_RE = re.compile(r"\b(campaign|program|programme|project)\s+(is\s+)?"
                          r"(closed|complete|DONE)\b", re.I)
PASS_SHIPPED_RE = re.compile(r"PASS-as-shipped")


class Errors:
    def __init__(self) -> None:
        self.items = []

    def add(self, code: str, detail: str) -> None:
        self.items.append({"code": code, "detail": detail})

    def codes(self):
        return [i["code"] for i in self.items]


def read_text(path: Path) -> str:
    return path.read_text(encoding="utf-8")


def load_register(path: Path):
    text = read_text(path)
    if BEGIN not in text or END not in text:
        raise SystemExit(f"closure-gate: register block markers missing in {path}")
    block = text.split(BEGIN, 1)[1].split(END, 1)[0]
    return text, json.loads(block)


def git_tags():
    try:
        out = subprocess.run(["git", "-C", str(REPO), "tag", "--list"],
                             capture_output=True, text=True, check=True).stdout
    except (OSError, subprocess.CalledProcessError):
        return set()
    return {line.strip() for line in out.splitlines() if line.strip()}


def head_sha() -> str:
    try:
        return subprocess.run(["git", "-C", str(REPO), "rev-parse", "--short", "HEAD"],
                              capture_output=True, text=True, check=True).stdout.strip()
    except (OSError, subprocess.CalledProcessError):
        return "unknown"


def resolve(rel: str) -> Path:
    return REPO / rel


def validate_schema(obj: dict, err: Errors) -> None:
    for key in ("schema", "cr", "head", "date", "reaudit", "program_closure",
                "milestones", "transversal", "prose_surface", "label_mapping",
                "handoff_registry"):
        if key not in obj:
            err.add("SCHEMA-ROOT", f"missing root key: {key}")
    if obj.get("schema") != "CLOSURE-REGISTER/1":
        err.add("SCHEMA-ROOT", f"unknown schema: {obj.get('schema')!r}")

    milestones = obj.get("milestones", [])
    ids = [m.get("id") for m in milestones]
    missing = sorted(set(EXPECTED_IDS) - set(ids))
    extra = sorted(set(ids) - set(EXPECTED_IDS))
    if missing:
        err.add("SCHEMA-IDS", f"missing milestone rows: {', '.join(missing)}")
    if extra:
        err.add("SCHEMA-IDS", f"unknown milestone rows: {', '.join(extra)}")
    if len(ids) != len(set(ids)):
        err.add("SCHEMA-IDS", "duplicate milestone rows")
    if not missing and not extra and ids != EXPECTED_IDS:
        err.add("SCHEMA-IDS", "milestone rows are not in canonical order")

    for m in milestones:
        mid = m.get("id", "?")
        if m.get("verdict") not in VERDICTS:
            err.add("VERDICT-ENUM", f"{mid}: verdict {m.get('verdict')!r} not in {sorted(VERDICTS)}")
        if m.get("acceptance") not in ACCEPTANCE:
            err.add("ACCEPTANCE-ENUM", f"{mid}: acceptance {m.get('acceptance')!r}")
        if m.get("public_state") not in PUBLIC_STATES:
            err.add("PUBLIC-STATE-ENUM", f"{mid}: public_state {m.get('public_state')!r}")
        if m.get("closure_label") not in {"gate-rows", "owner-acceptance", "none"}:
            err.add("SCHEMA-ROW", f"{mid}: closure_label {m.get('closure_label')!r}")
        for key in ("owner", "notes", "reaudit", "missing_rows", "rows", "evidence_doc"):
            if key not in m:
                err.add("SCHEMA-ROW", f"{mid}: missing field {key}")
        ev = m.get("evidence_doc", "")
        if ev and not resolve(ev).exists():
            err.add("EVIDENCE-MISSING", f"{mid}: evidence_doc does not exist: {ev}")
        for row in m.get("rows", []):
            rid = row.get("id", "?")
            if row.get("status") not in STATUSES:
                err.add("ROW-STATUS-ENUM", f"{mid}/{rid}: status {row.get('status')!r}")
            if row.get("repro") not in REPROS:
                err.add("ROW-REPRO-ENUM", f"{mid}/{rid}: repro {row.get('repro')!r}")
            rev = row.get("evidence", "")
            if rev and not resolve(rev).exists():
                err.add("EVIDENCE-MISSING", f"{mid}/{rid}: evidence does not exist: {rev}")
            if row.get("status") == "NC":
                if not row.get("signature"):
                    err.add("NC-WITHOUT-SIGNATURE", f"{mid}/{rid}: NC row carries no signature")
                if not row.get("reason"):
                    err.add("NC-WITHOUT-SIGNATURE", f"{mid}/{rid}: NC row carries no cannot-measure reason")
            if row.get("repro") == "two-process":
                if row.get("runs") != 2:
                    err.add("REPRO-DECLARED-BUT-SINGLE",
                            f"{mid}/{rid}: two-process row with runs={row.get('runs')!r}")
                if not row.get("digest_evidence"):
                    err.add("REPRO-NO-DIGEST-EVIDENCE", f"{mid}/{rid}: two-process row without digest_evidence")
                elif not resolve(row["digest_evidence"].split("#", 1)[0]).exists():
                    err.add("EVIDENCE-MISSING", f"{mid}/{rid}: digest_evidence does not exist")


def load_handoff_rows(path: Path):
    """(rows as [(lineno, dict)], [(lineno, detail)]); a bad line never aborts."""
    rows, bad = [], []
    for lineno, line in enumerate(read_text(path).splitlines(), 1):
        if not line.strip():
            continue
        try:
            row = json.loads(line)
        except json.JSONDecodeError as exc:
            bad.append((lineno, f"row is not JSON ({exc}) — emission anomaly"))
            continue
        if not isinstance(row, dict):
            bad.append((lineno, f"row is not a JSON object ({type(row).__name__})"))
            continue
        rows.append((lineno, row))
    return rows, bad


def load_legacy_ids(path: Path):
    ids = []
    for line in read_text(path).splitlines():
        line = line.split("#", 1)[0].strip()
        if line:
            ids.append(line)
    return ids


def legacy_of(registry: dict):
    legacy_file = registry.get("legacy_file")
    if not legacy_file:
        return set()
    path = resolve(str(legacy_file))
    if not path.is_file():
        return set()
    return set(load_legacy_ids(path))


def split_agent(value):
    """`og-qa (lane X) verificado por Y` -> (`og-qa`, `X`).

    The base is everything before the FIRST parenthetical; the lane is that
    parenthetical's label, with the optional `lane` word removed and truncated at
    the first comma/space so `(lane M44LaneDContent, sub de M44Mazmorra)` yields the
    same label the row declares in its `lane` field. A string with no parenthetical
    carries no lane: `None`, never a guess.
    """
    text = str(value or "").strip()
    match = AGENT_PAREN_RE.search(text)
    if not match:
        return text.lower(), None
    label = re.split(r"[,\s]+", LANE_WORD_RE.sub("", match.group(1).strip()))[0].strip()
    return text[:match.start()].strip().lower(), (label.lower() or None)


def agent_base(value) -> str:
    """`og-qa (lane X)` and `og-qa` are the same author: the lane is not the identity."""
    return split_agent(value)[0]


def short_cr(cr: str) -> str:
    """`CR-<uuid>` -> `CR-xxxxxxxx`: the form every lane in this repository cites."""
    match = CR_UUID_RE.match(cr)
    return cr[:11] if match else cr


def mentions_cr(text: str, cr: str) -> bool:
    low = text.lower()
    return cr.lower() in low or short_cr(cr).lower() in low


def signature_of(row: dict):
    def get(key):
        value = row.get(key)
        return value.strip() if isinstance(value, str) and value.strip() else None
    return get("signed_by"), get("signed_at"), get("qa_evidence")


def missing_signature_fields(row: dict):
    """The names among SIGNATURE_FIELDS the row does not carry, in order.

    One predicate for two readers: the per-row error list and the receipt. When
    they disagree the LIST wins, so the receipt is built from this and never from
    arithmetic on other counters (CR-9f3c17a2: `done_signed` was `done - legacy`,
    a subtraction that could claim a signature nobody gave).
    """
    return [name for name, value in zip(SIGNATURE_FIELDS, signature_of(row)) if value is None]


def validate_registry_shape(registry: dict, err: Errors, index_path: Path) -> None:
    """The registry-level contract: which index, which frozen legacy set, which cutoff."""
    if registry.get("schema") != HANDOFF_REGISTRY_SCHEMA:
        err.add("REGISTRY-SCHEMA",
                f"handoff_registry.schema {registry.get('schema')!r} != {HANDOFF_REGISTRY_SCHEMA}")
    declared_index = registry.get("index")
    if not declared_index:
        err.add("REGISTRY-SCHEMA", "handoff_registry.index is missing")
    elif resolve(str(declared_index)).resolve() != index_path.resolve():
        err.add("REGISTRY-INDEX-DRIFT",
                f"handoff_registry.index {declared_index!r} is not the validated registry "
                f"{index_path}")
    legacy_file = registry.get("legacy_file")
    legacy_path = resolve(str(legacy_file)) if legacy_file else None
    if legacy_path is None or not legacy_path.is_file():
        err.add("REGISTRY-LEGACY-FILE-MISSING",
                f"frozen legacy set does not exist: {legacy_file!r}")
    else:
        measured = "sha256:" + hashlib.sha256(legacy_path.read_bytes()).hexdigest()
        if measured != str(registry.get("legacy_digest") or ""):
            err.add("REGISTRY-LEGACY-DIGEST-DRIFT",
                    f"{legacy_file}: declared {registry.get('legacy_digest')!r} "
                    f"measured {measured}")
    cutoff = str(registry.get("legacy_cutoff") or "")
    if not ISO_DATE_RE.match(cutoff):
        err.add("REGISTRY-LEGACY-CUTOFF",
                f"legacy_cutoff {registry.get('legacy_cutoff')!r} is not an ISO-8601 date")


def validate_registry_rows(rows, bad, err: Errors, legacy) -> None:
    """`DONE` without an independent signature is not a state: it is a claim."""
    legacy = set(legacy)
    for lineno, detail in bad:
        err.add("REGISTRY-BAD-JSON", f"line {lineno}: {detail}")

    dated = {}
    for lineno, row in rows:
        rid = str(row.get("id") or f"line {lineno}")
        missed = [k for k in REGISTRY_ROW_REQUIRED if row.get(k) in (None, "")]
        if missed:
            err.add("REGISTRY-FIELD-MISSING", f"{rid}: missing {', '.join(missed)}")
        # CR-2b6d4f18: a receipt that names a path must name bytes. An empty `path`
        # is already `REGISTRY-FIELD-MISSING`; a non-empty one that is not a file
        # sailed through, because the digest check only runs `if own.is_file()`.
        # The asymmetry with `qa_evidence` (which does say MISSING) was the hole.
        raw_path = str(row.get("path") or "")
        own = resolve(raw_path)
        if raw_path and not own.is_file():
            err.add("REGISTRY-PATH-MISSING",
                    f"{rid}: path {raw_path!r} is not a file on disk (a receipt must "
                    f"point at bytes that exist)")
        if row.get("schema") != "HH-1":
            err.add("REGISTRY-ROW-SCHEMA", f"{rid}: schema {row.get('schema')!r}, expected 'HH-1'")
        status = row.get("status")
        if status not in CLOSURE_STATUSES:
            err.add("REGISTRY-STATUS-ENUM",
                    f"{rid}: status {status!r} not in {list(CLOSURE_STATUSES)}")
        date = str(row.get("date") or "")
        if not ISO_DATE_RE.match(date):
            err.add("REGISTRY-DATE-FORMAT", f"{rid}: date {date!r} is not ISO-8601")
        dated[rid] = date

        signed_by, signed_at, qa_evidence = signature_of(row)
        if signed_at and not (ISO_DATE_RE.match(signed_at) or ISO_DATETIME_RE.match(signed_at)):
            err.add("REGISTRY-SIGNED-AT-FORMAT",
                    f"{rid}: signed_at {signed_at!r} is not ISO-8601")
        if qa_evidence:
            evidence_path = resolve(qa_evidence)
            if not evidence_path.is_file():
                err.add("REGISTRY-QA-EVIDENCE-MISSING",
                        f"{rid}: qa_evidence is not on disk: {qa_evidence}")
            elif own.is_file() and evidence_path.resolve() == own.resolve():
                err.add("REGISTRY-QA-EVIDENCE-IS-SELF",
                        f"{rid}: qa_evidence points at the row's own HH-1 ({qa_evidence})")
            else:
                cr = str(row.get("cr") or "")
                text = evidence_path.read_text(encoding="utf-8", errors="replace")
                if cr and not mentions_cr(text, cr):
                    err.add("REGISTRY-QA-EVIDENCE-DISCONNECTED",
                            f"{rid}: qa_evidence {qa_evidence} does not mention {cr} "
                            f"(neither in full nor as {short_cr(cr)})")

        if status != "DONE" or rid in legacy:
            continue
        missing = missing_signature_fields(row)
        if missing:
            err.add("REGISTRY-DONE-WITHOUT-SIGNATURE",
                    f"{rid}: DONE without {', '.join(missing)} and not in the frozen legacy set")
        author_base, _author_paren_lane = split_agent(row.get("agent"))
        signer_base, signer_lane = split_agent(signed_by)
        if signed_by and signer_base == author_base:
            err.add("REGISTRY-SELF-SIGNED",
                    f"{rid}: signed_by {signed_by!r} is the HH-1 author {row.get('agent')!r}")
        # CR-9f3c17a2: a different agent id inside the SAME lane is still the lane
        # stamping its own closure (`og-l1-checklist (lane X)` signed by
        # `og-orquestador (lane X)`). Only when both sides declare a lane: a row
        # without labels is not presumed guilty, it falls back to the agent check.
        author_lane = str(row.get("lane") or "").strip().lower() or None
        if (signed_by and author_lane and signer_lane and signer_lane == author_lane
                and signer_base != author_base):
            err.add("REGISTRY-SAME-LANE-SIGNER",
                    f"{rid}: signed_by {signed_by!r} and author {row.get('agent')!r} share the "
                    f"lane {author_lane!r} — the lane signing its own DONE is not a signature")
        # A distinct signer is still nothing if the name is not an author: the
        # registry token, free text or a blank signature seals nothing (CR-5ca7f44e).
        raw_signer = row.get("signed_by")
        if isinstance(raw_signer, str) and not SOVEREIGN_SIGNER_RE.match(agent_base(raw_signer)):
            err.add("REGISTRY-BOGUS-SIGNER",
                    f"{rid}: signed_by {raw_signer!r} is not a sovereign author "
                    f"(expected an og-* base; a registry token, free text or a blank "
                    f"signature is not a signer)")
        declared_sha = str(row.get("sha256") or "").lower()
        if declared_sha and own.is_file():
            real = hashlib.sha256(own.read_bytes()).hexdigest()
            if real != declared_sha:
                err.add("REGISTRY-SHA-DRIFT",
                        f"{rid}: signed DONE declares sha256 {declared_sha[:16]}… and the bytes "
                        f"give {real[:16]}…")

    for lid in sorted(legacy):
        if lid not in dated:
            err.add("REGISTRY-LEGACY-ID-UNKNOWN", f"legacy id not in the registry: {lid}")


def registry_cutoff_errors(registry: dict, rows, err: Errors) -> None:
    """A legacy id dated after the cutoff is a forged exemption: fail it by date."""
    cutoff = str(registry.get("legacy_cutoff") or "")
    if not ISO_DATE_RE.match(cutoff):
        return
    legacy = legacy_of(registry)
    dated = {str(row.get("id")): str(row.get("date") or "") for _, row in rows}
    for lid in sorted(legacy):
        if lid in dated and dated[lid] > cutoff:
            err.add("REGISTRY-LEGACY-ID-POST-CUTOFF",
                    f"{lid}: dated {dated[lid]} is after the legacy cutoff {cutoff}")


def handoff_receipts(rows, legacy) -> dict:
    legacy = set(legacy)
    by_status = {}
    for _, row in rows:
        key = str(row.get("status"))
        by_status[key] = by_status.get(key, 0) + 1
    done = by_status.get("DONE", 0)
    legacy_done = len([1 for _, row in rows
                       if row.get("status") == "DONE" and str(row.get("id")) in legacy])
    # Measured, not inferred: a DONE row is signed only when it carries the three
    # SIGNATURE_FIELDS. `done - legacy_done` counted legacy exemptions as signatures.
    signed_done = len([1 for _, row in rows
                       if row.get("status") == "DONE" and not missing_signature_fields(row)])
    unsigned_done = len([1 for _, row in rows
                         if row.get("status") == "DONE" and str(row.get("id")) not in legacy
                         and missing_signature_fields(row)])
    legacy_signed = len([1 for _, row in rows
                         if row.get("status") == "DONE" and str(row.get("id")) in legacy
                         and not missing_signature_fields(row)])
    return {"rows": len(rows), "by_status": by_status, "done": done,
            "done_signed": signed_done, "done_unsigned": unsigned_done,
            "done_legacy": legacy_done, "done_legacy_signed": legacy_signed,
            "left_done": len(legacy) - legacy_done}


def receipt_errors(receipts: dict, err: Errors) -> None:
    """The error list is the authority: a receipt that disagrees with it is the bug.

    `REGISTRY-DONE-WITHOUT-SIGNATURE` is emitted once per unsigned, non-frozen DONE
    row, so `done_unsigned` must equal that count; and the buckets must reconstruct
    every DONE row. A subtraction-based `done_signed` fails the count.
    """
    listed = len([1 for item in err.items
                  if item["code"] == "REGISTRY-DONE-WITHOUT-SIGNATURE"])
    if receipts["done_unsigned"] != listed:
        err.add("REGISTRY-RECEIPT-DRIFT",
                f"receipt counts {receipts['done_unsigned']} unsigned DONE row(s) and the "
                f"error list names {listed} — the list wins")
    reconstructed = (receipts["done_signed"] + receipts["done_unsigned"]
                     + receipts["done_legacy"] - receipts["done_legacy_signed"])
    if reconstructed != receipts["done"]:
        err.add("REGISTRY-RECEIPT-DRIFT",
                f"receipt buckets sum {reconstructed} and the registry holds "
                f"{receipts['done']} DONE row(s)")


def run_registry_selftest():
    """Negatives that must bite and positives that must stay clean.

    A case is RED only when the produced codes are exactly the expected ones
    (`()` means "no code at all"), so relaxing a rule turns a negative green AND
    tightening one turns a positive red.
    """
    # Two real files of this repository: the QA row (mentions the CR) and the HH-1
    # the case cites as its own closure. Distinct on purpose, so `IS-SELF` never
    # contaminates the self-signature case.
    qa_row = "docs/sintergic/validation/CLOSURE-RULE.md"
    own = "docs/sintergic/validation/closure-register-CR-03390267.md"
    cr = "CR-03390267-3428-4386-b767-afc65d542fb9"
    sha = hashlib.sha256(resolve(own).read_bytes()).hexdigest()
    # An artefact that cites one change request only in the short `CR-xxxxxxxx` form.
    short_owned_cr = "CR-2c301a5a-1d9c-47fd-b385-7be6eb1d2f76"
    short_only = "docs/sintergic/validation/handoff-legacy-done-CR-2c301a5a.txt"

    def row(**over):
        base = {"id": "HH-NEG", "cr": cr, "date": "2026-10-29",
                "agent": "og-fixer (lane X)", "status": "DONE",
                "path": own, "sha256": sha, "schema": "HH-1"}
        base.update(over)
        return base

    # CR-9f3c17a2 — falsifier with REAL rows, not invented ones. Payload extracted
    # literally from `git show 5597baf^:docs/sintergic/handoffs/INDEX.jsonl`
    # (5597baf = the remediation; its PARENT still had L496 and L499 in `DONE`, signed
    # by `og-orquestador (lane SaveV3Cierre)` with authorship `og-l1-checklist (lane
    # SaveV3Cierre)`). Only the fields the gate reads. `path`/`qa_evidence` are
    # re-anchored to REPO so the fixture survives a path change: the relative
    # tail is the historical one, literal.
    def hist(path, **over):
        base = {"cr": "CR-364c0313-942f-4e89-9cb0-7c8f28be36f8", "date": "2026-10-30",
                "agent": "og-l1-checklist (lane SaveV3Cierre)", "lane": "SaveV3Cierre",
                "status": "DONE", "signed_at": "2026-10-30", "schema": "HH-1"}
        base["path"] = str(REPO / path[0])
        base["qa_evidence"] = str(REPO / path[1])
        base.update(over)
        return base

    desbloqueo = hist(
        ("docs/sintergic/handoffs/hh-1-20261030-savev3-desbloqueo-CR-364c0313.md",
         "docs/sintergic/handoffs/hh-1-20261030-savev3coresign-CR-364c0313.md"),
        id="HH-20261030-og-l1-checklist-savev3-desbloqueo",  # L496 @ 5597baf^
        sha256="063f25380c4b41ebe01637e80988b0ee92066eb2f5a9052e57593b7c084a327c",
        signed_by="og-orquestador (lane SaveV3Cierre) — cierre del ciclo")
    cierre = hist(
        ("docs/sintergic/handoffs/hh-1-20261030-savev3cierre-CR-b0f248a7.md",
         "docs/sintergic/orchestrator/packs/CR-364c0313-942f-4e89-9cb0-7c8f28be36f8-CIERRE.md"),
        id="HH-20261030-og-l1-checklist-savev3cierre",  # L499 @ 5597baf^
        cr="CR-b0f248a7-d6fc-4bb2-8ff6-18fbd1018a2f",
        sha256="7db16577ef929984b443246501d932fafb29a2869292facb4db9a0edd3bdced5",
        signed_by="og-orquestador (lane SaveV3Cierre)")
    coresign = hist(
        ("docs/sintergic/handoffs/hh-1-20261030-savev3coresign-CR-364c0313.md",
         "docs/sintergic/handoffs/hh-1-20261030-savev3-desbloqueo-CR-364c0313.md"),
        id="HH-20261030-og-qa-savev3coresign",  # fila vigente del registro remediado
        agent="og-qa (lane SaveV3CoreSign)", lane="SaveV3CoreSign",
        sha256="8e784fd1974f25a5a4720fe5472c439d247266905a5b1fc8b1aa4c96be8aefdc",
        signed_by="og-l1-checklist (lane SaveV3Cierre) — registro del cierre")

    # CR-2b6d4f18 — a real `candidato` row whose `path` promises bytes. Payload
    # extracted literally from `git show 5597baf^:docs/sintergic/handoffs/INDEX.jsonl`
    # (L434), only the fields the gate reads; the path's relative tail is the
    # historical one, literal, and re-anchored to REPO so the fixture survives a
    # root change. `candidato` requires no signature, so the only possible code is
    # the path's: the negative does not contaminate itself.
    contrato_sync = {
        "schema": "HH-1", "id": "HH-20261029-contrato-sync",
        "cr": "CR-01e3aebf-b0d1-49f8-af7a-564a57385c06", "date": "2026-10-29",
        "agent": "og-fixer", "lane": "ContratoSync", "status": "candidato",
        "path": str(REPO / "docs/sintergic/handoffs/hh-1-20261029-contrato-sync-CR-01e3aebf.md"),
        "sha256": "dc7fb452a6e86b8bb887ae520d44ffca4fc0b67e24559963d13aa900b9203723",
    }

    cases = [
        ("done-sin-firma",
         [row(id="HH-NEG-1")],
         ["REGISTRY-DONE-WITHOUT-SIGNATURE"],
         set()),
        ("autofirma",
         [row(id="HH-NEG-2", agent="og-qa (lane A)", signed_by="og-qa (lane B)",
              signed_at="2026-10-29T10:00:00Z", qa_evidence=qa_row)],
         ["REGISTRY-SELF-SIGNED"],
         set()),
        ("qa-evidence-ausente",
         [row(id="HH-NEG-3", agent="og-qa", signed_by="og-l1-70-quality-meta",
              signed_at="2026-10-29T10:00:00Z",
              qa_evidence="docs/sintergic/validation/absent-CR-2c301a5a.md")],
         ["REGISTRY-QA-EVIDENCE-MISSING"],
         set()),
        ("candidato-sin-firma",
         [row(id="HH-POS-1", status="candidato")],
         [],
         set()),
        ("legacy-done-sin-firma",
         [row(id="HH-POS-2")],
         [],
         {"HH-POS-2"}),
        ("qa-evidencia-desconectada",
         [row(id="HH-NEG-4", cr="CR-deadbeef-0000-4000-8000-000000000000",
              signed_by="og-qa", signed_at="2026-10-29T10:00:00Z",
              qa_evidence=short_only)],
         ["REGISTRY-QA-EVIDENCE-DISCONNECTED"],
         set()),
        ("firma-con-cr-corto",
         [row(id="HH-POS-4", cr=short_owned_cr, signed_by="og-qa",
              signed_at="2026-10-29T10:00:00Z", qa_evidence=short_only)],
         [],
         set()),
        # Regression measured by the captain: this row was lowered today, so it cannot be exempt.
        ("rebajado-hoy-sin-firma",
         [row(id="HH-20261029-gate-m43-lane-a3")],
         ["REGISTRY-DONE-WITHOUT-SIGNATURE"],
         set()),
        # A row created and self-signed on the cut's own day is not historical: it cannot be exempt.
        ("creado-hoy-sin-firma",
         [row(id="HH-20261030-capa-visible-shell-visible-layer", date="2026-10-30")],
         ["REGISTRY-DONE-WITHOUT-SIGNATURE"],
         set()),
        # CR-5ca7f44e: a signer that names no author is not a signature.
        ("firmante-fantasma",
         [row(id="HH-NEG-5", signed_by="REGISTRO",
              signed_at="2026-10-29T10:00:00Z", qa_evidence=qa_row)],
         ["REGISTRY-BOGUS-SIGNER"],
         set()),
        # ...and the rule is not vacuous: another sovereign author seals the row.
        ("firmante-soberano-externo",
         [row(id="HH-POS-5", signed_by="og-qa (lane X)",
              signed_at="2026-10-29T10:00:00Z", qa_evidence=qa_row)],
         [],
         set()),
        # The frozen legacy set stays exempt exactly as before: 364 rows untouched.
        ("legacy-done-firmante-fantasma",
         [row(id="HH-POS-6", signed_by="REGISTRO")],
         [],
         {"HH-POS-6"}),
        ("legacy-id-huerfano",
         [row(id="HH-POS-3", status="candidato")],
         ["REGISTRY-LEGACY-ID-UNKNOWN"],
         {"HH-GHOST"}),
        # ── CR-9f3c17a2: the lane that signs itself, with the real rows ──
        # L496 @ 5597baf^: author `og-l1-checklist (lane SaveV3Cierre)`, signer
        # `og-orquestador (lane SaveV3Cierre) — cycle close`. The gate of that time
        # compared only the agent base: it saw two different authors.
        ("misma-lane-savev3-desbloqueo",
         [desbloqueo],
         ["REGISTRY-SAME-LANE-SIGNER"],
         set()),
        # L499 @ 5597baf^: mismo indulto, sin prosa detras del parentesis.
        ("misma-lane-savev3cierre",
         [cierre],
         ["REGISTRY-SAME-LANE-SIGNER"],
         set()),
        # Prose AFTER the parenthesis does not erase the lane: `og-qa (lane X) verified`
        # is still lane X signing its own row.
        ("misma-lane-prosa-final",
         [dict(desbloqueo, signed_by="og-qa (lane SaveV3Cierre) verificado")],
         ["REGISTRY-SAME-LANE-SIGNER"],
         set()),
        # ...and the check does NOT always fire: a real DONE row of the remediated registry,
        # authored by lane `SaveV3CoreSign` and signed by lane `SaveV3Cierre` (with prose).
        ("firmante-de-otra-lane",
         [coresign],
         [],
         set()),
        # The incomplete signature still bites: the real row without `qa_evidence`.
        ("fila-real-sin-qa-evidence",
         [{k: v for k, v in coresign.items() if k != "qa_evidence"}],
         ["REGISTRY-DONE-WITHOUT-SIGNATURE"],
         set()),
        # Without a lane tag on one side guilt is not presumed: it falls to the agent
        # check and mints no new code (uncertainty is not a finding).
        ("firmante-sin-etiqueta-de-lane",
         [dict(coresign, signed_by="og-l1-checklist")],
         [],
         set()),
        ("autor-sin-campo-lane",
         [{k: v for k, v in coresign.items() if k != "lane"}],
         [],
         set()),
        # ── CR-2b6d4f18: the path that promises bytes that are not there ──
        # The same real row with a nonexistent `path`: the declared digest cannot
        # be checked against anything, and until now the gate kept silent.
        ("candidato-path-inexistente",
         [dict(contrato_sync,
               path=str(REPO / "docs/sintergic/handoffs/hh-1-20261029-contrato-sync-CR-2b6d4f18.md"))],
         ["REGISTRY-PATH-MISSING"],
         set()),
        # ...and the negative control: the SAME row with its real path mints no code.
        # Without this the check would be vacuity that always fires.
        ("candidato-path-real",
         [contrato_sync],
         [],
         set()),
    ]
    results, failures = [], []
    for name, case_rows, expected, case_legacy in cases:
        err = Errors()
        validate_registry_rows([(i + 1, r) for i, r in enumerate(case_rows)], [], err,
                               set(case_legacy))
        codes = sorted(set(err.codes()))
        red = codes == sorted(set(expected))
        results.append({"name": name, "expected": expected, "red": red, "codes": codes})
        if not red:
            failures.append(f"{name}: expected {expected or 'no error'}, got {codes or 'no error'}")
    return results, failures


def differing(rows):
    return [r.get("id", "?") for r in rows if r.get("status") != "PASS"]


def validate_rule(obj: dict, err: Errors, tags=None) -> None:
    tags = tags if tags is not None else git_tags()
    milestones = obj.get("milestones", [])

    for m in milestones:
        mid = m.get("id", "?")
        verdict = m.get("verdict")
        rows = m.get("rows", [])
        bad = differing(rows)

        if verdict == "DONE":
            if not rows:
                err.add("DONE-WITHOUT-ROWS", f"{mid}: DONE with no gate rows")
            if bad:
                err.add("DONE-WITHOUT-100PCT", f"{mid}: DONE but rows not PASS: {', '.join(bad)}")

        if m.get("public_state") == "DONE-technical" and verdict == "DONE":
            for row in rows:
                if row.get("repro") != "two-process":
                    err.add("TECHNICAL-CLOSURE-AS-DONE",
                            f"{mid}: labelled technical closure yet DONE, but row "
                            f"{row.get('id')} is {row.get('repro')}, not two-process")
                    break

        if verdict == "DONE":
            legacy = [r for r in rows if r.get("repro") != "two-process"]
            if legacy:
                tag = m.get("tag") or ""
                if not m.get("legacy_closure"):
                    err.add("DONE-ON-LEGACY-REPRO",
                            f"{mid}: DONE on row {legacy[0].get('id')} "
                            f"({legacy[0].get('repro')}) without legacy_closure")
                elif not tag or tag not in tags:
                    err.add("LEGACY-CLOSURE-WITHOUT-TAG",
                            f"{mid}: legacy_closure declared but tag {tag!r} not in git tag")

        if verdict in {"PARTIAL", "PARTIAL-TECHNICAL-CLOSURE"} and not m.get("missing_rows"):
            err.add("PARTIAL-WITHOUT-NAMED-GAP", f"{mid}: PARTIAL with no named missing row")

        if verdict == "PARTIAL-TAG-PENDING" and m.get("tag"):
            err.add("TAG-PENDING-WITH-TAG", f"{mid}: PARTIAL-TAG-PENDING but tag {m.get('tag')!r} is set")

        if verdict == "AUSENTE" and rows:
            err.add("AUSENTE-WITH-ROWS", f"{mid}: AUSENTE but {len(rows)} rows declared")

        for row in rows:
            if row.get("basis") in {"pass-as-shipped", "shipped", "inherited"}:
                err.add("PASS-AS-SHIPPED-PROHIBITED",
                        f"{mid}/{row.get('id')}: closure basis {row.get('basis')!r} is prohibited")

        if m.get("acceptance") == "ACCEPTED":
            tag = m.get("tag") or ""
            if not m.get("owner_signature"):
                err.add("ACCEPTED-WITHOUT-SIGNATURE", f"{mid}: ACCEPTED without owner_signature")
            if not tag or tag not in tags:
                err.add("ACCEPTED-WITHOUT-TAG", f"{mid}: ACCEPTED but tag {tag!r} not in git tag")
            if any(r.get("repro") != "two-process" for r in rows):
                err.add("ACCEPTED-ON-LEGACY-REPRO", f"{mid}: ACCEPTED with a non two-process row")

    m46 = next((m for m in milestones if m.get("id") == "M46"), None)
    if m46 is None or m46.get("verdict") != "AUSENTE" or m46.get("blocker") != "OWNER-GO":
        err.add("M46-NOT-BLOCKED", "M46 must stay AUSENTE with blocker OWNER-GO")

    for item in obj.get("transversal", []):
        tid = item.get("id", "?")
        target = item.get("deferred_to")
        owners = [m.get("id") for m in milestones if tid in m.get("missing_rows", [])] + [mm.get('id') for mm in milestones for r in mm.get('deferred_resolved', []) if r == tid]
        if owners != [target]:
            err.add("DEFERRED-NOT-COUNTED-ONCE",
                    f"{tid}: counted in {owners or 'no milestone'}, expected only {target}")

    closed = all(m.get("verdict") == "DONE" and m.get("acceptance") == "ACCEPTED"
                 for m in milestones)
    if closed != (obj.get("program_closure") == "CLOSED"):
        err.add("PROGRAM-CLOSURE-INCOHERENT",
                f"program_closure={obj.get('program_closure')!r} but rows imply "
                f"{'CLOSED' if closed else 'OPEN'}")

    text = obj.get("_text", "")
    for m in milestones:
        mid = m.get("id", "?")
        line = next((ln for ln in text.splitlines()
                     if re.match(r"^\|\s*\*\*" + re.escape(mid) + r"\*\*\s*\|", ln)), None)
        if line is None:
            err.add("TABLE-ROW-MISSING", f"{mid}: no table row in the human section")
        elif m.get("verdict") not in line:
            err.add("TABLE-VERDICT-DRIFT",
                    f"{mid}: table row does not carry verdict {m.get('verdict')}")


def milestone_ids(text: str):
    ids = set()
    for lo, hi in RANGE_RE.findall(text):
        a, b = int(lo), int(hi)
        if 0 <= a <= b <= 48:
            for n in range(a, b + 1):
                ids.add(f"M{n}")
    for num, b in ID_RE.findall(text):
        ids.add(f"M{num}{'b' if b else ''}")
    return {i for i in ids if i in EXPECTED_IDS}


def negated(text: str, start: int) -> bool:
    return bool(NEG_RE.search(text[max(0, start - 28):start]))


def claims(text: str):
    done = absent = False
    for match in DONE_RE.finditer(text):
        if not negated(text, match.start()):
            done = True
    for match in ABSENT_RE.finditer(text):
        if not negated(text, match.start()):
            absent = True
    return done, absent


SENT_SPLIT_RE = re.compile(r"\u00b7|;|\.\s")
PART_SPLIT_RE = re.compile(r":")
SENTINEL = (None, None)


def scope_parts(text: str):
    """Flat [(part, ids)] for one scope, with a sentence-boundary sentinel.

    A part is a colon-separated piece of a sentence. The sentinel resets the
    attribution memory, so `X is done. Y is not started` never leaks Y's claim
    onto X or the reverse.
    """
    parts = []
    for sentence in SENT_SPLIT_RE.split(text):
        if not sentence.strip():
            continue
        for part in PART_SPLIT_RE.split(sentence):
            if part.strip():
                parts.append((part, milestone_ids(part)))
        parts.append(SENTINEL)
    return parts


def prose_scopes(text: str):
    """Authoritative state scopes: (line, subject_ids, parts).

    In scope:
      * a milestone table row whose FIRST cell names a milestone (audit tables
        that quote third-party claims, e.g. `| **C-1** | "M18 is DONE" |`, are
        out of scope by construction);
      * a paragraph opening with `State in one line:` or `**M<n>`.
    """
    scopes = []
    lines = text.splitlines()
    i = 0
    while i < len(lines):
        line = lines[i]
        if ROW_START_RE.match(line):
            cells = line.split("|")
            subject = milestone_ids(cells[1]) if len(cells) > 1 else set()
            if subject:
                scopes.append((i + 1, subject, scope_parts(" | ".join(cells[1:]))))
            i += 1
            continue
        if STATE_IN_ONE_RE.match(line) or BOLD_STATE_RE.match(line):
            para = [line]
            j = i + 1
            while j < len(lines) and lines[j].strip() \
                    and not lines[j].startswith("|") and not lines[j].startswith("#"):
                para.append(lines[j])
                j += 1
            joined = " ".join(para)
            scopes.append((i + 1, milestone_ids(joined), scope_parts(joined)))
            i = j
            continue
        i += 1
    return scopes


def declared_defect(obj: dict, rel: str, clause: str, mid: str):
    """A registered documentation defect, valid only for understatement."""
    for item in obj.get("prose_defects", []):
        if item.get("file") != rel or item.get("kind") != "understatement":
            continue
        if item.get("milestone") not in (None, mid):
            continue
        if item.get("contains", "") and item["contains"] in clause:
            return item
    return None


def validate_prose(obj: dict, err: Errors) -> None:
    by_id = {m["id"]: m for m in obj.get("milestones", [])}
    used_defects = set()
    for item in obj.get("prose_defects", []):
        for key in ("file", "contains", "milestone", "kind", "owner", "cr", "why"):
            if not item.get(key):
                err.add("PROSE-DEFECT-MALFORMED", f"prose defect missing {key}: {item.get('contains')}")
        if item.get("kind") != "understatement":
            err.add("PROSE-DEFECT-UNSUPPORTED-KIND",
                    f"{item.get('file')}: kind {item.get('kind')!r}; only understatement may be "
                    f"declared, inflation is never exemptable")
    mapping = obj.get("label_mapping", {})
    if mapping.get("public_done_means") != "technical-closure":
        err.add("LABEL-MAPPING-MISSING", "public_done_means must be technical-closure")

    anchor_ok = {}
    for anchor in mapping.get("anchors", []):
        path = resolve(anchor.get("file", ""))
        if not path.exists():
            err.add("LABEL-ANCHOR-MISSING", f"anchor file does not exist: {anchor.get('file')}")
            continue
        ok = anchor.get("contains", "") in path.read_text(encoding="utf-8")
        anchor_ok[anchor["file"]] = anchor_ok.get(anchor["file"], True) and ok
        if not ok:
            err.add("LABEL-ANCHOR-MISSING",
                    f"{anchor['file']}: missing label anchor {anchor.get('contains')!r}")

    for rel in obj.get("prose_surface", []):
        path = resolve(rel)
        if not path.exists():
            err.add("PROSE-FILE-MISSING", f"public prose file does not exist: {rel}")
            continue
        text = path.read_text(encoding="utf-8")
        file_anchor = anchor_ok.get(rel, False)

        for lineno, subject, clauses in prose_scopes(text):
            last = None
            for clause, clause_ids in clauses:
                if clause is None:
                    last = None
                    continue
                if clause_ids:
                    last = clause_ids
                done, absent = claims(clause)
                if not (done or absent):
                    continue
                targets = clause_ids or last or subject
                for mid in sorted(targets, key=lambda x: (int(re.sub(r"\D", "", x)), x)):
                    m = by_id.get(mid)
                    if m is None:
                        continue
                    verdict = m["verdict"]
                    if done and by_id[mid].get("public_state") not in ("DONE", "DONE-technical"):
                        technical = m.get("public_state") == "DONE-technical"
                        compat = technical and (bool(TECH_RE.search(clause)) or file_anchor)
                        if verdict != "PARTIAL-TECHNICAL-CLOSURE" or not compat:
                            err.add("PROSE-DONE-VS-REGISTER",
                                    f"{rel}:{lineno} claims DONE/closed for {mid} "
                                    f"(register: {verdict})")
                    if absent and verdict != "AUSENTE":
                        defect = declared_defect(obj, rel, clause, mid)
                        if defect is not None:
                            used_defects.add(id(defect))
                            continue
                        err.add("PROSE-ABSENT-VS-REGISTER",
                                f"{rel}:{lineno} claims NOT STARTED/frozen for {mid} "
                                f"(register: {verdict})")
                if done and PASS_SHIPPED_RE.search(clause):
                    err.add("PASS-AS-SHIPPED-CLAIM",
                            f"{rel}:{lineno} uses PASS-as-shipped as a closure claim")

        for match in AGGREGATE_RE.finditer(text):
            if not negated(text, match.start()):
                err.add("PROSE-AGGREGATE-CLOSURE",
                        f"{rel}: aggregate closure claim {match.group(0)!r} while "
                        f"program_closure={obj.get('program_closure')}")


    for item in obj.get("prose_defects", []):
        if id(item) not in used_defects:
            err.add("STALE-PROSE-DEFECT",
                    f"{item.get('file')}: declared defect matches no live prose: "
                    f"{item.get('contains')!r}")


def mutation_fixtures(obj):
    if FIXTURE_DIR.exists():
        return [json.loads(p.read_text(encoding="utf-8"))
                for p in sorted(FIXTURE_DIR.glob("*.json"))]
    return []


def apply_mutations(obj, spec):
    mutated = copy.deepcopy(obj)
    mutated.pop("_text", None)
    by_id = {m["id"]: m for m in mutated["milestones"]}
    for mut in spec.get("mutations", []):
        if "milestone" in mut and mut["milestone"] in by_id:
            target = by_id[mut["milestone"]]
            for key, value in mut.get("set", {}).items():
                target[key] = value
            if "rows" in mut:
                target["rows"] = copy.deepcopy(mut["rows"])
            if "missing_rows" in mut:
                target["missing_rows"] = list(mut["missing_rows"])
        if "prose_surface" in mut:
            mutated["prose_surface"] = list(mut["prose_surface"])
    return mutated


def run_falsifiers(obj):
    results, failures = [], []
    for spec in mutation_fixtures(obj):
        name = spec.get("name", "?")
        expected = spec.get("expected_error", "")
        err = Errors()
        mutated = apply_mutations(obj, spec)
        validate_schema(mutated, err)
        validate_rule(mutated, err)
        validate_prose(mutated, err)
        codes = sorted(set(err.codes()))
        red = expected in codes
        results.append({"name": name, "expected": expected, "red": red, "codes": codes})
        if not red:
            failures.append(f"{name}: expected {expected}, got {codes or 'no error'}")
    return results, failures


def digest(obj, prose_paths, registry=None, registry_rows=()) -> str:
    projection = {
        "cr": obj.get("cr"),
        "program_closure": obj.get("program_closure"),
        "milestones": [
            {
                "id": m.get("id"),
                "verdict": m.get("verdict"),
                "acceptance": m.get("acceptance"),
                "rows": [{k: r.get(k) for k in ("id", "status", "repro", "runs", "evidence")}
                         for r in m.get("rows", [])],
                "missing_rows": m.get("missing_rows", []),
            }
            for m in obj.get("milestones", [])
        ],
        "handoff_registry": {
            "schema": (registry or {}).get("schema"),
            "legacy_digest": (registry or {}).get("legacy_digest"),
            "legacy_cutoff": (registry or {}).get("legacy_cutoff"),
            "rows": [
                {
                    "id": row.get("id"),
                    "status": row.get("status"),
                    "sha256": row.get("sha256"),
                    "signed_by": row.get("signed_by"),
                    "signed_at": row.get("signed_at"),
                    "qa_evidence": row.get("qa_evidence"),
                }
                for _, row in registry_rows
            ],
        },
        "prose": [],
    }
    for rel in prose_paths:
        path = resolve(rel)
        if path.exists():
            projection["prose"].append(
                {"file": rel, "sha256": hashlib.sha256(path.read_bytes()).hexdigest()})
    payload = json.dumps(projection, sort_keys=True, ensure_ascii=True)
    return hashlib.sha256(payload.encode("utf-8")).hexdigest()


def main() -> int:
    ap = argparse.ArgumentParser(description="Closure gate (CR-03390267)")
    ap.add_argument("--register", default=str(DEFAULT_REGISTER))
    ap.add_argument("--handoff-index", default=str(HANDOFF_INDEX),
                    help="sealed registry to validate (default: docs/sintergic/handoffs/"
                         "INDEX.jsonl); overriding it is a falsification sandbox, never a "
                         "green path")
    ap.add_argument("--self-test", action="store_true",
                    help="run the mutation fixtures and the registry cases only")
    args = ap.parse_args()

    register = Path(args.register)
    if not register.is_absolute():
        register = REPO / register
    try:
        text, obj = load_register(register)
    except (OSError, json.JSONDecodeError) as exc:
        print(f"closure-gate: cannot read register: {exc}")
        return 1
    obj["_text"] = text

    registry = obj.get("handoff_registry", {})
    index_path = Path(args.handoff_index)
    if not index_path.is_absolute():
        index_path = REPO / index_path
    try:
        registry_rows, registry_bad = load_handoff_rows(index_path)
    except OSError as exc:
        print(f"closure-gate: cannot read handoff registry: {exc}")
        return 1
    legacy = legacy_of(registry)

    if args.self_test:
        results, failures = run_falsifiers(obj)
        for item in results:
            print(f"falsifier {item['name']}: "
                  f"{'RED' if item['red'] else 'GREEN(bad)'} -> {item['expected']}")
        for line in failures:
            print(f"FALSIFIER-FAIL {line}")
        reg_results, reg_failures = run_registry_selftest()
        for item in reg_results:
            outcome = "RED" if item["expected"] else "CLEAN"
            detail = (", ".join(item["expected"]) if item["expected"] else "no error")
            if not item["red"]:
                outcome = "GREEN(bad)"
                detail = f"expected {detail}, got {', '.join(item['codes']) or 'no error'}"
            print(f"registry case {item['name']}: {outcome} -> {detail}")
        for line in reg_failures:
            print(f"REGISTRY-CASE-FAIL {line}")
        failures = failures + reg_failures
        if failures:
            print("VERDICT: FAIL")
            return 1
        print(f"FALSIFIERS: {len(results)}/{len(results)} red")
        print(f"REGISTRY CASES: "
              f"{sum(1 for i in reg_results if i['red'])}/{len(reg_results)} as declared")
        print("VERDICT: PASS")
        return 0

    err = Errors()
    validate_schema(obj, err)
    validate_registry_shape(registry, err, index_path)
    validate_registry_rows(registry_rows, registry_bad, err, legacy)
    registry_cutoff_errors(registry, registry_rows, err)
    validate_rule(obj, err, git_tags())
    validate_prose(obj, err)
    falsifiers, falsifier_failures = run_falsifiers(obj)

    milestones = obj.get("milestones", [])
    verdict_counts, acceptance_counts, row_counts = {}, {}, {}
    for m in milestones:
        verdict_counts[m.get("verdict", "?")] = verdict_counts.get(m.get("verdict", "?"), 0) + 1
        acceptance_counts[m.get("acceptance", "?")] = acceptance_counts.get(m.get("acceptance", "?"), 0) + 1
        for r in m.get("rows", []):
            row_counts[r.get("status", "?")] = row_counts.get(r.get("status", "?"), 0) + 1

    live_head = head_sha()
    print(f"closure-gate · {obj.get('cr')} · HEAD {live_head}")
    declared_head = obj.get("head")
    if declared_head and declared_head != live_head:
        print(f"head: register {declared_head} (derivation snapshot) · live {live_head} "
              f"— not pinned (R9)")
    print(f"milestones: {len(milestones)} · " +
          " · ".join(f"{k} {v}" for k, v in sorted(verdict_counts.items())))
    print("acceptance: " + " · ".join(f"{k} {v}" for k, v in sorted(acceptance_counts.items())))
    print(f"program_closure: {obj.get('program_closure')}")
    print(f"rows: {sum(row_counts.values())} · " +
          " · ".join(f"{k} {v}" for k, v in sorted(row_counts.items())))
    defects = obj.get("prose_defects", [])
    print(f"prose: {len(obj.get('prose_surface', []))} files · "
          f"{len([e for e in err.items if e['code'].startswith('PROSE')])} contradictions · "
          f"{len(defects)} declared defect(s)")
    for item in defects:
        print(f"PROSE-DEFECT {item.get('file')} · {item.get('milestone')} · "
              f"owner={item.get('owner')} · {item.get('cr')} · {item.get('kind')}")
    print(f"FALSIFIERS: {sum(1 for f in falsifiers if f['red'])}/{len(falsifiers)} red "
          f"({', '.join(f['expected'] for f in falsifiers)})")
    receipts = handoff_receipts(registry_rows, legacy)
    receipt_errors(receipts, err)
    print(f"handoff-registry: {receipts['rows']} rows · " +
          " · ".join(f"{k} {v}" for k, v in sorted(receipts["by_status"].items())))
    print(f"handoff-signature: DONE signed {receipts['done_signed']} · "
          f"DONE legacy-exempt {receipts['done_legacy']} · left DONE since freeze "
          f"{receipts['left_done']} · frozen legacy ids {len(legacy)} · "
          f"DONE unsigned-not-frozen {receipts['done_unsigned']}")

    for item in err.items:
        print(f"FAIL {item['code']}: {item['detail']}")
    for line in falsifier_failures:
        print(f"FAIL FALSIFIER: {line}")

    print(f"REGISTER-DIGEST: sha256:"
          f"{digest(obj, obj.get('prose_surface', []), registry, registry_rows)}")
    if err.items or falsifier_failures:
        print("VERDICT: FAIL")
        return 1
    print("VERDICT: PASS")
    return 0


if __name__ == "__main__":
    sys.exit(main())
