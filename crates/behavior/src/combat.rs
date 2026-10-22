//! M22 — pure combat formulas: hit chance (spec M22 §1 ·
//! `CalculateSuccessfulHit`), damage (§2), cadence (§3) and death (§4). Each
//! formula is transcribed from Daggerfall Unity @0038a1e6 (MIT) with its line
//! citation.
//!
//! Pure: 0 Bevy, 0 I/O, 0 global RNG — rolls enter as parameters and the
//! `Dice100` verdict is injected into [`success_roll`], the same pattern by
//! which the die enters as a parameter in [`crate::durability`] (M21). The
//! engine PRNG lives in `daggerfall-combat::rng` per actor (delta D-M22-1).

// ── constants ───────────────────────────────────────────────────────────────

/// Hit chance clamped to `[3, 97]` before rolling (`FormulaHelper.cs:825`).
pub const CHANCE_MIN: i32 = 3;
/// Hit chance clamped to `[3, 97]` before rolling (`FormulaHelper.cs:825`).
pub const CHANCE_MAX: i32 = 97;
/// `classicFrameUpdate` — denominator of every cadence (`FormulaHelper.cs:55`).
pub const CLASSIC_FRAME_UPDATE: i32 = 980;
/// Universal `−50` adjustment, always (`FormulaHelper.cs:1246-1248`).
pub const UNIVERSAL_ADJUSTMENT: i32 = -50;
/// Monster modifier `+40` (`FormulaHelper.cs:1242-1245`).
pub const MONSTER_HIT_BONUS: i32 = 40;
/// `CalculateStruckBodyPart` table (`FormulaHelper.cs:869-870`): 20
/// `Range(0,20)` entries ⇒ `{0:2, 1:3, 2:3, 3:4, 4:4, 5:3, 6:1}`.
/// `BodyParts`: Head=0, RightArm=1, LeftArm=2, Chest=3, Hands=4, Legs=5,
/// Feet=6 (`ItemEnums.cs:140-150`).
pub const STRUCK_BODY_PARTS: [usize; 20] = [
    0, 0, 1, 1, 1, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 6,
];
/// `fallSpeed = 2.5` m/s of the camera fall (`PlayerDeath.cs:29`).
pub const FALL_SPEED_MPS: f32 = 2.5;
/// `FadeDuration = 2` s (`PlayerDeath.cs:32`).
pub const FADE_SECONDS: f32 = 2.0;
/// `TimeBeforeReset = 3` s (`PlayerDeath.cs:33`).
pub const RESET_SECONDS: f32 = 3.0;

// ── §1 · Hit chance ─────────────────────────────────────────────────────────

/// `CalculateStruckBodyPart` (`FormulaHelper.cs:863-871`): `Range(0,20)`
/// over [`STRUCK_BODY_PARTS`]. An out-of-range roll is clamped (extra
/// defense; DFU only rolls 0..19).
pub fn struck_body_part(roll: i32) -> usize {
    let index = roll.clamp(0, 19) as usize;
    STRUCK_BODY_PARTS[index]
}

/// `Dice100.SuccessRoll(c)` (`Dice100.cs:15-18`): `roll(0..99) < c`;
/// `c ≤ 0` ⇒ never, `c ≥ 100` ⇒ always. The roll enters injected — the probe
/// consumes rolls, never a live `rand()` (spec §6).
pub fn success_roll(roll: i32, chance: i32) -> bool {
    if chance <= 0 {
        false
    } else if chance >= 100 {
        true
    } else {
        roll < chance
    }
}

/// Adrenaline of ONE actor (`FormulaHelper.cs:1163-1183`): `+5` with
/// `Career.AdrenalineRush`, `+8` with the improved one
/// (`ImprovedAdrenalineRush`), while `health < max/8` (integer division,
/// literal DFU). Returns `0` without the perk or when out of range.
pub fn adrenaline_bonus(rush: bool, improved: bool, health: i32, max_health: i32) -> i32 {
    if rush && health < max_health / 8 {
        if improved {
            8
        } else {
            5
        }
    } else {
        0
    }
}

/// Adrenaline Δ of the hit-chance calculation: attacker bonus minus target
/// bonus (`FormulaHelper.cs:1163-1183`).
pub fn adrenaline_to_hit(attacker_bonus: i32, target_bonus: i32) -> i32 {
    attacker_bonus - target_bonus
}

/// Attribute Δ /10 (`FormulaHelper.cs:1185-1200`): `i32` division truncated
/// toward zero (same as C#): `(luckA − luckT)/10 + (agiA − agiT)/10`.
pub fn stats_to_hit(att_luck: i32, tgt_luck: i32, att_agi: i32, tgt_agi: i32) -> i32 {
    (att_luck - tgt_luck) / 10 + (att_agi - tgt_agi) / 10
}

/// Critical (`FormulaHelper.cs:1216-1218`): after `SuccessRoll(critA)`,
/// `+ critA/10`. The roll verdict is injected.
pub fn critical_bonus(critical_strike: i32, crit_roll_ok: bool) -> i32 {
    if crit_roll_ok {
        critical_strike / 10
    } else {
        0
    }
}

/// Final adjustments (`FormulaHelper.cs:1224-1251`): the biography
/// (`− player.BiographyAvoidHitMod`) subtracts **only** if the target is the
/// player (delta D-M22-2), `+40` if the target is `EnemyMonster` (and not the
/// player) and `−50` **always** (R-22.2: author adjustment, replicated
/// literally).
pub fn adjustments(
    is_player_target: bool,
    biography_avoid_hit_mod: i32,
    target_is_monster: bool,
) -> i32 {
    let mut adjustment = UNIVERSAL_ADJUSTMENT;
    if is_player_target {
        adjustment -= biography_avoid_hit_mod;
    }
    if target_is_monster && !is_player_target {
        adjustment += MONSTER_HIT_BONUS;
    }
    adjustment
}

/// Components of `CalculateSuccessfulHit` (`FormulaHelper.cs:796-828`), in
/// spec §1.2 order. `Default` = canonical actor from §1.4 (no mods).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct HitMods {
    /// Base skill + swing + proficiency + racial + backstab + `weapon_to_hit`
    /// (`:592-612`, `:679`) — already summed by the composer (input side,
    /// spec §1.1).
    pub chance_mod: i32,
    /// `ArmorValues[part]`, negative (`:1149-1161`).
    pub armor_value: i32,
    /// `IncreasedArmorValueModifier` (`:1149-1161`).
    pub inc_armor_mod: i32,
    /// `DecreasedArmorValueModifier` (`:1149-1161`).
    pub dec_armor_mod: i32,
    /// Adrenaline Δ attacker − target (`:1163-1183`).
    pub adrenaline: i32,
    /// Enchantment `ChanceToHitModifier` (`:814`).
    pub enchant_mod: i32,
    /// Stats Δ/10 (`:1185-1200`).
    pub stats_diff: i32,
    /// Dodge skill of the **target** (`/4`, not `/10`; `:1213`).
    pub dodging: i32,
    /// `crit/10` already resolved with the injected roll (`:1216-1218`).
    pub critical_bonus: i32,
    /// Biography/monster/`−50` — see [`adjustments`] (`:1224-1251`).
    pub adjustments: i32,
}

/// `CalculateSuccessfulHit` — exact composition and `clamp(3, 97)`
/// (`FormulaHelper.cs:805-827`).
pub fn chance_to_hit(mods: HitMods) -> i32 {
    let chance = mods.chance_mod
        + mods.armor_value
        + mods.inc_armor_mod
        + mods.dec_armor_mod
        + mods.adrenaline
        + mods.enchant_mod
        + mods.stats_diff
        - mods.dodging / 4
        + mods.critical_bonus
        + mods.adjustments;
    chance.clamp(CHANCE_MIN, CHANCE_MAX)
}

/// Full `CalculateSuccessfulHit` with the **success** roll injected
/// (`FormulaHelper.cs:827`): `Dice100.SuccessRoll(clamp(…, 3, 97))`.
pub fn successful_hit(mods: HitMods, hit_roll: i32) -> bool {
    success_roll(hit_roll, chance_to_hit(mods))
}

// ── §2 · Damage ─────────────────────────────────────────────────────────────

/// Material gate (`FormulaHelper.cs:576-583`): if
/// `target.MinMetalToHit > weapon.NativeMaterialValue` the attack returns
/// **0 with no roll and no wear** (spec §2.1 · GV-DMG-12). `true` = blocked.
/// Both sides use the `WeaponMaterialTypes` scale (M21 §2.2).
pub fn material_gate(target_min_metal_to_hit: i32, weapon_native_material: i32) -> bool {
    target_min_metal_to_hit > weapon_native_material
}

/// Weapon damage floor (`FormulaHelper.cs:760-761`): `damage < 1 ⇒ 0`
/// (floor **0**, not 1 — anti-false-positive (e) of spec §6).
pub fn weapon_floor(damage: i32) -> i32 {
    if damage < 1 {
        0
    } else {
        damage
    }
}

/// Weapon damage (`FormulaHelper.cs:736-761`, spec §2.1): `roll + damageMods
/// + DamageModifier(str) + material`, with the 0 floor **after** strength and
/// material. The `roll` ranges are supplied by the composer (R-22.7: the
/// retail table lives in `FormulaHelper`). (GV-DMG-01..04)
pub fn weapon_damage(
    roll: i32,
    damage_mods: i32,
    strength_modifier: i32,
    material_mod: i32,
) -> i32 {
    weapon_floor(roll + damage_mods + strength_modifier + material_mod)
}

/// Literal skeletal halving (`FormulaHelper.cs:743`) and silver ×2
/// (`:748-749`) against `SkeletalWarrior` (spec §2.1): **non-sharp** weapons
/// (`flags & 0x10 == 0`) cut the damage in half and **then** silver doubles
/// it (GV-DMG-13: the golden passes a sharp silver weapon ⇒ `9 · 2`).
pub fn skeletal_weapon_adjust(damage: i32, weapon_flags: u16, is_silver: bool) -> i32 {
    let mut adjusted = damage;
    if weapon_flags & 0x10 == 0 {
        adjusted /= 2;
    }
    if is_silver {
        adjusted *= 2;
    }
    adjusted
}

/// Backstab (`FormulaHelper.cs:984-986`, call site `:688`): `×3` only with
/// `backstabbingLevel > 1` and a successful `SuccessRoll(level)` (injected
/// roll). (GV-DMG-05)
pub fn backstab_damage(damage: i32, backstabbing_level: i32, backstab_roll_ok: bool) -> i32 {
    if backstabing_requires_roll(backstabbing_level) && backstab_roll_ok {
        damage * 3
    } else {
        damage
    }
}

/// `backstabbingLevel > 1` — precondition of the ×3 (`:984-986`).
pub fn backstabing_requires_roll(backstabbing_level: i32) -> bool {
    backstabbing_level > 1
}

/// H2H min/max (`CalculateHandToHandMin/MaxDamage`,
/// `FormulaHelper.cs:434-453`): `skill/10 + 1` and `skill/5 + 1`; the strength
/// bonus applies only if the attacker is the **player** (H2H row of
/// spec §2.1 — GV-DMG-09..11).
pub fn h2h_range(hand_to_hand_skill: i32, strength_modifier: i32, attacker_is_player: bool) -> (i32, i32) {
    let min = hand_to_hand_skill / 10 + 1;
    let max = hand_to_hand_skill / 5 + 1;
    let bonus = if attacker_is_player {
        strength_modifier
    } else {
        0
    };
    (min + bonus, max + bonus)
}

/// Reflexes gate of the monster multi-hit (`FormulaHelper.cs:654-657`):
/// `reflexesChance = 50 − 10·(player.Reflexes − 2)` % — level and reflexes
/// are the **player's** (GV-DMG-15 · GV-CAD-09..11).
pub fn reflexes_gate(player_reflexes: i32) -> i32 {
    50 - 10 * (player_reflexes - 2)
}

/// Multi-hit gate verdict: `rand()%100 < reflexesChance` with the roll
/// injected (`FormulaHelper.cs:654-657`). (GV-DMG-14 · GV-CAD-12/13)
pub fn multi_hit_gate(rand_mod_100: i32, player_reflexes: i32) -> bool {
    rand_mod_100 < reflexes_gate(player_reflexes)
}

// ── §3 · Cadence ────────────────────────────────────────────────────────────

/// Melee animation tick (`FormulaHelper.cs:830-838`):
/// `3·(115 − LiveSpeed) / 980` seconds. (GV-CAD-01..04)
pub fn melee_tick_seconds(live_speed: i32) -> f32 {
    (3 * (115 - live_speed)) as f32 / CLASSIC_FRAME_UPDATE as f32
}

/// Bow cooldown (`FormulaHelper.cs:840-848`):
/// `(10·(100 − LiveSpeed) + 800) / 980` seconds. (GV-CAD-05..08)
pub fn bow_cooldown_seconds(live_speed: i32) -> f32 {
    (10 * (100 - live_speed) + 800) as f32 / CLASSIC_FRAME_UPDATE as f32
}

/// Enemy melee timer (`EnemyAttack.cs:114-132`): the already consumed
/// `Range(1500,3001)` roll, adjusted by the **player's level and reflexes**
/// (R-22.4: this is how DFU implements it), `max(0)` and `/980` s.
/// (GV-CAD-14/15)
pub fn enemy_melee_timer_seconds(roll_ms: i32, player_level: i32, player_reflexes: i32) -> f32 {
    let millis = roll_ms - 50 * (player_level - 10) + 450 * (player_reflexes - 2);
    millis.max(0) as f32 / CLASSIC_FRAME_UPDATE as f32
}

// ── §4 · Death ──────────────────────────────────────────────────────────────

/// `SetHealth(int, restore=false)` (`DaggerfallEntity.cs:331-338`):
/// `current = Clamp(amount, 0, MaxHealth)`; with `restoreMode`, `amount` is
/// written **without clamp** but the `≤ 0 ⇒ RaiseOnDeathEvent` check still
/// applies (`:333-335`, citation correction 30). Returns
/// `(new health, whether OnDeath must be emitted)`. (GV-DTH-01..04)
pub fn set_health(max_health: i32, amount: i32, restore: bool) -> (i32, bool) {
    let health = if restore {
        amount
    } else {
        amount.clamp(0, max_health)
    };
    (health, health <= 0)
}

/// Camera fall on player death (`PlayerDeath.cs:29`, `:160`):
/// `height·1.25` m travelled at `fallSpeed = 2.5` m/s ⇒
/// `height · 1.25 / 2.5` seconds. (GV-DTH-05/06)
pub fn fall_duration_seconds(controller_height: f32) -> f32 {
    controller_height * 1.25 / FALL_SPEED_MPS
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── GV-HIT-01..15 (spec §1.4) ──────────────────────────────────────────

    /// Canonical actor from §1.4: target `EnemyMonster`, armor 0, no
    /// adrenaline or stats, adjustments `+40 − 50 = −10`.
    fn gv_hit(chance_mod: i32) -> HitMods {
        HitMods {
            chance_mod,
            adjustments: adjustments(false, 0, true),
            ..HitMods::default()
        }
    }

    #[test]
    fn gv_hit_01_to_15() {
        // 01: skill 45, crit roll 40 (fails: 40 ≥ 20) → 35.
        let m = HitMods {
            critical_bonus: critical_bonus(20, success_roll(40, 20)),
            ..gv_hit(45)
        };
        assert_eq!(chance_to_hit(m), 35, "GV-HIT-01");
        // 02: armor −45 → −10 → clamp 3.
        let m = HitMods {
            armor_value: -45,
            ..gv_hit(45)
        };
        assert_eq!(chance_to_hit(m), 3, "GV-HIT-02");
        // 03: skill 100 + material +60, armor −105 → 45.
        let m = HitMods {
            armor_value: -105,
            ..gv_hit(160)
        };
        assert_eq!(chance_to_hit(m), 45, "GV-HIT-03");
        // 04: swing StrikeDown −10 → 25.
        assert_eq!(chance_to_hit(gv_hit(35)), 25, "GV-HIT-04");
        // 05: swing StrikeUp +10 → 45.
        assert_eq!(chance_to_hit(gv_hit(55)), 45, "GV-HIT-05");
        // 06: crit roll 5 < 20 ⇒ +20/10 = +2 → 37.
        let m = HitMods {
            critical_bonus: critical_bonus(20, success_roll(5, 20)),
            ..gv_hit(45)
        };
        assert_eq!(chance_to_hit(m), 37, "GV-HIT-06");
        // 07: dodging 60 → −60/4 = −15 → 20 (TARGET dodge, /4).
        let m = HitMods {
            dodging: 60,
            ..gv_hit(45)
        };
        assert_eq!(chance_to_hit(m), 20, "GV-HIT-07");
        // 08: PLAYER target, armor −50, bio 0 → −55 → clamp 3.
        let m = HitMods {
            armor_value: -50,
            adjustments: adjustments(true, 0, false),
            ..HitMods::default()
        };
        let m = HitMods {
            chance_mod: 45,
            ..m
        };
        assert_eq!(chance_to_hit(m), 3, "GV-HIT-08");
        // 09: skill 97 + material +10 → 97 (upper clamp reached).
        let m = gv_hit(107);
        assert_eq!(chance_to_hit(m), 97, "GV-HIT-09");
        // 10: skill 60, daedric +60, armor −15 → 95.
        let m = HitMods {
            armor_value: -15,
            ..gv_hit(120)
        };
        assert_eq!(chance_to_hit(m), 95, "GV-HIT-10");
        // 11: luck 80/50, agi 70/50 → +3 +2 → 40.
        let m = HitMods {
            stats_diff: stats_to_hit(80, 50, 70, 50),
            ..gv_hit(45)
        };
        assert_eq!(chance_to_hit(m), 40, "GV-HIT-11");
        // 12: luck 25/50 (−25/10 = −2), agi 35/50 (−15/10 = −1) → 32.
        let m = HitMods {
            stats_diff: stats_to_hit(25, 50, 35, 50),
            ..gv_hit(45)
        };
        assert_eq!(stats_to_hit(25, 50, 35, 50), -3, "GV-HIT-12 trunc");
        assert_eq!(chance_to_hit(m), 32, "GV-HIT-12");
        // 13: normal adrenaline on both → +5 −5 = 0 → 35.
        let both = adrenaline_bonus(true, false, 9, 100);
        assert_eq!(both, 5, "GV-HIT-13 bonus");
        let m = HitMods {
            adrenaline: adrenaline_to_hit(both, both),
            ..gv_hit(45)
        };
        assert_eq!(chance_to_hit(m), 35, "GV-HIT-13");
        // 14: backstab +20 → 55.
        assert_eq!(chance_to_hit(gv_hit(65)), 55, "GV-HIT-14");
        // 15: skill 3, armor −45 → −52 → clamp 3.
        let m = HitMods {
            armor_value: -45,
            ..gv_hit(3)
        };
        assert_eq!(chance_to_hit(m), 3, "GV-HIT-15");
    }

    #[test]
    fn success_roll_matches_table_not_float() {
        // chance 35: succeeds in [0,34], fails in [35,99] (spec §1.4).
        assert!(success_roll(0, 35));
        assert!(success_roll(34, 35));
        assert!(!success_roll(35, 35));
        assert!(!success_roll(99, 35));
        // c ≤ 0 ⇒ never; c ≥ 100 ⇒ always (`Dice100.cs:15-18`).
        assert!(!success_roll(0, 0));
        assert!(!success_roll(0, -5));
        assert!(success_roll(99, 100));
        assert!(success_roll(99, 120));
    }

    #[test]
    fn struck_body_part_distribution() {
        let mut counts = [0usize; 7];
        for roll in 0..20 {
            counts[struck_body_part(roll)] += 1;
        }
        // Distribution {0:2, 1:3, 2:3, 3:4, 4:4, 5:3, 6:1} (spec §1.3).
        assert_eq!(counts, [2, 3, 3, 4, 4, 3, 1]);
        // Defensive clamping out of range.
        assert_eq!(struck_body_part(-3), 0);
        assert_eq!(struck_body_part(99), 6);
    }

    // ── GV-DMG (spec §2.2) — the `strength_modifier` composer is the app's
    //    since M20 (`daggerfall_skills::character::damage_modifier`); here
    //    the already composed values travel (ORDER M22, 0 crate cycle). ─────

    #[test]
    fn gv_dmg_weapon_and_floor() {
        // 01: roll 7, swing 0, str 60 → +2, steel 0 → 9.
        assert_eq!(weapon_damage(7, 0, 2, 0), 9, "GV-DMG-01");
        // 02: same with iron −1 → 8.
        assert_eq!(weapon_damage(7, 0, 2, -1), 8, "GV-DMG-02");
        // 03: roll 2, StrikeUp −4, str 40 → −2, iron −1 → −5 ⇒ floor 0.
        assert_eq!(weapon_damage(2, -4, -2, -1), 0, "GV-DMG-03");
        // 04: roll 16, daedric +6, str 60 → +2 → 24.
        assert_eq!(weapon_damage(16, 0, 2, 6), 24, "GV-DMG-04");
        // 05: backstab ×3 with level > 1 and SuccessRoll ok (injected roll).
        assert_eq!(backstab_damage(9, 50, success_roll(7, 50)), 27, "GV-DMG-05");
        // No backstab (level ≤ 1) or failed roll ⇒ damage unchanged.
        assert_eq!(backstab_damage(9, 1, true), 9, "GV-DMG-05 level≤1");
        assert_eq!(backstab_damage(9, 50, false), 9, "GV-DMG-05 roll fails");
    }

    #[test]
    fn gv_dmg_h2h_range() {
        // 06: skill 30 → 4 / 7 · 07: skill 100 → 11 / 21 · 08: skill 5 → 1 / 2.
        assert_eq!(h2h_range(30, 0, true), (4, 7), "GV-DMG-06");
        assert_eq!(h2h_range(100, 0, true), (11, 21), "GV-DMG-07");
        assert_eq!(h2h_range(5, 0, true), (1, 2), "GV-DMG-08");
        // 09: str 60 → +2 on min and max (player) · 10: str 40 → −2.
        assert_eq!(h2h_range(30, 2, true), (6, 9), "GV-DMG-09");
        assert_eq!(h2h_range(30, -2, true), (2, 5), "GV-DMG-10");
        // 11: monster (not player) — the strength bonus is NOT applied.
        assert_eq!(h2h_range(30, 2, false), (4, 7), "GV-DMG-11");
    }

    #[test]
    fn gv_dmg_material_gate_and_silver() {
        // 12: iron (0) vs MinMetalToHit Steel (1) ⇒ blocked (no roll).
        assert!(material_gate(1, 0), "GV-DMG-12 gate");
        assert!(!material_gate(1, 1), "GV-DMG-12 equal ⇒ passes");
        assert!(!material_gate(0, 0), "GV-DMG-12 no gate ⇒ passes");
        // 13: silver vs SkeletalWarrior, damage 9 → 18 (sharp weapon 0x10 ⇒ no
        // /2; silver ×2) — spec §2.1 "after the /= 2 of non-sharp weapons".
        assert_eq!(skeletal_weapon_adjust(9, 0x10, true), 18, "GV-DMG-13");
        // Halving branches (no golden of their own: covered here).
        assert_eq!(skeletal_weapon_adjust(9, 0x00, true), 8, "non-sharp: /2 then ×2");
        assert_eq!(skeletal_weapon_adjust(9, 0x10, false), 9, "sharp, not silver");
        assert_eq!(skeletal_weapon_adjust(9, 0x00, false), 4, "non-sharp, not silver");
    }

    #[test]
    fn gv_dmg_reflexes_gate() {
        // 14: player Reflexes 2 → gate 50: rolls 0/49/50 ⇒ 2 hits.
        assert_eq!(reflexes_gate(2), 50, "GV-DMG-14 gate");
        assert!(multi_hit_gate(0, 2), "GV-DMG-14 roll 0");
        assert!(multi_hit_gate(49, 2), "GV-DMG-14 roll 49");
        assert!(!multi_hit_gate(50, 2), "GV-DMG-14 roll 50");
        // 15: player Reflexes 5 → gate 20 %.
        assert_eq!(reflexes_gate(5), 20, "GV-DMG-15");
    }

    // ── GV-CAD-01..15 (spec §3.1) — comparison to 6 decimals ──────────────

    #[test]
    fn gv_cad_01_to_15() {
        let f6 = |seconds: f32| format!("{seconds:.6}");
        // Melee: 195/980, 45/980, 255/980, 0/980.
        assert_eq!(f6(melee_tick_seconds(50)), "0.198980", "GV-CAD-01");
        assert_eq!(f6(melee_tick_seconds(100)), "0.045918", "GV-CAD-02");
        assert_eq!(f6(melee_tick_seconds(30)), "0.260204", "GV-CAD-03");
        assert_eq!(f6(melee_tick_seconds(115)), "0.000000", "GV-CAD-04");
        // Bow: 1300/980, 800/980, 1500/980, 1800/980.
        assert_eq!(f6(bow_cooldown_seconds(50)), "1.326531", "GV-CAD-05");
        assert_eq!(f6(bow_cooldown_seconds(100)), "0.816327", "GV-CAD-06");
        assert_eq!(f6(bow_cooldown_seconds(30)), "1.530612", "GV-CAD-07");
        assert_eq!(f6(bow_cooldown_seconds(0)), "1.836735", "GV-CAD-08");
        // Reflexes gate: 50/60/40 % and verdicts 19/20.
        assert_eq!(reflexes_gate(2), 50, "GV-CAD-09");
        assert_eq!(reflexes_gate(1), 60, "GV-CAD-10");
        assert_eq!(reflexes_gate(3), 40, "GV-CAD-11");
        assert!(multi_hit_gate(19, 5), "GV-CAD-12 passes (19 < 20)");
        assert!(!multi_hit_gate(20, 5), "GV-CAD-13 does not pass (20 ≥ 20)");
        // Enemy timer: 1950/980 and max(0) with lvl 40 / refl 1.
        assert_eq!(
            f6(enemy_melee_timer_seconds(1500, 10, 3)),
            "1.989796",
            "GV-CAD-14"
        );
        assert_eq!(
            f6(enemy_melee_timer_seconds(1500, 40, 1)),
            "0.000000",
            "GV-CAD-15"
        );
    }

    // ── GV-DTH-01..06 (spec §4.1) ──────────────────────────────────────────

    #[test]
    fn gv_dth_01_to_06() {
        // 01: SetHealth(150) with max 100 → 100, no event.
        assert_eq!(set_health(100, 150, false), (100, false), "GV-DTH-01");
        // 02: SetHealth(−5) → 0 + event.
        assert_eq!(set_health(100, -5, false), (0, true), "GV-DTH-02");
        // 03: SetHealth(0) → 0 + event (≤ 0).
        assert_eq!(set_health(100, 0, false), (0, true), "GV-DTH-03");
        // 04: restore=true → 150 without clamp and without event (150 > 0).
        assert_eq!(set_health(100, 150, true), (150, false), "GV-DTH-04");
        // 05: height 1.7 → 1.7·1.25/2.5 = 0.85 s · 06: height 2.0 → 1.0 s.
        assert_eq!(format!("{:.6}", fall_duration_seconds(1.7)), "0.850000", "GV-DTH-05");
        assert_eq!(format!("{:.6}", fall_duration_seconds(2.0)), "1.000000", "GV-DTH-06");
    }

    #[test]
    fn restore_mode_still_reports_death() {
        // Citation correction 30 (`DaggerfallEntity.cs:333-335`): restore
        // without clamp, but `≤ 0` still emits the event.
        assert_eq!(set_health(100, -5, true), (-5, true));
    }
}
