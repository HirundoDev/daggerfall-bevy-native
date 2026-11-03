# SCOPE AFTER M44 — map toward the 100 % playable game

- **CR:** `CR-d2dcd12a-f9cc-41cb-b98f-c03b66ad6f51` · **Date:** 2026-10-30 · **Lane:** read-only (og-scout)
- **Method:** read-only plus re-runnable censuses. Every positive runtime claim requires a gate
  that walks it (R-positive, `COVERAGE-100.md` §0); what is not measured is declared `NO-VERIFICADO`, not inferred.
- **Boundary:** 0 bytes in `crates/**`; `crates/promotion-gate/**`, `crates/side-contract/**`,
  `promotion-ledger.jsonl` and `daggerfall-side-projects/*` are not touched.

## 0. What follows after M44 (confirmed)

The sequence in force is **`M43 → M44 → M-SHELL → M-PLAY`**
(`docs/acknowledgments/roadmap-m18-m48.md:39`; `docs/acknowledgments/roadmap.md:11-14`):

| Milestone | What it is | Nominal | State | Gate |
|:---|:---|---:|:---|:---|
| M44 DUN-1 | Dungeon content and state (layers A/B/C + save v3 in W44-5) | 39.00 u (25.50–52.00) | `AUSENTE`, next, whole and unlocked | W44-0 spec + `DUN-*-001` gates |
| M-SHELL | Application and session: 13 capabilities (boot, loading, menu, new game, character, world, session, slots, HUD, map, travel, pause/options, exit) | **17.00 u** (12.92–21.93), §3.9b (`roadmap-m18-m48.md:400`) | `PLANNED`, register row since `CR-359b5d8e` | `SHELL-001..006` (declared) |
| M-PLAY | Connect the whole game; **single acceptance: probe P1..P9** | **6.50 u PROVISIONAL** (3.90–9.10), §3.9a (`roadmap-m18-m48.md:556`) | `PLANNED`, register row since `CR-359b5d8e` | `PLAYTHROUGH-PROBE` P1..P9 |
| M45 / M46 / M48 | Mods, online, modern | 39 + 78 + 31 u | `DEFERRED` after M-PLAY (`roadmap.md:42`) | — |
| M47 | Packaging/platform | 15.50 u | `OPTIONAL` (probe PASS + GO owner) | — |

Hard dependencies: `W44-5` (save v3) precedes the M-SHELL slots (`roadmap-m18-m48.md:397-398,523`);
`M44 → M-SHELL → M-PLAY` before M45/M46/M48/M47 (`roadmap-m18-m48.md:639`).
`M-PLAY` is neither deferred nor blocked by queue: it is the closing of the stretch.

## 1. Existing M-SHELL specs (what is already measured)

| Instrument / spec | Rows | Measured state |
|:---|:---|:---|
| `scripts/shell-001.py` (phase 1: boot→exit through the real window) | 10 (`SH-01..SH-10`) | **PASS** ×2 byte-identical, `DIGEST 89b100dd…` (`COVERAGE-100.md:48,82`) |
| `scripts/shell-002.py` (phase 2: 5-item menu, 10-stage creation, session/slots, map/travel/pause) | 14 (`SH2-01..SH2-14`) + `CTRL` + 3 `PROC-*` | **stable red FAIL** 3/3, `DIGEST 52691417…` (handoff `HH-20261029-shell-fase2-CR-75e7c81f`) |
| the M-SHELL gate specification | phase 1 gate, falsifiers, `src=` anchors (§1–§5) | spec + gate PASS |
| the M-SHELL closure criteria | 7 closure criteria; `SH2-05/13` blocked on M44 W44-5 (save v3) | spec, gate red |
| the visible-layer census | re-runnable censuses C1–C11 (`:33-90`): config effect = 0, automap in window = 0, save v3 = 0, createchar in window = 0 | measured matrix of zeros |
| the capability census | 25 capabilities | current **9 CUBIERTA · 14 PARCIAL · 0 AUSENTE · 2 NO-VERIFICADO** (§5-ter, `:242,274`) |
| the playthrough probe contract | P1..P9 contract + 9 negative controls (`:300-308`) | spec; **instrument nonexistent** (`COVERAGE-100.md` §0: `scripts/playthrough-001.py` absent) → M-PLAY `NOT-RUN` |
| the M44 acceptance specification | 46 requirement rows (42 RED · 4 GREEN) | M44 acceptance spec, measured |

## 2. Surface — state — M — evidence table

| # | Surface (from original start-up) | State | M that covers it | Evidence |
|:--|:---|:---|:---|:---|
| 1 | **Main menu** (see, navigate, choose) | **PARTIAL** | M18 B1-d + M-SHELL W-S2/W-S5 | `CUBIERTA` row 3 by `SHELL-001` SH-03 (menu, items≥2, nav); no options and no creation (FG-10, `FUNCTIONAL-GAPS.md:433`; GAP-SHELL-09) |
| 2 | **New game** (enter to play) | **PARTIAL** | M-SHELL W-S2 | `CUBIERTA` row 4 by SH-04 (`Return`→`Playing`); fixed demo character, no creation step (FG-10 `:433`) |
| 3 | **Load game** (load from menu) | **PARTIAL** | M33 + M-SHELL W-S4 (after M44 W44-5) | Format measured headless (`SAVE-ALL-001`, `main.rs:693`); census 0 in window (row 7); `SH2-05/13` blocked on save v3 |
| 4 | **Settings with real effect** | **PARTIAL** | M38 + M-SHELL W-S5 | Pause measured (SH-09: `Paused` + frozen ticks, row 12); 11 pages exist at gate level (M38 `UI-CONFIG-001` 11/11) but effect = 0 (census C3) and not composed (SH2-07/08 red) |
| 5 | **Character creation** (race/class/attributes) | **PARTIAL** | M38/M20 + M-SHELL W-S2 | 16 surfaces DONE at gate level (`UI-CREATE-001` 16/16); census C8 = 0 `createchar` in `m18_window.rs`; composition = W-S2 |
| 6 | **Save/load (slots)** | **PARTIAL** | M33 + M-SHELL W-S4 (save v3, after M44) | `SAVEGAME-WRITE/READ-001` measure the format, not the player's run (FG-03 `:299`); slots = W-S4 over save v3 |
| 7a | **World connections: street↔retail doors** | **DONE** (via gate; playthrough P2/P4 pending) | M18 + M-PLAY WMP-2 | `GATE fg01-world-doors.py` PASS ×2: spawn in the capital, `E` crosses into `Interior(N0000055.RDB)` (row 15 CUBIERTA); `GAP-FG01-02` CLOSED (`FUNCTIONAL-GAPS.md:31`) |
| 7b | **World connections: dungeon entrance** | **AUSENTE** | **M44** (content) + M18 runtime | No verb in runtime: `assemble(` 0 callers in `m18_window.rs`, `dungeon` 1 comment `:3562` (FG-02 `:234`); geometry is measured (4.232, `PROBE-DUNGEON-001`) |
| 7c | **Fast travel / regions** | **DONE** (via gate) | M31 + M43-W43-3 | `CUBIERTA` row 11 by SH-08 (`T` with map open, `delta_px>0`, minutes and gold) |
| 8 | **Story/lore playable from start-up** | **AUSENTE** (runtime) / quest systems DONE | M42 (systems); main thread → **GAP-MAINQUEST-01** (new, §3) | Fixed journal with 2 titles, 0 quest consumption in window (FG-09 `:408`); no P1..P9 step measures quests; M42 closed the systems (`roadmap.md:84`), not an executable thread |

## 3. GAPs

- **Covered by existing Ms:** FG-02→M44 · FG-03→M-SHELL W-S4 · FG-04→M-PLAY WMP-6 ·
  FG-05→M-PLAY WMP-4 · FG-01→WMP-2 (world doors) · FG-10→W-S2 · FG-11→W-S2/W-S5 ·
  FG-06/FG-07/FG-08 MOLESTA (M18 runtime; map→W-S5/automap) · FG-12 `GAP-TAX-11` deferred to M44.
- **`GAP-MAINQUEST-01` (NEW) — main thread / lore playable from start-up with no M covering it.**
  M42 closed the quest systems (dispatcher, factions, crime/law) at gate level; what is missing is the
  executable content: live journal, offer/acceptance from an NPC, objectives and rewards, and the main
  story hook. No WP of M43/M44/M-SHELL/M-PLAY names it (M-PLAY excludes it by
  construction: 0 P1..P9 steps measure quests).

## 4. Proposal for the missing M (spec, NOT implemented): M-QST — playable quest runtime

- **Name:** M-QST — quest runtime + main thread playable from start-up.
- **Scope:** (a) live journal in runtime (read/write of the M42 state, not a fixture);
  (b) quest offer/acceptance from an NPC (`E` composes minimal dialogue over `TEXT.RSC`);
  (c) `.QBN` execution in play (conditions, objectives, rewards); (d) main story hook
  available after new game; (e) persistence in save v3 (after M44 W44-5).
- **Acceptance:** gate `QST-RT-001` (accept a quest from an NPC → journal changes with objective and reward;
  panel 3 digest stops being constant; save/load round-trip preserves quests) + 1 step `P10`
  added to `PLAYTHROUGH-PROBE.md` before implementing (the probe rules, not the prose).
  Sequence: after M-SHELL (needs menu/dialogue/slots), in parallel with M-PLAY without blocking its P1..P9.
- **Non-scope:** does not rewrite M42 systems; does not touch dungeons (M44) or save v3 (W44-5).

## 5. Limits of this audit

- Rows 23 (weather) and 24 (day/night cycle) of `COVERAGE-100.md`: `NO-VERIFICADO` (only `CITA`, R-positive).
- Pixels/framebuffer: no gate reads pixels (`no-medido: píxeles` declared per row in §5-ter).
- This report runs no build and no window (no window instruments in the lane); it reuses
  digests and censuses cited with their `file:line` or re-runnable command.
