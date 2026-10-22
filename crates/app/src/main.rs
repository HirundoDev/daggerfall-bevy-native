//! daggerfall-app — Bevy application entry (F0 scaffold).
//!
//! Bevy 0.20.0 with default features disabled: the F0 scaffold compiles and
//! runs headless. Rendering/window features are enabled by the perception
//! lane when graphics land (ADR-GAME-004 posture: Bevy >= 0.19).
//!
//! M1 (CR-bbd5db82): the additive `window` feature enables window+render —
//! `cargo run -p daggerfall-app --features window` — and renders ONE
//! parsed block of real ARCH3D.BSA (Mode A) through the bridge crate
//! `daggerfall-parser-bevy-bridge` (DAG app → bridge → parser). The default
//! headless mode (CI F0) stays intact: conditional extension,
//! 0 deletes of the original scaffold.
//!
//! M2 (CR-bbd5db82): first-person camera (WASD + mouse-look, spawn inside the
//! block) + grid collisions (INV-L99-5) and the Anti-Screen probe
//! `--probe VIEW-001` — runs in **default headless** config (0 render), delta
//! documented against the `[INFERENCE]` command of VSIM §4 row 6.
//!
//! M3 (CR-bbd5db82): navigable city — deterministic 3D streets (MISSING-SPEC
//! GRIDCITY → synthetic layout over real ARCH3D, delta in note M3) + unlit
//! sky and clouds. Anti-Screen probe `--probe CITY-001` in default headless
//! config (same scheme as VIEW-001); with `--features window` the same
//! command, after PASS, opens the M3 window (visual evidence streets+sky+clouds).
//!
//! M4 (CR-bbd5db82 · real wiring CR-265af36b): first interior building —
//! Anti-Screen probe `--probe INTERIOR-001` (street→interior through a door
//! without crossing a wall + 24 h time system) in **default headless** config.
//! Block A uses the synthetic reference shop (0 I/O) and block C the **real**
//! interior `.RDB` + `ARCH3D` (M11); the premise "DFON1.BSA missing" was FALSE
//! (WI-3). The time system is the organic ❤️ heart
//! (`daggerfall_heart::clock`). With `--features window` the same command,
//! after PASS, opens the M4 window, which renders the **real retail interior**
//! of the deterministic subject of `RDB-001` + clock in the title (`m4_window`).
//!
//! M5 (CR-bbd5db82): interactive commerce — Anti-Screen probe
//! `--probe COMMERCE-001` (enter→interact→buy: **gold debited + item added**,
//! solid NPC + dialogue + [B]/[V] inventory-lite menu) in **default headless**
//! config, with block C = PROBE-FORMULA-001 (DFU golden vectors for the trade
//! price, N2). `--probe FORMULA-001` runs the formula block alone. With
//! `--features window` COMMERCE-001 on PASS opens the M5 window (`m5_window`:
//! NPC + menu in the title).
//!
//! M6 (CR-bbd5db82): integrated playthrough — Anti-Screen probe
//! `--probe WORLD-001` (street spawn → shop → trade → exit → return,
//! 0 pass-throughs, 3 crossings through the door gap) in **default headless**
//! config, plus the **unified suite** `--probe ALL-001` (all M2-M6 probes with
//! aggregated evidence and exit 0 only if ALL PASS — the documented command
//! of note M6).
//!
//! M7 (CR-bbd5db82): session read models — Anti-Screen probe
//! `--probe READMODEL-001` (the 3 PD4 T7 read models — HUD ←RPGStats ·
//! Automap ←Chunks · inventory/quests panel ←AssetSet/Quest — projected
//! from the ECS at runtime: change in ECS ⇒ change in read model +
//! determinism, REQ-L99-038/039/040) in **default headless** config (0
//! .BSA reads: declared synthetic session). With `--features window` and
//! PASS it opens the M7 window (`m7_window`: automap with tiles in the client
//! area + the 3 read models in the title bar). The ALL-001 suite becomes
//! 7 probes (M2-M7).
//!
//! M8 (CR-bbd5db82): closed session loop — Anti-Screen probe
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
//! M10 (CR-28d62bea): retail city `.RMB` — Anti-Screen Probes
//! `--probe RMB-001` (real cities of the Daggerfall region parsed
//! from ARENA2 — MAPS.BSA + BLOCKS.BSA + ARCH3D.BSA — with 0 panic, 100%
//! of the BuildingDataList consumed and retail navigability INV-L99-5) and
//! `--probe SEED-001` (real ext: the capital composed TWICE from the
//! same bytes ⇒ same city byte-by-byte, REQ-L99-023) in **default
//! headless** config (Mode A custody, 0 packaged bytes — INV-
//! L99-6). The ALL-001 suite becomes 10 probes (M2-M10).
//!
//! M14 (CR-28d62bea): retail audio — Anti-Screen probe `--probe AUDIO-001`
//! `--probe AUDIO-001` (MIDI.BSA catalog parsed with our own HMI parser: N/total songs,
//! 0 panic + song trigger by city day/night context replicating DFU
//! `SongManager` + SoundClips effects over DAGGER.SND with SKIP+warning
//! degradation if the file is missing — REQ-L99-045/046, Mode A custody).
//! The ALL-001 suite becomes 11 probes (M2-M14).
//!
//! M11 (CR-3f9d13a2): retail interior `.RDB` — Anti-Screen probe
//! `--probe RDB-001` (census of ALL `.RDB` records of BLOCKS.BSA +
//! deterministic subject by directory order + real 12-check probe:
//! street→real flanked door→interior→door→street with 0 pass-throughs,
//! INV-L99-5, REQ-L99-043) in **default headless** config (Mode A
//! custody). DoD mapping: PROBE-INTERIOR-001 (DoD) = INTERIOR-RDB-001 = `--probe
//! RDB-001`; the real variant also runs as block C inside
//! `--probe INTERIOR-001` (PASS requires synthetic A + B time + C real). The
//! ALL-001 suite becomes 12 probes (M2-M14).
//!
//! M12 (CR-46a43aba): retail textures `TEXTURE.*` — Anti-Screen probe
//! `--probe TEXTURE-001` (3 files × 4 rotations decoded at
//! runtime from ARENA2 + FNV-1a64 digest of the pixels against 12
//! constants + consumable `PAL.PAL` palette, DoD §c / WI-4) in
//! **default headless** config (Mode A custody, INV-L99-6: 0 retail bytes in
//! git). The ALL-001 suite becomes 13 probes (M2-M14 + M12).
//!
//! M13 (CR-46a43aba): terrain + retail biomes — Anti-Screen probe
//! `--probe TERRAIN-001` (8 height goldens derived from the sampler
//! formula + digests ×2 + walkable terrain outside the city: `WOODS.WLD`) and
//! `--probe CLIMATE-001` (0 climates outside 223..=232, 61 regions,
//! `NatureSet × Season → archive`, coastal dilation: `CLIMATE.PAK` /
//! `POLITIC.PAK`), DoD §e / WI-6, Mode A custody. The ALL-001 suite becomes
//! 14 probes.
//!
//! M15 (CR-46a43aba): NPC population — Anti-Screen probe `--probe NPC-001`
//! (`NAMEGEN.DAT` 11 banks parsed with 18 name goldens, deterministic
//! population by region/subclimate with 8-dir flats from `TEXTURE.256`,
//! retail captions from `TEXT.RSC`, crime guards and `place npc` /
//! `create npc at`), DoD §f / WI-7 / REQ-L99-047, Mode A custody. The suite
//! ALL-001 becomes 15 probes.
//!
//! M18 (CR-46a43aba): 100% real closure — `--probe MAP-001` resolves the
//! complete `MAPS.BSA` index (**61 regions / 15,251 locations**, no
//! sampling: hardened pack §5.3.1), with region + `mapIndex` + coordinate and
//! `.RMB` city of each location resolved from `BLOCKS.BSA`; FNV-1a 64 digest
//! identical in 2 passes, 0 panic, 0 skip, `MAPS.BSA` missing ⇒
//! `EXIT=2`. REQ-L99-051, DoD §a/§i, Mode A custody. The ALL-001 suite becomes
//! 18 probes.
//!
//! M19 (CR-b109bfe3 · inherits CR-8313e821): player locomotion and body —
//! `--probe LOCOMOTION-001` (spec §7: block A formulas §1.3/§2.1/§3.1/§5.3,
//! block B runtime engine in a synthetic world with `dt=1/60` — speeds,
//! jump, 10 m fall, climbing, levitation, swimming/air, overload and
//! 0 penetrations, block C stealth/pilfering, block D retail demo), plus the
//! alias `--probe STEALTH-001` (block C only). With `--features window` the
//! M18 demo uses `behavior::locomotion::Locomotion::step` and the title bar
//! shows mode + air. REQ-L99-052. **The ALL-001 suite becomes 20 probes**
//! (18/18 M18 freeze → 19/19 with `DEMO-001` → 20/20 with `LOCOMOTION-001`).
//!
//! M20 (CR-a64582b8 · CR-79173139): character — Anti-Screen probe
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
//! M21 (CR-776ad8a5 · inherits CR-5243af63/CR-654173c5): items, equipment,
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
//! M22 (CR-be05b6d8 · inherits CR-d1167088): combat — Anti-Screen probe
//! `--probe COMBAT-001` (spec §6 A: **47 checks** = GV-HIT 01-15 + GV-DMG
//! 01-17 + GV-CAD 01-15 with injected rolls), `--probe COMBAT-LOOP-001`
//! (600 ticks @ 60 Hz, swings=60, FNV-1a 64 digest ×2 and seed-sensitive) and
//! `--probe DEATH-001` (GV-DTH 01-06 + OnDeath latch ×1 of the player and the
//! enemy, fall 0.85 / fade 2.0 / reset 3.0). REQ-L99-055.
//! **The ALL-001 suite becomes 23 probes** (22/22 with `ITEM-001` → 23/23 with
//! `COMBAT-001`; `COMBAT-LOOP-001` and `DEATH-001` run outside the suite,
//! same pattern as `EQUIP-001`/`LOOT-001`).
//!
//! M23 (CR-a0b60b1c · closure CR-7a8d1842 · inherits CR-5243af63): magic —
//! Anti-Screen probe `--probe MAGIC-001` (spec §10: SPELLS.STD census +
//! spell cost + casting with the M20 Character SP, **44 checks**
//! A-D), plus the standalone `--probe EFFECT-001` (effect model: 19
//! checks B/C + digests ×2) and `--probe SPELLMAKER-001` (spell
//! creation: 7 checks). REQ-L99-056. **The ALL-001 suite becomes 24 probes**
//! (23/23 with `COMBAT-001` → 24/24 with `MAGIC-001`; `EFFECT-001` and
//! `SPELLMAKER-001` run outside the suite, same pattern as
//! `EQUIP-001`/`LOOT-001`).
//!
//! M24 (CR-f26e353c): enchantment/alchemy — Anti-Screen probe
//! `--probe ENCHANT-001` (spec §5 A-C: MAGIC.DEF census + per-material
//! budget + accounting/gates, **34 checks**, plus the §3.4 contract of
//! 19 M23 keys) and the standalone `--probe POTION-001` (20 recipes · **23
//! checks** GV-RCP/GV-POT + SHA-256 digest ×2). REQ-L99-057. **The suite
//! ALL-001 becomes 25 probes** (24/24 with `MAGIC-001` → 25/25 with
//! `ENCHANT-001`; `POTION-001` runs outside the suite, same pattern as
//! `EFFECT-001`/`SPELLMAKER-001`).
//!
//! M25 (CR-23c1381c): monsters, enemy AI and encounters — Anti-Screen probe
//! `--probe MONSTER-001` (spec §10 A-D: container census,
//! 43 `ENEMY###.CFG` of 74 B, class HP and deterministic scene) and the
//! standalone `--probe ENEMY-AI-001` (D1-D13 formulas + scenario
//! `Idle→Pursue`/give-up/doors) and `--probe ENCOUNTER-001` (E/B tables,
//! 1000 seeded rolls, dungeon with and without alarm, pacification F1-F7).
//! REQ-L99-058. **The ALL-001 suite becomes 26 probes** (25/25 with
//! `ENCHANT-001` → 26/26 with `MONSTER-001`; `ENEMY-AI-001` and
//! `ENCOUNTER-001` run outside the suite, same pattern as
//! `EFFECT-001`/`SPELLMAKER-001`).
//!
//! M26 (CR-94dcfe15): living NPCs — Anti-Screen probe `--probe NPC-AI-001`
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
//! M27 (CR-989281d9): guilds and factions — Anti-Screen probe
//! `--probe FACTION-001` (spec §7.1 A-D: GV-27-01..08 formulas over fixture,
//! determinism ×2 with `DFRandom::srand(0x1234)`, `DISK:FACTION.TXT` census and
//! `GetGuildGroup`) and `--probe GUILD-001` (§7.2 A-D: rank GV-27-11..20,
//! services GV-27-21..35, propagation GV-27-36..42 with `PROP_TREE` and menu
//! gating). `--probe M27-DEMO-001` runs the §9 demo over retail: joining the
//! Fighter's Guild (id 41), rising to rank 9 and measuring the benefit. REQ-L99-060.
//! **The ALL-001 suite becomes 28 probes** (27/27 with `NPC-AI-001` → 28/28 with
//! `GUILD-001`; `FACTION-001` and `M27-DEMO-001` run outside the suite).

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
mod m11_probe;
mod m12_probe;
mod m13_probe;
mod m14_probe;
mod m15_probe;
mod m16_probe;
mod m17_probe;
mod m18_probe;
mod m18_demo;
mod m19_probe;
mod m20_probe;
mod m21_probe;
mod m22_probe;
mod m23_probe;
mod m24_probe;
mod m25_probe;
mod m26_probe;
mod m27_probe;
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
            // M6 (CR-bbd5db82): integrated playthrough + unified suite.
            Some("WORLD-001") => m6_probe::run_world_001(),
            Some("ALL-001") => m6_probe::run_probe_all(),
            // M7 (CR-bbd5db82): session read models (PD4 T7).
            Some("READMODEL-001") => m7_probe::run_readmodel_001(),
            // M8 (CR-bbd5db82): session save/load roundtrip (INV-L99-4).
            Some("SESSION-001") => m8_probe::run_session_001(),
            // M10 (CR-28d62bea): retail city .RMB (REQ-L99-042/006/023).
            Some("RMB-001") => m10_probe::run_rmb_001(),
            Some("SEED-001") => m10_probe::run_seed_001(),
            // M11 (CR-3f9d13a2): retail interior .RDB (REQ-L99-043).
            Some("RDB-001") => m11_probe::run_rdb_001(),
            // M14 (CR-28d62bea): retail audio MIDI.BSA + DAGGER.SND (REQ-L99-045/046).
            Some("AUDIO-001") => m14_probe::run_audio_001(),
            // M12 (CR-46a43aba): retail textures TEXTURE.* (DoD §c / WI-4).
            Some("TEXTURE-001") => m12_probe::run_texture_001(),
            // M13 (CR-46a43aba): retail terrain + biomes WOODS.WLD / CLIMATE.PAK (DoD §e / WI-6).
            Some("TERRAIN-001") => m13_probe::run_terrain_001(),
            Some("CLIMATE-001") => m13_probe::run_climate_001(),
            // M15 (CR-46a43aba): NPC population + retail captions (DoD §f / WI-7).
            Some("NPC-001") => m15_probe::run_npc_001(),
            // M16 (CR-778b1a85): retail text + original quests (REQ-L99-048/049).
            Some("TEXT-RSC-001") => m16_probe::run_text_rsc_001(),
            Some("QUEST-ORIGINAL-001") => m16_probe::run_quest_original_001(),
            // M17 (CR-778b1a85): 1996 aesthetic UI — assets, font and panels (REQ-L99-050).
            Some("UI-001") => m17_probe::run_ui_001(),
            // M18 (CR-46a43aba): complete MAPS.BSA index — 61/15.251 (REQ-L99-051).
            Some("MAP-001") => m18_probe::run_map_001(),
            // M18 (CR-5d88011a): integrated demo (city+T+NPC+UI) headless.
            Some("DEMO-001") => m18_demo::run_demo_001(),
            // M19 (CR-b109bfe3): DFU locomotion (REQ-L99-052) + stealth.
            Some("LOCOMOTION-001") => m19_probe::run_locomotion_001(),
            Some("STEALTH-001") => m19_probe::run_stealth_001(),
            // M20 (CR-a64582b8/CR-79173139): character — classes/attributes/
            // derived stats/biography (STATS-001) and skills/progression (SKILLS-001).
            Some("STATS-001") => m20_probe::run_stats_001(),
            Some("SKILLS-001") => m20_probe::run_skills_001(),
            // M21 (CR-776ad8a5): items/equipment/durability (ITEM-001, in the
            // suite) + deterministic loot (LOOT-001), spec M21 (REQ-L99-054).
            Some("ITEM-001") => m21_probe::run_item_001(),
            Some("EQUIP-001") => m21_probe::run_equip_001(),
            Some("LOOT-001") => m21_probe::run_loot_001(),
            // M22 (CR-be05b6d8): combat — hit/damage/cadence (COMBAT-001,
            // in the suite), 600-tick loop (COMBAT-LOOP-001) and death
            // (DEATH-001), spec M22 (REQ-L99-055).
            Some("COMBAT-001") => m22_probe::run_combat_001(),
            Some("COMBAT-LOOP-001") => m22_probe::run_combat_loop_001(),
            Some("DEATH-001") => m22_probe::run_death_001(),
            // M23 (CR-a0b60b1c): magic — SPELLS.STD + cost + casting
            // (MAGIC-001, in the suite), effect model (EFFECT-001) and
            // spellmaker (SPELLMAKER-001), spec M23 (REQ-L99-056).
            Some("MAGIC-001") => m23_probe::run_magic_001(),
            Some("EFFECT-001") => m23_probe::run_effect_001(),
            Some("SPELLMAKER-001") => m23_probe::run_spellmaker_001(),
            // M24 (CR-f26e353c): enchantment/alchemy — ENCHANT-001 (in the
            // suite) and POTION-001 (standalone), spec M24 (REQ-L99-057).
            Some("ENCHANT-001") => m24_probe::run_enchant_001(),
            Some("POTION-001") => m24_probe::run_potion_001(),
            // M25 (CR-23c1381c): monsters/AI/encounters — MONSTER-001 (in the
            // suite) + ENEMY-AI-001 and ENCOUNTER-001 (standalone), spec M25.
            Some("MONSTER-001") => m25_probe::run_monster_001(),
            Some("ENEMY-AI-001") => m25_probe::run_enemy_ai_001(),
            Some("ENCOUNTER-001") => m25_probe::run_encounter_001(),
            // M26 (CR-94dcfe15): living NPCs — NPC-AI-001 (in the suite),
            // DIALOGUE-001, RUMOR-001 and NPC-DEMO-001 (standalone), spec M26.
            Some("NPC-AI-001") => m26_probe::run_npc_ai_001(),
            Some("DIALOGUE-001") => m26_probe::run_dialogue_001(),
            Some("RUMOR-001") => m26_probe::run_rumor_001(),
            Some("NPC-DEMO-001") => m26_probe::run_demo_001(),
            // M27 (CR-989281d9): guilds/factions — GUILD-001 (in the suite),
            // FACTION-001 and M27-DEMO-001 (standalone), spec M27 (REQ-L99-060).
            Some("FACTION-001") => m27_probe::run_faction_001(),
            Some("GUILD-001") => m27_probe::run_guild_001(),
            Some("M27-DEMO-001") => m27_probe::run_demo_001(),
            other => {
                eprintln!(
                    "--probe: unknown name {other:?} (supported: VIEW-001, CITY-001, INTERIOR-001, COMMERCE-001, FORMULA-001, WORLD-001, ALL-001, READMODEL-001, SESSION-001, RMB-001, SEED-001, RDB-001, AUDIO-001, TEXTURE-001, TERRAIN-001, CLIMATE-001, NPC-001, TEXT-RSC-001, QUEST-ORIGINAL-001, UI-001, MAP-001, DEMO-001, LOCOMOTION-001, STEALTH-001, STATS-001, SKILLS-001, ITEM-001, EQUIP-001, LOOT-001, COMBAT-001, COMBAT-LOOP-001, DEATH-001, MAGIC-001, EFFECT-001, SPELLMAKER-001, ENCHANT-001, POTION-001, MONSTER-001, ENEMY-AI-001, ENCOUNTER-001, NPC-AI-001, DIALOGUE-001, RUMOR-001, NPC-DEMO-001, FACTION-001, GUILD-001, M27-DEMO-001)"
                );
                2
            }
        };
        // M3 (CR-bbd5db82): with the `window` feature and PROBE-CITY-001 on
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
            // M4 (CR-bbd5db82): same scheme as M3 — with `window` and
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
            // M5 (CR-bbd5db82): same scheme as M3/M4 — with `window` and
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
            // M7 (CR-bbd5db82): same scheme as M3-M5 — with `window` and
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
            // M8 (CR-bbd5db82): same scheme as M3-M7 — with `window` and
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
            // M17 (CR-778b1a85): with `window` and PROBE-UI-001 on PASS the
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

    // M1 (CR-bbd5db82): only with the `window` feature compiled is there a
    // window+render path; without it, the F0 headless behavior is identical.
    // M2 (CR-bbd5db82): the same path enables the first-person camera +
    // grid collisions over the loaded block.
    #[cfg(feature = "window")]
    {
        // M18 (CR-5d88011a): windowed startup opens the INTEGRATED DEMO
        // (textured retail city + terrain + NPCs + UI). `--window` /
        // `--legacy-m2` keep the M2 path (one ARCH3D block) as a flag.
        let legacy_m2 = args
            .iter()
            .any(|arg| arg == "--window" || arg == "--legacy-m2");
        let launched = if legacy_m2 {
            run_windowed(&path_checker)
        } else {
            m18_window::run_windowed_m18(&path_checker)
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
        // M-1 (QA M1, CR-bbd5db82): no `return;` — with the `window` feature
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

/// M1/M2 (CR-bbd5db82): window+render with ONE block of real ARCH3D.BSA,
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
/// `looks_like_data_root` (both capitalization variants). M16 uses it to
/// enumerate the 247 `.QBN` / 244 `.QRC` of the directory.
fn mode_a_arena2_dir() -> Result<std::path::PathBuf, String> {
    let root: std::path::PathBuf = match std::env::var_os("DF_APP_ROOT") {
        Some(value) => std::path::PathBuf::from(value),
        None => {
            let home = std::env::var_os("HOME").ok_or("HOME not set (define $DF_APP_ROOT)")?;
            std::path::PathBuf::from(home).join("Games/Daggerfall/app")
        }
    };
    if !daggerfall_inventory::path_checker::PathChecker::looks_like_data_root(&root) {
        return Err(format!(
            "invalid Mode A data root: {root:?} (expected <root>/ARENA2; override: $DF_APP_ROOT)"
        ));
    }
    // looks_like_data_root tries both capitalization variants (path_checker).
    ["ARENA2", "arena2"]
        .iter()
        .map(|dir| root.join(dir))
        .find(|path| path.is_dir())
        .ok_or_else(|| format!("ARENA2 directory not found under {root:?}"))
}

/// Resolves a file inside the Mode A ARENA2 (see [`mode_a_arena2_dir`]).
/// Source custody (PRE-2 §8 rule 3): M10 uses it for MAPS.BSA /
/// BLOCKS.BSA / ARCH3D.BSA (PROBE-RMB-001 / PROBE-SEED-001); ModeState=A
/// custody is done by the callers via `PathChecker`.
fn mode_a_arena2_path(name: &str) -> Result<std::path::PathBuf, String> {
    let dir = mode_a_arena2_dir()?;
    let path = dir.join(name);
    if path.is_file() {
        Ok(path)
    } else {
        Err(format!("{name} not found under {dir:?}"))
    }
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

/// M3 landmarks (CR-bbd5db82): first `max` convertible records of
/// ARCH3D.BSA (≥1 triangle, in directory order — deterministic,
/// sha-pinned input S1) that are also **solid** (CR-3f9d13a2 §5a). Delegates
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
/// `hh-1-20261020-og-backend-m10-yneg-partial-CR-3f9d13a2.md`). The criterion
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
