# Status: what is actually done

*Case file split from [`docs/ACKNOWLEDGMENTS.md`](../ACKNOWLEDGMENTS.md): the body was moved and later cleaned of working notes; the verified facts are unchanged.*

M0–M17 carry annotated git tags of the form `M<n>-YYYY-MM-DD`, with matching `release/M<n>`
branches (`M13b` is a tagged parity follow-up). M18–M26 have **technical** closures with recorded
evidence but **no tag yet**: the owner signature gates that. The table below states only what those
closures contain.

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
| **M9** | Release slice and global verification | Global verification score ≥ 0.85, 0 critical failures |
| **M10** | Retail city blocks from `BLOCKS.BSA` (`.RMB`) | `PROBE-RMB-001`: 5-city sample, 951/951 building records, 252/252 `ARCH3D` models, 0 traversals |
| **M11** | Retail interiors from `.RDB` + `ARCH3D` | `PROBE-RDB-001`: 187/187 dungeon-block census; a real interior (`N0000055.RDB`) assembled |
| **M12** | Retail texture layer: `TEXTURE.000-010`, `TEXTURE.256`, palettes | 472 `TEXTURE.*` files hashed; 12 FNV-1a64 golden vectors |
| **M13** | Terrain and climate from `WOODS.WLD` / `MAPS.BSA` | `PROBE-TERRAIN-001`, `PROBE-CLIMATE-001` PASS; height field bit-exact |
| **M13b** | `extraNoise` parity (`Mathf.PerlinNoise` port) | **0 ULP** against the compiled reference implementation |
| **M14** | Music (`MIDI.BSA`) and sound effects (`DAGGER.SND`) | `PROBE-AUDIO-001`: 131/132 songs (one without a retail record, declared, not faked), 459 sound records, 60/60 day/night triggers |
| **M15** | NPC population, name banks, captions, spawn guards | `PROBE-NPC-001` 18/18; deterministic population digest reproduced twice |
| **M16** | Lore, text and quests: `TEXT.RSC`, `.QBN`, `.QRC`, quest verbs, books | 18,501 text tokens, 247/247 QBN tiled, 244/244 QRC validated, 82 quest verbs, 79 quests, 91 books |
| **M17** | 1996-style UI (IMG/CIF/FNT/RCI assets, windows, HUD) | `PROBE-UI-001` PASS (43/43 checks, 18 golden vectors); `--features window` build EXIT=0; visual signature deferred to M18 |
| **M18** | Full-map index probe, release scan, final acceptance | **Technical closure PASS (2026-10-20)** — `PROBE-MAP-001` resolves 15,251/15,251 locations (0 skipped, 0 panics, identical digest twice); the suite froze at 18/18 for this milestone and has grown with every later one; **owner side-by-side PENDING**, so no tag exists |
| **M19** | Player locomotion and body: DFU speeds, jump/gravity/fall damage, climbing, levitation, swimming and breath, encumbrance, stealth formulas | **Technical closure PASS (2026-10-21)** — `behavior::locomotion` (21 tests, no Bevy, no I/O) consumed by the demo and the window through a single `Locomotion::step`; `PROBE-LOCOMOTION-001` 70 checks (formulas / runtime / stealth / retail demo), `STEALTH-001` 16 checks |
| **M20–M26** | Gameplay parity, first tramo: stats and skills (M20), items/equipment/loot (M21), combat (M22), magic (M23), enchanting and alchemy (M24), monsters, enemy AI and encounters (M25), living NPCs, faction dialogue and `RUMOR.DAT` rumours (M26) | Each closed at technical level with its probe added to the suite: 18 probes at the M18 freeze → 27/27 at the M26 close. At the M26 close: **534 tests passed / 0 failed**, clippy clean in both configurations |

**Owner signature and tag are pending for M18–M26 alike.** Technical closure means the executable
evidence exists; it does not mean the game has been accepted.

## Verified facts at the M18 technical closure (2026-10-20, pre-signature)

- `cargo test --workspace`: **258 tests passed, 0 failed** across 26 test suites.
- `cargo clippy --workspace --all-targets -- -D warnings` and
  `cargo clippy -p daggerfall-app --features window --all-targets -- -D warnings`: clean.
- Behaviour-probe suite `--probe ALL-001`: **18/18 PASS** at that closure. It has since grown to
  **27/27** at the M26 close; `ALL-001` prints the total for the build you run.
- `scripts/ci/content-scan.sh`: **0/0/0** (no tracked BSA, no BSA in the tree, no BSA signature
  inside binaries), including build artifacts.
- `ASSET-REGISTRY.json` records **491 assets** by path, size and SHA-256 — hashes and metadata only.

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
- **Gameplay parity.** The second tramo (M27–M34) is specified and not implemented. "100% of the data
  definition of done" is not "100% of the game"; see [Roadmap](roadmap.md).
