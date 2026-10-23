#!/usr/bin/env bash
# test-language-check.sh — regression probe for scripts/ci/language-check.py
#
# Three hermetic fixtures in a throwaway directory (no git, no network, no
# writes outside mktemp -d):
#
#   es/      Spanish prose in markdown, Rust comments (leading and trailing)
#            and a Cargo `description` -> the gate MUST exit 1 and name the
#            offending file:line
#   en/      English prose, identifiers and comments that use the ambiguous
#            words (no, sin, son), probe IDs, paths and a fenced code block
#            quoting Spanish -> the gate MUST exit 0 with 0 hits
#   exempt/  Spanish under docs/sintergic/, a README.es.md and the vendored
#            MIT corpus -> the gate MUST exit 0 (declared exemptions)
#
# Usage: bash scripts/ci/test-language-check.sh
# Exit 0 = the gate behaves as specified.
set -uo pipefail

DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
CHECK="$DIR/language-check.py"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

fail=0
expect() { # description, actual, expected
  if [ "$2" = "$3" ]; then
    echo "  PASS $1"
  else
    echo "  FAIL $1 (got '$2', want '$3')"
    fail=1
  fi
}
contains() { # description, haystack, needle
  case "$2" in
    *"$3"*) echo "  PASS $1" ;;
    *) echo "  FAIL $1 (missing: $3)"; fail=1 ;;
  esac
}

# ── Fixture: Spanish surface ─────────────────────────────────────────────
mkdir -p "$TMP/es/src" "$TMP/es/crates/motor"
cat > "$TMP/es/README.md" <<'MD'
# Motor de prueba

El motor lee el archivo de datos del jugador y lo interpreta con cuidado. Cuando
el archivo no está presente, la carga se detiene y el programa informa el error
por la salida estándar, sin escribir nada en disco.

Cada registro se valida antes de usarse, porque un archivo corrupto puede
provocar un fallo durante la carga del mundo.
MD
cat > "$TMP/es/src/lib.rs" <<'RS'
//! Parser del motor de prueba.
//!
//! El parser lee el archivo y devuelve los registros que el motor necesita.
//! Si el archivo está vacío, se devuelve una lista vacía.

/// Lee el archivo de datos y devuelve el número de registros leídos.
pub fn load(path: &str) -> usize {
    let _ = path;
    0
}

pub fn check() -> bool {
    true // el resultado se informa por la salida estándar
}
RS
cat > "$TMP/es/crates/motor/Cargo.toml" <<'TOML'
[package]
name = "motor"
version = "0.1.0"
description = "Motor de prueba con el parser del juego y la carga del mundo"
TOML

# ── Fixture: English surface with ambiguous words ────────────────────────
mkdir -p "$TMP/en/src" "$TMP/en/docs"
cat > "$TMP/en/README.md" <<'MD'
# Probe fixture

The gate must accept English prose about the absence of a default: there is no
fallback, no silent retry and no implicit conversion in this path. The caller
supplies the table, and `sin` and `son` are borrowed words in this sentence.

```
el archivo de datos del jugador con el parser y la carga del mundo
```

Probe identifiers (PROBE-MAP-001, INV-L99-8) and paths such as
`crates/parser/src/lib.rs` are not prose either.
MD
cat > "$TMP/en/src/lib.rs" <<'RS'
//! Parser entry: no fallback and no silent default.
//!
//! The caller supplies the table; sin and cos are computed elsewhere.
pub fn parse_no(sin_table: &[u8]) -> Option<u32> {
    let _ = sin_table;
    None
}

// no, sin and son are ambiguous words: the density gate must not trip.
RS
cat > "$TMP/en/docs/notes.md" <<'MD'
# Notes

The engine reports failures through the exit code (REQ-PARSE-003), never by
printing a warning and continuing. See `crates/parser/src/lib.rs`.
MD

# ── Fixture: declared exemptions ─────────────────────────────────────────
mkdir -p "$TMP/exempt/docs/sintergic" "$TMP/exempt/reference/dfu-spec"
cat > "$TMP/exempt/README.md" <<'MD'
# Exemptions

The published surface is English; the files below are declared exemptions.
MD
cat > "$TMP/exempt/docs/sintergic/nota.md" <<'MD'
# Nota interna

El motor lee el archivo de datos del jugador y lo interpreta con cuidado. Cada
registro se valida antes de usarse, porque un archivo corrupto puede provocar
un fallo durante la carga del mundo.
MD
cat > "$TMP/exempt/README.es.md" <<'MD'
# Motor

El motor lee el archivo de datos del jugador y lo interpreta con cuidado.
MD
cat > "$TMP/exempt/reference/dfu-spec/readme.md" <<'MD'
# Corpus

El motor lee el archivo de datos del jugador y lo interpreta con cuidado.
MD

echo "test-language-check: language gate probe · CR-6f9f4742"

# ── Scenario 1: Spanish prose must fail the gate ─────────────────────────
out_es="$(python3 "$CHECK" --root "$TMP/es" 2>&1)"
code_es=$?
expect "es: exit code" "$code_es" "1"
contains "es: names README.md" "$out_es" "HIT README.md:"
contains "es: names Rust leading comment" "$out_es" "HIT src/lib.rs:1"
contains "es: names Rust trailing comment" "$out_es" "HIT src/lib.rs:13"
contains "es: names Cargo description" "$out_es" "HIT crates/motor/Cargo.toml:4"
contains "es: verdict" "$out_es" "language-check: FAIL"

# ── Scenario 2: English with ambiguous words must pass ───────────────────
out_en="$(python3 "$CHECK" --root "$TMP/en" 2>&1)"
code_en=$?
expect "en: exit code" "$code_en" "0"
contains "en: zero hits" "$out_en" "hits: 0"
contains "en: verdict" "$out_en" "language-check: PASS"

# ── Scenario 3: declared exemptions must pass ────────────────────────────
out_ex="$(python3 "$CHECK" --root "$TMP/exempt" 2>&1)"
code_ex=$?
expect "exempt: exit code" "$code_ex" "0"
contains "exempt: zero hits" "$out_ex" "hits: 0"

if [ "$fail" -eq 0 ]; then
  echo "test-language-check: PASS (3/3 scenarios)"
  exit 0
fi
echo "test-language-check: FAIL"
exit 1
