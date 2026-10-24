# How to play and how to probe

This guide is for the published `v0.2.0-alpha` build — the commit of `main` that the annotated
`v0.2.0-alpha` tag points at. It covers what you can actually do in the window, the keys that are
bound in code, how to read the probe harness that measures the engine, and what is known to be
broken. Everything below is read from the code in this repository, not from memory.

## 1. What you are running

`daggerfall-app` is a Bevy 0.20 engine that reads your own copy of Daggerfall's game data (through
`ARENA2/`) and plays it in a window, or measures it headless. It is an engine, not a finished game:
it ships no assets and it claims no parity.

## 2. Requirements

| Need | Why |
|---|---|
| Rust toolchain (`cargo`, `rustc`) | builds the release binary |
| Daggerfall game data | an `ARENA2/` folder with `ARCH3D.BSA`, `MAPS.BSA`, `BLOCKS.BSA`, `TEXTURE.*`, `NAMEGEN.DAT`, `TEXT.RSC`, `MIDI.BSA`, `DAGGER.SND`, `CLIMATE.PAK`, `SPELLS.STD`, `MAGIC.DEF`, `QBN`/`QRC` |
| `DF_APP_ROOT` | folder under which `ARENA2/` lives, when you do not want to pass `--data-root` |
| A display (X11 or Wayland) | only for the window; every probe runs headless with no GPU |

```bash
export DF_APP_ROOT="$HOME/Games/Daggerfall/app"     # or: ./scripts/launch.sh --data-root /path/to/Daggerfall/app
```

## 3. Start it

```bash
./scripts/launch.sh --mode complete    # release build, opens the window
./scripts/launch.sh --mode preview     # fast debug build (default)
./scripts/launch.sh --mode complete --no-build      # skip the build, reuse the binary
./scripts/launch.sh --mode complete --print-plan    # print what would run, execute nothing
```

`--mode complete` builds the parser, the bridge, the runtime and `daggerfall-app` in release and
opens the window. **The window opens on the retail main-menu panel** (`main-menu`, `TITL00I0` +
`MENU.IMG` + two buttons) with every world system gated: the clock stays at 0 minutes and nothing in
the scene moves until you press `Enter`, which enters the integrated demo (`--mode complete` with no
`--probe`). The transition is one-way. The panel is a picture: there is no character creation, no
new-game flow and no options screen behind the buttons.

Log lines worth knowing while the window is open:

- `M14 VE: feature \`audio\` not compiled - NullSink (no device playback)` — the build has no audio
  feature; music and sound effects do not play.
- `M18 VE: 0 ocean samples in the ring - no water surface` — the current composition has no ocean.
- `M20 VE: HUD could not be uploaded to the GPU` — the HUD texture did not reach the GPU; the HUD
  text is then not visible.

## 4. Keys, and the code that binds them

The integrated demo is `M18`; the keys below are bound in the window crate and in the player input
system. Every row cites the file that consumes the key.

| Key | What it does | Source |
|---|---|---|
| `W` `A` `S` `D` | walk / strafe (`W` forward, `S` back, `A` left, `D` right) | `crates/app/src/m2_player.rs` (`ActionState`) |
| Mouse move | look / turn (raw motion; yaw and pitch) | `crates/app/src/m2_player.rs` |
| Left click | attack (trigger `ActionState.Attack`, resolved by the combat engine) | `crates/app/src/m2_player.rs` |
| `Shift` (either) | run | `crates/app/src/m2_player.rs` |
| `Ctrl` (either) | crouch | `crates/app/src/m2_player.rs` |
| `Esc` | release the cursor (the window stays open; press again to reclaim input) | `crates/app/src/m2_player.rs` |
| `Enter` | leave the main menu and enter the world (one-way: `Menu` ⇒ `Playing`) | `crates/app/src/m18_window.rs` (`m18_phase_keys`) + `crates/app/src/m18_phase.rs` |
| `1` `2` `3` `4` | show one of the four 1996 world panels: HUD, inventory, quests, main menu | `crates/parser-bevy-bridge/src/ui_spec.rs` + `crates/app/src/m18_window.rs` |
| `5` | show the `map-local` panel (`MAP100I0.IMG`) | `crates/app/src/m18_window.rs` (`m18_switch_panel`) |
| `6` | show the `map-world` panel (`TRAV0I00.IMG`) | `crates/app/src/m18_window.rs` (`m18_switch_panel`) |
| `,` `.` | rotate the view ∓/± 5° (`YAW_KEY_STEP_RAD`), works in both phases | `crates/app/src/m18_window.rs` (`m18_yaw_keys`) |
| `T` | move the terrain origin between the capital pixel and the nearest coastal pixel of `WOODS.WLD` (re-samples and respawns the terrain) | `crates/app/src/m18_window.rs` (`m18_origin_toggle`) |
| `J` | toggle the retail dialogue overlay (`TEXT.RSC`; one overlay at a time) | `crates/app/src/m18_window.rs` (`m18_overlay_keys`) |
| `I` | toggle the retail journal overlay | `crates/app/src/m18_window.rs` (`m18_overlay_keys`) |
| `O` | advance to the next retail climate of `CLIMATE.PAK` that precipitates, rewriting the precipitation sheet in place | `crates/app/src/m18_window.rs` (`m18_weather_key`) |
| `F` | attack — the second binding of the one `attack` flag (the first is the left click) | `crates/app/src/m2_player.rs` (`attack_triggered`) |
| `Space` | jump (DFU locomotion) | `crates/app/src/m2_player.rs` (`ActionState.jump`) |
| `C` | sneak toggle (feeds the stealth engine: detection radius and noise) | `crates/app/src/m2_player.rs` |
| `R` | save the session (`M20`) | `crates/app/src/m2_player.rs`, `crates/memory/src/lib.rs` |
| `L` | load the last session save (`M20`) | `crates/app/src/m2_player.rs`, `crates/memory/src/lib.rs` |
| `G` | bash the nearest door if one is inside the door range (`M28`); otherwise nothing | `crates/app/src/m2_player.rs`, `crates/daggerfall-bevy-bridge/src/lib.rs` |

Inside the **commerce window** (`--mode complete --probe COMMERCE-001`, `M5`) additional keys apply:

| Key | What it does |
|---|---|
| `E` | talk to the merchant / close the dialogue |
| `B` | buy the first item of the menu, paying the DFU price formula |
| `V` | sell the first good, receiving the DFU sell price |
| `Enter` | close the dialogue |
| `M` | (in the shop) open the merchant's stock |

The panels are pictures: `1`–`6` choose which surface is drawn, and nothing on them is clickable
(there is no widget model yet). Mouse look and `,`/`.` yaw both drive the same camera; the mouse
cannot be scripted, which is why the automated captures use the keys.

## 5. How to read a probe

```bash
./scripts/launch.sh --probe CITY-001      # one probe, headless, exits with its code
./scripts/launch.sh --probe ALL-001       # the whole suite: 37 members, M2-M29
```

Each probe prints one row per check (`[OK]` / `[FAIL]`) and one decisive verdict line:

```
VERDICT: PROBE-CITY-001 PASS - street <name> walkable end-to-end, solid blocks, 0 crossings
```

Exit codes (the launcher's own contract):

| Code | Meaning |
|---|---|
| `0` | pass: every check green |
| `1` | a check failed, or retail data is unreadable |
| `2` | retail data unavailable to the probes that declare that contract (nothing was measured) |
| `3` | a window was requested without a display |
| `124` | timeout (probe took longer than the runner allows) |

If a probe fails, read the first `[FAIL]` row: it names the check. `2` is not a green run and `124` is
not a verdict; in both cases nothing was measured.

## 6. The suite: 37 members, M2-M29

`ALL-001` is the suite runner, not a member. Each member also runs on its own with `--probe <ID>`.
Expected decisive line for every member is `VERDICT: PROBE-<ID> PASS` (with its detail after `-`).

| Member | What it measures | Expected PASS (prefix) |
|---|---|---|
| `VIEW-001` | 3D viewer over one `ARCH3D.BSA` block + anti-screen invariants | `VERDICT: PROBE-VIEW-001 PASS - INV-L99-5 coherent ...` |
| `CITY-001` | M3 city street walkable end to end, solid blocks, 0 crossings | `VERDICT: PROBE-CITY-001 PASS - street ... walkable end-to-end` |
| `INTERIOR-001` | door transition without crossing a wall + time cycle + real `.RDB` interior | `VERDICT: PROBE-INTERIOR-001 PASS - door transition ...` |
| `COMMERCE-001` | enter → interact → buy (gold down, item up) with the DFU formula | `VERDICT: PROBE-COMMERCE-001 PASS - enter→interact→buy ...` |
| `FORMULA-001` | DFU trade-price formula, 100% parity on the on-disk corpus | `VERDICT: PROBE-FORMULA-001 PASS - 100% parity (Mode A only, corpus on disk)` |
| `WORLD-001` | exterior → interior → commerce → exit → return cycle | `VERDICT: PROBE-WORLD-001 PASS - ... cycle without crash` |
| `READMODEL-001` | live ECS → read models (an ECS change reaches the model) | `VERDICT: PROBE-READMODEL-001 PASS - live ECS→read models ...` |
| `SESSION-001` | byte-by-byte save → load round trip | `VERDICT: PROBE-SESSION-001 PASS - byte-by-byte save→load roundtrip ...` |
| `RMB-001` | retail `.RMB` cities from `ARENA2`, `BuildingDataList` 100% consumed | `VERDICT: PROBE-RMB-001 PASS - <N> retail cities ...` |
| `SEED-001` | retail city determinism: same bytes, same city | `VERDICT: PROBE-SEED-001 PASS - ...` |
| `RDB-001` | census of the retail `.RDB` interiors in `BLOCKS.BSA` | `VERDICT: PROBE-RDB-001 PASS - census <n>/<n> .RDB records ...` |
| `AUDIO-001` | `MIDI.BSA` songs and `DAGGER.SND` clips through the production playback path | `VERDICT: PROBE-AUDIO-001 PASS - ...` |
| `TEXTURE-001` | 12/12 FNV-1a64 texture digests (`TEXTURE.002/007/256`) | `VERDICT: PROBE-TEXTURE-001 PASS - 12/12 FNV-1a64 digests ...` |
| `TERRAIN-001` | 8 byte-by-byte terrain height goldens + determinism | `VERDICT: PROBE-TERRAIN-001 PASS - 8 byte-by-byte height goldens ...` |
| `NPC-001` | `NAMEGEN.DAT` parts, name goldens, population | `VERDICT: PROBE-NPC-001 PASS - NAMEGEN.DAT ...` |
| `TEXT-RSC-001` | `TEXT.RSC` token recomputation | `VERDICT: PROBE-TEXT-RSC-001 PASS - ...` |
| `QUEST-ORIGINAL-001` | original quest census (247/247 `QBN`, 244/244 `QRC`, 82 verbs) | `VERDICT: PROBE-QUEST-ORIGINAL-001 PASS - 247/247 QBN ...` |
| `UI-001` | the 1996 UI panels: assets, font, panel composition | `VERDICT: PROBE-UI-001 PASS - ...` |
| `MAP-001` | the complete `MAPS.BSA` location index | `VERDICT: PROBE-MAP-001 PASS (resolved=<n>/<n>, ...` |
| `DEMO-001` | the headless integrated composition: retail city + `M12` textures + `M13` terrain + `M15` NPCs + `M17` panels + `M20` character, digest identical across passes | `VERDICT: PROBE-DEMO-001 PASS - retail city <n> with M12 textures ...` |
| `LOCOMOTION-001` | the DFU locomotion engine: formulas, engine, stealth, demo, cast-driven effects | `VERDICT: PROBE-LOCOMOTION-001 PASS - <n> checks ...` |
| `STATS-001` | 18+1 classes, 20 derived values, biography, determinism | `VERDICT: PROBE-STATS-001 PASS - <n> checks ...` |
| `ITEM-001` | item census and the `A 15+15+8+17` checks | `VERDICT: PROBE-ITEM-001 PASS - checks=55 passed=55 failed=0 ...` |
| `COMBAT-001` | combat math with injected rolls, armour from equipment, no global RNG | `VERDICT: PROBE-COMBAT-001 PASS - checks=52 passed=52 failed=0 ...` |
| `COMBAT-LOOP-001` | 600 ticks / 60 swings, digest twice, seed sensitivity | `VERDICT: PROBE-COMBAT-LOOP-001 PASS - ticks=600 swings=60 ...` |
| `DEATH-001` | on-death latch, fall/fade/reset timings | `VERDICT: PROBE-DEATH-001 PASS - checks=8 passed=8 failed=0 ...` |
| `MAGIC-001` | `SPELLS.STD` census, spell cost, casting | `VERDICT: PROBE-MAGIC-001 PASS - A∧B∧C∧D ...` |
| `EFFECT-001` | effects: keys, templates, isolated instances, application and expiry | `VERDICT: PROBE-EFFECT-001 PASS - A∧B∧C∧D∧E∧F ...` |
| `ENCHANT-001` | `MAGIC.DEF` enchantment tables and keys | `VERDICT: PROBE-ENCHANT-001 PASS - A∧B∧C∧contract ...` |
| `MONSTER-001` | monster census, HP from the `EnemyBasics` table, spawn table | `VERDICT: PROBE-MONSTER-001 PASS - census 103/60/43 ...` |
| `ENCOUNTER-001` | encounter tables, cells resolve, `EnemyBasics` lookups, negative controls | `VERDICT: PROBE-ENCOUNTER-001 PASS - E1-E10 ...` |
| `NPC-AI-001` | enemy AI formulas, `Idle→Pursue`, contact distance, give-up | `VERDICT: PROBE-NPC-AI-001 PASS - formulas D1-D13 ...` |
| `GUILD-001` | guild and faction tables against the DFU data | `VERDICT: PROBE-GUILD-001 PASS` |
| `CRIME-001` | crime detection and legal state | `VERDICT: PROBE-CRIME-001 PASS` |
| `LAW-001` | law and punishment state | `VERDICT: PROBE-LAW-001 PASS` |
| `EXEC-001` | the execution model (condition/effect verbs) | `VERDICT: PROBE-EXEC-001 PASS` |
| `LIFECYCLE-001` | the world lifecycle (season/date-driven updates) | `VERDICT: PROBE-LIFECYCLE-001 PASS` |
| `JOURNAL-001` | quest journal entries and ordering | `VERDICT: PROBE-JOURNAL-001 PASS` |

A green suite prints `VERDICT: ANTI-SCREEN SUITE PASS (37/37 probes, exit 0)`.

There is **no M30 or later probe in this build**: M30 is frozen and M31-M35 are specified but not
implemented, so no `SHOP-001`, `ECONOMY-001`, `BANK-001`, `TRAVEL-001`, `WEATHER-001`,
`CALENDAR-001`, `CONDITION-001`, `VAMPIRE-001`, `REST-001` or `SPINE-001` exists here.

The M18 parity probes added after the 37-member freeze are **standalone** (outside `ALL-001`) and
exist in this build: `ORIGIN-001`, `ATTACK-KEY-001`, `MENU-001`, `INTERIOR-TRANSIT-001`,
`WEATHER-VISUAL-001`, `PANELS-001`, `POPULATION-001` and `RETAIL-TEXT-001`. Run them one at a time
with `--probe <name>`.

## 7. Known issues

- **The panels are pictures, not widgets.** The 1996 panels (HUD, inventory, quests, menu) are
  composed as retail images with **0 interactive widgets**. Nothing is clickable; there is no button
  navigation and no character creation.
- **No seamless exterior-to-interior transition in the demo.** Walking the city does not take you
  inside a building; interiors are shown by the `INTERIOR-001` probe window, and the
  `INTERIOR-TRANSIT-001` probe measures the transition state machine headlessly (steps 4–5 of the
  spine smoke). Wiring it into the live window is not part of this build.
- **Audio is silent unless you build the `audio` feature.**
  `./scripts/launch.sh --mode complete --features audio,window`. Without a usable device the engine
  degrades to a null sink and prints why instead of panicking.
- **The default composition has no ocean** (`M18 VE: 0 ocean samples ...`). Press `T` and the
  terrain origin moves to the nearest coastal pixel of `WOODS.WLD`, which does carry an ocean band
  (logged as `coastal pixel (x,y) ... ocean N / beach N / land N`); the water surface is drawn at
  sea level and is **not walkable** — walking into it is blocked by design.
- **`CITY-001` is probe geometry, not a retail city.** The headless probe runs a deterministic 3x3
  street grid; the M3 window (with the `window` feature) opens a 4x4 city window. Six `ARCH3D.BSA`
  landmarks, low-poly by design. The retail city is the `--mode complete` demo.
- **Two different dialogue surfaces.** `E` runs the M5 commerce dialogue; in the integrated demo `J`
  toggles the retail `TEXT.RSC` dialogue overlay and `I` the journal overlay (one overlay at a time,
  both read-only pictures — 0 invented text).
- **No day/night cycle and no nature sprites.** Vegetation and the visible night sky are not part of
  this alpha.
- **No damage numbers on screen.** Combat resolves and the HUD shows HP, but the hit number is not
  drawn over the target.
- **Mouse look cannot be scripted.** The demo reads raw mouse motion; synthetic input does not
  produce it, so automated captures keep a fixed yaw. A real mouse works.
- **HP can drop while you stand still**: enemy retaliation ticks while the window is open.

## 8. Where this build stands

`v0.2.0-alpha`, no parity claim. `main` carries the **M18 technical candidate**: the seven closure
states of the B3 tanda it set out to close (CS-12/13/14/15/16/18/19) are measured with window
evidence — PNG plus sha256 — so the technical side is closed. The **acceptance** review is not: the
21 acceptance rows read **3 PASS · 6 PARTIAL · 12 NO**, the owner signature is **pending**, and no
`M18` tag exists. M30 is frozen; M31–M35 are specified but not implemented. No date is promised for
any of them.

## 9. Where to look next

- [`README.md`](README.md) — the project story, data policy and repository layout.
- [`docs/acknowledgments/launcher.md`](docs/acknowledgments/launcher.md) — modes, flags, exit codes.
- [`docs/acknowledgments/status.md`](docs/acknowledgments/status.md) — the acceptance state per milestone.
- [`docs/acknowledgments/roadmap.md`](docs/acknowledgments/roadmap.md) — what is planned next.
- [`ARCHITECTURE.md`](ARCHITECTURE.md) — crates, data flow, the module pattern.
