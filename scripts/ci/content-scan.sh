#!/usr/bin/env bash
# content-scan.sh — BUILD-CONTENT-SCAN-001 · gate L99-REL/CI (M0 PRE-6)
#
# Deterministic: 0 .BSA file and 0 Daggerfall/TES II BSA payload packaged into
# binaries/artifacts.
#
# CHECK 3 GATES ONLY THE TES II SIGNATURE ('B','S','A',0x1a). Rationale, measured:
#   every retail asset this engine targets is a Daggerfall/TES II BSA container
#   (MIDI.BSA, ARCH3D.BSA, MONSTER.BSA, MAPS.BSA, ...) and all of them carry
#   version byte 0x1a. The remaining version bytes in the wild belong to OTHER
#   games' formats (0x18/0x19 Oblivion, 0x67/0x68 Skyrim) and no asset of this
#   repo can produce them.
#   Those other bytes were gating until 2026-10-23 and produced a FALSE POSITIVE
#   while 0 assets were packaged: a stale target/release/daggerfall-app
#   (126 271 032 B, mtime 01:38, built before HEAD) carried exactly ONE
#   BSA\x19 sequence at byte offset 12 607 237, where the hexdump reads
#     ...ord nameMONSTER.BSA\x19unknown DirectoryType: 0x...
#   The string literal "MONSTER.BSA" and the NEXT string-table literal (prefixed
#   by its length byte 0x19) are adjacent. That is string-table adjacency in the
#   compiler output, NOT an embedded asset: check1 (0 tracked .bsa) and check2
#   (0 .bsa in the tree) were 0 in the same run, and the string context proves
#   the bytes belong to two Rust literals.
#   Consequence: 0x18/0x19/0x67/0x68 are ADVISORY — printed as
#   `check3_bsa_magic_advisory` plus an `ADV <path>` line, never gating. They stay
#   visible so a genuine foreign-format payload could not slip by silently.
#   The gate keeps its full force: a binary carrying BSA\x1a still yields
#   `HIT <path>` and `content-scan: FAIL` (exit 1).
#   Two-sided proof (2026-10-23, temporary target/ci-selftest/, deleted after):
#     BSA\x1a probe => check3 HIT + content-scan: FAIL (exit 1)
#     BSA\x19 probe => PASS with check3_bsa_magic_advisory: 1 (ADV line)
#   Extension checks 1-2 (.bsa by name) are untouched and still catch every
#   retail container no matter which version byte it holds.
#   reference/ = MIT spec corpus (inherited: 114 files, 0 assets, commit
#   2aeafce) stays OUTSIDE the gate and is reported informationally only.
#   target/*/deps/ = rlibs/rmeta of third-party dependencies stay OUTSIDE
#   check 3: they are compiled code, NOT packaged assets. bevy_asset
#   (crates.io) embeds the BSA signature in its object code (measured in the
#   .rlib: bytes 42 53 41 1a = 'B','S','A',0x1a) and that tripped check3 with
#   0 real .bsa in the repo (false positive, cycle 2: 2 HITs, both in
#   target/release/deps/).
#   target/*/incremental/ = cargo incremental cache (dep-graph.bin, query
#   artifacts) regenerated on every build and NOT published: false positive
#   measured 2026-10-21 (1 HIT in dep-graph.bin; 0 .bsa in the repo; 0 HIT in
#   the project's own binaries). It stays OUTSIDE check 3 like target/*/deps/.
#   The project's own binaries REMAIN scanned by check 3:
#   verified target/debug/daggerfall-app and target/release/daggerfall-app =>
#   they do NOT match the signature (0 HIT); outside target/*/deps/ there are
#   3781 binaries scanned => 0 HIT. The intent of the gate (0 packaged assets)
#   stays intact: extension checks 1-2 (including target/) and magic signature
#   in own binaries by check 3.
#   Source that mentions .bsa/BSA (path_checker.rs, bsa.rs, mesh.rs) is NOT a
#   violation: the checks are by file extension (checks 1-2) and by magic
#   signature in binary files (check 3), never by code content.
#
# Residual (declared, not hidden): if a future fresh `--release` build placed a
#   literal ending in "BSA" adjacent to a following literal whose leading length
#   byte is 0x1a, check 3 would report a HIT with 0 packaged assets. The repair for
#   that case is header validation at the hit offset (parse the BSA header there:
#   offset0/offset1 record table, sane file count), not widening or narrowing
#   the byte pattern. Not implemented now: no such adjacency has been measured.
#
# Usage: bash scripts/ci/content-scan.sh    (exit 0 = PASS · exit 1 = FAIL)
set -euo pipefail

ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

# BSA file signature: 'B','S','A' + version byte.
#   GATE     : 0x1a = Daggerfall / TES II — the only retail format of this repo.
#   ADVISORY : 0x18/0x19 Oblivion · 0x67/0x68 Skyrim — printed, NOT gating (see
#              the measured false positive in the header: string-table adjacency
#              of "MONSTER.BSA" + length byte 0x19 at release-binary offset 12 607 237).
# ANSI-C quoting ($'...') keeps the pattern as literal bytes: no dependency on
# GNU grep's \xHH extension.
BSA_MAGIC_CANDIDATE=$'BSA[\x1a\x18\x19\x67\x68]'   # any version (filter)
BSA_MAGIC_GATE=$'BSA\x1a'                          # TES II only (gates)

echo "content-scan: BUILD-CONTENT-SCAN-001 (M0 PRE-6) · CR-bbd5db82"
echo "root: $ROOT"

# ── CHECK 1 — .BSA tracked by git (negative verification F0) ──────────────
tracked_total="$(git ls-files | LC_ALL=C grep -icE '\.bsa$' || true)"
tracked_ref="$(git ls-files reference | LC_ALL=C grep -icE '\.bsa$' || true)"
tracked_gate=$(( tracked_total - tracked_ref ))
echo "check1_tracked_bsa: gate=$tracked_gate total=$tracked_total reference_allowed=$tracked_ref"

# ── CHECK 2 — .BSA in the working tree (tracked + NOT tracked + ignored) ──
tree_list="$(find . -path ./.git -prune -o -path ./reference -prune -o -type f -iname '*.bsa' -print | LC_ALL=C sort)"
tree_count=0
if [ -n "$tree_list" ]; then
  tree_count="$(printf '%s\n' "$tree_list" | grep -c . || true)"
fi
echo "check2_tree_bsa: $tree_count (excludes .git/ and reference/)"
if [ "$tree_count" -gt 0 ]; then
  while IFS= read -r hit; do
    if [ -n "$hit" ]; then
      echo "  HIT $hit"
    fi
  done <<< "$tree_list"
fi

# ── CHECK 3 — BSA signature in tree binaries (repo + target/ artifacts) ───
# grep -I: if the file is binary it does not "contain" a match => exit != 0 => it is.
# Prune target/*/deps/: third-party rlibs/rmeta = compiled code that embeds
# the BSA signature (see header), not packaged assets.
# The project's own binaries target/{debug,release}/<bin> stay IN scope.
# One pass per binary with the any-version filter; only candidates pay the
# second (gating) pass, so the added precision costs nothing on clean trees.
magic_hits=""
adv_hits=""
n_files=0
n_bin=0
while IFS= read -r f; do
  if [ -z "$f" ] || [ ! -f "$f" ]; then
    continue
  fi
  n_files=$(( n_files + 1 ))
  if ! LC_ALL=C grep -qI . "$f" 2>/dev/null; then
    n_bin=$(( n_bin + 1 ))
    if LC_ALL=C grep -qaE "$BSA_MAGIC_CANDIDATE" "$f" 2>/dev/null; then
      if LC_ALL=C grep -qaF -- "$BSA_MAGIC_GATE" "$f" 2>/dev/null; then
        magic_hits="${magic_hits}${f}"$'\n'
      else
        adv_hits="${adv_hits}${f}"$'\n'
      fi
    fi
  fi
done < <(find . -path ./.git -prune -o -path ./reference -prune -o -path './target/*/deps' -prune -o -path './target/*/incremental' -prune -o -type f -print | LC_ALL=C sort)

magic_count=0
if [ -n "$magic_hits" ]; then
  magic_count="$(printf '%s' "$magic_hits" | grep -c . || true)"
fi
adv_count=0
if [ -n "$adv_hits" ]; then
  adv_count="$(printf '%s' "$adv_hits" | grep -c . || true)"
fi
echo "check3_bsa_magic: $magic_count (gating signature 'BSA'+0x1a only; files=$n_files binaries=$n_bin; excludes .git/ reference/ target/*/deps/ target/*/incremental/)"
if [ "$magic_count" -gt 0 ]; then
  while IFS= read -r hit; do
    if [ -n "$hit" ]; then
      echo "  HIT $hit"
    fi
  done <<< "$magic_hits"
fi
echo "check3_bsa_magic_advisory: $adv_count (NOT gating: 0x18/0x19/0x67/0x68 = foreign formats, 0 assets packaged — see header)"
if [ "$adv_count" -gt 0 ]; then
  while IFS= read -r hit; do
    if [ -n "$hit" ]; then
      echo "  ADV $hit"
    fi
  done <<< "$adv_hits"
fi

# ── Verdict (0 hits across the 3 checks = PASS) ──
fail=0
if [ "$tracked_gate" -ne 0 ]; then fail=1; fi
if [ "$tree_count" -ne 0 ]; then fail=1; fi
if [ "$magic_count" -ne 0 ]; then fail=1; fi

if [ "$fail" -eq 0 ]; then
  echo "content-scan: PASS (0 .BSA packaged into binaries/artifacts; 0 TES II signature; advisory=$adv_count non-gating; reference/ spec corpus excluded)"
  exit 0
fi
echo "content-scan: FAIL"
exit 1
