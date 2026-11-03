#!/usr/bin/env bash
# GUARD-KIT:CR-b367c9dd — cuerpo del hook pre-commit.
# Gate de apagado PRIMERO: sin `GUARD_ACTIVE=1` sale 0 antes de leer nada.
#
# Con GUARD_ALLOW_CR definido este hook NO evalúa la excepción: el mensaje del
# commit en curso todavía no existe en pre-commit (git lo escribe después y lo
# pasa en `$1` a commit-msg). Aquí solo se difiere; la conjunción —CR en el
# mensaje REAL, como TOKEN DELIMITADO, nunca subcadena— la decide `commit-msg.sh`,
# el ÚNICO sitio donde se decide — docs/NIGHT-RUN-GUARDRAILS.md §3.

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
