// Item enchanting — SPEC M24 §2.
//
// Transcribed from Daggerfall Unity `@0038a1e61999ad2c73fb1f33b706cae9a88ec66f`
// (MIT):
//
// - `EnchantmentSettings` shape: `MagicAndEffectsStructs.cs:163-173`
// - real `EnchantCost` per effect family: `Effects/Enchanting/*.cs` (§2.3)
// - power/side-effect split by the SIGN of `EnchantCost`:
//   `DaggerfallItemMakerWindow.cs:256-277`
// - point/gold accounting and the three gates: `:229-237`, `:733-746`, `:892-896`
//
// The power budget formula (`base + floor(base · mult)` per material) lives in
// `daggerfall_behavior::crafting`; this module owns the *selection* accounting:
// which entries are powers vs side-effects, their point and gold cost, and the
// `NotEnoughGold` / `BeyondLimit` / `NoRoomInItem` gates. Pure: 0 I/O.

/// Maximum enchantment slots per item (`DaggerfallItemMakerWindow.cs:629`,
/// the `== 10` guard; `:892` overflows with forced entries).
pub const MAX_ENCHANT_SLOTS: usize = 10;

/// Gold cost multiplier: the power list costs `points · 10`
/// (`DaggerfallItemMakerWindow.cs:234-237`).
pub const GOLD_PER_POINT: i32 = 10;

/// One registry entry: the real `(EffectKey, ClassicType, ClassicParam,
/// EnchantCost, ParentEnchantment)` of a daggerfall enchanting effect
/// (`EnchantmentSettings`, `MagicAndEffectsStructs.cs:163-173`).
///
/// `enchant_cost > 0` marks a **power**; `enchant_cost <= 0` marks a
/// **side-effect** (`DaggerfallItemMakerWindow.cs:256-277`). Only powers are
/// charged gold; both lists consume points.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EnchantmentSettings {
    /// `EffectKey` = `EnchantmentTypes.ToString()` (`Effects/Enchanting/*.cs`).
    pub effect_key: &'static str,
    /// `ClassicType` — the `EnchantmentTypes` value (`ItemsFile.cs:111-141`).
    pub classic_type: i32,
    /// `ClassicParam` — spell/variant/skill binding; `0` for the unparametrized
    /// families (the stored on-item byte, cf. GV-ENC demo `(11,0),(8,0)`).
    pub classic_param: i32,
    /// `EnchantCost` — the sign classifies power vs side-effect (§2.3).
    pub enchant_cost: i32,
    /// `ParentEnchantment` — `0` = free; non-zero = forced by another
    /// enchantment (excluded from points, still counted in gold for powers).
    pub parent_enchantment: i32,
}

/// ExtraSpellPts variant costs (`ExtraSpellPts.cs:214-227`, §2.3): 11 forms —
/// 4 seasons (500), 3 moon phases (200), then Undead 700 / Daedra 800 /
/// Humanoids 900 / Animals 1000. The `classic_param` is the variant ordinal;
/// only `enchant_cost` is golden-locked.
const EXTRA_SPELL_PTS: [(&str, i32, i32); 11] = [
    ("ExtraSpellPts.Spring", 500, 0),
    ("ExtraSpellPts.Summer", 500, 1),
    ("ExtraSpellPts.Fall", 500, 2),
    ("ExtraSpellPts.Winter", 500, 3),
    ("ExtraSpellPts.Waxing", 200, 4),
    ("ExtraSpellPts.Waning", 200, 5),
    ("ExtraSpellPts.New", 200, 6),
    ("ExtraSpellPts.Undead", 700, 7),
    ("ExtraSpellPts.Daedra", 800, 8),
    ("ExtraSpellPts.Humanoids", 900, 9),
    ("ExtraSpellPts.Animals", 1000, 10),
];

/// `EnchantmentTypes` values used by this registry (`ItemsFile.cs:111-141`).
pub mod ench_types {
    /// `ExtraSpellPts = 3`.
    pub const EXTRA_SPELL_PTS: i32 = 3;
    /// `RepairsObjects = 8`.
    pub const REPAIRS_OBJECTS: i32 = 8;
    /// `AbsorbsSpells = 9`.
    pub const ABSORBS_SPELLS: i32 = 9;
    /// `EnhancesSkill = 10`.
    pub const ENHANCES_SKILL: i32 = 10;
    /// `FeatherWeight = 11`.
    pub const FEATHER_WEIGHT: i32 = 11;
    /// `StrengthensArmor = 12`.
    pub const STRENGTHENS_ARMOR: i32 = 12;
    /// `ExtraWeight = 23` (secondary).
    pub const EXTRA_WEIGHT: i32 = 23;
    /// `WeakensArmor = 24` (secondary).
    pub const WEAKENS_ARMOR: i32 = 24;
}

/// Static registry of the real enchanting effects (§2.3). The six positive
/// families are powers (`EnchantCost > 0`); `ExtraWeight`/`WeakensArmor` are
/// the negative side-effects. ExtraSpellPts' 11 variants are appended from
/// [`EXTRA_SPELL_PTS`].
pub const ENCHANTMENT_REGISTRY: &[EnchantmentSettings] = &[
    // ── powers (EnchantCost > 0) ──────────────────────────────────────────
    EnchantmentSettings {
        effect_key: "FeatherWeight",
        classic_type: ench_types::FEATHER_WEIGHT,
        classic_param: 0,
        enchant_cost: 100,
        parent_enchantment: 0,
    },
    EnchantmentSettings {
        effect_key: "StrengthensArmor",
        classic_type: ench_types::STRENGTHENS_ARMOR,
        classic_param: 0,
        enchant_cost: 700,
        parent_enchantment: 0,
    },
    EnchantmentSettings {
        effect_key: "EnhancesSkill",
        classic_type: ench_types::ENHANCES_SKILL,
        classic_param: 0,
        enchant_cost: 900,
        parent_enchantment: 0,
    },
    EnchantmentSettings {
        effect_key: "RepairsObjects",
        classic_type: ench_types::REPAIRS_OBJECTS,
        classic_param: 0,
        enchant_cost: 900,
        parent_enchantment: 0,
    },
    EnchantmentSettings {
        effect_key: "AbsorbsSpells",
        classic_type: ench_types::ABSORBS_SPELLS,
        classic_param: 0,
        enchant_cost: 1500,
        parent_enchantment: 0,
    },
    // ── side-effects (EnchantCost <= 0; no gold cost) ─────────────────────
    EnchantmentSettings {
        effect_key: "ExtraWeight",
        classic_type: ench_types::EXTRA_WEIGHT,
        classic_param: 0,
        enchant_cost: -100,
        parent_enchantment: 0,
    },
    EnchantmentSettings {
        effect_key: "WeakensArmor",
        classic_type: ench_types::WEAKENS_ARMOR,
        classic_param: 0,
        enchant_cost: -700,
        parent_enchantment: 0,
    },
];

/// The ExtraSpellPts variants as registry entries (`EnchantCost` per §2.3).
///
/// Returns the 11 `(key, type, param, cost)` tuples with `ClassicType =
/// ExtraSpellPts (3)`; callers that need full [`EnchantmentSettings`] use
/// [`extra_spell_pts_settings`].
pub const fn extra_spell_pts_variants() -> [(&'static str, i32, i32, i32); 11] {
    let mut out = [("", 0, 0, 0); 11];
    let mut i = 0;
    while i < EXTRA_SPELL_PTS.len() {
        let (key, cost, param) = EXTRA_SPELL_PTS[i];
        out[i] = (key, ench_types::EXTRA_SPELL_PTS, param, cost);
        i += 1;
    }
    out
}

/// The 11 ExtraSpellPts variants as [`EnchantmentSettings`] (all powers).
pub fn extra_spell_pts_settings() -> Vec<EnchantmentSettings> {
    extra_spell_pts_variants()
        .into_iter()
        .map(|(effect_key, classic_type, classic_param, enchant_cost)| {
            EnchantmentSettings {
                effect_key,
                classic_type,
                classic_param,
                enchant_cost,
                parent_enchantment: 0,
            }
        })
        .collect()
}

/// Resolves an effect by `(key, param)` against the registry plus the 11
/// ExtraSpellPts variants.
///
/// The unparametrized families match on `key` alone (their `classic_param` is
/// a stored `0`, not a discriminator). ExtraSpellPts is the parametrized
/// family: it requires an exact `(key, param)` match.
pub fn find_enchantment(effect_key: &str, classic_param: i32) -> Option<EnchantmentSettings> {
    if effect_key == "ExtraSpellPts" {
        return extra_spell_pts_settings().into_iter().find(|entry| {
            entry.effect_key == "ExtraSpellPts" && entry.classic_param == classic_param
        });
    }
    ENCHANTMENT_REGISTRY
        .iter()
        .find(|entry| entry.effect_key == effect_key)
        .copied()
}

/// Convenience: resolve an unparametrized effect by key (param `0`).
pub fn effect(effect_key: &str) -> Option<EnchantmentSettings> {
    find_enchantment(effect_key, 0)
}

/// Resolves a named ExtraSpellPts variant, e.g. `"Animals"`
/// (`EnchantCost = 1000`). Accepts the bare variant name (`"Animals"`) or the
/// dotted key (`"ExtraSpellPts.Animals"`).
pub fn extra_spell_pts(variant: &str) -> Option<EnchantmentSettings> {
    let wanted = variant.strip_prefix("ExtraSpellPts.").unwrap_or(variant);
    let dotted = format!("ExtraSpellPts.{wanted}");
    extra_spell_pts_settings()
        .into_iter()
        .find(|entry| entry.effect_key == dotted)
}

/// Is this entry a **power**? (`EnchantCost > 0`, `:256-277`.)
pub const fn is_power(entry: &EnchantmentSettings) -> bool {
    entry.enchant_cost > 0
}

/// Total enchantment points of a selection: `Σ powers + Σ side-effects`,
/// both restricted to `parent == 0` (`:229-232`). Side-effects are negative
/// and therefore *subtract* points (GV-ENC-12).
pub fn cost_points(selection: &[EnchantmentSettings]) -> i32 {
    selection
        .iter()
        .filter(|entry| entry.parent_enchantment == 0)
        .map(|entry| entry.enchant_cost)
        .sum()
}

/// Total gold cost of a selection: `Σ powers (ALL, incl. forced) · 10`
/// (`:234-237`). Side-effects never cost gold.
pub fn cost_gold(selection: &[EnchantmentSettings]) -> i32 {
    let powers: i32 = selection
        .iter()
        .filter(|entry| is_power(entry))
        .map(|entry| entry.enchant_cost)
        .sum();
    powers * GOLD_PER_POINT
}

/// Gate failure when applying an enchantment selection (§2.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnchantGate {
    /// `player.gold < cost_gold` (`:733-739`).
    NotEnoughGold,
    /// `item.enchantment_power < cost_points` (`:741-746`).
    BeyondLimit,
    /// `occupied + forced + selection + 1 > 10` (`:892-896`); the 11th slot is
    /// also blocked at `== 10` (`:629`, `:674`).
    NoRoomInItem,
}

impl core::fmt::Display for EnchantGate {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            EnchantGate::NotEnoughGold => write!(f, "notEnoughGold"),
            EnchantGate::BeyondLimit => write!(f, "beyondLimit"),
            EnchantGate::NoRoomInItem => write!(f, "noRoomInItem"),
        }
    }
}

/// Request to enchant an item: its power budget, the player's gold, the slots
/// already occupied on the item, and the chosen enchantments.
#[derive(Debug, Clone, Copy)]
pub struct EnchantRequest<'a> {
    /// `item.GetEnchantmentPower()` — the material-scaled budget (§2.1).
    pub item_power: i32,
    /// Player's current gold.
    pub gold: i32,
    /// Slots already filled on the item (before this selection).
    pub occupied_slots: usize,
    /// Chosen enchantments, in list order.
    pub selection: &'a [EnchantmentSettings],
}

/// Successful application: the charged cost plus the resulting item state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnchantedItem {
    /// Point cost of the selection (`cost_points`).
    pub cost_points: i32,
    /// Gold cost of the selection (`cost_gold`).
    pub cost_gold: i32,
    /// `gold - cost_gold`.
    pub gold_after: i32,
    /// Slots occupied after applying (`occupied_slots + selection.len()`).
    pub occupied_slots: usize,
    /// `(ClassicType, ClassicParam)` pairs in list order — the item's
    /// `legacyMagic` (`item.set_enchantments(powers ++ side)`, `:757-760`).
    pub legacy_magic: Vec<(i32, i32)>,
}

/// Is the 11th slot free? (`occupied < 10`, the `== 10` guard at `:629`/`:674`.)
pub const fn slot_available(occupied_slots: usize) -> bool {
    occupied_slots < MAX_ENCHANT_SLOTS
}

/// Applies an enchantment selection, enforcing all three gates in source
/// order — slot (`:892`), gold (`:733`), power (`:741`). Strict comparisons:
/// *equaling* the budget or the gold is valid (`<`, never `<=`). On success the
/// gold is deducted and the `(ClassicType, ClassicParam)` pairs are returned
/// as the item's `legacyMagic`.
pub fn apply_enchantments(request: &EnchantRequest<'_>) -> Result<EnchantedItem, EnchantGate> {
    let total_slots = request.occupied_slots
        + request
            .selection
            .iter()
            .filter(|entry| entry.parent_enchantment != 0)
            .count()
        + request.selection.len();
    if total_slots + 1 > MAX_ENCHANT_SLOTS {
        return Err(EnchantGate::NoRoomInItem);
    }
    let points = cost_points(request.selection);
    let gold = cost_gold(request.selection);
    if request.gold < gold {
        return Err(EnchantGate::NotEnoughGold);
    }
    if request.item_power < points {
        return Err(EnchantGate::BeyondLimit);
    }
    let legacy_magic = request
        .selection
        .iter()
        .map(|entry| (entry.classic_type, entry.classic_param))
        .collect();
    Ok(EnchantedItem {
        cost_points: points,
        cost_gold: gold,
        gold_after: request.gold - gold,
        occupied_slots: request.occupied_slots + request.selection.len(),
        legacy_magic,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The six positive families are powers; the two negative ones are
    /// side-effects (sign rule, §2.4).
    #[test]
    fn classification_by_sign() {
        for key in [
            "FeatherWeight",
            "StrengthensArmor",
            "EnhancesSkill",
            "RepairsObjects",
            "AbsorbsSpells",
        ] {
            assert!(is_power(&effect(key).expect(key)), "{key} must be a power");
        }
        for key in ["ExtraWeight", "WeakensArmor"] {
            assert!(!is_power(&effect(key).expect(key)), "{key} must be a side-effect");
        }
    }

    /// GV-ENC-12: EnhancesSkill 900 (power) + ExtraWeight −100 (side) ⇒
    /// 800 points / 9000 gold — the side-effect subtracts points and costs no
    /// gold. This is the discriminating case for the sign rule.
    #[test]
    fn gv_enc_12_side_effect_subtracts() {
        let sel = [effect("EnhancesSkill").unwrap(), effect("ExtraWeight").unwrap()];
        assert_eq!(cost_points(&sel), 800);
        assert_eq!(cost_gold(&sel), 9000);
    }

    /// ExtraSpellPts variants resolve by name with the exact §2.3 costs.
    #[test]
    fn extra_spell_pts_variants_resolve() {
        assert_eq!(extra_spell_pts("Animals").unwrap().enchant_cost, 1000);
        assert_eq!(extra_spell_pts("Undead").unwrap().enchant_cost, 700);
        assert_eq!(extra_spell_pts("Spring").unwrap().enchant_cost, 500);
        assert_eq!(extra_spell_pts("Waxing").unwrap().enchant_cost, 200);
        assert_eq!(extra_spell_pts_settings().len(), 11);
    }
}
