//! daggerfall-app — Bevy application entry (F0 scaffold).
//!
//! Bevy 0.20.0 with default features disabled: the F0 scaffold compiles and
//! runs headless. Rendering/window features are enabled by the perception
//! lane when graphics land (ADR-GAME-004 posture: Bevy >= 0.19).
//!
//! M1: the additive `window` feature enables window+render —
//! `cargo run -p daggerfall-app --features window` — and renders ONE
//! parsed block of real ARCH3D.BSA (Mode A) through the bridge crate
//! `daggerfall-parser-bevy-bridge` (DAG app → bridge → parser). The default
//! headless mode (CI F0) stays intact: conditional extension,
//! 0 deletes of the original scaffold.
//!
//! M2: first-person camera (WASD + mouse-look, spawn inside the
//! block) + grid collisions (INV-L99-5) and the Anti-Screen probe
//! `--probe VIEW-001` — runs in **default headless** config (0 render), delta
//! documented against the `[INFERENCE]` command of VSIM §4 row 6.
//!
//! M3: navigable city — deterministic 3D streets (MISSING-SPEC
//! GRIDCITY → synthetic layout over real ARCH3D, delta in note M3) + unlit
//! sky and clouds. Anti-Screen probe `--probe CITY-001` in default headless
//! config (same scheme as VIEW-001); with `--features window` the same
//! command, after PASS, opens the M3 window (visual evidence streets+sky+clouds).
//!
//! M4 (real wiring): first interior building —
//! Anti-Screen probe `--probe INTERIOR-001` (street→interior through a door
//! without crossing a wall + 24 h time system) in **default headless** config.
//! Block A uses the synthetic reference shop (0 I/O) and block C the **real**
//! interior `.RDB` + `ARCH3D` (M11); the premise "DFON1.BSA missing" was FALSE
//! (WI-3). The time system is the organic ❤️ heart
//! (`daggerfall_heart::clock`). With `--features window` the same command,
//! after PASS, opens the M4 window, which renders the **real retail interior**
//! of the deterministic subject of `RDB-001` + clock in the title (`m4_window`).
//!
//! M5: interactive commerce — Anti-Screen probe
//! `--probe COMMERCE-001` (enter→interact→buy: **gold debited + item added**,
//! solid NPC + dialogue + [B]/[V] inventory-lite menu) in **default headless**
//! config, with block C = PROBE-FORMULA-001 (DFU golden vectors for the trade
//! price, N2). `--probe FORMULA-001` runs the formula block alone. With
//! `--features window` COMMERCE-001 on PASS opens the M5 window (`m5_window`:
//! NPC + menu in the title).
//!
//! M6: integrated playthrough — Anti-Screen probe
//! `--probe WORLD-001` (street spawn → shop → trade → exit → return,
//! 0 pass-throughs, 3 crossings through the door gap) in **default headless**
//! config, plus the **unified suite** `--probe ALL-001` (all M2-M6 probes with
//! aggregated evidence and exit 0 only if ALL PASS — the documented command
//! of note M6).
//!
//! M7: session read models — Anti-Screen probe
//! `--probe READMODEL-001` (the 3 PD4 T7 read models — HUD ←RPGStats ·
//! Automap ←Chunks · inventory/quests panel ←AssetSet/Quest — projected
//! from the ECS at runtime: change in ECS ⇒ change in read model +
//! determinism, REQ-L99-038/039/040) in **default headless** config (0
//! .BSA reads: declared synthetic session). With `--features window` and
//! PASS it opens the M7 window (`m7_window`: automap with tiles in the client
//! area + the 3 read models in the title bar). The ALL-001 suite becomes
//! 7 probes (M2-M7).
//!
//! M8: closed session loop — Anti-Screen probe
//! `--probe SESSION-001` (save/load roundtrip INV-L99-4 / REQ-L99-041:
//! save → exit → reload ⇒ the 5 VE states — position, stats, inventory,
//! quests, chunks — + the ❤️ clock restored byte-by-byte from
//! `saves/session.dfs`, with anti-false-positives: no save ⇒ honest
//! NotFound · altered byte ⇒ checksum · version ⇒ rejection) in **default
//! headless** config (0 .BSA reads: declared synthetic session). The
//! format and I/O live in 🧠 memory (`session_store`); the ECS mapping in
//! `m8_session` (the `app` composes, bus-only). With `--features window` and
//! PASS it opens the M8 window (`m8_window`: the restored session visible in
//! the title bar + automap of restored chunks). The ALL-001 suite becomes
//! 8 probes (M2-M8).
//!
//! M10: retail city `.RMB` — Anti-Screen Probes
//! `--probe RMB-001` (real cities of the Daggerfall region parsed
//! from ARENA2 — MAPS.BSA + BLOCKS.BSA + ARCH3D.BSA — with 0 panic, 100%
//! of the BuildingDataList consumed and retail navigability INV-L99-5) and
//! `--probe SEED-001` (real ext: the capital composed TWICE from the
//! same bytes ⇒ same city byte-by-byte, REQ-L99-023) in **default
//! headless** config (Mode A custody, 0 packaged bytes — INV-
//! L99-6). The ALL-001 suite becomes 10 probes (M2-M10).
//!
//! M14: retail audio — Anti-Screen probe `--probe AUDIO-001`
//! (MIDI.BSA catalog parsed with our own HMI parser: N/total songs,
//! 0 panic + song trigger by city day/night context replicating DFU
//! `SongManager` + **375/375** SoundClips effects over DAGGER.SND —
//! REQ-L99-045/046, Mode A custody). The ALL-001 suite becomes 11 probes
//! (M2-M14). R1 fixed the dispatch: `SoundClips` is a
//! **positional directory index** into DAGGER.SND (`SoundReader.cs:59-101` →
//! `SndFile.GetSound(index)`), not a `RecordID`; `AUDIO-001` asserts index
//! dispatch against a measured PCM golden per clip and keeps the record-id
//! lookup as an aggregated negative control. C3-L1/L3 closed the
//! 2/375 residual: every DFU member is checked against the embedded measured
//! goldens (373 `pcm` + `Invalid = 5` empty + `None = -1` no clip + the flagged
//! alias 138), a missing/unreadable DAGGER.SND is `[FAIL]` + exit 2 (no
//! degraded pass), and the probe also drives the production playback path
//! (gameplay events through `audio::AudioEngine`) and a deterministic context
//! song render from MIDI.BSA.
//!
//! M14 production playback (C3-L2): the game itself plays
//! audio. `daggerfall_app::audio` (library target: `src/audio.rs`) resolves
//! every `SoundClips` member by its `DAGGER.SND` **directory index** into a
//! 44-byte WAV + PCM, queues/drains it through a sink (`RodioSink` behind the
//! additive `audio` feature; `NullSink`/`RecordingSink` headless), retains the
//! HMI note events (`midi::parse_hmi_events`) to render the context song from
//! `MIDI.BSA` deterministically, and maps gameplay events to DFU clips
//! (`FallDamage`/`FallHard`, `SwingLow/HighPitch`, `Hit1..5`, arrows, body
//! fall, doors). `m18_window` wires fall damage, melee swings resolved by the
//! M22 engine and the city day/night song into the demo. `mode_a` (same
//! library target) is the single Mode A path resolver used by this binary and
//! by the audio runtime (0 retail bytes in git, 0 packaged audio).
//!
//! M11: retail interior `.RDB` — Anti-Screen probe
//! `--probe RDB-001` (census of ALL `.RDB` records of BLOCKS.BSA +
//! deterministic subject by directory order + real 12-check probe:
//! street→real flanked door→interior→door→street with 0 pass-throughs,
//! INV-L99-5, REQ-L99-043) in **default headless** config (Mode A
//! custody). DoD mapping: PROBE-INTERIOR-001 (DoD) = INTERIOR-RDB-001 = `--probe
//! RDB-001`; the real variant also runs as block C inside
//! `--probe INTERIOR-001` (PASS requires synthetic A + B time + C real). The
//! ALL-001 suite becomes 12 probes (M2-M14).
//!
//! M12: retail textures `TEXTURE.*` — Anti-Screen probe
//! `--probe TEXTURE-001` (3 files × 4 rotations decoded at
//! runtime from ARENA2 + FNV-1a64 digest of the pixels against 12
//! constants + consumable `PAL.PAL` palette, DoD §c / WI-4) in
//! **default headless** config (Mode A custody, INV-L99-6: 0 retail bytes in
//! git). The ALL-001 suite becomes 13 probes (M2-M14 + M12).
//!
//! M13: terrain + retail biomes — Anti-Screen probe
//! `--probe TERRAIN-001` (8 height goldens derived from the sampler
//! formula + digests ×2 + walkable terrain outside the city: `WOODS.WLD`) and
//! `--probe CLIMATE-001` (0 climates outside 223..=232, 61 regions,
//! `NatureSet × Season → archive`, coastal dilation: `CLIMATE.PAK` /
//! `POLITIC.PAK`), DoD §e / WI-6, Mode A custody. The ALL-001 suite becomes
//! 14 probes.
//!
//! M15: NPC population — Anti-Screen probe `--probe NPC-001`
//! (`NAMEGEN.DAT` 11 banks parsed with 18 name goldens, deterministic
//! population by region/subclimate with 8-dir flats from `TEXTURE.256`,
//! retail captions from `TEXT.RSC`, crime guards and `place npc` /
//! `create npc at`), DoD §f / WI-7 / REQ-L99-047, Mode A custody. The suite
//! ALL-001 becomes 15 probes.
//!
//! M18: 100% real closure — `--probe MAP-001` resolves the
//! complete `MAPS.BSA` index (**61 regions / 15,251 locations**, no
//! sampling: hardened pack §5.3.1), with region + `mapIndex` + coordinate and
//! `.RMB` city of each location resolved from `BLOCKS.BSA`; FNV-1a 64 digest
//! identical in 2 passes, 0 panic, 0 skip, `MAPS.BSA` missing ⇒
//! `EXIT=2`. REQ-L99-051, DoD §a/§i, Mode A custody. The ALL-001 suite becomes
//! 18 probes.
//!
//! M19: player locomotion and body —
//! `--probe LOCOMOTION-001` (spec §7: block A formulas §1.3/§2.1/§3.1/§5.3,
//! block B runtime engine in a synthetic world with `dt=1/60` — speeds,
//! jump, 10 m fall, climbing, levitation, swimming/air, overload and
//! 0 penetrations, block C stealth/pilfering, block D retail demo), plus the
//! alias `--probe STEALTH-001` (block C only). With `--features window` the
//! M18 demo uses `behavior::locomotion::Locomotion::step` and the title bar
//! shows mode + air. REQ-L99-052. **The ALL-001 suite becomes 20 probes**
//! (18/18 M18 freeze → 19/19 with `DEMO-001` → 20/20 with `LOCOMOTION-001`).
//!
//! M20: character — Anti-Screen probe
//! `--probe STATS-001` (spec §8.1: census of the 18 `CLASSnn.CFG` against the
//! §2 table + `CLASS18` as template, §4 derived from Spellsword/Mage,
//! `BIO.DAT` + the 18 `BIOGnnT0.TXT` with their effect census and the real
//! application of the biography to a `Character`, and determinism ×2 with 0
//! panics) and `--probe SKILLS-001` (spec §8.2: census of the 35 skills §3,
//! seed defaults, tally/360 gate/uses §5.2/100 cap/mastery, level/HP/pool/SP §5,
//! training §5.7 and integration with the M19 engine). REQ-L99-053. **The
//! ALL-001 suite becomes 21 probes** (20/20 with `LOCOMOTION-001` → 21/21 with
//! `STATS-001`; `SKILLS-001` runs outside the suite, same pattern as
//! `STEALTH-001`).
//!
//! M20 closure (audit P1-3): the progression driver —
//! `--probe PROGRESSION-001` (standalone, outside `ALL-001`) measures real
//! gameplay signals → skill uses (Running per 4 classic updates, Jumping per
//! edge, Climbing per check, Swimming per game minute, Dodging per strike,
//! weapon/H2H+CriticalStrike on hit, frame-rate independence), the 100
//! cap/mastery, the level-up applied by the driver, determinism ×2 and the
//! M25 humanoid contract (`HumanoidProfile` over `CLASS01.CFG`). The same
//! driver is the only progression path of the M18 window and `DEMO-001`
//! (`ALL-001` stayed **37 probes** at M20 — dated phase note; the suite is
//! **40 entries** since M34 integrated PLAYSESSION-001/UI-FULL-001/
//! AUDIO-RT-001, spec §11).
//!
//! M21: items, equipment,
//! durability and loot — `--probe ITEM-001` (spec §7: block A with **55
//! golden vectors** GV-IT/GV-MAT/GV-CND/GV-DUR + block C with `census:
//! records=288 first=Ruby last=Map offset=1796138 sha256=…` over the retail
//! `FALL.EXE` blob of the installation root; unreadable ⇒ `EXIT=2`), plus
//! the standalone `--probe EQUIP-001` (10 GV-EQP over the 27-slot
//! `ItemEquipTable` + uid dump) and `--probe LOOT-001` (500 `K` containers
//! level 5 with explicit `Rng64` per container, SHA-256 digest ×2 and
//! `--seed <u64>`). REQ-L99-054. **The ALL-001 suite becomes 22 probes**
//! (21/21 with `STATS-001` → 22/22 with `ITEM-001`; `EQUIP-001` and `LOOT-001`
//! run outside the suite, same pattern as `SKILLS-001`/`STEALTH-001`).
//!
//! M22: combat — Anti-Screen probe
//! `--probe COMBAT-001` (spec §6 A: **52 checks** = GV-HIT 01-15 + GV-DMG
//! 01-17 + GV-CAD 01-15 + GV-ARM 01-05 with injected rolls. R1
//! added the armour-from-equipment and `DamageEquipment` rows), `--probe
//! COMBAT-LOOP-001` (600 ticks @ 60 Hz, swings=60, FNV-1a 64 digest ×2 and
//! seed-sensitive) and `--probe DEATH-001` (GV-DTH 01-06 + OnDeath latch ×1 of
//! the player and the enemy, fall 0.85 / fade 2.0 / reset 3.0). REQ-L99-055.
//! `COMBAT-001`, `COMBAT-LOOP-001` and `DEATH-001` are all part of the
//! `ALL-001` suite (the last two entered it in R1 — their
//! work package's probe must be certified by the suite that certifies it).
//!
//! M23: magic —
//! Anti-Screen probe `--probe MAGIC-001` (spec §10: SPELLS.STD census +
//! spell cost + casting with the M20 Character SP, **44 checks**
//! A-D), plus `--probe EFFECT-001` (effect model + applied mutations:
//! blocks A-E, 34 checks, digests ×2) and `--probe SPELLMAKER-001` (spell
//! creation: 7 checks). REQ-L99-056. R1 made every one of the
//! 22 reference effects apply a real mutation and expose the movement flags
//! M19 consumes: `EFFECT-001` gained block E (E1-E15) and entered the
//! `ALL-001` suite. `SPELLMAKER-001` still runs outside the suite, same
//! pattern as `EQUIP-001`/`LOOT-001`.
//!
//! M24: enchantment/alchemy — Anti-Screen probe
//! `--probe ENCHANT-001` (spec §5 A-C: MAGIC.DEF census + per-material
//! budget + accounting/gates, **34 checks**, plus the §3.4 contract of
//! 19 M23 keys) and the standalone `--probe POTION-001` (20 recipes · **23
//! checks** GV-RCP/GV-POT + SHA-256 digest ×2). REQ-L99-057. **The suite
//! ALL-001 becomes 25 probes** (24/24 with `MAGIC-001` → 25/25 with
//! `ENCHANT-001`; `POTION-001` runs outside the suite, same pattern as
//! `EFFECT-001`/`SPELLMAKER-001`).
//!
//! M25: monsters, enemy AI and encounters — Anti-Screen probe
//! `--probe MONSTER-001` (spec §10 A-D: container census,
//! 43 `ENEMY###.CFG` of 74 B, class HP and deterministic scene) and the
//! standalone `--probe ENEMY-AI-001` (D1-D13 formulas + scenario
//! `Idle→Pursue`/give-up/doors) and `--probe ENCOUNTER-001` (E/B tables,
//! 1000 seeded rolls, dungeon with and without alarm, pacification F1-F7).
//! REQ-L99-058. R1 rebuilt the 45×20 encounter tables with the
//! real `MobileTypes` (humanoids 128..=146, no `id - 85`), added the typed
//! resolver and moved `ENCOUNTER-001` into the `ALL-001` suite.
//! `ENEMY-AI-001` still runs outside the suite, same pattern as
//! `SPELLMAKER-001`.
//!
//! M26: living NPCs — Anti-Screen probe `--probe NPC-AI-001`
//! (spec §8 A-D: population bounds, units/navgrid, appearance probabilities
//! and 60 s routine with digest ×2) and the standalone
//! `--probe DIALOGUE-001` (§3.1 greeting + per-faction topics + negative branch of
//! `answersToNonDirections` + stability) and `--probe RUMOR-001` (census of
//! `RUMOR.DAT`, channels per type, F4 weighted reservoir and F7-F10 edges).
//! `--probe NPC-DEMO-001` runs the §9 demo over retail. REQ-L99-059. **The
//! ALL-001 suite becomes 27 probes** (26/26 with `MONSTER-001` → 27/27 with
//! `NPC-AI-001`; `DIALOGUE-001`, `RUMOR-001` and `NPC-DEMO-001` run outside
//! the suite).
//!
//! M27: guilds and factions — Anti-Screen probe
//! `--probe FACTION-001` (spec §7.1 A-D: GV-27-01..08 formulas over fixture,
//! determinism ×2 with `DFRandom::srand(0x1234)`, `DISK:FACTION.TXT` census and
//! `GetGuildGroup`) and `--probe GUILD-001` (§7.2 A-D: rank GV-27-11..20,
//! services GV-27-21..35, propagation GV-27-36..42 with `PROP_TREE` and menu
//! gating). `--probe M27-DEMO-001` runs the §9 demo over retail: joining the
//! Fighter's Guild (id 41), rising to rank 9 and measuring the benefit. REQ-L99-060.
//! **The ALL-001 suite becomes 28 probes** (27/27 with `NPC-AI-001` → 28/28 with
//! `GUILD-001`; `FACTION-001` and `M27-DEMO-001` run outside the suite).
//!
//! M28: crime, law and courts — Anti-Screen probe
//! `--probe CRIME-001` (spec §5.1 A-D: GV-28-01..12 court table, GV-28-13..16
//! legal reputation with M27 propagation, sentence determinism ×2 with
//! `DFRandom::srand(12345)` and the seven crime triggers) and `--probe
//! LAW-001` (§5.2 steps 1-6: crime → M26 guard request → court → prison →
//! release, plus the anti-false-positive block). Both probes are pure
//! (0 retail bytes). REQ-L99-061. **The ALL-001 suite becomes 30 probes**
//! (28/28 with `GUILD-001` → 30/30 with `CRIME-001` + `LAW-001`).
//!
//! M29: playable quests — Anti-Screen probe
//! `--probe EXEC-001` (spec §6.1 A-D: GV-29-01..24 header/clock/task formulas,
//! determinism ×2 with `DfRandom::srand(0x5EED)` plus the seed falsifier, the
//! §2.1 census over the 247 `.QBN` / 244 `.QRC` and the type coverage table),
//! `--probe LIFECYCLE-001` (§6.2 steps 1-7: offer filter → accept → `end quest`
//! ⇒ 2 ticks → closing reputation `+5/-2/0` as a M27 request → mid-flight
//! `save→load`) and `--probe JOURNAL-001` (§6.3 blocks A-D: 3 `log` + 1 `note`
//! ⇒ 4 entries, Notebook with 3, sentinel/absent ids ⇒ `skipped=1` +
//! `QuestFailure` with no panic, reopen without duplicate `stepID`). REQ-L99-062.
//! **The ALL-001 suite becomes 33 probes** (30/30 with `LAW-001` → 33/33 with
//! `EXEC-001` + `LIFECYCLE-001` + `JOURNAL-001`).
//!
//! M34: the demo probes land in the suite (spec §11 — `ALL-001` **with the 3
//! new probes integrated**): `--probe PLAYSESSION-001` (S01-S12 in one session,
//! 12/12 scenarios = 216000 steps, FNV-1a 64 digest ×2 in two separate
//! processes), `--probe UI-FULL-001` (85-window census 16+11+7+51 + 85/85 shell
//! navigation + HUD GV-03 + **51/51 playable** = 48 image-class + 3 video,
//! three digests ×2) and `--probe AUDIO-RT-001` (132/132 songs + 459 DAGGER.SND
//! records + 60/60 triggers; real device output or a declared `[SKIP-DEVICE]`
//! ⇒ `PASS(degraded)`). Each keeps its own verdict and exit code; REQ-L99-067.
//! **The ALL-001 suite becomes 40 probes** (37 before this integration = 33
//! pre-R1 entries + the four R1 entries `COMBAT-LOOP-001`/`DEATH-001`/
//! `EFFECT-001`/`ENCOUNTER-001`; +3 M34 = 40).
//! Dated phase note: `UI-FULL-001` exited 1 while block B (the 51 retail
//! compositions) was still pending (W34-1); W34-2a..2f composed it, the
//! measurement is 51/51 and the probe exits 0 on PASS — measured 2026-10-26.
//!
//! M37 (F-AUD-01, design f-aud-01-02-streaming-navmesh-CR-96142f2f) —
//! `--probe STREAM-001` (async cell streaming): pure policy block P in every
//! build (window/hysteresis/digest of `bridge::stream_policy`), plus blocks
//! A (frame repartition) and M (measurements: active window under a
//! synthetic walk, per-tick cost, population-seed stability / 0 re-seed,
//! §2.5 per-cell cota) with `--features window` over a headless
//! `MinimalPlugins` app (0 GPU, I-6). Blocks C/D (F3), E (F4) and F (F5)
//! print `[PENDING]` until their phase lands; EXIT=2 without ARENA2.

use bevy::prelude::*;

mod m2_probe;
mod m3_probe;
mod m4_probe;
mod m5_probe;
mod m6_probe;
mod m7_probe;
mod m7_readmodels;
mod m8_probe;
mod m8_session;
mod m10_probe;
mod interior_transit_probe;
mod m11_probe;
mod m12_probe;
mod m13_probe;
mod m14_probe;
mod m15_probe;
mod m16_probe;
mod m17_probe;
mod m18_probe;
mod m18_demo;
mod m18_phase;
mod m18_b1_probe;
mod m18_b3_probe;
mod m18_b3a;
mod m18_b3b;
mod ui_weather;
mod m19_probe;
mod m20_probe;
mod m21_probe;
mod m22_probe;
mod m23_probe;
mod m24_probe;
mod m25_probe;
mod m26_probe;
mod m27_probe;
mod m28_probe;
mod m29_probe;
mod m30_probe;
mod m31_probe;
mod m32_probe;
mod m33_probe;
mod m34_audio_probe;
mod m34_ui_probe;
mod m37_ui1_probe;
mod m38_ui2_probe;
// M39/UI-3 — the 51 `Playable` windows (plan §4 frozen interface) and their
// executable gate `PROBE-UI-JUGABLE-001` (plan §4.1).
mod m39_state;
mod m39_block_a;
// Block B exposes accessors (`window`, `ordinal`, `tick`, `painted`,
// `last_delta`, `changed`) that only its unit tests read; the probe consumes
// `interact`/`evidence`, so the lint stays muted here — and only here.
#[allow(dead_code)]
mod m39_block_b;
mod m39_block_cd;
mod m39_block_ef;
mod m39_block_gh;
mod m39_surfaces;
mod m39_jugable_probe;
mod m34_video_probe;
mod m34_light_probe;
mod m35_biome_probe;
mod m35_sprite_probe;
mod f_aud_01_stream_probe;
mod playsession_probe;
mod dungeon_probe;
mod m34_automap;
mod interior_retail;
mod spine_probe;
mod m9_probe;
mod ui_assets;
#[cfg(feature = "window")]
mod m2_player;
#[cfg(feature = "window")]
mod m3_window;
#[cfg(feature = "window")]
mod m4_window;
#[cfg(feature = "window")]
mod m5_window;
#[cfg(feature = "window")]
mod m7_window;
#[cfg(feature = "window")]
mod m8_window;
#[cfg(feature = "window")]
mod m17_window;
#[cfg(feature = "window")]
mod m18_window;
// M37 (F-AUD-01): async cell streaming. The ECS layer is window-only by
// design (§2.8); its policy lives headless in the bridge (`stream_policy`)
// and the STREAM-001 probe runs its pure block in every build.
#[cfg(feature = "window")]
mod world_stream;

fn main() {
    // M2: Anti-Screen probe — handled BEFORE any other path
    // (default headless and --features window compile the same code).
    let args: Vec<String> = std::env::args().collect();
    if let Some(pos) = args.iter().position(|arg| arg == "--probe") {
        let code = match args.get(pos + 1).map(String::as_str) {
            Some("VIEW-001") => m2_probe::run_view_001(),
            Some("CITY-001") => m3_probe::run_city_001(),
            Some("INTERIOR-001") => m4_probe::run_interior_001(),
            Some("COMMERCE-001") => m5_probe::run_commerce_001(),
            Some("FORMULA-001") => m5_probe::run_formula_001(),
            // M6: integrated playthrough + unified suite.
            Some("WORLD-001") => m6_probe::run_world_001(),
            Some("ALL-001") => m6_probe::run_probe_all(),
            // M7: session read models (PD4 T7).
            Some("READMODEL-001") => m7_probe::run_readmodel_001(),
            // M8: session save/load roundtrip (INV-L99-4).
            Some("SESSION-001") => m8_probe::run_session_001(),
            // M10: retail city .RMB (REQ-L99-042/006/023).
            Some("RMB-001") => m10_probe::run_rmb_001(),
            Some("SEED-001") => m10_probe::run_seed_001(),
            // M11: retail interior .RDB (REQ-L99-043).
            Some("RDB-001") => m11_probe::run_rdb_001(),
            // M18 B2-b: world → interior → world through a door (spine §4).
            Some("INTERIOR-TRANSIT-001") => interior_transit_probe::run_interior_transit_001(),
            // M14: retail audio MIDI.BSA + DAGGER.SND (REQ-L99-045/046).
            Some("AUDIO-001") => m14_probe::run_audio_001(),
            // M12: retail textures TEXTURE.* (DoD §c / WI-4).
            Some("TEXTURE-001") => m12_probe::run_texture_001(),
            // M13: retail terrain + biomes WOODS.WLD / CLIMATE.PAK (DoD §e / WI-6).
            Some("TERRAIN-001") => m13_probe::run_terrain_001(),
            Some("CLIMATE-001") => m13_probe::run_climate_001(),
            // M15: NPC population + retail captions (DoD §f / WI-7).
            Some("NPC-001") => m15_probe::run_npc_001(),
            // M16: retail text + original quests (REQ-L99-048/049).
            Some("TEXT-RSC-001") => m16_probe::run_text_rsc_001(),
            Some("QUEST-ORIGINAL-001") => m16_probe::run_quest_original_001(),
            // M17: 1996 aesthetic UI — assets, font and panels (REQ-L99-050).
            Some("UI-001") => m17_probe::run_ui_001(),
            // M18: complete MAPS.BSA index — 61/15.251 (REQ-L99-051).
            Some("MAP-001") => m18_probe::run_map_001(),
            // M18: integrated demo (city+T+NPC+UI) headless.
            Some("DEMO-001") => m18_demo::run_demo_001(),
            // M18 B1 (paridad): standalone probes outside `ALL-001` —
            // origin/yaw (B1-a), keyboard attack + HUD target (B1-c) and the
            // startup menu phase (B1-d).
            Some("ORIGIN-001") => m18_b1_probe::run_origin_001(),
            Some("ATTACK-KEY-001") => m18_b1_probe::run_attack_key_001(),
            Some("MENU-001") => m18_b1_probe::run_menu_001(),
            // M19: DFU locomotion (REQ-L99-052) + stealth.
            Some("LOCOMOTION-001") => m19_probe::run_locomotion_001(),
            Some("STEALTH-001") => m19_probe::run_stealth_001(),
            // M20: character — classes/attributes/
            // derived stats/biography (STATS-001) and skills/progression (SKILLS-001).
            Some("STATS-001") => m20_probe::run_stats_001(),
            Some("SKILLS-001") => m20_probe::run_skills_001(),
            // M20 (audit P1-3): the gameplay→progression driver
            // (standalone, outside the suite, like SKILLS-001/EQUIP-001).
            Some("PROGRESSION-001") => m20_probe::run_progression_001(),
            // M21: items/equipment/durability (ITEM-001, in the
            // suite) + deterministic loot (LOOT-001), spec M21 (REQ-L99-054).
            Some("ITEM-001") => m21_probe::run_item_001(),
            Some("EQUIP-001") => m21_probe::run_equip_001(),
            Some("LOOT-001") => m21_probe::run_loot_001(),
            // M22: combat — hit/damage/cadence (COMBAT-001,
            // in the suite), 600-tick loop (COMBAT-LOOP-001) and death
            // (DEATH-001), spec M22 (REQ-L99-055).
            Some("COMBAT-001") => m22_probe::run_combat_001(),
            Some("COMBAT-LOOP-001") => m22_probe::run_combat_loop_001(),
            Some("DEATH-001") => m22_probe::run_death_001(),
            // M23: magic — SPELLS.STD + cost + casting
            // (MAGIC-001, in the suite), effect model (EFFECT-001) and
            // spellmaker (SPELLMAKER-001), spec M23 (REQ-L99-056).
            Some("MAGIC-001") => m23_probe::run_magic_001(),
            Some("EFFECT-001") => m23_probe::run_effect_001(),
            Some("SPELLMAKER-001") => m23_probe::run_spellmaker_001(),
            // M24: enchantment/alchemy — ENCHANT-001 (in the
            // suite) and POTION-001 (standalone), spec M24 (REQ-L99-057).
            Some("ENCHANT-001") => m24_probe::run_enchant_001(),
            Some("POTION-001") => m24_probe::run_potion_001(),
            // M25: monsters/AI/encounters — MONSTER-001 (in the
            // suite) + ENEMY-AI-001 and ENCOUNTER-001 (standalone), spec M25.
            Some("MONSTER-001") => m25_probe::run_monster_001(),
            Some("ENEMY-AI-001") => m25_probe::run_enemy_ai_001(),
            Some("ENCOUNTER-001") => m25_probe::run_encounter_001(),
            // M26: living NPCs — NPC-AI-001 (in the suite),
            // DIALOGUE-001, RUMOR-001 and NPC-DEMO-001 (standalone), spec M26.
            Some("NPC-AI-001") => m26_probe::run_npc_ai_001(),
            Some("DIALOGUE-001") => m26_probe::run_dialogue_001(),
            Some("RUMOR-001") => m26_probe::run_rumor_001(),
            Some("NPC-DEMO-001") => m26_probe::run_demo_001(),
            // M27: guilds/factions — GUILD-001 (in the suite),
            // FACTION-001 and M27-DEMO-001 (standalone), spec M27 (REQ-L99-060).
            Some("FACTION-001") => m27_probe::run_faction_001(),
            Some("GUILD-001") => m27_probe::run_guild_001(),
            Some("M27-DEMO-001") => m27_probe::run_demo_001(),
            // M28: crime/law — CRIME-001 and LAW-001 (both in
            // the suite), spec M28 (REQ-L99-061).
            Some("CRIME-001") => m28_probe::run_crime_001(),
            Some("LAW-001") => m28_probe::run_law_001(),
            // M29: playable quests — EXEC-001, LIFECYCLE-001
            // and JOURNAL-001 (all in the suite), spec M29 (REQ-L99-062).
            Some("EXEC-001") => m29_probe::run_quest_exec_001(),
            Some("LIFECYCLE-001") => m29_probe::run_quest_lifecycle_001(),
            Some("JOURNAL-001") => m29_probe::run_quest_journal_001(),
            // M9: verification-gap probes — chunk determinism
            // (INV-L99-2), phase scheduling and custom-universe isolation
            // (INV-L99-6 / REQ-L99-016). Standalone, outside the ALL-001 suite.
            Some("CHUNK-001") => m9_probe::run_chunk_001(),
            Some("SCHEDULE-001") => m9_probe::run_schedule_001(),
            Some("MODB-ISOLATION-001") => m9_probe::run_modb_isolation_001(),
            // M30: economy — shops, inns, banking and transport
            // (all standalone), spec M30 (REQ-L99-063).
            Some("SHOP-001") => m30_probe::run_shop_001(),
            Some("ECONOMY-001") => m30_probe::run_economy_001(),
            Some("BANK-001") => m30_probe::run_bank_001(),
            // M31: travel/weather/calendar — TRAVEL-001 (A-D, EXIT=2 without
            // ARENA2), WEATHER-001 (A-B) and CALENDAR-001 (A-B), spec M31
            // (REQ-L99-064).
            Some("TRAVEL-001") => m31_probe::run_travel_001(),
            Some("WEATHER-001") => m31_probe::run_weather_001(),
            Some("CALENDAR-001") => m31_probe::run_calendar_001(),
            // M32: conditions/death/rest — CONDITION-001 (diseases,
            // contagion, poisons, cure), VAMPIRE-001 (vampirism/lycanthropy),
            // REST-001 (rest + retail demo, EXIT=2 without ARENA2) and
            // DEATH-STATE-001 (death + NEGATIVE census), spec M32
            // (REQ-L99-065).
            Some("CONDITION-001") => m32_probe::run_condition_001(),
            Some("VAMPIRE-001") => m32_probe::run_vampire_001(),
            Some("REST-001") => m32_probe::run_rest_001(),
            Some("DEATH-STATE-001") => m32_probe::run_death_state_001(),
            // Spine: typed 5-type plumbing + the 12-step smoke (§7).
            // EXIT=0 ⟺ 12/12 with digest ×2; EXIT=2 while M33 is unavailable.
            Some("SPINE-001") => spine_probe::run_spine_001(),
            // M34-D / G-N1 (spec m34d-dungeon-grid-spec-CR-2d5c8f41.md):
            // PROBE-DUNGEON-001 — MAPDITEM dungeon grid, 4232 dungeons,
            // 51.2 m cells, 1.4 m door dedupe; EXIT=2 without ARENA2.
            Some("PROBE-DUNGEON-001") => dungeon_probe::run_dungeon_001(),
            // M34/W34-5 (spec m34-e2e-integration-presentation-spec-CR-5243af63.md
// §1.5): PROBE-AUTOMAP-001 — dungeon automap (per-block state
// discovered/visited + grey material, ASCII 24) and city automap
// (nameplates at depth 4.0, collision ⇒ `*`), persisted in the
// classic 0x33 record without touching the M33 keys; EXIT=2 without ARENA2.
            Some("PROBE-AUTOMAP-001") => m34_automap::run_automap_001(),
            // M33: total persistence — SAVE-ALL-001 (A-E: 24 states roundtrip,
            // S0 census, corruption, determinism, escaping), SAVE-MIGRATION-001
            // (3 v1 fixtures → v2, idempotence, GV-05/GV-09 census),
            // SAVEGAME-READ-001 (Mode A retail MAPSAVE.SAV/IMG layout + NEGATIVE
            // N2/N3, EXIT=2 without ARENA2), SAVEGAME-WRITE-001 (synthetic
            // SAVETREE/MAPSAVE round-trips through our reader) and
            // M33-DEMO-001 (DEMO-033 headless: save → mutate → load restores
            // the snapshot exactly), spec M33 (REQ-L99-066).
            Some("SAVE-ALL-001") => m33_probe::run_save_all_001(),
            Some("SAVE-MIGRATION-001") => m33_probe::run_save_migration_001(),
            Some("SAVEGAME-READ-001") => m33_probe::run_savegame_read_001(),
            Some("SAVEGAME-WRITE-001") => m33_probe::run_savegame_write_001(),
            Some("M33-DEMO-001") => m33_probe::run_m33_demo_001(),
            // M34/W34-3 (spec §5:239-257, REQ-L99-067): PROBE-AUDIO-RT-001 —
            // 132/132 songs + 459 DAGGER.SND records, 60/60 triggers, real
            // device output (`samples_played` from the audio callback) or a
            // declared `[SKIP-DEVICE]`, and the DFU water placement.
            // EXIT=2 when MIDI.BSA/DAGGER.SND are not readable.
            Some("AUDIO-RT-001") => m34_audio_probe::run_audio_rt_001(),
            // M18 B3-d (O3/CS-12): visual precipitation fed by the M13
            // climate index; EXIT=2 when CLIMATE.PAK is not readable.
            Some("WEATHER-VISUAL-001") => m18_b3_probe::run_weather_visual_001(),
            // M18 B3-c (O7/CS-18/CS-19): census of the 6 retail panels + the
            // per-asset palette range that unblocks MAP100I0/TRAV0I00.
            Some("PANELS-001") => m18_b3_probe::run_panels_001(),
            // M34/W34-1 (spec §4:217-239): PROBE-UI-FULL-001 — the closed
            // 85-window census (16+11+7+51, `N/A-engine` per row), the
            // canonical UiShell navigating all 85, the HUD census (GV-03) and
            // the three data-derived digests ×2.
            // Phase note (2026-10-26): W34-1 shipped block A alone and the
            // probe exited 1 because block B (the 51 retail compositions) was
            // still pending at that time; W34-2a..2f composed block B and the
            // measurement is **51/51** (48 image-class + 3 video,
            // `assets_missing=0`, `generic_widgets=0`), so the probe exits
            // **0** on PASS today — same entry, real verdict.
            Some("UI-FULL-001") => m34_ui_probe::run_ui_full_001(),
            // M37/UI-1 (plan §5, CR-c45ab703): the two executable gates of the
            // seven-axis layer — PROBE-UI-BASE-001 (A1/A2/A3/A4/A7: three
            // pilots composed, painted, presented 640×480, double measurement)
            // and PROBE-UI-INPUT-001 (A5/A6: fixed literal intent script over
            // the typed stack). Both EXIT=2 without ARENA2.
            Some("PROBE-UI-BASE-001") => m37_ui1_probe::run_base(),
            Some("PROBE-UI-INPUT-001") => m37_ui1_probe::run_input(),
            // M38/UI-2 (plan §4, CR-71f2b7bf): the 27 surfaces of the creator
            // (16) and the typed configuration (11) — PROBE-UI-CREATE-001 and
            // PROBE-UI-CONFIG-001 measure A1..A7, D-2 string map, and write one
            // 640x480 BMP per surface. Both EXIT=2 without ARENA2.
            Some("PROBE-UI-CREATE-001") => m38_ui2_probe::run_create(),
            Some("PROBE-UI-CONFIG-001") => m38_ui2_probe::run_config(),
            // M39/UI-3 (plan §4.1, CR-7bf61244): PROBE-UI-JUGABLE-001 — the 51
            // `Playable` surfaces measured (nodes/glyphs/focus/opaque/digest),
            // interacted through their real block functions, gated as 8 blocks
            // + 7 axes, one BMP per surface and a double-process digest.
            // EXIT=2 without ARENA2.
            Some("PROBE-UI-JUGABLE-001") => m39_jugable_probe::run(),
            // M18 B3-a (CS-13/CS-14): street population + visible city guard.
            Some("POPULATION-001") => m18_b3a::run_population_001(),
            // M18 B3-b (CS-15/CS-16): retail dialogue + journal overlays.
            Some("RETAIL-TEXT-001") => m18_b3b::run_retail_text_001(),
            // M34/W34-4 (spec §6:257-268): PROBE-VIDEO-001 — the 33 retail
            // cinematics (17 .VID + 16 .FLC) decoded frame by frame, the
            // chunk histogram counted from the data, GV-07/GV-08 clock.
            Some("VIDEO-001") => m34_video_probe::run_video_001(),
            // M34/W34-6 (spec §1.6:109-123, GV-10/11/12/14):
            // PROBE-LIGHT-001 — torch 1.0/1.25 (ratio 1.25) + guttering,
            // six ambient values, fade 0.1/3 every 0.1 s of 3 s, GV-11 u8
            // conversion, sun 180·time at dawn 6 / dusk 18, underwater fog
            // and dungeon light 51.5 m / 0.4 s. Pure arithmetic: EXIT=0/1
            // only, digest ×2 in two separate processes.
            Some("PROBE-LIGHT-001") => m34_light_probe::run_light_001(),
            // M35 (annex, CR-2fef7f8a): PROBE-BIOME-001 blocks A–G
            // (vegetation placement + night sky) and PROBE-SPRITE-001 blocks
            // A–C (species catalogue disposition + LRU budget). Both run
            // headless against retail ARENA2 only.
            Some("BIOME-001") => m35_biome_probe::run_biome_001(),
            Some("SPRITE-001") => m35_sprite_probe::run_sprite_001(),
            // M37 (F-AUD-01, design f-aud-01-02-streaming-navmesh-CR-96142f2f):
            // STREAM-001 — async cell streaming: pure policy block P in every
            // build + block A (frame repartition) and M (measurements:
            // window under movement, tick cost, population-seed stability)
            // with `--features window`; C/D/E/F land with F3/F4/F5. EXIT=2
            // without ARENA2 (or without the window feature — the ECS
            // blocks are cfg(window) by design §2.8).
            Some("STREAM-001") => f_aud_01_stream_probe::run_stream_001(),
            // W34-7 (spec §"PLAYSESSION-001"): the twelve-scenario
            // deterministic playthrough — 12 × 18 000 = 216 000 frames at
            // 1/60 s with an FNV-1a 64 fold of the live state. EXIT=2 when
            // the M33 v2 prerequisite or the ARENA2 retail custody is absent,
            // EXIT=1 on any FAIL row, EXIT=0 on 12/12 with the same digest in
            // two separate processes.
            Some("PLAYSESSION-001") => playsession_probe::run_playsession_001(),
            other => {
                eprintln!(
                    "--probe: unknown name {other:?} (supported: VIEW-001, CITY-001, INTERIOR-001, COMMERCE-001, FORMULA-001, WORLD-001, ALL-001, READMODEL-001, SESSION-001, RMB-001, SEED-001, RDB-001, INTERIOR-TRANSIT-001, AUDIO-001, TEXTURE-001, TERRAIN-001, CLIMATE-001, NPC-001, TEXT-RSC-001, QUEST-ORIGINAL-001, UI-001, MAP-001, DEMO-001, LOCOMOTION-001, STEALTH-001, STATS-001, SKILLS-001, PROGRESSION-001, ITEM-001, EQUIP-001, LOOT-001, COMBAT-001, COMBAT-LOOP-001, DEATH-001, MAGIC-001, EFFECT-001, SPELLMAKER-001, ENCHANT-001, POTION-001, MONSTER-001, ENEMY-AI-001, ENCOUNTER-001, NPC-AI-001, DIALOGUE-001, RUMOR-001, NPC-DEMO-001, FACTION-001, GUILD-001, M27-DEMO-001, CRIME-001, LAW-001, EXEC-001, LIFECYCLE-001, JOURNAL-001, CHUNK-001, SCHEDULE-001, MODB-ISOLATION-001, SHOP-001, ECONOMY-001, BANK-001, ORIGIN-001, ATTACK-KEY-001, MENU-001, SAVE-ALL-001, SAVE-MIGRATION-001, SAVEGAME-READ-001, SAVEGAME-WRITE-001, M33-DEMO-001, VIDEO-001, PLAYSESSION-001, BIOME-001, SPRITE-001, STREAM-001)"
                );
                2
            }
        };
        // M3: with the `window` feature and PROBE-CITY-001 on
        // PASS the M3 window opens (city + sky + clouds) as visual
        // evidence (criteria (a)/(c)) — the Anti-Screen invariant is
        // guaranteed by the headless probe; without the feature the probe is
        // 100% headless (CI-runnable, 0 GPU), same as VIEW-001.
        #[cfg(feature = "window")]
        {
            if code == 0 && args.get(pos + 1).map(String::as_str) == Some("CITY-001") {
                let path_checker = daggerfall_inventory::path_checker::PathChecker::new();
                if let Err(reason) = m3_window::run_windowed_m3(&path_checker) {
                    eprintln!("M3 (feature window): {reason}");
                    eprintln!(
                        "M3: probe CITY-001 PASS — the M3 window did not open (probe verdict intact)"
                    );
                }
            }
            // M4: same scheme as M3 — with `window` and
            // PROBE-INTERIOR-001 on PASS the M4 window opens (navigable shop
            // interior + ❤️ clock in the title) as visual evidence of (a)/(c);
            // without the feature the probe is 100% headless.
            if code == 0 && args.get(pos + 1).map(String::as_str) == Some("INTERIOR-001") {
                let path_checker = daggerfall_inventory::path_checker::PathChecker::new();
                if let Err(reason) = m4_window::run_windowed_m4(&path_checker) {
                    eprintln!("M4 (feature window): {reason}");
                    eprintln!(
                        "M4: probe INTERIOR-001 PASS — the M4 window did not open (probe verdict intact)"
                    );
                }
            }
            // M5: same scheme as M3/M4 — with `window` and
            // PROBE-COMMERCE-001 on PASS the M5 window opens (interior
            // with NPC Vendel + dialogue/[B]/[V] menu in the title bar with
            // visible gold — visual evidence of criterion (a)); without the
            // feature the probe is 100% headless.
            if code == 0 && args.get(pos + 1).map(String::as_str) == Some("COMMERCE-001") {
                let path_checker = daggerfall_inventory::path_checker::PathChecker::new();
                if let Err(reason) = m5_window::run_windowed_m5(&path_checker) {
                    eprintln!("M5 (feature window): {reason}");
                    eprintln!(
                        "M5: probe COMMERCE-001 PASS — the M5 window did not open (probe verdict intact)"
                    );
                }
            }
            // M7: same scheme as M3-M5 — with `window` and
            // PROBE-READMODEL-001 on PASS the M7 window opens (automap
            // with tiles in the client area + the 3 read models in the title
            // bar — visual evidence of criteria (a)-(c)); without the
            // feature the probe is 100% headless (0 GPU, 0 .BSA).
            if code == 0 && args.get(pos + 1).map(String::as_str) == Some("READMODEL-001") {
                let path_checker = daggerfall_inventory::path_checker::PathChecker::new();
                if let Err(reason) = m7_window::run_windowed_m7(&path_checker) {
                    eprintln!("M7 (feature window): {reason}");
                    eprintln!(
                        "M7: probe READMODEL-001 PASS — the M7 window did not open (probe verdict intact)"
                    );
                }
            }
            // M8: same scheme as M3-M7 — with `window` and
            // PROBE-SESSION-001 on PASS the M8 window opens (session
            // restored from `saves/session.dfs`: automap of restored chunks
            // + VE states in the title bar — visual evidence of (a)-(c));
            // without the feature the probe is 100% headless (0 GPU, 0 .BSA).
            if code == 0 && args.get(pos + 1).map(String::as_str) == Some("SESSION-001") {
                let path_checker = daggerfall_inventory::path_checker::PathChecker::new();
                if let Err(reason) = m8_window::run_windowed_m8(&path_checker) {
                    eprintln!("M8 (feature window): {reason}");
                    eprintln!(
                        "M8: probe SESSION-001 PASS — the M8 window did not open (probe verdict intact)"
                    );
                }
            }
            // M17: with `window` and PROBE-UI-001 on PASS the
            // window of the 4 retail panels opens (IMG/CIF/FNT from
            // ARENA2) presented ×2 nearest + letterbox; without the feature
            // the probe is 100% headless (0 GPU).
            if code == 0 && args.get(pos + 1).map(String::as_str) == Some("UI-001") {
                let path_checker = daggerfall_inventory::path_checker::PathChecker::new();
                if let Err(reason) = m17_window::run_windowed_m17(&path_checker) {
                    eprintln!("M17 (feature window): {reason}");
                    eprintln!(
                        "M17: probe UI-001 PASS — the M17 window did not open (probe verdict intact)"
                    );
                }
            }
        }
        std::process::exit(code);
    }

    println!("daggerfall-bevy-native F0 — organs online:");
    println!("  nervous    -> {}", daggerfall_nervous::organ_name());
    println!("  heart      -> {}", daggerfall_heart::organ_name());
    println!("  memory     -> {}", daggerfall_memory::organ_name());
    println!("  behavior   -> {}", daggerfall_behavior::organ_name());
    println!("  perception -> {}", daggerfall_perception::organ_name());
    println!("  skills     -> {}", daggerfall_skills::organ_name());
    println!("  inventory  -> {}", daggerfall_inventory::organ_name());
    println!("  antibodies -> {}", daggerfall_antibodies::organ_name());

    let path_checker = daggerfall_inventory::path_checker::PathChecker::new();
    println!(
        "  PathChecker: use_custom_universe={}",
        path_checker.use_custom_universe()
    );

    // M1: only with the `window` feature compiled is there a
    // window+render path; without it, the F0 headless behavior is identical.
    // M2: the same path enables the first-person camera +
    // grid collisions over the loaded block.
    #[cfg(feature = "window")]
    {
        // M18: windowed startup opens the INTEGRATED DEMO
        // (textured retail city + terrain + NPCs + UI). `--window` /
        // `--legacy-m2` keep the M2 path (one ARCH3D block) as a flag.
        let legacy_m2 = args
            .iter()
            .any(|arg| arg == "--window" || arg == "--legacy-m2");
        // B1-a: `--origin x,y` / `--yaw rad` (env `DF_M18_ORIGIN` / `DF_M18_YAW`).
        // A malformed value is an explicit error, never a silently ignored flag.
        let overrides = match m18_demo::parse_overrides(&args) {
            Ok(overrides) => overrides,
            Err(err) => {
                eprintln!("M18: {err}");
                std::process::exit(2);
            }
        };
        let launched = if legacy_m2 {
            run_windowed(&path_checker)
        } else {
            m18_window::run_windowed_m18(&path_checker, overrides)
        };
        if let Err(reason) = launched {
            eprintln!("M18 (feature window): {reason}");
            if legacy_m2 {
                eprintln!("M2: fallback to the F0 headless app (no window)");
            } else {
                eprintln!("M18: fallback to the F0 headless app (no window)");
            }
            App::new().run();
        }
        // M-1 (QA M1): no `return;` — with the `window` feature
        // the block is the last one of `main()` and clippy flagged it
        // (clippy::needless_return, EXIT=101). Implicit end of main.
    }

    #[cfg(not(feature = "window"))]
    {
        // Empty Bevy app: F0 smoke run. Plugins and systems arrive with their
        // organ lanes.
        App::new().run();
    }
}

/// M1/M2: window+render with ONE block of real ARCH3D.BSA,
/// Mode A — in M2 with first-person camera (spawn inside the block) +
/// grid collisions (INV-L99-5).
///
/// Source custody (PRE-2 §8 rule 3): inventory resolves the origin —
/// `PathChecker` validates the root and ModeState=A governs (the bridge has 0
/// mode logic, rule 1). Resolution shared with the probe in
/// [`mode_a_arch3d_path`].
#[cfg(feature = "window")]
fn run_windowed(
    path_checker: &daggerfall_inventory::path_checker::PathChecker,
) -> Result<(), String> {
    // Mode A only (milestone boundary): ModeState=B is left for
    // PHASE 4 (REQ-L99-028 toggle).
    if path_checker.use_custom_universe() {
        return Err("M2 supports Mode A only (ModeState=B out of scope)".into());
    }

    let bsa_path = mode_a_arch3d_path()?;

    let data = std::fs::read(&bsa_path).map_err(|e| format!("read {bsa_path:?}: {e}"))?;
    let (record_index, record_id, spec) = first_convertible_block(&data)?;
    println!(
        "M2 VE: {bsa_path:?} ({} bytes) -> record idx={record_index} id={record_id}: {} vertices, {} triangles (meters, Bevy handedness)",
        data.len(),
        spec.positions.len(),
        spec.triangle_count()
    );

    // M2: collision grid derived from the block + first-person spawn inside.
    use daggerfall_parser_bevy_bridge as bridge;
    let grid = bridge::CollisionGrid::from_mesh_spec(&spec, bridge::DEFAULT_CELL_M);
    let spawn = grid
        .find_spawn(&spec, bridge::EYE_HEIGHT_M, bridge::PLAYER_RADIUS_M)
        .ok_or("M2: no spawn (no column with floor + free eye in the block)")?;
    let yaw = m2_facing_yaw(&spec, &spawn.eye);
    println!(
        "M2 VE: first-person spawn eye=({:.2},{:.2},{:.2}) floor_y={:.2} yaw={yaw:.2} rad · grid {} solid cells dims {:?} · WASD+mouse-look (Esc releases the cursor)",
        spawn.eye[0],
        spawn.eye[1],
        spawn.eye[2],
        spawn.floor_y,
        grid.solid_cell_count(),
        grid.dims()
    );

    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: format!(
                    "daggerfall-bevy-native M2 — first person ARCH3D.BSA record {record_id} (CR-bbd5db82)"
                ),
                resolution: (1280u32, 720u32).into(),
                ..default()
            }),
            ..default()
        }))
        .insert_resource(LoadedBlock {
            spec,
            record_index,
            record_id,
        })
        .insert_resource(m2_player::M2Grid(grid))
        .insert_resource(m2_player::ActionState::default())
        .insert_resource(m2_player::M2Spawn {
            eye: Vec3::from(spawn.eye),
            yaw,
        })
        .add_systems(Startup, (m2_spawn_scene, m2_player::m2_setup_cursor))
        .add_systems(
            Update,
            (
                m2_player::m2_gather_action,
                m2_player::m2_escape_cursor,
                m2_player::m2_apply_player_action,
            ),
        )
        .run();
    Ok(())
}

/// Resolves the Mode A ARENA2 directory: root `$DF_APP_ROOT` or by default
/// `$HOME/Games/Daggerfall/app` (field-config `data.game_dir`), validated with
/// `looks_like_data_root` (both capitalization variants).
///
/// M14: the implementation lives in the library target
/// (`daggerfall_app::mode_a`) so the production audio runtime and the
/// integration tests resolve retail data exactly like this binary does —
/// one copy, 0 divergent logic.
fn mode_a_arena2_dir() -> Result<std::path::PathBuf, String> {
    daggerfall_app::mode_a::arena2_dir()
}

/// Resolves a file inside the Mode A ARENA2 (see [`mode_a_arena2_dir`]).
/// Source custody (PRE-2 §8 rule 3): M10 uses it for MAPS.BSA /
/// BLOCKS.BSA / ARCH3D.BSA (PROBE-RMB-001 / PROBE-SEED-001); ModeState=A
/// custody is done by the callers via `PathChecker`.
fn mode_a_arena2_path(name: &str) -> Result<std::path::PathBuf, String> {
    daggerfall_app::mode_a::arena2_path(name)
}

/// Resolves the Mode A ARCH3D.BSA path (specialization of
/// [`mode_a_arena2_path`] for the geometry record). Shared by the
/// M2 window and by PROBE-VIEW-001 (same origin custodian, PRE-2 §8
/// rule 3; ModeState=A custody is done by the callers via
/// `PathChecker`).
fn mode_a_arch3d_path() -> Result<std::path::PathBuf, String> {
    mode_a_arena2_path("ARCH3D.BSA")
}

/// Initial M2 yaw: looks toward the nearest geometry among 8 horizontal
/// directions (the spawn ends up framing a wall, not emptiness).
#[cfg(feature = "window")]
fn m2_facing_yaw(
    spec: &daggerfall_parser_bevy_bridge::MeshSpec,
    eye: &[f32; 3],
) -> f32 {
    use daggerfall_parser_bevy_bridge::{first_geometry_hit, view_probe::LOOK_DIRS};
    let (min, max) = match spec.bounds() {
        Some(bounds) => bounds,
        None => return 0.0,
    };
    let max_dist = (0..3)
        .map(|axis| (max[axis] - min[axis]).powi(2))
        .sum::<f32>()
        .sqrt()
        .max(1.0);
    let mut best: Option<(f32, [f32; 2])> = None;
    for dir in LOOK_DIRS {
        if let Some(t) = first_geometry_hit(spec, *eye, [dir[0], 0.0, dir[1]], max_dist) {
            if best.is_none_or(|(best_t, _)| t < best_t) {
                best = Some((t, dir));
            }
        }
    }
    match best {
        // Forward(yaw) = (-sin, 0, -cos) = dir → yaw = atan2(-dx, -dz).
        Some((_, dir)) => (-dir[0]).atan2(-dir[1]),
        None => 0.0,
    }
}

/// Deterministic block selection (M1, FIRST record of the directory —
/// BSA order, sha-pinned input S1 — that converts to visible geometry).
/// Shared by the M2 window and PROBE-VIEW-001.
fn first_convertible_block(
    data: &[u8],
) -> Result<(usize, u32, daggerfall_parser_bevy_bridge::MeshSpec), String> {
    use daggerfall_parser_bevy_bridge as bridge;
    let archive = bridge::parse_bsa(data).map_err(|e| format!("invalid ARCH3D.BSA: {e}"))?;
    for (index, entry) in archive.entries.iter().enumerate() {
        let bridge::Entry::Numbered { record_id, .. } = entry else {
            continue;
        };
        let Ok(spec) = bridge::mesh_spec_at(&archive, data, index) else {
            continue;
        };
        if spec.triangle_count() > 0 {
            return Ok((index, *record_id, spec));
        }
    }
    Err("no ARCH3D.BSA record converts to visible geometry".into())
}

/// M3 landmarks: first `max` convertible records of
/// ARCH3D.BSA (≥1 triangle, in directory order — deterministic,
/// sha-pinned input S1) that are also **solid** (§5a). Delegates
/// to the single function [`daggerfall_parser_bevy_bridge::solid_convertible_blocks`],
/// the SAME one the `probe_city_001_runtime` test consumes (guardrail §5a.2:
/// shared CLI↔test selector, 0 divergent duplicated logic). The resulting
/// specs are placed as fit-to-lot landmarks in the city (shared by the
/// `CITY-001` probe and the M3 window).
///
/// **Why the test subject changed from convertible to convertible+solid
/// (§5a.3 — 0 relaxation of the M3 invariants):** the previous calibration of
/// `manzana_collision`/`street_legibility` was done with inverted landmarks:
/// the first convertible `44005` has an OPEN 11 m passage along the `+X` axis
/// at `z=40` (the eye height of `street_spawn`), and the flank ray and the
/// `r=0.3` walk escape through it (HH-1
/// `hh-1-20261020-og-backend-m10-yneg-partial.md`). The criterion
/// —4 AABB faces without a gap for the `PLAYER_RADIUS_M` sphere at eye height
/// `EYE_HEIGHT_M`, **evaluated in the fit-to-lot world** (scale
/// `fit_to_lot_scale`, not raw local meters: a 3.15 m model scaled ×8.89 would
/// put the local eye at 0.18)— lives in `bridge::landmark_is_solid`.
///
/// [`first_convertible_block`] does NOT change: M2/PROBE-VIEW-001 needs a spawn
/// INSIDE the block (a solid block would have no walkable interior).
fn convertible_blocks(
    data: &[u8],
    max: usize,
) -> Result<Vec<daggerfall_parser_bevy_bridge::MeshSpec>, String> {
    use daggerfall_parser_bevy_bridge as bridge;
    let solid = bridge::solid_convertible_blocks(data, max)?;
    let ids: Vec<String> = solid.iter().map(|(id, _)| id.to_string()).collect();
    eprintln!(
        "solid landmarks: {} of ARCH3D.BSA -> ids [{}] (4 faces closed at y={}, r={}; shared CLI↔test selection)",
        solid.len(),
        ids.join(", "),
        bridge::EYE_HEIGHT_M,
        bridge::PLAYER_RADIUS_M
    );
    Ok(solid.into_iter().map(|(_, spec)| spec).collect())
}

/// Block loaded at runtime (Bevy world resource; neutral name since
/// M2 — the same resource fed the M1 view).
#[cfg(feature = "window")]
#[derive(Resource)]
struct LoadedBlock {
    spec: daggerfall_parser_bevy_bridge::MeshSpec,
    record_index: usize,
    record_id: u32,
}

/// M2 scene: PBR block (M1) + **first-person camera at the spawn inside the
/// block** (before: M1 orbit camera framing from outside — M2 delta,
/// documented) + directional light intact.
#[cfg(feature = "window")]
fn m2_spawn_scene(
    mut commands: Commands,
    block: Res<LoadedBlock>,
    spawn: Res<m2_player::M2Spawn>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    use daggerfall_parser_bevy_bridge as bridge;

    let mesh = bridge::mesh_from_spec(&block.spec, &mut meshes);
    let material = bridge::material_from_spec(&bridge::MaterialSpec::default(), &mut materials);
    commands.spawn((Mesh3d(mesh), MeshMaterial3d(material)));

    // First-person camera: eye + yaw computed by find_spawn/facing_yaw
    // (inside the parsed block, looking at geometry).
    let radius = block.spec.bounding_radius().unwrap_or(1.0).max(0.25);
    commands.spawn((
        Camera3d::default(),
        Transform::from_translation(spawn.eye)
            .with_rotation(Quat::from_euler(EulerRot::YXZ, spawn.yaw, 0.0, 0.0)),
        m2_player::M2Player {
            yaw: spawn.yaw,
            pitch: 0.0,
            radius: bridge::PLAYER_RADIUS_M,
        },
    ));

    // Directional light (default illuminance = AMBIENT_DAYLIGHT 10,000 lux).
    commands.spawn((
        DirectionalLight::default(),
        Transform::from_rotation(Quat::from_euler(EulerRot::XYZ, -0.9, 0.7, 0.0)),
    ));

    println!(
        "M2 VE: first-person window with ARCH3D.BSA block — record idx={} id={} ({} triangles, radius {:.2} m)",
        block.record_index,
        block.record_id,
        block.spec.triangle_count(),
        radius
    );
}
