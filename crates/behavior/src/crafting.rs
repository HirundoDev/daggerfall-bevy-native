// Copyright (c) 2026. MIT License.
//
// Crafting formulas — SPEC M24 §2-§3.
//
// Every constant and formula is transcribed from Daggerfall Unity
// `@0038a1e61999ad2c73fb1f33b706cae9a88ec66f` (MIT):
//
// - enchantment power per material: `FormulaHelper.cs:2644-2728`
//   (`GetItemEnchantmentPower`, weapons `:2664-2693`, armor `:2695-2728`)
// - recipe hash over sorted ids: `PotionRecipe.cs:295-307`
//   (`GetHashCode`, `int` wrap, seed 17, factor 23)
//
// **Frontier (not invented here):** the enchantment *budget* accounting and
// the power/side-effect classification by `EnchantCost` sign live in the
// `daggerfall-crafting` organ (`enchanting.rs`); the classic recipe table and
// the cauldron lookup live in `alchemy.rs`. This module is the pure arithmetic
// both consume: the material multiplier table, the power formula and the
// ingredient-id hash. 0 Bevy, 0 I/O.

/// Enchantment-power multiplier per weapon material
/// (`WeaponMaterialTypes`, `ItemEnums.cs:64-77`, Iron=0 .. Daedric=9) — the
/// single array shared by weapons and armor (`FormulaHelper.cs:2659-2661`).
///
/// Index: 0 Iron `-0.25`, 1 Steel `0`, 2 Silver `+0.75`, 3 Elven `+0.25`,
/// 4 Dwarven `+0.5`, 5 Mithril `+0.25`, 6 Adamantium `+0.75`, 7 Ebony `+1.0`,
/// 8 Orcish `+1.5`, 9 Daedric `+2.0`.
pub const MATERIAL_ENCHANT_MULT: [f32; 10] = [
    -0.25, 0.0, 0.75, 0.25, 0.5, 0.25, 0.75, 1.0, 1.5, 2.0,
];

/// `ArmorMaterialTypes` first plate value (`ItemEnums.cs:82-98`): Leather is
/// `0x000`, Iron is `0x200`, and each following material increments by one up
/// to Daedric `0x209`.
pub const ARMOR_MATERIAL_BASE: i32 = 0x200;

/// `ArmorMaterialTypes.Leacher` sentinel (`ItemEnums.cs:82`): Leather has no
/// plate multiplier (its `GetItemEnchantmentPower` branch returns the base).
pub const ARMOR_MATERIAL_LEATHER: i32 = 0x000;

/// Enchantment-power multiplier for a weapon material index
/// (`WeaponMaterialTypes`, 0..9). Out-of-range indices fall back to Steel (0),
/// mirroring the source's `default: 0` arm (`FormulaHelper.cs:2692`).
pub fn weapon_enchant_mult(material: usize) -> f32 {
    MATERIAL_ENCHANT_MULT
        .get(material)
        .copied()
        .unwrap_or(MATERIAL_ENCHANT_MULT[1])
}

/// Enchantment-power multiplier for an armor `native_material`
/// (`ArmorMaterialTypes`, `FormulaHelper.cs:2695-2728`). Leather (`0x000`)
/// and any non-plate value return `0.0`; Iron (`0x200`) .. Daedric (`0x209`)
/// reuse the shared [`MATERIAL_ENCHANT_MULT`] array indexed by
/// `native - 0x200`.
pub fn armor_enchant_mult(native_material: i32) -> f32 {
    if native_material == ARMOR_MATERIAL_LEATHER {
        return 0.0;
    }
    let index = native_material - ARMOR_MATERIAL_BASE;
    if (0..MATERIAL_ENCHANT_MULT.len() as i32).contains(&index) {
        MATERIAL_ENCHANT_MULT[index as usize]
    } else {
        // Non-plate armor values behave as Steel (0), the source default.
        MATERIAL_ENCHANT_MULT[1]
    }
}

/// Enchantment power of an item: `base + floor(base * mult)`
/// (`FormulaHelper.cs:2644-2662`, `Mathf.FloorToInt` — *floor*, not trunc: a
/// 175-base iron item yields `175 + floor(-43.75) = 131`, not `132`).
pub fn enchantment_power(base: i32, mult: f32) -> i32 {
    base + (f64::from(base) * f64::from(mult)).floor() as i32
}

/// Seed of the recipe hash (`PotionRecipe.cs:297`: `int hashCode = 17;`).
pub const RECIPE_HASH_SEED: i32 = 17;

/// Multiplicative factor of the recipe hash (`PotionRecipe.cs:303`:
/// `hashCode = (hashCode * 23) + ingredient.id`).
pub const RECIPE_HASH_FACTOR: i32 = 23;

/// Recipe key over a set of ingredient template ids
/// (`PotionRecipe.cs:295-307`).
///
/// The ids are sorted ascending before the fold — the source's
/// `PotionRecipe(List<int>)` constructor calls `ids.Sort()` (`:120`) so that
/// any insertion order maps to the same classic key. The fold runs in `int`
/// 32-bit wrapping arithmetic (C# `unchecked`): `h = 17; for id { h = h * 23
/// + id }`. An empty list returns `0` (`:297-298`).
pub fn recipe_hash(ids: &[i32]) -> i32 {
    if ids.is_empty() {
        return 0;
    }
    let mut sorted = ids.to_vec();
    sorted.sort_unstable();
    let mut hash = RECIPE_HASH_SEED;
    for id in sorted {
        hash = hash
            .wrapping_mul(RECIPE_HASH_FACTOR)
            .wrapping_add(id);
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;

    /// GV-PWR-01..06: Longsword base 600 across the weapon material table.
    #[test]
    fn gv_pwr_longsword_600() {
        assert_eq!(enchantment_power(600, weapon_enchant_mult(0)), 450); // Iron
        assert_eq!(enchantment_power(600, weapon_enchant_mult(1)), 600); // Steel
        assert_eq!(enchantment_power(600, weapon_enchant_mult(3)), 750); // Elven
        assert_eq!(enchantment_power(600, weapon_enchant_mult(4)), 900); // Dwarven
        assert_eq!(enchantment_power(600, weapon_enchant_mult(7)), 1200); // Ebony
        assert_eq!(enchantment_power(600, weapon_enchant_mult(9)), 1800); // Daedric
    }

    /// GV-PWR-07..10: armor uses `0x200`-based plate values and the same array.
    /// Iron boots `175 + floor(-43.75) = 131` proves *floor*, not trunc.
    #[test]
    fn gv_pwr_armor_plate() {
        assert_eq!(enchantment_power(175, armor_enchant_mult(0x200)), 131); // Iron
        assert_eq!(enchantment_power(175, armor_enchant_mult(0x209)), 525); // Daedric
        assert_eq!(enchantment_power(400, armor_enchant_mult(0x000)), 400); // Leather
        assert_eq!(enchantment_power(400, armor_enchant_mult(0x209)), 1200); // Daedric
    }

    /// GV-PWR-11/12: Staff base 700, Mithril and Daedric.
    #[test]
    fn gv_pwr_staff_700() {
        assert_eq!(enchantment_power(700, weapon_enchant_mult(5)), 875); // Mithril
        assert_eq!(enchantment_power(700, weapon_enchant_mult(9)), 2100); // Daedric
    }

    /// GV-RCP-01..12: exact classic recipe hashes over sorted ids.
    #[test]
    fn gv_recipe_hashes() {
        assert_eq!(recipe_hash(&[27, 30, 59]), 221871);
        assert_eq!(recipe_hash(&[]), 0);
        assert_eq!(recipe_hash(&[9]), 400);
        assert_eq!(recipe_hash(&[0]), 391);
        assert_eq!(recipe_hash(&[1, 2]), 9018);
        assert_eq!(recipe_hash(&[2, 3]), 9042);
        assert_eq!(recipe_hash(&[27, 56]), 9670);
        assert_eq!(recipe_hash(&[30, 59]), 9742);
        assert_eq!(recipe_hash(&[4, 5, 6]), 209076);
        assert_eq!(recipe_hash(&[1000]), 1391);
        assert_eq!(recipe_hash(&[112, 227]), 11796);
        // GV-RCP-12: unsorted input maps to the same key (both sides sort).
        assert_eq!(recipe_hash(&[59, 27, 30]), 221871);
    }

    /// GV-RCP-13: `purification` overflows `2^31` and needs the `int` wrap.
    /// `1346355782844 mod 2^32 = 2031019196` = `classicRecipeKeys[19]`.
    #[test]
    fn gv_recipe_purification_wraps() {
        let raw: i64 = {
            let mut h: i64 = 17;
            for id in [3, 31, 39, 49, 56, 60, 62, 63] {
                h = h * 23 + id;
            }
            h
        };
        assert_eq!(raw, 1_346_355_782_844);
        assert_eq!(recipe_hash(&[62, 63, 60, 31, 56, 39, 3, 49]), 2_031_019_196);
    }
}
