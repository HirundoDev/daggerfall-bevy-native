//! Restoration family — 25 registered templates from DFU `0038a1e6`
//! (`Assets/Scripts/Game/MagicAndEffects/Effects/Restoration/*.cs`).
//!
//! One row per concrete effect class, in DFU file order. The two abstract bases
//! of the family, `FortifyEffect.cs` (`FortifyEffect.cs:21`) and `HealEffect.cs`
//! (`HealEffect.cs:21`), are **not** rows: their payload is executed by their
//! subclasses and is cited from each subclass row (contract §1).
//!
//! Contract: `docs/sintergic/game/programming/m23-effects-contract.md`
//! (§2 vocabulary, §3 builder, §4.1 Restoration mapping).
//!
//! Each row cites its source `file:line` on the `///` comment immediately above
//! it, as the M23 dispatch requires. A doc comment on an expression (array
//! element) is ignored by rustdoc rather than shown, so `unused_doc_comments`
//! is allowed module-wide to keep the build warning-free.
#![allow(unused_doc_comments)]

use crate::magic::{
    classic_key, make_effect_costs, Attribute, ChanceFunction, ChanceState, Cure, EffectProperties,
    Flag, MagicSkill, Payload, Resource,
};

/// The 25 Restoration templates of DFU `0038a1e6`, in file order.
pub const RESTORATION: &[EffectProperties] = &[
    /// `CureDisease.cs:23-34` — `Key` :23, `ClassicKey` :28, `SupportChance` :29,
    /// `ChanceCosts` :34; payload `manager.CureAllDiseases()` :85 (`MagicRound` :75).
    EffectProperties::new(
        "Cure-Disease",
        classic_key(3, 0),
        MagicSkill::Restoration,
        &[Payload::Cure(Cure::Disease)],
    )
    .chance(make_effect_costs(8, 100, 0)),
    /// `CureParalyzation.cs:22-33` — `Key` :22, `ClassicKey` :27, `SupportChance` :28,
    /// `ChanceCosts` :33; payload `manager.EndIncumbentEffect<Paralyze>()` :44
    /// (`MagicRound` :41).
    EffectProperties::new(
        "Cure-Paralyzation",
        classic_key(3, 2),
        MagicSkill::Restoration,
        &[Payload::Cure(Cure::Paralyzation)],
    )
    .chance(make_effect_costs(20, 140, 0)),
    /// `CurePoison.cs:23-34` — `Key` :23, `ClassicKey` :28, `SupportChance` :29,
    /// `ChanceCosts` :34; payload `manager.CureAllPoisons()` :68 (`MagicRound` :58).
    EffectProperties::new(
        "Cure-Poison",
        classic_key(3, 1),
        MagicSkill::Restoration,
        &[Payload::Cure(Cure::Poison)],
    )
    .chance(make_effect_costs(8, 100, 0)),
    /// `FortifyAgility.cs:22-36` — `Key` :22, `ClassicKey` :27, `SupportDuration`
    /// :28, `SupportMagnitude` :29, `DurationCosts` :34, `MagnitudeCosts` :35,
    /// `fortifyStat = Agility` :36; payload `FortifyEffect.cs:33`
    /// (`ChangeStatMod(fortifyStat, GetMagnitude(caster))`, `BecomeIncumbent` :30-34).
    EffectProperties::new(
        "Fortify-Agility",
        classic_key(9, 3),
        MagicSkill::Restoration,
        &[Payload::FortifyAttribute(Attribute::Agility)],
    )
    .duration(make_effect_costs(28, 100, 0))
    .magnitude(make_effect_costs(40, 120, 0)),
    /// `FortifyEndurance.cs:22-36` — `Key` :22, `ClassicKey` :27, `SupportDuration`
    /// :28, `SupportMagnitude` :29, `DurationCosts` :34, `MagnitudeCosts` :35,
    /// `fortifyStat = Endurance` :36; payload `FortifyEffect.cs:33`.
    EffectProperties::new(
        "Fortify-Endurance",
        classic_key(9, 4),
        MagicSkill::Restoration,
        &[Payload::FortifyAttribute(Attribute::Endurance)],
    )
    .duration(make_effect_costs(28, 100, 0))
    .magnitude(make_effect_costs(40, 120, 0)),
    /// `FortifyIntelligence.cs:22-36` — `Key` :22, `ClassicKey` :27,
    /// `SupportDuration` :28, `SupportMagnitude` :29, `DurationCosts` :34,
    /// `MagnitudeCosts` :35, `fortifyStat = Intelligence` :36; payload
    /// `FortifyEffect.cs:33`.
    EffectProperties::new(
        "Fortify-Intelligence",
        classic_key(9, 1),
        MagicSkill::Restoration,
        &[Payload::FortifyAttribute(Attribute::Intelligence)],
    )
    .duration(make_effect_costs(28, 100, 0))
    .magnitude(make_effect_costs(40, 120, 0)),
    /// `FortifyLuck.cs:22-36` — `Key` :22, `ClassicKey` :27, `SupportDuration` :28,
    /// `SupportMagnitude` :29, `DurationCosts` :34, `MagnitudeCosts` :35,
    /// `fortifyStat = Luck` :36; payload `FortifyEffect.cs:33`.
    EffectProperties::new(
        "Fortify-Luck",
        classic_key(9, 7),
        MagicSkill::Restoration,
        &[Payload::FortifyAttribute(Attribute::Luck)],
    )
    .duration(make_effect_costs(28, 100, 0))
    .magnitude(make_effect_costs(40, 120, 0)),
    /// `FortifyPersonality.cs:22-36` — `Key` :22, `ClassicKey` :27,
    /// `SupportDuration` :28, `SupportMagnitude` :29, `DurationCosts` :34,
    /// `MagnitudeCosts` :35, `fortifyStat = Personality` :36; payload
    /// `FortifyEffect.cs:33`.
    EffectProperties::new(
        "Fortify-Personality",
        classic_key(9, 5),
        MagicSkill::Restoration,
        &[Payload::FortifyAttribute(Attribute::Personality)],
    )
    .duration(make_effect_costs(28, 100, 0))
    .magnitude(make_effect_costs(40, 120, 0)),
    /// `FortifySpeed.cs:22-36` — `Key` :22, `ClassicKey` :27, `SupportDuration`
    /// :28, `SupportMagnitude` :29, `DurationCosts` :34, `MagnitudeCosts` :35,
    /// `fortifyStat = Speed` :36; payload `FortifyEffect.cs:33`.
    EffectProperties::new(
        "Fortify-Speed",
        classic_key(9, 6),
        MagicSkill::Restoration,
        &[Payload::FortifyAttribute(Attribute::Speed)],
    )
    .duration(make_effect_costs(28, 100, 0))
    .magnitude(make_effect_costs(40, 120, 0)),
    /// `FortifyStrength.cs:22-36` — `Key` :22, `ClassicKey` :27, `SupportDuration`
    /// :28, `SupportMagnitude` :29, `DurationCosts` :34, `MagnitudeCosts` :35,
    /// `fortifyStat = Strength` :36; payload `FortifyEffect.cs:33`.
    EffectProperties::new(
        "Fortify-Strength",
        classic_key(9, 0),
        MagicSkill::Restoration,
        &[Payload::FortifyAttribute(Attribute::Strength)],
    )
    .duration(make_effect_costs(28, 100, 0))
    .magnitude(make_effect_costs(40, 120, 0)),
    /// `FortifyWillpower.cs:22-36` — `Key` :22, `ClassicKey` :27,
    /// `SupportDuration` :28, `SupportMagnitude` :29, `DurationCosts` :34,
    /// `MagnitudeCosts` :35, `fortifyStat = Willpower` :36; payload
    /// `FortifyEffect.cs:33`.
    EffectProperties::new(
        "Fortify-Willpower",
        classic_key(9, 2),
        MagicSkill::Restoration,
        &[Payload::FortifyAttribute(Attribute::Willpower)],
    )
    .duration(make_effect_costs(28, 100, 0))
    .magnitude(make_effect_costs(40, 120, 0)),
    /// `FreeAction.cs:23-34` — `Key` :23, `ClassicKey` :28, `SupportDuration` :29,
    /// `DurationCosts` :34; payload `IsImmuneToParalysis = true` :99
    /// (`ConstantEffect` :57-60, `Start` :63-66) and `= false` :109 (`End` :75-78).
    EffectProperties::new(
        "FreeAction",
        classic_key(26, 255),
        MagicSkill::Restoration,
        &[Payload::Flag(Flag::FreeAction)],
    )
    .duration(make_effect_costs(20, 8, 0)),
    /// `HealAgility.cs:22-34` — `Key` :22, `ClassicKey` :27, `SupportMagnitude`
    /// :28, `MagnitudeCosts` :33, `healStat = Agility` :34; payload
    /// `HealEffect.cs:30` (`manager.HealAttribute(healStat, magnitude)`,
    /// `MagicRound` :25).
    EffectProperties::new(
        "Heal-Agility",
        classic_key(10, 3),
        MagicSkill::Restoration,
        &[Payload::HealAttribute(Attribute::Agility)],
    )
    .magnitude(make_effect_costs(40, 28, 0)),
    /// `HealEndurance.cs:22-34` — `Key` :22, `ClassicKey` :27, `SupportMagnitude`
    /// :28, `MagnitudeCosts` :33, `healStat = Endurance` :34; payload
    /// `HealEffect.cs:30`.
    EffectProperties::new(
        "Heal-Endurance",
        classic_key(10, 4),
        MagicSkill::Restoration,
        &[Payload::HealAttribute(Attribute::Endurance)],
    )
    .magnitude(make_effect_costs(40, 28, 0)),
    /// `HealFatigue.cs:23-34` — `Key` :23, `ClassicKey` :28, `SupportMagnitude`
    /// :29, `MagnitudeCosts` :34; payload `IncreaseFatigue(magnitude, true)` :69
    /// (`MagicRound` :58).
    EffectProperties::new(
        "Heal-Fatigue",
        classic_key(10, 9),
        MagicSkill::Restoration,
        &[Payload::Resource {
            resource: Resource::Fatigue,
            sign: 1,
        }],
    )
    .magnitude(make_effect_costs(8, 28, 0)),
    /// `HealHealth.cs:23-34` — `Key` :23, `ClassicKey` :28, `SupportMagnitude` :29,
    /// `MagnitudeCosts` :34; payload `IncreaseHealth(magnitude)` :83
    /// (`MagicRound` :72).
    EffectProperties::new(
        "Heal-Health",
        classic_key(10, 8),
        MagicSkill::Restoration,
        &[Payload::Resource {
            resource: Resource::Health,
            sign: 1,
        }],
    )
    .magnitude(make_effect_costs(20, 28, 0)),
    /// `HealIntelligence.cs:22-34` — `Key` :22, `ClassicKey` :27,
    /// `SupportMagnitude` :28, `MagnitudeCosts` :33, `healStat = Intelligence`
    /// :34; payload `HealEffect.cs:30`.
    EffectProperties::new(
        "Heal-Intelligence",
        classic_key(10, 1),
        MagicSkill::Restoration,
        &[Payload::HealAttribute(Attribute::Intelligence)],
    )
    .magnitude(make_effect_costs(40, 28, 0)),
    /// `HealLuck.cs:22-34` — `Key` :22, `ClassicKey` :27, `SupportMagnitude` :28,
    /// `MagnitudeCosts` :33, `healStat = Luck` :34; payload `HealEffect.cs:30`.
    EffectProperties::new(
        "Heal-Luck",
        classic_key(10, 7),
        MagicSkill::Restoration,
        &[Payload::HealAttribute(Attribute::Luck)],
    )
    .magnitude(make_effect_costs(40, 28, 0)),
    /// `HealPersonality.cs:22-34` — `Key` :22, `ClassicKey` :27,
    /// `SupportMagnitude` :28, `MagnitudeCosts` :33, `healStat = Personality`
    /// :34; payload `HealEffect.cs:30`.
    EffectProperties::new(
        "Heal-Personality",
        classic_key(10, 5),
        MagicSkill::Restoration,
        &[Payload::HealAttribute(Attribute::Personality)],
    )
    .magnitude(make_effect_costs(40, 28, 0)),
    /// `HealSpeed.cs:22-34` — `Key` :22, `ClassicKey` :27, `SupportMagnitude`
    /// :28, `MagnitudeCosts` :33, `healStat = Speed` :34; payload
    /// `HealEffect.cs:30`.
    EffectProperties::new(
        "Heal-Speed",
        classic_key(10, 6),
        MagicSkill::Restoration,
        &[Payload::HealAttribute(Attribute::Speed)],
    )
    .magnitude(make_effect_costs(40, 28, 0)),
    /// `HealSpellPoints.cs:21-29` — `Key` :21, `SupportMagnitude` :26; the file
    /// declares **no** `ClassicKey`, `MagicSkill` or `MagnitudeCosts` (:23-29),
    /// so `classic_key = 0` (contract §3), `MagicSkill::None` (default,
    /// `EntityEffect.cs:302`) and the default `MagnitudeCosts` struct `(0,0,0)`
    /// (`MagicAndEffectsStructs.cs:58-64`); payload `IncreaseMagicka(magnitude)`
    /// :64 (`MagicRound` :53).
    EffectProperties::new(
        "Heal-SpellPoints",
        0,
        MagicSkill::None,
        &[Payload::Resource {
            resource: Resource::SpellPoints,
            sign: 1,
        }],
    )
    .magnitude(make_effect_costs(0, 0, 0)),
    /// `HealStrength.cs:22-34` — `Key` :22, `ClassicKey` :27, `SupportMagnitude`
    /// :28, `MagnitudeCosts` :33, `healStat = Strength` :34; payload
    /// `HealEffect.cs:30`.
    EffectProperties::new(
        "Heal-Strength",
        classic_key(10, 0),
        MagicSkill::Restoration,
        &[Payload::HealAttribute(Attribute::Strength)],
    )
    .magnitude(make_effect_costs(40, 28, 0)),
    /// `HealWillpower.cs:22-34` — `Key` :22, `ClassicKey` :27,
    /// `SupportMagnitude` :28, `MagnitudeCosts` :33, `healStat = Willpower` :34;
    /// payload `HealEffect.cs:30`.
    EffectProperties::new(
        "Heal-Willpower",
        classic_key(10, 2),
        MagicSkill::Restoration,
        &[Payload::HealAttribute(Attribute::Willpower)],
    )
    .magnitude(make_effect_costs(40, 28, 0)),
    /// `Regenerate.cs:23-36` — `Key` :23, `ClassicKey` :28, `SupportDuration` :29,
    /// `SupportMagnitude` :30, `DurationCosts` :35, `MagnitudeCosts` :36; payload
    /// `IncreaseHealth(GetMagnitude(caster))` :76 (`MagicRound` :66).
    EffectProperties::new(
        "Regenerate",
        classic_key(18, 255),
        MagicSkill::Restoration,
        &[Payload::ResourcePerRound {
            resource: Resource::Health,
            sign: 1,
        }],
    )
    .duration(make_effect_costs(100, 20, 0))
    .magnitude(make_effect_costs(8, 8, 0)),
    /// `SpellAbsorption.cs:22-36` — `Key` :22, `ClassicKey` :27,
    /// `SupportDuration` :28, `SupportChance` :29, `ChanceFunction.Custom` :30,
    /// `DurationCosts` :35, `ChanceCosts` :36; payload `ChanceState::SpellAbsorption`
    /// (the chance value is consumed per incoming spell, `EntityEffectManager.TryResistance`).
    EffectProperties::new(
        "SpellAbsorption",
        classic_key(20, 255),
        MagicSkill::Restoration,
        &[Payload::ChanceState(ChanceState::SpellAbsorption)],
    )
    .duration(make_effect_costs(28, 140, 0))
    .chance(make_effect_costs(28, 140, 0))
    .chance_fn(ChanceFunction::Custom),
];

// CONTRACT-GAP: `HealEffect.cs:19` — the DFU base documents that it heals
// only `DrainEffect` incumbents and never attribute loss caused by disease; the
// `HealAttribute` payload is generic and carries no source filter, so the
// engine must keep disease-caused attribute loss out of its heal path.
