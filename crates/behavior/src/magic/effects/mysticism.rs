//! Mysticism templates — DFU `0038a1e6`
//! `Assets/Scripts/Game/MagicAndEffects/Effects/Mysticism/`.
//! Contract: `docs/sintergic/game/programming/m23-effects-contract.md` §4.1.
//!
//! Ten files, ten concrete templates, no abstract base and no variants: ten rows.
//! Every payload is a `Request` except `Silence` (`Entity.IsSilenced`,
//! `Silence.cs:87`) and `SoulTrap` (`EnemyEntity.SoulTrapActive`,
//! `SoulTrap.cs:109`), which are entity flags.

use crate::magic::{
    classic_key, make_effect_costs, ChanceFunction, EffectProperties, Flag, MagicSkill, Payload,
    Request,
};

/// `ComprehendLanguages.cs:23-37` — `Key = "ComprehendLanguages"`,
/// `ClassicKey = MakeClassicKey(44, 255)`, `SupportDuration` + `SupportChance`,
/// `ChanceFunction = ChanceFunction.Custom`,
/// `DurationCosts = MakeEffectCosts(60, 68)`, `ChanceCosts = MakeEffectCosts(40, 68)`,
/// `MagicSkill = Mysticism`; the incumbent's presence is the comprehension
/// state (`Start`, `:55-58`).
const COMPREHEND_LANGUAGES: EffectProperties = EffectProperties::new(
    "ComprehendLanguages",
    classic_key(44, 255),
    MagicSkill::Mysticism,
    &[Payload::Request(Request::ComprehendLanguages)],
)
.duration(make_effect_costs(60, 68, 0))
.chance(make_effect_costs(40, 68, 0))
.chance_fn(ChanceFunction::Custom);

/// `CreateItem.cs:26-89` — `Key = "CreateItem"`,
/// `ClassicKey = MakeClassicKey(2, 255)`, `SupportDuration`,
/// `DurationCosts = MakeEffectCosts(60, 120)`, `MagicSkill = Mysticism`;
/// `Start` opens the item picker (`:96-99`, `PromptPlayer` `:102-120`) and the
/// conjured item carries its own lifetime.
const CREATE_ITEM: EffectProperties = EffectProperties::new(
    "CreateItem",
    classic_key(2, 255),
    MagicSkill::Mysticism,
    &[Payload::Request(Request::CreateItem)],
)
.duration(make_effect_costs(60, 120, 0));

/// `DispelDaedra.cs:25-37` — `Key = "Dispel-Daedra"`,
/// `ClassicKey = MakeClassicKey(6, 2)`, `SupportChance`,
/// `ChanceFunction = ChanceFunction.Custom`,
/// `ChanceCosts = MakeEffectCosts(120, 180)`, `MagicSkill = Mysticism`.
/// Payload: `Start` destroys each nearby daedra after its own `RollChance()`
/// (`:45-58`).
const DISPEL_DAEDRA: EffectProperties = EffectProperties::new(
    "Dispel-Daedra",
    classic_key(6, 2),
    MagicSkill::Mysticism,
    &[Payload::Request(Request::DispelDaedra)],
)
.chance(make_effect_costs(120, 180, 0))
.chance_fn(ChanceFunction::Custom);

/// `DispelMagic.cs:26-42` — `Key = "Dispel-Magic"`,
/// `ClassicKey = MakeClassicKey(6, 0)`, `SupportChance`,
/// `ChanceFunction = ChanceFunction.Custom`,
/// `ChanceCosts = MakeEffectCosts(120, 180)`, `MagicSkill = Mysticism`.
/// Payload: `MagicRound` opens the spell picker over the player's live bundles
/// (`:50-72`); the cost is never refunded.
const DISPEL_MAGIC: EffectProperties = EffectProperties::new(
    "Dispel-Magic",
    classic_key(6, 0),
    MagicSkill::Mysticism,
    &[Payload::Request(Request::DispelMagic)],
)
.chance(make_effect_costs(120, 180, 0))
.chance_fn(ChanceFunction::Custom);

/// `DispelUndead.cs:25-37` — `Key = "Dispel-Undead"`,
/// `ClassicKey = MakeClassicKey(6, 1)`, `SupportChance`,
/// `ChanceFunction = ChanceFunction::Custom`,
/// `ChanceCosts = MakeEffectCosts(80, 140)`, `MagicSkill = Mysticism`.
/// Payload: `Start` destroys each nearby undead after its own `RollChance()`
/// (`:45-58`).
const DISPEL_UNDEAD: EffectProperties = EffectProperties::new(
    "Dispel-Undead",
    classic_key(6, 1),
    MagicSkill::Mysticism,
    &[Payload::Request(Request::DispelUndead)],
)
.chance(make_effect_costs(80, 140, 0))
.chance_fn(ChanceFunction::Custom);

/// `Lock.cs:23-38` — `Key = "Lock"`, `ClassicKey = MakeClassicKey(16, 255)`,
/// `SupportChance`, `ChanceCosts = MakeEffectCosts(28, 120, 120)`,
/// `MagicSkill = Mysticism`. The source never assigns `properties.ChanceFunction`
/// (default `OnCast`; `StartWaitingForDoor` reads the base `ChanceSuccess`,
/// `:76-87`). Payload: `TriggerLockEffect` sets
/// `actionDoor.CurrentLockValue = caster.Level` (`:100-130`).
const LOCK: EffectProperties = EffectProperties::new(
    "Lock",
    classic_key(16, 255),
    MagicSkill::Mysticism,
    &[Payload::Request(Request::Lock)],
)
.chance(make_effect_costs(28, 120, 120));

/// `Open.cs:23-41` — `Key = "Open"`, `ClassicKey = MakeClassicKey(17, 255)`,
/// `SupportChance`, `ChanceFunction = ChanceFunction::Custom`,
/// `ChanceCosts = MakeEffectCosts(20, 100)`, `MagicSkill = Mysticism`.
/// Payload: `TriggerOpenEffect` clears `actionDoor.CurrentLockValue` when the
/// caster's level allows it (`:107-140`).
const OPEN: EffectProperties = EffectProperties::new(
    "Open",
    classic_key(17, 255),
    MagicSkill::Mysticism,
    &[Payload::Request(Request::Open)],
)
.chance(make_effect_costs(20, 100, 0))
.chance_fn(ChanceFunction::Custom);

/// `Silence.cs:23-38` — `Key = "Silence"`, `ClassicKey = MakeClassicKey(19, 255)`,
/// `SupportDuration` + `SupportChance`,
/// `DurationCosts`/`ChanceCosts = MakeEffectCosts(20, 100)`,
/// `MagicSkill = Mysticism`. Payload: `Entity.IsSilenced = true` every constant
/// tick (`StartSilence`, `:80-95`), cleared in `EndSilence` (`:100-107`).
const SILENCE: EffectProperties = EffectProperties::new(
    "Silence",
    classic_key(19, 255),
    MagicSkill::Mysticism,
    &[Payload::Flag(Flag::Silenced)],
)
.duration(make_effect_costs(20, 100, 0))
.chance(make_effect_costs(20, 100, 0));

/// `SoulTrap.cs:26-40` — `Key = "SoulTrap"`, `ClassicKey = MakeClassicKey(12, 255)`,
/// `SupportDuration` + `SupportChance`, `DurationCosts = MakeEffectCosts(60, 68)`,
/// `ChanceCosts = MakeEffectCosts(40, 68)`, `MagicSkill = Mysticism`.
/// `properties.ChanceFunction` is never assigned, but the effect owns the roll:
/// `ChanceSuccess` always returns true (`:47-52`) and the chance is re-rolled by
/// `RollTrapChance()` when the target dies (`:112`). Payload:
/// `EnemyEntity.SoulTrapActive = true` (`ConstantEffect`, `:99-110`).
const SOUL_TRAP: EffectProperties = EffectProperties::new(
    "SoulTrap",
    classic_key(12, 255),
    MagicSkill::Mysticism,
    &[Payload::Flag(Flag::SoulTrapped)],
)
.duration(make_effect_costs(60, 68, 0))
.chance(make_effect_costs(40, 68, 0))
.chance_fn(ChanceFunction::Custom);

/// `Teleport.cs:27-56` — `Key = "Teleport-Effect"`,
/// `ClassicKey = MakeClassicKey(43, 255)`, no duration/chance/magnitude costs,
/// `MagicSkill = Mysticism`. Payload: `Start` prompts anchor/teleport
/// (`:63-67`, `PromptPlayer` `:81-100`).
const TELEPORT_EFFECT: EffectProperties = EffectProperties::new(
    "Teleport-Effect",
    classic_key(43, 255),
    MagicSkill::Mysticism,
    &[Payload::Request(Request::Teleport)],
);

/// The Mysticism family in DFU file order (`Effects/Mysticism/`).
pub const MYSTICISM: &[EffectProperties] = &[
    COMPREHEND_LANGUAGES,
    CREATE_ITEM,
    DISPEL_DAEDRA,
    DISPEL_MAGIC,
    DISPEL_UNDEAD,
    LOCK,
    OPEN,
    SILENCE,
    SOUL_TRAP,
    TELEPORT_EFFECT,
];
