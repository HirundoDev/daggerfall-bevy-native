//! M23 (REQ-L99-056) — **magic**:
//! effect model (classic key, school, variants, rounds), the **two**
//! cost routes of §3, spellbook, spellmaker, cast animation, and the
//! active-rounds engine. Pure: 0 Bevy, 0 I/O, 0 global RNG (the probability
//! roll enters **injected** via [`crate::combat::success_roll`], the
//! PRNG lives in `daggerfall-combat`).
//!
//! Spec: `docs/sintergic/game/programming/m23-magic-spec-CR-5243af63.md`
//! (§1 SPELLS.STD → [`daggerfall_parser_rs::spells_std`]; §2 model; §3 the
//! two cost routes; §4 casting; §5 spellbook/spellmaker; §6 NEGATIVE).
//! Every constant cites `file:line@0038a1e6` (DFU MIT, executable spec).
//!
//! **NEGATIVE §6 N-1:** only the **17** reference effects are recorded
//! (the 5 from §3.1 + FreeAction from patch N-5 + the remaining 11 from the
//! M24 key contract §3.4) + the **5** `ElementalResistance` variants
//! (§2 VariantCount > 1) = **22 templates**; the semantics of the 153 effects
//! are not invented. `spell.cost` takes part in no route (§6 N-4).
//! `EffectCosts.Factor` is not applied (§6 N-6, `MagicAndEffectsStructs.cs:61`).

use crate::combat::success_roll;

// ── §2 · effect model ───────────────────────────────────────────────────────

/// Spell point floor (`FormulaHelper.cs:2193`, `:2223-2224`).
pub const CAST_COST_FLOOR: i32 = 5;

/// `EffectSettings` (`MagicAndEffectsStructs.cs:81-96`): duration, chance and
/// magnitude of one effect instance (spec §2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct EffectSettings {
    /// Base duration (rounds).
    pub duration_base: i32,
    /// Additional duration per level step.
    pub duration_plus: i32,
    /// Level per duration step.
    pub duration_per_level: i32,
    /// Base chance (%).
    pub chance_base: i32,
    /// Additional chance per level step.
    pub chance_plus: i32,
    /// Level per chance step.
    pub chance_per_level: i32,
    /// Minimum base magnitude.
    pub magnitude_base_min: i32,
    /// Maximum base magnitude.
    pub magnitude_base_max: i32,
    /// Minimum additional magnitude per level.
    pub magnitude_plus_min: i32,
    /// Maximum additional magnitude per level.
    pub magnitude_plus_max: i32,
    /// Level per magnitude step.
    pub magnitude_per_level: i32,
}

impl EffectSettings {
    /// Default settings of `BaseEntityEffect.DefaultEffectSettings()`
    /// (`EntityEffect.cs:946-968`): duration 1+1/1, chance 1+1/1,
    /// magnitude 1-1 + 1-1/1.
    pub const fn classic_default() -> Self {
        Self {
            duration_base: 1,
            duration_plus: 1,
            duration_per_level: 1,
            chance_base: 1,
            chance_plus: 1,
            chance_per_level: 1,
            magnitude_base_min: 1,
            magnitude_base_max: 1,
            magnitude_plus_min: 1,
            magnitude_plus_max: 1,
            magnitude_per_level: 1,
        }
    }
}

/// Caster level: `caster.Level` or **1** when there is no caster
/// (`FormulaHelper.cs:850-857`).
pub fn caster_level(level: Option<i32>) -> i32 {
    level.unwrap_or(1)
}

/// Rounds an effect lasts: `base + plus · max(1, ⌊level / perLevel⌋)` when
/// `support_duration`, otherwise **0** (`EntityEffect.cs:920-935`).
pub fn rounds(settings: &EffectSettings, support_duration: bool, level: i32) -> i32 {
    if !support_duration {
        return 0;
    }
    let mut multiplier = level / settings.duration_per_level;
    if multiplier < 1 {
        multiplier = 1;
    }
    settings.duration_base + settings.duration_plus * multiplier
}

/// Chance value: `base + plus · ⌊level / chancePerLevel⌋`
/// (`EntityEffect.cs:740-745`).
pub fn chance_value(settings: &EffectSettings, level: i32) -> i32 {
    settings.chance_base + settings.chance_plus * (level / settings.chance_per_level)
}

/// Chance roll: `Dice100.SuccessRoll(ChanceValue())` when the effect supports
/// chance, otherwise `false` (`EntityEffect.cs:751-761`); the roll enters
/// injected (`Dice100.cs:15-18`: `roll < chance`, no clamp to 100 — C6).
pub fn roll_chance(
    support_chance: bool,
    settings: &EffectSettings,
    level: i32,
    roll: i32,
) -> bool {
    support_chance && success_roll(roll, chance_value(settings, level))
}

/// `classicKey = (family << 16) + (group << 8) + subGroup`
/// (`EntityEffect.cs:999-1002`); family 0 = `ClassicEffectFamily.Spells`
/// (`EntityEffect.cs:279-283`).
pub const fn classic_key(group: u8, sub_group: u8) -> i32 {
    // Literal mirror of `MakeClassicKey`: the Spells family occupies the high
    // byte with value 0; the `0 << 16` term is kept as in C#
    // (`EntityEffect.cs:999-1002`).
    #[allow(clippy::identity_op)]
    const FAMILY_SPELLS: i32 = 0i32 << 16;
    FAMILY_SPELLS + ((group as i32) << 8) + sub_group as i32
}

/// Reversal `family = key >> 16; group = key >> 8; subGroup = key & 0xFF`
/// with byte casts (`EntityEffect.cs:1004-1009`).
pub fn reverse_classic_key(key: i32) -> (u8, u8, u8) {
    ((key >> 16) as u8, (key >> 8) as u8, key as u8)
}

/// Magic school (`DFCareer.MagicSkills`, `DFCareer.cs:506-515`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MagicSkill {
    /// No declared school (`None = -1`).
    None,
    /// `Destruction = 22`.
    Destruction,
    /// `Restoration = 23`.
    Restoration,
    /// `Illusion = 24`.
    Illusion,
    /// `Alteration = 25`.
    Alteration,
    /// `Thaumaturgy = 26`.
    Thaumaturgy,
    /// `Mysticism = 27`.
    Mysticism,
}

impl MagicSkill {
    /// Numeric index in `DFCareer.Skills` (`DFCareer.cs:506-515`); `None`
    /// does not resolve a skill.
    pub const fn career_skill_index(self) -> Option<i32> {
        match self {
            MagicSkill::None => None,
            MagicSkill::Destruction => Some(22),
            MagicSkill::Restoration => Some(23),
            MagicSkill::Illusion => Some(24),
            MagicSkill::Alteration => Some(25),
            MagicSkill::Thaumaturgy => Some(26),
            MagicSkill::Mysticism => Some(27),
        }
    }
}

/// `EffectCosts { OffsetGold, Factor, CostA, CostB }`
/// (`MagicAndEffectsStructs.cs:58-64`); `Factor` is not applied (§6 N-6).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct EffectCosts {
    /// Constant term in gold.
    pub offset_gold: i32,
    /// Cost per starting unit.
    pub cost_a: i32,
    /// Cost per increase unit.
    pub cost_b: i32,
}

/// `MakeEffectCosts(costA, costB, offsetGold = 0)`
/// (`EntityEffect.cs:1011-1019`).
pub const fn make_effect_costs(cost_a: i32, cost_b: i32, offset_gold: i32) -> EffectCosts {
    EffectCosts {
        offset_gold,
        cost_a,
        cost_b,
    }
}

/// Static properties of an effect template (`EntityEffectProperties`,
/// `MagicAndEffectsStructs.cs:28-64`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EffectProperties {
    /// `properties.Key` (code literal, e.g. `Damage-Health.cs:24`).
    pub key: &'static str,
    /// `properties.ClassicKey` (0 = does not declare; `HealSpellPoints.cs` has no line).
    pub classic_key: i32,
    /// `properties.MagicSkill` (`EntityEffect.cs:302` defaults to `None`).
    pub magic_skill: MagicSkill,
    /// `properties.SupportDuration`.
    pub support_duration: bool,
    /// `properties.SupportChance`.
    pub support_chance: bool,
    /// `properties.SupportMagnitude`.
    pub support_magnitude: bool,
    /// `properties.DurationCosts` (optional).
    pub duration_costs: Option<EffectCosts>,
    /// `properties.ChanceCosts` (optional).
    pub chance_costs: Option<EffectCosts>,
    /// `properties.MagnitudeCosts` (optional).
    pub magnitude_costs: Option<EffectCosts>,
    /// `variantCount` (`EntityEffect.cs:412`); 1 = no variants.
    pub variant_count: i32,
}

// ── reference registry (§6 N-1: 17 effects + 5 variants = 22) ──────────────

/// Reference effect registry: the 5 from §3.1 (`DamageHealth.cs:35`,
/// `HealHealth.cs:34`, `Levitate.cs:37`, `SpellResistance.cs:35-36`,
/// `Teleport.cs:48-57`), `FreeAction` (patch N-5, `EntityEffectBroker.cs:892-899`),
/// the remaining 11 from the M24 key contract §3.4 and the 5
/// `ElementalResistance` variants (`ElementalResistance.cs:25-28,73-80,83,193-194`).
/// M24 consumes these keys by name (hard edge `M23→M24`).
pub const REFERENCE_EFFECTS: &[EffectProperties] = &[
    // §3.1 · destruction/offensive
    EffectProperties {
        key: "Damage-Health",
        classic_key: classic_key(4, 0),
        magic_skill: MagicSkill::Destruction,
        support_duration: false,
        support_chance: false,
        support_magnitude: true,
        duration_costs: None,
        chance_costs: None,
        magnitude_costs: Some(make_effect_costs(20, 28, 0)),
        variant_count: 1,
    },
    EffectProperties {
        key: "Heal-Health",
        classic_key: classic_key(10, 8),
        magic_skill: MagicSkill::Restoration,
        support_duration: false,
        support_chance: false,
        support_magnitude: true,
        duration_costs: None,
        chance_costs: None,
        magnitude_costs: Some(make_effect_costs(20, 28, 0)),
        variant_count: 1,
    },
    EffectProperties {
        key: "Levitate",
        classic_key: classic_key(14, 255),
        magic_skill: MagicSkill::Thaumaturgy,
        support_duration: true,
        support_chance: false,
        support_magnitude: false,
        duration_costs: Some(make_effect_costs(60, 100, 0)),
        chance_costs: None,
        magnitude_costs: None,
        variant_count: 1,
    },
    EffectProperties {
        key: "SpellResistance",
        classic_key: classic_key(22, 255),
        magic_skill: MagicSkill::Thaumaturgy,
        support_duration: true,
        support_chance: true,
        support_magnitude: false,
        duration_costs: Some(make_effect_costs(20, 100, 0)),
        chance_costs: Some(make_effect_costs(20, 100, 0)),
        magnitude_costs: None,
        variant_count: 1,
    },
    EffectProperties {
        key: "Teleport-Effect",
        classic_key: classic_key(43, 255),
        magic_skill: MagicSkill::Mysticism,
        support_duration: false,
        support_chance: false,
        support_magnitude: false,
        duration_costs: None,
        chance_costs: None,
        magnitude_costs: None,
        variant_count: 1,
    },
    // M24 contract §3.4
    EffectProperties {
        key: "Heal-Fatigue",
        classic_key: classic_key(10, 9),
        magic_skill: MagicSkill::Restoration,
        support_duration: false,
        support_chance: false,
        support_magnitude: true,
        duration_costs: None,
        chance_costs: None,
        magnitude_costs: Some(make_effect_costs(8, 28, 0)),
        variant_count: 1,
    },
    EffectProperties {
        key: "Fortify-Strength",
        classic_key: classic_key(9, 0),
        magic_skill: MagicSkill::Restoration,
        support_duration: true,
        support_chance: false,
        support_magnitude: true,
        duration_costs: Some(make_effect_costs(28, 100, 0)),
        chance_costs: None,
        magnitude_costs: Some(make_effect_costs(40, 120, 0)),
        variant_count: 1,
    },
    EffectProperties {
        key: "Heal-SpellPoints",
        classic_key: 0,
        magic_skill: MagicSkill::None,
        support_duration: false,
        support_chance: false,
        support_magnitude: true,
        duration_costs: None,
        chance_costs: None,
        // No declared costs ⇒ default struct (0,0,0)
        // (HealSpellPoints.cs:21-26 declares no ClassicKey/MagicSkill/costs).
        magnitude_costs: Some(make_effect_costs(0, 0, 0)),
        variant_count: 1,
    },
    EffectProperties {
        key: "Cure-Disease",
        classic_key: classic_key(3, 0),
        magic_skill: MagicSkill::Restoration,
        support_duration: false,
        support_chance: true,
        support_magnitude: false,
        duration_costs: None,
        chance_costs: Some(make_effect_costs(8, 100, 0)),
        magnitude_costs: None,
        variant_count: 1,
    },
    EffectProperties {
        key: "Cure-Poison",
        classic_key: classic_key(3, 1),
        magic_skill: MagicSkill::Restoration,
        support_duration: false,
        support_chance: true,
        support_magnitude: false,
        duration_costs: None,
        chance_costs: Some(make_effect_costs(8, 100, 0)),
        magnitude_costs: None,
        variant_count: 1,
    },
    EffectProperties {
        key: "FreeAction",
        classic_key: classic_key(26, 255),
        magic_skill: MagicSkill::Restoration,
        support_duration: true,
        support_chance: false,
        support_magnitude: false,
        duration_costs: Some(make_effect_costs(20, 8, 0)),
        chance_costs: None,
        magnitude_costs: None,
        variant_count: 1,
    },
    EffectProperties {
        key: "WaterWalking",
        classic_key: classic_key(31, 255),
        magic_skill: MagicSkill::Thaumaturgy,
        support_duration: true,
        support_chance: false,
        support_magnitude: false,
        duration_costs: Some(make_effect_costs(20, 8, 0)),
        chance_costs: None,
        magnitude_costs: None,
        variant_count: 1,
    },
    EffectProperties {
        key: "WaterBreathing",
        classic_key: classic_key(30, 255),
        magic_skill: MagicSkill::Alteration,
        support_duration: true,
        support_chance: false,
        support_magnitude: false,
        duration_costs: Some(make_effect_costs(20, 8, 0)),
        chance_costs: None,
        magnitude_costs: None,
        variant_count: 1,
    },
    EffectProperties {
        key: "Slowfall",
        classic_key: classic_key(25, 255),
        magic_skill: MagicSkill::Alteration,
        support_duration: true,
        support_chance: false,
        support_magnitude: false,
        duration_costs: Some(make_effect_costs(20, 100, 0)),
        chance_costs: None,
        magnitude_costs: None,
        variant_count: 1,
    },
    EffectProperties {
        key: "Shadow-Normal",
        classic_key: classic_key(24, 0),
        magic_skill: MagicSkill::Illusion,
        support_duration: true,
        support_chance: false,
        support_magnitude: false,
        duration_costs: Some(make_effect_costs(20, 80, 0)),
        chance_costs: None,
        magnitude_costs: None,
        variant_count: 1,
    },
    EffectProperties {
        key: "Invisibility-Normal",
        classic_key: classic_key(13, 0),
        magic_skill: MagicSkill::Illusion,
        support_duration: true,
        support_chance: false,
        support_magnitude: false,
        duration_costs: Some(make_effect_costs(40, 120, 0)),
        chance_costs: None,
        magnitude_costs: None,
        variant_count: 1,
    },
    EffectProperties {
        key: "Chameleon-Normal",
        classic_key: classic_key(23, 0),
        magic_skill: MagicSkill::Illusion,
        support_duration: true,
        support_chance: false,
        support_magnitude: false,
        duration_costs: Some(make_effect_costs(20, 80, 0)),
        chance_costs: None,
        magnitude_costs: None,
        variant_count: 1,
    },
    // ElementalResistance: 5 variants, 5 classicKeys (8,0)..(8,4), no single
    // literal key (ElementalResistance.cs:193-194); variantCount 5 (:83).
    EffectProperties {
        key: "ElementalResistance-Fire",
        classic_key: classic_key(8, 0),
        magic_skill: MagicSkill::Alteration,
        support_duration: true,
        support_chance: true,
        support_magnitude: false,
        duration_costs: Some(make_effect_costs(100, 100, 0)),
        chance_costs: Some(make_effect_costs(8, 100, 0)),
        magnitude_costs: None,
        variant_count: 5,
    },
    EffectProperties {
        key: "ElementalResistance-Frost",
        classic_key: classic_key(8, 1),
        magic_skill: MagicSkill::Alteration,
        support_duration: true,
        support_chance: true,
        support_magnitude: false,
        duration_costs: Some(make_effect_costs(100, 100, 0)),
        chance_costs: Some(make_effect_costs(8, 100, 0)),
        magnitude_costs: None,
        variant_count: 5,
    },
    EffectProperties {
        key: "ElementalResistance-Poison",
        classic_key: classic_key(8, 2),
        magic_skill: MagicSkill::Alteration,
        support_duration: true,
        support_chance: true,
        support_magnitude: false,
        duration_costs: Some(make_effect_costs(100, 100, 0)),
        chance_costs: Some(make_effect_costs(8, 100, 0)),
        magnitude_costs: None,
        variant_count: 5,
    },
    EffectProperties {
        key: "ElementalResistance-Shock",
        classic_key: classic_key(8, 3),
        magic_skill: MagicSkill::Alteration,
        support_duration: true,
        support_chance: true,
        support_magnitude: false,
        duration_costs: Some(make_effect_costs(100, 100, 0)),
        chance_costs: Some(make_effect_costs(8, 100, 0)),
        magnitude_costs: None,
        variant_count: 5,
    },
    EffectProperties {
        key: "ElementalResistance-Magicka",
        classic_key: classic_key(8, 4),
        magic_skill: MagicSkill::Alteration,
        support_duration: true,
        support_chance: true,
        support_magnitude: false,
        duration_costs: Some(make_effect_costs(100, 100, 0)),
        chance_costs: Some(make_effect_costs(8, 100, 0)),
        magnitude_costs: None,
        variant_count: 5,
    },
];

/// Looks up a template by `Key` (`EntityEffectBroker.GetEffectTemplate(string)`,
/// `:630-637`).
pub fn effect_by_key(key: &str) -> Option<&'static EffectProperties> {
    REFERENCE_EFFECTS.iter().find(|effect| effect.key == key)
}

/// Looks up a template by classic key (`GetEffectTemplate(int)`, `:643-652`;
/// the runtime map `classicEffectMapping` `:788-814` does not exist
/// statically — NEGATIVE N-2, here it is built by inspecting the registry).
/// Like `MapClassicKey` (`EntityEffectBroker.cs:788-790,801`), the key **0**
/// is never mapped ("Must be an effect with classic key") and only the
/// Spells family counts.
pub fn effect_by_classic_key(key: i32) -> Option<&'static EffectProperties> {
    if key == 0 || ((key >> 16) & 0xFF) as u8 != 0 {
        return None;
    }
    REFERENCE_EFFECTS
        .iter()
        .find(|effect| effect.classic_key == key)
}

// ── classic registry (SPELLS.STD) → effect entry ───────────────────────────

/// View of one effect slot of the classic registry (equivalent to
/// `SpellRecord.EffectRecordData`, `SpellRecord.cs:86-103`; the
/// `daggerfall-parser-rs` parser converts it without a cross dependency).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ClassicEffect {
    /// Classic type (`-1` = empty slot).
    pub effect_type: i8,
    /// Classic subtype (`-1` = no subtype).
    pub sub_type: i8,
    /// `(durationBase, durationMod, durationPerLevel)`.
    pub duration: [u8; 3],
    /// `(chanceBase, chanceMod, chancePerLevel)`.
    pub chance: [u8; 3],
    /// `(magnitudeBaseLow, BaseHigh, LevelBase, LevelHigh, PerLevel)`.
    pub magnitude: [u8; 5],
}

/// Classic key of a slot: `subType < 0` becomes **255** ("entity effect
/// keys use 255 instead of -1", `EntityEffectBroker.cs:940`) and then
/// `MakeClassicKey(type, subType)` with the Spells family (`:943`).
pub fn classic_effect_key(effect: &ClassicEffect) -> Option<i32> {
    if effect.effect_type == -1 {
        return None;
    }
    let sub = if effect.sub_type < 0 {
        255u8
    } else {
        effect.sub_type as u8
    };
    Some(classic_key(effect.effect_type as u8, sub))
}

/// Settings of a classic slot over the defaults: only what the template
/// supports is copied and `perLevel` is clamped to **≥ 1**
/// (`ClassicEffectRecordToEffectSettings`, `EntityEffectBroker.cs:962-996`).
pub fn classic_effect_settings(
    effect: &ClassicEffect,
    props: &EffectProperties,
) -> EffectSettings {
    let mut settings = EffectSettings::classic_default();
    if props.support_duration {
        settings.duration_base = effect.duration[0] as i32;
        settings.duration_plus = effect.duration[1] as i32;
        settings.duration_per_level = (effect.duration[2] as i32).max(1);
    }
    if props.support_chance {
        settings.chance_base = effect.chance[0] as i32;
        settings.chance_plus = effect.chance[1] as i32;
        settings.chance_per_level = (effect.chance[2] as i32).max(1);
    }
    if props.support_magnitude {
        settings.magnitude_base_min = effect.magnitude[0] as i32;
        settings.magnitude_base_max = effect.magnitude[1] as i32;
        settings.magnitude_plus_min = effect.magnitude[2] as i32;
        settings.magnitude_plus_max = effect.magnitude[3] as i32;
        settings.magnitude_per_level = (effect.magnitude[4] as i32).max(1);
    }
    settings
}

/// Effect entry (template + settings), `EffectEntry`
/// (`MagicAndEffectsStructs.cs:102-121`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EffectEntry {
    /// Registered template.
    pub props: &'static EffectProperties,
    /// Settings of the entry.
    pub settings: EffectSettings,
}

/// Patch N-5 (spec §6): the retail registry `index 10` (Free Action) stores
/// `type 3 / subType 2` (Cure Paralysis) in slot 0; DFU fixes it at
/// runtime to `type 26 / subType −1` before looking up the entry
/// (`EntityEffectBroker.cs:892-899`). In place and with the same 4 conditions
/// (`index`, slot 0, type and subtype).
pub fn apply_free_action_patch(index: u8, slot: usize, effect: &mut ClassicEffect) {
    if index == 10 && slot == 0 && effect.effect_type == 3 && effect.sub_type == 2 {
        effect.effect_type = 26;
        effect.sub_type = -1;
    }
}

/// Classic slot → valid entry (`ClassicEffectRecordToEffectEntry`,
/// `EntityEffectBroker.cs:926-950`): `None` when no template is implemented.
pub fn classic_effect_entry(effect: &ClassicEffect) -> Option<EffectEntry> {
    let key = classic_effect_key(effect)?;
    let props = effect_by_classic_key(key)?;
    Some(EffectEntry {
        props,
        settings: classic_effect_settings(effect, props),
    })
}

// ── §3.1 · route (a): casting cost ─────────────────────────────────────────

/// Cost of one component: `trunc(OffsetGold + CostA·start + CostB·(increase /
/// perLevel))` with integer division (`FormulaHelper.cs:2356-2365`, `:2633`).
pub fn component_cost(
    costs: EffectCosts,
    starting: i32,
    increase: i32,
    per_level: i32,
    _skill: i32,
) -> i32 {
    costs.offset_gold + costs.cost_a * starting + costs.cost_b * (increase / per_level)
}

/// Cost of one effect (gold, points): sum of the active components; with no
/// components, `fudgeGold = comp(MakeEffectCosts(60,100,160), 1,1,1, skill)`
/// (`FormulaHelper.cs:2313-2322`); points = `gold · (110 − skill) / 400`
/// integer (`:2323`).
pub fn effect_cost(entry: &EffectEntry, skill: i32) -> (i32, i32) {
    let props = entry.props;
    let settings = &entry.settings;
    let mut active = false;
    let mut gold = 0;

    if props.support_duration {
        active = true;
        gold += component_cost(
            props.duration_costs.unwrap_or_default(),
            settings.duration_base,
            settings.duration_plus,
            settings.duration_per_level,
            skill,
        );
    }
    if props.support_chance {
        active = true;
        gold += component_cost(
            props.chance_costs.unwrap_or_default(),
            settings.chance_base,
            settings.chance_plus,
            settings.chance_per_level,
            skill,
        );
    }
    if props.support_magnitude {
        active = true;
        // Mean magnitude with integer division (`:2301-2302`).
        let start = (settings.magnitude_base_max + settings.magnitude_base_min) / 2;
        let increase = (settings.magnitude_plus_max + settings.magnitude_plus_min) / 2;
        gold += component_cost(
            props.magnitude_costs.unwrap_or_default(),
            start,
            increase,
            settings.magnitude_per_level,
            skill,
        );
    }
    if !active {
        gold += component_cost(
            make_effect_costs(60, 100, 160),
            1,
            1,
            1,
            skill,
        );
    }
    let points = gold * (110 - skill) / 400;
    (gold, points)
}

/// Target type (`TargetTypes`; classic mapping 0..4 in
/// `EntityEffectBroker.cs:986-999`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TargetType {
    /// `rangeType 0` — caster only (×1.0).
    #[default]
    CasterOnly,
    /// `rangeType 1` — touch (×1.0).
    ByTouch,
    /// `rangeType 2` — single target at range (×1.5).
    SingleTargetAtRange,
    /// `rangeType 3` — area around caster (×2.0).
    AreaAroundCaster,
    /// `rangeType 4` — area at range (×2.5).
    AreaAtRange,
}

impl TargetType {
    /// Classic `rangeType` 0..4 (`EntityEffectBroker.cs:986-999`).
    pub const fn classic_index(self) -> u8 {
        match self {
            TargetType::CasterOnly => 0,
            TargetType::ByTouch => 1,
            TargetType::SingleTargetAtRange => 2,
            TargetType::AreaAroundCaster => 3,
            TargetType::AreaAtRange => 4,
        }
    }

    /// Classic `rangeType` → type (throws `Exception` like DFU on an unknown
    /// index, `:986-999` — here `None`).
    pub fn from_classic_index(index: u8) -> Option<Self> {
        match index {
            0 => Some(TargetType::CasterOnly),
            1 => Some(TargetType::ByTouch),
            2 => Some(TargetType::SingleTargetAtRange),
            3 => Some(TargetType::AreaAroundCaster),
            4 => Some(TargetType::AreaAtRange),
            _ => None,
        }
    }
}

/// Target multiplier: ×1 / ×1.5 / ×2 / ×2.5 with an `(int)` cast on `f32`
/// (`ApplyTargetCostMultiplier`, `FormulaHelper.cs:2329-2354`).
pub fn apply_target_multiplier(cost: i32, target: TargetType) -> i32 {
    match target {
        TargetType::CasterOnly | TargetType::ByTouch => cost,
        TargetType::SingleTargetAtRange => (cost as f32 * 1.5f32) as i32,
        TargetType::AreaAroundCaster => (cost as f32 * 2.0f32) as i32,
        TargetType::AreaAtRange => (cost as f32 * 2.5f32) as i32,
    }
}

/// Total cost of a bundle (`CalculateTotalEffectCosts`,
/// `FormulaHelper.cs:2189-2227`): sums only entries with a non-empty `Key`,
/// multiplies gold and points by the target, `minimumCastingCost` sets points
/// to 5 and the point floor is 5. Empty bundle ⇒ (0, 0) without floor (early
/// return `:2198-2201`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SpellCost {
    /// Cost in gold.
    pub gold: i32,
    /// Cost in spell points.
    pub spell_points: i32,
}

/// Computes the total cost; `live` resolves the **live** skill per effect
/// (the source uses `GetLiveSkillValue` per `Properties.MagicSkill`,
/// `:2251-2264`).
pub fn calculate_total_effect_costs(
    entries: &[EffectEntry],
    target: TargetType,
    minimum_casting_cost: bool,
    live: &impl Fn(&EffectProperties) -> i32,
) -> SpellCost {
    if entries.is_empty() {
        return SpellCost::default();
    }
    let mut gold = 0;
    let mut points = 0;
    for entry in entries {
        if entry.props.key.is_empty() {
            continue;
        }
        let (effect_gold, effect_points) = effect_cost(entry, live(entry.props));
        gold += effect_gold;
        points += effect_points;
    }
    gold = apply_target_multiplier(gold, target);
    points = apply_target_multiplier(points, target);
    if minimum_casting_cost {
        points = CAST_COST_FLOOR;
    }
    if points < CAST_COST_FLOOR {
        points = CAST_COST_FLOOR;
    }
    SpellCost {
        gold,
        spell_points: points,
    }
}

// ── §3.2 · route (b): `CalculateCastingCost` (classic reverse, skill 50) ───

/// Row indices into the coefficient table by `(type, subType)` (51 groups × 12,
/// `FormulaHelper.cs:2402-2445`).
const EFFECT_INDICES: [u8; 612] = [
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // Paralysis
    0x01, 0x02, 0x03, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // Continuous Damage
    0x04, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // Create Item
    0x05, 0x05, 0x06, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // Cure
    0x07, 0x07, 0x07, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // Damage
    0x08, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // Disintegrate
    0x09, 0x08, 0x09, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // Dispel
    0x0A, 0x0A, 0x0A, 0x0A, 0x0A, 0x0A, 0x0A, 0x0A, 0x00, 0x00, 0x00, 0x00, // Drain
    0x0B, 0x0B, 0x0B, 0x0B, 0x0B, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // Elemental Resistance
    0x0C, 0x0C, 0x0C, 0x0C, 0x0C, 0x0C, 0x0C, 0x0C, 0x00, 0x00, 0x00, 0x00, // Fortify Attribute
    0x0D, 0x0D, 0x0D, 0x0D, 0x0D, 0x0D, 0x0D, 0x0D, 0x07, 0x0E, 0x00, 0x00, // Heal
    0x0F, 0x0F, 0x0F, 0x0F, 0x0F, 0x0F, 0x0F, 0x0F, 0x0F, 0x0F, 0x00, 0x00, // Transfer
    0x26, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // Soul Trap
    0x10, 0x11, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // Invisibility
    0x12, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // Levitate
    0x13, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // Light
    0x14, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // Lock
    0x28, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // Open
    0x15, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // Regenerate
    0x16, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // Silence
    0x17, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // Spell Absorption
    0x17, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // Spell Reflection
    0x16, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // Spell Resistance
    0x18, 0x10, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // Chameleon
    0x18, 0x10, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // Shadow
    0x14, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // Slowfall
    0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // Climbing
    0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // Jumping
    0x19, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // Free Action
    0x1A, 0x1A, 0x1A, 0x1A, 0x1A, 0x1A, 0x1A, 0x00, 0x00, 0x00, 0x00, 0x00, // Lycanthropy/Polymorph
    0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // Water Breathing
    0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // Water Walking
    0x1B, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // Dimunition
    0x1A, 0x1C, 0x1C, 0x1D, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // Calm
    0x1E, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // Charm
    0x1F, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // Shield
    0x27, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // Telekinesis
    0x20, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // Astral Travel
    0x20, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // Etherealness
    0x21, 0x21, 0x22, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // Detect
    0x23, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // Identify
    0x24, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // Wizard Sight
    0x07, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // Darkness
    0x25, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // Recall
    0x29, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // Comprehend Languages
    0x2A, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // Intensify Fire
    0x2A, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // Diminish Fire
    0x2A, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // Wall of Stone?
    0x2A, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // Wall of Fire?
    0x2A, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // Wall of Frost?
    0x2A, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // Wall of Poison?
];

/// Coefficients per row (43 rows × 4, `FormulaHelper.cs:2448-2494`).
const EFFECT_COEFFICIENTS: [u16; 172] = [
    0x07, 0x19, 0x07, 0x19, // 0 Paralysis / Cure Magic?
    0x07, 0x02, 0x0A, 0x07, // 1 Continuous Damage - Health
    0x05, 0x02, 0x0A, 0x07, // 2 Cont. Damage - Stamina / Climbing / Jumping / Water Breathing / Water Walking
    0x0A, 0x02, 0x0A, 0x07, // 3 Continuous Damage - Spell Points
    0x0F, 0x1E, 0x00, 0x00, // 4 Create Item
    0x02, 0x19, 0x00, 0x00, // 5 Cure Disease / Cure Poison
    0x05, 0x23, 0x00, 0x00, // 6 Cure Paralysis
    0x05, 0x07, 0x00, 0x00, // 7 Damage Health / Damage Stamina / Damage Spell Points / Heal Health / Darkness
    0x14, 0x23, 0x00, 0x00, // 8 Disintegrate / Dispel Undead
    0x1E, 0x2D, 0x00, 0x00, // 9 Dispel Magic / Dispel Daedra
    0x04, 0x19, 0x02, 0x19, // 10 Drain Attribute
    0x19, 0x19, 0x02, 0x19, // 11 Elemental Resistance
    0x07, 0x19, 0x0A, 0x1E, // 12 Fortify Attribute
    0x0A, 0x07, 0x00, 0x00, // 13 Heal Attribute
    0x02, 0x07, 0x00, 0x00, // 14 Heal Stamina
    0x05, 0x05, 0x0F, 0x19, // 15 Transfer
    0x0A, 0x1E, 0x00, 0x00, // 16 Invisibility
    0x0F, 0x23, 0x00, 0x00, // 17 True Invisibility
    0x0F, 0x19, 0x00, 0x00, // 18 Levitate
    0x02, 0x0A, 0x00, 0x00, // 19 Light
    0x05, 0x19, 0x07, 0x1E, // 20 Lock / Slowfall
    0x19, 0x05, 0x02, 0x02, // 21 Regenerate
    0x05, 0x19, 0x05, 0x19, // 22 Silence / Spell Resistance
    0x07, 0x23, 0x07, 0x23, // 23 Spell Absorption / Spell Reflection
    0x05, 0x14, 0x00, 0x00, // 24 Chameleon / Shadow
    0x05, 0x05, 0x00, 0x00, // 25 Free Action
    0x0F, 0x19, 0x0F, 0x19, // 26 Lycanthropy / Polymorph / Calm Animal
    0x0A, 0x14, 0x14, 0x28, // 27 Diminution
    0x0A, 0x05, 0x14, 0x23, // 28 Calm Undead / Calm Humanoid
    0x07, 0x02, 0x0F, 0x1E, // 29 Calm Daedra? (Unused)
    0x05, 0x02, 0x0A, 0x0F, // 30 Charm
    0x07, 0x02, 0x14, 0x0F, // 31 Shield
    0x23, 0x02, 0x0A, 0x19, // 32 Astral Travel / Etherealness
    0x05, 0x02, 0x14, 0x1E, // 33 Detect Magic / Detect Enemy
    0x05, 0x02, 0x0F, 0x19, // 34 Detect Treasure
    0x05, 0x02, 0x0A, 0x19, // 35 Identify
    0x07, 0x0C, 0x05, 0x05, // 36 Wizard Sight
    0x23, 0x2D, 0x00, 0x00, // 37 Recall
    0x0F, 0x11, 0x0A, 0x11, // 38 Soul Trap
    0x14, 0x11, 0x19, 0x23, // 39 Telekinesis
    0x05, 0x19, 0x00, 0x00, // 40 Open
    0x0F, 0x11, 0x0A, 0x11, // 41 Comprehend Languages
    0x0F, 0x0F, 0x05, 0x05, // 42 Intensify Fire / Diminish Fire / Wall of --
];

/// Magic school per `type` (51 values; `FormulaHelper.cs:2497-2502`),
/// indexing [`MAGIC_SCHOOLS`].
const EFFECT_MAGIC_SCHOOLS: [u8; 51] = [
    0, 2, 3, 1, 2, 2, 3, 2, 0, 1, 1, 2, 3, 5, 4, 5, 3, 3, 1, 3, 1, 4, 4, 5, 5, 0, 1, 0, 0, 5, 0,
    4, 0, 4, 4, 0, 3, 3, 0, 4, 4, 4, 5, 3, 3, 0, 0, 4, 4, 4, 4,
];

/// `magicSkills[] = { Alteration, Restoration, Destruction, Mysticism,
/// Thaumaturgy, Illusion }` (`FormulaHelper.cs:2504-2511`) as indices of
/// `DFCareer.Skills` (`DFCareer.cs:506-515`).
const MAGIC_SCHOOLS: [MagicSkill; 6] = [
    MagicSkill::Alteration,
    MagicSkill::Restoration,
    MagicSkill::Destruction,
    MagicSkill::Mysticism,
    MagicSkill::Thaumaturgy,
    MagicSkill::Illusion,
];

/// `settingsTypes` (51 values, `FormulaHelper.cs:2513-2520`); public so the
/// M23 probe can declare which slots are computable by route (b) — retail
/// registry 71 `!Indulcet` has `magnitudePerLevel` 0 with settingsType 6 and
/// C# would throw `DivideByZeroException` (erratum measured 2026-10-21; the
/// engine invents no replacement divisor).
pub const SETTINGS_TYPES: [u8; 51] = [
    1, 2, 3, 4, 5, 4, 4, 2, 1, 6, 5, 6, 1, 3, 3, 3, 1, 4, 2, 1, 1, 1, 1, 3, 3, 3, 3, 3, 3, 1, 3,
    3, 1, 1, 1, 2, 2, 1, 1, 1, 1, 1, 3, 4, 1, 2, 2, 2, 2, 2, 2,
];

/// `rangeTypeModifiers = {2,2,3,4,5}` (`FormulaHelper.cs:2522`).
const RANGE_TYPE_MODIFIERS: [u8; 5] = [2, 2, 3, 4, 5];

/// `getCostFromSettings` cases 1..6 (C# integer division, literal
/// associativity included; `FormulaHelper.cs:2566-2632`; case 7 "Not used"
/// does not exist in the settingsTypes source ⇒ out, §6 N-6 analogy).
fn cost_from_settings(
    settings_type: u8,
    effect: &ClassicEffect,
    coefficients: [u16; 4],
) -> i32 {
    let dur = effect.duration.map(i32::from);
    let chance = effect.chance.map(i32::from);
    let mag = effect.magnitude.map(i32::from);
    let c = |i: usize| i32::from(coefficients[i]);
    match settings_type {
        1 => {
            c(0) * dur[0] + dur[1] / dur[2] * c(1) + c(2) * chance[0] + chance[1] / chance[2] * c(3)
        }
        2 => {
            c(0) * dur[0]
                + dur[1] / dur[2] * c(1)
                + (mag[1] + mag[0]) / 2 * c(2)
                + (mag[2] + mag[3]) / 2 / mag[4] * c(3)
        }
        3 => c(0) * dur[0] + dur[1] / dur[2] * c(1),
        4 => c(0) * chance[0] + chance[1] / chance[2] * c(1),
        5 => {
            c(0) * ((mag[1] + mag[0]) / 2)
                + (mag[2] + mag[3]) / 2 / mag[4] * c(1)
        }
        6 => {
            c(0) * dur[0]
                + c(1) * dur[1] / dur[2]
                + ((mag[1] + mag[0]) / 2) * c(2)
                + c(3) / mag[4] * ((mag[2] + mag[3]) / 2)
        }
        _ => 0,
    }
}

/// `CalculateCastingCost(spell, enchantingItem)` (`FormulaHelper.cs:2526-2565`):
/// skill **50** unless `enchantingItem=false` (live skill per school),
/// accumulates `cost · (110 − skill) / 100` per effect, applies
/// `cost · modifier >> 1` and a floor of 5. `live` is only consulted when
/// `enchanting_item` is false.
pub fn calculate_casting_cost(
    effects: &[ClassicEffect],
    range_type: u8,
    enchanting_item: bool,
    live: &impl Fn(MagicSkill) -> i32,
) -> i32 {
    let mut cost = 0;
    let mut skill = 50;
    for effect in effects.iter().take(3) {
        if effect.effect_type == -1 {
            continue;
        }
        let type_index = effect.effect_type as usize;
        if type_index >= SETTINGS_TYPES.len() {
            continue;
        }
        let sub_index = if effect.sub_type < 0 {
            0
        } else {
            effect.sub_type as usize
        };
        let row = EFFECT_INDICES[12 * type_index + sub_index] as usize;
        let mut coefficients = [0u16; 4];
        for (slot, value) in coefficients.iter_mut().enumerate() {
            *value = EFFECT_COEFFICIENTS[4 * row + slot];
        }
        if !enchanting_item {
            let school =
                MAGIC_SCHOOLS[EFFECT_MAGIC_SCHOOLS[type_index] as usize];
            skill = live(school);
        }
        cost += cost_from_settings(SETTINGS_TYPES[type_index], effect, coefficients)
            * (110 - skill)
            / 100;
    }
    let modifier = i32::from(
        *RANGE_TYPE_MODIFIERS
            .get(range_type as usize)
            .unwrap_or(&2),
    );
    cost = (cost * modifier) >> 1;
    if cost < CAST_COST_FLOOR {
        cost = CAST_COST_FLOOR;
    }
    cost
}

/// Convenience with a fixed skill of 50 (`enchantingItem=true`, the
/// `GetSpellEnchantPtCost` path, `FormulaHelper.cs:2371-2386`).
pub fn casting_cost_enchanting(effects: &[ClassicEffect], range_type: u8) -> i32 {
    calculate_casting_cost(effects, range_type, true, &|_| 50)
}

/// `GetSpellEnchantPtCost(index) = 10 · CalculateCastingCost(record)`
/// (`FormulaHelper.cs:2371-2386`).
pub fn enchant_pt_cost(effects: &[ClassicEffect], range_type: u8) -> i32 {
    10 * casting_cost_enchanting(effects, range_type)
}

// ── §4 · casting: animation and rounds engine ──────────────────────────────

/// `frameIndices = {0,1,2,3,4,5,0}` (`FPSSpellCasting.cs:53`).
pub const CAST_FRAME_INDICES: [usize; 7] = [0, 1, 2, 3, 4, 5, 0];
/// `animSpeed = 0.04` s per frame (`FPSSpellCasting.cs:50`).
pub const CAST_ANIM_SPEED: f64 = 0.04;
/// `releaseFrame = 5` (`FPSSpellCasting.cs:49`).
pub const CAST_RELEASE_FRAME: usize = 5;

/// Cast release frame: `5 · 0.04 = 0.20 s` (F6,
/// `FPSSpellCasting.cs:48-52,283-284`).
pub fn cast_release_seconds() -> f64 {
    CAST_RELEASE_FRAME as f64 * CAST_ANIM_SPEED
}

/// Full duration of the animation: `7 · 0.04 = 0.28 s` (§5 D).
pub fn cast_full_seconds() -> f64 {
    CAST_FRAME_INDICES.len() as f64 * CAST_ANIM_SPEED
}

/// Instantiated active effect (settings isolated per instance,
/// `EntityEffectBroker.cs:660-683`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActiveEffect {
    /// Effect template.
    pub props: &'static EffectProperties,
    /// Settings of THIS instance (never shared).
    pub settings: EffectSettings,
    /// Initial rounds (`rounds` of §2).
    pub rounds_start: i32,
    /// Remaining rounds (`RoundsRemaining`, `EntityEffect.cs:339-343`).
    pub rounds_remaining: i32,
}

/// Active rounds engine: 1 round = 1 minute of game time
/// (`EntityEffectBroker.cs:202-249`); the clock is supplied by M31 (§0).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MagicState {
    /// Active effects with a duration.
    active: Vec<ActiveEffect>,
}

impl MagicState {
    /// Active effects (in cast order).
    pub fn active(&self) -> &[ActiveEffect] {
        &self.active
    }

    /// Instantiates the bundle effects with each entry's settings and
    /// returns their initial rounds (0 = instant, does not persist).
    pub fn cast(&mut self, entries: &[EffectEntry], level: i32) -> Vec<i32> {
        let mut starts = Vec::with_capacity(entries.len());
        for entry in entries {
            let start = rounds(&entry.settings, entry.props.support_duration, level);
            starts.push(start);
            if start > 0 {
                self.active.push(ActiveEffect {
                    props: entry.props,
                    settings: entry.settings,
                    rounds_start: start,
                    rounds_remaining: start,
                });
            }
        }
        starts
    }

    /// One magic round: decrements every round and returns the keys that
    /// **expired** in this round (reached 0).
    pub fn magic_round(&mut self) -> Vec<&'static str> {
        let mut expired = Vec::new();
        let mut kept = Vec::with_capacity(self.active.len());
        for mut effect in self.active.drain(..) {
            effect.rounds_remaining -= 1;
            if effect.rounds_remaining <= 0 {
                expired.push(effect.props.key);
            } else {
                kept.push(effect);
            }
        }
        self.active = kept;
        expired
    }

    /// Remaining rounds of the first instance of `key` (0 when absent).
    pub fn rounds_left(&self, key: &str) -> i32 {
        self.active
            .iter()
            .find(|effect| effect.props.key == key)
            .map_or(0, |effect| effect.rounds_remaining)
    }
}

// ── §5 · spellbook and spellmaker ──────────────────────────────────────────

/// Spellbook purchase price: `spellPointCost · 4`; in Witches Festival half
/// of it with a minimum of 1 (`DaggerfallSpellBookWindow.cs:518-532`).
pub fn purchase_price(spell_point_cost: i32, witches_festival: bool) -> i32 {
    let mut price = spell_point_cost * 4;
    if witches_festival {
        price >>= 1;
        if price == 0 {
            price = 1;
        }
    }
    price
}

/// Spell bundle (template of `EffectBundleSettings`,
/// `EntityEffectBundle.cs`): name, effects, target and icon.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpellBundle {
    /// Visible name (either the retail registry one or the spellmaker one).
    pub name: String,
    /// Effect entries (1..3).
    pub effects: Vec<EffectEntry>,
    /// Target type.
    pub target: TargetType,
    /// `MinimumCastingCost` (vampirism etc., §3.1).
    pub minimum_casting_cost: bool,
    /// Icon index.
    pub icon: u8,
    /// `StandardSpellIndex` when it comes from `SPELLS.STD`.
    pub standard_index: Option<u8>,
}

impl SpellBundle {
    /// Cost recalculated with the current live skills (the book recalculates
    /// on every repaint, `DaggerfallSpellBookWindow.cs:256-263`).
    pub fn cost(&self, live: &impl Fn(&EffectProperties) -> i32) -> SpellCost {
        calculate_total_effect_costs(
            &self.effects,
            self.target,
            self.minimum_casting_cost,
            live,
        )
    }
}

/// Spellbook purchase error.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PurchaseError {
    /// Insufficient gold (the purchase changes nothing).
    InsufficientGold,
}

/// Character spellbook (`SPBK00I0.IMG` resource,
/// `DaggerfallSpellBookWindow.cs:96-98`; persistence = M33 boundary §7.7).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Spellbook {
    spells: Vec<SpellBundle>,
}

impl Spellbook {
    /// Spells in the book (sale duplicates are allowed as in the classic,
    /// `DaggerfallSpellBookWindow.cs:299-307`).
    pub fn spells(&self) -> &[SpellBundle] {
        &self.spells
    }

    /// Does it contain any spell with that name?
    pub fn contains(&self, name: &str) -> bool {
        self.spells.iter().any(|spell| spell.name == name)
    }

    /// Purchase: deducts `purchase_price` from gold and adds the spell;
    /// returns the price paid. Insufficient gold ⇒ `Err` and 0 changes.
    pub fn purchase(
        &mut self,
        bundle: &SpellBundle,
        gold: &mut i32,
        witches_festival: bool,
        live: &impl Fn(&EffectProperties) -> i32,
    ) -> Result<i32, PurchaseError> {
        let price = purchase_price(bundle.cost(live).spell_points, witches_festival);
        if *gold < price {
            return Err(PurchaseError::InsufficientGold);
        }
        *gold -= price;
        self.spells.push(bundle.clone());
        Ok(price)
    }

    /// Erases by name and returns it; gold is unchanged
    /// (demo criterion §10.3).
    pub fn forget(&mut self, name: &str) -> Option<SpellBundle> {
        let position = self.spells.iter().position(|spell| spell.name == name)?;
        Some(self.spells.remove(position))
    }
}

/// Maximum effects per spellmaker spell (`maxEffectsPerSpell = 3`,
/// `DaggerfallSpellMakerWindow.cs:131`; the 4th triggers message 1707,
/// `:706-722`).
pub const MAX_EFFECTS_PER_SPELL: usize = 3;

/// Spellmaker error.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpellmakerError {
    /// No free slot: only [`MAX_EFFECTS_PER_SPELL`] effects fit (RSC 1707).
    NoFreeSlot,
}

/// Spellmaker: the cost is **recalculated** on every query with
/// `CalculateTotalEffectCosts(entries, selectedTarget)`
/// (`DaggerfallSpellMakerWindow.cs:698,357-360`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpellMaker {
    /// Spell name (set by the player).
    pub name: String,
    /// Selected icon (default 1, `DaggerfallSpellMakerWindow.cs:132`).
    pub icon: u8,
    /// Selected target.
    pub target: TargetType,
    entries: Vec<EffectEntry>,
}

impl Default for SpellMaker {
    fn default() -> Self {
        Self::new(1, TargetType::CasterOnly)
    }
}

impl SpellMaker {
    /// Spellmaker with initial icon and target.
    pub fn new(icon: u8, target: TargetType) -> Self {
        Self {
            name: String::new(),
            icon,
            target,
            entries: Vec::new(),
        }
    }

    /// Current entries (1..3).
    pub fn entries(&self) -> &[EffectEntry] {
        &self.entries
    }

    /// Adds an effect; the 4th ⇒ `Err(SpellmakerError::NoFreeSlot)` (`:706-722`).
    pub fn add_effect(&mut self, entry: EffectEntry) -> Result<(), SpellmakerError> {
        if self.entries.len() >= MAX_EFFECTS_PER_SPELL {
            return Err(SpellmakerError::NoFreeSlot);
        }
        self.entries.push(entry);
        Ok(())
    }

    /// Displayed cost (recalculated every time; it is also the cost charged).
    pub fn displayed_cost(&self, live: &impl Fn(&EffectProperties) -> i32) -> SpellCost {
        calculate_total_effect_costs(&self.entries, self.target, false, live)
    }
}

// __TESTS__

// Goldens of the spec `m23-magic-spec-CR-5243af63.md` §2.1/§3.3 (families
// B–F), with the corrections of the closing corrections section
// (2026-10-21) (#2 D3 → effect_points 1 / total 5; #3 E4 → 360; #4 E5 →
// 3600). Family A (parse) lives in `daggerfall-parser-rs::spells_std`;
// F1–F3 (spell points / recovery) in `daggerfall-skills::character`.
#[cfg(test)]
mod tests {
    use super::*;

    // ── construction helpers (perLevel always ≥ 1 to avoid dividing by 0)

    /// Duration settings `(base, plus, perLevel)`; the rest stay 0.
    fn dur(base: i32, plus: i32, per_level: i32) -> EffectSettings {
        EffectSettings {
            duration_base: base,
            duration_plus: plus,
            duration_per_level: per_level,
            ..EffectSettings::default()
        }
    }

    /// Chance settings `(base, plus, perLevel)`; the rest stay 0.
    fn chance(base: i32, plus: i32, per_level: i32) -> EffectSettings {
        EffectSettings {
            chance_base: base,
            chance_plus: plus,
            chance_per_level: per_level,
            ..EffectSettings::default()
        }
    }

    /// Magnitude settings `(min, max, plusMin, plusMax, perLevel)` over the
    /// classic defaults (remaining fields stay 1).
    fn mag(min: i32, max: i32, plus_min: i32, plus_max: i32, per_level: i32) -> EffectSettings {
        EffectSettings {
            magnitude_base_min: min,
            magnitude_base_max: max,
            magnitude_plus_min: plus_min,
            magnitude_plus_max: plus_max,
            magnitude_per_level: per_level,
            ..EffectSettings::classic_default()
        }
    }

    /// Entry `(reference template, settings)`; panics when the key does not
    /// exist in the 22-template registry (a test error, not a spec error).
    fn entry_of(key: &str, settings: EffectSettings) -> EffectEntry {
        EffectEntry {
            props: effect_by_key(key).expect("reference template missing"),
            settings,
        }
    }

    /// Classic slot with null magnitude (the duration/chance goldens do not
    /// use it; the `ClassicEffect` default is `(0, 0)` in type/subtype, which
    /// is NOT equivalent to an empty slot).
    fn slot(effect_type: i8, sub_type: i8, duration: [u8; 3], chance_vals: [u8; 3]) -> ClassicEffect {
        ClassicEffect {
            effect_type,
            sub_type,
            duration,
            chance: chance_vals,
            magnitude: [0; 5],
        }
    }

    // ── Family B · rounds (spec §2.1, `EntityEffect.cs:920-935`) ───────────

    #[test]
    fn golden_b1_rounds_1_1_1_level_1_is_2() {
        assert_eq!(rounds(&dur(1, 1, 1), true, 1), 2);
    }

    #[test]
    fn golden_b2_rounds_per_level_25_keeps_multiplier_at_one() {
        // ⌊1/25⌋ = 0 ⇒ max(1, 0) = 1 → 1 + 1·1 = 2.
        assert_eq!(rounds(&dur(1, 1, 25), true, 1), 2);
    }

    #[test]
    fn golden_b3_rounds_level_5_is_6() {
        assert_eq!(rounds(&dur(1, 1, 1), true, 5), 6);
    }

    #[test]
    fn golden_b4_rounds_5_plus_2_per_3_level_9_is_11() {
        assert_eq!(rounds(&dur(5, 2, 3), true, 9), 11);
    }

    #[test]
    fn golden_b5_rounds_2_plus_3_per_2_level_7_is_11() {
        assert_eq!(rounds(&dur(2, 3, 2), true, 7), 11);
    }

    #[test]
    fn golden_b6_without_support_duration_is_zero() {
        assert_eq!(rounds(&EffectSettings::classic_default(), false, 1), 0);
        assert_eq!(rounds(&dur(9, 9, 1), false, 50), 0);
    }

    #[test]
    fn caster_level_is_one_when_there_is_no_caster() {
        // `FormulaHelper.cs:850-857`.
        assert_eq!(caster_level(None), 1);
        assert_eq!(caster_level(Some(7)), 7);
    }

    // ── Family C · chance (spec §2.1, `EntityEffect.cs:740-761`) ───────────

    #[test]
    fn golden_c1_chance_value_6_1_10_level_5_is_6() {
        assert_eq!(chance_value(&chance(6, 1, 10), 5), 6);
    }

    #[test]
    fn golden_c2_chance_value_6_1_10_level_25_is_8() {
        assert_eq!(chance_value(&chance(6, 1, 10), 25), 8);
    }

    #[test]
    fn golden_c3_chance_value_1_1_1_level_7_is_8() {
        assert_eq!(chance_value(&chance(1, 1, 1), 7), 8);
    }

    #[test]
    fn golden_c4_chance_value_10_5_1_level_3_is_25() {
        assert_eq!(chance_value(&chance(10, 5, 1), 3), 25);
    }

    #[test]
    fn golden_c5_chance_zero_never_succeeds_for_any_roll() {
        let zero = chance(0, 0, 1);
        assert_eq!(chance_value(&zero, 50), 0);
        for roll in 0..=99 {
            assert!(!roll_chance(true, &zero, 50, roll), "roll {roll} cannot be < 0");
        }
    }

    #[test]
    fn golden_c6_chance_100_and_150_always_succeed_without_clamp() {
        let c100 = chance(50, 50, 1); // 50 + 50·1 = 100
        let c150 = chance(50, 100, 1); // 50 + 100·1 = 150
        assert_eq!(chance_value(&c100, 1), 100);
        assert_eq!(chance_value(&c150, 1), 150, "no clamp to 100 (C6)");
        for roll in 0..=99 {
            assert!(roll_chance(true, &c100, 1, roll));
            assert!(roll_chance(true, &c150, 1, roll));
        }
    }

    #[test]
    fn roll_chance_uses_success_roll_inequality_and_support_flag() {
        let s = chance(35, 0, 1);
        assert!(roll_chance(true, &s, 1, 0));
        assert!(roll_chance(true, &s, 1, 34));
        assert!(!roll_chance(true, &s, 1, 35), "strict roll < chance");
        assert!(!roll_chance(true, &s, 1, 99));
        // `SupportChance = false` ⇒ false without looking at the roll.
        assert!(!roll_chance(false, &chance(100, 0, 1), 1, 0));
    }

    // ── Family D · route (a) casting cost (spec §3.1) ──────────────────────

    #[test]
    fn golden_d1_damage_health_skill_50_is_48g_7sp() {
        let damage = entry_of("Damage-Health", mag(1, 1, 1, 1, 1));
        assert_eq!(effect_cost(&damage, 50), (48, 7));
        let cost = calculate_total_effect_costs(&[damage], TargetType::ByTouch, false, &|_| 50);
        assert_eq!(cost, SpellCost { gold: 48, spell_points: 7 });
    }

    #[test]
    fn golden_d2_damage_health_skill_0_is_48g_13sp() {
        let damage = entry_of("Damage-Health", mag(1, 1, 1, 1, 1));
        assert_eq!(effect_cost(&damage, 0), (48, 13));
        let cost = calculate_total_effect_costs(&[damage], TargetType::ByTouch, false, &|_| 0);
        assert_eq!(cost, SpellCost { gold: 48, spell_points: 13 });
    }

    #[test]
    fn golden_d3_damage_health_skill_100_effect_points_1_total_floor_5() {
        // Erratum (correction #2): 48·10/400 = 1 per effect; the total applies
        // the floor of 5 (`FormulaHelper.cs:2193,2223-2224`).
        let damage = entry_of("Damage-Health", mag(1, 1, 1, 1, 1));
        assert_eq!(effect_cost(&damage, 100), (48, 1));
        let cost = calculate_total_effect_costs(&[damage], TargetType::ByTouch, false, &|_| 100);
        assert_eq!(cost, SpellCost { gold: 48, spell_points: 5 });
    }

    #[test]
    fn golden_d4_levitate_duration_1_1_1_is_160g_24sp() {
        let levitate = entry_of("Levitate", dur(1, 1, 1));
        assert_eq!(effect_cost(&levitate, 50), (160, 24));
    }

    #[test]
    fn golden_d5_levitate_duration_5_3_2_is_400g_60sp() {
        // 60·5 + 100·(3/2 = 1) = 400.
        let levitate = entry_of("Levitate", dur(5, 3, 2));
        assert_eq!(effect_cost(&levitate, 50), (400, 60));
    }

    #[test]
    fn golden_d6_spell_resistance_duration_plus_chance_is_240g_36sp() {
        let settings = EffectSettings {
            duration_base: 1,
            duration_plus: 1,
            duration_per_level: 1,
            ..chance(1, 1, 1)
        };
        let sr = entry_of("SpellResistance", settings);
        assert_eq!(effect_cost(&sr, 50), (240, 36));
    }

    #[test]
    fn golden_d7_teleport_without_components_uses_fudge_gold() {
        // fudge = comp(MakeEffectCosts(60,100,160), 1,1,1) = 320.
        let teleport = entry_of("Teleport-Effect", EffectSettings::classic_default());
        assert_eq!(effect_cost(&teleport, 50), (320, 48));
    }

    #[test]
    fn golden_d8_damage_plus_levitate_by_touch_is_208g_31sp() {
        let entries = [
            entry_of("Damage-Health", mag(1, 1, 1, 1, 1)),
            entry_of("Levitate", dur(1, 1, 1)),
        ];
        let cost = calculate_total_effect_costs(&entries, TargetType::ByTouch, false, &|_| 50);
        assert_eq!(cost, SpellCost { gold: 208, spell_points: 31 });
    }

    #[test]
    fn golden_d9_single_target_at_range_is_312g_46sp() {
        let entries = [
            entry_of("Damage-Health", mag(1, 1, 1, 1, 1)),
            entry_of("Levitate", dur(1, 1, 1)),
        ];
        let cost = calculate_total_effect_costs(
            &entries,
            TargetType::SingleTargetAtRange,
            false,
            &|_| 50,
        );
        // (int)(208·1.5) = 312 and (int)(31·1.5f) = 46.
        assert_eq!(cost, SpellCost { gold: 312, spell_points: 46 });
    }

    #[test]
    fn golden_d10_area_at_range_is_520g_77sp() {
        let entries = [
            entry_of("Damage-Health", mag(1, 1, 1, 1, 1)),
            entry_of("Levitate", dur(1, 1, 1)),
        ];
        let cost = calculate_total_effect_costs(&entries, TargetType::AreaAtRange, false, &|_| 50);
        // (int)(208·2.5) = 520 and (int)(31·2.5f) = 77.
        assert_eq!(cost, SpellCost { gold: 520, spell_points: 77 });
    }

    #[test]
    fn golden_d11_gold_eight_effect_points_is_one() {
        // Synthetic effect with a fixed component cost of 8 gold:
        // 8·(110−50)/400 = 480/400 = 1 (points per effect, no floor).
        const GOLD8: EffectProperties = EffectProperties {
            key: "D11-Gold-8",
            classic_key: 0,
            magic_skill: MagicSkill::None,
            support_duration: true,
            support_chance: false,
            support_magnitude: false,
            duration_costs: Some(make_effect_costs(8, 0, 0)),
            chance_costs: None,
            magnitude_costs: None,
            variant_count: 1,
        };
        let entry = EffectEntry {
            props: &GOLD8,
            settings: dur(1, 1, 1),
        };
        assert_eq!(effect_cost(&entry, 50), (8, 1));
    }

    #[test]
    fn golden_d12_magnitude_averages_truncate_toward_zero() {
        // start = (6+3)/2 = 4, increase = (4+1)/2 = 2 (integer division).
        assert_eq!(component_cost(make_effect_costs(20, 28, 0), 4, 2, 1, 50), 136);
        let damage = entry_of("Damage-Health", mag(3, 6, 1, 4, 1));
        assert_eq!(effect_cost(&damage, 50), (136, 20));
    }

    #[test]
    fn reference_effect_cost_tables_match_spec_3_1() {
        let damage = effect_by_key("Damage-Health").unwrap();
        assert_eq!(damage.magnitude_costs, Some(make_effect_costs(20, 28, 0)));
        assert_eq!(damage.magic_skill, MagicSkill::Destruction);
        let heal = effect_by_key("Heal-Health").unwrap();
        assert_eq!(heal.magnitude_costs, Some(make_effect_costs(20, 28, 0)));
        let levitate = effect_by_key("Levitate").unwrap();
        assert_eq!(levitate.duration_costs, Some(make_effect_costs(60, 100, 0)));
        assert_eq!(levitate.magic_skill, MagicSkill::Thaumaturgy);
        let sr = effect_by_key("SpellResistance").unwrap();
        assert_eq!(sr.duration_costs, Some(make_effect_costs(20, 100, 0)));
        assert_eq!(sr.chance_costs, Some(make_effect_costs(20, 100, 0)));
        let teleport = effect_by_key("Teleport-Effect").unwrap();
        assert_eq!(teleport.duration_costs, None);
        assert_eq!(teleport.chance_costs, None);
        assert_eq!(teleport.magnitude_costs, None, "no components (§3.1)");
    }

    #[test]
    fn empty_bundle_costs_zero_without_floor() {
        let cost = calculate_total_effect_costs(&[], TargetType::AreaAtRange, false, &|_| 50);
        assert_eq!(cost, SpellCost::default(), "early return :2198-2201 without floor");
    }

    #[test]
    fn minimum_casting_cost_forces_points_to_five() {
        let damage = entry_of("Damage-Health", mag(1, 1, 1, 1, 1));
        let cost = calculate_total_effect_costs(&[damage], TargetType::CasterOnly, true, &|_| 100);
        assert_eq!(cost, SpellCost { gold: 48, spell_points: 5 });
    }

    #[test]
    fn target_multiplier_truncates_the_f32_cast() {
        assert_eq!(apply_target_multiplier(101, TargetType::CasterOnly), 101);
        assert_eq!(apply_target_multiplier(101, TargetType::ByTouch), 101);
        assert_eq!(apply_target_multiplier(101, TargetType::SingleTargetAtRange), 151);
        assert_eq!(apply_target_multiplier(101, TargetType::AreaAroundCaster), 202);
        assert_eq!(apply_target_multiplier(101, TargetType::AreaAtRange), 252);
    }

    #[test]
    fn target_type_roundtrips_classic_indices() {
        for index in 0..=4 {
            let target = TargetType::from_classic_index(index).expect("0..4 is valid");
            assert_eq!(target.classic_index(), index);
        }
        assert!(TargetType::from_classic_index(5).is_none());
    }

    #[test]
    fn entries_with_empty_key_are_skipped() {
        const NO_KEY: EffectProperties = EffectProperties {
            key: "",
            classic_key: 0,
            magic_skill: MagicSkill::None,
            support_duration: false,
            support_chance: false,
            support_magnitude: true,
            duration_costs: None,
            chance_costs: None,
            magnitude_costs: Some(make_effect_costs(1000, 0, 0)),
            variant_count: 1,
        };
        let skipped = EffectEntry {
            props: &NO_KEY,
            settings: mag(1, 1, 1, 1, 1),
        };
        let real = entry_of("Damage-Health", mag(1, 1, 1, 1, 1));
        let cost = calculate_total_effect_costs(&[skipped, real], TargetType::CasterOnly, false, &|_| 50);
        assert_eq!(cost, SpellCost { gold: 48, spell_points: 7 });
    }

    #[test]
    fn spell_bundle_recalculates_cost_with_live_skills_each_call() {
        let bundle = SpellBundle {
            name: "Recalc".to_string(),
            effects: vec![entry_of("Damage-Health", mag(1, 1, 1, 1, 1))],
            target: TargetType::CasterOnly,
            minimum_casting_cost: false,
            icon: 1,
            standard_index: None,
        };
        assert_eq!(bundle.cost(&|_| 50), SpellCost { gold: 48, spell_points: 7 });
        assert_eq!(bundle.cost(&|_| 0), SpellCost { gold: 48, spell_points: 13 });
    }

    // ── Family E · route (b) `CalculateCastingCost` (spec §3.2) ────────────

    #[test]
    fn golden_e1_lock_index_1_range_0_costs_25() {
        let lock = slot(16, -1, [1, 1, 25], [1, 1, 1]);
        assert_eq!(casting_cost_enchanting(&[lock], 0), 25);
    }

    #[test]
    fn golden_e2_lock_range_type_4_costs_62() {
        // 25·5 >> 1 = 125 >> 1 = 62.
        let lock = slot(16, -1, [1, 1, 25], [1, 1, 1]);
        assert_eq!(casting_cost_enchanting(&[lock], 4), 62);
    }

    #[test]
    fn golden_e3_levitate_costs_24() {
        let levitate = slot(14, -1, [1, 1, 1], [1, 1, 1]);
        assert_eq!(casting_cost_enchanting(&[levitate], 0), 24);
    }

    #[test]
    fn golden_e4_tame_calm_record_costs_360() {
        // 600·60/100 = 360 and cost·2>>1 = 360 (erratum #3, used to be 960).
        let tame = slot(33, 0, [5, 5, 1], [10, 10, 1]);
        assert_eq!(casting_cost_enchanting(&[tame], 1), 360);
    }

    #[test]
    fn golden_e5_enchant_point_cost_is_ten_times_casting_cost() {
        // Erratum #4: 10 × 360 = 3600.
        let tame = slot(33, 0, [5, 5, 1], [10, 10, 1]);
        assert_eq!(enchant_pt_cost(&[tame], 1), 3600);
        let lock = slot(16, -1, [1, 1, 25], [1, 1, 1]);
        assert_eq!(enchant_pt_cost(&[lock], 0), 250);
    }

    #[test]
    fn cost_from_settings_cases_1_to_6_and_unused_default() {
        let lock = slot(16, -1, [1, 1, 25], [1, 1, 1]);
        // Case 1: 5·1 + (1/25)·25 + 7·1 + (1/1)·30 = 42.
        assert_eq!(cost_from_settings(1, &lock, [5, 25, 7, 30]), 42);
        let levitate = slot(14, -1, [1, 1, 1], [1, 1, 1]);
        // Case 3: 15·1 + (1/1)·25 = 40.
        assert_eq!(cost_from_settings(3, &levitate, [15, 25, 0, 0]), 40);
        // Case 4: 2·1 + (1/1)·25 = 27.
        assert_eq!(cost_from_settings(4, &lock, [2, 25, 0, 0]), 27);
        // Case 5: 15·((1+1)/2) + ((1+1)/2)/1·30 = 45.
        let mut create_item = slot(4, -1, [1, 1, 1], [1, 1, 1]);
        create_item.magnitude = [1, 1, 1, 1, 1];
        assert_eq!(cost_from_settings(5, &create_item, [15, 30, 0, 0]), 45);
        // Case 6: 4·1 + 25·(1/1) + ((1+1)/2)·2 + (25/1)·((1+1)/2) = 56.
        let mut drain = slot(9, -1, [1, 1, 1], [1, 1, 1]);
        drain.magnitude = [1, 1, 1, 1, 1];
        assert_eq!(cost_from_settings(6, &drain, [4, 25, 2, 25]), 56);
        // Case 7 "Not used" and settingsType 0 ⇒ 0 (outside the table, §6).
        assert_eq!(cost_from_settings(7, &lock, [5, 25, 7, 30]), 0);
        assert_eq!(cost_from_settings(0, &lock, [5, 25, 7, 30]), 0);
    }

    #[test]
    fn casting_cost_live_skill_uses_the_effect_school() {
        let levitate = slot(14, -1, [1, 1, 1], [1, 1, 1]);
        // EFFECT_MAGIC_SCHOOLS[14] = 4 ⇒ Thaumaturgy; skill 0 ⇒ 40·110/100 = 44.
        let cost = calculate_casting_cost(&[levitate], 0, false, &|school| {
            assert_eq!(school, MagicSkill::Thaumaturgy);
            0
        });
        assert_eq!(cost, 44);
        assert_eq!(calculate_casting_cost(&[levitate], 0, false, &|_| 50), 24);
        // Skill 100 ⇒ 40·10/100 = 4 ⇒ floor 5.
        assert_eq!(calculate_casting_cost(&[levitate], 0, false, &|_| 100), 5);
    }

    #[test]
    fn casting_cost_skips_empty_and_out_of_range_slots_and_takes_three() {
        let lock = slot(16, -1, [1, 1, 25], [1, 1, 1]);
        let empty = ClassicEffect {
            effect_type: -1,
            ..ClassicEffect::default()
        };
        // Empty slot (type −1) contributes nothing: it is still 25.
        assert_eq!(casting_cost_enchanting(&[empty, lock], 0), 25);
        // Only the first 3 slots (`.take(3)`): 3rd ⇒ 75, 4th ignored.
        assert_eq!(casting_cost_enchanting(&[lock, lock, lock], 0), 75);
        assert_eq!(casting_cost_enchanting(&[lock, lock, lock, lock], 0), 75);
        // type 51 ≥ SETTINGS_TYPES.len() ⇒ out; cost 0 ⇒ floor 5.
        let out_of_range = ClassicEffect {
            effect_type: 51,
            ..ClassicEffect::default()
        };
        assert_eq!(casting_cost_enchanting(&[out_of_range], 0), 5);
    }

    // ── Family F · purse and animation (spec §3.3 F4–F6) ───────────────────
    // F1–F3 (`SpellPoints`/`recovery`) are tested in `daggerfall-skills`.

    #[test]
    fn golden_f4_purchase_price_is_sp_times_four_and_half_in_festival() {
        assert_eq!(purchase_price(46, false), 184);
        assert_eq!(purchase_price(46, true), 92);
    }

    #[test]
    fn golden_f5_purchase_price_festival_floor_is_one() {
        // 0>>1 = 0 ⇒ minimum 1 (`DaggerfallSpellBookWindow.cs:528-529`).
        assert_eq!(purchase_price(0, true), 1);
    }

    #[test]
    fn golden_f6_cast_animation_release_frame_five() {
        assert_eq!(CAST_FRAME_INDICES, [0, 1, 2, 3, 4, 5, 0]);
        assert_eq!(CAST_RELEASE_FRAME, 5);
        assert!((CAST_ANIM_SPEED - 0.04).abs() < 1e-12);
        assert!((cast_release_seconds() - 0.20).abs() < 1e-9, "5 · 0.04 = 0.20 s");
        assert!((cast_full_seconds() - 0.28).abs() < 1e-9, "7 · 0.04 = 0.28 s");
    }

    #[test]
    #[should_panic(expected = "attempt to divide by zero")]
    fn route_b_panics_on_zero_divisor_exactly_like_dfu_csharp() {
        // Exact replica (R-B: behavior, not intent). Retail registry 71
        // `!Indulcet` slot 1 (type 9/sub 7, settingsType 6,
        // `magnitudePerLevel` 0) would throw `DivideByZeroException` in
        // `getCostFromSettings` (`FormulaHelper.cs:2566-2632`) — with no
        // live callers of `GetSpellEnchantPtCost(79)` in DFU. The M23 probe
        // measures it and declares it; the engine invents no divisor.
        let indulcet_slot1 = ClassicEffect {
            effect_type: 9,
            sub_type: 7,
            duration: [1, 5, 1],
            chance: [1, 1, 1],
            magnitude: [0, 1, 0, 0, 0],
        };
        let _ = casting_cost_enchanting(&[indulcet_slot1], 1);
    }

    // ── Effect model (spec §2) and reference registry (§6 N-1) ─────────────

    #[test]
    fn classic_key_composes_and_reverts_with_family_byte() {
        assert_eq!(classic_key(4, 0), 0x0400);
        assert_eq!(reverse_classic_key(classic_key(4, 0)), (0, 4, 0));
        assert_eq!(reverse_classic_key(classic_key(8, 255)), (0, 8, 255));
        assert_eq!(reverse_classic_key(0x01_02_03), (1, 2, 3));
    }

    #[test]
    fn classic_effect_key_maps_empty_slot_to_none_and_negative_sub_to_255() {
        let empty = ClassicEffect {
            effect_type: -1,
            ..ClassicEffect::default()
        };
        assert_eq!(classic_effect_key(&empty), None);
        let lock = slot(16, -1, [1, 1, 25], [1, 1, 1]);
        assert_eq!(classic_effect_key(&lock), Some(classic_key(16, 255)));
        let tame = slot(33, 0, [5, 5, 1], [10, 10, 1]);
        assert_eq!(classic_effect_key(&tame), Some(classic_key(33, 0)));
    }

    #[test]
    fn reference_registry_has_22_templates_with_unique_classic_keys() {
        // §6 N-1: 17 reference effects + 5 variants = 22 templates.
        assert_eq!(REFERENCE_EFFECTS.len(), 22);
        let mut keys: Vec<i32> = REFERENCE_EFFECTS.iter().map(|e| e.classic_key).collect();
        keys.sort_unstable();
        let total = keys.len();
        keys.dedup();
        assert_eq!(keys.len(), total, "no classicKey collision (probe B)");
        let variants: Vec<&EffectProperties> = REFERENCE_EFFECTS
            .iter()
            .filter(|e| e.variant_count > 1)
            .collect();
        assert_eq!(variants.len(), 5, "only ElementalResistance");
        assert!(variants.iter().all(|e| e.variant_count == 5));
    }

    #[test]
    fn classic_key_lookup_skips_zero_and_non_spells_family() {
        assert!(effect_by_key("Levitate").is_some());
        assert!(effect_by_key("no-such-effect").is_none());
        let levitate = effect_by_classic_key(classic_key(14, 255)).unwrap();
        assert_eq!(levitate.key, "Levitate");
        // `MapClassicKey`: ClassicKey == 0 is never mapped.
        assert!(effect_by_classic_key(0).is_none());
        // Spells family only; Lock is not among the 17 (NEGATIVE N-1).
        assert!(effect_by_classic_key(0x01_04_00).is_none());
        assert!(effect_by_classic_key(classic_key(16, 255)).is_none());
    }

    #[test]
    fn classic_effect_settings_copies_only_supported_and_clamps_per_level() {
        let levitate = effect_by_key("Levitate").unwrap();
        let record = slot(14, -1, [5, 3, 0], [1, 1, 1]); // perLevel 0 ⇒ 1
        let settings = classic_effect_settings(&record, levitate);
        assert_eq!(settings.duration_base, 5);
        assert_eq!(settings.duration_plus, 3);
        assert_eq!(settings.duration_per_level, 1, "perLevel clamped to ≥ 1");
        assert_eq!(settings.chance_base, 1, "only what the template supports");
        assert_eq!(settings.magnitude_base_min, 1);
    }

    #[test]
    fn classic_effect_entry_resolves_implemented_templates_only() {
        let levitate = classic_effect_entry(&slot(14, -1, [1, 1, 1], [1, 1, 1]))
            .expect("Levitate implemented");
        assert_eq!(levitate.props.key, "Levitate");
        assert_eq!(levitate.settings.duration_base, 1);
        let lock = classic_effect_entry(&slot(16, -1, [1, 1, 25], [1, 1, 1]));
        assert!(lock.is_none(), "Lock is not among the 17 (NEGATIVE N-1)");
        let unused_slot = ClassicEffect {
            effect_type: -1,
            ..ClassicEffect::default()
        };
        assert!(classic_effect_entry(&unused_slot).is_none());
    }

    #[test]
    fn n5_free_action_patch_rewrites_only_index_10_slot_0() {
        let mut retail = slot(3, 2, [1, 1, 1], [1, 1, 1]);
        assert!(classic_effect_entry(&retail).is_none(), "without the patch it does not resolve");
        apply_free_action_patch(10, 0, &mut retail);
        assert_eq!((retail.effect_type, retail.sub_type), (26, -1));
        let entry = classic_effect_entry(&retail).expect("FreeAction after the patch");
        assert_eq!(entry.props.key, "FreeAction");

        // The other 3 conditions do not fire (`EntityEffectBroker.cs:892-899`).
        let mut other_index = slot(3, 2, [1, 1, 1], [1, 1, 1]);
        apply_free_action_patch(11, 0, &mut other_index);
        assert_eq!((other_index.effect_type, other_index.sub_type), (3, 2));
        let mut other_slot = slot(3, 2, [1, 1, 1], [1, 1, 1]);
        apply_free_action_patch(10, 1, &mut other_slot);
        assert_eq!((other_slot.effect_type, other_slot.sub_type), (3, 2));
        let mut other_type = slot(4, 2, [1, 1, 1], [1, 1, 1]);
        apply_free_action_patch(10, 0, &mut other_type);
        assert_eq!((other_type.effect_type, other_type.sub_type), (4, 2));
    }

    #[test]
    fn magic_skill_career_indices_match_dfcareer() {
        assert_eq!(MagicSkill::Destruction.career_skill_index(), Some(22));
        assert_eq!(MagicSkill::None.career_skill_index(), None);
    }

    // ── Active rounds engine (spec §2, probe EFFECT block C) ───────────────

    #[test]
    fn magic_state_casts_counts_down_and_expires_exactly_after_n_rounds() {
        let mut state = MagicState::default();
        let levitate = entry_of("Levitate", dur(1, 1, 1));
        assert_eq!(state.cast(&[levitate], 1), vec![2], "§10.2: N initial rounds");
        assert_eq!(state.rounds_left("Levitate"), 2);
        assert!(state.magic_round().is_empty(), "1st round: still active");
        assert_eq!(state.rounds_left("Levitate"), 1);
        assert_eq!(state.magic_round(), vec!["Levitate"], "expires on the 2nd");
        assert_eq!(state.rounds_left("Levitate"), 0);
        assert!(state.active().is_empty());
        assert!(state.magic_round().is_empty(), "no effects: nothing expires");
    }

    #[test]
    fn magic_state_instant_effects_do_not_persist() {
        let mut state = MagicState::default();
        let teleport = entry_of("Teleport-Effect", EffectSettings::classic_default());
        assert_eq!(state.cast(&[teleport], 10), vec![0]);
        assert!(state.active().is_empty());
    }

    #[test]
    fn magic_state_instances_isolate_their_settings_and_counters() {
        // Probe EFFECT block C: two instances with different settings share
        // neither `Settings` nor `RoundsRemaining`.
        let mut state = MagicState::default();
        let short = entry_of("Levitate", dur(1, 1, 1)); // 2 rounds @ level 1
        let long = entry_of("Levitate", dur(9, 1, 1)); // 10 rounds @ level 1
        state.cast(&[short, long], 1);
        assert_eq!(state.active().len(), 2);
        assert_eq!(state.active()[0].rounds_start, 2);
        assert_eq!(state.active()[1].rounds_start, 10);
        state.magic_round();
        assert_eq!(state.active()[0].rounds_remaining, 1);
        assert_eq!(state.active()[1].rounds_remaining, 9);
        assert_eq!(state.rounds_left("Levitate"), 1, "queries the first one");
    }

    // ── Spellbook and spellmaker (spec §5, demo §10.3-§10.4) ───────────────

    fn sample_bundle() -> SpellBundle {
        SpellBundle {
            name: "Fenrik's Door Jam".to_string(),
            effects: vec![entry_of("Damage-Health", mag(1, 1, 1, 1, 1))],
            target: TargetType::ByTouch,
            minimum_casting_cost: false,
            icon: 1,
            standard_index: Some(0),
        }
    }

    #[test]
    fn spellbook_purchase_discounts_price_and_adds_the_spell() {
        let mut book = Spellbook::default();
        let mut gold = 100;
        // cost 7 points ⇒ price 28 (sp·4, §5).
        let paid = book.purchase(&sample_bundle(), &mut gold, false, &|_| 50).unwrap();
        assert_eq!(paid, 28);
        assert_eq!(gold, 72);
        assert!(book.contains("Fenrik's Door Jam"));
        assert_eq!(book.spells().len(), 1);
    }

    #[test]
    fn spellbook_purchase_half_price_in_witches_festival() {
        let mut book = Spellbook::default();
        let mut gold = 100;
        let paid = book.purchase(&sample_bundle(), &mut gold, true, &|_| 50).unwrap();
        assert_eq!(paid, 14, "28>>1 in Witches Festival");
        assert_eq!(gold, 86);
    }

    #[test]
    fn spellbook_forget_removes_without_changing_gold() {
        let mut book = Spellbook::default();
        let mut gold = 100;
        book.purchase(&sample_bundle(), &mut gold, false, &|_| 50).unwrap();
        let gold_before = gold;
        let removed = book.forget("Fenrik's Door Jam");
        assert!(removed.is_some());
        assert!(!book.contains("Fenrik's Door Jam"));
        assert_eq!(gold, gold_before, "erasing does not change gold (§10.3)");
        assert!(book.forget("does-not-exist").is_none());
    }

    #[test]
    fn spellbook_insufficient_gold_changes_nothing() {
        let mut book = Spellbook::default();
        let mut gold = 5; // price 28
        let result = book.purchase(&sample_bundle(), &mut gold, false, &|_| 50);
        assert_eq!(result, Err(PurchaseError::InsufficientGold));
        assert_eq!(gold, 5);
        assert!(book.spells().is_empty());
    }

    #[test]
    fn spellbook_permits_duplicate_names_like_the_classic() {
        // `DaggerfallSpellBookWindow.cs:299-307`: selling allows duplicates.
        let mut book = Spellbook::default();
        let mut gold = 100;
        book.purchase(&sample_bundle(), &mut gold, false, &|_| 50).unwrap();
        book.purchase(&sample_bundle(), &mut gold, false, &|_| 50).unwrap();
        assert_eq!(book.spells().len(), 2);
        assert!(book.contains("Fenrik's Door Jam"));
    }

    #[test]
    fn spellmaker_rejects_the_fourth_effect() {
        let mut maker = SpellMaker::default();
        let entry = entry_of("Damage-Health", mag(1, 1, 1, 1, 1));
        assert!(maker.add_effect(entry).is_ok());
        assert!(maker.add_effect(entry).is_ok());
        assert!(maker.add_effect(entry).is_ok());
        assert_eq!(maker.add_effect(entry), Err(SpellmakerError::NoFreeSlot));
        assert_eq!(maker.entries().len(), MAX_EFFECTS_PER_SPELL, "3 slots (RSC 1707)");
    }

    #[test]
    fn spellmaker_displayed_cost_equals_the_charged_cost() {
        let mut maker = SpellMaker::default();
        let entry = entry_of("Damage-Health", mag(1, 1, 1, 1, 1));
        maker.add_effect(entry).unwrap();
        maker.add_effect(entry).unwrap();
        maker.add_effect(entry).unwrap();
        let displayed = maker.displayed_cost(&|_| 50);
        assert_eq!(displayed, SpellCost { gold: 144, spell_points: 21 });

        // The displayed cost is the charged cost (§10.4): same computation when buying.
        let bundle = SpellBundle {
            name: maker.name.clone(),
            effects: maker.entries().to_vec(),
            target: maker.target,
            minimum_casting_cost: false,
            icon: maker.icon,
            standard_index: None,
        };
        let mut book = Spellbook::default();
        let mut gold = 1000;
        let paid = book.purchase(&bundle, &mut gold, false, &|_| 50).unwrap();
        assert_eq!(paid, purchase_price(displayed.spell_points, false));
        assert_eq!(gold, 1000 - paid);
        assert_eq!(book.spells()[0].effects.len(), 3);
    }
}
