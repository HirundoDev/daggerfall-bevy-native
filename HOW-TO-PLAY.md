# How to play and how to probe

This guide is for the **`v0.1.0-alpha`** build: how to start it, what the keys actually do, how to
read a behaviour probe, and what is knowingly broken. Every binding below is read from the code that
consumes it (file names are given), not from memory; nothing here is a promise of parity with the
original game.

> **Status, honest:** this is a pre-release. The engine runs end to end and every claim is backed by
> an executable probe, but the owner-signed side-by-side review of M18 is still pending, so the
> alpha carries no `M18` tag and makes **no claim of visual parity**. See
> [Status: what is actually done](docs/acknowledgments/status.md).

## 1. Requirements

- **Rust toolchain** (`cargo` + `rustc`; the release record was verified with 1.97.1).
- **Linux with X11 or Wayland** for the windowed demos. Headless runs and every probe need no display.
- **Your own Daggerfall installation**, kept outside the repository. The engine reads `ARENA2/` at
  runtime and packages no game bytes. Resolution order (see `crates/app/src/main.rs` and
  `crates/app/src/mode_a.rs`):
  - `--data-root <dir>` (launcher flag), else
  - `DF_APP_ROOT`, else
  - `$HOME/Games/Daggerfall/app`.
  The directory must contain `ARENA2/` (either capitalisation).
- **First build takes minutes** (release build for `--mode complete`, debug for `--mode preview`).
  `--print-plan` resolves and prints the exact commands with zero side effects; `--no-build` reuses a
  binary that already exists.

Launcher exit codes (`scripts/launch.sh`): `0` success, `1` build/binary/feature failure, `2` invalid
usage, `3` window requested without a display, `124` timeout, anything else is the probe's own code.

## 2. Start it

```bash
export DF_APP_ROOT="$HOME/Games/Daggerfall/app"
./scripts/launch.sh --mode complete
```

That is the **integrated demo**: a retail `.RMB` city with retail textures, the terrain ring, NPC
billboards, the 1996-style panels and a live character HUD, in a window whose title bar is rewritten
every frame with the live state (`m19_update_title` in `crates/app/src/m18_window.rs`): phase
(`MENU`/`PLAYING`), locomotion mode, the character line (`SPELLSWORD - LV 1 - HP ... - STA ... - MAG
... - RUN ...`), breath, the sea level when the ring has water, and the crime/warrant line when the
city watch is active.

It opens on the **main-menu panel** (the 1996 title screen composition, an overlay over the world).
`Enter` transitions `MENU` to `PLAYING` (measured capture `01-enter-playing` in the owner pack); until
then the world is frozen (0 world frames, 0 clock minutes).

> The published `v0.1.0-alpha` artifact predates the M18 B1/B2/B3 parity fixes. In that artifact the
demo starts directly in the world, without the menu overlay and without the `,` / `.` / `T` bindings
listed below; this tree is the newer, still-unsigned state.

Other entry points, same launcher:

| What | Command | What you get on screen |
|---|---|---|
| Integrated demo (M18) | `./scripts/launch.sh --mode complete` | 647 buildings, 177,137 retail triangles, 136 textures, 3 NPC billboards, 6 UI panels + terrain + HUD; opens on the menu overlay |
| Legacy single-block window (M2) | `./scripts/launch.sh --mode complete -- --window` | one real `ARCH3D.BSA` block, first-person spawn inside it |
| City window (M3) | `./scripts/launch.sh --mode complete --probe CITY-001` | the M3 probe scene: 3x3 synthetic street grid, 6 `ARCH3D.BSA` landmarks, 487 triangles, sky + clouds; opens only after the probe PASSes |
| Interior window (M4) | `./scripts/launch.sh --mode complete --probe INTERIOR-001` | real `.RDB` shop interior, game clock in the title bar |
| Commerce window (M5) | `./scripts/launch.sh --mode complete --probe COMMERCE-001` | the same interior with the merchant Vendel, dialogue and `[B]`/`[V]` prices + gold in the title bar |

The `--probe` windows are **visual evidence** of a headless probe that already ran and passed: the
probe decides, the window only shows. If the probe fails, no window opens.

## 3. Keys, verified

### 3.1 Integrated demo (`--mode complete`)

Input is read in one place (`m2_gather_action` in `crates/app/src/m2_player.rs`); the systems that
consume the actions live in `crates/app/src/m18_window.rs`.

| Key / button | Effect | Where it is consumed |
|---|---|---|
| `Enter` | leave the start menu, one way (`Menu` to `Playing`) | `m18_phase_keys` (m18_window.rs) |
| `W` `A` `S` `D` | walk / strafe (locomotion engine decides the step, not the renderer) | `m2_player.rs`, `m18_apply_player_action` |
| Mouse move | look (raw motion; pitch clamped to +/-1.55 rad) | `m2_player.rs` (`LOOK_SENSITIVITY = 0.0025` rad/px) |
| `Esc` | release the cursor to the desktop. It is **not** a pause and opens no menu | `m2_escape_cursor` |
| `,` / `.` | rotate the view 5 degrees left / right (keyboard yaw) | `m18_yaw_keys` |
| `Space` | jump | `m18_apply_player_action` (M19 engine) |
| `Shift` (left or right) | run | same |
| `Ctrl` (left or right) | crouch | same |
| `C` | sneak toggle: changes the speed the locomotion engine resolves (the mode label stays `grounded`) | `m18_apply_player_action` |
| Left mouse **or** `F` | melee swing (one binding, two keys) | `m14_melee_sfx`, `m22_enemy_retaliation` |
| `G` | bash the nearest building door; a failed bash is a crime and can bring the city watch | `m28_door_bash` |
| `L` | cast Levitate (retail spell-point cost and pool gate; drives the movement flags) | `m23_cast_key` |
| `R` | demo accelerator, not gameplay: 109 `Running` skill uses plus one skill check, then the HUD re-renders | `m20_key_use_skill` |
| `T` | re-sample the terrain between the capital pixel and the nearest coastal pixel; the city does not move | `m18_origin_toggle` |
| `1` | HUD panel | `m18_switch_panel` |
| `2` | inventory panel (retail `IMG`/`CIF`/`FNT`, gold and item counts) | same |
| `3` | quests panel | same |
| `4` | main-menu panel (the same title screen, over the world) | same |

The first four panels follow the composition order returned by `ui_spec::build_all_panels`
(`crates/parser-bevy-bridge/src/ui_spec.rs`): hud, inventory, quests, main-menu. Two further map
panels are composed in this tree but have no key yet, so they are not reachable from the window.

**Keys that exist in the input layer but do nothing in this demo:** `E`, `B`, `V`. They are set by
`m2_gather_action` for the M5 commerce window (below), but the integrated demo composes no dialogue
window, so pressing them is silent.

### 3.2 City window (`--probe CITY-001`)

`W` `A` `S` `D` walk, mouse look, `Esc` release the cursor. Nothing else is bound. This is the M3
probe scene: a deterministic 3x3 street grid with **6 landmarks (487 triangles)** fitted into lots -
low-poly probe geometry, not a retail city layout. The retail city is the integrated demo (647
buildings, 177,137 retail triangles); the retail `.RMB` parser is measured by `PROBE-RMB-001`.

### 3.3 Interior window (`--probe INTERIOR-001`)

`W` `A` `S` `D` walk, mouse look, `Esc` release the cursor. The window title carries the real `.RDB`
block name and the game clock (1 real second = 1 game minute).

### 3.4 Commerce window (`--probe COMMERCE-001`)

| Key | Effect |
|---|---|
| `W` `A` `S` `D` + mouse | walk and look inside the shop |
| `Esc` | release the cursor |
| `E` | talk: opens the dialogue only when the merchant is in range and no wall is in between (otherwise it logs `E with no valid target`); pressed again it closes the dialogue |
| `B` | buy the first item of the menu, paying the DFU price formula (only while the dialogue is open) |
| `V` | sell the first good, receiving the DFU sell price (only while the dialogue is open) |

Gold is always visible in the title bar; prices are recomputed on every title refresh. Commerce is
in-memory only (no persistence).

### 3.5 Legacy block window (`-- --window`)

`W` `A` `S` `D` walk, mouse look, `Esc` release the cursor. This is the M2 path: one real block, no
city, no UI.

## 4. How to read a probe

A probe is a program that prints its own evidence and exits with a code. Run one through the launcher
(default mode is a fast debug build, headless):

```bash
./scripts/launch.sh --probe CITY-001
```

What a green run looks like:

- a header line naming the probe (`PROBE-CITY-001 - ...`);
- one line per check, prefixed `[OK]` or `[PASS]` (failures print `[FAIL]`);
- one final verdict line, `VERDICT: PROBE-CITY-001 PASS - ...`;
- the launcher's own last line, `launch: exit 0`.

How to read the exit code:

| Code | Meaning |
|:---:|---|
| `0` | the probe ran and every check passed |
| `1` | a check failed, or the probe could not read the data it needs (the old M2-M18 probes fail with `1` and a `[FAIL]` row) |
| `2` | **retail data unavailable** in the probes that declare this contract (`PROBE-DEMO-001`, `PROBE-LOCOMOTION-001` "NO DATA"). `2` is not a green run: it means the measurement could not happen |
| `3` | the launcher requested a window but there is no `DISPLAY`/`WAYLAND_DISPLAY` |
| `124` | the run hit the launcher timeout (`--timeout`, default 900 s) |

If a probe fails, in this order: (1) confirm `DF_APP_ROOT` points at a directory that contains
`ARENA2/`; (2) run `./scripts/launch.sh --print-plan --probe <ID>` to see the exact binary, features
and data root it resolved; (3) read the first `[FAIL]` row - it names the check, not just the suffix.

## 5. Probe matrix

One command per row; the last column is the decisive line you should see. Full suite: `ALL-001`.

| Probe | What it measures | Expected decisive line |
|---|---|---|
| `ALL-001` | the unified Anti-Screen suite: **37 probes, M2-M29** | `VERDICT: ANTI-SCREEN SUITE PASS (37/37 probes, exit 0)` |
| `VIEW-001` | 3D viewer over one `ARCH3D.BSA` block, collision coherence | `VERDICT: PROBE-VIEW-001 PASS` |
| `CITY-001` | M3 city street walkable end to end, solid blocks, 0 crossings | `VERDICT: PROBE-CITY-001 PASS` |
| `INTERIOR-001` | street to interior through the door, time cycle, real `.RDB` | `VERDICT: PROBE-INTERIOR-001 PASS` |
| `COMMERCE-001` | enter - interact - buy (gold down, item up) with the DFU price formula | `VERDICT: PROBE-COMMERCE-001 PASS` |
| `FORMULA-001` | the DFU trade-price formula golden vectors | `VERDICT: PROBE-FORMULA-001 PASS` |
| `WORLD-001` | full playthrough cycle exterior - interior - commerce - exit - return | `VERDICT: PROBE-WORLD-001 PASS` |
| `READMODEL-001` | the session read models behind the HUD and panels | `VERDICT: PROBE-READMODEL-001 PASS` |
| `SESSION-001` | session save/load round trip | `VERDICT: PROBE-SESSION-001 PASS` |
| `RMB-001` | retail `.RMB` cities from `MAPS.BSA`/`BLOCKS.BSA` | `VERDICT: PROBE-RMB-001 PASS` |
| `SEED-001` | retail city determinism (same bytes, same city) | `VERDICT: PROBE-SEED-001 (real ext) PASS` |
| `RDB-001` | census of the retail `.RDB` interiors in `BLOCKS.BSA` | `VERDICT: PROBE-RDB-001 PASS` |
| `TEXTURE-001` | retail textures and palettes, digest goldens | `VERDICT: PROBE-TEXTURE-001 PASS` |
| `TERRAIN-001` | retail terrain heights against byte goldens | `VERDICT: PROBE-TERRAIN-001 PASS` |
| `CLIMATE-001` | retail climate index (`CLIMATE.PAK`) | `VERDICT: PROBE-CLIMATE-001 PASS` |
| `NPC-001` | NPC names, population, retail captions | `VERDICT: PROBE-NPC-001 PASS` |
| `TEXT-RSC-001` | `TEXT.RSC` token recomputation | `VERDICT: PROBE-TEXT-RSC-001 PASS` |
| `QUEST-ORIGINAL-001` | the original `QBN`/`QRC` quest census | `VERDICT: PROBE-QUEST-ORIGINAL-001 PASS` |
| `UI-001` | the 1996 UI panels (assets, font, panels) | `VERDICT: PROBE-UI-001 PASS` |
| `MAP-001` | the complete `MAPS.BSA` location index | `VERDICT: PROBE-MAP-001 PASS (resolved=15251/15251, ...)` |
| `DEMO-001` | the integrated demo composition, headless: city + terrain + NPCs + panels + live HUD, digest identical across passes | `VERDICT: PROBE-DEMO-001 PASS` |
| `LOCOMOTION-001` | the DFU locomotion engine: speed, jump, sneak, water | `VERDICT: PROBE-LOCOMOTION-001 PASS` (or `NO DATA` without `ARENA2`) |
| `COMBAT-001` | hit, damage and cadence through the combat engine | `VERDICT: PROBE-COMBAT-001 PASS` |
| `MAGIC-001` | `SPELLS.STD`, spell cost and casting | `VERDICT: PROBE-MAGIC-001 PASS` |
| `AUDIO-001` | `MIDI.BSA` songs and `DAGGER.SND` sound clips, production playback path | `VERDICT: PROBE-AUDIO-001 PASS` |

The suite covers 37 probes across M2-M29 (the members include the ones above plus `STEALTH-001`,
`STATS-001`, `SKILLS-001`, `ITEM-001`, `ENCHANT-001`, `MONSTER-001`, `NPC-AI-001`, `GUILD-001`,
`CRIME-001`, `LAW-001`, `EXEC-001`, `LIFECYCLE-001` and `JOURNAL-001`); `ALL-001` prints the current
list and total. Nothing outside that public set is runnable here: the private M33 work in progress and
the later milestones are not part of this alpha.

`ALL-001` needs the retail data too: without `ARENA2` its members fail and the suite exits `1`.

## 6. Known issues (measured, not fixed in this pass)

These are the defects a player will actually hit. They are documented as they are; this pass changes
no UI code.

- **The menus and panels are pictures, not widgets.** The 1996 panels, including the main-menu overlay,
  are composed as retail images with **0 interactive widgets**. Nothing is clickable, there is no
  button navigation and no character creation; `Enter` is the only way out of the start screen, and
  the same is true of the four world panels (`1`-`4`). This is the "broken UI/menus" state and it is
  not being fixed in this documentation pass. A **seven-axis UI compatibility layer** now exists in
  code — surface, palette, typography, text, navigation, input and presentation, in
  `crates/parser-bevy-bridge/src/ui_compat/` — and it is exercised by **three pilot windows** under
  `UI-BASE-001` (9/9) and `UI-INPUT-001` (10/10). The panels this window draws are still composed by
  `ui_spec::build_all_panels`, not by that layer, so nothing on screen changed: the pilots prove the
  layer works, they are not playable UI.
- **No seamless exterior-to-interior transition in the integrated demo.** Walking in the city does not
  take you inside a building. Interiors are shown by the M4/M5 probe windows instead; the transition
  machinery itself is measured separately by the project, not wired into this window.
- **Audio is silent unless you build the `audio` feature.** `--mode complete` enables only `window`, so
  the runtime logs ``M14 VE: feature `audio` not compiled - NullSink (no device playback)``. Ask for
  both features explicitly: `./scripts/launch.sh --mode complete --features audio,window`. With the
  feature compiled but no usable device, the engine degrades to the same null sink and prints the
  reason instead of panicking.
- **The current composition has no ocean.** The measured run of the current composition logs
  `M18 VE: 0 ocean samples in the ring - no water surface`, so the sea is not reachable by walking in
  it. When a ring does contain water, the surface is rendered at the band elevation and is **not
  walkable** (the adapter stops the player and a jump at the shoreline).
- **`E` does not open a dialogue in the integrated demo.** NPC captions are drawn from `TEXT.RSC`, but
  the dialogue window is not composed there; commerce dialogue exists only in the M5 window.
- **`CITY-001` is the M3 probe scene, not a city.** 3x3 synthetic grid, 6 landmarks, 487 triangles.
  The retail city is the integrated demo; the retail `.RMB` parser is `PROBE-RMB-001`.
- **No nature sprites and no day/night cycle.** Vegetation and the visible night sky were measured by
  the M35 annex probes — `SPRITE-001` 11/11 and `BIOME-001` 25 passed / 0 failed / 1 NA on the declared
  gap `GAP-TAX-11` — and are still not part of this window; the scene lighting does not follow the
  clock yet.
- **No damage numbers on screen.** Combat retaliation is resolved and logged (`M22 VE`), and the HUD
  shows HP, but the hit number is not drawn over the target.
- **Mouse look cannot be scripted.** The demo reads raw mouse motion; synthetic X11 input does not
  produce it, so automated captures keep a fixed yaw. A real mouse works.
- **HP can drop while you stand still.** Enemy retaliation ticks while the window is open.

## 7. What is not in this alpha

`v0.1.0-alpha`, no parity claim. The public probe suite is **37 probes, M2-M29**. M18 is
**code-complete for its B1/B2/B3 parity work but unsigned**: the owner signature is pending, one
acceptance row (CS-09) has an open decision, and two further rows (CS-05 vegetation, CS-11 night) were
measured by the M35 annex - biomes, vegetation and night sky for the already-closed acceptance set,
**not** all buildings, cities or quests - and remain pending the owner's side-by-side. So no `M18` tag
exists.

The private M33 save/load work in progress is **not part of this alpha and not runnable from this
tree's public surface**. M34 closed at technical level on 2026-10-26 and the M35 annex was delivered
the same day; both are unsigned, with no milestone tag. The UI plan has closed its first two gates the
same way on 2026-10-27 - M36 the audit, M37 the seven-axis layer - and neither produced a playable
interface. No date is promised for any of them.

## 8. Where to look next

- [The launcher](docs/acknowledgments/launcher.md) - modes, flags, exit codes, examples.
- [How parity is verified](docs/acknowledgments/parity-verification.md) - sources of truth, golden
  vectors, the probe suite.
- [Status: what is actually done](docs/acknowledgments/status.md) - the per-milestone record and the
  known gaps.
- [Data, legal and trademarks](docs/acknowledgments/legal-and-trademarks.md) - why game data never
  ships here.
