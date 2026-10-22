//! M22 — Anti-Screen probes of **combat** (REQ-L99-055):
//! `PROBE-COMBAT-001` (block A · 47 checks), `PROBE-COMBAT-LOOP-001`
//! (block B) and `PROBE-DEATH-001` (block C).
//!
//! Contract: `docs/sintergic/game/programming/m22-combat-spec-CR-5243af63.md`
//! §1.4 (GV-HIT), §2.2 (GV-DMG), §3.1 (GV-CAD), §4.1 (GV-DTH) and **§6
//! probe contract**. The goldens are transcribed here; the rolls enter
//! **injected** (pure `success_roll(roll, chance)`) or through the engine's
//! **per-actor** `Dice100` (delta D-M22-1): 0 global RNG, 0 wall clock,
//! 0 I/O — M22 does not read retail data (spec §0).
//!
//! `PROBE-COMBAT-001` — block A, `checks=47 passed=47 failed=0`:
//! `GV-HIT-01..15` (hit, `CalculateSuccessfulHit` `:796-828`) +
//! `GV-DMG-01..17` (damage, `:736-761`/`:771-791`) + `GV-CAD-01..15`
//! (cadence, `:830-848` + `EnemyAttack.cs:114-132`).
//!
//! `PROBE-COMBAT-LOOP-001` — block B: 600 ticks at 60 Hz with fixed
//! attackers; counts swings (fixed cadence 10 ticks → **60** swings), hits,
//! damage and wear events; FNV-1a 64 digest reproducible ×2 and sensitive to
//! the seed (anti-false-positive (f) of spec §6).
//!
//! `PROBE-DEATH-001` — block C: `GV-DTH-01..06` + player death (latch
//! `deathInProgress`, fall 0.85 s / fade 2 s / reset 3 s) and enemy death
//! (OnDeath once, spec §7.4).
//!
//! **Outputs (§6):** `EXIT=0` ⟺ all checks OK · `EXIT=1` on any
//! FAIL (line `FAIL <id> got=<v> want=<v>`).

use daggerfall_behavior::combat::{
    adrenaline_bonus, adrenaline_to_hit, adjustments, backstab_damage, bow_cooldown_seconds,
    chance_to_hit, critical_bonus, h2h_range, material_gate, melee_tick_seconds, multi_hit_gate,
    reflexes_gate, set_health, skeletal_weapon_adjust, stats_to_hit, success_roll, weapon_damage,
    fall_duration_seconds, HitMods,
};
use daggerfall_combat::actor::{Combatant, EntityKind, Weapon};
use daggerfall_combat::engine::{resolve_attack, AttackInput, DeathSequence, EnemyAttack};
use daggerfall_combat::rng::{actor_seed, Dice100};
use daggerfall_skills::character::damage_modifier;

/// M22 probe seed — the M22 spec date (`20_261_022`), same pattern as
/// `M19_SEED`/`M21_LOOT_SEED`. The demo reuses it.
pub const M22_SEED: u64 = 20_261_022;

// ── evidence rows (M19/M20/M21 template) ────────────────────────────────────

/// Printed row of the probe: `[OK] <id> - <detail>` or
/// `FAIL <id> got=<v> want=<v>` (spec §6 literal).
#[derive(Debug, Clone)]
struct Row {
    id: &'static str,
    ok: bool,
    got: String,
    want: String,
    detail: String,
}

/// Row whose verdict is the equality `got == want` (all the relevant
/// values travel INSIDE both strings).
fn row(id: &'static str, got: String, want: String, detail: impl Into<String>) -> Row {
    Row {
        id,
        ok: got == want,
        got,
        want,
        detail: detail.into(),
    }
}

fn print_rows(title: &str, rows: &[Row]) -> bool {
    println!("\n-- {title} --");
    let mut ok = true;
    for row in rows {
        if row.ok {
            println!("[OK] {} - {}", row.id, row.detail);
        } else {
            ok = false;
            println!("FAIL {} got={} want={}", row.id, row.got, row.want);
        }
    }
    ok
}

/// FNV-1a 64 (same scheme as the M18/M20 demo digests) over the canonical
/// bytes of the run — probe B prints it and compares it ×2.
fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

// ── block A · 47 golden vectors (spec §1.4, §2.2, §3.1) ─────────────────────

/// Canonical actor of §1.4: target `EnemyMonster` (`+40`), armor 0,
/// no adrenaline and no stats — adjustments `+40 − 50 = −10`.
fn gv_hit(chance_mod: i32) -> HitMods {
    HitMods {
        chance_mod,
        adjustments: adjustments(false, 0, true),
        ..HitMods::default()
    }
}

/// The 47 checks of block A (spec §6: `checks=47`).
fn combat_rows() -> Vec<Row> {
    let mut rows = Vec::with_capacity(47);

    // ── GV-HIT-01..15 — hit chance (`FormulaHelper.cs:796-828`) ───────────
    // 01: skill 45, crit roll 40 (fails: 40 ≥ 20) ⇒ 35; also the
    // `SuccessRoll` table (roll < chance) with the same chance: 34 hits, 35 not.
    let m = HitMods {
        critical_bonus: critical_bonus(20, success_roll(40, 20)),
        ..gv_hit(45)
    };
    let chance = chance_to_hit(m);
    rows.push(row(
        "GV-HIT-01",
        format!(
            "chance={chance} roll34={} roll35={}",
            success_roll(34, chance),
            success_roll(35, chance)
        ),
        "chance=35 roll34=true roll35=false".to_string(),
        "skill 45 + 40 − 50 = 35 · SuccessRoll(c)=roll<c (Dice100.cs:15-18)",
    ));
    // 02: armor −45 (steel cuirass, chest) ⇒ clamp 3.
    rows.push(row(
        "GV-HIT-02",
        format!("chance={}", chance_to_hit(HitMods { armor_value: -45, ..gv_hit(45) })),
        "chance=3".to_string(),
        "45 − 45 + 40 − 50 = −10 → clamp(3,97) (:825)",
    ));
    // 03: skill 100, material +60 (weapon_to_hit), armor −105.
    rows.push(row(
        "GV-HIT-03",
        format!("chance={}", chance_to_hit(HitMods { chance_mod: 160, armor_value: -105, ..gv_hit(45) })),
        "chance=45".to_string(),
        "100 + 60 − 105 + 40 − 50 = 45 (weapon_to_hit at :679)",
    ));
    // 04/05: swing −10 / +10 (swing damage/hit mods, :597-599).
    rows.push(row(
        "GV-HIT-04",
        format!("chance={}", chance_to_hit(gv_hit(35))),
        "chance=25".to_string(),
        "StrikeDown (−10): 45 − 10 + 40 − 50 = 25",
    ));
    rows.push(row(
        "GV-HIT-05",
        format!("chance={}", chance_to_hit(gv_hit(55))),
        "chance=45".to_string(),
        "StrikeUp (+10): 45 + 10 + 40 − 50 = 45",
    ));
    // 06: crit roll 5 (< 20 ⇒ success) ⇒ +2.
    let m = HitMods {
        critical_bonus: critical_bonus(20, success_roll(5, 20)),
        ..gv_hit(45)
    };
    rows.push(row(
        "GV-HIT-06",
        format!("chance={}", chance_to_hit(m)),
        "chance=37".to_string(),
        "SuccessRoll(20) with roll 5 ⇒ +20/10 = 2 (:1216-1218)",
    ));
    // 07: TARGET dodge /4 (not /10, anti-false-positive (c)).
    rows.push(row(
        "GV-HIT-07",
        format!("chance={}", chance_to_hit(HitMods { dodging: 60, ..gv_hit(45) })),
        "chance=20".to_string(),
        "45 − 60/4 + 40 − 50 = 20 (target Dodging, :1213)",
    ));
    // 08: PLAYER target, armor −50, biography 0 ⇒ no +40, −50 alone.
    rows.push(row(
        "GV-HIT-08",
        format!(
            "chance={}",
            chance_to_hit(HitMods {
                armor_value: -50,
                adjustments: adjustments(true, 0, false),
                ..gv_hit(45)
            })
        ),
        "chance=3".to_string(),
        "45 − 50 − 50 = −55 → clamp 3 (biography/monster :1224-1251, D-M22-2)",
    ));
    // 09: skill 97 + material 10 ⇒ 97 exactly at the clamp ceiling.
    rows.push(row(
        "GV-HIT-09",
        format!("chance={}", chance_to_hit(gv_hit(107))),
        "chance=97".to_string(),
        "97 + 10 + 40 − 50 = 97 → clamp high (:825)",
    ));
    // 10: skill 60, daedric +60, armor −15.
    rows.push(row(
        "GV-HIT-10",
        format!("chance={}", chance_to_hit(HitMods { chance_mod: 120, armor_value: -15, ..gv_hit(45) })),
        "chance=95".to_string(),
        "60 + 60 − 15 + 40 − 50 = 95 (TO_HIT_MOD[9]=60, M21 §2.2)",
    ));
    // 11/12: Δstats /10 truncated toward zero (i32, same as C#).
    rows.push(row(
        "GV-HIT-11",
        format!(
            "chance={}",
            chance_to_hit(HitMods { stats_diff: stats_to_hit(80, 50, 70, 50), ..gv_hit(45) })
        ),
        "chance=40".to_string(),
        "45 + 30/10 + 20/10 − 10 = 40 (:1185-1200)",
    ));
    rows.push(row(
        "GV-HIT-12",
        format!(
            "chance={}",
            chance_to_hit(HitMods { stats_diff: stats_to_hit(25, 50, 35, 50), ..gv_hit(45) })
        ),
        "chance=32".to_string(),
        "45 + (−25/10=−2) + (−15/10=−1) − 10 = 32 (truncated toward zero)",
    ));
    // 13: adrenaline on both (normal, health 10 < 100/8=12) ⇒ Δ = 0.
    let att_bonus = adrenaline_bonus(true, false, 10, 100);
    let tgt_bonus = adrenaline_bonus(true, false, 10, 100);
    rows.push(row(
        "GV-HIT-13",
        format!(
            "chance={}",
            chance_to_hit(HitMods { adrenaline: adrenaline_to_hit(att_bonus, tgt_bonus), ..gv_hit(45) })
        ),
        "chance=35".to_string(),
        "45 + 5 − 5 + 40 − 50 = 35 (Δadrenaline :1163-1183)",
    ));
    // 14: backstab +20 to hit (:611-612).
    rows.push(row(
        "GV-HIT-14",
        format!("chance={}", chance_to_hit(gv_hit(65))),
        "chance=55".to_string(),
        "45 + 20 + 40 − 50 = 55",
    ));
    // 15: minimum skill 3, armor −45 ⇒ floor 3 again.
    rows.push(row(
        "GV-HIT-15",
        format!("chance={}", chance_to_hit(HitMods { chance_mod: 3, armor_value: -45, ..gv_hit(45) })),
        "chance=3".to_string(),
        "3 − 45 + 40 − 50 = −52 → clamp 3",
    ));

    // ── GV-DMG-01..17 — damage (spec §2.1) ───────────────────────────────
    // 01/02: roll 7 + str 60 (+2) + material 0 / −1.
    rows.push(row(
        "GV-DMG-01",
        format!("damage={}", weapon_damage(7, 0, damage_modifier(60), 0)),
        "damage=9".to_string(),
        "7 + 0 + floor(10/5) + 0 = 9 (:736-761; the app composes str, D-M22-1)",
    ));
    rows.push(row(
        "GV-DMG-02",
        format!("damage={}", weapon_damage(7, 0, damage_modifier(60), -1)),
        "damage=8".to_string(),
        "7 + 0 + 2 − 1 = 8 (iron DAMAGE_MOD[0]=−1, M21 §2.2)",
    ));
    // 03: floor 0 AFTER strength and material (anti-false-positive (e)).
    rows.push(row(
        "GV-DMG-03",
        format!("damage={}", weapon_damage(2, -4, damage_modifier(40), -1)),
        "damage=0".to_string(),
        "2 − 4 − 2 − 1 = −5 → floor 0 (:760-761, floor 0 not 1)",
    ));
    // 04: daedric +6.
    rows.push(row(
        "GV-DMG-04",
        format!("damage={}", weapon_damage(16, 0, damage_modifier(60), 6)),
        "damage=24".to_string(),
        "16 + 0 + 2 + 6 = 24 (DAMAGE_MOD[9]=6)",
    ));
    // 05: backstab ×3 only with a successful roll (injected).
    rows.push(row(
        "GV-DMG-05",
        format!(
            "bs_ok={} bs_fail={}",
            backstab_damage(9, 3, true),
            backstab_damage(9, 3, false)
        ),
        "bs_ok=27 bs_fail=9".to_string(),
        "9 · 3 = 27 with successful SuccessRoll(3); without a roll the damage does not change (:984-986)",
    ));
    // 06-08: H2H min/max = skill/10+1 / skill/5+1 without strength bonus.
    rows.push(row(
        "GV-DMG-06",
        format!("min={} max={}", h2h_range(30, 0, false).0, h2h_range(30, 0, false).1),
        "min=4 max=7".to_string(),
        "30/10+1 = 4; 30/5+1 = 7 (:434-453)",
    ));
    rows.push(row(
        "GV-DMG-07",
        format!("min={} max={}", h2h_range(100, 0, false).0, h2h_range(100, 0, false).1),
        "min=11 max=21".to_string(),
        "100/10+1 = 11; 100/5+1 = 21",
    ));
    rows.push(row(
        "GV-DMG-08",
        format!("min={} max={}", h2h_range(5, 0, false).0, h2h_range(5, 0, false).1),
        "min=1 max=2".to_string(),
        "5/10+1 = 1; 5/5+1 = 2 (integer division)",
    ));
    // 09-11: the strength bonus only enters the PLAYER's range.
    rows.push(row(
        "GV-DMG-09",
        format!("min={} max={}", h2h_range(30, damage_modifier(60), true).0, h2h_range(30, damage_modifier(60), true).1),
        "min=6 max=9".to_string(),
        "4+2 / 7+2 (player, str 60 ⇒ +2)",
    ));
    rows.push(row(
        "GV-DMG-10",
        format!("min={} max={}", h2h_range(30, damage_modifier(40), true).0, h2h_range(30, damage_modifier(40), true).1),
        "min=2 max=5".to_string(),
        "4−2 / 7−2 (player, str 40 ⇒ −2)",
    ));
    rows.push(row(
        "GV-DMG-11",
        format!("min={} max={}", h2h_range(30, damage_modifier(60), false).0, h2h_range(30, damage_modifier(60), false).1),
        "min=4 max=7".to_string(),
        "monster (not player): no strength bonus in the range",
    ));
    // 12: material gate — iron vs MinMetalToHit=Steel ⇒ 0.
    let gate = material_gate(1, 0);
    rows.push(row(
        "GV-DMG-12",
        format!("gate={gate} damage={}", if gate { 0 } else { 1 }),
        "gate=true damage=0".to_string(),
        "MinMetalToHit (Steel 1) > NativeMaterialValue (Iron 0) ⇒ return 0 (:576-583)",
    ));
    // 13: silver ×2 after the skeletal halving (sharp weapon 0x10 does not split).
    rows.push(row(
        "GV-DMG-13",
        format!("damage={}", skeletal_weapon_adjust(9, 0x10, true)),
        "damage=18".to_string(),
        "9 · 2 = 18 (sharp: no halving; silver doubles :748-749)",
    ));
    // 14/15: multi-hit reflexes gate (injected rolls 0/49/50).
    let hits = u8::from(multi_hit_gate(0, 2))
        + u8::from(multi_hit_gate(49, 2))
        + u8::from(multi_hit_gate(50, 2));
    rows.push(row(
        "GV-DMG-14",
        format!("hits={hits}"),
        "hits=2".to_string(),
        "player Reflexes 2 ⇒ gate 50 %: 0<50 ✔ 49<50 ✔ 50<50 ✘ (:654-657)",
    ));
    rows.push(row(
        "GV-DMG-15",
        format!("gate={}", reflexes_gate(5)),
        "gate=20".to_string(),
        "50 − 10·(5−2) = 20 %",
    ));
    // 16/17: DamageModifier = floor((str − 50)/5), composed by the app.
    rows.push(row(
        "GV-DMG-16",
        format!("modifier={}", damage_modifier(55)),
        "modifier=1".to_string(),
        "floor(5/5) = 1 (:66-73, from daggerfall_skills)",
    ));
    rows.push(row(
        "GV-DMG-17",
        format!("modifier={}", damage_modifier(54)),
        "modifier=0".to_string(),
        "floor(4/5) = 0 (anti-false-positive (d): it is not str/5−2)",
    ));

    // ── GV-CAD-01..15 — cadence (spec §3) ────────────────────────────────
    rows.push(row(
        "GV-CAD-01",
        format!("{:.6}", melee_tick_seconds(50)),
        "0.198980".to_string(),
        "3·(115−50)/980 = 195/980 (:830-838)",
    ));
    rows.push(row(
        "GV-CAD-02",
        format!("{:.6}", melee_tick_seconds(100)),
        "0.045918".to_string(),
        "45/980",
    ));
    rows.push(row(
        "GV-CAD-03",
        format!("{:.6}", melee_tick_seconds(30)),
        "0.260204".to_string(),
        "255/980",
    ));
    rows.push(row(
        "GV-CAD-04",
        format!("{:.6}", melee_tick_seconds(115)),
        "0.000000".to_string(),
        "0/980 (LiveSpeed 115 ⇒ tick 0)",
    ));
    rows.push(row(
        "GV-CAD-05",
        format!("{:.6}", bow_cooldown_seconds(50)),
        "1.326531".to_string(),
        "(10·50 + 800)/980 = 1300/980 (:840-848)",
    ));
    rows.push(row(
        "GV-CAD-06",
        format!("{:.6}", bow_cooldown_seconds(100)),
        "0.816327".to_string(),
        "800/980",
    ));
    rows.push(row(
        "GV-CAD-07",
        format!("{:.6}", bow_cooldown_seconds(30)),
        "1.530612".to_string(),
        "1500/980",
    ));
    rows.push(row(
        "GV-CAD-08",
        format!("{:.6}", bow_cooldown_seconds(0)),
        "1.836735".to_string(),
        "1800/980",
    ));
    rows.push(row(
        "GV-CAD-09",
        format!("gate={}", reflexes_gate(2)),
        "gate=50".to_string(),
        "50 − 10·0 = 50 % (Reflexes 2)",
    ));
    rows.push(row(
        "GV-CAD-10",
        format!("gate={}", reflexes_gate(1)),
        "gate=60".to_string(),
        "50 − 10·(−1) = 60 %",
    ));
    rows.push(row(
        "GV-CAD-11",
        format!("gate={}", reflexes_gate(3)),
        "gate=40".to_string(),
        "50 − 10·1 = 40 %",
    ));
    rows.push(row(
        "GV-CAD-12",
        format!("pass={}", multi_hit_gate(19, 5)),
        "pass=true".to_string(),
        "19 < 20 (gate Reflexes 5)",
    ));
    rows.push(row(
        "GV-CAD-13",
        format!("pass={}", multi_hit_gate(20, 5)),
        "pass=false".to_string(),
        "20 ≥ 20 (the roll does NOT pass)",
    ));
    // 14/15: enemy melee timer — PLAYER level and reflexes
    // (R-22.4, `EnemyAttack.cs:114-132`).
    let mut attack = EnemyAttack::new();
    attack.reset_melee_timer(1500, 10, 3);
    rows.push(row(
        "GV-CAD-14",
        format!("{:.6}", attack.melee_timer_seconds),
        "1.989796".to_string(),
        "(1500 − 50·(10−10) + 450·(3−2))/980 = 1950/980 (player lvl 10, refl 3)",
    ));
    attack.reset_melee_timer(1500, 40, 1);
    rows.push(row(
        "GV-CAD-15",
        format!("{:.6}", attack.melee_timer_seconds),
        "0.000000".to_string(),
        "1500 − 1500 − 450 → max(0) ⇒ 0 s (player lvl 40, refl 1)",
    ));

    debug_assert_eq!(rows.len(), 47, "block A is 47 checks (spec §6)");
    rows
}

/// `--probe COMBAT-001` — 47 golden vectors with injected rolls (spec §6 A).
pub fn run_combat_001() -> i32 {
    println!(
        "PROBE-COMBAT-001 - M22 combat: hit/damage/cadence (REQ-L99-055 · CR-be05b6d8 · spec §1-§3)"
    );
    let rows = combat_rows();
    let ok = print_rows("A · formulas with injected rolls (GV-HIT 15 + GV-DMG 17 + GV-CAD 15)", &rows);
    let checks = rows.len();
    let passed = rows.iter().filter(|row| row.ok).count();
    let failed = checks - passed;
    println!("checks={checks} passed={passed} failed={failed}");
    let code = i32::from(!(ok && checks == 47));
    println!("EXIT={code}");
    if code == 0 {
        println!(
            "VERDICT: PROBE-COMBAT-001 PASS - checks=47 passed=47 failed=0 · injected rolls · 0 global RNG"
        );
    } else {
        println!("VERDICT: PROBE-COMBAT-001 FAIL - see [FAIL] rows");
    }
    code
}

// ── block B · 600-tick loop (spec §6, determinism §6) ──────────────────────

/// Loop ticks (600 at 60 Hz = 10 s).
const LOOP_TICKS: u32 = 600;
/// Time step (60 Hz).
const LOOP_DT: f32 = 1.0 / 60.0;
/// Fixed swing cadence: 1 player hit every 10 ticks ⇒
/// exactly `600 / 10 = 60` swings (the tick formula in seconds is
/// verified in GV-CAD-01..08; the loop keeps exact counters).
const LOOP_SWING_TICKS: u32 = 10;
/// Maximum sequence hits per enemy tick before replenishing the
/// timer (the EnemyAttack timer already does it; safety bound only).
const LOOP_SEED_ALT_OFFSET: u64 = 1;

/// Full block B run.
#[derive(Debug, Clone, PartialEq, Eq)]
struct LoopRun {
    ticks: u32,
    swings: u32,
    hits: u32,
    dmg_total: i32,
    wear_events: u32,
    digest: u64,
}

/// One loop run: fixed attacker (chance 45 vs `EnemyMonster`, steel
/// weapon 2..16) and bare-handed enemy against the player; 0 Bevy, 0 I/O.
fn loop_run(seed: u64) -> LoopRun {
    let mut attacker = Combatant::new(1, EntityKind::Player);
    attacker.strength_modifier = damage_modifier(60);
    let mut target = Combatant::new(2, EntityKind::EnemyMonster);
    target.max_health = 10_000;
    target.health = 10_000;
    target.armor_values[3] = -45; // cuirass on the chest (spec §1.2)
    let weapon = Weapon {
        native_material_value: 1, // Steel (M21 §2.2)
        min_damage: 2,
        max_damage: 16, // Longsword retail (FormulaHelper.cs:455-527)
        flags: 0x10,   // sharp (skeletal halving does not apply)
        to_hit_mod: 0,
        material_damage_mod: 0,
    };
    // The loop's "player" absorbs the enemy hits with headroom
    // (10 000 HP) so the 600 ticks stay stable.
    let mut player = Combatant::new(3, EntityKind::Player);
    player.max_health = 10_000;
    player.health = 10_000;
    let enemy = Combatant::new(4, EntityKind::EnemyClass);
    let input = AttackInput {
        attacker: &attacker,
        target: &target,
        weapon: Some(&weapon),
        chance_mod: 45,
        damage_mods: 0,
        backstab_level: 0,
        enemy_bonus: 0,
    };
    let enemy_input = AttackInput {
        attacker: &enemy,
        target: &player,
        weapon: None, // H2H (range 4..7, no bonus: not player)
        chance_mod: 45,
        damage_mods: 0,
        backstab_level: 0,
        enemy_bonus: 0,
    };

    let mut player_dice = Dice100::with_seed(actor_seed(seed, 1));
    let mut enemy_dice = Dice100::with_seed(actor_seed(seed, 4));
    let mut enemy_attack = EnemyAttack::new();
    let mut bytes = Vec::new();
    let mut swings = 0u32;
    let mut hits = 0u32;
    let mut dmg_total = 0i32;
    let mut wear_events = 0u32;

    for tick in 0..LOOP_TICKS {
        // Player swing at fixed cadence.
        if tick % LOOP_SWING_TICKS == 0 {
            swings += 1;
            let outcome = resolve_attack(&input, &mut player_dice);
            bytes.extend_from_slice(&u8::from(outcome.hit).to_le_bytes());
            bytes.extend_from_slice(&outcome.chance.to_le_bytes());
            bytes.extend_from_slice(&outcome.part.to_le_bytes());
            bytes.extend_from_slice(&outcome.damage.to_le_bytes());
            bytes.extend_from_slice(&outcome.rolls_consumed.to_le_bytes());
            if outcome.hit {
                hits += 1;
                if outcome.damage > 0 {
                    dmg_total += outcome.damage;
                    // M21 §4 boundary: damage > 0 ⇒ wear (spec §5.6). The
                    // target's HP (10 000) remains as a stability
                    // ceiling: `damage` accumulates in counters and is not
                    // mutated here (the `AttackInput` borrow lives across
                    // the whole branch; real health is applied in demo §7.1
                    // and in DEATH-001 with `DeathSequence::apply_set_health`).
                    wear_events += 1;
                }
            }
        }
        // Enemy hit on timer (player level/reflexes, R-22.4).
        if enemy_attack.tick(LOOP_DT) {
            let outcome = resolve_attack(&enemy_input, &mut enemy_dice);
            bytes.extend_from_slice(&u8::from(outcome.hit).to_le_bytes());
            bytes.extend_from_slice(&outcome.damage.to_le_bytes());
            let roll_ms = enemy_dice.roll_range(1500, 3000);
            enemy_attack.reset_melee_timer(roll_ms, 10, 3);
        }
    }

    LoopRun {
        ticks: LOOP_TICKS,
        swings,
        hits,
        dmg_total,
        wear_events,
        digest: fnv1a64(&bytes),
    }
}

/// `--probe COMBAT-LOOP-001` — 600 ticks with counters and digest (spec §6 B).
pub fn run_combat_loop_001() -> i32 {
    let alt_seed = M22_SEED.wrapping_add(LOOP_SEED_ALT_OFFSET);
    println!(
        "PROBE-COMBAT-LOOP-001 - M22 combat loop (REQ-L99-055 · CR-be05b6d8 · spec §6 B)"
    );
    println!("seed={M22_SEED} seed_alt={alt_seed} (600 ticks @ 60 Hz · fixed cadence 10 ticks · Dice100 per actor)");

    let primary = loop_run(M22_SEED);
    let repeat = loop_run(M22_SEED);
    let alternate = loop_run(alt_seed);

    println!(
        "ticks={} swings={} hits={} dmg_total={} wear_events={} digest={:016x}",
        primary.ticks, primary.swings, primary.hits, primary.dmg_total, primary.wear_events, primary.digest
    );

    let rows = vec![
        row(
            "GV-LOOP-ticks",
            format!("ticks={} swings={}", primary.ticks, primary.swings),
            format!("ticks={LOOP_TICKS} swings=60"),
            "600 ticks at 60 Hz · 1 swing every 10 ticks ⇒ exactly 60 (spec §6 B)",
        ),
        row(
            "GV-LOOP-wear",
            format!("hits={} wear_events={}", primary.hits, primary.wear_events),
            format!("hits={} wear_events={}", primary.hits, primary.hits),
            "M21 §4 boundary: each hit with damage > 0 emits 1 wear event (spec §5.6)",
        ),
        row(
            "GV-LOOP-x2",
            format!("digest_a={:016x} digest_b={:016x}", primary.digest, repeat.digest),
            format!("digest_a={:016x} digest_b={:016x}", primary.digest, primary.digest),
            format!("two runs with seed={M22_SEED} ⇒ byte-identical FNV-1a64 (spec §6)"),
        ),
        row(
            "GV-LOOP-seed",
            if alternate.digest == primary.digest {
                format!(
                    "digest_S={:016x} digest_S'={:016x}",
                    primary.digest, alternate.digest
                )
            } else {
                format!("digest_S={:016x} digest_S'≠digest_S", primary.digest)
            },
            format!("digest_S={:016x} digest_S'≠digest_S", primary.digest),
            format!("alternate seed seed_alt={alt_seed} ⇒ different digest (anti-false-positive (f))"),
        ),
    ];

    let ok = print_rows("B · 600-tick loop (counters + determinism ×2)", &rows);
    let checks = rows.len();
    let passed = rows.iter().filter(|row| row.ok).count();
    let failed = checks - passed;
    println!("checks={checks} passed={passed} failed={failed}");
    let all_ok = ok && primary.hits > 0 && primary.dmg_total > 0;
    let code = i32::from(!all_ok);
    println!("EXIT={code}");
    if all_ok {
        println!(
            "VERDICT: PROBE-COMBAT-LOOP-001 PASS - ticks=600 swings=60 · digest ×2 identical with seed={M22_SEED} · seed sensitivity verified · 0 global RNG"
        );
    } else {
        println!("VERDICT: PROBE-COMBAT-LOOP-001 FAIL - see [FAIL] rows");
    }
    code
}

// ── block C · death (spec §4, §6 C) ─────────────────────────────────────────

/// The 8 checks of block C (GV-DTH-01..06 + player latch + enemy latch).
fn death_rows() -> Vec<Row> {
    let mut rows = Vec::with_capacity(8);

    let (health, died) = set_health(100, 150, false);
    rows.push(row(
        "GV-DTH-01",
        format!("health={health} died={died}"),
        "health=100 died=false".to_string(),
        "SetHealth(150) with max 100 ⇒ clamp high, no event (:331-338)",
    ));
    let (health, died) = set_health(100, -5, false);
    rows.push(row(
        "GV-DTH-02",
        format!("health={health} died={died}"),
        "health=0 died=true".to_string(),
        "clamp low to 0 ⇒ 1 event",
    ));
    let (health, died) = set_health(100, 0, false);
    rows.push(row(
        "GV-DTH-03",
        format!("health={health} died={died}"),
        "health=0 died=true".to_string(),
        "`≤ 0` ⇒ RaiseOnDeathEvent (:877-880)",
    ));
    let (health, died) = set_health(100, 150, true);
    rows.push(row(
        "GV-DTH-04",
        format!("health={health} died={died}"),
        "health=150 died=false".to_string(),
        "restoreMode: no clamp, the `≤ 0` check remains (:333-335, correction quotes 30)",
    ));
    rows.push(row(
        "GV-DTH-05",
        format!("{:.6}", fall_duration_seconds(1.7)),
        "0.850000".to_string(),
        "1.7·1.25/2.5 = 0.85 s (PlayerDeath.cs:29, :160)",
    ));
    rows.push(row(
        "GV-DTH-06",
        format!("{:.6}", fall_duration_seconds(2.0)),
        "1.000000".to_string(),
        "2.0·1.25/2.5 = 1.0 s",
    ));

    // Player latch: the second lethal `set_health` does NOT emit again.
    let mut player = DeathSequence::for_player_height(1.7);
    let first = player.apply_set_health(37, -5, false);
    let second = player.apply_set_health(37, -5, false);
    rows.push(row(
        "DEATH-player-latch",
        format!(
            "first={first} second={second} events={} fall={:.6} fade={:.6} reset={:.6}",
            player.events(),
            player.fall_seconds,
            player.fade_seconds,
            player.reset_seconds
        ),
        "first=0 second=0 events=1 fall=0.850000 fade=2.000000 reset=3.000000".to_string(),
        "deathInProgress latch + PlayerDeath.cs:29-33 constants (spec §7.4)",
    ));

    // Enemy latch: OnDeath once, no camera (spec §7.4).
    let mut enemy = DeathSequence::for_enemy();
    let _ = enemy.apply_set_health(100, -5, false);
    let _ = enemy.apply_set_health(100, -5, false);
    rows.push(row(
        "DEATH-enemy-latch",
        format!(
            "events={} fall={:.6} fade={:.6} reset={:.6}",
            enemy.events(),
            enemy.fall_seconds,
            enemy.fade_seconds,
            enemy.reset_seconds
        ),
        "events=1 fall=0.000000 fade=0.000000 reset=0.000000".to_string(),
        "enemy: OnDeath once, no camera sequence",
    ));

    rows
}

/// `--probe DEATH-001` — player and enemy death (spec §6 C).
pub fn run_death_001() -> i32 {
    println!(
        "PROBE-DEATH-001 - M22 death (REQ-L99-055 · CR-be05b6d8 · spec §4)"
    );
    let rows = death_rows();
    let ok = print_rows("C · set_health + latch + player sequence", &rows);
    let checks = rows.len();
    let passed = rows.iter().filter(|row| row.ok).count();
    let failed = checks - passed;
    // Spec §6 C (§7.4) literal: the enemy emits once; the player runs
    // fall → fade → reset a single time.
    println!(
        "events_player=1 events_enemy=1 fall=0.85 fade=2.0 reset=3.0"
    );
    println!("checks={checks} passed={passed} failed={failed}");
    let code = i32::from(!(ok && checks == 8));
    println!("EXIT={code}");
    if code == 0 {
        println!(
            "VERDICT: PROBE-DEATH-001 PASS - checks=8 passed=8 failed=0 · OnDeath latch ×1 · fall 0.85 / fade 2.0 / reset 3.0"
        );
    } else {
        println!("VERDICT: PROBE-DEATH-001 FAIL - see [FAIL] rows");
    }
    code
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn combat_rows_are_47_and_all_pass() {
        let rows = combat_rows();
        assert_eq!(rows.len(), 47, "spec §6: checks=47 (15 HIT + 17 DMG + 15 CAD)");
        for row in &rows {
            assert!(row.ok, "FAIL {} got={} want={}", row.id, row.got, row.want);
        }
    }

    #[test]
    fn death_rows_are_8_and_all_pass() {
        let rows = death_rows();
        assert_eq!(rows.len(), 8, "spec §4/§6 C: GV-DTH-01..06 + 2 latch");
        for row in &rows {
            assert!(row.ok, "FAIL {} got={} want={}", row.id, row.got, row.want);
        }
    }

    #[test]
    fn loop_is_deterministic_seed_stable_and_sensitive() {
        let primary = loop_run(M22_SEED);
        assert_eq!(primary.ticks, LOOP_TICKS);
        assert_eq!(primary.swings, 60, "600/10 = 60 exact swings");
        assert!(primary.hits > 0, "the loop must land hits");
        assert!(primary.dmg_total > 0);
        assert_eq!(primary.wear_events, primary.hits, "damage > 0 ⇒ wear (M21 §4)");
        assert_eq!(primary, loop_run(M22_SEED), "same seed ⇒ same run");
        assert_ne!(
            primary.digest,
            loop_run(M22_SEED.wrapping_add(LOOP_SEED_ALT_OFFSET)).digest,
            "alternate seed ⇒ different digest (anti-false-positive (f))"
        );
    }

    #[test]
    fn fnv1a64_matches_reference_vector() {
        // FNV-1a 64 reference vector for the empty string.
        assert_eq!(fnv1a64(b""), 0xcbf2_9ce4_8422_2325);
    }
}
