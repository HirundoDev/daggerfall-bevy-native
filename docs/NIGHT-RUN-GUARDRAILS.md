# Guardarraíles de corrida nocturna (hooks git)

CR: `CR-b367c9dd-db42-4465-9b2f-d8a341aa3135` · lane `NIGHT-GUARD-KIT` · target `daggerfall-bevy-native`.
Corrección de reservas del QA (R2 token · R1/D2b gates · R8 falsificador · R3 límite):
`CR-4b576323-4c49-4607-90e3-59c92cf5c2ea`.
Coherencia código↔prosa (D3: frontera `_` + censo fechado):
`CR-3862257b-fb7e-4eda-9178-e982e3a89ffc`.
Cierre de los dos huecos previos al encendido (D4: la excepción se decide en `commit-msg`
sobre el mensaje real + gates por PATRÓN):
`CR-a58f600a-6b7b-4b48-9c31-716625fce035`.

La prosa falló: se declaró cobertura que no existía y un gate que comparaba sus propios
literales. Este kit no pide confianza — es un hook de git que devuelve `exit != 0`.
Lo que no puede demostrarse con un comando que falla, aquí no está declarado.

Estado al entregar (D4, 2026-10-29): **instalado y APAGADO**. `GUARD_ACTIVE` sin definir ⇒
los 3 hooks salen 0 antes de leer nada (el kit apagado no estorba a ninguna lane). Se enciende
al cerrar M43 (ver §2).

---

## 1. Qué protege cada hook (tabla hook → path → efecto)

| Hook | Mira | Paths / condición | Efecto con `GUARD_ACTIVE=1` |
|---|---|---|---|
| `pre-commit` | el **índice** git (`git diff --cached`) | cualquiera que case `GUARD_PROTECTED_PATTERNS` o `GUARD_PROTECTED_DERIVED` | rechaza el commit (`exit 1`) e imprime path + patrón + remedio de **todos** los protegidos del índice. **NO evalúa la excepción**: con `GUARD_ALLOW_CR` definido no autoriza nada — imprime `GUARD-DEFERIDO: … pre-commit NO evalúa la excepción …` y sale 0, porque el mensaje del commit en curso todavía no existe |
| `commit-msg` | el **mensaje** del commit en curso: el fichero que git pasa en **`$1`** (nunca `.git/COMMIT_EDITMSG`) | mismo índice; exige que `GUARD_ALLOW_CR` aparezca como **token delimitado** en ese mensaje (ver §3) | **ÚNICO sitio donde se decide la excepción**. Rechaza si el CR esperado no está en el mensaje como token (`exit 1`, imprime `NO contiene`); concede con `GUARD-EXCEPCION` si está |
| `pre-push` | las refs que se publican (stdin) | todo push: ramas, tags y deletes | rechaza (`exit 1`) salvo `GUARD_ALLOW_PUSH` |

Paths protegidos (fuente única: `scripts/guard/guard-paths.sh`):

| Patrón | Origen | Qué es |
|---|---|---|
| `docs/sintergic/validation/closure-register*` | ORDER | registro de cierre |
| `docs/sintergic/handoffs/INDEX.jsonl` | ORDER | ledger de cierres |
| `crates/promotion-gate/**` | ORDER | el gate mismo |
| `crates/side-contract/**` | ORDER | contrato del lateral |
| `docs/sintergic/promotion-gate/promotion-ledger.jsonl` | ORDER | ledger de promociones |
| `daggerfall-side-projects/*` | ORDER | side-projects (ver D2: sin sujeto) |
| `scripts/*-00[0-9]-*.py` | ORDER | fuentes de gate (ver D2: 0 ficheros, re-medido 2026-10-29) |
| `scripts/*-00[0-9].py` | derivado D2 | los fuentes de gate reales: **23** en la foto de §1.1 (2026-10-29 11:44 MST), no un número fijo |
| `sides/*` | derivado D2 | el árbol real de side-projects |
| `scripts/*gate*.py` | derivado D2b/D4 | **gates de cierre, por PATRÓN** (no por nombre suelto): hoy casa 4 ficheros — ver §1.1 |
| `scripts/handoff-lint.py` | derivado D2b | lint mecánico del cierre HH-1 (no lleva `gate` en el nombre) |
| `scripts/mission-log.py` | derivado D2b | bitácora de misión (no lleva `gate` en el nombre) |
| `scripts/validate-lane-registry.py` | derivado D2b | validador del registro de lanes (no lleva `gate` en el nombre) |

Los patrones de directorio (`…/**` y `…/*`) cubren el propio directorio **y cualquier
profundidad**: `crates/promotion-gate/src/x.rs` es protegido. Los de fichero usan glob
(`*` cruza `/`, intencional). `./path` se normaliza antes de comparar.

Por qué estos y no otros: son el **instrumento** (gates, ledgers, contratos) y los
**registros de cierre**. Una corrida nocturna autónoma no puede editar lo que la mide.

### 1.1 Cobertura medida de `scripts/*.py` (D2b · CR-4b576323 · re-medido D4 CR-a58f600a)

**Esto es una foto con fecha, no una propiedad permanente del kit.** Medición:
**2026-10-29 12:01 MST**, sobre el árbol de trabajo tal como estaba (incluye ficheros aún
`untracked` de otras lanes: el hook los ve igual). El total es **41** `scripts/*.py`; el
patrón numérico `scripts/*-00[0-9].py` cubre **25**; el patrón de gates `scripts/*gate*.py`
cubre **4**; **3** nombres sueltos de D2b (no llevan `gate`) siguen declarados uno a uno; los
**9** restantes quedan fuera por decisión declarada, no por olvido.

| Dato | Comando de re-medición | Valor en esa foto (2026-10-29 12:01 MST) |
|---|---|---|
| total de fuentes | `printf '%s\n' scripts/*.py \| wc -l` | **41** |
| cubiertos por el patrón numérico | `printf '%s\n' scripts/*-00[0-9].py \| wc -l` | **25** |
| cubiertos por el patrón de gates | `ls scripts/*gate*.py` | **4** |
| fuera de ambos patrones | total − 25 − 4 | **12** |
| de esos, protegidos por nombre suelto (D2b) | 3 filas del array | **3** |
| fuera de la guarda | 12 − 3 | **9** |

Los 4 del patrón de gates están medidos, no supuestos:

| Protegido por `scripts/*gate*.py` | Por qué es instrumento de cierre |
|---|---|
| `scripts/closure-gate.py` | gate de cierre: decide si un cierre vale |
| `scripts/plan-gate-check.py` | chequeo del plan que el cierre cita |
| `scripts/m43-gate-contract.py` | contrato de los gates de M43 |
| `scripts/m43_gate_lib.py` | harness compartido de los gates M43 — el gap que la foto anterior declaraba abierto (untracked entonces y ahora: el patrón no depende de que esté trackeado) |

Los 3 que el patrón no alcanza porque **no llevan `gate` en el nombre**, declarados uno a uno:

| Protegido por nombre suelto (D2b) | Por qué es instrumento de cierre |
|---|---|
| `scripts/handoff-lint.py` | lint mecánico HH-1/INDEX (AB1-AB13): firma el cierre |
| `scripts/mission-log.py` | bitácora de misión: registro de cierre |
| `scripts/validate-lane-registry.py` | validador del registro de lanes |

Si esos números no coinciden con la foto, **la foto es vieja**: vuelve a correr los tres
comandos. El comentario de `scripts/guard/guard-paths.sh` conserva cifras de sus fechas
(«18 ficheros» de D2): son valores de entonces, no mediciones vigentes; este documento es el
que se re-mide.

### 1.1.1 Por qué el PATRÓN y no la lista de nombres (D4 · CR-a58f600a)

**Cualquier `*gate*.py` / `*_gate_lib.py` bajo `scripts/` entra por patrón, no por nombre
suelto.** La razón es mecánica: `guard_is_protected` compara el path del índice contra cada
patrón con un glob (`guard__match`), así que el patrón decide **sin que nadie lo edite**:

| Situación | Con lista de nombres | Con `scripts/*gate*.py` |
|---|---|---|
| gate ya conocido (`closure-gate.py`) | protegido si alguien escribió su línea | protegido |
| gate nuevo (`scripts/foo-gate.py`, `scripts/x_gate_lib.py`) | **fuera** hasta que alguien añada su línea — y mientras esté fuera, una corrida nocturna lo edita sin aviso | **protegido el día que existe** |
| gate `untracked` de otra lane en vuelo (`m43_gate_lib.py`) | fuera: exactamente el gap que este documento declaró medido y abierto | protegido: el glob no mira el índice de git, mira el path |

El caso medido que obligó a este cambio es `scripts/m43_gate_lib.py`: la foto anterior
(2026-10-29 11:44 MST) lo listaba en «fuera de la guarda» y proponía como remedio una línea
suelta `'scripts/m43_gate_lib.py'`. **Esa línea no se aplicó**: se sustituyó por el patrón,
que lo incluye y que además cubre al siguiente. Una lista de nombres protege solo lo que ya
se conocía el día que se escribió; el patrón protege lo que se llama igual mañana.

Las 2 filas sueltas que el patrón ya cubría (`scripts/closure-gate.py`,
`scripts/plan-gate-check.py`) se **retiraron** del array: cobertura idéntica, 0 duplicación
que mienta al censo (una fila que dice «nombre» cuando el patrón ya lo cubre invita a creer
que borrarla lo desprotege).

Límite honesto del patrón: depende del **nombre**. Un instrumento de cierre que no lleve
`gate` (`scripts/closure-check.py`, por ejemplo) sigue necesitando su línea — de ahí los 3
nombres sueltos de arriba.

| Fuera de ambos patrones y de D2b (medido, 9) | Rol real (docstring) — por qué NO está en la guarda |
|---|---|
| `scripts/principal-decision-audit.py` | auditor de decisiones del principal; no decide el cierre de una lane |
| `scripts/ledger-append.py` | **punto de escritura** de ledgers: protegerlo impediría appendear bitácora |
| `scripts/cost-ledger.py` | agregador de coste/tokens por lane (lectura) |
| `scripts/launch.py` | wrapper del launcher de release; editarlo es trabajo normal |
| `scripts/demo-035.py` | driver de campaña DEMO-035 (evidencia, no cierre) |
| `scripts/measure-m18-cs.py` | driver de evidencia CS-01..CS-21 de la campaña M18 |
| `scripts/npc-flats-capture.py` | driver de captura de ventana (evidencia visual) |
| `scripts/quest-82-census.py` | censo puntual de 82 acciones: informe, no gate |
| `scripts/verify-m18-owner-pack.py` | cross-check de **un** pack (M18): verificación puntual |

La lista es **retirable o ampliable línea a línea** de `GUARD_PROTECTED_DERIVED`
(`scripts/guard/guard-paths.sh`): borrar una línea saca ese path de la guarda sin tocar
nada más. Qué entra y qué no es **decisión del owner** — aquí solo está medido.

## 2. Cómo se enciende

```bash
export GUARD_ACTIVE=1
```

**Estado al entregar esta corrección (D4, CR-a58f600a, 2026-10-29): el kit sigue
APAGADO.** `GUARD_ACTIVE` no está exportado (`printenv GUARD_ACTIVE` ⇒ rc=1) y
`core.hooksPath` no está definido (`git config --get core.hooksPath` ⇒ rc=1). Los hooks
están instalados en `.git/hooks/`, pero su primera línea los apaga.

El encendido se hace **al cerrar M43**, no antes (decisión del capitán): mientras la lane
`CR-39d656e1` escribe sus gates, encender el kit convertiría cada escritura suya en un
rechazo que no mide nada. El comando es el mismo de siempre y no lo toma el kit por su
cuenta:

```bash
export GUARD_ACTIVE=1        # 0 export persistente: cada lane lo pone en su propio comando
```

Nada más que exportar: los hooks ya están instalados. Con la variable sin definir o con
cualquier valor distinto de `1`, los tres hooks salen **0** en su primera línea (sin leer
el índice, sin ejecutar git). Recomendado en la corrida nocturna: exportarlo en el script
que lanza al ejecutor, no en el shell del humano.

## 3. Cómo se desactiva por CR (escritura legítima)

La excepción es una **conjunción**, no una bandera — y se decide **en un solo sitio**:

1. `GUARD_ALLOW_CR=<cr>` definido ⇒ `pre-commit` **no evalúa nada**: imprime
   `GUARD-DEFERIDO: … pre-commit NO evalúa la excepción (el mensaje del commit en curso no
   existe todavía) …` y sale 0. Pre-commit corre **antes** de que git escriba el mensaje;
   si decidiera aquí, tendría que leer `.git/COMMIT_EDITMSG`, que en ese momento todavía
   contiene el mensaje del commit **anterior** — decidiría sobre un mensaje heredado.
2. El **mismo** CR debe aparecer en el mensaje del commit en curso **como token
   delimitado** ⇒ lo comprueba `commit-msg`, sobre el fichero que git le pasa en **`$1`**.
   Es el **único** sitio donde se concede la excepción; `commit-msg.sh` nunca abre
   `.git/COMMIT_EDITMSG` por su cuenta.

```bash
GUARD_ALLOW_CR=CR-b367c9dd-db42-4465-9b2f-d8a341aa3135 \
  git commit -m "docs(closure): registrar cierre CR-b367c9dd-db42-4465-9b2f-d8a341aa3135" -- docs/sintergic/handoffs/INDEX.jsonl
```

Falsificado en los dos sentidos (`scripts/guard/verify-guard.sh`): **J** prueba que el
mensaje llega por `git commit -F <fichero>` y el hook lo lee igual (`$1`, no una ruta
fija); **K** prueba que sin el CR en ese mensaje el commit cae; **L** reproduce el defecto
del mensaje heredado — dos pasos, el primero deja `.git/COMMIT_EDITMSG` con el CR, el
segundo trae un mensaje sin él y **debe** fallar.

### Token, no subcadena (R2 · CR-4b576323 · frontera `_` D3 CR-3862257b)

El CR es un identificador **delimitado**: a su izquierda y a su derecha debe haber
**cualquier carácter fuera de `[A-Za-z0-9_-]`** (inicio/fin de mensaje, espacio,
puntuación). El guion bajo cuenta como carácter de identificador, igual que las letras:
`CR-TEST-1_suffix` es **un** identificador, no el token `CR-TEST-1` seguido de un sufijo.
Se comprueba con la frontera `(^|[^A-Za-z0-9_-])<CR>([^A-Za-z0-9_-]|$)`, construida
escapando todo lo no alfanumérico del CR esperado (`commit-msg.sh`).

Esto no recorta ningún caso legítimo: los CR son del tipo `CR-<uuid>` y **no contienen
`_`**, así que toda forma real de citarlos (`CR-…`, `(CR-…)`, `CR-…, motivo`, `CR-…:`,
fin de línea) sigue pasando.

Antes la comparación era de subcadena, y eso convertía el permiso en un comodín: cualquier
mensaje que mencionara un CR **más largo que empezaba igual** autorizaba el commit.

| `GUARD_ALLOW_CR` | mensaje del commit | resultado |
|---|---|---|
| `CR-TEST` | `… ver CR-TEST-9999 …` | **RECHAZADO** — `CR-TEST` está dentro de otro identificador |
| `CR-TEST` | `… XCR-TEST …` o `… CR-TEST-9 …` | **RECHAZADO** — pegado por la izquierda o por la derecha |
| `CR-TEST` | `… CR-TEST_suffix …` o `… CR-TEST_suf …` | **RECHAZADO** — el `_` es carácter de identificador, no frontera |
| `CR-TEST` | `… ver CR-TEST …` / `(CR-TEST)` / `CR-TEST, motivo` / `CR-TEST:` / fin de línea | **PASA** — token delimitado |

Con (1) sin (2) el commit se rechaza: `GUARD_ALLOW_CR=… pero el mensaje NO contiene ese CR
como token delimitado`. Es el control negativo del falsificador (casos F, H e I) — el pase no
es libre, y el motivo queda escrito en el historial git para siempre.

Si no hace falta escribir el instrumento: `git restore --staged -- <path>` (remedio i).

## 4. Excepción de publicación

```bash
GUARD_ALLOW_PUSH="motivo explícito del owner" git push origin <rama>
```

`pre-push` imprime `GUARD_ALLOW_PUSH: <motivo>` y deja pasar. Sin la variable, todo push
—rama, tag o delete— se rechaza. Publicar no es un efecto secundario de una corrida
nocturna: es un acto firmado.

## 5. Instalación e idempotencia

```bash
bash scripts/guard/install-hooks.sh --dry-run   # plan, 0 escrituras
bash scripts/guard/install-hooks.sh             # instala los 3 wrappers
```

- Escribe `.git/hooks/{pre-commit,commit-msg,pre-push}`: wrappers **sin lógica** que
  resuelven `git rev-parse --show-toplevel` y fuentean `scripts/guard/<hook>.sh`.
- Bytes deterministas (sin timestamps ni rutas absolutas) ⇒ correrlo N veces deja los
  **mismos sha256**. Medido: 2 corridas consecutivas, 3/3 hashes idénticos.
- Hook ajeno (existe y **no** lleva el marcador `# GUARD-KIT:CR-b367c9dd`) ⇒ `exit 2`,
  sin backup y sin escribir nada: esta lane no toca hooks de otras.
- El kit se actualiza solo: el wrapper es estable y el cuerpo vive versionado en
  `scripts/guard/`.

## 6. Cómo se prueba

```bash
bash scripts/guard/verify-guard.sh    # exit 0 solo si las 12 filas coinciden
```

Crea un repo temporal (`mktemp -d`): un bare `remote.git` + un `repo` con stubs de todos
los paths protegidos, instala los hooks con el instalador real y ejecuta A..L. Imprime la
matriz y una línea por caso con el mensaje real del hook. Además comprueba: remote local
con **0 refs** (nada se publicó), repo temporal **limpio**, y huella del target (hooks +
`scripts/guard` + este doc + `core.hooksPath`) **idéntica** antes/después.

**No basta `rc != 0` (R8).** Cada caso de rechazo declara el **marcador** que debe imprimir
el hook que tiene que morder, y el veredicto lo exige en la salida real:

- `GUARD: commit RECHAZADO` → `pre-commit` / `commit-msg` (caso A)
- `NO contiene` → `commit-msg`, CR ausente o no delimitado (casos F, H, I, K y L)
- `GUARD: push RECHAZADO` → `pre-push` (casos D y G)

Si el comando sale `rc != 0` pero el marcador no aparece (typo, repo roto, dependencia
ausente), la fila sale **`FALLA (motivo equivocado)`** y el script termina `exit 1`. El
falsificador también se falsificó: con un marcador imposible en el caso A, la matriz da
`FALLA (motivo equivocado)` y exit 1 — medido.

**Bit `+x`.** Antes de los casos, el script mide el bit ejecutable de los 3 hooks: en el
temporal es **bloqueante** (si falta, aborta: git ignoraría el hook y la guarda
desaparecería en silencio); en el `.git/hooks/` del target real es **informativo** y, si
falta alguno, imprime `AVISO R3` con el remedio. Ver §7.6.

| caso | qué demuestra |
|---|---|
| A | commit a `INDEX.jsonl` con el kit encendido **falla** |
| B | misma escritura con `GUARD_ALLOW_CR` + CR delimitado en el mensaje **pasa** |
| C | commit a path libre **pasa** (no bloquea trabajo normal) |
| D | push de rama **falla** |
| E | kit apagado, commit a path protegido **pasa** (no estorba) |
| F | `GUARD_ALLOW_CR` con mensaje sin el CR **falla** (el pase no es libre) |
| G | push de tag **falla** |
| H | `GUARD_ALLOW_CR=CR-TEST-1` con un mensaje que solo trae `CR-TEST-1999` **falla** (token, no subcadena) |
| I | `GUARD_ALLOW_CR=CR-TEST-1` con un mensaje que trae el identificador `CR-TEST-1_suffix` **falla** (el `_` no delimita: sigue siendo otro identificador) |
| J | mensaje entregado con `git commit -F <fichero>` que sí contiene `CR-TEST-1` **pasa** — el hook lee el fichero que git le pasa en `$1`, no una ruta fija |
| K | mismo path protegido y misma excepción, mensaje nuevo **sin** el CR **falla** con `NO contiene` |
| L | **regresión del mensaje heredado**, en dos pasos: el paso 1 (con CR) deja `.git/COMMIT_EDITMSG` con ese CR y el paso 2 (mensaje sin CR) **falla**. Si el paso 1 no deja el CR en `COMMIT_EDITMSG`, el caso se declara **vacuo** y sale FALLA — nunca pasa por accidente. Un hook que leyera el mensaje heredado PASARÍA el paso 2: medido con un mutante de `commit-msg` que lee `git log -1 --format=%B` (paso 2 `rc=0`, es decir, el defecto reproducido), mientras el hook real lo rechaza |

Total medido: **12/12** (D4, CR-a58f600a, 2026-10-29). Antes de J/K/L eran 9/9 (D3,
CR-3862257b: el `_` contaba como frontera en el código y la prosa lo llamaba «nunca dentro
de otro identificador» — dos afirmaciones que no podían ser verdad a la vez; el caso I
impide que vuelvan a separarse). J/K/L cierran el otro par que podía separarse: la prosa
decía que la excepción se decidía «sobre el mensaje real» y el hook podía estar leyendo el
heredado sin que nada lo delatara.

## 7. Límites declarados (lo que este kit NO hace)

1. **No valida contenido ni veracidad.** Solo propiedad (qué ficheros) y publicación (si
   sale del repo). Una cifra falsa en un documento no la caza el hook: la caza el gate.
   Si un cierre miente sobre cobertura, el hook lo deja pasar — y eso está escrito aquí
   para que nadie lo confunda con una garantía.
2. **`--no-verify` salta los hooks.** Es un límite real de git, no se oculta. No hay
   defensa técnica dentro del repo contra quien puede ejecutar git con esa bandera; sí hay
   rastro: el commit queda sin el CR en el mensaje y sin la línea `GUARD-EXCEPCION` (la que
   solo concede `commit-msg`) — con `--no-verify` `pre-commit` tampoco imprime su
   `GUARD-DEFERIDO`, así que la ausencia de **ambas** marcas es la señal.
3. **La excepción se decide SOLO en `commit-msg`, sobre el mensaje REAL; `pre-commit` no
   autoriza nada** (D1 + D4). `pre-commit` corre antes de que git escriba el mensaje, así
   que no puede decidir: con `GUARD_ALLOW_CR` definido imprime `GUARD-DEFERIDO` y sale 0.
   El límite residual D1: **si alguien borra `.git/hooks/commit-msg`, `GUARD_ALLOW_CR`
   queda como pase libre** (el otro brazo de la conjunción desaparece y `pre-commit` ya no
   autorizaba nada que perder). Cómo se detecta, sin daemon:
   - `bash scripts/guard/install-hooks.sh --dry-run` ⇒ dice `crea` para `commit-msg` (con
     el hook del kit puesto diría `sobreescribe wrapper del kit`);
   - `ls -l .git/hooks/commit-msg` ⇒ ausente (y `stat -c '%A %n'` ⇒ sin `+x` si perdió el bit);
   - `bash scripts/guard/verify-guard.sh` ⇒ mide los 3 hooks ejecutables en su temporal y
     reporta `AVISO R3` si en el `.git/hooks/` del target falta alguno;
   - el commit autorizado por CR en ese estado queda **sin** la línea `GUARD-EXCEPCION`, que
     es la firma de que `commit-msg` concedió: su ausencia en un commit con CR en el mensaje
     es la evidencia de que el hook no corrió.
   Mitigación: `install-hooks.sh` reinstala los 3 (idempotente) — pero no vigila el
   `.git/hooks/` real en cada corrida.
4. **Dos literales del ORDER no tienen sujeto real en este árbol.** Se midió antes de
   escribir el kit; los derivados cubren la intención. Retirar la parte derivada es
   limpiar una línea por patrón del array `GUARD_PROTECTED_DERIVED` (§1.1 mide también los
   gates de cierre, que desde D4 entran **por patrón** —`scripts/*gate*.py`, 4 ficheros
   hoy— más 3 nombres sueltos que no llevan `gate`):

| literal del ORDER | medición | derivado declarado | por qué |
|---|---|---|---|
| `daggerfall-side-projects/*` | directorio **ausente** | `sides/*` | el árbol real de side-projects es `sides/` (p.ej. `sides/i1-minimal-side/`) |
| `scripts/*-00[0-9]-*.py` | **0** ficheros | `scripts/*-00[0-9].py` | los fuentes de gate reales son `scripts/<familia>-00[0-9].py` → **25** en la foto de §1.1 (2026-10-29 12:01 MST); el patrón es estable, la cuenta no |

   Consecuencia declarada: mientras existan los derivados, protección ligeramente más
   ancha que el ORDER (todo `sides/*` y todo `scripts/*-00[0-9].py`, no solo "fuentes de
   gate"). Preferible a un patrón que no protege nada.

   **Límite del límite por patrón (D4):** `scripts/*gate*.py` cubre por **nombre**, no por
   rol. Queda **dentro** todo instrumento cuyo fichero contenga `gate`
   (`*gate*.py`: `closure-gate.py`, `plan-gate-check.py`, `m43-gate-contract.py`,
   `m43_gate_lib.py`) y queda **fuera** cualquier instrumento de cierre que no lo lleve
   (`handoff-lint.py`, `mission-log.py`, `validate-lane-registry.py`: declarados uno a uno
   por eso). Un gate futuro con nombre sin `gate` (`scripts/closure-check.py`) seguiría
   necesitando su línea: el patrón cierra el hueco del «tercer gate con nombre de gate»,
   no el de cualquier instrumento con cualquier nombre.

5. **El instalador escribe en `.git/hooks/`, no en el repo versionado.** Un `git clone`
   nuevo no trae el kit activo: hay que correr `install-hooks.sh` (una línea).

6. **Modo de fallo silencioso: la guarda puede desaparecer sin que nada avise (R3).** Git
   solo ejecuta hooks que son ficheros **ejecutables** en el directorio de hooks. Si se
   pierde el bit `+x`, si el fichero no está, o si `core.hooksPath` apunta a otro sitio,
   git **ignora los hooks y no imprime nada**: los commits salen 0 y parece que el kit no
   tenía nada que decir. Cuatro formas del mismo agujero:

   | Modo | Qué se ve en el commit | Cómo lo delata el operador |
   |---|---|---|
   | bit `+x` perdido (chmod, copia, extracción rara) | nada; el hook no corre | `stat -c '%A %n' .git/hooks/pre-commit .git/hooks/commit-msg .git/hooks/pre-push` → alguna terna sin `x` (esperado `-rwxr-xr-x`) |
   | hook ausente, o instalación nunca corrida (clon nuevo) | nada | `bash scripts/guard/install-hooks.sh --dry-run` → dice `crea`; con el hook del kit ya puesto diría `sobreescribe wrapper del kit` |
   | `core.hooksPath` apuntando a otro directorio | nada; git usa ese otro | `git config --get core.hooksPath` → rc=0 con valor (esperado **rc=1**: sin definir) |
   | trabajo en otro clon/worktree sin el kit | nada | los dos comandos de arriba, en ese clon |

   **Este kit NO avisa solo.** No hay daemon, ni chequeo de arranque, ni latido: la única
   red es `verify-guard.sh`, que mide el bit `+x` de los tres hooks (bloqueante en su repo
   temporal; en el `.git/hooks/` del target imprime `AVISO R3` con el remedio) — y hay que
   **correrlo**. Si nadie lo corre, la guarda puede estar ausente y el árbol no lo dice.
   Reinstalar es una línea (`bash scripts/guard/install-hooks.sh`, idempotente) y el
   `--dry-run` no escribe nada. Esta declaración es el límite: no se afirma vigilancia que
   no existe.

## 8. Capa de detección (manifiesto + reflog) — CR-01e3aebf

Los hooks de §1–§6 son **fricción**: si están encendidos, rechazan el commit que toca un
instrumento. Esta sección es **detección**: no interviene en el commit, y sirve exactamente en
los casos en que la fricción no ocurrió — kit apagado (§2), `--no-verify` (§7.2), hook ausente
o sin bit `+x` o `core.hooksPath` desviado (§7.6), o el propio instrumento editado por otra vía
(`chmod +w`, edición directa, otro clon). Nada de esto **impide** escribir un instrumento:
**detecta, no impide**. Lo que garantiza es que el rastro queda y se puede nombrar después.

### 8.1 Los tres scripts y su interfaz literal

| Script | Interfaz | Exit codes | Qué hace |
|---|---|---|---|
| `scripts/guard/manifest-protected.sh` | `--write <fichero>` · `--check <fichero>` · `--help` | `--write`: 0 escrito / 2 error · `--check`: 0 todo `OK` / 1 hay `AUSENTE`·`CAMBIADO`·`NUEVO` / 2 uso o manifiesto ilegible-inexistente-vacío | Toma/verifica la huella **sha256 de todos los paths protegidos** |
| `scripts/guard/reflog-audit.sh` | `[--limit N] [--since <fecha>] [--ref <ref>]` | 0 = 0 toques medidos / 1 = hay toques / 2 = uso o git **o `SIN EVIDENCIA`** (0 entradas auditables) | Lista los commits del **reflog** que tocan algún path protegido |
| `scripts/guard/verify-manifest.sh` | (sin argumentos) | 0 = matriz 5/5 `OK` / 1 = alguna casilla o precondición falla / 2 = montaje imposible | Falsificador: prueba las 5 casillas (a)–(e) en un repo temporal |

Ninguno de los tres contiene patrones de paths, pero **no los reparten igual** (medido):

- `manifest-protected.sh` y `reflog-audit.sh` **sourcean** `scripts/guard/guard-paths.sh`
  (hermano, resuelto por `BASH_SOURCE`) y la propiedad de un path la decide
  `guard_is_protected` ⇒ **cero duplicación de patrones**: cambiar la lista protegida se hace
  en un solo sitio, como en los hooks.
- `verify-manifest.sh` **NO sourcea nada** (0 `source`): copia el kit a un repo temporal
  (`mktemp -d`) y **siembra dos paths protegidos como stubs** (`scripts/closure-gate.py`,
  `docs/sintergic/handoffs/INDEX.jsonl`) para que la matriz tenga sujeto. **No es fuente de
  verdad de patrones y no debe citarse como tal**: si se amplía la lista protegida, este
  script no cambia — sus stubs siguen siendo dos.

`manifest-protected.sh --write <fichero>` escribe `sha256  <path-rel>` (dos espacios), una línea
por path, ordenado con `LC_ALL=C sort` sobre la **lista de paths** (no sobre la línea: así los
paths con espacios no descolocan el orden). Sin timestamps ni rutas absolutas ⇒ **dos corridas
seguidas dejan bytes idénticos** (medido en este árbol el 2026-10-29: 475 paths protegidos,
sha256 del manifiesto idéntico en las dos corridas). El universo de paths es `git ls-files -z`
más `git ls-files -z --others --exclude-standard`: el segundo término es lo que hace literal la
definición de `NUEVO` («protegido **en disco**»), y cubre el instrumento `untracked` de otra
lane (el mismo caso que §1.1.1 midió con `m43_gate_lib.py`).

`manifest-protected.sh --check <fichero>` imprime **una línea por path**, sin adorno y por
stdout, con exactamente uno de los cuatro estados de §8.2, y una última línea
`RESUMEN OK=<n> AUSENTE=<n> CAMBIADO=<n> NUEVO=<n> TOTAL=<n>`. El recorrido es la **unión**
manifiesto ∪ protegidos-en-disco, ordenada `LC_ALL=C`: un path del manifiesto que ya no case
ningún patrón se sigue comparando (el manifiesto manda para su ciclo), y un path protegido que
esté en disco y no en el manifiesto sale `NUEVO` aunque nunca se haya commiteado.

`reflog-audit.sh` recorre las entradas **del reflog** del ref (default `HEAD`, más reciente
primero) y, por cada commit distinto, imprime:

```
<sha-corto> | <fecha ISO> | <autor %ae> | <path1>,<path2>
```

`--limit N` (default **50**) acota a las últimas N entradas del reflog: es cobertura
**PARCIAL**. Una ventana corta **no** equivale a «no hubo toques», y el cierre debe
**declarar la ventana usada** — el encabezado la imprime siempre, como `ventana medida:
últimas N entradas de <ref>` con `--limit` y `ventana medida: desde <fecha>` con `--since`.
`--since <fecha>` acepta lo que acepte `git log --since` (`2026-10-29`, `2 days ago`, …) y
**manda sobre `--limit`**: se recorren todas las entradas del reflog dentro de la ventana. La
segunda línea del encabezado dice lo que de verdad se audita: `entradas medidas: <n> · commits
distintos auditados: <m>` (con `--since`, n = entradas del reflog que caen dentro de la
ventana; con `--limit`, n = entradas leídas). Un pie cierra con
`commits infractores: N · paths protegidos diferentes: M`; con 0 infractores **y
`entradas medidas > 0`** imprime literalmente **`0 toques a instrumentos`** y sale 0. Si
`entradas medidas = 0` (reflog vacío, o un `--since` que no cae sobre ninguna entrada) imprime
por **stderr** `SIN EVIDENCIA: el reflog de <ref> no tiene entradas auditables (¿git reflog
expire / git gc --prune?) — NO se puede atestiguar «0 toques»` y sale **2**, nunca 0: la
ausencia de evidencia no es una absolución. No admite argumentos posicionales (un sha o un
path suelto es error de uso, exit 2).

Paths de un commit — decisión declarada, commits de merge incluidos: se resuelve el **primer
padre** con `git rev-list --parents -n1` y se compara contra él con
`git diff-tree -r --no-renames <padre> <commit>` (equivalente a
`--diff-merges=first-parent` / `git show --first-parent`, aplicado a mano para no depender de la
versión de git ni del modo por defecto de `git show`); un commit raíz se mide con
`git diff-tree -r --no-renames --root`. `--no-renames` hace que un rename aparezca como borrado
+ creación: renombrar un instrumento también se ve. Medido en un repo temporal con un merge cuyo
lado sí tocaba el ledger: la auditoría nombra **el merge**
(`b1d5a3f | … | docs/sintergic/handoffs/INDEX.jsonl`).

**Detecta `--no-verify` por diseño.** Un commit con `--no-verify` es un commit normal: lo único
que no ocurrió es el hook. La evidencia de esta capa es la **historia**, no el hook, así que el
commit aparece en el reflog como cualquier otro y `reflog-audit.sh` lo nombra. Medido en la
casilla (c) de `verify-manifest.sh`: con un hook `pre-commit` marcador que deja marca y sale 0,
el commit con `--no-verify` sale **sin marca** (el hook no corrió) y la auditoría lo nombra
igual, con su sha y su path.

`verify-manifest.sh` es la prueba falsable de la capa. Monta un repo temporal (`mktemp -d`),
copia `guard-paths.sh`, `manifest-protected.sh` y `reflog-audit.sh`, siembra dos paths protegidos
(`scripts/closure-gate.py`, `docs/sintergic/handoffs/INDEX.jsonl`) más uno libre (`src/foo.rs`),
instala el hook marcador y ejecuta la matriz:

| Casilla | Escenario | Esperado |
|---|---|---|
| (a) | manifiesto recién escrito ⇒ `--check` | rc 0 · todas las líneas `OK` · `AUSENTE=0 CAMBIADO=0 NUEVO=0` |
| (b) | editar `scripts/closure-gate.py` + commit ⇒ `--check` | rc 1 · imprime `CAMBIADO scripts/closure-gate.py` |
| (c) | `git commit --no-verify` tocando `INDEX.jsonl` ⇒ `reflog-audit.sh` | rc 1 · nombra el path y el sha · **el hook no dejó marca** |
| (d) | revert declarado (restaurar **bytes** y recommitear) ⇒ `--check` + `reflog-audit.sh` | `--check` rc 0 · `reflog-audit` rc 1 y sigue viendo los commits infractores (incluido el sha de (c)) |
| (e) | commit infractor y después `git reflog expire --expire=now --all && git gc --prune=now` ⇒ `reflog-audit.sh` | rc **2** · imprime `SIN EVIDENCIA` · **nunca** rc 0 con «0 toques» |

Exit 0 **solo** con las 5 casillas en lo esperado y 3 precondiciones medidas: `--write`
determinista (dos corridas, `cmp` idéntico), repositorio de partida **sin toques**
(`reflog-audit` rc 0 con `0 toques a instrumentos`) y **control positivo** del hook marcador
(un commit normal sí deja marca; sin ese control, la ausencia de marca en (c) no demostraría
nada). Diferencia deliberada con la casilla (b) de `verify-guard.sh`: aquí (b) commitea de
verdad, porque lo que se mide es la huella en disco, no el rechazo del hook. Decisión de (d):
se usa **restauración de bytes + commit** en lugar de `git revert` porque el stub nace
`untracked`, y un `git revert` del commit que lo añadió lo **borraría** (estado `AUSENTE`, no
`OK`); con la restauración el estado vuelve a `OK` y los commits infractores siguen en la
historia, que es justo lo que (d) debe demostrar. La casilla (e) va **última a propósito**
(destruye el reflog que (c) y (d) necesitan) y es la que hace falsable el falso negativo en
verde: un `reflog-audit.sh` que devolviera 0 sin evidencia pondría (e) en `FALLA` y el
falsificador en exit 1.

### 8.2 Los cuatro estados del manifiesto

| Estado | Significado | Qué implica |
|---|---|---|
| `OK <path>` | en el manifiesto **y** en disco, sha256 idéntico | el instrumento está byte a byte como al abrir el ciclo |
| `AUSENTE <path>` | en el manifiesto, **ya no en disco** | alguien lo borró o lo movió (incluye paths que además salieron de `git ls-files`) |
| `CAMBIADO <path>` | en ambos, **sha distinto** | alguien lo editó — el caso que esta capa nombra |
| `NUEVO <path>` | protegido **en disco** y ausente del manifiesto | instrumento que apareció después de sembrar: o es legítimo y se re-siembra, o nadie lo declaró |

Los cuatro estados son de **bytes en disco**, no de propiedad: un path del manifiesto que ya no
case ningún patrón se sigue comparando (el manifiesto manda para su ciclo), y la lista de
propiedad la sigue decidiendo `guard-paths.sh` para lo que entra como `NUEVO`.

### 8.3 El límite real: **detecta, no impide**

Sin adornos: el mismo uid que corre la corrida nocturna puede `chmod +w` los instrumentos,
editar `guard-paths.sh` para quitar un patrón, borrar `.git/hooks/*`, poner
`core.hooksPath` en otro sitio y `git commit --no-verify` para saltarse los hooks. **Nada de
eso se impide técnicamente** desde dentro del repo — los hooks (§7.2, §7.6) y esta capa
comparten ese techo, porque no hay defensa posible contra quien ya tiene escritura sobre el
árbol y el `.git`.

Lo que sí se garantiza, y su tamaño exacto:

- los **bytes** de los instrumentos están anclados: `manifest-protected.sh --check` nombra el
  path concreto que cambió (`CAMBIADO`), el que falta (`AUSENTE`) y el que apareció sin
  declarar (`NUEVO`);
- la **historia** está anclada, con su tamaño exacto: `reflog-audit.sh` nombra el commit
  exacto que tocó un instrumento — con su sha, su fecha, su autor y los paths — **aunque el
  hook se haya saltado** y aunque el commit ya no esté en ninguna rama: un `reset --hard`
  conserva el sha abandonado como entrada de su propio reflog, así que **se sigue detectando**.
  Lo que **no** sobrevive es la expiración del reflog seguida de `git gc --prune=now`: eso
  borra las entradas y con ellas la evidencia.

El techo del reflog, sin adornos: **`git reflog expire --expire=now --all` +
`git gc --prune=now` destruye la evidencia y el instrumento ya no puede atestiguar nada**. En
ese estado `reflog-audit.sh` sale **2 `SIN EVIDENCIA`** y no `0 toques` — ausencia de evidencia
≠ absolución: el hueco deja de ser silencioso, pero **destruir sigue siendo posible**, y
destruir el reflog **es en sí mismo** el acto que un cierre debe mirar. Detecta, no impide,
también aquí.

Eso es todo, y es suficiente para un cierre honesto: no hay prevención, hay **constancia**. Si
un cierre dice «no se tocó ningún instrumento», esta capa convierte esa frase en algo que se
puede comprobar en dos comandos — o desmentir nombrando el path y el commit.

### 8.4 Cómo se usa en un cierre

```bash
# 1. SEMBRAR (primera vez en este repo, y tras cada cambio legítimo del kit)
bash scripts/guard/manifest-protected.sh --write docs/sintergic/guard/protected.sha256

# 2. AL CERRAR: los bytes
bash scripts/guard/manifest-protected.sh --check docs/sintergic/guard/protected.sha256
#    exigido: RC=0 y `RESUMEN ... AUSENTE=0 CAMBIADO=0 NUEVO=0`

# 3. AL CERRAR: la historia del ciclo
bash scripts/guard/reflog-audit.sh --since "<inicio del ciclo>"
#    exigido: RC=0, `entradas medidas` > 0 y la línea `0 toques a instrumentos`
#    RC=2 `SIN EVIDENCIA` NO es un cierre válido: es un reflog sin evidencia
#    (¿expire/gc?) o una ventana vacía. El cierre declara la ventana que usó.
```

Notas de uso, sin letra pequeña:

- **El manifiesto hay que sembrarlo**: no viaja en el repo y ningún hook lo crea. La **ruta la
  elige quien cierra el ciclo** (recomendada `docs/sintergic/guard/protected.sha256`, que es
  relativa al directorio del invocante). Si se quiere que viaje, se commitea — decisión del
  cierre, no de estos scripts.
- **El propio fichero del manifiesto NO es un instrumento protegido**: no casa ningún patrón de
  `guard-paths.sh`, así que `--write` puede reescribirlo libremente (si lo fuera, cada
  re-siembra aparecería como `CAMBIADO` de sí misma). Añadirlo a la lista es **decisión del
  owner**, no un efecto de este documento; hoy no está.
- **Re-sembrar tras un cambio legítimo del kit** (`guard-paths.sh`, un gate, un ledger): se
  corre `--write`, y el motivo queda en el mensaje del commit que hizo el cambio. Re-sembrar
  **sin** cambio legítimo convierte la capa en decoración: borra la evidencia que se busca.
- **`--write` se niega con rc 2 si un protegido *tracked* falta del disco.** No hay drop
  silencioso: el path sigue en el índice de git, `sha256sum` no puede leerlo y el script
  imprime `no se pudo hashear '<path>' (¿permisos?): manifiesto NO escrito, el anterior queda
  intacto` — el manifiesto previo **no se toca**, así que un borrado no se blanquea
  re-sembrando. Qué hacer en ese caso: **commitear el borrado** (si es legítimo) o
  **revertirlo** (`git checkout -- <path>`) **antes** de re-sembrar con `--write`.
- **Append legítimo al ledger protegido (no hay allowlist, por diseño).** `INDEX.jsonl` es
  instrumento protegido y el protocolo HH-1 exige **un append por lane** ⇒ `reflog-audit.sh`
  dará **rc=1 de forma legítima**. El cierre debe **declarar** esos appends (sha + lane) como
  toques autorizados: el instrumento no distingue un append de una edición destructiva, y por
  eso la declaración es obligatoria, no opcional. Receta corta: correr
  `bash scripts/guard/reflog-audit.sh --since "<inicio>"` y, por cada línea que nombre
  `docs/sintergic/handoffs/INDEX.jsonl`, una fila en la tabla «toques autorizados» del cierre
  con `<sha-corto> · <lane> · append HH-1`. Un append sin declarar es hallazgo, no ruido.
- Los tres comandos son **deterministas y sin estado oculto**: no escriben en el repo (el
  manifiesto se escribe donde se le pida), no tocan `.git/hooks/`, no instalan nada.

### 8.5 No dependen de `GUARD_ACTIVE`

`manifest-protected.sh`, `reflog-audit.sh` y `verify-manifest.sh` **no leen `GUARD_ACTIVE`** en
ninguna decisión. Funcionan con el kit **apagado** (estado de §2: `GUARD_ACTIVE` sin definir),
que es justo el caso de uso: si la detección dependiera de que los hooks estén activos, no
detectaría nada en el escenario que la motiva. Medido: los tres corren con
`env -u GUARD_ACTIVE` y con la variable sin definir en el entorno (`printenv GUARD_ACTIVE`
⇒ rc 1). `verify-manifest.sh` además monta su repo temporal **sin** exportar la variable en
ningún momento.

### 8.6 Coherencia con §7.2 (el límite de `--no-verify`)

§7.2 declara que `--no-verify` salta los hooks y que el rastro es «el commit sin el CR en el
mensaje y sin la línea `GUARD-EXCEPCION`». Ese razonamiento tenía una dependencia incómoda: la
evidencia se infería de la **ausencia** de una marca escrita por un hook — si el hook no
corrió, la ausencia se explica sola, pero también se explicaría por otras causas, y en un
commit sin CR la ausencia es lo normal.

Ahora son **dos evidencias independientes**:

1. la **ausencia de `GUARD-EXCEPCION`** en un commit con el CR en el mensaje ⇒ `commit-msg` no
   concedió (o no corrió): sigue siendo la firma del hook, dependiente de que los hooks existan;
2. `reflog-audit.sh` **nombra el commit** que tocó el instrumento, con sha, fecha, autor y
   paths: no depende de que ningún hook haya corrido, ni de que la marca esté o falte, ni de
   que el commit siga en una rama.

La segunda no sustituye a la primera: la primera dice **quién autorizó** (y con qué CR), la
segunda dice **qué pasó de verdad**. Un cierre que exija las dos —`reflog-audit.sh --since
<inicio>` en 0 toques, y `GUARD-EXCEPCION` presente solo donde hubo excepción declarada— ya no
puede ser víctima del agujero de §7.6 (hook ausente, sin `+x`, `core.hooksPath` desviado): eso
apaga la fricción, no la constancia.

### 8.7 Aviso: la reconciliación de `AGENTS.md` es del bloque, no del fichero

La reconciliación con la fuente única cubre **solo el bloque `CONTRATO-EJECUTOR`** (1228 B,
sha256 `2606ce56…4d0f`): el próximo `sync --apply` real reordenará el resto del fichero
(`## Prohibiciones` pasa detrás del bloque) **sin tocar el bloque** — el resto de `AGENTS.md`
no está certificado por esta capa, solo el bloque.
