# Roadmap

*Case file split from [`docs/ACKNOWLEDGMENTS.md`](../ACKNOWLEDGMENTS.md) — split mechanically; only relative link targets and heading levels were rebased.*

> **The complete plan lives elsewhere.** This page is the stage-by-stage public summary. The single
> canonical plan — M18 to M48 plus E8, with the atomic work breakdown, gates, dependencies, risks and
> closure criteria — is [Roadmap M18–M48 + E8](roadmap-m18-m48.md). The operating rules for contributors
> and agents (source of truth, real state, limits, how to continue) are in the
> [Agent entry point](agent-entrypoint.md).

| Stage | State | What closes it |
|---|---|---|
| M0–M17 | Closed and tagged | Annotated tags `M<n>-YYYY-MM-DD` with matching `release/M<n>` branches (`M13b` is a tagged parity follow-up) |
| M18 — Acceptance | **`PARTIAL-TAG-PENDING`** — DONE at technical level; **acceptance signature recorded 2026-10-26 by delegation**; tag none | `PROBE-MAP-001` resolves 15,251/15,251 locations; the suite froze at 18/18 for this milestone and has grown with each later one (37/37 at the M29 close); the 21 acceptance rows were measured on the real window and signed **8 PASS · 9 NC-ACEPTA · 4 NOT-COMPARABLE · 0 FAIL** — `NC-ACEPTA` is acceptance with a declared non-conformity, `NOT-COMPARABLE` a declared class, not a downgraded pass. No `M18` tag and no `release/M18` branch exist: the tag remains a separate later act of the owner. The **ORIGINAL side is not certified** (Daggerfall 1.07.213 in DOSBox): comparing the engine rows against the original is still the owner's own act |
| M19 — Locomotion | DONE at gate level; acceptance pending, no tag | Daggerfall Unity locomotion and the player body (`behavior::locomotion`, 21 tests) integrated into the demo and the window through `Locomotion::step`; `PROBE-LOCOMOTION-001` **88/88** (stdout 11,904 B, sha256 `9de0cfdd7fe8d74818d9bb87618beca0ef7a9aabcf3b65c6438433c85a937b52`) and `PROBE-STEALTH-001` **16/16** (1,501 B, sha256 `a59c64550da912765902962635374af84128d82adab63410844de1f7134a9d43`), each run in **two separate OS processes byte-identical**; NC1–NC5 each turned its expected rows red (exit 1) and the source sha256 was restored to `c0e34bb2…` — `docs/sintergic/l5-mission/m19-closure-CR-44034827.md` |
| M20–M26 — gameplay parity, first tramo | DONE at gate level; acceptance pending, no tag | Stats and skills (M20), items/equipment/loot (M21), combat (M22), magic (M23), enchanting and alchemy (M24), monsters, enemy AI and encounters (M25), living NPCs, dialogue and rumours (M26). At the M26 close: 534 tests passed / 0 failed and the suite printed 27/27. Re-measured in `CR-c62fc6d3`: their gate rows ran in **two separate OS processes byte-identical**, binary `target/debug/daggerfall-app` sha256 `f0b9ba86…` at tree `e9e7908`, plus the negative control of each milestone turned red and restored — `docs/sintergic/l5-mission/evidence/re-measure-29-CR-c62fc6d3/manifest.json` |
| **M26 — living NPCs** | DONE at gate level (closed 2026-10-21); acceptance pending, no tag | `behavior::npc_ai` (population 24/48/72/96, 1.6 m navgrid, `SeekingTile↔MovingForward` routine, 1/32 guards) + `behavior::dialogue` (7209/7208/7207/7206 greetings, faction topics, `TEXT.RSC` answer IDs) + `parser::rumor` (a mill over `RUMOR.DAT`: 31 records, channels, a 50/1 reservoir, expiry and a region filter); `PROBE-NPC-AI-001` / `DIALOGUE-001` / `RUMOR-001` + `NPC-DEMO-001`. Re-measured in `CR-c62fc6d3`: its three gate rows ran in two separate OS processes byte-identical, binary sha256 `f0b9ba86…` at tree `e9e7908` |
| **M27 — guilds and factions** | DONE at gate level (closed 2026-10-21); acceptance pending, no tag | `behavior::factions` / `behavior::guild` over the `FACTION.TXT` graph (365 ids), reputation with tree propagation, ranks 0–9 and per-rank services; `PROBE-FACTION-001` (17/17), `PROBE-GUILD-001` (33/33), `M27-DEMO-001` (15/15). Re-measured in `CR-c62fc6d3`: its two gate rows ran in two separate OS processes byte-identical, binary sha256 `f0b9ba86…` at tree `e9e7908` |
| **M28 — crime, law and courts** | DONE at gate level (closed 2026-10-22); acceptance pending, no tag | `behavior::law` (penalty clamps, legal reputation through the M27 graph, the court and prison clock) plus the production composition layer that turns a guard request into an executable order; `PROBE-CRIME-001` and `PROBE-LAW-001` (sentence 262 steps, `5480/375 → 2740/187`, `clock_advanced=16,156,800`). Re-measured in `CR-c62fc6d3`: its two gate rows ran in two separate OS processes byte-identical, binary sha256 `f0b9ba86…` at tree `e9e7908` |
| **M29 — playable retail quests** | DONE at gate level (closed 2026-10-22); acceptance pending, no tag | `behavior::quests` — the `QBN` opcode interpreter, clock/time formulas, journal and lifecycle; `PROBE-QUEST-EXEC-001` (24 golden vectors, a 20-quest determinism trace), `LIFECYCLE-001`, `JOURNAL-001`; census 11/11 over 247 `.QBN` / 244 `.QRC`. Re-measured in `CR-c62fc6d3`: its gate row ran in two separate OS processes byte-identical, binary sha256 `f0b9ba86…` at tree `e9e7908` |
| **M30 — economy and banking** | Technical closure PASS (2026-10-23); owner signature and tag pending | `behavior::economy` — `CalculateCost` with regional prices, the 36 `(building, group)` buy table, five trade modes, 53 holidays, loans and default, house and ship (`BankLedger`, `LoanEvent`); `PROBE-SHOP-001`, `PROBE-ECONOMY-001`, `PROBE-BANK-001`. Implemented at technical level: its consumer is the probe, not the demo window |
| **M31 — travel, weather and calendar** | Technical closure PASS (2026-10-23); owner signature and tag pending | `heart::travel` / `heart::weather` / `heart::calendar`; `PROBE-TRAVEL-001` (34/34 vectors, 2,200-pixel route over the real `CLIMATE.PAK`, byte-identical stdout in two processes), `PROBE-WEATHER-001` (13/13, 240,000 draws, `max_abs_delta = 0.0121`), `PROBE-CALENDAR-001` (14/14). Probe-backed, not yet surfaced in the demo window |
| **M32 — conditions, death and rest** | Technical closure PASS (2026-10-23); owner signature and tag pending | `skills::conditions` and `skills::rest` — diseases, vampirism and lycanthropy stages, rest and the death state; `PROBE-CONDITION-001` (24/24), `PROBE-VAMPIRE-001` (16/16), `PROBE-REST-001` (10/10), `PROBE-DEATH-STATE-001` (2/2). Probe-backed, not yet surfaced in the demo window |
| **M33 — total savegame** | Technical closure PASS (2026-10-26); owner signature and tag pending | `memory::save_v2` / `memory::savegame` / `memory::savegame_v2` — savegame v2, v1→v2 migration and the classic `SAVEGAME` read/write; `PROBE-SAVE-ALL-001` (24/24 byte-identical round trips, five corruption mutations rejected, deterministic digest), `PROBE-SAVE-MIGRATION-001`, `PROBE-SAVEGAME-READ-001`, `PROBE-SAVEGAME-WRITE-001`, `PROBE-M33-DEMO-001` |
| **M34 — end-to-end integration + presentation** | **`PARTIAL-TECHNICAL-CLOSURE`** — DONE at technical level with the named row `AUDIO-RT-001-DEVICE`; acceptance pending, no tag | The E2E spine, the full presentation pass and the HUD, measured by `PLAYSESSION-001`, `UI-FULL-001` and `AUDIO-RT-001` (`ALL-001` **40/40**) with `SPINE-001` **12/12**. `AUDIO-RT-001` reproduces `PASS(degraded)` — `[SKIP-DEVICE]`, the binary built without the `audio` feature — so its measure covers the degraded path only and the audio branch is the owner's decision. `UI-FULL-001`, also the M40 gate, counts the 34 non-playable surfaces as `N/A-engine` entries of its own census and measures only the 51 playable windows (51/51): it neither depends on those 34 nor certifies them |
| **M35 — biomes, 2D vegetation, sky** | **`PARTIAL`** — the named row `GAP-TAX-11` is **deferred to M44**; no tag | **Optional annex**, non-blocking: `BIOME-001` **25 passed / 0 failed / 1 NA** and `SPRITE-001` **11/11**. The `NA` is that declared gap: the pinned corpus carries no tile-class data, so the probability gate cannot be measured and the probe returns `DEGRADED` instead of inventing a number. Not a gate for closing Plan A |
| **M36–M38 — Plan B, UI-0, UI-1 and UI-2** | Technical closure measured 2026-10-27; owner signature and tag pending | `UI-AUDIT-001` A+B+C recomputes the census from the code (`VERDICT: PASS`, two processes, identical stdout): **85 = 78 surfaces + 7 infrastructure**, measured **4 implemented / 47 partial / 34 absent**, rule **35 PATCH · 50 REWRITE**; then the seven-axis layer with `UI-BASE-001` **9/9** and `UI-INPUT-001` **10/10** over **3 pilot windows**, measuring **0.4708333 u/window at one QA round** (band `r`) against an L1 baseline of **0.0565 u/window**; then **M38 DONE** — `UI-CREATE-001` (**16 surfaces**) and `UI-CONFIG-001` (**11 surfaces**) — at requirements coverage **0.857 (6/7)**, with **K7** (the owner's side-by-side) the only open item |
| **M39–M48 — Plan B, remaining (E1–E7)** | State: M39 DONE at gate level (coverage 0.857, K7 open, no tag); M40–M48 `AUSENTE` | 10 milestones. Each one is listed with its scope, gate and `u` in [Plan B in detail](#plan-b-in-detail--13-milestones-m36m48--e8) below, and broken into work packages in the [complete plan](roadmap-m18-m48.md#3-atomic-work-breakdown-for-plan-b) |
| **E8 — deferred expansion horizon** | `AUSENTE`; not estimable | A **separate program** after M48, with its own genesis contract. Never "M49" |
| After M18 — content packs | Design | Declarative packs, a manifest with dependencies, load order, per-pack settings; no code execution |
| Android | Next platform, not started | Free distribution is the point: APK, F-Droid and plain sideloading, offline and online, phone-first interface and performance envelope. No build has been validated |
| Web / WASM | Exploratory, after Android | Bevy's WebGL2 path; bounded by the renderer and by the data requirement. Not on the critical path |
| Windows | Future, planned last | Portable by design, no build validated. Planned after the platforms above; not abandoned |
| macOS | Not planned | Not on the roadmap at this time |
| Online direction | Single-player today; the rest is research notes | The long-term direction is online-capable: optimisation for many concurrent users, more user control and larger world scale, recalibrating toward an MMO feel. Sharding, area-of-interest and replication are notes only — none implemented. Raids, dungeons and endless game modes are under exploration: a direction, not a commitment |

## Plan B in detail — 13 milestones M36–M48 + E8

M40–M48 and E8 are **`AUSENTE`** in the closure register — specifications live in the
[complete plan](roadmap-m18-m48.md), no code for them, no dispatch, no tag. M39 is **DONE at gate
level** (coverage 0.857, **K7** open, no tag). M36, M37 and M38 closed at technical level on
2026-10-27 — the gates are in the table below — and they carry no milestone tag either. `u` is a
nominal estimate with its rounding range (§3.15 of the plan); no dates.

| M | Epic | Scope | State | `u` nominal (range) | Gate | HH-1 | Plan |
|:-:|:---|:---|:---|--:|:---|:---|:-:|
| **M36** | UI-0 | audit of the 85 windows + patch/rewrite decision + compatibility contract + E1 re-estimate | DONE (technical closure 2026-10-27, no tag) | 2.00 (1.60–2.40) | `UI-AUDIT-001` A+B+C | `HH-20261027-og-orquestador-m36-ui0-audit` | [§3.1](roadmap-m18-m48.md#31-m36--ui-0-measure-before-touching-anything-200-u-range-160240) |
| **M37** | UI-1 | UI compatibility layer (7 axes) | DONE (technical closure 2026-10-27, no tag) | 4.50 (3.45–5.60) | `UI-BASE-001` A–E + `UI-INPUT-001` (3 pilot windows) | `HH-20261027-og-orquestador-m37-ui1` | [§3.2](roadmap-m18-m48.md#32-m37--ui-1-the-compatibility-layer-450-u-range-345560) |
| **M38** | UI-2 | 16 creation + 11 configuration + menus (27 surfaces) | DONE (technical closure 2026-10-27, no tag; coverage 0.857 — 6/7, K7 open) | 6.50 (4.95–8.40) | `UI-CREATE-001` (16/16) · `UI-CONFIG-001` (11/11) | `HH-20261027-og-l1-70-quality-meta-m38-verify` | [§3.3](roadmap-m18-m48.md#33-m38--ui-2-creation-configuration-and-menus-650-u-range-495840) |
| **M39** | UI-3 | 51 interactive playable windows | DONE at gate level (technical closure; coverage 0.857, K7 open, no tag) | 7.50 (5.65–9.40) | `UI-JUGABLE-001` A–H | — | [§3.4](roadmap-m18-m48.md#34-m39--ui-3-the-51-playable-windows-interactive-750-u-range-565940) |
| **M40** | UI-4 | UI gate 78/78 + side-by-side + input + accessibility | `AUSENTE` | 3.50 (2.45–4.55) | `UI-FULL-001` (85/85 resolved) | — | [§3.5](roadmap-m18-m48.md#35-m40--ui-4-the-gate-for-the-complete-ui-350-u-range-245455) |
| **M41** | SYS-1 | depth: combat, magic, items, stats, resistances | `AUSENTE` | 63.00 (41.30–84.70) | work-package gates (§3.6) | — | [§3.6](roadmap-m18-m48.md#36-m41--sys-1-depth-in-combat-magic-items-stats-and-resistances-6300-u-range-41308470) |
| **M42** | SYS-2 | depth: NPCs, guilds, crime and law, quests | `AUSENTE` | 42.00 (28.00–56.00) | work-package gates (§3.7) | — | [§3.7](roadmap-m18-m48.md#37-m42--sys-2-depth-in-npcs-guilds-crime-and-quests-4200-u-range-28005600) |
| **M43** | SYS-3 | depth: economy, travel, weather, calendar, conditions | `AUSENTE` | 51.00 (33.50–67.50) | work-package gates (§3.8) | — | [§3.8](roadmap-m18-m48.md#38-m43--sys-3-depth-in-economy-travel-weather-calendar-and-conditions-5100-u-range-33506750) |
| **M44** | DUN-1 | dungeon content: grid + loot/respawn/bosses/instances | `AUSENTE` | 39.00 (25.50–52.00) | work-package gates (§3.9) | — | [§3.9](roadmap-m18-m48.md#39-m44--dun-1-dungeon-content-3900-u-range-25505200) |
| **M45** | E4 MODS | declarative mod pipeline | `AUSENTE` | 39.00 (26.00–52.00) | `MODS-001` · `MODS-002` | — | [§3.10](roadmap-m18-m48.md#310-m45--e4-declarative-mod-pipeline-3900-u-range-26005200) |
| **M46** | E5 ONLINE | optional online: lobby, instances, authority — **owner go required** | `AUSENTE` | 78.00 (51.00–104.00) | `ONLINE-001`–`ONLINE-003` | — | [§3.11](roadmap-m18-m48.md#311-m46--e5-optional-online-7800-u-range-510010400) |
| **M47** | E6 PLATFORM | packaging, multi-OS CI, sustained perf, accessibility | `AUSENTE` | 15.50 (10.00–21.00) | `PLATFORM-001`–`PLATFORM-003` | — | [§3.12](roadmap-m18-m48.md#312-m47--e6-platform-and-distribution-1550-u-range-10002100) |
| **M48** | E7 MODERN | opt-in modern layer (pristine/modern) | `AUSENTE` | 31.00 (20.00–41.00) | `MODERN-001` · `MODERN-002` | — | [§3.13](roadmap-m18-m48.md#313-m48--e7-opt-in-modern-layer-3100-u-range-20004100) |
| **E8** | deferred expansion horizon | separate program after M48, own genesis contract | `AUSENTE` / not estimable | — | own contract | — | [§3.14](roadmap-m18-m48.md#314-e8--deferred-expansion-horizon-a-separate-program-not-estimable-not-m49) |

**Totals.** 13 milestones M36–M48 = **382.50 u** nominal (**253.40–508.55**). E8 is counted
**separately**: it is a program, not a milestone, and never "M49". The UI gate closes at **M40**
(`UI-FULL-001`, 78/78 resolved). M34 is **`PARTIAL-TECHNICAL-CLOSURE`** today — DONE at technical
level with the named row `AUDIO-RT-001-DEVICE` — and M35 is **`PARTIAL`**, with `GAP-TAX-11` deferred
to M44. M36, M37 and M38 are the first three Plan B gates, all closed at technical level on
2026-10-27 with no tags; M39 is **DONE at gate level** — its A–H runner and probe are in the tree
since `85979b9`, the closure report records coverage 0.857 with `K7` open, and the owner signature and
tag are still pending — and M40–M48 are **`AUSENTE`**.

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
milestones and 80 gate rows: 38 DONE · 1 PARTIAL · 1 PARTIAL-TAG-PENDING · 1 PARTIAL-TECHNICAL-CLOSURE
· 10 AUSENTE**, with `program_closure: OPEN`; `DONE` there means gate-level technical closure, never
owner acceptance. A milestone with a technical closure is not an accepted milestone; the record says
which is which.

The roadmap is deliberately narrow: finish the single-player engine and its acceptance protocol
before adding distribution channels. The platform order (Linux first, Android next, Web after that,
Windows last, macOS not planned) and the online direction are statements of intent, not dates — the
parity work is what moves each stage, and no "v1.0" is promised for either.
