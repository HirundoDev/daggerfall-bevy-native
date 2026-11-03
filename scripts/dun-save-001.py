#!/usr/bin/env python3
"""GATE DUN-SAVE-001 — M44 (DUN-1) block E acceptance: the save v3 document.

Spec under test: ``docs/sintergic/m44/blocks/E-save-v3-CR-7fd7260f.md``
CR:              CR-7fd7260f-ba3d-449a-aea9-e74386a2d5a3

What this gate measures, and how
--------------------------------
* **Artifact**: the *text* document a player's save has ever been
  (``DFL-SESSION n``). The binary ``DFSV`` envelope is a *derived* framing of the
  same M33 sections; it gets one consequence row's worth of evidence (SPINE-001)
  and no requirement of its own.
* **Rows** ``M44-R20..R24`` mix three instruments, each labelled on the row:
  ``probe_*`` live probe output, ``census_*`` a countable fact over the product
  sources, ``spec_*`` a byte-level check of the document's declared goldens
  recomputed from the live ``S0_TEXT``.
* **Two runs**: every probe is executed twice and the two captures must be
  byte-identical (``identical=1``). ``--self-test`` reuses the same evaluation
  over a *green* capture (v3 as if implemented) and each ``--falsify NAME``
  injects one defect back into it; a falsifier that does not flip its row is a
  hard error. So the rows are computed, not hardcoded.
* **Read-only**: nothing under ``crates/**`` is written; the gate never builds
  (the binary is prebuilt and hashed).

Exit codes: 0 PASS · 1 FAIL · 2 NC (Binary absent / harness incomplete).
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
import threading
import time
from dataclasses import dataclass, field, replace
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from m43_gate_lib import Gate, citation_check, file_sha256, sha256_hex  # noqa: E402

GATE = "DUN-SAVE-001"
CR = "CR-7fd7260f-ba3d-449a-aea9-e74386a2d5a3"
ROOT = Path(__file__).resolve().parent.parent
BLOCK_DOC = "docs/sintergic/m44/blocks/E-save-v3-CR-7fd7260f.md"

SOURCES = [
    "crates/app/src/m8_probe.rs",
    "crates/app/src/m8_session.rs",
    "crates/app/src/m8_window.rs",
    "crates/app/src/m33_probe.rs",
    "crates/app/src/spine_probe.rs",
    "crates/memory/src/save_v2.rs",
    "crates/memory/src/savegame_v2.rs",
    "crates/memory/src/session_store.rs",
    "scripts/dun-save-001.py",
]

# ── the v3 grammar, as data (one table = one source of truth) ───────────────

FAMILIES = (
    ("dungeon_state", ("location_id", "block_index", "door_id", "machine",
                       "since_minutes", "epoch")),
    ("dungeon_elig", ("token", "location_id", "container_id", "player_id",
                      "epoch", "seen")),
    ("dungeon_loot", ("location_id", "container_id", "source", "spawn_minutes",
                      "looted_minutes", "looted", "epoch")),
    ("dungeon_instance", ("instance_id", "location_id", "epoch", "player_id")),
)
FAMILY_FIELDS = {name: fields for name, fields in FAMILIES}
FAMILY_SET = set(FAMILY_FIELDS)
COUNT_KEYS = {name: name + "_count" for name in FAMILY_FIELDS}
COUNT_SET = set(COUNT_KEYS.values())

INT_RANGE = {
    "location_id": (0, 65535),
    "block_index": (0, 255),
    "door_id": (0, 4294967295),
    "container_id": (0, 4294967295),
    "player_id": (0, 255),
    "since_minutes": (0, 18446744073709551615),
    "spawn_minutes": (0, 18446744073709551615),
    "looted_minutes": (0, 18446744073709551615),
    "epoch": (1, 4294967295),
    "seen": (0, 1),
    "looted": (0, 1),
}
ENUMS = {
    "machine": ("Dormant", "Open", "Engaged", "Cleared", "Cooldown", "Closed"),
    "source": ("Chest", "Drop", "Boss"),
}
HEX16 = frozenset(("token", "instance_id"))
ORDER = {
    "dungeon_state": ("location_id", "block_index", "door_id"),
    "dungeon_elig": ("location_id", "container_id", "player_id", "epoch"),
    "dungeon_loot": ("location_id", "container_id", "epoch"),
    "dungeon_instance": ("location_id", "epoch", "instance_id"),
}
UINT = re.compile(r"(?:0|[1-9][0-9]*)")
HEX16_RE = re.compile(r"[0-9a-f]{16}")

# Declared goldens (the document must agree; the gate recomputes from S0_TEXT).
DECLARED = {
    "S0": ("85", "1705", "f793b743974d7f39",
           "e8a81cb851c33529fbcade0a9738b8a96b93ae99e2b6f7d6782113463b0199cd"),
    "FX-1": ("89", "1794", "a1199cefed185b41",
             "d77abd505c640d5cf2988708bb3af5ceb77f6f92b6056c97ddbfdd576f96f40d"),
    "FX-2": ("93", "1964", "75dc8bec9e9643f4",
             "63673c18e1ebd78400305efc60b66d777ff19f43c69cd70708425fb0b5439c27"),
    "FX-3": ("93", "2077", "6b3f925d0de96ae9",
             "c6529cbbd3777620f1392967bab9040f33d1c88b9fbebdaf08196e8a46016a29"),
}

# ── codec ──────────────────────────────────────────────────────────────────


def fnv1a64(data: bytes) -> int:
    h = 0xCBF29CE484222325
    for byte in data:
        h ^= byte
        h = (h * 0x100000001B3) & 0xFFFFFFFFFFFFFFFF
    return h


def fnv_hex(data: bytes) -> str:
    return "%016x" % fnv1a64(data)


def sha16(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()[:16]


def seal(payload: str) -> str:
    return payload + "checksum=" + fnv_hex(payload.encode()) + "\n"


def reseal(text: str) -> str:
    lines = text.splitlines()
    return seal("\n".join(lines[:-1]) + "\n")


def sub_once(text: str, old: str, new: str) -> str:
    hits = text.count(old)
    if hits != 1:
        raise SystemExit(f"VECTOR-BUILD: expected exactly 1 hit for {old!r}, got {hits}")
    return text.replace(old, new, 1)


# ── the reference v3 parser (the spec, executable) ─────────────────────────


def parse_entry(family: str, raw: str) -> tuple[dict | None, str | None]:
    fields = FAMILY_FIELDS[family]
    parts = raw.split(",")
    if len(parts) != len(fields):
        return None, "bad_field_count"
    out: dict = {}
    for name, part in zip(fields, parts):
        if name in HEX16:
            if not HEX16_RE.fullmatch(part):
                return None, "bad_hex"
            out[name] = part
            continue
        if name in ENUMS:
            if part not in ENUMS[name]:
                return None, "bad_enum"
            out[name] = part
            continue
        if not UINT.fullmatch(part):
            return None, "bad_int"
        number = int(part)
        low, high = INT_RANGE[name]
        if number < low or number > high:
            return None, "out_of_range"
        out[name] = number
    if family == "dungeon_loot":
        # DELTA-D1 (block D, M44-R16): minute 0 is the sentinel "never looted", so a
        # looted container can never carry minute 0 and a never-looted one can never
        # carry a minute. The grammar refuses the contradiction instead of letting a
        # consumer read a deadline out of contradictory state.
        if out["looted"] == 0 and out["looted_minutes"] != 0:
            return None, "bad_loot_minutes"
        if out["looted"] == 1 and out["looted_minutes"] == 0:
            return None, "bad_loot_minutes"
    return out, None


def parse_document(text: str, allowed_v2_keys: set[str]) -> tuple[dict | None, str | None]:
    """Strict, canonical parser of a `DFL-SESSION 2|3` document.

    Precedence (declared in the block doc §2.7): bad_magic → bad_version →
    no_checksum → bad_checksum → family_requires_v3 → unknown_key →
    missing_family → count_mismatch → duplicate_key → order → field token.
    """
    if not text:
        return None, "bad_magic"
    if not text.endswith("\n"):
        return None, "no_checksum"
    lines = text[:-1].split("\n")
    magic, sep, version_token = lines[0].partition(" ")
    if magic != "DFL-SESSION" or not sep:
        return None, "bad_magic"
    if not UINT.fullmatch(version_token):
        return None, "bad_version"
    version = int(version_token)
    if version not in (2, 3):
        return None, "bad_version"
    if len(lines) < 2:
        return None, "no_checksum"
    last = lines[-1]
    if not last.startswith("checksum="):
        return None, "no_checksum"
    declared = last[len("checksum="):]
    if not HEX16_RE.fullmatch(declared):
        return None, "bad_checksum"
    payload = "\n".join(lines[:-1]) + "\n"
    if fnv_hex(payload.encode()) != declared:
        return None, "bad_checksum"

    body = lines[1:-1]
    keys: list[str] = []
    dialect: list[str] = []
    for entry in body:
        key, eq, value = entry.partition("=")
        if not eq:
            return None, "unknown_key"
        keys.append(key)
        dialect.append(value)
    if version == 2 and any(k in FAMILY_SET or k in COUNT_SET for k in keys):
        return None, "family_requires_v3"
    for key in keys:
        if key not in allowed_v2_keys and key not in FAMILY_SET and key not in COUNT_SET:
            return None, "unknown_key"

    tail = next((i for i, k in enumerate(keys) if k in COUNT_SET), len(keys))
    entries: dict[str, list[dict]] = {}
    if version == 3 and tail == len(keys):
        return None, "missing_family"
    cursor = tail
    for family, _fields in FAMILIES:
        if version == 2:
            break
        if cursor >= len(keys):
            return None, "missing_family"
        count_key = COUNT_KEYS[family]
        if keys[cursor] != count_key:
            return None, ("order" if count_key in keys[cursor:] else "missing_family")
        if not UINT.fullmatch(dialect[cursor]):
            return None, "count_mismatch"
        declared_count = int(dialect[cursor])
        cursor += 1
        raw_entries: list[str] = []
        while cursor < len(keys) and keys[cursor] == family:
            raw_entries.append(dialect[cursor])
            cursor += 1
        if len(raw_entries) != declared_count:
            return None, "count_mismatch"
        parsed: list[dict] = []
        for raw in raw_entries:
            values, token = parse_entry(family, raw)
            if token is not None:
                return None, token
            parsed.append(values)
        sort_keys = [tuple(v[k] for k in ORDER[family]) for v in parsed]
        if len(set(sort_keys)) != len(sort_keys):
            return None, "duplicate_key"
        if sort_keys != sorted(sort_keys):
            return None, "order"
        entries[family] = parsed
    if cursor != len(keys):
        return None, ("order" if keys[cursor] in FAMILY_SET else "unknown_key")
    document = {"version": version, "prefix": body[:tail], "entries": entries}
    if emit_document(document) != text:
        return None, "non_canonical"
    return document, None


def emit_document(document: dict) -> str:
    out = ["DFL-SESSION %d" % document["version"]]
    out.extend(document["prefix"])
    for family, fields in FAMILIES:
        if family not in document["entries"]:
            continue
        values = document["entries"][family]
        out.append("%s=%d" % (COUNT_KEYS[family], len(values)))
        for value in values:
            out.append(family + "=" + ",".join(str(value[f]) for f in fields))
    return seal("\n".join(out) + "\n")


def migrate_v2_to_v3(text: str, allowed_v2_keys: set[str]) -> tuple[str | None, str | None]:
    document, token = parse_document(text, allowed_v2_keys)
    if token is not None:
        return None, token
    if document["version"] == 3:
        return text, None
    upgraded = dict(document)
    upgraded["version"] = 3
    upgraded["entries"] = {family: [] for family in FAMILY_FIELDS}
    return emit_document(upgraded), None


# ── fixtures: S0 from the live source, then FX-1/FX-2/FX-3 ─────────────────

S0_MARK = 'pub const S0_TEXT: &str = "\\\n'


def extract_s0() -> str | None:
    src = (ROOT / "crates/memory/src/savegame_v2.rs").read_text(encoding="utf-8")
    if S0_MARK not in src:
        return None
    start = src.index(S0_MARK) + len(S0_MARK)
    end = src.index('";', start)
    return src[start:end]


def build_fixture(s0: str, entries: dict[str, list[str]]) -> str:
    lines = s0.splitlines()
    if lines[-1][:9] != "checksum=":
        raise SystemExit("FIXTURE: S0 does not end with a checksum line")
    out = ["DFL-SESSION 3"]
    out.extend(lines[1:-1])
    for family, _fields in FAMILIES:
        values = entries.get(family, [])
        out.append("%s=%d" % (COUNT_KEYS[family], len(values)))
        out.extend("%s=%s" % (family, value) for value in values)
    return seal("\n".join(out) + "\n")


def sample_entries(text: str) -> dict[str, list[str]]:
    """Entries of a v3 sample, taken from its canonical lines."""
    entries: dict[str, list[str]] = {}
    for family, _fields in FAMILIES:
        entries[family] = [line[len(family) + 1:] for line in text.splitlines()
                           if line.startswith(family + "=")]
    return entries


def doc_negatives(fx1: str, fx2: str) -> list[tuple[str, str, str]]:
    """The eleven document-level negative vectors of block doc §4.1."""
    state = "dungeon_state=26381,0,26,Dormant,142561,1"
    loot = "dungeon_loot=26381,7,Chest,142561,0,0,1"
    loot_pair = "dungeon_loot_count=1\n" + loot + "\n"
    state_pair = ("dungeon_state_count=1\n" + state + "\n")
    fx1_checksum = fx1.splitlines()[-1][len("checksum="):]
    out: list[tuple[str, str, str]] = [
        ("N1", reseal(sub_once(fx2, ",Dormant,", ",sleeping,")), "bad_enum"),
        ("N2", reseal(sub_once(fx2, state, state[:-2] + ",0")), "out_of_range"),
        ("N3", reseal(sub_once(fx2, "dungeon_state_count=1", "dungeon_state_count=2")),
         "count_mismatch"),
        ("N4", "\n".join(fx2.splitlines()[:-1]) + "\nchecksum=" + fx1_checksum + "\n",
         "bad_checksum"),
        ("N5", reseal(sub_once(fx1, "checksum=", "dungeon_extra=0\nchecksum=")),
         "unknown_key"),
        ("N6", reseal(sub_once(fx2, loot_pair, "")), "missing_family"),
        ("N7", reseal(sub_once(fx2, state_pair,
                               "dungeon_state_count=2\n" + state + "\n" + state + "\n")),
         "duplicate_key"),
        ("N8", reseal(sub_once(fx2, state_pair,
                               "dungeon_state_count=2\n"
                               "dungeon_state=26381,0,27,Dormant,142561,1\n"
                               + state + "\n")), "order"),
        ("N9", reseal(sub_once(fx1, "DFL-SESSION 3\n", "DFL-SESSION 2\n")),
         "family_requires_v3"),
        ("N10", reseal(sub_once(fx2, state, ",".join(state.split(",")[:-1]))),
         "bad_field_count"),
        # DELTA-D1: a never-looted container carrying a loot minute is contradictory,
        # and the grammar names it instead of handing the consumer a fake deadline.
        ("N11", reseal(sub_once(fx2, loot, "dungeon_loot=26381,7,Chest,142561,5,0,1")),
         "bad_loot_minutes"),
    ]
    return out


# ── the block document: declared goldens, sample blocks, vector table ──────

DECL_ROW = re.compile(
    r"^\|\s*(S0|FX-[123])\s*\|\s*(\d+)\s*\|\s*(\d+)\s*\|\s*`([0-9a-f]{16})`\s*"
    r"\|\s*`([0-9a-f]{64})`\s*\|\s*$", re.M)
VEC_ROW = re.compile(
    r"^\|\s*(GV-E-\d\d)\s*\|\s*`([^`]+)`\s*\|\s*([a-z_]+)\s*\|\s*`([0-9a-f]{16})`\s*\|\s*$",
    re.M)
SAMPLE = re.compile(r"^```text (FX-[23])\n(.*?)^```$", re.S | re.M)


@dataclass
class DocSpec:
    present: bool = False
    raw: str = ""
    declared: dict = field(default_factory=dict)
    samples: dict = field(default_factory=dict)
    vectors: list = field(default_factory=list)


COMPAT_BLOCK = re.compile(r"<!-- v3-compat-defaults\n(.*?)-->", re.S)
COMPAT_ROW = re.compile(r"^(dungeon_[a-z_]+)=0$")


def compat_defaults(spec: DocSpec) -> int:
    """The four families the v3 reader must default when it loads a v2 save."""
    if not spec.present:
        return 0
    match = COMPAT_BLOCK.search(spec.raw)
    if match is None:
        return 0
    declared = [COMPAT_ROW.fullmatch(line.strip()) for line in match.group(1).splitlines()
                if line.strip()]
    names = [m.group(1) for m in declared if m is not None]
    return len({name for name in names if name in FAMILY_FIELDS})


def read_doc() -> DocSpec:
    path = ROOT / BLOCK_DOC
    spec = DocSpec()
    if not path.is_file():
        return spec
    spec.present = True
    text = path.read_text(encoding="utf-8")
    spec.raw = text
    for match in DECL_ROW.finditer(text):
        spec.declared[match.group(1)] = (match.group(2), match.group(3),
                                         match.group(4), match.group(5))
    for match in SAMPLE.finditer(text):
        spec.samples[match.group(1)] = match.group(2)
    for match in VEC_ROW.finditer(text):
        spec.vectors.append((match.group(1), match.group(2), match.group(3),
                             match.group(4)))
    return spec


# ── probes ─────────────────────────────────────────────────────────────────


@dataclass
class Capture:
    stdout: str = ""
    code: int = -1
    killed: bool = False

    def text(self) -> str:
        keep = []
        for line in self.stdout.splitlines():
            if re.match(r"^\d{4}-\d\d-\d\dT", line):
                continue
            keep.append(line)
        return "\n".join(keep)


def run_capture(argv: list[str], cwd: Path, timeout: float,
                markers: tuple[str, ...] = ()) -> Capture:
    proc = subprocess.Popen(argv, cwd=str(cwd), stdout=subprocess.PIPE,
                            stderr=subprocess.STDOUT)
    killer = threading.Timer(timeout, proc.kill)
    killer.start()
    collected: list[str] = []
    killed = False
    if proc.stdout is None:
        raise SystemExit("PROBE: no stdout pipe")
    try:
        while True:
            raw = proc.stdout.readline()
            if not raw:
                break
            line = raw.decode("utf-8", "replace")
            collected.append(line)
            if markers and any(marker in line for marker in markers):
                proc.kill()
                killed = True
                break
        rest = proc.stdout.read()
        if rest:
            collected.append(rest.decode("utf-8", "replace"))
    finally:
        killer.cancel()
    code = proc.wait()
    return Capture(stdout="".join(collected), code=code, killed=killed)


MAGIC = "DFL-SESSION"


def save_file_rel() -> Path:
    """The player's save path, read from the product instead of hard-coded."""
    src = (ROOT / "crates/app/src/m8_session.rs").read_text(encoding="utf-8")
    match = find(r'^pub const SAVE_FILE_REL: &str = "([^"]+)";', src)
    if match is None:
        raise SystemExit("PROBE: SAVE_FILE_REL absent in crates/app/src/m8_session.rs")
    return Path(match.group(1))


def find(pattern: str, text: str) -> re.Match | None:
    return re.search(pattern, text, re.MULTILINE)


# ── source derivation (pure) ───────────────────────────────────────────────
#
# The census rows this lane repairs (R22 coverage, R23 idempotence, R24a reader)
# are derived here and nowhere else: from the *product* sources, never from the
# golden text and never from a literal typed by hand.  Because the derivation is
# a pure function, ``--self-test`` drives it with synthetic sources — which is
# what makes a defect inside it visible to the oracle (a typed ``GREEN`` literal
# could not see one).

CONST_DECL = re.compile(r"^pub const ([A-Z][A-Z0-9_]*): (?:i64|usize) = (\d+);", re.M)
ARM_HEAD = re.compile(r"^[ \t]*([A-Za-z_][\w:]*(?:\([^)]*\))?|[0-9]+)[ \t]*=>[ \t]*", re.M)


def source_constants(src: str) -> dict:
    """`pub const NAME: i64|usize = N;` declarations of one source file."""
    return {m.group(1): int(m.group(2)) for m in CONST_DECL.finditer(src)}


def resolve_version(token: str, memory: dict, store: dict):
    """Resolve `3` / `VERSION` / `crate::session_store::FORMAT_VERSION` to its value."""
    token = token.strip()
    if token.isdigit():
        return int(token)
    name = token.rsplit("::", 1)[-1]
    if token.startswith("crate::session_store::"):
        return store.get(name)
    if name in memory:
        return memory[name]
    return store.get(name)


def balanced_block(src: str, anchor: str):
    """Body of the `{...}` opening after `anchor` (string literals and `//` skipped)."""
    start = src.find(anchor)
    if start < 0:
        return None
    open_at = src.find("{", start)
    if open_at < 0:
        return None
    depth = 0
    index = open_at
    while index < len(src):
        char = src[index]
        if char == '"':
            index += 1
            while index < len(src) and src[index] != '"':
                index += 2 if src[index] == "\\" else 1
        elif src.startswith("//", index):
            newline = src.find("\n", index)
            index = len(src) - 1 if newline < 0 else newline
        elif char == "{":
            depth += 1
        elif char == "}":
            depth -= 1
            if depth == 0:
                return src[open_at + 1:index]
        index += 1
    return None


def match_arms(body: str) -> list:
    """`(pattern, arm body)` of every arm of the `match` block inside `body`."""
    arms = []
    for match in ARM_HEAD.finditer(body):
        tail = body[match.end():]
        if tail.startswith("{"):
            arm = balanced_block(tail, "{") or ""
        else:
            newline = tail.find("\n")
            arm = tail if newline < 0 else tail[:newline]
        arms.append((match.group(1), arm))
    return arms


def arm_state(arm: str) -> str:
    """What the writer's `migrate_text` arm publishes.  Unrecognised is never v3."""
    if not arm:
        return "absent"
    if "v3" in arm or "dungeon" in arm or "DFL-SESSION 3" in arm:
        return "publishes_v3"
    if "SaveGameV2::from_text" in arm and "to_text()" in arm:
        return "identity"
    return "unclassified"


def migration_arm(memory_src: str, memory: dict, store: dict):
    """`(writer version, arm body, state)` for the writer's version in `migrate_text`."""
    writer = memory.get("VERSION")
    body = balanced_block(memory_src, "pub fn migrate_text") or ""
    for pattern, arm in match_arms(body):
        if resolve_version(pattern, memory, store) == writer:
            return writer, arm, arm_state(arm)
    return writer, "", "absent"


def reader_versions(memory_src: str, memory: dict, store: dict) -> tuple:
    """The version set the *reader path* accepts.

    Two independent points of acceptance, both read from the reader and both with
    every cited constant resolved to its value: the arms of `migrate_text` and the
    guard of `SaveGameV2::from_text`.
    """
    accepted = set()
    body = balanced_block(memory_src, "pub fn migrate_text") or ""
    for pattern, _arm in match_arms(body):
        value = resolve_version(pattern, memory, store)
        if value is not None:
            accepted.add(value)
    guard = balanced_block(memory_src, "pub fn from_text") or ""
    for match in re.finditer(r"if\s+version\s*(?:!=|==)\s*([A-Za-z_][\w:]*|[0-9]+)", guard):
        value = resolve_version(match.group(1), memory, store)
        if value is not None:
            accepted.add(value)
    return tuple(sorted(accepted))


@dataclass
class SourceFacts:
    writer_version: int = 0
    player_save_version: int = 0
    writer_absent: bool = False
    format_absent: bool = False
    unit_test_v3: int = 0
    migrate_arm_state: str = "absent"
    migrate_arm_identity: bool = False
    migrated_v3: int = 0
    idempotent_v3: int = 0
    idempotent_cause: str = ""
    families_present: int = 0
    families_v2: int = 0
    families_wired: int = 0
    families_roundtripped: int = 0
    families_required: int = 18
    reader_versions: tuple = ()
    reader_version: int = 0
    v3_reads_v2: int = 0


IDEMPOTENCE_CAUSE = {
    "identity": "migrate_text VERSION arm re-emits v2 (identity)",
    "absent": "migrate_text has no arm for VERSION",
    "unclassified": "migrate_text VERSION arm unrecognised (no v3 fixed point)",
    "publishes_v3": "migrate_text VERSION arm re-parses and re-emits v3 (fixed point)",
}

SOURCE_FIELDS = (
    "writer_version", "player_save_version", "unit_test_v3", "migrate_arm_state",
    "migrate_arm_identity", "migrated_v3", "idempotent_v3", "idempotent_cause",
    "families_present", "families_v2", "families_wired", "families_roundtripped",
    "families_required", "reader_versions", "reader_version", "v3_reads_v2")


def derive_source_facts(memory_src: str, store_src: str, corpus: str) -> SourceFacts:
    """Every census fact, derived from source text only (pure: the self-test oracle).

    ``families_roundtripped`` (R22) is what the *product document* covers:
    `pub const COUNT_KEYS` — the codec's own declared count-key capacity — plus the
    grammar families whose count key the document actually writes.  It is never the
    number of `_count=` lines inside the S0 golden: S0 is the *v2* golden (85 lines,
    14 counts), so demanding 18 inside it would demand 89 lines, which is exactly
    FX-1, and the measurement would then be reading the fixture it is meant to
    verify.  ``families_required`` stays 18: the criterion did not move.

    ``idempotent_v3`` (R23) is always assigned and always >= 0: 3 only when the
    writer's `migrate_text` arm publishes v3 (re-parse/re-emit, a fixed point by
    construction); 0 otherwise, with the cause named.  The old -1 sentinel made
    "not measured" indistinguishable from "measured zero".

    ``reader_version`` (R24a) comes from the reader path's own acceptance points
    (`migrate_text` arms + the `SaveGameV2::from_text` guard), with each cited
    constant resolved to its value — a different source from ``writer_version``,
    which comes from `pub const VERSION`.  Either one can now break alone.
    """
    memory = source_constants(memory_src)
    store = source_constants(store_src)
    src = SourceFacts()
    src.writer_absent = "VERSION" not in memory
    src.writer_version = memory.get("VERSION", 0)
    src.format_absent = "FORMAT_VERSION" not in store
    src.player_save_version = store.get("FORMAT_VERSION", 0)
    src.unit_test_v3 = 1 if "BadVersion(3)" in memory_src else 0
    _writer, arm, state = migration_arm(memory_src, memory, store)
    src.migrate_arm_state = state
    src.migrate_arm_identity = state == "identity"
    src.migrated_v3 = 3 if state == "publishes_v3" else 0
    src.idempotent_v3 = 3 if state == "publishes_v3" else 0
    src.idempotent_cause = IDEMPOTENCE_CAUSE[state]
    src.families_present = sum(1 for family in FAMILY_FIELDS if family in corpus)
    src.families_v2 = memory.get("COUNT_KEYS", 0)
    src.families_wired = sum(1 for name in FAMILY_FIELDS if COUNT_KEYS[name] in memory_src)
    src.families_roundtripped = src.families_v2 + src.families_wired
    src.reader_versions = reader_versions(memory_src, memory, store)
    src.reader_version = max(src.reader_versions) if src.reader_versions else 0
    src.v3_reads_v2 = 1 if src.reader_version >= 3 else 0
    return src


def apply_source_facts(facts: "Facts", src: SourceFacts) -> "Facts":
    for name in SOURCE_FIELDS:
        setattr(facts, name, getattr(src, name))
    return facts


# ── facts ──────────────────────────────────────────────────────────────────


@dataclass
class Facts:
    # artifact / version
    writer_version: int = 0
    player_save_version: int = 1
    product_save_bytes: int = 0
    anchor: str = "source"
    # families
    families_present: int = 0
    vectors_ok: int = 0
    vectors_total: int = 0
    vectors_expected: int = 30
    negatives_ok: int = 0
    negatives_total: int = 11
    # DELTA-D1 coverage: the goldens must carry both halves of the sentinel.
    loot_minutes_zero: int = 0
    loot_minutes_positive: int = 0
    # round trip
    roundtrip_ok: int = 0
    roundtrip_total: int = 0
    families_roundtripped: int = 0
    families_required: int = 18
    families_v2: int = 0
    families_wired: int = 0
    sample_sha_ok: bool = False
    sample_declared: str = ""
    sample_computed: str = ""
    identity: bool = False
    fixtures_ok: int = 0
    fixtures_total: int = 3
    # migration
    v1_to_v2: int = 0
    idempotent_v2: int = 0
    migrate_arm_identity: bool = False
    migrate_arm_state: str = "absent"
    migrated_v3: int = 0
    # R23: never a sentinel. "not measured" must be impossible, so 0 (with its
    # cause in `idempotent_cause`) is a measurement, not a hole.
    idempotent_v3: int = 0
    idempotent_cause: str = ""
    # compat
    reader_version: int = 0
    reader_versions: tuple = ()
    v3_reads_v2: int = 0
    defaults_declared: int = 0
    v2_reads_v3: str = "unknown"
    unit_test_v3: int = 0
    probe_rejected: int = 0


def collect(binary: Path, timeout: float, use_display: bool) -> tuple[Facts, list[str], list[str]]:
    """Measure. Returns (facts, harness lines, notes)."""
    facts = Facts()
    harness: list[str] = []
    notes: list[str] = []

    memory_src = (ROOT / "crates/memory/src/savegame_v2.rs").read_text(encoding="utf-8")
    store_src = (ROOT / "crates/memory/src/session_store.rs").read_text(encoding="utf-8")

    # R22 / R23 / R24a: all three census rows come from one pure derivation.
    corpus = []
    for path in sorted((ROOT / "crates").rglob("*.rs")):
        try:
            corpus.append(path.read_text(encoding="utf-8", errors="replace"))
        except OSError:
            continue
    joined = "\n".join(corpus)
    src = derive_source_facts(memory_src, store_src, joined)
    if src.writer_absent:
        notes.append("[FAIL] HARNESS - version_constant_absent file=crates/memory/src/savegame_v2.rs")
    if src.format_absent:
        notes.append("[FAIL] HARNESS - format_version_absent file=crates/memory/src/session_store.rs")
    apply_source_facts(facts, src)

    # ── probe 1: SAVE-ALL-001 (v2 golden, round trip, corruption, escaping)
    runs = [run_capture([str(binary), "--probe", "SAVE-ALL-001"], ROOT, timeout) for _ in (0, 1)]
    same = runs[0].text() == runs[1].text()
    text = runs[0].text()
    roundtrip = find(r"^\[OK\] roundtrip: (\d+)/(\d+)", text)
    if roundtrip is None:
        notes.append("[FAIL] HARNESS - roundtrip_row_absent probe=SAVE-ALL-001")
    else:
        facts.roundtrip_ok, facts.roundtrip_total = int(roundtrip.group(1)), int(roundtrip.group(2))
    s0_row = find(r"^\[OK\] S0 lines=(\d+) bytes=(\d+) checksum=([0-9a-f]{16})$", text)
    all_digest = find(r"^digest=([0-9a-f]{16}) ", text)
    harness.append(
        "PROBE SAVE-ALL-001 runs=2 identical=%d exit=%d roundtrip=%d/%d s0=%s digest=%s"
        % (int(same), runs[0].code, facts.roundtrip_ok, facts.roundtrip_total,
           ("%s/%s/%s" % s0_row.groups()) if s0_row else "absent",
           all_digest.group(1) if all_digest else "absent"))
    if not same:
        notes.append("[FAIL] HARNESS - probe_nondeterministic probe=SAVE-ALL-001")

    # ── probe 2: SAVE-MIGRATION-001 (v1→v2 ×3, idempotence)
    runs = [run_capture([str(binary), "--probe", "SAVE-MIGRATION-001"], ROOT, timeout) for _ in (0, 1)]
    same = runs[0].text() == runs[1].text()
    text = runs[0].text()
    migrated = find(r"^\[OK\] migration: (\d+)/(\d+) v1.*idempotente (\d+)/(\d+)", text)
    if migrated is None:
        notes.append("[FAIL] HARNESS - migration_row_absent probe=SAVE-MIGRATION-001")
    else:
        facts.v1_to_v2 = int(migrated.group(1))
        facts.idempotent_v2 = int(migrated.group(3))
    migration_digest = find(r"\bdigest=([0-9a-f]{16})\b", text)
    harness.append(
        "PROBE SAVE-MIGRATION-001 runs=2 identical=%d exit=%d v1_to_v2=%d/%d idempotent=%d/%d digest=%s"
        % (int(same), runs[0].code, facts.v1_to_v2, int(migrated.group(2)) if migrated else 0,
           facts.idempotent_v2, int(migrated.group(4)) if migrated else 0,
           migration_digest.group(1) if migration_digest else "absent"))
    if not same:
        notes.append("[FAIL] HARNESS - probe_nondeterministic probe=SAVE-MIGRATION-001")

    # ── probe 3: SPINE-001 (the envelope is 12 sections; evidence only)
    runs = [run_capture([str(binary), "--probe", "SPINE-001"], ROOT, timeout) for _ in (0, 1)]
    same = runs[0].text() == runs[1].text()
    text = runs[0].text()
    envelope = find(r"envelope roundtrip digest1=([0-9a-f]{16}) digest2=([0-9a-f]{16}) "
                    r"identical=true sections=(\d+)/(\d+)", text)
    harness.append(
        "PROBE SPINE-001 runs=2 identical=%d exit=%d envelope=%s sections=%s"
        % (int(same), runs[0].code,
           ("%s/%s" % (envelope.group(1), envelope.group(2))) if envelope else "absent",
           ("%s/%s" % (envelope.group(3), envelope.group(4))) if envelope else "absent"))
    if not same:
        notes.append("[FAIL] HARNESS - probe_nondeterministic probe=SPINE-001")

    # ── probe 4: SESSION-001 in a temp cwd (the save path is CWD-relative).
    # Two independent runs in two independent directories: the second run must not
    # inherit the first run's save, so what is compared is each run's text with its
    # own cwd replaced by <CWD>, plus the two save files byte for byte.
    save_rel = save_file_rel()
    dirs = [Path(tempfile.mkdtemp(prefix="dun-save-001-")) for _ in (0, 1)]
    try:
        argv = [str(binary), "--probe", "SESSION-001"]
        if not use_display:
            notes.append("[NOT-RUN] PROC-SESSION display=absent anchor=source census "
                         "crates/memory/src/session_store.rs:58")
        else:
            caps = [run_capture(argv, d, timeout, markers=("M8 VE: step 2",)) for d in dirs]
            # Two things in a SESSION capture are non-semantic: the cwd the gate
            # chose and the probe's own hermetic mkdtemp. Both are collapsed so the
            # comparison is about the product's numbers, nothing else.
            norm = [re.sub(r"/tmp/daggerfall-m8-probe-[0-9]+", "<PROBE-TMP>",
                           cap.text().replace(str(d), "<CWD>"))
                    for cap, d in zip(caps, dirs)]
            blobs = [d / save_rel for d in dirs]
            saved = [b.read_bytes() for b in blobs if b.is_file()]
            rej = find(r"unsupported_version_is_rejected - save with version (\d+) ⇒ "
                       r"SaveError::BadVersion\(\d+\)", caps[0].text())
            if rej is not None:
                facts.probe_rejected = int(rej.group(1))
            if saved:
                raw = saved[0]
                facts.product_save_bytes = len(raw)
                doc = raw.decode("utf-8", "replace")
                head = doc.splitlines()[0] if doc else ""
                head_magic, _, head_ver = head.partition(" ")
                if head_magic == MAGIC and UINT.fullmatch(head_ver):
                    facts.player_save_version = int(head_ver)
                    facts.anchor = "product"
                lines = doc.splitlines()
                payload = "\n".join(lines[:-1]) + "\n" if len(lines) > 1 else ""
                declared = ""
                if lines and lines[-1].startswith("checksum="):
                    declared = lines[-1][len("checksum="):]
                computed = fnv_hex(payload.encode())
                harness.append(
                    "SPEC product_save bytes=%d version=%d fnv=%s declared=%s agree=%d"
                    % (len(raw), facts.player_save_version, computed,
                       declared or "absent", int(declared == computed)))
                if len(saved) != 2:
                    notes.append("[FAIL] HARNESS - probe_save_absent runs=%d/2" % len(saved))
                elif saved[0] != saved[1]:
                    notes.append("[FAIL] HARNESS - probe_save_nondeterministic")
            else:
                notes.append("[FAIL] HARNESS - probe_save_absent runs=0/2")
            harness.append(
                "PROBE SESSION-001 runs=2 identical=%d exit=%d version_rejected=%d "
                "product_save=%d/%d anchor=%s"
                % (int(norm[0] == norm[1]), caps[0].code, facts.probe_rejected,
                   facts.product_save_bytes, facts.player_save_version, facts.anchor))
            if norm[0] != norm[1]:
                notes.append("[FAIL] HARNESS - probe_nondeterministic probe=SESSION-001")
    finally:
        for d in dirs:
            shutil.rmtree(d, ignore_errors=True)

    if facts.probe_rejected and facts.anchor == "product":
        facts.v2_reads_v3 = "named_error"
    elif facts.unit_test_v3:
        facts.v2_reads_v3 = "named_error"

    # ── spec: S0 from the live source, then the three fixtures
    s0 = extract_s0()
    if s0 is None:
        notes.append("[FAIL] HARNESS - s0_absent file=crates/memory/src/savegame_v2.rs:2310")
        return facts, harness, notes
    allowed = {line.partition("=")[0] for line in s0.splitlines()[1:-1]}
    # R22 is measured inside `derive_source_facts` (product-document coverage),
    # never over this golden: S0 is the v2 golden and its 14 counts are not the
    # codec's capacity.
    spec = read_doc()
    if s0_row is not None and s0_row.groups() == (str(len(s0.splitlines())),
                                                  str(len(s0.encode())),
                                                  fnv_hex(("\n".join(s0.splitlines()[:-1]) + "\n").encode())):
        runtime = "agree"
    else:
        runtime = "disagree"
    harness.append(
        "SPEC S0 lines=%d bytes=%d checksum=%s sha256=%s runtime=%s declared=%s"
        % (len(s0.splitlines()), len(s0.encode()),
           fnv_hex(("\n".join(s0.splitlines()[:-1]) + "\n").encode()), sha16(s0.encode()),
           runtime, DECLARED["S0"][2]))
    if runtime != "agree":
        notes.append("[FAIL] HARNESS - s0_runtime_disagrees")

    fx1 = build_fixture(s0, {})
    ok = 0
    for name, text in (("FX-1", fx1), ("FX-2", spec.samples.get("FX-2", "")),
                       ("FX-3", spec.samples.get("FX-3", ""))):
        if not text:
            harness.append("SPEC %s absent=sample-block" % name)
            continue
        lines = text.splitlines()
        payload = "\n".join(lines[:-1]) + "\n"
        checksum = fnv_hex(payload.encode())
        declared = spec.declared.get(name, DECLARED[name])
        agree = (declared[0] == str(len(lines)) and declared[1] == str(len(text.encode()))
                 and declared[2] == checksum and declared[3] == hashlib.sha256(text.encode()).hexdigest())
        document, token = parse_document(text, allowed)
        identity = token is None and emit_document(document) == text
        if name == "FX-2":
            facts.sample_sha_ok = agree
            facts.sample_declared = declared[3][:16]
            facts.sample_computed = hashlib.sha256(text.encode()).hexdigest()[:16]
            facts.identity = identity
        if name == "FX-1" and text == fx1:
            pass
        ok += int(agree)
        harness.append("SPEC %s lines=%d bytes=%d checksum=%s sha256=%s declared=%s identity=%d"
                       % (name, len(lines), len(text.encode()), checksum,
                          hashlib.sha256(text.encode()).hexdigest()[:16], declared[3][:16],
                          int(identity)))
    facts.fixtures_ok = ok
    algorithm, algorithm_token = migrate_v2_to_v3(s0, allowed)
    algorithm_idempotent = algorithm is not None and migrate_v2_to_v3(algorithm, allowed)[0] == algorithm
    harness.append(
        "SPEC algorithm migrate(S0)=fx1 match=%d idempotent=%d token=%s"
        % (int(algorithm == fx1), int(algorithm_idempotent),
           algorithm_token if algorithm_token else "accept"))
    if algorithm != fx1 or not algorithm_idempotent:
        notes.append("[FAIL] HARNESS - migration_algorithm_disagrees_with_fixture")
    if spec.samples.get("FX-2") and spec.samples.get("FX-3"):
        fx2 = spec.samples["FX-2"]
        expected_fx2 = build_fixture(s0, sample_entries(fx2))
        if expected_fx2 != fx2:
            notes.append("[FAIL] HARNESS - fx2_not_derivable_from_s0")
        if build_fixture(s0, sample_entries(spec.samples["FX-3"])) != spec.samples["FX-3"]:
            notes.append("[FAIL] HARNESS - fx3_not_derivable_from_s0")

    # ── vectors: the document's table, judged by the reference parser
    accept = reject = 0
    for gid, line, token_want, digest in spec.vectors:
        family, _, raw = line.partition("=")
        if family not in FAMILY_FIELDS:
            notes.append("[FAIL] HARNESS - vector_family_unknown id=%s" % gid)
            continue
        _values, token_got = parse_entry(family, raw)
        want = "accept" if token_want == "accept" else token_want
        got = "accept" if token_got is None else token_got
        if got == want and sha16(line.encode()) == digest:
            facts.vectors_ok += 1
        else:
            notes.append("[FAIL] SPEC vector_mismatch id=%s want=%s got=%s digest=%s"
                         % (gid, want, got, sha16(line.encode())))
        if want == "accept" and family == "dungeon_loot" and got == "accept":
            minutes = _values["looted_minutes"]
            facts.loot_minutes_zero += int(minutes == 0)
            facts.loot_minutes_positive += int(minutes > 0)
        accept += int(want == "accept")
        reject += int(want != "accept")
    facts.vectors_total = len(spec.vectors)
    harness.append("SPEC vectors=%d/%d declared=%d accept=%d reject=%d"
                   % (facts.vectors_ok, facts.vectors_total, facts.vectors_expected, accept, reject))

    # ── document-level negatives (built from the goldens, not hand-typed)
    if spec.samples.get("FX-2"):
        fx2 = spec.samples["FX-2"]
        for nid, text, want in doc_negatives(fx1, fx2):
            _document, token = parse_document(text, allowed)
            if token == want:
                facts.negatives_ok += 1
            else:
                notes.append("[FAIL] SPEC negative_mismatch id=%s want=%s got=%s"
                             % (nid, want, token or "accept"))
    harness.append("SPEC negatives=%d/%d" % (facts.negatives_ok, facts.negatives_total))
    harness.append(
        "CENSUS v3_families_in_crates=%d/4 v3_header_emitted=%d v3_header_fixtures=%d "
        "families_v2=%d families_wired=%d families_total=%d/%d "
        "reader_accepts=%s reader_version=%d writer_version=%d migrate_v2_arm=%s "
        "migrated_v3=%d/3 idempotent_v3=%d/3 cause=%s unit_test_v3=%d"
        % (facts.families_present, int(facts.writer_version == 3),
           joined.count("DFL-SESSION 3"), facts.families_v2, facts.families_wired,
           facts.families_roundtripped, facts.families_required,
           ",".join(str(v) for v in facts.reader_versions) or "none",
           facts.reader_version, facts.writer_version, facts.migrate_arm_state,
           facts.migrated_v3, facts.idempotent_v3, facts.idempotent_cause,
           facts.unit_test_v3))
    facts.defaults_declared = compat_defaults(spec)
    if facts.defaults_declared != 4:
        notes.append("[FAIL] SPEC defaults_declared=%d/4 (block doc §4.3 "
                     "`v3-compat-defaults` block)" % facts.defaults_declared)
    return facts, harness, notes


# ── rows ───────────────────────────────────────────────────────────────────


@dataclass
class Row:
    ident: str
    label: str
    ok: bool
    facts: str
    expected: str
    src: str
    symptom: str
    falsifier: str


def evaluate(f: Facts) -> list[Row]:
    rows: list[Row] = []

    ok = f.writer_version == 3 and f.player_save_version == 3
    rows.append(Row(
        "M44-R20", "version_is_three", ok,
        "writer=%d player_save=%d anchor=%s"
        % (f.writer_version, f.player_save_version, f.anchor),
        "3",
        "crates/memory/src/savegame_v2.rs:70",
        "the writer and the product-path save both still say 2/1, never 3",
        "r20-writer-two"))

    ok = (f.families_present == 4 and f.vectors_ok == f.vectors_total == f.vectors_expected
          and f.negatives_ok == f.negatives_total
          and f.loot_minutes_zero >= 1 and f.loot_minutes_positive >= 1)
    rows.append(Row(
        "M44-R21", "families_closed", ok,
        "families=%d/4 vectors=%d/%d negatives=%d/%d loot_minutes_zero=%d "
        "loot_minutes_gt0=%d"
        % (f.families_present, f.vectors_ok, f.vectors_total, f.negatives_ok,
           f.negatives_total, f.loot_minutes_zero, f.loot_minutes_positive),
        "families=4 vectors=%d/%d loot_minutes_zero=1 loot_minutes_gt0=1"
        % (f.vectors_expected, f.vectors_expected),
        "crates/memory/src/savegame_v2.rs:1968",
        "no dungeon_* family exists in the product sources (0/4); the spec's "
        "grammar is closed and vector-verified, the product has nothing to persist it",
        "r21-families-three"))

    ok = (f.sample_sha_ok and f.identity and f.families_roundtripped == f.families_required)
    rows.append(Row(
        "M44-R22", "byte_identical_roundtrip", ok,
        "sample_sha256=%s declared=%s identity=%d roundtrip=%d/%d "
        "v2=%d wired=%d total=%d/%d"
        % (f.sample_computed, f.sample_declared, int(f.identity), f.roundtrip_ok,
           f.roundtrip_total, f.families_v2, f.families_wired,
           f.families_roundtripped, f.families_required),
        "total=%d" % f.families_required,
        "crates/memory/src/savegame_v2.rs:2047",
        "the v3 sample is byte-canonical and its digest matches, but the product "
        "document covers only the v2 codec's families: the 4 dungeon_* families "
        "are not wired into crates/memory/src/savegame_v2.rs",
        "r22-coverage"))

    ok = (f.migrated_v3 == 3 and f.idempotent_v3 == 3)
    rows.append(Row(
        "M44-R23", "v2_to_v3_migration", ok,
        "v1_to_v2=%d/%d idempotent_v2=%d/%d arm=%s migrated_v3=%d/3 "
        "idempotent_v3=%d/3 cause=%s"
        % (f.v1_to_v2, f.fixtures_total, f.idempotent_v2, f.fixtures_total,
           f.migrate_arm_state, f.migrated_v3, f.idempotent_v3, f.idempotent_cause),
        "migrated_v3=3 idempotent_v3=3",
        "crates/memory/src/savegame_v2.rs:2219",
        "the v2 arm of migrate_text re-emits v2 byte-identically: a v2 document "
        "never becomes a v3 document (0 of 3 declared fixtures)",
        "r23-migration"))

    ok = f.v3_reads_v2 == 1 and f.defaults_declared == 4
    rows.append(Row(
        "M44-R24", "v3_reader_reads_v2", ok,
        "v3_reader=%d reader_accepts=%s reader_version=%d defaults_declared=%d"
        % (f.v3_reads_v2,
           ",".join(str(v) for v in f.reader_versions) or "none",
           f.reader_version, f.defaults_declared),
        "v3_reader=1 defaults_declared=4",
        "crates/memory/src/savegame_v2.rs:2000",
        "the reader pins version 2 and rejects 3: no v3 reader exists to apply "
        "the declared empty-family defaults to an old save",
        "r24a-reader-missing"))

    ok = f.v2_reads_v3 == "named_error" and f.unit_test_v3 >= 1
    rows.append(Row(
        "M44-R24", "v2_reader_names_v3", ok,
        "v2_reads_v3=%s version=3 error=BadVersion unit_test=%d probe_rejected=%d"
        % (f.v2_reads_v3, f.unit_test_v3, f.probe_rejected),
        "v2_reads_v3=named_error unit_test=1",
        "crates/memory/src/savegame_v2.rs:2667",
        "a v3 document must be refused by name, never diagnosed as Corrupt",
        "r24b-named-error"))
    return rows


GREEN_PROBE = Facts(
    player_save_version=3, product_save_bytes=454, anchor="product",
    vectors_ok=30, vectors_total=30, vectors_expected=30,
    negatives_ok=11, negatives_total=11,
    loot_minutes_zero=1, loot_minutes_positive=1,
    roundtrip_ok=24, roundtrip_total=24,
    sample_sha_ok=True, sample_declared="63673c18e1ebd784",
    sample_computed="63673c18e1ebd784",
    identity=True, fixtures_ok=3, fixtures_total=3,
    v1_to_v2=3, idempotent_v2=3,
    defaults_declared=4, v2_reads_v3="named_error", probe_rejected=999)


# ── the oracle: synthetic sources, so a defect in the derivation is visible ──
#
# `--self-test` no longer evaluates a hand-typed `Facts`: it derives the census
# facts from synthetic sources through the same `derive_source_facts` the live
# run uses, then pins what each source shape MUST derive.  Reverting any of the
# three repairs changes the derivation, the pin disagrees, and the self-test goes
# red — which is what the three defects (invisible behind a typed literal)
# needed.

GREEN_MEMORY = '''pub const VERSION: i64 = 3;
pub const COUNT_KEYS: usize = 14;

impl SaveGameV3 {
    pub fn from_text(text: &str) -> Result<Self, SaveError> {
        if version != VERSION {
            return Err(SaveError::BadVersion(version));
        }
        Ok(Self {})
    }
}

pub fn migrate_text(text: &str) -> Result<String, SaveError> {
    match version {
        crate::session_store::FORMAT_VERSION => {
            let snapshot = SessionSnapshot::from_text(text)?;
            Ok(SaveGameV3::from_v2(&snapshot).to_v3_text())
        }
        VERSION => {
            let doc = SaveGameV3::from_text(text)?;
            Ok(doc.to_v3_text())
        }
        other => Err(SaveError::BadVersion(other)),
    }
}

fn write_dungeon_families(out: &mut Vec<String>) {
    push_pair(out, "dungeon_state_count", 0);
    push_pair(out, "dungeon_elig_count", 0);
    push_pair(out, "dungeon_loot_count", 0);
    push_pair(out, "dungeon_instance_count", 0);
}

fn a_v3_document_is_refused_by_name() {
    assert!(matches!(SaveGameV2::from_text(&v3_text()), Err(SaveError::BadVersion(3))));
}
'''

V2_MEMORY = '''pub const VERSION: i64 = 2;
pub const COUNT_KEYS: usize = 14;

impl SaveGameV2 {
    pub fn from_text(text: &str) -> Result<Self, SaveError> {
        if version != VERSION {
            return Err(SaveError::BadVersion(version));
        }
        Ok(Self {})
    }
}

pub fn migrate_text(text: &str) -> Result<String, SaveError> {
    match version {
        crate::session_store::FORMAT_VERSION => {
            let snapshot = SessionSnapshot::from_text(text)?;
            Ok(SaveGameV2::from_v1(&snapshot).to_text())
        }
        VERSION => {
            let doc = SaveGameV2::from_text(text)?;
            Ok(doc.to_text())
        }
        other => Err(SaveError::BadVersion(other)),
    }
}
'''

# Writer bumped to 3, reader still pinned to 2 through its own constant: the
# case that proves writer_version and reader_version are two measurements.
READER_BEHIND_MEMORY = '''pub const VERSION: i64 = 3;
pub const LEGACY_VERSION: i64 = 2;
pub const COUNT_KEYS: usize = 14;

impl SaveGameV2 {
    pub fn from_text(text: &str) -> Result<Self, SaveError> {
        if version != LEGACY_VERSION {
            return Err(SaveError::BadVersion(version));
        }
        Ok(Self {})
    }
}

pub fn migrate_text(text: &str) -> Result<String, SaveError> {
    match version {
        crate::session_store::FORMAT_VERSION => {
            let snapshot = SessionSnapshot::from_text(text)?;
            Ok(SaveGameV2::from_v1(&snapshot).to_text())
        }
        LEGACY_VERSION => {
            let doc = SaveGameV2::from_text(text)?;
            Ok(doc.to_text())
        }
        other => Err(SaveError::BadVersion(other)),
    }
}
'''

STORE_V1 = "pub const FORMAT_VERSION: i64 = 1;\n"
CORPUS_ALL = "\n".join(FAMILY_FIELDS)


@dataclass
class Suite:
    name: str
    memory: str
    store: str
    corpus: str
    expected: dict


SUITES = (
    Suite("green", GREEN_MEMORY, STORE_V1, CORPUS_ALL, {
        "writer_version": 3, "reader_versions": (1, 3), "reader_version": 3,
        "v3_reads_v2": 1, "migrate_arm_state": "publishes_v3",
        "migrate_arm_identity": False, "migrated_v3": 3, "idempotent_v3": 3,
        "families_v2": 14, "families_wired": 4, "families_roundtripped": 18,
        "families_required": 18, "families_present": 4, "unit_test_v3": 1}),
    Suite("v2-product", V2_MEMORY, STORE_V1, CORPUS_ALL, {
        "writer_version": 2, "reader_versions": (1, 2), "reader_version": 2,
        "v3_reads_v2": 0, "migrate_arm_state": "identity",
        "migrate_arm_identity": True, "migrated_v3": 0, "idempotent_v3": 0,
        "families_v2": 14, "families_wired": 0, "families_roundtripped": 14,
        "families_required": 18, "families_present": 4, "unit_test_v3": 0}),
    Suite("reader-behind", READER_BEHIND_MEMORY, STORE_V1, CORPUS_ALL, {
        "writer_version": 3, "reader_versions": (1, 2), "reader_version": 2,
        "v3_reads_v2": 0, "migrate_arm_state": "absent", "migrated_v3": 0,
        "idempotent_v3": 0, "families_v2": 14, "families_wired": 0,
        "families_roundtripped": 14, "families_required": 18}),
)


def suite_problems() -> list:
    """Pin the derivation to each source shape; a mismatch is the oracle biting."""
    problems = []
    for suite in SUITES:
        derived = derive_source_facts(suite.memory, suite.store, suite.corpus)
        for field_name, want in sorted(suite.expected.items()):
            got = getattr(derived, field_name)
            if got != want:
                problems.append("SELFTEST-MISMATCH suite=%s field=%s want=%r got=%r"
                                % (suite.name, field_name, want, got))
    return problems


def green_facts() -> Facts:
    """The green capture: probe/doc facts + census rows *derived* from green sources."""
    facts = replace(GREEN_PROBE)
    apply_source_facts(facts, derive_source_facts(GREEN_MEMORY, STORE_V1, CORPUS_ALL))
    facts.player_save_version = 3
    facts.product_save_bytes = 454
    facts.anchor = "product"
    return facts


def defect(name: str, facts: Facts) -> Facts:
    import copy
    mutated = copy.deepcopy(facts)
    if name == "r20-writer-two":
        mutated.writer_version, mutated.player_save_version = 2, 2
    elif name == "r20-player-one":
        mutated.player_save_version = 1
    elif name == "r21-families-three":
        mutated.families_present = 3
    elif name == "r21-vector-mismatch":
        mutated.vectors_ok = 29
    elif name == "r21-loot-minutes":
        mutated.loot_minutes_positive = 0
    elif name == "r22-sample-sha":
        mutated.sample_sha_ok = False
    elif name == "r22-identity":
        mutated.identity = False
    elif name == "r22-coverage":
        mutated.families_roundtripped = 14
    elif name == "r23-migration":
        mutated.migrated_v3 = 2
    elif name == "r23-idempotence":
        mutated.idempotent_v3 = 2
    elif name == "r24a-reader-missing":
        mutated.v3_reads_v2 = 0
    elif name == "r24b-named-error":
        mutated.v2_reads_v3 = "corrupt"
    elif name == "r24b-unit-test":
        mutated.unit_test_v3 = 0
    else:
        raise SystemExit("FALSIFY: unknown name %r" % name)
    return mutated


# (row ident, row label, what the defect is). M44-R24 has two rows, so the label
# is part of the key: a falsifier must flip ITS row, not merely "a row of R24".
FALSIFIERS = {
    "r20-writer-two": ("M44-R20", "version_is_three", "the writer still pins version 2"),
    "r20-player-one": ("M44-R20", "version_is_three", "the product-path save on disk still says 1"),
    "r21-families-three": ("M44-R21", "families_closed", "one of the four families is not persisted"),
    "r21-vector-mismatch": ("M44-R21", "families_closed",
                            "one declared golden vector disagrees with the parser"),
    "r21-loot-minutes": ("M44-R21", "families_closed",
                         "no golden vector exercises a looted container (DELTA-D1)"),
    "r22-sample-sha": ("M44-R22", "byte_identical_roundtrip",
                       "the sample's declared sha256 is not the sample's sha256"),
    "r22-identity": ("M44-R22", "byte_identical_roundtrip",
                     "encode(decode(sample)) differs from the sample"),
    "r22-coverage": ("M44-R22", "byte_identical_roundtrip", "the codec covers 14 of the 18 families"),
    "r23-migration": ("M44-R23", "v2_to_v3_migration", "one of three fixtures fails to migrate"),
    "r23-idempotence": ("M44-R23", "v2_to_v3_migration",
                        "re-migrating a v3 document is not a fixed point"),
    "r24a-reader-missing": ("M44-R24", "v3_reader_reads_v2", "no reader accepts version 3"),
    "r24b-named-error": ("M44-R24", "v2_reader_names_v3",
                         "a v3 document is refused as Corrupt, not BadVersion"),
    "r24b-unit-test": ("M44-R24", "v2_reader_names_v3", "the named-error unit test is gone"),
}


def find_row(rows: list, row_id: str, label: str) -> list:
    return [row for row in rows if row.ident == row_id and row.label == label]


# ── rendering ──────────────────────────────────────────────────────────────


def render_row(row: Row) -> str:
    if row.ok:
        return "[OK] %s %s %s src=%s" % (row.ident, row.label, row.facts, row.src)
    return "[FAIL] %s - %s %s expected=%s (%s) src=%s" % (
        row.ident, row.label, row.facts, row.expected, row.symptom, row.src)


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(
        prog="dun-save-001.py", description="GATE DUN-SAVE-001 — M44 block E, save v3")
    parser.add_argument("--binary", default="target/debug/daggerfall-app")
    parser.add_argument("--skip-build", action="store_true",
                        help="accepted and required: this gate never compiles")
    parser.add_argument("--out", default="", help="directory for the .log/.json copy")
    parser.add_argument("--json", action="store_true")
    parser.add_argument("--list", action="store_true", help="print rows + falsifiers")
    parser.add_argument("--falsify", default="", metavar="NAME")
    parser.add_argument("--self-test", action="store_true")
    parser.add_argument("--timeout", type=float, default=90.0)
    args = parser.parse_args(argv)

    green = green_facts()

    if args.list:
        for row in evaluate(green):
            print("%s  %s  falsifier=%s  src=%s  expected=%s"
                  % (row.ident, row.label, row.falsifier, row.src, row.expected))
        for name, (row_id, label, what) in FALSIFIERS.items():
            print("FALSIFIER %-22s row=%s/%s  %s" % (name, row_id, label, what))
        return 0

    if args.self_test:
        problems = suite_problems()
        for line in problems:
            print(line)
        rows = evaluate(green)
        green_ok = all(row.ok for row in rows)
        caught = 0
        for name, (row_id, label, _what) in FALSIFIERS.items():
            mutated = evaluate(defect(name, green))
            flipped = [r for r in find_row(mutated, row_id, label) if not r.ok]
            collateral = ["%s/%s" % (r.ident, r.label) for r in mutated
                          if (r.ident, r.label) != (row_id, label) and not r.ok]
            if flipped and not collateral:
                caught += 1
            elif collateral:
                print("FALSIFIER-SPILL %s row=%s/%s collateral=%s"
                      % (name, row_id, label, ",".join(sorted(set(collateral)))))
            else:
                print("FALSIFIER-MISS %s row=%s/%s (the row did not flip)"
                      % (name, row_id, label))
        print("GATE-SELFTEST rows=%d green=%d falsifiers=%d caught=%d"
              % (len(rows), int(green_ok and not problems), len(FALSIFIERS), caught))
        return 0 if green_ok and not problems and caught == len(FALSIFIERS) else 1

    if args.falsify:
        if args.falsify not in FALSIFIERS:
            print("FALSIFY: unknown name %r (see --list)" % args.falsify)
            return 2
        row_id, label = FALSIFIERS[args.falsify][0], FALSIFIERS[args.falsify][1]
        rows = evaluate(defect(args.falsify, green))
        target = find_row(rows, row_id, label)
        print("FALSIFY %s row=%s/%s" % (args.falsify, row_id, label))
        for row in rows:
            print(render_row(row))
        spill = ["%s/%s" % (r.ident, r.label) for r in rows
                 if (r.ident, r.label) != (row_id, label) and not r.ok]
        if spill:
            print("FALSIFIER-SPILL %s collateral=%s" % (args.falsify, ",".join(sorted(set(spill)))))
        bites = len(target) == 1 and not target[0].ok and not spill
        print("FALSIFIER %s bites=%d" % (args.falsify, int(bites)))
        return 0 if bites else 1

    binary = Path(args.binary)
    if not binary.is_absolute():
        binary = ROOT / binary
    lines: list[str] = []
    if not binary.is_file():
        lines.append("[NOT-RUN] binary_absent path=%s (nothing to measure)" % args.binary)
        lines.append("VERDICT: NC")
        print("\n".join(lines))
        return 2

    digest = file_sha256(binary) or "absent"
    lines.append("GATE %s — M44 block E · save v3 (target daggerfall-bevy-native)" % GATE)
    lines.append("CR %s · rows M44-R20..M44-R24 · block E (W44-5)" % CR)
    lines.append("BINARY path=%s sha256=%s bytes=%d" % (args.binary, digest[:16],
                                                        binary.stat().st_size))
    lines.append("BUILD skipped=%d reason=lane-forbids-compile flag=%s"
                 % (1, "--skip-build" if args.skip_build else "absent"))

    facts, harness, notes = collect(binary, args.timeout, bool(os.environ.get("DISPLAY")))
    lines.extend(harness)
    rows = evaluate(facts)
    lines.extend(render_row(row) for row in rows)
    lines.extend(notes)

    fingerprint, detail = Gate.recompute_fingerprint(SOURCES)
    lines.append("SOURCES fingerprint=%s files=%s paths=%s"
                 % (fingerprint or "absent", detail.split(" ")[0], ",".join(sorted(SOURCES))))

    cites = sorted({row.src for row in evaluate(green)} | {row.src for row in rows})
    cites_ok, cites_detail = citation_check(cites)
    lines.append("CITATIONS ok=%d %s" % (int(cites_ok), cites_detail))

    failed = [row for row in rows if not row.ok]
    harness_failed = any(line.startswith("[FAIL]") for line in lines)
    verdict = "FAIL" if (failed or harness_failed) else (
        "NC" if any(line.startswith("[NC]") or line.startswith("[NOT-RUN]") for line in lines)
        else "PASS")
    lines.append("VERDICT: %s" % verdict)
    body = "\n".join(lines)
    full = hashlib.sha256((body + "\n").encode()).hexdigest()
    lines.append("DIGEST %s" % full)
    output = "\n".join(lines)
    print(output)

    if args.out:
        out = Path(args.out)
        out.mkdir(parents=True, exist_ok=True)
        (out / "dun-save-001.log").write_text(output + "\n", encoding="utf-8")
        (out / "dun-save-001.json").write_text(json.dumps({
            "gate": GATE, "cr": CR, "verdict": verdict, "digest": full,
            "binary": {"path": args.binary, "sha256": digest},
            "sources": {"fingerprint": fingerprint, "paths": sorted(SOURCES)},
            "rows": [{"id": r.ident, "label": r.label, "ok": r.ok, "facts": r.facts,
                      "src": r.src, "falsifier": r.falsifier} for r in rows],
        }, indent=2, sort_keys=True) + "\n", encoding="utf-8")

    if args.json:
        print(json.dumps({"gate": GATE, "verdict": verdict, "digest": full,
                          "rows": [{"id": r.ident, "label": r.label, "ok": r.ok,
                                    "facts": r.facts, "src": r.src} for r in rows]},
                         sort_keys=True))
    return 0 if verdict == "PASS" else (2 if verdict == "NC" else 1)


if __name__ == "__main__":
    sys.exit(main())
