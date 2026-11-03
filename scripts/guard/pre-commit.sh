#!/usr/bin/env bash
#  GUARD-KIT:CR-b367c9dd — body of the pre-commit hook.
#  The OFF gate FIRST: without `GUARD_ACTIVE=1` it exits 0 before reading anything.
#
#  With GUARD_ALLOW_CR set this hook does NOT evaluate the exception: the in-progress
#  commit message does not exist yet in pre-commit (git writes it afterwards and
#  passes it in `$1` to commit-msg). Here it only defers; the conjunction —CR in the
#  REAL message, as a DELIMITED TOKEN, never a substring— is decided by `commit-msg.sh`,
#  the ONLY place where it is decided — docs/NIGHT-RUN-GUARDRAILS.md §3.

[ "${GUARD_ACTIVE:-0}" = 1 ] || exit 0

guard_self="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=./guard-paths.sh
. "$guard_self/guard-paths.sh"

guard_blocked=()
while IFS= read -r -d '' guard_p; do
  if guard_is_protected "$guard_p"; then
    guard_blocked+=("$guard_p")
  fi
done < <(guard_staged_paths)

[ "${#guard_blocked[@]}" -eq 0 ] && exit 0

if [ -n "${GUARD_ALLOW_CR:-}" ]; then
  printf 'GUARD-DEFERIDO: GUARD_ALLOW_CR=%s definido, pero pre-commit NO evalúa la excepción (el mensaje del commit en curso no existe todavía: git lo escribe después y lo pasa en $1 a commit-msg). La decide commit-msg.sh sobre el mensaje REAL; %d path(s) protegido(s) del índice quedan a la espera de esa verificación. Ver docs/NIGHT-RUN-GUARDRAILS.md §3.\n' \
    "$GUARD_ALLOW_CR" "${#guard_blocked[@]}"
  exit 0
fi

printf 'GUARD: commit RECHAZADO — %d path(s) protegido(s) en el índice:\n' "${#guard_blocked[@]}"
for guard_p in "${guard_blocked[@]}"; do
  guard_print_remedy "$guard_p"
done
exit 1
