# ALCANCE POST-M44 — mapa hacia el juego 100 % jugable

- **CR:** `CR-d2dcd12a-f9cc-41cb-b98f-c03b66ad6f51` · **Fecha:** 2026-10-30 · **Lane:** read-only (og-scout)
- **Método:** solo lectura + censos re-ejecutables. Toda afirmación positiva de runtime exige gate
  que la recorra (R-positiva, `COVERAGE-100.md` §0); lo no medido se declara `NO-VERIFICADO`, no se infiere.
- **Frontera:** 0 bytes en `crates/**`; no se tocan `crates/promotion-gate/**`, `crates/side-contract/**`,
  `promotion-ledger.jsonl` ni `daggerfall-side-projects/*`.

## 0. Qué sigue después de M44 (confirmado)

La secuencia en vigor es **`M43 → M44 → M-SHELL → M-PLAY`**
(`docs/acknowledgments/roadmap-m18-m48.md:39`; `docs/acknowledgments/roadmap.md:11-14`):

| Hito | Qué es | Nominal | Estado | Puerta |
|:---|:---|---:|:---|:---|
| M44 DUN-1 | Contenido y estado de mazmorras (capas A/B/C + save v3 en W44-5) | 39.00 u (25.50–52.00) | `AUSENTE`, siguiente, entero y desbloqueado | W44-0 spec + gates `DUN-*-001` |
| M-SHELL | Aplicación y sesión: 13 capacidades (boot, loading, menú, new game, personaje, mundo, sesión, slots, HUD, mapa, viaje, pausa/opciones, salida) | **17.00 u** (12.92–21.93), §3.9b (`roadmap-m18-m48.md:400`) | `PLANNED`, fila del register desde `CR-359b5d8e` | `SHELL-001..006` (declarados) |
| M-PLAY | Conectar todo el juego; **única aceptación: probe P1..P9** | **6.50 u PROVISIONAL** (3.90–9.10), §3.9a (`roadmap-m18-m48.md:556`) | `PLANNED`, fila del register desde `CR-359b5d8e` | `PLAYTHROUGH-PROBE` P1..P9 |
| M45 / M46 / M48 | Mods, online, modern | 39 + 78 + 31 u | `DEFERRED` tras M-PLAY (`roadmap.md:42`) | — |
| M47 | Packaging/plataforma | 15.50 u | `OPTIONAL` (probe PASS + GO owner) | — |

Dependencias duras: `W44-5` (save v3) precede a los slots de M-SHELL (`roadmap-m18-m48.md:397-398,523`);
`M44 → M-SHELL → M-PLAY` antes de M45/M46/M48/M47 (`roadmap-m18-m48.md:639`).
`M-PLAY` no se difiere ni se bloquea por cola: es el cierre del tramo.

## 1. Specs M-SHELL existentes (lo ya medido)

| Instrumento / spec | Filas | Estado medido |
|:---|:---|:---|
| `scripts/shell-001.py` (fase 1: boot→exit por la ventana real) | 10 (`SH-01..SH-10`) | **PASS** ×2 byte-idénticas, `DIGEST 89b100dd…` (`COVERAGE-100.md:48,82`) |
| `scripts/shell-002.py` (fase 2: menú 5 ítems, creación 10 etapas, sesión/slots, mapa/viaje/pausa) | 14 (`SH2-01..SH2-14`) + `CTRL` + 3 `PROC-*` | **FAIL rojo estable** 3/3, `DIGEST 52691417…` (handoff `HH-20261029-shell-fase2-CR-75e7c81f`) |
| `docs/sintergic/playability/SHELL-001.md` | gate fase 1, falsificadores, anclas `src=` (§1–§5) | spec + gate PASS |
| `docs/sintergic/playability/SHELL-002.md` | 7 criterios de cierre; `SH2-05/13` bloqueadas en M44 W44-5 (save v3) | spec, gate en rojo |
| `docs/sintergic/playability/SHELL-VISIBLE-LAYER.md` | censos C1–C11 re-ejecutables (`:33-90`): efecto config = 0, automap en ventana = 0, save v3 = 0, createchar en ventana = 0 | matriz de ceros medida |
| `docs/sintergic/playability/COVERAGE-100.md` | 25 capacidades | vigente **9 CUBIERTA · 14 PARCIAL · 0 AUSENTE · 2 NO-VERIFICADO** (§5-ter, `:242,274`) |
| `docs/sintergic/playability/PLAYTHROUGH-PROBE.md` | contrato P1..P9 + 9 controles negativos (`:300-308`) | spec; **instrumento inexistente** (`COVERAGE-100.md` §0: `scripts/playthrough-001.py` ausente) → M-PLAY `NOT-RUN` |
| `docs/sintergic/m44/DUN-1-ACCEPTANCE-SPEC-CR-7fd7260f.md` | 46 filas requisito (42 ROJO · 4 VERDE) | spec de aceptación de M44, medido |

## 2. Tabla superficie — estado — M — evidencia

| # | Superficie (desde arranque original) | Estado | M que la cubre | Evidencia |
|:--|:---|:---|:---|:---|
| 1 | **Main menu** (ver, navegar, elegir) | **PARTIAL** | M18 B1-d + M-SHELL W-S2/W-S5 | `CUBIERTA` fila 3 por `SHELL-001` SH-03 (menú, ítems≥2, nav); sin opciones ni creación (FG-10, `FUNCTIONAL-GAPS.md:433`; GAP-SHELL-09) |
| 2 | **New game** (entrar a jugar) | **PARTIAL** | M-SHELL W-S2 | `CUBIERTA` fila 4 por SH-04 (`Return`→`Playing`); personaje fijo demo, sin paso por creación (FG-10 `:433`) |
| 3 | **Load game** (cargar desde menú) | **PARTIAL** | M33 + M-SHELL W-S4 (tras M44 W44-5) | Formato medido headless (`SAVE-ALL-001`, `main.rs:693`); censo 0 en ventana (fila 7); `SH2-05/13` bloqueadas en save v3 |
| 4 | **Settings con efecto real** | **PARTIAL** | M38 + M-SHELL W-S5 | Pausa medida (SH-09: `Paused` + ticks congelados, fila 12); 11 páginas existen a nivel gate (M38 `UI-CONFIG-001` 11/11) pero efecto = 0 (censo C3) y sin componer (SH2-07/08 en rojo) |
| 5 | **Creación de personaje** (raza/clase/atributos) | **PARTIAL** | M38/M20 + M-SHELL W-S2 | 16 superficies DONE a nivel gate (`UI-CREATE-001` 16/16); censo C8 = 0 `createchar` en `m18_window.rs`; composición = W-S2 |
| 6 | **Guardado/carga (slots)** | **PARTIAL** | M33 + M-SHELL W-S4 (save v3, tras M44) | `SAVEGAME-WRITE/READ-001` miden formato, no la partida del jugador (FG-03 `:299`); slots = W-S4 sobre save v3 |
| 7a | **Conexiones mundo: puertas calle↔retail** | **DONE** (vía gate; P2/P4 del playthrough pendientes) | M18 + M-PLAY WMP-2 | `GATE fg01-world-doors.py` PASS ×2: spawn en capital, `E` cruza a `Interior(N0000055.RDB)` (fila 15 CUBIERTA); `GAP-FG01-02` CERRADA (`FUNCTIONAL-GAPS.md:31`) |
| 7b | **Conexiones mundo: entrada a mazmorra** | **AUSENTE** | **M44** (contenido) + runtime M18 | Sin verbo en runtime: `assemble(` 0 callers en `m18_window.rs`, `dungeon` 1 comentario `:3562` (FG-02 `:234`); geometría sí medida (4.232, `PROBE-DUNGEON-001`) |
| 7c | **Viaje rápido / regiones** | **DONE** (vía gate) | M31 + M43-W43-3 | `CUBIERTA` fila 11 por SH-08 (`T` con mapa abierto, `delta_px>0`, minutos y oro) |
| 8 | **Historia/lore jugable desde arranque** | **AUSENTE** (runtime) / sistemas quest DONE | M42 (sistemas) ; hilo principal → **GAP-MAINQUEST-01** (nuevo, §3) | Diario fijo 2 títulos, 0 consumo quest en ventana (FG-09 `:408`); ningún paso P1..P9 mide quests; M42 cerró sistemas (`roadmap.md:84`), no hilo ejecutable |

## 3. GAPs

- **Cubiertos por M existentes:** FG-02→M44 · FG-03→M-SHELL W-S4 · FG-04→M-PLAY WMP-6 ·
  FG-05→M-PLAY WMP-4 · FG-01→WMP-2 (puertas mundo) · FG-10→W-S2 · FG-11→W-S2/W-S5 ·
  FG-06/FG-07/FG-08 MOLESTA (runtime M18; mapa→W-S5/automap) · FG-12 `GAP-TAX-11` diferido a M44.
- **`GAP-MAINQUEST-01` (NUEVO) — hilo principal / lore jugable desde el arranque sin M que lo cubra.**
  M42 cerró sistemas quest (despachador, facciones, crimen/ley) a nivel gate; lo que falta es el
  contenido ejecutable: diario vivo, oferta/aceptación desde NPC, objetivos y recompensas, y el gancho
  de la historia principal. Ningún WP de M43/M44/M-SHELL/M-PLAY lo nombra (M-PLAY lo excluye por
  construcción: 0 pasos P1..P9 miden quests).

## 4. Propuesta de M faltante (spec, SIN implementar): M-QST — quest runtime jugable

- **Nombre:** M-QST — quest runtime + hilo principal jugables desde el arranque.
- **Alcance:** (a) journal vivo en runtime (lectura/escritura del estado M42, no fixture);
  (b) oferta/aceptación de quest desde NPC (`E` compone diálogo mínimo sobre `TEXT.RSC`);
  (c) ejecución `.QBN` en partida (condiciones, objetivos, recompensas); (d) gancho de historia
  principal disponible tras new game; (e) persistencia en save v3 (tras M44 W44-5).
- **Aceptación:** gate `QST-RT-001` (aceptar quest desde NPC → diario cambia con objetivo y recompensa;
  digest del panel 3 deja de ser constante; round-trip save/load conserva quests) + 1 paso `P10`
  añadido a `PLAYTHROUGH-PROBE.md` antes de implementar (la probe manda, no la prosa).
  Secuencia: tras M-SHELL (necesita menú/diálogo/slots), en paralelo a M-PLAY sin bloquear su P1..P9.
- **No-alcance:** no reescribe sistemas M42; no toca mazmorras (M44) ni save v3 (W44-5).

## 5. Límites de esta auditoría

- Filas 23 (clima) y 24 (ciclo día/noche) de `COVERAGE-100.md`: `NO-VERIFICADO` (solo `CITA`, R-positiva).
- Píxeles/framebuffer: ningún gate lee píxeles (`no-medido: píxeles` declarado por fila en §5-ter).
- Este informe no ejecuta build ni ventana (sin instrumentos de ventana en la lane); reutiliza
  digests y censos citados con su `file:line` o comando re-ejecutable.
