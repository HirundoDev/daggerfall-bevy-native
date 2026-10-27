# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

> This changelog was added after the three public tags below and documents them
> retroactively. The tags themselves are immutable and were not moved.
>
> The engine contains no game data. It reads a copy of Daggerfall that you already
> own and keep outside the repository. No retail assets are shipped. This project is
> not affiliated with Bethesda. All releases in this file are pre-releases.

## [Unreleased]

### Added

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

- Public documentation moves M36 and M37 to **DONE at technical level** (2026-10-27)
  with no milestone tag; M38–M48 remain **NOT STARTED**. M36/M37 state is now carried
  by the `status.md` table and by both public roadmaps, and the UI-1 measured cost —
  **0.4708333 u/window at one QA round** (band `r`, n = 3, W = 50), i.e. **8.33×** the
  L1 baseline of **0.0565 u/window** — is published next to the gate instead of living
  only in the mission record.
- Public documentation moves M34 to **DONE at technical level** (2026-10-26) and M35
  to **PARTIAL** with one declared gap; M31–M33 remain **SPEC-ONLY** and M30 stays
  **FROZEN**. Recount then: **12 DONE · 2 PARTIAL · 1 FROZEN · 3 SPEC-ONLY · 13 NOT
  STARTED · 1 separate program**; M36 and M37 lift it to **14 DONE · 11 NOT STARTED**
  (entry above).
- Plan A is therefore **delivered at technical level**, not accepted: the M34 owner
  signature and milestone tag are still pending, and no milestone tag exists for M18
  or later.
- M36 and M37 are closed technically, **not accepted**: their owner signature and
  milestone tag are still pending, exactly as for M34 and M35. **M38 (UI-2) remains
  NOT STARTED** and no UI-2 surface is claimed.
- The workspace declared in `Cargo.toml` spans **16 crates**; the exported surface
  documents the new members by name rather than by count.

### Fixed

- Spanish comments in `crates/promotion-gate/src/{contract,manifest}.rs` translated to
  English. Comments only — no string literal, fixture or byte length was touched.
- `.gitignore` no longer whitelists `.sintergic/artifacts/`, so internal ledger
  artifacts stay out of the public export.

**Verification:** `cargo test --workspace` **1,489 passed / 0 failed** across 66 test
suites; `cargo clippy --workspace --all-targets -- -D warnings` clean and
`cargo clippy -p daggerfall-app --features window --all-targets -- -D warnings` clean;
`scripts/ci/run-ci.sh` **PASS (3/3)** with `new_hits=0`; `--probe ALL-001` **40/40**;
`SPINE-001` 12/12; `VIDEO-001` PASS; `PROBE-AUTOMAP-001` 13/13; `PROBE-LIGHT-001` PASS;
`PROBE-DUNGEON-001` 12/12; `SPRITE-001` 11/11; `BIOME-001` **25 passed / 0 failed /
1 NA** (`GAP-TAX-11`, declared); `UI-AUDIT-001` blocks A+B+C **PASS**, exit 0 in two
separate processes with identical stdout; `UI-BASE-001` **9/9** and `UI-INPUT-001`
**10/10** **PASS**, exit 0 in two separate processes with byte-identical stdout.

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

**Verification:** 854 tests passed / 0 failed; clippy clean for headless and window
features; content/trademark scans 0/0/0; CI runner PASS (3/3); anti-screen suite 37/37;
M9 global verification 19/19.

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

[Unreleased]: https://github.com/HirundoDev/daggerfall-bevy-native/compare/v0.2.0-alpha...HEAD
[0.2.0-alpha]: https://github.com/HirundoDev/daggerfall-bevy-native/compare/v0.1.0-alpha...v0.2.0-alpha
[0.1.0-alpha]: https://github.com/HirundoDev/daggerfall-bevy-native/compare/v0.0.0-alpha.1...v0.1.0-alpha
[0.0.0-alpha.1]: https://github.com/HirundoDev/daggerfall-bevy-native/tree/v0.0.0-alpha.1
