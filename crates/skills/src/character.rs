//! `character` — living character: 8 attributes, 35 skills, DFU progression and
//! derivatives (M20; spec `m20-character-spec-CR-a64582b8.md`).
//!
//! Rules faithful to the MIT source:
//! - attributes `STR/INT/WIL/AGI/END/PER/SPD/LCK` defaulting to 50 plus effect
//!   mods (`DaggerfallStats.cs:27-44`);
//! - 35 skills with defaults `3..=6`, live = permanent + mod
//!   (`DaggerfallSkills.cs:26-31,91-98,135-143`);
//! - use-based advancement `TallySkill`/`RaiseSkills` with the 360 min gate,
//!   reflexes, `CalculateSkillUsesForAdvancement` and cap/mastery
//!   (`PlayerEntity.cs:1243-1415`, `FormulaHelper.cs:319-327`);
//! - level from the skill sum (`PlayerEntity.cs:1439-1501`,
//!   `FormulaHelper.cs:330-337`), HP per level (`FormulaHelper.cs:340-354`) and
//!   attribute pool 4..6 (`FormulaHelper.cs:140-153`);
//! - derivatives (`FormulaHelper.cs:66-129`, `DaggerfallEntity.cs:262-276`).
//!
//! Pure: 0 Bevy, 0 I/O, randomness only through an explicit [`Rng64`]. The
//! `PlayerBody` consumed by the M19 engine is built here from the live values
//! (`player_body`), 0 duplicated logic.

use daggerfall_behavior::locomotion::{max_breath, max_encumbrance, PlayerBody};

use crate::bio::{parse_effect, BiographyEffect};
use crate::career::Career;
use crate::rng::Rng64;
use crate::stats::RpgStats;

/// Number of attributes (`DaggerfallStats.Count`, `DaggerfallStats.cs:27`).
pub const ATTRIBUTE_COUNT: usize = 8;
/// Number of skills (`DFCareer.Skills.Count`, `DFCareer.cs:448-487`).
pub const SKILL_COUNT: usize = 35;
/// Default value of an attribute (`DaggerfallStats.cs:28`).
pub const DEFAULT_ATTRIBUTE: i32 = 50;
/// Attribute cap (`FormulaHelper.MaxStatValue`, `FormulaHelper.cs:131-138`).
pub const MAX_ATTRIBUTE: i32 = 100;
/// Skill cap (`PlayerEntity.cs:1384`).
pub const MAX_SKILL: i16 = 100;
/// Cap of accumulated uses (`PlayerEntity.cs:1250-1255`).
pub const MAX_SKILL_USES: i32 = 20_000;
/// Skill creation defaults: `Random.Range(3, 7)` (`DaggerfallSkills.cs:30-31,95`).
pub const SKILL_DEFAULT_MIN: i32 = 3;
/// Skill creation defaults (inclusive maximum).
pub const SKILL_DEFAULT_MAX: i32 = 6;
/// Advancement re-evaluation gate, classic minutes (`PlayerEntity.cs:1367`).
pub const SKILL_CHECK_INTERVAL_MINUTES: i64 = 360;
/// Training gate, classic minutes (`DaggerfallGuildServiceTraining.cs:54`).
pub const TRAINING_INTERVAL_MINUTES: i64 = 720;
/// Training cost per level for members (`Guild.cs:27`).
pub const MEMBER_TRAINING_COST: i32 = 100;
/// Training cost per level for non-members (`Guild.cs:28`).
pub const NON_MEMBER_TRAINING_COST: i32 = 400;
/// Base fatigue loss (`PlayerEntity.cs:109`).
pub const DEFAULT_FATIGUE_LOSS: i32 = 11;
/// Raw fatigue multiplier (`DaggerfallEntity.cs:39`).
pub const FATIGUE_MULTIPLIER: i32 = 64;

// ── table enums ─────────────────────────────────────────────────────────

/// The 8 attributes (`DFCareer.Stats`, `DFCareer.cs:429-438`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Attribute {
    /// Strength.
    Strength = 0,
    /// Intelligence.
    Intelligence = 1,
    /// Willpower.
    Willpower = 2,
    /// Agility.
    Agility = 3,
    /// Endurance.
    Endurance = 4,
    /// Personality.
    Personality = 5,
    /// Speed.
    Speed = 6,
    /// Luck.
    Luck = 7,
}

impl Attribute {
    /// The 8 attributes in source order.
    pub const ALL: [Attribute; ATTRIBUTE_COUNT] = [
        Attribute::Strength,
        Attribute::Intelligence,
        Attribute::Willpower,
        Attribute::Agility,
        Attribute::Endurance,
        Attribute::Personality,
        Attribute::Speed,
        Attribute::Luck,
    ];

    /// Index 0..7.
    pub fn index(self) -> usize {
        self as usize
    }

    /// Short uppercase name (`STR`, `INT`, …).
    pub fn short_name(self) -> &'static str {
        match self {
            Attribute::Strength => "STR",
            Attribute::Intelligence => "INT",
            Attribute::Willpower => "WIL",
            Attribute::Agility => "AGI",
            Attribute::Endurance => "END",
            Attribute::Personality => "PER",
            Attribute::Speed => "SPD",
            Attribute::Luck => "LCK",
        }
    }
}

/// The 35 skills in the exact order of `DFCareer.Skills`
/// (`DFCareer.cs:448-487`; indices shared with `BIOG*.TXT` and `CLASS*.CFG`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Skill {
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

impl Skill {
    /// The 35 skills in index order.
    pub const ALL: [Skill; SKILL_COUNT] = [
        Skill::Medical,
        Skill::Etiquette,
        Skill::Streetwise,
        Skill::Jumping,
        Skill::Orcish,
        Skill::Harpy,
        Skill::Giantish,
        Skill::Dragonish,
        Skill::Nymph,
        Skill::Daedric,
        Skill::Spriggan,
        Skill::Centaurian,
        Skill::Impish,
        Skill::Lockpicking,
        Skill::Mercantile,
        Skill::Pickpocket,
        Skill::Stealth,
        Skill::Swimming,
        Skill::Climbing,
        Skill::Backstabbing,
        Skill::Dodging,
        Skill::Running,
        Skill::Destruction,
        Skill::Restoration,
        Skill::Illusion,
        Skill::Alteration,
        Skill::Thaumaturgy,
        Skill::Mysticism,
        Skill::ShortBlade,
        Skill::LongBlade,
        Skill::HandToHand,
        Skill::Axe,
        Skill::BluntWeapon,
        Skill::Archery,
        Skill::CriticalStrike,
    ];

    /// Index 0..34.
    pub fn index(self) -> usize {
        self as usize
    }

    /// Skill by index (`None` outside `0..=34`).
    pub fn from_index(index: i32) -> Option<Self> {
        if (0..SKILL_COUNT as i32).contains(&index) {
            Some(Self::ALL[index as usize])
        } else {
            None
        }
    }

    /// Enum name (`DFCareer.cs:448-487`).
    pub fn name(self) -> &'static str {
        const NAMES: [&str; SKILL_COUNT] = [
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
        NAMES[self.index()]
    }

    /// Primary stat of the skill (`DaggerfallSkills.cs:401-478`).
    pub fn primary_stat(self) -> Attribute {
        match self {
            Skill::Medical
            | Skill::Orcish
            | Skill::Harpy
            | Skill::Giantish
            | Skill::Dragonish
            | Skill::Nymph
            | Skill::Daedric
            | Skill::Spriggan
            | Skill::Centaurian
            | Skill::Impish
            | Skill::Lockpicking => Attribute::Intelligence,
            Skill::Etiquette | Skill::Streetwise | Skill::Mercantile => Attribute::Personality,
            Skill::Jumping | Skill::Climbing | Skill::Axe | Skill::BluntWeapon => {
                Attribute::Strength
            }
            Skill::Pickpocket
            | Skill::Stealth
            | Skill::Backstabbing
            | Skill::ShortBlade
            | Skill::LongBlade
            | Skill::HandToHand
            | Skill::Archery
            | Skill::CriticalStrike => Attribute::Agility,
            Skill::Swimming => Attribute::Endurance,
            Skill::Dodging | Skill::Running => Attribute::Speed,
            Skill::Destruction
            | Skill::Restoration
            | Skill::Illusion
            | Skill::Alteration
            | Skill::Thaumaturgy
            | Skill::Mysticism => Attribute::Willpower,
        }
    }

    /// Advancement multiplier (`DaggerfallSkills.cs:485-547`).
    pub fn advancement_multiplier(self) -> i32 {
        match self {
            Skill::Medical => 12,
            Skill::Etiquette | Skill::Streetwise => 1,
            Skill::Jumping => 5,
            Skill::Orcish
            | Skill::Harpy
            | Skill::Giantish
            | Skill::Dragonish
            | Skill::Nymph
            | Skill::Daedric
            | Skill::Spriggan
            | Skill::Centaurian
            | Skill::Impish => 15,
            Skill::Lockpicking => 2,
            Skill::Mercantile => 1,
            Skill::Pickpocket | Skill::Stealth => 2,
            Skill::Swimming => 1,
            Skill::Climbing => 2,
            Skill::Backstabbing => 1,
            Skill::Dodging => 4,
            Skill::Running => 50,
            Skill::Destruction => 1,
            Skill::Restoration => 2,
            Skill::Illusion | Skill::Alteration => 1,
            Skill::Thaumaturgy => 2,
            Skill::Mysticism => 1,
            Skill::ShortBlade
            | Skill::LongBlade
            | Skill::HandToHand
            | Skill::Axe
            | Skill::BluntWeapon => 2,
            Skill::Archery => 1,
            Skill::CriticalStrike => 8,
        }
    }

    /// Is this a language skill? (`DaggerfallSkills.cs:549-566`).
    pub fn is_language(self) -> bool {
        matches!(
            self,
            Skill::Orcish
                | Skill::Harpy
                | Skill::Giantish
                | Skill::Dragonish
                | Skill::Nymph
                | Skill::Daedric
                | Skill::Spriggan
                | Skill::Centaurian
                | Skill::Impish
        )
    }
}

/// Player reflexes (`EntityEnums.cs:171-178`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reflex {
    /// VeryHigh = 0.
    VeryHigh = 0,
    /// High = 1.
    High = 1,
    /// Average = 2.
    Average = 2,
    /// Low = 3.
    Low = 3,
    /// VeryLow = 4.
    VeryLow = 4,
}

impl Reflex {
    /// `0x10000 − ((reflexes − 2) << 13)` (`PlayerEntity.cs:1377`).
    pub fn modifier(self) -> i32 {
        0x10000 - ((self as i32 - 2) << 13)
    }
}

// ── formulas (spec §4/§5) ───────────────────────────────────────────────

/// `floor((STR − 50) / 5)` (`FormulaHelper.cs:66-73`).
pub fn damage_modifier(strength: i32) -> i32 {
    ((strength - 50) as f32 / 5.0).floor() as i32
}

/// `floor(INT × multiplicador)` (`FormulaHelper.cs:84-91`).
pub fn spell_points(intelligence: i32, multiplier: f32) -> i32 {
    (intelligence as f32 * multiplier).floor() as i32
}

/// `floor(WIL / 10)` (`FormulaHelper.cs:93-100`).
pub fn magic_resist(willpower: i32) -> i32 {
    (willpower as f32 / 10.0).floor() as i32
}

/// `floor(AGI / 10) − 5` (`FormulaHelper.cs:102-109`).
pub fn to_hit_modifier(agility: i32) -> i32 {
    (agility as f32 / 10.0).floor() as i32 - 5
}

/// `floor(END / 10) − 5` (`FormulaHelper.cs:111-118`).
pub fn hit_points_modifier(endurance: i32) -> i32 {
    (endurance as f32 / 10.0).floor() as i32 - 5
}

/// `floor(END / 10) − 5` (`FormulaHelper.cs:120-129`).
pub fn healing_rate_modifier(endurance: i32) -> i32 {
    (endurance as f32 / 10.0).floor() as i32 - 5
}

/// `(STR + END) × 64` (`DaggerfallEntity.cs:39,262`).
pub fn max_fatigue_raw(strength: i32, endurance: i32) -> i32 {
    (strength + endurance) * FATIGUE_MULTIPLIER
}

/// Uses needed to advance: `floor(v·adv·career·1.04^level·2/5 + 1)`
/// (`FormulaHelper.cs:319-327`).
pub fn uses_for_advancement(
    skill_value: i32,
    advancement_multiplier: i32,
    career_advancement_multiplier: f32,
    level: i32,
) -> i32 {
    let level_mod = 1.04f64.powi(level);
    (f64::from(skill_value)
        * f64::from(advancement_multiplier)
        * f64::from(career_advancement_multiplier)
        * level_mod
        * 2.0
        / 5.0
        + 1.0)
        .floor() as i32
}

/// `floor((current − starting + 28) / 15)` (`FormulaHelper.cs:330-337`;
/// the source performs integer division and then applies `Mathf.Floor`,
/// equivalent to Rust integer division over the game range).
pub fn player_level(starting_skill_sum: i32, current_skill_sum: i32) -> i32 {
    (current_skill_sum - starting_skill_sum + 28) / 15
}

/// Health recovery per hour (`FormulaHelper.cs:178-206`).
pub fn health_recovery_rate(
    healing_modifier: i32,
    medical_skill: i32,
    medical_extra: i32,
    max_health: i32,
) -> i32 {
    (healing_modifier + (medical_skill + medical_extra) * max_health / 1000).max(1)
}

/// Fatigue recovery per hour (`FormulaHelper.cs:209-216`).
pub fn fatigue_recovery_rate(max_fatigue: i32) -> i32 {
    (max_fatigue / 8).max(1)
}

/// Spell point recovery per hour (`FormulaHelper.cs:219-229`).
pub fn spell_recovery_rate(max_magicka: i32, no_regen: bool) -> i32 {
    if no_regen {
        0
    } else {
        (max_magicka / 8).max(1)
    }
}

// ── state containers ────────────────────────────────────────────────────

/// Permanent attributes + effect mods (`DaggerfallStats`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Attributes {
    permanent: [i32; ATTRIBUTE_COUNT],
    mods: [i32; ATTRIBUTE_COUNT],
}

impl Attributes {
    /// Attributes at `[DEFAULT_ATTRIBUTE; 8]` (`SetDefaults`,
    /// `DaggerfallStats.cs:102-114`).
    pub fn defaults() -> Self {
        Self {
            permanent: [DEFAULT_ATTRIBUTE; ATTRIBUTE_COUNT],
            mods: [0; ATTRIBUTE_COUNT],
        }
    }

    /// Base attributes of the career (`SetPermanentFromCareer`,
    /// `DaggerfallStats.cs:285-298`).
    pub fn from_career(career: &Career) -> Self {
        Self {
            permanent: career.attributes,
            mods: [0; ATTRIBUTE_COUNT],
        }
    }

    /// Permanent value.
    pub fn permanent(&self, attribute: Attribute) -> i32 {
        self.permanent[attribute.index()]
    }

    /// Live value (permanent + mod).
    pub fn live(&self, attribute: Attribute) -> i32 {
        self.permanent[attribute.index()] + self.mods[attribute.index()]
    }

    /// Sets the permanent value (clamped to `0..=100`; the advancement cap is 100).
    pub fn set_permanent(&mut self, attribute: Attribute, value: i32) {
        self.permanent[attribute.index()] = value.clamp(0, MAX_ATTRIBUTE);
    }

    /// Assigns the effect mod (`AssignMods` equivalent, `DaggerfallStats.cs:43`).
    pub fn set_mod(&mut self, attribute: Attribute, value: i32) {
        self.mods[attribute.index()] = value;
    }

    /// Clears the mods (`Array.Clear(mods)`, `DaggerfallStats.cs:112`).
    pub fn clear_mods(&mut self) {
        self.mods = [0; ATTRIBUTE_COUNT];
    }
}

/// Permanent skills + effect mods (`DaggerfallSkills`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillSet {
    permanent: [i16; SKILL_COUNT],
    mods: [i16; SKILL_COUNT],
}

impl SkillSet {
    /// All at 0 (not present in the source: `SetDefaults` rolls them; useful
    /// for tests and for building from a save).
    pub fn zeros() -> Self {
        Self {
            permanent: [0; SKILL_COUNT],
            mods: [0; SKILL_COUNT],
        }
    }

    /// Creation defaults: each skill `range_inclusive(3, 6)`
    /// (`DaggerfallSkills.cs:91-98`).
    pub fn defaults(rng: &mut Rng64) -> Self {
        let mut permanent = [0i16; SKILL_COUNT];
        for value in &mut permanent {
            *value = rng.range_inclusive(SKILL_DEFAULT_MIN, SKILL_DEFAULT_MAX) as i16;
        }
        Self {
            permanent,
            mods: [0; SKILL_COUNT],
        }
    }

    /// Permanent value (`GetPermanentSkillValue`, `DaggerfallSkills.cs:176-253`).
    pub fn permanent(&self, skill: Skill) -> i16 {
        self.permanent[skill.index()]
    }

    /// Live value (`GetLiveSkillValue`, `DaggerfallSkills.cs:135-143`).
    pub fn live(&self, skill: Skill) -> i16 {
        self.permanent[skill.index()].saturating_add(self.mods[skill.index()])
    }

    /// Sets the permanent value (`SetPermanentSkillValue`, `DaggerfallSkills.cs:264-374`).
    pub fn set_permanent(&mut self, skill: Skill, value: i16) {
        self.permanent[skill.index()] = value;
    }

    /// Adds to the permanent value (biography effects, `BiogFile.cs:253-267`).
    pub fn add_permanent(&mut self, skill: Skill, delta: i16) {
        let value = self.permanent[skill.index()].saturating_add(delta);
        self.permanent[skill.index()] = value;
    }

    /// Assigns the effect mod (`AssignMods`, `DaggerfallSkills.cs:392-395`).
    pub fn set_mod(&mut self, skill: Skill, value: i16) {
        self.mods[skill.index()] = value;
    }

    /// Clears the mods (`DaggerfallSkills.cs:97`).
    pub fn clear_mods(&mut self) {
        self.mods = [0; SKILL_COUNT];
    }

    /// All permanent values in index order.
    pub fn permanents(&self) -> &[i16; SKILL_COUNT] {
        &self.permanent
    }
}

/// Stored biography modifiers (`PlayerEntity.cs:172-177`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct BiographyMods {
    /// `RP` — poison resistance.
    pub resist_poison: i32,
    /// `RD` — disease resistance.
    pub resist_disease: i32,
    /// `MR` — magic resistance.
    pub resist_magic: i32,
    /// `TH` — avoid hit.
    pub avoid_hit: i32,
    /// `FT` — fatigue.
    pub fatigue: i32,
    /// `RR` — reaction.
    pub reaction: i32,
}

/// Report from `raise_skills` (one pass).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RaiseReport {
    /// Was the pass evaluated? (`false` if the 360 min gate did not open).
    pub checked: bool,
    /// Skills that advanced (skill, new value).
    pub raised: Vec<(Skill, i16)>,
    /// Did it become ready to level up?
    pub ready_to_level_up: bool,
}

/// Report of an applied level-up.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LevelUpReport {
    /// New level.
    pub level: i32,
    /// HP gained (`CalculateHitPointsPerLevelUp`).
    pub hit_points: i32,
    /// Attribute points to distribute (`BonusPool`).
    pub bonus_pool: i32,
}

/// Training error.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrainingError {
    /// 720 classic minutes have not elapsed (`DaggerfallGuildServiceTraining.cs:54`).
    TooSoon {
        /// Minutes elapsed since the last training.
        minutes_since: i64,
    },
    /// The skill exceeds the guild's trainable maximum (`Guild.cs:290-293`).
    TooSkilled {
        /// Current permanent value.
        value: i16,
    },
}

impl std::fmt::Display for TrainingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TrainingError::TooSoon { minutes_since } => {
                write!(f, "training too soon ({minutes_since} < {TRAINING_INTERVAL_MINUTES} min)")
            }
            TrainingError::TooSkilled { value } => {
                write!(f, "skill {value} above the trainable maximum")
            }
        }
    }
}

impl std::error::Error for TrainingError {}

/// Report of an applied training session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TrainingReport {
    /// Cost charged.
    pub cost: i32,
    /// Use tally added (`rand(10..=20) × advMult`).
    pub tally: i32,
}

/// Complete living character (M20).
#[derive(Debug, Clone, PartialEq)]
pub struct Character {
    /// Career/class.
    pub career: Career,
    /// Attributes (permanent + mods).
    pub attributes: Attributes,
    /// Skills (permanent + mods).
    pub skills: SkillSet,
    /// Accumulated uses per skill (`skillUses`, `PlayerEntity.cs:1249-1255`).
    pub uses: [i32; SKILL_COUNT],
    /// Stored biography mods (§6.3).
    pub biography: BiographyMods,
    /// Reflexes.
    pub reflex: Reflex,
    /// Level (≥ 1).
    pub level: i32,
    /// Maximum health (`maxHealth`).
    pub max_health: i32,
    /// Current health.
    pub health: i32,
    /// Current **raw** fatigue (the HUD divides by 64).
    pub fatigue: i32,
    /// Current spell points.
    pub magicka: i32,
    /// Current breath.
    pub breath: i32,
    /// Skill sum at creation (`startingLevelUpSkillSum`).
    pub starting_level_up_skill_sum: i32,
    /// Current skill sum (`currentLevelUpSkillSum`).
    pub level_up_skill_sum: i32,
    /// Ready to level up (`readyToLevelUp`).
    pub ready_to_level_up: bool,
    /// Attribute points pending distribution.
    pub pending_attribute_points: i32,
    /// Last advancement evaluation (classic minutes).
    pub last_skill_check_minutes: i64,
    /// Last training (classic minutes).
    pub last_training_minutes: i64,
}

impl Character {
    /// Creates a character from a career at `level` (normally 1).
    ///
    /// Order faithful to creation: career attributes, rolled skills,
    /// starting health `25 + HPL + Σ HP per level` and filled vital signs
    /// (`PlayerEntity.SetEntityDefaults` `PlayerEntity.cs:1171-1193`;
    /// `FormulaHelper.RollMaxHealth` `FormulaHelper.cs:160-175`).
    ///
    /// **Boundary (spec §7):** DFU does not create characters above level 1;
    /// for `level > 1` the starting sum is derived from the current one minus
    /// `15 × (level − 1)` (the `EstimateStartingLevelUpSkillSum` estimate
    /// assumes half a level, `PlayerEntity.cs:1480-1488`) and is only used for
    /// per-level HP goldens.
    pub fn starting(career: Career, rng: &mut Rng64, level: i32) -> Self {
        let attributes = Attributes::from_career(&career);
        let skills = SkillSet::defaults(rng);
        let level = level.max(1);
        let mut character = Self {
            career,
            attributes,
            skills,
            uses: [0; SKILL_COUNT],
            biography: BiographyMods::default(),
            reflex: Reflex::Average,
            level,
            max_health: 0,
            health: 0,
            fatigue: 0,
            magicka: 0,
            breath: 0,
            starting_level_up_skill_sum: 0,
            level_up_skill_sum: 0,
            ready_to_level_up: false,
            pending_attribute_points: 0,
            last_skill_check_minutes: 0,
            last_training_minutes: 0,
        };
        let mut max_health = 25 + character.career.hit_points_per_level;
        for _ in 1..level {
            max_health += character.roll_hit_points(rng);
        }
        character.max_health = max_health;
        character.fill_vital_signs();
        character.set_current_level_up_skill_sum();
        character.starting_level_up_skill_sum = if level <= 1 {
            character.level_up_skill_sum
        } else {
            character.level_up_skill_sum - (level - 1) * 15
        };
        character
    }

    // ── derivatives ─────────────────────────────────────────────────────

    /// Live attribute.
    pub fn live(&self, attribute: Attribute) -> i32 {
        self.attributes.live(attribute)
    }

    /// Live skill.
    pub fn live_skill(&self, skill: Skill) -> i16 {
        self.skills.live(skill)
    }

    /// Permanent skill.
    pub fn permanent_skill(&self, skill: Skill) -> i16 {
        self.skills.permanent(skill)
    }

    /// Damage modifier (`FormulaHelper.cs:66-73`).
    pub fn damage_modifier(&self) -> i32 {
        damage_modifier(self.live(Attribute::Strength))
    }

    /// Maximum weight (`FormulaHelper.cs:75-82`; formula shared with M19).
    pub fn max_encumbrance(&self) -> i32 {
        max_encumbrance(self.live(Attribute::Strength))
    }

    /// Maximum spell points (`FormulaHelper.cs:84-91`).
    pub fn max_magicka(&self) -> i32 {
        spell_points(
            self.live(Attribute::Intelligence),
            self.career.spell_point_multiplier,
        )
    }

    /// Magic resistance (`FormulaHelper.cs:93-100`).
    pub fn magic_resist(&self) -> i32 {
        magic_resist(self.live(Attribute::Willpower))
    }

    /// To-hit modifier (`FormulaHelper.cs:102-109`).
    pub fn to_hit_modifier(&self) -> i32 {
        to_hit_modifier(self.live(Attribute::Agility))
    }

    /// HP-per-level modifier (`FormulaHelper.cs:111-118`).
    pub fn hit_points_modifier(&self) -> i32 {
        hit_points_modifier(self.live(Attribute::Endurance))
    }

    /// Healing rate modifier (`FormulaHelper.cs:120-129`).
    pub fn healing_rate_modifier(&self) -> i32 {
        healing_rate_modifier(self.live(Attribute::Endurance))
    }

    /// Raw maximum fatigue (`DaggerfallEntity.cs:262`).
    pub fn max_fatigue(&self) -> i32 {
        max_fatigue_raw(
            self.live(Attribute::Strength),
            self.live(Attribute::Endurance),
        )
    }

    /// Maximum breath (`DaggerfallEntity.cs:267`; formula shared with M19).
    pub fn max_breath(&self) -> i32 {
        max_breath(self.live(Attribute::Endurance))
    }

    /// Maximum health with effect limiter (M20: no limiter, that is M32).
    pub fn raw_max_health(&self) -> i32 {
        self.max_health
    }

    /// Medical extra for rapid healing (`FormulaHelper.cs:191-203`).
    ///
    /// The source compares the flags by equality (a combined value matches no
    /// branch), and only applies `InLight` during the day and outdoors.
    pub fn medical_extra(&self, daylight: bool, inside: bool) -> i32 {
        let rapid = self.career.rapid_healing;
        if rapid == 4 {
            // RapidHealingFlags.Always
            100
        } else if daylight && !inside {
            if rapid == 1 {
                100 // InLight
            } else {
                60
            }
        } else if rapid == 2 {
            100 // InDarkness
        } else {
            60
        }
    }

    /// Health recovery per hour (`FormulaHelper.cs:178-206`).
    pub fn health_recovery_rate(&self, daylight: bool, inside: bool) -> i32 {
        health_recovery_rate(
            self.healing_rate_modifier(),
            i32::from(self.live_skill(Skill::Medical)),
            self.medical_extra(daylight, inside),
            self.max_health,
        )
    }

    /// Fatigue recovery per hour (`FormulaHelper.cs:209-216`).
    pub fn fatigue_recovery_rate(&self) -> i32 {
        fatigue_recovery_rate(self.max_fatigue())
    }

    /// Spell recovery per hour (`FormulaHelper.cs:219-229`).
    pub fn spell_recovery_rate(&self) -> i32 {
        spell_recovery_rate(self.max_magicka(), self.career.no_regen_spell_points)
    }

    // ── vital signs ─────────────────────────────────────────────────────

    /// Fills the vital signs to their maximum (`FillVitalSigns`, `DaggerfallEntity.cs:442-447`).
    pub fn fill_vital_signs(&mut self) {
        self.health = self.max_health;
        self.fatigue = self.max_fatigue();
        self.magicka = self.max_magicka();
        self.breath = self.max_breath();
    }

    /// Applies damage clamped at 0 (M20 does not declare death; M32).
    pub fn damage(&mut self, amount: i32) {
        if amount > 0 {
            self.health = (self.health - amount).max(0);
        }
    }

    /// Restores health clamped to the maximum.
    pub fn heal(&mut self, amount: i32) {
        if amount > 0 {
            self.health = (self.health + amount).min(self.max_health);
        }
    }

    /// Spends raw fatigue clamped to `[0, max]` (`SetFatigue`,
    /// `DaggerfallEntity.cs:355-366`).
    pub fn spend_fatigue(&mut self, amount: i32) {
        if amount > 0 {
            self.fatigue = (self.fatigue - amount).max(0);
        }
    }

    /// Restores raw fatigue clamped to the maximum.
    pub fn rest_fatigue(&mut self, amount: i32) {
        if amount > 0 {
            self.fatigue = (self.fatigue + amount).min(self.max_fatigue());
        }
    }

    // ── progression ─────────────────────────────────────────────────────

    /// Sums the level-share skills (`SetCurrentLevelUpSkillSum`,
    /// `PlayerEntity.cs:1439-1475`).
    pub fn set_current_level_up_skill_sum(&mut self) {
        let mut sum = 0i32;
        for skill in self.career.primary {
            sum += i32::from(self.permanent_skill(skill));
        }
        let mut lowest_major = i32::from(self.permanent_skill(self.career.major[0]));
        for &skill in &self.career.major {
            let value = i32::from(self.permanent_skill(skill));
            sum += value;
            if value < lowest_major {
                lowest_major = value;
            }
        }
        sum -= lowest_major;
        let mut highest_minor = i32::from(self.permanent_skill(self.career.minor[0]));
        for &skill in &self.career.minor {
            let value = i32::from(self.permanent_skill(skill));
            if value > highest_minor {
                highest_minor = value;
            }
        }
        sum += highest_minor;
        self.level_up_skill_sum = sum;
    }

    /// Is there already a primary skill at 100? (`AlreadyMasteredASkill`,
    /// `PlayerEntity.cs:1420-1434`).
    pub fn already_mastered_skill(&self) -> bool {
        self.career
            .primary
            .iter()
            .any(|&skill| self.permanent_skill(skill) == MAX_SKILL)
    }

    /// Accumulates uses of a skill (`TallySkill`, `PlayerEntity.cs:1243-1266`).
    pub fn tally_skill(&mut self, skill: Skill, amount: i32) {
        let uses = &mut self.uses[skill.index()];
        *uses = uses.saturating_add(amount).clamp(0, MAX_SKILL_USES);
    }

    /// Uses needed for the next advancement of a skill (`CalculateSkillUsesForAdvancement`).
    pub fn uses_needed(&self, skill: Skill) -> i32 {
        uses_for_advancement(
            i32::from(self.permanent_skill(skill)),
            skill.advancement_multiplier(),
            self.career.advancement_multiplier,
            self.level,
        )
    }

    /// Level calculated from the skill sum (`CalculatePlayerLevel`).
    pub fn calculated_level(&self) -> i32 {
        player_level(self.starting_level_up_skill_sum, self.level_up_skill_sum)
    }

    /// Re-evaluates pending advancements (`RaiseSkills`, `PlayerEntity.cs:1359-1415`).
    ///
    /// The gate is `(now − lastCheck) <= 360 ⇒ does not evaluate`; with the gate
    /// open, every skill with enough uses advances +1 (the counter is always
    /// reset, including when the cap or mastery prevents it).
    pub fn raise_skills(&mut self, now_minutes: i64) -> RaiseReport {
        if now_minutes - self.last_skill_check_minutes <= SKILL_CHECK_INTERVAL_MINUTES {
            return RaiseReport {
                checked: false,
                raised: Vec::new(),
                ready_to_level_up: self.ready_to_level_up,
            };
        }
        self.last_skill_check_minutes = now_minutes;
        let reflex_mod = self.reflex.modifier();
        let mut raised = Vec::new();
        for (index, skill) in Skill::ALL.iter().enumerate() {
            let needed = self.uses_needed(*skill);
            let calculated = (self.uses[index] * reflex_mod) >> 16;
            if calculated >= needed {
                self.uses[index] = 0;
                let permanent = i32::from(self.permanent_skill(*skill));
                if permanent < i32::from(MAX_SKILL)
                    && (permanent < 95 || !self.already_mastered_skill())
                {
                    let value = (permanent + 1) as i16;
                    self.skills.set_permanent(*skill, value);
                    self.set_current_level_up_skill_sum();
                    raised.push((*skill, value));
                }
            }
        }
        let ready = self.check_for_level_up();
        RaiseReport {
            checked: true,
            raised,
            ready_to_level_up: ready,
        }
    }

    /// Sets `readyToLevelUp` if the calculated level exceeds the current one
    /// (`CheckForLevelUp`, `PlayerEntity.cs:1493-1501`).
    pub fn check_for_level_up(&mut self) -> bool {
        let calculated = self.calculated_level();
        let level_up = self.level < calculated;
        if level_up {
            self.ready_to_level_up = true;
        }
        level_up
    }

    /// HP roll per level (`CalculateHitPointsPerLevelUp`,
    /// `FormulaHelper.cs:340-354`).
    pub fn roll_hit_points(&self, rng: &mut Rng64) -> i32 {
        let hpl = self.career.hit_points_per_level;
        let roll = rng.range_inclusive(hpl / 2, hpl);
        (roll + hit_points_modifier(self.attributes.permanent(Attribute::Endurance))).max(1)
    }

    /// Applies a pending level-up (`DaggerfallCharacterSheetWindow.cs:369-391`).
    ///
    /// `Level++`, `MaxHealth = RawMaxHealth + HP per level`, attribute pool
    /// `range(4, 6)`. Current health is not refilled (the source does not
    /// either in the character sheet).
    pub fn level_up(&mut self, rng: &mut Rng64) -> Option<LevelUpReport> {
        if !self.ready_to_level_up {
            return None;
        }
        self.level += 1;
        let hit_points = self.roll_hit_points(rng);
        self.max_health += hit_points;
        let bonus_pool = rng.range_inclusive(4, 6);
        self.pending_attribute_points += bonus_pool;
        self.ready_to_level_up = false;
        Some(LevelUpReport {
            level: self.level,
            hit_points,
            bonus_pool,
        })
    }

    /// Spends a pending attribute point (cap 100,
    /// `FormulaHelper.cs:131-138`).
    pub fn spend_attribute_point(&mut self, attribute: Attribute) -> bool {
        if self.pending_attribute_points <= 0
            || self.attributes.permanent(attribute) >= MAX_ATTRIBUTE
        {
            return false;
        }
        let value = self.attributes.permanent(attribute) + 1;
        self.attributes.set_permanent(attribute, value);
        self.pending_attribute_points -= 1;
        true
    }

    /// Trains a skill paying the guild rule
    /// (`DaggerfallGuildServiceTraining.cs:52-55,119-128`; `Guild.cs:27-28,295-299`).
    ///
    /// `max_trainable` is the guild's `GetTrainingMax` (default value used by
    /// guilds: M27).
    pub fn train_skill(
        &mut self,
        skill: Skill,
        now_minutes: i64,
        guild_member: bool,
        max_trainable: i16,
        rng: &mut Rng64,
    ) -> Result<TrainingReport, TrainingError> {
        let since = now_minutes - self.last_training_minutes;
        if since < TRAINING_INTERVAL_MINUTES {
            return Err(TrainingError::TooSoon {
                minutes_since: since,
            });
        }
        let value = self.permanent_skill(skill);
        if value > max_trainable {
            return Err(TrainingError::TooSkilled { value });
        }
        self.last_training_minutes = now_minutes;
        self.spend_fatigue(DEFAULT_FATIGUE_LOSS * 180);
        let cost = if guild_member {
            MEMBER_TRAINING_COST
        } else {
            NON_MEMBER_TRAINING_COST
        } * self.level;
        let tally = rng.range_inclusive(10, 20) * skill.advancement_multiplier();
        self.tally_skill(skill, tally);
        Ok(TrainingReport { cost, tally })
    }

    // ── biography ───────────────────────────────────────────────────────

    /// Applies biography effects (`ApplyPlayerEffect`, `BiogFile.cs:248-445`).
    ///
    /// Applies skills and stores the `RP/RD/MR/TH/FT/RR` mods. `GP`, `IT` and
    /// `r/rf` are counted as **boundary** (M30/M21/M27) and the texts/`AE/AF/AO`
    /// as not applied (spec §6.3/§7).
    pub fn apply_biography_effects(&mut self, effects: &[String]) -> BiographyApplyReport {
        let mut report = BiographyApplyReport::default();
        for effect in effects {
            match parse_effect(effect) {
                BiographyEffect::Skill { skill, delta } => {
                    self.skills.add_permanent(skill, delta as i16);
                    report.skills += 1;
                }
                BiographyEffect::PlayerMod { kind, amount } => {
                    match kind {
                        crate::bio::PlayerMod::ResistPoison => self.biography.resist_poison = amount,
                        crate::bio::PlayerMod::ResistDisease => {
                            self.biography.resist_disease = amount
                        }
                        crate::bio::PlayerMod::ResistMagic => self.biography.resist_magic = amount,
                        crate::bio::PlayerMod::AvoidHit => self.biography.avoid_hit = amount,
                        crate::bio::PlayerMod::Fatigue => self.biography.fatigue = amount,
                        crate::bio::PlayerMod::Reaction => self.biography.reaction = amount,
                    }
                    report.mods += 1;
                }
                BiographyEffect::Gold(_) | BiographyEffect::Item { .. } => report.deferred += 1,
                BiographyEffect::Reputation { .. } => report.deferred += 1,
                BiographyEffect::TextRef => report.text_refs += 1,
                BiographyEffect::Unsupported(_) => report.unsupported += 1,
                BiographyEffect::Unknown(_) => report.unknown += 1,
            }
        }
        self.set_current_level_up_skill_sum();
        report
    }

    // ── projections ─────────────────────────────────────────────────────

    /// `PlayerBody` for the M19 locomotion engine (0 duplicated logic).
    ///
    /// Spell/race effects (`enhanced_*`, `slow_falling`, `water_breathing`,
    /// `khajiit`, `argonian`, `improved_athleticism`) belong to M23/M34 and stay
    /// `false`; `carried_weight_kg` belongs to M21 (0.0).
    pub fn player_body(&self) -> PlayerBody {
        PlayerBody {
            live_speed: self.live(Attribute::Speed),
            live_endurance: self.live(Attribute::Endurance),
            live_luck: self.live(Attribute::Luck),
            live_strength: self.live(Attribute::Strength),
            running_skill: i32::from(self.live_skill(Skill::Running)),
            swimming_skill: i32::from(self.live_skill(Skill::Swimming)),
            climbing_skill: i32::from(self.live_skill(Skill::Climbing)),
            jumping_skill: i32::from(self.live_skill(Skill::Jumping)),
            stealth_skill: i32::from(self.live_skill(Skill::Stealth)),
            pickpocket_skill: i32::from(self.live_skill(Skill::Pickpocket)),
            lockpicking_skill: i32::from(self.live_skill(Skill::Lockpicking)),
            khajiit: false,
            argonian: false,
            athleticism: self.career.athleticism,
            improved_athleticism: false,
            enhanced_jumping: false,
            enhanced_climbing: false,
            slow_falling: false,
            water_breathing: false,
            carried_weight_kg: 0.0,
            level: self.level,
        }
    }

    /// HUD view (`RpgStats`): level, sum progress, health and fatigue
    /// divided by 64 (`DaggerfallCharacterSheetWindow.cs:401`).
    pub fn rpg_stats(&self) -> RpgStats {
        RpgStats {
            level: self.level,
            experience: self.level_up_skill_sum - self.starting_level_up_skill_sum,
            health: self.health,
            max_health: self.max_health,
            stamina: self.fatigue / FATIGUE_MULTIPLIER,
            max_stamina: self.max_fatigue() / FATIGUE_MULTIPLIER,
        }
    }
}

/// Result of applying biography effects.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct BiographyApplyReport {
    /// Skill effects applied.
    pub skills: usize,
    /// Player mods stored.
    pub mods: usize,
    /// Effects deferred to other modules (`GP`/`IT`/reputation).
    pub deferred: usize,
    /// Text references (`#`/`!`).
    pub text_refs: usize,
    /// Commands DFU declares unimplemented (`AE`/`AF`/`AO`).
    pub unsupported: usize,
    /// Unrecognized lines.
    pub unknown: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::career::Career;

    fn mage_career() -> Career {
        let mut bytes = vec![0u8; crate::career::CFG_BYTES];
        bytes[4..6].copy_from_slice(&0x0400u16.to_le_bytes()); // SP× 2.0
        bytes[16..19].copy_from_slice(&[27, 25, 26]);
        bytes[19..22].copy_from_slice(&[24, 22, 23]);
        bytes[22..28].copy_from_slice(&[0, 28, 32, 7, 9, 20]);
        bytes[28..32].copy_from_slice(b"Mage");
        bytes[52..54].copy_from_slice(&6u16.to_le_bytes());
        bytes[54..58].copy_from_slice(&0x0001_0A00u32.to_le_bytes()); // 1.039…→1.04
        let attributes: [u16; 8] = [42, 60, 65, 45, 45, 50, 43, 50];
        for (index, value) in attributes.iter().enumerate() {
            bytes[58 + index * 2..60 + index * 2].copy_from_slice(&value.to_le_bytes());
        }
        Career::parse(&bytes).expect("synthetic Mage career")
    }

    /// `CLASS01.CFG` measured in ARENA2 (spec §2 row 1): HPL 12, adv 0.91,
    /// SP× 1.50, attributes `60/50/55/50/50/40/45/50`.
    fn spellsword_career() -> Career {
        let mut bytes = vec![0u8; crate::career::CFG_BYTES];
        bytes[4..6].copy_from_slice(&0x0C00u16.to_le_bytes()); // SP× 1.5
        bytes[16..19].copy_from_slice(&[31, 29, 32]);
        bytes[19..22].copy_from_slice(&[22, 24, 25]);
        bytes[22..28].copy_from_slice(&[23, 26, 27, 28, 30, 33]);
        bytes[28..38].copy_from_slice(b"Spellsword");
        bytes[52..54].copy_from_slice(&12u16.to_le_bytes());
        bytes[54..58].copy_from_slice(&0x0000_EA00u32.to_le_bytes()); // 0.914 → 0.91
        let attributes: [u16; 8] = [60, 50, 55, 50, 50, 40, 45, 50];
        for (index, value) in attributes.iter().enumerate() {
            bytes[58 + index * 2..60 + index * 2].copy_from_slice(&value.to_le_bytes());
        }
        Career::parse(&bytes).expect("synthetic Spellsword career")
    }

    #[test]
    fn defaults_and_derived_match_goldens() {
        let mut rng = Rng64::new(20_261_020);
        let character = Character::starting(mage_career(), &mut rng, 1);
        let defaults: [i16; SKILL_COUNT] = [
            4, 4, 6, 6, 3, 4, 6, 4, 5, 6, 6, 4, 6, 4, 3, 3, 5, 5, 5, 4, 6, 5, 3, 5, 5, 3, 4, 3,
            4, 5, 6, 5, 6, 6, 3,
        ];
        assert_eq!(*character.skills.permanents(), defaults);
        assert_eq!(character.max_health, 31);
        // Raw fatigue `(live STR + live END) × 64` (`DaggerfallEntity.cs:39`;
        // spec §4): Mage = (42 + 45) × 64 = 5568 (HUD 87). The 6400/HUD 100 in
        // spec §4 assumes STR/END 50/50, incompatible with the rest of the row
        // (weight 63 ⇒ STR 42, damage −2 ⇒ STR 42, breath 22 ⇒ END 45).
        assert_eq!(character.max_fatigue(), 5568);
        assert_eq!(character.max_magicka(), 120);
        assert_eq!(character.max_encumbrance(), 63);
        assert_eq!(character.damage_modifier(), -2);
        assert_eq!(character.magic_resist(), 6);
        assert_eq!(character.to_hit_modifier(), -1);
        assert_eq!(character.hit_points_modifier(), -1);
        assert_eq!(character.healing_rate_modifier(), -1);
        assert_eq!(character.max_breath(), 22);
        assert_eq!(character.health, 31);
        assert_eq!(character.fatigue, 5568);
        assert_eq!(character.magicka, 120);
    }

    #[test]
    fn progression_formulas_match_goldens() {
        assert_eq!(uses_for_advancement(20, 2, 1.04, 1), 18);
        assert_eq!(uses_for_advancement(5, 50, 1.04, 1), 109);
        assert_eq!(uses_for_advancement(50, 2, 1.0, 1), 42);
        assert_eq!(uses_for_advancement(90, 2, 1.04, 10), 111);
        assert_eq!(uses_for_advancement(0, 1, 1.0, 1), 1);
        assert_eq!(player_level(0, 0), 1);
        assert_eq!(player_level(0, 2), 2);
        assert_eq!(player_level(0, 15), 2);
        assert_eq!(player_level(0, 17), 3);
        assert_eq!(player_level(100, 130), 3);
    }

    #[test]
    fn tally_clamps_and_raise_gate_is_exact() {
        let mut rng = Rng64::new(1);
        let mut character = Character::starting(mage_career(), &mut rng, 1);
        character.tally_skill(Skill::Running, -5);
        assert_eq!(character.uses[Skill::Running.index()], 0);
        character.tally_skill(Skill::Running, 25_000);
        assert_eq!(character.uses[Skill::Running.index()], MAX_SKILL_USES);
        character.uses[Skill::Running.index()] = 109;
        character.skills.set_permanent(Skill::Running, 5);
        let gate_closed = character.raise_skills(360);
        assert!(!gate_closed.checked);
        assert_eq!(character.permanent_skill(Skill::Running), 5);
        let report = character.raise_skills(361);
        assert!(report.checked);
        assert_eq!(character.permanent_skill(Skill::Running), 6);
        assert_eq!(character.uses[Skill::Running.index()], 0);
    }

    #[test]
    fn mastery_rule_blocks_96th_point_but_resets_uses() {
        let mut rng = Rng64::new(1);
        let mut character = Character::starting(mage_career(), &mut rng, 1);
        character.skills.set_permanent(Skill::Running, 95);
        character.uses[Skill::Running.index()] = 10_000;
        let primary = character.career.primary[0];
        character.skills.set_permanent(primary, 100);
        let report = character.raise_skills(361);
        assert!(report.checked);
        assert_eq!(character.permanent_skill(Skill::Running), 95);
        assert_eq!(character.uses[Skill::Running.index()], 0);
        // Without prior mastery, 95 → 96 does advance.
        character.skills.set_permanent(primary, 99);
        character.uses[Skill::Running.index()] = 10_000;
        let report = character.raise_skills(722);
        assert!(report.checked);
        assert_eq!(character.permanent_skill(Skill::Running), 96);
    }

    #[test]
    fn level_up_rolls_hp_and_bonus_pool() {
        // Golden from spec §8.2 D: C01 Spellsword (HPL 12, END 50), seed
        // `20_261_020` ⇒ HP per level 10, pool 5, health 37 → 47.
        let mut rng = Rng64::new(20_261_020);
        let mut character = Character::starting(spellsword_career(), &mut rng, 1);
        assert_eq!(character.max_health, 37);
        character.set_current_level_up_skill_sum();
        character.starting_level_up_skill_sum = character.level_up_skill_sum;
        character.level_up_skill_sum = character.starting_level_up_skill_sum + 2;
        assert!(character.check_for_level_up());
        let report = character.level_up(&mut rng).expect("pending level-up");
        assert_eq!(report.level, 2);
        assert_eq!(report.hit_points, 10);
        // Pool `rand_inclusive(4, 6)` (`FormulaHelper.cs:140-153`). Spec
        // §8.2 D records 5, but with the declared SplitMix64 flow (§7.1) after
        // the 35 creation rolls and the HP one the value is 4 (the pair (10, 5)
        // does not co-occur at any offset 0..3: it yields 4/5/6/6 with HP 10/8/10/11).
        assert!((4..=6).contains(&report.bonus_pool));
        assert_eq!(report.bonus_pool, 4);
        assert_eq!(character.max_health, 37 + 10);
        assert_eq!(character.pending_attribute_points, 4);
    }

    #[test]
    fn training_respects_gates_cost_and_tally() {
        let mut rng = Rng64::new(20_261_020);
        let mut character = Character::starting(mage_career(), &mut rng, 1);
        assert_eq!(
            character.train_skill(Skill::Running, 719, true, 50, &mut rng),
            Err(TrainingError::TooSoon { minutes_since: 719 })
        );
        let report = character
            .train_skill(Skill::Running, 720, true, 50, &mut rng)
            .expect("valid training");
        assert_eq!(report.cost, 100);
        assert_eq!(report.tally % 50, 0);
        assert!((500..=1000).contains(&report.tally));
        assert_eq!(character.fatigue, character.max_fatigue().max(0) - 1980);
        // Running starts from the seed `20_261_020` default = 5 (§8.2 B).
        assert_eq!(
            character.train_skill(Skill::Running, 1500, true, 1, &mut rng),
            Err(TrainingError::TooSkilled { value: 5 })
        );
    }

    #[test]
    fn player_body_reflects_live_skills_and_stats() {
        let mut rng = Rng64::new(1);
        let mut character = Character::starting(mage_career(), &mut rng, 1);
        character.skills.set_permanent(Skill::Running, 100);
        let body = character.player_body();
        assert_eq!(body.running_skill, 100);
        assert_eq!(body.live_speed, 43);
        assert_eq!(body.level, 1);
        assert_eq!(body.live_endurance, 45);
        assert!(!body.athleticism);
    }

    #[test]
    fn biography_effects_apply_skills_and_store_mods() {
        let mut rng = Rng64::new(1);
        let mut character = Character::starting(mage_career(), &mut rng, 1);
        let effects = vec![
            "22 +6".to_string(),
            "FT -5".to_string(),
            "GP +500".to_string(),
            "IT 3 2 1".to_string(),
            "r4 +10".to_string(),
            "#4150".to_string(),
            "AE M 8 12 +3".to_string(),
        ];
        let destruction = character.permanent_skill(Skill::Destruction);
        let report = character.apply_biography_effects(&effects);
        assert_eq!(character.permanent_skill(Skill::Destruction), destruction + 6);
        assert_eq!(character.biography.fatigue, -5);
        assert_eq!(report.skills, 1);
        assert_eq!(report.mods, 1);
        assert_eq!(report.deferred, 3);
        assert_eq!(report.text_refs, 1);
        assert_eq!(report.unsupported, 1);
    }

    #[test]
    fn rpg_stats_projects_hud_values() {
        let mut rng = Rng64::new(1);
        let mut character = Character::starting(mage_career(), &mut rng, 1);
        character.damage(6);
        let stats = character.rpg_stats();
        assert_eq!(stats.level, 1);
        assert_eq!((stats.health, stats.max_health), (25, 31));
        // Fatigue 5568 raw ÷ 64 = 87 (HUD), spec §4 with the real arithmetic
        // of the Mage attributes (STR 42 + END 45).
        assert_eq!((stats.stamina, stats.max_stamina), (87, 87));
        assert_eq!(stats.experience, 0);
    }

    // M23 §3.3 family F goldens (spell points and recovery;
    // `FormulaHelper.cs:84-91,219-229`). The rest of family F (F4–F6,
    // purchase price and animation) lives in `daggerfall-behavior::magic`.

    #[test]
    fn golden_m23_f1_spell_points_floor_of_int_times_multiplier() {
        assert_eq!(spell_points(50, 1.0), 50);
        assert_eq!(spell_points(50, 1.5), 75);
        assert_eq!(spell_points(37, 1.5), 55, "55.5 truncated by floor");
    }

    #[test]
    fn golden_m23_f2_f3_spell_recovery_rate_and_no_regen_flag() {
        assert_eq!(spell_recovery_rate(80, false), 10, "max(⌊80/8⌋, 1)");
        assert_eq!(spell_recovery_rate(4, false), 1);
        assert_eq!(spell_recovery_rate(0, false), 1);
        assert_eq!(spell_recovery_rate(80, true), 0, "NoRegenSpellPoints");
    }
}
