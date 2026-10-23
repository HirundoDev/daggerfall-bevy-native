//! Enchanting family — `Assets/Scripts/Game/MagicAndEffects/Effects/Enchanting/`
//! (DFU `0038a1e6`).
//!
//! Census: **24** `.cs` files, **24** concrete `BaseEntityEffect` templates
//! (no abstract base in this family) ⇒ **24** rows, one per file, in DFU file
//! order. None of them declares `properties.ClassicKey`, `properties.MagicSkill`
//! or `DefaultEffectSettings`: `classic_key` is `0`, `magic_skill` is
//! `MagicSkill::None` and no row overrides settings.
//!
//! Every row is `Payload::On { trigger, action }`: `trigger` is the
//! `properties.EnchantmentPayloadFlags` of its `SetProperties()` and `action`
//! holds the primitives the payload method actually executes
//! (`ConstantEffect` for `Held`, `EnchantmentPayloadCallback` for the rest,
//! `MagicRound` for the per-round payloads). `Payload::Request(
//! Request::CastSpell(0))` stands for `CastWhenHeld/Strikes/Used`: the classic
//! spell `ClassicID` is chosen by the item (`EnchantmentParam.ClassicParam`,
//! `CastWhenHeld.cs:48-100`), so the template carries the placeholder `0`.
//!
//! Abstractos no registrados: ninguno (24 concretos).
//!
//! Param-dependent payloads (classic params select a group/skill/condition)
//! are declared with **every** alternative the source enumerates, because
//! `EffectProperties` has no enchantment-param slot; see `CONTRACT-GAP` at the
//! end of this file. `AllowedTargets`, `AllowedCraftingStations`,
//! `ItemMakerFlags`, `ShowSpellIcon` and `EnchantCost` are not fields of
//! `EffectProperties` (contract §2) and are therefore not carried.
//!
//! Citations sit on the `//` comment immediately above each row (an array
//! element is an expression position, where `///` would require
//! `stmt_expr_attributes`, E0658).

use crate::magic::{
    Amount, EffectProperties, EffectSkill, EnemyGroup, Flag, Gate, MagicSkill, Payload, Request,
    Resource, SocialGroup, Trigger,
};

/// The 24 concrete enchantment templates of `Effects/Enchanting/`
/// (165-row registry of contract §1).
pub const ENCHANTING: &[EffectProperties] = &[
    // AbsorbsSpells.cs:26-33 (SetProperties: Held) · :55-65 (ConstantEffect sets IsAbsorbingSpells).
    EffectProperties::new(
        "AbsorbsSpells",
        0,
        MagicSkill::None,
        &[Payload::On {
            trigger: Trigger::Held,
            action: &[Payload::Flag(Flag::SpellAbsorbing)],
        }],
    ),
    // BadReactionsFrom.cs:31-38 (SetProperties: Held) · :71-89 (ConstantEffect applies
    // armour + hit-chance penalties while a nearby group is affected) · :91-140 (MagicRound
    // picks the group param: Humanoids/Animals/Daedra).
    EffectProperties::new(
        "BadReactionsFrom",
        0,
        MagicSkill::None,
        &[Payload::On {
            trigger: Trigger::Held,
            action: &[
                Payload::ArmorValue(-5),
                Payload::ChanceToHit(Amount::Fixed(-5)),
            ],
        }],
    ),
    // BadRepWith.cs:32-39 (SetProperties: Held) · :30 (adjustmentAmount = -10) ·
    // :72-95 (MagicRound: one group, or all five when Params.All).
    EffectProperties::new(
        "BadRepWith",
        0,
        MagicSkill::None,
        &[Payload::On {
            trigger: Trigger::Held,
            action: &[Payload::Reaction(SocialGroup::All, Amount::Fixed(-10))],
        }],
    ),
    // CastWhenHeld.cs:32-39 (SetProperties: Equipped|MagicRound|RerollEffect) ·
    // :166 (Equipped assigns the classic spell bundle) · :138 (MagicRound durability loss) ·
    // :175-177 (RerollEffect recasts the bundle).
    EffectProperties::new(
        "CastWhenHeld",
        0,
        MagicSkill::None,
        &[
            Payload::On {
                trigger: Trigger::Equipped,
                action: &[Payload::Request(Request::CastSpell(0))],
            },
            Payload::On {
                trigger: Trigger::Reroll,
                action: &[Payload::Request(Request::CastSpell(0))],
            },
        ],
    ),
    // CastWhenStrikes.cs:32-39 (SetProperties: Strikes) · :125 (callback readies the
    // classic spell bundle on the struck target).
    EffectProperties::new(
        "CastWhenStrikes",
        0,
        MagicSkill::None,
        &[Payload::On {
            trigger: Trigger::Strikes,
            action: &[Payload::Request(Request::CastSpell(0))],
        }],
    ),
    // CastWhenUsed.cs:32-39 (SetProperties: Used) · :138 (callback assigns or readies the
    // classic spell bundle).
    EffectProperties::new(
        "CastWhenUsed",
        0,
        MagicSkill::None,
        &[Payload::On {
            trigger: Trigger::Used,
            action: &[Payload::Request(Request::CastSpell(0))],
        }],
    ),
    // EnhancesSkill.cs:30-37 (SetProperties: Held) · :28 (modAmount = 15) · :44-62
    // (every DFCareer.Skills value is a classic param) · :69-78 (Start sets the selected
    // skill). All 35 alternatives are declared in param order (see CONTRACT-GAP).
    EffectProperties::new(
        "EnhancesSkill",
        0,
        MagicSkill::None,
        &[Payload::On {
            trigger: Trigger::Held,
            action: &[
                Payload::Skill(EffectSkill::Medical, Amount::Fixed(15)),
                Payload::Skill(EffectSkill::Etiquette, Amount::Fixed(15)),
                Payload::Skill(EffectSkill::Streetwise, Amount::Fixed(15)),
                Payload::Skill(EffectSkill::Jumping, Amount::Fixed(15)),
                Payload::Skill(EffectSkill::Orcish, Amount::Fixed(15)),
                Payload::Skill(EffectSkill::Harpy, Amount::Fixed(15)),
                Payload::Skill(EffectSkill::Giantish, Amount::Fixed(15)),
                Payload::Skill(EffectSkill::Dragonish, Amount::Fixed(15)),
                Payload::Skill(EffectSkill::Nymph, Amount::Fixed(15)),
                Payload::Skill(EffectSkill::Daedric, Amount::Fixed(15)),
                Payload::Skill(EffectSkill::Spriggan, Amount::Fixed(15)),
                Payload::Skill(EffectSkill::Centaurian, Amount::Fixed(15)),
                Payload::Skill(EffectSkill::Impish, Amount::Fixed(15)),
                Payload::Skill(EffectSkill::Lockpicking, Amount::Fixed(15)),
                Payload::Skill(EffectSkill::Mercantile, Amount::Fixed(15)),
                Payload::Skill(EffectSkill::Pickpocket, Amount::Fixed(15)),
                Payload::Skill(EffectSkill::Stealth, Amount::Fixed(15)),
                Payload::Skill(EffectSkill::Swimming, Amount::Fixed(15)),
                Payload::Skill(EffectSkill::Climbing, Amount::Fixed(15)),
                Payload::Skill(EffectSkill::Backstabbing, Amount::Fixed(15)),
                Payload::Skill(EffectSkill::Dodging, Amount::Fixed(15)),
                Payload::Skill(EffectSkill::Running, Amount::Fixed(15)),
                Payload::Skill(EffectSkill::Destruction, Amount::Fixed(15)),
                Payload::Skill(EffectSkill::Restoration, Amount::Fixed(15)),
                Payload::Skill(EffectSkill::Illusion, Amount::Fixed(15)),
                Payload::Skill(EffectSkill::Alteration, Amount::Fixed(15)),
                Payload::Skill(EffectSkill::Thaumaturgy, Amount::Fixed(15)),
                Payload::Skill(EffectSkill::Mysticism, Amount::Fixed(15)),
                Payload::Skill(EffectSkill::ShortBlade, Amount::Fixed(15)),
                Payload::Skill(EffectSkill::LongBlade, Amount::Fixed(15)),
                Payload::Skill(EffectSkill::HandToHand, Amount::Fixed(15)),
                Payload::Skill(EffectSkill::Axe, Amount::Fixed(15)),
                Payload::Skill(EffectSkill::BluntWeapon, Amount::Fixed(15)),
                Payload::Skill(EffectSkill::Archery, Amount::Fixed(15)),
                Payload::Skill(EffectSkill::CriticalStrike, Amount::Fixed(15)),
            ],
        }],
    ),
    // ExtraSpellPts.cs:28-35 (SetProperties: Held) · :80-130 (ConstantEffect adds +75
    // max spell points when the classic param's condition holds: season 0-3, moon 4-6 or
    // nearby group 7-10; `maxIncrease = 75` at :84).
    EffectProperties::new(
        "ExtraSpellPts",
        0,
        MagicSkill::None,
        &[Payload::On {
            trigger: Trigger::Held,
            action: &[Payload::MaxMagicka(Amount::Fixed(75))],
        }],
    ),
    // ExtraWeight.cs:28-35 (SetProperties: Enchanted) · :26 (weightMultiplier = 4) ·
    // :61 (`sourceItem.weightInKg *= 4`). No primitive carries item weight (CONTRACT-GAP).
    EffectProperties::new(
        "ExtraWeight",
        0,
        MagicSkill::None,
        &[Payload::On {
            trigger: Trigger::Enchanted,
            action: &[Payload::WeightAllowance(400)],
        }],
    ),
    // FeatherWeight.cs:28-35 (SetProperties: Enchanted) · :26 (weightValue = 0.25f, a fixed
    // kg value, not a percentage) · :61 (`sourceItem.weightInKg = 0.25f`). No primitive
    // carries item weight; 25 is the closest scalar (CONTRACT-GAP).
    EffectProperties::new(
        "FeatherWeight",
        0,
        MagicSkill::None,
        &[Payload::On {
            trigger: Trigger::Enchanted,
            action: &[Payload::WeightAllowance(25)],
        }],
    ),
    // GoodRepWith.cs:28-35 (SetProperties: Held) · :26 (adjustmentAmount = +10) ·
    // :68-91 (MagicRound: one group, or all five when Params.All).
    EffectProperties::new(
        "GoodRepWith",
        0,
        MagicSkill::None,
        &[Payload::On {
            trigger: Trigger::Held,
            action: &[Payload::Reaction(SocialGroup::All, Amount::Fixed(10))],
        }],
    ),
    // HealthLeech.cs:32-39 (SetProperties: Held|Used|Strikes|Enchanted) · :27-30
    // (leechWeaponAmount = 8, leechCastAmount = 16, timeLeechAmount = 1 every 4 rounds) ·
    // :89 (Strikes) · :91 (Used) · :113-132 (MagicRound while the daily/weekly leech is due).
    // The Enchanted context only records `timeHealthLeechLastUsed` (:80-81), not a payload.
    EffectProperties::new(
        "HealthLeech",
        0,
        MagicSkill::None,
        &[
            // `leechWeaponAmount = 8` charged to the **bearer**
            // (`sourceEntity.Entity.DecreaseHealth(leechWeaponAmount)`, :88-89):
            // a strike trigger carries a `targetEntity`, so the scope is explicit.
            Payload::On {
                trigger: Trigger::Strikes,
                action: &[Payload::Bearer(&[Payload::FixedResource {
                    resource: Resource::Health,
                    sign: -1,
                    amount: 8,
                }])],
            },
            // `leechCastAmount = 16` (:28-30, `:91`).
            Payload::On {
                trigger: Trigger::Used,
                action: &[Payload::FixedResource {
                    resource: Resource::Health,
                    sign: -1,
                    amount: 16,
                }],
            },
            // `timeLeechAmount = 1` every 4 rounds (:113-132). The cadence is
            // item state (`timeHealthLeechLastUsed`, :80-81) that M23 does not
            // own: the owner fires this trigger only when the timer is due.
            Payload::On {
                trigger: Trigger::MagicRound,
                action: &[Payload::FixedResource {
                    resource: Resource::Health,
                    sign: -1,
                    amount: 1,
                }],
            },
        ],
    ),
    // ImprovesTalents.cs:25-32 (SetProperties: Held) · :65-87 (ConstantEffect sets one
    // talent flag selected by the classic param: Hearing/Athleticism/AdrenalineRush).
    // The contract's Payload::Skill doc names `ImprovedTalents`, but the source assigns
    // flags, so the source wins (all three alternatives declared, see CONTRACT-GAP).
    EffectProperties::new(
        "ImprovesTalents",
        0,
        MagicSkill::None,
        &[Payload::On {
            trigger: Trigger::Held,
            action: &[
                Payload::Flag(Flag::ImprovedAcuteHearing),
                Payload::Flag(Flag::ImprovedAthleticism),
                Payload::Flag(Flag::ImprovedAdrenalineRush),
            ],
        }],
    ),
    // IncreasedWeightAllowance.cs:25-31 (SetProperties: Held) · :64-88 (ConstantEffect:
    // param 0 = +25%, param 1 = +50%, `SetIncreasedWeightAllowanceMultiplier(0.25f/0.5f)`).
    EffectProperties::new(
        "IncreasedWeightAllowance",
        0,
        MagicSkill::None,
        &[Payload::On {
            trigger: Trigger::Held,
            action: &[
                Payload::WeightAllowance(125),
                Payload::WeightAllowance(150),
            ],
        }],
    ),
    // ItemDeteriorates.cs:32-39 (SetProperties: Held) · :29-30 (conditionLossPerRounds = 4,
    // conditionLossAmount = 1) · :72-104 (MagicRound lowers the item condition under the
    // param's gate: AllTheTime/Sunlight/HolyPlaces). The cadence and the gate are item
    // state the owner resolves; this payload is the condition delta it applies.
    EffectProperties::new(
        "ItemDeteriorates",
        0,
        MagicSkill::None,
        &[Payload::On {
            trigger: Trigger::MagicRound,
            action: &[Payload::ItemCondition { delta: -1 }],
        }],
    ),
    // LowDamageVs.cs:29-36 (SetProperties: Strikes) · :27 (reduceDamageAmount = -5) ·
    // :73-101 (callback modulates strike damage for the param's group:
    // Undead/Daedra/Humanoid/Animals).
    EffectProperties::new(
        "LowDamageVs",
        0,
        MagicSkill::None,
        &[Payload::On {
            trigger: Trigger::Strikes,
            action: &[
                Payload::Versus {
                    group: EnemyGroup::Undead,
                    amount: -5,
                },
                Payload::Versus {
                    group: EnemyGroup::Daedra,
                    amount: -5,
                },
                Payload::Versus {
                    group: EnemyGroup::Humanoids,
                    amount: -5,
                },
                Payload::Versus {
                    group: EnemyGroup::Animals,
                    amount: -5,
                },
            ],
        }],
    ),
    // PotentVs.cs:29-36 (SetProperties: Strikes) · :27 (increaseDamageAmount = +5) ·
    // :73-101 (callback modulates strike damage for the param's group:
    // Undead/Daedra/Humanoid/Animals).
    EffectProperties::new(
        "PotentVs",
        0,
        MagicSkill::None,
        &[Payload::On {
            trigger: Trigger::Strikes,
            action: &[
                Payload::Versus {
                    group: EnemyGroup::Undead,
                    amount: 5,
                },
                Payload::Versus {
                    group: EnemyGroup::Daedra,
                    amount: 5,
                },
                Payload::Versus {
                    group: EnemyGroup::Humanoids,
                    amount: 5,
                },
                Payload::Versus {
                    group: EnemyGroup::Animals,
                    amount: 5,
                },
            ],
        }],
    ),
    // RegensHealth.cs:28-35 (SetProperties: Held) · :25-26 (regenerateAmount = 1 every
    // 4 rounds) · :72-108 (MagicRound heals under the param's condition:
    // AllTheTime/Sunlight/InDarkness).
    EffectProperties::new(
        "RegensHealth",
        0,
        MagicSkill::None,
        &[Payload::On {
            trigger: Trigger::MagicRound,
            action: &[Payload::FixedResource {
                resource: Resource::Health,
                sign: 1,
                amount: 1,
            }],
        }],
    ),
    // RepairsObjects.cs:29-36 (SetProperties: Held) · :25-26 (conditionAmount = 1 every
    // 4 rounds) · :69-99 (MagicRound raises the condition of one worn item). The
    // cadence is item state the owner resolves; this payload is the condition delta.
    EffectProperties::new(
        "RepairsObjects",
        0,
        MagicSkill::None,
        &[Payload::On {
            trigger: Trigger::MagicRound,
            action: &[Payload::ItemCondition { delta: 1 }],
        }],
    ),
    // SoulBound.cs:35-42 (SetProperties: Enchanted|Breaks) · :93-101 (callback: Enchanted
    // removes the filled soul trap, Breaks spawns the trapped soul as a hostile foe,
    // `CreateFoeSpawner(..., (MobileTypes)param.Value.ClassicParam, 1)` at :100).
    EffectProperties::new(
        "SoulBound",
        0,
        MagicSkill::None,
        &[Payload::On {
            trigger: Trigger::Breaks,
            action: &[Payload::Request(Request::SpawnAlly)],
        }],
    ),
    // StrengthensArmor.cs:27-34 (SetProperties: Held) · :25 (increaseArmorValue = -5,
    // lower is stronger) · :54-65 (ConstantEffect SetIncreasedArmorValueModifier).
    EffectProperties::new(
        "StrengthensArmor",
        0,
        MagicSkill::None,
        &[Payload::On {
            trigger: Trigger::Held,
            action: &[Payload::ArmorValue(-5)],
        }],
    ),
    // UserTakesDamage.cs:30-37 (SetProperties: Held) · :27-28 (damageAmount = 1 every
    // 4 rounds) · :70-88 (MagicRound: InSunlight / InHolyPlaces params gate the damage).
    EffectProperties::new(
        "UserTakesDamage",
        0,
        MagicSkill::None,
        &[Payload::On {
            trigger: Trigger::Held,
            action: &[
                Payload::GatedDamage {
                    gate: Gate::Sunlight,
                    amount: 1,
                },
                Payload::GatedDamage {
                    gate: Gate::HolyPlace,
                    amount: 1,
                },
            ],
        }],
    ),
    // VampiricEffect.cs:30-37 (SetProperties: Held|Strikes) · :27-28 (regenerateAmount = 1
    // every 4 rounds) · :55-107 (AtRange: drains 1 health from nearby enemies and heals the
    // bearer) · :110-128 (WhenStrikes: heals the bearer by the damage caused).
    EffectProperties::new(
        "VampiricEffect",
        0,
        MagicSkill::None,
        &[
            Payload::On {
                trigger: Trigger::MagicRound,
                action: &[Payload::FixedResource {
                    resource: Resource::Health,
                    sign: 1,
                    amount: 1,
                }],
            },
            // Heals the bearer by the damage the strike caused (:127).
            Payload::On {
                trigger: Trigger::Strikes,
                action: &[Payload::TriggerAmount {
                    resource: Resource::Health,
                    sign: 1,
                }],
            },
        ],
    ),
    // WeakensArmor.cs:27-34 (SetProperties: Held) · :25 (decreaseArmorValue = +5,
    // higher is weaker) · :54-65 (ConstantEffect SetDecreasedArmorValueModifier).
    EffectProperties::new(
        "WeakensArmor",
        0,
        MagicSkill::None,
        &[Payload::On {
            trigger: Trigger::Held,
            action: &[Payload::ArmorValue(5)],
        }],
    ),
];

// CONTRACT-GAP: no enchantment-param slot. `EffectProperties` (contract §2) has no
// analogue of `EnchantmentParam.ClassicParam`, so every template whose classic param
// selects a concrete payload declares **all** its alternatives in `action` and the
// engine must pick one by param at cast/equip time: EnhancesSkill.cs:69-78 (skill),
// GoodRepWith.cs:88-90 / BadRepWith.cs:92-94 (social group or All), PotentVs.cs:88-90 /
// LowDamageVs.cs:88-90 (enemy group), ImprovesTalents.cs:76-87 (talent flag),
// IncreasedWeightAllowance.cs:82-85 (125 vs 150), RegensHealth.cs:88-99 (regen gate),
// UserTakesDamage.cs:85-87 (sunlight vs holy place), VampiricEffect.cs:57-67 (mode),
// HealthLeech.cs:84-92 (WheneverUsed only), ExtraSpellPts.cs:96-128 (condition).
//
// CONTRACT-GAP: item weight. `ExtraWeight.cs:61` (`weightInKg *= 4`) and
// `FeatherWeight.cs:61` (`weightInKg = 0.25f`) mutate the item, while
// `Payload::WeightAllowance` targets the entity's carrying capacity; the closest
// primitive is used (400 = ×4, 25 = the 0.25 kg literal) but the target is not the same
// and FeatherWeight's value is not a percentage at all.
//
// CONTRACT-GAP: item condition/durability. `ItemDeteriorates.cs:103`
// (`LowerCondition`), `RepairsObjects.cs:97` (`currentCondition += 1`),
// `CastWhenHeld.cs:138` / `:177` (durability loss on round/equip) and the
// `Enchanted` half of `SoulBound.cs:96-97` (filled soul trap removal) have no
// payload variant; `ItemDeteriorates` and `RepairsObjects` carry an empty `action`.
//
// CONTRACT-GAP: conditional non-damage payloads. Only `Payload::GatedDamage` accepts a
// `Gate`; `BadReactionsFrom.cs:91-140`, `RegensHealth.cs:83-101`,
// `ExtraSpellPts.cs:96-128` and `ItemDeteriorates.cs:84-88` gate armour/skills/max
// magicka/condition, so no `Gated { gate, payload }` wrapper exists.
//
// CONTRACT-GAP: seasons. `ExtraSpellPts.cs:96-103` gates on Winter/Spring/Summer/Fall,
// which `Gate` does not enumerate (only moons and proximity are present).
