//! PROBE-WORLD-001 + unified Anti-Screen suite (M6).
//!
//! **PROBE-WORLD-001** — integrated playthrough of the complete QW3 world:
//! spawn on the street (M3) → walk to the door → enter the shop (M4)
//! → interact + buy/sell with gold (M5) → exit to the street →
//! **enter again**. Anti-Screen invariants verified at runtime:
//! 0 clipping in the WHOLE route, door-plane crossings only through the
//! opening, integer gold (NUMERIC-never-FLOAT), 0 persistence. Runs in the
//! **default headless** config (CI-runnable, 0 GPU), same as the M2-M5
//! probes; 0 window of its own (the M5 window already evidenced the interior
//! — M5 precedent, delta documented in the M6 note).
//!
//! **Unified suite** (`--probe ALL-001`): runs ALL the M2-M23 Anti-Screen
//! probes (VIEW-001, CITY-001, INTERIOR-001, COMMERCE-001, FORMULA-001,
//! WORLD-001, READMODEL-001, SESSION-001, RMB-001, SEED-001, RDB-001,
//! AUDIO-001, TEXTURE-001, TERRAIN-001, NPC-001, TEXT-RSC-001, UI-001,
//! MAP-001, DEMO-001, LOCOMOTION-001, STATS-001, ITEM-001, COMBAT-001,
//! COMBAT-LOOP-001, DEATH-001, MAGIC-001, EFFECT-001, MONSTER-001,
//! ENCOUNTER-001) up to the M29 quest probes (EXEC-001, LIFECYCLE-001,
//! JOURNAL-001) plus the three M34 demo probes the spec §11 requires inside
//! `ALL-001` (PLAYSESSION-001, UI-FULL-001, AUDIO-RT-001) — the suite spans
//! **M2-M34**, **40 entries**.
//!
//! R1 moved `COMBAT-LOOP-001`, `DEATH-001`, `EFFECT-001` and
//! `ENCOUNTER-001` from standalone-only into this suite: a green `ALL-001`
//! must cover every probe of the work packages it certifies.
//! from ONE entrypoint
//! with aggregated evidence (summary table + exit 0 only if ALL PASS). Same
//! core as the individual probes — 0 duplicated logic.
//!
//! Composition in `app` (depends on all organs + bridge — the organs do NOT
//! call each other; see bevy-bridge-matrix and bus-only).
//! Source custody: Mode A only (PathChecker), same scheme as M4/M5.

use daggerfall_inventory::commerce::{TradeError, TradeSession};
use daggerfall_parser_bevy_bridge as bridge;
use daggerfall_parser_bevy_bridge::{
    march_axis, CellStatus, CityLayout, CityMesh, MerchantProp, ProbeCheck, ShopMesh,
    DEFAULT_CELL_M, DOOR_Z_MAX, DOOR_Z_MIN, EYE_HEIGHT_M, INTERACT_RANGE_M, PLAYER_RADIUS_M,
};

use crate::m5_probe::{
    dfu_buy_price, dfu_sell_price, merchant_stock, starting_inventory, PIELES_BASE_VALUE,
    STARTING_GOLD,
};

/// Complete PROBE-WORLD-001 report (integrated M3+M4+M5 playthrough).
#[derive(Debug, Clone)]
pub struct WorldProbeReport {
    /// Checks executed, in route order.
    pub checks: Vec<ProbeCheck>,
    /// Global PASS = all checks ok.
    pub pass: bool,
}

/// Executable core of **PROBE-WORLD-001** over an already-built world — the
/// CLI and the tests run the SAME code (Anti-Screen).
///
/// `merchant` is parametric: the anti-false-positive tests run the same core
/// with the tampered NPC and require `!pass`.
pub fn run_world_probe(
    city: &CityMesh,
    layout: &CityLayout,
    shop: &ShopMesh,
    gap: usize,
    merchant: &MerchantProp,
) -> WorldProbeReport {
    let mut checks: Vec<ProbeCheck> = Vec::with_capacity(8);

    // ── QW3 world: M3 city + M4 shop + M5 NPC (0 touch to M1-M5) ────────
    let world = bridge::world_mesh(city, shop);
    let world_m5 = bridge::merchant_world(&world, merchant);
    let grid = bridge::CollisionGrid::from_mesh_spec(&world_m5, DEFAULT_CELL_M);
    let (street_point, street_name) = bridge::street_spawn(layout, gap);
    let chest = merchant.chest();

    // Effective door opening in cells (1 m grain) — same formula as
    // PROBE-INTERIOR-001: the crossings recorded by `march_axis` can only
    // fall in [17,19) m with DOOR_Z_MIN=16.8/DOOR_Z_MAX=19.2.
    let cell = DEFAULT_CELL_M;
    let door_min = ((DOOR_Z_MIN / cell).floor() + 1.0) * cell;
    let door_max = ((DOOR_Z_MAX / cell).floor()) * cell;
    let z_ok = |z: f32| z - PLAYER_RADIUS_M >= door_min && z + PLAYER_RADIUS_M <= door_max;

    // ── W1: street spawn (free eye, outside the interior) ───────────────
    let eye = [
        street_point[0],
        street_point[1] + EYE_HEIGHT_M,
        street_point[2],
    ];
    checks.push(ProbeCheck {
        id: "world_spawn_on_street",
        ok: grid.is_free(eye, PLAYER_RADIUS_M) && !shop.contains_interior(eye),
        detail: format!(
            "{street_name} · eye=({:.2},{:.2},{:.2}) · free_radius_{PLAYER_RADIUS_M}={} · \
             region=interior={} (must be false — the playthrough starts EXTERIOR)",
            eye[0],
            eye[1],
            eye[2],
            grid.is_free(eye, PLAYER_RADIUS_M),
            shop.contains_interior(eye),
        ),
    });

    // ── W2: street → lane → door → interior (5 marches, 1st crossing) ───
    let r1 = march_axis(&world_m5, &grid, eye, 2, -1.0, 6.0);
    let r2 = march_axis(&world_m5, &grid, r1.pos, 0, -1.0, 7.0);
    let r3 = march_axis(&world_m5, &grid, r2.pos, 2, 1.0, 13.0);
    let r4 = march_axis(&world_m5, &grid, r3.pos, 2, 1.0, 18.0);
    let r5 = march_axis(&world_m5, &grid, r4.pos, 0, -1.0, 3.0);
    let stance = r5.pos;
    let enter_crossings = r1.crossings.len() + r2.crossings.len() + r3.crossings.len()
        + r4.crossings.len()
        + r5.crossings.len();
    let enter_violations =
        r1.violations + r2.violations + r3.violations + r4.violations + r5.violations;
    let enter_samples = r1.oracle_samples
        + r2.oracle_samples
        + r3.oracle_samples
        + r4.oracle_samples
        + r5.oracle_samples;

    // ── W2b: the wall WITHOUT a door blocks the entrance (M4 parity) —
    //      without an enclosure (empty shop) this march does NOT collide and
    //      the check fails. ─────────────────────────────────────────────────
    let no_door = march_axis(&world_m5, &grid, r3.pos, 0, -1.0, bridge::SHOP_X_MAX);
    checks.push(ProbeCheck {
        id: "world_wall_blocks_without_door",
        ok: no_door.blocked == Some(CellStatus::Solid)
            && !no_door.reached
            && !shop.contains_interior(no_door.pos)
            && no_door.violations == 0,
        detail: format!(
            "-X march from the lane at z=13.0 toward the interior: blocked at x={:.2} \
             (reason {:?}) · entered={} · violations={} (0 req)",
            no_door.pos[0],
            no_door.blocked,
            shop.contains_interior(no_door.pos),
            no_door.violations,
        ),
    });

    checks.push(ProbeCheck {
        id: "world_enter_shop",
        ok: [r1.reached, r2.reached, r3.reached, r4.reached, r5.reached]
            .iter()
            .all(|r| *r)
            && enter_violations == 0
            && enter_crossings == 1
            && r5.crossings.iter().all(|c| z_ok(c[2]))
            && shop.contains_interior(stance)
            && grid.is_free(stance, PLAYER_RADIUS_M),
        detail: format!(
            "{street_name} → lane x=7 → z=18 → interior x=3 (5 marches of 0.25 m) · \
             targets {reached}/5 · violations={enter_violations} (0 req, geometry oracle) · \
             oracle samples={enter_samples} · door crossings \
             = {enter_crossings} (req 1, crossing z [{cr}] ∈ [{door_min:.1},{door_max:.1}]) · \
             region=interior={inside} · free={free} · pos=({:.2},{:.2},{:.2})",
            stance[0], stance[1], stance[2],
            reached = [r1.reached, r2.reached, r3.reached, r4.reached, r5.reached]
                .iter()
                .filter(|r| **r)
                .count(),
            cr = r5
                .crossings
                .iter()
                .map(|c| format!("{:.2}", c[2]))
                .collect::<Vec<_>>()
                .join(", "),
            inside = shop.contains_interior(stance),
            free = grid.is_free(stance, PLAYER_RADIUS_M),
        ),
    });

    // ── W3: interact (raycast) + BUY — gold deducted + item ─────────────
    let dir_chest = [
        chest[0] - stance[0],
        chest[1] - stance[1],
        chest[2] - stance[2],
    ];
    let hit = bridge::interact_target(&grid, stance, dir_chest, merchant, INTERACT_RANGE_M);
    let mut session = TradeSession::new(merchant_stock(), starting_inventory());
    let gate = session.buy("potion", dfu_buy_price(80));
    let gate_closed = gate == Err(TradeError::DialogueClosed);
    if hit.is_some() {
        session.open_dialogue();
    }
    let gold_before = session.inventory.gold;
    let buy_price = dfu_buy_price(80);
    let bought = session.buy("potion", buy_price);
    let gold_after = session.inventory.gold;
    checks.push(ProbeCheck {
        id: "world_interact_and_buy",
        ok: hit.is_some()
            && gate_closed
            && session.dialogue_open
            && bought == Ok(buy_price)
            && gold_after == gold_before - buy_price
            && session.inventory.count("potion") == 1,
        detail: format!(
            "from the interior → NPC chest at {:.2} m (range {INTERACT_RANGE_M}) · target={:?} · \
             buy with dialogue closed → {gate:?} (req DialogueClosed) · valid interaction → \
             dialogue={} · gold {gold_before} → {gold_after} (DFU buy price = {buy_price}) · \
             potions 0 → {}",
            hit.map(|h| h.dist).unwrap_or(-1.0),
            hit.map(|h| h.cell),
            session.dialogue_open,
            session.inventory.count("potion"),
        ),
    });

    // ── W4: SELL — gold credited + good removed (full trade cycle) ──────
    let sell_price = dfu_sell_price(&PIELES_BASE_VALUE);
    let gold_mid = session.inventory.gold;
    let sold = session.sell("furs", sell_price);
    let gold_end = session.inventory.gold;
    checks.push(ProbeCheck {
        id: "world_sell_completes_cycle",
        ok: sold == Ok(sell_price)
            && gold_end == gold_mid + sell_price
            && session.inventory.count("furs") == 0
            && gold_end == STARTING_GOLD - buy_price + sell_price,
        detail: format!(
            "DFU sell price = {sell_price} · sell → {sold:?} · gold {gold_mid} → {gold_end} \
             (cycle: {STARTING_GOLD} −{buy_price} +{sell_price} = {gold_end}, integers — \
             NUMERIC-never-FLOAT ✓) · furs 1 → 0 · 0 persistence (QW3)",
        ),
    });

    // ── W5: exit to the street (2nd crossing through the door) ──────────
    let out = march_axis(&world_m5, &grid, stance, 0, 1.0, 7.0);
    checks.push(ProbeCheck {
        id: "world_exit_to_street",
        ok: out.reached
            && out.violations == 0
            && !shop.contains_interior(out.pos)
            && out.crossings.len() == 1
            && out.crossings.iter().all(|c| z_ok(c[2]))
            && grid.is_free(out.pos, PLAYER_RADIUS_M),
        detail: format!(
            "+X from the interior {s}p → x={x:.2} · region=interior={inside} (must be false) · \
             door crossings = {count} (req 1, z ∈ [{door_min:.1},{door_max:.1}]: {zs}) · \
             violations={viol} (0 req)",
            s = out.steps,
            x = out.pos[0],
            inside = shop.contains_interior(out.pos),
            count = out.crossings.len(),
            zs = out
                .crossings
                .iter()
                .map(|c| format!("{:.2}", c[2]))
                .collect::<Vec<_>>()
                .join(", "),
            viol = out.violations,
        ),
    });

    // ── W6: RETURN — re-entry through the same door (3rd crossing) ──────
    let back = march_axis(&world_m5, &grid, out.pos, 0, -1.0, 3.0);
    checks.push(ProbeCheck {
        id: "world_reenter_shop",
        ok: back.reached
            && back.violations == 0
            && shop.contains_interior(back.pos)
            && back.crossings.len() == 1
            && back.crossings.iter().all(|c| z_ok(c[2]))
            && grid.is_free(back.pos, PLAYER_RADIUS_M),
        detail: format!(
            "-X from the street {s}p → x={x:.2} (enter again) · region=interior={inside} \
             (req true) · door crossings = {count} (req 1, z ∈ [{door_min:.1},{door_max:.1}]: {zs}) · \
             violations={viol} (0 req)",
            s = back.steps,
            x = back.pos[0],
            inside = shop.contains_interior(back.pos),
            count = back.crossings.len(),
            zs = back
                .crossings
                .iter()
                .map(|c| format!("{:.2}", c[2]))
                .collect::<Vec<_>>()
                .join(", "),
            viol = back.violations,
        ),
    });

    // ── W7: global playthrough invariant (full route) ───────────────────
    let total_crossings = enter_crossings + out.crossings.len() + back.crossings.len();
    let total_violations = enter_violations + out.violations + back.violations;
    let total_samples = enter_samples + out.oracle_samples + back.oracle_samples;
    checks.push(ProbeCheck {
        id: "world_playthrough_invariant",
        ok: total_violations == 0
            && total_crossings == 3
            && z_ok(r5.crossings[0][2])
            && z_ok(out.crossings[0][2])
            && z_ok(back.crossings[0][2])
            && grid.is_free(back.pos, PLAYER_RADIUS_M),
        detail: format!(
            "full cycle spawn→enter→trade→exit→return · TOTAL violations={total_violations} \
             (0 req, geometry oracle) · oracle samples={total_samples} · crossings of the plane \
             x={plane} = {total_crossings} (req 3: enter/exit/re-entry, \
             all through the opening z∈[{door_min:.1},{door_max:.1}]) · final pos=({:.2},{:.2},{:.2}) free",
            back.pos[0], back.pos[1], back.pos[2],
            plane = bridge::SHOP_X_MAX,
        ),
    });

    let pass = checks.iter().all(|c| c.ok);
    WorldProbeReport { checks, pass }
}

/// Resolves the Mode A retail route (PathChecker) → ARCH3D.BSA bytes.
/// Source custody (PRE-2 §8 rule 3): Mode A only — same scheme as the M4/M5
/// dispatches (0 policy duplication, reading is duplicated here because this
/// is the only M6 caller).
fn load_retail_world() -> Result<Vec<u8>, String> {
    let path_checker = daggerfall_inventory::path_checker::PathChecker::new();
    if path_checker.use_custom_universe() {
        return Err("custom universe (PathChecker) not supported in this probe".into());
    }
    let path = crate::mode_a_arch3d_path()?;
    println!("[OK] modo_a_arch3d_path: {}", path.display());
    std::fs::read(&path).map_err(|err| format!("ARCH3D.BSA read: {err}"))
}

/// CLI dispatch of PROBE-WORLD-001 (Mode A retail, M4/M5 scheme).
pub fn run_world_001() -> i32 {
    println!("PROBE-WORLD-001 - full QW3 playthrough (INV-L99-10, M6, CR-bbd5db82)");
    let data = match load_retail_world() {
        Ok(data) => data,
        Err(reason) => {
            println!("[FAIL] modo_a_arch3d_path: {reason}");
            return 1;
        }
    };
    let specs = match crate::convertible_blocks(&data, 6) {
        Ok(specs) => specs,
        Err(err) => {
            println!("[FAIL] convertible_blocks: {err}");
            return 1;
        }
    };

    let layout = CityLayout::new(3);
    let city = bridge::build_city_mesh(&layout, &specs);
    let shop = bridge::build_shop_mesh();
    let merchant = bridge::shop_merchant();
    println!(
        "[OK] QW3 world: 3x3 city ({} triangles) + shop ({} triangles) + NPC ({:?})",
        city.triangle_count(),
        shop.triangle_count(),
        merchant.min,
    );

    let report = run_world_probe(&city, &layout, &shop, 1, &merchant);
    for check in &report.checks {
        let status = if check.ok { "OK" } else { "FAIL" };
        println!("[{status}] {} - {}", check.id, check.detail);
    }

    let passed = report.checks.iter().filter(|c| c.ok).count();
    let total = report.checks.len();
    if report.pass {
        // The summary derives the gold path from the price functions the probe
        // charges (two-stage DFU price), so a price change cannot leave a stale
        // literal behind.
        let buy = dfu_buy_price(80);
        let end = 100 - buy + dfu_sell_price(&PIELES_BASE_VALUE);
        println!(
            "M6 VE: integrated playthrough — {passed}/{total} checks · street spawn → shop → \
             trade (gold 100→{} on buy, furs sale → {end}) → exit → return · 0 clipping · \
             3 crossings through the door opening · Anti-Screen: executable runtime invariant",
            100 - buy
        );
        println!(
            "VERDICT: PROBE-WORLD-001 PASS - exterior→interior→commerce→exit→return cycle without crash"
        );
        0
    } else {
        let failed: Vec<&str> = report
            .checks
            .iter()
            .filter(|c| !c.ok)
            .map(|c| c.id)
            .collect();
        println!("VERDICT: PROBE-WORLD-001 FAIL - failed checks: {}", failed.join(", "));
        1
    }
}

/// Suite entry: (name, description, dispatch).
type SuiteEntry = (&'static str, &'static str, fn() -> i32);

/// Unified Anti-Screen suite (`--probe ALL-001`): runs ALL the M2-M29 probes
/// in order with aggregated evidence (final summary table) — exit 0 only if
/// ALL PASS. Same individual dispatches (0 duplicated logic); command
/// documented in the M6 note + VSIM §4.
pub fn run_probe_all() -> i32 {
    println!(
        "UNIFIED ANTI-SCREEN SUITE - M2-M34 probes (CR-bbd5db82 + M19 CR-b109bfe3 + M20 CR-79173139 + M21 CR-776ad8a5 + M22 CR-be05b6d8 + M23 CR-a0b60b1c + M24 CR-f26e353c + M25 CR-23c1381c + M26 CR-94dcfe15 + M27 CR-989281d9 + M28 CR-5243af63 + M29 CR-73d23384 + M34 CR-5243af63)"
    );
    println!("================================================================");
    // 40 entries: the 33 pre-R1 probes, the four that were standalone-only
    // although they belong to the WPs R1 touched (COMBAT-LOOP-001, DEATH-001,
    // EFFECT-001, ENCOUNTER-001) and the three M34 demo probes that the M34
    // spec §11 requires inside `ALL-001` (PLAYSESSION-001, UI-FULL-001,
    // AUDIO-RT-001) — a green suite must cover every probe it certifies.
    let suite: [SuiteEntry; 40] = [
        ("VIEW-001", "M2 · 3D viewer + INV-L99-5", crate::m2_probe::run_view_001),
        ("CITY-001", "M3 · 3D street city", crate::m3_probe::run_city_001),
        (
            "INTERIOR-001",
            "M4 · interior + time system",
            crate::m4_probe::run_interior_001,
        ),
        (
            "COMMERCE-001",
            "M5 · interactive commerce",
            crate::m5_probe::run_commerce_001,
        ),
        (
            "FORMULA-001",
            "M5 · DFU pricing formula",
            crate::m5_probe::run_formula_001,
        ),
        (
            "WORLD-001",
            "M6 · full playthrough",
            run_world_001,
        ),
        (
            "READMODEL-001",
            "M7 · session read models (PD4 T7)",
            crate::m7_probe::run_readmodel_001,
        ),
        (
            "SESSION-001",
            "M8 · session save/load roundtrip (INV-L99-4)",
            crate::m8_probe::run_session_001,
        ),
        (
            "RMB-001",
            "M10 · retail .RMB city (REQ-L99-042)",
            crate::m10_probe::run_rmb_001,
        ),
        (
            "SEED-001",
            "M10 · retail city determinism (REQ-L99-023)",
            crate::m10_probe::run_seed_001,
        ),
        (
            "RDB-001",
            "M11 · retail .RDB interior (REQ-L99-043)",
            crate::m11_probe::run_rdb_001,
        ),
        (
            "AUDIO-001",
            "M14 · MIDI.BSA audio + 375/375 SoundClips goldens + production playback (REQ-L99-045/046)",
            crate::m14_probe::run_audio_001,
        ),
        (
            "TEXTURE-001",
            "M12 · retail textures (REQ-L99-007/005/010/022/011/026)",
            crate::m12_probe::run_texture_001,
        ),
        (
            "TERRAIN-001",
            "M13 · retail terrain/biomes (REQ-L99-044)",
            crate::m13_probe::run_terrain_001,
        ),
        (
            "NPC-001",
            "M15 · NPC population + retail captions (REQ-L99-047)",
            crate::m15_probe::run_npc_001,
        ),
        (
            "TEXT-RSC-001",
            "M16 · retail text + quests (REQ-L99-048/049)",
            crate::m16_probe::run_text_rsc_001,
        ),
        (
            "UI-001",
            "M17 · 1996 UI aesthetics: IMG/CIF/FNT + panels (REQ-L99-050)",
            crate::m17_probe::run_ui_001,
        ),
        (
            "MAP-001",
            "M18 · full MAPS.BSA index: 45 regions with locations (62 slots) / 15,251 locations (REQ-L99-051)",
            crate::m18_probe::run_map_001,
        ),
        (
            "DEMO-001",
            "M18 · integrated demo: textured retail city + terrain + NPC + UI (CR-5d88011a)",
            crate::m18_demo::run_demo_001,
        ),
        (
            "LOCOMOTION-001",
            "M19 · DFU locomotion: speeds/jump/fall/climb/levitate/swim + stealth (REQ-L99-052)",
            crate::m19_probe::run_locomotion_001,
        ),
        (
            "STATS-001",
            "M20 · character: class/attributes/derived/biography/progression (REQ-L99-053)",
            crate::m20_probe::run_stats_001,
        ),
        (
            "ITEM-001",
            "M21 · items/equipment/durability/loot (REQ-L99-054)",
            crate::m21_probe::run_item_001,
        ),
        (
            "COMBAT-001",
            "M22 · combat: hit/damage/cadence/armour-from-equipment (REQ-L99-055)",
            crate::m22_probe::run_combat_001,
        ),
        (
            "COMBAT-LOOP-001",
            "M22 · combat loop: 60 s / 600 ticks / wear from equipment (REQ-L99-055)",
            crate::m22_probe::run_combat_loop_001,
        ),
        (
            "DEATH-001",
            "M22 · death: OnDeath latch + fall/fade/reset (REQ-L99-055)",
            crate::m22_probe::run_death_001,
        ),
        (
            "MAGIC-001",
            "M23 · magic: SPELLS.STD + cost + casting (REQ-L99-056)",
            crate::m23_probe::run_magic_001,
        ),
        (
            "EFFECT-001",
            "M23 · effects: 165/165 keys (153 DFU templates) applied + expiry + movement flags (REQ-L99-056)",
            crate::m23_probe::run_effect_001,
        ),
        (
            "ENCHANT-001",
            "M24 · enchanting: MAGIC.DEF + budget + cost (REQ-L99-057)",
            crate::m24_probe::run_enchant_001,
        ),
        (
            "MONSTER-001",
            "M25 · monsters: MONSTER.BSA + 43 CFG + class HP (REQ-L99-058)",
            crate::m25_probe::run_monster_001,
        ),
        (
            "ENCOUNTER-001",
            "M25 · encounters: 45×20 true MobileTypes + humanoid resolution (REQ-L99-058)",
            crate::m25_probe::run_encounter_001,
        ),
        (
            "NPC-AI-001",
            "M26 · living NPCs: population + navgrid + routine + dialogue + rumors (REQ-L99-059)",
            crate::m26_probe::run_npc_ai_001,
        ),
        (
            "GUILD-001",
            "M27 · guilds: rank + services + reputation propagation (REQ-L99-060)",
            crate::m27_probe::run_guild_001,
        ),
        (
            "CRIME-001",
            "M28 · crime: court table + legal reputation + triggers (REQ-L99-061)",
            crate::m28_probe::run_crime_001,
        ),
        (
            "LAW-001",
            "M28 · law: crime -> guards -> court -> prison -> release (REQ-L99-061)",
            crate::m28_probe::run_law_001,
        ),
        (
            "EXEC-001",
            "M29 · quests: opcodes + tasks + clocks + census + coverage (REQ-L99-062)",
            crate::m29_probe::run_quest_exec_001,
        ),
        (
            "LIFECYCLE-001",
            "M29 · quest lifecycle: offer -> accept -> end -> reputation -> save/load (REQ-L99-062)",
            crate::m29_probe::run_quest_lifecycle_001,
        ),
        (
            "JOURNAL-001",
            "M29 · journal: log/note -> notebook -> missing message -> reopen (REQ-L99-062)",
            crate::m29_probe::run_quest_journal_001,
        ),
        (
            "PLAYSESSION-001",
            "M34 · demo: S01-S12 in one session, 12/12 scenarios = 216000 steps, digest ×2 (spec §11, REQ-L99-067)",
            crate::playsession_probe::run_playsession_001,
        ),
        (
            "UI-FULL-001",
            "M34 · 85-window census (16+11+7+51) + 85/85 shell + HUD GV-03 + 51/51 playable, digests ×2 (spec §11, REQ-L99-067)",
            crate::m34_ui_probe::run_ui_full_001,
        ),
        (
            "AUDIO-RT-001",
            "M34 · 132/132 songs + 459 SND records + 60/60 triggers + real device or declared SKIP-DEVICE (spec §11, REQ-L99-067)",
            crate::m34_audio_probe::run_audio_rt_001,
        ),
    ];
    let mut results: Vec<(&str, &str, i32)> = Vec::with_capacity(suite.len());
    for (name, desc, run) in suite {
        println!("\n----------------------------------------------------------------");
        let code = run();
        results.push((name, desc, code));
    }
    println!("\n================================================================");
    println!("AGGREGATED EVIDENCE - real exit codes:");
    for (name, desc, code) in &results {
        let verdict = if *code == 0 { "PASS" } else { "FAIL" };
        println!("  [{verdict}] PROBE-{name} (exit {code}) — {desc}");
    }
    let failed: Vec<&str> = results
        .iter()
        .filter(|(_, _, code)| *code != 0)
        .map(|(name, _, _)| *name)
        .collect();
    let pass_count = results.len() - failed.len();
    if failed.is_empty() {
        println!(
            "VERDICT: ANTI-SCREEN SUITE PASS ({pass_count}/{} probes, exit 0)",
            results.len()
        );
        0
    } else {
        println!(
            "VERDICT: ANTI-SCREEN SUITE FAIL ({pass_count}/{} probes) - failed: {}",
            results.len(),
            failed.join(", ")
        );
        1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Synthetic 0 I/O world (same as the M4/M5 tests): full PASS with the
    /// default NPC.
    #[test]
    fn probe_world_001_passes_on_synthetic_world() {
        let (layout, city, shop) = bridge::synthetic_world(3);
        let report = run_world_probe(&city, &layout, &shop, 1, &bridge::shop_merchant());
        for check in &report.checks {
            eprintln!(
                "[{}] {} — {}",
                if check.ok { "PASS" } else { "FAIL" },
                check.id,
                check.detail
            );
        }
        eprintln!(
            "VERDICT: PROBE-WORLD-001 {} (synthetic, CR-bbd5db82)",
            if report.pass { "PASS" } else { "FAIL" }
        );
        for check in &report.checks {
            assert!(check.ok, "check {} FAIL: {}", check.id, check.detail);
        }
        assert!(report.pass, "PROBE-WORLD-001 FAIL: {:?}", report.checks);
        assert_eq!(report.checks.len(), 8, "8 stable W1-W7 checks (+W2b wall)");
    }

    /// Determinism: same input → same report.
    #[test]
    fn probe_world_001_is_deterministic() {
        let (layout, city, shop) = bridge::synthetic_world(3);
        let a = run_world_probe(&city, &layout, &shop, 1, &bridge::shop_merchant());
        let b = run_world_probe(&city, &layout, &shop, 1, &bridge::shop_merchant());
        assert_eq!(a.pass, b.pass);
        assert_eq!(
            a.checks
                .iter()
                .map(|c| (c.id, c.ok, c.detail.as_str()))
                .collect::<Vec<_>>(),
            b.checks
                .iter()
                .map(|c| (c.id, c.ok, c.detail.as_str()))
                .collect::<Vec<_>>(),
            "same input → same report (determinism)"
        );
    }

    /// Anti-false-positive: the same core with the NPC **outside the interior**
    /// (tamper) STOPS passing — the probe does not PASS by construction.
    #[test]
    fn probe_world_001_fails_without_merchant_on_spot() {
        let (layout, city, shop) = bridge::synthetic_world(3);
        let tampered = MerchantProp {
            min: [6.5, 0.0, 20.0],
            size: [1.0, 1.9, 1.0],
        };
        let report = run_world_probe(&city, &layout, &shop, 1, &tampered);
        assert!(!report.pass, "tampered NPC CANNOT give PASS");
        let failed: Vec<&str> = report
            .checks
            .iter()
            .filter(|c| !c.ok)
            .map(|c| c.id)
            .collect();
        assert!(
            failed.contains(&"world_interact_and_buy"),
            "the check that must break is world_interact_and_buy (interaction with the NPC); failed: {failed:?}"
        );
        eprintln!("anti-false-positive: failed checks with tampered NPC = {failed:?}");
    }

    /// Inherited M4 anti-false-positive: without a shop (world = city only)
    /// the playthrough CANNOT PASS (no interior and no door).
    #[test]
    fn probe_world_001_fails_without_shop() {
        let layout = CityLayout::new(3);
        let city = bridge::build_city_mesh(&layout, &[]);
        let empty_shop = ShopMesh {
            shell: bridge::MeshSpec {
                positions: Vec::new(),
                normals: Vec::new(),
                indices: Vec::new(),
                uvs: Vec::new(),
                face_textures: Vec::new(),
            },
            floor: bridge::MeshSpec {
                positions: Vec::new(),
                normals: Vec::new(),
                indices: Vec::new(),
                uvs: Vec::new(),
                face_textures: Vec::new(),
            },
            fixtures: bridge::MeshSpec {
                positions: Vec::new(),
                normals: Vec::new(),
                indices: Vec::new(),
                uvs: Vec::new(),
                face_textures: Vec::new(),
            },
            combined: bridge::MeshSpec {
                positions: Vec::new(),
                normals: Vec::new(),
                indices: Vec::new(),
                uvs: Vec::new(),
                face_textures: Vec::new(),
            },
        };
        let report = run_world_probe(&city, &layout, &empty_shop, 1, &bridge::shop_merchant());
        assert!(!report.pass, "world without a shop cannot PASS");
        assert!(
            report.checks.iter().any(|c| !c.ok),
            "at least one check must fail"
        );
    }
}
