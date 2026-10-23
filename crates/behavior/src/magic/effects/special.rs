//! Special family — `Assets/Scripts/Game/MagicAndEffects/Effects/Special/`
//! (DFU `0038a1e6`).
//!
//! Census: **13** `.cs` files, **1** abstract base that is *not* a template
//! (`RacialOverrideEffect.cs:24`, `public abstract class RacialOverrideEffect :
//! IncumbentEffect`) ⇒ **12** concrete templates, one row each, in DFU file
//! order. DFU does not register the abstract base either
//! (`EntityEffectBroker.cs:1126-1134` filters `IsAbstract`); its payload
//! (`CustomRace`, spell points, paper doll) runs from `LycanthropyEffect` and
//! `VampirismEffect`, and each of those carries its own row.
//!
//! Two shapes live here (contract §4.1):
//! * **living conditions** (`Lycanthropy-Curse`, `Vampirism-Curse`,
//!   `Passive-Specials`) are flat composite payloads — every state they mutate
//!   while incumbent is declared at top level;
//! * **item artifacts** (`Azuras_Star`, `Mace_of_Molag_Bal`, `Masque_of_Clavicus`,
//!   `Mehrunes_Razor`, `Oghma_Infinium`, `Ring_of_Namira`, `Sanguine_Rose`,
//!   `Skull_of_Corruption`, `Wabbajack`) are `Payload::On { trigger, action }`
//!   with the `properties.EnchantmentPayloadFlags` of their `SetProperties()`.
//!
//! Abstractos no registrados: `RacialOverrideEffect.cs` (sin payload de plantilla).
//!
//! None of the 12 declares `properties.ClassicKey`, `properties.MagicSkill` or
//! `DefaultEffectSettings`: `classic_key` is `0`, `magic_skill` is
//! `MagicSkill::None`, and no row overrides settings.
//!
//! Citations sit on the `//` comment immediately above each row (an array
//! element is an expression position, where `///` would require
//! `stmt_expr_attributes`, E0658).

use crate::magic::{
    Amount, Attribute, EffectProperties, EffectSkill, Flag, Gate, MagicSkill, Payload, Request,
    Resource, SocialGroup, Trigger,
};

/// The 12 concrete templates of `Effects/Special/` (165-row registry of
/// contract §1).
pub const SPECIAL: &[EffectProperties] = &[
    // AzurasStarEffect.cs:27-32 (SetProperties: Held|Used) · :36-50 (ConstantEffect sets
    // `PlayerEntity.IsAzurasStarEquipped`) · :61-72 (Used releases the trapped soul:
    // `TrappedSoulType = None`). The released soul is an item mutation, not a `Request`.
    EffectProperties::new(
        "Azuras_Star",
        0,
        MagicSkill::None,
        &[
            Payload::On {
                trigger: Trigger::Held,
                action: &[],
            },
            Payload::On {
                trigger: Trigger::Used,
                action: &[Payload::Request(Request::SoulCapture)],
            },
        ],
    ),
    // LycanthropyEffect.cs:138-143 (SetProperties: `Lycanthropy-Curse`, no payload flags;
    // IncumbentEffect lives while cursed) · :145-166 (Start cures the stage-one disease,
    // `CureAll()` at :162) · :184-242 (ConstantEffect: ImmuneToDisease at :194, silver-only
    // while transformed at :198, `SetMaxHealthLimiter` at :237) · :244-260 (MagicRound) ·
    // :566-585 (ApplyLycanthropeAdvantages: +40 Str/Agi/End/Spd at :570-573, +30
    // Swimming/Running/Stealth/CriticalStrike/Climbing/HandToHand/Jumping at :577-583).
    // The infection type selects Werewolf or Wereboar (`LycanthropyTypes`); both flags are
    // declared because the template carries both outcomes. `HealthLimiter` is dynamic
    // (`RawMaxHealth - elapsed * 24/min`, floor 4, :232-237) ⇒ `Amount::Rolled`.
    EffectProperties::new(
        "Lycanthropy-Curse",
        0,
        MagicSkill::None,
        &[
            Payload::Skill(EffectSkill::Swimming, Amount::Fixed(30)),
            Payload::Skill(EffectSkill::Running, Amount::Fixed(30)),
            Payload::Skill(EffectSkill::Stealth, Amount::Fixed(30)),
            Payload::Skill(EffectSkill::CriticalStrike, Amount::Fixed(30)),
            Payload::Skill(EffectSkill::Climbing, Amount::Fixed(30)),
            Payload::Skill(EffectSkill::HandToHand, Amount::Fixed(30)),
            Payload::Skill(EffectSkill::Jumping, Amount::Fixed(30)),
            Payload::MaxAttribute(Attribute::Strength, Amount::Fixed(40)),
            Payload::MaxAttribute(Attribute::Agility, Amount::Fixed(40)),
            Payload::MaxAttribute(Attribute::Endurance, Amount::Fixed(40)),
            Payload::MaxAttribute(Attribute::Speed, Amount::Fixed(40)),
            Payload::Flag(Flag::ImmuneToDisease),
            Payload::Flag(Flag::Werewolf),
            Payload::Flag(Flag::Wereboar),
            Payload::HealthLimiter(Amount::Rolled),
            Payload::Request(Request::CureAll),
        ],
    ),
    // MaceOfMolagBalEffect.cs:34-39 (SetProperties: Strikes|Held) · :53-64 (ConstantEffect
    // re-applies `ChangeMaxMagickaModifier(currentMaxMagickaIncrease)` at :63) ·
    // :66-77 (MagicRound wipes the temporary Str/max-magicka bonus after 12 minutes) ·
    // :80-160 (Strikes, `sourceEntity` = bearer, `targetEntity` = victim: drains spell
    // points at :113 and gives them to the bearer at :126 with the overflow raising its
    // max magicka at :122, or drains 1-6 strength at :131 and buffs the bearer's max and
    // current Str at :138-139).
    //
    // Declared residuals (no conditional primitive in the vocabulary):
    // * the two branches are exclusive in DFU (`if (CurrentMagicka > 0)` at :111); the
    //   rows below declare both halves, and applying both is the union DFU never runs at
    //   once. The selector belongs to the item/spell owner, like `GatedDamage`'s gate.
    // * the bearer's buff amount is the drained amount (`strengthDrained`, the SP
    //   overflow), not `EnchantmentParam.Value`; the payloads use the item magnitude.
    // * `durabilityLoss = sourceDamage` (:150) is wear equal to the damage: the owner
    //   applies it (`Payload::ItemCondition` only carries a constant delta).
    EffectProperties::new(
        "Mace_of_Molag_Bal",
        0,
        MagicSkill::None,
        &[
            Payload::On {
                trigger: Trigger::Held,
                action: &[Payload::MaxMagicka(Amount::Rolled)],
            },
            Payload::On {
                trigger: Trigger::Strikes,
                action: &[
                    Payload::TransferResource(Resource::SpellPoints),
                    Payload::DrainAttribute(Attribute::Strength),
                    Payload::Bearer(&[
                        Payload::MaxMagicka(Amount::Rolled),
                        Payload::MaxAttribute(Attribute::Strength, Amount::Rolled),
                        Payload::FortifyAttribute(Attribute::Strength),
                    ]),
                ],
            },
        ],
    ),
    // MasqueOfClavicusEffect.cs:26-31 (SetProperties: Held) · :35-47 (MagicRound improves
    // every social group by `LivePersonality / 5`, `ChangeReactionMod` at :43).
    EffectProperties::new(
        "Masque_of_Clavicus",
        0,
        MagicSkill::None,
        &[Payload::On {
            trigger: Trigger::Held,
            action: &[Payload::Reaction(SocialGroup::All, Amount::Rolled)],
        }],
    ),
    // MehrunesRazorEffect.cs:26-31 (SetProperties: Strikes) · :35-60 (callback: failed
    // magic save ⇒ `strikesModulateDamage = CurrentHealth`, i.e. instant kill, :44-55).
    EffectProperties::new(
        "Mehrunes_Razor",
        0,
        MagicSkill::None,
        &[Payload::On {
            trigger: Trigger::Strikes,
            action: &[Payload::Request(Request::Instakill)],
        }],
    ),
    // OghmaInfiniumEffect.cs:24-29 (SetProperties: Used) · :33-55 (callback sets
    // `ReadyToLevelUp`/`OghmaLevelUp` at :41-42 and removes the book).
    EffectProperties::new(
        "Oghma_Infinium",
        0,
        MagicSkill::None,
        &[Payload::On {
            trigger: Trigger::Used,
            action: &[Payload::Request(Request::LevelUp)],
        }],
    ),
    // PassiveSpecialsEffect.cs:45-51 (SetProperties: `Passive-Specials`, no payload flags) ·
    // :84-94 (ConstantEffect: LightPoweredMagery/DarknessPoweredMagery) · :96-106
    // (MagicRound) · :109-147 (regeneration +1 health, career-gated) · :149-176 (sun
    // damage 12 at :170) · :179-206 (holy-place damage 12 at :200) · :207-250 (max
    // magicka −33% at :218/:237, unable-to-cast −10000000 at :222/:241).
    //
    // The regeneration is the **literal** `regenerateAmount = 1`
    // (`PassiveSpecialsEffect.cs:33`) applied by `IncreaseHealth(regenerateAmount)`
    // (`:142`) — not a `GetMagnitude()` roll. This template declares no
    // `SupportMagnitude`, so a `ResourcePerRound` row would roll 0 and heal
    // nothing (a dead payload the `PROBE-EFFECT-001` F-p detector flags); the
    // constant payload carries the literal instead, like `HealthLeech`'s.
    EffectProperties::new(
        "Passive-Specials",
        0,
        MagicSkill::None,
        &[
            Payload::MaxMagicka(Amount::Percent(-33)),
            Payload::MaxMagicka(Amount::Fixed(-10_000_000)),
            Payload::FixedResource {
                resource: Resource::Health,
                sign: 1,
                amount: 1,
            },
            Payload::GatedDamage {
                gate: Gate::Sunlight,
                amount: 12,
            },
            Payload::GatedDamage {
                gate: Gate::HolyPlace,
                amount: 12,
            },
        ],
    ),
    // RingOfNamiraEffect.cs:24-29 (`SetProperties`: `EnchantmentPayloadFlags.None`) ·
    // :33-72 (callback invoked by `FormulaHelper.cs:704-719` when the struck entity is
    // the player: team-scaled reflect of `sourceDamage` — 0 vs Vermin/Spriggans, ×1/2 vs
    // Daedra at :50, ×2 vs Undead at :53, ×1 otherwise at :57,
    // `enemy.CurrentHealth -= reflectedDamage` at :60 — plus the item's durability cost
    // at :65). Damage reflection has no payload primitive (CONTRACT-GAP): the attacker,
    // the scaling and the durability loss belong to the combat owner, and the effect
    // declares the request.
    EffectProperties::new(
        "Ring_of_Namira",
        0,
        MagicSkill::None,
        &[Payload::On {
            trigger: Trigger::Struck,
            action: &[Payload::Request(Request::ReflectDamage)],
        }],
    ),
    // SanguineRoseEffect.cs:29-34 (SetProperties: Used) · :38-65 (callback summons one
    // allied Daedroth when an enemy is within 12 m, :56).
    EffectProperties::new(
        "Sanguine_Rose",
        0,
        MagicSkill::None,
        &[Payload::On {
            trigger: Trigger::Used,
            action: &[Payload::Request(Request::SpawnAlly)],
        }],
    ),
    // SkullOfCorruptionEffect.cs:29-34 (SetProperties: Used) · :38-90 (callback clones the
    // nearest non-allied enemy as a player ally, :76).
    EffectProperties::new(
        "Skull_of_Corruption",
        0,
        MagicSkill::None,
        &[Payload::On {
            trigger: Trigger::Used,
            action: &[Payload::Request(Request::Clone)],
        }],
    ),
    // VampirismEffect.cs:73-78 (SetProperties: `Vampirism-Curse`, no payload flags;
    // IncumbentEffect lives while cursed) · :80-108 (Start cures the stage-one disease,
    // `CureAll()` at :98) · :112-125 (ConstantEffect: ImmuneToDisease at :122,
    // immune-to-paralysis at :123 ⇒ `Flag::FreeAction` (`IsImmuneToParalysis`),
    // silver-only at :124) · :127-131 (MagicRound) ·
    // :346-371 (ApplyVampireAdvantages: +20 Str/Wil/Agi/End/Per/Spd/Luc at :350-356,
    // +20 Intelligence for clan Anthotis at :369, +30 Jumping/Running/Stealth/
    // CriticalStrike/Climbing/HandToHand at :360-365).
    EffectProperties::new(
        "Vampirism-Curse",
        0,
        MagicSkill::None,
        &[
            Payload::MaxAttribute(Attribute::Strength, Amount::Fixed(20)),
            Payload::MaxAttribute(Attribute::Willpower, Amount::Fixed(20)),
            Payload::MaxAttribute(Attribute::Agility, Amount::Fixed(20)),
            Payload::MaxAttribute(Attribute::Endurance, Amount::Fixed(20)),
            Payload::MaxAttribute(Attribute::Personality, Amount::Fixed(20)),
            Payload::MaxAttribute(Attribute::Speed, Amount::Fixed(20)),
            Payload::MaxAttribute(Attribute::Luck, Amount::Fixed(20)),
            Payload::MaxAttribute(Attribute::Intelligence, Amount::Fixed(20)),
            Payload::Skill(EffectSkill::Jumping, Amount::Fixed(30)),
            Payload::Skill(EffectSkill::Running, Amount::Fixed(30)),
            Payload::Skill(EffectSkill::Stealth, Amount::Fixed(30)),
            Payload::Skill(EffectSkill::CriticalStrike, Amount::Fixed(30)),
            Payload::Skill(EffectSkill::Climbing, Amount::Fixed(30)),
            Payload::Skill(EffectSkill::HandToHand, Amount::Fixed(30)),
            Payload::Flag(Flag::ImmuneToDisease),
            Payload::Flag(Flag::FreeAction),
            Payload::Flag(Flag::Vampire),
            Payload::Request(Request::CureAll),
        ],
    ),
    // WabbajackEffect.cs:46-52 (SetProperties: Strikes, `bypassSavingThrows = true`) ·
    // :56-100 (callback turns the struck enemy into a random monster from `careerIDs`).
    EffectProperties::new(
        "Wabbajack",
        0,
        MagicSkill::None,
        &[Payload::On {
            trigger: Trigger::Strikes,
            action: &[Payload::Request(Request::Wabbajack)],
        }],
    ),
];

// CONTRACT-GAP: `SetStatMod` vs `MaxAttribute`. `LycanthropyEffect.cs:570-573` and
// `VampirismEffect.cs:350-356,369` call `SetStatMod` (a live stat modifier), while the
// closest payload primitive is `MaxAttribute` (`ChangeStatMaxMod`); the vocabulary has no
// `SetStatMod` equivalent carrying an amount (`FortifyAttribute` has no amount). The
// declared amounts (+40 / +20) are the source literals.
//
// CONTRACT-GAP: `PlayerEntity.IsAzurasStarEquipped` (`AzurasStarEffect.cs:48`) has no
// `Flag` variant; the `Held` trigger of the row carries an empty action.
//
// CONTRACT-GAP: damage reflection (`RingOfNamiraEffect.cs:37-66`: incoming damage
// reflected at ×0/×0.5/×2 by target team, durability cost = reflected damage) has no
// payload primitive, so the row carries an empty payload list.
//
// CONTRACT-GAP: magical "minimum metal to hit" (`LycanthropyEffect.cs:197-200`,
// `VampirismEffect.cs:124,271`) has no payload primitive.
//
// CONTRACT-GAP: `Passive-Specials` gates. `RegenerationFlags` selects Always /
// InDarkness / InLight / InWater (`PassiveSpecialsEffect.cs:115-138`) and the two magery
// disadvantages are selected by career flags (`:207-250`); the table declares every
// alternative because `EffectProperties` has no career/param slot and no
// `Gated { gate, payload }` wrapper exists (only `GatedDamage` accepts a `Gate`).
//
// CONTRACT-GAP: soul release. `AzurasStarEffect.cs:61-72` empties the trapped soul of
// the item; `Request::SoulCapture` (the only soul variant of the vocabulary) is used as
// the closest primitive, but it captures rather than releases.
