# Status: what is actually done

*Case file split from [`docs/ACKNOWLEDGMENTS.md`](../ACKNOWLEDGMENTS.md): the body was moved and later cleaned of working notes; the verified facts are unchanged.*

**Public release line: `v0.3.0-alpha`, published 2026-10-27.** `v0.3.1-alpha` is in preparation as a
**documentation correction**: it fixes this page and its siblings and adds no feature. The alpha
publishes the mechanism measured below; it does **not** claim visual parity. M18 — the global
acceptance gate — has its **acceptance signature recorded on 2026-10-26 by delegation**: its 21
acceptance rows are signed **8 PASS · 9 NC-ACEPTA · 4 NOT-COMPARABLE · 0 FAIL**. Two acts remain the
owner's own — running the original for the left column of the side-by-side, and cutting the `M18` tag —
so no `M18` tag exists and nothing here claims the game looks like 1996.

M0–M17 carry annotated git tags of the form `M<n>-YYYY-MM-DD`, with matching `release/M<n>`
branches (`M13b` is a tagged parity follow-up). From M18 on no milestone tag exists. The closure
register — checked by `scripts/closure-gate.py` with
`VERDICT: PASS` (0 prose↔register contradictions, falsifiers 3/3 red) — holds **51 milestones and 80 gate
rows: 38 DONE · 1 PARTIAL · 1 PARTIAL-TAG-PENDING · 1 PARTIAL-TECHNICAL-CLOSURE · 10 AUSENTE**, with
`program_closure: OPEN`. Its states, milestone by milestone:

- **M18 — `PARTIAL-TAG-PENDING`.** DONE at technical level: its acceptance signature is recorded
  2026-10-26 by delegation, the tag is none, and the ORIGINAL-side comparison is not certified.
- **M19–M33 — DONE at gate level.** Acceptance pending, no tag.
- **M34 — `PARTIAL-TECHNICAL-CLOSURE`.** DONE at technical level with the named row
  `AUDIO-RT-001-DEVICE`.
- **M35 — `PARTIAL`**, with `GAP-TAX-11` deferred to M44.
- **M36–M39 — DONE at gate level.** Acceptance pending, no tag.
- **M40–M48 — `AUSENTE`.** M46 additionally carries the owner's `T-M46-GO`.

`DONE` here means **gate-level technical closure** — 100% of the milestone's declared gate rows
passing in `PASS` at the recorded head — never owner acceptance. The table below states only what
those closures contain.

| Milestone | Scope | Evidence at closure |
|---|---|---|
| **M0** | Naming/legal guardrails, position gates, CI content + trademark scans | `scripts/ci/run-ci.sh` → PASS (2/2) |
| **M1** | 3D block viewer: `ARCH3D.BSA` → Bevy mesh, window feature, PathChecker | Real block rendered from retail data |
| **M2** | First-person camera and grid collision | **DONE** and tagged `M2-2026-10-19` — `PROBE-VIEW-001` PASS; repaired after the campaign re-audit, final QA **0.98** (`docs/sintergic/l5-mission/final-qa-M2M5M19-CR-380233e5.md`) |
| **M3** | Navigable city streets, deterministic layout, sky | `PROBE-CITY-001` PASS |
| **M4** | Interior prototype and time system; the window renders a real `.RDB` interior | `PROBE-INTERIOR-001` PASS (block A synthetic + B time + C real `RDB`), plus a measured window capture |
| **M5** | Interactive commerce (inventory-lite) and the trade-price formula | **DONE** and tagged `M5-2026-10-19` — `PROBE-COMMERCE-001`; `PROBE-FORMULA-001` with 11 independent golden vectors; repaired, final QA **0.97** (`docs/sintergic/l5-mission/final-qa-M2M5M19-CR-380233e5.md`) plus `dfu_price_pipeline_golden_parity` **40/40** vectors against the mirror of DFU `FormulaHelper.cs` (`CalculateCost:1884-1899` → `CalculateTradePrice:1976-2004`, pin `DFU@2343305d`), its gates byte-identical across two processes — `FORMULA-001` sha256 `ffc0b5e67ca8123aecb30b2af4b27f38519f4f7147bfa62aabf6950e214f9838`, `COMMERCE-001` sha256 `37007115115cac8fd6fa5e574278c64670ca7398d7ad4eb2c074216341d17d8f` (`docs/sintergic/l5-mission/m5-calculate-cost-parity-CR-112b1058.md`) |
| **M6** | Integrated world playthrough and the unified behaviour-probe suite | `PROBE-WORLD-001` PASS |
| **M7** | Session read models: HUD, automap, inventory/quests | `PROBE-READMODEL-001` PASS |
| **M8** | Save/load round-trip of a played session | `PROBE-SESSION-001` PASS |
| **M9** | Release slice and global verification | Global verification **19/19 = 1.00**, 0 critical failures, 19 rows with command + exit + output and **0 exclusions**. See the erratum below |
| **M10** | Retail city blocks from `BLOCKS.BSA` (`.RMB`) | `PROBE-RMB-001`: 5-city sample, 951/951 building records, 252/252 `ARCH3D` models, 0 traversals |
| **M11** | Retail interiors from `.RDB` + `ARCH3D` | **DONE** and tagged `M11-2026-10-20` — repaired after the campaign re-audit, QA **0.99** (`docs/sintergic/l5-mission/mission-report-m11-CR-90853661.md`); `PROBE-RDB-001`: 187/187 dungeon-block census; a real interior (`N0000055.RDB`) assembled |
| **M12** | Retail texture layer: `TEXTURE.000-010`, `TEXTURE.256`, palettes | **DONE** and tagged `M12-2026-10-20` — repaired, digest `a41b36eb9bae1bab` reproduced twice (`docs/sintergic/l5-mission/mission-report-fix-m12-minors-CR-d54b4324.md`); 472 `TEXTURE.*` files hashed; 12 FNV-1a64 golden vectors |
| **M13** | Terrain and climate from `WOODS.WLD` / `MAPS.BSA` | `PROBE-TERRAIN-001`, `PROBE-CLIMATE-001` PASS; height field bit-exact |
| **M13b** | `extraNoise` parity (`Mathf.PerlinNoise` port) | **0 ULP** against the compiled reference implementation |
| **M14** | Music (`MIDI.BSA`) and sound effects (`DAGGER.SND`) | `PROBE-AUDIO-001`: 131/132 songs (one without a retail record, declared, not faked), 459 sound records, 60/60 day/night triggers |
| **M15** | NPC population, name banks, captions, spawn guards | `PROBE-NPC-001` 18/18; deterministic population digest reproduced twice |
| **M16** | Lore, text and quests: `TEXT.RSC`, `.QBN`, `.QRC`, quest verbs, books | 18,501 text tokens, 247/247 QBN tiled, 244/244 QRC validated, 82 quest verbs, 79 quests, 91 books |
| **M17** | 1996-style UI (IMG/CIF/FNT/RCI assets, windows, HUD) | `PROBE-UI-001` PASS (43/43 checks, 18 golden vectors); `--features window` build EXIT=0; visual signature deferred to M18 |
| **M18** | Full-map index probe, release scan, final acceptance | **Technical closure PASS (2026-10-20)** — `PROBE-MAP-001` resolves 15,251/15,251 locations (0 skipped, 0 panics, identical digest twice); the suite froze at 18/18 for this milestone and has grown with every later one; the 21 acceptance rows were measured on the real window and **signed by delegation on 2026-10-26** (**8 PASS · 9 NC-ACEPTA · 4 NOT-COMPARABLE · 0 FAIL**); the comparison against the original game and the tag remain the owner's, so no tag exists |
| **M19** | Player locomotion and body: DFU speeds, jump/gravity/fall damage, climbing, levitation, swimming and breath, encumbrance, stealth formulas | **DONE at gate level** — `behavior::locomotion` (21 tests, no Bevy, no I/O) consumed by the demo and the window through a single `Locomotion::step`; `PROBE-LOCOMOTION-001` **88/88** (stdout 11,904 B, sha256 `9de0cfdd7fe8d74818d9bb87618beca0ef7a9aabcf3b65c6438433c85a937b52`) and `PROBE-STEALTH-001` **16/16** (1,501 B, sha256 `a59c64550da912765902962635374af84128d82adab63410844de1f7134a9d43`), each measured in **two separate processes, byte-identical**; NC1–NC5 each turned its expected rows red (exit 1) and the source sha256 was restored to `c0e34bb2…` — `docs/sintergic/l5-mission/m19-closure-CR-44034827.md`. Acceptance pending, no `M19` tag |
| **M20–M26** | Gameplay parity, first tramo: stats and skills (M20), items/equipment/loot (M21), combat (M22), magic (M23), enchanting and alchemy (M24), monsters, enemy AI and encounters (M25), living NPCs, faction dialogue and `RUMOR.DAT` rumours (M26) | **DONE at gate level** (register verdict DONE; acceptance pending, no tag) — each closed at technical level with its probe added to the suite: 18 probes at the M18 freeze → 27/27 at the M26 close. At the M26 close: **534 tests passed / 0 failed**, clippy clean in both configurations. M20–M29 (and M34) were re-measured in `CR-c62fc6d3`: **29 gate rows, each run in two separate OS processes byte-identical**, binary `target/debug/daggerfall-app` sha256 `f0b9ba86…` at tree `e9e7908`, plus **11 negative controls** turned red and restored — manifest `docs/sintergic/l5-mission/evidence/re-measure-29-CR-c62fc6d3/manifest.json` |
| **M27** | Guilds and factions: `FACTION.TXT` graph (365 ids), reputation with tree propagation, ranks 0–9, per-rank services | **DONE at gate level** — `PROBE-FACTION-001` 17/17, `PROBE-GUILD-001` 33/33, `M27-DEMO-001` 15/15; the suite went from 27 to 28 probes. Re-measured in `CR-c62fc6d3`: its gate rows ran in two separate OS processes byte-identical, binary sha256 `f0b9ba86…` at tree `e9e7908` (`docs/sintergic/l5-mission/evidence/re-measure-29-CR-c62fc6d3/manifest.json`). Acceptance pending, no `M27` tag |
| **M28** | Crime, law and courts: penalty clamps, legal reputation, city guards, court sentence, prison clock | **DONE at gate level** — `PROBE-CRIME-001` and `PROBE-LAW-001` measure `steps=262`, `5480/375 → 2740/187` and `clock_advanced=16,156,800` against an independent recomputation. Re-measured in `CR-c62fc6d3`: its gate rows ran in two separate OS processes byte-identical, binary sha256 `f0b9ba86…` at tree `e9e7908` (`docs/sintergic/l5-mission/evidence/re-measure-29-CR-c62fc6d3/manifest.json`). Acceptance pending, no `M28` tag |
| **M29** | Playable retail quests: the `QBN` opcode interpreter, clocks, journal, lifecycle | **DONE at gate level** — `PROBE-QUEST-EXEC-001` (24 golden vectors, a 20-quest determinism trace with its seed falsifier), `LIFECYCLE-001`, `JOURNAL-001`; `covered=62 negative=7 uncovered=0`; census 11/11 over 247 `.QBN` / 244 `.QRC`. Re-measured in `CR-c62fc6d3`: its gate row ran in two separate OS processes byte-identical, binary sha256 `f0b9ba86…` at tree `e9e7908` (`docs/sintergic/l5-mission/evidence/re-measure-29-CR-c62fc6d3/manifest.json`). Acceptance pending, no `M29` tag |
| **M30** | Economy: shops, inns, banking, transport — `CalculateCost` with regional prices, the 36 `(building, group)` buy table, five trade modes, 53 holidays, loans and default, house and ship | **Technical closure PASS (2026-10-23)** — `behavior::economy` (including `BankLedger` and `LoanEvent`) measured by `PROBE-SHOP-001`, `PROBE-ECONOMY-001` (E1–E28 vectors plus a seeded-determinism block) and `PROBE-BANK-001`, all exit 0 with the digest reproduced twice. No `M30` tag |
| **M31** | Travel, weather and calendar: fast travel over the real climate data, weather replay, holidays and the calendar clock | **Technical closure PASS (2026-10-23)** — `heart::{travel, weather, calendar}` measured by `PROBE-TRAVEL-001` (34/34 vectors, a 2,200-pixel route over the real `CLIMATE.PAK`, byte-identical stdout in two processes), `PROBE-WEATHER-001` (13/13 vectors, 240,000 draws, `max_abs_delta = 0.0121 < 0.02`) and `PROBE-CALENDAR-001` (14/14). No `M31` tag |
| **M32** | Conditions, death and rest: diseases, vampirism and lycanthropy stages, rest and the death state | **Technical closure PASS (2026-10-23)** — `skills::{conditions, rest}` measured by `PROBE-CONDITION-001` (24/24), `PROBE-VAMPIRE-001` (16/16), `PROBE-REST-001` (10/10) and `PROBE-DEATH-STATE-001` (2/2). No `M32` tag |
| **M33** | Total persistence: savegame v2, migration, and the classic `SAVEGAME` read/write | **Technical closure PASS (2026-10-26)** — `memory::{save_v2, savegame, savegame_v2}` measured by `PROBE-SAVE-ALL-001` (24/24 round trips byte-identical, the S0 census, five corruption mutations all rejected, deterministic digest), `PROBE-SAVE-MIGRATION-001` (v1→v2, idempotent), `PROBE-SAVEGAME-READ-001`, `PROBE-SAVEGAME-WRITE-001` and `PROBE-M33-DEMO-001`. No `M33` tag |
| **M34** | End-to-end integration and presentation: the E2E spine, the full presentation pass and the HUD | **`PARTIAL-TECHNICAL-CLOSURE`** — DONE at technical level with the named row `AUDIO-RT-001-DEVICE`. `PLAYSESSION-001`, `UI-FULL-001` and `AUDIO-RT-001` entered the suite, taking `ALL-001` to **40/40**; the independent probes agree on the same head: `SPINE-001` **12/12** (4 controls, 6 falsifiers, identical digest over two runs), `VIDEO-001` PASS, `PROBE-AUTOMAP-001` **13/13**, `PROBE-LIGHT-001` PASS (digest reproduced in-process, `0x61d950d0d13da8de`), `PROBE-DUNGEON-001` **12/12**. QA round **0.94**, 0 critical / 0 major. Owner rows recorded **12/12** (5 PASS · 7 accepted with a declared deviation · 0 FAIL). No `M34` tag. `AUDIO-RT-001` reproduces `PASS(degraded)` — `[SKIP-DEVICE]`, the binary built without the `audio` feature — so its measure covers the degraded path only and the audio branch is the owner's decision. `UI-FULL-001`, also the M40 gate, counts the 34 non-playable surfaces as `N/A-engine` entries of its own census and measures only the 51 playable windows (51/51): it neither depends on those 34 nor certifies them |
| **M35** | Biomes, 2D vegetation and sky — optional, non-blocking annex | **`PARTIAL`** — the register's verdict, with the named row `GAP-TAX-11` **deferred to M44**. `BIOME-001` **25 passed / 0 failed / 1 NA** and `SPRITE-001` **11/11`. The NA is that declared gap: the pinned corpus carries no tile-class data, so the probability gate cannot be measured — the probe reports `DEGRADED` instead of inventing a number. Owner rows CS-05 (`NC-ACEPTA`) and CS-11 (`NOT-COMPARABLE`) are declared classes of the signed acceptance matrix, not passes, and the side-by-side against the original remains the owner's. No `M35` tag |
| **M36** | UI-0: audit of the census windows, the patch/rewrite rule, the seven-axis compatibility contract and the E1 re-estimate | **Technical closure measured 2026-10-27** — `scripts/ui-audit-001.py` prints `VERDICT: PASS` on blocks A+B+C, exit 0, in two separate processes with byte-identical stdout; **85 = 78 surfaces (27 + 51) + 7 infrastructure rows**, measured **4 implemented / 47 partial / 34 absent**, and the rule settles **35 PATCH · 50 REWRITE**. No `M36` tag |
| **M37** | UI-1: the seven-axis UI compatibility layer — surface, palette, typography, text, navigation, input, presentation — and its three pilots | **Technical closure measured 2026-10-27** — `crates/parser-bevy-bridge/src/ui_compat/` shipped behind two gates: `UI-BASE-001` **9/9** and `UI-INPUT-001` **10/10**, `VERDICT: PASS`, exit 0, in two separate processes with byte-identical stdout. The pilots measured a depth cost of **0.4708333 u/window at one QA round** — band `r`, n = 3, W = 50 — against the L1 baseline of **0.0565 u/window** (**8.33×**). No `M37` tag |
| **M38** | UI-2: the 27 surfaces outside the in-play screens — 16 creation windows (`C01`–`C16`) and 11 configuration pages (`CFG01`–`CFG11`) — plus the menu family | **Technical closure measured 2026-10-27** — `scripts/ui-create-001.py` (16 surfaces, 12 checks) and `scripts/ui-config-001.py` (11 surfaces, 12 checks) print `VERDICT: PASS`, exit 0, in two separate processes with identical stdout and identical digests (`249a028172da0e98c2b50186b65e782d9e054356dcfaf3b888a18e7170ebd813`, `6dd450e3b387415bbe4e1004529c22390336c82198ccfc4f183f5da5209df79e`). Measured cost **0.8434 u over 27 surfaces = 0.0312 u/surface**. The UI requirement's coverage at that close is **0.857 (6/7)**: **K7** (the owner's side-by-side acceptance) is the only open clause, with **K2** (the composed `FACES.CIF` face) closed and its handoff recording `status: DONE`. No `M38` tag |
| **M39** | UI-3: the 51 interactive playable windows, in the eight blocks of the plan | **DONE at gate level** (register verdict DONE, 3 gate rows) — `ui-jugable-001` gate **9/9 PASS ×2** at HEAD `5803297` (stdout 29,539 B, sha256 `1843dc66…`, digest `d93dc12f…`), AXIS A5 **`focus-rows=153` ×2**, **4/4 falsifiers red**; 51/51 surfaces measured, 8/8 blocks and 7/7 axes OK. `requirements_coverage 0.857` with **K7** the single open clause (`docs/sintergic/l5-mission/m39-a5-focus-mission-report-CR-4e32cea3.md`). Not signed, no `M39` tag, and the 51 windows are not yet the running demo window |
| **M40–M48** | Plan B remaining: UI-4 gate, SYS-1–SYS-3 depth, DUN-1 dungeon content, E4 mods, E5 online, E6 platform, E7 modern | **`AUSENTE`** — no code, no dispatch, no tag; M46 additionally carries the owner's `T-M46-GO`. Scope, gates and `u` are in the [complete plan](roadmap-m18-m48.md) |
| **E8** | Deferred expansion horizon | **`AUSENTE`** — a separate program with its own genesis contract, never "M49" |

**No milestone tag exists from M18 on; M18's acceptance signature is recorded by delegation on
2026-10-26.** Technical closure means the executable evidence exists; it does not mean the game has
been accepted, and every milestone after M18 is still unsigned. M39 is DONE at gate level: its three
gate rows pass, `ui-jugable-001` prints `9/9 PASS ×2` at HEAD `5803297`, AXIS A5 measures
`focus-rows=153` twice and 4/4 falsifiers turn red, and the closure report — kept at
`docs/sintergic/l5-mission/m39-a5-focus-mission-report-CR-4e32cea3.md` — publishes
`requirements_coverage 0.857` with K7 (the owner's side-by-side acceptance) as the single open clause.
M40–M48 are `AUSENTE` in the register, and E8 is a separate program.

## M9 — release slice and global verification

The historical verification run declared `15/15 = 1.00` while **excluding four critical rows**
(`PROBE-SEED-001`, `PROBE-CHUNK-001`, `PROBE-SCHEDULE-001`, `PROBE-MODB-ISOLATION-001`). Erratum:
given that denominator the honest figure for that run is **15/19 = 0.79 — FAIL**. The three probes
that did not exist were implemented with falsifiable negative controls and the verification was
re-measured over the full 19 rows: **19/19 = 1.00 PASS, 0 exclusions, 0 rows pending**. The history
is not rewritten: both numbers, and the reason they differ, stay on the record.

## Verified facts at the current head (2026-10-27)

Measured on the development head this page travels with, after the M30–M33 organs and the M36–M38
technical closures landed:

- `cargo test --workspace`, refreshed measurement: **1,528 passed / 0 failed / 3 ignored across 59
  test binaries** on the exported tree of `c63f342` (exit 0), and **1,606 passed / 0 failed / 3
  ignored across 66 test binaries** in the private workspace at `137e30a`. The delta of **78** is the
  excluded test files (43 `#[test]` in the four files the export omits, plus 35 tests from the three
  promotion-gate test files); [Export limits](export-limits.md) gives the file-by-file account and the
  reproduction commands. The **1,532 passed / 0 failed** (3 ignored) figure is kept as the
  **M38-close historical anchor of the private tree**: it is not a count of this head and it is
  published only with that label. An earlier cut, `673b8a9`, printed **1,524 passed / 0 failed / 3
  ignored** across 59 test binaries on its public export and **1,602 / 0 / 3** across 66 in the
  private workspace at the same commit.
- `cargo clippy --workspace --all-targets -- -D warnings` and
  `cargo clippy -p daggerfall-app --features window --all-targets -- -D warnings`: clean at the same
  head.
- Behaviour-probe suite `--probe ALL-001`: **40/40 PASS**.
- Independent probes on the same head: `SPINE-001` **12/12**, `VIDEO-001` PASS,
  `PROBE-AUTOMAP-001` **13/13**, `PROBE-LIGHT-001` PASS, `PROBE-DUNGEON-001` **12/12**,
  `SPRITE-001` **11/11**, `BIOME-001` **25 passed / 0 failed / 1 NA** with the declared gap
  `GAP-TAX-11`.
- Gameplay organs, cited from their closure records: `PROBE-SHOP-001`, `PROBE-ECONOMY-001` and
  `PROBE-BANK-001` (M30); `PROBE-TRAVEL-001` 34/34, `PROBE-WEATHER-001` 13/13 with
  `max_abs_delta = 0.0121` over 240,000 draws and `PROBE-CALENDAR-001` 14/14 (M31);
  `PROBE-CONDITION-001` 24/24, `PROBE-VAMPIRE-001` 16/16, `PROBE-REST-001` 10/10 and
  `PROBE-DEATH-STATE-001` 2/2 (M32); `PROBE-SAVE-ALL-001` 24/24 with five rejected corruption
  mutations, `PROBE-SAVE-MIGRATION-001`, `PROBE-SAVEGAME-READ-001`, `PROBE-SAVEGAME-WRITE-001` and
  `PROBE-M33-DEMO-001` (M33).
- `scripts/ci/run-ci.sh`: **PASS (3/3)** with `new_hits=0` on the language ratchet.
- UI instruments, cited from their closure records on this same head: `UI-AUDIT-001` blocks A+B+C
  `VERDICT: PASS` (exit 0, two processes, identical stdout) for M36; `UI-BASE-001` **9/9** and
  `UI-INPUT-001` **10/10** `VERDICT: PASS` (exit 0, two processes, byte-identical stdout) for M37;
  `UI-CREATE-001` (16 surfaces, 12 checks) and `UI-CONFIG-001` (11 surfaces, 12 checks)
  `VERDICT: PASS`, exit 0, in two processes with identical stdout and identical digests
  (`249a028172da0e98c2b50186b65e782d9e054356dcfaf3b888a18e7170ebd813` and
  `6dd450e3b387415bbe4e1004529c22390336c82198ccfc4f183f5da5209df79e`) for M38.
- UI-1 measured depth cost, cited: **0.4708333 u/window at one QA round** (`113/240`, band `r`,
  n = 3 pilots, W = 50), **8.33×** the L1 baseline of **0.0565 u/window**. The literal form that
  keeps the five QA rounds in the numerator is `2.3541667 u/window` — a retread upper bound, not a
  mission cost.
- UI-2 measured cost, cited: **0.8434 u consumed over 27 surfaces = 0.0312 u/surface**. The UI
  requirement's coverage at the M38 close is **0.857 (6/7)**: the owner's side-by-side acceptance
  (K7) is the only open clause, and the composed `FACES.CIF` face (K2) is closed — its handoff
  records `status: DONE` and the UI-2 probe prints the `FACE … match=ok` and `FACENAV` lines.

### Public export of the earlier cut

The run recorded here is the earlier cut: its figures are measured from commit
`673b8a92f2ec7dbe9663d443fd91e91c75cd50b6` — the content commit that cut follows, exported from a
clean detached worktree — with `tools/public-export/export-public.py --release-commit 673b8a9` and
verified by `tools/public-export/verify-export.py --export <dir> --regen-dir <dir2>`, which returned
**PASS** on all five passes A, B, C, D and E:

- `A_tree` **PASS** — **516 files**, **10,292,276 bytes**, **0 broken links**, README first line
  `# daggerfall-bevy-native`, no secret, no personal address outside the upstream allowlist
  (119 upstream addresses tolerated).
- `B_gitobjects` **PASS** — **600 objects / 516 blobs**, **1 commit**, **no remotes**.
- `C_regeneration` **PASS** — regeneration into a second directory is byte-identical: same tree
  `08560a768acd5884cf092e9a2f82d41e1bae39da`, same export commit
  `30fa584c47e18b5483837965cea891dcb2291810`, 0 files present on only one side, 0 content diffs.
- `D_file_by_file` **PASS** — 516 expected from the tree / 516 exported, **502 verbatim + 14
  transformed** (name substitution and internal-link neutralisation), `manifest_sha256`
  `da05a23aab7c1fa67a77c62ffcc7c36e3c364df6a10ba37338457d923a4d9618`, 0 missing, 0 extra and
  0 unexplained content.
- `E_internal_refs` **PASS** — the internal-reference metric [Export limits](export-limits.md)
  declares, with its command and its tolerance rows: `unallowlisted []` and `stale []` at that
  commit. Its counts are printed by the verifier at export time and are not restated on this page.

The commit was exported twice from the same worktree and both runs carry the same export commit and
the same tree, so the run is identified by export commit **and** tree: the export's own report
carries `commit` and neither `tree` nor `manifest_sha256`, while `manifest_sha256` is reported by
pass D of the verifier. Each figure is measured at one named commit. `docs/sintergic/**` and
`.sintergic/**` lie outside the export, so edits there change nothing in it; a documentation commit
elsewhere moves the byte count and `manifest_sha256` of **that commit's** export but does not alter
the verdicts **of the commit measured above** — and an edit that adds a dangling link, a secret or an
unresolvable reference can still fail `A_tree`. That is why every run is recorded with its commit and
its verdicts in the lane's HH-1, not asserted here for later trees.

**Stability of these figures.** The figures of the earlier cut are measured at `673b8a9`. The commit
that adds this section publishes no figure of itself, and the export regenerates byte for byte
identical from `673b8a9`. A count measured on the commit that carries this page is not published
here: the page is part of what it measures, so the run that carries it is the verifier's to print at
export time. The excluded test files move the test count, not the export verdicts.

The metric, the four excluded test files and every count behind them live in [Export limits](export-limits.md).

## Verified facts at the M29 close (2026-10-22)

- `cargo test --workspace` at the campaign re-audit base `6d48146`: **854 tests passed, 0 failed**
  across 36 test binaries, a delta of **0** against the M9 close
  (`docs/sintergic/l5-mission/final-reaudit-six-CR-acc1b551.md` §2). It is the figure of that base —
  not a count of this head.
- `cargo clippy --workspace --all-targets -- -D warnings` and
  `cargo clippy -p daggerfall-app --features window --all-targets -- -D warnings`: clean.
- Behaviour-probe suite `--probe ALL-001`: **37/37 PASS** (37 entries; 52 `VERDICT:` emissions over
  43 names, because `DEMO-001` re-runs nine of them). `ALL-001` prints the total for the build you
  run.
- `scripts/ci/run-ci.sh`: **PASS (3/3)** · `scripts/ci/content-scan.sh`: **0/0/0** (no tracked BSA,
  no BSA in the tree, no BSA signature inside binaries), including build artifacts.
- `ASSET-REGISTRY.json` records **492 assets** by path, size and SHA-256 — hashes and metadata only.

## Verified facts at the M18 technical closure (2026-10-20; acceptance signed 2026-10-26)

- `cargo test --workspace`: **258 tests passed, 0 failed** across 26 test suites.
- Behaviour-probe suite `--probe ALL-001`: **18/18 PASS** at that closure. It has since grown to
  **37/37** at the M29 close.

## Final re-audit of the campaign (M0–M29)

The 30 milestones were re-measured on the closing campaign head by fresh QA lanes: probes and gates
re-executed, verdicts taken from the live tree rather than from the closure reports. Tally: **21 PASS ·
2 PASS-as-shipped (M3, M6: they pass against their campaign cut, with declared scope drift) · 7 FAIL
(M2, M5, M9, M11, M12, M18, M19)**. M9 was a denominator defect and is closed (19/19 = 1.00 above).
Six milestones were below the bar at that re-audit: **M2** (a tautological traversal counter and no entry tests),
**M5** (`CalculateCost` missing, so prices are about half of DFU), **M11** (`.RDI` unparsed, 0/187),
**M12** (no render proof for the texture layer), **M19** (three parity defects: inverted drowning,
fall damage on slip, `water walking` claimed but absent), and **M18**, whose acceptance was unsigned
when the re-audit ran; its 21 rows were signed by delegation on 2026-10-26 (0 FAIL: 8 PASS / 9
NC-ACEPTA / 4 NOT-COMPARABLE).

That tally is history, not the state of the tree. The five milestones it put below the bar are DONE
after their repairs, each with the report cited: **M2** (final QA **0.98**), **M5** (final QA
**0.97** plus the 40/40 DFU price parity with both gate digests `ffc0b5e6…` and `37007115…`), **M11**
(QA **0.99**), **M12** (digest `a41b36eb9bae1bab` reproduced twice) and **M19**
(`PROBE-LOCOMOTION-001` **88/88** and `PROBE-STEALTH-001` **16/16**, measured twice in separate
processes, with NC1–NC5 red and the source digest restored) — in
`final-qa-M2M5M19-CR-380233e5.md`, `m5-calculate-cost-parity-CR-112b1058.md`,
`mission-report-m11-CR-90853661.md`, `mission-report-fix-m12-minors-CR-d54b4324.md` and
`m19-closure-CR-44034827.md`, all under `docs/sintergic/l5-mission/`. **M9**'s denominator defect is
closed: the global verification now runs 19/19 with the full denominator. **M18** is
**`PARTIAL-TAG-PENDING`** — its acceptance signature is on record by delegation (2026-10-26), while
the tag and the ORIGINAL-side comparison remain the owner's.

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
  owner in a side-by-side review against the original. M18's acceptance signature — recorded by
  delegation on 2026-10-26 — certifies the engine's 21 measured rows; it does not stand in for that
  comparison, and no `M18` tag exists.
- **Test and probe automation.** CI runs the content and trademark scans. The test, clippy and probe
  battery is run locally by contributors and reviewers, not yet as required CI jobs.
- **Windows/macOS.** Development and verification are Linux-first. The Bevy window feature is
  portable, but no Windows/macOS release has been validated yet.
- **Playable surface.** M30–M33 are **implemented at technical level** — economy and banking, travel
  with weather and calendar, conditions/death/rest and the total savegame, each with its module, its
  probe and a recorded closure. Their consumer today is the probe; the running demo window does not
  expose them, so they add mechanism, not playability. The 27 UI-2 surfaces of M38 are measured by
  their own gates and are not composed into that window either, and M39 — the 51 interactive playable
  windows — reached technical closure at coverage 0.857 but is not signed and its windows are not yet
  the running demo window. "100% of the data definition of done" is not "100%
  of the game"; see [Roadmap](roadmap.md).
- **UI acceptance (`K7`).** The M38 surfaces pass their gates, but acceptance is the owner's
  side-by-side review against the original window; that decision is open and no `M38` tag exists.
- **Internal paths inside the exported tree.** The export does not copy anything under
  `docs/sintergic/` or `.sintergic/`, yet the exported tree still quotes a bounded set of such paths
  as plain text. Every functional line among them is carried by a tolerance row in
  `tools/public-export/internal-refs-allowlist.txt`, and the verifier reports both failure lists
  empty (`unallowlisted []`, `stale []`) when it runs over the finished tree. The check measures
  `path:line` references, not files — that is the metric, and nothing here claims it counts whole
  files; the counts themselves are printed by pass E at export time and are not restated here. The
  two load-bearing references of the
  earlier cut are closed by declared means: `crates/side-contract/tests/matrix.rs` is excluded by
  **exact path** in `tools/public-export/exclude.txt` (never by a `tests/` wildcard), and the
  `DEFAULT_LEDGER` of `crates/promotion-gate/src/main.rs` is **allowlisted with its reason** in
  `tools/public-export/internal-refs-allowlist.txt`. No silent tolerance: every remaining kept
  reference is either non-functional or carries its declared row. The full metric, the four excluded
  test files and the behaviour of every reference are declared in [Export limits](export-limits.md).
- **Biome probability gate (`GAP-TAX-11`).** M35's `BIOME-001` cannot measure the tile-class
  probability gate: the pinned corpus has no tile-class data, so the check reports `NA` and the
  probe returns `DEGRADED` (25 passed / 0 failed / 1 NA). M35 is therefore **`PARTIAL`** in the closure
  register, and the named row `GAP-TAX-11` is **deferred to M44**; the gate is declared, not faked.
- **Visual acceptance of the presentation pass.** M34's probes measure the spine, the HUD and the
  capture digests; whether the presentation *looks* like 1996 is still the owner's side-by-side
  judgement, exactly as for M18.
