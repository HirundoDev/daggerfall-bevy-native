//! Destruction family — `Assets/Scripts/Game/MagicAndEffects/Effects/Destruction/`
//! (DFU `0038a1e6`).
//!
//! Census: **28** `.cs` files, **3** abstract bases that are *not* templates
//! (`DrainEffect.cs:25`, `TransferEffect.cs:18`, `VampiricFortifyEffect.cs:26`)
//! ⇒ **25** concrete templates, one row each, in DFU file order. DFU does not
//! register the abstract classes either: `ReflectiveEnumerator.GetEnumerableOfType`
//! filters `IsAbstract` (`EntityEffectBroker.cs:1126-1134`); their payload runs
//! from the subclass (e.g. `DrainEffect.cs:122` applied per `Drain-*` template).
//!
//! Not registered because they are abstract: `DrainEffect.cs`, `TransferEffect.cs`
//! and `VampiricFortifyEffect.cs` (no template payload of their own; the last one
//! has no upstream subclass).
//!
//! Mapping (contract §4.1): `Damage-*` ⇒ `Payload::Resource { sign: -1 }`;
//! `ContinuousDamage-*` ⇒ `Payload::ResourcePerRound { sign: -1 }`;
//! `Drain-<Attr>` ⇒ `Payload::DrainAttribute`; `Transfer-<Attr>` ⇒
//! `Payload::DrainAttribute` + `Payload::HealCaster` (caster heal of the same
//! magnitude, `TransferEffect.cs:37-43`); `Transfer-Health/Fatigue` ⇒
//! `Payload::TransferResource` (both use `BaseEntityEffect` and only the
//! `MagicRound` heal of the caster, `TransferHealth.cs:44-58`); `Disintegrate`
//! ⇒ `Payload::Request(Request::Disintegrate)`.
//!
//! Citations: every row carries its `file:line` on the comment immediately
//! above it. They are `//` rather than `///` because a doc comment is an item
//! attribute and an array element is an expression position (`///` there is
//! `stmt_expr_attributes`, unstable/E0658) — the citation contract is the same.
//! `AllowedTargets`/`AllowedElements`/`AllowedCraftingStations`/`ShowSpellIcon`
//! are not fields of `EffectProperties` (contract §2), so they are not carried.

use crate::magic::{
    classic_key, make_effect_costs, Attribute, EffectProperties, MagicSkill, Payload, Request,
    Resource,
};

/// The 25 concrete effect templates of `Effects/Destruction/` (165-row registry
/// of contract §1). Keys and classic keys are the literals of each
/// `SetProperties()`; costs are the literal `MakeEffectCosts(...)` of the same
/// block.
pub const DESTRUCTION: &[EffectProperties] = &[
    // ContinuousDamageFatigue.cs:26-37 (SetProperties) · :56 (MagicRound damage).
    EffectProperties::new(
        "ContinuousDamage-Fatigue",
        classic_key(1, 1),
        MagicSkill::Destruction,
        &[Payload::ResourcePerRound {
            resource: Resource::Fatigue,
            sign: -1,
        }],
    )
    .duration(make_effect_costs(20, 8, 0))
    .magnitude(make_effect_costs(40, 28, 0)),
    // ContinuousDamageHealth.cs:26-37 (SetProperties) · :56 (MagicRound damage).
    EffectProperties::new(
        "ContinuousDamage-Health",
        classic_key(1, 0),
        MagicSkill::Destruction,
        &[Payload::ResourcePerRound {
            resource: Resource::Health,
            sign: -1,
        }],
    )
    .duration(make_effect_costs(28, 8, 0))
    .magnitude(make_effect_costs(40, 28, 0)),
    // ContinuousDamageSpellPoints.cs:26-37 (SetProperties) · :56 (MagicRound damage).
    EffectProperties::new(
        "ContinuousDamage-SpellPoints",
        classic_key(1, 2),
        MagicSkill::Destruction,
        &[Payload::ResourcePerRound {
            resource: Resource::SpellPoints,
            sign: -1,
        }],
    )
    .duration(make_effect_costs(40, 8, 0))
    .magnitude(make_effect_costs(40, 28, 0)),
    // DamageFatigue.cs:25-34 (SetProperties) · :53 (MagicRound damage).
    EffectProperties::new(
        "Damage-Fatigue",
        classic_key(4, 1),
        MagicSkill::Destruction,
        &[Payload::Resource {
            resource: Resource::Fatigue,
            sign: -1,
        }],
    )
    .magnitude(make_effect_costs(20, 28, 0)),
    // DamageHealth.cs:26-35 (SetProperties) · :54 (MagicRound damage).
    EffectProperties::new(
        "Damage-Health",
        classic_key(4, 0),
        MagicSkill::Destruction,
        &[Payload::Resource {
            resource: Resource::Health,
            sign: -1,
        }],
    )
    .magnitude(make_effect_costs(20, 28, 0)),
    // DamageSpellPoints.cs:25-34 (SetProperties) · :53 (MagicRound damage).
    EffectProperties::new(
        "Damage-SpellPoints",
        classic_key(4, 2),
        MagicSkill::Destruction,
        &[Payload::Resource {
            resource: Resource::SpellPoints,
            sign: -1,
        }],
    )
    .magnitude(make_effect_costs(20, 28, 0)),
    // Disintegrate.cs:25-34 (SetProperties) · :51 (MagicRound: SetHealth(0)).
    EffectProperties::new(
        "Disintegrate",
        classic_key(5, 255),
        MagicSkill::Destruction,
        &[Payload::Request(Request::Disintegrate)],
    )
    .chance(make_effect_costs(80, 140, 0)),
    // DrainAgility.cs:24-35 (SetProperties, drainStat = Agility) · DrainEffect.cs:122.
    EffectProperties::new(
        "Drain-Agility",
        classic_key(7, 3),
        MagicSkill::Destruction,
        &[Payload::DrainAttribute(Attribute::Agility)],
    )
    .magnitude(make_effect_costs(8, 100, 116)),
    // DrainEndurance.cs:24-35 (SetProperties, drainStat = Endurance) · DrainEffect.cs:122.
    EffectProperties::new(
        "Drain-Endurance",
        classic_key(7, 4),
        MagicSkill::Destruction,
        &[Payload::DrainAttribute(Attribute::Endurance)],
    )
    .magnitude(make_effect_costs(8, 100, 116)),
    // DrainIntelligence.cs:24-35 (SetProperties, drainStat = Intelligence) · DrainEffect.cs:122.
    EffectProperties::new(
        "Drain-Intelligence",
        classic_key(7, 1),
        MagicSkill::Destruction,
        &[Payload::DrainAttribute(Attribute::Intelligence)],
    )
    .magnitude(make_effect_costs(8, 100, 116)),
    // DrainLuck.cs:24-35 (SetProperties, drainStat = Luck) · DrainEffect.cs:122.
    EffectProperties::new(
        "Drain-Luck",
        classic_key(7, 7),
        MagicSkill::Destruction,
        &[Payload::DrainAttribute(Attribute::Luck)],
    )
    .magnitude(make_effect_costs(8, 100, 116)),
    // DrainPersonality.cs:24-35 (SetProperties, drainStat = Personality) · DrainEffect.cs:122.
    EffectProperties::new(
        "Drain-Personality",
        classic_key(7, 5),
        MagicSkill::Destruction,
        &[Payload::DrainAttribute(Attribute::Personality)],
    )
    .magnitude(make_effect_costs(8, 100, 116)),
    // DrainSpeed.cs:24-35 (SetProperties, drainStat = Speed) · DrainEffect.cs:122.
    EffectProperties::new(
        "Drain-Speed",
        classic_key(7, 6),
        MagicSkill::Destruction,
        &[Payload::DrainAttribute(Attribute::Speed)],
    )
    .magnitude(make_effect_costs(8, 100, 116)),
    // DrainStrength.cs:24-35 (SetProperties, drainStat = Strength) · DrainEffect.cs:122.
    EffectProperties::new(
        "Drain-Strength",
        classic_key(7, 0),
        MagicSkill::Destruction,
        &[Payload::DrainAttribute(Attribute::Strength)],
    )
    .magnitude(make_effect_costs(8, 100, 116)),
    // DrainWillpower.cs:24-35 (SetProperties, drainStat = Willpower) · DrainEffect.cs:122.
    EffectProperties::new(
        "Drain-Willpower",
        classic_key(7, 2),
        MagicSkill::Destruction,
        &[Payload::DrainAttribute(Attribute::Willpower)],
    )
    .magnitude(make_effect_costs(8, 100, 116)),
    // TransferAgility.cs:24-35 (SetProperties, drainStat = Agility) · DrainEffect.cs:122
    // (drain target) · TransferEffect.cs:41 (HealAttribute caster, same magnitude).
    EffectProperties::new(
        "Transfer-Agility",
        classic_key(11, 3),
        MagicSkill::Destruction,
        &[
            Payload::DrainAttribute(Attribute::Agility),
            Payload::HealCaster(Attribute::Agility),
        ],
    )
    .magnitude(make_effect_costs(60, 100, 40)),
    // TransferEndurance.cs:24-35 (SetProperties, drainStat = Endurance) · DrainEffect.cs:122
    // (drain target) · TransferEffect.cs:41 (HealAttribute caster, same magnitude).
    EffectProperties::new(
        "Transfer-Endurance",
        classic_key(11, 4),
        MagicSkill::Destruction,
        &[
            Payload::DrainAttribute(Attribute::Endurance),
            Payload::HealCaster(Attribute::Endurance),
        ],
    )
    .magnitude(make_effect_costs(60, 100, 40)),
    // TransferFatigue.cs:25-35 (SetProperties) · :54-55 (MagicRound: damage target,
    // IncreaseFatigue caster).
    EffectProperties::new(
        "Transfer-Fatigue",
        classic_key(11, 9),
        MagicSkill::Destruction,
        &[Payload::TransferResource(Resource::Fatigue)],
    )
    .magnitude(make_effect_costs(60, 100, 40)),
    // TransferHealth.cs:26-36 (SetProperties) · :55-56 (MagicRound: damage target,
    // IncreaseHealth caster).
    EffectProperties::new(
        "Transfer-Health",
        classic_key(11, 8),
        MagicSkill::Destruction,
        &[Payload::TransferResource(Resource::Health)],
    )
    .magnitude(make_effect_costs(60, 100, 40)),
    // TransferIntelligence.cs:24-35 (SetProperties, drainStat = Intelligence) · DrainEffect.cs:122
    // (drain target) · TransferEffect.cs:41 (HealAttribute caster, same magnitude).
    EffectProperties::new(
        "Transfer-Intelligence",
        classic_key(11, 1),
        MagicSkill::Destruction,
        &[
            Payload::DrainAttribute(Attribute::Intelligence),
            Payload::HealCaster(Attribute::Intelligence),
        ],
    )
    .magnitude(make_effect_costs(60, 100, 40)),
    // TransferLuck.cs:24-35 (SetProperties, drainStat = Luck) · DrainEffect.cs:122
    // (drain target) · TransferEffect.cs:41 (HealAttribute caster, same magnitude).
    EffectProperties::new(
        "Transfer-Luck",
        classic_key(11, 7),
        MagicSkill::Destruction,
        &[
            Payload::DrainAttribute(Attribute::Luck),
            Payload::HealCaster(Attribute::Luck),
        ],
    )
    .magnitude(make_effect_costs(60, 100, 40)),
    // TransferPersonality.cs:24-35 (SetProperties, drainStat = Personality) · DrainEffect.cs:122
    // (drain target) · TransferEffect.cs:41 (HealAttribute caster, same magnitude).
    EffectProperties::new(
        "Transfer-Personality",
        classic_key(11, 5),
        MagicSkill::Destruction,
        &[
            Payload::DrainAttribute(Attribute::Personality),
            Payload::HealCaster(Attribute::Personality),
        ],
    )
    .magnitude(make_effect_costs(60, 100, 40)),
    // TransferSpeed.cs:24-35 (SetProperties, drainStat = Speed) · DrainEffect.cs:122
    // (drain target) · TransferEffect.cs:41 (HealAttribute caster, same magnitude).
    EffectProperties::new(
        "Transfer-Speed",
        classic_key(11, 6),
        MagicSkill::Destruction,
        &[
            Payload::DrainAttribute(Attribute::Speed),
            Payload::HealCaster(Attribute::Speed),
        ],
    )
    .magnitude(make_effect_costs(60, 100, 40)),
    // TransferStrength.cs:24-35 (SetProperties, drainStat = Strength) · DrainEffect.cs:122
    // (drain target) · TransferEffect.cs:41 (HealAttribute caster, same magnitude).
    EffectProperties::new(
        "Transfer-Strength",
        classic_key(11, 0),
        MagicSkill::Destruction,
        &[
            Payload::DrainAttribute(Attribute::Strength),
            Payload::HealCaster(Attribute::Strength),
        ],
    )
    .magnitude(make_effect_costs(60, 100, 40)),
    // TransferWillpower.cs:24-35 (SetProperties, drainStat = Willpower) · DrainEffect.cs:122
    // (drain target) · TransferEffect.cs:41 (HealAttribute caster, same magnitude).
    EffectProperties::new(
        "Transfer-Willpower",
        classic_key(11, 2),
        MagicSkill::Destruction,
        &[
            Payload::DrainAttribute(Attribute::Willpower),
            Payload::HealCaster(Attribute::Willpower),
        ],
    )
    .magnitude(make_effect_costs(60, 100, 40)),
];

// CENSUS -- the ticket says "26 rows (28 files - 2 abstract)"; measured on the
// source it is **25**: this family has **3** abstract bases. `DrainEffect` and
// `TransferEffect` are the two the ticket names, and `VampiricFortifyEffect` is
// `public abstract class VampiricFortifyEffect : IncumbentEffect`
// (`VampiricFortifyEffect.cs:26`) with the header "WORK IN PROGRESS - Not currently
// implemented by any effects" (`VampiricFortifyEffect.cs:19-22`): it has no
// subclass, so no template executes its payload. DFU does not register it
// either (`IsAbstract` filter, `EntityEffectBroker.cs:1126-1134`). Adding a row
// would invent a template that does not exist, so the faithful census is 28 - 3 = 25.
//
// CONTRACT-GAP: none. The family's 25 payloads fall into existing primitives of
// contract §2 (`Resource`, `ResourcePerRound`, `DrainAttribute`,
// `HealCaster`, `TransferResource`, `Request`); no falta ningún primitivo.
