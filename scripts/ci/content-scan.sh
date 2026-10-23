#!/usr/bin/env bash
# content-scan.sh — BUILD-CONTENT-SCAN-001 · gate L99-REL/CI (M0 PRE-6)
#
# Deterministic: 0 .BSA file and 0 BSA file signature packaged into
# binaries/artifacts.
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
# Usage: bash scripts/ci/content-scan.sh    (exit 0 = PASS · exit 1 = FAIL)
set -euo pipefail

ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

# BSA file signature: 'B','S','A' + version byte
#   0x1a Daggerfall/TES II · 0x18/0x19 Oblivion · 0x67/0x68 Skyrim
BSA_MAGIC=$'BSA[\x1a\x18\x19\x67\x68]'

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
magic_hits=""
n_files=0
n_bin=0
while IFS= read -r f; do
  if [ -z "$f" ] || [ ! -f "$f" ]; then
    continue
  fi
  n_files=$(( n_files + 1 ))
  if ! LC_ALL=C grep -qI . "$f" 2>/dev/null; then
    n_bin=$(( n_bin + 1 ))
    if LC_ALL=C grep -qaE "$BSA_MAGIC" "$f" 2>/dev/null; then
      magic_hits="${magic_hits}${f}"$'\n'
    fi
  fi
done < <(find . -path ./.git -prune -o -path ./reference -prune -o -path './target/*/deps' -prune -o -path './target/*/incremental' -prune -o -type f -print | LC_ALL=C sort)

magic_count=0
if [ -n "$magic_hits" ]; then
  magic_count="$(printf '%s' "$magic_hits" | grep -c . || true)"
fi
echo "check3_bsa_magic: $magic_count (files=$n_files binaries=$n_bin; excludes .git/ reference/ target/*/deps/ target/*/incremental/)"
if [ "$magic_count" -gt 0 ]; then
  while IFS= read -r hit; do
    if [ -n "$hit" ]; then
      echo "  HIT $hit"
    fi
  done <<< "$magic_hits"
fi

# ── Verdict (0 hits across the 3 checks = PASS) ──
fail=0
if [ "$tracked_gate" -ne 0 ]; then fail=1; fi
if [ "$tree_count" -ne 0 ]; then fail=1; fi
if [ "$magic_count" -ne 0 ]; then fail=1; fi

if [ "$fail" -eq 0 ]; then
  echo "content-scan: PASS (0 .BSA packaged into binaries/artifacts; reference/ spec corpus excluded)"
  exit 0
fi
echo "content-scan: FAIL"
exit 1
