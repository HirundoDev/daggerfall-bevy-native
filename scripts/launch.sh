#!/usr/bin/env bash
# launch.sh — release launcher with feature flags (release tooling).
#
# A single entry point to (a) launch the game in one of its three modes,
# (b) run an Anti-Screen probe, (c) build without launching. No dependencies
# outside bash + cargo: the repo root is resolved from the script's own path
# (0 hardcoded absolute paths) and the features are derived from the real
# manifest `crates/app/Cargo.toml` (0 invented lists).
#
# Contract, mode matrix and examples:
# docs/sintergic/game/RELEASE-TOOLING.md
#
# Usage: scripts/launch.sh [options] [-- <binary args>]
#        scripts/launch.sh --help
#
# Exit codes: 0 ok · 1 build/binary · 2 usage · 3 environment (window without display)
#             · 124 timeout · >124 the probe/binary's own code.
set -uo pipefail   # no -e: each step adds its exit code by hand

ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
APP_MANIFEST="$ROOT/crates/app/Cargo.toml"
APP_PKG="daggerfall-app"
APP_BIN="daggerfall-app"
LAUNCHER_VERSION="1.0.0"

MODE="preview"
PROBE=""
HEADLESS=0
WINDOW_REQ=0
EXPLICIT_FEATURES=""
DATA_ROOT="${DF_APP_ROOT:-}"
NO_BUILD=0
BUILD_ONLY=0
PRINT_PLAN=0
TIMEOUT_BUILD="${LAUNCH_TIMEOUT_BUILD:-1800}"
TIMEOUT_RUN="${LAUNCH_TIMEOUT_RUN:-900}"
PASSTHRU=()

die() {
  local code="$1"
  shift
  printf 'launch: ERROR: %s\n' "$*" >&2
  exit "$code"
}

say() { printf '%s\n' "$*"; }

usage() {
  cat <<EOF
launch.sh ${LAUNCHER_VERSION} — release launcher

Usage: scripts/launch.sh [options] [-- <binary args>]

Modes:
  --mode complete   Release build + run with window/render (feature \`window\`).
                    Opens the M18 INTEGRATED DEMO (textured retail city + terrain
                    + NPCs + 1996 UI). Requires DISPLAY/WAYLAND_DISPLAY unless --headless.
                    For the legacy M2 window (one ARCH3D block): -- --window.
  --mode preview    Debug build + fast start. Headless by default (CI-safe).
                    This is the default mode.
  --mode pristine   Parity only: debug build, 0 opt-in features, always headless.
                    Declared hook for modern layers/mod packs (still nonexistent):
                    reserved features are rejected until they exist as a
                    cargo feature in crates/app/Cargo.toml.

Execution:
  --probe <ID>      Runs ONE Anti-Screen probe and exits with its exit code
                    (e.g. ALL-001 = M2-M16 suite, VIEW-001, RMB-001, ...).
  --headless        Forces headless: no \`window\` feature, no display requirement.
  --window          Requests window/render (feature \`window\`). Incompatible with
                    --headless and with --mode pristine.
  --features <csv>  Explicit daggerfall-app features (comma/space).
                    \`none\` clears the mode's features. Today only \`window\` exists;
                    \`modern\`/\`mods\` are RESERVED (no cargo feature: exit 2).
  --data-root <dir> Sets DF_APP_ROOT (Mode A data root; must contain ARENA2).
                    Default: \$DF_APP_ROOT or ~/Games/Daggerfall/app.
  --no-build        Do not build: run the binary already present (fails if missing).
  --build-only      Build only and exit (does not run).
  --print-plan      Prints the resolved plan and exits (0 side effects).
  --timeout <sec>   Hard cap for the run (default 900; env LAUNCH_TIMEOUT_RUN).
  --timeout-build <sec>  Hard cap for the build (default 1800; env LAUNCH_TIMEOUT_BUILD).
  -h, --help        This help.

Examples:
  scripts/launch.sh --mode complete                 # release + window (demo)
  scripts/launch.sh --mode complete --headless      # release without window
  scripts/launch.sh --mode preview --headless       # debug headless (CI)
  scripts/launch.sh --probe ALL-001                 # full Anti-Screen suite
  scripts/launch.sh --mode preview --probe RMB-001 --no-build
  scripts/launch.sh --print-plan --mode complete    # see the command without running it
EOF
}

is_declared_feature() {
  local want="$1" f
  while IFS= read -r f; do
    [ "$f" = "$want" ] && return 0
  done < <(awk '
    /^\[features\]/ { inb = 1; next }
    /^\[/          { inb = 0 }
    inb && /^[A-Za-z0-9_-]+[[:space:]]*=/ {
      line = $0
      sub(/[[:space:]]*=.*/, "", line)
      print line
    }
  ' "$APP_MANIFEST")
  return 1
}

is_reserved_feature() {
  case "$1" in
    modern|modern-mod|modern-modpack|mod|mods|modpack|layers|pack-mod) return 0 ;;
    *) return 1 ;;
  esac
}

# ── Argument parsing ──────────────────────────────────────────────────────
while [ $# -gt 0 ]; do
  case "$1" in
    --mode)
      [ $# -ge 2 ] || die 2 "--mode requires a value (complete|preview|pristine)"
      MODE="$2"; shift 2 ;;
    --probe)
      [ $# -ge 2 ] || die 2 "--probe requires an ID (e.g. ALL-001)"
      PROBE="$2"
      case "$PROBE" in -*) die 2 "--probe: invalid ID '$PROBE'" ;; esac
      shift 2 ;;
    --headless) HEADLESS=1; shift ;;
    --window) WINDOW_REQ=1; shift ;;
    --features)
      [ $# -ge 2 ] || die 2 "--features requires a value (\`window\` | \`none\` | csv)"
      EXPLICIT_FEATURES="$2"; shift 2 ;;
    --data-root)
      [ $# -ge 2 ] || die 2 "--data-root requires a directory"
      DATA_ROOT="$2"; shift 2 ;;
    --no-build) NO_BUILD=1; shift ;;
    --build-only) BUILD_ONLY=1; shift ;;
    --print-plan) PRINT_PLAN=1; shift ;;
    --timeout)
      [ $# -ge 2 ] || die 2 "--timeout requires seconds"
      TIMEOUT_RUN="$2"; shift 2 ;;
    --timeout-build)
      [ $# -ge 2 ] || die 2 "--timeout-build requires seconds"
      TIMEOUT_BUILD="$2"; shift 2 ;;
    -h|--help) usage; exit 0 ;;
    --) shift; PASSTHRU=("$@"); break ;;
    -*) die 2 "unknown option: $1 (see --help)" ;;
    *) die 2 "unexpected argument: $1 (binary args go after --)" ;;
  esac
done

case "$TIMEOUT_RUN" in ''|*[!0-9]*) die 2 "--timeout not numeric: '$TIMEOUT_RUN'" ;; esac
case "$TIMEOUT_BUILD" in ''|*[!0-9]*) die 2 "--timeout-build not numeric: '$TIMEOUT_BUILD'" ;; esac

# ── Mode → profile + window default ───────────────────────────────────────
case "$MODE" in
  complete) PROFILE="release"; MODE_WINDOW=1 ;;
  preview)  PROFILE="dev";     MODE_WINDOW=0 ;;
  pristine) PROFILE="dev";     MODE_WINDOW=0 ;;
  *) die 2 "invalid --mode: '$MODE' (complete|preview|pristine)" ;;
esac

EXPLICIT_NONE=0
case ",${EXPLICIT_FEATURES// /,}," in
  *,none,*) EXPLICIT_NONE=1; EXPLICIT_FEATURES="" ;;
esac

WINDOW=0
if [ "$MODE_WINDOW" -eq 1 ] && [ "$HEADLESS" -eq 0 ] && [ "$EXPLICIT_NONE" -eq 0 ]; then
  WINDOW=1
fi
if [ "$WINDOW_REQ" -eq 1 ]; then
  [ "$HEADLESS" -eq 1 ] && die 2 "--window and --headless are incompatible"
  [ "$MODE" = "pristine" ] && die 2 "--mode pristine is parity only (0 modern layers): --window does not apply"
  WINDOW=1
fi

# ── Resolved features (derived from the manifest, 0 invented lists) ───────
FEATURES=()
add_feature() {
  local f="$1" x
  if [ ${#FEATURES[@]} -gt 0 ]; then
    for x in "${FEATURES[@]}"; do
      [ "$x" = "$f" ] && return 0
    done
  fi
  FEATURES+=("$f")
}

if [ "$WINDOW" -eq 1 ]; then
  is_declared_feature "window" || die 1 "manifest $APP_MANIFEST does not declare the 'window' feature"
  add_feature "window"
fi

if [ -n "$EXPLICIT_FEATURES" ]; then
  IFS=',' read -r -a _req <<< "${EXPLICIT_FEATURES// /,}"
  for _f in "${_req[@]}"; do
    [ -z "$_f" ] && continue
    [ "$_f" = "default" ] && continue
    if is_declared_feature "$_f"; then
      add_feature "$_f"
    elif is_reserved_feature "$_f"; then
      die 2 "feature '$_f' RESERVED: not yet present in crates/app/Cargo.toml (modern/mod pack layers)"
    else
      die 2 "unknown feature: '$_f' (declared: $(awk '/^\[features\]/{i=1;next}/^\[/{i=0}i&&/^[A-Za-z0-9_-]+[[:space:]]*=/{l=$0;sub(/[[:space:]]*=.*/,"",l);printf "%s ",l}' "$APP_MANIFEST"))"
    fi
  done
fi

# Headless wins: no combination may end up with `window` while headless.
if [ "$HEADLESS" -eq 1 ] && [ ${#FEATURES[@]} -gt 0 ]; then
  for x in "${FEATURES[@]}"; do
    [ "$x" = "window" ] && die 2 "--headless incompatible with the 'window' feature (window/render)"
  done
fi
FEATURE_CSV="$(IFS=','; echo "${FEATURES[*]:-}")"
HAS_WINDOW=0
case ",$FEATURE_CSV," in
  *,window,*) HAS_WINDOW=1 ;;
esac
FEATURE_LABEL="${FEATURE_CSV:-none}"

# ── Display (only when the window feature is active) ──────────────────────
DISPLAY_OK=1
case ",$FEATURE_CSV," in
  *,window,*) if [ -z "${DISPLAY:-}${WAYLAND_DISPLAY:-}" ]; then
      DISPLAY_OK=0
    fi ;;
esac

# ── Mode A data (DF_APP_ROOT; read-only, 0 copy) ──────────────────────────
if [ -n "$DATA_ROOT" ]; then
  export DF_APP_ROOT="$DATA_ROOT"
fi
if [ -n "${DF_APP_ROOT:-}" ]; then
  EFFECTIVE_DATA="$DF_APP_ROOT"
elif [ -n "${HOME:-}" ]; then
  EFFECTIVE_DATA="$HOME/Games/Daggerfall/app"
else
  EFFECTIVE_DATA=""
fi
if [ -n "$EFFECTIVE_DATA" ] && { [ -d "$EFFECTIVE_DATA/ARENA2" ] || [ -d "$EFFECTIVE_DATA/arena2" ]; }; then
  DATA_LINE="ARENA2 OK -> $EFFECTIVE_DATA/ARENA2"
elif [ -n "$EFFECTIVE_DATA" ]; then
  DATA_LINE="WARNING: ARENA2 not found under $EFFECTIVE_DATA (Mode A probes will fail; override --data-root or \$DF_APP_ROOT)"
else
  DATA_LINE="WARNING: no HOME and no \$DF_APP_ROOT (Mode A probes will fail; use --data-root)"
fi

# ── Build/run paths ───────────────────────────────────────────────────────
TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT/target}"
case "$TARGET_DIR" in
  /*) ;;
  *) TARGET_DIR="$ROOT/$TARGET_DIR" ;;
esac
if [ "$PROFILE" = "release" ]; then
  BIN="$TARGET_DIR/release/$APP_BIN"
else
  BIN="$TARGET_DIR/debug/$APP_BIN"
fi

BUILD_CMD=(cargo build -p "$APP_PKG")
[ "$PROFILE" = "release" ] && BUILD_CMD+=(--release)
[ -n "$FEATURE_CSV" ] && BUILD_CMD+=(--features "$FEATURE_CSV")

RUN_CMD=("$BIN")
[ -n "$PROBE" ] && RUN_CMD+=(--probe "$PROBE")
if [ ${#PASSTHRU[@]} -gt 0 ]; then
  RUN_CMD+=("${PASSTHRU[@]}")
fi

if [ "$MODE" = "pristine" ]; then
  PRISTINE_LINE="pristine: pure parity — 0 modern layers exist today; reserved hook: --features modern|mods (rejected until the manifest declares them)"
else
  PRISTINE_LINE=""
fi

if [ "$PRINT_PLAN" -eq 1 ]; then
  say "launch plan"
  say "  mode       : $MODE ($PROFILE)"
  say "  features   : $FEATURE_LABEL"
  say "  headless   : $([ "$HAS_WINDOW" -eq 0 ] && echo yes || echo no)"
  say "  probe      : ${PROBE:-(none)}"
  say "  data root  : $DATA_LINE"
  if [ "$DISPLAY_OK" -eq 1 ]; then
    say "  display    : ok"
  else
    say "  display    : ABSENT (without --headless it will fail with exit 3)"
  fi
  say "  binary     : $BIN"
  say "  build      : ${BUILD_CMD[*]}"
  if [ "$BUILD_ONLY" -eq 1 ]; then
    say "  run        : — (build-only)"
  else
    say "  run        : ${RUN_CMD[*]}"
  fi
  say "  timeouts   : build ${TIMEOUT_BUILD}s · run ${TIMEOUT_RUN}s"
  [ -n "$PRISTINE_LINE" ] && say "  note       : $PRISTINE_LINE"
  exit 0
fi

if [ "$DISPLAY_OK" -eq 0 ]; then
  die 3 "'window' feature active but no DISPLAY/WAYLAND_DISPLAY (use --headless)"
fi

# ── Hard cap without external dependencies (pure bash) ────────────────────
run_capped() {
  local cap="$1"
  shift
  local pid wd rc
  "$@" &
  pid=$!
  if [ "$cap" -le 0 ]; then
    wait "$pid"
    return $?
  fi
  ( sleep "$cap"; kill -TERM "$pid" 2>/dev/null; sleep 3; kill -KILL "$pid" 2>/dev/null ) >/dev/null 2>&1 &
  wd=$!
  wait "$pid"
  rc=$?
  kill "$wd" 2>/dev/null
  wait "$wd" 2>/dev/null
  if [ "$rc" -eq 143 ] || [ "$rc" -eq 137 ]; then
    printf 'launch: TIMEOUT (%ss) — command aborted\n' "$cap" >&2
    return 124
  fi
  return "$rc"
}

# ── Build ─────────────────────────────────────────────────────────────────
if [ "$NO_BUILD" -eq 1 ]; then
  [ -x "$BIN" ] || die 1 "--no-build: missing binary ($BIN). Drop --no-build to build it."
  say "launch: build skipped (--no-build) · $BIN"
else
  say "launch: build ($PROFILE, features: $FEATURE_LABEL)"
  ( cd "$ROOT" && run_capped "$TIMEOUT_BUILD" "${BUILD_CMD[@]}" )
  rc=$?
  [ "$rc" -eq 0 ] || die 1 "build failed (exit $rc)"
  [ -x "$BIN" ] || die 1 "build OK but binary missing: $BIN"
fi

if [ "$BUILD_ONLY" -eq 1 ]; then
  say "launch: build-only OK · $BIN"
  exit 0
fi

# ── Run ───────────────────────────────────────────────────────────────────
say "launch: data · $DATA_LINE"
[ -n "$PRISTINE_LINE" ] && say "launch: $PRISTINE_LINE"
say "launch: run · ${RUN_CMD[*]}"
run_capped "$TIMEOUT_RUN" "${RUN_CMD[@]}"
rc=$?
say "launch: exit $rc"
exit "$rc"
