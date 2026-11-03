#!/usr/bin/env python3
"""UI-FULL-001 - gate of M40/UI-4: the **85/85** census surfaces.

Two separate processes, same binary, same retail bank
-----------------------------------------------------
1. `cargo build -p daggerfall-app` once (`--skip-build` reuses the binary).
2. `PROBE-UI-FULL-001` runs in **two independent processes** (`PROBE_RUN=1`
   and `PROBE_RUN=2`; the probe never prints that value, so an environment
   dependence shows up as a stdout difference).
3. Both stdout captures must contain `VEREDICTO: ... PASS` and exit 0, must be
   **byte-identical** (`cmp` + equal `sha256`), and the probe's own `DIGEST`
   line must be the `sha256` of every line it printed before it.
4. The 85 census surfaces are checked **one by one**: ordinal, census name and
   family against the frozen table below, a non-empty per-surface evidence
   digest, a focus outcome that is either a measured row count or an explicit
   `na=<reason>` — never a silent zero — and the three raster counts it prints
   (`nodes`, `glyphs`, `opaque`) asserted per row and tied to the probe's own
   `AXIS A1/A2/A3` details (audit GAP-M42-07: they used to be parsed and never
   read, so 85 rows with `nodes`/`glyphs` adulterated left the gate green).
5. The eleven blocks (A..H playable, I createchar, J config, K infra) must be
   non-empty and add up to 85 with focus and `na` totals that match their rows.
6. A `FAMILY-ROW-ERROR` line — the probe's controlled failure for a `SURFACE`
   field that is absent or unreadable — is red here: the coupling to M38/M39
   stdout never degrades into a silent `0`.

The gate composes the three family gates (`PROBE-UI-CREATE-001`,
`PROBE-UI-CONFIG-001`, `PROBE-UI-JUGABLE-001`) through the probe: it never
re-derives a family measurement, so a surface a family gate refuses cannot be
certified here.

Falsifiers (each one regresses the probe's **source**, rebuilds it in a detached
`git worktree` and requires the named rule red): `nodes-zero`, `glyphs-zero`,
`opaque-zero`, `field-absent`, `focus-zero`, `surface-missing`, `block-empty`,
`empty-digest`. Mutating the captures is gone: rewriting the text the gate had
already read accredited the rule, never the behaviour.

Exit codes: 0 PASS, 1 FAIL, 2 usage/IO (including `SKIP`).
"""

from __future__ import annotations

import argparse
import difflib
import hashlib
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
from dataclasses import dataclass
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
GATE = "UI-FULL-001"
PROBE = "PROBE-UI-FULL-001"
SLUG = "ui-full-001"
BINARY = ROOT / "target" / "debug" / "daggerfall-app"
ARTIFACTS = ROOT / ".sintergic" / "artifacts" / "m40-ui4" / SLUG
CR = "CR-bc68d7ce-46ba-49aa-9451-560ec00b6d92"
CRATE = "daggerfall-app"

# The frozen census (M36 §1.3, ordinals ascending): this is the nominal list
# the gate certifies. A drift between this table and either the probe's census
# block or its `FULL` rows fails the gate.
CENSUS: tuple[tuple[str, str, str], ...] = (
    ("001", "AmbientOcclusionConfigPage", "config"),
    ("002", "AntialiasingConfigPage", "config"),
    ("003", "BloomConfigPage", "config"),
    ("004", "ColorBoostConfigPage", "config"),
    ("005", "ColorPicker", "infra"),
    ("006", "CreateCharAddBonusSkills", "createchar"),
    ("007", "CreateCharAddBonusStats", "createchar"),
    ("008", "CreateCharBiography", "createchar"),
    ("009", "CreateCharChooseBio", "createchar"),
    ("010", "CreateCharChooseClassGen", "createchar"),
    ("011", "CreateCharClassQuestions", "createchar"),
    ("012", "CreateCharClassSelect", "createchar"),
    ("013", "CreateCharCustomClass", "createchar"),
    ("014", "CreateCharFaceSelect", "createchar"),
    ("015", "CreateCharGenderSelect", "createchar"),
    ("016", "CreateCharNameSelect", "createchar"),
    ("017", "CreateCharRaceSelect", "createchar"),
    ("018", "CreateCharReflexSelect", "createchar"),
    ("019", "CreateCharReputationWindow", "createchar"),
    ("020", "CreateCharSpecialAdvantageWindow", "createchar"),
    ("021", "CreateCharSummary", "createchar"),
    ("022", "DaggerfallAdvancedSettingsWindow", "playable"),
    ("023", "DaggerfallAutomapWindow", "playable"),
    ("024", "DaggerfallBankPurchasePopUp", "playable"),
    ("025", "DaggerfallBankingWindow", "playable"),
    ("026", "DaggerfallBaseWindow", "infra"),
    ("027", "DaggerfallBookReaderWindow", "playable"),
    ("028", "DaggerfallCharacterSheetWindow", "playable"),
    ("029", "DaggerfallControlsWindow", "playable"),
    ("030", "DaggerfallCourtWindow", "playable"),
    ("031", "DaggerfallDaedraSummonedWindow", "playable"),
    ("032", "DaggerfallEffectSettingsEditorWindow", "playable"),
    ("033", "DaggerfallExteriorAutomapWindow", "playable"),
    ("034", "DaggerfallGuildServiceCureDisease", "playable"),
    ("035", "DaggerfallGuildServiceDonation", "playable"),
    ("036", "DaggerfallGuildServicePopupWindow", "playable"),
    ("037", "DaggerfallGuildServiceTraining", "playable"),
    ("038", "DaggerfallHUD", "playable"),
    ("039", "DaggerfallInputMessageBox", "playable"),
    ("040", "DaggerfallInventoryWindow", "playable"),
    ("041", "DaggerfallItemMakerWindow", "playable"),
    ("042", "DaggerfallJoystickControlsWindow", "playable"),
    ("043", "DaggerfallListPickerWindow", "playable"),
    ("044", "DaggerfallLoadClassicGameWindow", "playable"),
    ("045", "DaggerfallMerchantRepairPopupWindow", "playable"),
    ("046", "DaggerfallMerchantServicePopupWindow", "playable"),
    ("047", "DaggerfallMessageBox", "playable"),
    ("048", "DaggerfallPauseOptionsWindow", "playable"),
    ("049", "DaggerfallPlayerHistoryWindow", "playable"),
    ("050", "DaggerfallPopupWindow", "infra"),
    ("051", "DaggerfallPotionMakerWindow", "playable"),
    ("052", "DaggerfallQuestJournalWindow", "playable"),
    ("053", "DaggerfallQuestOfferWindow", "playable"),
    ("054", "DaggerfallQuestPopupWindow", "infra"),
    ("055", "DaggerfallRestWindow", "playable"),
    ("056", "DaggerfallSpellBookWindow", "playable"),
    ("057", "DaggerfallSpellMakerWindow", "playable"),
    ("058", "DaggerfallStartNewGameWizard", "playable"),
    ("059", "DaggerfallStartWindow", "playable"),
    ("060", "DaggerfallTalkWindow", "playable"),
    ("061", "DaggerfallTavernWindow", "playable"),
    ("062", "DaggerfallTeleportPopUp", "playable"),
    ("063", "DaggerfallTradeWindow", "playable"),
    ("064", "DaggerfallTransportWindow", "playable"),
    ("065", "DaggerfallTravelMapWindow", "playable"),
    ("066", "DaggerfallTravelPopUp", "playable"),
    ("067", "DaggerfallUnityMouseControlsWindow", "playable"),
    ("068", "DaggerfallUnitySaveGameWindow", "playable"),
    ("069", "DaggerfallUnitySetupGameWizard", "playable"),
    ("070", "DaggerfallUseMagicItemWindow", "playable"),
    ("071", "DaggerfallVidPlayerWindow", "playable"),
    ("072", "DaggerfallWitchesCovenPopupWindow", "playable"),
    ("073", "DemoClassQuestionsWindow", "playable"),
    ("074", "DemoFLCWindow", "playable"),
    ("075", "DepthOfFieldConfigPage", "config"),
    ("076", "DitherConfigPage", "config"),
    ("077", "GameEffectConfigPage", "config"),
    ("078", "GameEffectsConfigWindow", "config"),
    ("079", "HardStrings", "infra"),
    ("080", "MotionBlurConfigPage", "config"),
    ("081", "RetroModeConfigPage", "config"),
    ("082", "SpellIconPickerWindow", "playable"),
    ("083", "UIWindowFactory", "infra"),
    ("084", "VignetteConfigPage", "config"),
    ("085", "WindowMessages", "infra"),
)
CENSUS_IDS: tuple[str, ...] = tuple(ordinal for ordinal, _, _ in CENSUS)
CENSUS_NAME: dict[str, str] = {ordinal: name for ordinal, name, _ in CENSUS}
CENSUS_FAMILY: dict[str, str] = {ordinal: family for ordinal, _, family in CENSUS}
FAMILY_OF_CATEGORY: dict[str, str] = {
    "createchar": "CreateChar",
    "config": "Config",
    "infra": "Infra",
    "playable": "Playable",
}
CATEGORY_OF_FAMILY: dict[str, str] = {value: key for key, value in FAMILY_OF_CATEGORY.items()}
BLOCKS: tuple[str, ...] = ("A", "B", "C", "D", "E", "F", "G", "H", "I", "J", "K")
AXES: tuple[str, ...] = ("A1", "A2", "A3", "A4", "A5", "A6", "A7")
FAMILIES: tuple[tuple[str, str, int], ...] = (
    ("create", "PROBE-UI-CREATE-001", 16),
    ("config", "PROBE-UI-CONFIG-001", 11),
    ("playable", "PROBE-UI-JUGABLE-001", 51),
)
# Infra rows have no raster by contract; the remaining zero-focus rows are the
# three read-only playables of M39 plan §3.4.
NOT_APPLICABLE = (
    "005",
    "026",
    "031",
    "050",
    "054",
    "071",
    "074",
    "079",
    "083",
    "085",
)
EMPTY_SHA256 = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"

# The certified raster signature of the 85 census rows: `sha256` of
# `ordinal:nodes:glyphs:opaque` in census order, taken at the M40/UI-4
# certification (HEAD a6529a1, retail ARENA2 bank, 4356 B SPOP.RCI).
# GAP-M42-07: the three counts were parsed and never read, so 85 adulterated
# rows still passed. A single replaced count changes this digest, and the rule
# refuses it; a *legitimate* UI change re-registers this value in the same
# commit as the probe change — never silently.
RASTER_SHA256 = "6acf904690c9bc5b713602ca7dd551e3887812c3a2068db9605978882868c5b3"

CENSUS_LINE = re.compile(r"^\[(\d{3})\]\s+(\S+)\s+(\S+)\s+(\S+)$")
FULL_LINE = re.compile(
    r"^FULL (\d{3}) id=(\S+) family=(CreateChar|Config|Infra|Playable) block=(\S) "
    r"registry=(ok|missing) open-close=(ok|leaked) nodes=(\d+) glyphs=(\d+) "
    r"focus=(\d+) na=(\S+) opaque=(\d+) evidence=(\S+)$"
)
BLOCK_LINE = re.compile(r"^BLOCK (\S) surfaces=(\d+) focus=(\d+) na=(\d+)$")
AXIS_LINE = re.compile(r"^AXIS (A\d) (OK|FAIL) (.*)$")
AXIS_A5_ROWS = re.compile(r"(?:^|\s)focus-rows=(\d+)")
AXIS_A5_WITH = re.compile(r"(?:^|\s)surfaces-with-focus=(\d+)")
AXIS_A5_NA = re.compile(r"(?:^|\s)not-applicable=(\S+)")
FAMILY_LINE = re.compile(
    r"^FAMILY (\S+) probe=(\S+) surfaces=(\d+) axes=(\d)/7 digest=([0-9a-f]{64}) status=(OK|FAIL)$"
)
SHELL_LINE = re.compile(
    r"^SHELL (OK|FAIL) registered=(\d+) registered-named=(\d+) open-close=(\d+) depth=(\d+)$"
)
AXIS_A1 = re.compile(r"surfaces=(\d+) composed=(\d+)/(\d+) raster=(\d+)")
AXIS_A2 = re.compile(r"renderable=(\d+) opaque-min=(\d+) opaque-total=(\d+)")
AXIS_A3 = re.compile(r"renderable=(\d+) glyphs-min=(\d+) glyphs-total=(\d+)")
DIGEST_LINE = re.compile(r"^DIGEST ([0-9a-f]{64})$")
FULL_FOCUS = re.compile(r"focus=(\d+) na=(\S+)")


# ── Process helpers ────────────────────────────────────────────────────────


def arena2_dir() -> Path | None:
    """Same resolution as the probe: `$DAGGERFALL_ARENA2`, `$DF_ARENA2`,
    `$DF_APP_ROOT/ARENA2`, `$HOME/Games/Daggerfall/app/ARENA2`."""
    for name in ("DAGGERFALL_ARENA2", "DF_ARENA2"):
        explicit = os.environ.get(name)
        if explicit and explicit.strip():
            return Path(explicit)
    root = os.environ.get("DF_APP_ROOT")
    base = Path(root) if root else Path(os.environ.get("HOME", "")) / "Games" / "Daggerfall" / "app"
    for name in ("ARENA2", "arena2"):
        candidate = base / name
        if candidate.is_dir():
            return candidate
    return None


def run(
    cmd: list[str], env: dict[str, str] | None = None, cwd: Path | None = None
) -> subprocess.CompletedProcess:
    return subprocess.run(
        cmd,
        cwd=cwd or ROOT,
        env={**os.environ, **(env or {})},
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
    )


def digest(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def head_commit() -> str | None:
    proc = run(["git", "rev-parse", "HEAD"])
    if proc.returncode != 0:
        return None
    return proc.stdout.decode("utf-8", "replace").strip() or None


def build(skip: bool) -> bool:
    if skip:
        return True
    proc = run(["cargo", "build", "-p", CRATE])
    if proc.returncode == 0:
        return True
    sys.stderr.write(proc.stderr.decode("utf-8", "replace")[-4000:])
    return False


@dataclass(frozen=True)
class Capture:
    run: int
    exit: int
    stdout: bytes
    stderr: bytes = b""


@dataclass(frozen=True)
class Rule:
    name: str
    ok: bool
    detail: str


def capture_run(binary: Path, run_id: int) -> Capture:
    proc = run([str(binary), "--probe", "UI-FULL-001"], {"PROBE_RUN": str(run_id)})
    return Capture(run=run_id, exit=proc.returncode, stdout=proc.stdout, stderr=proc.stderr)


def capture_all(binary: Path) -> list[Capture]:
    return [capture_run(binary, 1), capture_run(binary, 2)]


def lines_of(capture: Capture) -> list[str]:
    return capture.stdout.decode("utf-8", "replace").splitlines()


def full_rows(lines: list[str]) -> list[re.Match[str]]:
    return [match for line in lines if (match := FULL_LINE.match(line))]


def verdict_of(capture: Capture) -> str | None:
    found = [line for line in lines_of(capture) if line.startswith("VEREDICTO")]
    return found[-1] if found else None


def digest_of(capture: Capture) -> str | None:
    found = [match.group(1) for line in lines_of(capture) if (match := DIGEST_LINE.match(line))]
    return found[0] if len(found) == 1 else None


# ── Pure checks: no I/O, only the captures ─────────────────────────────────


def rule_probe_verdict(captures: list[Capture]) -> Rule:
    problems: list[str] = []
    evidence: list[str] = []
    for capture in captures:
        verdict = verdict_of(capture) or "absent"
        evidence.append(f"run{capture.run}={verdict.split(' ')[0]} exit={capture.exit}")
        if " PASS" not in verdict or capture.exit != 0:
            problems.append(f"run{capture.run}:{verdict}")
    if problems:
        return Rule("probe-verdict", False, " ".join(problems))
    return Rule("probe-verdict", True, " ".join(evidence))


def rule_stdout_byte_identical(captures: list[Capture]) -> Rule:
    first, second = captures
    if first.stdout != second.stdout:
        return Rule(
            "stdout-byte-identical",
            False,
            f"run1={len(first.stdout)}B run2={len(second.stdout)}B mismatch",
        )
    sha = digest(first.stdout)
    probe_digest = digest_of(first)
    if probe_digest is None:
        return Rule("stdout-byte-identical", False, "no single DIGEST line")
    return Rule(
        "stdout-byte-identical",
        True,
        f"stdout={len(first.stdout)}B sha256={sha} digest={probe_digest}",
    )


def rule_census_surface_set(captures: list[Capture]) -> Rule:
    lines = lines_of(captures[0])
    census = [match for line in lines if (match := CENSUS_LINE.match(line))]
    rows = full_rows(lines)
    problems: list[str] = []
    if [match.group(1) for match in census] != list(CENSUS_IDS):
        problems.append(f"census-ids={len(census)}")
    for match in census:
        ordinal, name, category = match.group(1), match.group(2), match.group(3)
        if CENSUS_NAME.get(ordinal) != name:
            problems.append(f"census:{ordinal}:{name}")
        if CATEGORY_OF_FAMILY.get(FAMILY_OF_CATEGORY.get(category, ""), "") != category:
            problems.append(f"census-category:{ordinal}:{category}")
    if [match.group(1) for match in rows] != list(CENSUS_IDS):
        problems.append(f"full-ids={len(rows)}")
    for match in rows:
        ordinal = match.group(1)
        if CENSUS_NAME.get(ordinal) != match.group(2):
            problems.append(f"full:{ordinal}:{match.group(2)}")
        family = CENSUS_FAMILY.get(ordinal)
        if family is None or FAMILY_OF_CATEGORY.get(family) != match.group(3):
            problems.append(f"full-family:{ordinal}:{match.group(3)}")
    duplicated = {ordinal for ordinal in CENSUS_IDS if [m.group(1) for m in rows].count(ordinal) > 1}
    if duplicated:
        problems.append(f"duplicated={','.join(sorted(duplicated))}")
    missing = [ordinal for ordinal in CENSUS_IDS if ordinal not in {m.group(1) for m in rows}]
    if missing:
        problems.append(f"missing={','.join(missing)}")
    if problems:
        return Rule("census-surface-set", False, " ".join(problems[:8]))
    return Rule(
        "census-surface-set",
        True,
        f"surfaces={len(rows)} census={len(census)} missing=0 duplicated=0",
    )


def rule_block_counts(captures: list[Capture]) -> Rule:
    lines = lines_of(captures[0])
    rows = full_rows(lines)
    blocks = [match for line in lines if (match := BLOCK_LINE.match(line))]
    problems: list[str] = []
    if [match.group(1) for match in blocks] != list(BLOCKS):
        problems.append(f"block-set={len(blocks)}")
    focus_of: dict[str, int] = {block: 0 for block in BLOCKS}
    na_of: dict[str, int] = {block: 0 for block in BLOCKS}
    for match in rows:
        block = match.group(4)
        focus_of[block] = focus_of.get(block, 0) + int(match.group(9))
        if match.group(10) != "-":
            na_of[block] = na_of.get(block, 0) + 1
    total = 0
    detail: list[str] = []
    for match in blocks:
        block, surfaces, focus, na = (
            match.group(1),
            int(match.group(2)),
            int(match.group(3)),
            int(match.group(4)),
        )
        total += surfaces
        if surfaces == 0:
            problems.append(f"{block}:empty")
        if focus != focus_of.get(block, 0):
            problems.append(f"{block}:focus {focus}!={focus_of.get(block, 0)}")
        if na != na_of.get(block, 0):
            problems.append(f"{block}:na {na}!={na_of.get(block, 0)}")
        detail.append(f"{block}={surfaces}")
    if total != len(CENSUS_IDS):
        problems.append(f"sum={total}")
    if problems:
        return Rule("block-counts", False, " ".join(problems[:8]))
    return Rule("block-counts", True, f"surfaces={total} blocks={','.join(detail)}")


def rule_focus_rows(captures: list[Capture]) -> Rule:
    lines = lines_of(captures[0])
    rows = full_rows(lines)
    problems: list[str] = []
    focus_total = 0
    with_focus = 0
    na_list: list[str] = []
    for match in rows:
        ordinal = match.group(1)
        focus = int(match.group(9))
        na = match.group(10)
        focus_total += focus
        if focus > 0:
            with_focus += 1
            if na != "-":
                problems.append(f"{ordinal}:focus+na")
        else:
            if na == "-":
                problems.append(f"{ordinal}:zero-without-na")
            else:
                na_list.append(ordinal)
    axis = next(
        (line for line in lines if line.startswith("AXIS A5 ")),
        "",
    )
    axis_rows = AXIS_A5_ROWS.search(axis)
    axis_with = AXIS_A5_WITH.search(axis)
    axis_na = AXIS_A5_NA.search(axis)
    if axis_rows is None or int(axis_rows.group(1)) != focus_total:
        problems.append(f"axis-rows!={focus_total}")
    if axis_with is None or int(axis_with.group(1)) != with_focus:
        problems.append(f"axis-with!={with_focus}")
    if axis_na is not None and axis_na.group(1).split(",") != na_list:
        problems.append("axis-na-list")
    if na_list != [ordinal for ordinal in NOT_APPLICABLE if ordinal in {m.group(1) for m in rows}]:
        problems.append(f"na-set={','.join(na_list)}")
    if focus_total == 0:
        problems.append("focus-zero")
    if problems:
        return Rule("focus-rows", False, " ".join(problems[:8]))
    return Rule(
        "focus-rows",
        True,
        f"surfaces={len(rows)} focus-rows={focus_total} surfaces-with-focus={with_focus} "
        f"not-applicable={','.join(na_list)}",
    )


def rule_surface_evidence(captures: list[Capture]) -> Rule:
    rows = full_rows(lines_of(captures[0]))
    problems: list[str] = []
    for match in rows:
        ordinal, family, evidence = match.group(1), match.group(3), match.group(12)
        if match.group(5) != "ok" or match.group(6) != "ok":
            problems.append(f"{ordinal}:shell")
        if evidence == "-":
            problems.append(f"{ordinal}:no-evidence")
        elif family == "Infra":
            if not (evidence.startswith("contract:") and evidence.endswith(":closed")):
                problems.append(f"{ordinal}:{evidence}")
        elif not (len(evidence) == 16 and all(ch in "0123456789abcdef" for ch in evidence)):
            problems.append(f"{ordinal}:{evidence}")
    if problems:
        return Rule("surface-evidence", False, " ".join(problems[:8]))
    return Rule("surface-evidence", True, f"surfaces={len(rows)} evidenced={len(rows)} unproven=0")


def rule_raster_fields(captures: list[Capture]) -> Rule:
    """`nodes`, `glyphs` and `opaque` are measurements, not decoration.

    GAP-M42-07: `FULL_LINE` parsed the three counts and **no rule read them**, so
    85 rows with `nodes`/`glyphs` adulterated still left the gate green. Three
    independent assertions close it:

    1. **Per-row contract** — an `Infra` row carries no raster (0/0/0); a
       renderable row composes (`nodes + glyphs > 0`, so a glyph-only window
       still passes) and has `opaque > 0`. A `FAMILY-ROW-ERROR` line (the
       probe's controlled failure for an absent or unreadable `SURFACE` field)
       is red here: the coupling never degrades into a silent `0`.
    2. **Axis agreement** — `AXIS A1/A2/A3` are recomputed from the rows and
       must match the details the probe printed, so the probe cannot certify an
       axis over numbers it did not publish.
    3. **Certified signature** — the `sha256` of `ordinal:nodes:glyphs:opaque`
       over the 85 rows must equal [`RASTER_SHA256`]: replacing a single count
       (R3a) changes it.
    """
    lines = lines_of(captures[0])
    rows = full_rows(lines)
    renderable = [match for match in rows if match.group(3) != "Infra"]
    problems: list[str] = []
    for match in rows:
        ordinal, family = match.group(1), match.group(3)
        nodes, glyphs, opaque = int(match.group(7)), int(match.group(8)), int(match.group(11))
        if family == "Infra":
            if (nodes, glyphs, opaque) != (0, 0, 0):
                problems.append(f"{ordinal}:infra-raster={nodes}/{glyphs}/{opaque}")
            continue
        if nodes + glyphs == 0:
            problems.append(f"{ordinal}:empty-composition")
        if opaque == 0:
            problems.append(f"{ordinal}:opaque=0")
    field_errors = [line for line in lines if line.startswith("FAMILY-ROW-ERROR ")]
    if field_errors:
        problems.append(f"family-row-errors={len(field_errors)}")
    totals = {
        "nodes": sum(int(match.group(7)) for match in renderable),
        "glyphs": sum(int(match.group(8)) for match in renderable),
        "opaque": sum(int(match.group(11)) for match in renderable),
    }
    minima = {
        "nodes": min((int(match.group(7)) for match in renderable), default=0),
        "glyphs": min((int(match.group(8)) for match in renderable), default=0),
        "opaque": min((int(match.group(11)) for match in renderable), default=0),
    }
    if minima["glyphs"] == 0:
        problems.append("glyphs-min=0")
    if minima["opaque"] == 0:
        problems.append("opaque-min=0")
    composed = sum(1 for match in renderable if int(match.group(7)) > 0 or int(match.group(8)) > 0)
    raster = sum(1 for match in renderable if int(match.group(7)) > 0)
    expected = {
        "A1": (AXIS_A1, (len(rows), composed, len(renderable), raster)),
        "A2": (AXIS_A2, (len(renderable), minima["opaque"], totals["opaque"])),
        "A3": (AXIS_A3, (len(renderable), minima["glyphs"], totals["glyphs"])),
    }
    for axis, (pattern, want) in expected.items():
        detail = next((line for line in lines if line.startswith(f"AXIS {axis} ")), None)
        match = pattern.search(detail) if detail else None
        if match is None:
            problems.append(f"{axis}-detail-absent")
        elif tuple(int(group) for group in match.groups()) != want:
            problems.append(f"{axis}={'/'.join(match.groups())}!={'/'.join(map(str, want))}")
    signature = digest(
        "\n".join(
            f"{match.group(1)}:{match.group(7)}:{match.group(8)}:{match.group(11)}"
            for match in rows
        ).encode()
    )
    if signature != RASTER_SHA256:
        problems.append(f"raster-signature={signature}!=certified={RASTER_SHA256}")
    if problems:
        return Rule("raster-fields", False, " ".join(problems[:8]))
    return Rule(
        "raster-fields",
        True,
        f"surfaces={len(rows)} renderable={len(renderable)} composed={composed} raster={raster} "
        f"nodes-total={totals['nodes']} glyphs-total={totals['glyphs']} "
        f"opaque-total={totals['opaque']} nodes-min={minima['nodes']} "
        f"glyphs-min={minima['glyphs']} opaque-min={minima['opaque']} "
        f"axes=A1/A2/A3-agree family-row-errors=0 signature={signature}",
    )


def rule_axis_set(captures: list[Capture]) -> Rule:
    lines = lines_of(captures[0])
    axes = [match for line in lines if (match := AXIS_LINE.match(line))]
    families = [match for line in lines if (match := FAMILY_LINE.match(line))]
    shell = next((match for line in lines if (match := SHELL_LINE.match(line))), None)
    problems: list[str] = []
    if [match.group(1) for match in axes] != list(AXES):
        problems.append(f"axis-set={len(axes)}")
    for match in axes:
        if match.group(2) != "OK":
            problems.append(f"{match.group(1)}:FAIL")
    if [(match.group(1), match.group(2), int(match.group(3))) for match in families] != [
        (label, probe, count) for label, probe, count in FAMILIES
    ]:
        problems.append(f"families={len(families)}")
    for match in families:
        if match.group(6) != "OK" or int(match.group(4)) != 7:
            problems.append(f"{match.group(1)}:{match.group(4)}/7:{match.group(6)}")
    if shell is None:
        problems.append("shell-absent")
    else:
        if shell.group(1) != "OK":
            problems.append("shell:FAIL")
        measured = {
            "registered": int(shell.group(2)),
            "registered-named": int(shell.group(3)),
            "open-close": int(shell.group(4)),
        }
        for key, value in measured.items():
            if value != len(CENSUS):
                problems.append(f"shell:{key}={value}")
        if int(shell.group(5)) != 0:
            problems.append(f"shell:depth={shell.group(5)}")
    if problems:
        return Rule("axis-set", False, " ".join(problems[:8]))
    a6 = next((line for line in lines if line.startswith("AXIS A6 ")), "")
    return Rule(
        "axis-set",
        True,
        f"axes=7 families=3/3 shell=ok registered={shell.group(2)} "
        f"registered-named={shell.group(3)} open-close={shell.group(4)} depth={shell.group(5)} {a6[8:]}",
    )


def rule_no_empty_digest(captures: list[Capture]) -> Rule:
    capture = captures[0]
    value = digest_of(capture)
    if value is None:
        return Rule("no-empty-digest", False, "no single DIGEST line")
    if value == EMPTY_SHA256:
        return Rule("no-empty-digest", False, "DIGEST is sha256 of the empty string")
    lines = lines_of(capture)
    index = next((i for i, line in enumerate(lines) if line.startswith("DIGEST ")), None)
    if index is None:
        return Rule("no-empty-digest", False, "DIGEST line not found")
    expected = digest("\n".join(lines[:index]).encode())
    if expected != value:
        return Rule("no-empty-digest", False, f"digest={value} != sha256(prefix)={expected}")
    return Rule("no-empty-digest", True, f"digest={value} prefix=verified")


RULES = (
    rule_probe_verdict,
    rule_stdout_byte_identical,
    rule_census_surface_set,
    rule_block_counts,
    rule_focus_rows,
    rule_surface_evidence,
    rule_raster_fields,
    rule_axis_set,
    rule_no_empty_digest,
)


def check(captures: list[Capture]) -> list[Rule]:
    """The nine rules over two captures. Pure: no disk, no clock, no build."""
    return [rule(captures) for rule in RULES]


# ── Reporting ──────────────────────────────────────────────────────────────


def failed_names(rules: list[Rule]) -> list[str]:
    return [rule.name for rule in rules if not rule.ok]


def payload(binary: Path, captures: list[Capture], rules: list[Rule], extra: dict | None = None) -> dict:
    lines = lines_of(captures[0])
    rows = full_rows(lines)
    blocks = [match for line in lines if (match := BLOCK_LINE.match(line))]
    renderable = [match for match in rows if match.group(3) != "Infra"]
    focus_total = sum(int(match.group(9)) for match in rows)
    na = [match.group(1) for match in rows if match.group(10) != "-"]
    shell = next((match for line in lines if (match := SHELL_LINE.match(line))), None)
    body = {
        "schema": "UI-FULL-001/2",
        "gate": GATE,
        "cr": CR,
        "probe": PROBE,
        "head": head_commit(),
        "binary": str(binary),
        "binary_sha256": digest(binary.read_bytes()) if binary.is_file() else None,
        "surfaces": len(rows),
        "census": len(CENSUS),
        "shell": None
        if shell is None
        else {
            "registered": int(shell.group(2)),
            "registered_named": int(shell.group(3)),
            "open_close": int(shell.group(4)),
            "depth": int(shell.group(5)),
        },
        "raster": {
            "nodes_min": min((int(match.group(7)) for match in renderable), default=0),
            "glyphs_min": min((int(match.group(8)) for match in renderable), default=0),
            "opaque_min": min((int(match.group(11)) for match in renderable), default=0),
            "nodes_total": sum(int(match.group(7)) for match in renderable),
            "glyphs_total": sum(int(match.group(8)) for match in renderable),
            "opaque_total": sum(int(match.group(11)) for match in renderable),
        },
        "blocks": {match.group(1): int(match.group(2)) for match in blocks},
        "focus_rows": focus_total,
        "surfaces_with_focus": sum(1 for match in rows if int(match.group(9)) > 0),
        "not_applicable": na,
        "probe_digest": digest_of(captures[0]),
        "stdout": [
            {"run": capture.run, "exit": capture.exit, "bytes": len(capture.stdout), "sha256": digest(capture.stdout)}
            for capture in captures
        ],
        "rules": [{"name": rule.name, "ok": rule.ok, "detail": rule.detail} for rule in rules],
        "failed": failed_names(rules),
    }
    if extra:
        body.update(extra)
    return body


def emit(binary: Path, captures: list[Capture], rules: list[Rule], as_json: bool, extra: dict | None = None) -> int:
    failed = failed_names(rules)
    body = payload(binary, captures, rules, extra)
    if as_json:
        print(json.dumps(body, indent=2, sort_keys=True))
    else:
        print(
            f"gate {GATE} probe={PROBE} cr={CR} surfaces={body['surfaces']} "
            f"blocks={len(body['blocks'])} focus-rows={body['focus_rows']}"
        )
        for capture in captures:
            print(
                f"run{capture.run} exit={capture.exit} sha256={digest(capture.stdout)} "
                f"bytes={len(capture.stdout)}"
            )
        for rule in rules:
            print(f"{'OK' if rule.ok else 'FAIL'} {rule.name} {rule.detail}")
        print(
            f"CHECKS {','.join(rule.name for rule in rules)} "
            f"{'all-OK' if not failed else 'failed=' + ','.join(failed)}"
        )
    return 0 if not failed else 1


def write_manifest(binary: Path, captures: list[Capture], rules: list[Rule], out: Path, extra: dict | None = None) -> Path:
    out.mkdir(parents=True, exist_ok=True)
    path = out / "manifest.json"
    path.write_text(json.dumps(payload(binary, captures, rules, extra), indent=2, sort_keys=True) + "\n")
    for capture in captures:
        (out / f"run{capture.run}.stdout").write_bytes(capture.stdout)
        (out / f"run{capture.run}.stderr").write_bytes(capture.stderr)
    return path


# ── Falsifiers: regress the probe's SOURCE, rebuild in a detached worktree ──
#
# The old modes rewrote the capture the gate had already read, so they
# accredited the rule, never the behaviour (`DESIGN-AUDIT-276c7d94`,
# «`--falsify` muta el texto ya capturado»). A falsifier now mutates
# `m34_ui_probe.rs` inside `git worktree add --detach`, rebuilds it there with
# its own `CARGO_TARGET_DIR`, and requires the named rule red on the rebuilt
# probe's own stdout. The shared working tree is never mutated.

PROBE_SOURCE = "crates/app/src/m34_ui_probe.rs"


@dataclass(frozen=True)
class SourceMutation:
    """One real source regression, keyed by the rule it must turn red."""

    rule: str
    path: str
    old: str
    new: str
    note: str


FALSIFIERS: dict[str, SourceMutation] = {
    # GAP-M42-07 / R3a: the three raster counts are asserted per row, so zeroing
    # any one of them in the probe must turn `raster-fields` red.
    "nodes-zero": SourceMutation(
        "raster-fields",
        PROBE_SOURCE,
        'nodes: count_of(line, "nodes")?,',
        "nodes: 0,",
        "parse_surface(nodes) -> 0 on all 85 rows (R3a)",
    ),
    "glyphs-zero": SourceMutation(
        "raster-fields",
        PROBE_SOURCE,
        'glyphs: count_of(line, "glyphs")?,',
        "glyphs: 0,",
        "parse_surface(glyphs) -> 0 on all 85 rows (R3a)",
    ),
    "opaque-zero": SourceMutation(
        "raster-fields",
        PROBE_SOURCE,
        'opaque: count_of(line, "opaque")?,',
        "opaque: 0,",
        "parse_surface(opaque) -> 0 on all 85 rows (R3a)",
    ),
    "field-absent": SourceMutation(
        "surface-evidence",
        PROBE_SOURCE,
        'nodes: count_of(line, "nodes")?,',
        'nodes: count_of(line, "node-count")?,',
        "read a SURFACE field that does not exist: the coupling must fail loudly",
    ),
    "focus-zero": SourceMutation(
        "focus-rows",
        PROBE_SOURCE,
        'focus: count_of(line, "focus")?,',
        "focus: 0,",
        "parse_surface(focus) -> 0 while `na` keeps its reason",
    ),
    "surface-missing": SourceMutation(
        "census-surface-set",
        PROBE_SOURCE,
        "for (index, row) in ui_census::census().iter().enumerate() {",
        "for (index, row) in ui_census::census().iter().skip(1).enumerate() {",
        "drop census row 001 from the probe's own output",
    ),
    "block-empty": SourceMutation(
        "block-counts",
        PROBE_SOURCE,
        "const FULL_BLOCKS: [char; 11] = ['A', 'B', 'C', 'D', 'E', 'F', 'G', 'H', 'I', 'J', 'K'];",
        "const FULL_BLOCKS: [char; 10] = ['A', 'B', 'C', 'D', 'E', 'F', 'G', 'H', 'I', 'J'];",
        "drop block K (7 infra surfaces) from the block partition",
    ),
    "empty-digest": SourceMutation(
        "no-empty-digest",
        PROBE_SOURCE,
        'let stdout_digest = crate::m37_ui1_probe::sha256_hex(out.join("\\n").as_bytes());',
        f'let stdout_digest = "{EMPTY_SHA256}".to_owned();',
        "report sha256(empty) as the probe's own digest",
    ),
}


def falsify_target_dir(explicit: str | None) -> Path:
    """Cargo target dir reused across falsifier runs (one cold workspace build,
    then one crate recompile per mutation)."""
    if explicit:
        return Path(explicit)
    return Path(tempfile.gettempdir()) / f"{SLUG}-falsify-target"


def falsify_work_root(target_dir: Path, explicit: str | None) -> Path:
    """Directory that hosts the detached worktree.

    It sits beside the target dir and keeps a **stable** path: cargo keys its
    fingerprints on the package root, so re-using one worktree path is what
    turns the sweep into one workspace build plus one crate rebuild per
    mutation instead of a cold build per mode.
    """
    root = Path(explicit) if explicit else target_dir.parent / f"{SLUG}-falsify-work"
    root.mkdir(parents=True, exist_ok=True)
    return root


def mutate_source(worktree: Path, mutation: SourceMutation) -> str:
    """Apply the regression to the worktree copy; return its unified diff."""
    source = worktree / mutation.path
    original = source.read_text(encoding="utf-8")
    hits = original.count(mutation.old)
    if hits != 1:
        raise SystemExit(
            f"FALSIFY: anchor appears {hits}x in {mutation.path}, expected once: {mutation.old!r}"
        )
    mutated = original.replace(mutation.old, mutation.new)
    source.write_text(mutated, encoding="utf-8")
    return "".join(
        difflib.unified_diff(
            original.splitlines(keepends=True),
            mutated.splitlines(keepends=True),
            fromfile=f"a/{mutation.path}",
            tofile=f"b/{mutation.path}",
        )
    )


def falsify(
    mode: str,
    work_root: Path,
    target_dir: Path,
    out: Path,
    clean: list[Rule],
    as_json: bool,
) -> int:
    """Regress the probe's source, rebuild, require the named rule red."""
    mutation = FALSIFIERS[mode]
    already_red = failed_names(clean)
    if already_red:
        print(f"FALSIFY {mode}: clean gate is already red ({','.join(already_red)}): no falsification")
        return 1
    worktree = work_root / "wt"
    evidence: dict = {"mode": mode, "target": mutation.rule, "mutation": mutation.note}
    try:
        run(["git", "worktree", "remove", "--force", str(worktree)])
        run(["git", "worktree", "prune"])
        shutil.rmtree(worktree, ignore_errors=True)
        added = run(["git", "worktree", "add", "--detach", str(worktree), "HEAD"])
        if added.returncode != 0:
            sys.stderr.write(added.stderr.decode("utf-8", "replace")[-2000:])
            raise SystemExit(f"FALSIFY {mode}: git worktree add failed")
        diff = mutate_source(worktree, mutation)
        built = run(
            ["cargo", "build", "-p", CRATE],
            {"CARGO_TARGET_DIR": str(target_dir)},
            cwd=worktree,
        )
        if built.returncode != 0:
            sys.stderr.write(built.stderr.decode("utf-8", "replace")[-4000:])
            raise SystemExit(f"FALSIFY {mode}: mutated build failed")
        mutated_binary = target_dir / "debug" / "daggerfall-app"
        captures = capture_all(mutated_binary)
        rules = check(captures)
        failed = failed_names(rules)
        red = mutation.rule in failed
        evidence.update(
            {
                "source": mutation.path,
                "source_sha256": digest((worktree / mutation.path).read_bytes()),
                "binary_sha256": digest(mutated_binary.read_bytes()),
                "exits": [capture.exit for capture in captures],
                "failed": failed,
                "red": red,
            }
        )
        archive = out / f"falsify-{mode}"
        write_manifest(mutated_binary, captures, rules, archive, {"falsify": evidence})
        (archive / "mutation.diff").write_text(diff)
    finally:
        run(["git", "worktree", "remove", "--force", str(worktree)])
        run(["git", "worktree", "prune"])
    if as_json:
        print(json.dumps(evidence, sort_keys=True))
    else:
        print(f"FALSIFY {mode}: source={mutation.path} ({mutation.note})")
        print(f"FALSIFY {mode}: exits={evidence['exits']} failed={','.join(evidence['failed']) or 'none'}")
        print(f"FALSIFY {mode}: rule {mutation.rule} {'red' if red else 'STAYED GREEN'}")
    return 0 if red else 1


# ── main ───────────────────────────────────────────────────────────────────


def main() -> int:
    parser = argparse.ArgumentParser(
        description=__doc__,
        formatter_class=argparse.RawDescriptionHelpFormatter,
    )
    parser.add_argument("--binary", default=str(BINARY), help="prebuilt daggerfall-app")
    parser.add_argument("--out", default=str(ARTIFACTS), help="archive root (manifest.json + captures)")
    parser.add_argument("--skip-build", action="store_true", help="do not run cargo build")
    parser.add_argument("--json", action="store_true", help="emit one JSON object instead of the table")
    parser.add_argument("--verbose", action="store_true", help="echo the run1 probe stdout first")
    parser.add_argument(
        "--falsify",
        choices=sorted(FALSIFIERS),
        help="regress the probe's source in a detached worktree and require a red gate",
    )
    parser.add_argument(
        "--falsify-target-dir",
        default=None,
        help="cargo target dir for falsifier builds (default: $TMPDIR/ui-full-001-falsify-target)",
    )
    parser.add_argument(
        "--falsify-work-root",
        default=None,
        help="directory hosting the detached falsifier worktree (default: beside the target dir)",
    )
    args = parser.parse_args()

    if arena2_dir() is None:
        print("VERDICT: SKIP - no ARENA2")
        return 2

    binary = Path(args.binary)
    out = Path(args.out)
    if not build(args.skip_build):
        print("VERDICT: FAIL - build")
        return 2
    if not binary.is_file():
        print(f"VERDICT: FAIL - missing binary {binary}")
        return 2

    captures = capture_all(binary)
    rules = check(captures)

    if args.falsify:
        target_dir = falsify_target_dir(args.falsify_target_dir)
        return falsify(
            args.falsify,
            falsify_work_root(target_dir, args.falsify_work_root),
            target_dir,
            out,
            rules,
            args.json,
        )

    write_manifest(binary, captures, rules, out)
    if args.verbose:
        print(captures[0].stdout.decode("utf-8", "replace"))
    code = emit(binary, captures, rules, args.json)
    print("VERDICT: PASS" if code == 0 else "VERDICT: FAIL")
    return code


if __name__ == "__main__":
    sys.exit(main())
