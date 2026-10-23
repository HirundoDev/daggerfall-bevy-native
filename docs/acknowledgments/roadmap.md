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
| **M30–M32 — gameplay parity, third tramo** | Technical closure PASS each; owner signature and tag pending | Economy and banking (M30), travel/weather/calendar (M31), conditions/death/rest (M32), each with its probes |
| **M33 — total savegame (v2)** | In flight | Save v2 checkpoint; no probe executed yet |
| **M34 — end-to-end integration + presentation** | Specified, not implemented | The E2E spine plus the full presentation pass; its acceptance probes have not run. **Closes Plan A** |
| **M35 — biomes, 2D vegetation, sky** | Specified, not implemented | **Optional annex**, non-blocking, delivered within the `0.1.0` surface. Not a gate for closing Plan A |
| **M36–M48 — Plan B (E1–E7)** | Not started | Full UI (M36–M40), system depth (M41–M43), dungeon content (M44), declarative mods (M45), optional online with an owner go (M46), platform and distribution (M47), opt-in modern layer (M48). See [the plan](roadmap-m18-m48.md) |
| **E8 — Starfield-like horizon** | Not started; not estimable | A **separate program** after M48, with its own genesis contract. Never "M49" |
| After M18 — content packs | Design | Declarative packs, a manifest with dependencies, load order, per-pack settings; no code execution |
| Android | Next platform, not started | Free distribution is the point: APK, F-Droid and plain sideloading, offline and online, phone-first interface and performance envelope. No build has been validated |
| Web / WASM | Exploratory, after Android | Bevy's WebGL2 path; bounded by the renderer and by the data requirement. Not on the critical path |
| Windows | Future, planned last | Portable by design, no build validated. Planned after the platforms above; not abandoned |
| macOS | Not planned | Not on the roadmap at this time |
| Online direction | Single-player today; the rest is research notes | The long-term direction is online-capable: optimisation for many concurrent users, more user control and larger world scale, recalibrating toward an MMO feel. Sharding, area-of-interest and replication are notes only — none implemented. Raids, dungeons and endless game modes are under exploration: a direction, not a commitment |

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
