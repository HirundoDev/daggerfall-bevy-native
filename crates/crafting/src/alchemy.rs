// Potion alchemy — SPEC M24 §3.
//
// The 20 classic recipes are transcribed from Daggerfall Unity
// `@0038a1e61999ad2c73fb1f33b706cae9a88ec66f` (MIT), each registered in its
// effect's `SetPotionProperties()` (`Effects/**.cs`) with the exact
// `DisplayNameKey`, `Price`, ingredient template ids, `TextureRecord` override
// (default 11, `PotionRecipe.cs:34`) and `EffectSettings`. The recipe key is
// the hash of the **sorted** ingredient ids (`PotionRecipe.cs:295-307`,
// `daggerfall_behavior::crafting::recipe_hash`).
//
// The 20 canonical keys (`classicRecipeKeys`, `PotionRecipe.cs:28-29`) are
// reproduced exactly by folding each recipe's sorted ids — see
// [`CLASSIC_RECIPE_KEYS`] and the GV-RCP-13 test. Only `purification`
// overflows `2^31` and needs the `int` wrap.
//
// Brewing (`§3.3`, `DaggerfallPotionMakerWindow.cs:314-317`): filter the
// cauldron to `is_ingredient` items, take their `TemplateIndex`, hash the
// sorted ids and look the key up. The effect-key contract (§3.4) is enforced
// by the ENCHANT-001 probe against M23's registry; this module only stores the
// key each recipe references. Pure: 0 I/O.

use daggerfall_behavior::crafting::recipe_hash;
use daggerfall_behavior::magic::EffectSettings;

/// Default `TextureRecord` (archive 205) for a potion
/// (`PotionRecipe.cs:34,61-68`).
pub const RECIPE_TEXTURE_DEFAULT: i32 = 11;

/// Number of classic recipes (§3.2).
pub const CLASSIC_RECIPE_COUNT: usize = 20;

/// The 20 canonical recipe keys, in table order (`classicRecipeKeys`,
/// `PotionRecipe.cs:28-29`).
pub const CLASSIC_RECIPE_KEYS: [i32; CLASSIC_RECIPE_COUNT] = [
    221871, 239524, 4975678, 5017404, 5188896, 111516185, 4826108, 216843, 224588, 220192, 240081,
    4937012, 228890, 221117, 4870452, 5361377, 112080144, 4842851, 4815872, 2031019196,
];

/// One classic potion recipe.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClassicRecipe {
    /// `DisplayNameKey` (`PotionRecipe.cs:48-54`).
    pub display_name_key: &'static str,
    /// `Price` in gold (`:59`).
    pub price: i32,
    /// The M23 effect key this recipe references (§3.4). For
    /// `ElementalResistance` this is the (class, variant) composite key.
    pub effect_key: &'static str,
    /// Variant discriminator for the effect (`"Fire"`, `"Frost"`, `"Shock"`,
    /// `"Poison"`; `""` when the recipe maps to a bare effect key).
    pub variant: &'static str,
    /// Ingredient template ids (any order; the hash sorts them).
    pub ingredients: &'static [i32],
    /// `TextureRecord` override (default [`RECIPE_TEXTURE_DEFAULT`]).
    pub texture_record: i32,
    /// `EffectSettings` (magnitude/duration/chance, `PotionRecipe.cs:73`).
    pub settings: EffectSettings,
    /// `SecondaryEffects` (`AddSecondaryEffect`, e.g. purification).
    pub secondary_effects: &'static [&'static str],
}

impl ClassicRecipe {
    /// The recipe key = hash of the sorted ingredient ids.
    pub fn key(&self) -> i32 {
        recipe_hash(self.ingredients)
    }
}

/// `EffectSettings` helper: magnitude `(base_min, base_max, plus_min,
/// plus_max, per_level)` over the classic defaults.
const fn mag(
    base_min: i32,
    base_max: i32,
    plus_min: i32,
    plus_max: i32,
    per_level: i32,
) -> EffectSettings {
    EffectSettings {
        magnitude_base_min: base_min,
        magnitude_base_max: base_max,
        magnitude_plus_min: plus_min,
        magnitude_plus_max: plus_max,
        magnitude_per_level: per_level,
        ..EffectSettings::classic_default()
    }
}

/// `EffectSettings` helper: chance `(base, plus, per_level)` over the classic
/// defaults (`SetEffectChance`, `EntityEffect.cs:979`).
const fn chance(base: i32, plus: i32, per_level: i32) -> EffectSettings {
    EffectSettings {
        chance_base: base,
        chance_plus: plus,
        chance_per_level: per_level,
        ..EffectSettings::classic_default()
    }
}

/// The 20 classic recipes (§3.2), in `classicRecipeKeys` order. Ingredient
/// ids come from each effect's `SetPotionProperties()`; hashes verified against
/// [`CLASSIC_RECIPE_KEYS`].
pub const CLASSIC_RECIPES: [ClassicRecipe; CLASSIC_RECIPE_COUNT] = [
    // 1 · stamina (HealFatigue.cs:46) — Pure_water 59, Aloe 30, Ginkgo 27.
    ClassicRecipe {
        display_name_key: "stamina",
        price: 25,
        effect_key: "Heal-Fatigue",
        variant: "",
        ingredients: &[59, 30, 27],
        texture_record: RECIPE_TEXTURE_DEFAULT,
        settings: mag(5, 5, 4, 4, 1),
        secondary_effects: &[],
    },
    // 2 · orcStrength (FortifyStrength.cs:48) — Orcs_blood 61, Iron 71, Pure_water 59.
    ClassicRecipe {
        display_name_key: "orcStrength",
        price: 50,
        effect_key: "Fortify-Strength",
        variant: "",
        ingredients: &[61, 71, 59],
        texture_record: 13,
        settings: mag(1, 1, 14, 14, 1),
        secondary_effects: &[],
    },
    // 3 · healing (HealHealth.cs:46) — Elixir 62, Red_berries 16, Mercury 65, Troll_blood 42.
    ClassicRecipe {
        display_name_key: "healing",
        price: 50,
        effect_key: "Heal-Health",
        variant: "",
        ingredients: &[62, 16, 65, 42],
        texture_record: 15,
        settings: mag(5, 5, 9, 9, 1),
        secondary_effects: &[],
    },
    // 4 · waterWalking (WaterWalking.cs:43) — Pure_water 59, Palm 29, Yellow_rose 20, Sulphur 69.
    ClassicRecipe {
        display_name_key: "waterWalking",
        price: 50,
        effect_key: "WaterWalking",
        variant: "",
        ingredients: &[59, 29, 20, 69],
        texture_record: 32,
        settings: EffectSettings::classic_default(),
        secondary_effects: &[],
    },
    // 5 · restorePower (HealSpellPoints.cs:39) — Nectar 63, Silver 73, Werewolfs_blood 33, Saints_hair 54.
    ClassicRecipe {
        display_name_key: "restorePower",
        price: 75,
        effect_key: "Heal-SpellPoints",
        variant: "",
        ingredients: &[63, 73, 33, 54],
        texture_record: 12,
        settings: mag(5, 5, 4, 4, 1),
        secondary_effects: &[],
    },
    // 6 · resistFire (ElementalResistance.cs:100) — Ichor 64, Amber 7, Red_flowers 10, Fairy_dragon 35, Cactus 32.
    ClassicRecipe {
        display_name_key: "resistFire",
        price: 75,
        effect_key: "ElementalResistance-Fire",
        variant: "Fire",
        ingredients: &[64, 7, 10, 35, 32],
        texture_record: 34,
        settings: chance(100, 1, 1),
        secondary_effects: &[],
    },
    // 7 · resistFrost (ElementalResistance.cs:110) — Ichor 64, Turquoise 5, Pine_branch 14, White_rose 22.
    ClassicRecipe {
        display_name_key: "resistFrost",
        price: 75,
        effect_key: "ElementalResistance-Frost",
        variant: "Frost",
        ingredients: &[64, 5, 14, 22],
        texture_record: 34,
        settings: chance(100, 1, 1),
        secondary_effects: &[],
    },
    // 8 · resistShock (ElementalResistance.cs:119) — Ichor 64, Lodestone 68, Red_berries 16.
    ClassicRecipe {
        display_name_key: "resistShock",
        price: 75,
        effect_key: "ElementalResistance-Shock",
        variant: "Shock",
        ingredients: &[64, 68, 16],
        texture_record: 34,
        settings: chance(100, 1, 1),
        secondary_effects: &[],
    },
    // 9 · cureDisease (CureDisease.cs:45) — Elixir 62, Fig 31, Big_tooth 56.
    ClassicRecipe {
        display_name_key: "cureDisease",
        price: 100,
        effect_key: "Cure-Disease",
        variant: "",
        ingredients: &[62, 31, 56],
        texture_record: 35,
        settings: chance(1, 10, 1),
        secondary_effects: &[],
    },
    // 10 · slowFalling (Slowfall.cs:45) — Pure_water 59, White_poppy 26, Black_poppy 24.
    ClassicRecipe {
        display_name_key: "slowFalling",
        price: 100,
        effect_key: "Slowfall",
        variant: "",
        ingredients: &[59, 26, 24],
        texture_record: RECIPE_TEXTURE_DEFAULT,
        settings: EffectSettings::classic_default(),
        secondary_effects: &[],
    },
    // 11 · waterBreathing (WaterBreathing.cs:43) — Rain_water 60, Elixir 62, Ivory 76.
    ClassicRecipe {
        display_name_key: "waterBreathing",
        price: 100,
        effect_key: "WaterBreathing",
        variant: "",
        ingredients: &[60, 62, 76],
        texture_record: 32,
        settings: EffectSettings::classic_default(),
        secondary_effects: &[],
    },
    // 12 · healTrue (HealHealth.cs:57) — Elixir 62, Red_berries 16, Pine_branch 14, Unicorn_horn 37.
    ClassicRecipe {
        display_name_key: "healTrue",
        price: 100,
        effect_key: "Heal-Health",
        variant: "",
        ingredients: &[62, 16, 14, 37],
        texture_record: 16,
        settings: mag(5, 5, 19, 19, 1),
        secondary_effects: &[],
    },
    // 13 · levitation (Levitate.cs:46) — Pure_water 59, Nectar 63, Ectoplasm 39.
    ClassicRecipe {
        display_name_key: "levitation",
        price: 125,
        effect_key: "Levitate",
        variant: "",
        ingredients: &[59, 63, 39],
        texture_record: RECIPE_TEXTURE_DEFAULT,
        settings: EffectSettings::classic_default(),
        secondary_effects: &[],
    },
    // 14 · resistPoison (ElementalResistance.cs:128) — Ichor 64, Snake_venom 43, Golden_poppy 25.
    ClassicRecipe {
        display_name_key: "resistPoison",
        price: 125,
        effect_key: "ElementalResistance-Poison",
        variant: "Poison",
        ingredients: &[64, 43, 25],
        texture_record: 14,
        settings: chance(5, 19, 1),
        secondary_effects: &[],
    },
    // 15 · freeAction (FreeAction.cs:44) — Ichor 64, Spider_venom 41, Twigs 8, Bamboo 28.
    ClassicRecipe {
        display_name_key: "freeAction",
        price: 125,
        effect_key: "FreeAction",
        variant: "",
        ingredients: &[64, 41, 8, 28],
        texture_record: 14,
        settings: chance(5, 19, 1),
        secondary_effects: &[],
    },
    // 16 · curePoison (CurePoison.cs:45) — Ichor 64, Giant_scorpion 47, Small_tooth 58, Pearl 77.
    ClassicRecipe {
        display_name_key: "curePoison",
        price: 200,
        effect_key: "Cure-Poison",
        variant: "",
        ingredients: &[64, 47, 58, 77],
        texture_record: 35,
        settings: chance(5, 19, 1),
        secondary_effects: &[],
    },
    // 17 · chameleonForm (ChameleonNormal.cs:47) — Rain 60, Nectar 63, Green_leaves 9, Yellow_flowers 11, Red_berries 16.
    ClassicRecipe {
        display_name_key: "chameleonForm",
        price: 200,
        effect_key: "Chameleon-Normal",
        variant: "",
        ingredients: &[60, 63, 9, 11, 16],
        texture_record: 33,
        settings: EffectSettings::classic_default(),
        secondary_effects: &[],
    },
    // 18 · shadowForm (ShadowNormal.cs:47) — Rain 60, Nectar 63, Malachite 6, Black_rose 21.
    ClassicRecipe {
        display_name_key: "shadowForm",
        price: 200,
        effect_key: "Shadow-Normal",
        variant: "",
        ingredients: &[60, 63, 6, 21],
        texture_record: 33,
        settings: EffectSettings::classic_default(),
        secondary_effects: &[],
    },
    // 19 · invisibility (InvisibilityNormal.cs:47) — Rain 60, Nectar 63, Ectoplasm 39, Diamond 3.
    ClassicRecipe {
        display_name_key: "invisibility",
        price: 250,
        effect_key: "Invisibility-Normal",
        variant: "",
        ingredients: &[60, 63, 39, 3],
        texture_record: 33,
        settings: EffectSettings::classic_default(),
        secondary_effects: &[],
    },
    // 20 · purification (CureDisease.cs:55) — Elixir 62, Nectar 63, Rain 60, Fig 31,
    // Big_tooth 56, Ectoplasm 39, Diamond 3, Mummy_wrappings 49. Overflows 2^31.
    ClassicRecipe {
        display_name_key: "purification",
        price: 500,
        effect_key: "Cure-Disease",
        variant: "",
        ingredients: &[62, 63, 60, 31, 56, 39, 3, 49],
        texture_record: 35,
        settings: {
            let base_settings = chance(1, 10, 1);
            EffectSettings {
                magnitude_base_min: 5,
                magnitude_base_max: 5,
                magnitude_plus_min: 19,
                magnitude_plus_max: 19,
                magnitude_per_level: 1,
                ..base_settings
            }
        },
        secondary_effects: &["Heal-Health", "Invisibility-Normal"],
    },
];

/// A brewed potion: the resolved recipe plus the vial price.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Potion {
    /// The recipe that matched.
    pub recipe: &'static ClassicRecipe,
    /// The vial's gold price (`recipe.price`).
    pub price: i32,
}

/// Looks a recipe up by its key (the broker's `GetPotionRecipe`,
/// `EntityEffectBroker.cs:461-470`).
pub fn find_recipe_by_key(key: i32) -> Option<&'static ClassicRecipe> {
    CLASSIC_RECIPES.iter().find(|recipe| recipe.key() == key)
}

/// Brews from a set of cauldron ingredient ids
/// (`DaggerfallPotionMakerWindow.cs:314-317`): hash the sorted ids, look the
/// key up. `None` if no classic recipe matches.
pub fn brew(cauldron_ids: &[i32]) -> Option<Potion> {
    let key = recipe_hash(cauldron_ids);
    find_recipe_by_key(key).map(|recipe| Potion {
        recipe,
        price: recipe.price,
    })
}

/// Outcome of drinking a brewed potion: the resolved M23 effect key and its
/// rolled magnitude. A bottled potion only *declares* its `EffectSettings`
/// (§4.3 — the effect application is M23).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DrinkOutcome {
    /// The M23 effect key this potion resolves to (§3.4).
    pub effect_key: &'static str,
    /// Variant discriminator (`""` for bare keys).
    pub variant: &'static str,
    /// Rolled magnitude: `base_roll + plus_roll · max(1, level / per_level)`.
    pub magnitude: i32,
}

/// Rolls a magnitude from a recipe's settings at a caster `level`, with the
/// two dice rolls injected (`base_roll` ∈ `[magnitude_base_min,
/// magnitude_base_max]`, `plus_roll` ∈ `[magnitude_plus_min,
/// magnitude_plus_max]`; the plus term scales by `max(1, level /
/// magnitude_per_level)`). For `stamina` (`mag(5,5,4,4,1)`) at level 1 with
/// rolls pinned to their minima: `5 + 4·1 = 9`.
pub fn drink_magnitude(settings: &EffectSettings, level: i32, base_roll: i32, plus_roll: i32) -> i32 {
    let mut multiplier = level / settings.magnitude_per_level;
    if multiplier < 1 {
        multiplier = 1;
    }
    base_roll + plus_roll * multiplier
}

/// Drinks a brewed potion: resolves its effect key and rolls the magnitude
/// from the recipe's settings (rolls injected by the caller). The effect key's
/// presence in M23 is the ENCHANT-001 contract gate (§3.4), checked there.
pub fn drink(potion: &Potion, level: i32, base_roll: i32, plus_roll: i32) -> DrinkOutcome {
    DrinkOutcome {
        effect_key: potion.recipe.effect_key,
        variant: potion.recipe.variant,
        magnitude: drink_magnitude(&potion.recipe.settings, level, base_roll, plus_roll),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// GV-RCP-13: every recipe's sorted-id fold reproduces its canonical key
    /// (`classicRecipeKeys`, 20/20); only purification wraps `2^31`.
    #[test]
    fn gv_rcp_13_all_20_keys_match() {
        for (index, recipe) in CLASSIC_RECIPES.iter().enumerate() {
            assert_eq!(
                recipe.key(),
                CLASSIC_RECIPE_KEYS[index],
                "recipe #{} ({}) key mismatch",
                index + 1,
                recipe.display_name_key
            );
        }
    }

    /// GV-POT-06/07: the 20 prices sum to 2600 and average 130.
    #[test]
    fn gv_pot_06_07_price_stats() {
        let total: i32 = CLASSIC_RECIPES.iter().map(|r| r.price).sum();
        assert_eq!(total, 2600);
        assert_eq!(total / CLASSIC_RECIPE_COUNT as i32, 130);
    }

    /// GV-POT-01..05: exact prices for five named recipes.
    #[test]
    fn gv_pot_prices() {
        let price = |name: &str| {
            CLASSIC_RECIPES
                .iter()
                .find(|r| r.display_name_key == name)
                .unwrap()
                .price
        };
        assert_eq!(price("stamina"), 25);
        assert_eq!(price("healing"), 50);
        assert_eq!(price("purification"), 500);
        assert_eq!(price("invisibility"), 250);
        assert_eq!(price("resistPoison"), 125);
    }

    /// GV-POT-08/09: 5 recipes priced ≥ 200; 4 priced ≤ 50.
    #[test]
    fn gv_pot_08_09_price_buckets() {
        let ge200 = CLASSIC_RECIPES.iter().filter(|r| r.price >= 200).count();
        let le50 = CLASSIC_RECIPES.iter().filter(|r| r.price <= 50).count();
        assert_eq!(ge200, 5);
        assert_eq!(le50, 4);
    }

    /// GV-POT-10: the default texture record is 11; overridden recipes carry
    /// their measured override.
    #[test]
    fn gv_pot_10_textures() {
        assert_eq!(RECIPE_TEXTURE_DEFAULT, 11);
        let texture = |name: &str| {
            CLASSIC_RECIPES
                .iter()
                .find(|r| r.display_name_key == name)
                .unwrap()
                .texture_record
        };
        assert_eq!(texture("stamina"), 11);
        assert_eq!(texture("waterBreathing"), 32);
        assert_eq!(texture("healing"), 15);
        assert_eq!(texture("purification"), 35);
    }

    /// Demo §6.3: the 3 stamina ingredients in any order brew `stamina`
    /// (key 221871, 25 gp); a 2-ingredient subset brews nothing.
    #[test]
    fn brew_stamina_any_order() {
        assert_eq!(recipe_hash(&[59, 30, 27]), 221871);
        let potion = brew(&[27, 59, 30]).expect("stamina brews");
        assert_eq!(potion.recipe.display_name_key, "stamina");
        assert_eq!(potion.price, 25);
        assert!(brew(&[27, 30]).is_none());
    }
}
