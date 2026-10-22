# Crates, data custody and determinism

*Case file split from [`docs/ACKNOWLEDGMENTS.md`](../ACKNOWLEDGMENTS.md): the body was moved verbatim; only relative link targets and heading levels were rebased.*

## Crates (13 workspace members)

| Crate | Responsibility |
|---|---|
| `daggerfall-app` | Bevy application, CLI, systems, probe dispatch |
| `daggerfall-nervous` | Organ bus and event vocabulary |
| `daggerfall-heart` | Calendar/time (`DaggerfallDateTime` model), session orchestration |
| `daggerfall-memory` | Session snapshot format, save/load, chunk map |
| `daggerfall-behavior` | Quest log and gameplay rules |
| `daggerfall-perception` | HUD/automap/inventory read models and sensory pipelines |
| `daggerfall-skills` | Stats and the clean-room trade-price formula |
| `daggerfall-inventory` | PathChecker A/B data custody, commerce, configuration |
| `daggerfall-crafting` | Enchantment and alchemy on top of the M23 effect registry and M21 item instances |
| `daggerfall-combat` | Melee attack resolution, enemy melee timer and death sequence (M22) |
| `daggerfall-antibodies` | Invariants and guardrails |
| `daggerfall_parser_rs` | Clean-room parsers: BSA containers, `ARCH3D`, `BLOCKS`/`.RMB`/`.RDB`, `MAPS`, `WOODS.WLD`, climates, textures/palettes, images, MIDI, `DAGGER.SND`, `TEXT.RSC`, `NAMEGEN.DAT`, DFRandom |
| `daggerfall-parser-bevy-bridge` | Pure `MeshSpec`/material/UV/Image conversion into Bevy handles |

The parser is a **leaf**: zero engine dependencies, testable in isolation, with no knowledge of
Bevy. The bridge is pure where it matters (`MeshRecord → MeshSpec`) and only touches Bevy at the
edges. The app owns data custody (`PathChecker`) and decides which mode is active.

## Data custody: Mode A vs. Mode B

`PathChecker` (`daggerfall-inventory`) is the single predicate for "where does game data come
from". Mode A reads the player's own installation; Mode B is reserved for a custom universe. The
mode is never spread as ad-hoc strings across systems.

## Determinism

- A single seed drives world generation and name generation; the same seed reproduces the same
  city and the same NPC names byte-for-byte.
- Probes compare digests, not pixels: `PROBE-SEED-001` (city), `PROBE-NPC-001` (population),
  `PROBE-FORMULA-001` (trade-price outcomes).
- Headless is the default build. A probe can run in CI without a GPU, and a graphical run cannot
  silently change simulation results.
