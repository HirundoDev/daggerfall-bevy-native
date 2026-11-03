#!/usr/bin/env bash
#  GUARD-KIT:CR-b367c9dd — body of the pre-push hook.
#  It rejects every publication: branches, tags and deletes. The exception is explicit,
#  named and printed (GUARD_ALLOW_PUSH=<reason>).

[ "${GUARD_ACTIVE:-0}" = 1 ] || exit 0

#  The pre-push protocol delivers over stdin: <local-ref> <local-sha> <remote-ref> <remote-sha>
guard_refs=0
guard_tags=0
guard_dels=0
while IFS=' ' read -r guard_local_ref guard_local_sha guard_remote_ref guard_remote_sha; do
  case "${guard_local_ref:-}" in
    '') continue ;;
  esac
  guard_refs=$((guard_refs + 1))
  case "$guard_local_ref" in
    refs/tags/*) guard_tags=$((guard_tags + 1)) ;;
  esac
  case "$guard_local_sha" in
    0000000000000000000000000000000000000000) guard_dels=$((guard_dels + 1)) ;;
  esac
done

if [ -n "${GUARD_ALLOW_PUSH:-}" ]; then
  printf 'GUARD_ALLOW_PUSH: %s\n' "$GUARD_ALLOW_PUSH"
  exit 0
fi

printf 'GUARD: push RECHAZADO — %d ref(s): %d tag(s), %d borrado(s) remoto(s).\n' \
  "$guard_refs" "$guard_tags" "$guard_dels"
printf '  Cubre: push de ramas, push de tags y borrado de ramas/tags remotas (delete).\n'
printf '  Por qué: una corrida nocturna no publica. La publicación la firma el owner.\n'
printf '  Remedio: GUARD_ALLOW_PUSH="<motivo>" git push ...   (el motivo se imprime en este hook)\n'
printf '  NO es remedio: --no-verify (salta los hooks: 0 motivo registrado, 0 guardarraíl).\n'
exit 1
