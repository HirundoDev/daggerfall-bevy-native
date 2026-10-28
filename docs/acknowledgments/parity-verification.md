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
./scripts/launch.sh --probe ALL-001   # 40/40 (37/37 at the M29 close)
```

`ALL-001` runs **40 entries** covering M2 to M34 (37 of them M2-M29), starting with the data layer (`VIEW-001`,
`CITY-001`, `INTERIOR-001`, `COMMERCE-001`, `FORMULA-001`, `WORLD-001`, `READMODEL-001`,
`SESSION-001`, `RMB-001`, `SEED-001`, `RDB-001`, `AUDIO-001`, `TEXTURE-001`, `TERRAIN-001`,
`NPC-001`, `TEXT-RSC-001`, `UI-001`, `MAP-001`, `DEMO-001`) and continuing with the gameplay
tramo (`LOCOMOTION-001`, `STATS-001`, `ITEM-001`, `COMBAT-001`, `MAGIC-001`, `ENCHANT-001`,
`MONSTER-001`, `NPC-AI-001`, `GUILD-001`, `CRIME-001`, `LAW-001`, `EXEC-001`, `LIFECYCLE-001`,
`JOURNAL-001` and the loop probes that belong to the milestones they certify). The suite grew
milestone by milestone: 18 probes at the M18 freeze, one more per closed gameplay milestone from
M19, and 37 entries at the M29 close. M34 added its three demo probes (`PLAYSESSION-001`,
`UI-FULL-001`, `AUDIO-RT-001`) on the same entrypoint — 40 entries, spec §11. It prints the count
for the build you run.

`CLIMATE-001`, `QUEST-ORIGINAL-001`, `PROBE-CHUNK-001`, `PROBE-SCHEDULE-001`,
`PROBE-MODB-ISOLATION-001`, `PROGRESSION-001`, `SKILLS-001`, `STEALTH-001`, `EQUIP-001`,
`LOOT-001`, `SPELLMAKER-001` and `ENEMY-AI-001` remain standalone probes.

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
original. M18's **acceptance signature is recorded on 2026-10-26 by delegation** — 8 PASS · 9
NC-ACEPTA · 4 NOT-COMPARABLE · 0 FAIL, tag NINGUNO — and the **ORIGINAL side (Daggerfall 1.07.213 in
DOSBox) is not certified**: comparing the engine rows against the original remains the owner's own
act. What stays open is that comparison and the tag, not the signature.
