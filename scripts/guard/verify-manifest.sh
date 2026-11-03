#!/usr/bin/env bash
#  GUARD-KIT:CR-01e3aebf — falsifier of the DETECTION LAYER: 5 boxes (a..e).
#
#  It exercises the mechanism in a TEMPORARY repo (`mktemp -d`), never on the tree of
#  the target: it copies the needed kit (`guard-paths.sh`, `manifest-protected.sh`,
#  `reflog-audit.sh`), seeds the minimum protected paths and leaves a `pre-commit`
#  MARKER hook that does `exit 0` (blocks nothing) so that `--no-verify`
#  has something real to skip in box (c) and its absence is measurable.
#
#  Mandatory matrix (expected vs observed):
#    (a) freshly written manifest ⇒ `--check` rc=0, all lines `OK`, RESUMEN
#        without AUSENTE/CAMBIADO/NUEVO.
#    (b) `scripts/closure-gate.py` is edited and committed ⇒ `--check` rc=1 and prints
#        `CAMBIADO scripts/closure-gate.py`.
#    (c) `git commit --no-verify` touching `docs/sintergic/handoffs/INDEX.jsonl` ⇒
#        `reflog-audit.sh` rc=1 and names that path; and the marker hook left NO mark
#        (it did not run) — with a prior positive control that on a normal commit it DOES leave it.
#    (d) declared revert: the original BYTES of the two paths are restored and
#        re-committed (not `git revert`: with an added path, revert would DELETE it and the
#        state would be AUSENTE, not OK) ⇒ `--check` returns to rc=0 and `reflog-audit.sh`
#        still sees the offending commits (history is not rewritten), including
#        the sha of the commit from (c).
#    (e) evidence destruction: after an offending commit, `git reflog expire
#        --expire=now --all && git gc --prune=now` wipes the reflog ⇒ `reflog-audit.sh`
#        MUST exit 2 and print `SIN EVIDENCIA` (and NOT rc 0 with '0 toques'). This
#        box falsifies the green false negative: if the script returned 0 without
#        evidence, (e) would be FALLA and the falsifier would exit 1. Destroying the
#        reflog is in itself an act a closure must look at: the hole stops being
#        silent, but destroying it is still possible.
#
#  Documented decision for (b)/(c)/(d)/(e): the protected stubs are born UNTRACKED in
#  the baseline commit, so the repository starts with a reflog WITHOUT touches
#  (precondition 2). Committing (b)/(c) turns them tracked; (d) restores their bytes
#  and re-commits. (e) is the LAST box on purpose: it destroys the reflog that (c)/(d)
#  need, so it runs once those two are already measured.
#
#  Blocking preconditions (printed and measured, not assumed):
#    1. deterministic `--write`: two consecutive runs ⇒ identical bytes (`cmp`).
#    2. starting repository WITHOUT touches: `reflog-audit.sh` rc=0 and prints
#       `0 toques a instrumentos`.
#    3. positive control of the marker hook: a normal commit DOES leave its mark
#       (otherwise, the absence of a mark in (c) would prove nothing).
#
#  Exit 0 ONLY if the 5 boxes give what is expected and the 3 preconditions hold.
#  Any deviation ⇒ exit 1 with the matrix printed (and, if it is a setup issue, exit 2).
#
#  Cleanup: `find -delete` + `rmdir` bounded to the mktemp prefix (same criterion as
#  verify-guard.sh); the temporary disappears on exit.

set -u

guard_src="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
guard_tmp="$(mktemp -d "${TMPDIR:-/tmp}/guard-manifest.XXXXXXXX")" || {
  printf 'NO EJECUTADO: no se pudo crear el directorio temporal\n' >&2
  exit 2
}
guard_cleanup() {
  case "$guard_tmp" in
    "${TMPDIR:-/tmp}"/guard-manifest.*) ;;
    *) return 0 ;;
  esac
  [ -d "$guard_tmp" ] || return 0
  find -- "$guard_tmp" -mindepth 1 -depth -delete 2>/dev/null || true
  rmdir -- "$guard_tmp" 2>/dev/null || true
}
trap guard_cleanup EXIT

abortar() {
  printf 'NO EJECUTADO: %s\n' "$*" >&2
  exit 2
}

#  trims to 110 columns for the detail line
recorta() { printf '%s' "$1" | tr '\n' ' ' | cut -c1-110; }

# contiene <texto> <aguja>
contiene() {
  case "${1-}" in
    *"${2-}"*) return 0 ;;
  esac
  return 1
}

nombre=(); comando=(); esperado=(); obtenido=(); ok_caso=(); detalle=()
registrar() {
  nombre+=("$1"); comando+=("$2"); esperado+=("$3"); obtenido+=("$4"); ok_caso+=("$5"); detalle+=("$(recorta "${6-}")")
}

# ── montaje del repo temporal ────────────────────────────────────────────────
repo="$guard_tmp/repo"
marca="$guard_tmp/hook-marca.txt"
mkdir -p "$repo/scripts/guard" "$repo/docs/sintergic/handoffs" "$repo/src" || abortar "mkdir del temporal falló"
for f in guard-paths.sh manifest-protected.sh reflog-audit.sh; do
  [ -f "$guard_src/$f" ] || abortar "falta $f junto a este script ($guard_src)"
  cp -- "$guard_src/$f" "$repo/scripts/guard/$f" || abortar "no se pudo copiar $f al temporal"
done

cd "$repo" || abortar "no se pudo entrar al repo temporal"
git init -q -b main . || abortar "git init falló"
git config user.email "guard-manifest@example.invalid" || abortar "git config email falló"
git config user.name "guard-manifest" || abortar "git config name falló"
git config commit.gpgsign false || abortar "git config gpgsign falló"
#  core.hooksPath is left UNSET (as in the target): the hooks live in
#  .git/hooks by default, and that way the temporary reproduces the real configuration.

#  MARKER hook: leaves a mark and exits 0 (blocks nothing; it serves to measure whether it ran)
printf '#!/usr/bin/env bash\nprintf "HOOK-MARCADOR:pre-commit\\n" >> "%s"\nexit 0\n' "$marca" > .git/hooks/pre-commit || abortar "no se pudo escribir el hook marcador"
chmod +x .git/hooks/pre-commit || abortar "no se pudo dar +x al hook marcador"

#  stubs: 2 protected paths (pattern gate + ORDER ledger) and 1 free path
printf 'stub gate\n' > scripts/closure-gate.py || abortar "stub gate falló"
printf '{}\n' > docs/sintergic/handoffs/INDEX.jsonl || abortar "stub INDEX falló"
printf 'stub libre\n' > src/foo.rs || abortar "stub libre falló"
mkdir -p "$guard_tmp/orig" || abortar "no se pudo crear el directorio de originales"
cp -- scripts/closure-gate.py "$guard_tmp/orig/closure-gate.py" || abortar "no se pudo guardar el original del gate"
cp -- docs/sintergic/handoffs/INDEX.jsonl "$guard_tmp/orig/INDEX.jsonl" || abortar "no se pudo guardar el original del INDEX"

#  baseline: ONLY the kit and the free path (the protected ones stay untracked ⇒ reflog without touches)
git add -- scripts/guard/guard-paths.sh scripts/guard/manifest-protected.sh \
           scripts/guard/reflog-audit.sh src/foo.rs || abortar "git add del baseline falló"
if ! out_base="$(git commit -q -m "baseline: kit de deteccion + path libre (protegidos untracked)" 2>&1)"; then
  abortar "el commit baseline falló: $(recorta "$out_base")"
fi
#  positive control: the marker hook ran in the baseline
marca_base='NO'
[ -s "$marca" ] && marca_base='SI'

#  ── precondition 1: deterministic --write ───────────────────────────────────
#  The manifests live OUTSIDE the temporary repo (in $guard_tmp): that keeps the repo
#  clean (`git status` = 0) and also measures that the absolute path works.
man="$guard_tmp/manifiesto.sha256"
man2="$guard_tmp/manifiesto2.sha256"
out_w1="$(bash scripts/guard/manifest-protected.sh --write "$man" 2>&1)"; rc_w1=$?
out_w2="$(bash scripts/guard/manifest-protected.sh --write "$man2" 2>&1)"; rc_w2=$?
if [ "$rc_w1" -eq 0 ] && [ "$rc_w2" -eq 0 ] && cmp -s "$man" "$man2"; then
  det_1='SI'
else
  det_1='NO'
fi
sha_man1="$(sha256sum "$man" | cut -d' ' -f1)"
sha_man2="$(sha256sum "$man2" | cut -d' ' -f1)"
n_protegidos="$(grep -c . "$man" 2>/dev/null || printf '0')"

# ── precondición 2: reflog limpio (0 toques) ────────────────────────────────
out_aud0="$(bash scripts/guard/reflog-audit.sh --limit 100 2>&1)"; rc_aud0=$?
if [ "$rc_aud0" -eq 0 ] && contiene "$out_aud0" '0 toques a instrumentos'; then
  det_2='SI'
else
  det_2='NO'
fi

# ── precondición 3: control positivo del hook marcador ──────────────────────
if [ "$marca_base" = 'SI' ]; then det_3='SI'; else det_3='NO'; fi

printf 'precondiciones (bloqueantes):\n'
printf '  1. --write determinista (2 corridas, bytes idénticos): %s (sha1=%s sha2=%s)\n' "$det_1" "$sha_man1" "$sha_man2"
printf '     paths protegidos en el manifiesto: %s\n' "$n_protegidos"
printf '  2. repositorio de partida SIN toques (reflog-audit rc=%s, «0 toques a instrumentos»): %s\n' "$rc_aud0" "$det_2"
printf '  3. el hook marcador corre en un commit normal (control positivo): %s\n' "$det_3"

# ── casilla (a) ─────────────────────────────────────────────────────────────
out_a="$(bash scripts/guard/manifest-protected.sh --check "$man" 2>&1)"; rc_a=$?
lineas_ok=0
lineas_no_ok=0
while IFS= read -r l; do
  case "$l" in
    '' | RESUMEN*) continue ;;
    'OK '*) lineas_ok=$((lineas_ok + 1)) ;;
    *) lineas_no_ok=$((lineas_no_ok + 1)) ;;
  esac
done <<< "$out_a"
resumen_a=''
case "$out_a" in
  *RESUMEN*) resumen_a="$(printf '%s\n' "$out_a" | grep '^RESUMEN' | tail -1)" ;;
esac
if [ "$rc_a" -eq 0 ] && [ "$lineas_no_ok" -eq 0 ] && [ "$lineas_ok" -eq "$n_protegidos" ] \
   && contiene "$resumen_a" 'AUSENTE=0' && contiene "$resumen_a" 'CAMBIADO=0' && contiene "$resumen_a" 'NUEVO=0'; then
  registrar a 'manifest-protected.sh --check (manifiesto recién escrito)' \
    'rc=0 · todas las líneas OK · AUSENTE=0 CAMBIADO=0 NUEVO=0' \
    "rc=$rc_a · OK=$lineas_ok · no-OK=$lineas_no_ok · $resumen_a" 'SI' "$resumen_a"
else
  registrar a 'manifest-protected.sh --check (manifiesto recién escrito)' \
    'rc=0 · todas las líneas OK · AUSENTE=0 CAMBIADO=0 NUEVO=0' \
    "rc=$rc_a · OK=$lineas_ok · no-OK=$lineas_no_ok · $resumen_a" 'NO' "$(printf '%s\n' "$out_a" | tail -3)"
fi

# ── casilla (b): editar un path protegido y commitearlo ──────────────────────
printf 'linea de prueba b\n' >> scripts/closure-gate.py
git add -- scripts/closure-gate.py || abortar "git add del caso b falló"
out_cb="$(git commit -q -m "caso b: edita un gate protegido" 2>&1)"; rc_cb=$?
[ "$rc_cb" -eq 0 ] || abortar "el commit del caso b falló: $(recorta "$out_cb")"
out_b="$(bash scripts/guard/manifest-protected.sh --check "$man" 2>&1)"; rc_b=$?
if [ "$rc_b" -eq 1 ] && contiene "$out_b" 'CAMBIADO scripts/closure-gate.py'; then
  registrar b 'editar scripts/closure-gate.py + git commit, luego --check' \
    'rc=1 · imprime `CAMBIADO scripts/closure-gate.py`' \
    "rc=$rc_b · línea: $(printf '%s\n' "$out_b" | grep '^CAMBIADO' | cut -c1-60)" 'SI' \
    "$(printf '%s\n' "$out_b" | grep '^CAMBIADO')"
else
  registrar b 'editar scripts/closure-gate.py + git commit, luego --check' \
    'rc=1 · imprime `CAMBIADO scripts/closure-gate.py`' \
    "rc=$rc_b · salida: $(recorta "$out_b")" 'NO' "$(printf '%s\n' "$out_b" | tail -3)"
fi

# ── casilla (c): --no-verify tocando un path protegido ──────────────────────
: > "$marca"   # se limpia la marca: cualquier marca posterior sería de otro commit
printf 'linea de prueba c\n' >> docs/sintergic/handoffs/INDEX.jsonl
git add -- docs/sintergic/handoffs/INDEX.jsonl || abortar "git add del caso c falló"
out_cc="$(git commit -q --no-verify -m "caso c: toca el ledger protegido con --no-verify" 2>&1)"; rc_cc=$?
[ "$rc_cc" -eq 0 ] || abortar "el commit --no-verify del caso c falló: $(recorta "$out_cc")"
sha_c="$(git rev-parse --short HEAD)"
marca_c='SI'
[ -s "$marca" ] || marca_c='NO'
out_c="$(bash scripts/guard/reflog-audit.sh --limit 100 2>&1)"; rc_c=$?
if [ "$rc_c" -eq 1 ] && contiene "$out_c" 'docs/sintergic/handoffs/INDEX.jsonl' \
   && contiene "$out_c" "$sha_c" && [ "$marca_c" = 'NO' ]; then
  registrar c 'git commit --no-verify tocando docs/sintergic/handoffs/INDEX.jsonl' \
    'reflog-audit rc=1 · nombra el path y el sha · el hook NO dejó marca' \
    "rc=$rc_c · sha=$sha_c · marca del hook=$marca_c · naming=SI" 'SI' \
    "$(printf '%s\n' "$out_c" | grep -F "$sha_c" | cut -c1-110)"
else
  registrar c 'git commit --no-verify tocando docs/sintergic/handoffs/INDEX.jsonl' \
    'reflog-audit rc=1 · nombra el path y el sha · el hook NO dejó marca' \
    "rc=$rc_c · sha=$sha_c · marca del hook=$marca_c · naming=$(contiene "$out_c" 'docs/sintergic/handoffs/INDEX.jsonl' && printf SI || printf NO)" \
    'NO' "$(printf '%s\n' "$out_c" | tail -4)"
fi

# ── casilla (d): revert declarado (restaurar bytes) ─────────────────────────
cp -- "$guard_tmp/orig/closure-gate.py" scripts/closure-gate.py || abortar "restaurar el gate falló"
git add -- scripts/closure-gate.py || abortar "git add del revert del gate falló"
out_rd1="$(git commit -q -m "caso d: revert de bytes en el gate protegido" 2>&1)"; rc_rd1=$?
[ "$rc_rd1" -eq 0 ] || abortar "el commit de revert del gate falló: $(recorta "$out_rd1")"
cp -- "$guard_tmp/orig/INDEX.jsonl" docs/sintergic/handoffs/INDEX.jsonl || abortar "restaurar el INDEX falló"
git add -- docs/sintergic/handoffs/INDEX.jsonl || abortar "git add del revert del INDEX falló"
out_rd2="$(git commit -q -m "caso d: revert de bytes en el ledger protegido" 2>&1)"; rc_rd2=$?
[ "$rc_rd2" -eq 0 ] || abortar "el commit de revert del INDEX falló: $(recorta "$out_rd2")"
out_d="$(bash scripts/guard/manifest-protected.sh --check "$man" 2>&1)"; rc_d=$?
out_d_aud="$(bash scripts/guard/reflog-audit.sh --limit 100 2>&1)"; rc_d_aud=$?
resumen_d="$(printf '%s\n' "$out_d" | grep '^RESUMEN' | tail -1)"
if [ "$rc_d" -eq 0 ] && contiene "$resumen_d" 'AUSENTE=0' && contiene "$resumen_d" 'CAMBIADO=0' \
   && contiene "$resumen_d" 'NUEVO=0' && [ "$rc_d_aud" -eq 1 ] && contiene "$out_d_aud" "$sha_c"; then
  registrar d 'restaurar bytes de ambos paths protegidos + git commit, luego --check y reflog-audit' \
    '--check rc=0 · reflog-audit rc=1 y sigue nombrando el commit de (c)' \
    "check rc=$rc_d · audit rc=$rc_d_aud · $resumen_d" 'SI' \
    "$(printf '%s\n' "$out_d_aud" | grep -F "$sha_c" | cut -c1-110)"
else
  registrar d 'restaurar bytes de ambos paths protegidos + git commit, luego --check y reflog-audit' \
    '--check rc=0 · reflog-audit rc=1 y sigue nombrando el commit de (c)' \
    "check rc=$rc_d · audit rc=$rc_d_aud · $resumen_d" 'NO' \
    "$(printf '%s\n' "$out_d" | tail -3) / $(printf '%s\n' "$out_d_aud" | tail -3)"
fi

#  ── box (e): evidence destruction (reflog expire + gc --prune) ─────────────
#  Last on purpose: it wipes the reflog that (c) and (d) use as evidence.
printf 'linea de prueba e\n' >> scripts/closure-gate.py
git add -- scripts/closure-gate.py || abortar "git add del caso e falló"
out_ce="$(git commit -q -m "caso e: toca un gate protegido antes de destruir la evidencia" 2>&1)"; rc_ce=$?
[ "$rc_ce" -eq 0 ] || abortar "el commit del caso e falló: $(recorta "$out_ce")"
out_exp="$(git reflog expire --expire=now --all 2>&1)"; rc_exp=$?
[ "$rc_exp" -eq 0 ] || abortar "git reflog expire falló: $(recorta "$out_exp")"
out_gcp="$(git gc --prune=now 2>&1)"; rc_gcp=$?
[ "$rc_gcp" -eq 0 ] || abortar "git gc --prune=now falló: $(recorta "$out_gcp")"
out_e="$(bash scripts/guard/reflog-audit.sh --limit 100 2>&1)"; rc_e=$?
if [ "$rc_e" -eq 2 ] && contiene "$out_e" 'SIN EVIDENCIA' \
   && ! contiene "$out_e" '0 toques a instrumentos'; then
  registrar e 'reflog expire --expire=now --all + gc --prune=now, luego reflog-audit' \
    'reflog-audit rc=2 con `SIN EVIDENCIA` (nunca rc=0 «0 toques»)' \
    "rc=$rc_e · SIN EVIDENCIA=$(contiene "$out_e" 'SIN EVIDENCIA' && printf SI || printf NO) · «0 toques»=$(contiene "$out_e" '0 toques a instrumentos' && printf SI || printf NO)" \
    'SI' "$(printf '%s\n' "$out_e" | grep -F 'SIN EVIDENCIA' | cut -c1-110)"
else
  registrar e 'reflog expire --expire=now --all + gc --prune=now, luego reflog-audit' \
    'reflog-audit rc=2 con `SIN EVIDENCIA` (nunca rc=0 «0 toques»)' \
    "rc=$rc_e · SIN EVIDENCIA=$(contiene "$out_e" 'SIN EVIDENCIA' && printf SI || printf NO) · «0 toques»=$(contiene "$out_e" '0 toques a instrumentos' && printf SI || printf NO) · FALLA: sin evidencia NO se absuelve" \
    'NO' "$(printf '%s\n' "$out_e" | tail -4)"
fi

# ── matriz ──────────────────────────────────────────────────────────────────
printf '\n== MATRIZ ==\n'
printf '| caso | comando | esperado | observado | veredicto |\n'
printf '|---|---|---|---|---|\n'
fallos=0
for i in "${!nombre[@]}"; do
  if [ "${ok_caso[$i]}" = 'SI' ]; then v='OK'; else v='FALLA'; fallos=$((fallos + 1)); fi
  printf '| %s | %s | %s | %s | %s |\n' \
    "${nombre[$i]}" "${comando[$i]}" "${esperado[$i]}" "${obtenido[$i]}" "$v"
done

printf '\n== detalle (línea decisoria, recortada a 110 col) ==\n'
for i in "${!nombre[@]}"; do
  printf '%s: %s\n' "${nombre[$i]}" "${detalle[$i]:-<sin salida>}"
done

printf '\n== salida cruda de la precondición 2 (reflog sin toques, rc=%s) ==\n' "$rc_aud0"
printf '%s\n' "$out_aud0"
printf '\n== salida cruda de la auditoría tras el revert (caso d, rc=%s) ==\n' "$rc_d_aud"
printf '%s\n' "$out_d_aud"

printf '\n== condiciones de contorno ==\n'
printf 'repo temporal limpio: %s fichero(s) en git status (esperado 0)\n' "$(git status --porcelain | wc -l | tr -d ' ')"
printf 'temporal de trabajo: %s (borrado al salir)\n' "$guard_tmp"

total="${#nombre[@]}"
pre_fallos=0
[ "$det_1" = 'SI' ] || pre_fallos=$((pre_fallos + 1))
[ "$det_2" = 'SI' ] || pre_fallos=$((pre_fallos + 1))
[ "$det_3" = 'SI' ] || pre_fallos=$((pre_fallos + 1))

if [ "$total" -ne 5 ]; then
  printf '\nRESULTADO: INCOMPLETO — %s/5 casillas ejecutadas · exit 1\n' "$total"
  exit 1
fi
if [ "$pre_fallos" -ne 0 ]; then
  printf '\nRESULTADO: %s precondición(es) FALLIDA(S) de 3 — la matriz no demuestra nada · exit 1\n' "$pre_fallos"
  exit 1
fi
if [ "$fallos" -ne 0 ]; then
  printf '\nRESULTADO: %s/5 casillas OK, %s FALLA(S) — exit 1\n' "$((5 - fallos))" "$fallos"
  exit 1
fi
printf '\nRESULTADO: 5/5 casillas OK — exit 0\n'
exit 0
