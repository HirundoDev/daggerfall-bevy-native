# How parity is verified

*Case file split from [`docs/ACKNOWLEDGMENTS.md`](../ACKNOWLEDGMENTS.md): the body was moved verbatim; only relative link targets and heading levels were rebased.*

Parity here means "matches the declared source of truth", with the source of truth written down.
Every claim is attached to a probe, a golden vector, or a documented data census.

## 1. Sources of truth

| Source | Role |
|---|---|
| Original retail data (`ARENA2/`, read-only) | Byte-level ground truth for what exists: archives, records, palettes, text |
| UESP BSA format documentation, notably `dfbsa.txt` by Dave Humphrey (2002) | Public format specification for the container and record layouts |
| Daggerfall Unity `v1.1.1`, commit `0038a1e6…` (MIT) | Executable specification for behaviour and formulas that the formats alone do not explain |

The DFU source used as a specification is retained under [`reference/dfu-spec/`](../../reference/dfu-spec/)
with its original MIT license. It is **not** product code: no DFU `.cs` file is compiled, linked or
copied into a crate. Product parsers are clean-room implementations from the format documentation.

## 2. Golden vectors, not screenshots

- **Terrain:** heights are derived from the specification formula, not captured from the
  implementation, so a bug cannot certify itself. The `Mathf.PerlinNoise` port is compared against
  a compiled reference at **0 ULP**.
- **Textures:** decoded pixels are pinned with 12 FNV-1a64 vectors across three retail textures and
  four rotations, read at runtime from `ARENA2` in Mode A.
- **Formulas:** `PROBE-FORMULA-001` checks trade-price outcomes against golden cases.
- **Text and quests:** `TEXT.RSC` tokens (18,501, excluding record terminators), `.QBN` tiling
  (247/247), and `.QRC` headers (244/244) are recomputed from retail bytes and compared exactly.

## 3. Behaviour probes (the anti-screen suite)

The probe suite is the project's answer to "it renders, therefore it works". A probe exercises
behaviour and emits a verdict:

```bash
./scripts/launch.sh --probe ALL-001   # 27/27 at the M26 close
```

`ALL-001` runs 27 probes: `VIEW-001`, `CITY-001`, `INTERIOR-001`, `COMMERCE-001`,
`FORMULA-001`, `WORLD-001`, `READMODEL-001`, `SESSION-001`, `RMB-001`, `SEED-001`, `RDB-001`,
`AUDIO-001`, `TEXTURE-001`, `TERRAIN-001`, `NPC-001`, `TEXT-RSC-001`, `UI-001`, `MAP-001`,
`DEMO-001`, `LOCOMOTION-001`, `STATS-001`, `ITEM-001`, `COMBAT-001`, `MAGIC-001`,
`ENCHANT-001`, `MONSTER-001` and `NPC-AI-001`. The suite grew milestone by milestone: 18 probes at
the M18 freeze, one more per closed gameplay milestone from M19 to M26.

`CLIMATE-001`, `QUEST-ORIGINAL-001`, `STEALTH-001`, `SKILLS-001`, `EQUIP-001`, `LOOT-001`,
`COMBAT-LOOP-001`, `DEATH-001`, `EFFECT-001`, `SPELLMAKER-001`, `ENEMY-AI-001` and `ENCOUNTER-001`
are available as standalone probes.

## 4. CI guardrails

```bash
bash scripts/ci/run-ci.sh
```

Two checks run on every push and pull request:

- **Content scan** — no tracked `.BSA`, no `.BSA` in the working tree, and no BSA signature inside
  binary build artifacts. Threshold: zero.
- **Trademark check** — no unauthorized franchise marks in public surfaces. Internal documentation
  and the third-party specification corpus are classified and reported, not silently excluded.

Both are wired in [`.github/workflows/ci.yml`](../../.github/workflows/ci.yml) and runnable locally with
no remote.

## What is *not* claimed

Passing probes do not mean the game is finished or visually identical. No probe can sign off
"it looks like 1996": that judgement belongs to the owner in a side-by-side review against the
original. Until it is signed, M18 stays owner-pending and no `M18` tag exists.
