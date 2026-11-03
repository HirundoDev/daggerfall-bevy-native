#!/usr/bin/env bash
#  GUARD-KIT:CR-b367c9dd — kit falsifier: 20 cases (A..T).
#
#  Cases M..Q (CR-51d25f1e): falsifier of the LANE-CLOSURE AUDITOR
#  (`lane-integrity.py`). Each case runs `--selftest --case <id>` INSIDE the
#  temporary repo: the instrument builds its dangling commit chains with
#  plumbing (0 refs, 0 HEAD, 0 work index) over the temporary repo, and here we
#  measure that its verdict bites where it must (4 negatives) and that the pass
#  controls give no false red. An instrument that does not bite comes out FALLA and breaks exit 0.
#  Cases S/T (CR-9dd5c91e): the auditor's own two new controls — S measures that
#  the short form of the CR without prefix resolves its commits (before: SIN_COMMITS/FAIL
#  on a lane with commits), T measures that a citation that is a DIRECTORY present on
#  disk comes out WARN and not EVIDENCIA_AUSENTE/FAIL (a false positive by design).
#  Case R (CR-9dd5c91e): `scripts/guard/lane-integrity.py` joins the list of
#  protected paths — editing the auditor without an authorized CR must be REJECTED.
#
#  It exercises the mechanism in a TEMPORARY repo (`mktemp -d`): bare remote + work
#  repo + hooks installed by the real installer. The target tree is only
#  READ (fingerprint before/after). Exit 0 ONLY if the 20 matrix rows match.
#
#  R8 (CR-4b576323): every REJECTION case does not settle for `rc != 0` — it requires the
#  MARKER of the hook that must have bitten (`GUARD: commit RECHAZADO`, `NO
#  contiene`, `GUARD: push RECHAZADO`). An rc != 0 for another cause (typo, broken repo,
#  missing dependency) comes out as 'FALLA (wrong reason)' and breaks exit 0.
#  Case H (R2): token negative control — CR-TEST-1 authorized, message with
#  ONLY CR-TEST-1999 ⇒ REJECTS (before, with substring comparison, it PASSED).
#  Case I (D3, CR-3862257b): `_` is an identifier character — CR-TEST-1
#  authorized, message with `CR-TEST-1_suffix` ⇒ REJECTS (before, with the boundary
#  [^A-Za-z0-9-], the `_` delimited and this commit PASSED).
#  Cases J/K/L (D4, CR-a58f600a): the exception is decided on the REAL message that
#  git passes in `$1` to commit-msg, never on an inherited `.git/COMMIT_EDITMSG`.
#    J — message via `git commit -F <file>` with the CR delimited ⇒ PASSES (the
#        hook reads the file git passes it in $1, not a fixed path).
#    K — same protected path, message WITHOUT the CR ⇒ FALLA with `NO contiene`.
#    L — exact regression of the defect: two steps in the same repo. Step 1 leaves
#        `.git/COMMIT_EDITMSG` with the CR (explicit assertion: if not, the case is
#        EMPTY and comes out FALLA) and step 2, with a new message WITHOUT the CR, must
#        FALLA. A hook that read the inherited fixed path would PASS step 2: that
#        is the defect this case bites.
#
#  Compliance note (deviation D3 of the HH-1): deleting the temporary does NOT use
#  `rm -rf` (harness rule); it uses `find -delete` + `rmdir` bounded to the mktemp
#  prefix. Identical effect: the temporary disappears on exit.

set -u

guard_src="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
guard_target="$(git -C "$guard_src" rev-parse --show-toplevel 2>/dev/null || true)"

guard_tmp="$(mktemp -d "${TMPDIR:-/tmp}/guard-verify.XXXXXXXX")" || {
  printf 'NO EJECUTADO: no se pudo crear el directorio temporal\n' >&2
  exit 2
}
guard_cleanup() {
  case "$guard_tmp" in
    "${TMPDIR:-/tmp}"/guard-verify.*) ;;
    *) return 0 ;;
  esac
  [ -d "$guard_tmp" ] || return 0
  find -- "$guard_tmp" -mindepth 1 -depth -delete 2>/dev/null || true
  rmdir -- "$guard_tmp" 2>/dev/null || true
}
trap guard_cleanup EXIT

nombre=()
comando=()
esperado=()
obtenido=()
detalle=()
marcador=()   # marcador que DEBE imprimir el hook en los casos de rechazo (R8)
marca_ok=()   # SI/NO — ¿apareció ese marcador en la salida real?
forzado=()    # SI/NO — caso declarado FALLO aunque su rc/marcador digan OK (aserción vacua)

abortar() {
  printf 'NO EJECUTADO: %s\n' "$*" >&2
  exit 2
}

#  decisive line of the hook: prefer the REJECTION (the one that closes the case), then
#  the last GUARD line (e.g. the exception verification), then the 1st.
linea_hook() {
  local out="$1" line
  line="$(printf '%s\n' "$out" | grep -E 'RECHAZAD' | cut -c1-110 | head -1)"
  [ -n "$line" ] || line="$(printf '%s\n' "$out" | grep -E 'GUARD' | cut -c1-110 | tail -1)"
  [ -n "$line" ] || line="$(printf '%s\n' "$out" | grep -v '^[[:space:]]*$' | cut -c1-110 | head -1)"
  printf '%s' "$line"
}

#  hooks directory of the REAL target (resolves core.hooksPath when it exists).
target_hooks_dir() {
  local hooks_dir
  hooks_dir="$(git -C "$guard_target" rev-parse --git-path hooks 2>/dev/null || printf '%s/.git/hooks' "$guard_target")"
  case "$hooks_dir" in
    /*) ;;
    *) hooks_dir="$guard_target/$hooks_dir" ;;
  esac
  printf '%s' "$hooks_dir"
}

#  target fingerprint: what the kit owns + state of the installed hooks.
huella_target() {
  local hooks_dir h
  hooks_dir="$(target_hooks_dir)"
  {
    printf 'hooksPath=%s\n' "$(git -C "$guard_target" config --get core.hooksPath 2>/dev/null || printf '<sin-definir>')"
    for h in pre-commit commit-msg pre-push; do
      if [ -f "$hooks_dir/$h" ]; then
        printf '%s %s\n' "$h" "$(sha256sum "$hooks_dir/$h" | cut -d' ' -f1)"
      else
        printf '%s ausente\n' "$h"
      fi
    done
    git -C "$guard_target" status --porcelain=v1 -- scripts/guard docs/NIGHT-RUN-GUARDRAILS.md 2>/dev/null
  } | sha256sum | cut -d' ' -f1
}

#  registrar <case> <command> <expected> <rc> <output> [expected-marker]
#  R8: for rejection cases `rc != 0` is NOT enough — it is measured whether the marker of the
#  expected hook appears in the real output. '' = pass case (no marker required).
registrar() {
  local n="$1" c="$2" e="$3" rc="$4" out="$5" mk="${6-}" got found='—'
  if [ "$rc" -eq 0 ]; then got="PASA (rc=0)"; else got="FALLA (rc=$rc)"; fi
  if [ -n "$mk" ]; then
    case "$out" in
      *"$mk"*) found='SI' ;;
      *)       found='NO' ;;
    esac
    marcador+=("$mk")
  else
    marcador+=('—')
  fi
  nombre+=("$n"); comando+=("$c"); esperado+=("$e"); obtenido+=("$got")
  marca_ok+=("$found")
  forzado+=('NO')
  detalle+=("$(linea_hook "$out")")
}

# caso_commit <caso> <comando-legible> <esperado> <marcador|''> <mensaje> <path> [env...]
caso_commit() {
  local n="$1" c="$2" e="$3" mk="$4" msg="$5" path="$6"
  shift 6
  printf 'linea de prueba %s\n' "$n" >> "$path"
  git add -- "$path"
  local out rc
  out="$(env "$@" git commit -q -m "$msg" 2>&1)"
  rc=$?
  registrar "$n" "$c" "$e" "$rc" "$out" "$mk"
  git reset -q --hard HEAD
}

#  caso_commit_F <case> <readable-command> <expected> <marker|''> <message-file> <path> [env...]
#  Same as caso_commit but the message comes in with `git commit -F <file>`: it proves
#  that commit-msg reads the file git passes in $1, not a fixed path.
caso_commit_F() {
  local n="$1" c="$2" e="$3" mk="$4" msgfile="$5" path="$6"
  shift 6
  printf 'linea de prueba %s\n' "$n" >> "$path"
  git add -- "$path"
  local out rc
  out="$(env "$@" git commit -q -F "$msgfile" 2>&1)"
  rc=$?
  registrar "$n" "$c" "$e" "$rc" "$out" "$mk"
  git reset -q --hard HEAD
}

huella_antes="$(huella_target)"

# ── montaje del repo temporal ────────────────────────────────────────────────
git init -q --bare -b main "$guard_tmp/remote.git" || abortar "git init --bare falló"
git init -q -b main "$guard_tmp/repo" || abortar "git init del repo de trabajo falló"
cd "$guard_tmp/repo" || abortar "no se pudo entrar al repo temporal"
git config user.email "guard-verify@example.invalid"
git config user.name "guard-verify"
git config commit.gpgsign false

mkdir -p scripts/guard
cp -- "$guard_src"/*.sh scripts/guard/ || abortar "no se pudo copiar el kit al temporal"
command -v python3 >/dev/null 2>&1 || abortar "python3 ausente: el auditor lane-integrity.py no puede correr"
for guard_py in "$guard_src"/*.py; do
  [ -e "$guard_py" ] || continue
  cp -- "$guard_py" scripts/guard/ || abortar "no se pudo copiar $guard_py al temporal"
done

mkdir -p docs/sintergic/handoffs docs/sintergic/validation docs/sintergic/promotion-gate \
         crates/promotion-gate/src crates/side-contract/src sides/i1-minimal-side src
printf '{}\n' > docs/sintergic/handoffs/INDEX.jsonl
printf 'stub\n' > docs/sintergic/validation/closure-register-CR-test.md
printf '{}\n' > docs/sintergic/promotion-gate/promotion-ledger.jsonl
printf 'stub\n' > crates/promotion-gate/src/lib.rs
printf 'stub\n' > crates/side-contract/src/lib.rs
printf 'stub\n' > sides/i1-minimal-side/README.md
printf 'stub\n' > scripts/sys-combat-001.py
printf 'stub\n' > src/foo.rs

baseline_paths=(
  docs/sintergic/handoffs/INDEX.jsonl
  docs/sintergic/validation/closure-register-CR-test.md
  docs/sintergic/promotion-gate/promotion-ledger.jsonl
  crates/promotion-gate/src/lib.rs
  crates/side-contract/src/lib.rs
  sides/i1-minimal-side/README.md
  scripts/sys-combat-001.py
  src/foo.rs
)
for f in scripts/guard/*.sh scripts/guard/*.py; do
  [ -e "$f" ] || continue
  baseline_paths+=("$f")
done

# ── precondición 1: instalador real dentro del temporal ──────────────────────
if ! out_inst="$(env -u GUARD_ACTIVE bash scripts/guard/install-hooks.sh 2>&1)"; then
  abortar "install-hooks.sh falló en el repo temporal: $(linea_hook "$out_inst")"
fi
#  ── bit +x step (R3) — blocking in the temporary ─────────────────────────────
#  A hook without +x is IGNORED by git without printing anything: the guard vanishes in
#  silence. That is why it is measured here, not assumed.
printf 'bit +x (temporal, bloqueante): '
for h in pre-commit commit-msg pre-push; do
  [ -x ".git/hooks/$h" ] || abortar "hook $h quedó SIN bit +x en el temporal (R3: git lo ignoraría y la guarda desaparecería en silencio)"
  printf '%s=+x ' "$h"
done
printf '\n'

#  bit +x of the REAL target: REPORTED (this falsifier runs in the temporary; the
#  target is only read). It is the same failure mode, measured where it matters.
guard_x_dir="$(target_hooks_dir)"
guard_x_falta=0
printf 'bit +x (target real, informativo): '
for h in pre-commit commit-msg pre-push; do
  if [ -x "$guard_x_dir/$h" ]; then
    printf '%s=+x ' "$h"
  else
    printf '%s=SIN+x ' "$h"
    guard_x_falta=$((guard_x_falta + 1))
  fi
done
printf '\n'
if [ "$guard_x_falta" -ne 0 ]; then
  printf 'AVISO R3: %s de 3 hook(s) del target NO son ejecutables ⇒ git los IGNORA y la guarda desaparece en SILENCIO. Remedio: bash scripts/guard/install-hooks.sh\n' "$guard_x_falta"
fi

#  ── precondition 2: baseline with the kit OFF (it does not disturb startup) ──
for p in "${baseline_paths[@]}"; do git add -- "$p" || abortar "git add falló en $p"; done
if ! out_base="$(env -u GUARD_ACTIVE git commit -q -m "baseline: stubs protegidos + kit (GUARD_ACTIVE sin definir)" 2>&1)"; then
  abortar "el commit baseline falló con el kit apagado: $(linea_hook "$out_base")"
fi
git remote add origin "$guard_tmp/remote.git" || abortar "no se pudo añadir el remote local"

printf 'precondiciones: install-hooks temporal OK (3/3) · baseline con GUARD_ACTIVE sin definir OK\n'

#  ── cases ────────────────────────────────────────────────────────────────────
#  The 4th argument is the marker the matching hook MUST print;
#  '' in pass cases (no hook must bite).
caso_commit A 'GUARD_ACTIVE=1 commit INDEX.jsonl' 'FALLA' 'GUARD: commit RECHAZADO' \
  'caso A: tocando path protegido' docs/sintergic/handoffs/INDEX.jsonl GUARD_ACTIVE=1

caso_commit B 'GUARD_ACTIVE=1 GUARD_ALLOW_CR=CR-TEST-1 + mensaje con CR' 'PASA' '' \
  'caso B: excepcion CR-TEST-1' docs/sintergic/handoffs/INDEX.jsonl \
  GUARD_ACTIVE=1 GUARD_ALLOW_CR=CR-TEST-1

caso_commit C 'GUARD_ACTIVE=1 commit src/foo.rs' 'PASA' '' \
  'caso C: path libre' src/foo.rs GUARD_ACTIVE=1

out="$(env GUARD_ACTIVE=1 git push -q origin HEAD:refs/heads/guard-caso-d 2>&1)"; rc=$?
registrar D 'GUARD_ACTIVE=1 push de rama a origin' 'FALLA' "$rc" "$out" 'GUARD: push RECHAZADO'

caso_commit E 'sin GUARD_ACTIVE, commit INDEX.jsonl' 'PASA' '' \
  'caso E: kit apagado' docs/sintergic/handoffs/INDEX.jsonl -u GUARD_ACTIVE

caso_commit F 'GUARD_ALLOW_CR=CR-TEST-2 con mensaje SIN el CR' 'FALLA' 'NO contiene' \
  'caso F: mensaje sin el CR' docs/sintergic/handoffs/INDEX.jsonl \
  GUARD_ACTIVE=1 GUARD_ALLOW_CR=CR-TEST-2

git tag -f guard-tag-1 >/dev/null 2>&1 || abortar "no se pudo crear el tag de prueba"
out="$(env GUARD_ACTIVE=1 git push -q origin refs/tags/guard-tag-1 2>&1)"; rc=$?
registrar G 'GUARD_ACTIVE=1 push de tag a origin' 'FALLA' "$rc" "$out" 'GUARD: push RECHAZADO'

#  H (R2, token negative control): CR-TEST-1 authorized, message that only
#  mentions CR-TEST-1999 (superstring). With substring comparison this commit
#  PASSED; with the token conjunction it must REJECT with the CR-in-message
#  arm marker. If it passes ⇒ the token boundary is broken.
caso_commit H 'GUARD_ALLOW_CR=CR-TEST-1 con mensaje que SOLO trae CR-TEST-1999' 'FALLA' 'NO contiene' \
  'caso H: mensaje que solo trae CR-TEST-1999 (supercadena del CR autorizado)' docs/sintergic/handoffs/INDEX.jsonl \
  GUARD_ACTIVE=1 GUARD_ALLOW_CR=CR-TEST-1

#  I (D3, CR-3862257b): `_` does NOT delimit. CR-TEST-1 authorized, message with
#  `identifier CR-TEST-1_suffix`: with the previous boundary ([^A-Za-z0-9-]) the
#  `_` counted as a delimiter and the commit PASSED, even though the CR was still inside
#  another identifier. It must REJECT with the CR-in-message arm marker.
caso_commit I 'GUARD_ALLOW_CR=CR-TEST-1 con mensaje con identificador CR-TEST-1_suffix' 'FALLA' 'NO contiene' \
  'caso I: mensaje con identificador CR-TEST-1_suffix (el _ no delimita)' docs/sintergic/handoffs/INDEX.jsonl \
  GUARD_ACTIVE=1 GUARD_ALLOW_CR=CR-TEST-1

#  J (D4): the message arrives via `git commit -F <file>` and contains the CR as a
#  delimited token ⇒ PASSES. It proves commit-msg reads the file git
#  passes in $1 (the IN-PROGRESS commit's), not a fixed path.
printf 'caso J: excepcion CR-TEST-1 con mensaje via -F\n' > "$guard_tmp/msg-j.txt"
caso_commit_F J 'GUARD_ALLOW_CR=CR-TEST-1 con mensaje via git commit -F (CR delimitado)' 'PASA' '' \
  "$guard_tmp/msg-j.txt" docs/sintergic/handoffs/INDEX.jsonl \
  GUARD_ACTIVE=1 GUARD_ALLOW_CR=CR-TEST-1

#  K (D4): same protected path and same exception, message WITHOUT the CR ⇒ FALLA with
#  the CR-in-message arm marker (the pass is not free).
printf 'caso K: mensaje nuevo sin el CR tras la excepcion\n' > "$guard_tmp/msg-k.txt"
caso_commit_F K 'GUARD_ALLOW_CR=CR-TEST-1 con mensaje via -F SIN el CR' 'FALLA' 'NO contiene' \
  "$guard_tmp/msg-k.txt" docs/sintergic/handoffs/INDEX.jsonl \
  GUARD_ACTIVE=1 GUARD_ALLOW_CR=CR-TEST-1

#  L (D4) — exact regression of the INHERITED-message defect, in two steps over
#  the same temporary repo:
#    step 1: commit with the CR in the message ⇒ PASSES and leaves `.git/COMMIT_EDITMSG`
#            with that CR. EXPLICIT ASSERTION: if the file does not contain it, this
#            case is EMPTY and comes out FALLA (it never passes by accident).
#    step 2: new change to the protected path, message WITHOUT the CR ⇒ FALLA.
#  A hook that read `.git/COMMIT_EDITMSG` instead of `$1` would PASS step 2: that
#  is what is measured here. The case verdict is step 2's.
printf 'caso L paso 1 (heredado): excepcion CR-TEST-1\n' > "$guard_tmp/msg-l1.txt"
printf 'caso L paso 2: mensaje nuevo sin el CR heredado\n' > "$guard_tmp/msg-l2.txt"
printf 'linea de prueba L-paso1\n' >> docs/sintergic/handoffs/INDEX.jsonl
git add -- docs/sintergic/handoffs/INDEX.jsonl
out_l1="$(env GUARD_ACTIVE=1 GUARD_ALLOW_CR=CR-TEST-1 git commit -q -F "$guard_tmp/msg-l1.txt" 2>&1)"; rc_l1=$?
editmsg_cr='NO'
grep -qF 'CR-TEST-1' .git/COMMIT_EDITMSG 2>/dev/null && editmsg_cr='SI'
printf 'L paso 1: rc=%s · .git/COMMIT_EDITMSG contiene CR-TEST-1 tras el paso 1: %s (la aserción anti-vacua)\n' \
  "$rc_l1" "$editmsg_cr"
if [ "$rc_l1" -ne 0 ] || [ "$editmsg_cr" != 'SI' ]; then
  #  empty or broken: FALLA is recorded with the required marker NOT found ⇒ the
  #  verdict falls into 'FALLA (wrong reason)' and the matrix breaks exit 0.
  registrar L 'GUARD_ALLOW_CR=CR-TEST-1, 2 pasos: el mensaje heredado tenía el CR y el nuevo no' 'FALLA' \
    "$rc_l1" "L VACUO: el paso 1 no dejó el CR en .git/COMMIT_EDITMSG (rc=$rc_l1, contiene=$editmsg_cr) — $out_l1" 'NO contiene'
  forzado[$((${#nombre[@]} - 1))]='SI'
  git reset -q --hard HEAD
else
  printf 'linea de prueba L-paso2\n' >> docs/sintergic/handoffs/INDEX.jsonl
  git add -- docs/sintergic/handoffs/INDEX.jsonl
  out_l2="$(env GUARD_ACTIVE=1 GUARD_ALLOW_CR=CR-TEST-1 git commit -q -F "$guard_tmp/msg-l2.txt" 2>&1)"; rc_l2=$?
  registrar L 'GUARD_ALLOW_CR=CR-TEST-1, 2 pasos: el mensaje heredado tenía el CR y el nuevo no' 'FALLA' \
    "$rc_l2" "$out_l2" 'NO contiene'
  printf 'L paso 2: rc=%s · mensaje heredado (COMMIT_EDITMSG del paso 1) SÍ llevaba CR-TEST-1; el mensaje en curso NO ⇒ rechazo esperado\n' "$rc_l2"
  git reset -q --hard HEAD
fi

#  ── cases M..Q: lane-closure auditor (CR-51d25f1e) ─────────────────────────
#  The instrument self-falsifies with 5 cases; here each one is a row. The selftest
#  rc is 0 when the case behaved as it MUST (the negatives come out with the
#  instrument FAIL verdict), and the required marker is the CODE of the finding
#  it must have emitted: without the code, the row is 'FALLA (wrong reason)'.
caso_lane() {
  local id="$1" mk="$2" etiqueta="$3" out rc
  out="$(python3 scripts/guard/lane-integrity.py --selftest --case "$id" 2>&1)"; rc=$?
  registrar "$etiqueta" "lane-integrity --selftest --case $id" 'PASA' "$rc" "$out" "$mk"
}

caso_lane mixto     'A-MIXTO'          M
caso_lane posterior 'GATE_POSTERIOR'   N
caso_lane evidencia 'EVIDENCIA_AUSENTE' O
caso_lane status    'STATUS_SIN_QA'    P
caso_lane pase      '| pase | PASS | PASS |' Q
caso_lane corto     '| corto | PASS | PASS |' S
caso_lane artefacto 'EVIDENCIA_NO_VERSIONADA' T

#  ── case R (CR-9dd5c91e): the closure auditor is an INSTRUMENT, not a source ─
#  The protected path is edited with the kit armed and a message WITHOUT the CR: it must bite
#  with the rejected-commit marker. If the path were not in the list, git
#  would accept the commit and the row would come out FALLA (the instrument would stay editable).
caso_commit R 'GUARD_ACTIVE=1 commit scripts/guard/lane-integrity.py' 'FALLA' 'GUARD: commit RECHAZADO' \
  'caso R: tocando el auditor de cierre (instrumento) sin CR' scripts/guard/lane-integrity.py GUARD_ACTIVE=1

# ── condiciones de contorno ──────────────────────────────────────────────────
refs_remote="$(git -C "$guard_tmp/remote.git" for-each-ref | wc -l | tr -d ' ')"
sucio_tmp="$(git status --porcelain | wc -l | tr -d ' ')"
huella_despues="$(huella_target)"
huella_igual=0
[ "$huella_antes" = "$huella_despues" ] && huella_igual=1

# ── matriz ───────────────────────────────────────────────────────────────────
printf '\n== MATRIZ ==\n'
printf '| caso | comando | esperado | obtenido | marcador esperado | marcador encontrado | veredicto |\n'
printf '|---|---|---|---|---|---|---|\n'
fallos=0
for i in "${!nombre[@]}"; do
  veredicto='FALLO'
  if [ "${esperado[$i]}" = 'PASA' ]; then
    [ "${obtenido[$i]}" = 'PASA (rc=0)' ] && veredicto='OK'
  elif [ "${obtenido[$i]}" != 'PASA (rc=0)' ]; then
    # R8: rc != 0 solo cuenta si mordió el hook ESPERADO
    if [ "${marca_ok[$i]}" = 'SI' ]; then veredicto='OK'; else veredicto='FALLA (motivo equivocado)'; fi
  fi
  #  empty case: its precondition was not met, so it proves nothing ->
  #  FALLO even if rc/marker match (it never passes by accident)
  [ "${forzado[$i]}" = 'SI' ] && veredicto='FALLA (caso vacuo: precondición no cumplida)'
  [ "$veredicto" = 'OK' ] || fallos=$((fallos + 1))
  printf '| %s | %s | %s | %s | %s | %s | %s |\n' \
    "${nombre[$i]}" "${comando[$i]}" "${esperado[$i]}" "${obtenido[$i]}" \
    "${marcador[$i]}" "${marca_ok[$i]}" "$veredicto"
done

printf '\n== detalle (mensaje real del hook, recortado a 110 col) ==\n'
for i in "${!nombre[@]}"; do
  printf '%s: %s\n' "${nombre[$i]}" "${detalle[$i]:-<sin salida>}"
done

printf '\n== condiciones de contorno ==\n'
printf 'refs en el remote local tras D/G: %s (esperado 0)\n' "$refs_remote"
printf 'repo temporal limpio: %s fichero(s) en git status (esperado 0)\n' "$sucio_tmp"
printf 'huella del target idéntica antes/después: %s\n' "$( [ "$huella_igual" -eq 1 ] && printf 'SI' || printf 'NO')"
printf 'temporal de trabajo: %s (borrado al salir)\n' "$guard_tmp"

total="${#nombre[@]}"
if [ "$total" -ne 20 ]; then
  printf '\nRESULTADO: INCOMPLETO — %s/20 casos ejecutados · exit 1\n' "$total"
  exit 1
fi
if [ "$fallos" -ne 0 ]; then
  printf '\nRESULTADO: %s/20 coinciden, %s FALLO(S) — exit 1\n' "$((total - fallos))" "$fallos"
  exit 1
fi
if [ "$refs_remote" -ne 0 ] || [ "$sucio_tmp" -ne 0 ] || [ "$huella_igual" -ne 1 ]; then
  printf '\nRESULTADO: 20/20 filas OK pero condición de contorno FALLIDA (remote/higiene/target) — exit 1\n'
  exit 1
fi
printf '\nRESULTADO: 20/20 OK — exit 0\n'
exit 0
