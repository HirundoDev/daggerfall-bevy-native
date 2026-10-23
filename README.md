# daggerfall-bevy-native

A data-driven, first-person 2.5D RPG engine written in Rust on top of Bevy, built to reproduce the
look, rules and world of **Daggerfall** from the original retail data files — and to verify that
reproduction with golden vectors instead of screenshots.

The engine itself contains **no game data**. It reads a copy of Daggerfall that you already own and
keep outside this repository. See [Data, legal and trademarks](docs/acknowledgments/legal-and-trademarks.md).

> **Release status (honest): `v0.1.0-alpha` — pre-release, M18 is still open.** M0–M29 exist and
> each has a recorded technical closure. The only closure still missing is the **owner-signed
> side-by-side review** of M18 — the global acceptance gate — whose 21 acceptance rows measured on
> the real window are **3 PASS · 6 PARTIAL · 12 NO**. So this alpha carries **no `M18` tag**, makes
> **no claim of full visual parity** with the original game, and the **0.0.0** release line ("living
> country, fully offline") remains in progress. A technical closure is not an accepted milestone,
> and this README does not blur the two.

## What `v0.1.0-alpha` means

This is the first public pre-release. The mechanism is playable end to end and every claim below is
backed by an executable probe or golden vector, but the acceptance gate is unsigned and part of the
presentation is not at parity. Concretely:

- **In the alpha:** the retail data layer (M10–M18, including the full-map index over all 15,251
  locations); the gameplay organs from M19–M29 — locomotion, character, items, combat, magic,
  enchanting, monsters, NPCs, factions, law and quests; the 1996-style UI shell; save/load; and the
  probe/CI harness that measures all of it.
- **Measured and below the acceptance bar:** M2, M5, M11, M12 and M19 on named parity defects, and
  M18 because its closure is the owner's signature (**3 PASS · 6 PARTIAL · 12 NO**, 12 rows still
  NO). The tally is published in [Status](docs/acknowledgments/status.md#final-re-audit-of-the-campaign-m0m29).
- **Not in the alpha:** M30 economy and banking is **frozen** (written specification, no code);
  M31–M34 — travel/weather/calendar, conditions/death/rest, total savegame and end-to-end UI
  integration — are specified and **not implemented**; and the visual side-by-side against the
  original has not been signed.
- **No parity claim:** nothing here should be read as "visually identical to Daggerfall", "feature
  complete" or "release quality". What is verified is the mechanism, probe by probe.

## Status: what is actually done

M0–M17 are closed and tagged. M18 passed its **technical** closure on 2026-10-20 (`PROBE-MAP-001`
resolves all **15,251** locations of `MAPS.BSA`); its 21 acceptance rows were then measured on the
real window (**3 PASS · 6 PARTIAL · 12 NO**) and the owner's signature is pending. M19–M29 each
closed at technical level, each adding its probes to the suite. At the M29 close: **854 tests passed
/ 0 failed**, clippy clean for both headless and `--features window`, the content/trademark scans are
0/0/0, the CI runner is **PASS (3/3)** and the anti-screen probe suite prints **37/37**. The global
verification of M9 re-measures **19/19 = 1.00** with its full denominator (the earlier `15/15` claim
excluded four critical rows; the erratum is recorded in [Status](docs/acknowledgments/status.md)).

- **Closed with evidence:** M19 locomotion · M20 stats/skills · M21 items, equipment and loot ·
  M22 combat · M23 magic · M24 enchanting/alchemy · M25 monsters, enemy AI and encounters ·
  M26 living NPCs (schedules, faction dialogue, `RUMOR.DAT` rumours) · M27 guilds and factions
  (`FACTION.TXT` graph, reputation with tree propagation, ranks 0–9 and per-rank services) ·
  M28 crime, law and courts (penalty clamps, legal reputation, guards, sentence and prison clock) ·
  M29 playable retail quests (the `QBN` opcode interpreter, clocks, journal and lifecycle). The
  requirements each milestone closes are recorded with their acceptance checks.
- **Measured, not assumed:** a final re-audit re-ran every probe and gate on the campaign head and
  put six milestones below the acceptance bar — M2, M5, M11, M12 and M19 on named parity defects, and
  M18 because its closure is the owner's signature. The tally and the residual defects are published
  in [Status](docs/acknowledgments/status.md#final-re-audit-of-the-campaign-m0m29).
- **In flight:** nothing at code level. Owner signature and tag are pending for M18–M29 alike.
- **Frozen:** M30 economy and banking — a written specification, no code, frozen by owner decision.
- **Specified, not implemented:** M31–M34 — travel/weather/calendar, conditions/death/rest, total
  savegame, and end-to-end integration with the full UI. One written specification each, no code.
- **Design landed 2026-10-21 (no code):** instance taxonomy and biomes, procedural classification and
  asset modularity, and a generative-asset catalogue with a VRAM budget — design only.

The full per-milestone record — scope and evidence at each closure, the verified facts and the
declared known gaps — is in [Status: what is actually done](docs/acknowledgments/status.md).

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
enchanting, monsters, NPCs, factions, law and quests), each probe-backed; M30 is frozen and M31–M34
are specified and not implemented. A fork is welcome to start from exactly what is measurable.

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

**Still missing — the rest of the gameplay-parity phase (M30–M34), not “out of scope”:** the full
economy (shops, inns, banking, transport; the milestone is frozen at specification), fast
travel/weather/calendar, diseases/vampirism/lycanthropy, rest and death, a save that covers every
system, and the remaining UI windows. Guilds and factions (M27), crime/law/courts (M28) and playable
retail quests (M29) are implemented and probe-backed; their parity defects found by the final re-audit
are named in [Status](docs/acknowledgments/status.md). **“100% of the data DoD” is not “100% of the
game.”** That distinction, the module proposals M19–M34 and the cost decision behind their order are
recorded with the plan in [Roadmap](docs/acknowledgments/roadmap.md).

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
  so probes run with no window and no GPU — which is what makes the 37-entry suite CI-usable.
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
   anti-screen probe suite (**37/37** at the M29 close) and `cargo test --workspace` measured **854
   passed / 0 failed** there. Golden vectors and digests — 0 ULP on the Perlin port, FNV-1a64
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

**What is not claimed.** No judgement is automated: every tag since M18 waits for the owner's
side-by-side signature. "AI-native" means the loop is machine-executable, not that review left it.

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
