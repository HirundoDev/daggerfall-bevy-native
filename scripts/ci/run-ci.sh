#!/usr/bin/env bash
# run-ci.sh — local runner for PHASE 1 CI (M0 PRE-6)
# CR-bbd5db82-f194-4772-932d-364206dca16b
#
# Local equivalent of the .github/workflows/ci.yml workflow: the repo has no
# remote yet (gap declared in docs/sintergic/game/release/), so the
# "first CI PASS" is materialized by running this runner on the same machine.
# Every check ALWAYS runs (no -e) and the verdict aggregates the exit codes.
#
# The language gate (CR-6f9f4742) is the third check: the exportable surface
# carries no Spanish prose. It does not replace the publication gate — the
# public export (tools/public-export/export-public.py) remains the last barrier
# and re-checks the exported tree itself.
#
# Usage: bash scripts/ci/run-ci.sh    (exit 0 = PASS · exit 1 = FAIL)
set -uo pipefail

DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
fail=0

echo "== PHASE 1 CI (anti-asset / anti-trademark / English-only) · CR-bbd5db82 =="
"$DIR/content-scan.sh" || fail=1
"$DIR/trademark-check.sh" || fail=1
"$DIR/language-check.sh" || fail=1

if [ "$fail" -eq 0 ]; then
  echo "ci: PASS (3/3 checks)"
  exit 0
fi
echo "ci: FAIL"
exit 1
