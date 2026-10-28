# daggerfall-bevy-native

A data-driven, first-person 2.5D RPG engine written in Rust on top of Bevy, built to reproduce the
look, rules and world of **Daggerfall** from the original retail data files — and to verify that
reproduction with golden vectors instead of screenshots.

The engine itself contains **no game data**. It reads a copy of Daggerfall that you already own and
keep outside this repository. See [Data, legal and trademarks](docs/acknowledgments/legal-and-trademarks.md).

> **Release status (honest): `v0.3.0-alpha`, published 2026-10-27.** `v0.3.1-alpha` is in
> preparation and is a **documentation correction only** — it fixes this page and the record it points
> at and adds no feature. The closure register carries the state, and it is the only authority quoted
> here — `docs/sintergic/validation/closure-register-CR-03390267.md`, which `scripts/closure-gate.py` reports with `VERDICT: PASS` (0 prose↔register contradictions,
> falsifiers 3/3 red): M0–M17 **DONE**
> and tagged; M18 **`PARTIAL-TAG-PENDING`**; M19–M33 **DONE** at gate level; M34
> **`PARTIAL-TECHNICAL-CLOSURE`** with the named row `AUDIO-RT-001-DEVICE`; M35
> **`PARTIAL`** with `GAP-TAX-11` deferred to M44; M36–M39 **DONE** at gate level; M40–M48
> **`AUSENTE`**; E8 **`AUSENTE`**. Everywhere on this page `DONE` means **gate-level technical
> closure, never owner acceptance**. M18 — the global acceptance gate
> — has its **acceptance signature recorded on 2026-10-26 by delegation**: its 21 acceptance rows are
> signed **8 PASS · 9 NC-ACEPTA · 4 NOT-COMPARABLE · 0 FAIL**, and `NOT-COMPARABLE` is a declared class,
> not a downgraded pass. Two acts remain the owner's own: running the original (Daggerfall 1.07.213 in
> DOSBox) for the left column of the side-by-side, and cutting the **`M18` tag**. So no release carries
> an **`M18` tag**, none makes a claim of full visual parity with the original game — the signature
> certifies the engine's measured rows, not that it looks like 1996 — and the **0.0.0** release line
> ("living country, fully offline") remains in progress. A technical closure is not an accepted
> milestone, and this README does not blur the two.

## What `v0.3.0-alpha` means

This is the current public pre-release. The mechanism is playable end to end and every claim below is
backed by an executable probe or golden vector, but the acceptance gates are not yet the owner's own
act — M18's is recorded by delegation, the later ones are unsigned — and part of the presentation is
not at parity. Concretely:

- **In the alpha:** the retail data layer (M10–M18, including the full-map index over all 15,251
  locations); the gameplay organs from M19–M29 — locomotion, character, items, combat, magic,
  enchanting, monsters, NPCs, factions, law and quests — plus M30–M33: economy and banking, travel with
  weather and calendar, conditions/death/rest and the total savegame (v2 and the classic `SAVEGAME`);
  the 1996-style UI shell; save/load; and the probe/CI harness that measures all of it.
- **Repaired and DONE:** M2 (final QA **0.98**); M5 (final QA **0.97**, plus **40/40** golden parity
  vectors against the pinned DFU `FormulaHelper.cs`, with `FORMULA-001` sha256 `ffc0b5e6…` and
  `COMMERCE-001` sha256 `37007115…` identical in two separate processes); M11 (QA **0.99**); M12
  (digest `a41b36eb9bae1bab` reproduced twice); M19 — `LOCOMOTION-001` **88/88** sha256 `9de0cfdd…`
  and `STEALTH-001` **16/16** sha256 `a59c6455…`, both measured twice, with NC1–NC5 each driven red
  and restored. Reports under `docs/sintergic/l5-mission/`: `final-qa-M2M5M19-CR-380233e5.md`,
  `mission-report-m11-CR-90853661.md`, `mission-report-fix-m12-minors-CR-d54b4324.md`,
  `m5-calculate-cost-parity-CR-112b1058.md`, `m19-closure-CR-44034827.md`. M18 is not in that list:
  its 21 acceptance rows were **signed by delegation on 2026-10-26** with
  **0 FAIL** (**8 PASS · 9 NC-ACEPTA · 4 NOT-COMPARABLE**), and what remains is the owner's comparison
  against the original and the tag. The M2–M19 tally is published in
  [Status](docs/acknowledgments/status.md#final-re-audit-of-the-campaign-m0m29).
- **M34 and M35, with their register labels:** M34 (end-to-end integration and presentation) is
  **`PARTIAL-TECHNICAL-CLOSURE`**, its named row `AUDIO-RT-001-DEVICE` still open — the audio branch is
  the owner's decision; M35 (biomes, vegetation and sky) is **`PARTIAL`**, with the named row
  `GAP-TAX-11` deferred to M44. The UI plan's first three gates — M36 (the census audit), M37 (the
  seven-axis compatibility layer) and M38 (the 27 creation and configuration surfaces) — are **DONE at
  gate level**. Each has its gate and none is composed into the running demo window yet, so the
  playable surface is still the M18-era window.
- **DONE at gate level (M39):** the 51 interactive playable windows, at coverage **0.857**
  with K7 (the owner's side-by-side acceptance) as the only open clause. Not signed, no tag, and the
  windows are not yet the running demo window.
- **Owner acts still open:** the side-by-side against the *original* game — left column, Daggerfall
  1.07.213 in DOSBox — and the milestone tag, for M18 and for every later presentation closure.
  Accepting the engine's own measured rows is recorded; comparing them with the original is not.
- **No parity claim:** nothing here should be read as "visually identical to Daggerfall", "feature
  complete" or "release quality". What is verified is the mechanism, probe by probe.

## Status: what is actually done

M0–M17 are closed and tagged. M18 passed its **technical** closure on 2026-10-20 (`PROBE-MAP-001`
resolves all **15,251** locations of `MAPS.BSA`); its 21 acceptance rows were measured on the real
window and **signed by delegation on 2026-10-26** — **8 PASS · 9 NC-ACEPTA · 4 NOT-COMPARABLE · 0
FAIL** — with the comparison against the original game and the tag still the owner's. M19–M29 each
closed at technical level, each adding its probes to the suite; the 29 gate rows of M20–M29 and M34
were then re-derived from the live tree at `e9e7908` (CR-c62fc6d3), each row run in two separate OS
processes with byte-identical stdout and equal sha256, plus 11 negative controls mutated red and
restored — manifest
`docs/sintergic/l5-mission/evidence/re-measure-29-CR-c62fc6d3/manifest.json`. The
**854 tests passed / 0 failed** figure belongs to the campaign re-audit base `6d48146` (36 test
binaries, Δ 0 against the M9 close); it is not a count of this head. Clippy was clean for both
headless and `--features window`, the content/trademark scans are
0/0/0, the CI runner is **PASS (3/3)** and the anti-screen probe suite prints **37/37**. The global
verification of M9 re-measures **19/19 = 1.00** with its full denominator (the earlier `15/15` claim
excluded four critical rows; the erratum is recorded in [Status](docs/acknowledgments/status.md)).

**The suite today, in the tree that ships.** The public export of `e4c4869` — **516 files**, tree
`07bcce77…`, export commit `7663f5e` — prints `cargo test --workspace` **1,528 passed / 0 failed / 3
ignored** across **59** test binaries. The private workspace at `137e30a` prints **1,606 passed / 0
failed / 3 ignored** across **66**. The **78**-test delta is the set of test files excluded from the
export, declared in [Export limits](docs/acknowledgments/export-limits.md), which names them, counts
their `#[test]` attributes and explains the gap. The earlier cut stands on its own record: `673b8a9`
measured **1,524 passed / 0 failed / 3 ignored** on its export (59 binaries) and **1,602 / 0 / 3** in
the private workspace (66). No figure here is inherited from an earlier head.

- **Closed with evidence:** M19 locomotion · M20 stats/skills · M21 items, equipment and loot ·
  M22 combat · M23 magic · M24 enchanting/alchemy · M25 monsters, enemy AI and encounters ·
  M26 living NPCs (schedules, faction dialogue, `RUMOR.DAT` rumours) · M27 guilds and factions
  (`FACTION.TXT` graph, reputation with tree propagation, ranks 0–9 and per-rank services) ·
  M28 crime, law and courts (penalty clamps, legal reputation, guards, sentence and prison clock) ·
  M29 playable retail quests (the `QBN` opcode interpreter, clocks, journal and lifecycle). The
  requirements each milestone closes are recorded with their acceptance checks.
- **Measured, not assumed:** the campaign re-audit is history, not a standing verdict. It re-ran every
  probe and gate on the campaign head and put five milestones below the bar; those five — M2, M5, M11,
  M12 and M19 — are now **DONE after repair**, with the repair reports and the
  two-process evidence listed above. M18, then unsigned, is **`PARTIAL-TAG-PENDING`** today: signed by
  delegation on 2026-10-26 (0 FAIL; 8 PASS / 9 NC-ACEPTA / 4 NOT-COMPARABLE), with the tag still the
  owner's. The tally and the repair record are published in
  [Status](docs/acknowledgments/status.md#final-re-audit-of-the-campaign-m0m29).
- **Closed with evidence (M30):** economy — shops, inns, banking and transport. `behavior::economy`
  (`CalculateCost` with regional prices, the 36 `(building, group)` buy table, five trade modes, 53
  holidays, loans and default, house and ship) measured by `PROBE-SHOP-001`, `PROBE-ECONOMY-001`
  (E1–E28 vectors plus a seeded-determinism block) and `PROBE-BANK-001`, all exit 0 with the digest
  reproduced twice. Technical closure 2026-10-23; no milestone tag.
- **Closed with evidence (M31):** travel, weather and calendar — `heart::{travel, weather, calendar}`
  measured by `PROBE-TRAVEL-001` (34/34 vectors; a 2,200-pixel route over the real `CLIMATE.PAK` with
  byte-identical stdout in two processes), `PROBE-WEATHER-001` (13/13 vectors, 240,000 draws,
  `max_abs_delta = 0.0121 < 0.02`) and `PROBE-CALENDAR-001` (14/14). Technical closure 2026-10-23; no
  milestone tag.
- **Closed with evidence (M32):** conditions, death and rest — `skills::{conditions, rest}` and the
  vampirism/lycanthropy stages, measured by `PROBE-CONDITION-001` (24/24), `PROBE-VAMPIRE-001`
  (16/16), `PROBE-REST-001` (10/10) and `PROBE-DEATH-STATE-001` (2/2). Technical closure 2026-10-23;
  no milestone tag.
- **Closed with evidence (M33):** total persistence — `memory::{save_v2, savegame, savegame_v2}`:
  `PROBE-SAVE-ALL-001` (24/24 round trips byte-identical, the S0 census, five corruption mutations all
  rejected, deterministic digest), `PROBE-SAVE-MIGRATION-001` (v1→v2, idempotent),
  `PROBE-SAVEGAME-READ-001`, `PROBE-SAVEGAME-WRITE-001` and `PROBE-M33-DEMO-001`. Technical closure
  2026-10-26; no milestone tag.
- **Tags and signatures:** no milestone tag exists from M18 on; M18's acceptance signature is
  recorded by delegation on 2026-10-26, and every later milestone — M39 included, **DONE at gate
  level** — is still unsigned.
- **`PARTIAL-TECHNICAL-CLOSURE` (M34):** end-to-end integration and presentation — the E2E spine, the
  full presentation pass and the HUD, with `PLAYSESSION-001`, `UI-FULL-001` and `AUDIO-RT-001` inside
  the probe suite (**40/40**) — DONE at technical level with the named row **`AUDIO-RT-001-DEVICE`**
  still open. `AUDIO-RT-001` reproduces `PASS(degraded)` (`[SKIP-DEVICE]`: the binary is built without
  the audio feature), so its two-process measure covers the degraded path only, and the device branch
  is not promoted here — that call belongs to the owner. `UI-FULL-001`, which is also the M40 gate,
  counts the **34** non-playable surfaces as `N/A-engine` entries of its own census and measures only
  the **51** playable windows (**51/51**): its `PASS` neither depends on those 34 nor certifies them.
  No milestone tag.
- **`PARTIAL` (M35):** biomes, 2D vegetation and sky — optional, non-blocking
  annex. `BIOME-001` **25 passed / 0 failed / 1 NA** (`GAP-TAX-11`: no tile-class data in the corpus,
  so the probability gate cannot be measured) and `SPRITE-001` **11/11**. The named row `GAP-TAX-11`
  is deferred to M44. No milestone tag.
- **Closed with evidence (M36):** the UI-0 audit — one row per census window, recomputed from the
  code by `scripts/ui-audit-001.py` (blocks A+B+C print `VERDICT: PASS`, exit 0, in two separate
  processes with byte-identical stdout). It measured what the census actually holds — **85 = 78
  surfaces (27 + 51) + 7 infrastructure rows**, and **4 implemented / 47 partial / 34 absent** — and
  settled the patch/rewrite rule at **35 PATCH · 50 REWRITE**. Technical closure measured 2026-10-27;
  no milestone tag.
- **Closed with evidence (M37):** the UI compatibility layer — seven axes (surface, palette,
  typography, text, navigation, input, presentation) in
  `crates/parser-bevy-bridge/src/ui_compat/`, exercised by **3 pilot windows** under `UI-BASE-001`
  (9/9) and `UI-INPUT-001` (10/10): `VERDICT: PASS`, exit 0, two separate processes with
  byte-identical stdout. The pilots measured a depth cost of **0.4708333 u/window at one QA round**
  (band `r`, n = 3, W = 50) — **8.33×** the L1 baseline of **0.0565 u/window**. Technical closure
  measured 2026-10-27; no milestone tag.
- **Closed with evidence (M38):** the 27 UI-2 surfaces — 16 creation windows (`C01`–`C16`) and 11
  configuration pages (`CFG01`–`CFG11`) — each measured by its own gate: `scripts/ui-create-001.py`
  (16 surfaces, 12 checks) and `scripts/ui-config-001.py` (11 surfaces, 12 checks) print
  `VERDICT: PASS`, exit 0, in two separate processes with identical stdout and identical digests. The
  UI requirement's coverage is **0.857 (6/7)**: the one open clause is the owner's side-by-side
  acceptance (K7). The composed `FACES.CIF` face (K2) is closed, measured by the UI-2 probe's `FACE
  … painted=… source=… match=ok` lines and the `FACENAV` traversal, and its handoff records
  `status: DONE`. Technical closure measured 2026-10-27; no milestone tag.
- **DONE at gate level (M39):** the 51 interactive playable windows. The surface registry, the
  block grammar, the playable-window probe and the A–H runner are in the tree since `85979b9`, and
  `UI-JUGABLE-001` prints `VERDICT: PASS` on its 8 rules in two processes with byte-identical stdout
  (21,141 B, sha256 `e1efb40e…`, gate digest `fc0ebb54…`). The closure report — measured at `e249004`
  and recorded in docs/sintergic/l5-mission/m39-ui3-mission-report-CR-aecd1cef.md — publishes
  `requirements_coverage 0.857` with **K7** (the owner's side-by-side acceptance) as the single open
  clause. M39 is **not signed and carries no tag**, and the 51 windows it delivers are not yet the
  running demo window.
- **Design, and the part of it that was implemented:** the instance taxonomy, biomes, procedural
  classification, asset modularity and the generative-asset catalogue with a VRAM budget were designed
  on 2026-10-21. The biome, vegetation and sky parts were then **implemented and measured** in the M35
  annex (`BIOME-001` 25 passed / 0 failed / 1 NA on the declared `GAP-TAX-11`, `SPRITE-001` 11/11);
  what remains design only is the generative-asset catalogue.

The full per-milestone record — scope and evidence at each closure, the verified facts and the
declared known gaps — is in [Status: what is actually done](docs/acknowledgments/status.md).

## Roadmap M18–M48 + E8

The single canonical plan is **[Roadmap M18 to M48 + E8](docs/acknowledgments/roadmap-m18-m48.md)**:
atomic work breakdown, gates, dependencies, risks, estimates in `u` and **zero dates**. Start there, or
at the **[Agent entry point](docs/acknowledgments/agent-entrypoint.md)** for the operating rules (source
of truth, real state, closure rule, limits, how to continue).

State against the register
`docs/sintergic/validation/closure-register-CR-03390267.md` (`scripts/closure-gate.py` → `VERDICT:
PASS`, 0 prose↔register contradictions, falsifiers 3/3 red):
**51 milestones · 80 gate rows · 38 DONE · 1 PARTIAL · 1 PARTIAL-TAG-PENDING · 1
PARTIAL-TECHNICAL-CLOSURE · 10 AUSENTE**, `program_closure: OPEN`. M0–M17 **DONE and tagged** ·
**M18 `PARTIAL-TAG-PENDING`** (acceptance signature recorded 2026-10-26 by delegation — 8 PASS / 9
NC-ACEPTA / 4 NOT-COMPARABLE / 0 FAIL; tag none; comparing the rows against the original is the
owner's) · **M19–M33 DONE** at gate level (no milestone tags; M30–M33 closed 2026-10-23, M33 on
2026-10-26) · **M34 `PARTIAL-TECHNICAL-CLOSURE`** (named row `AUDIO-RT-001-DEVICE`) · **M35
`PARTIAL`** (`GAP-TAX-11` deferred to M44) · **M36 DONE**, **M37 DONE**, **M38 DONE** and **M39
DONE**, all four at gate level and without a tag (M39: coverage 0.857, K7 open) · **M40–M48
`AUSENTE`** (no code, no dispatch, no tag) · **E8 `AUSENTE`**, a separate program with its own genesis
contract.

A milestone marked above is at **gate-level technical closure — never owner acceptance**.
Public tags: `v0.3.0-alpha`, published 2026-10-27 · `v0.3.1-alpha` in preparation as a documentation
correction. No milestone tag exists from M18 on; M18's acceptance signature is recorded by delegation
on 2026-10-26.

| M | Epic | Scope | State | `u` nominal (range) | Gate |
|:-:|:---|:---|:---|--:|:---|
| **M36** | UI-0 | audit the 85 windows, patch/rewrite rule, compatibility contract, E1 re-estimate | DONE (technical closure 2026-10-27, no tag) | 2.00 (1.60–2.40) | `UI-AUDIT-001` A+B+C |
| **M37** | UI-1 | UI compatibility layer (7 axes) | DONE (technical closure 2026-10-27, no tag) | 4.50 (3.45–5.60) | `UI-BASE-001` + `UI-INPUT-001` (3 pilot windows) |
| **M38** | UI-2 | 16 creation + 11 configuration + menus (27 surfaces) | DONE (technical closure 2026-10-27, no tag) | 6.50 (4.95–8.40) | `UI-CREATE-001` 16/16 · `UI-CONFIG-001` 11/11 |
| **M39** | UI-3 | 51 interactive playable windows | DONE at gate level (0.857, K7 open; no tag) | 7.50 (5.65–9.40) | `UI-JUGABLE-001` A–H |
| **M40** | UI-4 | UI gate + side-by-side + input + accessibility | AUSENTE (no code, no dispatch, no tag) | 3.50 (2.45–4.55) | `UI-FULL-001` 85/85 |
| **M41** | SYS-1 | depth: combat, magic, items, stats, resistances | AUSENTE (no code, no dispatch, no tag) | 63.00 (41.30–84.70) | work-package gates (§3.6) |
| **M42** | SYS-2 | depth: NPCs, guilds, crime and law, quests | AUSENTE (no code, no dispatch, no tag) | 42.00 (28.00–56.00) | work-package gates (§3.7) |
| **M43** | SYS-3 | depth: economy, travel, weather, calendar, conditions | AUSENTE (no code, no dispatch, no tag) | 51.00 (33.50–67.50) | work-package gates (§3.8) |
| **M44** | DUN-1 | dungeon content: grid + loot/respawn/bosses/instances | AUSENTE (no code, no dispatch, no tag) | 39.00 (25.50–52.00) | work-package gates (§3.9) |
| **M45** | E4 MODS | declarative mod pipeline | AUSENTE (no code, no dispatch, no tag) | 39.00 (26.00–52.00) | `MODS-001` · `MODS-002` |
| **M46** | E5 ONLINE | optional online: lobby, instances, authority (**owner go required**) | AUSENTE (no code, no dispatch, no tag) | 78.00 (51.00–104.00) | `ONLINE-001`–`003` |
| **M47** | E6 PLATFORM | packaging, multi-OS CI, sustained perf, accessibility | AUSENTE (no code, no dispatch, no tag) | 15.50 (10.00–21.00) | `PLATFORM-001`–`003` |
| **M48** | E7 MODERN | opt-in modern layer (pristine/modern) | AUSENTE (no code, no dispatch, no tag) | 31.00 (20.00–41.00) | `MODERN-001` · `MODERN-002` |
| **E8** | deferred expansion horizon | **separate program** after M48, own genesis contract (`E8` is not "M49") | AUSENTE | — | own contract |

**Totals.** M36–M48 = **382.50 u** nominal (**253.40–508.55**); **E8 counted separately**. Plan A is
**planned scope, not delivery**: it closes when M34's outstanding row **`AUDIO-RT-001-DEVICE`** passes,
and today M34 is **`PARTIAL-TECHNICAL-CLOSURE`**; **M35 is an optional non-blocking
annex** stated `PARTIAL`, with `GAP-TAX-11` deferred to M44. The UI gate of M36–M40 is the stretch that decides
whether the game *feels* the original: the audit (M36), the seven-axis layer exercised by its three
pilots (M37) and the 27 UI-2 surfaces (M38) are DONE at gate level, the 51 playable windows
(M39) are DONE at gate level at coverage 0.857 with K7 still the owner's — runner and probe in the
tree since `85979b9` — and the 78/78
 gate (M40) is still to be built.

## What this is

Daggerfall is a large, systemic RPG: a province-scale world map, procedurally assembled cities and
dungeons, a simulated calendar, factions, quests, shops, language and lore. This project rebuilds
that machinery as a native Rust engine with four explicit goals:

1. **Retail-data fidelity.** Cities, interiors, textures, terrain, audio, NPC names and quest text
   come from the player's own `ARENA2` files, parsed at runtime — never copied into the repository.
2. **Deterministic simulation.** The same seed and the same data produce the same world, the same
   names and the same outcomes. Determinism is a testable property here, not a slogan.
3. **Data-oriented architecture.** Bevy's ECS is the substrate; game rules live in small, pure
   crates that can be tested headlessly, without a GPU or a window.
4. **Verifiable parity.** Behaviour is pinned against the original formats and, where the logic is
   documented, against the MIT-licensed Daggerfall Unity source used as an executable spec. Golden
   vectors and probes decide what "parity" means.

The long-term direction is a mod-friendly, data-only content pipeline: content packs describe
records; they do not ship retail bytes and do not require compiling code.

## A seed, not a one-off

The engine is built to be **adopted**, not only played: the data layer, the organ crates and the
probe harness are meant to be the starting point of a *different* game as much as the base of this
one. **MIT** is part of that decision — an architecture constraint, not paperwork filed at the end.

- **Licence mechanics are settled once.** `license = "MIT"` in the workspace `Cargo.toml` is
  inherited by every member (`license.workspace = true`), and [LICENSE](LICENSE) covers app, organs
  and parser under one permissive chain ([the Why Bevy section](#why-bevy--and-why-not-build-on-daggerfall-unity-or-daggerfalljs)).
- **Adoption can be partial.** The organs carry no Bevy types and the parser is a zero-dependency
  leaf ([the trade-offs](#the-trade-offs-named)), so a fork can take the parsers and rules and
  replace the presentation — or keep the presentation and swap the world.
- **The boundary is explicit.** MIT covers this source: not Daggerfall's data (the player's own
  copy), and not the pinned Daggerfall Unity corpus under `reference/dfu-spec/` (its own MIT notice).

What a derived game inherits today is the **verified mechanism**, not a finished RPG: M10–M18 closed
the retail data layer, M19–M29 the game-rule organs (locomotion, character, items, combat, magic,
enchanting, monsters, NPCs, factions, law and quests) and M30–M33 the next four (economy and banking,
travel/weather/calendar, conditions/death/rest, total savegame), each probe-backed; M34–M38 then added
the end-to-end spine and the first three UI gates. A fork is welcome to start from exactly what is
measurable.

## Community: grow, don't divide

This project belongs to a community that has kept Daggerfall alive for three decades, and nothing in
it is meant to divide that community. We are not here to break, replace or compete with what already
exists: we hope you play the original game, and we hope you play
[Daggerfall Unity](https://github.com/Interkarma/daggerfall-unity) and
[DaggerfallJS](https://github.com/Lattymoy/daggerfall-js-source). We already had that experience, and
it is exactly what motivated us to build this.

**Both existing versions are a jewel.** Daggerfall Unity and DaggerfallJS are serious engineering that
deserves respect — and deserves contributions, more of them, not fewer. This project asks nobody to
abandon either one; a player who is happy there is not someone we are trying to move. Both are
credited, with their licences, in the [acknowledgements](docs/acknowledgments/credits.md), and the
technical comparison is argued in full under
[Why Bevy](#why-bevy--and-why-not-build-on-daggerfall-unity-or-daggerfalljs).

**We believe this path adds.** Not by taking something away from the other two, but by widening the
field around the same game: more contributions, more good ideas, more projects that outlive us — which
is what keeps Daggerfall from dying. A third implementation is only worth building if it makes the
whole community larger.

**Why we are here — a conviction, stated as ours.** We consider Daggerfall the ideal RPG: the closest
thing we know to the essence of a true RPG, an essence that was gradually lost as the genre moved
elsewhere. Getting back to that definition is what moves this project. That is our conviction, not a
claim about what an RPG must be or about what anyone else should enjoy.

**Grow, don't divide.** The goal is to give players more places where they feel comfortable playing
the Daggerfall they love, and to expand the side the other projects do not cover: native, and
[AI-native](#ai-native-text--diff--probe--gate). Neither is a slogan here — a native Rust engine is a
different distribution and performance envelope, and a code-first project whose whole state is text in
version control is a different way of building one. With these tools the engine became possible in
months; without them it would have taken years. That is the entire case for a third option existing.

## Gameplay scope (honest)

The engine reproduces the **world and its data**; since M19 it also reproduces a growing part of the
**game rules**. The M10–M18 tramo closed the *data* DoD (retail city, interiors, textures, terrain,
audio, NPC population, text/quest containers, UI assets), and M19–M29 added locomotion and the body,
live stats and skills, items/equipment/loot, combat, magic, enchanting and alchemy, monsters with
enemy AI, living NPCs with dialogue and rumours, guilds and factions, crime/law/courts, and playable
retail quests.

A 49-system gameplay checklist still carries the split measured **before** that tramo — **2 covered /
15 partial / 32 missing** — and has not been re-measured; what is verified is the probe and
requirement record in [Status: what is actually done](docs/acknowledgments/status.md), not a percentage.

**Still missing — the depth and the surface, not “out of scope”:** the gameplay depth of M41–M43
(combat, magic, items, stats and resistances; NPCs, guilds, crime and quests; economy, travel, weather,
calendar and conditions) and the UI surface of M39–M40 — the 51 interactive playable windows and the
78/78 gate. The systems those milestones deepen already run and are probe-backed: economy and banking
(M30), travel with weather and calendar (M31), conditions, death and rest (M32) and the total savegame
(M33) are implemented at technical level, but their consumer today is the probe — the running demo
window does not expose them, and the 27 UI-2 surfaces of M38 are measured by their own gate rather than
composed into that window. Guilds and factions (M27), crime/law/courts (M28) and playable retail quests
(M29) are implemented and probe-backed, and the register states them **DONE at gate level**; the
measured rows are named in
[Status](docs/acknowledgments/status.md). **“100% of the data DoD” is not “100% of the game.”** That
distinction, the module proposals M19–M34 and the cost decision behind their order are recorded with
the plan in [Roadmap](docs/acknowledgments/roadmap.md).

## Why Bevy — and why not build on Daggerfall Unity or DaggerfallJS

Both existing projects are excellent, and both are credited in
[ACKNOWLEDGMENTS](docs/acknowledgments/credits.md). This project deliberately forks neither:
the reasons are technical, not tribal, and each one names the cost it accepts.

### Why Bevy 0.20

- **Data-oriented ECS.** Daggerfall is a system soup — time, factions, quests, shops, AI, weather,
  magic. Bevy's archetype ECS shares that data without an inheritance graph, and hot paths stay
  cache-friendly: one query walks contiguous columns.
- **Rust without a GC.** Determinism needs predictable memory: no collector pause may land between
  two ticks a digest must match. Rust gives safety with no runtime and testable plain libraries.
- **Memory control and streaming.** A retail install is ~516 MB of `ARENA2` (`ARCH3D.BSA` 27 MB,
  `BLOCKS.BSA` 33 MB, `MAPS.BSA` 22 MB, measured). None of it is unpacked or preloaded: archives are
  indexed at startup and read by offset on demand, models become GPU handles lazily through one bridge
  crate, and the 🧠 memory organ owns the loaded-chunk set plus its save/restore — lifetimes checked at
  compile time instead of discovered by a profiler.
- **Headless by construction.** The app builds with `default-features = false`; `window` is additive,
  so probes run with no window and no GPU — which is what makes the 40-entry suite CI-usable.
- **No proprietary runtime; MIT all the way down.** A Bevy application is a native binary — no engine
  runtime licence, no vendor seat, no editor project to migrate — and Bevy, parser, organs, the DFU
  spec corpus and this engine share one licence family: no copyleft boundary, no runtime EULA.
- **Toolchain.** `cargo` is the only build system (`test`, `clippy -D warnings`, `fmt`, one pinned
  `rustc` 1.97.1). Dependencies are locked in `Cargo.lock`, the `daggerfall_parser_rs` leaf has
  **zero** dependencies and the organs carry no Bevy, so a supply-chain review is a short graph.
- **Pure-Rust asset pipeline.** Parsed retail records become `Mesh`, `Image` and `StandardMaterial`
  handles through a small bridge crate; conversion is testable without a renderer.

### The trade-offs, named

- **Web/WASM is a build target, not a promise.** Bevy compiles to WebAssembly/WebGL2, but native needs
  the player's ~516 MB `ARENA2` anyway, WebGL2 ceilings the renderer (mesh shaders first) and a
  browser build imports a second performance/determinism envelope. DaggerfallJS already does
  browser-first well.
- **Ecosystem and dependency risk.** Bevy is younger than Unity and its API still moves between
  releases; this project pins `0.20.0` and treats upgrades as reviewed work, never automatic churn.
  The mitigation is structural: organ crates carry no Bevy types, so a render-side rewrite cannot
  reach the game rules or the parsers.

### Why not build on Daggerfall Unity

Daggerfall Unity (Interkarma and contributors, MIT, © 2009–2023 Daggerfall Workshop) is the single
most valuable piece of Daggerfall engineering in existence, and we use it as an **executable
specification** — pinned at `v1.1.1` (commit `0038a1e61999ad2c73fb1f33b706cae9a88ec66f`), with a
small licensed copy under `reference/dfu-spec/` so citations stay reproducible. It is not the base:
a Unity game depends on the Unity runtime and editor toolchain, it is organized around
`MonoBehaviour`s and scene objects rather than an ECS of plain data, C# on a managed runtime is a
different performance and determinism profile, and its product goal — Daggerfall inside Unity with a
script-mod ecosystem — is not ours. Sharing the spec helps both; sharing the architecture fights both.

### Why not build on DaggerfallJS / DFO Online

[DaggerfallJS](https://github.com/Lattymoy/daggerfall-js-source) (DFO Online, MIT, © 2026 Lattymoy) is
a serious, fast-moving JavaScript/WebGL2 reimplementation. We adopt its product lessons —
bring-your-own-data with no game bytes in the repository, a browser-first distribution next to native
installers, and one registry of feature switches — but not its code: the platform (JS/WebGL2 vs
Rust/Bevy), the optimization target (download size and WebGL2 limits vs native simulation throughput)
and the content strategy (a JS mod runtime vs declarative Rust-side content packs) all differ. Good
ideas travel with attribution; vendored code would add a second execution model. Neither project's
assets are used, and this project does not present itself as either one.

The long-form comparison, with every point argued in full, remains in
[Why not build on Daggerfall Unity or DaggerfallJS](docs/acknowledgments/why-not-dfu-or-daggerfalljs.md).

## Built with the Sintergic architecture

The engine is not only a Bevy project; it is an instance of a construction method — the **Sintergic
architecture** — built in public and validated by this repository. It is not a brand claim: the method
is the set of mechanical steps that lets a reader falsify every status statement above. The whole
method — what it commits to, why it was chosen over the alternatives, what it costs and which parts
are still open — is in [ARCHITECTURE.md](ARCHITECTURE.md).

- **Positions, not phases.** Work advances through declared positions, each with an entry condition;
  a position opens only when the previous one is met by recorded evidence, not when it feels finished.
- **Requirements before code.** Every milestone names the requirement it closes and the check that
  counts as acceptance, before the implementation exists.
- **Organs as boundaries.** The organ crates (nervous, heart, memory, behaviour, perception, skills,
  inventory, crafting, combat, antibodies) carry the method's organ vocabulary into the workspace
  layout: one responsibility each, event-bus only, testable without a window.
- **Probes as the only currency.** Acceptance is a probe, a golden vector or a digest — not a
  screenshot and not an opinion. The suite, the golden vectors and the `0 ULP`/FNV-1a64 comparisons
  are the evidence recorded for each milestone.
- **Registers and evidence.** State is designed to outlive a session: `ASSET-REGISTRY.json`,
  `DATA-CLASSIFICATION.md` and the per-milestone record in
  [Status: what is actually done](docs/acknowledgments/status.md). Nothing load-bearing lives only in
  someone's memory of a chat.

What the method buys is falsifiability in one direction: README claim → probe or QA result → recorded
evidence. What it does not buy is a finished game — it records progress and it records what is still
missing ([known gaps](docs/acknowledgments/status.md#known-gaps-declared-not-hidden)), but acceptance
still belongs to the owner.

## AI-native: text → diff → probe → gate

"AI-native" describes the **tooling**, not an industry category: the game — code, tests, probes,
specs, decisions — is text in version control, and nothing load-bearing lives in an editor's binary
project state. Bevy is **code-first** — no scene editor, no prefab holding behaviour — which is what
makes the project controllable by agents. The loop:

1. **Text.** Inputs are the Rust source, `Cargo.toml`/`Cargo.lock`, the public record under
   [`docs/acknowledgments/`](docs/acknowledgments/) and the design note attached to each change. A
   session reads files and writes files.
2. **Diff.** Every change is a patch a reviewer can read whole; organ crates import no Bevy and the
   parser is a leaf, so an automated edit stays local and a render-side patch cannot reach game
   rules.
3. **Probe.** A claim counts once it executes: `./scripts/launch.sh --probe ALL-001` runs the
   anti-screen probe suite (**37/37** at the M29 close, **40/40** since the three M34 demo probes
   entered it, spec §11) and `cargo test --workspace` prints **1,528 passed / 0 failed / 3 ignored**
   on the public export of `e4c4869` (59 test binaries; **1,606 / 0 / 3** in the private workspace at
   `137e30a`). Golden vectors and digests — 0 ULP on the Perlin port, FNV-1a64
   texture vectors, map and population digests — stop a bug from certifying itself.
4. **Gate.** The battery listed under [Contributing](#contributing), plus the method's position gate:
   the declared position of a milestone must be met by a recorded, mechanical result before the next
   one opens.

**Open-weight models, no shipped weights.** The loop is driven by open-weight models through a local
harness. No model weights, training data, prompts or inference runtime ship in this repository, and
the engine has no inference dependency at runtime: a released binary needs `cargo`, your data and
nothing else. Nothing in the AI tooling is load-bearing for the product, which is why the loop can be
reproduced or replaced without touching the game.

**Registers, so state survives the session.** `ASSET-REGISTRY.json` (hashes only),
`DATA-CLASSIFICATION.md` and the per-milestone record keep state readable at session start and
diffable at the end; a fixed `Cargo.lock`, one verified rustc (1.97.1) and seeds as a first-class
input make the next run yield the same digests elsewhere.

**What is not claimed.** No judgement is automated: every milestone tag from M18 on waits for the
owner's side-by-side signature. M18's own acceptance signature, recorded by delegation on 2026-10-26,
covers the engine's measured rows — it does not stand in for the comparison with the original game.
"AI-native" means the loop is machine-executable, not that review left it.

## Linux-first, by conviction

The code is **MIT**, and development is **Linux-first on purpose**. Daggerfall survives today because
people kept free, open reimplementations of it running on the platforms they actually use; Linux
players are the part of that community the commercial ecosystem has served least. Building for Linux
first — and releasing under MIT — is how this project returns that, and it is a design constraint,
not a checkbox: the organ crates carry no Bevy types and the parser is a zero-dependency leaf, so
another platform can be added without forking the game.

- **Verified:** Linux with X11 or Wayland for the windowed mode; Linux headless for every probe and
  every CI run (`ubuntu-latest`, [`.github/workflows/ci.yml`](.github/workflows/ci.yml)). Every number
  quoted above is a Linux measurement.
- **Source distribution:** no storefront, no proprietary runtime, no installer to sign; `cargo` plus
  the player's own data is the whole requirement.

### Platform direction, in order

The order is intentional: each target is chosen to widen who can run the game without splitting the
codebase. Nothing beyond Linux has been validated today, so read this as direction, not as a release
plan.

1. **Linux native** — current, and first. Every number quoted in this document is a Linux measurement.
2. **Android** — next. Free distribution is the point: an APK, F-Droid and plain sideloading, offline
   and online, with a phone-first interface and performance envelope. The organs carry no Bevy types
   and the parser is a leaf precisely so this port is not a fork.
3. **Web/WASM** — after Android, exploratory, bounded by the trade-off named above.
4. **Windows** — future, and planned **last**. Not abandoned: it is simply not the platform this
   project serves first.
5. **macOS** — not planned at this time.

A port is a horizon, not a commitment. No dates are attached to any of these, and a line changes only
when a build has been validated and the probe output is quoted.

A failure on Linux is a failure in the first-class target: please report it with the probe output.

## Online: a direction, not a shipped feature

**Single-player is what runs today.** The long-term direction is *online-capable*: optimisation for
many concurrent users, more user control and a larger world scale, recalibrating toward an MMO feel.
The distance matters: sharding, area-of-interest and replication exist as research notes in the design
record, and **none of them is implemented** — describing them as features would be false.

The future modes under exploration — **raids, dungeons, endless game modes** — are the same kind of
statement: a direction, not a commitment. **Parity comes first.** The single-player engine and its
acceptance protocol are the work; these are where it can go afterwards, and no date or "v1.0" is
promised for either.

## Quick start

### Prerequisites

- A Rust toolchain (`cargo` + `rustc`; verified with 1.97.1).
- Linux with X11 or Wayland for the windowed mode. Headless mode and all probes need no display.
- A legally obtained copy of Daggerfall with its `ARENA2/` directory, kept **outside** this repo. The
  default path is `~/Games/Daggerfall/app`; override it with `DF_APP_ROOT` or `--data-root`.

```bash
# 1. Clone or copy the repository, then enter it
cd daggerfall-bevy-native
# 2. Point the engine at your own data (do not copy it into the repo)
export DF_APP_ROOT="$HOME/Games/Daggerfall/app"
# 3. Build and run
./scripts/launch.sh --mode complete          # release build + window
./scripts/launch.sh --mode preview --headless  # fast debug build, no window (default mode)
```

The launcher, its modes, flags and exit codes are documented in
[The launcher](docs/acknowledgments/launcher.md).

## Bring your own data (BYOA)

**Nothing is downloaded and nothing is redistributed.** The repository ships code, tests, probes and
documentation; the game data stays on your disk, where you installed it.

- **No assets in the tree.** No `.BSA`, `.DAT`, `.RSC`, `.IMG/.CIF`, `.FNT`, `.CFG`, `.QBN/.QRC`,
  texture, palette or sound file is committed, and CI rejects them if they appear
  (`scripts/ci/content-scan.sh`, threshold zero).
- **What *is* committed** is metadata: `ASSET-REGISTRY.json` records 492 assets by path, size and
  SHA-256 so a run can prove which copy it read; `DATA-CLASSIFICATION.md` states the policy.
- **One variable points at your install.** `DF_APP_ROOT` (or `--data-root`) is the only thing the
  engine needs; `PathChecker` resolves the archives by offset at runtime.
- **Two modes, declared.** Mode A reads your Daggerfall installation. Mode B (custom universe) is an
  explicit, separate mode that must not open retail archives — measured by
  `PROBE-MODB-ISOLATION-001`, which asserts zero `.BSA` opens.
- **Why it is built this way.** Daggerfall is freeware, not public domain: free to download, not free
  to redistribute its assets. Distribution without game bytes is the only form this project can ship
  in, and it is also the form that keeps the legal position clean. See
  [Data, legal and trademarks](docs/acknowledgments/legal-and-trademarks.md).

## Architecture

The workspace is deliberately layered: the application composes; the organs do not call each other;
the parser knows nothing about rendering; the bridge is the only place where records become assets.

```text
daggerfall-app  (Bevy 0.20.0, window/headless, ECS systems, probes)
├── 10 organ crates (pure Rust, no Bevy, event-bus only)
│   nervous · heart · memory · behaviour · perception · skills
│   inventory · crafting · combat · antibodies
├── daggerfall-parser-bevy-bridge  (MeshSpec/TextureRecord → Bevy handles)
└── daggerfall_parser_rs           (zero-dependency leaf: clean-room BSA parsers)
```

## Repository layout

```text
crates/                 13 Rust crates (10 organs + app + parser + bridge)
scripts/launch.sh       release launcher (modes, features, probes, timeouts)
scripts/launch.py       zero-logic Python wrapper around the launcher
scripts/ci/             content scan, trademark check, local CI runner
reference/dfu-spec/     pinned DFU source used as an executable spec (MIT, not compiled)
docs/acknowledgments/   the long-form record: status, parity, credits, legal, roadmap
docs/i18n/              language policy and translation inventory
ARCHITECTURE.md         the construction method: commitments, alternatives, costs, open gaps
CONTRIBUTING.md         build, battery, the module pattern, pull-request checklist
DATA-CLASSIFICATION.md  what is committed vs. what is runtime-only
ASSET-REGISTRY.json     path/size/SHA-256 of every retail asset used (hashes only)
```

Every claim in this README is backed by the record under [`docs/acknowledgments/`](docs/acknowledgments/):
the per-milestone status, the verification method, the legal position and the roadmap.

**Languages.** English is the canonical language of this repository; translations are derived files
(`*.es.md`), produced on demand. Policy, naming convention and staleness rules:
[`docs/i18n/POLICY.md`](docs/i18n/POLICY.md); documents, language and priority:
[`docs/i18n/STATUS.md`](docs/i18n/STATUS.md).

## Further detail

`README.md` is the entry door; the long-form record is [`docs/ACKNOWLEDGMENTS.md`](docs/ACKNOWLEDGMENTS.md), one file per case:

- [Acknowledgements](docs/acknowledgments/credits.md) — DFU, DaggerfallJS, UESP, modders, Bethesda, Rust/Bevy.
- [Data, legal and trademarks](docs/acknowledgments/legal-and-trademarks.md) — code license, why game data is not MIT, third-party source, marks.
- [Why not build on Daggerfall Unity or DaggerfallJS](docs/acknowledgments/why-not-dfu-or-daggerfalljs.md) — the long-form comparison.
- [How parity is verified](docs/acknowledgments/parity-verification.md) — sources of truth, golden vectors, probe suite, CI guardrails.
- [Status: what is actually done](docs/acknowledgments/status.md) — full M0–M29 record, the campaign
  re-audit and the known gaps.
- [Roadmap](docs/acknowledgments/roadmap.md) · [The launcher](docs/acknowledgments/launcher.md) — stage detail; modes, flags, exit codes.
- [Complete plan: M18 to M48 + E8](docs/acknowledgments/roadmap-m18-m48.md) — the single canonical plan: atomic work breakdown, gates, dependencies, risks, estimates in `u` and zero dates.
- [Agent entry point](docs/acknowledgments/agent-entrypoint.md) — source of truth, measured state, closure rule, next gate, hard limits and how to continue.
- [Export limits](docs/acknowledgments/export-limits.md) — what the public export leaves out, what the shipped code does without it, and the measured internal-reference metric.
- [Crates, data custody and determinism](docs/acknowledgments/crates-and-determinism.md) — 13 members, Mode A/B, determinism rules.
- [Architecture](ARCHITECTURE.md) — the construction method, the alternatives it rejects and why, its costs and open gaps.
- [Contributing](CONTRIBUTING.md) — the full battery, the module pattern and the pull-request checklist.

## Contributing

- Read [`DATA-CLASSIFICATION.md`](DATA-CLASSIFICATION.md) first. **Never commit game data**, BSA
  files, extracted textures, audio or any retail bytes. CI will reject them.
- Keep the parser a zero-dependency leaf and keep organs free of Bevy. New cross-crate
  dependencies need a written reason.
- Every behavioural claim needs a probe or a golden vector. "It looks right" is not evidence.
- Run the local battery before opening a pull request:

```bash
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo clippy -p daggerfall-app --features window --all-targets -- -D warnings
bash scripts/ci/run-ci.sh
./scripts/launch.sh --probe ALL-001
```

- If you change a status, a decision or a requirement, update the corresponding record under
  [`docs/acknowledgments/`](docs/acknowledgments/) rather than this README alone.
- The full pattern — requirement → contract → probe → code → test → QA → record — the commit
  convention and the review checklist are in [CONTRIBUTING.md](CONTRIBUTING.md).
- By contributing, you agree that your contribution is licensed under the MIT License.

## License

[MIT](LICENSE) © 2026 hirundodev.

The license covers the source code in this repository. It does **not** cover Daggerfall game data,
which is not distributed here, nor the third-party source under `reference/dfu-spec/` (its own MIT
copyright notice, © 2009–2023 Daggerfall Workshop).

Why game data cannot ship here, the third-party source terms and the trademark position:
[Data, legal and trademarks](docs/acknowledgments/legal-and-trademarks.md).
