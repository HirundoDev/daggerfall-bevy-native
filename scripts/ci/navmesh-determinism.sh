#!/usr/bin/env bash
# navmesh-determinism.sh — N5-D (design §3.8 D): `path_digest` ×2 and
# `NavGraph::digest` ×2 must be byte-identical across SEPARATE PROCESSES
# (not just in-process). Runs the NAVMESH-001 harness twice — two distinct
# test-binary processes — and diffs the deterministic verdict block (every
# digest token; timing fields such as `build_ms` and the harness `finished
# in …s` line are excluded because they legitimately vary run-to-run).
set -euo pipefail

cd "$(dirname "$0")/../.."

run() {
  cargo test -p daggerfall-parser-bevy-bridge --test navmesh_measure -- --nocapture 2>/dev/null \
    | grep -ioE '[a-z_]*digest=[0-9a-f]+'
}

p1="$(mktemp)"; p2="$(mktemp)"
trap 'rm -f "$p1" "$p2"' EXIT

run > "$p1"
run > "$p2"

if diff -q "$p1" "$p2" >/dev/null; then
  echo "N5-D PASS: verdict block byte-identical across 2 processes ($(wc -l < "$p1") digest tokens)"
else
  echo "N5-D FAIL: verdict block differs across processes"
  diff "$p1" "$p2" || true
  exit 1
fi
