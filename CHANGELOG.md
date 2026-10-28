# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

> This changelog was added after the public tags below and documents them
> retroactively. The tags themselves are immutable and were not moved.
>
> The engine contains no game data. It reads a copy of Daggerfall that you already
> own and keep outside the repository. No retail assets are shipped. This project is
> not affiliated with Bethesda. All releases in this file are pre-releases.

## [0.3.1-alpha] - 2026-10-27

This release adds no feature. It corrects the public record so that it matches the code and
the closure records that already shipped in `v0.3.0-alpha`. The date is the landing commit's date —
`git log -1 --format=%ad` on the commit that adds this entry — and no release act or tag is claimed
by it.

### Changed

- Release line corrected in the README, `HOW-TO-PLAY.md`, `status.md`, both roadmaps, the
  agent entry point and this file: **`v0.3.0-alpha` published 2026-10-27**, with `v0.3.1-alpha`
  dated by this entry and carrying no tag. No milestone tag exists for M18 or later, and no release
  claims full visual parity with the original game.
- The public record now quotes the closure register's own matrix and verdicts
  (`docs/sintergic/validation/closure-register-CR-03390267.md`, checked by `scripts/closure-gate.py` →
  `VERDICT: PASS` (0 prose↔register contradictions, falsifiers 3/3 red): **51 milestones
  · 80 gate rows · 38 DONE · 1 PARTIAL · 1 PARTIAL-TAG-PENDING
  · 1 PARTIAL-TECHNICAL-CLOSURE · 10 AUSENTE**, `program_closure: OPEN`. `DONE` in public prose is
  **gate-level technical closure, never owner acceptance**, and every `DONE` carries its register
  verdict beside it.
- M2, M5, M11, M12 and M19 move to **DONE**, after repair and re-measurement: M2 final QA **0.98**;
  M5 final QA **0.97** plus **40/40** golden parity vectors against the pinned DFU
  `FormulaHelper.cs`, with `FORMULA-001` sha256 `ffc0b5e6…` and `COMMERCE-001` sha256 `37007115…`
  identical in two separate processes; M11 QA **0.99**; M12 digest `a41b36eb9bae1bab` reproduced
  twice; M19 `LOCOMOTION-001` **88/88** (sha256 `9de0cfdd…`) and `STEALTH-001` **16/16** (sha256
  `a59c6455…`), both measured twice, with NC1–NC5 each driven red and restored. The reports are
  `final-qa-M2M5M19-CR-380233e5.md`, `m5-calculate-cost-parity-CR-112b1058.md`,
  `mission-report-m11-CR-90853661.md`, `mission-report-fix-m12-minors-CR-d54b4324.md` and
  `m19-closure-CR-44034827.md`, all under `docs/sintergic/l5-mission/`.
- M20–M29 move to **DONE** on a real re-measurement rather than an inherited figure: the 29 gate rows
  of M20–M29 and M34 were re-derived from the live tree at `e9e7908` (CR-c62fc6d3), each row run in
  two separate OS processes with byte-identical stdout and equal sha256, plus **11** negative controls
  mutated red and restored — manifest
  `docs/sintergic/l5-mission/evidence/re-measure-29-CR-c62fc6d3/manifest.json`.
- M30 (economy and banking), M31 (travel, weather and calendar), M32 (conditions, death and
  rest) and M33 (total savegame) move from **FROZEN / SPEC-ONLY** to **DONE at technical
  level** — 2026-10-23, and M33 on 2026-10-26 — each with the organ module and the probe ids
  that measure it, replacing the previous "no code" statement.
- M34 is stated **`PARTIAL-TECHNICAL-CLOSURE`**, not closed: the named row `AUDIO-RT-001-DEVICE`
  remains open as the owner's decision on the audio branch. `AUDIO-RT-001` reproduces
  `PASS(degraded)` (`[SKIP-DEVICE]`, binary built without the audio feature), so its two-process
  measure covers the degraded path only; and `UI-FULL-001` — also the M40 gate — counts the **34**
  non-playable surfaces as `N/A-engine` entries of its own census and measures only the **51**
  playable windows (**51/51**), so its `PASS` neither depends on those 34 nor certifies them.
- M35 is stated **`PARTIAL`**, with the named row `GAP-TAX-11` deferred to **M44**; its measured
  figures do not move (`BIOME-001` **25 passed / 0 failed / 1 NA**, `SPRITE-001` **11/11**).
- M38 (the 27 UI-2 surfaces) moves to **DONE at technical level** (2026-10-27) and M39 (the 51
  interactive playable windows) is stated **DONE at gate level** — coverage **0.857**, K7 open, not
  signed, no tag. No UI-2 or UI-3 surface is claimed to be part of the running demo window.
- M40–M48 are stated **`AUSENTE`**: no code, no dispatch, no tag. E8 is **`AUSENTE`** too — a
  separate program with its own genesis contract, never "M49".
- The E8 rename scrub is complete: the retired identifier returns **0** hits across the public
  surface, and no page reintroduces it.
- Every "implemented, therefore playable" implication is replaced by the consumer boundary:
  for M30–M33 the consumer today is the probe, and composing those organs into the demo window
  is M39/M40 work; for M38 the consumer is the surface gate.
- The plan's recount is replaced by the register's own tally — **38 DONE · 1 PARTIAL · 1
  PARTIAL-TAG-PENDING · 1 PARTIAL-TECHNICAL-CLOSURE · 10 AUSENTE** over 80 gate rows — and the M33
  residual that the plan carried as a contingency is recorded as fired.
- The M18 record now carries the canonical truth wherever the release state is stated:
  **`PARTIAL-TAG-PENDING`**, acceptance signature **recorded 2026-10-26 by delegation**,
  **8 PASS · 9 NC-ACEPTA · 4 NOT-COMPARABLE · 0 FAIL** (21/21), no tag, and the original side of the
  comparison not certified.
- The entries below `[0.3.0-alpha]` and older are left as published: each records what its release
  stated at its own date, including labels that the register has since replaced. Where any of them
  differs from the register, the register wins, and the correction lives in this entry and in the
  public pages it names.

**Verification:** the release is cut from the content commit `9f9cc3b` — a clean detached
worktree — and exports **516 files** with tree `e465804a…`. Exported twice and verified by
`tools/public-export/verify-export.py --export <dir> --regen-dir <dir2>`, the run returns **PASS on
all five passes**:

- `A_tree` **PASS** — **516 files**.
- `B_gitobjects` **PASS**.
- `C_regeneration` **PASS** — the second export into another directory is byte-identical: same tree
  `e465804a…`, 0 files present on only one side, 0 content diffs.
- `D_file_by_file` **PASS**.
- `E_internal_refs` **PASS** — `unallowlisted []` and `stale []`. The line itself is printed by the
  verifier over the finished tree and is not restated here: this file travels inside the export, so
  it is part of the surface that pass measures, and a count written into it moves the count.

The two internal references that the previous cut carried are closed by a declared mechanism rather
than by tolerance creep: `crates/side-contract/tests/matrix.rs` is excluded by its **exact path** in
`tools/public-export/exclude.txt` — never by a `tests/` wildcard — and the `DEFAULT_LEDGER` of
`crates/promotion-gate/src/main.rs` lives allowlisted, with its reason, in
`tools/public-export/internal-refs-allowlist.txt`.

A release is identified by its name and the commit it was cut from, never by a figure of its own
export. The engineering record (`docs/sintergic/` and the other internal trees) lies outside the
export, so a commit that touches only it moves neither the tree nor the exported bytes; an edit to an
exported file moves both, which is why this entry cites the tree of the release and not the tree of
the commit that carries these sentences.

**Test count of the exported tree:** `cargo test --workspace` prints **1,528 passed / 0 failed / 3
ignored** on the exported tree of `c63f342` across **59** test binaries, and **1,606 passed / 0
failed / 3 ignored** in the private workspace at `137e30a` across **66**; `crates/` is
byte-identical at `c63f342` and at the release cut `9f9cc3b`, so the count belongs to the code
and not to one commit's prose. The **78**-test delta is
the set of test files excluded from the export by exact path;
[Export limits](docs/acknowledgments/export-limits.md) names them, counts their `#[test]` attributes
and explains the gap. The 1,532 figure recorded at the M38 close stays as the historical anchor of
the private tree, and the earlier cut of this entry was measured on the export of `673b8a9` —
**1,524 passed / 0 failed / 3 ignored** on its export (59 binaries) against **1,602 / 0 / 3** in the
private workspace (66); those figures stay attributed to that commit.

**Stability of these figures.** Every number above is measured at the commit named beside it, and
the commit that adds this paragraph publishes no figure of itself. An edit to an exported file moves
the byte count, the tree and the exported content of *that* commit and can overturn `A_tree`: a
dangling link, a secret, an internal name or an address outside the allowlist each fails that pass,
so the five verdicts hold only for the commit measured and not for later trees. The final run's
commit and verdicts are recorded in the lane's HH-1.

## [0.3.0-alpha] - 2026-10-27

### Added

- M30 — economy and banking: `behavior::economy` with `CalculateCost` and regional prices, the
  36 `(building, group)` buy table, five trade modes, 53 holidays, loans and default, house and
  ship (including `BankLedger` and `LoanEvent`), measured by `PROBE-SHOP-001`,
  `PROBE-ECONOMY-001` (E1–E28 vectors plus a seeded-determinism block) and `PROBE-BANK-001`.
- M31 — travel, weather and calendar: `heart::travel`, `heart::weather` and `heart::calendar`,
  measured by `PROBE-TRAVEL-001` (34/34 vectors, a 2,200-pixel route over the real
  `CLIMATE.PAK`, byte-identical stdout in two processes), `PROBE-WEATHER-001` (13/13 vectors,
  240,000 draws, `max_abs_delta = 0.0121 < 0.02`) and `PROBE-CALENDAR-001` (14/14).
- M32 — conditions, death and rest: `skills::conditions` and `skills::rest` (diseases,
  vampirism and lycanthropy stages, rest, death state), measured by `PROBE-CONDITION-001`
  (24/24), `PROBE-VAMPIRE-001` (16/16), `PROBE-REST-001` (10/10) and `PROBE-DEATH-STATE-001`
  (2/2).
- M33 — total persistence: `memory::save_v2`, `memory::savegame` and `memory::savegame_v2` —
  savegame v2, the v1→v2 migration and the classic `SAVEGAME` read/write — measured by
  `PROBE-SAVE-ALL-001` (24/24 byte-identical round trips, the S0 census, five corruption
  mutations rejected, deterministic digest), `PROBE-SAVE-MIGRATION-001`,
  `PROBE-SAVEGAME-READ-001`, `PROBE-SAVEGAME-WRITE-001` and `PROBE-M33-DEMO-001`.
- M38 — the 27 UI-2 surfaces: 16 creation windows (`C01`–`C16`) and 11 configuration pages
  (`CFG01`–`CFG11`), measured by `scripts/ui-create-001.py` (16 surfaces, 12 checks) and
  `scripts/ui-config-001.py` (11 surfaces, 12 checks). Both print `VERDICT: PASS`, exit 0, in
  two separate processes with identical stdout and identical digests
  (`249a028172da0e98c2b50186b65e782d9e054356dcfaf3b888a18e7170ebd813` and
  `6dd450e3b387415bbe4e1004529c22390336c82198ccfc4f183f5da5209df79e`). Requirements
  coverage at the M38 close: **0.714** (open: K2 the composed `FACES.CIF` face, K7 the owner's
  side-by-side acceptance). K2 closed afterwards: coverage **0.857 (6/7)**, with **K7 the only
  open item** — HH-1 `HH-20261027-og-orquestador-m38-k2`, `status: DONE`.
- M36 — UI-0 audit: the census of the 85 windows recomputed from the code by
  `scripts/ui-audit-001.py` (blocks A+B+C), measuring **85 = 78 surfaces (27 + 51) + 7
  infrastructure rows**, **4 implemented / 47 partial / 34 absent**, and settling the
  patch/rewrite rule at **35 PATCH · 50 REWRITE**. Technical closure 2026-10-27, no tag.
- M37 — UI compatibility layer: seven axes (surface, palette, typography, text,
  navigation, input, presentation) in `crates/parser-bevy-bridge/src/ui_compat/`,
  exercised by **3 pilot windows** behind `UI-BASE-001` (**9/9**) and `UI-INPUT-001`
  (**10/10**). Technical closure 2026-10-27, no tag. The layer is not yet the
  composition path of the demo window, so it adds no playability.
- Six workspace crates that make the engine boundary explicit: `game-core` (engine
  contract), `side-contract` (side-facing contract), `game-runtime-api` (runtime
  boundary), `content-api` + `content-runtime` (content loading split) and
  `promotion-gate` (deterministic promotion gate with a signed manifest).
- M34 — end-to-end integration and presentation: one continuous session from load to
  combat, inventory and save, with the HUD and the presentation layer wired to the
  measured organs.
- M35 — biomes, 2D vegetation and sky, delivered as a non-blocking annex for the
  `0.1.0` surface.
- `HOW-TO-PLAY.md`: player-facing quickstart (build, bring your own data, first
  session, controls), so the entry path is not buried in the README.

### Changed

- Public documentation moves M36, M37 and M38 to **DONE at technical level** (2026-10-27)
  with no milestone tag; M39 is **IN PROGRESS** and M40–M48 remain **NOT STARTED**. The state
  is carried by the `status.md` table and by both public roadmaps, and the UI-1 measured cost —
  **0.4708333 u/window at one QA round** (band `r`, n = 3, W = 50), i.e. **8.33×** the
  L1 baseline of **0.0565 u/window** — is published next to the gate instead of living
  only in the mission record. The UI-2 cost is **0.0312 u/surface** (0.8434 u over 27).
- Public documentation moves M34 to **DONE at technical level** (2026-10-26) and M35
  to **PARTIAL** with one declared gap. Recount at this release:
  **19 DONE · 2 PARTIAL · 1 IN PROGRESS · 9 NOT STARTED · 1 separate program**.
- Plan A is therefore **delivered at technical level**, not accepted: the M34 owner
  signature and milestone tag are still pending, and no milestone tag exists for M18
  or later.
- M36, M37 and M38 are closed technically, **not accepted**: their owner signature and
  milestone tag are still pending, exactly as for M34 and M35. No UI-2 surface is claimed
  as playable in the demo window, and **M39's gate is not closed**: its A–H runner and probe are
  in the tree since `85979b9` and `UI-JUGABLE-001` passes its 8 rules in two processes, while the
  closure report and the owner signature are still pending.
- The workspace declared in `Cargo.toml` spans **16 crates**; the exported surface
  documents the new members by name rather than by count.

### Fixed

- Spanish comments in `crates/promotion-gate/src/{contract,manifest}.rs` translated to
  English. Comments only — no string literal, fixture or byte length was touched.
- `.gitignore` no longer whitelists `.sintergic/artifacts/`, so internal ledger
  artifacts stay out of the public export.

**Verification:** `cargo test --workspace` **1,532 passed / 0 failed** (3 ignored) at the
M38 close; `cargo clippy --workspace --all-targets -- -D warnings` clean and
`cargo clippy -p daggerfall-app --features window --all-targets -- -D warnings` clean;
`scripts/ci/run-ci.sh` **PASS (3/3)** with `new_hits=0`; `--probe ALL-001` **40/40**;
`SPINE-001` 12/12; `VIDEO-001` PASS; `PROBE-AUTOMAP-001` 13/13; `PROBE-LIGHT-001` PASS;
`PROBE-DUNGEON-001` 12/12; `SPRITE-001` 11/11; `BIOME-001` **25 passed / 0 failed /
1 NA** (`GAP-TAX-11`, declared); `UI-AUDIT-001` blocks A+B+C **PASS**, exit 0 in two
separate processes with identical stdout; `UI-BASE-001` **9/9** and `UI-INPUT-001`
**10/10** **PASS**, exit 0 in two separate processes with byte-identical stdout;
`UI-CREATE-001` (16 surfaces) and `UI-CONFIG-001` (11 surfaces) **PASS**, exit 0 in two
separate processes with identical digests.

## [0.2.0-alpha] - 2026-10-23

### Added

- M18 B3 parity tanda: street population and city watch, retail dialogue and journal
  overlays, both map panels, and visual precipitation, with captured window evidence
  (PNG + SHA-256) for each of the seven closure states.
- M18 B2 interior transit: world-to-interior and interior-to-world traversal through
  doors, with the supporting probe.
- M18 evidence tooling for reproducible window captures and checksums.

### Changed

- Public documentation records M18 as a **technical candidate**, M30 as **FROZEN**,
  and M31–M35 as **SPEC-ONLY**.
- The M18 owner-signed acceptance remains **PENDING**; there is no `M18` tag and no
  claim of full visual parity. The 21 acceptance rows remain 3 PASS · 6 PARTIAL · 12 NO.

**Verification:** `cargo check --workspace --all-targets` clean; `cargo test
--workspace` 905 passed / 0 failed; window suite 157 passed / 0 failed; content scan
0 findings; trademark and English-surface checks PASS.

## [0.1.0-alpha] - 2026-10-22

### Added

- M19–M29 gameplay organs: locomotion, character, items/equipment/loot, combat,
  magic, enchanting/alchemy, monsters and enemy AI, living NPCs, factions and guilds,
  crime/law/courts, and playable retail quests with the QBN opcode interpreter,
  clocks, journal, and quest lifecycle.
- The 1996-style UI shell, save/load, and the probe/CI harness.
- The full-map index over all 15,251 locations of `MAPS.BSA`.

### Changed

- M30 economy/banking is frozen as a specification; M31–M34 are specified, not
  implemented.
- M18 remains open: the owner-signed side-by-side review is pending, so there is no
  `M18` tag and no claim of full visual parity. The 21 acceptance rows remain
  3 PASS · 6 PARTIAL · 12 NO.

**Verification:** 854 tests passed / 0 failed across 36 test binaries — the count recorded for this
cut, re-measured Δ 0 at the campaign re-audit base `6d48146`
(`docs/sintergic/l5-mission/final-reaudit-six-CR-acc1b551.md` §2); the count is a property of that
tree, not of this one. clippy clean for headless and window features; content/trademark scans
0/0/0; CI runner PASS (3/3); anti-screen suite 37/37; M9 global verification 19/19.

## [0.0.0-alpha.1] - 2026-10-21

### Added

- First public, sanitized source snapshot of the engine.
- M0–M27 implemented at technical level with probe evidence. M0–M17 are closed and
  tagged; M18 passed its technical closure with `PROBE-MAP-001` resolving all 15,251
  locations of `MAPS.BSA`; M19–M27 added the first gameplay organs.

### Changed

- M18–M27 owner-signed acceptance and milestone tags are pending.
- M28–M34 are specified, not implemented.

**Verification:** 582 tests passed / 0 failed at the M27 close; anti-screen suite 28/28;
content and trademark scans clean.

[0.3.1-alpha]: https://github.com/HirundoDev/daggerfall-bevy-native/compare/v0.3.0-alpha...HEAD
[0.3.0-alpha]: https://github.com/HirundoDev/daggerfall-bevy-native/compare/v0.2.0-alpha...v0.3.0-alpha
[0.2.0-alpha]: https://github.com/HirundoDev/daggerfall-bevy-native/compare/v0.1.0-alpha...v0.2.0-alpha
[0.1.0-alpha]: https://github.com/HirundoDev/daggerfall-bevy-native/compare/v0.0.0-alpha.1...v0.1.0-alpha
[0.0.0-alpha.1]: https://github.com/HirundoDev/daggerfall-bevy-native/tree/v0.0.0-alpha.1
