#!/usr/bin/env bash
# GUARD-KIT:CR-b367c9dd — cuerpo del hook commit-msg (brazo de GUARD_ALLOW_CR).
# ESTE ES EL ÚNICO SITIO donde se decide la excepción por CR: pre-commit solo
# bloquea y nombra el remedio, y con GUARD_ALLOW_CR definido imprime
# GUARD-DEFERIDO y cede el paso (= no evalúa nada).
# El pre-commit solo ve el índice; el mensaje del commit en curso NO es legible
# allí (hecho 6: .git/COMMIT_EDITMSG trae el mensaje ANTERIOR). Por eso la
# conjunción «CR en el mensaje» se comprueba aquí, sobre $1.
# NUNCA lee `.git/COMMIT_EDITMSG` ni ninguna ruta fija: git le pasa en $1 el
# fichero del mensaje EN CURSO (con `-m`, `-F` o editor; siempre $1). Leer la
# ruta fija reintroduciría el defecto que cubre el caso L del falsificador
# (`scripts/guard/verify-guard.sh`): un mensaje heredado autorizaría el commit.

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

# sin paths protegidos en el índice no hay nada que verificar
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

# El CR se exige como TOKEN, no como subcadena (R2, CR-4b576323): una
# comparación de subcadena dejaba pasar GUARD_ALLOW_CR=CR-TEST con un mensaje
# que solo mencionaba CR-TEST-9999. Frontera = inicio/fin de texto o cualquier
# carácter fuera de [A-Za-z0-9_-], así que el CR autorizado no puede ir pegado a
# otro identificador. El guion bajo cuenta como carácter de identificador: en
# `CR-TEST-1_suffix` el CR sigue dentro de otro identificador y NO es token
# (D3, CR-3862257b). El ERE se construye escapando todo lo que no sea
# alfanumérico del CR esperado (los CR son del tipo CR-<uuid>, sin `_`: ninguna
# forma legítima de citarlos pierde el pase por esta frontera).
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
