#!/usr/bin/env bash
#  GUARD-KIT:CR-01e3aebf — reflog audit: which commits touched a protected
#  instrument (DETECTION LAYER).
#
#  What it is: walks the reflog entries of one ref and, for each commit, says whether it
#  touched any protected path. It installs no hooks, reads no `GUARD_ACTIVE`, does not depend on
#  any hook having run: detection is by HISTORY. That is why it also sees commits
#  made with `git commit --no-verify` (they are normal commits; the only thing that
#  did not happen is the hook) and commits that are no longer on any branch but remain
#  in the reflog (a reset does not erase the trace).
#
#  Usage:
#    reflog-audit.sh [--limit N] [--since <date>] [--ref <ref>]
#    reflog-audit.sh --help
#
#    --limit N        walks the last N reflog entries (default 50).
#    --since <date>   whatever `git log --since` accepts (`2026-10-29`, `2 days ago`, …).
#                     IF PASSED, IT OVERRIDES --limit: every reflog entry
#                     whose date falls in the window is walked (the filter applies
#                     with the same date semantics as `git log --since`).
#    --ref <ref>      ref whose reflog is audited (default HEAD).
#    No positional argument: a loose path or sha is a usage error (2).
#
#  Single source of patterns: it sources `guard-paths.sh` (sibling, via BASH_SOURCE).
#  This script contains not a single pattern.
#
#  Paths of a commit — declared decision (merge commits included):
#    · the FIRST PARENT is resolved with `git rev-list --parents -n1 <commit>` and
#      compared against with `git diff-tree -r --no-renames <parent> <commit>`. That is,
#      the merge diff against its first branch (equivalent to
#      `--diff-merges=first-parent` / `git show --first-parent`), applied by hand
#      so as not to depend on the git version or on `git show`'s default mode.
#    · root commit (no parents): `git diff-tree -r --no-renames --root <commit>`.
#    · `--no-renames` in both cases: a rename shows up as delete + create,
#      so touching (renaming) an instrument does not escape.
#
#  Output: a header with the audited window, one line per offending commit
#    `<short-sha> | <ISO date> | <author %ae> | <path1>,<path2>`
#  (most recent first) and a footer with the total offending commits and the distinct
#  protected paths. With no offenders it prints `0 toques a instrumentos`.
#
#  Exit codes: 0 = no commit in the measured window touches protected paths · 1 = at
#  least one · 2 = usage or git error (missing ref, unreadable reflog, non-numeric --limit)
#  or SIN EVIDENCIA (0 auditable entries).
#
#  Absence of evidence != absolution (rule of this header): if the reflog has no
#  auditable entries — e.g. after `git reflog expire --expire=now --all` +
#  `git gc --prune=now`, or with a `--since` that falls on no entry — this
#  script does NOT print `0 toques`: it prints `SIN EVIDENCIA` on stderr and exits 2. A
#  `0 toques` is only emitted over a window with `measured entries > 0`.
#
#  Properties: `set -u` and NOT `set -e`; portable on bash 5; paths with spaces
#  supported (arrays + `IFS= read -r`); deterministic order.

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

#  reflog__top: repo root; rc 1 if we are not inside a git repository.
reflog__top() {
  local top
  top="$(git rev-parse --show-toplevel 2>/dev/null)" || return 1
  [ -n "$top" ] || return 1
  printf '%s' "$top"
}

#  reflog__paths_del_commit <commit>: prints one path per line (the diff of the
#  commit against its first parent; root = diff against the empty tree).
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

#  ── reflog entries (most recent first) ──────────────────────────────────────
if [ -n "$ventana" ]; then
  #  --since overrides --limit: EVERY reflog entry is enumerated
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

#  window: date filter with the SAME semantics as `git log --since`
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

#  measured entries = the reflog ones falling INSIDE the declared window.
#  With --since the window is by date (counting what was read is not enough: an old
#  entry outside the window is not audited); with --limit the window IS the N
#  entries read. This number is what decides whether there is evidence or not.
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

#  Absence of evidence != absolution: without auditable entries nothing is emitted
#  ('0 toques' would be a green false negative). It is declared and it exits 2.
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
