#!/usr/bin/env bash
# GUARD-KIT:CR-01e3aebf — auditoría del reflog: qué commits tocaron un instrumento
# protegido (CAPA DE DETECCIÓN).
#
# Qué es: recorre las entradas del reflog de un ref y, para cada commit, dice si
# tocó algún path protegido. No instala hooks, no lee `GUARD_ACTIVE`, no depende de
# que ningún hook haya corrido: la detección es por HISTORIA. Por eso ve también los
# commits hechos con `git commit --no-verify` (son commits normales; lo único que
# no ocurrió es el hook) y los commits que ya no están en ninguna rama pero siguen
# en el reflog (un reset no borra el rastro).
#
# Uso:
#   reflog-audit.sh [--limit N] [--since <fecha>] [--ref <ref>]
#   reflog-audit.sh --help
#
#   --limit N        recorre las últimas N entradas del reflog (default 50).
#   --since <fecha>  lo que acepte `git log --since` (`2026-10-29`, `2 days ago`, …).
#                    SI SE PASA, MANDA SOBRE --limit: se recorren todas las entradas
#                    del reflog cuya fecha caiga en la ventana (el filtro se aplica
#                    con la misma semántica de fecha que `git log --since`).
#   --ref <ref>      ref cuyo reflog se audita (default HEAD).
#   Ningún argumento posicional: un path o un sha suelto es error de uso (2).
#
# Fuente única de patrones: sourcea `guard-paths.sh` (hermano, por BASH_SOURCE).
# Este script NO contiene ni un patrón.
#
# Paths de un commit — decisión declarada (commits de merge incluidos):
#   · se resuelve el PRIMER PADRE con `git rev-list --parents -n1 <commit>` y se
#     compara contra él con `git diff-tree -r --no-renames <padre> <commit>`. Es
#     decir: el diff del merge contra su primera rama (equivalente a
#     `--diff-merges=first-parent` / `git show --first-parent`), aplicado a mano
#     para no depender de la versión de git ni del modo por defecto de `git show`.
#   · commit raíz (sin padres): `git diff-tree -r --no-renames --root <commit>`.
#   · `--no-renames` en los dos casos: un rename aparece como borrado + creación,
#     así que tocar (renombrar) un instrumento no se escapa.
#
# Salida: encabezado con la ventana auditada, una línea por commit infractor
#   `<sha-corto> | <fecha ISO> | <autor %ae> | <path1>,<path2>`
# (más reciente primero) y un pie con el total de commits infractores y los paths
# protegidos diferentes. Sin infractores imprime `0 toques a instrumentos`.
#
# Exit codes: 0 = ningún commit de la ventana medida toca paths protegidos · 1 = al
# menos uno · 2 = error de uso o de git (ref inexistente, reflog ilegible, --limit no
# numérico) o SIN EVIDENCIA (0 entradas auditables).
#
# Ausencia de evidencia ≠ absolución (regla de esta cabecera): si el reflog no tiene
# entradas auditables — p.ej. tras `git reflog expire --expire=now --all` +
# `git gc --prune=now`, o con un `--since` que no cae sobre ninguna entrada — este
# script NO imprime `0 toques`: imprime `SIN EVIDENCIA` por stderr y sale 2. Un
# `0 toques` solo se emite sobre una ventana con `entradas medidas > 0`.
#
# Propiedades: `set -u` y NO `set -e`; portable en bash 5; paths con espacios
# soportados (arrays + `IFS= read -r`); orden determinista.

set -u

reflog_self="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=./guard-paths.sh
. "$reflog_self/guard-paths.sh"

uso() {
  printf 'Uso: reflog-audit.sh [--limit N] [--since <fecha>] [--ref <ref>]\n'
  printf '     reflog-audit.sh --help\n'
  printf '\n'
  printf 'Lista los commits del reflog que tocan algún path protegido.\n'
  printf '  --limit N        últimas N entradas del reflog (default 50). Cobertura\n'
  printf '                   PARCIAL: una ventana corta NO equivale a «no hubo toques»\n'
  printf '                   y un cierre debe declarar la ventana usada.\n'
  printf '  --since <fecha>  ventana tipo `git log --since` (`2026-10-29`, `2 days ago`…).\n'
  printf '                   SI SE PASA, MANDA SOBRE --limit: se recorren TODAS las\n'
  printf '                   entradas del reflog dentro de la ventana.\n'
  printf '  --ref <ref>      ref auditado (default HEAD)\n'
  printf '\n'
  printf 'Salida: `<sha-corto> | <fecha ISO> | <autor> | <paths protegidos tocados>`\n'
  printf '        (más reciente primero) + pie con el total de infractores.\n'
  printf 'Exit: 0 = 0 toques en la ventana medida · 1 = hay toques · 2 = uso/git.\n'
  printf '      2 = SIN EVIDENCIA si el reflog no tiene entradas auditables (la\n'
  printf '      ausencia de evidencia no absuelve: no se atestigua «0 toques»).\n'
  printf 'Detecta commits hechos con `git commit --no-verify`: la evidencia es la\n'
  printf 'historia, no el hook. No lee GUARD_ACTIVE (funciona con el guard apagado).\n'
}

err() {
  printf 'reflog-audit: %s\n' "$*" >&2
}

# reflog__top: raíz del repo; rc 1 si no estamos dentro de un repositorio git.
reflog__top() {
  local top
  top="$(git rev-parse --show-toplevel 2>/dev/null)" || return 1
  [ -n "$top" ] || return 1
  printf '%s' "$top"
}

# reflog__paths_del_commit <commit>: imprime un path por línea (los del diff del
# commit contra su primer padre; raíz = diff contra el árbol vacío).
reflog__paths_del_commit() {
  local c="${1-}" linea padres resto primer
  linea="$(git rev-list --parents -n1 "$c" 2>/dev/null)" || return 1
  resto="${linea#* }"
  if [ "$resto" = "$linea" ]; then
    # sin padres: commit raíz
    git diff-tree -r --no-renames --name-only --root "$c" 2>/dev/null
    return $?
  fi
  primer="${resto%% *}"
  git diff-tree -r --no-renames --name-only "$primer" "$c" 2>/dev/null
}

# ── argumentos ───────────────────────────────────────────────────────────────
limite=50
ventana=''
ref='HEAD'

while [ "$#" -gt 0 ]; do
  case "$1" in
    --help | -h) uso; exit 0 ;;
    --limit)
      [ "$#" -ge 2 ] || { err "--limit necesita un valor"; exit 2; }
      limite="$2"; shift 2 ;;
    --limit=*)
      limite="${1#*=}"; shift ;;
    --since)
      [ "$#" -ge 2 ] || { err "--since necesita un valor"; exit 2; }
      ventana="$2"; shift 2 ;;
    --since=*)
      ventana="${1#*=}"; shift ;;
    --ref)
      [ "$#" -ge 2 ] || { err "--ref necesita un valor"; exit 2; }
      ref="$2"; shift 2 ;;
    --ref=*)
      ref="${1#*=}"; shift ;;
    --*)
      err "opción desconocida: $1"; uso >&2; exit 2 ;;
    *)
      err "argumento posicional no admitido: $1 (ningún argumento posicional)"; uso >&2; exit 2 ;;
  esac
done

case "$limite" in
  '' | *[!0-9]*) err "--limit debe ser un entero positivo (recibido: '$limite')"; exit 2 ;;
esac
[ "$limite" -gt 0 ] || { err "--limit debe ser > 0 (recibido: $limite)"; exit 2; }
[ -n "$ventana" ] || ventana=''
[ -n "$ref" ] || { err "--ref vacío"; exit 2; }

top="$(reflog__top)" || { err "no estamos dentro de un repositorio git"; exit 2; }
cd -- "$top" || { err "no se pudo entrar a $top"; exit 2; }

# ── entradas del reflog (más reciente primero) ───────────────────────────────
if [ -n "$ventana" ]; then
  # --since manda sobre --limit: se enumeran TODAS las entradas del reflog
  if ! entradas_raw="$(git reflog show --format=%H "$ref" 2>&1)"; then
    err "no se pudo leer el reflog de '$ref': $entradas_raw"
    exit 2
  fi
else
  if ! entradas_raw="$(git reflog show --format=%H -n "$limite" "$ref" 2>&1)"; then
    err "no se pudo leer el reflog de '$ref': $entradas_raw"
    exit 2
  fi
fi

declare -A visto=()
declare -a commits=()
while IFS= read -r c; do
  [ -n "$c" ] || continue
  [ -n "${visto[$c]+x}" ] && continue
  visto["$c"]=1
  commits+=("$c")
done <<< "$entradas_raw"

n_entradas=0
while IFS= read -r c; do
  [ -n "$c" ] && n_entradas=$((n_entradas + 1))
done <<< "$entradas_raw"

# ventana: filtro por fecha con la MISMA semántica que `git log --since`
if [ -n "$ventana" ] && [ "${#commits[@]}" -gt 0 ]; then
  if ! filtrados="$(printf '%s\n' "${commits[@]}" | git rev-list --no-walk=unsorted --since="$ventana" --stdin 2>&1)"; then
    err "ventana inválida o git falló con --since '$ventana': $filtrados"
    exit 2
  fi
  commits=()
  while IFS= read -r c; do
    [ -n "$c" ] && commits+=("$c")
  done <<< "$filtrados"
fi

# entradas medidas = las del reflog que caen DENTRO de la ventana declarada.
# Con --since la ventana es de fecha (no basta con contar lo leído: una entrada
# vieja fuera de la ventana no se audita); con --limit la ventana SON las N
# entradas leídas. Este número es el que decide si hay evidencia o no.
if [ -n "$ventana" ]; then
  entradas_medidas=0
  if [ "${#commits[@]}" -gt 0 ]; then
    declare -A en_ventana=()
    for c in "${commits[@]}"; do en_ventana["$c"]=1; done
    while IFS= read -r c; do
      [ -n "$c" ] || continue
      [ -n "${en_ventana[$c]+x}" ] && entradas_medidas=$((entradas_medidas + 1))
    done <<< "$entradas_raw"
  fi
else
  entradas_medidas=$n_entradas
fi

# ── auditoría ────────────────────────────────────────────────────────────────
if [ -n "$ventana" ]; then
  printf '== AUDITORÍA DE REFLOG — ventana medida: desde %s · ref=%s ==\n' "$ventana" "$ref"
else
  printf '== AUDITORÍA DE REFLOG — ventana medida: últimas %s entradas de %s (cobertura PARCIAL) ==\n' \
    "$limite" "$ref"
fi
printf 'entradas medidas: %d · commits distintos auditados: %d\n' "$entradas_medidas" "${#commits[@]}"

# Ausencia de evidencia ≠ absolución: sin entradas auditables NO se emite
# «0 toques» (sería un falso negativo en verde). Se declara y se sale 2.
if [ "$entradas_medidas" -eq 0 ]; then
  printf -- '--\n'
  err "SIN EVIDENCIA: el reflog de $ref no tiene entradas auditables (¿git reflog expire / git gc --prune?) — NO se puede atestiguar «0 toques»"
  exit 2
fi

infractores=0
declare -A paths_distintos=()
declare -a lineas=()

for c in "${commits[@]}"; do
  declare -a tocados=() protegidos=()
  mapfile -t tocados < <(reflog__paths_del_commit "$c")
  for p in "${tocados[@]}"; do
    [ -n "$p" ] || continue
    guard_is_protected "$p" && protegidos+=("$p")
  done
  [ "${#protegidos[@]}" -gt 0 ] || continue

  declare -a ordenados=()
  mapfile -t ordenados < <(printf '%s\n' "${protegidos[@]}" | LC_ALL=C sort -u)

  meta="$(git log -1 --format='%h%x1f%aI%x1f%ae' "$c" 2>/dev/null)" || meta=''
  sha_corto="${meta%%$'\x1f'*}"
  resto_meta="${meta#*$'\x1f'}"
  fecha="${resto_meta%%$'\x1f'*}"
  autor="${resto_meta#*$'\x1f'}"
  [ -n "$sha_corto" ] || sha_corto="${c:0:7}"

  lista="$(IFS=','; printf '%s' "${ordenados[*]}")"
  lineas+=("$sha_corto | $fecha | $autor | $lista")
  infractores=$((infractores + 1))
  for p in "${ordenados[@]}"; do paths_distintos["$p"]=1; done
done

if [ "${#lineas[@]}" -gt 0 ]; then
  printf '%s\n' "${lineas[@]}"
fi

printf -- '--\n'
if [ "$infractores" -eq 0 ]; then
  printf 'commits infractores: 0 · paths protegidos diferentes: 0\n'
  printf '0 toques a instrumentos\n'
  exit 0
fi
printf 'commits infractores: %d · paths protegidos diferentes: %d\n' \
  "$infractores" "${#paths_distintos[@]}"
exit 1
