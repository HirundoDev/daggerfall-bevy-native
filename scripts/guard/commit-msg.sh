#!/usr/bin/env bash
#  GUARD-KIT:CR-b367c9dd — body of the commit-msg hook (the GUARD_ALLOW_CR arm).
#  THIS IS THE ONLY PLACE where the per-CR exception is decided: pre-commit only
#  blocks and names the remedy, and with GUARD_ALLOW_CR set it prints
#  GUARD-DEFERIDO and steps aside (= it evaluates nothing).
#  The pre-commit only sees the index; the in-progress commit message is NOT readable
#  there (fact 6: .git/COMMIT_EDITMSG carries the PREVIOUS message). That is why the
#  'CR in the message' conjunction is checked here, on $1.
#  It NEVER reads `.git/COMMIT_EDITMSG` or any fixed path: git passes in $1 the
#  IN-PROGRESS message file (with `-m`, `-F` or editor; always $1). Reading the
#  fixed path would bring back the defect covered by case L of the falsifier
#  (`scripts/guard/verify-guard.sh`): an inherited message would authorize the commit.

[ "${GUARD_ACTIVE:-0}" = 1 ] || exit 0

guard_self="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=./guard-paths.sh
. "$guard_self/guard-paths.sh"

guard_msg_file="${1-}"

guard_blocked=()
while IFS= read -r -d '' guard_p; do
  if guard_is_protected "$guard_p"; then
    guard_blocked+=("$guard_p")
  fi
done < <(guard_staged_paths)

#  with no protected paths in the index there is nothing to verify
[ "${#guard_blocked[@]}" -eq 0 ] && exit 0

if [ -z "${GUARD_ALLOW_CR:-}" ]; then
  printf 'GUARD: commit RECHAZADO — el índice toca %d path(s) protegido(s) y GUARD_ALLOW_CR no está definido:\n' \
    "${#guard_blocked[@]}"
  for guard_p in "${guard_blocked[@]}"; do
    guard_print_remedy "$guard_p"
  done
  exit 1
fi

if [ -z "$guard_msg_file" ] || [ ! -f "$guard_msg_file" ]; then
  printf 'GUARD: commit RECHAZADO — commit-msg sin fichero de mensaje legible (%s): no se puede verificar GUARD_ALLOW_CR=%s.\n' \
    "${guard_msg_file:-<vacío>}" "$GUARD_ALLOW_CR"
  exit 1
fi

#  The CR is required as a TOKEN, not as a substring (R2, CR-4b576323): a
#  substring comparison let GUARD_ALLOW_CR=CR-TEST through with a message
#  that only mentioned CR-TEST-9999. Boundary = start/end of text or any
#  character outside [A-Za-z0-9_-], so the authorized CR cannot sit glued to
#  another identifier. The underscore counts as an identifier character: in
#  `CR-TEST-1_suffix` the CR is still inside another identifier and is NOT a token
#  (D3, CR-3862257b). The ERE is built by escaping everything that is not
#  alphanumeric in the expected CR (CRs are of the form CR-<uuid>, with no `_`: no
#  legitimate way of citing them loses the pass through this boundary).
guard_cr_token_ere() {
  local cr="${1-}" esc='' c i
  for (( i = 0; i < ${#cr}; i++ )); do
    c="${cr:i:1}"
    case "$c" in
      [A-Za-z0-9]) esc+="$c" ;;
      *)           esc+="\\$c" ;;
    esac
  done
  printf '(^|[^A-Za-z0-9_-])%s([^A-Za-z0-9_-]|$)' "$esc"
}

guard_msg="$(< "$guard_msg_file")"
guard_cr_ere="$(guard_cr_token_ere "$GUARD_ALLOW_CR")"
if ! [[ "$guard_msg" =~ $guard_cr_ere ]]; then
  printf 'GUARD: commit RECHAZADO — GUARD_ALLOW_CR=%s pero el mensaje NO contiene ese CR como token delimitado.\n' "$GUARD_ALLOW_CR"
  printf '  Token exigido: %s entre fronteras (inicio/fin de texto o cualquier carácter fuera de [A-Za-z0-9_-]), nunca dentro de otro identificador (p.ej. %s-9999 o %s_suffix no valen).\n' \
    "$GUARD_ALLOW_CR" "$GUARD_ALLOW_CR" "$GUARD_ALLOW_CR"
  printf '  Remedio: añadir %s al mensaje (git commit -m "... %s ...") o descartar el cambio del índice con git restore --staged -- <path>.\n' \
    "$GUARD_ALLOW_CR" "$GUARD_ALLOW_CR"
  exit 1
fi

printf 'GUARD-EXCEPCION: el mensaje verifica GUARD_ALLOW_CR=%s sobre %d path(s) protegido(s).\n' \
  "$GUARD_ALLOW_CR" "${#guard_blocked[@]}"
exit 0
