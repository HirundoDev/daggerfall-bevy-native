//! Alteration family — 11 registered templates from DFU `0038a1e6`
//! (`Assets/Scripts/Game/MagicAndEffects/Effects/Alteration/*.cs`).
//!
//! Seven files: six single-variant classes plus `ElementalResistance`, whose five
//! variants (`ElementalResistance.cs:83`) are five rows sharing one property block
//! and differing only in `Key`/`ClassicKey`/element (`ElementalResistance.cs:193-194`).
//! File order of DFU; every row cites its source `file:line`.
//!
//! Contract: `docs/sintergic/game/programming/m23-effects-contract.md`
//! (§2 vocabulary, §3 builder, §4.1 Alteration mapping).
//!
//! Each row cites its source `file:line` on the `///` comment immediately above
//! it, as the M23 dispatch requires. A doc comment on an expression (array
//! element) is ignored by rustdoc rather than shown, so `unused_doc_comments`
//! is allowed module-wide to keep the build warning-free.
#![allow(unused_doc_comments)]

use crate::magic::{
    classic_key, make_effect_costs, Amount, ChanceFunction, EffectProperties, Element, Flag,
    MagicSkill, Payload,
};

/// The 11 Alteration templates of DFU `0038a1e6`, in file order.
pub const ALTERATION: &[EffectProperties] = &[
    /// `Climbing.cs:23-34` — `Key` :23, `ClassicKey` :28, `SupportDuration` :29,
    /// `DurationCosts` :34; payload `IsEnhancedClimbing = true` :83
    /// (`ConstantEffect` :41-44, `Start` :47-50) and `= false` :93 (`End` :59-62).
    EffectProperties::new(
        "Climbing",
        classic_key(28, 255),
        MagicSkill::Alteration,
        &[Payload::Flag(Flag::EnhancedClimbing)],
    )
    .duration(make_effect_costs(20, 20, 0)),
    /// `ElementalResistance.cs:73-83,193-194` — shared block :73-80
    /// (`SupportDuration` :73, `SupportChance` :74, `DurationCosts` :79,
    /// `ChanceCosts` :80), `variantCount = 5` :83, `variantIndex 0`
    /// (`Fire`, `subGroupTextKeys` :28, set :84), `Key` :193, `ClassicKey` :194;
    /// payload `SetResistanceFlag(element, true)` :206 +
    /// `RaiseResistanceChance(element, ChanceValue())` :207, revert :217.
    EffectProperties::new(
        "ElementalResistance-Fire",
        classic_key(8, 0),
        MagicSkill::Alteration,
        &[Payload::Resistance(Element::Fire, Amount::Rolled)],
    )
    .duration(make_effect_costs(100, 100, 0))
    .chance(make_effect_costs(8, 100, 0))
    .variants(5)
    .chance_fn(ChanceFunction::Custom),
    /// `ElementalResistance.cs:73-83,193-194` — shared block :73-80, `variantCount`
    /// :83, `variantIndex 1` (`Frost`, `subGroupTextKeys` :28, set :85), `Key`
    /// :193, `ClassicKey` :194; payload :206-207, revert :217.
    EffectProperties::new(
        "ElementalResistance-Frost",
        classic_key(8, 1),
        MagicSkill::Alteration,
        &[Payload::Resistance(Element::Frost, Amount::Rolled)],
    )
    .duration(make_effect_costs(100, 100, 0))
    .chance(make_effect_costs(8, 100, 0))
    .variants(5)
    .chance_fn(ChanceFunction::Custom),
    /// `ElementalResistance.cs:73-83,193-194` — shared block :73-80, `variantCount`
    /// :83, `variantIndex 2` (`Poison`, `subGroupTextKeys` :28, set
    /// `DFCareer.Elements.DiseaseOrPoison` :86), `Key` :193, `ClassicKey` :194;
    /// payload :206-207, revert :217.
    EffectProperties::new(
        "ElementalResistance-Poison",
        classic_key(8, 2),
        MagicSkill::Alteration,
        &[Payload::Resistance(Element::DiseaseOrPoison, Amount::Rolled)],
    )
    .duration(make_effect_costs(100, 100, 0))
    .chance(make_effect_costs(8, 100, 0))
    .variants(5)
    .chance_fn(ChanceFunction::Custom),
    /// `ElementalResistance.cs:73-83,193-194` — shared block :73-80, `variantCount`
    /// :83, `variantIndex 3` (`Shock`, `subGroupTextKeys` :28, set :87), `Key`
    /// :193, `ClassicKey` :194; payload :206-207, revert :217.
    EffectProperties::new(
        "ElementalResistance-Shock",
        classic_key(8, 3),
        MagicSkill::Alteration,
        &[Payload::Resistance(Element::Shock, Amount::Rolled)],
    )
    .duration(make_effect_costs(100, 100, 0))
    .chance(make_effect_costs(8, 100, 0))
    .variants(5)
    .chance_fn(ChanceFunction::Custom),
    /// `ElementalResistance.cs:73-83,193-194` — shared block :73-80, `variantCount`
    /// :83, `variantIndex 4` (`Magicka`, `subGroupTextKeys` :28, set
    /// `DFCareer.Elements.Magic` :88), `Key` :193, `ClassicKey` :194; payload
    /// :206-207, revert :217.
    EffectProperties::new(
        "ElementalResistance-Magicka",
        classic_key(8, 4),
        MagicSkill::Alteration,
        &[Payload::Resistance(Element::Magic, Amount::Rolled)],
    )
    .duration(make_effect_costs(100, 100, 0))
    .chance(make_effect_costs(8, 100, 0))
    .variants(5)
    .chance_fn(ChanceFunction::Custom),
    /// `Jumping.cs:23-34` — `Key` :23, `ClassicKey` :28, `SupportDuration` :29,
    /// `DurationCosts` :34; payload `IsEnhancedJumping = true` :84
    /// (`ConstantEffect` :42-45, `Start` :48-51) and `= false` :94 (`End` :60-63).
    EffectProperties::new(
        "Jumping",
        classic_key(27, 255),
        MagicSkill::Alteration,
        &[Payload::Flag(Flag::EnhancedJumping)],
    )
    .duration(make_effect_costs(20, 8, 0)),
    /// `Paralyze.cs:23-38` — `Key` :23, `ClassicKey` :30, `SupportDuration` :31,
    /// `SupportChance` :32, `DurationCosts` :37, `ChanceCosts` :38; payload
    /// `IsParalyzed = true` :87 (`ConstantEffect` :51-54) and `= false` :105
    /// (`End` :63-66).
    EffectProperties::new(
        "Paralyze",
        classic_key(0, 255),
        MagicSkill::Alteration,
        &[Payload::Flag(Flag::Paralyzed)],
    )
    .duration(make_effect_costs(28, 100, 0))
    .chance(make_effect_costs(28, 100, 0)),
    /// `Shield.cs:25-41` — `Key` :25, `ClassicKey` :33, `SupportDuration` :34,
    /// `SupportMagnitude` :35, `DurationCosts` :40, `MagnitudeCosts` :41; payload
    /// `startingShield = shieldRemaining = GetMagnitude(...)` :70 (`Start` :65).
    EffectProperties::new(
        "Shield",
        classic_key(35, 255),
        MagicSkill::Alteration,
        &[Payload::Shield],
    )
    .duration(make_effect_costs(28, 8, 0))
    .magnitude(make_effect_costs(80, 60, 0)),
    /// `Slowfall.cs:23-36` — `Key` :23, `ClassicKey` :30, `SupportDuration` :31,
    /// `DurationCosts` :36; payload `IsSlowFalling = true` :92 (`ConstantEffect`
    /// :56-59) and `= false` :109 (`End` :68-71).
    EffectProperties::new(
        "Slowfall",
        classic_key(25, 255),
        MagicSkill::Alteration,
        &[Payload::Flag(Flag::SlowFalling)],
    )
    .duration(make_effect_costs(20, 100, 0)),
    /// `WaterBreathing.cs:23-34` — `Key` :23, `ClassicKey` :28,
    /// `SupportDuration` :29, `DurationCosts` :34; payload `IsWaterBreathing = true`
    /// :97 (`ConstantEffect` :55-58, `Start` :61-64) and `= false` :107 (`End` :73-76).
    EffectProperties::new(
        "WaterBreathing",
        classic_key(30, 255),
        MagicSkill::Alteration,
        &[Payload::Flag(Flag::WaterBreathing)],
    )
    .duration(make_effect_costs(20, 8, 0)),
];

// CONTRACT-GAP: `Paralyze.cs:48`/`:88` — `PlayerAggro()` runs on `Start` and
// when paralysis lands; that is target AI state and the payload vocabulary has
// no "aggro the target on the caster" primitive (AI is M25, contract §6).
// CONTRACT-GAP: `Shield.cs:85-89` — the pool bust ends the effect immediately
// (`ResignAsIncumbent`, `RoundsRemaining = 0`) and returns the overflow to the
// incoming damage; `Payload::Shield` declares the pool, but the early end is
// consumed by the M22 damage stage, not expressible on the template.
