# Roadmap

*Case file split from [`docs/ACKNOWLEDGMENTS.md`](../ACKNOWLEDGMENTS.md) — split mechanically; only relative link targets and heading levels were rebased.*

| Stage | State | What closes it |
|---|---|---|
| M0–M17 | Closed and tagged | Release branches `release/M0` … `release/M17` |
| M18 — Acceptance | Technical closure PASS; owner signature pending | `PROBE-MAP-001` resolves 15,251/15,251 locations; the suite froze at 18/18 for this milestone and has grown with each later one (27/27 at the M26 close); release scans clean; the side-by-side review against the original still has to be signed by the owner, so no `M18` tag exists |
| M19 — Locomotion | Technical closure PASS; owner signature and tag pending | Daggerfall Unity locomotion and the player body (`behavior::locomotion`, 21 tests) integrated into the demo and the window through `Locomotion::step`; `PROBE-LOCOMOTION-001` runs 70 checks (`STEALTH-001` 16) |
| M20–M26 — gameplay parity, first tramo | Technical closure PASS each; owner signature and tag pending | Stats and skills (M20), items/equipment/loot (M21), combat (M22), magic (M23), enchanting and alchemy (M24), monsters, enemy AI and encounters (M25), living NPCs, dialogue and rumours (M26). At the M26 close: 534 tests passed / 0 failed and the suite prints 27/27 |
| **M26 — living NPCs** | Technical closure PASS (2026-10-21); owner signature and tag pending | `behavior::npc_ai` (population 24/48/72/96, 1.6 m navgrid, `SeekingTile↔MovingForward` routine, 1/32 guards) + `behavior::dialogue` (7209/7208/7207/7206 greetings, faction topics, `TEXT.RSC` answer IDs) + `parser::rumor` (a mill over `RUMOR.DAT`: 31 records, channels, a 50/1 reservoir, expiry and a region filter); `PROBE-NPC-AI-001` / `DIALOGUE-001` / `RUMOR-001` + `NPC-DEMO-001` |
| **M27–M34 — gameplay parity, second tramo** | Specified, not implemented | Guilds and factions, crime/law/courts, playable retail quests, full economy and banking, travel/weather/calendar, conditions/death/rest, total savegame, end-to-end integration. One written specification each, no code |
| After M18 — content packs | Design | Declarative packs, a manifest with dependencies, load order, per-pack settings; no code execution |
| Android | Next platform, not started | Free distribution is the point: APK, F-Droid and plain sideloading, offline and online, phone-first interface and performance envelope. No build has been validated |
| Web / WASM | Exploratory, after Android | Bevy's WebGL2 path; bounded by the renderer and by the data requirement. Not on the critical path |
| Windows | Future, planned last | Portable by design, no build validated. Planned after the platforms above; not abandoned |
| macOS | Not planned | Not on the roadmap at this time |
| Online direction | Single-player today; the rest is research notes | The long-term direction is online-capable: optimisation for many concurrent users, more user control and larger world scale, recalibrating toward an MMO feel. Sharding, area-of-interest and replication are notes only — none implemented. Raids, dungeons and endless game modes are under exploration: a direction, not a commitment |

The roadmap is deliberately narrow: finish the single-player engine and its acceptance protocol
before adding distribution channels. The platform order (Linux first, Android next, Web after that,
Windows last, macOS not planned) and the online direction are statements of intent, not dates — the
parity work is what moves each stage, and no "v1.0" is promised for either.
