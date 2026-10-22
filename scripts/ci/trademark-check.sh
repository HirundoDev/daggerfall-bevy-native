#!/usr/bin/env bash
# trademark-check.sh — INV-L99-8 · R2 clause (M0 PRE-6)
# CR-bbd5db82-f194-4772-932d-364206dca16b
#
# Gate: 0 "Elder Scrolls" / "TES" on PUBLIC surfaces (brand/domain/store/
# metadata). DEFAULT-DENY policy: the whole working tree is gating except the
# exemptions DECLARED in is_exempt(); nothing stays hidden — exempted hits are
# scanned anyway and listed in the ADVISORY band (non-gating) with their class.
#
#   R2 (DECISION-A): the internal codename "Daggerfall Bevy" and the
#   daggerfall-* prefix are NOT a violation while the repo is private. G1 (public
#   brand name) remains OPEN by design —
#   docs/sintergic/game/2026-10-18-decision-a-naming-CR-bbd5db82.md
#
# Usage: bash scripts/ci/trademark-check.sh    (exit 0 = PASS · exit 1 = FAIL)
set -euo pipefail

ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

# Single pattern (case-insensitive; \b = word boundary: "tests", "assets",
# "crates" do NOT match; "TES"/"Tes"/"tes" do).
PAT='Elder[[:space:]]+Scrolls|\bTES\b'

echo "trademark-check: INV-L99-8 / R2 (M0 PRE-6) · CR-bbd5db82"
echo "root: $ROOT"
echo "pattern: 'Elder[[:space:]]+Scrolls' | '\\bTES\\b' (case-insensitive, word-boundary)"

# ── DECLARED exemptions (internal surface / third parties / meta) ─────────
is_exempt() {
  case "$1" in
    ./reference/*)                return 0 ;;  # third-party MIT spec corpus (inherited: 0 assets)
    ./docs/sintergic/*|./.sintergic/*) return 0 ;;  # live internal record: defines the guardrail by naming the mark
    ./sovereign-arch/*)           return 0 ;;  # READ-ONLY copy of the Mother (framework)
    ./scripts/ci/*)               return 0 ;;  # guardrail implementation (self-match)
    ./MASTER-PLAN-100J*.md|./prototype-gap-*.md) return 0 ;;  # internal planning artifacts (cite the mark when stating the gate)
    *)                            return 1 ;;
  esac
}

classify() {
  case "$1" in
    ./docs/sintergic/*|./.sintergic/*) echo "internal-governance" ;;
    ./sovereign-arch/*)                echo "mother-copy-framework" ;;
    ./scripts/ci/*)                    echo "guardrail-implementation" ;;
    ./reference/*)                     echo "third-party-corpus" ;;
    ./*.md)                            echo "internal-planning" ;;
    *)                                 echo "UNCLASSIFIED-REVIEW" ;;
  esac
}

n_scanned=0
n_gate=0
n_names=0
n_names_hit=0
n_unclassified=0
gate_block=""
advisory_lines=""

# ── Single tree sweep: gating vs advisory per declared exemption ──────────
while IFS= read -r f; do
  if [ -z "$f" ] || [ ! -f "$f" ]; then
    continue
  fi
  n_scanned=$(( n_scanned + 1 ))
  if ! LC_ALL=C grep -qIiE "$PAT" "$f" 2>/dev/null; then
    continue
  fi
  if is_exempt "$f"; then
    cls="$(classify "$f")"
    if [ "$cls" = "UNCLASSIFIED-REVIEW" ]; then
      n_unclassified=$(( n_unclassified + 1 ))
    fi
    advisory_lines="${advisory_lines}${cls}  ${f}"$'\n'
  else
    out="$(LC_ALL=C grep -nIiE "$PAT" "$f" 2>/dev/null || true)"
    n_gate=$(( n_gate + 1 ))
    gate_block="${gate_block}file: ${f}"$'\n'"${out}"$'\n'
  fi
done < <(find . -path ./.git -prune -o -path ./target -prune -o -type f -print | LC_ALL=C sort)

# ── GATE: brand names (crate directories, package, bin) ──────────────────
names=""
for d in crates/*/; do
  if [ -d "$d" ]; then
    names="${names}$(basename "$d")"$'\n'
  fi
done
for m in Cargo.toml crates/*/Cargo.toml; do
  if [ ! -f "$m" ]; then
    continue
  fi
  while IFS= read -r line; do
    n="$(printf '%s' "$line" | sed -E 's/^[[:space:]]*name[[:space:]]*=[[:space:]]*"([^"]+)".*/\1/')"
    if [ -n "$n" ] && [ "$n" != "$line" ]; then
      names="${names}${n}"$'\n'
    fi
  done < <(grep -E '^[[:space:]]*name[[:space:]]*=' "$m" 2>/dev/null || true)
done

name_block=""
while IFS= read -r n; do
  if [ -z "$n" ]; then
    continue
  fi
  n_names=$(( n_names + 1 ))
  if printf '%s' "$n" | LC_ALL=C grep -qiE "$PAT" 2>/dev/null; then
    n_names_hit=$(( n_names_hit + 1 ))
    name_block="${name_block}name: ${n}"$'\n'
  fi
done <<< "$names"

# ── Output ──
n_adv="$(printf '%s' "$advisory_lines" | grep -c . || true)"
echo "gate_files_scanned: $n_scanned (target/ and .git/ excluded; exemptions: reference docs/sintergic .sintergic sovereign-arch scripts/ci MASTER-PLAN* prototype-gap*)"
echo "gate_names_scanned: $n_names"
echo "gate_file_hits: $n_gate"
echo "gate_name_hits: $n_names_hit"
if [ "$n_gate" -gt 0 ]; then
  printf '%s' "$gate_block" | sed 's/^/  HIT /'
fi
if [ "$n_names_hit" -gt 0 ]; then
  printf '%s' "$name_block" | sed 's/^/  HIT /'
fi
echo "advisory_hits: $n_adv (NOT gating — internal/exempt surface, classified)"
if [ "$n_adv" -gt 0 ]; then
  printf '%s' "$advisory_lines" | sed 's/^/  ADV /'
fi

if [ "$n_gate" -eq 0 ] && [ "$n_names_hit" -eq 0 ]; then
  echo "trademark-check: PASS (0 mark on public surfaces; R2 respected; advisory=$n_adv unclassified=$n_unclassified)"
  exit 0
fi
echo "trademark-check: FAIL"
exit 1
