#!/usr/bin/env bash
# GUARD-KIT:CR-b367c9dd — falsificador del kit: 20 casos (A..T).
#
# Casos M..Q (CR-51d25f1e): falsificador del AUDITOR DE CIERRE DE LANE
# (`lane-integrity.py`). Cada caso corre `--selftest --case <id>` DENTRO del
# temporal: el instrumento construye sus cadenas de commits colgantes con
# plumbing (0 refs, 0 HEAD, 0 índice de trabajo) sobre el repo temporal, y aquí
# se mide que su veredicto muerde donde debe (4 negativos) y que los controles de
# pase no dan falso rojo. Un instrumento que no muerde sale FALLA y rompe el exit 0.
# Casos S/T (CR-9dd5c91e): los dos controles nuevos del propio auditor — S mide que
# el CR en forma corta sin prefijo resuelve sus commits (antes: SIN_COMMITS/FAIL
# sobre una lane con commits), T mide que una cita que es DIRECTORIO presente en
# disco sale WARN y no EVIDENCIA_AUSENTE/FAIL (falso positivo por diseño).
# Caso R (CR-9dd5c91e): `scripts/guard/lane-integrity.py` entra en la lista de
# paths protegidos — editar el auditor sin CR autorizado debe RECHAZARSE.
#
# Prueba el mecanismo en un repo TEMPORAL (`mktemp -d`): bare remote + repo de
# trabajo + hooks instalados por el instalador real. El árbol del target solo se
# LEE (huella antes/después). Exit 0 SOLO si las 20 filas de la matriz coinciden.
#
# R8 (CR-4b576323): cada caso de RECHAZO no se conforma con `rc != 0` — exige el
# MARCADOR del hook que debe haber mordido (`GUARD: commit RECHAZADO`, `NO
# contiene`, `GUARD: push RECHAZADO`). Un rc != 0 por otra causa (typo, repo roto,
# dependencia ausente) sale como «FALLA (motivo equivocado)» y rompe el exit 0.
# Caso H (R2): control negativo del token — CR-TEST-1 autorizado, mensaje con
# SOLO CR-TEST-1999 ⇒ RECHAZA (antes, con comparación de subcadena, PASABA).
# Caso I (D3, CR-3862257b): el `_` es carácter de identificador — CR-TEST-1
# autorizado, mensaje con `CR-TEST-1_suffix` ⇒ RECHAZA (antes, con la frontera
# [^A-Za-z0-9-], el `_` delimitaba y este commit PASABA).
# Casos J/K/L (D4, CR-a58f600a): la excepción se decide sobre el mensaje REAL que
# git pasa en `$1` a commit-msg, nunca sobre `.git/COMMIT_EDITMSG` heredado.
#   J — mensaje vía `git commit -F <fichero>` con el CR delimitado ⇒ PASA (el
#       hook lee el fichero que git le pasa en $1, no una ruta fija).
#   K — mismo path protegido, mensaje SIN el CR ⇒ FALLA con `NO contiene`.
#   L — regresión exacta del defecto: dos pasos en el mismo repo. El paso 1 deja
#       `.git/COMMIT_EDITMSG` con el CR (aserción explícita: si no, el caso es
#       VACUO y sale FALLA) y el paso 2, con mensaje nuevo SIN el CR, debe
#       FALLAR. Un hook que leyera la ruta fija heredada PASARÍA el paso 2: ése
#       es el defecto que este caso muerde.
#
# Nota de cumplimiento (desviación D3 del HH-1): el borrado del temporal NO usa
# `rm -rf` (regla del arnés); usa `find -delete` + `rmdir` acotado al prefijo
# mktemp. Efecto idéntico: el temporal desaparece al salir.

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

# línea decisoria del hook: preferir el RECHAZO (la que cierra el caso), luego
# la última línea GUARD (p.ej. la verificación de la excepción), luego la 1ª.
linea_hook() {
  local out="$1" line
  line="$(printf '%s\n' "$out" | grep -E 'RECHAZAD' | cut -c1-110 | head -1)"
  [ -n "$line" ] || line="$(printf '%s\n' "$out" | grep -E 'GUARD' | cut -c1-110 | tail -1)"
  [ -n "$line" ] || line="$(printf '%s\n' "$out" | grep -v '^[[:space:]]*$' | cut -c1-110 | head -1)"
  printf '%s' "$line"
}

# directorio de hooks del target REAL (resuelve core.hooksPath cuando exista).
target_hooks_dir() {
  local hooks_dir
  hooks_dir="$(git -C "$guard_target" rev-parse --git-path hooks 2>/dev/null || printf '%s/.git/hooks' "$guard_target")"
  case "$hooks_dir" in
    /*) ;;
    *) hooks_dir="$guard_target/$hooks_dir" ;;
  esac
  printf '%s' "$hooks_dir"
}

# huella del target: lo que el kit posee + estado de los hooks instalados.
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

# registrar <caso> <comando> <esperado> <rc> <salida> [marcador-esperado]
# R8: en los casos de rechazo NO basta rc != 0 — se mide si el marcador del hook
# esperado aparece en la salida real. '' = caso de pase (sin marcador exigido).
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

# caso_commit_F <caso> <comando-legible> <esperado> <marcador|''> <fichero-mensaje> <path> [env...]
# Igual que caso_commit pero el mensaje entra con `git commit -F <fichero>`: prueba
# que commit-msg lee el fichero que git le pasa en $1, no una ruta fija.
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
# ── paso bit +x (R3) — bloqueante en el temporal ─────────────────────────────
# Un hook sin +x es IGNORADO por git sin imprimir nada: la guarda desaparece en
# silencio. Por eso se mide aquí, no se supone.
printf 'bit +x (temporal, bloqueante): '
for h in pre-commit commit-msg pre-push; do
  [ -x ".git/hooks/$h" ] || abortar "hook $h quedó SIN bit +x en el temporal (R3: git lo ignoraría y la guarda desaparecería en silencio)"
  printf '%s=+x ' "$h"
done
printf '\n'

# bit +x del target REAL: se REPORTA (este falsificador corre en el temporal; el
# target solo se lee). Es el mismo modo de fallo, medido donde importa.
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

# ── precondición 2: baseline con el kit APAGADO (no estorba el arranque) ─────
for p in "${baseline_paths[@]}"; do git add -- "$p" || abortar "git add falló en $p"; done
if ! out_base="$(env -u GUARD_ACTIVE git commit -q -m "baseline: stubs protegidos + kit (GUARD_ACTIVE sin definir)" 2>&1)"; then
  abortar "el commit baseline falló con el kit apagado: $(linea_hook "$out_base")"
fi
git remote add origin "$guard_tmp/remote.git" || abortar "no se pudo añadir el remote local"

printf 'precondiciones: install-hooks temporal OK (3/3) · baseline con GUARD_ACTIVE sin definir OK\n'

# ── casos ────────────────────────────────────────────────────────────────────
# El 4º argumento es el marcador que el hook correspondiente DEBE imprimir;
# '' en los casos de pase (no hay hook que deba morder).
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

# H (R2, control negativo del token): CR-TEST-1 autorizado, mensaje que solo
# menciona CR-TEST-1999 (supercadena). Con comparación de subcadena este commit
# PASABA; con la conjunción por token debe RECHAZAR con el marcador del brazo
# CR-en-mensaje. Si pasa ⇒ la frontera de token está rota.
caso_commit H 'GUARD_ALLOW_CR=CR-TEST-1 con mensaje que SOLO trae CR-TEST-1999' 'FALLA' 'NO contiene' \
  'caso H: mensaje que solo trae CR-TEST-1999 (supercadena del CR autorizado)' docs/sintergic/handoffs/INDEX.jsonl \
  GUARD_ACTIVE=1 GUARD_ALLOW_CR=CR-TEST-1

# I (D3, CR-3862257b): el `_` NO delimita. CR-TEST-1 autorizado, mensaje con
# `identificador CR-TEST-1_suffix`: con la frontera anterior ([^A-Za-z0-9-]) el
# `_` contaba como delimitador y el commit PASABA, aunque el CR seguía dentro de
# otro identificador. Debe RECHAZAR con el marcador del brazo CR-en-mensaje.
caso_commit I 'GUARD_ALLOW_CR=CR-TEST-1 con mensaje con identificador CR-TEST-1_suffix' 'FALLA' 'NO contiene' \
  'caso I: mensaje con identificador CR-TEST-1_suffix (el _ no delimita)' docs/sintergic/handoffs/INDEX.jsonl \
  GUARD_ACTIVE=1 GUARD_ALLOW_CR=CR-TEST-1

# J (D4): el mensaje llega por `git commit -F <fichero>` y contiene el CR como
# token delimitado ⇒ PASA. Demuestra que commit-msg lee el fichero que git le
# pasa en $1 (el del commit EN CURSO), no una ruta fija.
printf 'caso J: excepcion CR-TEST-1 con mensaje via -F\n' > "$guard_tmp/msg-j.txt"
caso_commit_F J 'GUARD_ALLOW_CR=CR-TEST-1 con mensaje via git commit -F (CR delimitado)' 'PASA' '' \
  "$guard_tmp/msg-j.txt" docs/sintergic/handoffs/INDEX.jsonl \
  GUARD_ACTIVE=1 GUARD_ALLOW_CR=CR-TEST-1

# K (D4): mismo path protegido y misma excepción, mensaje SIN el CR ⇒ FALLA con
# el marcador del brazo CR-en-mensaje (el pase no es libre).
printf 'caso K: mensaje nuevo sin el CR tras la excepcion\n' > "$guard_tmp/msg-k.txt"
caso_commit_F K 'GUARD_ALLOW_CR=CR-TEST-1 con mensaje via -F SIN el CR' 'FALLA' 'NO contiene' \
  "$guard_tmp/msg-k.txt" docs/sintergic/handoffs/INDEX.jsonl \
  GUARD_ACTIVE=1 GUARD_ALLOW_CR=CR-TEST-1

# L (D4) — regresión exacta del defecto del mensaje HEREDADO, en dos pasos sobre
# el mismo repo temporal:
#   paso 1: commit con el CR en el mensaje ⇒ PASA y deja `.git/COMMIT_EDITMSG`
#           con ese CR. ASERCIÓN EXPLÍCITA: si el fichero no lo contiene, este
#           caso es VACUO y sale FALLA (nunca pasa por accidente).
#   paso 2: cambio nuevo al path protegido, mensaje SIN el CR ⇒ FALLA.
# Un hook que leyera `.git/COMMIT_EDITMSG` en vez de `$1` PASARÍA el paso 2: eso
# es lo que aquí se mide. El veredicto del caso es el del paso 2.
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
  # vacuo o roto: se registra FALLA con el marcador exigido NO encontrado ⇒ el
  # veredicto cae en «FALLA (motivo equivocado)» y la matriz rompe el exit 0.
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

# ── casos M..Q: auditor de cierre de lane (CR-51d25f1e) ──────────────────────
# El instrumento se autofalsifica con 5 casos; aquí cada uno es una fila. El rc
# del selftest es 0 cuando el caso se comportó como DEBE (los negativos salen con
# veredicto FAIL del instrumento), y el marcador exigido es el CÓDIGO del hallazgo
# que debe haber emitido: sin el código, la fila es «FALLA (motivo equivocado)».
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

# ── caso R (CR-9dd5c91e): el auditor de cierre es INSTRUMENTO, no fuente ─────
# Se edita el path protegido con el kit armado y mensaje SIN el CR: debe morder
# con el marcador de commit rechazado. Si el path no estuviera en la lista, git
# aceptaría el commit y la fila saldría FALLA (el instrumento quedaría editable).
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
  # caso vacuo: su precondición no se cumplió, así que no demuestra nada ->
  # FALLO aunque rc/marcador coincidan (nunca pasa por accidente)
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
