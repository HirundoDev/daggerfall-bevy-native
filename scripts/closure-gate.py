#!/usr/bin/env python3
"""closure-gate.py - mechanical gate for the closure rule (CR-03390267).

Reads the machine-readable block of the closure register, derives the program
state from the per-milestone rows, and fails when public prose contradicts it.
The rule itself lives in docs/sintergic/validation/CLOSURE-RULE.md.

Deterministic: two processes over the same tree print byte-identical stdout,
including REGISTER-DIGEST. No timestamps, no randomness, no network.

Usage:
    python3 scripts/closure-gate.py [--register PATH] [--self-test]
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

EXPECTED_IDS = [f"M{i}" for i in range(0, 14)] + ["M13b"] + [f"M{i}" for i in range(14, 49)] + ["E8"]

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
                "milestones", "transversal", "prose_surface", "label_mapping"):
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


def digest(obj, prose_paths) -> str:
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
    ap.add_argument("--self-test", action="store_true",
                    help="run the mutation fixtures only")
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

    if args.self_test:
        results, failures = run_falsifiers(obj)
        for item in results:
            print(f"falsifier {item['name']}: "
                  f"{'RED' if item['red'] else 'GREEN(bad)'} -> {item['expected']}")
        for line in failures:
            print(f"FALSIFIER-FAIL {line}")
        if failures:
            print("VERDICT: FAIL")
            return 1
        print(f"FALSIFIERS: {len(results)}/{len(results)} red")
        print("VERDICT: PASS")
        return 0

    err = Errors()
    validate_schema(obj, err)
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

    for item in err.items:
        print(f"FAIL {item['code']}: {item['detail']}")
    for line in falsifier_failures:
        print(f"FAIL FALSIFIER: {line}")

    print(f"REGISTER-DIGEST: sha256:{digest(obj, obj.get('prose_surface', []))}")
    if err.items or falsifier_failures:
        print("VERDICT: FAIL")
        return 1
    print("VERDICT: PASS")
    return 0


if __name__ == "__main__":
    sys.exit(main())
