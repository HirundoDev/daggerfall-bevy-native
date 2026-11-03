#!/usr/bin/env bash
# GUARD-KIT:CR-01e3aebf — manifiesto sha256 de los paths protegidos (CAPA DE DETECCIÓN).
#
# Qué es: toma una huella sha256 de TODOS los paths protegidos y la verifica después.
# NO instala hooks, NO modifica hooks, NO lee `GUARD_ACTIVE`: la detección no depende
# de que el guard esté encendido — es justo al revés, sirve precisamente cuando el
# hook no corrió (§7.2 de docs/NIGHT-RUN-GUARDRAILS.md) o cuando alguien lo saltó.
#
# Uso:
#   manifest-protected.sh --write <fichero>   escribe el manifiesto y sale 0
#   manifest-protected.sh --check <fichero>   compara árbol vs manifiesto
#   manifest-protected.sh --help              este texto
#
# Fuente única de patrones: sourcea `guard-paths.sh` (hermano de este fichero,
# resuelto por BASH_SOURCE). Este script NO contiene ni un patrón: la lista vive
# en un solo sitio y `guard_is_protected` es la única decisión de propiedad.
#
# Universo de paths: `git ls-files -z` (tracked) + `git ls-files -z --others
# --exclude-standard` (presentes en disco y no ignorados). El segundo término hace
# literal la definición de NUEVO («protegido en disco y no en el manifiesto»): un
# instrumento `untracked` de otra lane (p.ej. un gate nuevo aún sin `git add`) es
# instrumento igual, y el guard de hooks también lo ve.
#
# Formato del manifiesto: una línea por path, `sha256  <path-rel>`, ordenado por
# path con `LC_ALL=C sort`. Determinista: dos corridas consecutivas dejan bytes
# idénticos (sin timestamps, sin rutas absolutas, sin orden de directorio).
# Límite declarado: paths con salto de línea o barra invertida no se soportan
# (el formato es de una línea por fichero); en este árbol no existen.
#
# Estados que imprime `--check` (uno por línea, sin adorno, stdout):
#   OK <path>        en el manifiesto y en disco, sha idéntico
#   AUSENTE <path>   en el manifiesto y ya no en disco (borrado)
#   CAMBIADO <path>  en ambos, sha distinto (editado)
#   NUEVO <path>     protegido en disco y ausente del manifiesto
# y una última línea de resumen con los conteos por estado.
#
# Exit codes de `--check`: 0 solo si TODOS son OK · 1 si hay AUSENTE/CAMBIADO/NUEVO
# · 2 error de uso o manifiesto ilegible/inexistente/vacío. `--write`: 0 al escribir
# · 2 ante error (repositorio ausente, fichero no escribible, hash imposible).
#
# Propiedades: `set -u` y NO `set -e` (el flujo decide códigos); portable en bash 5;
# paths con espacios soportados (arrays + `IFS= read -r -d ''`); salida determinista.

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

# manifest__top: raíz del repo; rc 1 si no estamos dentro de un repositorio git.
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

# manifest__hash <path>: sha256 hex del fichero; rc 1 si no se puede leer.
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
  # absoluta ANTES de cambiar de directorio (relativa al invocante, no al repo)
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
    # sin argumentos = error de uso (2); con --help = 0
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
