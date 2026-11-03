#!/usr/bin/env bash
#  GUARD-KIT:CR-01e3aebf — sha256 manifest of the protected paths (DETECTION LAYER).
#
#  What it is: takes a sha256 fingerprint of ALL protected paths and verifies it later.
#  It does NOT install hooks, does NOT modify hooks, does NOT read `GUARD_ACTIVE`: detection does not
#  depend on the guard being on — it is the other way around, it serves precisely when the
#  hook did not run (§7.2 of docs/NIGHT-RUN-GUARDRAILS.md) or when someone skipped it.
#
#  Usage:
#    manifest-protected.sh --write <file>   writes the manifest and exits 0
#    manifest-protected.sh --check <file>   compares tree vs manifest
#    manifest-protected.sh --help           this text
#
#  Single source of patterns: it sources `guard-paths.sh` (sibling of this file,
#  resolved through BASH_SOURCE). This script contains not a single pattern: the list lives
#  in one place and `guard_is_protected` is the only ownership decision.
#
#  Path universe: `git ls-files -z` (tracked) + `git ls-files -z --others
#  --exclude-standard` (present on disk and not ignored). The second term makes the
#  definition of NUEVO literal ('protected on disk and not in the manifest'): an
#  `untracked` instrument from another lane (e.g. a new gate with no `git add` yet) is
#  an instrument all the same, and the hook guard sees it too.
#
#  Manifest format: one line per path, `sha256  <rel-path>`, sorted by
#  path with `LC_ALL=C sort`. Deterministic: two consecutive runs leave identical
#  bytes (no timestamps, no absolute paths, no directory order).
#  Declared limit: paths with a newline or a backslash are not supported
#  (the format is one line per file); in this tree they do not exist.
#
#  States `--check` prints (one per line, no decoration, stdout):
#    OK <path>        in the manifest and on disk, identical sha
#    AUSENTE <path>   in the manifest and no longer on disk (deleted)
#    CAMBIADO <path>  in both, different sha (edited)
#    NUEVO <path>     protected on disk and absent from the manifest
#  and a final summary line with the counts per state.
#
#  Exit codes of `--check`: 0 only if ALL are OK · 1 if there is AUSENTE/CAMBIADO/NUEVO
#  · 2 usage error or manifest unreadable/missing/empty. `--write`: 0 on writing
#  · 2 on error (missing repository, unwritable file, impossible hash).
#
#  Properties: `set -u` and NOT `set -e` (the flow decides the codes); portable on bash 5;
#  paths with spaces supported (arrays + `IFS= read -r -d ''`); deterministic output.

set -u

manifest_self="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=./guard-paths.sh
. "$manifest_self/guard-paths.sh"

uso() {
  printf 'Uso: manifest-protected.sh --write <fichero>\n'
  printf '     manifest-protected.sh --check <fichero>\n'
  printf '     manifest-protected.sh --help\n'
  printf '\n'
  printf 'Contrato:\n'
  printf '  --write <f>  escribe el manifiesto sha256 de TODOS los paths protegidos\n'
  printf '               (formato `sha256  <path-rel>`, orden LC_ALL=C por path); sale 0.\n'
  printf '  --check <f>  compara árbol vs manifiesto e imprime una línea por path:\n'
  printf '               `OK|AUSENTE|CAMBIADO|NUEVO <path>` + línea RESUMEN de conteos.\n'
  printf '               exit 0 = todo OK · 1 = hay AUSENTE/CAMBIADO/NUEVO · 2 = uso/manifiesto.\n'
  printf '\n'
  printf '  Lista de protegidos: fuente única `scripts/guard/guard-paths.sh` (se sourcea).\n'
  printf '  No lee GUARD_ACTIVE: funciona con el guard APAGADO (detecta después, no impide).\n'
  printf '  La ruta del manifiesto la elige quien cierra el ciclo (recomendada\n'
  printf '  `docs/sintergic/guard/protected.sha256`); relativa al directorio del invocante.\n'
}

err() {
  printf 'manifest-protected: %s\n' "$*" >&2
}

#  manifest__top: repo root; rc 1 if we are not inside a git repository.
manifest__top() {
  local top
  top="$(git rev-parse --show-toplevel 2>/dev/null)" || return 1
  [ -n "$top" ] || return 1
  printf '%s' "$top"
}

# manifest__paths_protegidos: paths protegidos actuales, NUL-separados, orden
# LC_ALL=C y sin duplicados. Requiere cwd = raíz del repo (paths repo-relativos).
manifest__paths_protegidos() {
  local -a acc=()
  local p
  while IFS= read -r -d '' p; do
    guard_is_protected "$p" && acc+=("$p")
  done < <( { git ls-files -z; git ls-files -z --others --exclude-standard; } )
  [ "${#acc[@]}" -gt 0 ] || return 0
  printf '%s\0' "${acc[@]}" | LC_ALL=C sort -zu
}

#  manifest__hash <path>: sha256 hex of the file; rc 1 if it cannot be read.
manifest__hash() {
  local h
  h="$(sha256sum -- "${1-}" 2>/dev/null)" || return 1
  h="${h%% *}"
  [ "${#h}" -eq 64 ] || return 1
  printf '%s' "$h"
}

# ── --write ──────────────────────────────────────────────────────────────────
manifest_write() {
  local destino="${1-}" top tmp
  [ -n "$destino" ] || { err "--write necesita la ruta del manifiesto"; return 2; }
  #  absolute BEFORE changing directory (relative to the caller, not to the repo)
  case "$destino" in
    /*) ;;
    *) destino="$PWD/$destino" ;;
  esac
  top="$(manifest__top)" || { err "no estamos dentro de un repositorio git"; return 2; }
  cd -- "$top" || { err "no se pudo entrar a $top"; return 2; }

  local -a rutas=()
  mapfile -d '' -t rutas < <(manifest__paths_protegidos)
  [ "${#rutas[@]}" -gt 0 ] || { err "la lista de protegidos salió vacía: revisa $manifest_self/guard-paths.sh"; return 2; }

  tmp="$destino.tmp.$$"
  : > "$tmp" || { err "no se pudo escribir $tmp"; return 2; }
  local p h n=0
  for p in "${rutas[@]}"; do
    if ! h="$(manifest__hash "$p")"; then
      err "no se pudo hashear '$p' (¿permisos?): manifiesto NO escrito, el anterior queda intacto"
      rm -f -- "$tmp"
      return 2
    fi
    printf '%s  %s\n' "$h" "$p" >> "$tmp" || { err "fallo escribiendo $tmp"; rm -f -- "$tmp"; return 2; }
    n=$((n + 1))
  done
  mv -f -- "$tmp" "$destino" || { err "no se pudo mover $tmp a $destino"; rm -f -- "$tmp"; return 2; }
  printf 'manifiesto escrito: %s (%d paths protegidos, orden LC_ALL=C)\n' "$destino" "$n" >&2
  return 0
}

# ── --check ──────────────────────────────────────────────────────────────────
manifest_check() {
  local fichero="${1-}" top
  [ -n "$fichero" ] || { err "--check necesita la ruta del manifiesto"; return 2; }
  case "$fichero" in
    /*) ;;
    *) fichero="$PWD/$fichero" ;;
  esac
  if [ ! -f "$fichero" ]; then
    err "manifiesto inexistente: $fichero (¿sembraste el ciclo con --write?)"
    return 2
  fi
  if [ ! -r "$fichero" ]; then
    err "manifiesto ilegible: $fichero"
    return 2
  fi
  top="$(manifest__top)" || { err "no estamos dentro de un repositorio git"; return 2; }
  cd -- "$top" || { err "no se pudo entrar a $top"; return 2; }

  local -A man_hash=()   # path -> sha256 del manifiesto
  local -A universo=()   # unión manifiesto ∪ protegidos-en-disco
  local line h rest p n_linea=0
  while IFS= read -r line || [ -n "$line" ]; do
    n_linea=$((n_linea + 1))
    case "$line" in
      '' | '#'*) continue ;;
    esac
    h="${line:0:64}"
    rest="${line:64}"
    if [ "${#h}" -ne 64 ] || [ "${rest:0:2}" != '  ' ] || [ -z "${rest:2}" ]; then
      err "manifiesto malformado en la línea $n_linea de $fichero (esperado 'sha256  <path>'): $line"
      return 2
    fi
    case "$h" in
      *[!0-9a-f]*) err "manifiesto malformado en la línea $n_linea de $fichero (sha no hexadecimal): $line"; return 2 ;;
    esac
    p="${rest:2}"
    man_hash["$p"]="$h"
    universo["$p"]=1
  done < "$fichero"

  if [ "${#man_hash[@]}" -eq 0 ]; then
    err "manifiesto vacío: $fichero (0 entradas; un manifiesto sin sujeto no mide nada)"
    return 2
  fi

  local -A actual=()     # paths protegidos presentes en disco según guard-paths.sh
  local -a rutas=()
  mapfile -d '' -t rutas < <(manifest__paths_protegidos)
  for p in "${rutas[@]}"; do
    actual["$p"]=1
    universo["$p"]=1
  done

  local -a orden=()
  mapfile -d '' -t orden < <(printf '%s\0' "${!universo[@]}" | LC_ALL=C sort -zu)

  local ok=0 ausente=0 cambiado=0 nuevo=0
  local hh
  for p in "${orden[@]}"; do
    if [ -n "${man_hash[$p]+x}" ]; then
      if [ ! -e "$p" ]; then
        printf 'AUSENTE %s\n' "$p"
        ausente=$((ausente + 1))
      elif hh="$(manifest__hash "$p")" && [ "$hh" = "${man_hash[$p]}" ]; then
        printf 'OK %s\n' "$p"
        ok=$((ok + 1))
      else
        printf 'CAMBIADO %s\n' "$p"
        cambiado=$((cambiado + 1))
      fi
    else
      printf 'NUEVO %s\n' "$p"
      nuevo=$((nuevo + 1))
    fi
  done

  printf 'RESUMEN OK=%d AUSENTE=%d CAMBIADO=%d NUEVO=%d TOTAL=%d\n' \
    "$ok" "$ausente" "$cambiado" "$nuevo" "$((ok + ausente + cambiado + nuevo))"

  [ $((ausente + cambiado + nuevo)) -eq 0 ] && return 0
  return 1
}

# ── main ─────────────────────────────────────────────────────────────────────
case "${1-}" in
  --help | -h | '')
    uso
    #  no arguments = usage error (2); with --help = 0
    [ "${1-}" = '' ] && exit 2
    exit 0
    ;;
  --write)
    manifest_write "${2-}"
    exit $?
    ;;
  --check)
    manifest_check "${2-}"
    exit $?
    ;;
  *)
    err "opción desconocida: $1"
    uso >&2
    exit 2
    ;;
esac
