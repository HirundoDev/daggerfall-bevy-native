# Status: what is actually done

*Case file split from [`docs/ACKNOWLEDGMENTS.md`](../ACKNOWLEDGMENTS.md): the body was moved and later cleaned of working notes; the verified facts are unchanged.*

**Public release line: `v0.2.0-alpha` (pre-release).** The alpha publishes the mechanism measured
below; it does **not** claim acceptance. M18 — the global acceptance gate — is open: its 21
acceptance rows measured on the real window are **3 PASS · 6 PARTIAL · 12 NO**, the owner's
side-by-side signature is pending, and no `M18` tag exists.

M0–M17 carry annotated git tags of the form `M<n>-YYYY-MM-DD`, with matching `release/M<n>`
branches (`M13b` is a tagged parity follow-up). M18–M29, M34, M35, M36 and M37 have **technical**
closures with recorded evidence but **no tag yet**: the owner signature gates that. The table below
states only what those closures contain.

| Milestone | Scope | Evidence at closure |
|---|---|---|
| **M0** | Naming/legal guardrails, position gates, CI content + trademark scans | `scripts/ci/run-ci.sh` → PASS (2/2) |
| **M1** | 3D block viewer: `ARCH3D.BSA` → Bevy mesh, window feature, PathChecker | Real block rendered from retail data |
| **M2** | First-person camera and grid collision | `PROBE-VIEW-001` PASS |
| **M3** | Navigable city streets, deterministic layout, sky | `PROBE-CITY-001` PASS |
| **M4** | Interior prototype and time system; the window renders a real `.RDB` interior | `PROBE-INTERIOR-001` PASS (block A synthetic + B time + C real `RDB`), plus a measured window capture |
| **M5** | Interactive commerce (inventory-lite) and the trade-price formula | `PROBE-COMMERCE-001`; `PROBE-FORMULA-001` with 11 independent golden vectors |
| **M6** | Integrated world playthrough and the unified behaviour-probe suite | `PROBE-WORLD-001` PASS |
| **M7** | Session read models: HUD, automap, inventory/quests | `PROBE-READMODEL-001` PASS |
| **M8** | Save/load round-trip of a played session | `PROBE-SESSION-001` PASS |
| **M9** | Release slice and global verification | Global verification **19/19 = 1.00**, 0 critical failures, 19 rows with command + exit + output and **0 exclusions**. See the erratum below |
| **M10** | Retail city blocks from `BLOCKS.BSA` (`.RMB`) | `PROBE-RMB-001`: 5-city sample, 951/951 building records, 252/252 `ARCH3D` models, 0 traversals |
| **M11** | Retail interiors from `.RDB` + `ARCH3D` | `PROBE-RDB-001`: 187/187 dungeon-block census; a real interior (`N0000055.RDB`) assembled |
| **M12** | Retail texture layer: `TEXTURE.000-010`, `TEXTURE.256`, palettes | 472 `TEXTURE.*` files hashed; 12 FNV-1a64 golden vectors |
| **M13** | Terrain and climate from `WOODS.WLD` / `MAPS.BSA` | `PROBE-TERRAIN-001`, `PROBE-CLIMATE-001` PASS; height field bit-exact |
| **M13b** | `extraNoise` parity (`Mathf.PerlinNoise` port) | **0 ULP** against the compiled reference implementation |
| **M14** | Music (`MIDI.BSA`) and sound effects (`DAGGER.SND`) | `PROBE-AUDIO-001`: 131/132 songs (one without a retail record, declared, not faked), 459 sound records, 60/60 day/night triggers |
| **M15** | NPC population, name banks, captions, spawn guards | `PROBE-NPC-001` 18/18; deterministic population digest reproduced twice |
| **M16** | Lore, text and quests: `TEXT.RSC`, `.QBN`, `.QRC`, quest verbs, books | 18,501 text tokens, 247/247 QBN tiled, 244/244 QRC validated, 82 quest verbs, 79 quests, 91 books |
| **M17** | 1996-style UI (IMG/CIF/FNT/RCI assets, windows, HUD) | `PROBE-UI-001` PASS (43/43 checks, 18 golden vectors); `--features window` build EXIT=0; visual signature deferred to M18 |
| **M18** | Full-map index probe, release scan, final acceptance | **Technical closure PASS (2026-10-20)** — `PROBE-MAP-001` resolves 15,251/15,251 locations (0 skipped, 0 panics, identical digest twice); the suite froze at 18/18 for this milestone and has grown with every later one; the 21 acceptance rows were measured on the real window (**3 PASS · 6 PARTIAL · 12 NO**); **owner side-by-side PENDING**, so no tag exists |
| **M19** | Player locomotion and body: DFU speeds, jump/gravity/fall damage, climbing, levitation, swimming and breath, encumbrance, stealth formulas | **Technical closure PASS (2026-10-21)** — `behavior::locomotion` (21 tests, no Bevy, no I/O) consumed by the demo and the window through a single `Locomotion::step`; `PROBE-LOCOMOTION-001` 70 checks (formulas / runtime / stealth / retail demo), `STEALTH-001` 16 checks |
| **M20–M26** | Gameplay parity, first tramo: stats and skills (M20), items/equipment/loot (M21), combat (M22), magic (M23), enchanting and alchemy (M24), monsters, enemy AI and encounters (M25), living NPCs, faction dialogue and `RUMOR.DAT` rumours (M26) | Each closed at technical level with its probe added to the suite: 18 probes at the M18 freeze → 27/27 at the M26 close. At the M26 close: **534 tests passed / 0 failed**, clippy clean in both configurations |
| **M27** | Guilds and factions: `FACTION.TXT` graph (365 ids), reputation with tree propagation, ranks 0–9, per-rank services | **Technical closure PASS (2026-10-21)** — `PROBE-FACTION-001` 17/17, `PROBE-GUILD-001` 33/33, `M27-DEMO-001` 15/15; the suite went from 27 to 28 probes |
| **M28** | Crime, law and courts: penalty clamps, legal reputation, city guards, court sentence, prison clock | **Technical closure PASS (2026-10-22)** — `PROBE-CRIME-001` and `PROBE-LAW-001` measure `steps=262`, `5480/375 → 2740/187` and `clock_advanced=16,156,800` against an independent recomputation |
| **M29** | Playable retail quests: the `QBN` opcode interpreter, clocks, journal, lifecycle | **Technical closure PASS (2026-10-22)** — `PROBE-QUEST-EXEC-001` (24 golden vectors, a 20-quest determinism trace with its seed falsifier), `LIFECYCLE-001`, `JOURNAL-001`; `covered=62 negative=7 uncovered=0`; census 11/11 over 247 `.QBN` / 244 `.QRC` |
| **M34** | End-to-end integration and presentation: the E2E spine, the full presentation pass and the HUD | **Technical closure measured 2026-10-26** — `PLAYSESSION-001`, `UI-FULL-001` and `AUDIO-RT-001` entered the suite, taking `ALL-001` to **40/40**; the independent probes agree on the same head: `SPINE-001` **12/12** (4 controls, 6 falsifiers, identical digest over two runs), `VIDEO-001` PASS, `PROBE-AUTOMAP-001` **13/13**, `PROBE-LIGHT-001` PASS (digest reproduced in-process, `0x61d950d0d13da8de`), `PROBE-DUNGEON-001` **12/12**. QA round **0.94**, 0 critical / 0 major. Owner rows recorded **12/12** (5 PASS · 7 accepted with a declared deviation · 0 FAIL). No `M34` tag |
| **M35** | Biomes, 2D vegetation and sky — optional, non-blocking annex | **Annex delivered 2026-10-26** — `BIOME-001` **25 passed / 0 failed / 1 NA** and `SPRITE-001` **11/11**. The NA is the declared gap `GAP-TAX-11`: the pinned corpus carries no tile-class data, so the probability gate cannot be measured — the probe reports `DEGRADED` instead of inventing a number. Owner rows CS-05 and CS-11 remain pending (side-by-side). No `M35` tag |

| **M36** | UI-0: audit of the census windows, the patch/rewrite rule, the seven-axis compatibility contract and the E1 re-estimate | **Technical closure measured 2026-10-27** — `scripts/ui-audit-001.py` prints `VERDICT: PASS` on blocks A+B+C, exit 0, in two separate processes with byte-identical stdout; **85 = 78 surfaces (27 + 51) + 7 infrastructure rows**, measured **4 implemented / 47 partial / 34 absent**, and the rule settles **35 PATCH · 50 REWRITE**. No `M36` tag |
| **M37** | UI-1: the seven-axis UI compatibility layer — surface, palette, typography, text, navigation, input, presentation — and its three pilots | **Technical closure measured 2026-10-27** — `crates/parser-bevy-bridge/src/ui_compat/` shipped behind two gates: `UI-BASE-001` **9/9** and `UI-INPUT-001` **10/10**, `VERDICT: PASS`, exit 0, in two separate processes with byte-identical stdout. The pilots measured a depth cost of **0.4708333 u/window at one QA round** — band `r`, n = 3, W = 50 — against the L1 baseline of **0.0565 u/window** (**8.33×**). No `M37` tag |

**Owner signature and tag are pending for M18–M29, M34, M35, M36 and M37 alike.** Technical closure
means the executable evidence exists; it does not mean the game has been accepted.

## M9 — release slice and global verification

The historical verification run declared `15/15 = 1.00` while **excluding four critical rows**
(`PROBE-SEED-001`, `PROBE-CHUNK-001`, `PROBE-SCHEDULE-001`, `PROBE-MODB-ISOLATION-001`). Erratum:
given that denominator the honest figure for that run is **15/19 = 0.79 — FAIL**. The three probes
that did not exist were implemented with falsifiable negative controls and the verification was
re-measured over the full 19 rows: **19/19 = 1.00 PASS, 0 exclusions, 0 rows pending**. The history
is not rewritten: both numbers, and the reason they differ, stay on the record.

## Verified facts at the current head (2026-10-27)

Measured on the development head this page travels with, after M34, M35 and the M36/M37 technical
closures landed:

- `cargo test --workspace`: **1,489 tests passed, 0 failed** across **66** test suites.
- `cargo clippy --workspace --all-targets -- -D warnings` and
  `cargo clippy -p daggerfall-app --features window --all-targets -- -D warnings`: clean.
- Behaviour-probe suite `--probe ALL-001`: **40/40 PASS**.
- Independent probes on the same head: `SPINE-001` **12/12**, `VIDEO-001` PASS,
  `PROBE-AUTOMAP-001` **13/13**, `PROBE-LIGHT-001` PASS, `PROBE-DUNGEON-001` **12/12**,
  `SPRITE-001` **11/11**, `BIOME-001` **25 passed / 0 failed / 1 NA** with the declared gap
  `GAP-TAX-11`.
- `scripts/ci/run-ci.sh`: **PASS (3/3)** with `new_hits=0` on the language ratchet.
- UI instruments, cited from their closure records on this same head: `UI-AUDIT-001` blocks A+B+C
  `VERDICT: PASS` (exit 0, two processes, identical stdout) for M36; `UI-BASE-001` **9/9** and
  `UI-INPUT-001` **10/10** `VERDICT: PASS` (exit 0, two processes, byte-identical stdout) for M37.
- UI-1 measured depth cost, cited: **0.4708333 u/window at one QA round** (`113/240`, band `r`,
  n = 3 pilots, W = 50), **8.33×** the L1 baseline of **0.0565 u/window**. The literal form that
  keeps the five QA rounds in the numerator is `2.3541667 u/window` — a retread upper bound, not a
  mission cost.

## Verified facts at the M29 close (2026-10-22)

- `cargo test --workspace`: **854 tests passed, 0 failed** across 36 test suites.
- `cargo clippy --workspace --all-targets -- -D warnings` and
  `cargo clippy -p daggerfall-app --features window --all-targets -- -D warnings`: clean.
- Behaviour-probe suite `--probe ALL-001`: **37/37 PASS** (37 entries; 52 `VERDICT:` emissions over
  43 names, because `DEMO-001` re-runs nine of them). `ALL-001` prints the total for the build you
  run.
- `scripts/ci/run-ci.sh`: **PASS (3/3)** · `scripts/ci/content-scan.sh`: **0/0/0** (no tracked BSA,
  no BSA in the tree, no BSA signature inside binaries), including build artifacts.
- `ASSET-REGISTRY.json` records **492 assets** by path, size and SHA-256 — hashes and metadata only.

## Verified facts at the M18 technical closure (2026-10-20, pre-signature)

- `cargo test --workspace`: **258 tests passed, 0 failed** across 26 test suites.
- Behaviour-probe suite `--probe ALL-001`: **18/18 PASS** at that closure. It has since grown to
  **37/37** at the M29 close.

## Final re-audit of the campaign (M0–M29)

The 30 milestones were re-measured on the closing campaign head by fresh QA lanes: probes and gates
re-executed, verdicts taken from the live tree rather than from the closure reports. Tally: **21 PASS ·
2 PASS-as-shipped (M3, M6: they pass against their campaign cut, with declared scope drift) · 7 FAIL
(M2, M5, M9, M11, M12, M18, M19)**. M9 was a denominator defect and is closed (19/19 = 1.00 above).
Six milestones remain below the bar: **M2** (a tautological traversal counter and no entry tests),
**M5** (`CalculateCost` missing, so prices are about half of DFU), **M11** (`.RDI` unparsed, 0/187),
**M12** (no render proof for the texture layer), **M19** (three parity defects: inverted drowning,
fall damage on slip, `water walking` claimed but absent), and **M18**, open on the owner's signature.

## Known gaps (declared, not hidden)

- **Full-map index — resolved in M18.** `PROBE-MAP-001` resolves **all 15,251 locations** and every
  `.RMB` block they reference, twice, with an identical FNV-1a 64 digest. `MAPS.BSA` holds **62
  region slots: 45 with locations and 17 structurally empty**; the "61 regions" figure is the source
  label (UESP `dfbsa.txt`), not what the file enumerates, and the probe prints what it measures. City
  *geometry* (`PROBE-RMB-001`) still exercises a documented five-city sample; the index-wide guarantee
  is the map probe.
- **Audio data availability.** `MIDI.DAT` and `SOUND.BSA` are absent from some retail installations;
  the parser reports a declared skip instead of faking audio.
- **Visual acceptance.** No probe can sign off "it looks like 1996". That judgement belongs to the
  owner in a side-by-side review against the original; until it is signed, no `M18` tag exists.
- **Test and probe automation.** CI runs the content and trademark scans. The test, clippy and probe
  battery is run locally by contributors and reviewers, not yet as required CI jobs.
- **Windows/macOS.** Development and verification are Linux-first. The Bevy window feature is
  portable, but no Windows/macOS release has been validated yet.
- **Gameplay parity.** M30 is **frozen** (specification only, no code) and M31–M33 are specified and
  not implemented. "100% of the data definition of done" is not "100% of the game"; see
  [Roadmap](roadmap.md).
- **Biome probability gate (`GAP-TAX-11`).** M35's `BIOME-001` cannot measure the tile-class
  probability gate: the pinned corpus has no tile-class data, so the check reports `NA` and the
  probe returns `DEGRADED` (25 passed / 0 failed / 1 NA). The gate is declared, not faked; it stays
  open until a corpus with tile classes exists.
- **Visual acceptance of the presentation pass.** M34's probes measure the spine, the HUD and the
  capture digests; whether the presentation *looks* like 1996 is still the owner's side-by-side
  judgement, exactly as for M18.
