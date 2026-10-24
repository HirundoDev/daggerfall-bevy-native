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

_No unreleased changes._

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
