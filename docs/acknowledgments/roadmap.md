# Roadmap

*Case file split from [`docs/ACKNOWLEDGMENTS.md`](../ACKNOWLEDGMENTS.md) — split mechanically; only relative link targets and heading levels were rebased.*

> **The complete plan lives elsewhere.** This page is the stage-by-stage public summary. The single
> canonical plan — M18 to M48 plus E8, with the atomic work breakdown, gates, dependencies, risks and
> closure criteria — is [Roadmap M18–M48 + E8](roadmap-m18-m48.md). The operating rules for contributors
> and agents (source of truth, real state, limits, how to continue) are in the
> [Agent entry point](agent-entrypoint.md).

**Sequence in force (re-baselined 2026-10-29):** **M43 → M44 → M-SHELL → M-PLAY**. M43 (SYS-3) and
M44 (DUN-1 — the dungeons of the original, content and state) are **DONE at gate level** (measured
2026-10-30 at `de94974` and `3d25278`; no tag — acceptance is the owner's); **M-SHELL** —
the application and the session, 17.00 u, [§3.9b of the plan](roadmap-m18-m48.md#39b-m-shell--shell-the-application-and-the-session-1700-u-range-12922193) —
comes after M44 (its `W44-5` writes save **v3**, the format the shell's slots are built on), and **M-PLAY** is the milestone
that connects the whole game and whose **only** acceptance condition is the complete playthrough probe (                      ,
`P1..P9`, where `P5` walks a **real dungeon of the original** ⇒ hard dependency on M44). The wide tail does not queue behind it:

M45, M46, M48 and M47 do not open until `M-PLAY` passes its end-to-end playthrough probe; M44 is neither deferred nor blocked.

| Stage | State | What closes it |
|---|---|---|
| M0–M17 | Closed and tagged | Annotated tags `M<n>-YYYY-MM-DD` with matching `release/M<n>` branches (`M13b` is a tagged parity follow-up) |
| M18 — Acceptance | **`PARTIAL-TAG-PENDING`** — DONE at technical level; **acceptance signature recorded 2026-10-26 by delegation**; tag none | `PROBE-MAP-001` resolves 15,251/15,251 locations; the suite froze at 18/18 for this milestone and has grown with each later one (37/37 at the M29 close); the 21 acceptance rows were measured on the real window and signed **8 PASS · 9 NC-ACEPTA · 4 NOT-COMPARABLE · 0 FAIL** — `NC-ACEPTA` is acceptance with a declared non-conformity, `NOT-COMPARABLE` a declared class, not a downgraded pass. No `M18` tag and no `release/M18` branch exist: the tag remains a separate later act of the owner. The **ORIGINAL side is not certified** (Daggerfall 1.07.213 in DOSBox): comparing the engine rows against the original is still the owner's own act |
| M19 — Locomotion | DONE at gate level; acceptance pending, no tag | Daggerfall Unity locomotion and the player body (`behavior::locomotion`, 21 tests) integrated into the demo and the window through `Locomotion::step`; `PROBE-LOCOMOTION-001` **88/88** (stdout 11,904 B, sha256 `9de0cfdd7fe8d74818d9bb87618beca0ef7a9aabcf3b65c6438433c85a937b52`) and `PROBE-STEALTH-001` **16/16** (1,501 B, sha256 `a59c64550da912765902962635374af84128d82adab63410844de1f7134a9d43`), each run in **two separate OS processes byte-identical**; NC1–NC5 each turned its expected rows red (exit 1) and the source sha256 was restored to `c0e34bb2…` — the M19 closure record |
| M20–M26 — gameplay parity, first tramo | DONE at gate level; acceptance pending, no tag | Stats and skills (M20), items/equipment/loot (M21), combat (M22), magic (M23), enchanting and alchemy (M24), monsters, enemy AI and encounters (M25), living NPCs, dialogue and rumours (M26). At the M26 close: 534 tests passed / 0 failed and the suite printed 27/27. Re-measured in `CR-c62fc6d3`: their gate rows ran in **two separate OS processes byte-identical**, binary `target/debug/daggerfall-app` sha256 `f0b9ba86…` at tree `e9e7908`, plus the negative control of each milestone turned red and restored — the recorded re-measurement manifest |
| **M26 — living NPCs** | DONE at gate level (closed 2026-10-21); acceptance pending, no tag | `behavior::npc_ai` (population 24/48/72/96, 1.6 m navgrid, `SeekingTile↔MovingForward` routine, 1/32 guards) + `behavior::dialogue` (7209/7208/7207/7206 greetings, faction topics, `TEXT.RSC` answer IDs) + `parser::rumor` (a mill over `RUMOR.DAT`: 31 records, channels, a 50/1 reservoir, expiry and a region filter); `PROBE-NPC-AI-001` / `DIALOGUE-001` / `RUMOR-001` + `NPC-DEMO-001`. Re-measured in `CR-c62fc6d3`: its three gate rows ran in two separate OS processes byte-identical, binary sha256 `f0b9ba86…` at tree `e9e7908` |
| **M27 — guilds and factions** | DONE at gate level (closed 2026-10-21); acceptance pending, no tag | `behavior::factions` / `behavior::guild` over the `FACTION.TXT` graph (365 ids), reputation with tree propagation, ranks 0–9 and per-rank services; `PROBE-FACTION-001` (17/17), `PROBE-GUILD-001` (33/33), `M27-DEMO-001` (15/15). Re-measured in `CR-c62fc6d3`: its two gate rows ran in two separate OS processes byte-identical, binary sha256 `f0b9ba86…` at tree `e9e7908` |
| **M28 — crime, law and courts** | DONE at gate level (closed 2026-10-22); acceptance pending, no tag | `behavior::law` (penalty clamps, legal reputation through the M27 graph, the court and prison clock) plus the production composition layer that turns a guard request into an executable order; `PROBE-CRIME-001` and `PROBE-LAW-001` (sentence 262 steps, `5480/375 → 2740/187`, `clock_advanced=16,156,800`). Re-measured in `CR-c62fc6d3`: its two gate rows ran in two separate OS processes byte-identical, binary sha256 `f0b9ba86…` at tree `e9e7908` |
| **M29 — playable retail quests** | DONE at gate level (closed 2026-10-22); acceptance pending, no tag | `behavior::quests` — the `QBN` opcode interpreter, clock/time formulas, journal and lifecycle; `PROBE-QUEST-EXEC-001` (24 golden vectors, a 20-quest determinism trace), `LIFECYCLE-001`, `JOURNAL-001`; census 11/11 over 247 `.QBN` / 244 `.QRC`. Re-measured in `CR-c62fc6d3`: its gate row ran in two separate OS processes byte-identical, binary sha256 `f0b9ba86…` at tree `e9e7908` |
| **M30 — economy and banking** | Technical closure PASS (2026-10-23); owner signature and tag pending | `behavior::economy` — `CalculateCost` with regional prices, the 36 `(building, group)` buy table, five trade modes, 53 holidays, loans and default, house and ship (`BankLedger`, `LoanEvent`); `PROBE-SHOP-001`, `PROBE-ECONOMY-001`, `PROBE-BANK-001`. Implemented at technical level: its consumer is the probe, not the demo window |
| **M31 — travel, weather and calendar** | Technical closure PASS (2026-10-23); owner signature and tag pending | `heart::travel` / `heart::weather` / `heart::calendar`; `PROBE-TRAVEL-001` (34/34 vectors, 2,200-pixel route over the real `CLIMATE.PAK`, byte-identical stdout in two processes), `PROBE-WEATHER-001` (13/13, 240,000 draws, `max_abs_delta = 0.0121`), `PROBE-CALENDAR-001` (14/14). Probe-backed, not yet surfaced in the demo window |
| **M32 — conditions, death and rest** | Technical closure PASS (2026-10-23); owner signature and tag pending | `skills::conditions` and `skills::rest` — diseases, vampirism and lycanthropy stages, rest and the death state; `PROBE-CONDITION-001` (24/24), `PROBE-VAMPIRE-001` (16/16), `PROBE-REST-001` (10/10), `PROBE-DEATH-STATE-001` (2/2). Probe-backed, not yet surfaced in the demo window |
| **M33 — total savegame** | Technical closure PASS (2026-10-26); owner signature and tag pending | `memory::save_v2` / `memory::savegame` / `memory::savegame_v2` — savegame v2, v1→v2 migration and the classic `SAVEGAME` read/write; `PROBE-SAVE-ALL-001` (24/24 byte-identical round trips, five corruption mutations rejected, deterministic digest), `PROBE-SAVE-MIGRATION-001`, `PROBE-SAVEGAME-READ-001`, `PROBE-SAVEGAME-WRITE-001`, `PROBE-M33-DEMO-001` |
| **M34 — end-to-end integration + presentation** | **`PARTIAL-TECHNICAL-CLOSURE`** — DONE at technical level with the named row `AUDIO-RT-001-DEVICE`; acceptance pending, no tag | The E2E spine, the full presentation pass and the HUD, measured by `PLAYSESSION-001`, `UI-FULL-001` and `AUDIO-RT-001` (`ALL-001` **40/40**) with `SPINE-001` **12/12**. `AUDIO-RT-001` reproduces `PASS(degraded)` — `[SKIP-DEVICE]`, the binary built without the `audio` feature — so its measure covers the degraded path only and the audio branch is the owner's decision. `UI-FULL-001`, also the M40 gate, certifies **85/85** census surfaces with per-surface evidence since `CR-bc68d7ce` (2026-10-27): the 7 `Infra` entries as layer contract of `ui_compat`, and 3 read-only `Playable` windows recorded `not-applicable` on axis A5, so no surface is left uncertified |
| **M35 — biomes, 2D vegetation, sky** | **`PARTIAL`** — the named row `GAP-TAX-11` is **deferred to M44**; no tag | **Optional annex**, non-blocking: `BIOME-001` **25 passed / 0 failed / 1 NA** and `SPRITE-001` **11/11**. The `NA` is that declared gap: the pinned corpus carries no tile-class data, so the probability gate cannot be measured and the probe returns `DEGRADED` instead of inventing a number. Not a gate for closing Plan A |
| **M36–M38 — Plan B, UI-0, UI-1 and UI-2** | Technical closure measured 2026-10-27; owner signature and tag pending | `UI-AUDIT-001` A+B+C recomputes the census from the code (`VERDICT: PASS`, two processes, identical stdout): **85 = 78 surfaces + 7 infrastructure**, measured **4 implemented / 47 partial / 34 absent**, rule **35 PATCH · 50 REWRITE**; then the seven-axis layer with `UI-BASE-001` **9/9** and `UI-INPUT-001` **10/10** over **3 pilot windows**, measuring **0.4708333 u/window at one QA round** (band `r`) against an L1 baseline of **0.0565 u/window**; then **M38 DONE** — `UI-CREATE-001` (**16 surfaces**) and `UI-CONFIG-001` (**11 surfaces**) — at requirements coverage **0.857 (6/7)**, with **K7** (the owner's side-by-side) the only open item |
| **M39–M40 — Plan B, UI-3 and UI-4** | State: M39 DONE at gate level (coverage 0.857, K7 open, no tag); M40 DONE at gate level (`UI-FULL-001` 85/85 on 8/8 rules, no tag) | 2 milestones, each listed with its scope, gate and `u` in [Plan B in detail](#plan-b-in-detail--13-milestones-m36m48--e8) below and broken into work packages in the [complete plan](roadmap-m18-m48.md#3-atomic-work-breakdown-for-plan-b) |
| **M41–M42 — Plan B, depth (SYS-1, SYS-2)** | Technical closure measured; owner signature and tag pending | Combat, magic, items, stats and resistances (M41) and NPCs, guilds, crime/law and quests (M42), each closed by its work-package gates (§3.6 and §3.7 of the plan) |
| **M43–M44 — Plan B, depth and dungeons (SYS-3, DUN-1)** | both **DONE at gate level** (M43 measured at `de94974` on 2026-10-30, M44 at `3d25278`; no tag — acceptance is the owner's) | M43: economy, travel, weather, calendar and conditions, gated by its work-package gates (§3.8). M44: the **content and state** of the original's dungeons — loot, respawn, bosses, instances, in layers A/B/C (§3.9); it does **not** produce the geometry, which M34 delivered and `PROBE-DUNGEON-001` measures at 4,232, and its first act is `W44-0`, the acceptance spec with golden vectors. Their gate is what releases the probe's `P5`, and M44's `W44-5` writes save **v3**, the format the shell's slots are built on |
| **M-SHELL — the application and the session** | `PLANNED` — no code, no dispatch yet; **a row of the closure register since 2026-10-29** (`CR-359b5d8e` extended `EXPECTED_IDS`; `verdict: AUSENTE`, gates in `missing_rows`) | The 13 capabilities the coverage matrix measures as unreachable from the binary — boot, loading screen, menu, new game, character creation/selection, world load, session, save/load slots, HUD, map, fast travel, pause/options, exit (                 : **2 CUBIERTA · 17 PARCIAL · 2 AUSENTE · 4 NO-VERIFICADO** (corrected 2026-10-29, `CR-359b5d8e`: rows 1 and 15 re-measured by `scripts/fg01-world-doors.py`); the four NO-VERIFICADO keep implementation and gate measured — their reachability is a positive runtime claim with no instrument, because `PLAYTHROUGH-001` and `scripts/playthrough-001.py` do not exist yet). Six work packages and six gates to build (`SHELL-001..006`, none measured yet); **17.00 u nominal (12.92–21.93)**. It composes delivered surfaces (M38/M39/M33/M34) and precedes `M-PLAY`, whose `P1`/`P8`/`P9` run through the application. §3.9b of the plan |
| **M-PLAY — the whole game, connected** | `PLANNED` — no code, no dispatch yet; **a row of the closure register since 2026-10-29** (`CR-359b5d8e`; `verdict: AUSENTE`, `PLAYTHROUGH-PROBE` in `missing_rows`) | One gate and only one: `PLAYTHROUGH-PROBE` `P1..P9` PASS in two independent processes with byte-identical stdout and 9/9 negative controls red. The work list is the defect inventory `the private engineering recordplayability/FUNCTIONAL-GAPS.md` (5 `BLOQUEA-JUGAR` · 6 `MOLESTA` · 1 `COSMÉTICO`); nominal `u` **provisional** (6.50, 3.90–9.10). §3.9a of the plan |
| **M45–M48 — Plan B, the wide tail (E4–E7)** | **`DEFERRED`**: M45, M46, M48 — **148.00 u** (39 + 78 + 31) · **`OPTIONAL`**: M47 — **15.50 u**, with an explicit entry condition | M45, M46, M48 and M47 do not open until `M-PLAY` passes its end-to-end playthrough probe; M44 is neither deferred nor blocked. Reasons in one line: mods (M45), online (M46) and the modern render layer (M48) add **width**, not the functional parity of the original; M47 (packaging, CI, perf, accessibility) opens only if the probe passes **and** the owner asks for distribution. Each is listed with its scope, gate and `u` in [Plan B in detail](#plan-b-in-detail--13-milestones-m36m48--e8) |
| **E8 — deferred expansion horizon** | `AUSENTE`; not estimable | A **separate program** after M48, with its own genesis contract. Never "M49" |
| After M18 — content packs | Design | Declarative packs, a manifest with dependencies, load order, per-pack settings; no code execution |
| Android | Next platform, not started | Free distribution is the point: APK, F-Droid and plain sideloading, offline and online, phone-first interface and performance envelope. No build has been validated |
| Web / WASM | Exploratory, after Android | Bevy's WebGL2 path; bounded by the renderer and by the data requirement. Not on the critical path |
| Windows | Future, planned last | Portable by design, no build validated. Planned after the platforms above; not abandoned |
| macOS | Not planned | Not on the roadmap at this time |
| Online direction | Single-player today; the rest is research notes | The long-term direction is online-capable: optimisation for many concurrent users, more user control and larger world scale, recalibrating toward an MMO feel. Sharding, area-of-interest and replication are notes only — none implemented. Raids, dungeons and endless game modes are under exploration: a direction, not a commitment |

## Plan B in detail — 13 milestones M36–M48 + E8

M45–M48, `M-SHELL`, `M-PLAY` and E8 are **`AUSENTE`** in the closure register (M41–M44 are closed
without a tag) —
specifications live in the [complete plan](roadmap-m18-m48.md), no code for them, no dispatch, no tag.
**The table carries 15 rows today:** the 13 epics M36–M48 plus **`M-SHELL`** (the application and the
session, 17.00 u, section 3.9b of the plan) and **`M-PLAY`**, the closing milestone of the
tramo (section 3.9a of the plan). Neither is a register row because the register's schema stops at
M48 + E8 (`G-N12`/`G-N13` in the plan), and neither sits inside the E1–E7 multipliers. The sequence in
force is **M43 → M44 → M-SHELL → M-PLAY**. M40 closed at gate level
on 2026-10-27 (`UI-FULL-001` 85/85, 8/8 rules), unsigned and untagged. M39 is **DONE at gate
level** (coverage 0.857, **K7** open, no tag). M36, M37 and M38 closed at technical level on
2026-10-27 — the gates are in the table below — and they carry no milestone tag either. `u` is a
nominal estimate with its rounding range (§3.15 of the plan); no dates.

**No `M-CONTENT` is created — the dictamen is NO, and it is measured.** The row-by-row evidence is in
§3.9b of the plan; in one line: the 25-capability matrix finds the rows that would justify a content
milestone already covered by implementation + gate + milestone — 14 regions and 20 commerce ride on
M18/M30 with depth in M43, 15 locations is **NO-VERIFICADO** (implementation and gate exist — `RMB-001`,
`MAP-001` — but *being inside* a location has no executed instrument) and 21 guilds is **PARCIAL** (the
system and its gate exist; the application does not expose it), 17 enemies and
18 items are gated by `MONSTER-001`/`ITEM-001` and owned by M41, and 16 dungeons — the only row with an
uncovered **content** gap — is **M44**, already next in the sequence. What those rows actually lack is
(a) depth already planned in M43/M44 and (b) exposure to the player, which is exactly `M-SHELL`. A fourth
milestone would duplicate M44/M43/M41/M42 and count the same work twice.

| M | Epic | Scope | State | `u` nominal (range) | Gate | HH-1 | Plan |
|:-:|:---|:---|:---|--:|:---|:---|:-:|
| **M36** | UI-0 | audit of the 85 windows + patch/rewrite decision + compatibility contract + E1 re-estimate | DONE (technical closure 2026-10-27, no tag) | 2.00 (1.60–2.40) | `UI-AUDIT-001` A+B+C | `HH-20261027-og-orquestador-m36-ui0-audit` | [§3.1](roadmap-m18-m48.md#31-m36--ui-0-measure-before-touching-anything-200-u-range-160240) |
| **M37** | UI-1 | UI compatibility layer (7 axes) | DONE (technical closure 2026-10-27, no tag) | 4.50 (3.45–5.60) | `UI-BASE-001` A–E + `UI-INPUT-001` (3 pilot windows) | `HH-20261027-og-orquestador-m37-ui1` | [§3.2](roadmap-m18-m48.md#32-m37--ui-1-the-compatibility-layer-450-u-range-345560) |
| **M38** | UI-2 | 16 creation + 11 configuration + menus (27 surfaces) | DONE (technical closure 2026-10-27, no tag; coverage 0.857 — 6/7, K7 open) | 6.50 (4.95–8.40) | `UI-CREATE-001` (16/16) · `UI-CONFIG-001` (11/11) | `HH-20261027-og-l1-70-quality-meta-m38-verify` | [§3.3](roadmap-m18-m48.md#33-m38--ui-2-creation-configuration-and-menus-650-u-range-495840) |
| **M39** | UI-3 | 51 interactive playable windows | DONE at gate level (technical closure; coverage 0.857, K7 open, no tag) | 7.50 (5.65–9.40) | `UI-JUGABLE-001` A–H | — | [§3.4](roadmap-m18-m48.md#34-m39--ui-3-the-51-playable-windows-interactive-750-u-range-565940) |
| **M40** | UI-4 | UI gate 78/78 + side-by-side + input + accessibility | DONE at gate level (technical closure 2026-10-27; 85/85, 8/8 rules; no tag) | 3.50 (2.45–4.55) | `UI-FULL-001` (85/85 resolved) | — | [§3.5](roadmap-m18-m48.md#35-m40--ui-4-the-gate-for-the-complete-ui-350-u-range-245455) |
| **M41** | SYS-1 | depth: combat, magic, items, stats, resistances | `DONE` | 63.00 (41.30–84.70) | work-package gates (§3.6) | — | [§3.6](roadmap-m18-m48.md#36-m41--sys-1-depth-in-combat-magic-items-stats-and-resistances-6300-u-range-41308470) |
| **M42** | SYS-2 | depth: NPCs, guilds, crime and law, quests | `DONE` | 42.00 (28.00–56.00) | work-package gates (§3.7) | — | [§3.7](roadmap-m18-m48.md#37-m42--sys-2-depth-in-npcs-guilds-crime-and-quests-4200-u-range-28005600) — technical closure (gate-rows, 8 gates ×2, CR-c71b453d) |
| **M43** | SYS-3 | depth: economy, travel, weather, calendar, conditions | `AUSENTE` — **in flight** under `CR-39d656e1`, first of the sequence | 51.00 (33.50–67.50) | work-package gates (§3.8) | — | [§3.8](roadmap-m18-m48.md#38-m43--sys-3-depth-in-economy-travel-weather-calendar-and-conditions-5100-u-range-33506750) |
| **M44** | DUN-1 | dungeon content and state: loot/respawn/bosses/instances, over the geometry M34 already delivered (`PROBE-DUNGEON-001`, 4,232); first act `W44-0`, the acceptance spec with golden vectors | `AUSENTE` — **next** in the sequence; its close is the hard precondition of `M-PLAY` (`P5`) and it precedes `M-SHELL` (`W44-5` writes save **v3**) | 39.00 (25.50–52.00) | work-package gates (§3.9) | — | [§3.9](roadmap-m18-m48.md#39-m44--dun-1-dungeon-content-3900-u-range-25505200) |
| **M-SHELL** | SHELL | application and session: boot, loading screen, menu, new game, creation/selection, world load, session, save/load slots, HUD, map, fast travel, pause/options, exit — the 13 capabilities of                   | `PLANNED` — gates **to build**: `SHELL-001` (composed: 13 rows × 2 processes + 13/13 negative controls) and `SHELL-002..006` (one per work package); **not a register row** (G-N13) | 17.00 (12.92–21.93) | `SHELL-001..006` — declared, 0 measured | — | [§3.9b](roadmap-m18-m48.md#39b-m-shell--shell-the-application-and-the-session-1700-u-range-12922193) |
| **M-PLAY** | PLAY | connect the whole game: doors/transitions, world↔building↔dungeon, combat↔inventory↔stats, save/load, UI hooks | `PLANNED` — gate: `PLAYTHROUGH-PROBE` `P1..P9`; work list: `the private engineering recordplayability/FUNCTIONAL-GAPS.md`; **not a register row** (G-N12) | 6.50 (**provisional**, 3.90–9.10) | `PLAYTHROUGH-PROBE` (9/9 ×2 processes + 9/9 negative controls) | — | [§3.9a](roadmap-m18-m48.md#39a-m-play--play-connect-the-whole-game-and-prove-it-end-to-end-650-u-nominal-provisional-range-390910) |
| **M45** | E4 MODS | declarative mod pipeline | **`DEFERRED`** — reason: width, not parity of the original; register verdict stays `AUSENTE`; blocked behind `M-PLAY` | 39.00 (26.00–52.00) | `MODS-001` · `MODS-002` | — | [§3.10](roadmap-m18-m48.md#310-m45--e4-declarative-mod-pipeline-3900-u-range-26005200) |
| **M46** | E5 ONLINE | optional online: lobby, instances, authority — **owner go required** | **`DEFERRED`** — reason: width, not parity of the original; register verdict stays `AUSENTE` with `blocker: OWNER-GO`; blocked behind `M-PLAY` | 78.00 (51.00–104.00) | `ONLINE-001`–`ONLINE-003` | — | [§3.11](roadmap-m18-m48.md#311-m46--e5-optional-online-7800-u-range-510010400) |
| **M47** | E6 PLATFORM | packaging, multi-OS CI, sustained perf, accessibility | **`OPTIONAL`** — entry condition: the `M-PLAY` probe passes end to end **and** the owner asks for distribution; register verdict stays `AUSENTE` | 15.50 (10.00–21.00) | `PLATFORM-001`–`PLATFORM-003` | — | [§3.12](roadmap-m18-m48.md#312-m47--e6-platform-and-distribution-1550-u-range-10002100) |
| **M48** | E7 MODERN | opt-in modern layer (pristine/modern) | **`DEFERRED`** — reason: width, not parity of the original; register verdict stays `AUSENTE`; blocked behind `M-PLAY` | 31.00 (20.00–41.00) | `MODERN-001` · `MODERN-002` | — | [§3.13](roadmap-m18-m48.md#313-m48--e7-opt-in-modern-layer-3100-u-range-20004100) |
| **E8** | deferred expansion horizon | separate program after M48, own genesis contract | `AUSENTE` / not estimable | — | own contract | — | [§3.14](roadmap-m18-m48.md#314-e8--deferred-expansion-horizon-a-separate-program-not-estimable-not-m49) |

**Totals.** The 13 milestones M36–M48 = **382.50 u** nominal (**253.40–508.55**); the table carries two
more rows: **`M-SHELL`** at **17.00 u** (12.92–21.93; §3.9b of the plan — the composition layer of the
application and the session) and **`M-PLAY`** at **6.50 u provisional** (3.90–9.10; §3.9a), so the full
vision is **406.00 u** (270.22–539.58). Written out so it can be checked: `382.50 + 17.00 + 6.50 = 406.00`
and `253.40 + 12.92 + 3.90 = 270.22` · `508.55 + 21.93 + 9.10 = 539.58`. Split by the re-baseline of
2026-10-29, with nothing deleted: **active 113.50 u** (`M43 51.00 + M44 39.00 + M-SHELL 17.00 + M-PLAY
6.50` = 113.50; 75.82–150.53) · **deferred 148.00 u** (`M45 + M46 + M48` = 39 + 78 + 31) · **optional
15.50 u** (`M47`); the three buckets sum to `113.50 + 148.00 + 15.50 = 277.00` and
`E1 24.00 + M41 63.00 + M42 42.00 + 277.00 = 406.00`, the same figure from the other side. `M-PLAY`'s
figure is **provisional by construction** — it is quoted from the open defect inventory and re-measured
when M44 closes — and both `M-SHELL` and `M-PLAY` sit **outside** the E1–E7 multipliers (§1.1 of the
plan). E8 is counted **separately**: it is a program, not a milestone, and never "M49". The UI gate closes at **M40**
(`UI-FULL-001`, 85/85 resolved: 78 renderable surfaces + 7 infrastructure rows by layer contract, 0
rows without a verdict). M34 is **`PARTIAL-TECHNICAL-CLOSURE`** today — DONE at technical
level with the named row `AUDIO-RT-001-DEVICE` — and M35 is **`PARTIAL`**, with `GAP-TAX-11` deferred
to M44. M36, M37 and M38 are the first three Plan B gates, all closed at technical level on
2026-10-27 with no tags; M39 is **DONE at gate level** — its A–H runner and probe are in the tree
since `85979b9`, the closure report records coverage 0.857 with `K7` open, and the owner signature and
tag are still pending — and M43–M48 stay **`AUSENTE`** in the register until each closes on its own gate.

---

## Campaign re-audit (M0–M29, re-measured)

The 30 milestones were re-measured on the closing campaign head by fresh QA lanes — probes and gates
re-executed, not read from the closure reports. The tally there was **21 PASS · 2 PASS-as-shipped
(M3, M6) · 7 FAIL (M2, M5, M9, M11, M12, M18, M19)**. M9's finding was the verification denominator
itself and is closed: the global verification now runs **19/19 = 1.00** with the full denominator (see
the erratum in [Status](status.md#m9--release-slice-and-global-verification)). That tally is history,
not the state of the tree: the five milestones it put below the bar are **DONE** after their repairs —
**M2** (final QA **0.98**), **M5** (final QA **0.97** plus the 40/40 DFU price parity with both gate
digests `ffc0b5e6…` and `37007115…`), **M11** (QA **0.99**), **M12** (digest `a41b36eb9bae1bab`
reproduced twice) and **M19** (`PROBE-LOCOMOTION-001` **88/88** and `PROBE-STEALTH-001` **16/16**,
twice, with NC1–NC5 red and the source digest restored) — and **M18** is **`PARTIAL-TAG-PENDING`**:
its acceptance signature is on record by delegation (2026-10-26), while the tag and the ORIGINAL-side
comparison remain the owner's. The closure register — which `scripts/closure-gate.py` checks with
`VERDICT: PASS`, 0 prose↔register contradictions and falsifiers 3/3 red — holds **51
milestones and 100 gate rows: 41 DONE · 1 PARTIAL · 1 PARTIAL-TAG-PENDING · 1 PARTIAL-TECHNICAL-CLOSURE
· 7 AUSENTE**, with `program_closure: OPEN`; `DONE` there means gate-level technical closure, never
owner acceptance. A milestone with a technical closure is not an accepted milestone; the record says
which is which.

The roadmap is deliberately narrow: finish the single-player engine and its acceptance protocol
before adding distribution channels. The platform order (Linux first, Android next, Web after that,
Windows last, macOS not planned) and the online direction are statements of intent, not dates — the
parity work is what moves each stage, and no "v1.0" is promised for either.
