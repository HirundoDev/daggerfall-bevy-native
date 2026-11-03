#!/usr/bin/env bash
# GUARD-KIT:CR-b367c9dd — canonical list of protected paths (SINGLE SOURCE).
#
# Contract (cited by the hooks and by docs/NIGHT-RUN-GUARDRAILS.md):
#   GUARD_PROTECTED_PATTERNS   array — 7 literals from the commission's ORDER
#   GUARD_PROTECTED_DERIVED    array — measured coverage (D2 derivatives + gates by PATTERN D2b/D4
#                              + 3 loose names the pattern does not reach)
#   guard_is_protected <path>  exit 0 = protected / 1 = free
#   guard_staged_paths         paths of the git INDEX, NUL-separated
#   guard_print_remedy <path>  exact remedy the rejection prints
#
# Properties: SOURCEABLE (0 effects when sourced) and RE-SOURCEABLE (flat
# assignments, no accumulation). It does NOT use `set -e`: it is sourced from hooks that decide
# their own exit code.

# CR of the lane that introduced the kit (the marker also lives in the wrappers).
GUARD_CR="CR-b367c9dd-db42-4465-9b2f-d8a341aa3135"

# ORDER verbatim: 7 literals protected by the commission (7 and not 8: the directory
# pattern and the file pattern already cover their depth, no need to duplicate).
GUARD_PROTECTED_PATTERNS=(
  'docs/sintergic/validation/closure-register*'
  'docs/sintergic/handoffs/INDEX.jsonl'
  'crates/promotion-gate/**'
  'crates/side-contract/**'
  'docs/sintergic/promotion-gate/promotion-ledger.jsonl'
  'daggerfall-side-projects/*'
  'scripts/*-00[0-9]-*.py'
)

# Declared derivatives: NOT invented, measured over this tree (see D2 of the HH-1).
#  · `daggerfall-side-projects/` does not exist here; the real side-projects tree
#    is `sides/` (e.g. `sides/i1-minimal-side/`).
#  · the glob `scripts/*-00[0-9]-*.py` matches 0 files; the real gate
#    sources are `scripts/<family>-00[0-9].py` (18 files).
# Removing this part is the owner's decision: drop the patterns below and D2
# is void (0 changes in the rest of the kit).
GUARD_PROTECTED_DERIVED=(
  'scripts/*-00[0-9].py'
  'sides/*'
  # D2b (CR-4b576323-4c49-4607-90e3-59c92cf5c2ea) — close gates.
  # D4 (CR-a58f600a-6b7b-4b48-9c31-716625fce035): gate coverage is BY
  # PATTERN, not by loose name. `scripts/*gate*.py` matches exactly 4 files today
  # —closure-gate.py, m43-gate-contract.py, m43_gate_lib.py,
  # plan-gate-check.py— and any NEW `*gate*.py` under scripts/ joins by itself:
  # a third gate cannot be left out tomorrow without editing this array (the pattern
  # is what guarantees it; a list of names only covers what is already known).
  # They are close instruments, not system sources: a nightly run cannot
  # edit what measures it. Each line can be removed/extended on its own
  # (owner's decision).
  'scripts/*gate*.py'
  # Loose names the pattern does NOT reach (they do not carry `gate` in the name).
  'scripts/handoff-lint.py'
  'scripts/mission-log.py'
  'scripts/validate-lane-registry.py'
  # CR-9dd5c91e (S-13/D5): the LANE CLOSE AUDITOR. No pattern reaches it
  # (neither `scripts/*gate*.py` nor `scripts/*-00[0-9].py`: it does not carry `gate`
  # and it hangs off scripts/guard/). It is an instrument: a lane cannot edit what
  # measures it without an authorized CR — exactly the boundary the rest of the kit protects.
  'scripts/guard/lane-integrity.py'
)

# ── internos ────────────────────────────────────────────────────────────────

# strips leading `./` and `/` (index paths are repo-relative)
guard__normalize() {
  local p="${1-}"
  while [ "${p#./}" != "$p" ]; do p="${p#./}"; done
  while [ "${p#/}" != "$p" ]; do p="${p#/}"; done
  printf '%s' "$p"
}

# directory prefix of a `dir/**` or `dir/*` pattern; rc=1 if it is not a directory pattern
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
    # directory pattern: it matches the directory itself and ANY depth
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

# guard__why <path>: prints the pattern protecting it (ORDER first)
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

# guard_staged_paths: INDEX paths, NUL-separated.
# Without --diff-filter: deletes, renames (with --no-renames they already come as a pair) and
# unmerged entries count too — deleting an instrument is touching it.
guard_staged_paths() {
  local top
  top="$(git rev-parse --show-toplevel 2>/dev/null)" || return 0
  [ -n "$top" ] || return 0
  git -C "$top" diff --cached --name-only --no-renames -z
}

# guard_print_remedy <path>: path + why it is protected + exact remedy
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
