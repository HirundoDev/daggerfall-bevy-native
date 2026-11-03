#!/usr/bin/env bash
# GUARD-KIT:CR-b367c9dd — instalador idempotente de los 3 hooks del kit.
#
# Escribe `.git/hooks/{pre-commit,commit-msg,pre-push}` como wrappers SIN lógica:
# resuelven la raíz del repo y fuentean `scripts/guard/<hook>.sh`. Los bytes del
# wrapper son deterministas (sin timestamps ni rutas absolutas) ⇒ correrlo dos
# veces deja los mismos sha256.
#
# NUNCA sobreescribe un hook ajeno: si el destino existe y no lleva el marcador
# del kit, sale 2 sin escribir nada (ni backup). Esta lane no toca hooks de otras.

set -u

GUARD_MARK='# GUARD-KIT:CR-b367c9dd'
GUARD_HOOKS=(pre-commit commit-msg pre-push)
GUARD_DRY=0

uso() {
  printf '%s\n' \
    'uso: install-hooks.sh [--dry-run]' \
    '  Instala .git/hooks/{pre-commit,commit-msg,pre-push} como wrappers del kit.' \
    '  --dry-run   imprime el plan y NO escribe nada.' \
    '  Un hook ajeno (sin el marcador del kit) ⇒ exit 2 sin escribir nada.' \
    '  Uso normal: bash scripts/guard/install-hooks.sh  (re-ejecutable, idempotente).'
}

for guard_arg in "$@"; do
  case "$guard_arg" in
    --dry-run|-n) GUARD_DRY=1 ;;
    -h|--help) uso; exit 0 ;;
    *)
      printf 'argumento desconocido: %s\n' "$guard_arg" >&2
      uso >&2
      exit 64
      ;;
  esac
done

guard_self="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
if ! guard_top="$(git -C "$guard_self" rev-parse --show-toplevel 2>/dev/null)"; then
  printf 'ERROR: %s no está dentro de un repo git\n' "$guard_self" >&2
  exit 1
fi
[ -n "$guard_top" ] || { printf 'ERROR: toplevel vacío\n' >&2; exit 1; }

guard_hooks_dir="$(git -C "$guard_top" rev-parse --git-path hooks)"
case "$guard_hooks_dir" in
  /*) ;;
  *) guard_hooks_dir="$guard_top/$guard_hooks_dir" ;;
esac

printf 'repo:   %s\n' "$guard_top"
printf 'hooks:  %s\n' "$guard_hooks_dir"
printf 'modo:   %s\n\n' "$( [ "$GUARD_DRY" -eq 1 ] && printf 'DRY-RUN (0 escrituras)' || printf 'APLICAR' )"

# ── wrapper determinista (0 lógica propia) ───────────────────────────────────
guard_wrapper() {
  local hook="$1"
  printf '#!/usr/bin/env bash\n'
  printf '%s\n' "$GUARD_MARK"
  printf '# Wrapper generado por scripts/guard/install-hooks.sh — NO editar a mano (se sobreescribe).\n'
  printf '# 0 lógica propia: resuelve la raíz del repo y fuentea el cuerpo del kit.\n'
  printf '# El gate GUARD_ACTIVE vive en el cuerpo (scripts/guard/%s.sh), no aquí.\n' "$hook"
  printf 'top="$(git rev-parse --show-toplevel 2>/dev/null)" || exit 0\n'
  printf '[ -n "$top" ] || exit 0\n'
  printf '. "$top/scripts/guard/%s.sh"\n' "$hook"
}

# ── pase 1: pre-validar TODOS los destinos antes de escribir ninguno ─────────
guard_conflict=0
for guard_hook in "${GUARD_HOOKS[@]}"; do
  guard_dst="$guard_hooks_dir/$guard_hook"
  if [ -e "$guard_dst" ] && ! grep -Fq -- "$GUARD_MARK" "$guard_dst" 2>/dev/null; then
    printf 'RECHAZO: %s ya existe y NO es del kit (sin marcador %s).\n' "$guard_dst" "$GUARD_MARK" >&2
    printf '  Esta lane no toca hooks ajenos: 0 backup, 0 sobreescritura, 0 escrituras en este pase.\n' >&2
    guard_conflict=1
  fi
done
if [ "$guard_conflict" -ne 0 ]; then
  printf 'RESULTADO: instalación ABORTADA (exit 2). Resolver el conflicto a mano con GO del owner.\n' >&2
  exit 2
fi

# ── pase 2: instalar ─────────────────────────────────────────────────────────
if [ ! -d "$guard_hooks_dir" ]; then
  if [ "$GUARD_DRY" -eq 1 ]; then
    printf '[dry-run] crearía el directorio %s\n' "$guard_hooks_dir"
  else
    mkdir -p "$guard_hooks_dir" || { printf 'ERROR: no se pudo crear %s\n' "$guard_hooks_dir" >&2; exit 1; }
  fi
fi

guard_instalados=0
for guard_hook in "${GUARD_HOOKS[@]}"; do
  guard_dst="$guard_hooks_dir/$guard_hook"
  if [ -e "$guard_dst" ]; then guard_estado='sobreescribe wrapper del kit'; else guard_estado='crea'; fi

  if [ "$GUARD_DRY" -eq 1 ]; then
    printf '[dry-run] %-11s -> %s (%s)\n' "$guard_hook" "$guard_dst" "$guard_estado"
    continue
  fi

  guard_tmp="$guard_dst.tmp.$$"
  if ! guard_wrapper "$guard_hook" > "$guard_tmp"; then
    rm -f -- "$guard_tmp"
    printf 'ERROR: no se pudo escribir %s\n' "$guard_dst" >&2
    exit 1
  fi
  chmod 0755 "$guard_tmp" || { rm -f -- "$guard_tmp"; printf 'ERROR: chmod falló en %s\n' "$guard_tmp" >&2; exit 1; }
  mv -f -- "$guard_tmp" "$guard_dst" || { rm -f -- "$guard_tmp"; printf 'ERROR: no se pudo mover a %s\n' "$guard_dst" >&2; exit 1; }

  guard_sha='<sha256sum no disponible>'
  if command -v sha256sum >/dev/null 2>&1; then
    guard_sha="$(sha256sum "$guard_dst" | cut -d' ' -f1)"
  fi
  printf 'instalado: %-11s sha256=%s (%s)\n' "$guard_hook" "$guard_sha" "$guard_estado"
  guard_instalados=$((guard_instalados + 1))
done

printf '\n'
if [ "$GUARD_DRY" -eq 1 ]; then
  printf 'DRY-RUN: 0 escrituras. Nada instalado.\n'
else
  printf 'Hooks instalados: %d/3 (%s)\n' "$guard_instalados" "pre-commit commit-msg pre-push"
fi
printf '\nGUARD_ACTIVE → APAGADO. Con la variable sin definir (o !=1) los 3 hooks salen 0 sin leer nada.\n'
printf 'Encendido (comando exacto):\n'
printf '  export GUARD_ACTIVE=1\n'
printf 'Encender es DECISIÓN DEL OWNER. La precondición que lo bloqueaba ya aterrizó: la\n'
printf 'reparación R2 de CR-19b69c84 está en 4e5d852 — encender no espera a cerrar nada más.\n'
printf 'Con el kit encendido, toda escritura a path protegido pasa por GUARD_ALLOW_CR.\n'
printf 'Excepciones: GUARD_ALLOW_CR=<cr> (escritura; commit-msg.sh exige ese CR como token\n'
printf 'delimitado en el mensaje, nunca como subcadena — ver docs/NIGHT-RUN-GUARDRAILS.md §3) y\n'
printf 'GUARD_ALLOW_PUSH="<motivo>" (publicación).\n'
