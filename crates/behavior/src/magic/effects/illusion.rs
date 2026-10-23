//! Illusion templates — DFU `0038a1e6`
//! `Assets/Scripts/Game/MagicAndEffects/Effects/Illusion/`.
//! Contract: `docs/sintergic/game/programming/m23-effects-contract.md` §4.1.
//!
//! Nine concrete templates (ten files minus the abstract `ConcealmentEffect`);
//! `MageLight` contributes its five variants, so the table holds 13 rows.
//! `ConcealmentEffect` declares no `Key` and no `ClassicKey`
//! (`ConcealmentEffect.cs:22`) and DFU never registers abstract classes
//! (`EntityEffectBroker.cs:1126-1134`); its payload (`StartConcealment`,
//! `ConcealmentEffect.cs:56-76`) runs through the six subclasses below.
//!
//! Abstractos no registrados: `ConcealmentEffect.cs` (sin payload de plantilla).
//!
//! Census: contract §4 tabulates 14 rows for this family, but that count treats
//! the ten *files* as rows and adds MageLight's four extra variants without
//! dropping the abstract `ConcealmentEffect.cs` — registering it would violate
//! contract §1. The broker expands `VariantCount` into one template per variant
//! (`EntityEffectBroker.cs:286-296`), so the registered keys are 13:
//! 6 concealment + `Light` + 5 `MageLight` + `MorphSelf`.

use crate::magic::{
    classic_key, make_effect_costs, ConcealmentFlag, EffectProperties, MagicSkill, Payload, Request,
};

/// `ChameleonNormal.cs:23-35` — `Key = "Chameleon-Normal"`,
/// `ClassicKey = MakeClassicKey(23, 0)`, `SupportDuration`,
/// `DurationCosts = MakeEffectCosts(20, 80)`, `MagicSkill = Illusion`,
/// `concealmentFlag = MagicalConcealmentFlags.BlendingNormal`.
const CHAMELEON_NORMAL: EffectProperties = EffectProperties::new(
    "Chameleon-Normal",
    classic_key(23, 0),
    MagicSkill::Illusion,
    &[Payload::Concealment(ConcealmentFlag::BlendingNormal)],
)
.duration(make_effect_costs(20, 80, 0));

/// `ChameleonTrue.cs:23-35` — `Key = "Chameleon-True"`,
/// `ClassicKey = MakeClassicKey(23, 1)`, `SupportDuration`,
/// `DurationCosts = MakeEffectCosts(40, 120)`, `MagicSkill = Illusion`,
/// `concealmentFlag = MagicalConcealmentFlags.BlendingTrue`.
const CHAMELEON_TRUE: EffectProperties = EffectProperties::new(
    "Chameleon-True",
    classic_key(23, 1),
    MagicSkill::Illusion,
    &[Payload::Concealment(ConcealmentFlag::BlendingTrue)],
)
.duration(make_effect_costs(40, 120, 0));

/// `InvisibilityNormal.cs:23-35` — `Key = "Invisibility-Normal"`,
/// `ClassicKey = MakeClassicKey(13, 0)`, `SupportDuration`,
/// `DurationCosts = MakeEffectCosts(40, 120)`, `MagicSkill = Illusion`,
/// `concealmentFlag = MagicalConcealmentFlags.InvisibleNormal`.
const INVISIBILITY_NORMAL: EffectProperties = EffectProperties::new(
    "Invisibility-Normal",
    classic_key(13, 0),
    MagicSkill::Illusion,
    &[Payload::Concealment(ConcealmentFlag::InvisibleNormal)],
)
.duration(make_effect_costs(40, 120, 0));

/// `InvisibilityTrue.cs:23-35` — `Key = "Invisibility-True"`,
/// `ClassicKey = MakeClassicKey(13, 1)`, `SupportDuration`,
/// `DurationCosts = MakeEffectCosts(60, 140)`, `MagicSkill = Illusion`,
/// `concealmentFlag = MagicalConcealmentFlags.InvisibleTrue`.
const INVISIBILITY_TRUE: EffectProperties = EffectProperties::new(
    "Invisibility-True",
    classic_key(13, 1),
    MagicSkill::Illusion,
    &[Payload::Concealment(ConcealmentFlag::InvisibleTrue)],
)
.duration(make_effect_costs(60, 140, 0));

/// `LightNormal.cs:20-34` — `Key = "Light"`, `ClassicKey = MakeClassicKey(15, 255)`,
/// `SupportDuration`, `DurationCosts = MakeEffectCosts(8, 40)`,
/// `MagicSkill = Illusion`; the payload is the magic candle spawned by
/// `Start` → `StartLight` → `StartMagicCandle` (`LightNormal.cs:41-45,71-113`)
/// and destroyed on `End` (`:110-114`).
const LIGHT: EffectProperties = EffectProperties::new(
    "Light",
    classic_key(15, 255),
    MagicSkill::Illusion,
    &[Payload::Request(Request::Light)],
)
.duration(make_effect_costs(8, 40, 0));

/// `MageLight.cs:33,96-106,219` — variant 0: `Key = string.Format("{0}-{1}",
/// "MageLight", subGroupKeys[0])` = `MageLight-Inferno`; no `ClassicKey` is
/// declared (stays 0), `SupportDuration`, `DurationCosts = MakeEffectCosts(8, 40)`,
/// `MagicSkill = Illusion`, `variantCount = 5`. Light is created in `StartLight`
/// (`:182-200`).
const MAGE_LIGHT_INFERNO: EffectProperties = EffectProperties::new(
    "MageLight-Inferno",
    0,
    MagicSkill::Illusion,
    &[Payload::Request(Request::Light)],
)
.duration(make_effect_costs(8, 40, 0))
.variants(5);

/// `MageLight.cs:33,96-106,219` — variant 1: `Key == "MageLight-Rime"`
/// (`subGroupKeys[1]`), same properties as variant 0 (no `ClassicKey`).
const MAGE_LIGHT_RIME: EffectProperties = EffectProperties::new(
    "MageLight-Rime",
    0,
    MagicSkill::Illusion,
    &[Payload::Request(Request::Light)],
)
.duration(make_effect_costs(8, 40, 0))
.variants(5);

/// `MageLight.cs:33,96-106,219` — variant 2: `Key == "MageLight-Venom"`
/// (`subGroupKeys[2]`), same properties as variant 0 (no `ClassicKey`).
const MAGE_LIGHT_VENOM: EffectProperties = EffectProperties::new(
    "MageLight-Venom",
    0,
    MagicSkill::Illusion,
    &[Payload::Request(Request::Light)],
)
.duration(make_effect_costs(8, 40, 0))
.variants(5);

/// `MageLight.cs:33,96-106,219` — variant 3: `Key == "MageLight-Storm"`
/// (`subGroupKeys[3]`), same properties as variant 0 (no `ClassicKey`).
const MAGE_LIGHT_STORM: EffectProperties = EffectProperties::new(
    "MageLight-Storm",
    0,
    MagicSkill::Illusion,
    &[Payload::Request(Request::Light)],
)
.duration(make_effect_costs(8, 40, 0))
.variants(5);

/// `MageLight.cs:33,96-106,219` — variant 4: `Key == "MageLight-Arcane"`
/// (`subGroupKeys[4]`), same properties as variant 0 (no `ClassicKey`).
const MAGE_LIGHT_ARCANE: EffectProperties = EffectProperties::new(
    "MageLight-Arcane",
    0,
    MagicSkill::Illusion,
    &[Payload::Request(Request::Light)],
)
.duration(make_effect_costs(8, 40, 0))
.variants(5);

/// `MorphSelf.cs:22-32` — `Key = "MorphSelf"`, `ClassicKey = MakeClassicKey(29, 255)`,
/// no duration/chance/magnitude costs, `MagicSkill = Illusion`; the payload
/// triggers the lycanthropy racial override morph and ends immediately
/// (`MagicRound`, `MorphSelf.cs:39-50`).
const MORPH_SELF: EffectProperties = EffectProperties::new(
    "MorphSelf",
    classic_key(29, 255),
    MagicSkill::Illusion,
    &[Payload::Request(Request::MorphSelf)],
);

/// `ShadowNormal.cs:23-35` — `Key = "Shadow-Normal"`,
/// `ClassicKey = MakeClassicKey(24, 0)`, `SupportDuration`,
/// `DurationCosts = MakeEffectCosts(20, 80)`, `MagicSkill = Illusion`,
/// `concealmentFlag = MagicalConcealmentFlags.ShadeNormal`.
const SHADOW_NORMAL: EffectProperties = EffectProperties::new(
    "Shadow-Normal",
    classic_key(24, 0),
    MagicSkill::Illusion,
    &[Payload::Concealment(ConcealmentFlag::ShadowNormal)],
)
.duration(make_effect_costs(20, 80, 0));

/// `ShadowTrue.cs:23-35` — `Key = "Shadow-True"`,
/// `ClassicKey = MakeClassicKey(24, 1)`, `SupportDuration`,
/// `DurationCosts = MakeEffectCosts(40, 120)`, `MagicSkill = Illusion`,
/// `concealmentFlag = MagicalConcealmentFlags.ShadeTrue`.
const SHADOW_TRUE: EffectProperties = EffectProperties::new(
    "Shadow-True",
    classic_key(24, 1),
    MagicSkill::Illusion,
    &[Payload::Concealment(ConcealmentFlag::ShadowTrue)],
)
.duration(make_effect_costs(40, 120, 0));

/// The Illusion family in DFU file order (`Effects/Illusion/`).
pub const ILLUSION: &[EffectProperties] = &[
    CHAMELEON_NORMAL,
    CHAMELEON_TRUE,
    INVISIBILITY_NORMAL,
    INVISIBILITY_TRUE,
    LIGHT,
    MAGE_LIGHT_INFERNO,
    MAGE_LIGHT_RIME,
    MAGE_LIGHT_VENOM,
    MAGE_LIGHT_STORM,
    MAGE_LIGHT_ARCANE,
    MORPH_SELF,
    SHADOW_NORMAL,
    SHADOW_TRUE,
];
