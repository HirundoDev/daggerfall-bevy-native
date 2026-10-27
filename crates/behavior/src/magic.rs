//! M23 (REQ-L99-056) — **magic**:
//! effect model (classic key, school, variants, rounds), the **two**
//! cost routes of §3, spellbook, spellmaker, cast animation, and the
//! active-rounds engine. Pure: 0 Bevy, 0 I/O, 0 global RNG (the probability
//! roll enters **injected** via [`crate::combat::success_roll`], the
//! PRNG lives in `daggerfall-combat`).
//!
//! Spec: `docs/sintergic/game/programming/m23-magic-spec.md`
//! (§1 SPELLS.STD → `daggerfall_parser_rs::spells_std`; §2 model; §3 the
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

// ── §2b · application layer: neutral target model ──────────────────────────
//
// M23's effects were metadata only (audit §4 item 10): a cast scheduled rounds
// but nothing could mutate health/fatigue/attributes/resistances or raise the
// flags M19 consumes. This layer applies the DFU effect scripts to a **neutral
// target** owned by `behavior`, so the organ never depends upward on
// `daggerfall-skills` (`Character` lives in `crates/skills/src/character.rs`).
//
// DFU lifecycle transcribed here: `Start()` sets duration/chance
// (`EntityEffect.cs:528-535`, `:920-935`, `:940-943`), the bundle calls the
// **initial** `MagicRound()` right after adding the effect
// (`EntityEffectManager.cs:594`), `DoMagicRound()` calls `MagicRound()` only
// while `RoundsRemaining > 0` and `End()` when it reaches 0
// (`EntityEffectManager.cs:1730-1735`). `MagicRound()` applies the payload and
// `RemoveRound()` decrements (`EntityEffect.cs:572-581`). `End()` is what
// reverts the flags/mods (`Levitate.cs:78-81`, `Slowfall.cs:71-74`,
// `WaterBreathing.cs:76-79`, `WaterWalking.cs:77-80`, `FreeAction.cs:78-81`,
// `ConcealmentEffect.cs:47-50`, `ElementalResistance.cs:180-183`).
//
// Declared deltas (nothing invented):
// - **No incumbent merging**: DFU's `IncumbentEffect.AddState` stacks rounds on
//   an existing effect (`Levitate.cs:90-94`); this engine keeps one instance per
//   entry (the M23 §9 C instance-isolation contract) and re-derives the aggregate
//   state from the live instances, so two `Levitate` casts hold the flag until
//   the last one expires.
// - **No saving-throw magnitude reduction**: `GetMagnitude` runs
//   `FormulaHelper.ModifyEffectAmount` for non-self targets
//   (`EntityEffect.cs:800-806`); the neutral model has no saving-throw table, so
//   the rolled magnitude is applied as-is.
// - **No positional teleport**: `Teleport` prompts and restores a world position
//   (`Teleport.cs:67`, `:110-160`); here it increments a declared
//   `teleport_requests` counter that the world owner (M31/M34) consumes.

/// `DFCareer.Stats` (`DFCareer.cs:429-440`): **8** attributes, `None = -1`.
pub const ATTRIBUTE_COUNT: usize = 8;

/// `DFCareer.Elements` (`DFCareer.cs:493-501`): **5** elements, `None = -1`.
pub const ELEMENT_COUNT: usize = 5;

/// `DaggerfallEntity.FatigueMultiplier` (`DaggerfallEntity.cs:39`): fatigue is
/// stored in 1/64 units, so `IncreaseFatigue(amount, assignMultiplier: true)`
/// multiplies by this constant (`DaggerfallEntity.cs:340-348`).
pub const FATIGUE_MULTIPLIER: i32 = 64;

/// Sentinel `rounds_remaining` of a [`Lifetime::Permanent`] effect: it applies
/// every magic round and never expires (`DrainEffect` keeps its stat mod until
/// healed, a disease/poison until cured or out of days/minutes).
pub const PERMANENT_ROUNDS: i32 = i32::MAX;

/// The 8 character attributes, indexed as `DFCareer.Stats`
/// (`DFCareer.cs:429-440`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(usize)]
pub enum Attribute {
    /// `Strength = 0`.
    Strength = 0,
    /// `Intelligence = 1`.
    Intelligence = 1,
    /// `Willpower = 2`.
    Willpower = 2,
    /// `Agility = 3`.
    Agility = 3,
    /// `Endurance = 4`.
    Endurance = 4,
    /// `Personality = 5`.
    Personality = 5,
    /// `Speed = 6`.
    Speed = 6,
    /// `Luck = 7`.
    Luck = 7,
}

impl Attribute {
    /// `DFCareer.Stats` name, used by the observable-field labels.
    pub const fn name(self) -> &'static str {
        match self {
            Attribute::Strength => "Strength",
            Attribute::Intelligence => "Intelligence",
            Attribute::Willpower => "Willpower",
            Attribute::Agility => "Agility",
            Attribute::Endurance => "Endurance",
            Attribute::Personality => "Personality",
            Attribute::Speed => "Speed",
            Attribute::Luck => "Luck",
        }
    }
}

/// All attributes in `DFCareer.Stats` order.
pub const ATTRIBUTES: [Attribute; ATTRIBUTE_COUNT] = [
    Attribute::Strength,
    Attribute::Intelligence,
    Attribute::Willpower,
    Attribute::Agility,
    Attribute::Endurance,
    Attribute::Personality,
    Attribute::Speed,
    Attribute::Luck,
];

/// The 5 elements, indexed as `DFCareer.Elements` (`DFCareer.cs:493-501`);
/// `DiseaseOrPoison` is the `ElementalResistance-Poison` variant
/// (`ElementalResistance.cs:79`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(usize)]
pub enum Element {
    /// `Fire = 0`.
    Fire = 0,
    /// `Frost = 1`.
    Frost = 1,
    /// `DiseaseOrPoison = 2`.
    DiseaseOrPoison = 2,
    /// `Shock = 3`.
    Shock = 3,
    /// `Magic = 4`.
    Magic = 4,
}

impl Element {
    /// `DFCareer.Elements` name, used by the observable-field labels.
    pub const fn name(self) -> &'static str {
        match self {
            Element::Fire => "Fire",
            Element::Frost => "Frost",
            Element::DiseaseOrPoison => "Poison",
            Element::Shock => "Shock",
            Element::Magic => "Magicka",
        }
    }
}

/// All elements in `DFCareer.Elements` order.
pub const ELEMENTS: [Element; ELEMENT_COUNT] = [
    Element::Fire,
    Element::Frost,
    Element::DiseaseOrPoison,
    Element::Shock,
    Element::Magic,
];

/// When `ChanceValue()` is rolled (`MagicAndEffectsEnums.cs:107-111`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChanceFunction {
    /// Rolled once in `Start()`; a failed roll drops the effect from the
    /// bundle (`EntityEffectManager.cs:541-555`).
    OnCast,
    /// The effect interprets the chance component itself
    /// (`ElementalResistance.cs:50-54` always succeeds and turns the value into
    /// resistance; `SpellResistance` rolls it per incoming spell,
    /// `EntityEffectManager.TryResistance`).
    Custom,
}

// ── §2b · effect payload vocabulary (contract §2) ──────────────
//
// One template is **data**: `EffectProperties { payload: &'static [Payload], .. }`.
// The engine interprets the payload; no template carries code. The vocabulary is
// the transitive closure of every entity mutation the 153 DFU effect files
// perform (`grep -rhoE 'ChangeStatMod|SetStatMod|ChangeSkillMod| …' Effects/`).

/// The 35 skills in `DFCareer.Skills` order (`DFCareer.cs:448-487`), the same
/// indices as `daggerfall-skills::Skill`. Declared here because the behaviour
/// organ keeps 0 cross-crate dependencies.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum EffectSkill {
    /// 0 — Medical.
    Medical = 0,
    /// 1 — Etiquette.
    Etiquette = 1,
    /// 2 — Streetwise.
    Streetwise = 2,
    /// 3 — Jumping.
    Jumping = 3,
    /// 4 — Orcish.
    Orcish = 4,
    /// 5 — Harpy.
    Harpy = 5,
    /// 6 — Giantish.
    Giantish = 6,
    /// 7 — Dragonish.
    Dragonish = 7,
    /// 8 — Nymph.
    Nymph = 8,
    /// 9 — Daedric.
    Daedric = 9,
    /// 10 — Spriggan.
    Spriggan = 10,
    /// 11 — Centaurian.
    Centaurian = 11,
    /// 12 — Impish.
    Impish = 12,
    /// 13 — Lockpicking.
    Lockpicking = 13,
    /// 14 — Mercantile.
    Mercantile = 14,
    /// 15 — Pickpocket.
    Pickpocket = 15,
    /// 16 — Stealth.
    Stealth = 16,
    /// 17 — Swimming.
    Swimming = 17,
    /// 18 — Climbing.
    Climbing = 18,
    /// 19 — Backstabbing.
    Backstabbing = 19,
    /// 20 — Dodging.
    Dodging = 20,
    /// 21 — Running.
    Running = 21,
    /// 22 — Destruction.
    Destruction = 22,
    /// 23 — Restoration.
    Restoration = 23,
    /// 24 — Illusion.
    Illusion = 24,
    /// 25 — Alteration.
    Alteration = 25,
    /// 26 — Thaumaturgy.
    Thaumaturgy = 26,
    /// 27 — Mysticism.
    Mysticism = 27,
    /// 28 — ShortBlade.
    ShortBlade = 28,
    /// 29 — LongBlade.
    LongBlade = 29,
    /// 30 — HandToHand.
    HandToHand = 30,
    /// 31 — Axe.
    Axe = 31,
    /// 32 — BluntWeapon.
    BluntWeapon = 32,
    /// 33 — Archery.
    Archery = 33,
    /// 34 — CriticalStrike.
    CriticalStrike = 34,
}

/// Number of skills (`DFCareer.Skills`).
pub const EFFECT_SKILL_COUNT: usize = 35;

impl EffectSkill {
    /// The 35 skills in index order.
    pub const ALL: [EffectSkill; EFFECT_SKILL_COUNT] = [
        EffectSkill::Medical,
        EffectSkill::Etiquette,
        EffectSkill::Streetwise,
        EffectSkill::Jumping,
        EffectSkill::Orcish,
        EffectSkill::Harpy,
        EffectSkill::Giantish,
        EffectSkill::Dragonish,
        EffectSkill::Nymph,
        EffectSkill::Daedric,
        EffectSkill::Spriggan,
        EffectSkill::Centaurian,
        EffectSkill::Impish,
        EffectSkill::Lockpicking,
        EffectSkill::Mercantile,
        EffectSkill::Pickpocket,
        EffectSkill::Stealth,
        EffectSkill::Swimming,
        EffectSkill::Climbing,
        EffectSkill::Backstabbing,
        EffectSkill::Dodging,
        EffectSkill::Running,
        EffectSkill::Destruction,
        EffectSkill::Restoration,
        EffectSkill::Illusion,
        EffectSkill::Alteration,
        EffectSkill::Thaumaturgy,
        EffectSkill::Mysticism,
        EffectSkill::ShortBlade,
        EffectSkill::LongBlade,
        EffectSkill::HandToHand,
        EffectSkill::Axe,
        EffectSkill::BluntWeapon,
        EffectSkill::Archery,
        EffectSkill::CriticalStrike,
    ];

    /// `DFCareer.Skills` name, used by the observable-field labels.
    pub const fn name(self) -> &'static str {
        const NAMES: [&str; EFFECT_SKILL_COUNT] = [
            "Medical",
            "Etiquette",
            "Streetwise",
            "Jumping",
            "Orcish",
            "Harpy",
            "Giantish",
            "Dragonish",
            "Nymph",
            "Daedric",
            "Spriggan",
            "Centaurian",
            "Impish",
            "Lockpicking",
            "Mercantile",
            "Pickpocket",
            "Stealth",
            "Swimming",
            "Climbing",
            "Backstabbing",
            "Dodging",
            "Running",
            "Destruction",
            "Restoration",
            "Illusion",
            "Alteration",
            "Thaumaturgy",
            "Mysticism",
            "ShortBlade",
            "LongBlade",
            "HandToHand",
            "Axe",
            "BluntWeapon",
            "Archery",
            "CriticalStrike",
        ];
        NAMES[self as usize]
    }
}

/// Resource an effect moves (`DaggerfallEntity` health/fatigue/magicka).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Resource {
    /// Current/maximum health.
    Health,
    /// Current fatigue in DFU raw units (1/64).
    Fatigue,
    /// Current/maximum spell points.
    SpellPoints,
}

impl Resource {
    /// Label used by the observable-field names.
    pub const fn name(self) -> &'static str {
        match self {
            Resource::Health => "health",
            Resource::Fatigue => "fatigue",
            Resource::SpellPoints => "spell_points",
        }
    }
}

/// Numeric amount of a payload: rolled from the effect settings, a constant of
/// the template, or a percentage of the field's current maximum.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Amount {
    /// `GetMagnitude()` (`EntityEffect.cs:786-800`).
    Rolled,
    /// Constant declared by the template (e.g. `StrengthensArmor.cs:28` −5).
    Fixed(i32),
    /// Percentage of the field's live maximum (`RawMaxMagicka * -0.33f`,
    /// `PassiveSpecialsEffect.cs`).
    Percent(i32),
}

/// `FactionFile.SocialGroups` (`FactionFile.cs`), the groups `ChangeReactionMod`
/// targets (`GoodRepWith.cs`, `BadRepWith.cs`, `MasqueOfClavicusEffect.cs`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SocialGroup {
    /// Commoners.
    Commoners,
    /// Merchants.
    Merchants,
    /// Scholars.
    Scholars,
    /// Nobility.
    Nobility,
    /// Underworld.
    Underworld,
    /// Every group (`Params.All`, `GoodRepWith.cs:60-70`).
    All,
}

impl SocialGroup {
    /// The five concrete groups (the order of `reaction_mods`).
    pub const GROUPS: [SocialGroup; 5] = [
        SocialGroup::Commoners,
        SocialGroup::Merchants,
        SocialGroup::Scholars,
        SocialGroup::Nobility,
        SocialGroup::Underworld,
    ];

    /// Label used by the observable-field names.
    pub const fn name(self) -> &'static str {
        match self {
            SocialGroup::Commoners => "commoners",
            SocialGroup::Merchants => "merchants",
            SocialGroup::Scholars => "scholars",
            SocialGroup::Nobility => "nobility",
            SocialGroup::Underworld => "underworld",
            SocialGroup::All => "all",
        }
    }
}

/// `DFCareer.EnemyGroups`, the target groups of `Pacify-*`
/// (`PacifyEffect.cs:27`) and of `PotentVs`/`LowDamageVs` params.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum EnemyGroup {
    /// Animals.
    Animals,
    /// Undead.
    Undead,
    /// Humanoids.
    Humanoids,
    /// Daedra.
    Daedra,
}

impl EnemyGroup {
    /// The four groups in table order.
    pub const ALL: [EnemyGroup; 4] = [
        EnemyGroup::Animals,
        EnemyGroup::Undead,
        EnemyGroup::Humanoids,
        EnemyGroup::Daedra,
    ];

    /// Label used by the observable-field names.
    pub const fn name(self) -> &'static str {
        match self {
            EnemyGroup::Animals => "animals",
            EnemyGroup::Undead => "undead",
            EnemyGroup::Humanoids => "humanoids",
            EnemyGroup::Daedra => "daedra",
        }
    }
}

/// World condition a payload is gated on. Every variant is a predicate the
/// caller resolves (sunlight, interior, moon phase, nearby foes) and reports
/// into [`EffectTarget::gates`]; the effect layer never reads the world.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Gate {
    /// `PlayerEnterExit.IsPlayerInSunlight` (`PassiveSpecialsEffect.cs`).
    Sunlight,
    /// `IsPlayerInHolyPlace`.
    HolyPlace,
    /// `IsPlayerInsideBuilding`.
    InsideBuilding,
    /// `WorldTime.Now.IsNight`.
    Night,
    /// `WorldTime.Now.IsDay`.
    Day,
    /// Swimming (`PlayerMotor.IsSwimming`).
    Swimming,
    /// `WorldContext.Dungeon`.
    Dungeon,
    /// `IsNearUndead`.
    NearUndead,
    /// `IsNearDaedra`.
    NearDaedra,
    /// `IsNearAnimals`.
    NearAnimals,
    /// `IsNearHumanoids`.
    NearHumanoids,
    /// `GetNearbyObjects(Enemy)` non-empty.
    EnemyNearby,
    /// `IsFullMoon`.
    FullMoon,
    /// `IsHalfMoon`.
    HalfMoon,
    /// `IsNewMoon`.
    NewMoon,
    /// `IsWearingHircineRing`.
    WearingHircineRing,
}

impl Gate {
    /// Every gate, in declaration order.
    pub const ALL: [Gate; 16] = [
        Gate::Sunlight,
        Gate::HolyPlace,
        Gate::InsideBuilding,
        Gate::Night,
        Gate::Day,
        Gate::Swimming,
        Gate::Dungeon,
        Gate::NearUndead,
        Gate::NearDaedra,
        Gate::NearAnimals,
        Gate::NearHumanoids,
        Gate::EnemyNearby,
        Gate::FullMoon,
        Gate::HalfMoon,
        Gate::NewMoon,
        Gate::WearingHircineRing,
    ];

    /// Label used by the observable-field names.
    pub const fn name(self) -> &'static str {
        match self {
            Gate::Sunlight => "sunlight",
            Gate::HolyPlace => "holy_place",
            Gate::InsideBuilding => "inside_building",
            Gate::Night => "night",
            Gate::Day => "day",
            Gate::Swimming => "swimming",
            Gate::Dungeon => "dungeon",
            Gate::NearUndead => "near_undead",
            Gate::NearDaedra => "near_daedra",
            Gate::NearAnimals => "near_animals",
            Gate::NearHumanoids => "near_humanoids",
            Gate::EnemyNearby => "enemy_nearby",
            Gate::FullMoon => "full_moon",
            Gate::HalfMoon => "half_moon",
            Gate::NewMoon => "new_moon",
            Gate::WearingHircineRing => "wearing_hircine_ring",
        }
    }
}

/// Boolean entity state raised by a duration/permanent payload.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Flag {
    /// `LevitateMotor.IsLevitating` (`Levitate.cs:131`).
    Levitating,
    /// `Entity.IsSlowFalling` (`Slowfall.cs:92`).
    SlowFalling,
    /// `Entity.IsWaterBreathing` (`WaterBreathing.cs:97`).
    WaterBreathing,
    /// `Entity.IsWaterWalking` (`WaterWalking.cs:98`).
    WaterWalking,
    /// `Entity.IsImmuneToParalysis` (`FreeAction.cs:99`).
    FreeAction,
    /// `Entity.IsSilenced` (`Silence.cs`).
    Silenced,
    /// `Entity.IsParalyzed` (`Paralyze.cs`).
    Paralyzed,
    /// `EnemyEntity.SoulTrapActive` (`SoulTrap.cs`).
    SoulTrapped,
    /// `Entity.IsAbsorbingSpells` (`AbsorbsSpells.cs:73`).
    SpellAbsorbing,
    /// `PlayerEntity.NoClipMode` (`MorphSelf.cs`).
    NoClip,
    /// Population spawns suppressed (`RacialOverrideEffect.cs`, `LycanthropyEffect.cs`).
    PreventSpawns,
    /// `Entity.IsImmuneToDisease` (`VampirismEffect.cs`).
    ImmuneToDisease,
    /// `IsEnhancedJumping` (`Jumping.cs:78-88`).
    EnhancedJumping,
    /// `IsEnhancedClimbing` (`Climbing.cs:78-88`).
    EnhancedClimbing,
    /// `ImprovedAthleticism` (`PassiveSpecialsEffect.cs`).
    ImprovedAthleticism,
    /// `ImprovedAdrenalineRush`.
    ImprovedAdrenalineRush,
    /// `ImprovedAcuteHearing`.
    ImprovedAcuteHearing,
    /// Vampire condition (`VampirismEffect.cs`).
    Vampire,
    /// Werewolf condition (`LycanthropyEffect.cs`).
    Werewolf,
    /// Wereboar condition (`LycanthropyEffect.cs`).
    Wereboar,
    /// Sated flag (`VampirismEffect.cs` "not sated" tick).
    Sated,
}

impl Flag {
    /// Every flag, in declaration order.
    pub const ALL: [Flag; 21] = [
        Flag::Levitating,
        Flag::SlowFalling,
        Flag::WaterBreathing,
        Flag::WaterWalking,
        Flag::FreeAction,
        Flag::Silenced,
        Flag::Paralyzed,
        Flag::SoulTrapped,
        Flag::SpellAbsorbing,
        Flag::NoClip,
        Flag::PreventSpawns,
        Flag::ImmuneToDisease,
        Flag::EnhancedJumping,
        Flag::EnhancedClimbing,
        Flag::ImprovedAthleticism,
        Flag::ImprovedAdrenalineRush,
        Flag::ImprovedAcuteHearing,
        Flag::Vampire,
        Flag::Werewolf,
        Flag::Wereboar,
        Flag::Sated,
    ];

    /// Label used by the observable-field names.
    pub const fn name(self) -> &'static str {
        match self {
            Flag::Levitating => "levitating",
            Flag::SlowFalling => "slow_falling",
            Flag::WaterBreathing => "water_breathing",
            Flag::WaterWalking => "water_walking",
            Flag::FreeAction => "free_action",
            Flag::Silenced => "silenced",
            Flag::Paralyzed => "paralyzed",
            Flag::SoulTrapped => "soul_trapped",
            Flag::SpellAbsorbing => "spell_absorbing",
            Flag::NoClip => "no_clip",
            Flag::PreventSpawns => "prevent_spawns",
            Flag::ImmuneToDisease => "immune_to_disease",
            Flag::EnhancedJumping => "enhanced_jumping",
            Flag::EnhancedClimbing => "enhanced_climbing",
            Flag::ImprovedAthleticism => "improved_athleticism",
            Flag::ImprovedAdrenalineRush => "improved_adrenaline_rush",
            Flag::ImprovedAcuteHearing => "improved_acute_hearing",
            Flag::Vampire => "vampire",
            Flag::Werewolf => "werewolf",
            Flag::Wereboar => "wereboar",
            Flag::Sated => "sated",
        }
    }
}

/// `MagicalConcealmentFlags` (`MagicAndEffectsEnums.cs`), the six flags of the
/// `ConcealmentEffect` family (`ConcealmentEffect.cs:63` / `:83`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ConcealmentFlag {
    /// `ShadeNormal` (`ShadowNormal.cs:39`).
    ShadowNormal,
    /// `ShadeTrue` (`ShadowTrue.cs`).
    ShadowTrue,
    /// `InvisibleNormal` (`InvisibilityNormal.cs:39`).
    InvisibleNormal,
    /// `InvisibleTrue` (`InvisibilityTrue.cs`).
    InvisibleTrue,
    /// `BlendingNormal` (`ChameleonNormal.cs`).
    BlendingNormal,
    /// `BlendingTrue` (`ChameleonTrue.cs`).
    BlendingTrue,
}

impl ConcealmentFlag {
    /// Every concealment flag, in declaration order.
    pub const ALL: [ConcealmentFlag; 6] = [
        ConcealmentFlag::ShadowNormal,
        ConcealmentFlag::ShadowTrue,
        ConcealmentFlag::InvisibleNormal,
        ConcealmentFlag::InvisibleTrue,
        ConcealmentFlag::BlendingNormal,
        ConcealmentFlag::BlendingTrue,
    ];

    /// Label used by the observable-field names.
    pub const fn name(self) -> &'static str {
        match self {
            ConcealmentFlag::ShadowNormal => "shadow_normal",
            ConcealmentFlag::ShadowTrue => "shadow_true",
            ConcealmentFlag::InvisibleNormal => "invisible_normal",
            ConcealmentFlag::InvisibleTrue => "invisible_true",
            ConcealmentFlag::BlendingNormal => "blending_normal",
            ConcealmentFlag::BlendingTrue => "blending_true",
        }
    }
}

/// Chance-valued state carried by a live effect (`EntityEffectManager.TryResistance`,
/// `SpellAbsorption.cs`, `SpellReflection.cs`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ChanceState {
    /// Spell resistance (`SpellResistance.cs`, `TryResistance`).
    SpellResistance,
    /// Spell absorption (`SpellAbsorption.cs`).
    SpellAbsorption,
    /// Spell reflection (`SpellReflection.cs`).
    SpellReflection,
}

impl ChanceState {
    /// Every chance state, in declaration order.
    pub const ALL: [ChanceState; 3] = [
        ChanceState::SpellResistance,
        ChanceState::SpellAbsorption,
        ChanceState::SpellReflection,
    ];

    /// Label used by the observable-field names.
    pub const fn name(self) -> &'static str {
        match self {
            ChanceState::SpellResistance => "spell_resistance",
            ChanceState::SpellAbsorption => "spell_absorption",
            ChanceState::SpellReflection => "spell_reflection",
        }
    }
}

/// Instant cure of a `Cure-*` template.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Cure {
    /// `CureAllDiseases()` (`CureDisease.cs:85`).
    Disease,
    /// `CureAllPoisons()` (`CurePoison.cs:68`).
    Poison,
    /// Paralysis cleared (`CureParalyzation.cs`).
    Paralyzation,
}

impl Cure {
    /// Label used by the observable-field names.
    pub const fn name(self) -> &'static str {
        match self {
            Cure::Disease => "cure_disease",
            Cure::Poison => "cure_poison",
            Cure::Paralyzation => "cure_paralyzation",
        }
    }
}

/// `EnchantmentPayloadFlags` (`MagicAndEffectsEnums.cs`): when an enchantment
/// payload runs. `Held` is the `ConstantEffect` family; the rest are
/// `EnchantmentPayloadCallback` contexts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Trigger {
    /// `EnchantmentPayloadFlags.Held` (`ConstantEffect`).
    Held,
    /// `EnchantmentPayloadFlags.Equipped` (`CastWhenHeld.cs`).
    Equipped,
    /// `EnchantmentPayloadFlags.Strikes` (`CastWhenStrikes.cs`).
    Strikes,
    /// `EnchantmentPayloadFlags.Used` (`CastWhenUsed.cs`).
    Used,
    /// `EnchantmentPayloadFlags.Enchanted` (`ExtraWeight.cs`).
    Enchanted,
    /// `EnchantmentPayloadFlags.Breaks` (`SoulBound.cs`).
    Breaks,
    /// `EnchantmentPayloadFlags.MagicRound` (`CastWhenHeld.cs`).
    MagicRound,
    /// `EnchantmentPayloadFlags.RerollEffect` (`CastWhenHeld.cs`).
    Reroll,
    /// `EnchantmentPayloadFlags.None` invoked **out of band** when the bearer is
    /// struck: `FormulaHelper.CalculateWeaponAttackDamage` calls the worn
    /// artifact's `EnchantmentPayloadCallback(None, targetEntity: attacker,
    /// sourceDamage: damage)` (`FormulaHelper.cs:704-719`, and only when the
    /// struck entity is the player — the Ring of Namira case).
    Struck,
}

impl Trigger {
    /// Label used by the observable-field names.
    pub const fn name(self) -> &'static str {
        match self {
            Trigger::Held => "held",
            Trigger::Equipped => "equipped",
            Trigger::Strikes => "strikes",
            Trigger::Used => "used",
            Trigger::Enchanted => "enchanted",
            Trigger::Breaks => "breaks",
            Trigger::MagicRound => "magic_round",
            Trigger::Reroll => "reroll",
            Trigger::Struck => "struck",
        }
    }
}

/// World/engine request emitted by a payload — the typed boundary of spec §7
/// (same pattern as M29's `QuestRequest`). The target counts requests; the owner
/// organ consumes them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Request {
    /// Anchor/teleport (`Teleport.cs:67`).
    Teleport,
    /// `TriggerOpenEffect` (`Open.cs:96-123`).
    Open,
    /// `Lock.cs` lock-level request.
    Lock,
    /// `CreateItem.cs`.
    CreateItem,
    /// `Identify.cs:79` (refunds the spell cost).
    Identify,
    /// `ComprehendLanguages.cs`.
    ComprehendLanguages,
    /// `DispelMagic.cs` spell picker.
    DispelMagic,
    /// `DispelUndead.cs`.
    DispelUndead,
    /// `DispelDaedra.cs`.
    DispelDaedra,
    /// `DetectMagic.cs`.
    DetectMagic,
    /// `DetectEnemy.cs`.
    DetectEnemy,
    /// `DetectTreasure.cs`.
    DetectTreasure,
    /// `LightNormal.cs` / `MageLight.cs` light source.
    Light,
    /// `MorphSelf.cs` form change.
    MorphSelf,
    /// `Disintegrate.cs` item destruction.
    Disintegrate,
    /// `CharmEffect.cs` reaction shift.
    Charm,
    /// `PacifyEffect.cs` calm by group.
    Pacify(EnemyGroup),
    /// `AzurasStarEffect.cs` soul capture.
    SoulCapture,
    /// `MehrunesRazorEffect.cs` instant kill roll.
    Instakill,
    /// `WabbajackEffect.cs` random outcome.
    Wabbajack,
    /// `SanguineRoseEffect.cs` / `SkullOfCorruptionEffect.cs` summon.
    SpawnAlly,
    /// `OghmaInfiniumEffect.cs` level-up.
    LevelUp,
    /// `SkullOfCorruptionEffect.cs` clone.
    Clone,
    /// `VampirismEffect.cs` vampire spell assignment.
    AssignVampireSpells,
    /// `LycanthropyEffect.cs` lycanthropy spell assignment.
    AssignLycanthropySpell,
    /// `EntityEffectManager.CureAll()` (`VampirismEffect.cs`).
    CureAll,
    /// `CastWhenHeld/Strikes/Used` invoke a `SPELLS.STD` record.
    CastSpell(u16),
    /// `RingOfNamiraEffect.cs:50-72`: damage dealt back to the attacker, scaled
    /// by its team (`Vermin`/`Spriggans`/… 0, `Daedra` ½, `Undead` ×2, else ×1).
    /// The attacker, the amount (`sourceDamage`) and the item's durability loss
    /// live in the combat owner, not in the effect layer.
    ReflectDamage,
}

impl Request {
    /// Label used by the observable-field names. `CastSpell` keeps its record
    /// id in the label so different enchantments stay distinguishable.
    pub fn label(self) -> String {
        let name = match self {
            Request::Teleport => "teleport",
            Request::Open => "open",
            Request::Lock => "lock",
            Request::CreateItem => "create_item",
            Request::Identify => "identify",
            Request::ComprehendLanguages => "comprehend_languages",
            Request::DispelMagic => "dispel_magic",
            Request::DispelUndead => "dispel_undead",
            Request::DispelDaedra => "dispel_daedra",
            Request::DetectMagic => "detect_magic",
            Request::DetectEnemy => "detect_enemy",
            Request::DetectTreasure => "detect_treasure",
            Request::Light => "light",
            Request::MorphSelf => "morph_self",
            Request::Disintegrate => "disintegrate",
            Request::Charm => "charm",
            Request::Pacify(EnemyGroup::Animals) => "pacify_animals",
            Request::Pacify(EnemyGroup::Undead) => "pacify_undead",
            Request::Pacify(EnemyGroup::Humanoids) => "pacify_humanoids",
            Request::Pacify(EnemyGroup::Daedra) => "pacify_daedra",
            Request::SoulCapture => "soul_capture",
            Request::Instakill => "instakill",
            Request::Wabbajack => "wabbajack",
            Request::SpawnAlly => "spawn_ally",
            Request::LevelUp => "level_up",
            Request::Clone => "clone",
            Request::AssignVampireSpells => "assign_vampire_spells",
            Request::AssignLycanthropySpell => "assign_lycanthropy_spell",
            Request::CureAll => "cure_all",
            Request::CastSpell(id) => return format!("cast_spell_{id}"),
            Request::ReflectDamage => "reflect_damage",
        };
        name.to_string()
    }
}

/// `DiseaseData` (`MagicAndEffectsStructs.cs:232-256`): the 15 bytes a classic
/// disease applies per day. `days_max == 0xFF` means permanent
/// (`DiseaseEffect.IsDiseasePermanent`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DiseaseData {
    /// Strength component (`STR`).
    pub strength: u8,
    /// Intelligence component (`INT`).
    pub intelligence: u8,
    /// Willpower component (`WIL`).
    pub willpower: u8,
    /// Agility component (`AGI`).
    pub agility: u8,
    /// Endurance component (`END`).
    pub endurance: u8,
    /// Personality component (`PER`).
    pub personality: u8,
    /// Speed component (`SPD`).
    pub speed: u8,
    /// Luck component (`LUC`).
    pub luck: u8,
    /// Health component (`HEA`).
    pub health: u8,
    /// Fatigue component (`FAT`).
    pub fatigue: u8,
    /// Spell-point component (`SPL`).
    pub spell_points: u8,
    /// Minimum daily damage (`minDamage`).
    pub min_damage: u8,
    /// Maximum daily damage (`maxDamage`).
    pub max_damage: u8,
    /// Minimum symptomatic days (`daysOfSymptomsMin`).
    pub days_min: u8,
    /// Maximum symptomatic days (`daysOfSymptomsMax`); `0xFF` = permanent.
    pub days_max: u8,
}

impl DiseaseData {
    /// `days_max == 0xFF` (`DiseaseEffect.IsDiseasePermanent`).
    pub const fn permanent(&self) -> bool {
        self.days_max == 0xFF
    }
}

/// Per-minute payload of one poison (`PoisonEffect.IncrementPoisonEffects`,
/// `PoisonEffect.cs:247-311`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PoisonTick {
    /// Attribute rows: `(attribute, min, max, sign)` — `sign` −1 damages,
    /// +1 buffs (drugs).
    pub attributes: &'static [(Attribute, i32, i32, i8)],
    /// Resource rows: `(resource, min, max, sign, scale)` — `scale` is 64 for
    /// the `raw = true` fatigue calls, 1 otherwise.
    pub resources: &'static [(Resource, i32, i32, i8, i32)],
}

/// One poison variant: incubation, duration and the per-minute payload
/// (`PoisonEffect.cs:137-149`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PoisonSpec {
    /// `MinMinutesToPoison` / `MaxMinutesToPoison` (inclusive).
    pub minutes_to_start: (u16, u16),
    /// `MinRoundsOfPoison` / `MaxRoundsOfPoison` (inclusive).
    pub minutes_of_poison: (u16, u16),
    /// Per-minute payload.
    pub tick: PoisonTick,
}

/// Lifecycle class derived from the payload (contract §2.1). A template never
/// declares it: it is the strongest class its payloads imply.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Lifetime {
    /// Runs once at cast, never persists.
    Instant = 0,
    /// Live while rounds remain; `End` reverts its state.
    Duration = 1,
    /// Never expires by rounds: drain persists until healed, disease/poison
    /// until cured or out of days/minutes.
    Permanent = 2,
}

/// Payload of an effect template (contract §2). Every DFU entity mutation of
/// the 153 `Effects/` files is one of these; `&[Payload]` composes them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Payload {
    /// `Damage-*` / `Heal-*`: instant resource delta of `magnitude`.
    Resource {
        /// Affected resource.
        resource: Resource,
        /// `-1` damages, `+1` heals.
        sign: i8,
    },
    /// `ContinuousDamage-*` / `Regenerate`: resource delta every magic round.
    ResourcePerRound {
        /// Affected resource.
        resource: Resource,
        /// `-1` damages, `+1` heals.
        sign: i8,
    },
    /// `HealthLeech`: instant resource delta of a **constant the template
    /// declares** (`leechWeaponAmount = 8`, `leechCastAmount = 16`,
    /// `HealthLeech.cs:27-30`), independent of `SupportMagnitude` — an item
    /// enchantment has no spellmaker magnitude to scale. Same for
    /// `Passive-Specials` regeneration: the literal `regenerateAmount = 1`
    /// (`PassiveSpecialsEffect.cs:33`, applied at `:142`) a template without
    /// `SupportMagnitude` cannot roll. Inside a live instance
    /// [`MagicState::cast_effects`] re-applies it on every application
    /// (cast + each magic round), which is the per-round cadence DFU ticks.
    FixedResource {
        /// Affected resource.
        resource: Resource,
        /// `-1` damages, `+1` heals.
        sign: i8,
        /// Constant delta in the template's own units.
        amount: i32,
    },
    /// `ItemDeteriorates`: instant item-condition delta
    /// (`conditionLossAmount = 1`, `ItemDeteriorates.cs:29-30`, applied at
    /// `:72-104` while the param's gate holds).
    ItemCondition {
        /// Signed condition delta: negative wears the item down.
        delta: i32,
    },
    /// `VampiricEffect` `WhenStrikes`: signed resource delta of the **amount
    /// the trigger supplies** (`sourceEntity.CurrentHealth += sourceDamage`,
    /// `VampiricEffect.cs:127`) — no constant, no magnitude.
    TriggerAmount {
        /// Affected resource.
        resource: Resource,
        /// `-1` damages, `+1` heals.
        sign: i8,
    },
    /// `Heal-<Attribute>`: instant heal of attribute damage.
    HealAttribute(Attribute),
    /// `Drain-<Attribute>` / `Transfer-<Attribute>`: permanent damage, clamped
    /// to `base − 1` (`DrainEffect.cs:125-137`).
    DrainAttribute(Attribute),
    /// `Transfer-<Attribute>`: heals the caster's attribute damage.
    HealCaster(Attribute),
    /// `Transfer-Health` / `Transfer-Fatigue`: drain target, heal caster.
    TransferResource(Resource),
    /// `Fortify-<Attribute>`: duration attribute modifier.
    FortifyAttribute(Attribute),
    /// `ChangeStatMaxMod`: duration maximum-attribute modifier.
    MaxAttribute(Attribute, Amount),
    /// Duration skill modifier.
    Skill(EffectSkill, Amount),
    /// Duration resistance modifier.
    Resistance(Element, Amount),
    /// `ChangeMaxMagickaModifier`.
    MaxMagicka(Amount),
    /// `SetIncreased/DecreasedArmorValueModifier` (negative is better).
    ArmorValue(i32),
    /// `SetIncreasedWeightAllowanceMultiplier`, percent of the base.
    WeightAllowance(i32),
    /// `ChangeReactionMod`.
    Reaction(SocialGroup, Amount),
    /// `ChangeChanceToHitModifier`.
    ChanceToHit(Amount),
    /// `SetMaxHealthLimiter`.
    HealthLimiter(Amount),
    /// Boolean entity flag.
    Flag(Flag),
    /// `MagicalConcealmentFlags` bit.
    Concealment(ConcealmentFlag),
    /// Chance-valued state (`ChanceValue()`).
    ChanceState(ChanceState),
    /// Instant cure.
    Cure(Cure),
    /// Absorb pool of `magnitude` points (`Shield.cs:55-63`).
    Shield,
    /// World/engine request.
    Request(Request),
    /// Damage modulation against an enemy group (`PotentVs`/`LowDamageVs`).
    Versus {
        /// Affected group.
        group: EnemyGroup,
        /// Damage delta.
        amount: i32,
    },
    /// Enchantment payload: `action` runs when `trigger` fires.
    On {
        /// Context of the payload.
        trigger: Trigger,
        /// Primitives the payload runs.
        action: &'static [Payload],
    },
    /// Payloads DFU applies to the enchantment's **bearer** (`sourceEntity`)
    /// from inside a trigger whose remaining payloads act on `targetEntity`
    /// (`MaceOfMolagBalEffect.cs:126`, `:138-139`: what the victim lost buffs
    /// the bearer's max magicka and its strength). Only meaningful inside
    /// [`Payload::On`]; [`run_enchantment`] applies it to the bearer.
    Bearer(&'static [Payload]),
    /// Per-round damage gated by a world condition.
    GatedDamage {
        /// Condition that must hold.
        gate: Gate,
        /// Damage per round.
        amount: i32,
    },
    /// Daily disease payload (`Diseases/`).
    Disease(&'static DiseaseData),
    /// Per-minute poison payload (`Poison-*`).
    Poison(&'static PoisonSpec),
}

impl Payload {
    /// Lifecycle class of one payload (contract §2.1).
    pub const fn lifetime(self) -> Lifetime {
        match self {
            Payload::DrainAttribute(_)
            | Payload::HealCaster(_)
            | Payload::Disease(_)
            | Payload::Poison(_) => Lifetime::Permanent,
            Payload::ResourcePerRound { .. }
            | Payload::FortifyAttribute(_)
            | Payload::MaxAttribute(_, _)
            | Payload::Skill(_, _)
            | Payload::Resistance(_, _)
            | Payload::MaxMagicka(_)
            | Payload::ArmorValue(_)
            | Payload::WeightAllowance(_)
            | Payload::Reaction(_, _)
            | Payload::ChanceToHit(_)
            | Payload::HealthLimiter(_)
            | Payload::Flag(_)
            | Payload::Concealment(_)
            | Payload::ChanceState(_)
            | Payload::Shield
            | Payload::GatedDamage { .. } => Lifetime::Duration,
            Payload::Resource { .. }
            | Payload::FixedResource { .. }
            | Payload::ItemCondition { .. }
            | Payload::TriggerAmount { .. }
            | Payload::HealAttribute(_)
            | Payload::TransferResource(_)
            | Payload::Cure(_)
            | Payload::Request(_)
            | Payload::Versus { .. }
            | Payload::On { .. }
            | Payload::Bearer(_) => Lifetime::Instant,
        }
    }

    /// Rolls a fresh magnitude on every application (`GetMagnitude` **inside**
    /// `MagicRound`): only the per-round resource payloads do.
    pub const fn rolls_magnitude_per_round(self) -> bool {
        matches!(self, Payload::ResourcePerRound { .. })
    }

    /// True when the payload is a **duration state** (`accumulate_state`): it
    /// contributes while its instance lives and disappears with it.
    pub const fn carries_state(&self) -> bool {
        matches!(
            self,
            Payload::FortifyAttribute(_)
                | Payload::MaxAttribute(_, _)
                | Payload::Skill(_, _)
                | Payload::Resistance(_, _)
                | Payload::MaxMagicka(_)
                | Payload::ArmorValue(_)
                | Payload::WeightAllowance(_)
                | Payload::Reaction(_, _)
                | Payload::ChanceToHit(_)
                | Payload::HealthLimiter(_)
                | Payload::Flag(_)
                | Payload::Concealment(_)
                | Payload::ChanceState(_)
                | Payload::Shield
                | Payload::Versus { .. }
        )
    }
}

/// Bit set of the 21 [`Flag`]s a live effect can raise. A bitmask keeps
/// [`EffectTarget::observe`] exhaustive without 21 named fields.
///
/// `Levitating`, `SlowFalling` and `WaterBreathing` have an M19 consumer
/// (`locomotion.rs` `MoveInput.levitating`, `PlayerBody.slow_falling`,
/// `PlayerBody.water_breathing`); the rest are **declared without a consumer** —
/// M19 has no branch for them (audit M19 gap P2-2) and this lane does not invent
/// one.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct StatusFlags {
    bits: u64,
}

impl StatusFlags {
    /// Raw bits (stable evidence/`EffectTarget` hashing).
    pub const fn bits(self) -> u64 {
        self.bits
    }

    /// Restores a bit set.
    pub const fn from_bits(bits: u64) -> Self {
        Self { bits }
    }

    /// Whether `flag` is set.
    pub const fn get(self, flag: Flag) -> bool {
        self.bits & (1 << flag as u64) != 0
    }

    /// Sets or clears `flag`.
    pub fn set(&mut self, flag: Flag, value: bool) {
        let mask = 1u64 << flag as u64;
        if value {
            self.bits |= mask;
        } else {
            self.bits &= !mask;
        }
    }

    /// `(name, value)` of every flag, in [`Flag::ALL`] order.
    pub fn named(self) -> [(&'static str, bool); Flag::ALL.len()] {
        let mut out = [("", false); Flag::ALL.len()];
        for (index, flag) in Flag::ALL.iter().enumerate() {
            out[index] = (flag.name(), self.get(*flag));
        }
        out
    }
}

/// Exactly the flags M19 consumes. `EffectTarget::movement_effects()` is the
/// only bridge the locomotion engine needs: the caller copies these four
/// values into `MoveInput.levitating` / `PlayerBody.water_breathing` /
/// `PlayerBody.slow_falling` / `PlayerBody.water_walking` (no flag is set by
/// hand).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MovementEffects {
    /// `MoveInput.levitating` (drives `MotionMode::Levitating`).
    pub levitating: bool,
    /// `PlayerBody.water_breathing` (suppresses the breath drain).
    pub water_breathing: bool,
    /// `PlayerBody.slow_falling` (slow-fall descent).
    pub slow_falling: bool,
    /// `PlayerBody.water_walking` (`PlayerEntity.IsWaterWalking`, set by
    /// `WaterWalking.StartWaterWalking`, `WaterWalking.cs:91-98`): swimming
    /// keeps the normal resolved speed (`LevitateMotor.cs:116-121`) and cannot
    /// sink under encumbrance (`LevitateMotor.cs:83-85`).
    pub water_walking: bool,
}

/// Neutral target of a cast: the subset of `DaggerfallEntity` state the 153
/// templates touch, plus the worn item the enchantment payloads act on. Every
/// clamped write follows `DaggerfallEntity.SetHealth/SetFatigue/SetMagicka`
/// (`DaggerfallEntity.cs:331-338`, `:379-387`) and `SetStatMod`
/// (`DaggerfallEntity.cs:440-475`).
///
/// The struct holds no world: gates, nearby groups and moon phase arrive through
/// [`EffectTarget::gates`]; world actions leave as [`Request`]s in
/// [`EffectTarget::requests`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EffectTarget {
    /// Current health, clamped to `[0, effective_max_health()]`.
    pub health: i32,
    /// Maximum health (`DaggerfallEntity.MaxHealth`).
    pub max_health: i32,
    /// Current fatigue in DFU raw units (1/64, `DaggerfallEntity.cs:39`),
    /// clamped to `[0, max_fatigue()]`.
    pub fatigue: i32,
    /// Current spell points (magicka), clamped to `[0, effective_max_spell_points()]`.
    pub spell_points: i32,
    /// Maximum spell points (`DaggerfallEntity.MaxMagicka`).
    pub max_spell_points: i32,
    /// Base attribute values (8, `DFCareer.Stats` order).
    pub attributes: [i32; ATTRIBUTE_COUNT],
    /// Duration attribute modifiers (`EntityEffect.StatMods`), summed over the
    /// live effects; read through [`Self::effective_attribute`].
    pub attribute_mods: [i32; ATTRIBUTE_COUNT],
    /// Duration maximum-attribute modifiers (`ChangeStatMaxMod`,
    /// `SetStatMaxMod`), summed over the live effects.
    pub attribute_max_mods: [i32; ATTRIBUTE_COUNT],
    /// Permanent attribute damage (`ChangeStatMod`, `DrainEffect.cs:125-137`);
    /// removed only by `Heal-<Attribute>`.
    pub attribute_damage: [i32; ATTRIBUTE_COUNT],
    /// Base skill values (`DFCareer.Skills` order).
    pub skills: [i32; EFFECT_SKILL_COUNT],
    /// Duration skill modifiers (`ChangeSkillMod`), summed over live effects.
    pub skill_mods: [i32; EFFECT_SKILL_COUNT],
    /// Permanent skill damage (`ChangeSkillMod` with a negative sign).
    pub skill_damage: [i32; EFFECT_SKILL_COUNT],
    /// Duration resistance chances (`RaiseResistanceChance`,
    /// `DaggerfallEntity.cs:689-692`), summed over the live effects.
    pub resistances: [i32; ELEMENT_COUNT],
    /// Duration resistance flags (`SetResistanceFlag`,
    /// `DaggerfallEntity.cs:657-660`).
    pub resistance_flags: [bool; ELEMENT_COUNT],
    /// `ChanceValue()` of the live chance states (`SpellResistance`,
    /// `SpellAbsorption`, `SpellReflection`).
    pub chance_states: [i32; 3],
    /// `ChangeMaxMagickaModifier` (`+ MaxMagicka`).
    pub max_magicka_mod: i32,
    /// `SetMaxHealthLimiter` (`Vampirism.cs`, `Lycanthropy.cs`); `0` = none.
    pub max_health_limiter: i32,
    /// `SetIncreased/DecreasedArmorValueModifier` (negative is better).
    pub armor_value_mod: i32,
    /// `SetIncreasedWeightAllowanceMultiplier`, percent of the base (`100`).
    pub weight_allowance: i32,
    /// `ChangeReactionMod` per [`SocialGroup::GROUPS`] index.
    pub reaction_mods: [i32; 5],
    /// `ChangeChanceToHitModifier`.
    pub chance_to_hit_mod: i32,
    /// `PotentVs` / `LowDamageVs` damage deltas, indexed by [`EnemyGroup`].
    pub versus: [i32; 4],
    /// Status flags raised by duration/permanent payloads.
    pub flags: StatusFlags,
    /// `MagicalConcealmentFlags` bits, indexed by [`ConcealmentFlag`].
    pub concealment: u8,
    /// Absorb pool of `Shield` (`Shield.cs:55-63`), points.
    pub shield: i32,
    /// Active diseases, keyed by the `Diseases/` template key
    /// (`CureAllDiseases` empties it).
    pub diseases: Vec<&'static str>,
    /// Active poisons, keyed by the `Poison-*` template key
    /// (`CureAllPoisons` empties it).
    pub poisons: Vec<&'static str>,
    /// Requests emitted by applied payloads, in application order. The owner
    /// organ consumes them (spec §7.6).
    pub requests: Vec<Request>,
    /// Worn item the enchantment payloads act on.
    pub item: ItemState,
    /// World conditions the caller resolved (`PassiveSpecialsEffect.cs`).
    pub gates: GateSet,
}

/// Worn-item state the enchantment family mutates (`ExtraWeight`,
/// `FeatherWeight`, `ItemDeteriorates`, `RepairsObjects`, `SoulBound`,
/// `StrengthensArmor`/`WeakensArmor` read it through `armor_value`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ItemState {
    /// Weight as a percent of the template weight (`100` = unmodified).
    pub weight_percent: i32,
    /// Current condition.
    pub condition: i32,
    /// Maximum condition before modifiers.
    pub max_condition: i32,
    /// `SetItemMaxConditionModifier` delta.
    pub max_condition_mod: i32,
    /// `SoulBound.cs` flag.
    pub soul_bound: bool,
    /// `EnchantmentParam.Value` of the worn enchantment: the magnitude an item
    /// payload rolls (`MaceOfMolagBalEffect.cs`, `MasqueOfClavicusEffect.cs`)
    /// and the `ClassicParam` `SoulBound.cs:100` reads. `0` = none declared.
    pub magnitude: i32,
}

impl Default for ItemState {
    fn default() -> Self {
        Self {
            weight_percent: 100,
            condition: 1000,
            max_condition: 1000,
            max_condition_mod: 0,
            soul_bound: false,
            magnitude: 0,
        }
    }
}

/// Bit set of the 16 [`Gate`]s the caller resolved for this target.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct GateSet {
    bits: u32,
}

impl GateSet {
    /// Raw bits.
    pub const fn bits(self) -> u32 {
        self.bits
    }

    /// Restores a bit set.
    pub const fn from_bits(bits: u32) -> Self {
        Self { bits }
    }

    /// Whether `gate` holds.
    pub const fn holds(self, gate: Gate) -> bool {
        self.bits & (1 << gate as u32) != 0
    }

    /// Sets or clears `gate`.
    pub fn set(&mut self, gate: Gate, value: bool) {
        let mask = 1u32 << gate as u32;
        if value {
            self.bits |= mask;
        } else {
            self.bits &= !mask;
        }
    }
}

impl EffectTarget {
    /// Target with full health/spell points, zero fatigue and the given base
    /// attributes.
    pub fn new(health: i32, spell_points: i32, attributes: [i32; ATTRIBUTE_COUNT]) -> Self {
        Self {
            health,
            max_health: health,
            fatigue: 0,
            spell_points,
            max_spell_points: spell_points,
            attributes,
            attribute_mods: [0; ATTRIBUTE_COUNT],
            attribute_max_mods: [0; ATTRIBUTE_COUNT],
            attribute_damage: [0; ATTRIBUTE_COUNT],
            skills: [0; EFFECT_SKILL_COUNT],
            skill_mods: [0; EFFECT_SKILL_COUNT],
            skill_damage: [0; EFFECT_SKILL_COUNT],
            resistances: [0; ELEMENT_COUNT],
            resistance_flags: [false; ELEMENT_COUNT],
            chance_states: [0; 3],
            max_magicka_mod: 0,
            max_health_limiter: 0,
            armor_value_mod: 0,
            weight_allowance: 100,
            reaction_mods: [0; 5],
            chance_to_hit_mod: 0,
            versus: [0; 4],
            flags: StatusFlags::default(),
            concealment: 0,
            shield: 0,
            diseases: Vec::new(),
            poisons: Vec::new(),
            requests: Vec::new(),
            item: ItemState::default(),
            gates: GateSet::default(),
        }
    }

    /// `MaxFatigue = (LiveStrength + LiveEndurance) · 64`
    /// (`DaggerfallEntity.cs:262`, `:39`); live values include the active
    /// modifiers.
    pub fn max_fatigue(&self) -> i32 {
        (self.effective_attribute(Attribute::Strength)
            + self.effective_attribute(Attribute::Endurance))
            * FATIGUE_MULTIPLIER
    }

    /// `MaxHealth` after `SetMaxHealthLimiter` (`0` disables the limiter).
    pub fn effective_max_health(&self) -> i32 {
        if self.max_health_limiter > 0 {
            self.max_health.min(self.max_health_limiter)
        } else {
            self.max_health
        }
    }

    /// `MaxMagicka` after `ChangeMaxMagickaModifier`, floored at 0.
    pub fn effective_max_spell_points(&self) -> i32 {
        (self.max_spell_points + self.max_magicka_mod).max(0)
    }

    /// Base attribute plus the duration and maximum modifiers (DFU `LiveStats`)
    /// minus permanent damage.
    pub fn effective_attribute(&self, attribute: Attribute) -> i32 {
        let index = attribute as usize;
        self.attributes[index] + self.attribute_mods[index] + self.attribute_max_mods[index]
            - self.attribute_damage[index]
    }

    /// Base skill plus duration modifiers minus permanent damage.
    pub fn effective_skill(&self, skill: EffectSkill) -> i32 {
        let index = skill as usize;
        self.skills[index] + self.skill_mods[index] - self.skill_damage[index]
    }

    /// `EntityEffectManager.TryResistance` chance for `state`.
    pub fn chance_state(&self, state: ChanceState) -> i32 {
        self.chance_states[state as usize]
    }

    /// How many times `request` was emitted and not yet consumed.
    pub fn request_count(&self, request: Request) -> usize {
        self.requests.iter().filter(|r| **r == request).count()
    }

    /// Whether the target carries `flag`.
    pub fn has_flag(&self, flag: Flag) -> bool {
        self.flags.get(flag)
    }

    /// Whether the target carries `flag` (`MagicalConcealmentFlags` bit).
    pub fn concealed(&self, flag: ConcealmentFlag) -> bool {
        self.concealment & (1 << flag as u8) != 0
    }

    /// The four flags M19 consumes (see [`MovementEffects`]).
    pub fn movement_effects(&self) -> MovementEffects {
        MovementEffects {
            levitating: self.flags.get(Flag::Levitating),
            water_breathing: self.flags.get(Flag::WaterBreathing),
            slow_falling: self.flags.get(Flag::SlowFalling),
            water_walking: self.flags.get(Flag::WaterWalking),
        }
    }
}

/// One observable mutation of the target: `before`/`after` of a **labelled**
/// field, never a delta-only claim. Used by the probes and by the handoff
/// evidence.
///
/// `field` is a stable label produced by [`EffectTarget::observe`]
/// (`"health"`, `"attribute.Strength"`, `"skill.Destruction"`,
/// `"chance.spell_resistance"`, `"request.teleport"`, `"disease"`,
/// `"item.condition"`, …), so a new payload never needs a new change variant
/// and the evidence covers every field the engine can touch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EffectChange {
    /// Observable field label.
    pub field: String,
    /// Value before the application (`bool` as `0/1`).
    pub before: i64,
    /// Value after the application.
    pub after: i64,
}

impl EffectChange {
    /// One row. `before == after` rows are dropped by [`diff_observations`].
    pub fn new(field: impl Into<String>, before: i64, after: i64) -> Self {
        Self {
            field: field.into(),
            before,
            after,
        }
    }
}

/// Diff of two [`EffectTarget::observe`] snapshots, ordered by label.
pub fn diff_observations(
    before: &[(String, i64)],
    after: &[(String, i64)],
) -> Vec<EffectChange> {
    use std::collections::BTreeMap;
    let mut previous: BTreeMap<&str, i64> =
        before.iter().map(|(field, value)| (field.as_str(), *value)).collect();
    let mut changes = Vec::new();
    for (field, value) in after {
        let prior = previous.remove(field.as_str()).unwrap_or(0);
        if prior != *value {
            changes.push(EffectChange::new(field.as_str(), prior, *value));
        }
    }
    // Fields that disappeared (a consumed request, an empty request list).
    for (field, value) in previous {
        if value != 0 {
            changes.push(EffectChange::new(field, value, 0));
        }
    }
    changes
}

/// Observations of a set of request labels plus their counts.
fn observe_requests(target: &EffectTarget, out: &mut Vec<(String, i64)>) {
    for request in &target.requests {
        let field = format!("request.{}", request.label());
        let count = out
            .iter_mut()
            .find(|(label, _)| *label == field)
            .map(|(_, count)| count);
        match count {
            Some(count) => *count += 1,
            None => out.push((field, 1)),
        }
    }
}

impl EffectTarget {
    /// Every observable scalar as `(label, value)` (`bool` → `0/1`), in a
    /// stable order: resources, attributes, skills, resistances, chance states,
    /// modifiers, flags, concealment, glyph counters, item and requests.
    ///
    /// [`diff_observations`] turns two snapshots into [`EffectChange`]s, which
    /// is how a probe or the QA evidence states *what changed*, never a bare
    /// "applied" claim.
    pub fn observe(&self) -> Vec<(String, i64)> {
        let mut out: Vec<(String, i64)> = Vec::with_capacity(160);
        out.push(("health".to_string(), self.health as i64));
        out.push(("max_health".to_string(), self.effective_max_health() as i64));
        out.push(("fatigue".to_string(), self.fatigue as i64));
        out.push(("max_fatigue".to_string(), self.max_fatigue() as i64));
        out.push(("spell_points".to_string(), self.spell_points as i64));
        out.push((
            "max_spell_points".to_string(),
            self.effective_max_spell_points() as i64,
        ));
        for attribute in ATTRIBUTES {
            let index = attribute as usize;
            let name = attribute.name();
            out.push((format!("attribute.{name}"), self.effective_attribute(attribute) as i64));
            out.push((format!("attribute_mod.{name}"), self.attribute_mods[index] as i64));
            out.push((
                format!("attribute_max_mod.{name}"),
                self.attribute_max_mods[index] as i64,
            ));
            out.push((
                format!("attribute_damage.{name}"),
                self.attribute_damage[index] as i64,
            ));
        }
        for skill in EffectSkill::ALL {
            let index = skill as usize;
            let name = skill.name();
            out.push((format!("skill.{name}"), self.effective_skill(skill) as i64));
            out.push((format!("skill_mod.{name}"), self.skill_mods[index] as i64));
            out.push((format!("skill_damage.{name}"), self.skill_damage[index] as i64));
        }
        for element in ELEMENTS {
            let index = element as usize;
            out.push((format!("resistance.{}", element.name()), self.resistances[index] as i64));
            out.push((
                format!("resistance_flag.{}", element.name()),
                self.resistance_flags[index] as i64,
            ));
        }
        for state in ChanceState::ALL {
            out.push((
                format!("chance.{}", state.name()),
                self.chance_states[state as usize] as i64,
            ));
        }
        out.push(("max_magicka_mod".to_string(), self.max_magicka_mod as i64));
        out.push(("max_health_limiter".to_string(), self.max_health_limiter as i64));
        out.push(("armor_value_mod".to_string(), self.armor_value_mod as i64));
        out.push(("weight_allowance".to_string(), self.weight_allowance as i64));
        out.push(("chance_to_hit_mod".to_string(), self.chance_to_hit_mod as i64));
        for group in EnemyGroup::ALL {
            out.push((format!("versus.{}", group.name()), self.versus[group as usize] as i64));
        }
        for group in SocialGroup::GROUPS {
            let index = SocialGroup::GROUPS
                .iter()
                .position(|candidate| *candidate == group)
                .unwrap_or(0);
            out.push((
                format!("reaction.{}", group.name()),
                self.reaction_mods[index] as i64,
            ));
        }
        for (name, value) in self.flags.named() {
            out.push((name.to_string(), value as i64));
        }
        for flag in ConcealmentFlag::ALL {
            out.push((
                format!("concealment.{}", flag.name()),
                self.concealed(flag) as i64,
            ));
        }
        out.push(("shield".to_string(), self.shield as i64));
        out.push(("disease".to_string(), self.diseases.len() as i64));
        out.push(("poison".to_string(), self.poisons.len() as i64));
        out.push(("item.weight_percent".to_string(), self.item.weight_percent as i64));
        out.push(("item.condition".to_string(), self.item.condition as i64));
        out.push((
            "item.max_condition".to_string(),
            (self.item.max_condition + self.item.max_condition_mod) as i64,
        ));
        out.push(("item.soul_bound".to_string(), self.item.soul_bound as i64));
        observe_requests(self, &mut out);
        out
    }
}

/// Injected randomness of the effect layer. Magnitudes use
/// `UnityEngine.Random.Range(min, max + 1)` (`EntityEffect.cs:795-797`, i.e.
/// **inclusive**) and chances use `Dice100.SuccessRoll` (`EntityEffect.cs:744`).
pub trait EffectDice {
    /// `Range(low..=high)` (`EntityEffect.cs:795-797`).
    fn range_inclusive(&mut self, low: i32, high: i32) -> i32;
    /// `Dice100` roll, `0..=99` (`Dice100.cs:15-18`).
    fn d100(&mut self) -> i32;
}

/// Deterministic DFU LCG for tests and probes
/// (`DFRandom.cs:52-56`: `next = next·1103515245 + 12345`,
/// `rand() = (next >> 16) & 0x7FFF`). Stateful but never shared: the caller
/// owns the generator (the crate keeps 0 global RNG).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MagicDice {
    next: u64,
}

impl MagicDice {
    /// Seeds the generator (`DFRandom.cs:47-50`).
    pub fn with_seed(seed: u64) -> Self {
        Self { next: seed }
    }

    /// `DFRandom.rand()` (`DFRandom.cs:52-56`).
    fn rand(&mut self) -> i32 {
        self.next = self.next.wrapping_mul(1_103_515_245).wrapping_add(12_345);
        ((self.next >> 16) & 0x7FFF) as i32
    }
}

impl EffectDice for MagicDice {
    fn range_inclusive(&mut self, low: i32, high: i32) -> i32 {
        if high < low {
            return low;
        }
        low + self.rand() % (high - low + 1)
    }

    fn d100(&mut self) -> i32 {
        self.rand() % 100
    }
}

/// `GetMagnitude` (`EntityEffect.cs:786-800`):
/// `base = Range(min, max + 1)`, `plus = Range(plusMin, plusMax + 1)`,
/// `magnitude = base + plus · ⌊level / perLevel⌋`. The multiplier is **not**
/// clamped to 1 (unlike duration, `EntityEffect.cs:923-928`).
pub fn roll_magnitude(
    settings: &EffectSettings,
    support_magnitude: bool,
    level: i32,
    dice: &mut impl EffectDice,
) -> i32 {
    if !support_magnitude {
        return 0;
    }
    let base = dice.range_inclusive(settings.magnitude_base_min, settings.magnitude_base_max);
    let plus = dice.range_inclusive(settings.magnitude_plus_min, settings.magnitude_plus_max);
    // `classic_effect_settings` clamps perLevel to ≥ 1 (`:562`); a hand-built
    // settings struct with 0 would divide by zero in C# — 0 multiplier here.
    let multiplier = if settings.magnitude_per_level > 0 {
        level / settings.magnitude_per_level
    } else {
        0
    };
    base + plus * multiplier
}

/// One application of an effect instance (cast round or magic round).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EffectApplication {
    /// Template key.
    pub key: &'static str,
    /// Applied payload (the template's declared behaviour).
    pub payload: &'static [Payload],
    /// Rounds from `SetDuration` (0 = instant, does not persist).
    pub scheduled_rounds: i32,
    /// `ChanceValue()` of the instance (0 when `SupportChance` is false).
    pub chance_value: i32,
    /// Whether the effect ran: `false` only for an `OnCast` chance failure,
    /// in which case DFU drops the effect from the bundle
    /// (`EntityEffectManager.cs:541-555`).
    pub chance_passed: bool,
    /// Magnitude used by this application (0 when unsupported).
    pub magnitude: i32,
    /// Every mutation this application performed.
    pub changes: Vec<EffectChange>,
}

/// Result of [`MagicState::cast_effects`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CastReport {
    /// `SetDuration` rounds per entry (0 = instant), in entry order.
    pub scheduled: Vec<i32>,
    /// One row per entry, whether it ran or was dropped by chance.
    pub applications: Vec<EffectApplication>,
    /// Aggregate duration-state changes (flags, mods, resistances).
    pub state_changes: Vec<EffectChange>,
    /// Classic keys of the slots that were **not** silently skipped: the key
    /// had no registered template (the 131 missing DFU effects, §6 N-1).
    pub unresolved: Vec<i32>,
}

/// Result of [`MagicState::magic_round_with`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RoundReport {
    /// Applications performed this round (one per still-running effect).
    pub applications: Vec<EffectApplication>,
    /// Keys whose `End()` ran this round (their state was reverted).
    pub expired: Vec<&'static str>,
    /// Aggregate duration-state changes of this round.
    pub state_changes: Vec<EffectChange>,
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
    /// Payload of the template — what its `Start`/`MagicRound`/`End` executes,
    /// in execution order. Composed from [`Payload`]; empty only for the
    /// registry-only templates declared in the contract §1.3.
    pub payload: &'static [Payload],
    /// `properties.ChanceFunction` (default `OnCast`,
    /// `MagicAndEffectsEnums.cs:107-111`).
    pub chance_function: ChanceFunction,
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

impl EffectProperties {
    /// Mandatory fields; every support flag starts `false`, every cost `None`,
    /// `variant_count = 1` and `chance_function = OnCast` — exactly the
    /// `EntityEffectProperties` defaults (`MagicAndEffectsStructs.cs:44-64`,
    /// `EntityEffect.cs:299-412`). A template only declares what DFU declares.
    pub const fn new(
        key: &'static str,
        classic_key: i32,
        magic_skill: MagicSkill,
        payload: &'static [Payload],
    ) -> Self {
        Self {
            key,
            classic_key,
            magic_skill,
            payload,
            chance_function: ChanceFunction::OnCast,
            support_duration: false,
            support_chance: false,
            support_magnitude: false,
            duration_costs: None,
            chance_costs: None,
            magnitude_costs: None,
            variant_count: 1,
        }
    }

    /// `SupportDuration = true` plus its declared costs.
    pub const fn duration(mut self, costs: EffectCosts) -> Self {
        self.support_duration = true;
        self.duration_costs = Some(costs);
        self
    }

    /// `SupportChance = true` plus its declared costs.
    pub const fn chance(mut self, costs: EffectCosts) -> Self {
        self.support_chance = true;
        self.chance_costs = Some(costs);
        self
    }

    /// `SupportMagnitude = true` plus its declared costs.
    pub const fn magnitude(mut self, costs: EffectCosts) -> Self {
        self.support_magnitude = true;
        self.magnitude_costs = Some(costs);
        self
    }

    /// `variantCount` (`EntityEffect.cs:412`).
    pub const fn variants(mut self, count: i32) -> Self {
        self.variant_count = count;
        self
    }

    /// `properties.ChanceFunction` (`MagicAndEffectsEnums.cs:107-111`).
    pub const fn chance_fn(mut self, function: ChanceFunction) -> Self {
        self.chance_function = function;
        self
    }

    /// Strongest lifecycle class implied by the payload (contract §2.1).
    pub const fn lifetime(&self) -> Lifetime {
        payload_lifetime(self.payload)
    }

    /// True when any payload rolls a fresh magnitude each magic round
    /// (`DamageHealth.cs:53`, `HealHealth.cs:82`, `Regenerate.cs`).
    pub const fn rolls_magnitude_per_round(&self) -> bool {
        payload_rolls_per_round(self.payload)
    }

    /// True when the template leaves duration state behind (flags, modifiers,
    /// concealment, chance states) that `End()` must revert.
    pub const fn has_duration_state(&self) -> bool {
        (self.lifetime() as u8) >= (Lifetime::Duration as u8)
    }

    /// True when the template carries at least one duration state payload.
    pub const fn carries_state(&self) -> bool {
        payload_carries_state(self.payload)
    }

    /// True when the cast (`Start`) itself touches the target: an instant
    /// payload, duration state or an item trigger. A template whose payloads
    /// are **all** per-round (`ContinuousDamage-*`, `:56`) applies nothing at
    /// `Start`, so the cast round is not one of its applications and it starts
    /// with all `scheduled` rounds ticked by `MagicRound`.
    pub const fn has_start_payload(&self) -> bool {
        payload_has_start(self.payload)
    }
}

/// Strongest lifecycle class of a payload set ([`Payload::lifetime`],
/// contract §2.1) — the slice form the narrowed leaf drives classify with.
const fn payload_lifetime(payloads: &[Payload]) -> Lifetime {
    let mut strongest = Lifetime::Instant;
    let mut index = 0;
    while index < payloads.len() {
        let candidate = payloads[index].lifetime();
        if candidate as u8 > strongest as u8 {
            strongest = candidate;
        }
        index += 1;
    }
    strongest
}

/// Whether any payload of the set rolls a fresh magnitude each magic round.
const fn payload_rolls_per_round(payloads: &[Payload]) -> bool {
    let mut index = 0;
    while index < payloads.len() {
        if payloads[index].rolls_magnitude_per_round() {
            return true;
        }
        index += 1;
    }
    false
}

/// Whether the set leaves duration state behind that `End()` must revert.
const fn payload_carries_state(payloads: &[Payload]) -> bool {
    let mut index = 0;
    while index < payloads.len() {
        if payloads[index].carries_state() {
            return true;
        }
        index += 1;
    }
    false
}

/// Whether the set touches the target at `Start` (anything but a per-round
/// payload, `ContinuousDamage-*.cs:56`).
const fn payload_has_start(payloads: &[Payload]) -> bool {
    let mut index = 0;
    while index < payloads.len() {
        if !matches!(payloads[index], Payload::ResourcePerRound { .. }) {
            return true;
        }
        index += 1;
    }
    false
}

// ── registry: the 153 DFU `Effects/` templates (contract §1) ───────────────

pub mod effects;

use effects::{
    ALTERATION, DESTRUCTION, DISEASES, ENCHANTING, ILLUSION, MYSTICISM, POISONS, RESTORATION,
    SPECIAL, THAUMATURGY,
};

/// Every family table, in family order. [`templates`] is the single registry
/// traversal, so a key added to a family is registered and probe-visible with
/// no second list to keep in sync.
pub const FAMILIES: [&[EffectProperties]; 10] = [
    ALTERATION,
    DESTRUCTION,
    DISEASES,
    ENCHANTING,
    ILLUSION,
    MYSTICISM,
    POISONS,
    RESTORATION,
    SPECIAL,
    THAUMATURGY,
];

/// Registered keys (165: 143 concrete templates + 22 expanded variants of
/// `ElementalResistance`, `MageLight`, `Pacify` and `Poison`). The contract §1
/// census pairs it with the 153 source files (143 + 10 abstract bases).
pub const TEMPLATE_COUNT: usize = ALTERATION.len()
    + DESTRUCTION.len()
    + DISEASES.len()
    + ENCHANTING.len()
    + ILLUSION.len()
    + MYSTICISM.len()
    + POISONS.len()
    + RESTORATION.len()
    + SPECIAL.len()
    + THAUMATURGY.len();

/// Every registered template, family order then table order.
pub fn templates() -> impl Iterator<Item = &'static EffectProperties> {
    FAMILIES.iter().flat_map(|family| family.iter())
}

/// Templates whose payload leaves duration state (flags, modifiers,
/// concealment, chance states) that `End()` must revert.
pub fn duration_templates() -> impl Iterator<Item = &'static EffectProperties> {
    templates().filter(|template| template.has_duration_state())
}

/// Looks up a template by `Key` (`EntityEffectBroker.GetEffectTemplate(string)`,
/// `:630-637`).
pub fn effect_by_key(key: &str) -> Option<&'static EffectProperties> {
    templates().find(|effect| effect.key == key)
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
    templates().find(|effect| effect.classic_key == key)
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

/// Resolves a bundle's slots to executable entries and **reports** the keys
/// that have no registered template instead of dropping them silently
/// (the 131 DFU effects missing from the 22-entry registry, spec §6 N-1).
///
/// Empty slots (`effect_type == -1`, `classic_effect_key` → `None`) are
/// expected padding and are not reported. The N-5 Free Action patch is the
/// caller's responsibility ([`apply_free_action_patch`]).
pub fn resolve_classic_effects(
    slots: &[ClassicEffect],
    unresolved: &mut Vec<i32>,
) -> Vec<EffectEntry> {
    let mut entries = Vec::with_capacity(slots.len());
    for slot in slots {
        match classic_effect_entry(slot) {
            Some(entry) => entries.push(entry),
            None => {
                if let Some(key) = classic_effect_key(slot) {
                    unresolved.push(key);
                }
            }
        }
    }
    entries
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
    /// The payload set this instance replays on its rounds: `props.payload`,
    /// or one narrowed leaf for [`MagicState::cast_leaf_effect`].
    pub payload: &'static [Payload],
    /// Settings of THIS instance (never shared).
    pub settings: EffectSettings,
    /// Initial rounds (`rounds` of §2).
    pub rounds_start: i32,
    /// Remaining rounds (`RoundsRemaining`, `EntityEffect.cs:339-343`).
    pub rounds_remaining: i32,
    /// Magnitude frozen at cast for the state effects (`FortifyEffect.cs:33`
    /// calls `GetMagnitude` once in `BecomeIncumbent`); 0 otherwise.
    pub applied_magnitude: i32,
    /// `ChanceValue()` frozen at cast (resistance payloads,
    /// `ElementalResistance.cs:206-207`).
    pub applied_chance: i32,
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

    /// **Scheduling only** (M18 demo contract, `m18_demo` §10.2): instantiates
    /// the bundle effects with each entry's settings and returns their initial
    /// rounds (0 = instant, does not persist). It does not touch a target; for
    /// a cast that **applies** its effects use [`Self::cast_effects`], which
    /// additionally runs the initial `MagicRound()` and leaves
    /// `rounds_remaining` in the post-`Start` DFU state (N − 1).
    pub fn cast(&mut self, entries: &[EffectEntry], level: i32) -> Vec<i32> {
        let mut starts = Vec::with_capacity(entries.len());
        for entry in entries {
            let start = rounds(&entry.settings, entry.props.support_duration, level);
            starts.push(start);
            if start > 0 {
                self.active.push(ActiveEffect {
                    props: entry.props,
                    payload: entry.props.payload,
                    settings: entry.settings,
                    rounds_start: start,
                    rounds_remaining: start,
                    applied_magnitude: 0,
                    applied_chance: 0,
                });
            }
        }
        starts
    }

    /// Casts a bundle **and applies it** to `target`.
    ///
    /// Per entry, in DFU order:
    /// 1. `ChanceValue()` and, for `ChanceFunction::OnCast`, one `Dice100`
    ///    roll; a failure drops the entry from the bundle
    ///    (`EntityEffectManager.cs:541-555`) and is reported with
    ///    `chance_passed = false`.
    /// 2. `SetDuration` rounds (`EntityEffect.cs:920-935`).
    /// 3. The initial `MagicRound()` (`EntityEffectManager.cs:594`) applies the
    ///    payload once; `RemoveRound()` leaves `N − 1` rounds
    ///    (`EntityEffect.cs:576-581`), so a duration of N gives exactly N
    ///    applications (`Start` + N − 1 rounds) and `End()` on the next round.
    /// 4. Instant effects (`SupportDuration = false`) are applied once and are
    ///    never added to the active list.
    ///
    /// The duration state (flags, attribute modifiers, resistances) is
    /// re-derived from the live instances at the end, which is also how
    /// re-casting the same effect keeps the state raised until the last
    /// instance ends (declared delta: no `IncumbentEffect` merging).
    pub fn cast_effects(
        &mut self,
        entries: &[EffectEntry],
        level: i32,
        target: &mut EffectTarget,
        dice: &mut impl EffectDice,
    ) -> CastReport {
        let mut report = CastReport::default();
        for entry in entries {
            let (scheduled, application) = self.cast_entry(
                entry.props,
                entry.props.payload,
                entry.settings,
                level,
                target,
                dice,
            );
            report.scheduled.push(scheduled);
            report.applications.push(application);
        }
        report.state_changes = self.derive_state(target);
        report
    }

    /// One entry of [`Self::cast_effects`] with the replayed payload set
    /// narrowed to `payload`: the chance gate, the duration scheduling, the
    /// magnitude freeze, the instance push and the application — the code
    /// both the bundle cast and the per-leaf evidence drive share (contract
    /// §5.4). Returns `(scheduled, application)`.
    fn cast_entry(
        &mut self,
        props: &'static EffectProperties,
        payload: &'static [Payload],
        settings: EffectSettings,
        level: i32,
        target: &mut EffectTarget,
        dice: &mut impl EffectDice,
    ) -> (i32, EffectApplication) {
        let chance = if props.support_chance {
            chance_value(&settings, level)
        } else {
            0
        };
        let chance_passed = match (props.support_chance, props.chance_function) {
            (false, _) => true,
            (true, ChanceFunction::OnCast) => success_roll(dice.d100(), chance),
            // `ElementalResistance.ChanceSuccess => true`
            // (`ElementalResistance.cs:50-54`) and `SpellResistance` rolls
            // its chance per incoming spell (`TryResistance`), so neither
            // is gated at cast.
            (true, ChanceFunction::Custom) => true,
        };
        let scheduled = rounds(&settings, props.support_duration, level);
        // A `Duration` template without `SupportDuration` has no round to
        // count and nothing to expire, yet DFU keeps it applied until the
        // owner removes it (`LycanthropyInfectionEffect`, `VampirismEffect`,
        // `PassiveEffects` — the curses live in the effect manager): its
        // state payloads are applied at `Start` and stay derived.
        let permanent = payload_lifetime(payload) == Lifetime::Permanent
            || (scheduled == 0 && payload_carries_state(payload));
        let mut application = EffectApplication {
            key: props.key,
            payload,
            scheduled_rounds: if permanent { 0 } else { scheduled },
            chance_value: chance,
            chance_passed,
            magnitude: 0,
            changes: Vec::new(),
        };
        if !chance_passed {
            return (0, application);
        }
        // State effects freeze their magnitude at cast (`FortifyEffect.cs:33`);
        // per-round effects roll inside `apply_instance` (`DamageHealth.cs:53`).
        let frozen = if payload_rolls_per_round(payload) {
            None
        } else {
            Some(roll_magnitude(&settings, props.support_magnitude, level, dice))
        };
        // A permanent template (drain, disease, poison, curse) applies once
        // and never expires, so it stays live to keep `derive_state` honest.
        if scheduled > 0 || permanent {
            let magnitude = frozen.unwrap_or(0);
            self.active.push(ActiveEffect {
                props,
                payload,
                settings,
                rounds_start: scheduled,
                rounds_remaining: if permanent {
                    PERMANENT_ROUNDS
                } else if payload_has_start(payload) {
                    // The cast round counts as the first application.
                    scheduled - 1
                } else {
                    // Nothing to apply at `Start`: round 1 is the first tick.
                    scheduled
                },
                applied_magnitude: magnitude,
                applied_chance: chance,
            });
        }
        let before = target.observe();
        application.magnitude =
            apply_instance(payload, props.key, props.support_magnitude, &settings, frozen, level, target, dice);
        application.changes = diff_observations(&before, &target.observe());
        (if permanent { 0 } else { scheduled }, application)
    }

    /// Casts **one payload** of `props` in isolation — the per-payload
    /// evidence drive of `PROBE-EFFECT-001` (contract §5.4): the
    /// template's chance gate and settings roll, the instant application of
    /// `leaf` alone and the derived state of its single instance. Exactly the
    /// code [`Self::cast_effects`] runs per entry, with the replayed payload
    /// set narrowed to the leaf — never a second interpreter.
    pub fn cast_leaf_effect(
        &mut self,
        props: &'static EffectProperties,
        leaf: &'static Payload,
        settings: &EffectSettings,
        level: i32,
        target: &mut EffectTarget,
        dice: &mut impl EffectDice,
    ) -> CastReport {
        let mut report = CastReport::default();
        let (scheduled, application) = self.cast_entry(
            props,
            std::slice::from_ref(leaf),
            *settings,
            level,
            target,
            dice,
        );
        report.scheduled.push(scheduled);
        report.applications.push(application);
        report.state_changes = self.derive_state(target);
        report
    }

    /// DFU `DoMagicRound()` (`EntityEffectManager.cs:1730-1735`): every effect
    /// with `RoundsRemaining > 0` applies its payload and decrements; an effect
    /// that reaches 0 stays one more round and then runs `End()`, which is
    /// where its flags/modifiers are reverted (§2b).
    ///
    /// Unlike [`Self::magic_round`] (the M18 demo's fixed countdown, which
    /// expires on the Nth decrement), this engine matches DFU round for round:
    /// `End()` lands on round N + 1 after a cast whose duration is N minutes.
    pub fn magic_round_with(
        &mut self,
        level: i32,
        target: &mut EffectTarget,
        dice: &mut impl EffectDice,
    ) -> RoundReport {
        let mut report = RoundReport::default();
        let mut kept = Vec::with_capacity(self.active.len());
        for effect in self.active.drain(..) {
            if effect.rounds_remaining > 0 {
                let before = target.observe();
                let magnitude = if payload_rolls_per_round(effect.payload) {
                    Some(roll_magnitude(
                        &effect.settings,
                        effect.props.support_magnitude,
                        level,
                        dice,
                    ))
                } else {
                    Some(effect.applied_magnitude)
                };
                let used = apply_instance(
                    effect.payload,
                    effect.props.key,
                    effect.props.support_magnitude,
                    &effect.settings,
                    magnitude,
                    level,
                    target,
                    dice,
                );
                let changes = diff_observations(&before, &target.observe());
                report.applications.push(EffectApplication {
                    key: effect.props.key,
                    payload: effect.payload,
                    scheduled_rounds: effect.rounds_start,
                    chance_value: effect.applied_chance,
                    chance_passed: true,
                    magnitude: used,
                    changes,
                });
                let mut kept_effect = effect;
                // A permanent effect (drain, disease, poison, curse) ticks every
                // round and never expires.
                if kept_effect.rounds_remaining != PERMANENT_ROUNDS {
                    kept_effect.rounds_remaining -= 1;
                }
                kept.push(kept_effect);
            } else {
                report.expired.push(effect.props.key);
            }
        }
        self.active = kept;
        report.state_changes = self.derive_state(target);
        report
    }

    /// Re-derives the duration state from the live instances and returns the
    /// observable changes. Mirrors what DFU keeps per effect: `statMods`
    /// (`FortifyEffect.cs:33`), `resistanceMods`/resistance flags
    /// (`ElementalResistance.cs:206-207`, `DaggerfallEntity.cs:657-660`) and the
    /// entity flags set by `Start`/`End` (locomotion, breath, concealment).
    /// Recomputing (instead of applying deltas) makes `End()` exact: when the
    /// last instance of a key expires, its contribution disappears.
    fn derive_state(&self, target: &mut EffectTarget) -> Vec<EffectChange> {
        let before = target.observe();
        target.flags = StatusFlags::default();
        target.concealment = 0;
        target.attribute_mods = [0; ATTRIBUTE_COUNT];
        target.attribute_max_mods = [0; ATTRIBUTE_COUNT];
        target.skill_mods = [0; EFFECT_SKILL_COUNT];
        target.resistances = [0; ELEMENT_COUNT];
        target.resistance_flags = [false; ELEMENT_COUNT];
        target.chance_states = [0; 3];
        target.max_magicka_mod = 0;
        target.max_health_limiter = 0;
        target.armor_value_mod = 0;
        target.weight_allowance = 100;
        target.reaction_mods = [0; 5];
        target.chance_to_hit_mod = 0;
        target.versus = [0; 4];
        target.shield = 0;
        for effect in &self.active {
            let magnitude = effect.applied_magnitude;
            let chance = effect.applied_chance;
            for payload in effect.payload {
                accumulate_state(payload, magnitude, chance, target);
            }
        }
        let after = target.observe();
        diff_observations(&before, &after)
    }

    /// One magic round: decrements every round and returns the keys that
    /// **expired** in this round (reached 0).
    pub fn magic_round(&mut self) -> Vec<&'static str> {
        let mut expired = Vec::new();
        let mut kept = Vec::with_capacity(self.active.len());
        for mut effect in self.active.drain(..) {
            // Permanent effects never expire, not even under the M18 countdown.
            if effect.rounds_remaining == PERMANENT_ROUNDS {
                kept.push(effect);
                continue;
            }
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

/// Applies the payload of one effect instance (DFU `MagicRound`):
/// Payload interpreter — what DFU's `Start`/`MagicRound` do: applies every
/// instant payload of `props` to `target`, in declaration order, then returns
/// the magnitude used. Duration payloads (flags, modifiers, concealment,
/// chance states, shield, versus) are owned by [`MagicState::derive_state`];
/// `Payload::On` is owned by [`run_enchantment`] (an item trigger, not a cast).
///
/// `magnitude` is `Some` when it was frozen at cast (state effects) or already
/// rolled (per-round applications); `None` rolls it here from `settings`.
#[allow(clippy::too_many_arguments)] // parent context (key + support flags) + drive (level/target/dice)
fn apply_instance(
    payloads: &'static [Payload],
    key: &'static str,
    support_magnitude: bool,
    settings: &EffectSettings,
    magnitude: Option<i32>,
    level: i32,
    target: &mut EffectTarget,
    dice: &mut impl EffectDice,
) -> i32 {
    let value = match magnitude {
        Some(value) => value,
        None => roll_magnitude(settings, support_magnitude, level, dice),
    };
    for payload in payloads {
        apply_payload(payload, key, value, target, dice);
    }
    value
}

/// One instant payload. `key` is the template key, recorded when the payload
/// registers a disease/poison so `Cure-*` and the ticks can find its data.
fn apply_payload(
    payload: &Payload,
    key: &'static str,
    value: i32,
    target: &mut EffectTarget,
    dice: &mut impl EffectDice,
) {
    match payload {
        Payload::Resource { resource, sign } => adjust_resource(target, *resource, *sign, value),
        Payload::FixedResource {
            resource,
            sign,
            amount,
        } => adjust_resource(target, *resource, *sign, *amount),
        // `ItemDeteriorates.cs:72-104`: the item loses condition, floored at 0
        // and never above the (modified) maximum.
        Payload::ItemCondition { delta } => {
            let ceiling = (target.item.max_condition + target.item.max_condition_mod).max(0);
            target.item.condition = (target.item.condition + delta).clamp(0, ceiling);
        }
        Payload::ResourcePerRound { resource, sign } => {
            adjust_resource(target, *resource, *sign, value);
        }
        // The amount is the trigger's own event argument, applied by
        // [`run_enchantment`]; a spell cast carries none.
        Payload::TriggerAmount { .. } => {}
        // `HealEffect.cs:19`: heals `DrainEffect` incumbents only. Disease loss
        // is a separate counter owned by M25, so it is never touched here.
        Payload::HealAttribute(attribute) => {
            let damage = &mut target.attribute_damage[*attribute as usize];
            *damage = (*damage - value).max(0);
        }
        // The heal half of a `TransferEffect` lands on the **caster**
        // (`TransferEffect.cs:41`). The neutral target is the victim and models
        // no second entity, so applying it here would silently cancel the drain
        // this same instance just applied. Declared residual: a two-entity cast
        // is needed to observe the caster half (M25 owns entities).
        Payload::HealCaster(_) => {}
        // `DrainEffect.cs:117-137`: permanent damage whose **effective** value
        // is clamped to `base − 1`.
        Payload::DrainAttribute(attribute) => {
            let index = *attribute as usize;
            let ceiling = (target.attributes[index] - 1).max(0);
            let damage = &mut target.attribute_damage[index];
            *damage = (*damage + value).min(ceiling);
        }
        // The drained half belongs to the *other* entity; in the neutral
        // single-target model the target is the caster, so only the heal half
        // is observable here (declared residual: needs a two-entity cast).
        Payload::TransferResource(resource) => adjust_resource(target, *resource, 1, value),
        Payload::Cure(cure) => match cure {
            Cure::Disease => target.diseases.clear(),
            Cure::Poison => target.poisons.clear(),
            Cure::Paralyzation => target.flags.set(Flag::Paralyzed, false),
        },
        Payload::Request(request) => target.requests.push(*request),
        Payload::GatedDamage { gate, amount } => {
            if target.gates.holds(*gate) {
                adjust_resource(target, Resource::Health, -1, *amount);
            }
        }
        Payload::Disease(_) => {
            if !target.diseases.contains(&key) {
                target.diseases.push(key);
            }
        }
        Payload::Poison(_) => {
            if !target.poisons.contains(&key) {
                target.poisons.push(key);
            }
        }
        // Duration state and item triggers: not this path.
        Payload::FortifyAttribute(_)
        | Payload::MaxAttribute(_, _)
        | Payload::Skill(_, _)
        | Payload::Resistance(_, _)
        | Payload::MaxMagicka(_)
        | Payload::ArmorValue(_)
        | Payload::WeightAllowance(_)
        | Payload::Reaction(_, _)
        | Payload::ChanceToHit(_)
        | Payload::HealthLimiter(_)
        | Payload::Flag(_)
        | Payload::Concealment(_)
        | Payload::ChanceState(_)
        | Payload::Shield
        | Payload::Versus { .. }
        | Payload::On { .. }
        | Payload::Bearer(_) => {}
    }
    let _ = dice;
}

/// Applies a **duration-state** payload to `target` and reports whether the
/// payload was one. State is derived, not accumulated: [`MagicState::derive_state`]
/// resets the fields and replays the active effects, while [`run_enchantment`]
/// applies a constant effect directly (`ConstantEffect` sets the entity flag
/// until the item is unequipped).
fn accumulate_state(
    payload: &Payload,
    magnitude: i32,
    chance: i32,
    target: &mut EffectTarget,
) -> bool {
    match payload {
        Payload::FortifyAttribute(attribute) => {
            target.attribute_mods[*attribute as usize] += magnitude;
        }
        Payload::MaxAttribute(attribute, amount) => {
            let base = target.attributes[*attribute as usize];
            target.attribute_max_mods[*attribute as usize] +=
                resolve_amount(*amount, magnitude, base);
        }
        Payload::Skill(skill, amount) => {
            let base = target.skills[*skill as usize];
            target.skill_mods[*skill as usize] += resolve_amount(*amount, magnitude, base);
        }
        Payload::Resistance(element, amount) => {
            let base = target.resistances[*element as usize];
            target.resistances[*element as usize] += resolve_amount(*amount, chance, base);
            target.resistance_flags[*element as usize] = true;
        }
        Payload::MaxMagicka(amount) => {
            target.max_magicka_mod += resolve_amount(*amount, magnitude, target.max_spell_points);
        }
        Payload::ArmorValue(delta) => target.armor_value_mod += *delta,
        Payload::WeightAllowance(percent) => target.weight_allowance = *percent,
        Payload::Reaction(group, amount) => {
            for candidate in SocialGroup::GROUPS {
                if *group == SocialGroup::All || *group == candidate {
                    let index = candidate as usize;
                    target.reaction_mods[index] += resolve_amount(*amount, magnitude, 0);
                }
            }
        }
        Payload::ChanceToHit(amount) => {
            target.chance_to_hit_mod += resolve_amount(*amount, magnitude, 0);
        }
        Payload::HealthLimiter(amount) => {
            let limit = resolve_amount(*amount, magnitude, target.max_health);
            target.max_health_limiter = target.max_health_limiter.max(limit);
        }
        Payload::Flag(flag) => target.flags.set(*flag, true),
        Payload::Concealment(flag) => target.concealment |= 1 << *flag as u8,
        // `ElementEffectManager.TryResistance` keeps the strongest chance; the
        // `ChanceValue()` is the one rolled at cast.
        Payload::ChanceState(state) => {
            let index = *state as usize;
            target.chance_states[index] = target.chance_states[index].max(chance);
        }
        // `Shield.cs:55-63`: the pool is the frozen magnitude.
        Payload::Shield => target.shield = target.shield.max(magnitude),
        Payload::Versus { group, amount } => {
            target.versus[*group as usize] += *amount;
        }
        _ => return false,
    }
    true
}

/// `SetHealth/SetFatigue/SetMagicka` clamping (`DaggerfallEntity.cs:331-338`,
/// `:340-348`, `:379-387`). Fatigue is stored in 1/64 units, so every write
/// multiplies by [`FATIGUE_MULTIPLIER`].
fn adjust_resource(target: &mut EffectTarget, resource: Resource, sign: i8, value: i32) {
    let delta = sign as i32 * value;
    match resource {
        Resource::Health => {
            let ceiling = target.effective_max_health();
            target.health = (target.health + delta).clamp(0, ceiling);
        }
        Resource::Fatigue => {
            let ceiling = target.max_fatigue();
            target.fatigue = (target.fatigue + delta * FATIGUE_MULTIPLIER).clamp(0, ceiling);
        }
        Resource::SpellPoints => {
            let ceiling = target.effective_max_spell_points();
            target.spell_points = (target.spell_points + delta).clamp(0, ceiling);
        }
    }
}

/// Resource quantity in the unit the payloads speak: fatigue is stored in
/// 1/64 units (`DaggerfallEntity.cs:340-348`), health and spell points as is.
fn resource_amount(target: &EffectTarget, resource: Resource) -> i32 {
    match resource {
        Resource::Health => target.health,
        Resource::Fatigue => target.fatigue / FATIGUE_MULTIPLIER,
        Resource::SpellPoints => target.spell_points,
    }
}

/// Takes up to `value` units of `resource` from `target` and returns what was
/// actually taken — the drain half of `TransferEffect` (`TransferEffect.cs:109-137`),
/// bounded by what the victim still has.
fn drain_resource(target: &mut EffectTarget, resource: Resource, value: i32) -> i32 {
    let before = resource_amount(target, resource);
    adjust_resource(target, resource, -1, value);
    before - resource_amount(target, resource)
}

/// Numeric value of a duration payload: `Rolled` is the frozen `GetMagnitude`,
/// `Fixed` the template constant and `Percent` a percentage of `base`
/// (`PassiveSpecialsEffect.cs`'s `RawMaxMagicka * -0.33f`).
fn resolve_amount(amount: Amount, magnitude: i32, base: i32) -> i32 {
    match amount {
        Amount::Rolled => magnitude,
        Amount::Fixed(value) => value,
        Amount::Percent(percent) => base * percent / 100,
    }
}

/// Outcome of one item trigger, split by the entity it changed
/// (`EnchantmentPayloadCallback`'s `sourceEntity` / `targetEntity`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EnchantmentReport {
    /// Changes to the bearer (its resources and its worn item).
    pub bearer: Vec<EffectChange>,
    /// Changes to the entity the enchantment struck.
    pub struck: Vec<EffectChange>,
}

impl EnchantmentReport {
    /// Every change, bearer first — the flat view a single-entity caller wants.
    pub fn changes(&self) -> impl Iterator<Item = &EffectChange> {
        self.bearer.iter().chain(self.struck.iter())
    }

    /// Whether the trigger changed anything at all.
    pub fn is_empty(&self) -> bool {
        self.bearer.is_empty() && self.struck.is_empty()
    }
}

/// Runs the enchantment payloads of `props` whose trigger is `trigger`
/// (`EnchantmentPayloadCallback`), applying them in declaration order. An
/// enchantment is item-driven: the owner organ decides *when* the trigger
/// fires, this function decides *what* it does (M23 does not own items).
/// `event_amount` is the trigger's own argument (`sourceDamage` for
/// `Strikes`, `VampiricEffect.cs:112`; 0 when the trigger carries none).
///
/// Two entities: `bearer` owns the item and receives the bearer-scoped
/// payloads, `struck` receives the rest — the same split DFU makes between
/// `sourceEntity` and `targetEntity`. On a weapon `Strikes`, `bearer` is the
/// attacker and `struck` the defender (`WeaponManager.cs:618`,
/// `EnemyAttack.cs:267,382`); for the out-of-band `Struck` callback
/// (`FormulaHelper.cs:704-719`) the bearer is the wearer and `struck` is the
/// attacker.
pub fn run_enchantment(
    props: &EffectProperties,
    trigger: Trigger,
    event_amount: i32,
    bearer: &mut EffectTarget,
    struck: &mut EffectTarget,
    dice: &mut impl EffectDice,
) -> EnchantmentReport {
    let bearer_before = bearer.observe();
    let struck_before = struck.observe();
    for payload in props.payload {
        if let Payload::On {
            trigger: declared,
            action,
        } = payload
        {
            if *declared != trigger {
                continue;
            }
            // An item enchantment carries no spellmaker settings: its magnitude
            // is `EnchantmentParam.Value` — the worn item's `magnitude` — not a
            // roll (`MaceOfMolagBalEffect.cs:99` `param.Value`, `SoulBound.cs:100`).
            let magnitude = bearer.item.magnitude;
            for inner in *action {
                apply_enchantment_leaf(
                    props.key,
                    inner,
                    trigger,
                    event_amount,
                    magnitude,
                    bearer,
                    struck,
                    dice,
                );
            }
        }
    }
    EnchantmentReport {
        bearer: diff_observations(&bearer_before, &bearer.observe()),
        struck: diff_observations(&struck_before, &struck.observe()),
    }
}

/// Runs ONE enchantment payload as `EnchantmentPayloadCallback` would — the
/// per-leaf routing of [`run_enchantment`] (probe evidence, contract
/// §5.4/§5.7). `through_bearer` is the `Bearer(..)` scope, and
/// `magnitude` the worn item's `EnchantmentParam.Value`. Returns the changes
/// this one leaf produced on each entity.
#[allow(clippy::too_many_arguments)] // the routing context of `run_enchantment`, one leaf at a time
pub fn run_enchantment_leaf(
    key: &'static str,
    leaf: &Payload,
    through_bearer: bool,
    trigger: Trigger,
    event_amount: i32,
    magnitude: i32,
    bearer: &mut EffectTarget,
    struck: &mut EffectTarget,
    dice: &mut impl EffectDice,
) -> EnchantmentReport {
    let bearer_before = bearer.observe();
    let struck_before = struck.observe();
    if through_bearer {
        apply_bearer_leaf(key, leaf, event_amount, magnitude, bearer, dice);
    } else {
        apply_enchantment_leaf(key, leaf, trigger, event_amount, magnitude, bearer, struck, dice);
    }
    EnchantmentReport {
        bearer: diff_observations(&bearer_before, &bearer.observe()),
        struck: diff_observations(&struck_before, &struck.observe()),
    }
}

/// One `action` payload of a fired `On{trigger, ..}`: the routing
/// [`run_enchantment`] applies per leaf.
#[allow(clippy::too_many_arguments)] // the routing context of `run_enchantment`, one leaf at a time
fn apply_enchantment_leaf(
    key: &'static str,
    leaf: &Payload,
    trigger: Trigger,
    event_amount: i32,
    magnitude: i32,
    bearer: &mut EffectTarget,
    struck: &mut EffectTarget,
    dice: &mut impl EffectDice,
) {
    // `Bearer(..)`: the payloads DFU applies to `sourceEntity`.
    if let Payload::Bearer(bearer_action) = leaf {
        for scoped in *bearer_action {
            apply_bearer_leaf(key, scoped, event_amount, magnitude, bearer, dice);
        }
        return;
    }
    // `DrainEffect.cs:110-125` (attributes) and
    // `MaceOfMolagBalEffect.cs:113-126` (resources): the drain
    // leaves the struck entity, limited to what the victim still
    // had, and the bearer gains exactly that (`TransferEffect.cs:37-43`
    // is the caster half).
    if let Payload::TransferResource(resource) = leaf {
        let taken = drain_resource(struck, *resource, magnitude);
        adjust_resource(bearer, *resource, 1, taken);
        return;
    }
    // `ConstantEffect` sets entity state directly; instant payloads
    // (`CastWhenStrikes`, `CastWhenUsed`, …) run here. DFU only
    // carries a `targetEntity` on `Strikes`/`Struck`
    // (`EnchantmentPayloadCallback`); on every other trigger the
    // payload acts on the bearer. `TriggerAmount`/`HealCaster`/
    // `ItemCondition` name `sourceEntity` even on a strike.
    let entity = if !trigger_carries_target(trigger) || payload_is_bearer_scoped(leaf) {
        &mut *bearer
    } else {
        &mut *struck
    };
    if let Payload::TriggerAmount { resource, sign } = leaf {
        adjust_resource(entity, *resource, *sign, event_amount);
    } else if !accumulate_state(leaf, magnitude, magnitude, entity) {
        apply_payload(leaf, key, magnitude, entity, dice);
    }
}

/// One payload of a `Bearer(..)` block: everything lands on `sourceEntity`
/// (`MaceOfMolagBalEffect.cs:126,138-139`), `TriggerAmount` by the trigger's
/// own argument.
fn apply_bearer_leaf(
    key: &'static str,
    scoped: &Payload,
    event_amount: i32,
    magnitude: i32,
    bearer: &mut EffectTarget,
    dice: &mut impl EffectDice,
) {
    if let Payload::TriggerAmount { resource, sign } = scoped {
        adjust_resource(bearer, *resource, *sign, event_amount);
    } else if !accumulate_state(scoped, magnitude, magnitude, bearer) {
        apply_payload(scoped, key, magnitude, bearer, dice);
    }
}

/// Whether DFU passes a `targetEntity` to the callback of `trigger`: only the
/// two out-of-band strike callbacks do (`WeaponManager.cs:618`,
/// `EnemyAttack.cs:267,382` for `Strikes`; `FormulaHelper.cs:704-719` for
/// `Struck`); `Held`, `Equipped`, `Used`, `Breaks`, `MagicRound`, `Enchanting`
/// and `Reroll` pass `null` and act on the bearer.
fn trigger_carries_target(trigger: Trigger) -> bool {
    matches!(trigger, Trigger::Strikes | Trigger::Struck)
}

/// Payloads whose DFU mutation names `sourceEntity` — the enchantment's bearer:
/// `VampiricEffect.cs:127` (`sourceEntity.CurrentHealth += sourceDamage`),
/// `TransferEffect.cs:41` (`HealCaster`) and the worn item itself
/// (`ItemDeteriorates`, `RepairsObjects`). Everything else acts on the struck
/// entity (`targetEntity`).
fn payload_is_bearer_scoped(payload: &Payload) -> bool {
    matches!(
        payload,
        Payload::TriggerAmount { .. } | Payload::HealCaster(_) | Payload::ItemCondition { .. }
    )
}

/// One day of a registered disease (`DiseaseEffect.IncrementDiseaseEffects`):
/// `damage = Range(minDamage, maxDamage + 1)`, then
/// `ChangeStatMod(attr, −data.attr · damage)` for the eight attributes and
/// `DecreaseHealth(HEA · damage)`. Permanent diseases (`days_max == 0xFF`) are
/// day-agnostic, so no duration bookkeeping is consumed here.
pub fn apply_disease_day(
    key: &str,
    target: &mut EffectTarget,
    dice: &mut impl EffectDice,
) -> Vec<EffectChange> {
    let before = target.observe();
    let Some(data) = disease_data(key) else {
        return Vec::new();
    };
    let damage = dice.range_inclusive(data.min_damage as i32, data.max_damage as i32);
    let rows = [
        (Attribute::Strength, data.strength),
        (Attribute::Intelligence, data.intelligence),
        (Attribute::Willpower, data.willpower),
        (Attribute::Agility, data.agility),
        (Attribute::Endurance, data.endurance),
        (Attribute::Personality, data.personality),
        (Attribute::Speed, data.speed),
        (Attribute::Luck, data.luck),
    ];
    for (attribute, amount) in rows {
        let index = attribute as usize;
        let ceiling = (target.attributes[index] - 1).max(0);
        let damage_now = &mut target.attribute_damage[index];
        *damage_now = (*damage_now + amount as i32 * damage).min(ceiling);
    }
    adjust_resource(target, Resource::Health, -1, data.health as i32 * damage);
    adjust_resource(
        target,
        Resource::Fatigue,
        -1,
        data.fatigue as i32 * damage / FATIGUE_MULTIPLIER.max(1),
    );
    adjust_resource(
        target,
        Resource::SpellPoints,
        -1,
        data.spell_points as i32 * damage,
    );
    let after = target.observe();
    diff_observations(&before, &after)
}

/// One minute of a registered poison (`PoisonEffect.IncrementPoisonEffects`,
/// `PoisonEffect.cs:247-311`): every `PoisonTick` row rolls its own range.
/// Incubation and the per-instance minute counters (`PoisonStates`) belong to
/// the clock owner (M25); this function is one tick of an established poison.
pub fn apply_poison_minute(
    key: &str,
    target: &mut EffectTarget,
    dice: &mut impl EffectDice,
) -> Vec<EffectChange> {
    let before = target.observe();
    let Some(spec) = poison_spec(key) else {
        return Vec::new();
    };
    for (attribute, min, max, sign) in spec.tick.attributes {
        let amount = dice.range_inclusive(*min, *max) * *sign as i32;
        let index = *attribute as usize;
        let ceiling = (target.attributes[index] - 1).max(0);
        let damage_now = &mut target.attribute_damage[index];
        *damage_now = (*damage_now + amount).min(ceiling).max(0);
    }
    for (resource, min, max, sign, scale) in spec.tick.resources {
        let amount = dice.range_inclusive(*min, *max) * *sign as i32;
        let delta = amount * *scale;
        match resource {
            Resource::Health => {
                let ceiling = target.effective_max_health();
                target.health = (target.health + delta).clamp(0, ceiling);
            }
            Resource::Fatigue => {
                let ceiling = target.max_fatigue();
                target.fatigue = (target.fatigue + delta).clamp(0, ceiling);
            }
            Resource::SpellPoints => {
                let ceiling = target.effective_max_spell_points();
                target.spell_points = (target.spell_points + delta).clamp(0, ceiling);
            }
        }
    }
    let after = target.observe();
    diff_observations(&before, &after)
}

/// `DiseaseData` of a `Diseases/` template key.
pub fn disease_data(key: &str) -> Option<&'static DiseaseData> {
    match effect_by_key(key)?.payload.first()? {
        Payload::Disease(data) => Some(data),
        _ => None,
    }
}

/// `PoisonSpec` of a `Poison-*` template key.
pub fn poison_spec(key: &str) -> Option<&'static PoisonSpec> {
    match effect_by_key(key)?.payload.first()? {
        Payload::Poison(spec) => Some(spec),
        _ => None,
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

// Goldens of the spec `m23-magic-spec.md` §2.1/§3.3 (families
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
            payload: &[Payload::Resource { resource: Resource::Health, sign: -1 }],
            chance_function: ChanceFunction::OnCast,
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
            payload: &[Payload::Resource { resource: Resource::Health, sign: -1 }],
            chance_function: ChanceFunction::OnCast,
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
    fn registry_registers_every_template_with_unique_keys_and_classic_keys() {
        // `TEMPLATE_COUNT` is the compiled census of the family tables, and
        // `templates()` is the only registry traversal, so the two cannot
        // disagree.
        assert_eq!(templates().count(), TEMPLATE_COUNT);
        const { assert!(TEMPLATE_COUNT > 133, "the 8 landed families alone are 133 rows") };

        let mut names: Vec<&str> = templates().map(|e| e.key).collect();
        names.sort_unstable();
        let rows = names.len();
        names.dedup();
        assert_eq!(names.len(), rows, "no duplicate Key");

        // Classic keys are the SPELLS.STD slots: unique, and 0 (non-spell
        // families: enchantments, curses) never maps one (`MapClassicKey`).
        let mut keys: Vec<i32> = templates()
            .map(|e| e.classic_key)
            .filter(|key| *key != 0)
            .collect();
        keys.sort_unstable();
        let mapped = keys.len();
        keys.dedup();
        assert_eq!(keys.len(), mapped, "no classicKey collision (probe B)");
        assert!(
            templates().any(|e| e.classic_key == 0),
            "non-spell templates declare classic_key 0"
        );
        assert!(templates().all(|e| e.variant_count >= 1));
    }

    #[test]
    fn classic_key_lookup_skips_zero_and_non_spells_family() {
        assert!(effect_by_key("Levitate").is_some());
        assert!(effect_by_key("no-such-effect").is_none());
        let levitate = effect_by_classic_key(classic_key(14, 255)).unwrap();
        assert_eq!(levitate.key, "Levitate");
        // `MapClassicKey`: ClassicKey == 0 is never mapped.
        assert!(effect_by_classic_key(0).is_none());
        // Spells family only (family byte != 0 never maps).
        assert!(effect_by_classic_key(0x01_04_00).is_none());
        // Lock is registered now (M23 §1 supersedes the 17-effect N-1 scope).
        assert_eq!(
            effect_by_classic_key(classic_key(16, 255)).map(|t| t.key),
            Some("Lock")
        );
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
    fn classic_effect_entry_resolves_the_registry_and_skips_empty_slots() {
        let levitate = classic_effect_entry(&slot(14, -1, [1, 1, 1], [1, 1, 1]))
            .expect("Levitate implemented");
        assert_eq!(levitate.props.key, "Levitate");
        assert_eq!(levitate.settings.duration_base, 1);
        // Lock is registered now; M23 §1 supersedes the 17-effect scope.
        let lock = classic_effect_entry(&slot(16, -1, [1, 1, 25], [1, 1, 1]))
            .expect("Lock implemented");
        assert_eq!(lock.props.key, "Lock");
        let unused_slot = ClassicEffect {
            effect_type: -1,
            ..ClassicEffect::default()
        };
        assert!(classic_effect_entry(&unused_slot).is_none());
        // Type 9 has no DFU template (`MagicAndEffectsEnums.cs:116-136`).
        assert!(classic_effect_entry(&slot(9, 9, [1, 1, 1], [1, 1, 1])).is_none());
    }

    #[test]
    fn n5_free_action_patch_rewrites_only_index_10_slot_0() {
        // `ClassicKey(3, 2)` resolves as a real template now, so the patch is
        // asserted on the transform itself, not on a pre-patch absence.
        let mut retail = slot(3, 2, [1, 1, 1], [1, 1, 1]);
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

    // ── Application layer (audit §4 item 10, R1) ───────────────────────────

    /// Dice with scripted answers: `ranges` answers `Range(low..=high)` in
    /// order and `rolls` answers `Dice100` (0 when exhausted), so the
    /// magnitude/chance math is checked against exact values.
    struct ScriptDice {
        ranges: std::collections::VecDeque<i32>,
        rolls: std::collections::VecDeque<i32>,
    }

    impl ScriptDice {
        fn new(ranges: &[i32], rolls: &[i32]) -> Self {
            Self {
                ranges: ranges.iter().copied().collect(),
                rolls: rolls.iter().copied().collect(),
            }
        }
    }

    impl EffectDice for ScriptDice {
        fn range_inclusive(&mut self, _low: i32, _high: i32) -> i32 {
            self.ranges.pop_front().unwrap_or(0)
        }

        fn d100(&mut self) -> i32 {
            self.rolls.pop_front().unwrap_or(0)
        }
    }

    /// Flat 100/100 target with all attributes at 50.
    fn applied_target() -> EffectTarget {
        EffectTarget::new(100, 100, [50; ATTRIBUTE_COUNT])
    }

    fn duration_and_magnitude(
        duration: (i32, i32, i32),
        magnitude: (i32, i32, i32, i32, i32),
    ) -> EffectSettings {
        EffectSettings {
            duration_base: duration.0,
            duration_plus: duration.1,
            duration_per_level: duration.2,
            magnitude_base_min: magnitude.0,
            magnitude_base_max: magnitude.1,
            magnitude_plus_min: magnitude.2,
            magnitude_plus_max: magnitude.3,
            magnitude_per_level: magnitude.4,
            ..EffectSettings::classic_default()
        }
    }

    fn chance_only(chance: (i32, i32, i32)) -> EffectSettings {
        EffectSettings {
            chance_base: chance.0,
            chance_plus: chance.1,
            chance_per_level: chance.2,
            ..EffectSettings::classic_default()
        }
    }

    #[test]
    fn damage_health_subtracts_the_rolled_magnitude_and_clamps_at_zero() {
        // GetMagnitude: 10 + 5·⌊1/1⌋ = 15 (`EntityEffect.cs:786-800`).
        let mut target = applied_target();
        let mut dice = ScriptDice::new(&[10, 5], &[]);
        let mut state = MagicState::default();
        let report = state.cast_effects(
            &[entry_of("Damage-Health", mag(10, 10, 5, 5, 1))],
            1,
            &mut target,
            &mut dice,
        );
        assert_eq!(report.scheduled, vec![0], "instant: not scheduled");
        assert_eq!(report.applications[0].magnitude, 15);
        assert_eq!(target.health, 85);
        assert_eq!(
            report.applications[0].changes,
            vec![EffectChange::new("health", 100, 85)]
        );

        let mut target = applied_target();
        let mut dice = ScriptDice::new(&[200, 0], &[]);
        let mut state = MagicState::default();
        state.cast_effects(
            &[entry_of("Damage-Health", mag(200, 200, 0, 0, 1))],
            1,
            &mut target,
            &mut dice,
        );
        assert_eq!(target.health, 0, "SetHealth clamps at 0 (DaggerfallEntity.cs:331-338)");
    }

    #[test]
    fn heal_health_adds_the_magnitude_and_clamps_at_max() {
        let mut target = applied_target();
        target.health = 40;
        let mut dice = ScriptDice::new(&[10, 5], &[]);
        let mut state = MagicState::default();
        state.cast_effects(
            &[entry_of("Heal-Health", mag(10, 10, 5, 5, 1))],
            1,
            &mut target,
            &mut dice,
        );
        assert_eq!(target.health, 55);

        let mut target = applied_target();
        target.health = 95;
        let mut dice = ScriptDice::new(&[10, 5], &[]);
        let mut state = MagicState::default();
        state.cast_effects(
            &[entry_of("Heal-Health", mag(10, 10, 5, 5, 1))],
            1,
            &mut target,
            &mut dice,
        );
        assert_eq!(target.health, 100, "IncreaseHealth clamps at MaxHealth");
    }

    #[test]
    fn heal_fatigue_multiplies_by_64_and_clamps() {
        // `IncreaseFatigue(magnitude, true)` ⇒ ×64 (`DaggerfallEntity.cs:340-348`).
        let mut target = applied_target();
        target.fatigue = 1_000;
        let mut dice = ScriptDice::new(&[10, 5], &[]);
        let mut state = MagicState::default();
        state.cast_effects(
            &[entry_of("Heal-Fatigue", mag(10, 10, 5, 5, 1))],
            1,
            &mut target,
            &mut dice,
        );
        assert_eq!(target.max_fatigue(), 6_400, "(50+50)·64");
        assert_eq!(target.fatigue, 1_960, "1000 + 15·64");

        let mut target = applied_target();
        target.fatigue = 6_390;
        let mut dice = ScriptDice::new(&[10, 5], &[]);
        let mut state = MagicState::default();
        state.cast_effects(
            &[entry_of("Heal-Fatigue", mag(10, 10, 5, 5, 1))],
            1,
            &mut target,
            &mut dice,
        );
        assert_eq!(target.fatigue, 6_400, "clamped to MaxFatigue");
    }

    #[test]
    fn heal_spell_points_adds_and_clamps() {
        let mut target = applied_target();
        target.spell_points = 50;
        let mut dice = ScriptDice::new(&[10, 5], &[]);
        let mut state = MagicState::default();
        state.cast_effects(
            &[entry_of("Heal-SpellPoints", mag(10, 10, 5, 5, 1))],
            1,
            &mut target,
            &mut dice,
        );
        assert_eq!(target.spell_points, 65);

        let mut target = applied_target();
        target.spell_points = 95;
        let mut dice = ScriptDice::new(&[10, 5], &[]);
        let mut state = MagicState::default();
        state.cast_effects(
            &[entry_of("Heal-SpellPoints", mag(10, 10, 5, 5, 1))],
            1,
            &mut target,
            &mut dice,
        );
        assert_eq!(target.spell_points, 100);
    }

    #[test]
    fn fortify_strength_raises_the_modifier_and_expiry_reverts_it() {
        let settings = duration_and_magnitude((1, 1, 1), (10, 10, 5, 5, 1));
        let mut target = applied_target();
        let mut dice = ScriptDice::new(&[10, 5], &[]);
        let mut state = MagicState::default();
        let report = state.cast_effects(&[entry_of("Fortify-Strength", settings)], 1, &mut target, &mut dice);
        assert_eq!(report.scheduled, vec![2]);
        assert_eq!(target.attribute_mods[Attribute::Strength as usize], 15);
        assert_eq!(target.effective_attribute(Attribute::Strength), 65);
        // `MaxFatigue` is `(STR + END) · 64`, so a strength fortify also moves
        // the derived ceiling: both rows are observed, never a delta alone.
        assert!(report
            .state_changes
            .contains(&EffectChange::new("attribute.Strength", 50, 65)));
        assert!(report
            .state_changes
            .contains(&EffectChange::new("attribute_mod.Strength", 0, 15)));
        assert_eq!(state.rounds_left("Fortify-Strength"), 1, "post-Start DFU rounds");

        let first = state.magic_round_with(1, &mut target, &mut dice);
        assert!(first.expired.is_empty(), "round 1 still applies");
        let second = state.magic_round_with(1, &mut target, &mut dice);
        assert_eq!(second.expired, vec!["Fortify-Strength"]);
        assert_eq!(target.attribute_mods[Attribute::Strength as usize], 0);
        assert_eq!(target.effective_attribute(Attribute::Strength), 50);
        assert!(second
            .state_changes
            .contains(&EffectChange::new("attribute.Strength", 65, 50)));
    }

    #[test]
    fn elemental_resistance_uses_the_chance_value_and_clears_on_expiry() {
        // ElementalResistance: chance 10 + 5·⌊1/1⌋ = 15 becomes the resistance
        // (`ElementalResistance.cs:206-207`), duration 1+1/1 = 2 rounds.
        let settings = EffectSettings {
            duration_base: 1,
            duration_plus: 1,
            duration_per_level: 1,
            chance_base: 10,
            chance_plus: 5,
            chance_per_level: 1,
            ..EffectSettings::classic_default()
        };
        let mut target = applied_target();
        let mut dice = ScriptDice::new(&[], &[]);
        let mut state = MagicState::default();
        let report = state.cast_effects(
            &[entry_of("ElementalResistance-Fire", settings)],
            1,
            &mut target,
            &mut dice,
        );
        let application = &report.applications[0];
        assert_eq!(application.chance_value, 15);
        assert!(application.chance_passed, "Custom chance always starts (ElementalResistance.cs:50-54)");
        assert_eq!(target.resistances[Element::Fire as usize], 15);
        assert!(target.resistance_flags[Element::Fire as usize]);
        assert!(!target.resistance_flags[Element::Frost as usize]);

        state.magic_round_with(1, &mut target, &mut dice);
        let end = state.magic_round_with(1, &mut target, &mut dice);
        assert_eq!(end.expired, vec!["ElementalResistance-Fire"]);
        assert_eq!(target.resistances[Element::Fire as usize], 0);
        assert!(!target.resistance_flags[Element::Fire as usize]);
    }

    #[test]
    fn spell_resistance_raises_the_resistance_chance_until_expiry() {
        let settings = EffectSettings {
            duration_base: 1,
            duration_plus: 1,
            duration_per_level: 1,
            chance_base: 20,
            chance_plus: 0,
            chance_per_level: 1,
            ..EffectSettings::classic_default()
        };
        let mut target = applied_target();
        let mut dice = ScriptDice::new(&[], &[]);
        let mut state = MagicState::default();
        state.cast_effects(&[entry_of("SpellResistance", settings)], 1, &mut target, &mut dice);
        // `SpellResistance.cs` raises no `Entity` boolean: the chance value is
        // what `EntityEffectManager.TryResistance` rolls against.
        assert_eq!(target.chance_state(ChanceState::SpellResistance), 20);

        state.magic_round_with(1, &mut target, &mut dice);
        state.magic_round_with(1, &mut target, &mut dice);
        assert_eq!(
            target.chance_state(ChanceState::SpellResistance),
            0,
            "End() drops the raised chance"
        );
    }

    #[test]
    fn levitate_slowfall_water_breathing_and_water_walking_drive_movement_effects_until_expiry() {
        let mut target = applied_target();
        let mut dice = ScriptDice::new(&[], &[]);
        let mut state = MagicState::default();
        let entries = [
            entry_of("Levitate", dur(1, 1, 1)),
            entry_of("Slowfall", dur(1, 1, 1)),
            entry_of("WaterBreathing", dur(1, 1, 1)),
            entry_of("WaterWalking", dur(1, 1, 1)),
        ];
        state.cast_effects(&entries, 1, &mut target, &mut dice);
        assert_eq!(
            target.movement_effects(),
            MovementEffects {
                levitating: true,
                water_breathing: true,
                slow_falling: true,
                water_walking: true,
            }
        );
        // `StartWaterWalking` sets the `Entity` boolean (`WaterWalking.cs:91-98`).
        assert!(target.has_flag(Flag::WaterWalking));

        state.magic_round_with(1, &mut target, &mut dice);
        assert!(target.movement_effects().levitating, "round 1 keeps it");
        assert!(target.movement_effects().water_walking, "round 1 keeps it");
        let end = state.magic_round_with(1, &mut target, &mut dice);
        assert_eq!(end.expired.len(), 4);
        assert_eq!(
            target.movement_effects(),
            MovementEffects::default(),
            "expiry clears every locomotion flag"
        );
        // `StopWaterWalking` clears it (`WaterWalking.cs:101-109`).
        assert!(!target.has_flag(Flag::WaterWalking), "End() reverts the flag");
    }

    #[test]
    fn two_instances_keep_the_flag_until_the_last_one_ends() {
        let mut target = applied_target();
        let mut dice = ScriptDice::new(&[], &[]);
        let mut state = MagicState::default();
        // Instance A lasts N=2, instance B lasts N=3 rounds.
        state.cast_effects(
            &[
                entry_of("Levitate", dur(1, 1, 1)),
                entry_of("Levitate", dur(1, 2, 1)),
            ],
            1,
            &mut target,
            &mut dice,
        );
        assert!(target.has_flag(Flag::Levitating));
        let first = state.magic_round_with(1, &mut target, &mut dice);
        assert!(first.expired.is_empty());
        assert!(target.has_flag(Flag::Levitating));
        let second = state.magic_round_with(1, &mut target, &mut dice);
        assert_eq!(second.expired.len(), 1, "A ended, B is still alive");
        assert!(target.has_flag(Flag::Levitating), "one instance is still alive");
        let third = state.magic_round_with(1, &mut target, &mut dice);
        assert_eq!(third.expired.len(), 1, "B ended");
        assert!(!target.has_flag(Flag::Levitating), "no instance left");
        assert!(state.active().is_empty());
    }

    #[test]
    fn remaining_entity_flags_are_set_and_declared_without_consumer() {
        let mut target = applied_target();
        let mut dice = ScriptDice::new(&[], &[]);
        let mut state = MagicState::default();
        let entries = [
            entry_of("FreeAction", dur(1, 1, 1)),
            entry_of("Shadow-Normal", dur(1, 1, 1)),
            entry_of("Invisibility-Normal", dur(1, 1, 1)),
            entry_of("Chameleon-Normal", dur(1, 1, 1)),
        ];
        let report = state.cast_effects(&entries, 1, &mut target, &mut dice);
        // One `Entity` boolean plus three `MagicalConcealmentFlags` bits.
        let fields: Vec<&str> = report
            .state_changes
            .iter()
            .map(|change| change.field.as_str())
            .collect();
        for expected in [
            "free_action",
            "concealment.shadow_normal",
            "concealment.invisible_normal",
            "concealment.blending_normal",
        ] {
            assert!(fields.contains(&expected), "{expected} not observed");
        }
        assert!(target.has_flag(Flag::FreeAction));
        assert!(target.concealed(ConcealmentFlag::ShadowNormal));
        assert!(target.concealed(ConcealmentFlag::InvisibleNormal));
        assert!(target.concealed(ConcealmentFlag::BlendingNormal));
        assert_eq!(
            target.movement_effects(),
            MovementEffects::default(),
            "none of them is an M19 movement flag"
        );
    }

    #[test]
    fn cure_disease_and_poison_respect_the_on_cast_chance() {
        let mut target = applied_target();
        target.diseases = vec!["Disease-WitchesPox", "Disease-BrainFever"];
        target.poisons = vec!["Poison-Arsenic"];
        // chance 50: roll 49 passes, roll 50 fails (`success_roll`).
        let mut dice = ScriptDice::new(&[], &[49, 50]);
        let mut state = MagicState::default();
        let report = state.cast_effects(
            &[
                entry_of("Cure-Disease", chance_only((50, 0, 1))),
                entry_of("Cure-Poison", chance_only((50, 0, 1))),
            ],
            1,
            &mut target,
            &mut dice,
        );
        assert!(report.applications[0].chance_passed);
        assert!(!report.applications[1].chance_passed);
        assert!(target.diseases.is_empty(), "CureAllDiseases");
        assert_eq!(
            target.poisons.len(),
            1,
            "failed roll: the bundle drops the effect"
        );
        assert!(report.applications[1].changes.is_empty());
    }

    #[test]
    fn teleport_emits_a_declared_request_instead_of_moving_the_world() {
        let mut target = applied_target();
        let mut dice = ScriptDice::new(&[], &[]);
        let mut state = MagicState::default();
        let report = state.cast_effects(
            &[entry_of("Teleport-Effect", EffectSettings::classic_default())],
            1,
            &mut target,
            &mut dice,
        );
        assert_eq!(target.request_count(Request::Teleport), 1);
        assert_eq!(
            report.applications[0].changes,
            vec![EffectChange::new("request.teleport", 0, 1)]
        );
    }

    #[test]
    fn cast_effects_counts_exactly_n_applications_for_a_duration_of_n() {
        // Levitate with 1+1/1 ⇒ N = 2: initial MagicRound + 1 round, End on the
        // next one (`EntityEffectManager.cs:594`, `:1730-1735`).
        let mut target = applied_target();
        let mut dice = ScriptDice::new(&[], &[]);
        let mut state = MagicState::default();
        let report = state.cast_effects(&[entry_of("Levitate", dur(1, 1, 1))], 1, &mut target, &mut dice);
        assert_eq!(report.scheduled, vec![2]);
        assert_eq!(state.rounds_left("Levitate"), 1);
        let first = state.magic_round_with(1, &mut target, &mut dice);
        assert_eq!(first.applications.len(), 1);
        assert!(first.expired.is_empty());
        let end = state.magic_round_with(1, &mut target, &mut dice);
        assert!(end.applications.is_empty());
        assert_eq!(end.expired, vec!["Levitate"]);
    }

    /// Neutral target with room in both directions: half-full pools, a worn
    /// item, pre-existing attribute/skill damage, one disease, one poison and
    /// the requested paralysis. Every drain, heal, cure, wear and repair the
    /// registry can perform is observable on it.
    fn neutral_target(paralyzed: bool) -> EffectTarget {
        let mut target = applied_target();
        target.health = 50;
        target.spell_points = 50;
        target.fatigue = target.max_fatigue() / 2;
        target.attribute_damage = [20; ATTRIBUTE_COUNT];
        target.skill_damage = [20; EFFECT_SKILL_COUNT];
        target.item.max_condition = 100;
        target.item.condition = 50;
        // The worn enchantment's `EnchantmentParam.Value`.
        target.item.magnitude = 5;
        target.diseases = vec!["Disease-Legacy"];
        target.poisons = vec!["Poison-Legacy"];
        target.flags.set(Flag::Paralyzed, paralyzed);
        target
    }

    /// Runs one template through its **own** path — cast, magic round, its
    /// declared item triggers (`Payload::On`) and its declared gates — and
    /// reports whether anything changed. No path is synthesised: an
    /// enchantment only runs what its trigger declares, a gated passive only
    /// once its gate holds.
    fn template_outcome(props: &'static EffectProperties, paralyzed: bool) -> bool {
        let mut target = neutral_target(paralyzed);
        let settings = EffectSettings::classic_default();
        let mut dice = ScriptDice::new(&[1, 1, 1, 1], &[0]);
        let mut state = MagicState::default();
        let report = state.cast_effects(&[EffectEntry { props, settings }], 1, &mut target, &mut dice);
        if !report.applications[0].chance_passed {
            return false;
        }
        if !report.applications[0].changes.is_empty() || !report.state_changes.is_empty() {
            return true;
        }
        let round = state.magic_round_with(1, &mut target, &mut dice);
        if round
            .applications
            .iter()
            .any(|applied| !applied.changes.is_empty())
            || !round.state_changes.is_empty()
        {
            return true;
        }
        for payload in props.payload {
            let action: &[Payload] = match payload {
                Payload::On { action, .. } => action,
                single => std::slice::from_ref(single),
            };
            for inner in action {
                if let Payload::GatedDamage { gate, .. } = inner {
                    target.gates.set(*gate, true);
                }
            }
        }
        for payload in props.payload {
            if let Payload::On { trigger, .. } = payload {
                // A second neutral entity plays the `targetEntity` of the
                // callback: the bearer keeps its own item and pools.
                let mut struck = neutral_target(paralyzed);
                let report = run_enchantment(props, *trigger, 1, &mut target, &mut struck, &mut dice);
                if !report.is_empty() {
                    return true;
                }
            }
        }
        let held = state.cast_effects(&[EffectEntry { props, settings }], 1, &mut target, &mut dice);
        held.applications
            .iter()
            .any(|applied| !applied.changes.is_empty())
            || !held.state_changes.is_empty()
    }

    #[test]
    fn every_template_produces_a_visible_outcome_on_its_own_path() {
        // Contract §5 (no silent template): every registered key must change the
        // neutral target through its own path.
        for props in templates() {
            let visible = template_outcome(props, false) || template_outcome(props, true);
            assert!(
                visible,
                "{} produced no observable outcome (silent template)",
                props.key
            );
        }
    }

    #[test]
    fn unresolved_classic_keys_are_reported_not_skipped() {
        // Type 9 / subtype 9 has no template among the 22 (the 131 missing DFU
        // effects, §6 N-1); an empty slot (`-1`) is expected padding.
        let unimplemented = slot(9, 9, [1, 1, 1], [1, 1, 1]);
        let empty = slot(-1, -1, [0, 0, 0], [0, 0, 0]);
        let mut unresolved = Vec::new();
        let entries = resolve_classic_effects(&[empty, unimplemented], &mut unresolved);
        assert!(entries.is_empty());
        assert_eq!(unresolved, vec![classic_key(9, 9)]);
    }

    #[test]
    fn magic_dice_is_deterministic_and_inclusive() {
        let mut first = MagicDice::with_seed(7);
        let mut second = MagicDice::with_seed(7);
        for _ in 0..8 {
            let a = roll_magnitude(&mag(3, 9, 1, 4, 1), true, 2, &mut first);
            let b = roll_magnitude(&mag(3, 9, 1, 4, 1), true, 2, &mut second);
            assert_eq!(a, b, "same seed ⇒ same magnitude");
            assert!((3..=9 + 4 * 2).contains(&a), "magnitude inside the DFU range");
        }
        assert_ne!(
            roll_magnitude(&mag(1, 100, 0, 0, 1), true, 1, &mut MagicDice::with_seed(1)),
            roll_magnitude(&mag(1, 100, 0, 0, 1), true, 1, &mut MagicDice::with_seed(99))
        );
    }
}
