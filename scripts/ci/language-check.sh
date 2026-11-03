#!/usr/bin/env bash
# language-check.sh — CI-ENGLISH-SURFACE-001 · English-only gate (M0 CI)
#
# Gate: the exportable surface (everything tools/public-export/exclude.txt
# does NOT exclude) carries no Spanish prose. The policy is written in
# docs/i18n/POLICY.md and CONTRIBUTING.md ("Language"); this script is what
# makes it fail in CI instead of in a review.
#
# The detector is language-check.py (python3 is already required by the
# public-export tooling). This wrapper keeps the CI entry points uniform:
#
# Usage: bash scripts/ci/language-check.sh [--root DIR] [--verbose] [--list-surface]
#        exit 0 = PASS · exit 1 = Spanish prose on the surface · exit 2 = usage
set -euo pipefail

DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
exec python3 "$DIR/language-check.py" "$@"
