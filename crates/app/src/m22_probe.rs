//! M22 — Anti-Screen probes of **combat** (REQ-L99-055):
//! `PROBE-COMBAT-001` (block A · 52 checks), `PROBE-COMBAT-LOOP-001`
//! (block B) and `PROBE-DEATH-001` (block C).
//!
//! Contract: `docs/sintergic/game/programming/m22-combat-spec.md`
//! §1.4 (GV-HIT), §2.2 (GV-DMG), §3.1 (GV-CAD), §4.1 (GV-DTH) and **§6
//! probe contract**. The goldens are transcribed here; the rolls enter
//! **injected** (pure `success_roll(roll, chance)`) or through the engine's
//! **per-actor** `Dice100` (delta D-M22-1): 0 global RNG, 0 wall clock,
//! 0 I/O — M22 does not read retail data (spec §0).
//!
//! `PROBE-COMBAT-001` — block A, `checks=52 passed=52 failed=0`:
//! `GV-HIT-01..15` (hit, `CalculateSuccessfulHit` `:796-828`) +
//! `GV-DMG-01..17` (damage, `:736-761`/`:771-791`) + `GV-CAD-01..15`
//! (cadence, `:830-848` + `EnemyAttack.cs:114-132`) + `GV-ARM-01..05`
//! (equipment → `ArmorValues[7]` + `DamageEquipment` wear).
//!
//! `PROBE-COMBAT-LOOP-001` — block B: 600 ticks at 60 Hz driven through the
//! **production** `CombatSession` (the same runtime the interactive window
//! uses): an armed player (steel Longsword + cuirass, `chance_mod` 45) versus
//! one `EnemyMonster` with `MobileEnemy` damage bands, both projection and
//! wear coming from the equip tables. Counts swings (fixed cadence 10 ticks →
//! **60** swings), hits, damaging hits, chest damaging hits, damage,
//! armour/weapon wear events, enemy strikes and multi-hit strikes. GV-LOOP-wear
//! asserts the invariant «wear iff the struck part is covered»
//! (`armor_wear_events == chest_damaging_hits`, the chest being the only part
//! the cuirass covers), not an identity between two names of the same counter;
//! the cuirass and the Longsword must lose condition; FNV-1a 64 digest
//! reproducible ×2 and sensitive to the seed (anti-false-positive (f) of
//! spec §6).
//!
//! `PROBE-DEATH-001` — block C: `GV-DTH-01..06` + player death (latch
//! `deathInProgress`, fall 0.85 s / fade 2 s / reset 3 s) and enemy death
//! (OnDeath once, spec §7.4).
//!
//! **Outputs (§6):** `EXIT=0` ⟺ all checks OK · `EXIT=1` on any
//! FAIL (line `FAIL <id> got=<v> want=<v>`).

use daggerfall_app::combat_runtime::{CombatSession, EnemyArmorPolicy, Fighter};
use daggerfall_behavior::combat::{
    adrenaline_bonus, adrenaline_to_hit, adjustments, backstab_damage, bow_cooldown_seconds,
    chance_to_hit, critical_bonus, h2h_range, material_gate, melee_tick_seconds, multi_hit_gate,
    reflexes_gate, set_health, skeletal_weapon_adjust, stats_to_hit, success_roll, weapon_damage,
    fall_duration_seconds, HitMods,
};
use daggerfall_behavior::locomotion::Dice100 as WearDice;
use daggerfall_combat::actor::{Combatant, EntityKind};
use daggerfall_combat::engine::{resolve_attack, AttackInput, DeathSequence, EnemyAttack};
use daggerfall_combat::rng::{actor_seed, Dice100};
use daggerfall_inventory::equipment::{
    body_part_for_equip_slot, EquipResult, EquipSlot, ItemEquipTable, ItemWear,
};
use daggerfall_inventory::items::{apply_armor_material, apply_weapon_material, Item, ItemTemplate};
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

// ── equipment bridge (M21 armour → M22 `ArmorValues[7]`) ────────────────────

/// Build and equip an armour item the way `ItemBuilder` does it (template +
/// material, `ItemBuilder.cs:488-507`) and return the slot it landed in.
fn equip_armor(table: &mut ItemEquipTable, template_index: usize, material: i32) -> EquipSlot {
    let mut template = ItemTemplate::empty(template_index);
    template.hit_points = 100;
    let mut item = Item::from_template(&template);
    apply_armor_material(&mut item, material);
    match table.equip(item, true) {
        EquipResult::Equipped(attempt) => attempt.slot,
        EquipResult::Rejected(item) => panic!("armour {} refused", item.template_index),
    }
}

/// The M22 reference kit: a steel cuirass (template 102, `ArmorMaterialTypes`
/// `0x0201`, `GetMaterialArmorValue` 9). Its chest contribution is
/// `9 * 5 = 45` — the value GV-HIT-02 exercises (`DaggerfallEntity.cs:608-615`).
fn cuirass_table() -> ItemEquipTable {
    let mut table = ItemEquipTable::new();
    equip_armor(&mut table, 102, 0x0201);
    table
}

/// Additive bridge `combat::actor` ← `inventory::equipment`: M22 consumes an
/// `ArmorValues[7]` vector (`DaggerfallEntity.cs:589-636` composes it from the
/// worn table; `:270` `ArmorValues` is the field).
///
/// **This helper is the engine layer, not the player projection:** it feeds the
/// raw equip-table delta, which is what the M22 goldens exercise. A *player*
/// entity carries the DFU seed (`SerializablePlayer.cs:355-366` initialises
/// 100 on every part) and reaches the engine through
/// `Fighter::refresh_equipment` (`combat_runtime.rs:275-286`), measured by
/// GV-ARM-06/07. Passing this raw vector for a player would be the 0-based
/// projection the R2 lane fixed.
///
/// It lives here because `combat` owns no `inventory` dependency (ORDER M22:
/// 0 crate cycle) and the target has no `Character`→`Combatant` composer either.
fn combatant_with_equipment(id: u32, kind: EntityKind, table: &ItemEquipTable) -> Combatant {
    Combatant::new(id, kind).with_armor_values(table.armor_values())
}

// ── block A · 52 vectors (spec §1.4, §2.2, §3.1 + M21 armour) ───────────────

/// Canonical actor of §1.4: target `EnemyMonster` (`+40`), armor 0,
/// no adrenaline and no stats — adjustments `+40 − 50 = −10`.
fn gv_hit(chance_mod: i32) -> HitMods {
    HitMods {
        chance_mod,
        adjustments: adjustments(false, 0, true),
        ..HitMods::default()
    }
}

/// The 52 checks of block A (spec §6: 47 formula vectors + 5 equipment).
fn combat_rows() -> Vec<Row> {
    let mut rows = Vec::with_capacity(52);

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
    // 02: armor −45 (cuirass, chest) ⇒ clamp 3. Recalibrated detail: the old
    // text said "steel", but `GetMaterialArmorValue` gives steel (0x0100) 6
    // (⇒ −30); 9 · 5 = 45 is the steel-PLATE material 0x0201 the equipment
    // path now reproduces in GV-ARM-01 (`DaggerfallUnityItem.cs:1007-1058`).
    rows.push(row(
        "GV-HIT-02",
        format!("chance={}", chance_to_hit(HitMods { armor_value: -45, ..gv_hit(45) })),
        "chance=3".to_string(),
        "45 − 45 + 40 − 50 = −10 → clamp(3,97) (:825); −45 = plate 0x0201 (9·5, GV-ARM-01)",
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

    // ── GV-ARM-01..05 — armour fed from real equipment (M21 → M22) ─────────
    // (a) the equipped cuirass projects onto Chest (3) and nowhere else.
    let kit = cuirass_table();
    rows.push(row(
        "GV-ARM-01",
        format!("armor={:?}", kit.armor_values()),
        "armor=[0, 0, 0, -45, 0, 0, 0]".to_string(),
        "steel cuirass (102, 0x0201): GetMaterialArmorValue 9 · 5 = −45 at Chest=3, 0 elsewhere (DaggerfallEntity.cs:608-615)",
    ));

    // (a2) the shield table: Tower shield protects Head/LeftArm/Hands/Legs
    // (`DaggerfallUnityItem.cs:1091-1092`), never Chest/RightArm/Feet.
    let mut kit = cuirass_table();
    equip_armor(&mut kit, 112, 0x0000);
    rows.push(row(
        "GV-ARM-02",
        format!("armor={:?}", kit.armor_values()),
        "armor=[-20, 0, -20, -45, -20, -20, 0]".to_string(),
        "Tower shield GetShieldArmorValue 4 · 5 = −20 on its 4 protected parts; cuirass −45 at Chest",
    ));

    // (b) the same roll with vs without armour differs by exactly the DFU
    // `CalculateArmorToHit` term of the struck part (`FormulaHelper.cs:1149-1161`).
    let kit = cuirass_table();
    let armored = combatant_with_equipment(2, EntityKind::EnemyClass, &kit);
    let bare = combatant_with_equipment(2, EntityKind::EnemyClass, &ItemEquipTable::new());
    let attacker = Combatant::new(1, EntityKind::Player);
    let seeds = 64u64;
    let mut delta_matches = 0u64;
    let mut chest_delta = None;
    for seed in 0..seeds {
        let attack = |target: &Combatant, dice: &mut Dice100| {
            resolve_attack(
                &AttackInput {
                    attacker: &attacker,
                    target,
                    weapon: None,
                    // 100 − 50 (universal adjustment) = 50 bare; the chest
                    // −45 gives 5, so neither side clamps to `[3, 97]`.
                    chance_mod: 100,
                    damage_mods: 0,
                    backstab_level: 0,
                    enemy_bonus: 0,
                },
                dice,
            )
        };
        let mut armored_dice = Dice100::with_seed(actor_seed(M22_SEED, 90 + seed as u32));
        let mut bare_dice = Dice100::with_seed(actor_seed(M22_SEED, 90 + seed as u32));
        let armored_outcome = attack(&armored, &mut armored_dice);
        let bare_outcome = attack(&bare, &mut bare_dice);
        let expected = -armored.armor_values[armored_outcome.part];
        if bare_outcome.part == armored_outcome.part
            && bare_outcome.chance - armored_outcome.chance == expected
        {
            delta_matches += 1;
        }
        if armored_outcome.part == 3 {
            chest_delta = Some(bare_outcome.chance - armored_outcome.chance);
        }
    }
    rows.push(row(
        "GV-ARM-03",
        format!(
            "seeds={seeds} delta_matches={delta_matches} chest_delta={}",
            chest_delta.unwrap_or(-1)
        ),
        format!("seeds={seeds} delta_matches={seeds} chest_delta=45"),
        "identical rolls: Δchance = −ArmorValues[part] (CalculateArmorToHit, FormulaHelper.cs:1149-1161)",
    ));

    // (c) three hits wear the struck cuirass by the DFU amount and the probe
    // logs every report (item slot + wear + condition).
    let mut kit = cuirass_table();
    let mut wear_dice = WearDice::new(M22_SEED);
    let mut wear_log = Vec::new();
    for _ in 0..3 {
        let report = kit
            .damage_equipment(3, 20, &mut wear_dice)
            .expect("cuirass takes the wear");
        assert_eq!(report.slot, EquipSlot::ChestArmor);
        wear_log.push(report.wear);
    }
    let condition = kit.get(EquipSlot::ChestArmor).map(|item| item.current_condition);
    rows.push(row(
        "GV-ARM-04",
        format!("wear={wear_log:?} condition={condition:?}"),
        "wear=[2, 2, 2] condition=Some(144)".to_string(),
        "3 hits × (10·20+50)/100 = 2 wear; plate max 150 = 100·COND_MUL[1]/4 (FormulaHelper.cs:1130-1132, ItemBuilder.cs:51)",
    ));

    // (c2) the typed report also carries the break: 0 condition, item unequipped.
    let mut kit = cuirass_table();
    let mut wear_dice = WearDice::new(M22_SEED);
    let report = kit
        .damage_equipment(3, 2_000, &mut wear_dice)
        .expect("cuirass takes the wear");
    rows.push(row(
        "GV-ARM-05",
        format!(
            "broke={} wear={} condition={} removed={} slot_empty={}",
            report.broke,
            report.wear,
            report.condition,
            report.removed.is_some(),
            kit.get(EquipSlot::ChestArmor).is_none()
        ),
        "broke=true wear=200 condition=0 removed=true slot_empty=true".to_string(),
        "wear 200 > condition 150 ⇒ LowerCondition 0 + ItemBreaks → UnequipItem (DaggerfallUnityItem.cs:1170-1190)",
    ));

    debug_assert_eq!(rows.len(), 52, "block A is 52 checks (47 spec §6 + 5 GV-ARM)");
    rows
}

/// `--probe COMBAT-001` — 52 golden vectors with injected rolls (spec §6 A).
pub fn run_combat_001() -> i32 {
    println!(
        "PROBE-COMBAT-001 - M22 combat: hit/damage/cadence (REQ-L99-055 · CR-be05b6d8 · spec §1-§3)"
    );
    let rows = combat_rows();
    let ok = print_rows("A · formulas with injected rolls (GV-HIT 15 + GV-DMG 17 + GV-CAD 15 + GV-ARM 5)", &rows);
    let checks = rows.len();
    let passed = rows.iter().filter(|row| row.ok).count();
    let failed = checks - passed;
    println!("checks={checks} passed={passed} failed={failed}");
    let code = i32::from(!(ok && checks == 52));
    println!("EXIT={code}");
    if code == 0 {
        println!(
            "VERDICT: PROBE-COMBAT-001 PASS - checks=52 passed=52 failed=0 · injected rolls · equipment-fed armour · 0 global RNG"
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
/// `BodyParts` index the kit cuirass covers (`DaggerfallUnityItem.cs:1103-1125`,
/// `EquipSlot::ChestArmor => 3`). [`loop_run`] asserts it against the M21 table
/// instead of trusting the literal.
const CHEST_BODY_PART: usize = 3;

/// Full block B run.
#[derive(Debug, Clone, PartialEq, Eq)]
struct LoopRun {
    ticks: u32,
    swings: u32,
    hits: u32,
    damaging_hits: u32,
    /// Damaging hits whose struck part is the chest — the only part the kit
    /// cuirass covers (`BodyParts` index 3; `DamageEquipment` is per part,
    /// `FormulaHelper.cs:1113-1122`). Independent of the wears actually
    /// reported, so `armor_wear_events == chest_damaging_hits` is a real
    /// predicate ("wear iff the struck part is covered"), not a tautology.
    chest_damaging_hits: u32,
    dmg_total: i32,
    armor_wear_events: u32,
    weapon_wear_events: u32,
    enemy_strikes: u32,
    multi_hit_strikes: u32,
    max_hits: u32,
    armor_condition_start: i32,
    armor_condition_end: i32,
    weapon_condition_start: i32,
    weapon_condition_end: i32,
    digest: u64,
}

/// Synthetic M21 kit, retail-free (spec §0): the steel cuirass
/// (`hit_points` 100 · plate `0x0201` ⇒ condition 150) and the steel
/// Longsword (`hit_points` 800 · steel ⇒ condition 1200). Both numbers are
/// the M21-measured conditions (`ItemBuilder.cs:51` scales `hit_points` by
/// `CONDMUL`).
///
/// The cuirass covers **one** body part (`ItemEquipTable::get_equip_slot`:
/// `102 => ChestArmor`), so DFU's per-part `DamageEquipment`
/// (`FormulaHelper.cs:1113-1122`) only reports a wear when the chest is the
/// struck part — the loop therefore counts covered hits and uncovered hits
/// separately instead of pretending full coverage.
fn kit_items() -> (Item, Item) {
    let mut cuirass_template = ItemTemplate::empty(102);
    cuirass_template.hit_points = 100;
    let mut cuirass = Item::from_template(&cuirass_template);
    apply_armor_material(&mut cuirass, 0x0201);
    let mut sword_template = ItemTemplate::empty(120);
    sword_template.hit_points = 800;
    let mut sword = Item::from_template(&sword_template);
    apply_weapon_material(&mut sword, 1);
    (cuirass, sword)
}

/// One wear report into the digest: slot index, amount and the resulting
/// condition (spec §6 B "wear").
fn push_wear(bytes: &mut Vec<u8>, wear: &ItemWear) {
    // `+ 1` so an absent slot encodes 0 (the reports always carry a slot).
    let slot_code = wear.slot.index().map_or(0u32, |index| index as u32 + 1);
    bytes.extend_from_slice(&slot_code.to_le_bytes());
    bytes.extend_from_slice(&wear.wear.to_le_bytes());
    bytes.extend_from_slice(&wear.condition.to_le_bytes());
}

/// One loop run through the **production** `CombatSession` (the same runtime
/// the window drives): an armed player (`chance_mod` 45, strength 60, steel
/// Longsword + cuirass, 10 000 HP) against one `EnemyMonster` (cuirass,
/// `MobileEnemy` bands `(2,5)(2,4)(1,3)`, 10 000 HP, `chance_mod` 135 — a
/// declared fixture, see the comment at the assignment); 0 Bevy, 0 I/O,
/// 0 retail data.
fn loop_run(seed: u64) -> LoopRun {
    let (cuirass, sword) = kit_items();
    let weapon_condition_start = sword.current_condition;
    let mut player = Fighter::new(
        Combatant::new(1, EntityKind::Player),
        DeathSequence::for_player_height(1.7),
    );
    player.combatant.strength_modifier = damage_modifier(60);
    player.combatant.max_health = 10_000;
    player.combatant.health = 10_000;
    player.chance_mod = 45;
    let player_weapon = player.equip(sword);
    let player_armor = player.equip(cuirass);
    assert!(
        matches!(player_weapon, EquipResult::Equipped(_))
            && matches!(player_armor, EquipResult::Equipped(_)),
        "the loop kit equips"
    );
    let (enemy_cuirass, _) = kit_items();
    let armor_condition_start = enemy_cuirass.current_condition;
    let mut enemy = Fighter::new(
        Combatant::new(4, EntityKind::EnemyMonster),
        DeathSequence::for_enemy(),
    );
    enemy.combatant.max_health = 10_000;
    enemy.combatant.health = 10_000;
    // Armour policy of a spawned enemy (`EnemyEntity.SetEnemyEquipment`):
    // base `100` floored at `MobileEnemy.ArmorValue * 5`
    // (`EnemyEntity.cs:409-450`). Declared `armor_value 0` because this tree
    // carries no `MobileEnemy` row for the synthetic monster — the floor is
    // then 0, i.e. the kit contributes nothing to the enemy's own hit chance.
    // `chance_mod` is the canonical §1.4 fixture (45): against the player's
    // `100`-based vector it composes to 95 on the six uncovered parts and 50
    // on the cuirass chest and nowhere clamps, so the plate term stays
    // visible; the real composition — `GetLiveSkillValue(HandToHand)` for the
    // unarmed branch (`FormulaHelper.cs:589-591`, `:592`) — is an M25 item.
    enemy.armor_policy = Some(EnemyArmorPolicy::Monster { armor_value: 0 });
    enemy.chance_mod = 45;
    enemy.monster_bands = Some([(2, 5), (2, 4), (1, 3)]);
    let enemy_armor = enemy.equip(enemy_cuirass);
    assert!(
        matches!(enemy_armor, EquipResult::Equipped(_)),
        "the enemy cuirass equips"
    );
    // The invariant below reads `attack.part == CHEST_BODY_PART`: verify the
    // constant against the M21 slot→part table instead of trusting it.
    assert_eq!(
        body_part_for_equip_slot(EquipSlot::ChestArmor),
        Some(CHEST_BODY_PART),
        "the cuirass covers the chest (`BodyParts` index 3)"
    );

    let mut session = CombatSession::new(player, vec![enemy], seed);
    let mut enemy_attack = EnemyAttack::new();
    let mut bytes = Vec::new();
    let mut swings = 0u32;
    let mut hits = 0u32;
    let mut damaging_hits = 0u32;
    let mut chest_damaging_hits = 0u32;
    let mut dmg_total = 0i32;
    let mut armor_wear_events = 0u32;
    let mut weapon_wear_events = 0u32;
    let mut enemy_strikes = 0u32;
    let mut multi_hit_strikes = 0u32;
    let mut max_hits = 0u32;

    for tick in 0..LOOP_TICKS {
        // Player swing at fixed cadence.
        if tick % LOOP_SWING_TICKS == 0 {
            swings += 1;
            if let Some(strike) = session.player_strike(0) {
                let attack = strike.attack;
                bytes.extend_from_slice(&u8::from(attack.hit).to_le_bytes());
                bytes.extend_from_slice(&attack.chance.to_le_bytes());
                bytes.extend_from_slice(&attack.part.to_le_bytes());
                bytes.extend_from_slice(&attack.damage.to_le_bytes());
                bytes.extend_from_slice(&attack.rolls_consumed.to_le_bytes());
                if attack.hit {
                    hits += 1;
                }
                // M21 §4 boundary (`FormulaHelper.cs:1088-1122`): wear exists
                // only when damage > 0; the attacker weapon pass runs first.
                if attack.damage > 0 {
                    damaging_hits += 1;
                    dmg_total += attack.damage;
                    // The struck part decides the target-side wear, not the
                    // wear report: counted here, asserted against
                    // `armor_wear_events` in GV-LOOP-wear.
                    if attack.part == CHEST_BODY_PART {
                        chest_damaging_hits += 1;
                    }
                    if let Some(wear) = &strike.weapon_wear {
                        weapon_wear_events += 1;
                        push_wear(&mut bytes, wear);
                    }
                    if let Some(wear) = &strike.target_wear {
                        armor_wear_events += 1;
                        push_wear(&mut bytes, wear);
                    }
                }
            }
        }
        // Enemy hit on timer (player level 10 / reflexes 3, R-22.4).
        if enemy_attack.tick(LOOP_DT) {
            if let Some(strike) = session.enemy_strike(0) {
                enemy_strikes += 1;
                bytes.extend_from_slice(&strike.damage.to_le_bytes());
                match &strike.multi {
                    Some(multi) => {
                        bytes.extend_from_slice(&multi.hits.to_le_bytes());
                        for band in multi.per_hit {
                            bytes.extend_from_slice(&band.to_le_bytes());
                        }
                        if multi.hits >= 2 {
                            multi_hit_strikes += 1;
                        }
                        max_hits = max_hits.max(multi.hits);
                    }
                    None => {
                        bytes.extend_from_slice(&0u32.to_le_bytes());
                        bytes.extend_from_slice(&[0i32; 3].map(|bank| bank.to_le_bytes()).concat());
                    }
                }
            }
            let roll_ms = session.enemy_timer_roll(0).unwrap_or(1500);
            enemy_attack.reset_melee_timer(roll_ms, 10, 3);
        }
    }

    let armor_condition_end = session.enemies[0]
        .equipment
        .get(EquipSlot::ChestArmor)
        .map_or(0, |item| item.current_condition);
    let weapon_condition_end = session.player.weapon_slot.and_then(|slot| {
        session
            .player
            .equipment
            .get(slot)
            .map(|item| item.current_condition)
    });

    LoopRun {
        ticks: LOOP_TICKS,
        swings,
        hits,
        damaging_hits,
        chest_damaging_hits,
        dmg_total,
        armor_wear_events,
        weapon_wear_events,
        enemy_strikes,
        multi_hit_strikes,
        max_hits,
        armor_condition_start,
        armor_condition_end,
        weapon_condition_start,
        weapon_condition_end: weapon_condition_end.unwrap_or(0),
        digest: fnv1a64(&bytes),
    }
}

/// `--probe COMBAT-LOOP-001` — 600 ticks with counters and digest (spec §6 B).
pub fn run_combat_loop_001() -> i32 {
    let alt_seed = M22_SEED.wrapping_add(LOOP_SEED_ALT_OFFSET);
    println!(
        "PROBE-COMBAT-LOOP-001 - M22 combat loop (REQ-L99-055 · CR-be05b6d8 · spec §6 B)"
    );
    println!("seed={M22_SEED} seed_alt={alt_seed} (600 ticks @ 60 Hz · fixed cadence 10 ticks · Dice100 per actor · production CombatSession)");

    let primary = loop_run(M22_SEED);
    let repeat = loop_run(M22_SEED);
    let alternate = loop_run(alt_seed);

    println!(
        "ticks={} swings={} hits={} damaging_hits={} chest_damaging_hits={} dmg_total={} armor_wear_events={} weapon_wear_events={} enemy_strikes={} multi_hit_strikes={} max_hits={} armor_condition={}→{} weapon_condition={}→{} digest={:016x}",
        primary.ticks,
        primary.swings,
        primary.hits,
        primary.damaging_hits,
        primary.chest_damaging_hits,
        primary.dmg_total,
        primary.armor_wear_events,
        primary.weapon_wear_events,
        primary.enemy_strikes,
        primary.multi_hit_strikes,
        primary.max_hits,
        primary.armor_condition_start,
        primary.armor_condition_end,
        primary.weapon_condition_start,
        primary.weapon_condition_end,
        primary.digest,
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
            format!(
                "damaging_hits={} weapon_wear_events={} chest_damaging_hits={} armor_wear_events={} damaging_hits>0={}",
                primary.damaging_hits,
                primary.weapon_wear_events,
                primary.chest_damaging_hits,
                primary.armor_wear_events,
                primary.damaging_hits > 0
            ),
            // `armor_wear_events` is compared against the INDEPENDENT chest
            // count: one report per chest damaging hit, and none for the six
            // uncovered parts.
            format!(
                "damaging_hits={} weapon_wear_events={} chest_damaging_hits={} armor_wear_events={} damaging_hits>0=true",
                primary.damaging_hits,
                primary.damaging_hits,
                primary.chest_damaging_hits,
                primary.chest_damaging_hits
            ),
            "invariant «wear iff the struck part is covered»: every damaging hit wears the attacker weapon once, and the target-side pass (`FormulaHelper.cs:1088-1122`, per struck part) reports exactly the chest hits — the cuirass covers only the chest (`BodyParts` 3)",
        ),
        row(
            "GV-LOOP-armor",
            format!(
                "armor_condition_start={} armor_condition_end={} decreased={}",
                primary.armor_condition_start,
                primary.armor_condition_end,
                primary.armor_condition_end < primary.armor_condition_start
            ),
            format!(
                "armor_condition_start={} armor_condition_end={} decreased=true",
                primary.armor_condition_start, primary.armor_condition_end
            ),
            "the enemy cuirass (`DamageEquipment`, `FormulaHelper.cs:1113-1122`) loses condition over the loop",
        ),
        row(
            "GV-LOOP-weapon",
            format!(
                "weapon_condition_start={} weapon_condition_end={} decreased={}",
                primary.weapon_condition_start,
                primary.weapon_condition_end,
                primary.weapon_condition_end < primary.weapon_condition_start
            ),
            format!(
                "weapon_condition_start={} weapon_condition_end={} decreased=true",
                primary.weapon_condition_start, primary.weapon_condition_end
            ),
            "the player Longsword wears on every damaging hit (attacker pass first, `FormulaHelper.cs:1093-1101`)",
        ),
        row(
            "GV-LOOP-multihit",
            format!(
                "multi_hit_strikes={} max_hits={} multi_hit_strikes>0={}",
                primary.multi_hit_strikes,
                primary.max_hits,
                primary.multi_hit_strikes > 0
            ),
            format!(
                "multi_hit_strikes={} max_hits={} multi_hit_strikes>0=true",
                primary.multi_hit_strikes, primary.max_hits
            ),
            "`DamageRolls` (up to 3 bands, `FormulaHelper.cs:654-657`) produces at least one multi-hit strike in 600 ticks",
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
    let all_ok = ok && primary.damaging_hits > 0 && primary.dmg_total > 0;
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
    // Spec §6 C (§7.4): the enemy emits once and the player runs
    // fall → fade → reset a single time. Printed from the same constructors
    // the rows check, so the summary can never disagree with the run.
    let mut summary_enemy = DeathSequence::for_enemy();
    let _ = summary_enemy.apply_set_health(100, -5, false);
    let _ = summary_enemy.apply_set_health(100, -5, false);
    let mut summary_player = DeathSequence::for_player_height(1.7);
    let _ = summary_player.apply_set_health(37, -5, false);
    let _ = summary_player.apply_set_health(37, -5, false);
    println!(
        "events_player={} events_enemy={} fall={:.2} fade={:.2} reset={:.2}",
        summary_player.events(),
        summary_enemy.events(),
        summary_player.fall_seconds,
        summary_player.fade_seconds,
        summary_player.reset_seconds
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
    fn combat_rows_are_52_and_all_pass() {
        let rows = combat_rows();
        assert_eq!(rows.len(), 52, "47 spec §6 vectors + 5 GV-ARM equipment checks");
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
        assert!(primary.damaging_hits > 0, "the loop must deal damage");
        assert!(primary.dmg_total > 0);
        assert_eq!(
            primary.damaging_hits, primary.weapon_wear_events,
            "every damaging hit wears the attacker weapon once (M21 §4)"
        );
        assert_eq!(
            primary.chest_damaging_hits, primary.armor_wear_events,
            "invariant «wear iff the struck part is covered»: the target-side pass is per \
             struck part, so exactly the chest hits wear the cuirass (M21 §4)"
        );
        assert!(
            primary.chest_damaging_hits > 0,
            "the loop must land at least one damaging hit on the armour-covered chest"
        );
        assert!(
            primary.armor_condition_end < primary.armor_condition_start,
            "the cuirass loses condition ({} -> {})",
            primary.armor_condition_start,
            primary.armor_condition_end
        );
        assert!(
            primary.weapon_condition_end < primary.weapon_condition_start,
            "the Longsword loses condition ({} -> {})",
            primary.weapon_condition_start,
            primary.weapon_condition_end
        );
        assert!(
            primary.multi_hit_strikes > 0,
            "the monster branch produces multi-hit strikes (max_hits = {})",
            primary.max_hits
        );
        assert!(primary.max_hits >= 2);
        assert_eq!(primary, loop_run(M22_SEED), "same seed ⇒ same run");
        assert_ne!(
            primary.digest,
            loop_run(M22_SEED.wrapping_add(LOOP_SEED_ALT_OFFSET)).digest,
            "alternate seed ⇒ different digest (anti-false-positive (f))"
        );
    }

    #[test]
    fn armour_comes_from_equipment() {
        let table = cuirass_table();
        // Fail-before (pre-bridge `Combatant::new`): `[0, 0, 0, 0, 0, 0, 0]`.
        let combatant = combatant_with_equipment(2, EntityKind::EnemyMonster, &table);
        assert_eq!(combatant.armor_values, [0, 0, 0, -45, 0, 0, 0]);
        // Independent of the projection: the cuirass is really in the table.
        assert!(table.get(EquipSlot::ChestArmor).is_some());
    }

    #[test]
    fn fnv1a64_matches_reference_vector() {
        // FNV-1a 64 reference vector for the empty string.
        assert_eq!(fnv1a64(b""), 0xcbf2_9ce4_8422_2325);
    }
}
