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
| M18 — Acceptance | Technical closure PASS; **owner signature pending** | `PROBE-MAP-001` resolves 15,251/15,251 locations; the suite froze at 18/18 for this milestone and has grown with each later one (37/37 at the M29 close); the 21 acceptance rows were measured on the real window (**3 PASS · 6 PARTIAL · 12 NO**) and the side-by-side review against the original still has to be signed by the owner, so no `M18` tag and no `release/M18` branch exist |
| M19 — Locomotion | Technical closure PASS; owner signature and tag pending | Daggerfall Unity locomotion and the player body (`behavior::locomotion`, 21 tests) integrated into the demo and the window through `Locomotion::step`; `PROBE-LOCOMOTION-001` runs 70 checks (`STEALTH-001` 16) |
| M20–M26 — gameplay parity, first tramo | Technical closure PASS each; owner signature and tag pending | Stats and skills (M20), items/equipment/loot (M21), combat (M22), magic (M23), enchanting and alchemy (M24), monsters, enemy AI and encounters (M25), living NPCs, dialogue and rumours (M26). At the M26 close: 534 tests passed / 0 failed and the suite printed 27/27 |
| **M26 — living NPCs** | Technical closure PASS (2026-10-21); owner signature and tag pending | `behavior::npc_ai` (population 24/48/72/96, 1.6 m navgrid, `SeekingTile↔MovingForward` routine, 1/32 guards) + `behavior::dialogue` (7209/7208/7207/7206 greetings, faction topics, `TEXT.RSC` answer IDs) + `parser::rumor` (a mill over `RUMOR.DAT`: 31 records, channels, a 50/1 reservoir, expiry and a region filter); `PROBE-NPC-AI-001` / `DIALOGUE-001` / `RUMOR-001` + `NPC-DEMO-001` |
| **M27 — guilds and factions** | Technical closure PASS (2026-10-21); owner signature and tag pending | `behavior::factions` / `behavior::guild` over the `FACTION.TXT` graph (365 ids), reputation with tree propagation, ranks 0–9 and per-rank services; `PROBE-FACTION-001` (17/17), `PROBE-GUILD-001` (33/33), `M27-DEMO-001` (15/15) |
| **M28 — crime, law and courts** | Technical closure PASS (2026-10-22); owner signature and tag pending | `behavior::law` (penalty clamps, legal reputation through the M27 graph, the court and prison clock) plus the production composition layer that turns a guard request into an executable order; `PROBE-CRIME-001` and `PROBE-LAW-001` (sentence 262 steps, `5480/375 → 2740/187`, `clock_advanced=16,156,800`) |
| **M29 — playable retail quests** | Technical closure PASS (2026-10-22); owner signature and tag pending | `behavior::quests` — the `QBN` opcode interpreter, clock/time formulas, journal and lifecycle; `PROBE-QUEST-EXEC-001` (24 golden vectors, a 20-quest determinism trace), `LIFECYCLE-001`, `JOURNAL-001`; census 11/11 over 247 `.QBN` / 244 `.QRC` |
| **M30 — economy and banking** | **Frozen**: written specification, no code | Frozen by owner decision; the specification stands, nothing runs |
| **M31–M32 — travel/weather/calendar · conditions/death/rest** | **Specified, not implemented** | One written specification each; no code |
| **M33 — total savegame (v2)** | **Specified, not implemented** | Save v2 specification; no probe executed |
| **M34 — end-to-end integration + presentation** | Specified, not implemented | The E2E spine plus the full presentation pass; its acceptance probes have not run. **Closes Plan A** |
| **M35 — biomes, 2D vegetation, sky** | Specified, not implemented | **Optional annex**, non-blocking, delivered within the `0.1.0` surface. Not a gate for closing Plan A |
| **M36–M48 — Plan B (E1–E7)** | Not started | 13 milestones. Each one is listed with its scope, gate and `u` in [Plan B in detail](#plan-b-in-detail--13-milestones-m36m48--e8) below, and broken into work packages in the [complete plan](roadmap-m18-m48.md#3-atomic-work-breakdown-for-plan-b) |
| **E8 — Starfield-like horizon** | Not started; not estimable | A **separate program** after M48, with its own genesis contract. Never "M49" |
| After M18 — content packs | Design | Declarative packs, a manifest with dependencies, load order, per-pack settings; no code execution |
| Android | Next platform, not started | Free distribution is the point: APK, F-Droid and plain sideloading, offline and online, phone-first interface and performance envelope. No build has been validated |
| Web / WASM | Exploratory, after Android | Bevy's WebGL2 path; bounded by the renderer and by the data requirement. Not on the critical path |
| Windows | Future, planned last | Portable by design, no build validated. Planned after the platforms above; not abandoned |
| macOS | Not planned | Not on the roadmap at this time |
| Online direction | Single-player today; the rest is research notes | The long-term direction is online-capable: optimisation for many concurrent users, more user control and larger world scale, recalibrating toward an MMO feel. Sharding, area-of-interest and replication are notes only — none implemented. Raids, dungeons and endless game modes are under exploration: a direction, not a commitment |

## Plan B in detail — 13 milestones M36–M48 + E8

All 13 milestones and E8 are **NOT STARTED**: specifications live in the [complete plan](roadmap-m18-m48.md), no code,
no dispatch, no tag. `u` is a nominal estimate with its rounding range (§3.15 of the plan); no dates.

| M | Epic | Scope | State | `u` nominal (range) | Gate | Plan |
|:-:|:---|:---|:---|--:|:---|:-:|
| **M36** | UI-0 | audit of the 85 windows + patch/rewrite decision + compatibility contract + E1 re-estimate | NOT STARTED | 2.00 (1.60–2.40) | `UI-AUDIT-001` A+B+C | [§3.1](roadmap-m18-m48.md#31-m36--ui-0-measure-before-touching-anything-200-u-range-160240) |
| **M37** | UI-1 | UI compatibility layer (7 axes) | NOT STARTED | 4.50 (3.45–5.60) | `UI-BASE-001` A–E + `UI-INPUT-001` (3 pilot windows) | [§3.2](roadmap-m18-m48.md#32-m37--ui-1-the-compatibility-layer-450-u-range-345560) |
| **M38** | UI-2 | 16 creation + 11 configuration + menus (27 surfaces) | NOT STARTED | 6.50 (4.95–8.40) | `UI-CREATE-001` (16/16) · `UI-CONFIG-001` (11/11) | [§3.3](roadmap-m18-m48.md#33-m38--ui-2-creation-configuration-and-menus-650-u-range-495840) |
| **M39** | UI-3 | 51 interactive playable windows | NOT STARTED | 7.50 (5.65–9.40) | `UI-JUGABLE-001` A–H | [§3.4](roadmap-m18-m48.md#34-m39--ui-3-the-51-playable-windows-interactive-750-u-range-565940) |
| **M40** | UI-4 | UI gate 78/78 + side-by-side + input + accessibility | NOT STARTED | 3.50 (2.45–4.55) | `UI-FULL-001` (85/85 resolved) | [§3.5](roadmap-m18-m48.md#35-m40--ui-4-the-gate-for-the-complete-ui-350-u-range-245455) |
| **M41** | SYS-1 | depth: combat, magic, items, stats, resistances | NOT STARTED | 63.00 (41.30–84.70) | work-package gates (§3.6) | [§3.6](roadmap-m18-m48.md#36-m41--sys-1-depth-in-combat-magic-items-stats-and-resistances-6300-u-range-41308470) |
| **M42** | SYS-2 | depth: NPCs, guilds, crime and law, quests | NOT STARTED | 42.00 (28.00–56.00) | work-package gates (§3.7) | [§3.7](roadmap-m18-m48.md#37-m42--sys-2-depth-in-npcs-guilds-crime-and-quests-4200-u-range-28005600) |
| **M43** | SYS-3 | depth: economy, travel, weather, calendar, conditions | NOT STARTED | 51.00 (33.50–67.50) | work-package gates (§3.8) | [§3.8](roadmap-m18-m48.md#38-m43--sys-3-depth-in-economy-travel-weather-calendar-and-conditions-5100-u-range-33506750) |
| **M44** | DUN-1 | dungeon content: grid + loot/respawn/bosses/instances | NOT STARTED | 39.00 (25.50–52.00) | work-package gates (§3.9) | [§3.9](roadmap-m18-m48.md#39-m44--dun-1-dungeon-content-3900-u-range-25505200) |
| **M45** | E4 MODS | declarative mod pipeline | NOT STARTED | 39.00 (26.00–52.00) | `MODS-001` · `MODS-002` | [§3.10](roadmap-m18-m48.md#310-m45--e4-declarative-mod-pipeline-3900-u-range-26005200) |
| **M46** | E5 ONLINE | optional online: lobby, instances, authority — **owner go required** | NOT STARTED | 78.00 (51.00–104.00) | `ONLINE-001`–`ONLINE-003` | [§3.11](roadmap-m18-m48.md#311-m46--e5-optional-online-7800-u-range-510010400) |
| **M47** | E6 PLATFORM | packaging, multi-OS CI, sustained perf, accessibility | NOT STARTED | 15.50 (10.00–21.00) | `PLATFORM-001`–`PLATFORM-003` | [§3.12](roadmap-m18-m48.md#312-m47--e6-platform-and-distribution-1550-u-range-10002100) |
| **M48** | E7 MODERN | opt-in modern layer (pristine/modern) | NOT STARTED | 31.00 (20.00–41.00) | `MODERN-001` · `MODERN-002` | [§3.13](roadmap-m18-m48.md#313-m48--e7-opt-in-modern-layer-3100-u-range-20004100) |
| **E8** | Starfield-like | separate program after M48, own genesis contract | NOT STARTED / not estimable | — | own contract | [§3.14](roadmap-m18-m48.md#314-e8--starfield-like-horizon-a-separate-program-not-estimable-not-m49) |

**Totals.** 13 milestones M36–M48 = **382.50 u** nominal (**253.40–508.55**). E8 is counted
**separately**: it is a program, not a milestone, and never "M49". M36 closes Plan A's UI gate only when
`UI-FULL-001` prints 85/85; Plan A itself already closes at **M34 PASS**, with M35 as a non-blocking annex.

---

## Campaign re-audit (M0–M29, re-measured)

The 30 milestones were re-measured on the closing campaign head by fresh QA lanes — probes and gates
re-executed, not read from the closure reports. The tally there was **21 PASS · 2 PASS-as-shipped
(M3, M6) · 7 FAIL (M2, M5, M9, M11, M12, M18, M19)**. M9's finding was the verification denominator
itself and is closed: the global verification now runs **19/19 = 1.00** with the full denominator (see
the erratum in [Status](status.md#m9--release-slice-and-global-verification)). Six milestones remain
below the acceptance bar today: **M2, M5, M11, M12 and M19** on named parity defects, and **M18**
because its closure is the owner's signature. A milestone with a technical closure is not an accepted
milestone; the record says which is which.

The roadmap is deliberately narrow: finish the single-player engine and its acceptance protocol
before adding distribution channels. The platform order (Linux first, Android next, Web after that,
Windows last, macOS not planned) and the online direction are statements of intent, not dates — the
parity work is what moves each stage, and no "v1.0" is promised for either.
