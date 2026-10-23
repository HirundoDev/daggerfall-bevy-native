//! Thaumaturgy templates — DFU `0038a1e6`
//! `Assets/Scripts/Game/MagicAndEffects/Effects/Thaumaturgy/`.
//! Contract: `docs/sintergic/game/programming/m23-effects-contract.md` §4.1.
//!
//! Ten concrete templates (eleven files minus the abstract `DetectEffect`);
//! `PacifyEffect` contributes its four variants, so the table holds 13 rows.
//! `DetectEffect` declares no `Key`, no `ClassicKey` and no payload of its own
//! (`DetectEffect.cs:20-28`); DFU never registers abstract classes
//! (`EntityEffectBroker.cs:1126-1134`) and each Detect subclass fills
//! `DetectedObjects` in its own `MagicRound`.
//!
//! Abstractos no registrados: `DetectEffect.cs` (sin payload de plantilla).
//!
//! Census: contract §4 tabulates 14 rows for this family, but that count treats
//! the eleven *files* as rows and adds Pacify's three extra variants without
//! dropping the abstract `DetectEffect.cs` — registering it would violate
//! contract §1. The broker expands `VariantCount` into one template per variant
//! (`EntityEffectBroker.cs:286-296`), so the registered keys are 13:
//! `Charm` + 3 Detect + `Identify` + `Levitate` + 4 `Pacify` +
//! `SpellReflection` + `SpellResistance` + `WaterWalking`.

use crate::magic::{
    classic_key, make_effect_costs, ChanceFunction, ChanceState, EffectProperties, EnemyGroup, Flag,
    MagicSkill, Payload, Request,
};

/// `CharmEffect.cs:30-43` — `Key = "Charm"`, `ClassicKey = MakeClassicKey(34, 255)`,
/// `SupportChance` only (duration line is commented out at `:36`),
/// `ChanceCosts = MakeEffectCosts(40, 60)`, `MagicSkill = Thaumaturgy`.
/// Payload: `MagicRound` clears `EnemyMotor.IsHostile` (`:50-67`).
const CHARM: EffectProperties = EffectProperties::new(
    "Charm",
    classic_key(34, 255),
    MagicSkill::Thaumaturgy,
    &[Payload::Request(Request::Charm)],
)
.chance(make_effect_costs(40, 60, 0));

/// `DetectEnemy.cs:23-34` — `Key = "Detect-Enemy"`,
/// `ClassicKey = MakeClassicKey(39, 1)`, `SupportDuration`,
/// `DurationCosts = MakeEffectCosts(20, 8, 200)`, `MagicSkill = Thaumaturgy`.
/// Payload: compass detector (`Start`, `:53-57`) fed with
/// `NearbyObjectFlags.Enemy` every round (`MagicRound`, `:65-70`).
const DETECT_ENEMY: EffectProperties = EffectProperties::new(
    "Detect-Enemy",
    classic_key(39, 1),
    MagicSkill::Thaumaturgy,
    &[Payload::Request(Request::DetectEnemy)],
)
.duration(make_effect_costs(20, 8, 200));

/// `DetectMagic.cs:23-34` — `Key = "Detect-Magic"`,
/// `ClassicKey = MakeClassicKey(39, 0)`, `SupportDuration`,
/// `DurationCosts = MakeEffectCosts(20, 8, 200)`, `MagicSkill = Thaumaturgy`.
/// Payload: compass detector (`Start`, `:53-57`) fed with
/// `NearbyObjectFlags.Magic` every round (`MagicRound`, `:65-70`).
const DETECT_MAGIC: EffectProperties = EffectProperties::new(
    "Detect-Magic",
    classic_key(39, 0),
    MagicSkill::Thaumaturgy,
    &[Payload::Request(Request::DetectMagic)],
)
.duration(make_effect_costs(20, 8, 200));

/// `DetectTreasure.cs:23-34` — `Key = "Detect-Treasure"`,
/// `ClassicKey = MakeClassicKey(39, 2)`, `SupportDuration`,
/// `DurationCosts = MakeEffectCosts(20, 8, 160)`, `MagicSkill = Thaumaturgy`.
/// Payload: compass detector (`Start`, `:53-57`) fed with
/// `NearbyObjectFlags.Treasure` every round (`MagicRound`, `:65-70`).
const DETECT_TREASURE: EffectProperties = EffectProperties::new(
    "Detect-Treasure",
    classic_key(39, 2),
    MagicSkill::Thaumaturgy,
    &[Payload::Request(Request::DetectTreasure)],
)
.duration(make_effect_costs(20, 8, 160));

/// `Identify.cs:26-38` — `Key = "Identify"`, `ClassicKey = MakeClassicKey(40, 255)`,
/// `SupportChance`, `ChanceCosts = MakeEffectCosts(40, 100, 28)`,
/// `ChanceFunction = ChanceFunction.Custom`, `MagicSkill = Thaumaturgy`.
/// Payload: refunds the cost then opens the identify trade window with
/// `IdentifySpellChance = ChanceValue()` (`Start`, `:45-79`).
const IDENTIFY: EffectProperties = EffectProperties::new(
    "Identify",
    classic_key(40, 255),
    MagicSkill::Thaumaturgy,
    &[Payload::Request(Request::Identify)],
)
.chance(make_effect_costs(40, 100, 28))
.chance_fn(ChanceFunction::Custom);

/// `Levitate.cs:23-37` — `Key = "Levitate"`, `ClassicKey = MakeClassicKey(14, 255)`,
/// `SupportDuration`, `DurationCosts = MakeEffectCosts(60, 100)`,
/// `MagicSkill = Thaumaturgy`. Payload: `LevitateMotor.IsLevitating` for the
/// player / `EnemyMotor.IsLevitating` for enemies, cleared on `End`
/// (`StartLevitating`, `:92-108`).
const LEVITATE: EffectProperties = EffectProperties::new(
    "Levitate",
    classic_key(14, 255),
    MagicSkill::Thaumaturgy,
    &[Payload::Flag(Flag::Levitating)],
)
.duration(make_effect_costs(60, 100, 0));

/// `PacifyEffect.cs:27,62-66,110-119` — variant 0: `Key = string.Format(
/// "Pacify-{0}", subGroupTextKeys[0])` = `Pacify-Animal`,
/// `ClassicKey = MakeClassicKey(33, 0)`, `SupportChance`,
/// `ChanceCosts = MakeEffectCosts(60, 100, 160)`, `MagicSkill = Thaumaturgy`,
/// `variantCount = 4`; `MagicRound` clears `IsHostile` when the target's
/// `EnemyGroups` matches (`:81-97`).
const PACIFY_ANIMAL: EffectProperties = EffectProperties::new(
    "Pacify-Animal",
    classic_key(33, 0),
    MagicSkill::Thaumaturgy,
    &[Payload::Request(Request::Pacify(EnemyGroup::Animals))],
)
.chance(make_effect_costs(60, 100, 160))
.variants(4);

/// `PacifyEffect.cs:27,62-66,110-119` — variant 1: `Key == "Pacify-Undead"`
/// (`subGroupTextKeys[1]`), `ClassicKey = MakeClassicKey(33, 1)`, `SupportChance`,
/// `ChanceCosts = MakeEffectCosts(80, 140, 60)`.
const PACIFY_UNDEAD: EffectProperties = EffectProperties::new(
    "Pacify-Undead",
    classic_key(33, 1),
    MagicSkill::Thaumaturgy,
    &[Payload::Request(Request::Pacify(EnemyGroup::Undead))],
)
.chance(make_effect_costs(80, 140, 60))
.variants(4);

/// `PacifyEffect.cs:27,62-66,110-119` — variant 2: `Key == "Pacify-Humanoid"`
/// (`subGroupTextKeys[2]`), `ClassicKey = MakeClassicKey(33, 2)`, `SupportChance`,
/// `ChanceCosts = MakeEffectCosts(80, 140, 60)`.
const PACIFY_HUMANOID: EffectProperties = EffectProperties::new(
    "Pacify-Humanoid",
    classic_key(33, 2),
    MagicSkill::Thaumaturgy,
    &[Payload::Request(Request::Pacify(EnemyGroup::Humanoids))],
)
.chance(make_effect_costs(80, 140, 60))
.variants(4);

/// `PacifyEffect.cs:27,62-66,110-119` — variant 3: `Key == "Pacify-Daedra"`
/// (`subGroupTextKeys[3]`), `ClassicKey = MakeClassicKey(33, 3)`, `SupportChance`,
/// `ChanceCosts = MakeEffectCosts(60, 120, 36)`.
const PACIFY_DAEDRA: EffectProperties = EffectProperties::new(
    "Pacify-Daedra",
    classic_key(33, 3),
    MagicSkill::Thaumaturgy,
    &[Payload::Request(Request::Pacify(EnemyGroup::Daedra))],
)
.chance(make_effect_costs(60, 120, 36))
.variants(4);

/// `SpellReflection.cs:22-36` — `Key = "SpellReflection"`,
/// `ClassicKey = MakeClassicKey(21, 255)`, `SupportDuration` + `SupportChance`,
/// `ChanceFunction = ChanceFunction.Custom`,
/// `DurationCosts`/`ChanceCosts = MakeEffectCosts(28, 140)`,
/// `MagicSkill = Thaumaturgy`; the chance value is the reflection roll
/// (`ChanceValue()` read on each incoming spell).
const SPELL_REFLECTION: EffectProperties = EffectProperties::new(
    "SpellReflection",
    classic_key(21, 255),
    MagicSkill::Thaumaturgy,
    &[Payload::ChanceState(ChanceState::SpellReflection)],
)
.duration(make_effect_costs(28, 140, 0))
.chance(make_effect_costs(28, 140, 0))
.chance_fn(ChanceFunction::Custom);

/// `SpellResistance.cs:22-36` — `Key = "SpellResistance"`,
/// `ClassicKey = MakeClassicKey(22, 255)`, `SupportDuration` + `SupportChance`,
/// `ChanceFunction = ChanceFunction.Custom`,
/// `DurationCosts`/`ChanceCosts = MakeEffectCosts(20, 100)`,
/// `MagicSkill = Thaumaturgy`; the chance value is the resistance roll
/// (`EntityEffectManager.TryResistance`).
const SPELL_RESISTANCE: EffectProperties = EffectProperties::new(
    "SpellResistance",
    classic_key(22, 255),
    MagicSkill::Thaumaturgy,
    &[Payload::ChanceState(ChanceState::SpellResistance)],
)
.duration(make_effect_costs(20, 100, 0))
.chance(make_effect_costs(20, 100, 0))
.chance_fn(ChanceFunction::Custom);

/// `WaterWalking.cs:23-34` — `Key = "WaterWalking"`,
/// `ClassicKey = MakeClassicKey(31, 255)`, `SupportDuration`,
/// `DurationCosts = MakeEffectCosts(20, 8)`, `MagicSkill = Thaumaturgy`.
/// Payload: `Entity.IsWaterWalking` set in `StartWaterWalking` (`:91-98`),
/// cleared on `End`.
const WATER_WALKING: EffectProperties = EffectProperties::new(
    "WaterWalking",
    classic_key(31, 255),
    MagicSkill::Thaumaturgy,
    &[Payload::Flag(Flag::WaterWalking)],
)
.duration(make_effect_costs(20, 8, 0));

/// The Thaumaturgy family in DFU file order (`Effects/Thaumaturgy/`).
pub const THAUMATURGY: &[EffectProperties] = &[
    CHARM,
    DETECT_ENEMY,
    DETECT_MAGIC,
    DETECT_TREASURE,
    IDENTIFY,
    LEVITATE,
    PACIFY_ANIMAL,
    PACIFY_UNDEAD,
    PACIFY_HUMANOID,
    PACIFY_DAEDRA,
    SPELL_REFLECTION,
    SPELL_RESISTANCE,
    WATER_WALKING,
];
