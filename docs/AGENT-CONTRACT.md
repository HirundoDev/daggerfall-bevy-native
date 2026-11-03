# Contrato del ejecutor

> **Propósito:** definir qué puede y qué no puede hacer un **ejecutor** (modelo barato que produce)
> frente a un **auditor** (el capitán, que certifica). El contrato rige cada corrida sin renegociarse
> y no depende de la memoria del modelo.
>
> **Portabilidad:** §0-§8 son normativos y genéricos — no nombran ningún repo ni ruta absoluta.
> Copiar este archivo a otro project-target sin cambios: la parte normativa es agnóstica. El **anexo**
> final es un caso concreto, marcado como no normativo.
>
> **Stub autocargado:** el bloque `CONTRATO-EJECUTOR` de `AGENTS.md` resume los mínimos y apunta aquí.
> Este documento manda; el bloque es su forma telegráfica.

## §0 — Roles y autoridad

| Rol | Decide | Produce | Certifica | Frontera dura |
|---|---|---|---|---|
| **Owner** (humano) | GO irreducible: alcance, canon, gasto, publicación | — | — | Nunca se auto-delega; el resto escala aquí |
| **Capitán** (principal del target) | Decide con fuente citada (`archivo:línea`, fila de ledger, veredicto o GO previo) | — | **SÍ — es el único auditor** | Sin fuente citada no decide: abre decisión pendiente |
| **Orquestador** | Routing y descomposición: qué lane, con qué pack, con qué presupuesto | Plan de lanes | No certifica el producto | No produce dominio ni audita su propia lane |
| **Ejecutor** (modelo barato) | Nada de alcance: ejecuta el encargo cerrado | Código, docs, mediciones | **NUNCA** | Su estado máximo es `candidato` |
| **Auditor** (= capitán, o lane con mandato explícito de auditoría) | Acepta / re-delega / escala | Veredicto | **SÍ** | No hereda afirmaciones: re-deriva |

**Regla de autoridad:** el capitán es el **único auditor**. El ejecutor nunca audita ni se audita —
ni cuando "solo comprueba que quedó bien". Un ejecutor que emite veredicto sobre su propio trabajo
ha salido del contrato, aunque el veredicto sea correcto.

## §1 — Los 8 mínimos no negociables

Cada cláusula es literal y vinculante. La línea de consecuencia dice qué implica en la práctica.

- **(a)** El gate/instrumento se escribe ANTES, por otra lane, y el ejecutor no puede editarlo.
  - Consecuencia: si el instrumento no existe al arrancar, el ejecutor para y lo reporta; no lo inventa.
- **(b)** Entregable = commits por pathspec individual + salida cruda de comandos; la palabra DONE/PASS no la pronuncia el ejecutor nunca — su estado máximo es `candidato`.
  - Consecuencia: el reporte termina en `candidato` y el veredicto lo escribe otro.
- **(c)** Si una cifra no se sostiene, se retira; jamás se ajusta el instrumento a la cifra.
  - Consecuencia: ante una métrica que no cuadra, se retira la cifra y se declara el hueco; no se relaja el umbral.
- **(d)** 0 mock / 0 TODO / 0 placeholder / 0 unsafe / 0 unwrap en sondas.
  - Consecuencia: una sonda que aborta por `unwrap`/`expect` produce fila roja, no un exit opaco.
- **(e)** Prohibido escribirse sondas de auto-verificación, editar el registro de cierre, el índice de cierres ni ledger alguno.
  - Consecuencia: los ledgers de cierre son de solo-append y su fila la escribe quien cierra, no quien produce.
- **(f)** 0 push / 0 tag.
  - Consecuencia: el trabajo vive en commits locales; publicar es decisión del capitán/owner.
- **(g)** Checkpoint de contexto: a 70% emite handoff y cierra.
  - Consecuencia: mejor un cierre parcial honesto que un cierre completo inventado por agotamiento.
- **(h)** Cada afirmación debe llevar `file:line` o comando re-ejecutable, y el auditor lo re-deriva con build frío — no hereda.
  - Consecuencia: una afirmación sin ancla no es evidencia; es prosa y se descarta.

## §2 — Anatomía del entregable

1. **Commits por pathspec individual.** Un `git add <ruta>` por commit. Nunca `git add -A` ni `git add .`:
   el árbol compartido trae cambios ajenos y el commit global los secuestra.
2. **Salida cruda.** Cada comando que sostiene una afirmación se pega con su salida observada
   (`comando -> salida`). El resumen interpretado va aparte; nunca sustituye a la salida.
3. **Estado declarado `candidato`.** El entregable nunca se autodeclara terminado. Se declara
   *qué se hizo*, *qué se verificó ejecutándolo* y *qué quedó abierto*.
4. **Formato de reporte al auditor** (mínimo):
   - alcance ejecutado vs. alcance pedido (una línea por divergencia);
   - lista de commits (sha + pathspec);
   - comandos ejecutados con su salida;
   - afirmaciones con `file:line` o comando re-ejecutable;
   - cifras retiradas (y por qué no se sostenían);
   - huecos abiertos, con el bloqueo exacto.
5. **Frontera de árbol.** No se tocan ficheros de otras lanes en vuelo; no se revierten cambios ajenos.

## §3 — Verificación del auditor

El auditor **re-deriva**, no lee y acepta:

1. **Build frío.** Reconstruye desde cero (sin caché ni artefactos previos). Un verde en caliente no
   cuenta como verde.
2. **Re-derivación de cada cifra.** Toda métrica del reporte se recomputa con su propio comando.
   Cifra no reproducible ⇒ se retira del reporte y se abre hallazgo.
3. **`file:line` obligatorio.** Una afirmación sin ancla verificable no se audita: se devuelve.
4. **No herencia.** El auditor no hereda afirmaciones del ejecutor ni de auditorías previas; hereda
   únicamente el instrumento y su salida.
5. **Separación de gates y veredictos.** Un gate mecánico (lint, linter de cierres, validador) produce
   PASS/FAIL; el veredicto semántico (¿esto cumple el encargo?) es del auditor y va en fila aparte.
6. **Auditoría del cierre (§9) obligatoria.** Antes de aceptar, el auditor corre el auditor
   mecánico de cierre sobre el CR (o el rango) y adjunta su matriz. Un FAIL de §9 no se
   compensa con prosa: el cierre se rechaza o se re-delega.

## §4 — Checkpoint de contexto al 70%

Al alcanzar ~70% del presupuesto de contexto, el ejecutor:

- **emite handoff** con: hecho / verificado / abierto / para quién, más las rutas tocadas y los
  comandos ejecutados;
- **cierra** la lane en estado `candidato` (o `PARTIAL` si el encargo no se completó);
- **NO declara** cierre del ciclo, ni aceptación, ni PASS: eso es del auditor;
- **NO** intenta "terminar rápido" recortando verificación: recorta alcance y lo declara.

## §5 — Buen encargo / mal encargo

| Buen encargo (delegable de noche) | Mal encargo (NUNCA a un ejecutor) |
|---|---|
| Código contra un contrato ya escrito | Escribir su propia verificación |
| Refactor mecánico con criterio explícito | Decidir cobertura |
| Implementar una spec cerrada | Declarar cierre |
| Reparar hallazgos con reporte previo | Auditarse a sí mismo |
| Rellenar una tabla cuyo esquema ya existe | Arbitrar estado (quién tiene razón) |

Criterio de corte: si el encargo exige **decidir** (qué cuenta como hecho, qué cubre, quién tiene
razón), no es encargo de ejecutor. Si exige **producir contra un criterio ya escrito**, sí lo es.

## §6 — Por qué la separación es estructural

Los últimos 3 milestones mostraron **producto honesto** (552/552 líneas reales, 0 `unsafe`, 0 mock)
junto a **medición inflada** (0/16 de cobertura real; un gate que comparaba sus propios literales;
un delta sin consumidor). El código era verdadero; el número que lo certificaba, no. La causa no fue
mala intención: fue **posición** — el mismo actor que producía se medía, y medir el propio trabajo
sesga qué se cuenta como terminado. Por eso la separación ejecutor/auditor es **estructural, no
cosmética**: no se arregla pidiendo honestidad, se arregla quitando al productor la pluma de la
certificación.

## §7 — Cómo adoptarlo en otro target

1. **Copiar este documento tal cual** a `docs/AGENT-CONTRACT.md` del target: §0-§8 son agnósticos y no
   requieren ajuste alguno. Si se quiere conservar el caso de origen como muestra, sustituir el
   contenido del **anexo** (marcado NO normativo) por el del nuevo repo.
2. **Pegar el bloque** de `AGENTS.md` (marcadores `CONTRATO-EJECUTOR:INICIO` … `CONTRATO-EJECUTOR:FIN`)
   como última sección, respetando el tope de ~1,5 KB del bloque.
3. **Declarar qué es «instrumento» en ese repo**: qué gate/linter/validador mide, quién lo escribe y
   quién no puede editarlo. Sin esa declaración, la cláusula (a) no tiene referente.

**Aviso de durabilidad:** si `AGENTS.md` lo **genera un sync** desde una fuente externa, el bloque se
pierde en la siguiente aplicación del sync. Los marcadores existen para eso: permiten **re-sembrarlo**
tras cada sync y **detectar la pérdida de forma mecánica** (buscar el marcador de inicio y contar
exactamente 1). La durabilidad real exige que el bloque viva también en la **fuente** del sync; hasta
entonces, la re-siembra manual es el mecanismo y la comprobación del marcador es su alarma.

## §8 — Métricas de muerte del contrato

El contrato está muerto (no muerde) si, en una ventana de **≥10 cierres**:

1. el auditor **no retiró ninguna cifra inflada** — señal de que no re-deriva, solo lee; y
2. **no encontró ninguna afirmación sin `file:line`** — señal de que no audita anclas.

Cualquiera de las dos, sostenida en la ventana, obliga a revisar el instrumento (no al ejecutor):
un contrato que nunca produce un hallazgo no está midiendo nada.

## §9 — Auditoría mecánica del cierre (paso OBLIGATORIO)

Ningún cierre se acepta por lo que dice su reporte: se acepta por lo que dice la
historia. Este paso es obligatorio y se ejecuta **antes** de proponer un cierre:

1. **El instrumento.** Un auditor de cierre independiente que recibe **un CR** o **un
   rango de commits** y dicta PASS/FAIL sin conocer al ejecutor. No lee reportes de la
   lane; re-deriva de git y de los ficheros de cierre. En este repo:
   `python3 scripts/guard/lane-integrity.py --cr <CR>` (o `--commits a..b`).
2. **Qué exige** (un solo FAIL hace FAIL al cierre):
   - **(a) Separación de actos:** ningún commit toca a la vez implementación
     (`crates/**`, `src/**`) y gate (`scripts/*-001.py`, `scripts/*gate*.py`).
   - **(b) Gate anterior:** el gate de la lane existe en un commit **anterior** al
     primer commit de implementación del mismo CR. Si la lane no toca gates, debe
     preexistir un gate escrito por otra lane.
   - **(c) Estado ≤ `candidato`:** el HH-1 no puede declarar más que `candidato` salvo
     que exista fila de QA **independiente** (autor ≠ autor del HH-1) con veredicto.
   - **(d) Evidencia viva:** toda ruta citada como evidencia existe en la revisión
     auditada y, si el INDEX declara `sha256`, coincide con los bytes de ese fichero.
   - **(e) 0 push / 0 tag:** ningún commit del rango es alcanzable desde
     `refs/remotes/*` ni desde `refs/tags/*`.
3. **Exit codes:** `0` = PASS · `1` = FAIL (con hallazgos tipados) · `2` = uso/error.
   El código del hallazgo (`A-MIXTO`, `GATE_POSTERIOR`, `STATUS_SIN_QA`,
   `EVIDENCIA_AUSENTE`, `EVIDENCIA_SHA`, `PUSH_DETECTADO`, `TAG_EN_RANGO`) se cita tal
   cual en el reporte: el auditor no parafrasea el hallazgo.
4. **Un FAIL no se parchea en la lane que lo sufre.** Se registra (HH-1 + INDEX) y se
   abre lane propia. Retirar la cifra o editar la evidencia para poner verde es la
   violación de §1(c).
5. **El instrumento se falsifica a sí mismo:** `--selftest` construye 4 casos negativos
   (gate+impl en un commit, gate posterior a la impl, evidencia con ruta inexistente,
   `status` alto sin QA) y 1 control de no-falso-rojo, con cadenas de commits colgantes
   **sobre el repo real** (plumbing: 0 refs, 0 HEAD, 0 índice). Si un caso no muerde, el
   instrumento sale 1.

**Límite declarado (lo que este paso NO verifica):** si el gate es **débil por dentro**
—existe, corre y siempre da verde— pasa todas las comprobaciones; su calidad es materia
del auditor (§3.5), no de este instrumento. Tampoco juzga si el veredicto de la fila de
QA es honesto (§3.4: no se hereda, se re-deriva), ni ve un push cuyo ref remoto fue
borrado, ni una historia reescrita por `amend`/`rebase`.

---

## Anexo — Caso que motivó el contrato (NO normativo)

Datos del repo donde se escribió este documento; sirven como muestra, no como regla:

- Auditoría `CR-276c7d94`, sha `eead706f`: 7 hallazgos `BLOQUEA-M43`.
- Milestone M42 cerrado como `PARTIAL` con 8 `GAP-M42-01..08`.
- Milestone M41: 3 afirmaciones falsas detectadas en la re-auditoría.
- 64,5% de `crates/app/src` son sondas (código de medición, no de producto).
- 26 `expect()` que abortan con exit 101 en vez de emitir fila roja.

Lectura: el producto existía y era real; la certificación no. Ese delta es exactamente lo que este
contrato separa.
