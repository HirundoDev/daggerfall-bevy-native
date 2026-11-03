#!/usr/bin/env bash
# GUARD-KIT:CR-b367c9dd — lista canónica de paths protegidos (FUENTE ÚNICA).
#
# Contrato (citado por los hooks y por docs/NIGHT-RUN-GUARDRAILS.md):
#   GUARD_PROTECTED_PATTERNS   array — 7 literales del ORDER del encargo
#   GUARD_PROTECTED_DERIVED    array — cobertura medida (derivados D2 + gates por PATRÓN D2b/D4
#                              + 3 nombres sueltos que el patrón no alcanza)
#   guard_is_protected <path>  exit 0 = protegido / 1 = libre
#   guard_staged_paths         paths del ÍNDICE git, NUL-separados
#   guard_print_remedy <path>  remedio exacto que imprime el rechazo
#
# Propiedades: SOURCEABLE (0 efectos al sourcear) y RE-SOURCEABLE (asignaciones
# planas, sin acumulación). NO usa `set -e`: se sourcea desde hooks que deciden
# su propio código de salida.

# CR de la lane que introdujo el kit (el marcador vive también en los wrappers).
GUARD_CR="CR-b367c9dd-db42-4465-9b2f-d8a341aa3135"

# ORDER verbatim: 7 literales protegidos por el encargo (7 y no 8: el patrón de
# directorio y el de fichero ya cubren su profundidad, no hace falta duplicar).
GUARD_PROTECTED_PATTERNS=(
  'docs/sintergic/validation/closure-register*'
  'docs/sintergic/handoffs/INDEX.jsonl'
  'crates/promotion-gate/**'
  'crates/side-contract/**'
  'docs/sintergic/promotion-gate/promotion-ledger.jsonl'
  'daggerfall-side-projects/*'
  'scripts/*-00[0-9]-*.py'
)

# Derivados declarados: NO inventados, medidos sobre este árbol (ver D2 del HH-1).
#  · `daggerfall-side-projects/` no existe aquí; el árbol real de side-projects
#    es `sides/` (p.ej. `sides/i1-minimal-side/`).
#  · el glob `scripts/*-00[0-9]-*.py` casa 0 ficheros; los fuentes de gate
#    reales son `scripts/<familia>-00[0-9].py` (18 ficheros).
# Retirar esta parte es decisión del owner: quitar los patrones de abajo y D2
# queda sin efecto (0 cambios en el resto del kit).
GUARD_PROTECTED_DERIVED=(
  'scripts/*-00[0-9].py'
  'sides/*'
  # D2b (CR-4b576323-4c49-4607-90e3-59c92cf5c2ea) — gates de cierre.
  # D4 (CR-a58f600a-6b7b-4b48-9c31-716625fce035): la cobertura de gates es POR
  # PATRÓN, no por nombre suelto. `scripts/*gate*.py` casa hoy exactamente 4
  # ficheros —closure-gate.py, m43-gate-contract.py, m43_gate_lib.py,
  # plan-gate-check.py— y cualquier `*gate*.py` NUEVO bajo scripts/ entra solo:
  # un tercer gate no puede quedar fuera mañana sin editar este array (el patrón
  # es lo que lo garantiza; una lista de nombres solo cubre lo que ya se conoce).
  # Son instrumento de cierre, no fuentes de sistema: una corrida nocturna no
  # puede editar lo que la mide. Cada línea es retirable/ampliable por sí sola
  # (decisión del owner).
  'scripts/*gate*.py'
  # Nombres sueltos que el patrón NO alcanza (no llevan `gate` en el nombre).
  'scripts/handoff-lint.py'
  'scripts/mission-log.py'
  'scripts/validate-lane-registry.py'
  # CR-9dd5c91e (S-13/D5): el AUDITOR DE CIERRE DE LANE. Ningún patrón lo alcanza
  # (ni `scripts/*gate*.py` ni `scripts/*-00[0-9].py`: no lleva `gate` en el nombre
  # y cuelga de scripts/guard/). Es instrumento: una lane no puede editar lo que
  # la mide sin CR autorizado — justo la frontera que el resto del kit protege.
  'scripts/guard/lane-integrity.py'
)

# ── internos ────────────────────────────────────────────────────────────────

# quita `./` y `/` iniciales (los paths del índice son repo-relativos)
guard__normalize() {
  local p="${1-}"
  while [ "${p#./}" != "$p" ]; do p="${p#./}"; done
  while [ "${p#/}" != "$p" ]; do p="${p#/}"; done
  printf '%s' "$p"
}

# prefijo de directorio de un patrón `dir/**` o `dir/*`; rc=1 si no es de directorio
guard__dir_prefix() {
  case "${1-}" in
    */'**') printf '%s' "${1%/\*\*}"; return 0 ;;
    *'/*')  printf '%s' "${1%/\*}";  return 0 ;;
  esac
  return 1
}

# guard__match <path> <patrón>
guard__match() {
  local path="${1-}" pat="${2-}" dir
  if dir="$(guard__dir_prefix "$pat")"; then
    # patrón de directorio: casa el propio directorio y CUALQUIER profundidad
    [ "$path" = "$dir" ] && return 0
    case "$path" in
      "$dir"/*) return 0 ;;
    esac
    return 1
  fi
  # patrón de fichero: glob en `case` (`*` cruza `/`, intencional)
  case "$path" in
    $pat) return 0 ;;
  esac
  return 1
}

# guard__why <path>: imprime el patrón que lo protege (ORDER primero)
guard__why() {
  local path="${1-}" pat
  for pat in "${GUARD_PROTECTED_PATTERNS[@]}"; do
    if guard__match "$path" "$pat"; then printf '%s' "$pat"; return 0; fi
  done
  for pat in "${GUARD_PROTECTED_DERIVED[@]}"; do
    if guard__match "$path" "$pat"; then printf '%s' "$pat"; return 0; fi
  done
  return 1
}

# ── contrato público ─────────────────────────────────────────────────────────

# guard_is_protected <path-rel>: exit 0 protegido / 1 libre
guard_is_protected() {
  local path
  path="$(guard__normalize "${1-}")"
  [ -n "$path" ] || return 1
  guard__why "$path" >/dev/null
}

# guard_staged_paths: paths del ÍNDICE, NUL-separados.
# Sin --diff-filter: deletes, renames (con --no-renames ya vienen como par) y
# entradas unmerged también cuentan — borrar un instrumento es tocarlo.
guard_staged_paths() {
  local top
  top="$(git rev-parse --show-toplevel 2>/dev/null)" || return 0
  [ -n "$top" ] || return 0
  git -C "$top" diff --cached --name-only --no-renames -z
}

# guard_print_remedy <path>: path + por qué está protegido + remedio exacto
guard_print_remedy() {
  local path="${1-}" why origen
  path="$(guard__normalize "$path")"
  if why="$(guard__why "$path")"; then
    case " ${GUARD_PROTECTED_PATTERNS[*]} " in
      *" $why "*) origen="patrón del ORDER (instrumento o registro de cierre)" ;;
      *) origen="derivado declarado D2 (cobertura medida de este árbol)" ;;
    esac
  else
    why="<ninguno>"
    origen="no protegido"
  fi
  printf 'RECHAZO: %s está protegido por `%s` — %s.\n' "$path" "$why" "$origen"
  printf '  Por qué: la corrida nocturna no puede editar el instrumento que la mide.\n'
  printf '  Remedio (i)   descartar el cambio del índice:  git restore --staged -- %s\n' "$path"
  printf '  Remedio (ii)  escritura legítima por CR:  GUARD_ALLOW_CR=%s git commit -m "... %s"\n' "$GUARD_CR" "$GUARD_CR"
  printf '                (el CR debe aparecer LITERAL en el mensaje: lo comprueba el hook commit-msg)\n'
  printf '  NO es remedio: --no-verify (salta los hooks: deja el árbol sin guardarraíl y sin motivo registrado).\n'
}
